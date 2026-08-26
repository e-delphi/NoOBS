(*
  OBSTranscribe - fila de transcricao contra a Transcritor API
  (https://github.com/e-delphi/transcritor-api), um container local com
  faster-whisper + diarizacao por falante.

  Desenho em tres pontos que nao sao obvios:

  1. MANDA AUDIO, NUNCA O VIDEO. A API tem MAX_UPLOAD_MB=512 por padrao e
     uma gravacao 4K passa disso facil. A faixa de MISTURA (stream 0, que
     ja tem tudo) sai em m4a pelo ExtractAudioTracks: 1 hora da ~70 MB.

  2. MODELO ASSINCRONO, COM PROGRESSO REAL. POST /jobs devolve um id na
     hora; GET /jobs/{id} da estagio, percentual e ETA, e o `result`
     quando termina. O percentual mede o trecho de audio ja coberto pelos
     segmentos — nao ha estimativa nossa. Avanca aos saltos (janelas de
     30s do Whisper), e o ETA so existe a partir de ~10%.

  3. UMA POR VEZ. O proprio container serializa as requisicoes (os
     modelos nao sao thread-safe), entao paralelizar aqui so encheria a
     fila do outro lado. Uma thread, uma fila — e o cancelamento aborta
     ate o item em voo, via DELETE /jobs/{id}.

  O resultado vai pra dois arquivos no cache:
    <hash>.transcript.json  resposta inteira (turnos, segmentos, palavras)
    <hash>.txt              so o texto puro, pra BUSCA
  A busca le o .txt: carregar o JSON inteiro de cada gravacao pra filtrar
  a lista seria varios MB por item.
*)
unit OBSTranscribe;

interface

uses
  System.SysUtils;

type
  // Callback de mudanca de estado — a UI redesenha a partir daqui.
  // Chamado SEMPRE na main thread (a unit faz o marshalling).
  TTranscribeChanged = procedure;

// Enfileira uma gravacao. Ignora silenciosamente se ja esta na fila ou
// se ja tem transcricao (use Retranscribe pra refazer).
procedure Enqueue(const APath: string);
// Enfileira varias de uma vez (botao "transcrever pendentes").
procedure EnqueueMany(const APaths: TArray<string>);
// Cancela a fila inteira. O item EM CURSO nao e abortado no meio (a API
// nao tem cancelamento); ele termina e o resultado e descartado.
procedure CancelAll;
// Para a thread e limpa — chamada no shutdown.
procedure Shutdown;

procedure SetOnChanged(ACallback: TTranscribeChanged);

// --- estado pra UI (le da main thread) ---
function IsRunning: Boolean;
function QueueLength: Integer;        // inclui o item em curso
function CurrentName: string;
function CurrentElapsedSec: Integer;
// Progresso REAL vindo da API (0..1), medido pelo trecho de audio ja
// coberto pelos segmentos — nao e barra estimada. -1 = ainda nao ha
// numero (extraindo audio, subindo, ou job na fila do servidor).
function CurrentProgress: Double;
// Segundos restantes segundo a API. -1 = ela ainda nao arrisca (so
// aparece a partir de ~10% de progresso).
function CurrentEtaSec: Integer;
// Etapa em curso: 'extracting' | 'uploading' | 'queued' | 'decoding' |
// 'transcribing' | 'diarizing' | 'done'. Vazio = parado.
function CurrentStage: string;
function DoneCount: Integer;
function FailedCount: Integer;
function LastError: string;
// Nome (sem extensao) da gravacao a que o LastError se refere.
function LastErrorName: string;
// Total planejado do lote atual (pra "3 de 7"). Zera quando a fila esvazia.
function BatchTotal: Integer;

// --- fila visivel/editavel (aba Transcricao) ---
// Caminhos que AINDA ESPERAM, na ordem em que serao processados. O item
// em curso NAO entra aqui — ele ja saiu da fila quando a worker o pegou;
// quem o identifica e o CurrentPath.
function QueuedPaths: TArray<string>;
// Caminho do item em curso ('' se parado). O CurrentName so tem o nome,
// e nome nao identifica arquivo (duas pastas podem ter homonimos).
function CurrentPath: string;
// Contador que muda quando a COMPOSICAO da fila muda (entrou, saiu,
// mudou de lugar) — e nao quando so o progresso andou. O Bridge compara
// com o ultimo que empurrou pra nao remandar a lista inteira a cada
// tique de 1s (ver PushTranscribeQueue).
function QueueRevision: Integer;
// Move um item da espera pra posicao ANewIndex (0 = proximo a rodar).
// False se o path nao esta esperando. Nao mexe no item em curso.
function MoveInQueue(const APath: string; ANewIndex: Integer): Boolean;
// Tira um item da espera. False se nao estava la. O item EM CURSO nao
// sai por aqui — pra ele existe o CancelAll.
function RemoveFromQueue(const APath: string): Boolean;

// True se a gravacao ja tem transcricao no cache.
function HasTranscript(const APath: string): Boolean;
// Estado da transcricao em UMA palavra, pros cards da biblioteca:
//   ''      nunca foi transcrita
//   'ok'    transcrita, com fala
//   'empty' transcrita, e o audio nao tinha fala nenhuma
// Distinguir os dois ultimos importa: sem isso o player dizia "ainda nao
// foi transcrita" numa gravacao que FOI transcrita e simplesmente nao
// tem ninguem falando. So faz stat — nada de ler o conteudo, porque isto
// roda uma vez por gravacao a cada listagem da biblioteca.
function TranscriptState(const APath: string): string;
// Caminho do JSON completo (pode nao existir).
function TranscriptPath(const APath: string): string;
// Caminho do texto puro usado pela busca (pode nao existir).
function TranscriptTextPath(const APath: string): string;
// Apaga os dois arquivos de uma gravacao (usado ao excluir/mover).
procedure DeleteTranscript(const APath: string);

// Testa o servidor: GET /health. Devolve '' se ok, senao a mensagem.
function CheckHealth(const AHost: string): string;

// Base do servidor, do config ('transcribeHost'). Sempre sem barra final.
function HostBase: string;

implementation

uses
  Winapi.Windows,
  System.Classes,
  System.JSON,
  System.IOUtils,
  System.SyncObjs,
  System.Net.HttpClient,
  // TNetHTTPClient e do HttpClientComponent (o HttpClient so traz o
  // THTTPClient) e o IHTTPResponse vem do URLClient. Sem os tres, o
  // TNetHTTPClient fica indefinido e o parser descarrila daqui pra baixo.
  System.Net.HttpClientComponent,
  System.Net.URLClient,
  System.Net.Mime,
  System.Generics.Collections,
  OBSLog,
  OBSLang,
  OBSConfig,
  OBSPlayer,
  FFmpegOps;

const
  // Rotas FIXAS de proposito: o usuario informa host e porta, nao o
  // caminho. O modelo e ASSINCRONO — POST /jobs devolve um id na hora e o
  // andamento sai de GET /jobs/{id}.
  JOBS_PATH   = '/jobs';
  HEALTH_PATH = '/health';

  DEFAULT_HOST = 'http://localhost:8000';

  HEALTH_TIMEOUT_MS = 4000;
  CONNECT_TIMEOUT_MS = 5000;
  // O POST /jobs so espera o UPLOAD (a API responde ao enfileirar), mas
  // uma gravacao longa vira dezenas de MB — dai a folga.
  SUBMIT_TIMEOUT_MS = 5 * 60 * 1000;
  POLL_TIMEOUT_MS   = 10000;
  // Consulta barata e local. O progresso anda aos saltos (o Whisper
  // processa em janelas de 30s), entao 1s ja mostra cada degrau.
  POLL_INTERVAL_MS  = 1000;

  // Sentinela de "cancelado pelo usuario". Nao e erro: nao entra na
  // contagem de falhas nem vira mensagem na tela.
  CANCELED_MARK = #1'canceled';

type
  TTranscribeThread = class(TThread)
  protected
    procedure Execute; override;
  end;

var
  GQueue: TList<string> = nil;
  GLock: TCriticalSection = nil;
  Worker: TTranscribeThread = nil;
  StopEvent: THandle = 0;
  OnChanged: TTranscribeChanged = nil;

  // Estado observado pela UI. Escrito na worker, lido na main — sempre
  // sob GLock, exceto os inteiros simples (leitura atomica em x86-64).
  GRunning: Boolean = False;
  GCurrentName: string = '';
  GCurrentPath: string = '';
  GCurrentStartTick: Cardinal = 0;
  GStage: string = '';
  GProgress: Double = -1;      // -1 = ainda sem numero
  GEta: Integer = -1;          // -1 = a API ainda nao arrisca
  GCancelCurrent: Boolean = False;
  GDone: Integer = 0;
  GFailed: Integer = 0;
  GBatchTotal: Integer = 0;
  GLastError: string = '';
  // Nome da gravacao que falhou. Sem ele, "1 com falha" num lote de 7
  // nao diz QUAL — e a mensagem sozinha raramente identifica o item.
  GLastErrorName: string = '';
  // Sobe a cada mudanca de COMPOSICAO da fila. Ver QueueRevision.
  GQueueRev: Integer = 0;

function HostBase: string;
begin
  Result := Trim(GetConfigStr('transcribeHost', DEFAULT_HOST));
  if Result = '' then Result := DEFAULT_HOST;
  while (Result <> '') and (Result[Length(Result)] = '/') do
    Delete(Result, Length(Result), 1);
end;

function TranscriptPath(const APath: string): string;
begin
  Result := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath) + '.transcript.json';
end;

function TranscriptTextPath(const APath: string): string;
begin
  Result := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath) + '.txt';
end;

function HasTranscript(const APath: string): Boolean;
begin
  Result := (APath <> '') and TFile.Exists(TranscriptPath(APath));
end;

function TranscriptState(const APath: string): string;
var
  Txt: string;
begin
  Result := '';
  if APath = '' then Exit;
  if not TFile.Exists(TranscriptPath(APath)) then Exit;
  // O .txt e o texto puro. Vazio = a API respondeu, mas nao havia fala.
  // O TAMANHO basta: ler o arquivo so pra saber se e vazio seria I/O a
  // toa em cada item da lista.
  Txt := TranscriptTextPath(APath);
  if not TFile.Exists(Txt) then Exit('empty');
  try
    if TFile.GetSize(Txt) > 0 then Result := 'ok' else Result := 'empty';
  except
    Result := 'ok';
  end;
end;

procedure DeleteTranscript(const APath: string);
begin
  try if TFile.Exists(TranscriptPath(APath)) then TFile.Delete(TranscriptPath(APath)); except end;
  try if TFile.Exists(TranscriptTextPath(APath)) then TFile.Delete(TranscriptTextPath(APath)); except end;
end;

procedure SetOnChanged(ACallback: TTranscribeChanged);
begin
  OnChanged := ACallback;
end;

procedure NotifyChanged;
// Sempre marshalla pra main: a UI so pode ser tocada de la, e este
// callback e chamado da worker em quase todos os pontos.
begin
  TThread.Queue(nil,
    procedure
    begin
      if Assigned(OnChanged) then
        try OnChanged; except end;
    end);
end;

function IsRunning: Boolean;
begin
  Result := GRunning;
end;

function QueueLength: Integer;
begin
  Result := 0;
  if GLock = nil then Exit;
  GLock.Enter;
  try
    if GQueue <> nil then Result := GQueue.Count;
    if GRunning then Inc(Result);
  finally
    GLock.Leave;
  end;
end;

function CurrentName: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GCurrentName; finally GLock.Leave; end;
end;

function CurrentElapsedSec: Integer;
begin
  Result := 0;
  if not GRunning then Exit;
  if GCurrentStartTick = 0 then Exit;
  Result := Integer((GetTickCount - GCurrentStartTick) div 1000);
end;

function CurrentProgress: Double;
begin
  Result := GProgress;
end;

function CurrentEtaSec: Integer;
begin
  Result := GEta;
end;

function CurrentStage: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GStage; finally GLock.Leave; end;
end;

function CancelRequested: Boolean;
begin
  Result := GCancelCurrent;
end;

function ShuttingDown: Boolean;
// O ProcessOne roda na worker mas NAO e metodo dela, entao nao enxerga o
// Terminated. O sinal de fechamento que vale aqui e o StopEvent (manual
// reset, ligado pelo Stop) — o mesmo que tira a espera do laco de polling.
begin
  Result := (StopEvent <> 0) and
    (WaitForSingleObject(StopEvent, 0) = WAIT_OBJECT_0);
end;

procedure SetStage(const AStage: string; AProgress: Double; AEta: Integer);
// Publica a etapa/progresso e avisa a UI. Chamado da worker a cada
// consulta ao servidor.
var
  Mudou: Boolean;
begin
  if GLock = nil then Exit;
  GLock.Enter;
  try
    Mudou := (GStage <> AStage) or (Abs(GProgress - AProgress) > 0.0005) or
             (GEta <> AEta);
    GStage := AStage;
    GProgress := AProgress;
    GEta := AEta;
  finally
    GLock.Leave;
  end;
  // So notifica quando algo mudou de fato: o progresso anda aos saltos
  // (janelas de 30s do Whisper), entao a maioria dos tiques e igual ao
  // anterior e empurrar tudo seria ruido no canal.
  if Mudou then NotifyChanged;
end;

function DoneCount: Integer;   begin Result := GDone;   end;
function FailedCount: Integer; begin Result := GFailed; end;
function BatchTotal: Integer;  begin Result := GBatchTotal; end;

function LastError: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GLastError; finally GLock.Leave; end;
end;

function LastErrorName: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GLastErrorName; finally GLock.Leave; end;
end;

function CurrentPath: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GCurrentPath; finally GLock.Leave; end;
end;

function QueueRevision: Integer;
begin
  Result := GQueueRev;
end;

function QueuedPaths: TArray<string>;
// Copia sob o lock: a worker tira itens da fila a qualquer momento, e
// devolver a TList crua deixaria o Bridge iterando sobre ela sem lock.
var
  i: Integer;
begin
  Result := nil;
  if GLock = nil then Exit;
  GLock.Enter;
  try
    if GQueue = nil then Exit;
    SetLength(Result, GQueue.Count);
    for i := 0 to GQueue.Count - 1 do Result[i] := GQueue[i];
  finally
    GLock.Leave;
  end;
end;

function IndexInQueue(const APath: string): Integer;
// Caller segura o GLock. Mesma regra do AlreadyQueued: compara por PATH.
var
  i: Integer;
begin
  Result := -1;
  if GQueue = nil then Exit;
  for i := 0 to GQueue.Count - 1 do
    if SameText(GQueue[i], APath) then Exit(i);
end;

function MoveInQueue(const APath: string; ANewIndex: Integer): Boolean;
var
  Old: Integer;
begin
  Result := False;
  if (GLock = nil) or (APath = '') then Exit;
  GLock.Enter;
  try
    Old := IndexInQueue(APath);
    if Old < 0 then Exit;
    // Clamp em vez de recusar: a UI manda um indice calculado de posicao
    // de mouse, e a fila pode ter encolhido entre o gesto e a mensagem
    // (a worker pegou o primeiro item nesse meio-tempo).
    if ANewIndex < 0 then ANewIndex := 0;
    if ANewIndex > GQueue.Count - 1 then ANewIndex := GQueue.Count - 1;
    if ANewIndex = Old then Exit;
    GQueue.Move(Old, ANewIndex);
    Inc(GQueueRev);
    Result := True;
  finally
    GLock.Leave;
  end;
  if Result then
  begin
    Log('Transcribe: "%s" movido pra posicao %d da fila.', [APath, ANewIndex]);
    NotifyChanged;
  end;
end;

function RemoveFromQueue(const APath: string): Boolean;
var
  Idx: Integer;
begin
  Result := False;
  if (GLock = nil) or (APath = '') then Exit;
  GLock.Enter;
  try
    Idx := IndexInQueue(APath);
    if Idx < 0 then Exit;
    GQueue.Delete(Idx);
    Inc(GQueueRev);
    // O total do lote conta o que VAI rodar. Sem descontar, a linha de
    // progresso ficaria presa em "6 de 7" com a fila vazia.
    if GBatchTotal > 0 then Dec(GBatchTotal);
    Result := True;
  finally
    GLock.Leave;
  end;
  if Result then
  begin
    Log('Transcribe: "%s" tirado da fila.', [APath]);
    NotifyChanged;
  end;
end;

function HttpErrText(const AResp: IHTTPResponse): string;
// "HTTP 422" sozinho nao diz NADA — quem sabe o motivo e o CORPO da
// resposta, e ele estava sendo jogado fora. A Transcritor API e FastAPI,
// entao erro sai como {"detail": ...}: string nos erros de negocio e
// ARRAY nos de validacao (422). Pega o detail quando da, senao o corpo
// cru. Trunca e tira quebras de linha porque isto vira UMA linha na tela.
const
  MAX_LEN = 300;
var
  Body, Detail: string;
  Json, Val: TJSONValue;
begin
  Result := Format('HTTP %d', [AResp.StatusCode]);
  if Trim(AResp.StatusText) <> '' then
    Result := Result + ' ' + Trim(AResp.StatusText);
  Body := '';
  try Body := Trim(AResp.ContentAsString(TEncoding.UTF8)); except end;
  if Body = '' then Exit;
  Detail := Body;
  Json := TJSONObject.ParseJSONValue(Body);
  if Json <> nil then
  try
    if Json is TJSONObject then
    begin
      Val := TJSONObject(Json).GetValue('detail');
      if Val is TJSONString then Detail := TJSONString(Val).Value
      else if Val <> nil then Detail := Val.ToJSON;
    end;
  finally
    Json.Free;
  end;
  Detail := Trim(StringReplace(StringReplace(Detail, #13, ' ', [rfReplaceAll]),
    #10, ' ', [rfReplaceAll]));
  if Detail = '' then Exit;
  if Length(Detail) > MAX_LEN then Detail := Copy(Detail, 1, MAX_LEN) + '...';
  Result := Result + ': ' + Detail;
end;

function NetErrText(const AMsg: string): string;
// Falha de CONEXAO chega aqui como texto cru do WinINet — "Error sending
// data: (12152) O servidor retornou uma resposta invalida" — que descreve
// o sintoma e nao o que fazer. Container parado da justamente isso, e o
// usuario nao tem como ligar uma coisa a outra. Troca pela frase que diz
// o que houve e ONDE; o texto tecnico vai pro log, que e onde ele serve.
begin
  Log('Transcribe: falha de rede contra %s: %s', [HostBase, AMsg]);
  Result := OBSLang.T('error.transcribe.serverDown', ['host', HostBase]);
end;

function CheckHealth(const AHost: string): string;
// Roda na worker (a UI chama por um botao "Testar"). '' = ok.
// Devolve o texto CRU da falha de proposito: aqui o usuario apertou
// "Testar" e quer o diagnostico. Quem transcreve usa o serverDown.
var
  Http: TNetHTTPClient;
  Resp: IHTTPResponse;
  Base: string;
begin
  Base := Trim(AHost);
  while (Base <> '') and (Base[Length(Base)] = '/') do
    Delete(Base, Length(Base), 1);
  if Base = '' then Exit(OBSLang.T('error.transcribe.emptyHost'));
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := HEALTH_TIMEOUT_MS;
    Http.ResponseTimeout := HEALTH_TIMEOUT_MS;
    try
      Resp := Http.Get(Base + HEALTH_PATH);
    except
      on E: Exception do Exit(E.Message);
    end;
    if Resp = nil then Exit(OBSLang.T('error.transcribe.noResponse'));
    if Resp.StatusCode <> 200 then
      Exit(HttpErrText(Resp));
    Result := '';
  finally
    Http.Free;
  end;
end;

function ExtractPlainText(AObj: TJSONObject): string;
// Texto puro pra BUSCA. Prefere o campo 'text' da resposta; se faltar,
// remonta a partir dos turnos. Nunca falha — busca com texto vazio so
// nao acha nada, o que e melhor que abortar a transcricao inteira.
var
  Arr: TJSONValue;
  i: Integer;
  Turn: TJSONValue;
  S: string;
  SB: TStringBuilder;
begin
  Result := '';
  if AObj = nil then Exit;
  if AObj.TryGetValue<string>('text', S) and (Trim(S) <> '') then Exit(S);
  Arr := AObj.GetValue('turns');
  if not (Arr is TJSONArray) then Exit;
  SB := TStringBuilder.Create;
  try
    for i := 0 to TJSONArray(Arr).Count - 1 do
    begin
      Turn := TJSONArray(Arr).Items[i];
      if not (Turn is TJSONObject) then Continue;
      if TJSONObject(Turn).TryGetValue<string>('text', S) then
      begin
        if SB.Length > 0 then SB.Append(' ');
        SB.Append(Trim(S));
      end;
    end;
    Result := SB.ToString;
  finally
    SB.Free;
  end;
end;

function SubmitJob(const AAudioPath: string; out AJobId: string): string;
// POST /jobs — devolve na hora um identificador (HTTP 202), sem esperar a
// transcricao. O upload e a unica parte demorada aqui, e ele e rapido
// porque so vai audio (ver cabecalho da unit).
var
  Http: TNetHTTPClient;
  Data: TMultipartFormData;
  Resp: IHTTPResponse;
  Lang, Body: string;
  Json: TJSONValue;
begin
  AJobId := '';
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    // So o upload: a API responde assim que enfileira.
    Http.ResponseTimeout := SUBMIT_TIMEOUT_MS;
    Http.UserAgent := 'NoOBS';
    Data := TMultipartFormData.Create;
    try
      Data.AddFile('file', AAudioPath);
      // Idioma: vazio = deteccao automatica (padrao da API). Fica no
      // config pra quem grava sempre no mesmo idioma ganhar precisao.
      Lang := Trim(GetConfigStr('transcribeLanguage', ''));
      if Lang <> '' then Data.AddField('language', Lang);
      // Separacao por falante — e o que torna o painel do player util.
      Data.AddField('diarization', 'true');
      try
        Resp := Http.Post(HostBase + JOBS_PATH, Data);
      except
        on E: Exception do Exit(NetErrText(E.Message));
      end;
      if Resp = nil then Exit(OBSLang.T('error.transcribe.noResponse'));
      // 202 e o esperado; aceita 200 tambem pra nao quebrar se a API
      // mudar o codigo de sucesso.
      if (Resp.StatusCode <> 202) and (Resp.StatusCode <> 200) then
        Exit(HttpErrText(Resp));
      Body := Resp.ContentAsString(TEncoding.UTF8);
      Json := TJSONObject.ParseJSONValue(Body);
      if not (Json is TJSONObject) then
      begin
        if Json <> nil then Json.Free;
        Exit(OBSLang.T('error.transcribe.jobsNotJson'));
      end;
      try
        TJSONObject(Json).TryGetValue<string>('job_id', AJobId);
      finally
        Json.Free;
      end;
      if Trim(AJobId) = '' then Exit(OBSLang.T('error.transcribe.noJobId'));
      Result := '';
    finally
      Data.Free;
    end;
  finally
    Http.Free;
  end;
end;

procedure DeleteJob(const AJobId: string);
// Melhor esforco: se o cancelamento nao chegar, o job termina sozinho e
// expira pelo JOB_TTL_SECONDS da API. Nao vale falhar por causa disso.
var
  Http: TNetHTTPClient;
begin
  if AJobId = '' then Exit;
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    Http.ResponseTimeout := POLL_TIMEOUT_MS;
    try Http.Delete(HostBase + JOBS_PATH + '/' + AJobId); except end;
    Log('Transcribe: job %s cancelado no servidor.', [AJobId]);
  finally
    Http.Free;
  end;
end;

function PollJob(const AJobId: string; out AStatus, AStage, AError: string;
  out AProgress: Double; out AEta: Integer; out AResult: string): string;
// GET /jobs/{id}. Devolve '' se a consulta em si funcionou (mesmo com
// status "error" do job — isso vem em AStatus/AError); senao a falha de
// rede.
var
  Http: TNetHTTPClient;
  Resp: IHTTPResponse;
  Body: string;
  Json: TJSONValue;
  Obj: TJSONObject;
  ResVal: TJSONValue;
  Eta: Double;
begin
  AStatus := '';
  AStage := '';
  AError := '';
  AResult := '';
  AProgress := 0;
  AEta := -1;
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    Http.ResponseTimeout := POLL_TIMEOUT_MS;
    Http.UserAgent := 'NoOBS';
    try
      Resp := Http.Get(HostBase + JOBS_PATH + '/' + AJobId);
    except
      on E: Exception do Exit(NetErrText(E.Message));
    end;
    if Resp = nil then Exit(OBSLang.T('error.transcribe.noResponse'));
    if Resp.StatusCode = 404 then Exit(OBSLang.T('error.transcribe.jobGone'));
    if Resp.StatusCode <> 200 then Exit(HttpErrText(Resp));
    Body := Resp.ContentAsString(TEncoding.UTF8);
    Json := TJSONObject.ParseJSONValue(Body);
    if not (Json is TJSONObject) then
    begin
      if Json <> nil then Json.Free;
      Exit(OBSLang.T('error.transcribe.statusNotJson'));
    end;
    try
      Obj := TJSONObject(Json);
      Obj.TryGetValue<string>('status', AStatus);
      Obj.TryGetValue<string>('stage', AStage);
      Obj.TryGetValue<string>('error', AError);
      Obj.TryGetValue<Double>('progress', AProgress);
      // eta_seconds so aparece a partir de ~10% (antes disso extrapolar
      // nao faz sentido). -1 = ainda nao da pra dizer.
      if Obj.TryGetValue<Double>('eta_seconds', Eta) then AEta := Round(Eta);
      // O `result` so vem quando status = done, e e o MESMO JSON que o
      // /transcribe devolvia — o resto da unit nao muda por causa disso.
      ResVal := Obj.GetValue('result');
      if ResVal is TJSONObject then AResult := ResVal.ToJSON;
    finally
      Json.Free;
    end;
    Result := '';
  finally
    Http.Free;
  end;
end;

function ProcessOne(const APath: string): string;
// Transcreve UMA gravacao. Devolve '' em sucesso, senao a mensagem.
// Roda inteiro na worker thread.
//
// Modelo ASSINCRONO: POST /jobs devolve um id na hora, e o andamento sai
// de GET /jobs/{id} — progresso REAL, medido pelo trecho de audio ja
// coberto pelos segmentos. Nao ha mais estimativa nossa nem requisicao
// HTTP pendurada por 40 minutos.
var
  Audio, Body, Txt, JobId: string;
  Status, Stage, JobErr, ResJson: string;
  Prog: Double;
  Eta: Integer;
  Json: TJSONValue;
  HealthErr: string;
begin
  if not TFile.Exists(APath) then Exit(OBSLang.T('error.transcribe.fileMissing'));

  // PING ANTES DE QUALQUER TRABALHO. Duas razoes, e a segunda e a que
  // pesa: (1) servidor fora do ar so aparecia depois de extrair o audio
  // — dezenas de MB, segundos a minutos jogados fora por item do lote;
  // (2) o erro que chegava era o do WinINet no meio do upload ("Error
  // sending data: (12152)..."), que nao tem como ser lido como
  // "o container nao esta rodando". O /health e barato e local.
  SetStage('checking', 0, -1);
  HealthErr := CheckHealth(HostBase);
  if HealthErr <> '' then
  begin
    Log('Transcribe: /health nao respondeu (%s): %s', [HostBase, HealthErr]);
    Exit(OBSLang.T('error.transcribe.serverDown', ['host', HostBase]));
  end;

  // SO O AUDIO. O video passaria do MAX_UPLOAD_MB da API (512 MB) numa
  // gravacao 4K de poucos minutos. Indice 0 = faixa de MISTURA, que ja
  // tem todos os microfones e o som do sistema juntos.
  Audio := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath) + '_tr.m4a';
  try if TFile.Exists(Audio) then TFile.Delete(Audio); except end;
  SetStage('extracting', 0, -1);
  if not FFmpegOps.ExtractAudioTracks(APath, [Audio], 0) then
    Exit(OBSLang.T('error.transcribe.extractFailed'));
  if not TFile.Exists(Audio) then Exit(OBSLang.T('error.transcribe.extractMissing'));

  Body := '';
  try
    SetStage('uploading', 0, -1);
    Result := SubmitJob(Audio, JobId);
    if Result <> '' then Exit;
    Log('Transcribe: job %s aceito.', [JobId]);

    // Acompanha ate terminar. O intervalo e curto porque a consulta e
    // local e barata; o progresso em si anda aos saltos (o Whisper
    // processa em janelas de 30s), entao poucos tiques mostram avanco.
    while True do
    begin
      if ShuttingDown then
      begin
        DeleteJob(JobId);
        Exit(OBSLang.T('error.transcribe.shuttingDown'));
      end;
      if CancelRequested then
      begin
        // Agora DA pra abortar de verdade: a API tem DELETE /jobs/{id}.
        DeleteJob(JobId);
        Exit(CANCELED_MARK);
      end;

      Result := PollJob(JobId, Status, Stage, JobErr, Prog, Eta, ResJson);
      if Result <> '' then Exit;

      if Status = 'done' then
      begin
        SetStage('done', 1, 0);
        Body := ResJson;
        if Trim(Body) = '' then Exit(OBSLang.T('error.transcribe.emptyResult'));
        Break;
      end;
      if Status = 'error' then
      begin
        if JobErr = '' then JobErr := OBSLang.T('error.transcribe.serverSilent');
        Exit(JobErr);
      end;

      // 'queued' | 'decoding' | 'transcribing' | 'diarizing'
      SetStage(Stage, Prog, Eta);

      // Espera interrompivel: o StopEvent tira a thread daqui na hora, em
      // vez de deixar o fechamento do app esperando um Sleep.
      if WaitForSingleObject(StopEvent, POLL_INTERVAL_MS) = WAIT_OBJECT_0 then
      begin
        DeleteJob(JobId);
        Exit(OBSLang.T('error.transcribe.shuttingDown'));
      end;
    end;

    Json := TJSONObject.ParseJSONValue(Body);
    if not (Json is TJSONObject) then
    begin
      if Json <> nil then Json.Free;
      Exit(OBSLang.T('error.transcribe.resultNotJson'));
    end;
    try
      Txt := ExtractPlainText(TJSONObject(Json));
    finally
      Json.Free;
    end;

    // Dois arquivos: o JSON inteiro pro player, o texto puro pra busca.
    // Ver o cabecalho da unit.
    try
      TFile.WriteAllText(TranscriptPath(APath), Body, TEncoding.UTF8);
      TFile.WriteAllText(TranscriptTextPath(APath), Txt, TEncoding.UTF8);
    except
      on E: Exception do Exit(OBSLang.T('error.transcribe.writeFailed', ['error', E.Message]));
    end;
    Result := '';
  finally
    // O m4a e so o veiculo do upload — regenera em segundos se precisar,
    // e uma hora de audio ocupa ~70 MB que nao vale guardar.
    try if TFile.Exists(Audio) then TFile.Delete(Audio); except end;
  end;
end;

procedure TTranscribeThread.Execute;
var
  Path, Err: string;
  Has: Boolean;
begin
  while not Terminated do
  begin
    Path := '';
    if GLock <> nil then
    begin
      GLock.Enter;
      try
        Has := (GQueue <> nil) and (GQueue.Count > 0);
        if Has then
        begin
          Path := GQueue[0];
          GQueue.Delete(0);
          Inc(GQueueRev);
          GRunning := True;
          GCurrentPath := Path;
          GCurrentName := ChangeFileExt(ExtractFileName(Path), '');
          GCurrentStartTick := GetTickCount;
          GStage := 'extracting';
          GProgress := -1;
          GEta := -1;
          // O cancelamento vale pro item que estava em curso quando o
          // usuario clicou; um item NOVO comeca limpo.
          GCancelCurrent := False;
        end
        else
        begin
          if GRunning then Inc(GQueueRev);   // a linha "em curso" sai da lista
          GRunning := False;
          GCurrentPath := '';
          GCurrentName := '';
          GCurrentStartTick := 0;
          GStage := '';
          GProgress := -1;
          GEta := -1;
          GCancelCurrent := False;
          // Os contadores do lote NAO sao zerados aqui: a UI precisa
          // continuar mostrando "7 de 7 concluidas" depois que a fila
          // esvazia. Quem zera e o proximo lote (ver ResetBatchIfIdle).
        end;
      finally
        GLock.Leave;
      end;
    end;

    if Path = '' then
    begin
      NotifyChanged;
      if WaitForSingleObject(StopEvent, 400) = WAIT_OBJECT_0 then Break;
      Continue;
    end;

    NotifyChanged;
    Log('Transcribe: iniciando "%s"', [Path]);
    Err := '';
    try
      Err := ProcessOne(Path);
    except
      on E: Exception do Err := E.Message;
    end;

    GLock.Enter;
    try
      if Err = CANCELED_MARK then
        // Cancelado pelo usuario: nao conta como concluido nem como
        // falha, e nao vira mensagem de erro na tela.
        Log('Transcribe: cancelado "%s"', [Path])
      else if Err = '' then Inc(GDone)
      else
      begin
        Inc(GFailed);
        GLastError := Err;
        GLastErrorName := ChangeFileExt(ExtractFileName(Path), '');
      end;
    finally
      GLock.Leave;
    end;
    if Err = '' then Log('Transcribe: concluida "%s"', [Path])
    else Log('Transcribe: FALHOU "%s": %s', [Path, Err]);
    NotifyChanged;
  end;
end;

procedure EnsureStarted;
// Cria fila/lock/thread na primeira necessidade. Chamado sempre da main.
begin
  if GLock = nil then GLock := TCriticalSection.Create;
  if GQueue = nil then GQueue := TList<string>.Create;
  if StopEvent = 0 then StopEvent := CreateEvent(nil, True, False, nil);
  if Worker = nil then
  begin
    // Nao-suspensa e sem Start explicito — pegadinha #45: criar suspensa
    // e chamar Start no proprio construtor dispara EThread por
    // double-resume.
    Worker := TTranscribeThread.Create(False);
    Worker.FreeOnTerminate := False;
  end;
end;

function AlreadyQueued(const APath: string): Boolean;
// Compara por PATH, nunca pelo nome de exibicao: duas pastas podem ter
// gravacoes homonimas, e enfileirar a mesma duas vezes gastaria minutos
// do servidor pra reescrever o mesmo arquivo.
var
  i: Integer;
begin
  Result := False;
  if GQueue = nil then Exit;
  for i := 0 to GQueue.Count - 1 do
    if SameText(GQueue[i], APath) then Exit(True);
  Result := GRunning and SameText(GCurrentPath, APath);
end;

procedure ResetBatchIfIdle;
// Zera os contadores do lote quando um NOVO lote comeca. Nao da pra
// zerar ao esvaziar a fila: a UI tem que continuar mostrando o
// resultado ("7 de 7") depois que tudo termina. Caller segura o GLock.
begin
  if GRunning then Exit;
  if (GQueue <> nil) and (GQueue.Count > 0) then Exit;
  GBatchTotal := 0;
  GDone := 0;
  GFailed := 0;
  GLastError := '';
  GLastErrorName := '';
end;

procedure Enqueue(const APath: string);
begin
  if APath = '' then Exit;
  EnsureStarted;
  GLock.Enter;
  try
    if AlreadyQueued(APath) then Exit;
    ResetBatchIfIdle;
    GQueue.Add(APath);
    Inc(GBatchTotal);
    Inc(GQueueRev);
  finally
    GLock.Leave;
  end;
  Log('Transcribe: enfileirado "%s" (fila=%d)', [APath, QueueLength]);
  NotifyChanged;
end;

procedure EnqueueMany(const APaths: TArray<string>);
var
  i, Added: Integer;
begin
  if Length(APaths) = 0 then Exit;
  EnsureStarted;
  Added := 0;
  GLock.Enter;
  try
    ResetBatchIfIdle;
    for i := 0 to High(APaths) do
    begin
      if APaths[i] = '' then Continue;
      if AlreadyQueued(APaths[i]) then Continue;
      GQueue.Add(APaths[i]);
      Inc(GBatchTotal);
      Inc(GQueueRev);
      Inc(Added);
    end;
  finally
    GLock.Leave;
  end;
  Log('Transcribe: %d item(ns) enfileirado(s).', [Added]);
  NotifyChanged;
end;

procedure CancelAll;
begin
  if GLock = nil then Exit;
  GLock.Enter;
  try
    if GQueue <> nil then GQueue.Clear;
    Inc(GQueueRev);
    // Agora o item EM CURSO tambem para: a API ganhou DELETE /jobs/{id}.
    // A worker ve esta flag na proxima consulta (no maximo 1s), manda o
    // DELETE e desiste. Antes, sem rota de cancelamento, ele tinha que
    // ir ate o fim.
    GCancelCurrent := True;
    GBatchTotal := 0;
    GDone := 0;
    GFailed := 0;
    GLastError := '';
    GLastErrorName := '';
  finally
    GLock.Leave;
  end;
  Log('Transcribe: fila cancelada.');
  NotifyChanged;
end;

procedure Shutdown;
begin
  OnChanged := nil;
  if Worker <> nil then
  begin
    Worker.Terminate;
    if StopEvent <> 0 then SetEvent(StopEvent);
    // Espera curta: se o POST em curso ainda esta em voo, nao da pra
    // aborta-lo — deixa o OS limpar no fim do processo em vez de segurar
    // o fechamento do app.
    if WaitForSingleObject(Worker.Handle, 2000) = WAIT_TIMEOUT then
    begin
      // POST em voo nao da pra abortar (a API nao tem cancelamento).
      // Abandona a thread e VAZA fila/lock de proposito: libera-los aqui
      // seria uso-apos-liberacao no proximo tique dela. O OS limpa tudo
      // no fim do processo — mesma escolha do OBSRecordWatch.
      Log('Transcribe: worker nao parou em 2s — abandonando (cleanup pelo OS).');
      Worker := nil;
      Exit;
    end;
    FreeAndNil(Worker);
  end;
  if StopEvent <> 0 then
  begin
    CloseHandle(StopEvent);
    StopEvent := 0;
  end;
  if GQueue <> nil then FreeAndNil(GQueue);
  if GLock <> nil then FreeAndNil(GLock);
end;

end.

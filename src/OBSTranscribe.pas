(*
  OBSTranscribe - fila de transcricao contra a Transcritor API
  (https://github.com/e-delphi/transcritor-api), um container local com
  faster-whisper + diarizacao por falante.

  Desenho em tres pontos que nao sao obvios:

  1. MANDA AUDIO, NUNCA O VIDEO. A API tem MAX_UPLOAD_MB=1024 por padrao
     (era 512 ate a v2) e uma gravacao 4K passa disso facil. O audio sai
     em m4a pelo ExtractAudioTracks: 1 hora da ~70 MB.

     E manda as faixas ISOLADAS, uma transcricao por faixa, nao a
     mistura: cada faixa e um dispositivo, com o nome dele escrito no MKV
     — entao a atribuicao de falante sai de graca, com "Microfone (X)" no
     lugar de SPEAKER_00. Custa N vezes mais tempo de servidor, entao so
     vale com 2+ isoladas; com uma so a mistura tem o mesmo conteudo.
     Faixa muda nem e enviada. Ver PlanTracks.

     O preco disso e o ECO: o que sai pelos alto-falantes volta pelo
     microfone e o mesmo trecho e transcrito duas vezes. O
     MergeTranscripts derruba a copia — ver o comentario dele.

  2. MODELO ASSINCRONO, COM PROGRESSO REAL. POST /jobs devolve um id na
     hora; GET /jobs/{id} da estagio, percentual e ETA — e o `result` so
     com ?incluir_resultado=true (mudou na v2; ver STATUS_QUERY). O
     percentual mede o trecho de audio ja coberto pelos segmentos — nao
     ha estimativa nossa. Avanca aos saltos (janelas de 30s do Whisper),
     e o ETA so existe a partir de ~10%.

     Etapas da v2: queued -> decoding -> transcribing -> aligning ->
     diarizing -> done. O `aligning` e novo (WhisperX alinha as palavras
     com wav2vec2 antes de separar falantes).

     O id do job e o SHA-256 do AUDIO combinado com as opcoes, entao
     reenviar a mesma gravacao devolve o resultado pronto na hora, com
     HTTP 200 e `cached: true` em vez de 202. Retentar depois de uma falha
     de rede custa so o upload.

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
// Faixa em transcricao e quantas ha, quando a gravacao esta sendo mandada
// por faixas isoladas. TrackCount = 1 = faixa unica (a mistura, ou uma
// isolada so) — a UI nao mostra nada nesse caso.
function CurrentTrack: Integer;
function CurrentTrackCount: Integer;
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
  System.Generics.Defaults,
  System.Character,
  System.Math,
  OBSLog,
  OBSLang,
  OBSProbe,
  OBSConfig,
  OBSPlayer,
  FFmpegOps;

const
  // Rotas FIXAS de proposito: o usuario informa host e porta, nao o
  // caminho. O modelo e ASSINCRONO — POST /jobs devolve um id na hora e o
  // andamento sai de GET /jobs/{id}.
  JOBS_PATH   = '/jobs';
  HEALTH_PATH = '/health';
  // A v2 da API separou andamento de resultado: o GET /jobs/{id} devolve
  // so o progresso, e o JSON da transcricao vem neste parametro. (O outro
  // caminho seria o GET /jobs/{id}/download, que existe pra json/txt/srt/
  // vtt — mas ai seriam duas requisicoes pra ter a mesma coisa.)
  STATUS_QUERY = '?incluir_resultado=true';

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

  // --- deduplicacao entre faixas ---
  // Com faixas isoladas, o que sai pelos alto-falantes VOLTA pelo
  // microfone (a menos que o usuario use fone). O mesmo trecho e entao
  // transcrito duas vezes, e a copia precisa cair fora.
  //
  // Dois testes, os DOIS obrigatorios — cada um sozinho tem falso
  // positivo obvio: so tempo derrubaria duas pessoas falando junto (que
  // e exatamente o que faixas isoladas existem pra preservar), e so
  // texto derrubaria uma repeticao legitima minutos depois.
  DEDUP_OVERLAP_MIN = 0.5;   // fracao do turno mais curto em comum
  DEDUP_SIM_MIN     = 0.7;   // fracao das palavras do turno CURTO no longo
  // Turno de uma palavra so nao entra: "sim"/"certo" esta contido em
  // quase qualquer frase mais longa, e derrubar um "sim" simultaneo por
  // isso seria falso positivo garantido.
  DEDUP_MIN_WORDS   = 2;

  // --- recorte de turnos a partir das PALAVRAS ---
  // O `turns` da API agrupa por MUDANCA DE FALANTE. Na mistura os
  // falantes se alternam e isso da turnos curtos; numa faixa isolada de
  // uma pessoa so NAO HA troca, e a faixa inteira vira um turno unico —
  // medido: 22,5 s num bloco so, com o player destacando esse bloco
  // enquanto as palavras acontecem em algum lugar dentro dele.
  //
  // Por isso os turnos sao remontados a partir do timestamp POR PALAVRA
  // que a resposta ja traz (segments[].words). Corta em: troca de
  // falante, pausa, fim de frase depois de um tempo, e um teto duro pra
  // fala continua que nunca respira.
  TURN_GAP_SEC      = 1.0;   // silencio que ja separa duas falas
  TURN_SOFT_MAX_SEC = 8.0;   // a partir daqui, corta no fim de frase
  TURN_HARD_MAX_SEC = 20.0;  // fala continua: corta de qualquer jeito

  // Faixa cujo pico nunca passa disso nao vai pro servidor: e o
  // microfone que ninguem usou, ou o alto-falante de uma gravacao muda.
  // Mandar assim so gastaria minutos de fila pra receber zero turnos.
  SILENCE_PEAK_MIN = 0.02;
  SILENCE_BUCKETS  = 400;

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
  GTrack: Integer = 0;
  GTrackCount: Integer = 1;

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

function CurrentTrack: Integer;      begin Result := GTrack; end;
function CurrentTrackCount: Integer; begin Result := GTrackCount; end;

procedure SetTrack(ATrack, ACount: Integer);
// Qual faixa esta sendo transcrita. So notifica quando MUDA: e uma vez
// por faixa, nao a cada tique.
var
  Mudou: Boolean;
begin
  if GLock = nil then Exit;
  GLock.Enter;
  try
    Mudou := (GTrack <> ATrack) or (GTrackCount <> ACount);
    GTrack := ATrack;
    GTrackCount := ACount;
  finally
    GLock.Leave;
  end;
  if Mudou then NotifyChanged;
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
      // Alinhamento por palavra. O padrao da API ja e true, mas vai
      // EXPLICITO porque o TurnsFromWords depende dele: sem os
      // `segments[].words` o recorte de turnos cai no `turns` cru da API,
      // que so quebra em troca de falante (pegadinha #60k).
      Data.AddField('alignment', 'true');
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
      Resp := Http.Get(HostBase + JOBS_PATH + '/' + AJobId + STATUS_QUERY);
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
      // /transcribe devolve. E so vem porque pedimos: desde a v2 da API o
      // GET /jobs/{id} traz SO o andamento por padrao, e o resultado
      // depende do ?incluir_resultado=true (ver STATUS_QUERY). Sem ele a
      // transcricao terminava e o job "concluia sem resultado".
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

type
  TTurn = record
    Speaker: string;      // rotulo final (nome da faixa, + falante se houver)
    StartS, EndS: Double;
    Text: string;
    Track: Integer;       // de qual faixa veio — o dedup so cruza faixas
    Drop: Boolean;
  end;

function NormalizeForCompare(const S: string): string;
// Minusculas, so letras/digitos, espaco unico. A pontuacao do Whisper
// varia entre duas passagens do MESMO trecho (o eco chega mais fraco),
// entao compara-la geraria falso negativo no dedup.
var
  i: Integer;
  Ch: Char;
  SB: TStringBuilder;
  LastSpace: Boolean;
begin
  SB := TStringBuilder.Create;
  try
    LastSpace := True;
    for i := 1 to Length(S) do
    begin
      Ch := S[i];
      if Ch.IsLetterOrDigit then
      begin
        SB.Append(Ch.ToLower);
        LastSpace := False;
      end
      else if not LastSpace then
      begin
        SB.Append(' ');
        LastSpace := True;
      end;
    end;
    Result := Trim(SB.ToString);
  finally
    SB.Free;
  end;
end;

function WordSimilarity(const A, B: string): Double;
// CONTENCAO: quantas das palavras do turno CURTO aparecem no longo.
//
// Nao e Dice (2*comuns/(total+total)), e a diferenca decide o caso que
// mais importa: o eco PARCIAL. O microfone costuma pegar so um pedaco do
// que saiu no alto-falante, e Dice pune diferenca de tamanho — "e adiar
// tudo" dentro de "entao eu acho que a melhor saida aqui e adiar tudo
// pra semana que vem" da 0,33 no Dice (nao dedupa) e 1,00 aqui.
//
// Medido sobre 10 pares (5 ecos reais, 5 falas legitimas): Dice pegou
// 4/5 ecos, contencao pegou 5/5 — os dois com zero falso positivo,
// desde que o turno curto tenha 2+ palavras.
//
// Palavra e nao caractere de proposito: o eco troca uma palavra aqui e
// ali, mas as demais coincidem; distancia de edicao seria mais cara e
// mais sensivel justamente a essas trocas.
var
  WA, WB: TArray<string>;
  UsedB: TArray<Boolean>;
  i, j, Common, Shortest: Integer;
begin
  Result := 0;
  WA := A.Split([' '], TStringSplitOptions.ExcludeEmpty);
  WB := B.Split([' '], TStringSplitOptions.ExcludeEmpty);
  if (Length(WA) = 0) or (Length(WB) = 0) then Exit;
  Shortest := Min(Length(WA), Length(WB));
  if Shortest < DEDUP_MIN_WORDS then Exit;
  SetLength(UsedB, Length(WB));
  Common := 0;
  for i := 0 to High(WA) do
    for j := 0 to High(WB) do
      if (not UsedB[j]) and (WA[i] = WB[j]) then
      begin
        UsedB[j] := True;
        Inc(Common);
        Break;
      end;
  Result := Common / Shortest;
end;

function TimeOverlapRatio(const A, B: TTurn): Double;
// Fracao do turno mais CURTO coberta pela intersecao. Dividir pelo mais
// curto e deliberado: o eco costuma sair picado em turnos menores, e
// dividir pela uniao faria um turno curto dentro de um longo pontuar
// baixo justamente no caso que interessa.
var
  Inter, Shortest: Double;
begin
  Result := 0;
  Inter := Min(A.EndS, B.EndS) - Max(A.StartS, B.StartS);
  if Inter <= 0 then Exit;
  Shortest := Min(A.EndS - A.StartS, B.EndS - B.StartS);
  if Shortest <= 0 then Exit;
  Result := Inter / Shortest;
end;

function PlanTracks(const APath: string; out ANames: TArray<string>;
  out AStreams: TArray<Integer>): Boolean;
// Decide QUAIS streams de audio vao pra transcricao.
//
// Stream 0 e a MISTURA; 1..N-1 sao as ISOLADAS por dispositivo. Mandar
// as isoladas separadas da a atribuicao de falante DE GRACA — cada faixa
// e um dispositivo, e o nome dele foi escrito como titulo da stream no
// MKV pelo OBSEngine. Diarizacao dentro da faixa vira detalhe, nao a
// unica pista.
//
// O custo e uma transcricao POR FAIXA, e o container roda uma de cada
// vez. Dai o corte: so vale com DUAS ou mais isoladas. Com uma so, a
// mistura tem exatamente o mesmo conteudo e sairia o mesmo texto pelo
// dobro do tempo.
var
  Rep: TProbeReport;
  Auds: TStreamArray;
  i: Integer;
begin
  SetLength(ANames, 0);
  SetLength(AStreams, 0);
  Result := False;

  if not GetConfigBool('transcribePerTrack', True) then Exit;
  if not Probe(APath, Rep) then Exit;
  Auds := Rep.AudioStreams;
  if Length(Auds) < 3 then Exit;   // mistura + no minimo 2 isoladas

  SetLength(ANames, Length(Auds) - 1);
  SetLength(AStreams, Length(Auds) - 1);
  for i := 1 to High(Auds) do
  begin
    AStreams[i - 1] := i;
    // O titulo vem do BuildTrackNames, gravado como metadata da stream.
    // Sem ele (gravacao de outra ferramenta) cai num rotulo generico.
    ANames[i - 1] := Trim(Auds[i].Title);
    if ANames[i - 1] = '' then
      ANames[i - 1] := OBSLang.T('transcript.trackN', ['n', IntToStr(i + 1)]);
  end;
  Result := True;
end;

function TrackHasSound(const AAudioPath: string): Boolean;
// Decoda a faixa e olha o pico. Faixa muda (microfone que ninguem usou,
// alto-falante de uma gravacao silenciosa) nao vai pro servidor: gastaria
// minutos de fila pra voltar com zero turnos. Decodar custa segundos; a
// transcricao custaria minutos.
var
  Peaks: TArray<Single>;
  i: Integer;
begin
  Result := True;   // na duvida, MANDA: perder fala e pior que gastar tempo
  if not FFmpegOps.ComputeAudioPeaks(AAudioPath, SILENCE_BUCKETS, Peaks) then Exit;
  if Length(Peaks) = 0 then Exit;
  for i := 0 to High(Peaks) do
    if Peaks[i] >= SILENCE_PEAK_MIN then Exit;
  Result := False;
end;

function RunJob(const AAudioPath: string; ATrack, ATrackCount: Integer;
  var AEstPerTrackSec: Integer; out ABody: string): string;
// Sobe UM arquivo de audio e acompanha ate o fim. '' = sucesso (JSON em
// ABody), CANCELED_MARK se o usuario cancelou, senao a mensagem de erro.
//
// O progresso publicado e o do CONJUNTO de faixas, nao o da faixa: com 3
// faixas, a segunda a 50% vale 50% do total, senao a barra voltaria pra
// zero tres vezes. E o "faltam" soma as faixas que ainda nem comecaram,
// usando o tempo medido da primeira que terminou — antes disso nao ha
// base pra estimar e sai so o da faixa atual.
var
  JobId, Status, Stage, JobErr, ResJson: string;
  Prog: Double;
  Eta, TotalEta: Integer;
  StartTick: Cardinal;

  function Overall(AInner: Double): Double;
  begin
    if ATrackCount <= 1 then Exit(AInner);
    if AInner < 0 then AInner := 0;
    Result := (ATrack + AInner) / ATrackCount;
  end;

begin
  ABody := '';
  StartTick := GetTickCount;
  SetTrack(ATrack, ATrackCount);
  SetStage('uploading', Overall(0), -1);
  Result := SubmitJob(AAudioPath, JobId);
  if Result <> '' then Exit;
  Log('Transcribe: job %s aceito (faixa %d de %d).',
    [JobId, ATrack + 1, ATrackCount]);

  while True do
  begin
    if ShuttingDown then
    begin
      DeleteJob(JobId);
      Exit(OBSLang.T('error.transcribe.shuttingDown'));
    end;
    if CancelRequested then
    begin
      DeleteJob(JobId);
      Exit(CANCELED_MARK);
    end;

    Result := PollJob(JobId, Status, Stage, JobErr, Prog, Eta, ResJson);
    if Result <> '' then Exit;

    if Status = 'done' then
    begin
      ABody := ResJson;
      if Trim(ABody) = '' then Exit(OBSLang.T('error.transcribe.emptyResult'));
      // A primeira faixa concluida vira a regua pras que faltam.
      if AEstPerTrackSec <= 0 then
        AEstPerTrackSec := Integer((GetTickCount - StartTick) div 1000);
      SetStage('done', Overall(1), -1);
      Result := '';
      Exit;
    end;
    if Status = 'error' then
    begin
      if JobErr = '' then JobErr := OBSLang.T('error.transcribe.serverSilent');
      Exit(JobErr);
    end;

    TotalEta := Eta;
    if (TotalEta >= 0) and (AEstPerTrackSec > 0) then
      Inc(TotalEta, (ATrackCount - ATrack - 1) * AEstPerTrackSec);
    SetStage(Stage, Overall(Prog), TotalEta);

    // Espera interrompivel: o StopEvent tira a thread daqui na hora, em
    // vez de deixar o fechamento do app esperando um Sleep.
    if WaitForSingleObject(StopEvent, POLL_INTERVAL_MS) = WAIT_OBJECT_0 then
    begin
      DeleteJob(JobId);
      Exit(OBSLang.T('error.transcribe.shuttingDown'));
    end;
  end;
end;

function EndsSentence(const S: string): Boolean;
// Ultima letra util e ponto final / interrogacao / exclamacao / reticencia.
var
  T: string;
begin
  T := TrimRight(S);
  Result := (T <> '') and CharInSet(T[Length(T)], ['.', '?', '!']);
end;

procedure AppendWord(var ADest: string; const AWord: string);
// Junta a palavra ao texto do turno inserindo o espaco SO quando falta.
//
// O formato mudou com o WhisperX: o faster-whisper devolvia a palavra
// com o espaco na frente (" Bom"), o alinhador devolve ela pelada
// ("Bom") — e e por isso que o _build_turns da propria API junta com
// " ".join(). Somar direto emendava tudo ("Bomdiapessoal"); somar sempre
// um espaco dobraria no formato antigo, que ainda aparece nos segmentos
// que nao passaram pelo alinhamento (idioma sem alinhador embutido).
begin
  if AWord = '' then Exit;
  if (ADest <> '') and (ADest[Length(ADest)] <> ' ') and (AWord[1] <> ' ') then
    ADest := ADest + ' ';
  ADest := ADest + AWord;
end;

procedure TurnsFromWords(AObj: TJSONObject; ATrack: Integer;
  var ATurns: TArray<TTurn>);
// Remonta os turnos a partir de segments[].words, que traz start/end/
// speaker POR PALAVRA. Ver o bloco TURN_* nas constantes pra o porque.
//
// Nao substitui a diarizacao: o `speaker` continua vindo da API, palavra
// a palavra. O que muda e ONDE o turno quebra — o da API so quebra em
// troca de falante, e uma faixa isolada raramente tem uma.
var
  SegsVal, WordsVal: TJSONValue;
  Segs, Words: TJSONArray;
  Seg, W: TJSONObject;
  i, j, n: Integer;
  WStart, WEnd: Double;
  WText, WSpk: string;
  HasTs, AnyTimed: Boolean;
  SegText, SegSpk, Pending: string;
  SegStart, SegEnd: Double;
  Cur: TTurn;
  Have: Boolean;
  Corta: Boolean;

  procedure Flush;
  begin
    if not Have then Exit;
    Cur.Text := Trim(Cur.Text);
    if Cur.Text <> '' then
    begin
      n := Length(ATurns);
      SetLength(ATurns, n + 1);
      ATurns[n] := Cur;
    end;
    Have := False;
  end;

begin
  SegsVal := AObj.GetValue('segments');
  if not (SegsVal is TJSONArray) then Exit;
  Segs := TJSONArray(SegsVal);
  Have := False;
  Cur := Default(TTurn);

  for i := 0 to Segs.Count - 1 do
  begin
    if not (Segs.Items[i] is TJSONObject) then Continue;
    Seg := TJSONObject(Segs.Items[i]);
    WordsVal := Seg.GetValue('words');
    if not (WordsVal is TJSONArray) then Continue;
    Words := TJSONArray(WordsVal);
    AnyTimed := False;
    Pending := '';
    for j := 0 to Words.Count - 1 do
    begin
      if not (Words.Items[j] is TJSONObject) then Continue;
      W := TJSONObject(Words.Items[j]);
      WText := ''; WSpk := ''; WStart := 0; WEnd := 0;
      W.TryGetValue<string>('word', WText);
      W.TryGetValue<string>('speaker', WSpk);
      // PALAVRA SEM TIMESTAMP. A v2 da API manda `word` sem `start`/`end`
      // quando o alinhador nao reconhece o token (numeros, simbolos) — e
      // manda de proposito, pra o texto nao perder pedaco. Sem esta
      // guarda o TryGetValue deixava os dois em 0 e a palavra ancorava o
      // turno no segundo ZERO da gravacao. O turno perde essas palavras;
      // o `text` do topo, que a busca le, continua inteiro.
      HasTs := W.TryGetValue<Double>('start', WStart);
      if not W.TryGetValue<Double>('end', WEnd) then WEnd := WStart;
      if Trim(WText) = '' then Continue;
      if not HasTs then
      begin
        // Guarda o texto e segue: ela entra no turno da proxima palavra
        // alinhada, sem mexer no inicio/fim dele. Descartar era o que a
        // API faz, mas aqui o turno e o que o usuario LE — "Custou 1500
        // reais" viraria "Custou reais".
        AppendWord(Pending, WText);
        Continue;
      end;
      // A API nao poe speaker na palavra em toda configuracao; cai no
      // do segmento pra o rotulo nao ficar vazio.
      if WSpk = '' then Seg.TryGetValue<string>('speaker', WSpk);

      if Have then
      begin
        Corta := (WSpk <> Cur.Speaker)
              or (WStart - Cur.EndS > TURN_GAP_SEC)
              or (WEnd - Cur.StartS > TURN_HARD_MAX_SEC)
              or ((WEnd - Cur.StartS > TURN_SOFT_MAX_SEC) and
                  EndsSentence(Cur.Text));
        if Corta then Flush;
      end;

      if not Have then
      begin
        Cur.Speaker := WSpk;
        Cur.StartS := WStart;
        Cur.Text := '';
        Cur.Track := ATrack;
        Cur.Drop := False;
        Have := True;
      end;
      AppendWord(Cur.Text, Pending);
      AppendWord(Cur.Text, WText);
      Pending := '';
      Cur.EndS := WEnd;
      AnyTimed := True;
    end;

    // Sobrou pendente no fim do segmento (ultima palavra sem timestamp):
    // entra no turno corrente, senao o texto dela sumiria.
    if Have and (Pending <> '') then
    begin
      AppendWord(Cur.Text, Pending);
      Pending := '';
    end;

    // Segmento inteiro sem palavra alinhada: usa o proprio segmento, que
    // tem start/end. E o mesmo fallback do _build_turns da API — sem ele
    // o trecho sumiria dos turnos, e some CALADO: o `text` do topo
    // continua completo, entao so o painel do player perde a fala.
    if not AnyTimed then
    begin
      Flush;
      SegText := '';
      SegSpk := '';
      SegStart := 0;
      SegEnd := 0;
      Seg.TryGetValue<string>('text', SegText);
      Seg.TryGetValue<string>('speaker', SegSpk);
      Seg.TryGetValue<Double>('start', SegStart);
      Seg.TryGetValue<Double>('end', SegEnd);
      if Trim(SegText) <> '' then
      begin
        Cur.Speaker := SegSpk;
        Cur.StartS := SegStart;
        Cur.EndS := SegEnd;
        Cur.Text := SegText;
        Cur.Track := ATrack;
        Cur.Drop := False;
        Have := True;
        Flush;
      end;
    end;
  end;
  Flush;
end;

function RechunkTurns(const ABody: string; out ANewBody: string): Boolean;
// Troca o `turns` da resposta pelos turnos remontados das PALAVRAS,
// preservando todo o resto (segments, words, language, by_speaker...).
//
// Vale pro caminho da MISTURA, que guarda o JSON da API como veio. O
// defeito e o mesmo do caminho por faixa: o `turns` da API so quebra em
// troca de falante, entao uma fala corrida de uma pessoa vira um bloco
// unico. Medido numa gravacao real: um turno de 10,04 s com SETE
// segundos de silencio no meio; remontado, virou dois de ~2 s.
var
  Root: TJSONValue;
  Obj: TJSONObject;
  Turns: TArray<TTurn>;
  Arr: TJSONArray;
  Item: TJSONObject;
  Old: TJSONPair;
  i: Integer;
begin
  Result := False;
  ANewBody := ABody;
  Root := TJSONObject.ParseJSONValue(ABody);
  if not (Root is TJSONObject) then
  begin
    if Root <> nil then Root.Free;
    Exit;
  end;
  try
    Obj := TJSONObject(Root);
    SetLength(Turns, 0);
    TurnsFromWords(Obj, 0, Turns);
    // Sem word timestamps nao ha o que remontar — deixa como veio.
    if Length(Turns) = 0 then Exit;

    Arr := TJSONArray.Create;
    for i := 0 to High(Turns) do
    begin
      Item := TJSONObject.Create;
      Item.AddPair('speaker', Turns[i].Speaker);
      Item.AddPair('start', TJSONNumber.Create(Turns[i].StartS));
      Item.AddPair('end', TJSONNumber.Create(Turns[i].EndS));
      Item.AddPair('text', Turns[i].Text);
      Arr.AddElement(Item);
    end;
    Old := Obj.RemovePair('turns');
    if Old <> nil then Old.Free;
    Obj.AddPair('turns', Arr);
    ANewBody := Obj.ToJSON;
    Result := True;
  finally
    Root.Free;
  end;
end;

function CollectTurns(const ABody, ATrackName: string; ATrack: Integer;
  var ATurns: TArray<TTurn>): Boolean;
// Le os turnos de UMA faixa e rotula o falante.
//
// A regra do rotulo: se a faixa tem um falante so (o caso do microfone),
// o nome do DISPOSITIVO ja diz tudo e o SPEAKER_00 seria ruido. Se tem
// mais de um (o caso do alto-falante numa reuniao), o nome da faixa vira
// prefixo e a diarizacao continua distinguindo quem e quem dentro dela.
var
  Root: TJSONValue;
  Obj, T: TJSONObject;
  Arr: TJSONValue;
  A: TJSONArray;
  i, n: Integer;
  Spk, Txt: string;
  St, En: Double;
  Distinct: TStringList;
  Tmp: TArray<TTurn>;
begin
  Result := False;
  if Trim(ABody) = '' then Exit;
  Root := TJSONObject.ParseJSONValue(ABody);
  if not (Root is TJSONObject) then
  begin
    if Root <> nil then Root.Free;
    Exit;
  end;
  Distinct := TStringList.Create;
  try
    Distinct.Sorted := True;
    Distinct.Duplicates := dupIgnore;
    Obj := TJSONObject(Root);
    SetLength(Tmp, 0);

    // Caminho bom: remonta pelas PALAVRAS (ver o bloco TURN_*). Sem isto,
    // faixa de uma pessoa so vira um turno unico de dezenas de segundos.
    TurnsFromWords(Obj, ATrack, Tmp);

    // Resposta sem word timestamps: usa o `turns` como veio. Fica grosso,
    // mas e melhor que nao ter turno nenhum.
    if Length(Tmp) = 0 then
    begin
      Arr := Obj.GetValue('turns');
      if not (Arr is TJSONArray) then Exit(True);   // faixa sem fala: ok
      A := TJSONArray(Arr);
      for i := 0 to A.Count - 1 do
      begin
        if not (A.Items[i] is TJSONObject) then Continue;
        T := TJSONObject(A.Items[i]);
        Spk := ''; Txt := ''; St := 0; En := 0;
        T.TryGetValue<string>('speaker', Spk);
        T.TryGetValue<string>('text', Txt);
        T.TryGetValue<Double>('start', St);
        T.TryGetValue<Double>('end', En);
        if Trim(Txt) = '' then Continue;
        n := Length(Tmp);
        SetLength(Tmp, n + 1);
        Tmp[n].Speaker := Trim(Spk);
        Tmp[n].StartS := St;
        Tmp[n].EndS := En;
        Tmp[n].Text := Trim(Txt);
        Tmp[n].Track := ATrack;
        Tmp[n].Drop := False;
      end;
    end;

    for i := 0 to High(Tmp) do
      if Tmp[i].Speaker <> '' then Distinct.Add(Tmp[i].Speaker);

    for i := 0 to High(Tmp) do
    begin
      if (Distinct.Count > 1) and (Tmp[i].Speaker <> '') then
        Tmp[i].Speaker := ATrackName + ' · ' + Tmp[i].Speaker
      else
        Tmp[i].Speaker := ATrackName;
      n := Length(ATurns);
      SetLength(ATurns, n + 1);
      ATurns[n] := Tmp[i];
    end;
    Result := True;
  finally
    Distinct.Free;
    Root.Free;
  end;
end;

function MergeTranscripts(const ABodies, ANames: TArray<string>;
  out AMerged, AText: string): Boolean;
// Junta as faixas numa transcricao so, no MESMO formato que a API
// devolve pra uma faixa unica ({turns:[{speaker,start,end,text}], text}).
// Manter o formato e o que faz o painel do player e a busca continuarem
// funcionando sem saber que isto existe.
//
// DEDUPLICACAO: o que sai pelos alto-falantes volta pelo microfone, e o
// mesmo trecho aparece nas duas faixas. Turnos de faixas DIFERENTES que
// se sobrepoem no tempo E dizem quase a mesma coisa sao a mesma fala
// capturada duas vezes — fica a versao com MAIS palavras, que na pratica
// e a da fonte direta: o eco chega mais fraco e o Whisper corta pedacos.
//
// O criterio nao tenta adivinhar qual faixa e microfone e qual e
// alto-falante. Poderia (o titulo da faixa vem do BuildTrackNames), mas
// seria casar texto traduzivel — e o "fica o mais completo" resolve os
// dois sentidos do eco sem depender disso.
var
  Turns: TArray<TTurn>;
  i, j, Kept: Integer;
  Sim, Ov: Double;
  NA, NB: string;
  Norm: TArray<string>;
  Obj: TJSONObject;
  Arr, TracksArr: TJSONArray;
  Item: TJSONObject;
  SB: TStringBuilder;
begin
  Result := False;
  AMerged := '';
  AText := '';
  SetLength(Turns, 0);
  for i := 0 to High(ABodies) do
    if not CollectTurns(ABodies[i], ANames[i], i, Turns) then
      Exit;

  // Ordena por inicio: o dedup abaixo depende disso pra so olhar a
  // janela vizinha em vez de todos contra todos.
  TArray.Sort<TTurn>(Turns, TComparer<TTurn>.Construct(
    function(const A, B: TTurn): Integer
    begin
      Result := CompareValue(A.StartS, B.StartS);
      if Result = 0 then Result := CompareValue(A.Track, B.Track);
    end));

  SetLength(Norm, Length(Turns));
  for i := 0 to High(Turns) do Norm[i] := NormalizeForCompare(Turns[i].Text);

  for i := 0 to High(Turns) do
  begin
    if Turns[i].Drop then Continue;
    j := i + 1;
    // Ordenado por inicio: assim que um turno comeca depois do fim
    // deste, nenhum dos seguintes se sobrepoe.
    while (j <= High(Turns)) and (Turns[j].StartS < Turns[i].EndS) do
    begin
      if Turns[j].Drop or (Turns[j].Track = Turns[i].Track) then
      begin
        Inc(j);
        Continue;
      end;
      Ov := TimeOverlapRatio(Turns[i], Turns[j]);
      if Ov >= DEDUP_OVERLAP_MIN then
      begin
        NA := Norm[i];
        NB := Norm[j];
        Sim := WordSimilarity(NA, NB);
        if Sim >= DEDUP_SIM_MIN then
        begin
          // Fica a versao com MAIS palavras: o eco chega mais fraco e o
          // Whisper corta pedacos dele.
          if Length(NB) > Length(NA) then Turns[i].Drop := True
          else Turns[j].Drop := True;
          if Turns[i].Drop then
            Log('Transcribe: eco descartado (sobrep=%.2f cont=%.2f) "%s"',
              [Ov, Sim, Copy(Turns[i].Text, 1, 40)])
          else
            Log('Transcribe: eco descartado (sobrep=%.2f cont=%.2f) "%s"',
              [Ov, Sim, Copy(Turns[j].Text, 1, 40)]);
          if Turns[i].Drop then Break;
        end;
      end;
      Inc(j);
    end;
  end;

  Obj := TJSONObject.Create;
  SB := TStringBuilder.Create;
  try
    Arr := TJSONArray.Create;
    Kept := 0;
    for i := 0 to High(Turns) do
    begin
      if Turns[i].Drop then Continue;
      Item := TJSONObject.Create;
      Item.AddPair('speaker', Turns[i].Speaker);
      Item.AddPair('start', TJSONNumber.Create(Turns[i].StartS));
      Item.AddPair('end', TJSONNumber.Create(Turns[i].EndS));
      Item.AddPair('text', Turns[i].Text);
      Arr.AddElement(Item);
      if SB.Length > 0 then SB.Append(' ');
      SB.Append(Turns[i].Text);
      Inc(Kept);
    end;
    AText := SB.ToString;
    Obj.AddPair('turns', Arr);
    Obj.AddPair('text', AText);
    // Proveniencia: quais faixas entraram. Nao e lido por ninguem hoje,
    // mas e a unica pista de que este arquivo veio de varias faixas.
    TracksArr := TJSONArray.Create;
    for i := 0 to High(ANames) do TracksArr.Add(ANames[i]);
    Obj.AddPair('tracks', TracksArr);
    AMerged := Obj.ToJSON;
  finally
    SB.Free;
    Obj.Free;
  end;
  Log('Transcribe: %d faixas -> %d turnos (%d descartados por eco).',
    [Length(ABodies), Kept, Length(Turns) - Kept]);
  Result := True;
end;

function ProcessOne(const APath: string): string;
// Transcreve UMA gravacao. Devolve '' em sucesso, senao a mensagem.
// Roda inteiro na worker thread.
//
// Duas formas, decididas pelo PlanTracks:
//   1 faixa  — a MISTURA (stream 0), como sempre foi.
//   N faixas — as ISOLADAS por dispositivo, uma transcricao cada, o
//              resultado juntado e desduplicado pelo MergeTranscripts.
// A segunda custa N vezes mais tempo de servidor e entrega atribuicao de
// falante de verdade, com o nome do dispositivo em vez de SPEAKER_00.
var
  Body, Txt, Merged: string;
  Files, Send, Names, Bodies, UsedNames: TArray<string>;
  Streams: TArray<Integer>;
  CacheBase: string;
  HealthErr: string;
  PerTrack: Boolean;
  Est, i, n: Integer;

  procedure CleanupFiles;
  var
    k: Integer;
  begin
    // Os m4a sao so veiculo do upload — regeneram em segundos, e uma
    // hora de audio ocupa ~70 MB que nao vale guardar.
    for k := 0 to High(Files) do
      try if TFile.Exists(Files[k]) then TFile.Delete(Files[k]); except end;
  end;

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

  CacheBase := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath);

  PerTrack := PlanTracks(APath, Names, Streams);
  if not PerTrack then
  begin
    // Caminho antigo: SO O AUDIO da mistura. O video passaria do
    // MAX_UPLOAD_MB da API (512 MB) numa gravacao 4K de poucos minutos.
    SetLength(Names, 1);
    SetLength(Streams, 1);
    Names[0] := '';
    Streams[0] := 0;
  end;

  SetLength(Files, Length(Streams));
  for i := 0 to High(Files) do
    Files[i] := Format('%s_tr%d.m4a', [CacheBase, Streams[i]]);

  try
    CleanupFiles;
    SetStage('extracting', 0, -1);
    // UMA passada de demux pra todas as faixas. Streams[0] e o indice do
    // primeiro audio pedido; como PlanTracks devolve sempre um intervalo
    // contiguo comecando em Streams[0], o start index cobre o resto.
    if not FFmpegOps.ExtractAudioTracks(APath, Files, Streams[0]) then
      Exit(OBSLang.T('error.transcribe.extractFailed'));

    // Faixa muda nao vai pro servidor. Com varias faixas isto e o que
    // impede o microfone que ninguem usou de gastar uma fila inteira.
    SetLength(Bodies, 0);
    SetLength(UsedNames, 0);
    SetLength(Send, 0);
    n := 0;
    for i := 0 to High(Files) do
    begin
      if not TFile.Exists(Files[i]) then
        Exit(OBSLang.T('error.transcribe.extractMissing'));
      if PerTrack and not TrackHasSound(Files[i]) then
      begin
        Log('Transcribe: faixa "%s" sem som — nao vai pro servidor.', [Names[i]]);
        Continue;
      end;
      SetLength(UsedNames, n + 1);
      SetLength(Send, n + 1);
      UsedNames[n] := Names[i];
      // Lista SEPARADA: compactar o Files em si mesmo perderia os paths
      // das faixas puladas, e o CleanupFiles deixaria .m4a orfaos no cache.
      Send[n] := Files[i];
      Inc(n);
    end;

    // Todas mudas: e uma gravacao sem fala, nao uma falha. Escreve a
    // transcricao vazia — o app distingue "sem fala" de "nao transcrita"
    // (transcribed=True, has=False) e o player diz a coisa certa.
    if n = 0 then
    begin
      Log('Transcribe: nenhuma faixa com som em "%s".', [APath]);
      Merged := '{"turns":[],"text":""}';
      Txt := '';
    end
    else
    begin
      Est := 0;
      SetLength(Bodies, n);
      for i := 0 to n - 1 do
      begin
        Result := RunJob(Send[i], i, n, Est, Body);
        if Result <> '' then Exit;
        Bodies[i] := Body;
      end;

      // So a MISTURA passa direto: o JSON da API ja esta no formato
      // final, e reescreve-lo jogaria fora os segmentos com timestamp por
      // palavra que o merge nao carrega.
      //
      // Por faixa, mesmo com UMA sobrando (as outras mudas), o merge
      // roda: e ele quem troca o SPEAKER_00 pelo nome do dispositivo, e
      // sair com rotulo generico justo na faixa que tem a fala anularia
      // o motivo de mandar por faixa.
      if not PerTrack then
      begin
        Merged := Bodies[0];
        // Mesmo aqui os turnos sao remontados: o `turns` da API quebra so
        // em troca de falante, e o player destaca o bloco inteiro.
        // O resto da resposta (segments, words) e preservado.
        var Rechunked: string;
        if RechunkTurns(Merged, Rechunked) then Merged := Rechunked;
        var Json := TJSONObject.ParseJSONValue(Merged);
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
      end
      else if not MergeTranscripts(Bodies, UsedNames, Merged, Txt) then
        Exit(OBSLang.T('error.transcribe.resultNotJson'));
    end;

    // Dois arquivos: o JSON pro player, o texto puro pra busca.
    // Ver o cabecalho da unit.
    try
      TFile.WriteAllText(TranscriptPath(APath), Merged, TEncoding.UTF8);
      TFile.WriteAllText(TranscriptTextPath(APath), Txt, TEncoding.UTF8);
    except
      on E: Exception do Exit(OBSLang.T('error.transcribe.writeFailed', ['error', E.Message]));
    end;
    Result := '';
  finally
    CleanupFiles;
    SetTrack(0, 1);
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
          GTrack := 0;
          GTrackCount := 1;
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

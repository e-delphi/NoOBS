(*
  OBSTranscribe - fila de transcricao contra a Transcritor API
  (https://github.com/e-delphi/transcritor-api), um container local com
  faster-whisper + diarizacao por falante.

  Desenho em tres pontos que nao sao obvios:

  1. MANDA AUDIO, NUNCA O VIDEO. A API tem MAX_UPLOAD_MB=512 por padrao e
     uma gravacao 4K passa disso facil. A faixa de MISTURA (stream 0, que
     ja tem tudo) sai em m4a pelo ExtractAudioTracks: 1 hora da ~70 MB.

  2. NAO HA ROTA DE PROGRESSO. O POST /transcribe so responde no fim.
     Entao "progresso" aqui e posicao na fila + tempo decorrido + uma
     ESTIMATIVA derivada do ~1,5x tempo real que o README da API mede. A
     UI mostra isso como estimativa declarada; fingir porcentagem exata
     seria mentir.

  3. UMA POR VEZ. O proprio container serializa as requisicoes (os
     modelos nao sao thread-safe), entao paralelizar aqui so encheria a
     fila do outro lado. Uma thread, uma fila.

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
function CurrentEstimateSec: Integer; // 0 = desconhecido
function DoneCount: Integer;
function FailedCount: Integer;
function LastError: string;
// Total planejado do lote atual (pra "3 de 7"). Zera quando a fila esvazia.
function BatchTotal: Integer;

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
  System.Math,
  NoOBSTypes,
  OBSLog,
  OBSConfig,
  OBSPlayer,
  FFmpegOps;

const
  // Rota FIXA de proposito: o usuario informa host e porta, nao o caminho.
  TRANSCRIBE_PATH = '/transcribe';
  HEALTH_PATH     = '/health';

  DEFAULT_HOST = 'http://localhost:8000';

  // O README da API mede ~1,5x mais rapido que o tempo real com large-v3.
  // Usamos o inverso disso como estimativa, com uma folga: preferimos a
  // barra chegar no fim e esperar um pouco a passar do fim e ficar presa.
  ESTIMATE_FACTOR = 0.80;

  HEALTH_TIMEOUT_MS = 4000;
  // Conexao e rapida; a RESPOSTA e que demora (minutos a horas). O
  // ResponseTimeout e calculado por item a partir da duracao.
  CONNECT_TIMEOUT_MS = 5000;
  MIN_RESPONSE_TIMEOUT_MS = 10 * 60 * 1000;    // 10 min de piso

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
  GCurrentEstimate: Integer = 0;
  GDone: Integer = 0;
  GFailed: Integer = 0;
  GBatchTotal: Integer = 0;
  GLastError: string = '';

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

function CurrentEstimateSec: Integer;
begin
  Result := GCurrentEstimate;
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

function CheckHealth(const AHost: string): string;
// Roda na worker (a UI chama por um botao "Testar"). '' = ok.
var
  Http: TNetHTTPClient;
  Resp: IHTTPResponse;
  Base: string;
begin
  Base := Trim(AHost);
  while (Base <> '') and (Base[Length(Base)] = '/') do
    Delete(Base, Length(Base), 1);
  if Base = '' then Exit('endereco vazio');
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := HEALTH_TIMEOUT_MS;
    Http.ResponseTimeout := HEALTH_TIMEOUT_MS;
    try
      Resp := Http.Get(Base + HEALTH_PATH);
    except
      on E: Exception do Exit(E.Message);
    end;
    if Resp = nil then Exit('sem resposta');
    if Resp.StatusCode <> 200 then
      Exit(Format('HTTP %d', [Resp.StatusCode]));
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

function PostTranscription(const AAudioPath: string; ATimeoutMs: Integer;
  out ABody: string): string;
// POST multipart. Devolve '' em sucesso; senao a mensagem de erro.
var
  Http: TNetHTTPClient;
  Data: TMultipartFormData;
  Resp: IHTTPResponse;
  Lang: string;
begin
  ABody := '';
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    // A resposta so vem no FIM da transcricao — minutos, as vezes horas.
    Http.ResponseTimeout := ATimeoutMs;
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
        Resp := Http.Post(HostBase + TRANSCRIBE_PATH, Data);
      except
        on E: Exception do Exit(E.Message);
      end;
      if Resp = nil then Exit('sem resposta');
      if Resp.StatusCode <> 200 then
        Exit(Format('HTTP %d', [Resp.StatusCode]));
      ABody := Resp.ContentAsString(TEncoding.UTF8);
      if Trim(ABody) = '' then Exit('resposta vazia');
      Result := '';
    finally
      Data.Free;
    end;
  finally
    Http.Free;
  end;
end;

function ProcessOne(const APath: string): string;
// Transcreve UMA gravacao. Devolve '' em sucesso, senao a mensagem.
// Roda inteiro na worker thread.
var
  Meta: TRecordingMeta;
  Audio, Body, Txt: string;
  TimeoutMs: Integer;
  Json: TJSONValue;
begin
  if not TFile.Exists(APath) then Exit('arquivo nao existe');

  // Duracao: alimenta a ESTIMATIVA mostrada na UI e o timeout do POST.
  Meta := Default(TRecordingMeta);
  try OBSPlayer.LoadRecordingMeta(APath, Meta); except end;
  if Meta.DurationSec > 0 then
    GCurrentEstimate := Round(Meta.DurationSec * ESTIMATE_FACTOR)
  else
    GCurrentEstimate := 0;

  // Timeout generoso: 6x a duracao, com piso. Um servidor lento (CPU
  // fraca, modelo grande) pode passar bem do 1,5x medido no README, e
  // derrubar por timeout uma transcricao que ia terminar seria pior que
  // esperar.
  TimeoutMs := MIN_RESPONSE_TIMEOUT_MS;
  if Meta.DurationSec > 0 then
    TimeoutMs := Max(TimeoutMs, Meta.DurationSec * 6 * 1000);

  // SO O AUDIO. O video passaria do MAX_UPLOAD_MB da API (512 MB) numa
  // gravacao 4K de poucos minutos. Indice 0 = faixa de MISTURA, que ja
  // tem todos os microfones e o som do sistema juntos.
  Audio := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath) + '_tr.m4a';
  try if TFile.Exists(Audio) then TFile.Delete(Audio); except end;
  if not FFmpegOps.ExtractAudioTracks(APath, [Audio], 0) then
    Exit('falha ao extrair o audio');
  if not TFile.Exists(Audio) then Exit('audio extraido nao encontrado');

  try
    Result := PostTranscription(Audio, TimeoutMs, Body);
    if Result <> '' then Exit;

    Json := TJSONObject.ParseJSONValue(Body);
    if not (Json is TJSONObject) then
    begin
      if Json <> nil then Json.Free;
      Exit('resposta nao e JSON');
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
      on E: Exception do Exit('falha ao gravar: ' + E.Message);
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
          GRunning := True;
          GCurrentPath := Path;
          GCurrentName := ChangeFileExt(ExtractFileName(Path), '');
          GCurrentStartTick := GetTickCount;
          GCurrentEstimate := 0;
        end
        else
        begin
          GRunning := False;
          GCurrentPath := '';
          GCurrentName := '';
          GCurrentStartTick := 0;
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
      if Err = '' then Inc(GDone)
      else
      begin
        Inc(GFailed);
        GLastError := Err;
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
    // O item EM CURSO nao e abortado: a API nao tem cancelamento e o
    // POST ja esta em voo. Ele termina e o resultado e gravado — nao ha
    // motivo pra jogar fora trabalho que ja foi feito do outro lado.
    GBatchTotal := 0;
    GDone := 0;
    GFailed := 0;
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

(*
  OBSLocalAsr - transcricao LOCAL, na GPU, sem Docker nem servidor a
  instalar a parte.

  O motor e o Qwen3-ASR (texto) com o Qwen3-ForcedAligner (instante de cada
  palavra), servidos pelo audio.cpp (https://github.com/0xShug0/audio.cpp)
  com backend Vulkan — roda em placa AMD, NVIDIA e Intel sem CUDA/ROCm.
  Medido numa reuniao de 88 min numa RX 9070 XT: ~7 min, com o inicio de
  cada palavra a ~22 ms (mediana) do instante real. Nao separa falantes:
  com faixas isoladas o falante continua vindo do nome do dispositivo
  (OBSTranscribe.LabelTrackSpeakers).

  Tres pontos que nao sao obvios:

  1. O audio.cpp NAO tem biblioteca (DLL) — so executaveis. Entao o motor e
     o audiocpp_server.exe rodando ESCONDIDO, numa porta livre de
     127.0.0.1, subido sob demanda e derrubado depois de IDLE_STOP_MS sem
     uso (os modelos ocupam ~4 GB de memoria de video, que o jogo gravado
     precisa). Fica num Job Object com KILL_ON_JOB_CLOSE: se o NoOBS morrer
     de qualquer jeito, o servidor morre junto.

  2. A LOGICA e a mesma da Transcritor API (versao Windows, app/engine.py),
     traduzida: so as REGIOES DE FALA vao pro modelo (ruido e silencio
     longo viram texto inventado), em blocos de ate ~60 s cortados no
     trecho mais silencioso,
     DUAS passadas (transcrever tudo, depois alinhar tudo — trocar de modelo
     a cada bloco custaria segundos por bloco), a pontuacao do texto
     recolocada nas palavras do alinhador (que as devolve sem ela) e os
     numeros por extenso em algarismos (OBSNumbersPt). Mudou la, mude aqui.

  3. O REAMOSTRADOR e nosso: sinc janelado (Blackman), 8 cruzamentos por
     lado, tabela com interpolacao linear. As libs do OBS trazem o
     swresample, mas usa-lo exigiria declarar structs de ABI da libav; a
     conta e curta e foi validada em Python antes (mesma matematica):
     48 kHz -> 16 kHz deu erro mediano de 22 ms no inicio das palavras,
     igual ao swresample. Filtro simetrico = sem atraso de fase — um
     reamostrador com atraso deslocaria TODAS as palavras.

  Arquivos em %LOCALAPPDATA%\NoOBS\asr\ (audiocpp\, models\, server.json,
  audiocpp.log). A instalacao baixa ~3,7 GB, retoma download interrompido
  e confere o SHA-256 de cada arquivo antes de aceita-lo. Se o GitHub ou o
  HuggingFace falharem, cada arquivo e buscado num espelho no Google Drive
  (ENGINE_MIRROR).
*)
unit OBSLocalAsr;

interface

type
  TLocalAsrStatus = (lasMissing, lasInstalling, lasReady, lasError);

  TLocalAsrState = record
    Status: TLocalAsrStatus;
    // Durante a instalacao: 'engine' | 'asr' | 'align' | 'verifying' |
    // 'extracting' | 'detecting'.
    Stage: string;
    BytesDone: Int64;
    BytesTotal: Int64;
    Error: string;       // motivo quando Status = lasError
    Device: string;      // GPU em uso ("AMD Radeon RX 9070 XT"); '' = CPU
  end;

  // Chamado SEMPRE na main thread.
  TLocalAsrChanged = procedure;

  // Etapa ('decoding' | 'starting' | 'transcribing' | 'aligning') e fracao
  // 0..1 do arquivo inteiro. Chamado na thread de quem transcreve.
  TLocalAsrProgress = reference to procedure(const AStage: string; AFraction: Double);
  // True = pare (cancelamento ou fechamento do app).
  TLocalAsrCancel = reference to function: Boolean;

function GetState: TLocalAsrState;
// Motor e os dois modelos presentes e conferidos.
function IsInstalled: Boolean;
// Tamanho total do download (pra UI dizer quanto vai baixar).
function InstallSizeBytes: Int64;
// Instala em worker thread. No-op se ja instalado ou instalando.
procedure StartInstall;
procedure CancelInstall;
// Apaga motor e modelos. False (sem fazer nada) se estiver instalando ou
// transcrevendo.
function Uninstall: Boolean;
procedure SetOnChanged(ACallback: TLocalAsrChanged);

// Transcreve UM arquivo de audio (qualquer formato que a libav leia).
// '' = sucesso, com o JSON em ABody no MESMO formato da Transcritor API
// (segments[].words com start/end), senao a mensagem de erro. ACanceled =
// parou por ACancel (nao e erro). ALanguage = codigo ISO ('pt', 'en') ou
// '' pra detectar.
function Transcribe(const AAudioPath, ALanguage: string;
  AOnProgress: TLocalAsrProgress; ACancel: TLocalAsrCancel;
  out ABody: string; out ACanceled: Boolean): string;

// Derruba o servidor se esta ocioso ha IDLE_STOP_MS. Chamado pela worker
// da transcricao quando a fila esta vazia (barato: so compara tempos).
procedure StopIfIdle;
// Derruba o servidor e para a instalacao. Chamado no fechamento.
procedure Shutdown;

implementation

uses
  Winapi.Windows,
  Winapi.Winsock2,
  System.SysUtils,
  System.Classes,
  System.Math,
  System.JSON,
  System.IOUtils,
  System.Hash,
  System.Zip,
  System.SyncObjs,
  System.Character,
  System.Generics.Collections,
  System.Generics.Defaults,
  System.StrUtils,
  System.Net.HttpClient,
  System.Net.URLClient,
  System.Net.Mime,
  Winapi.WinHTTP,
  OBSLog,
  OBSLang,
  OBSConfig,
  OBSNumbersPt,
  OBSTranscribe,
  FFmpegOps;

const
  // --- o que se baixa (versoes FIXAS, conferidas por SHA-256) ---
  ENGINE_ZIP  = 'audio-v0.8.1-bin-windows-x64-vulkan.zip';
  ENGINE_URL  = 'https://github.com/0xShug0/audio.cpp/releases/download/v0.8.1/' + ENGINE_ZIP;
  ENGINE_SHA  = 'c787971e025ba8ef900f0482a2cc36a049367081fe89f4841aae521a0b49de32';
  ENGINE_SIZE = Int64(58059664);
  SERVER_EXE  = 'audiocpp_server.exe';
  CLI_EXE     = 'audiocpp_cli.exe';

  HF_BASE     = 'https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/main/';
  ASR_FILE    = 'qwen3-asr-1.7b-q8_0.gguf';
  ASR_URL     = HF_BASE + 'Qwen3-ASR-1.7B-GGUF/' + ASR_FILE;
  ASR_SHA     = 'da4fc2ac7f24dee784d1684eb1f35836cdbf559519452ae11777670734c0a4f8';
  ASR_SIZE    = Int64(2473010048);
  ALIGN_FILE  = 'qwen3-forced-aligner-0.6b-q8_0.gguf';
  ALIGN_URL   = HF_BASE + 'Qwen3-ForcedAligner-0.6B-GGUF/' + ALIGN_FILE;
  ALIGN_SHA   = '75209490b11cec2b0db749ca5f4ff92266f58efd30f7fd04d9eb2a3ac9cc929f';
  ALIGN_SIZE  = Int64(1129966496);

  // Escrito so depois de TUDO baixado, conferido e detectado. Sem ele a
  // instalacao conta como incompleta, mesmo com os arquivos no lugar.
  MARKER_FILE = 'instalado.json';

  ASR_MODEL_ID   = 'qwen3-asr';
  ALIGN_MODEL_ID = 'qwen3-align';

  // --- audio ---
  RATE      = 16000;
  FRAME     = 480;       // 30 ms a 16 kHz: a unidade do corte em blocos
  // Blocos de ate ~60 s: o alinhador recusa audio acima de ~120 s. O corte
  // e procurado nos ultimos 15 s do bloco. (1999, e nao 2000: e o que o
  // int(60 / 0.03) do Python da, e os dois lados cortam no mesmo lugar.)
  MAX_CHUNK_FRAMES = 1999;
  SEARCH_FRAMES    = 500;
  SMOOTH_FRAMES    = 10;  // media movel da energia antes de achar o vale
  SILENCE_PCT      = 20;  // percentil de energia tratado como silencio
  // Bloco com menos que isto de quadros "com voz" e silencio: nao vai pro
  // modelo. O Qwen3 inventa texto em silencio longo (medido: 20 min de
  // silencio viraram frases soltas no WhisperX; aqui, nada).
  MIN_VOICED       = 0.05;
  // So as REGIOES DE FALA vao pro modelo. Em ruido ou silencio longo o
  // Qwen3 inventa frases (medido: 90 s de ruido com estalos viraram "E um
  // dos mais importantes..."), e um bloco sem fala nenhuma faz o audio.cpp
  // responder 500 — o que derrubava o arquivo inteiro.
  MIN_RUN_FRAMES    = 4;    // 120 ms: acima do limiar por menos que isso e estalo
  MIN_SPEECH_FRAMES = 17;   // 0,5 s: regiao com menos fala que isso e descartada
  SPLIT_GAP_FRAMES  = 167;  // 5 s: silencio maior que isso separa blocos
  PAD_FRAMES        = 10;   // 0,3 s de folga antes e depois da fala

  // Laco do modelo: em musica ou ruido continuo o Qwen3 as vezes repete a
  // mesma palavra ou frase ate o limite (medido numa gravacao de 2 h: blocos
  // de 50 s com 258 palavras e so 3 distintas). Frase de ate LOOP_MAX_UNIT
  // palavras repetida LOOP_MIN_REPS vezes seguidas fica com LOOP_KEEP.
  LOOP_MAX_UNIT = 8;
  LOOP_MIN_REPS = 4;
  LOOP_KEEP     = 2;
  // Pontuacao colada no fim da palavra (a da ultima repeticao e mantida).
  LOOP_PUNCT    = '.,;:!?…»"'')]';

  RESAMPLE_ZEROS = 8;     // cruzamentos de zero do sinc por lado
  TABLE_RES      = 512;   // pontos da tabela por amostra de entrada

  // Peso da 1a passada no progresso; o alinhamento leva o resto.
  TRANSCRIBE_SHARE = 0.5;

  // --- servidor ---
  SERVER_START_TIMEOUT_MS = 90000;
  HEALTH_TIMEOUT_MS       = 2000;
  // Um bloco de 60 s leva ~2 s na GPU; na CPU pode passar de um minuto.
  REQUEST_TIMEOUT_MS      = 15 * 60 * 1000;
  // Ocioso por mais que isto, o servidor cai e libera a memoria de video.
  // Subir de novo custa ~3 s + carregar os modelos na 1a requisicao.
  IDLE_STOP_MS            = 3 * 60 * 1000;

  // Idiomas que o alinhador aceita, pelo nome que ele espera. O ASR
  // devolve o CODIGO quando o idioma e pedido ("pt") e o NOME quando
  // detecta sozinho ("Portuguese") — dai a tabela nos dois sentidos.
  LANG_CODES: array[0..10] of string =
    ('pt', 'en', 'es', 'fr', 'de', 'it', 'ru', 'ko', 'ja', 'zh', 'yue');
  LANG_NAMES: array[0..10] of string =
    ('Portuguese', 'English', 'Spanish', 'French', 'German', 'Italian',
     'Russian', 'Korean', 'Japanese', 'Chinese', 'Cantonese');

  CANCELED_MARK = #1'canceled';
  // O audio.cpp responde 500 com esta frase quando o bloco nao tem texto
  // reconhecivel. Nao e erro do arquivo: o bloco fica vazio e segue.
  NO_SPEECH_MARK = #1'nospeech';
  NO_SPEECH_TEXT = 'did not contain transcript text';

type
  // Arquivo do espelho no Google Drive (ver ENGINE_MIRROR).
  TMirrorFile = record
    Path: string;     // relativo a pasta do motor
    Id: string;       // id do arquivo no Drive
    Size: Int64;
    Sha: string;
  end;

  // Trecho igual entre as duas listas de palavras (ver Opcodes).
  TBlock = record
    I, J, K: Integer;
  end;

  TOpcode = record
    Tag: Char;    // 'e' igual, 'r' troca, 'd' so em A, 'i' so em B
    A0, A1, B0, B1: Integer;
  end;

  TChunk = record
    StartF, EndF: Integer;   // em quadros de FRAME amostras
    Text: string;
    Lang: string;            // codigo ('pt') ou nome em minusculas
    Aligned: Boolean;
    Words: TJSONArray;       // dono: o segmento do JSON final
  end;

  // Reamostrador de fluxo: recebe blocos na taxa original e acumula a saida
  // em 16 kHz, 16 bits. Guarda da entrada so a janela ainda necessaria.
  TResampler = class
  private
    FReady: Boolean;
    FSrcRate: Integer;
    FRatio: Double;          // entrada por saida (48000/16000 = 3)
    FWidth: Double;          // meia largura do filtro, em amostras de entrada
    FTable: TArray<Double>;
    // Razao inteira (48k, 32k): os instantes de saida caem em amostras
    // inteiras e os pesos sao sempre os mesmos — calculados uma vez.
    FIntRatio: Integer;
    FIntTaps: TArray<Double>;
    FIntHalf: Integer;
    FBuf: TArray<Single>;
    FBufStart: Int64;        // indice absoluto de FBuf[0]
    FBufLen: Integer;
    FTotalIn: Int64;
    FNextOut: Int64;
    procedure Setup(ARate: Integer);
    function Kernel(D: Double): Double; inline;
    procedure Emit(V: Double); inline;
    procedure Produce(AFinal: Boolean);
  public
    Output: TArray<SmallInt>;
    OutLen: Int64;
    function Push(AData: System.PSingle; ACount, ARate: Integer): Boolean;
    procedure Finish;
  end;

const
  // --- ESPELHO no Google Drive ---
  // Segunda fonte, pra quem nao alcanca o GitHub ou o HuggingFace (rede
  // corporativa, bloqueio regional, fora do ar). Mesmos arquivos, mesmos
  // SHA-256 — conferidos um a um contra os originais. O link direto do
  // drive.usercontent aceita Range (retoma) e, com confirm=t, nao para na
  // pagina de aviso de antivirus dos arquivos grandes.
  //
  // O Drive nao tem o zip do motor, so os arquivos soltos: vao os 14 que o
  // servidor precisa (testado: sobe, transcreve e alinha so com eles). Os
  // ids NAO mudam ao apagar outros arquivos da pasta ou mover estes; mudam
  // se o arquivo for enviado de novo — ai e preciso atualizar aqui.
  DRIVE_URL = 'https://drive.usercontent.google.com/download?id=%s&export=download&confirm=t';
  ASR_DRIVE_ID   = '1BF6NgHuCLrrSkn8xlSbaPe6WfECoYhTb';
  ALIGN_DRIVE_ID = '1jC-4mX6-TLZHUh8dL7rWLyGrJivL28kL';
  ENGINE_MIRROR_SIZE = Int64(167614090);
  ENGINE_MIRROR: array[0..13] of TMirrorFile = (
    (Path: 'audiocpp_server.exe'; Id: '1EH-dt419IuyxIEcg-3QvFpTRQwvoViXE';
     Size: 87112704; Sha: '737ae419af34bfe31764cd7741fb227e3d7764e23a34224db8cce611c0f5d4c2'),
    (Path: 'audiocpp_cli.exe'; Id: '1u4BntT8rF4SSVEl9Jvjg54vCUvcfnZXw';
     Size: 79118336; Sha: '7256d03a7e708a30d4c6dbf9865425ffb6004a4c46126fb9a2350fc00ae0f713'),
    (Path: 'LICENSE'; Id: '15gmk-WXo8dwuGOsuPJdZVugYamK49U0W';
     Size: 10466; Sha: '7e125adced85b7f84c04371179d15d51fbd702cefd060a37043fe38d6eb1722e'),
    (Path: 'msvcp140.dll'; Id: '1CNp0dZ90mdzmOBpgH8fj2E9k-locXaDR';
     Size: 557728; Sha: '0f885b509a685d2bbfa652fed26b5fb31d88fbdab0a978c641d1c7b8aa460aa9'),
    (Path: 'msvcp140_1.dll'; Id: '1twhHlvSpV6Np1fckWDdfkH22GysXIHGr';
     Size: 35952; Sha: 'bfad5aef4c63a669e3c140655cdfdf395b6c979b400a447bd5dcb65ed8826c3d'),
    (Path: 'msvcp140_2.dll'; Id: '1_YWdULRF1F55HvSZvzbLhl-JlcVQR3Qf';
     Size: 280200; Sha: '3ea06f0ee098b4823cb79599df3780e7f23cce52c19aac31d2a0d47efe33a5e9'),
    (Path: 'msvcp140_atomic_wait.dll'; Id: '1sqaL9xT0P-BHVrBGgzwqjcpjQ1FlkARX';
     Size: 50304; Sha: '640b2aefced484d0368eea5bdd06addd0658a3a70a49256e560d6923b404a479'),
    (Path: 'msvcp140_codecvt_ids.dll'; Id: '1sVSL-HHpHXUujUZOV7lAJRPB0UZ9U64i';
     Size: 31872; Sha: 'f2069a52880ec885ee7f0511186100eb7fada0411a2b4948fafea7735b878a18'),
    (Path: 'vcomp140.dll'; Id: '1vBNF18_EOOOf9YQbJJFZldV-a0dzMg53';
     Size: 193152; Sha: '55aba23cdcd6484fbb06f4155b8ca75adfce7a881f10afd0c49457165e677164'),
    (Path: 'vcruntime140.dll'; Id: '1CejUkn0s-NKjTOVLhPzeRmAS6hfxZ4lp';
     Size: 124544; Sha: 'd5e4d9a3e835fa679450145d6a7d94e36573a509317111904d9b3712c30d9066'),
    (Path: 'vcruntime140_1.dll'; Id: '14KDGWf9qFpzgbRvgrxCPFRB0UWIrJio0';
     Size: 49792; Sha: '1f2d41c4aa5db0bc33ebf7b66d72943a817d7ce6cbe880502a9403823633093f'),
    (Path: 'vcruntime140_threads.dll'; Id: '1PU2Tk0uk5ZabwZrWIELMZilMFy27xoZZ';
     Size: 38528; Sha: '219915cf20822f34d5e7c1fdd4e21ae7f3396881096c51036225fb8f84b47afa'),
    (Path: 'model_specs\qwen3_asr.json'; Id: '13xrvMCAYpdmYuDm5E5CF7_ljIYQubg-t';
     Size: 6117; Sha: 'af83793bf6e75af3bd99c0dabd43102fe3f85ca030b9c5d12aafcc3f87cd23b4'),
    (Path: 'model_specs\qwen3_forced_aligner.json'; Id: '12sJ9a7OZDVp-vqhKTO7DUFox6xIm8x0J';
     Size: 4395; Sha: '18e4805a2224e93968892038d7c02c42ae4f75a9bc105f13293cc42aed3a0f94'));

var
  GLock: TCriticalSection = nil;
  GState: TLocalAsrState;
  GStateLoaded: Boolean = False;
  GOnChanged: TLocalAsrChanged = nil;
  GInstallCancel: Boolean = False;
  GShuttingDown: Boolean = False;
  GLastNotify: UInt64 = 0;

  GServerLock: TCriticalSection = nil;
  GProc: THandle = 0;          // processo do audiocpp_server (0 = parado)
  GJob: THandle = 0;
  GPort: Integer = 0;
  GLastUse: UInt64 = 0;
  GBusy: Integer = 0;          // transcricoes em curso (Interlocked)

// =====================================================================
// Caminhos
// =====================================================================

function RootDir: string;
var
  Base: string;
begin
  Base := GetEnvironmentVariable('LOCALAPPDATA');
  if Base = '' then Base := GetEnvironmentVariable('APPDATA');
  Result := IncludeTrailingPathDelimiter(Base) + 'NoOBS\asr\';
end;

function EngineDir: string; begin Result := RootDir + 'audiocpp\'; end;
function ModelsDir: string; begin Result := RootDir + 'models\'; end;
function LogPath: string;   begin Result := RootDir + 'audiocpp.log'; end;

function FileSize64(const APath: string): Int64;
// -1 = nao existe. GetFileAttributesEx le metadado, nao abre o arquivo.
var
  Data: TWin32FileAttributeData;
begin
  if not GetFileAttributesEx(PChar(APath), GetFileExInfoStandard, @Data) then
    Exit(-1);
  Result := (Int64(Data.nFileSizeHigh) shl 32) or Data.nFileSizeLow;
end;

function InstallSizeBytes: Int64;
begin
  Result := ENGINE_SIZE + ASR_SIZE + ALIGN_SIZE;
end;

function IsInstalled: Boolean;
begin
  Result := FileExists(RootDir + MARKER_FILE) and
    FileExists(EngineDir + SERVER_EXE) and
    (FileSize64(ModelsDir + ASR_FILE) = ASR_SIZE) and
    (FileSize64(ModelsDir + ALIGN_FILE) = ALIGN_SIZE);
end;

// =====================================================================
// Estado e notificacao
// =====================================================================

procedure SetOnChanged(ACallback: TLocalAsrChanged);
begin
  GOnChanged := ACallback;
end;

procedure NotifyChanged(AForce: Boolean);
// Marshalla pra main. O download avisa a cada pedaco recebido; sem o
// limite de 250 ms seriam milhares de mensagens por segundo pra UI.
var
  T0: UInt64;
begin
  T0 := GetTickCount64;
  if (not AForce) and (T0 - GLastNotify < 250) then Exit;
  GLastNotify := T0;
  TThread.Queue(nil,
    procedure
    begin
      if Assigned(GOnChanged) then
        try GOnChanged; except end;
    end);
end;

procedure EnsureStateLoaded;
// Caller segura o GLock.
begin
  if GStateLoaded then Exit;
  GStateLoaded := True;
  GState := Default(TLocalAsrState);
  GState.BytesTotal := InstallSizeBytes;
  if IsInstalled then
  begin
    GState.Status := lasReady;
    GState.BytesDone := GState.BytesTotal;
  end
  else
    GState.Status := lasMissing;
  GState.Device := GetConfigStr('localAsrDevice', '');
end;

function GetState: TLocalAsrState;
begin
  GLock.Enter;
  try
    EnsureStateLoaded;
    Result := GState;
  finally
    GLock.Leave;
  end;
end;

procedure SetInstallProgress(const AStage: string; ADone: Int64);
begin
  GLock.Enter;
  try
    GState.Stage := AStage;
    GState.BytesDone := ADone;
  finally
    GLock.Leave;
  end;
  NotifyChanged(False);
end;

// =====================================================================
// Instalacao
// =====================================================================

function DownloadResume(const AUrl, APart, AName: string; ASize, ABase: Int64;
  const AStage: string; AScale: Double = 1): string;
// Baixa AUrl em APart, RETOMANDO de onde parou (Range). '' = ok,
// CANCELED_MARK = cancelado, senao a mensagem. HuggingFace e GitHub
// redirecionam pra CDN; o THTTPClient segue o redirecionamento.
var
  Http: THTTPClient;
  Fs: TFileStream;
  Have: Int64;
  Resp: IHTTPResponse;
  Headers: TNetHeaders;
  Attempt: Integer;
  ErrMsg: string;
begin
  for Attempt := 1 to 2 do
  begin
    Have := FileSize64(APart);
    if Have > ASize then
    begin
      DeleteFile(APart);
      Have := -1;
    end;
    if Have = ASize then Exit('');
    if Have > 0 then
    begin
      Fs := TFileStream.Create(APart, fmOpenReadWrite or fmShareDenyWrite);
      Fs.Seek(0, soEnd);
    end
    else
    begin
      Have := 0;
      Fs := TFileStream.Create(APart, fmCreate);
    end;
    Resp := nil;
    ErrMsg := '';
    try
      Http := THTTPClient.Create;
      try
        Http.ConnectionTimeout := 15000;
        // Tempo sem receber NADA, nao tempo total: 2,4 GB levam minutos.
        Http.ResponseTimeout := 60000;
        Http.HandleRedirects := True;
        Http.UserAgent := 'NoOBS';
        Http.ReceiveDataCallback :=
          procedure(const Sender: TObject; AContentLength, AReadCount: Int64;
            var AAbort: Boolean)
          begin
            SetInstallProgress(AStage, ABase + Round((Have + AReadCount) * AScale));
            AAbort := GInstallCancel or GShuttingDown;
          end;
        SetLength(Headers, 0);
        if Have > 0 then
          Headers := [TNameValuePair.Create('Range', Format('bytes=%d-', [Have]))];
        try
          Resp := Http.Get(AUrl, Fs, Headers);
        except
          on E: Exception do ErrMsg := E.Message;
        end;
      finally
        Http.Free;
      end;
    finally
      Fs.Free;
    end;

    if GInstallCancel or GShuttingDown then Exit(CANCELED_MARK);
    if ErrMsg <> '' then
      Exit(OBSLang.T('error.localAsr.download', ['file', AName, 'error', ErrMsg]));
    if Resp = nil then
      Exit(OBSLang.T('error.localAsr.download', ['file', AName, 'error', '?']));
    case Resp.StatusCode of
      206: ;
      200:
        // O servidor ignorou o Range e mandou o arquivo INTEIRO depois do
        // pedaco que ja existia. Nao da pra salvar: recomeca do zero.
        if Have > 0 then
        begin
          Log('LocalAsr: %s ignorou o Range — recomecando o download.', [AName]);
          DeleteFile(APart);
          Continue;
        end;
    else
      Exit(OBSLang.T('error.localAsr.download',
        ['file', AName, 'error', 'HTTP ' + IntToStr(Resp.StatusCode)]));
    end;
    if FileSize64(APart) <> ASize then
      Exit(OBSLang.T('error.localAsr.download',
        ['file', AName, 'error', Format('%d de %d bytes', [FileSize64(APart), ASize])]));
    Exit('');
  end;
  Result := OBSLang.T('error.localAsr.download', ['file', AName, 'error', 'Range']);
end;

function Fetch(const AStage, AUrl, ADest, ASha: string; ASize, ABase: Int64;
  AScale: Double = 1): string;
// Baixa, confere o SHA-256 e so entao poe no lugar definitivo. O arquivo
// final existir = ja foi conferido (so o .part pode estar pela metade).
var
  Part, Name: string;
begin
  if FileSize64(ADest) = ASize then Exit('');
  Name := ExtractFileName(ADest);
  Part := ADest + '.part';
  Log('LocalAsr: baixando %s', [Name]);
  Result := DownloadResume(AUrl, Part, Name, ASize, ABase, AStage, AScale);
  if Result <> '' then Exit;
  SetInstallProgress('verifying', ABase + Round(ASize * AScale));
  if not SameText(THashSHA2.GetHashStringFromFile(Part), ASha) then
  begin
    DeleteFile(Part);
    Exit(OBSLang.T('error.localAsr.checksum', ['file', Name]));
  end;
  if FileExists(ADest) then DeleteFile(ADest);
  if not RenameFile(Part, ADest) then
    Exit(OBSLang.T('error.localAsr.download', ['file', Name, 'error', SysErrorMessage(GetLastError)]));
end;

function ExtractEngine(const AZip: string): string;
var
  Tmp, Src: string;
  Found: TArray<string>;
begin
  Result := '';
  Tmp := RootDir + 'audiocpp.tmp\';
  try
    if TDirectory.Exists(Tmp) then TDirectory.Delete(Tmp, True);
    TZipFile.ExtractZipFile(AZip, Tmp);
    // O zip traz os executaveis na raiz; se um dia vier numa subpasta,
    // acha a pasta do servidor em vez de falhar.
    Src := Tmp;
    if not FileExists(Src + SERVER_EXE) then
    begin
      Found := TDirectory.GetFiles(Tmp, SERVER_EXE, TSearchOption.soAllDirectories);
      if Length(Found) = 0 then
        Exit(OBSLang.T('error.localAsr.extract', ['error', SERVER_EXE + '?']));
      Src := IncludeTrailingPathDelimiter(ExtractFilePath(Found[0]));
    end;
    if TDirectory.Exists(EngineDir) then TDirectory.Delete(EngineDir, True);
    TDirectory.Move(ExcludeTrailingPathDelimiter(Src), ExcludeTrailingPathDelimiter(EngineDir));
    if TDirectory.Exists(Tmp) then TDirectory.Delete(Tmp, True);
    DeleteFile(AZip);
  except
    on E: Exception do Result := OBSLang.T('error.localAsr.extract', ['error', E.Message]);
  end;
end;

function FetchEngineMirror: string;
// O motor pelo Google Drive, arquivo a arquivo (o Drive nao tem o zip).
// Pasta propria: o ExtractEngine apaga a audiocpp.tmp, e o que o espelho
// ja baixou tem de sobrar pra retomar.
var
  Tmp, Dest: string;
  i: Integer;
  Done: Int64;
  Scale: Double;
begin
  Result := '';
  Tmp := RootDir + 'audiocpp.mirror\';
  Scale := ENGINE_SIZE / ENGINE_MIRROR_SIZE;
  Done := 0;
  for i := 0 to High(ENGINE_MIRROR) do
  begin
    Dest := Tmp + ENGINE_MIRROR[i].Path;
    ForceDirectories(ExtractFilePath(Dest));
    Result := Fetch('engineMirror', Format(DRIVE_URL, [ENGINE_MIRROR[i].Id]), Dest,
      ENGINE_MIRROR[i].Sha, ENGINE_MIRROR[i].Size, Round(Done * Scale), Scale);
    if Result <> '' then Exit;
    Inc(Done, ENGINE_MIRROR[i].Size);
  end;
  try
    if TDirectory.Exists(EngineDir) then TDirectory.Delete(EngineDir, True);
    TDirectory.Move(ExcludeTrailingPathDelimiter(Tmp), ExcludeTrailingPathDelimiter(EngineDir));
    // Sobra do zip do GitHub que nao chegou a completar.
    DeleteFile(RootDir + ENGINE_ZIP + '.part');
    DeleteFile(RootDir + ENGINE_ZIP);
  except
    on E: Exception do Result := OBSLang.T('error.localAsr.extract', ['error', E.Message]);
  end;
end;

function WithMirror(const APrimaryErr, AWhat: string; AMirror: TFunc<string>): string;
// Falhou a fonte principal (qualquer motivo que nao seja cancelamento):
// tenta o espelho. Falhando os dois, a mensagem leva os dois motivos.
begin
  Result := APrimaryErr;
  if (Result = '') or (Result = CANCELED_MARK) then Exit;
  Log('LocalAsr: %s falhou na fonte principal (%s) — tentando o Google Drive.',
    [AWhat, APrimaryErr]);
  Result := AMirror();
  if (Result <> '') and (Result <> CANCELED_MARK) then
    Result := APrimaryErr + ' | Google Drive: ' + Result;
end;

function DetectDevice(out ABackend, ADevice: string): string;
// Pergunta ao proprio audio.cpp que placas ele enxerga. Linha que importa:
//   Vulkan:0 "AMD Radeon RX 9070 XT" [GPU]
// Sem Vulkan, roda na CPU — funciona, so que ~10x mais devagar.
var
  Output, Line: string;
  Code, q1, q2: Integer;
begin
  Result := '';
  ABackend := 'cpu';
  ADevice := '';
  Code := OBSTranscribe.RunHidden(EngineDir + CLI_EXE, '--list-devices', 60000, Output);
  Log('LocalAsr: --list-devices saiu com %d', [Code]);
  // -1/erro de carga: o executavel nem abriu (tipicamente falta a
  // vulkan-1.dll, que vem com o driver de video).
  if Code <> 0 then
    Exit(OBSLang.T('error.localAsr.noVulkan'));
  for Line in Output.Split([#10]) do
    if Trim(Line).StartsWith('Vulkan:') then
    begin
      q1 := Pos('"', Line);
      q2 := Pos('"', Line, q1 + 1);
      ABackend := 'vulkan';
      if (q1 > 0) and (q2 > q1) then ADevice := Copy(Line, q1 + 1, q2 - q1 - 1);
      Break;
    end;
end;

procedure InstallWorker;
var
  Err, Backend, Device: string;
  Marker: TJSONObject;
begin
  Err := '';
  Backend := '';
  Device := '';
  try
    ForceDirectories(ModelsDir);
    if not FileExists(EngineDir + SERVER_EXE) then
    begin
      Err := Fetch('engine', ENGINE_URL, RootDir + ENGINE_ZIP, ENGINE_SHA, ENGINE_SIZE, 0);
      if Err = '' then
      begin
        SetInstallProgress('extracting', ENGINE_SIZE);
        Err := ExtractEngine(RootDir + ENGINE_ZIP);
      end;
      Err := WithMirror(Err, 'o motor',
        function: string
        begin
          Result := FetchEngineMirror;
        end);
    end;
    // Modelos: o .part que parou no meio pelo HuggingFace continua pelo
    // Drive — sao os mesmos bytes.
    if Err = '' then
      Err := WithMirror(
        Fetch('asr', ASR_URL, ModelsDir + ASR_FILE, ASR_SHA, ASR_SIZE, ENGINE_SIZE),
        ASR_FILE,
        function: string
        begin
          Result := Fetch('asrMirror', Format(DRIVE_URL, [ASR_DRIVE_ID]),
            ModelsDir + ASR_FILE, ASR_SHA, ASR_SIZE, ENGINE_SIZE);
        end);
    if Err = '' then
      Err := WithMirror(
        Fetch('align', ALIGN_URL, ModelsDir + ALIGN_FILE, ALIGN_SHA, ALIGN_SIZE,
          ENGINE_SIZE + ASR_SIZE),
        ALIGN_FILE,
        function: string
        begin
          Result := Fetch('alignMirror', Format(DRIVE_URL, [ALIGN_DRIVE_ID]),
            ModelsDir + ALIGN_FILE, ALIGN_SHA, ALIGN_SIZE, ENGINE_SIZE + ASR_SIZE);
        end);
    if Err = '' then
    begin
      SetInstallProgress('detecting', InstallSizeBytes);
      Err := DetectDevice(Backend, Device);
    end;
    if Err = '' then
    begin
      SetConfigStr('localAsrBackend', Backend);
      SetConfigStr('localAsrDevice', Device);
      Marker := TJSONObject.Create;
      try
        Marker.AddPair('engine', ENGINE_ZIP);
        Marker.AddPair('asr', ASR_FILE);
        Marker.AddPair('align', ALIGN_FILE);
        Marker.AddPair('backend', Backend);
        Marker.AddPair('device', Device);
        TFile.WriteAllText(RootDir + MARKER_FILE, Marker.ToJSON, TEncoding.UTF8);
      finally
        Marker.Free;
      end;
      // Quem instalou quer usar: vira o motor da transcricao na hora.
      SetConfigStr('transcribeEngine', 'local');
      Log('LocalAsr: instalado (%s%s).', [Backend, IfThen(Device <> '', ', ' + Device, '')]);
    end;
  except
    on E: Exception do Err := E.Message;
  end;

  GLock.Enter;
  try
    if Err = CANCELED_MARK then
    begin
      GState.Status := lasMissing;
      GState.Error := '';
      Log('LocalAsr: instalacao cancelada (o que ja baixou fica pra retomar).');
    end
    else if Err <> '' then
    begin
      GState.Status := lasError;
      GState.Error := Err;
      Log('LocalAsr: instalacao falhou: %s', [Err]);
    end
    else
    begin
      GState.Status := lasReady;
      GState.Error := '';
      GState.BytesDone := GState.BytesTotal;
      GState.Device := Device;
    end;
    GState.Stage := '';
  finally
    GLock.Leave;
  end;
  NotifyChanged(True);
end;

procedure StartInstall;
begin
  GLock.Enter;
  try
    EnsureStateLoaded;
    if GState.Status in [lasInstalling, lasReady] then Exit;
    GState.Status := lasInstalling;
    GState.Error := '';
    GState.Stage := 'engine';
    GState.BytesDone := 0;
    GInstallCancel := False;
  finally
    GLock.Leave;
  end;
  NotifyChanged(True);
  TThread.CreateAnonymousThread(
    procedure
    begin
      InstallWorker;
    end).Start;
end;

procedure CancelInstall;
begin
  GInstallCancel := True;
end;

// =====================================================================
// Servidor
// =====================================================================

function FreePort: Integer;
// Porta livre de 127.0.0.1: o SO escolhe (bind na porta 0) e a gente
// solta. Entre soltar e o servidor pegar da uma corrida teorica; se
// perder, o servidor nao sobe e a proxima tentativa escolhe outra.
var
  Wsa: TWSAData;
  S: TSocket;
  Addr: TSockAddrIn;
  Len: Integer;
begin
  Result := 0;
  if WSAStartup($0202, Wsa) <> 0 then Exit;
  try
    S := socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if S = INVALID_SOCKET then Exit;
    try
      FillChar(Addr, SizeOf(Addr), 0);
      Addr.sin_family := AF_INET;
      Addr.sin_addr.S_addr := htonl(INADDR_LOOPBACK);
      Addr.sin_port := 0;
      if bind(S, PSockAddr(@Addr)^, SizeOf(Addr)) <> 0 then Exit;
      Len := SizeOf(Addr);
      if getsockname(S, PSockAddr(@Addr)^, Len) <> 0 then Exit;
      Result := ntohs(Addr.sin_port);
    finally
      closesocket(S);
    end;
  finally
    WSACleanup;
  end;
end;

function BaseUrl: string;
begin
  Result := Format('http://127.0.0.1:%d', [GPort]);
end;

function HealthOk: Boolean;
var
  Http: THTTPClient;
  Resp: IHTTPResponse;
begin
  Result := False;
  if GPort = 0 then Exit;
  Http := THTTPClient.Create;
  try
    Http.ConnectionTimeout := HEALTH_TIMEOUT_MS;
    Http.ResponseTimeout := HEALTH_TIMEOUT_MS;
    try
      Resp := Http.Get(BaseUrl + '/health');
      Result := (Resp <> nil) and (Resp.StatusCode = 200);
    except
      Result := False;
    end;
  finally
    Http.Free;
  end;
end;

function ProcessAlive: Boolean;
begin
  Result := (GProc <> 0) and (WaitForSingleObject(GProc, 0) = WAIT_TIMEOUT);
end;

procedure StopServerLocked;
// Caller segura o GServerLock.
begin
  if GProc <> 0 then
  begin
    if WaitForSingleObject(GProc, 0) = WAIT_TIMEOUT then
    begin
      TerminateProcess(GProc, 0);
      WaitForSingleObject(GProc, 3000);
      Log('LocalAsr: servidor encerrado.');
    end;
    CloseHandle(GProc);
    GProc := 0;
  end;
  GPort := 0;
end;

procedure EnsureJob;
// Job Object com KILL_ON_JOB_CLOSE: o handle morre com o NoOBS (fechado,
// travado, derrubado pelo Gerenciador de Tarefas) e o Windows mata o
// servidor junto. Sem isso sobraria um processo segurando 4 GB de VRAM.
var
  Info: TJobObjectExtendedLimitInformation;
begin
  if GJob <> 0 then Exit;
  GJob := CreateJobObject(nil, nil);
  if GJob = 0 then Exit;
  FillChar(Info, SizeOf(Info), 0);
  Info.BasicLimitInformation.LimitFlags := JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
  if not SetInformationJobObject(GJob, JobObjectExtendedLimitInformation,
    @Info, SizeOf(Info)) then
    Log('LocalAsr: SetInformationJobObject falhou (%d).', [GetLastError]);
end;

procedure WriteServerConfig(const ABackend: string);
var
  Cfg, M: TJSONObject;
  Models: TJSONArray;
begin
  Cfg := TJSONObject.Create;
  try
    Cfg.AddPair('host', '127.0.0.1');
    Cfg.AddPair('port', TJSONNumber.Create(GPort));
    Cfg.AddPair('backend', ABackend);
    Cfg.AddPair('device', TJSONNumber.Create(0));
    Cfg.AddPair('threads', TJSONNumber.Create(Min(TThread.ProcessorCount, 8)));
    Cfg.AddPair('lazy_load', TJSONBool.Create(True));
    // Os dois modelos juntos na memoria: e o que evita recarregar a cada
    // troca entre a passada de transcricao e a de alinhamento.
    Cfg.AddPair('max_loaded_models', TJSONNumber.Create(2));
    Cfg.AddPair('idle_unload_ms', TJSONNumber.Create(0));
    Cfg.AddPair('min_free_memory_mb', TJSONNumber.Create(0));
    Models := TJSONArray.Create;
    M := TJSONObject.Create;
    M.AddPair('id', ASR_MODEL_ID);
    M.AddPair('family', 'qwen3_asr');
    M.AddPair('task', 'asr');
    M.AddPair('mode', 'offline');
    M.AddPair('path', StringReplace(ModelsDir + ASR_FILE, '\', '/', [rfReplaceAll]));
    Models.AddElement(M);
    M := TJSONObject.Create;
    M.AddPair('id', ALIGN_MODEL_ID);
    M.AddPair('family', 'qwen3_forced_aligner');
    M.AddPair('task', 'align');
    M.AddPair('mode', 'offline');
    M.AddPair('path', StringReplace(ModelsDir + ALIGN_FILE, '\', '/', [rfReplaceAll]));
    Models.AddElement(M);
    Cfg.AddPair('models', Models);
    TFile.WriteAllText(RootDir + 'server.json', Cfg.ToJSON, TEncoding.UTF8);
  finally
    Cfg.Free;
  end;
end;

function EnsureServer(ACancel: TLocalAsrCancel): string;
// Sobe o servidor se ele nao esta de pe. '' = pronto, CANCELED_MARK, ou
// a mensagem. A espera checa o cancelamento e o fechamento a cada 250 ms,
// entao o Shutdown nunca fica preso aqui.
var
  Backend, Cmd: string;
  SA: TSecurityAttributes;
  SI: TStartupInfo;
  PI: TProcessInformation;
  LogH: THandle;
  Start: UInt64;
  Code: DWORD;
begin
  GServerLock.Enter;
  try
    if ProcessAlive and HealthOk then Exit('');
    StopServerLocked;

    GPort := FreePort;
    if GPort = 0 then Exit(OBSLang.T('error.localAsr.serverStart', ['log', LogPath]));
    Backend := GetConfigStr('localAsrBackend', 'vulkan');
    WriteServerConfig(Backend);

    // stdout/stderr num arquivo: e onde o audio.cpp diz POR QUE nao subiu
    // (driver, memoria de video, modelo). Recriado a cada subida.
    FillChar(SA, SizeOf(SA), 0);
    SA.nLength := SizeOf(SA);
    SA.bInheritHandle := True;
    LogH := CreateFile(PChar(LogPath), GENERIC_WRITE, FILE_SHARE_READ, @SA,
      CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
    FillChar(SI, SizeOf(SI), 0);
    SI.cb := SizeOf(SI);
    SI.dwFlags := STARTF_USESHOWWINDOW;
    SI.wShowWindow := SW_HIDE;
    if LogH <> INVALID_HANDLE_VALUE then
    begin
      SI.dwFlags := SI.dwFlags or STARTF_USESTDHANDLES;
      SI.hStdOutput := LogH;
      SI.hStdError := LogH;
    end;
    Cmd := Format('"%s" --config "%s" --no-ui',
      [EngineDir + SERVER_EXE, RootDir + 'server.json']);
    UniqueString(Cmd);
    FillChar(PI, SizeOf(PI), 0);
    try
      // SUSPENSO ate entrar no Job: senao ele poderia rodar (e deixar
      // filhos) fora do alcance do KILL_ON_JOB_CLOSE.
      if not CreateProcessW(nil, PChar(Cmd), nil, nil, LogH <> INVALID_HANDLE_VALUE,
        CREATE_NO_WINDOW or CREATE_SUSPENDED, nil, PChar(EngineDir), SI, PI) then
      begin
        Log('LocalAsr: CreateProcess falhou (%d).', [GetLastError]);
        Exit(OBSLang.T('error.localAsr.serverStart', ['log', LogPath]));
      end;
    finally
      if LogH <> INVALID_HANDLE_VALUE then CloseHandle(LogH);
    end;
    EnsureJob;
    if GJob <> 0 then AssignProcessToJobObject(GJob, PI.hProcess);
    ResumeThread(PI.hThread);
    CloseHandle(PI.hThread);
    GProc := PI.hProcess;
    Log('LocalAsr: servidor iniciando na porta %d (%s).', [GPort, Backend]);

    Start := GetTickCount64;
    while True do
    begin
      if GShuttingDown or (Assigned(ACancel) and ACancel()) then
      begin
        StopServerLocked;
        Exit(CANCELED_MARK);
      end;
      if WaitForSingleObject(GProc, 0) = WAIT_OBJECT_0 then
      begin
        GetExitCodeProcess(GProc, Code);
        Log('LocalAsr: servidor terminou ao subir (codigo %d). Ver %s', [Code, LogPath]);
        StopServerLocked;
        Exit(OBSLang.T('error.localAsr.serverStart', ['log', LogPath]));
      end;
      if HealthOk then Break;
      if GetTickCount64 - Start > SERVER_START_TIMEOUT_MS then
      begin
        StopServerLocked;
        Exit(OBSLang.T('error.localAsr.serverStart', ['log', LogPath]));
      end;
      Sleep(250);
    end;
    GLastUse := GetTickCount64;
    Result := '';
  finally
    GServerLock.Leave;
  end;
end;

procedure StopIfIdle;
begin
  if (GProc = 0) or (GBusy > 0) then Exit;
  if GetTickCount64 - GLastUse < IDLE_STOP_MS then Exit;
  GServerLock.Enter;
  try
    if (GBusy = 0) and (GProc <> 0) and (GetTickCount64 - GLastUse >= IDLE_STOP_MS) then
    begin
      Log('LocalAsr: ocioso — derrubando o servidor pra liberar a memoria de video.');
      StopServerLocked;
    end;
  finally
    GServerLock.Leave;
  end;
end;

function Uninstall: Boolean;
begin
  Result := False;
  GLock.Enter;
  try
    EnsureStateLoaded;
    if GState.Status = lasInstalling then Exit;
  finally
    GLock.Leave;
  end;
  if GBusy > 0 then Exit;
  GServerLock.Enter;
  try
    StopServerLocked;
  finally
    GServerLock.Leave;
  end;
  try
    if TDirectory.Exists(RootDir) then
      TDirectory.Delete(ExcludeTrailingPathDelimiter(RootDir), True);
  except
    on E: Exception do Log('LocalAsr: falha ao apagar %s: %s', [RootDir, E.Message]);
  end;
  if GetConfigStr('transcribeEngine', 'server') = 'local' then
    SetConfigStr('transcribeEngine', 'server');
  GLock.Enter;
  try
    GState.Status := lasMissing;
    GState.Stage := '';
    GState.Error := '';
    GState.BytesDone := 0;
    GState.Device := '';
  finally
    GLock.Leave;
  end;
  Log('LocalAsr: motor local removido.');
  NotifyChanged(True);
  Result := True;
end;

// =====================================================================
// Reamostragem (ver o cabecalho, ponto 3)
// =====================================================================

procedure TResampler.Setup(ARate: Integer);
var
  Fc, D, X, S, U, Win: Double;
  j, n, k: Integer;
begin
  FSrcRate := ARate;
  FRatio := ARate / RATE;
  // Corte em 90% da Nyquist de SAIDA quando desce (anti-aliasing); na
  // subida, 90% da Nyquist de entrada.
  if FRatio > 1 then Fc := 0.9 / FRatio else Fc := 0.9;
  FWidth := RESAMPLE_ZEROS / Fc;
  n := Ceil(FWidth * TABLE_RES) + 2;
  SetLength(FTable, n);
  for j := 0 to n - 1 do
  begin
    D := j / TABLE_RES;
    if D >= FWidth then
    begin
      FTable[j] := 0;
      Continue;
    end;
    X := Fc * D;
    if X = 0 then S := 1 else S := Sin(Pi * X) / (Pi * X);
    U := D / FWidth;
    Win := 0.42 + 0.5 * Cos(Pi * U) + 0.08 * Cos(2 * Pi * U);
    FTable[j] := Fc * S * Win;
  end;

  FIntRatio := 0;
  if (ARate mod RATE = 0) then
  begin
    FIntRatio := ARate div RATE;
    FIntHalf := Floor(FWidth);
    SetLength(FIntTaps, 2 * FIntHalf + 1);
    for k := -FIntHalf to FIntHalf do
      if Abs(k) < FWidth then FIntTaps[k + FIntHalf] := Kernel(Abs(k))
      else FIntTaps[k + FIntHalf] := 0;
  end;
  SetLength(FBuf, 1 shl 16);
  FBufLen := 0;
  FBufStart := 0;
  FTotalIn := 0;
  FNextOut := 0;
  SetLength(Output, RATE * 60 * 5);
  OutLen := 0;
  FReady := True;
end;

function TResampler.Kernel(D: Double): Double;
var
  P, F: Double;
  j: Integer;
begin
  P := D * TABLE_RES;
  j := Trunc(P);
  if j + 1 >= Length(FTable) then Exit(0);
  F := P - j;
  Result := FTable[j] + (FTable[j + 1] - FTable[j]) * F;
end;

procedure TResampler.Emit(V: Double);
var
  I: Integer;
begin
  I := Round(V * 32767);
  if I > 32767 then I := 32767 else if I < -32768 then I := -32768;
  if OutLen >= Length(Output) then
    SetLength(Output, Length(Output) + Length(Output) div 2);
  Output[OutLen] := SmallInt(I);
  Inc(OutLen);
end;

procedure TResampler.Produce(AFinal: Boolean);
var
  T, Acc, D: Double;
  Lo, Hi, i, Last: Int64;
  k, c: Integer;
  TotalOut, Keep, Drop: Int64;
begin
  TotalOut := Trunc(FTotalIn / FRatio);
  while True do
  begin
    if AFinal and (FNextOut >= TotalOut) then Break;
    T := FNextOut * FRatio;
    Hi := Floor(T + FWidth);
    // Sem amostras suficientes a frente: espera o proximo bloco.
    if (not AFinal) and (Hi >= FBufStart + FBufLen) then Break;
    Acc := 0;
    if FIntRatio > 0 then
    begin
      // Razao inteira: T e inteiro, pesos fixos.
      i := FNextOut * FIntRatio;
      for c := -FIntHalf to FIntHalf do
      begin
        Last := i + c;
        if (Last < 0) or (Last >= FTotalIn) then Continue;
        k := Integer(Last - FBufStart);
        if (k < 0) or (k >= FBufLen) then Continue;
        Acc := Acc + FBuf[k] * FIntTaps[c + FIntHalf];
      end;
    end
    else
    begin
      Lo := Ceil(T - FWidth);
      i := Lo;
      while i <= Hi do
      begin
        // Fora do sinal conta como zero.
        if (i >= 0) and (i < FTotalIn) then
        begin
          k := Integer(i - FBufStart);
          D := Abs(i - T);
          if (k >= 0) and (k < FBufLen) and (D < FWidth) then
            Acc := Acc + FBuf[k] * Kernel(D);
        end;
        Inc(i);
      end;
    end;
    Emit(Acc);
    Inc(FNextOut);
  end;

  // Descarta a entrada que nenhuma saida futura vai usar. Em lotes grandes,
  // pra nao mover memoria a cada bloco.
  Keep := Floor(FNextOut * FRatio - FWidth) - 1;
  Drop := Keep - FBufStart;
  if Drop > 65536 then
  begin
    if Drop > FBufLen then Drop := FBufLen;
    Move(FBuf[Integer(Drop)], FBuf[0], (FBufLen - Integer(Drop)) * SizeOf(Single));
    Dec(FBufLen, Integer(Drop));
    Inc(FBufStart, Drop);
  end;
end;

function TResampler.Push(AData: System.PSingle; ACount, ARate: Integer): Boolean;
begin
  Result := True;
  if not FReady then Setup(ARate)
  else if ARate <> FSrcRate then
  begin
    // Taxa mudando no meio do arquivo nao acontece no que o OBS grava.
    Log('LocalAsr: taxa mudou no meio do audio (%d -> %d) — abortando.', [FSrcRate, ARate]);
    Exit(False);
  end;
  if FBufLen + ACount > Length(FBuf) then
    SetLength(FBuf, Max(Length(FBuf) * 2, FBufLen + ACount));
  Move(AData^, FBuf[FBufLen], ACount * SizeOf(Single));
  Inc(FBufLen, ACount);
  Inc(FTotalIn, ACount);
  Produce(False);
end;

procedure TResampler.Finish;
begin
  if FReady then Produce(True);
  SetLength(Output, OutLen);
end;

// =====================================================================
// Pipeline
// =====================================================================

procedure SplitLong(const Rms: TArray<Double>; AStart, AEnd: Integer;
  AList: TList<TBlock>);
// Parte [AStart, AEnd) em blocos de ate MAX_CHUNK_FRAMES, cortando no vale
// da energia suavizada nos ultimos SEARCH_FRAMES de cada um. Janela [i-5,
// i+4]: o mode='same' do np.convolve com 10 pontos. (TBlock.I = inicio,
// TBlock.J = fim; K nao e usado aqui.)
var
  Start, Stop, Lo, i, j, Best: Integer;
  V, BestV: Double;
  B: TBlock;
begin
  Start := AStart;
  while Start < AEnd do
  begin
    if AEnd - Start <= MAX_CHUNK_FRAMES then
      Stop := AEnd
    else
    begin
      Lo := Start + MAX_CHUNK_FRAMES - SEARCH_FRAMES;
      Best := 0;
      BestV := MaxDouble;
      for i := 0 to SEARCH_FRAMES - 1 do
      begin
        V := 0;
        for j := i - 5 to i + 4 do
          if (j >= 0) and (j < SEARCH_FRAMES) then V := V + Rms[Lo + j];
        V := V / SMOOTH_FRAMES;
        if V < BestV then
        begin
          BestV := V;
          Best := i;
        end;
      end;
      Stop := Lo + Best;
    end;
    B.I := Start;
    B.J := Stop;
    B.K := 0;
    AList.Add(B);
    Start := Stop;
  end;
end;

function EnergyChunks(const S: TArray<SmallInt>; N: Int64): TArray<TChunk>;
// Mesma conta do energy_chunks do Python (ver cabecalho, ponto 2):
//   1. quadro "alto" = energia acima do dobro do percentil 20 (o chao de
//      ruido);
//   2. sequencia alta mais curta que MIN_RUN_FRAMES e estalo, nao conta;
//   3. regioes separadas por menos de SPLIT_GAP_FRAMES ficam no mesmo bloco;
//   4. regiao com menos de MIN_SPEECH_FRAMES de fala e descartada;
//   5. regiao longa e partida no vale de energia perto de ~60 s.
var
  NF, f, i, j, Loud, a, b: Integer;
  Rms, Sorted: TArray<Double>;
  IsLoud: TArray<Boolean>;
  Sum, PPos, Frac, Silence: Double;
  Regions, Parts: TList<TBlock>;
  R: TBlock;
  List: TList<TChunk>;
  C: TChunk;
begin
  SetLength(Result, 0);
  NF := Integer(N div FRAME);
  if NF = 0 then Exit;
  SetLength(Rms, NF);
  for f := 0 to NF - 1 do
  begin
    Sum := 0;
    for i := f * FRAME to f * FRAME + FRAME - 1 do
      Sum := Sum + Double(S[i]) * S[i];
    Rms[f] := Sqrt(Sum / FRAME);
  end;
  // Percentil com interpolacao linear, igual ao numpy.percentile.
  Sorted := Copy(Rms);
  TArray.Sort<Double>(Sorted);
  PPos := SILENCE_PCT / 100 * (NF - 1);
  i := Floor(PPos);
  Frac := PPos - i;
  Silence := Sorted[i];
  if i + 1 < NF then Silence := Silence + (Sorted[i + 1] - Sorted[i]) * Frac;
  SetLength(IsLoud, NF);
  for f := 0 to NF - 1 do IsLoud[f] := Rms[f] > Silence * 2;

  Regions := TList<TBlock>.Create;
  Parts := TList<TBlock>.Create;
  List := TList<TChunk>.Create;
  try
    // Sequencias altas longas o bastante, juntadas em regioes. Em cada
    // regiao: I = inicio, J = fim, K = quadros de fala dentro dela.
    i := 0;
    while i < NF do
    begin
      if not IsLoud[i] then
      begin
        Inc(i);
        Continue;
      end;
      j := i;
      while (j < NF) and IsLoud[j] do Inc(j);
      if j - i >= MIN_RUN_FRAMES then
      begin
        if (Regions.Count > 0) and (i - Regions.Last.J < SPLIT_GAP_FRAMES) then
        begin
          R := Regions.Last;
          R.J := j;
          Inc(R.K, j - i);
          Regions[Regions.Count - 1] := R;
        end
        else
        begin
          R.I := i;
          R.J := j;
          R.K := j - i;
          Regions.Add(R);
        end;
      end;
      i := j;
    end;

    for R in Regions do
    begin
      if R.K < MIN_SPEECH_FRAMES then Continue;
      Parts.Clear;
      SplitLong(Rms, Max(0, R.I - PAD_FRAMES), Min(NF, R.J + PAD_FRAMES), Parts);
      for f := 0 to Parts.Count - 1 do
      begin
        a := Parts[f].I;
        b := Parts[f].J;
        Loud := 0;
        for j := a to b - 1 do
          if IsLoud[j] then Inc(Loud);
        if Loud / (b - a) >= MIN_VOICED then
        begin
          C := Default(TChunk);
          C.StartF := a;
          C.EndF := b;
          List.Add(C);
        end;
      end;
    end;
    Result := List.ToArray;
  finally
    List.Free;
    Parts.Free;
    Regions.Free;
  end;
end;

function WavBytes(const S: TArray<SmallInt>; AFrom, ACount: Int64): TBytes;
// WAV PCM 16 bits mono 16 kHz em memoria (cabecalho de 44 bytes).
var
  Buf: TBytes;
  DataLen: Cardinal;

  procedure Put32(AAt: Integer; AV: Cardinal);
  begin
    PCardinal(@Buf[AAt])^ := AV;
  end;

  procedure Put16(AAt: Integer; AV: Word);
  begin
    PWord(@Buf[AAt])^ := AV;
  end;

  procedure PutTag(AAt: Integer; const ATag: AnsiString);
  var
    k: Integer;
  begin
    for k := 1 to Length(ATag) do Buf[AAt + k - 1] := Byte(ATag[k]);
  end;

begin
  DataLen := Cardinal(ACount * 2);
  SetLength(Buf, 44 + DataLen);
  PutTag(0, 'RIFF');
  Put32(4, 36 + DataLen);
  PutTag(8, 'WAVEfmt ');
  Put32(16, 16);            // tamanho do bloco fmt
  Put16(20, 1);             // PCM
  Put16(22, 1);             // mono
  Put32(24, RATE);
  Put32(28, RATE * 2);      // bytes por segundo
  Put16(32, 2);             // bytes por quadro
  Put16(34, 16);            // bits por amostra
  PutTag(36, 'data');
  Put32(40, DataLen);
  if ACount > 0 then Move(S[AFrom], Buf[44], DataLen);
  Result := Buf;
end;

function HttpPostLocal(const ARoute, AContentType: string; const ABody: TBytes;
  out AStatus: Integer; out AResponse: string): string;
// POST no servidor local pelo WinHTTP DIRETO. O THTTPClient nao serve aqui:
// ele ajusta os tempos de conexao, envio e recebimento, mas nao o de ESPERA
// PELA RESPOSTA depois do envio (WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT),
// que o Windows fixa em 90 s. Um bloco que o modelo leve mais que isso
// derrubava a gravacao inteira com "Error sending data: (12002)".
// '' = foi e voltou (AStatus = codigo HTTP); senao o motivo.
var
  Session, Conn, Req: HINTERNET;
  Timeout, Code, CodeLen, Avail, Got: DWORD;
  Headers: string;
  Buf: TBytes;
  Resp: TBytesStream;

  function Fail(const AWhat: string): string;
  var
    Err: DWORD;
  begin
    Err := GetLastError;
    if Err = 12002 then
      Result := Format('%s: sem resposta em %d s (WinHTTP 12002)',
        [AWhat, REQUEST_TIMEOUT_MS div 1000])
    else
      Result := Format('%s: WinHTTP %d', [AWhat, Err]);
  end;

begin
  Result := '';
  AStatus := 0;
  AResponse := '';
  Conn := nil;
  Req := nil;
  Session := WinHttpOpen('NoOBS', WINHTTP_ACCESS_TYPE_NO_PROXY, nil, nil, 0);
  if Session = nil then Exit(Fail('WinHttpOpen'));
  Resp := TBytesStream.Create;
  try
    // Resolucao/conexao curtas (e 127.0.0.1); envio de ~2 MB; recebimento
    // e espera pela resposta com o teto de REQUEST_TIMEOUT_MS.
    WinHttpSetTimeouts(Session, 5000, 5000, 60000, REQUEST_TIMEOUT_MS);
    Conn := WinHttpConnect(Session, '127.0.0.1', INTERNET_PORT(GPort), 0);
    if Conn = nil then Exit(Fail('WinHttpConnect'));
    Req := WinHttpOpenRequest(Conn, 'POST', PChar(ARoute), nil, nil, nil, 0);
    if Req = nil then Exit(Fail('WinHttpOpenRequest'));
    Timeout := REQUEST_TIMEOUT_MS;
    WinHttpSetOption(Req, WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT, @Timeout, SizeOf(Timeout));
    Headers := 'Content-Type: ' + AContentType;
    if not WinHttpSendRequest(Req, PChar(Headers), DWORD(Length(Headers)),
      Pointer(ABody), DWORD(Length(ABody)), DWORD(Length(ABody)), 0) then
      Exit(Fail('WinHttpSendRequest'));
    if not WinHttpReceiveResponse(Req, nil) then Exit(Fail('WinHttpReceiveResponse'));
    Code := 0;
    CodeLen := SizeOf(Code);
    if WinHttpQueryHeaders(Req, WINHTTP_QUERY_STATUS_CODE or WINHTTP_QUERY_FLAG_NUMBER,
      nil, @Code, CodeLen, nil) then
      AStatus := Integer(Code);
    while True do
    begin
      Avail := 0;
      if not WinHttpQueryDataAvailable(Req, @Avail) then Exit(Fail('WinHttpQueryDataAvailable'));
      if Avail = 0 then Break;
      SetLength(Buf, Avail);
      Got := 0;
      if not WinHttpReadData(Req, Buf[0], Avail, @Got) then Exit(Fail('WinHttpReadData'));
      if Got = 0 then Break;
      Resp.WriteBuffer(Buf[0], Got);
    end;
    AResponse := TEncoding.UTF8.GetString(Resp.Bytes, 0, Integer(Resp.Size));
  finally
    Resp.Free;
    if Req <> nil then WinHttpCloseHandle(Req);
    if Conn <> nil then WinHttpCloseHandle(Conn);
    WinHttpCloseHandle(Session);
  end;
end;

function PostAudio(const ARoute: string; const AFields: array of string;
  const AWav: TBytes; out AObj: TJSONObject): string;
// POST multipart com o bloco em WAV. AFields = pares nome, valor (valor
// vazio nao vai). '' = ok com o JSON em AObj (do caller liberar). O corpo e
// montado aqui (texto em UTF-8) e sai pelo HttpPostLocal.
var
  Boundary, Body, Err: string;
  Payload: TBytesStream;
  Json: TJSONValue;
  Status, i: Integer;

  procedure PutText(const AText: string);
  var
    B: TBytes;
  begin
    B := TEncoding.UTF8.GetBytes(AText);
    if Length(B) > 0 then Payload.WriteBuffer(B[0], Length(B));
  end;

begin
  AObj := nil;
  Boundary := '----NoOBS' + IntToHex(Random(MaxInt), 8) + IntToHex(Random(MaxInt), 8);
  Payload := TBytesStream.Create;
  try
    i := 0;
    while i + 1 <= High(AFields) do
    begin
      if AFields[i + 1] <> '' then
        PutText('--' + Boundary + #13#10 +
          'Content-Disposition: form-data; name="' + AFields[i] + '"'#13#10#13#10 +
          AFields[i + 1] + #13#10);
      Inc(i, 2);
    end;
    PutText('--' + Boundary + #13#10 +
      'Content-Disposition: form-data; name="file"; filename="bloco.wav"'#13#10 +
      'Content-Type: audio/wav'#13#10#13#10);
    if Length(AWav) > 0 then Payload.WriteBuffer(AWav[0], Length(AWav));
    PutText(#13#10'--' + Boundary + '--'#13#10);

    Err := HttpPostLocal(ARoute, 'multipart/form-data; boundary=' + Boundary,
      Copy(Payload.Bytes, 0, Integer(Payload.Size)), Status, Body);
  finally
    Payload.Free;
  end;
  if Err <> '' then
    Exit(OBSLang.T('error.localAsr.request', ['error', Err]));
  if (Status <> 200) and (Pos(NO_SPEECH_TEXT, Body) > 0) then
    Exit(NO_SPEECH_MARK);
  if Status <> 200 then
    Exit(OBSLang.T('error.localAsr.request',
      ['error', Format('HTTP %d %s', [Status, Copy(Body, 1, 300)])]));
  Json := TJSONObject.ParseJSONValue(Body);
  if not (Json is TJSONObject) then
  begin
    Json.Free;
    Exit(OBSLang.T('error.localAsr.request', ['error', Copy(Body, 1, 300)]));
  end;
  AObj := TJSONObject(Json);
  GLastUse := GetTickCount64;
  Result := '';
end;

function LangCode(const ALang: string): string;
// 'Portuguese' -> 'pt'; 'pt' -> 'pt'; idioma fora da tabela: minusculas.
var
  k: Integer;
  L: string;
begin
  L := LowerCase(Trim(ALang));
  for k := 0 to High(LANG_NAMES) do
    if SameText(L, LANG_NAMES[k]) then Exit(LANG_CODES[k]);
  Result := L;
end;

function AlignName(const ACode: string): string;
// Nome que o alinhador espera, ou '' se ele nao cobre o idioma.
var
  k: Integer;
begin
  for k := 0 to High(LANG_CODES) do
    if SameText(ACode, LANG_CODES[k]) then Exit(LANG_NAMES[k]);
  for k := 0 to High(LANG_CODES) do
    if SameText(Copy(ACode, 1, 2), LANG_CODES[k]) then Exit(LANG_NAMES[k]);
  Result := '';
end;

function SplitWords(const AText: string): TArray<string>;
// Como o str.split() do Python: separa em espaco e descarta vazios.
var
  Parts: TArray<string>;
  L: TList<string>;
  P: string;
begin
  Parts := AText.Split([' ', #9, #10, #13]);
  L := TList<string>.Create;
  try
    for P in Parts do
      if P <> '' then L.Add(P);
    Result := L.ToArray;
  finally
    L.Free;
  end;
end;

function WordKey(const AWord: string): string;
// Forma de comparacao: minusculas, so letras e digitos (acento incluso).
var
  Ch: Char;
  L: string;
begin
  Result := '';
  L := AnsiLowerCase(AWord);
  for Ch in L do
    if Ch.IsLetterOrDigit then Result := Result + Ch;
end;

function Opcodes(const A, B: TArray<string>): TArray<TOpcode>;
// Porte do difflib.SequenceMatcher(autojunk=False).get_opcodes(): maior
// trecho comum, recursivo dos dois lados, e as diferencas entre eles.
// Conferido em Python contra o difflib: 3000 casos aleatorios identicos.
var
  Blocks: TList<TBlock>;
  Merged: TList<TBlock>;
  Ops: TList<TOpcode>;
  Prev, Cur: TArray<Integer>;
  alo, ahi, blo, bhi, i, j, k, besti, bestj, best, ii, jj: Integer;
  Pending: TList<TArray<Integer>>;
  R: TArray<Integer>;
  Bl, M: TBlock;
  Op: TOpcode;
begin
  Blocks := TList<TBlock>.Create;
  Pending := TList<TArray<Integer>>.Create;
  Merged := TList<TBlock>.Create;
  Ops := TList<TOpcode>.Create;
  try
    Pending.Add([0, Length(A), 0, Length(B)]);
    SetLength(Prev, Length(B) + 1);
    SetLength(Cur, Length(B) + 1);
    while Pending.Count > 0 do
    begin
      // LIFO, como a lista do difflib (pop do fim).
      R := Pending[Pending.Count - 1];
      Pending.Delete(Pending.Count - 1);
      alo := R[0]; ahi := R[1]; blo := R[2]; bhi := R[3];
      // find_longest_match: comprimento da sequencia comum terminando em
      // (i, j); em empate fica a de menor i, depois menor j.
      besti := alo; bestj := blo; best := 0;
      for jj := 0 to Length(B) do Prev[jj] := 0;
      for i := alo to ahi - 1 do
      begin
        for jj := 0 to Length(B) do Cur[jj] := 0;
        for j := blo to bhi - 1 do
          if A[i] = B[j] then
          begin
            if j > blo then k := Prev[j - 1] + 1 else k := 1;
            Cur[j] := k;
            if k > best then
            begin
              besti := i - k + 1;
              bestj := j - k + 1;
              best := k;
            end;
          end;
        for jj := 0 to Length(B) do Prev[jj] := Cur[jj];
      end;
      if best > 0 then
      begin
        Bl.I := besti; Bl.J := bestj; Bl.K := best;
        Blocks.Add(Bl);
        if (alo < besti) and (blo < bestj) then
          Pending.Add([alo, besti, blo, bestj]);
        if (besti + best < ahi) and (bestj + best < bhi) then
          Pending.Add([besti + best, ahi, bestj + best, bhi]);
      end;
    end;

    Blocks.Sort(TComparer<TBlock>.Construct(
      function(const X, Y: TBlock): Integer
      begin
        if X.I <> Y.I then Exit(X.I - Y.I);
        if X.J <> Y.J then Exit(X.J - Y.J);
        Result := X.K - Y.K;
      end));

    // Junta blocos encostados.
    M.I := 0; M.J := 0; M.K := 0;
    for Bl in Blocks do
      if (M.I + M.K = Bl.I) and (M.J + M.K = Bl.J) then
        Inc(M.K, Bl.K)
      else
      begin
        if M.K > 0 then Merged.Add(M);
        M := Bl;
      end;
    if M.K > 0 then Merged.Add(M);
    M.I := Length(A); M.J := Length(B); M.K := 0;
    Merged.Add(M);

    ii := 0; jj := 0;
    for Bl in Merged do
    begin
      Op.Tag := #0;
      if (ii < Bl.I) and (jj < Bl.J) then Op.Tag := 'r'
      else if ii < Bl.I then Op.Tag := 'd'
      else if jj < Bl.J then Op.Tag := 'i';
      if Op.Tag <> #0 then
      begin
        Op.A0 := ii; Op.A1 := Bl.I; Op.B0 := jj; Op.B1 := Bl.J;
        Ops.Add(Op);
      end;
      ii := Bl.I + Bl.K;
      jj := Bl.J + Bl.K;
      if Bl.K > 0 then
      begin
        Op.Tag := 'e';
        Op.A0 := Bl.I; Op.A1 := ii; Op.B0 := Bl.J; Op.B1 := jj;
        Ops.Add(Op);
      end;
    end;
    Result := Ops.ToArray;
  finally
    Ops.Free;
    Merged.Free;
    Pending.Free;
    Blocks.Free;
  end;
end;

function JsonNum(AObj: TJSONObject; const AKey: string; out AValue: Double): Boolean;
var
  V: TJSONValue;
begin
  AValue := 0;
  V := AObj.GetValue(AKey);
  Result := V is TJSONNumber;
  if Result then AValue := TJSONNumber(V).AsDouble;
end;

function WordObj(const AWord: string; AHasTime: Boolean; AStart, AEnd: Double): TJSONObject;
begin
  Result := TJSONObject.Create;
  Result.AddPair('word', AWord);
  if AHasTime then
  begin
    Result.AddPair('start', TJSONNumber.Create(RoundTo(AStart, -3)));
    Result.AddPair('end', TJSONNumber.Create(RoundTo(AEnd, -3)));
  end;
end;

function AttachTimes(const AText: string; AAligned: TJSONArray; AOffset: Double): TJSONArray;
// Palavras do texto ORIGINAL (com pontuacao) com o tempo do alinhador,
// que as devolve sem pontuacao e as vezes partidas ("A.C." -> "A", "C").
// Os dois lados sao casados pela forma sem pontuacao (WordKey).
var
  Tokens, AK, BK: TArray<string>;
  Has: TArray<Boolean>;
  TS, TE: TArray<Double>;
  Ops: TArray<TOpcode>;
  Op: TOpcode;
  i, k: Integer;
  S, E: Double;
  Joined: string;

  function AlignedAt(AIdx: Integer): TJSONObject;
  begin
    if (AAligned <> nil) and (AIdx < AAligned.Count) and
       (AAligned.Items[AIdx] is TJSONObject) then
      Result := TJSONObject(AAligned.Items[AIdx])
    else
      Result := nil;
  end;

  procedure TakeTime(AIdx, AFromB, AToB: Integer);
  var
    W1, W2: TJSONObject;
    S1, E1: Double;
  begin
    W1 := AlignedAt(AFromB);
    W2 := AlignedAt(AToB);
    if (W1 = nil) or (W2 = nil) then Exit;
    if not JsonNum(W1, 'start', S1) then Exit;
    if not JsonNum(W2, 'end', E1) then E1 := S1;
    Has[AIdx] := True;
    TS[AIdx] := S1;
    TE[AIdx] := E1;
  end;

begin
  Tokens := SplitWords(AText);
  SetLength(AK, Length(Tokens));
  for i := 0 to High(Tokens) do AK[i] := WordKey(Tokens[i]);
  if AAligned <> nil then SetLength(BK, AAligned.Count) else SetLength(BK, 0);
  for i := 0 to High(BK) do
    if AlignedAt(i) <> nil then BK[i] := WordKey(AlignedAt(i).GetValue<string>('word', ''))
    else BK[i] := '';
  SetLength(Has, Length(Tokens));
  SetLength(TS, Length(Tokens));
  SetLength(TE, Length(Tokens));

  Ops := Opcodes(AK, BK);
  for Op in Ops do
    if (Op.Tag = 'e') or ((Op.Tag = 'r') and (Op.A1 - Op.A0 = Op.B1 - Op.B0)) then
    begin
      for k := 0 to Op.A1 - Op.A0 - 1 do
        TakeTime(Op.A0 + k, Op.B0 + k, Op.B0 + k);
    end
    else if (Op.Tag = 'r') and (Op.A1 - Op.A0 = 1) then
    begin
      Joined := '';
      for k := Op.B0 to Op.B1 - 1 do Joined := Joined + BK[k];
      if Joined = AK[Op.A0] then TakeTime(Op.A0, Op.B0, Op.B1 - 1);
    end;

  Result := TJSONArray.Create;
  for i := 0 to High(Tokens) do
  begin
    S := RoundTo(TS[i], -3) + AOffset;
    E := RoundTo(TE[i], -3) + AOffset;
    Result.AddElement(WordObj(Tokens[i], Has[i], S, E));
  end;
end;

function SpreadTimes(const AText: string; AStart, AEnd: Double): TJSONArray;
// Idioma sem alinhador: reparte o bloco pelas palavras, pelo tamanho.
var
  Tokens: TArray<string>;
  Total, i: Integer;
  Cursor, Span: Double;
begin
  Tokens := SplitWords(AText);
  Total := 0;
  for i := 0 to High(Tokens) do Inc(Total, Length(Tokens[i]) + 1);
  if Total = 0 then Total := 1;
  Result := TJSONArray.Create;
  Cursor := AStart;
  for i := 0 to High(Tokens) do
  begin
    Span := (AEnd - AStart) * (Length(Tokens[i]) + 1) / Total;
    Result.AddElement(WordObj(Tokens[i], True, Cursor, Cursor + Span));
    Cursor := Cursor + Span;
  end;
end;

function NormalizeSpaces(const AText: string): string;
begin
  Result := string.Join(' ', SplitWords(AText));
end;

function StripTrail(const AWord: string): string;
var
  b: Integer;
begin
  b := Length(AWord);
  while (b > 0) and (Pos(AWord[b], LOOP_PUNCT) > 0) do Dec(b);
  Result := Copy(AWord, 1, b);
end;

function CollapseLoops(const AText: string): string;
// Corta repeticoes em laco ("vai vai vai vai vai" -> "vai vai"). Compara
// sem pontuacao e sem caixa (WordKey); a pontuacao final da ultima
// repeticao fica ("sim, sim, sim, sim." -> "sim, sim."). Mesma regra do
// collapse_loops do Python.
var
  W, Keys: TArray<string>;
  Parts: TList<string>;
  i, j, n, k, Reps, BestN, BestReps: Integer;
  Same, AnyKey: Boolean;
  Last, Kept: string;
begin
  W := SplitWords(AText);
  SetLength(Keys, Length(W));
  for i := 0 to High(W) do Keys[i] := WordKey(W[i]);
  Parts := TList<string>.Create;
  try
    i := 0;
    while i < Length(W) do
    begin
      BestN := 0;
      BestReps := 0;
      for n := 1 to LOOP_MAX_UNIT do
      begin
        if i + n > Length(W) then Break;
        AnyKey := False;
        for k := i to i + n - 1 do
          if Keys[k] <> '' then AnyKey := True;
        if not AnyKey then Continue;
        Reps := 1;
        j := i + n;
        while j + n <= Length(W) do
        begin
          Same := True;
          for k := 0 to n - 1 do
            if Keys[j + k] <> Keys[i + k] then
            begin
              Same := False;
              Break;
            end;
          if not Same then Break;
          Inc(Reps);
          Inc(j, n);
        end;
        if (Reps >= LOOP_MIN_REPS) and (Reps * n > BestReps * BestN) then
        begin
          BestN := n;
          BestReps := Reps;
        end;
      end;
      if BestN = 0 then
      begin
        Parts.Add(W[i]);
        Inc(i);
        Continue;
      end;
      for k := i to i + BestN * LOOP_KEEP - 1 do Parts.Add(W[k]);
      Last := W[i + BestN * BestReps - 1];
      Kept := Parts[Parts.Count - 1];
      Parts[Parts.Count - 1] := StripTrail(Kept) +
        Copy(Last, Length(StripTrail(Last)) + 1, MaxInt);
      Inc(i, BestN * BestReps);
    end;
    Result := string.Join(' ', Parts.ToArray);
  finally
    Parts.Free;
  end;
end;

function DecodeSink(ARs: TResampler; ACancel: TLocalAsrCancel): TAudioBlockFunc;
// O callback da decodificacao. System.PSingle, e nao PSingle: a
// Winapi.Windows declara um PSingle PROPRIO, e com ela no uses o tipo do
// parametro deixa de ser o da TAudioBlockFunc (E2010).
begin
  Result :=
    function(AData: System.PSingle; ACount, ARate: Integer): Boolean
    begin
      Result := (not (GShuttingDown or (Assigned(ACancel) and ACancel()))) and
        ARs.Push(AData, ACount, ARate);
    end;
end;

function Transcribe(const AAudioPath, ALanguage: string;
  AOnProgress: TLocalAsrProgress; ACancel: TLocalAsrCancel;
  out ABody: string; out ACanceled: Boolean): string;
var
  Rs: TResampler;
  Chunks: TArray<TChunk>;
  Obj: TJSONObject;
  Root, Seg: TJSONObject;
  Segs: TJSONArray;
  i, n, Id: Integer;
  Err, Name, Text, Lang, AllText, ReqLang: string;
  StartS, EndS, Duration: Double;
  Langs: TDictionary<string, Integer>;
  BestLang: string;
  BestCount: Integer;
  AnyAligned: Boolean;
  Pair: TPair<string, Integer>;

  function Stop: Boolean;
  begin
    Result := GShuttingDown or (Assigned(ACancel) and ACancel());
  end;

  procedure Report(const AStage: string; AFrac: Double);
  begin
    if Assigned(AOnProgress) then AOnProgress(AStage, EnsureRange(AFrac, 0, 1));
  end;

  function ChunkWav(const C: TChunk): TBytes;
  begin
    Result := WavBytes(Rs.Output, Int64(C.StartF) * FRAME,
      Int64(C.EndF - C.StartF) * FRAME);
  end;

begin
  ABody := '';
  ACanceled := False;
  if not IsInstalled then Exit(OBSLang.T('error.localAsr.notInstalled'));
  // "Portuguese" vira "pt": o alinhador e a conversao de numeros procuram o
  // codigo, e e o codigo que vai no JSON.
  ReqLang := LangCode(ALanguage);

  TInterlocked.Increment(GBusy);
  Rs := TResampler.Create;
  Langs := TDictionary<string, Integer>.Create;
  SetLength(Chunks, 0);
  try
    // 1. Audio -> 16 kHz mono, sem guardar a taxa original inteira.
    Report('decoding', 0);
    if not FFmpegOps.DecodeAudioMono(AAudioPath, DecodeSink(Rs, ACancel)) then
    begin
      if Stop then
      begin
        ACanceled := True;
        Exit('');
      end;
      Exit(OBSLang.T('error.localAsr.decode'));
    end;
    Rs.Finish;
    Duration := Rs.OutLen / RATE;
    Chunks := EnergyChunks(Rs.Output, Rs.OutLen);
    Log('LocalAsr: %.1f s de audio, %d blocos com fala.', [Duration, Length(Chunks)]);
    n := Length(Chunks);

    if n > 0 then
    begin
      Report('starting', 0);
      Err := EnsureServer(ACancel);
      if Err = CANCELED_MARK then
      begin
        ACanceled := True;
        Exit('');
      end;
      if Err <> '' then Exit(Err);
    end;

    // 2. Primeira passada: texto de todos os blocos.
    for i := 0 to n - 1 do
    begin
      if Stop then begin ACanceled := True; Exit(''); end;
      Report('transcribing', TRANSCRIBE_SHARE * i / n);
      Err := PostAudio('/v1/audio/transcriptions/details',
        ['model', ASR_MODEL_ID, 'language', ReqLang], ChunkWav(Chunks[i]), Obj);
      if Err = NO_SPEECH_MARK then
      begin
        // Bloco sem fala reconhecivel: fica vazio e o arquivo segue.
        Chunks[i].Text := '';
        Chunks[i].Lang := ReqLang;
        Continue;
      end;
      if Err <> '' then Exit(Err);
      try
        Lang := LangCode(Obj.GetValue<string>('language', ''));
        if Lang = '' then Lang := ReqLang;
        Text := CollapseLoops(NormalizeSpaces(Obj.GetValue<string>('text', '')));
      finally
        Obj.Free;
      end;
      if Lang.StartsWith('pt') then Text := OBSNumbersPt.ToDigits(Text);
      Chunks[i].Text := Text;
      Chunks[i].Lang := Lang;
    end;

    // 3. Segunda passada: o instante de cada palavra.
    for i := 0 to n - 1 do
    begin
      if Stop then begin ACanceled := True; Exit(''); end;
      Report('aligning', TRANSCRIBE_SHARE + (1 - TRANSCRIBE_SHARE) * i / n);
      StartS := Chunks[i].StartF * FRAME / RATE;
      EndS := Chunks[i].EndF * FRAME / RATE;
      Name := AlignName(Chunks[i].Lang);
      if Chunks[i].Text = '' then
        Chunks[i].Words := TJSONArray.Create
      else if Name <> '' then
      begin
        Err := PostAudio('/v1/audio/alignments',
          ['model', ALIGN_MODEL_ID, 'language', Name, 'text', Chunks[i].Text],
          ChunkWav(Chunks[i]), Obj);
        if Err <> '' then Exit(Err);
        try
          Chunks[i].Words := AttachTimes(Chunks[i].Text,
            Obj.GetValue('words') as TJSONArray, StartS);
          Chunks[i].Aligned := True;
        finally
          Obj.Free;
        end;
      end
      else
        Chunks[i].Words := SpreadTimes(Chunks[i].Text, StartS, EndS);
    end;
    Report('aligning', 1);

    // 4. JSON no formato da Transcritor API (sem falantes).
    Root := TJSONObject.Create;
    try
      Segs := TJSONArray.Create;
      AnyAligned := False;
      AllText := '';
      Id := 0;
      for i := 0 to n - 1 do
      begin
        if Chunks[i].Lang <> '' then
        begin
          if not Langs.ContainsKey(Chunks[i].Lang) then Langs.Add(Chunks[i].Lang, 0);
          Langs[Chunks[i].Lang] := Langs[Chunks[i].Lang] + 1;
        end;
        if Chunks[i].Text = '' then
        begin
          FreeAndNil(Chunks[i].Words);
          Continue;
        end;
        Seg := TJSONObject.Create;
        Seg.AddPair('id', TJSONNumber.Create(Id));
        Seg.AddPair('start', TJSONNumber.Create(RoundTo(Chunks[i].StartF * FRAME / RATE, -3)));
        Seg.AddPair('end', TJSONNumber.Create(RoundTo(Chunks[i].EndF * FRAME / RATE, -3)));
        Seg.AddPair('speaker', TJSONNull.Create);
        Seg.AddPair('text', Chunks[i].Text);
        Seg.AddPair('aligned', TJSONBool.Create(Chunks[i].Aligned));
        Seg.AddPair('words', Chunks[i].Words);
        Chunks[i].Words := nil;   // agora e do Seg
        Segs.AddElement(Seg);
        AnyAligned := AnyAligned or Chunks[i].Aligned;
        if AllText <> '' then AllText := AllText + ' ';
        AllText := AllText + Chunks[i].Text;
        Inc(Id);
      end;
      BestLang := '';
      BestCount := 0;
      for Pair in Langs do
        if Pair.Value > BestCount then
        begin
          BestLang := Pair.Key;
          BestCount := Pair.Value;
        end;
      if BestLang <> '' then Root.AddPair('language', BestLang)
      else Root.AddPair('language', TJSONNull.Create);
      Root.AddPair('duration', TJSONNumber.Create(RoundTo(Duration, -3)));
      Root.AddPair('alignment', TJSONBool.Create(AnyAligned));
      Root.AddPair('diarization', TJSONBool.Create(False));
      Root.AddPair('num_speakers', TJSONNumber.Create(0));
      Root.AddPair('speakers', TJSONArray.Create);
      Root.AddPair('text', Trim(AllText));
      Root.AddPair('turns', TJSONArray.Create);
      Root.AddPair('by_speaker', TJSONObject.Create);
      Root.AddPair('segments', Segs);
      Root.AddPair('engine', 'noobs:qwen3');
      ABody := Root.ToJSON;
    finally
      Root.Free;
    end;
    Result := '';
  finally
    // Palavras de blocos que nao chegaram ao JSON (erro no meio).
    for i := 0 to High(Chunks) do
      if Chunks[i].Words <> nil then FreeAndNil(Chunks[i].Words);
    Langs.Free;
    Rs.Free;
    GLastUse := GetTickCount64;
    TInterlocked.Decrement(GBusy);
  end;
end;

procedure Shutdown;
begin
  GShuttingDown := True;
  GInstallCancel := True;
  GOnChanged := nil;
  if GServerLock <> nil then
  begin
    GServerLock.Enter;
    try
      StopServerLocked;
    finally
      GServerLock.Leave;
    end;
  end;
  if GJob <> 0 then
  begin
    CloseHandle(GJob);
    GJob := 0;
  end;
end;

initialization
  GLock := TCriticalSection.Create;
  GServerLock := TCriticalSection.Create;

finalization
  // Sem liberar os locks: uma thread de instalacao abandonada no fechamento
  // ainda poderia toca-los. O processo esta acabando; o SO limpa.

end.

(*
  OBSEngine - motor de gravacao via libobs (obs.dll) direto.

  Wrapper de alto nivel sobre LibOBS (bindings raw). Controla o ciclo
  de vida do libobs in-process: init, scene, sources, encoder, output,
  recording. Selecao de encoder fica em OBSEncoder; atribuicao de
  tracks de audio + enumeracao de devices fica em OBSAudioTracks.

  Principio: o core (obs_startup) inicializa uma vez e permanece vivo
  entre gravacoes. Scene/encoder/output sao reconstruidos a cada
  sessao. Teardown completo so no exit do app.
*)
unit OBSEngine;

interface

uses
  System.SysUtils,
  NoOBSTypes;

type
  // Callback de "gravacao parou de verdade" — invocado na MAIN thread
  // depois que o output emitiu o sinal "stop" (arquivo completo) e os
  // objetos foram liberados. APath = arquivo final. Bridge registra pra
  // adicionar o card / salvar meta. Equivale ao RecordingStop do OBS.
  TOBSStoppedProc = procedure(const AOutputPath: string);

  TOBSEngine = class
  private
    FInitialized: Boolean;
    FRecording: Boolean;
    // Stop assincrono em andamento: obs_output_stop foi chamado, mas o
    // sinal "stop" (release seguro) ainda nao chegou. Distinto de
    // FRecording — durante o stopping ambos sao True ate FinalizeStop.
    FStopping: Boolean;
    // Momento (GetTickCount64) em que FStopping virou True. Deixa o
    // OBSBridge distinguir "finalizacao normal em curso" de "travou":
    // sem isso o guard do HandleRecordStart recusaria gravar PARA SEMPRE
    // se o sinal "stop" nunca chegasse e o timeout falhasse.
    FStoppingSince: UInt64;
    FShuttingDown: Boolean;
    // signal_handler_t (= Pointer) do output, guardado entre connect e
    // disconnect. Tipado como Pointer aqui pra nao puxar LibOBS pra
    // interface (LibOBS so e usado na implementation).
    FStopSignalHandler: Pointer;
    FOutputPath: string;
    FExeDir: string;
    FObsPluginBinDir: string;
    FObsPluginDataDir: string;
    FOnStopped: TOBSStoppedProc;
    // Layout do canvas atual — preenchido durante BuildAndStartRecording
    // conforme cada monitor/webcam vai sendo posicionado. Bridge le
    // depois do start pra persistir em <hash>.json (player usa pra
    // seletor de monitor / zoom).
    FCurrentLayout: TRecordingLayout;
    // ID do encoder de video que esta gravacao REALMENTE usou (o fallback
    // pode ter escolhido outro que o pedido). Vira meta do arquivo.
    FVideoEncoderId: string;
    procedure ResolvePaths;
    procedure LoadModules;
    procedure ReleaseRecordingObjects;
    procedure ConnectStopSignal;
    procedure DisconnectStopSignal;
    procedure OnStopSignal;            // main thread (via TThread.Queue)
    procedure FinalizeStop(AInvokeCallback: Boolean); // main thread
  public
    constructor Create;
    destructor Destroy; override;
    procedure EnsureInitialized;
    // AAutoProfile=True: usa o perfil de auto-gravacao (subconjunto dos
    // dispositivos habilitados, marcado na aba Comportamento) em vez de
    // gravar todos os habilitados. Setado quando a gravacao nasce do
    // watcher de mic. Pegadinha #47 + perfil de auto-gravacao.
    procedure BuildAndStartRecording(const AOutputPath: string;
      AAutoProfile: Boolean = False);
    // Pede o stop e retorna NA HORA (nao bloqueia a UI). A conclusao
    // chega depois via FOnStopped quando o sinal "stop" dispara. Modelo
    // identico ao do OBS (SimpleOutput::StopRecording).
    procedure RequestStop;
    // Forca a finalizacao agora (usado pelo timeout do Bridge caso o
    // sinal "stop" nunca chegue). Idempotente. Main thread.
    procedure ForceCompleteStop;
    function  StopRecording: string;   // sincrono — usado so no shutdown
    function  IsRecording: Boolean;
    function  IsStopping: Boolean;
    // Ha quanto tempo esta finalizando (ms). 0 se nao esta.
    function  StoppingElapsedMs: UInt64;
    // False = libobs ainda nao subiu. Usado pra detectar cold start (ver
    // COLD_START_AUDIO_SETTLE_MS em OBSBridge).
    function  IsInitialized: Boolean;
    procedure SetSourceMuted(const ASourceName: string; AMuted: Boolean);
    procedure Teardown;
    property Initialized: Boolean read FInitialized;
    property OutputPath: string read FOutputPath;
    property CurrentLayout: TRecordingLayout read FCurrentLayout;
    property VideoEncoderId: string read FVideoEncoderId;
    property OnStopped: TOBSStoppedProc read FOnStopped write FOnStopped;
  end;

// Sinaliza que o win-wasapi falhou em agendar a captura de audio (RTWQ) na
// ULTIMA montagem de gravacao — as fontes existem mas entregam silencio.
// Detectado no ObsLogHandler, unico lugar onde o libobs reporta isso.
// Chamar ResetAudioCaptureFault ANTES de montar; consultar DEPOIS.
function HadAudioCaptureFault: Boolean;
procedure ResetAudioCaptureFault;

// Tipos publicos (TGpuVendor, TEncoderCaps, TObsAudioDev) ficam em
// NoOBSTypes. Selecao de encoder foi pra OBSEncoder. Atribuicao de
// tracks de audio + enumeracao de devices foi pra OBSAudioTracks.
// Esta unit so cuida do ciclo de vida do libobs + montagem da cena
// de gravacao (TOBSEngine).

implementation

uses
  Winapi.Windows,
  System.Classes,
  System.Generics.Collections,
  System.AnsiStrings,
  System.Math,
  LibOBS,
  OBSScene,
  OBSConfig,
  OBSLog,
  OBSEncoder,
  OBSAudioTracks,
  WinPreview,
  WinAudioMeter,
  WinWebcam;

const
  // ENCODER_MAX_DIM agora vem dinamico de OBSEncoder.GetEncoderMaxDimension
  // (variavel por codec — H.264 hw = 4096, HEVC/AV1 = 8192). Pegadinha
  // #7: ANTES era hardcoded 8192 que so funcionava em NVENC. AMD H.264
  // batia em "amf_avc_create_texencode failed" quando o canvas passava
  // de 4096 pixels (multi-monitor lado a lado em telas 4K).
  SCENE_NAME = 'NoOBS';
  MANAGED_PREFIX = 'NoOBS ';
  // Nome do sinal de output emitido quando a gravacao terminou de fato
  // (ver output_signals[] em obs-output.c). ASCII puro.
  SIG_STOP: AnsiString = 'stop';


type
  TSourceEntry = record
    Source: obs_source_t;
    Name: AnsiString;
  end;

var
  // Escrito pelo ObsLogHandler (thread do libobs), lido pela main apos a
  // montagem. Boolean de uma via — nao precisa de lock.
  AudioCaptureFault: Boolean = False;
  // Ligado so durante o Teardown — ver ObsLogHandler.
  VerboseShutdownLog: Boolean = False;
  GScene: obs_scene_t;
  GOutput: obs_output_t;
  GVideoEncoder: obs_encoder_t;
  GAudioEncoders: TArray<obs_encoder_t>;
  GSources: TArray<TSourceEntry>;

function RemoveSourceCb(param: Pointer; source: obs_source_t): ByteBool; cdecl;
// Callback do obs_enum_sources/obs_enum_scenes: remove a source do core.
// Copia do que o frontend do OBS faz em ClearSceneData:
//   auto cb = [](void *, obs_source_t *source) {
//       obs_source_remove(source); return true; };
begin
  if source <> nil then
    try obs_source_remove(source); except end;
  Result := True;   // continua a enumeracao
end;

// Desmonta TUDO que o libobs ainda tem registrado, na ordem do
// ClearSceneData do OBS. Sem isto o obs_shutdown precisa desmontar as
// sources sozinho — e trava, porque fontes WASAPI ainda tem thread de
// captura viva. Liberar as NOSSAS referencias (ReleaseRecordingObjects)
// nao basta: a source continua na lista interna do core.
procedure ClearAllObsData;
const
  // libobs/obs-defs.h: #define MAX_CHANNELS 64. Limpamos TODOS, nao so os
  // que usamos — igual ao loop do ClearSceneData.
  MAX_CHANNELS = 64;
var
  i: Integer;
begin
  for i := 0 to MAX_CHANNELS - 1 do
    try obs_set_output_source(i, nil); except end;
  try obs_enum_scenes(RemoveSourceCb, nil); except end;
  try obs_enum_sources(RemoveSourceCb, nil); except end;
end;

procedure StopSignalThunk(data: Pointer; cd: calldata_t); cdecl;
// Callback C do sinal "stop". Roda numa thread INTERNA do libobs —
// proibido tocar libobs/UI aqui (pegadinha #3). So marshala pra main
// thread, onde OnStopSignal faz o release + notifica o Bridge.
begin
  if data = nil then Exit;
  TThread.Queue(nil,
    procedure
    begin
      TOBSEngine(data).OnStopSignal;
    end);
end;

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

function ToAnsi(const S: string): AnsiString;
// libobs e FFmpeg convencionam que todas as strings sao UTF-8 — ate
// em Windows. Conversao explicita evita depender do DefaultSystemCodePage
// do usuario (1252 quebra acentos; 65001 funcionaria por coincidencia).
begin
  Result := AnsiString(UTF8Encode(S));
end;

function FromAnsi(P: PAnsiChar): string;
// Strings vindas do libobs/FFmpeg sao UTF-8. UTF8ToString decodifica
// corretamente independente da locale.
begin
  if P = nil then Result := ''
  else Result := UTF8ToString(P);
end;

function HadAudioCaptureFault: Boolean;
begin
  Result := AudioCaptureFault;
end;

procedure ResetAudioCaptureFault;
begin
  AudioCaptureFault := False;
end;

procedure ObsLogHandler(log_level: Integer; msg: PAnsiChar;
  args: Pointer; p: Pointer); cdecl;
var
  Prefix, Raw, Arg1Str: string;
  Arg1Ptr: PAnsiChar;
begin
  if msg = nil then Exit;
  // Durante o Teardown deixamos passar INFO: as mensagens do obs_shutdown
  // sao todas informativas, e sem elas nao da pra saber em QUAL subsistema
  // ele para (video, audio, graphics, unload de modulo). Fora do shutdown
  // o filtro segue em WARNING pra nao poluir o log.
  if log_level > (LOG_WARNING + Ord(VerboseShutdownLog) * 100) then Exit;

  Raw := string(AnsiString(msg));

  // Falha de captura de audio do win-wasapi. A RTWQ (Real-Time Work Queue
  // do Windows) agenda a entrega dos buffers; se o lock da fila
  // compartilhada falha, o callback NUNCA e agendado e a fonte entrega
  // SILENCIO — sem erro, sem retorno, sem nada. A gravacao sai muda e o
  // usuario so descobre depois, assistindo.
  // Como o libobs nao expoe isso em nenhum valor de retorno, o log e o
  // unico lugar onde a falha aparece. Marcamos aqui pra o Bridge avisar.
  // Roda em thread do libobs — Boolean simples, sem lock (escrita unica).
  if (Pos('RTWQ setup failed', Raw) > 0) or
     (Pos('Could not requeue sample receive work', Raw) > 0) then
    AudioCaptureFault := True;

  // Filtra warnings benignos do libobs que nao indicam bug no nosso codigo:
  // - "Double destroy": cleanup interno de plugins durante shutdown.
  // - "UI task could not be queued": libobs tenta agendar tarefa pra um
  //   frontend OBS Studio (nao usado aqui). Disparado por callbacks
  //   internos durante hot-plug de audio. Sem impacto funcional.
  // - "duplicate name": destruicao diferida de sources entre gravacoes;
  //   OBS auto-renomeia o novo source sem afetar a gravacao.
  if Pos('Double destroy just occurred', Raw) > 0 then Exit;
  if Pos('UI task could not be queued', Raw) > 0 then Exit;
  if Pos('duplicate name', Raw) > 0 then Exit;

  case log_level of
    LOG_ERROR:   Prefix := 'obs[E]';
    LOG_WARNING: Prefix := 'obs[W]';
  else
    Prefix := 'obs[?]';
  end;

  // X64 cdecl varargs: args e um ponteiro pra primeira variadica na stack.
  // Pra mensagens com '%s' como primeiro argumento, derefer pra pegar a string.
  Arg1Str := '';
  if (args <> nil) and (Pos('%s', Raw) > 0) then
  begin
    try
      Arg1Ptr := PPAnsiChar(args)^;
      if Arg1Ptr <> nil then
        Arg1Str := string(AnsiString(Arg1Ptr));
    except
      Arg1Str := '';
    end;
  end;

  if Arg1Str <> '' then
    Log('%s %s [arg1="%s"]', [Prefix, Raw, Arg1Str])
  else
    Log('%s %s', [Prefix, Raw]);
end;

function MakeSettings: obs_data_t;
begin
  Result := obs_data_create;
end;

procedure SetStr(D: obs_data_t; const K: AnsiString; const V: AnsiString);
begin
  obs_data_set_string(D, PAnsiChar(K), PAnsiChar(V));
end;

procedure SetInt(D: obs_data_t; const K: AnsiString; V: Int64);
begin
  obs_data_set_int(D, PAnsiChar(K), V);
end;

procedure SetBool(D: obs_data_t; const K: AnsiString; V: Boolean);
begin
  obs_data_set_bool(D, PAnsiChar(K), ByteBool(V));
end;

function CreateSource(const AId, AName: AnsiString;
  ASettings: obs_data_t): obs_source_t;
var
  Entry: TSourceEntry;
begin
  Result := obs_source_create(PAnsiChar(AId), PAnsiChar(AName),
    ASettings, nil);
  if Result = nil then
    raise Exception.CreateFmt('obs_source_create falhou para "%s" (%s).',
      [string(AName), string(AId)]);
  Entry.Source := Result;
  Entry.Name := AName;
  SetLength(GSources, Length(GSources) + 1);
  GSources[High(GSources)] := Entry;
  if ASettings <> nil then
    obs_data_release(ASettings);
end;

function FindSourceByName(const AName: AnsiString): obs_source_t;
var
  i: Integer;
begin
  for i := 0 to High(GSources) do
    if GSources[i].Name = AName then
      Exit(GSources[i].Source);
  Result := nil;
end;

// -----------------------------------------------------------------------
// Monitor ID resolution via obs_properties
// -----------------------------------------------------------------------

function ResolveMonitorId(const AMonitor: TOBSMonitor): AnsiString;
var
  Props: obs_properties_t;
  Prop: obs_property_t;
  Count: NativeUInt;
  i: NativeUInt;
  ItemName, ItemValue: AnsiString;
  PosTag: AnsiString;
begin
  Result := '';
  PosTag := AnsiString(Format('@ %d,%d', [AMonitor.PositionX, AMonitor.PositionY]));

  Props := obs_get_source_properties(PAnsiChar(AnsiString('monitor_capture')));
  if Props = nil then Exit;
  try
    Prop := obs_properties_get(Props, 'monitor_id');
    if Prop = nil then Exit;
    Count := obs_property_list_item_count(Prop);
    // Count e NativeUInt — se 0, Count-1 underflowa pra $FFFFFFFF e
    // dispara EIntOverflow (compiler com {$Q+}).
    if Count = 0 then Exit;
    for i := 0 to Count - 1 do
    begin
      ItemName := AnsiString(obs_property_list_item_name(Prop, i));
      ItemValue := AnsiString(obs_property_list_item_string(Prop, i));
      if (Pos(PosTag, ItemName) > 0) and (ItemValue <> 'DUMMY') then
      begin
        Result := ItemValue;
        Exit;
      end;
    end;
  finally
    obs_properties_destroy(Props);
  end;
end;



// -----------------------------------------------------------------------
// TOBSEngine
// -----------------------------------------------------------------------

constructor TOBSEngine.Create;
begin
  inherited Create;
  FInitialized := False;
  FRecording := False;
  GScene := nil;
  GOutput := nil;
  GVideoEncoder := nil;
  SetLength(GAudioEncoders, 0);
  SetLength(GSources, 0);
end;

destructor TOBSEngine.Destroy;
begin
  Teardown;
  inherited;
end;

procedure TOBSEngine.ResolvePaths;
begin
  // Layout esperado: NoOBS.exe roda em obs\bin\64bit\ (ao lado de obs.dll
  // e dos helpers como obs-ffmpeg-mux.exe). Plugins e data ficam em
  // ..\..\obs-plugins\64bit\ e ..\..\data\.
  FExeDir := ExtractFilePath(ParamStr(0));
  FObsPluginBinDir := ExpandFileName(FExeDir + '..\..\obs-plugins\64bit');
  FObsPluginDataDir := ExpandFileName(FExeDir + '..\..\data\obs-plugins');
end;

procedure TOBSEngine.LoadModules;
// Carrega APENAS os plugins que precisamos. Pular obs-websocket.dll
// (crash: tenta chamar obs_frontend_* sem UI). obs_load_all_modules
// nao filtra, entao usamos obs_open_module + obs_init_module por plugin.
const
  WANTED: array[0..5] of string = (
    'obs-ffmpeg',       // ffmpeg_muxer (output) + ffmpeg_aac (audio enc)
    'obs-x264',         // encoder CPU fallback
    'obs-nvenc',        // encoder HEVC/H264 NVIDIA (opcional)
    'win-capture',      // monitor_capture (gravar tela)
    'win-dshow',        // dshow_input (webcam)
    'win-wasapi'        // wasapi_input/output_capture (audio)
  );
var
  i: Integer;
  Module: Pointer;
  BinPath, DataPath: AnsiString;
  Rc: Integer;
  Loaded, Failed: Integer;
begin
  Loaded := 0;
  Failed := 0;
  for i := 0 to High(WANTED) do
  begin
    BinPath := ToAnsi(FObsPluginBinDir + '\' + WANTED[i] + '.dll');
    DataPath := ToAnsi(FObsPluginDataDir + '\' + WANTED[i]);
    Module := nil;
    Rc := obs_open_module(Module, PAnsiChar(BinPath), PAnsiChar(DataPath));
    if (Rc = 0) and (Module <> nil) then
    begin
      if obs_init_module(Module) then
      begin
        Inc(Loaded);
        Continue;
      end;
    end;
    Inc(Failed);
    Log('libobs: plugin %s falhou (rc=%d).', [WANTED[i], Rc]);
  end;
  obs_post_load_modules;
  Log('libobs: %d plugins carregados, %d falharam.', [Loaded, Failed]);
end;

procedure TOBSEngine.EnsureInitialized;
var
  Ret: Integer;
  OVI: obs_video_info;
  OAI: obs_audio_info;
  GraphicsModule: AnsiString;
begin
  if FInitialized then Exit;

  ResolvePaths;

  if not FileExists(FExeDir + 'obs.dll') then
    raise Exception.CreateFmt('obs.dll nao encontrado em %s. NoOBS.exe ' +
      'precisa rodar da pasta bin\64bit do OBS.', [FExeDir]);

  if not obs_startup('en-US', nil, nil) then
    raise Exception.Create('obs_startup falhou.');
  Log('libobs: startup ok.');

  base_set_log_handler(ObsLogHandler, nil);

  try
    // Video: canvas placeholder 1920x1080 (reconfigurado depois).
    // graphics_module = nome simples — LoadLibrary resolve via pasta do exe.
    GraphicsModule := 'libobs-d3d11';
    FillChar(OVI, SizeOf(OVI), 0);
    OVI.graphics_module := PAnsiChar(GraphicsModule);
    OVI.fps_num := 30;
    OVI.fps_den := 1;
    OVI.base_width := 1920;
    OVI.base_height := 1080;
    OVI.output_width := 1920;
    OVI.output_height := 1080;
    OVI.output_format := VIDEO_FORMAT_NV12;
    OVI.adapter := 0;
    OVI.gpu_conversion := ByteBool(True);
    OVI.colorspace := VIDEO_CS_709;
    OVI.range := VIDEO_RANGE_PARTIAL;
    OVI.scale_type := OBS_SCALE_BICUBIC;

    Ret := obs_reset_video(@OVI);
    if Ret <> OBS_VIDEO_SUCCESS then
      raise Exception.CreateFmt('obs_reset_video falhou (code=%d).', [Ret]);
    Log('libobs: video ok (1920x1080 placeholder).');

    OAI.samples_per_sec := 48000;
    OAI.speakers := SPEAKERS_STEREO;
    if not obs_reset_audio(@OAI) then
      raise Exception.Create('obs_reset_audio falhou.');
    Log('libobs: audio ok (48kHz stereo).');

    LoadModules;
  except
    Log('libobs: init parcial — chamando obs_shutdown pra reset.');
    try obs_shutdown; except end;
    raise;
  end;

  FInitialized := True;
end;

procedure TOBSEngine.ReleaseRecordingObjects;
var
  i: Integer;
begin
  // Se libobs nao foi inicializado, nao temos nada que precise de
  // limpeza via API — so zeramos os ponteiros locais (que ja deveriam
  // ser nil). Chamar obs_set_output_source antes de obs_startup
  // resulta em AV dentro do obs.dll.
  if not FInitialized then
  begin
    GOutput := nil;
    GVideoEncoder := nil;
    SetLength(GAudioEncoders, 0);
    SetLength(GSources, 0);
    GScene := nil;
    Exit;
  end;

  // Limpa todos os canais de saida (cena + audio sources atribuidos).
  // Sem isso, a proxima gravacao herda referencias velhas e crasha.
  // try/except defensivo: AV dentro do obs.dll durante cleanup nao
  // pode derrubar o app (ex.: libobs em estado intermediario apos um
  // init parcial).
  for i := 0 to 63 do
    try obs_set_output_source(Cardinal(i), nil); except end;

  // Ordem: output -> encoders -> sources -> scene
  if GOutput <> nil then
  begin
    try obs_output_release(GOutput); except end;
    GOutput := nil;
  end;
  if GVideoEncoder <> nil then
  begin
    try obs_encoder_release(GVideoEncoder); except end;
    GVideoEncoder := nil;
  end;
  for i := 0 to High(GAudioEncoders) do
    if GAudioEncoders[i] <> nil then
      try obs_encoder_release(GAudioEncoders[i]); except end;
  SetLength(GAudioEncoders, 0);
  for i := 0 to High(GSources) do
    if GSources[i].Source <> nil then
      try obs_source_release(GSources[i].Source); except end;
  SetLength(GSources, 0);
  if GScene <> nil then
  begin
    try obs_scene_release(GScene); except end;
    GScene := nil;
  end;
end;

procedure TOBSEngine.BuildAndStartRecording(const AOutputPath: string;
  AAutoProfile: Boolean = False);

  // Resolvedor de "este dispositivo entra na gravacao?". Na gravacao manual
  // e o GetSourceActive de sempre; na auto-gravacao, tambem exige o device
  // marcado no perfil (aba Comportamento). Centraliza pra os pontos de
  // webcam/mic/speaker nao repetirem o if do perfil.
  function SrcActive(const ACat, AId: string; ADefault: Boolean): Boolean;
  begin
    if AAutoProfile then
      Result := GetSourceActiveForAuto(ACat, AId, ADefault)
    else
      Result := GetSourceActive(ACat, AId, ADefault);
  end;

var
  Monitors: TOBSMonitorArray;
  Cams: TWebcamInfoArray;
  BoundingW, BoundingH: Integer;
  CanvasW, CanvasH: Integer;
  EncoderScale, Scale: Double;
  RawBoundingW: Integer;
  i, j: Integer;
  Ret: Integer;
  OVI: obs_video_info;
  GraphicsModule: AnsiString;
  MonId: AnsiString;
  SourceName: AnsiString;
  Settings: obs_data_t;
  Src: obs_source_t;
  Item: obs_sceneitem_t;
  Pos, Sc: TVec2;
  PosX: Double;
  Mics, Outputs, ReorderedMics, ReorderedOuts: TArray<TObsAudioDev>;
  MicTracks, OutTracks: TArray<Integer>;
  MicEnabledArr, MicDefaultArr: TArray<Boolean>;
  OutEnabledArr, OutDefaultArr: TArray<Boolean>;
  DefaultMicId, DefaultSpkId: string;
  WinDevs: WinAudioMeter.TAudioDeviceInfoArray;
  TotalTracks: Integer;
  TrackBitmask: Cardinal;
  AudioName: AnsiString;
  Enabled: Boolean;
  AudioChannel: Cardinal;
  OutputSettings: obs_data_t;
  AEncSettings: obs_data_t;
  TrackNames: TArray<string>;
begin
  if FRecording then
    raise Exception.Create('Ja esta gravando.');

  ReleaseRecordingObjects;

 try
  // 1. Inventario de monitores (Win32 — mesmo indexador da UI).
  Log('-- Inventario --');
  Monitors := MonitorsFromWinPreview;
  Log('   %d monitor(es) detectado(s).', [Length(Monitors)]);
  Monitors := FilterEnabledMonitors(Monitors, AAutoProfile);
  Log('   %d monitor(es) habilitado(s).', [Length(Monitors)]);

  // Sort por PositionX.
  for i := 0 to High(Monitors) - 1 do
    for j := i + 1 to High(Monitors) do
      if Monitors[j].PositionX < Monitors[i].PositionX then
      begin
        var Tmp := Monitors[i]; Monitors[i] := Monitors[j]; Monitors[j] := Tmp;
      end;

  // Bounding compacto (monitores + webcams habilitadas).
  BoundingW := 0;
  BoundingH := 0;
  for i := 0 to High(Monitors) do
  begin
    BoundingW := BoundingW + Monitors[i].Width;
    if Monitors[i].Height > BoundingH then BoundingH := Monitors[i].Height;
  end;
  Cams := EnumerateWebcams;
  for i := 0 to High(Cams) do
    if SrcActive('webcams', Cams[i].Name, False) then
    begin
      BoundingW := BoundingW + Cams[i].Width;
      if Cams[i].Height > BoundingH then BoundingH := Cams[i].Height;
    end;

  // Fallback audio-only: canvas preto 800x600.
  if (BoundingW = 0) or (BoundingH = 0) then
  begin
    BoundingW := 800;
    BoundingH := 600;
    Log('   Sem monitor/webcam — canvas preto 800x600 (gravacao so-audio).');
  end
  else
    Log('   Bounding compacto: %dx%d', [BoundingW, BoundingH]);

  RawBoundingW := BoundingW;

  // Clamp baseado no encoder que sera usado. H.264 hw em AMD/Intel/NVENC
  // pre-Turing = 4096; HEVC/AV1 hw = 8192; x264 = 8192.
  var EncoderMaxDim: Integer := GetEncoderMaxDimension;
  EncoderScale := 1.0;
  if (BoundingW > EncoderMaxDim) or (BoundingH > EncoderMaxDim) then
  begin
    var Sx: Double := EncoderMaxDim / BoundingW;
    var Sy: Double := EncoderMaxDim / BoundingH;
    EncoderScale := Sx;
    if Sy < EncoderScale then EncoderScale := Sy;
    BoundingW := Round(BoundingW * EncoderScale);
    BoundingH := Round(BoundingH * EncoderScale);
    if Odd(BoundingW) then Dec(BoundingW);
    if Odd(BoundingH) then Dec(BoundingH);
    Log('   Bounding clamped: %dx%d (limite %d, scale=%.3f).',
      [BoundingW, BoundingH, EncoderMaxDim, EncoderScale]);
  end;
  CanvasW := BoundingW;
  CanvasH := BoundingH;

  // NV12 (4:2:0) exige largura E altura PARES — chroma subsampled por 2.
  // A correcao Odd acima so rodava DENTRO do branch de clamp; no caminho
  // sem clamp (bounding <= max), um total impar de larguras de monitor ou
  // uma resolucao incomum chegava cru no obs_reset_video. Forca par sempre.
  if Odd(CanvasW) then Dec(CanvasW);
  if Odd(CanvasH) then Dec(CanvasH);
  if CanvasW < 2 then CanvasW := 2;
  if CanvasH < 2 then CanvasH := 2;

  // 2. Configura video (canvas). obs_reset_video pode ser chamado
  // entre gravacoes sem problemas — so nao durante output ativo.
  var FpsVal: Integer := OBSConfig.GetConfigInt('recordingFps', 30);
  // 0 = nao configurado (config vazio) → usa 30 (padrao do NoOBS, mais
  // compacto que o 60fps do OBS Studio). User pode subir no slider de
  // Configuracoes ate o Hz do monitor mais rapido. Clamp defensivo nos
  // DOIS extremos (single source of truth): < 10 → 30; teto sanitario de
  // 1000 pra um config.json editado a mao nao mandar fps_num absurdo
  // (ex.: 100000000) direto pro obs_reset_video.
  if FpsVal < 10 then FpsVal := 30
  else if FpsVal > 1000 then FpsVal := 1000;
  Log('-- Configurando video %dx%d @ %d fps --', [CanvasW, CanvasH, FpsVal]);
  GraphicsModule := 'libobs-d3d11';
  FillChar(OVI, SizeOf(OVI), 0);
  OVI.graphics_module := PAnsiChar(GraphicsModule);
  OVI.fps_num := Cardinal(FpsVal);
  OVI.fps_den := 1;
  OVI.base_width := Cardinal(CanvasW);
  OVI.base_height := Cardinal(CanvasH);
  OVI.output_width := Cardinal(CanvasW);
  OVI.output_height := Cardinal(CanvasH);
  OVI.output_format := VIDEO_FORMAT_NV12;
  OVI.adapter := 0;
  OVI.gpu_conversion := ByteBool(True);
  OVI.colorspace := VIDEO_CS_709;
  OVI.range := VIDEO_RANGE_PARTIAL;
  OVI.scale_type := OBS_SCALE_BICUBIC;

  Ret := obs_reset_video(@OVI);
  if Ret <> OBS_VIDEO_SUCCESS then
    raise Exception.CreateFmt('obs_reset_video %dx%d falhou (code=%d).',
      [CanvasW, CanvasH, Ret]);

  // Zera + grava canvas no layout — regions sao adicionados conforme
  // os sources sao posicionados na scene mais abaixo.
  FCurrentLayout := Default(TRecordingLayout);
  FCurrentLayout.CanvasW := CanvasW;
  FCurrentLayout.CanvasH := CanvasH;

  // Scale final dos sources.
  if RawBoundingW > 0 then
    Scale := CanvasW / RawBoundingW
  else
    Scale := 1.0;

  // 3. Criar scene.
  Log('-- Cena "%s" --', [SCENE_NAME]);
  GScene := obs_scene_create(PAnsiChar(ToAnsi(SCENE_NAME)));
  if GScene = nil then
    raise Exception.Create('obs_scene_create falhou.');
  obs_set_output_source(0, obs_scene_get_source(GScene));

  // 4. Monitores.
  Log('-- Capturas de monitor --');
  PosX := 0;
  for i := 0 to High(Monitors) do
  begin
    SourceName := ToAnsi(Format('NoOBS Monitor %d', [Monitors[i].Index]));
    MonId := ResolveMonitorId(Monitors[i]);

    Settings := MakeSettings;
    SetInt(Settings, 'monitor', Monitors[i].Index);
    if MonId <> '' then
      SetStr(Settings, 'monitor_id', MonId);

    Src := CreateSource('monitor_capture', SourceName, Settings);
    Item := obs_scene_add(GScene, Src);

    Pos := MakeVec2(Single(PosX), 0);
    obs_sceneitem_set_pos(Item, @Pos);
    Sc := MakeVec2(Single(Scale), Single(Scale));
    obs_sceneitem_set_scale(Item, @Sc);

    Log('   %s -> canvas (%.0f, 0) scale=%.3f monitor_id=%s',
      [string(SourceName), PosX, Scale, string(MonId)]);

    // Registra a regiao no layout (player usa pra seletor de monitor).
    SetLength(FCurrentLayout.Regions, Length(FCurrentLayout.Regions) + 1);
    var RegIdx := High(FCurrentLayout.Regions);
    FCurrentLayout.Regions[RegIdx].Name := Monitors[i].Name;
    if FCurrentLayout.Regions[RegIdx].Name = '' then
      FCurrentLayout.Regions[RegIdx].Name :=
        Format('Monitor %d', [Monitors[i].Index]);
    FCurrentLayout.Regions[RegIdx].Kind := 'monitor';
    FCurrentLayout.Regions[RegIdx].X    := Round(PosX);
    FCurrentLayout.Regions[RegIdx].Y    := 0;
    FCurrentLayout.Regions[RegIdx].W    := Round(Monitors[i].Width  * Scale);
    FCurrentLayout.Regions[RegIdx].H    := Round(Monitors[i].Height * Scale);

    PosX := PosX + Monitors[i].Width * Scale;
  end;

  // 5. Webcams habilitadas.
  Log('-- Webcams --');
  for i := 0 to High(Cams) do
  begin
    if not SrcActive('webcams', Cams[i].Name, False) then Continue;

    SourceName := ToAnsi('NoOBS Webcam - ' + Cams[i].Name);
    Settings := MakeSettings;
    SetStr(Settings, 'video_device_id', ToAnsi(Cams[i].DeviceId));
    SetStr(Settings, 'last_video_device_id', ToAnsi(Cams[i].DeviceId));
    SetInt(Settings, 'res_type', 1);
    SetStr(Settings, 'resolution', ToAnsi(Format('%dx%d', [Cams[i].Width, Cams[i].Height])));
    SetStr(Settings, 'last_resolution', ToAnsi(Format('%dx%d', [Cams[i].Width, Cams[i].Height])));
    SetInt(Settings, 'video_format', 400); // MJPEG
    SetInt(Settings, 'frame_interval', 333333); // 30fps
    SetBool(Settings, 'active', True);
    SetInt(Settings, 'audio_output_mode', 2); // none

    Src := CreateSource('dshow_input', SourceName, Settings);
    Item := obs_scene_add(GScene, Src);

    Pos := MakeVec2(Single(PosX), 0);
    obs_sceneitem_set_pos(Item, @Pos);
    // Bounds stretch: preenche o espaco reservado.
    obs_sceneitem_set_bounds_type(Item, OBS_BOUNDS_STRETCH);
    var Bounds := MakeVec2(Single(Cams[i].Width * Scale),
      Single(Cams[i].Height * Scale));
    obs_sceneitem_set_bounds(Item, @Bounds);

    Log('   %s -> canvas (%.0f, 0) bounds=%dx%d',
      [string(SourceName), PosX,
       Round(Cams[i].Width * Scale), Round(Cams[i].Height * Scale)]);

    // Registra a webcam no layout pra player oferecer "zoom" nela.
    SetLength(FCurrentLayout.Regions, Length(FCurrentLayout.Regions) + 1);
    var CamRegIdx := High(FCurrentLayout.Regions);
    FCurrentLayout.Regions[CamRegIdx].Name := 'Webcam — ' + Cams[i].Name;
    FCurrentLayout.Regions[CamRegIdx].Kind := 'webcam';
    FCurrentLayout.Regions[CamRegIdx].X    := Round(PosX);
    FCurrentLayout.Regions[CamRegIdx].Y    := 0;
    FCurrentLayout.Regions[CamRegIdx].W    := Round(Cams[i].Width  * Scale);
    FCurrentLayout.Regions[CamRegIdx].H    := Round(Cams[i].Height * Scale);

    PosX := PosX + Cams[i].Width * Scale;
  end;

  // 6. Audio: enumera devices via obs_properties. Try/except defensivo:
  // se WASAPI/libobs falhar (driver de audio bugado), grava ainda
  // funciona — fica so com mix vazio (silencio).
  Log('-- Audio --');
  SetLength(Mics, 0);
  SetLength(Outputs, 0);
  try Mics    := EnumerateObsAudioDevices('wasapi_input_capture');  except
    on E: Exception do Log('   enum mics falhou: %s', [E.Message]); end;
  try Outputs := EnumerateObsAudioDevices('wasapi_output_capture'); except
    on E: Exception do Log('   enum outputs falhou: %s', [E.Message]); end;
  Log('   %d mic(s), %d output(s)', [Length(Mics), Length(Outputs)]);

  // Track strategy: Track 1 = mix, Tracks 2-6 = isolated (5 slots max).
  //
  // Atribuicao de tracks via funcao centralizada (mesma logica usada
  // pra montar a lista pra UI). Prepara arrays paralelos de flags.
  SetLength(MicEnabledArr, Length(Mics));
  SetLength(MicDefaultArr, Length(Mics));
  SetLength(OutEnabledArr, Length(Outputs));
  SetLength(OutDefaultArr, Length(Outputs));

  DefaultMicId := '';
  DefaultSpkId := '';
  WinDevs := WinAudioMeter.EnumerateAudioDevices;
  for j := 0 to High(WinDevs) do
    if WinDevs[j].IsDefault then
    begin
      if WinDevs[j].Kind = adkInput then DefaultMicId := WinDevs[j].DeviceId
      else DefaultSpkId := WinDevs[j].DeviceId;
    end;

  // Reordena: default primeiro, depois os outros (na ordem original).
  // Mesma logica que BuildAudioJsonWithTracks no OBSBridge — mantem
  // engine e UI sincronizados, default sempre na primeira track isolada.
  ReorderedMics := nil;
  for j := 0 to High(Mics) do
    if (DefaultMicId <> '') and
       SameText(FromAnsi(PAnsiChar(Mics[j].DeviceId)), DefaultMicId) then
    begin
      SetLength(ReorderedMics, Length(ReorderedMics) + 1);
      ReorderedMics[High(ReorderedMics)] := Mics[j];
    end;
  for j := 0 to High(Mics) do
    if (DefaultMicId = '') or
       not SameText(FromAnsi(PAnsiChar(Mics[j].DeviceId)), DefaultMicId) then
    begin
      SetLength(ReorderedMics, Length(ReorderedMics) + 1);
      ReorderedMics[High(ReorderedMics)] := Mics[j];
    end;
  Mics := ReorderedMics;

  ReorderedOuts := nil;
  for j := 0 to High(Outputs) do
    if (DefaultSpkId <> '') and
       SameText(FromAnsi(PAnsiChar(Outputs[j].DeviceId)), DefaultSpkId) then
    begin
      SetLength(ReorderedOuts, Length(ReorderedOuts) + 1);
      ReorderedOuts[High(ReorderedOuts)] := Outputs[j];
    end;
  for j := 0 to High(Outputs) do
    if (DefaultSpkId = '') or
       not SameText(FromAnsi(PAnsiChar(Outputs[j].DeviceId)), DefaultSpkId) then
    begin
      SetLength(ReorderedOuts, Length(ReorderedOuts) + 1);
      ReorderedOuts[High(ReorderedOuts)] := Outputs[j];
    end;
  Outputs := ReorderedOuts;

  // Re-aloca arrays apos reorder.
  SetLength(MicEnabledArr, Length(Mics));
  SetLength(MicDefaultArr, Length(Mics));
  SetLength(OutEnabledArr, Length(Outputs));
  SetLength(OutDefaultArr, Length(Outputs));

  for j := 0 to High(Mics) do
  begin
    MicEnabledArr[j] := SrcActive('mics', Mics[j].Name, True);
    MicDefaultArr[j] := (DefaultMicId <> '') and
      SameText(FromAnsi(PAnsiChar(Mics[j].DeviceId)), DefaultMicId);
  end;
  for j := 0 to High(Outputs) do
  begin
    OutEnabledArr[j] := SrcActive('speakers', Outputs[j].Name, True);
    OutDefaultArr[j] := (DefaultSpkId <> '') and
      SameText(FromAnsi(PAnsiChar(Outputs[j].DeviceId)), DefaultSpkId);
  end;

  ComputeAudioTrackAssignments(MicEnabledArr, MicDefaultArr,
    OutEnabledArr, OutDefaultArr, MicTracks, OutTracks, TotalTracks);

  Log('   habilitados: %d mic(s), %d output(s)',
    [CountTrue(MicEnabledArr), CountTrue(OutEnabledArr)]);

  // Canal 0 ja e a cena (video). Canais 1+ recebem audio sources.
  // Esse e o jeito canonico do OBS — sources soltas atribuidas a
  // canais sao mixadas no output mesmo sem estar na cena.
  // OBS tem MAX_CHANNELS = 64, entao cabe tudo.
  AudioChannel := 1;

  Log('-- Microfones --');
  for j := 0 to High(Mics) do
  begin
    AudioName := ToAnsi(MANAGED_PREFIX + 'Mic - ' + Mics[j].Name);
    Settings := MakeSettings;
    SetStr(Settings, 'device_id', Mics[j].DeviceId);
    Src := CreateSource('wasapi_input_capture', AudioName, Settings);

    // Bitmask: bit 0 = Mix (track 1). Se MicTracks[j] > 0, adiciona o
    // bit da track isolada. Disabled (MicTracks[j] = 0) fica so no Mix
    // — mas como esta muted, nao contribui pra nada.
    if MicTracks[j] > 0 then
      TrackBitmask := 1 or Cardinal(1 shl (MicTracks[j] - 1))
    else
      TrackBitmask := 1;
    obs_source_set_audio_mixers(Src, TrackBitmask);

    Enabled := SrcActive('mics', Mics[j].Name, True);
    obs_source_set_muted(Src, ByteBool(not Enabled));

    obs_set_output_source(AudioChannel, Src);
    Inc(AudioChannel);

    Log('   %s -> tracks 1,%d muted=%s',
      [string(AudioName), MicTracks[j], BoolToStr(not Enabled, True)]);
  end;
  if Length(Mics) = 0 then Log('   (nenhum mic detectado)');

  Log('-- Saidas de audio --');
  for j := 0 to High(Outputs) do
  begin
    AudioName := ToAnsi(MANAGED_PREFIX + 'Out - ' + Outputs[j].Name);
    Settings := MakeSettings;
    SetStr(Settings, 'device_id', Outputs[j].DeviceId);
    Src := CreateSource('wasapi_output_capture', AudioName, Settings);

    if OutTracks[j] > 0 then
      TrackBitmask := 1 or Cardinal(1 shl (OutTracks[j] - 1))
    else
      TrackBitmask := 1;
    obs_source_set_audio_mixers(Src, TrackBitmask);

    Enabled := SrcActive('speakers', Outputs[j].Name, True);
    obs_source_set_muted(Src, ByteBool(not Enabled));

    obs_set_output_source(AudioChannel, Src);
    Inc(AudioChannel);

    Log('   %s -> tracks 1,%d muted=%s ch=%d',
      [string(AudioName), OutTracks[j], BoolToStr(not Enabled, True),
       AudioChannel - 1]);
  end;
  if Length(Outputs) = 0 then Log('   (nenhuma saida detectada)');

  // 7. Encoder de video.
  Log('-- Encoder --');
  GVideoEncoder := SelectVideoEncoder;
  obs_encoder_set_video(GVideoEncoder, obs_get_video);
  // Guarda COMO esta gravacao foi feita, pra virar meta do arquivo. Le do
  // encoder criado, nao do config: o fallback pode ter escolhido outro
  // codec, e o config pode mudar antes do stop.
  FVideoEncoderId := '';
  if GVideoEncoder <> nil then
  begin
    var EncIdPtr := obs_encoder_get_id(GVideoEncoder);
    if EncIdPtr <> nil then FVideoEncoderId := FromAnsi(EncIdPtr);
  end;

  // Audio encoders: um por track. O "name" do encoder (2o param de
  // obs_audio_encoder_create) e escrito como metadata "title" da
  // stream no MKV — visivel no info panel e em editores externos.
  Log('-- Audio encoders (%d tracks) --', [TotalTracks]);
  TrackNames := BuildTrackNames(TotalTracks, Mics, Outputs,
    MicTracks, OutTracks);
  SetLength(GAudioEncoders, TotalTracks);
  for i := 0 to TotalTracks - 1 do
  begin
    AEncSettings := MakeSettings;
    SetInt(AEncSettings, 'bitrate', 192);
    GAudioEncoders[i] := obs_audio_encoder_create(
      'ffmpeg_aac',
      PAnsiChar(ToAnsi(TrackNames[i])),
      AEncSettings, NativeUInt(i), nil);
    obs_data_release(AEncSettings);
    if GAudioEncoders[i] = nil then
      raise Exception.CreateFmt('obs_audio_encoder_create falhou (track %d).', [i + 1]);
    obs_encoder_set_audio(GAudioEncoders[i], obs_get_audio);
    Log('   Track %d: %s', [i + 1, TrackNames[i]]);
  end;

  // 8. Output (ffmpeg_muxer = gravacao em arquivo).
  Log('-- Output --');
  OutputSettings := MakeSettings;
  SetStr(OutputSettings, 'path', ToAnsi(AOutputPath));
  SetStr(OutputSettings, 'muxer_settings', '');
  GOutput := obs_output_create('ffmpeg_muxer', 'NoOBS Recording',
    OutputSettings, nil);
  obs_data_release(OutputSettings);
  if GOutput = nil then
    raise Exception.Create('obs_output_create falhou.');

  obs_output_set_video_encoder(GOutput, GVideoEncoder);
  for i := 0 to High(GAudioEncoders) do
    obs_output_set_audio_encoder(GOutput, GAudioEncoders[i], NativeUInt(i));

  // 9. Iniciar gravacao.
  Log('-- StartRecording -> %s --', [AOutputPath]);
  if not obs_output_start(GOutput) then
  begin
    var ErrMsg := FromAnsi(obs_output_get_last_error(GOutput));
    ReleaseRecordingObjects;
    raise Exception.CreateFmt('obs_output_start falhou: %s', [ErrMsg]);
  end;

  FOutputPath := AOutputPath;
  FRecording := True;
  FStopping := False;
  // Conecta ao sinal "stop" do output — e por ele que sabemos, sem poll
  // nem Sleep, que a gravacao terminou de verdade (arquivo completo,
  // threads encerradas). Mesma estrategia do frontend do OBS.
  ConnectStopSignal;
  Log('Gravacao iniciada.');
 except
   // Qualquer excecao no meio do build (encoder falhou, source nil,
   // ResolveMonitorId, obs_output_create nil, etc.) deixaria scene/
   // sources/encoders meio-criados e canais ligados a sources que
   // serao destruidas. Limpa antes de propagar pro chamador.
   ReleaseRecordingObjects;
   raise;
 end;
end;

procedure TOBSEngine.ConnectStopSignal;
begin
  if GOutput = nil then Exit;
  FStopSignalHandler := obs_output_get_signal_handler(GOutput);
  if FStopSignalHandler <> nil then
    signal_handler_connect(FStopSignalHandler, PAnsiChar(SIG_STOP),
      @StopSignalThunk, Self);
end;

procedure TOBSEngine.DisconnectStopSignal;
begin
  // Desconecta ANTES do release do output (o handler vive dentro do
  // output; depois do release o ponteiro fica invalido). Idempotente.
  if FStopSignalHandler <> nil then
  begin
    try
      signal_handler_disconnect(FStopSignalHandler, PAnsiChar(SIG_STOP),
        @StopSignalThunk, Self);
    except
    end;
    FStopSignalHandler := nil;
  end;
end;

procedure TOBSEngine.OnStopSignal;
// Main thread (via TThread.Queue do StopSignalThunk). O sinal "stop"
// disparou = arquivo completo + threads encerradas. Agora e seguro
// liberar e notificar o Bridge.
begin
  if FShuttingDown then Exit;
  FinalizeStop(True);
end;

procedure TOBSEngine.FinalizeStop(AInvokeCallback: Boolean);
// Main thread. Libera os objetos da gravacao e (opcional) chama o
// callback OnStopped. Idempotente via FStopping — o sinal "stop" e o
// timeout do Bridge podem ambos chamar; so o primeiro age.
var
  P: string;
begin
  if not FStopping then Exit;
  FStopping := False;
  FRecording := False;
  P := FOutputPath;
  DisconnectStopSignal;
  ReleaseRecordingObjects;
  Log('Gravacao finalizada: %s', [P]);
  if AInvokeCallback and Assigned(FOnStopped) then
    try FOnStopped(P); except on E: Exception do
      Log('OnStopped levantou: %s', [E.Message]); end;
end;

procedure TOBSEngine.RequestStop;
// Pede o stop e retorna na hora. obs_output_stop e assincrono: o output
// emite "stop" quando terminou, e StopSignalThunk -> OnStopSignal ->
// FinalizeStop conduz o resto. NAO bloqueia a UI (sem poll, sem Sleep).
begin
  if (not FRecording) or FStopping then Exit;
  FStopping := True;
  FStoppingSince := GetTickCount64;
  Log('Parando gravacao (assincrono)...');
  try obs_output_stop(GOutput); except on E: Exception do
    Log('obs_output_stop levantou: %s', [E.Message]); end;
end;

procedure TOBSEngine.ForceCompleteStop;
// Chamado pelo timeout do Bridge se o sinal "stop" nunca chegou. Forca
// a finalizacao (o obs_output_release no FinalizeStop ainda espera/junta
// as threads internamente, entao e seguro). Idempotente.
begin
  if not FStopping then Exit;
  Log('ForceCompleteStop: sinal "stop" nao chegou — finalizando a forca.');
  FinalizeStop(True);
end;

function TOBSEngine.StopRecording: string;
// Caminho SINCRONO — usado so no shutdown do app, onde bloquear e
// aceitavel e nao ha message loop pra drenar o TThread.Queue do sinal.
var
  Deadline: Cardinal;
begin
  Result := FOutputPath;
  if not (FRecording or FStopping) then Exit;

  if not FStopping then
  begin
    FStopping := True;
  FStoppingSince := GetTickCount64;
    Log('Parando gravacao (sincrono, shutdown)...');
    try obs_output_stop(GOutput); except end;
  end;

  // Espera output parar (flush de buffers). obs_output_release no
  // FinalizeStop tambem espera/junta, mas poll aqui evita o force-stop.
  Deadline := GetTickCount + 10000;
  while obs_output_active(GOutput) do
  begin
    if GetTickCount > Deadline then
    begin
      Log('Timeout esperando output parar.');
      Break;
    end;
    Sleep(50);
  end;

  FinalizeStop(False); // sem callback — shutdown nao precisa de UI push
end;

function TOBSEngine.IsRecording: Boolean;
begin
  Result := FRecording;
end;

function TOBSEngine.IsInitialized: Boolean;
begin
  Result := FInitialized;
end;

function TOBSEngine.StoppingElapsedMs: UInt64;
begin
  if not FStopping then Exit(0);
  Result := GetTickCount64 - FStoppingSince;
end;

function TOBSEngine.IsStopping: Boolean;
begin
  Result := FStopping;
end;

procedure TOBSEngine.SetSourceMuted(const ASourceName: string;
  AMuted: Boolean);
var
  Src: obs_source_t;
begin
  Src := FindSourceByName(ToAnsi(ASourceName));
  if Src <> nil then
    obs_source_set_muted(Src, ByteBool(AMuted));
end;

type
  TShutdownWatchdog = class(TThread)
  // Rede de seguranca do obs_shutdown, que roda na MAIN thread.
  //
  // Com o RtwqStartup no arranque (pegadinha #48) o obs_shutdown conclui
  // em ~80ms e este watchdog nunca dispara. Ele existe pro caso de UMA
  // regressao futura no win-wasapi (ou outro plugin) voltar a travar o
  // teardown: como a main fica bloqueada dentro do obs_shutdown, ela nao
  // pode vigiar a si mesma. Esta thread espera o prazo e, se estourar,
  // encerra o processo — melhor que virar zumbi segurando o mutex de
  // instancia unica.
  //
  // NAO dumpa mais a pilha das threads: o forense (SuspendThread +
  // GetThreadContext + resolucao modulo+offset) que crackeou a #48 foi
  // removido depois do fix — eram ~220 linhas de Win32 que so rodavam
  // neste caso, que nao deve mais ocorrer. A TECNICA esta preservada na
  // pegadinha #31 do CLAUDE.md; se um hang voltar, reintroduza-a de la.
  private
    FEvent: THandle;
    FTimeoutMs: Cardinal;
  protected
    procedure Execute; override;
  public
    constructor Create(ATimeoutMs: Cardinal);
    destructor Destroy; override;
    procedure Cancel;
  end;

constructor TShutdownWatchdog.Create(ATimeoutMs: Cardinal);
begin
  FTimeoutMs := ATimeoutMs;
  FEvent := CreateEvent(nil, True, False, nil);
  // Create(False) + NAO chamar Start: o AfterConstruction da TThread faz o
  // unico ResumeThread, depois do construtor inteiro (pegadinha #45).
  inherited Create(False);
  FreeOnTerminate := False;
end;

destructor TShutdownWatchdog.Destroy;
begin
  inherited;
  if FEvent <> 0 then CloseHandle(FEvent);
end;

procedure TShutdownWatchdog.Execute;
begin
  if WaitForSingleObject(FEvent, FTimeoutMs) = WAIT_OBJECT_0 then Exit;
  // Segunda chance curta: fecha a corrida em que o obs_shutdown terminou
  // exatamente no estouro do prazo. Matar o processo nesse instante seria
  // desperdicar um shutdown que deu certo.
  if WaitForSingleObject(FEvent, 250) = WAIT_OBJECT_0 then Exit;
  // Regressao: o obs_shutdown travou (nao deve acontecer com a #48 no
  // lugar). Pra diagnosticar QUAL thread travou, reintroduza o dump de
  // pilhas descrito na pegadinha #31 do CLAUDE.md. Aqui so encerramos.
  Log('libobs: obs_shutdown NAO retornou em %dms — encerrando o processo ' +
    '(OS libera o resto).', [FTimeoutMs]);
  TerminateProcess(GetCurrentProcess, 0);
end;

procedure TShutdownWatchdog.Cancel;
begin
  SetEvent(FEvent);
  WaitForSingleObject(Handle, 2000);
end;

procedure TOBSEngine.Teardown;
const
  // 5s: com o RtwqStartup no lugar (pegadinha #48) o obs_shutdown conclui
  // em ~80ms. Este prazo so existe pro caso de uma regressao futura voltar
  // a travar — ai o watchdog encerra o processo.
  SHUTDOWN_TIMEOUT_MS = 5000;
var
  Watchdog: TShutdownWatchdog;
  T0: UInt64;
begin
  // Marca shutdown: qualquer OnStopSignal enfileirado que ainda venha a
  // rodar (improvavel — o message loop ja saiu) vira no-op, evitando
  // mexer em objetos meio-liberados pelo obs_shutdown.
  FShuttingDown := True;
  DisconnectStopSignal;
  // Pede o stop do output. A liberacao dos objetos vem logo abaixo, ANTES
  // do obs_shutdown (ver bloco seguinte).
  if FRecording or FStopping then
  begin
    try obs_output_stop(GOutput); except end;
    // obs_output_stop e ASSINCRONO (pegadinha #41): retorna na hora e o
    // output continua drenando encoder + muxer. Chamar obs_shutdown logo
    // em seguida ATROPELA essa finalizacao — e ela custa SEGUNDOS (medimos
    // 5,3s num canvas 2x4K + webcam), mais do que o timeout do shutdown.
    // Resultado: fechar o app gravando abandonava a finalizacao e podia
    // truncar o arquivo, justamente o que o MKV deveria evitar.
    FRecording := False;
    FStopping := False;
  end;

  // LIBERA OS OBJETOS ANTES DO obs_shutdown — ordem do proprio OBS Studio,
  // cujo frontend e explicito: "any obs data must be released before
  // calling obs_shutdown" (OBSBasic.cpp), e faz outputHandler.reset()
  // antes de chamar obs_shutdown() no OBSApp.
  //
  // Antes faziamos o CONTRARIO (deixar o obs_shutdown liberar tudo),
  // achando que liberar antes causaria "Double destroy". O efeito era o
  // obs_shutdown ter que desmontar objetos vivos nas threads internas
  // dele — e estourar o timeout de 5s, sendo abandonado.
  //
  // De quebra isso resolve a espera do output sem poll: o
  // obs_output_release se AUTO-SINCRONIZA (os_event_wait no stopping_event
  // + pthread_join na thread de captura, pegadinha #41), entao ele so
  // retorna quando a finalizacao terminou de verdade.
  if FInitialized then
  begin
    T0 := GetTickCount64;
    try ReleaseRecordingObjects; except end;
    // ReleaseRecordingObjects solta as NOSSAS referencias. O core, porem,
    // continua com as sources registradas — e desmonta-las e justamente o
    // que trava o obs_shutdown. ClearAllObsData faz o que o frontend do
    // OBS faz antes de encerrar: zera os 64 canais e chama
    // obs_source_remove em toda scene/source.
    try ClearAllObsData; except end;
    Log('libobs: objetos liberados em %dms (antes do shutdown).',
      [GetTickCount64 - T0]);
    // A partir daqui queremos TODAS as mensagens do libobs: a ultima que
    // aparecer antes do timeout aponta o subsistema em que ele travou.
    VerboseShutdownLog := True;

    // obs_shutdown na MAIN THREAD — igual ao OBS Studio, que o chama do
    // cleanup do OBSApp (frontend/OBSApp.cpp:1976).
    //
    // Nao e preferencia de estilo, e contrato da libobs: o obs_startup faz
    // CoInitializeEx(0, COINIT_APARTMENTTHREADED) na thread que o chama
    // (obs.c:1332 -> obs-windows.c:1235) e o obs_shutdown faz o
    // CoUninitialize correspondente (obs.c:1475). As duas TEM que rodar na
    // mesma thread. Como toda chamada libobs nossa e da main (pegadinha
    // #3), o shutdown tambem.
    //
    // O que travava o shutdown NAO era isto — era o RtwqStartup faltando
    // (pegadinha #48): sem a plataforma RTWQ o win-wasapi entrava em modo
    // reconnect e o WASAPISource::Stop() esperava um evento INFINITE que
    // nunca vinha, prendendo a thread de destruicao e, com ela, o
    // obs_wait_for_destroy_queue (1a instrucao do obs_shutdown). Com o
    // RtwqStartup no arranque (OBSRtwq) isso conclui em ~80ms.
    //
    // O watchdog e a rede de seguranca: se uma regressao futura voltar a
    // travar, ele encerra o processo (nada de zumbi segurando o mutex).
    // Ver pegadinhas #31/#48.
    Watchdog := TShutdownWatchdog.Create(SHUTDOWN_TIMEOUT_MS);
    try
      T0 := GetTickCount64;
      try obs_shutdown; except end;
      Log('libobs: shutdown ok em %dms.', [GetTickCount64 - T0]);
    finally
      Watchdog.Cancel;
      Watchdog.Free;
    end;
    GOutput := nil;
    GVideoEncoder := nil;
    SetLength(GAudioEncoders, 0);
    SetLength(GSources, 0);
    GScene := nil;
    FInitialized := False;
  end;
end;

initialization
  // Mascara excecoes da FPU (pegadinha Delphi <-> DLL C).
  //
  // O Delphi por padrao habilita EInvalidOp/EZeroDivide/EOverflow na FPU
  // (mask = [exDenormalized, exUnderflow, exPrecision]). Ja libobs, libav,
  // D3D11 e drivers de GPU assumem o default do Windows (TODAS mascaradas)
  // e rotineiramente produzem NaN/Inf em calculos internos (projecoes,
  // matrizes vazias, scale=0/0 enquanto source assincrona inicializa, etc).
  //
  // Quando o controle volta pro Delphi, o flag invalido fica pendente na
  // FPU. Qualquer operacao FP subsequente (ate em outra unit) dispara
  // "Invalid floating point operation" com stack trace enganoso — o erro
  // aparece muito longe da causa raiz.
  //
  // Sintoma classico: gravacao "Falha ao iniciar: Invalid floating point
  // operation" depois de N segundos enumerando webcam/audio.
  SetExceptionMask(exAllArithmeticExceptions);

end.

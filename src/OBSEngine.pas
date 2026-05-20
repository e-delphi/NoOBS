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
  TOBSEngine = class
  private
    FInitialized: Boolean;
    FRecording: Boolean;
    FOutputPath: string;
    FExeDir: string;
    FObsPluginBinDir: string;
    FObsPluginDataDir: string;
    procedure ResolvePaths;
    procedure LoadModules;
    procedure ReleaseRecordingObjects;
  public
    constructor Create;
    destructor Destroy; override;
    procedure EnsureInitialized;
    procedure BuildAndStartRecording(const AOutputPath: string);
    function  StopRecording: string;
    function  IsRecording: Boolean;
    procedure SetSourceMuted(const ASourceName: string; AMuted: Boolean);
    procedure Teardown;
    property Initialized: Boolean read FInitialized;
  end;

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
  ENCODER_MAX_DIM = 8192;
  SCENE_NAME = 'NoOBS';
  MANAGED_PREFIX = 'NoOBS ';


type
  TSourceEntry = record
    Source: obs_source_t;
    Name: AnsiString;
  end;

var
  GScene: obs_scene_t;
  GOutput: obs_output_t;
  GVideoEncoder: obs_encoder_t;
  GAudioEncoders: TArray<obs_encoder_t>;
  GSources: TArray<TSourceEntry>;

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

procedure ObsLogHandler(log_level: Integer; msg: PAnsiChar;
  args: Pointer; p: Pointer); cdecl;
var
  Prefix, Raw, Arg1Str: string;
  Arg1Ptr: PAnsiChar;
begin
  if msg = nil then Exit;
  if log_level > LOG_WARNING then Exit;

  Raw := string(AnsiString(msg));

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

procedure TOBSEngine.BuildAndStartRecording(const AOutputPath: string);
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

  // 1. Inventario de monitores (Win32 — mesmo indexador da UI).
  Log('-- Inventario --');
  Monitors := MonitorsFromWinPreview;
  Log('   %d monitor(es) detectado(s).', [Length(Monitors)]);
  Monitors := FilterEnabledMonitors(Monitors);
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
    if GetSourceBool('webcams', Cams[i].Name, False) then
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

  // Clamp NVENC 8192.
  EncoderScale := 1.0;
  if (BoundingW > ENCODER_MAX_DIM) or (BoundingH > ENCODER_MAX_DIM) then
  begin
    var Sx: Double := ENCODER_MAX_DIM / BoundingW;
    var Sy: Double := ENCODER_MAX_DIM / BoundingH;
    EncoderScale := Sx;
    if Sy < EncoderScale then EncoderScale := Sy;
    BoundingW := Round(BoundingW * EncoderScale);
    BoundingH := Round(BoundingH * EncoderScale);
    if Odd(BoundingW) then Dec(BoundingW);
    if Odd(BoundingH) then Dec(BoundingH);
    Log('   Bounding clamped: %dx%d (limite %d, scale=%.3f).',
      [BoundingW, BoundingH, ENCODER_MAX_DIM, EncoderScale]);
  end;
  CanvasW := BoundingW;
  CanvasH := BoundingH;

  // 2. Configura video (canvas). obs_reset_video pode ser chamado
  // entre gravacoes sem problemas — so nao durante output ativo.
  Log('-- Configurando video %dx%d --', [CanvasW, CanvasH]);
  GraphicsModule := 'libobs-d3d11';
  FillChar(OVI, SizeOf(OVI), 0);
  OVI.graphics_module := PAnsiChar(GraphicsModule);
  OVI.fps_num := 30;
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
    PosX := PosX + Monitors[i].Width * Scale;
  end;

  // 5. Webcams habilitadas.
  Log('-- Webcams --');
  for i := 0 to High(Cams) do
  begin
    if not GetSourceBool('webcams', Cams[i].Name, False) then Continue;

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
    MicEnabledArr[j] := GetSourceBool('mics', Mics[j].Name, True);
    MicDefaultArr[j] := (DefaultMicId <> '') and
      SameText(FromAnsi(PAnsiChar(Mics[j].DeviceId)), DefaultMicId);
  end;
  for j := 0 to High(Outputs) do
  begin
    OutEnabledArr[j] := GetSourceBool('speakers', Outputs[j].Name, True);
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

    Enabled := GetSourceBool('mics', Mics[j].Name, True);
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

    Enabled := GetSourceBool('speakers', Outputs[j].Name, True);
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
  Log('Gravacao iniciada.');
end;

function TOBSEngine.StopRecording: string;
var
  Deadline: Cardinal;
begin
  Result := FOutputPath;
  if not FRecording then Exit;

  Log('Parando gravacao...');
  obs_output_stop(GOutput);

  // Espera output parar (flush de buffers).
  Deadline := GetTickCount + 10000;
  while obs_output_active(GOutput) do
  begin
    if GetTickCount > Deadline then
    begin
      Log('Timeout esperando output parar.');
      Break;
    end;
    Sleep(100);
  end;

  FRecording := False;
  ReleaseRecordingObjects;
  Log('Gravacao finalizada: %s', [FOutputPath]);
end;

function TOBSEngine.IsRecording: Boolean;
begin
  Result := FRecording;
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

procedure TOBSEngine.Teardown;
var
  ShutdownThread: TThread;
  Wait: DWORD;
begin
  // Para output ativo, mas NAO chama ReleaseRecordingObjects.
  // obs_shutdown libera tudo internamente (sources, encoders, output).
  // Liberar manualmente antes causa "Double destroy" porque obs_shutdown
  // tenta liberar sources que ja foram destruidos.
  if FRecording then
  begin
    try obs_output_stop(GOutput); except end;
    FRecording := False;
  end;
  if FInitialized then
  begin
    // obs_shutdown pode bloquear indefinidamente se threads internas
    // de audio/render estiverem com trabalho pendente. Rodamos em
    // worker com timeout — se nao retornar em 5s, abandonamos.
    // O processo esta saindo, OS limpa o resto.
    ShutdownThread := TThread.CreateAnonymousThread(
      procedure
      begin
        try
          obs_shutdown;
        except
        end;
      end
    );
    ShutdownThread.FreeOnTerminate := False;
    ShutdownThread.Start;
    Wait := WaitForSingleObject(ShutdownThread.Handle, 5000);
    if Wait = WAIT_TIMEOUT then
      Log('libobs: obs_shutdown nao retornou em 5s — abandonando.')
    else
    begin
      ShutdownThread.Free;
      Log('libobs: shutdown ok.');
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

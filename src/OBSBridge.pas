(*
  OBSBridge — ponte entre a UI HTML (via OBSUI) e o motor de gravacao
  (via OBSEngine).

  Fluxo:
    JS  -> OBSUI.OnUIMessage -> Dispatch -> handlers -> OBSEngine
    handlers -> Build*State -> OBSUI.PostJSON -> JS

  Mensagens JS -> Delphi (campo "type"):
    ready                : pagina carregou; envia init de volta
    toggle_source        : kind=monitor|mic|speaker, id, enabled
    record_start         : —
    record_stop          : —
    rename_recording     : id (filepath), newName
    open_recording       : id (filepath)
    set_recording_fps    : fps (Integer, >= 10)
    set_language         : language ('', 'auto', 'pt-BR', 'en', ...)

  Mensagens Delphi -> JS (campo "type"):
    init                 : monitors, mics, speakers, recordings
    recording_state      : active, elapsed
    recording_added      : item
    error                : message
*)
unit OBSBridge;

interface

uses
  Winapi.Windows;

procedure Dispatch(const AJsonMsg: string);
procedure OnTimer(ATimerId: UINT_PTR);
procedure Shutdown;

implementation

uses
  Winapi.ShellAPI,
  Winapi.ShlObj,
  Winapi.ActiveX,
  System.SysUtils,
  System.StrUtils,
  System.Types,
  System.JSON,
  System.Generics.Collections,
  System.Generics.Defaults,
  System.IOUtils,
  System.Classes,
  System.NetEncoding,
  Vcl.Graphics,
  Vcl.Imaging.PngImage,
  OBSUI,
  OBSLang,
  OBSLog,
  OBSScene,
  OBSPlayer,
  OBSConfig,
  OBSAudioWatch,
  OBSProbe,
  OBSRecordWatch,
  NoOBSLockDetector,
  System.SyncObjs,
  FFmpegLib,
  FFmpegOps,
  NoOBSTypes,
  OBSEncoder,
  OBSAudioTracks,
  OBSEngine,
  OBSHotkey,
  OBSAutostart,
  OBSTray,
  OBSScrollLock,
  WinPreview,
  WinAudioMeter,
  WinWebcam;

const
  TIMER_RECORDING_TICK = 7001;
  RECORDING_TICK_MS    = 1000;

  THUMB_TICK_MS        = 500;   // 2 FPS — boa sensacao de "ao vivo"
                                //         sem custo proibitivo de BitBlt

  TIMER_AUDIO_REFRESH  = 7003;
  AUDIO_REFRESH_DEBOUNCE_MS = 800;

  TIMER_AUDIO_METER    = 7005;
  AUDIO_METER_MS       = 100;

  TIMER_MONITOR_REFRESH = 7004;
  MONITOR_REFRESH_DEBOUNCE_MS = 2500;

  TIMER_WEBCAM_REFRESH  = 7007;
  WEBCAM_REFRESH_DEBOUNCE_MS = 800;

  TIMER_OBS_WARMUP      = 7006;
  // Apos N ms sem janela visivel + sem gravacao em curso, o app se
  // re-spawna em modo /hibernate (~5MB RAM vs ~150MB). Disparado por
  // HideToTray/MinimizeToTaskbar; cancelado por qualquer interacao
  // (restore, record start). 1 minuto e suficiente pra evitar
  // re-spawn durante uso ativo via tray.
  TIMER_HIBERNATE_IDLE      = 7008;
  // Pisca o LED de Scroll Lock como indicador de gravacao em curso.
  // 1s aceso / 1s apagado = blink visivel sem ser irritante. Ativado
  // pela config 'scrollLockIndicator' (default false).
  TIMER_SCROLL_LOCK_BLINK     = 7009;
  SCROLL_LOCK_BLINK_INTERVAL_MS = 1000;
  HIBERNATE_IDLE_DELAY_MS   = 60_000;
  OBS_WARMUP_DELAY_MS   = 1500;  // tempo pra UI renderizar antes do init
  // Fallback do stop assincrono: se o sinal "stop" do output nunca
  // chegar (muxer travado, crash interno), forca a finalizacao depois
  // deste prazo pra nao deixar a gravacao "presa".
  TIMER_STOP_TIMEOUT    = 7010;
  STOP_TIMEOUT_MS       = 10_000;

  PFX_MONITOR = 'NoOBS Monitor ';
  PFX_MIC     = 'NoOBS Mic - ';
  PFX_OUT     = 'NoOBS Out - ';
  PFX_WEBCAM  = 'NoOBS Webcam - ';

  // Extensoes de video reconhecidas como gravacao. Fonte unica usada por
  // ListRecordings e pela whitelist de open_recording (pegadinha de
  // seguranca: nao deixar a UI mandar ShellExecute('open') num .exe).
  RECORDING_EXTS: array[0..6] of string = (
    '.mkv', '.mp4', '.mov', '.m4v', '.ts', '.flv', '.webm');

type
  // Trigger periodico pra captura de thumbnails. Roda em thread propria
  // porque WM_TIMER e suprimido pelo modal sizemove loop do Windows
  // (drag/resize da janela). Com thread, a captura continua tocando.
  TThumbTimerThread = class(TThread)
  private
    FIntervalMs: Cardinal;
  public
    constructor Create(AIntervalMs: Cardinal);
    procedure Execute; override;
  end;

  // Holder pra callback `of object` do TMachineLockDetector. Detector
  // exporta evento de instancia (TMachineLockEvent = procedure(...) of
  // object), nao da pra apontar pra procedure unit-level — precisa de
  // metodo de classe/instancia. Esse holder e a "ponte" minima:
  // recebe o evento na thread do detector e despacha pra main via
  // TThread.Queue.
  TLockEventHolder = class
    procedure OnLockChanged(Sender: TObject; ALocked: Boolean);
  end;

const
  // IDs dos atalhos globais (registrados em DoInit via OBSUI).
  // Faixa 100..999 reservada — caller pode usar > 1000 livre.
  HK_RECORD_TOGGLE     = 100;
  // Alias do mesmo atalho com VK_CANCEL no lugar de VK_PAUSE. Necessario
  // porque Windows mapeia Ctrl+Pause -> VK_CANCEL (a tecla Pause vira
  // "Break" quando Ctrl esta pressionado). Sem isso, Ctrl+Pause nunca
  // dispara o atalho registrado com VK_PAUSE.
  HK_RECORD_TOGGLE_ALT = 101;

var
  Engine: TOBSEngine = nil;
  Initialized: Boolean = False;
  // Setado no TOPO de DoInit (antes do init async terminar). Initialized
  // so vira True no fim — sem esta flag, um segundo 'ready' do JS (reload/
  // re-navegacao da pagina) re-entraria DoInit e re-armaria timers /
  // re-iniciaria watchers no meio do init.
  DoInitStarted: Boolean = False;

  // Flag de shutdown lido por TODAS as worker threads (capture, ffprobe,
  // transcode...). Usa Integer + TInterlocked pra garantir:
  //   1. Atomicidade (sem tearing — Boolean em x64 ja seria atomico,
  //      mas Integer + TInterlocked deixa explicito).
  //   2. Memory barrier — workers veem a mudanca IMEDIATAMENTE, nao
  //      dependem de cache flush implicito por function call.
  //   3. Compilador nao pode cachear em registrador (TInterlocked.Add
  //      com 0 e barreira de leitura forcada).
  // Acesso via IsShuttingDown / SignalShutdown abaixo.
  GShuttingDownFlag: Integer = 0;

  RecordingActive: Boolean = False;
  RecordingStartTickMs: Cardinal = 0;
  // True a partir do momento que o JS sobe e manda 'ready'. Antes
  // disso o `show_notification` pra UI nao funciona — usa fallback
  // de tray balloon (NIF_INFO). Caso classico: /start-record vindo
  // da hibernacao, gravacao comeca antes do WebView2 finalizar init.
  UIReady: Boolean = False;
  // True se a janela estava visivel quando o auto-minimize on record
  // disparou. Usado pra restaurar a janela quando a gravacao parar
  // (via hotkey ou tray menu). Caso o user tenha minimizado manual
  // antes de gravar, esse flag fica False e nao restauramos.
  WindowWasVisibleBeforeRecord: Boolean = False;
  ThumbBusy: Boolean = False;       // evita pile-up se o tick anterior atrasar
  ThumbThread: TThumbTimerThread = nil;
  // Detector de bloqueio de tela (Win+L / lock automatico). Quando o
  // config 'stopOnLock' esta ativo e o app esta gravando, a transicao
  // WTS_SESSION_LOCK dispara HandleRecordStop.
  LockDetector: TMachineLockDetector = nil;
  LockEventHolder: TLockEventHolder = nil;
  // True enquanto o modal de player de video esta aberto. UI envia o
  // estado via mensagem 'player_state'. Quando aberto, suspendemos os
  // audio meters e a captura de thumbs de monitor — a sidebar do app
  // esta escondida atras do modal, esses updates so geram reflow
  // desperdicado e roubam GPU/CPU do video player.
  PlayerOpen: Boolean = False;
  LastRecordingPath: string = '';
  LastRecordingDuration: Integer = 0;
  // Snapshot de monitores capturado no inicio da gravacao. Usado pra
  // estabilizar os slots de preview enquanto grava — sem isso, se o
  // user desplugar monitor durante gravacao, os indices da enum mudam
  // e a UI passa a renderizar conteudo do monitor errado nos slots.
  RecordingMonitorsSnapshot: TMonitorInfoArray;

  RecordDir: string = '';

// =====================================================================
// Shutdown signaling — thread-safe
// =====================================================================

function IsShuttingDown: Boolean;
begin
  // TInterlocked.Add(target, 0) le com memory barrier — garante que
  // workers vejam o valor atual mesmo se o compilador quisesse cachear
  // em registrador. Equivalente em custo a um MFENCE + load (~1ns).
  // Nao usa "inline" porque TInterlocked esta no uses do implementation
  // — Delphi nao consegue inlinar entao gera hint H2445.
  Result := TInterlocked.Add(GShuttingDownFlag, 0) <> 0;
end;

procedure SignalShutdown;
begin
  TInterlocked.Exchange(GShuttingDownFlag, 1);
end;

// =====================================================================
// JSON helpers
// =====================================================================

function GetStrField(AObj: TJSONObject; const AName: string;
  const ADefault: string = ''): string;
var V: TJSONValue;
begin
  Result := ADefault;
  if AObj = nil then Exit;
  V := AObj.GetValue(AName);
  if V <> nil then Result := V.Value;
end;

function GetIntField(AObj: TJSONObject; const AName: string;
  ADefault: Integer = 0): Integer;
var V: TJSONValue;
begin
  Result := ADefault;
  if AObj = nil then Exit;
  V := AObj.GetValue(AName);
  if V is TJSONNumber then Result := TJSONNumber(V).AsInt;
end;

function GetBoolField(AObj: TJSONObject; const AName: string;
  ADefault: Boolean = False): Boolean;
var V: TJSONValue;
begin
  Result := ADefault;
  if AObj = nil then Exit;
  V := AObj.GetValue(AName);
  if V is TJSONBool then Result := TJSONBool(V).AsBoolean;
end;

procedure PostOwned(AObj: TJSONObject);
var
  S: string;
begin
  if AObj = nil then Exit;
  try
    S := AObj.ToJSON;
  finally
    AObj.Free;
  end;
  OBSUI.PostJSON(S);
end;

procedure PostError(const AMsg: string);
var Obj: TJSONObject;
begin
  Log('ERROR: %s', [AMsg]);
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'error');
  Obj.AddPair('message', AMsg);
  PostOwned(Obj);
end;

// =====================================================================
// Build arrays (monitores, mics, speakers, recordings)
// =====================================================================

// GetSceneItemsMap, BuildMonitorInfo, GetSourceThumb, BuildSourcesArray
// removidos — dependiam de websocket (OBSClient). Preview usa Win32/WASAPI.
// =====================================================================
// Sources sem OBS — Win32/WASAPI direto pra preview phase
// =====================================================================

procedure SplitSourceId(const AId: string; out ACategory, ARawId: string);
// "NoOBS Monitor 1" -> ('monitors', '1')
// "NoOBS Mic - X"   -> ('mics', 'X')
// "NoOBS Out - Y"   -> ('speakers', 'Y')
// "NoOBS Webcam - Z" -> ('webcams', 'Z')
begin
  if StartsText(PFX_MONITOR, AId) then
  begin
    ACategory := 'monitors';
    ARawId := Copy(AId, Length(PFX_MONITOR) + 1, MaxInt);
  end
  else if StartsText(PFX_MIC, AId) then
  begin
    ACategory := 'mics';
    ARawId := Copy(AId, Length(PFX_MIC) + 1, MaxInt);
  end
  else if StartsText(PFX_OUT, AId) then
  begin
    ACategory := 'speakers';
    ARawId := Copy(AId, Length(PFX_OUT) + 1, MaxInt);
  end
  else if StartsText(PFX_WEBCAM, AId) then
  begin
    ACategory := 'webcams';
    ARawId := Copy(AId, Length(PFX_WEBCAM) + 1, MaxInt);
  end
  else
  begin
    ACategory := '';
    ARawId := AId;
  end;
end;

function GetSourceEnabled(const AId: string; ADefault: Boolean): Boolean;
var
  Cat, Raw: string;
begin
  SplitSourceId(AId, Cat, Raw);
  if Cat = '' then Exit(ADefault);
  Result := OBSConfig.GetSourceBool(Cat, Raw, ADefault);
end;

procedure SetSourceEnabled(const AId: string; AEnabled: Boolean);
var
  Cat, Raw: string;
begin
  SplitSourceId(AId, Cat, Raw);
  if Cat = '' then Exit;
  OBSConfig.SetSourceBool(Cat, Raw, AEnabled);
end;

function MonitorIdFromIndex(AIndex: Integer): string;
begin
  Result := PFX_MONITOR + IntToStr(AIndex);
end;

function MicIdFromName(const AName: string): string;
begin
  Result := PFX_MIC + AName;
end;

function OutIdFromName(const AName: string): string;
begin
  Result := PFX_OUT + AName;
end;

function BuildMonitorsFromWin: TJSONArray;
var
  Mons: TMonitorInfoArray;
  i: Integer;
  Item: TJSONObject;
  Id, Thumb: string;
begin
  Result := TJSONArray.Create;
  Mons := EnumerateMonitors;
  for i := 0 to High(Mons) do
  begin
    Id := MonitorIdFromIndex(Mons[i].Index);
    Thumb := CaptureMonitorAsDataUrl(Mons[i], 480, 270);
    Item := TJSONObject.Create;
    Item.AddPair('id', Id);
    Item.AddPair('name', Format('Monitor %d', [Mons[i].Index + 1]));
    Item.AddPair('info', Format('%dx%d @ %d,%d',
      [Mons[i].Width, Mons[i].Height, Mons[i].X, Mons[i].Y]));
    Item.AddPair('enabled', TJSONBool.Create(GetSourceEnabled(Id, True)));
    // x/y/width/height numericos pro layout visual proporcional na UI.
    Item.AddPair('x',      TJSONNumber.Create(Mons[i].X));
    Item.AddPair('y',      TJSONNumber.Create(Mons[i].Y));
    Item.AddPair('width',  TJSONNumber.Create(Mons[i].Width));
    Item.AddPair('height', TJSONNumber.Create(Mons[i].Height));
    if Thumb <> '' then
      Item.AddPair('thumb', Thumb);
    Result.AddElement(Item);
  end;
end;

function WebcamIdFromName(const AName: string): string;
begin
  Result := PFX_WEBCAM + AName;
end;

function BuildWebcamsFromWin: TJSONArray;
var
  Cams: TWebcamInfoArray;
  i: Integer;
  Item: TJSONObject;
  Id: string;
begin
  Result := TJSONArray.Create;
  Cams := EnumerateWebcams;
  for i := 0 to High(Cams) do
  begin
    Id := WebcamIdFromName(Cams[i].Name);
    Item := TJSONObject.Create;
    Item.AddPair('id',   Id);
    Item.AddPair('name', Cams[i].Name);
    Item.AddPair('info', Format('%dx%d', [Cams[i].Width, Cams[i].Height]));
    // Default DESMARCADO pra webcams (gravacao normalmente nao precisa).
    Item.AddPair('enabled', TJSONBool.Create(GetSourceEnabled(Id, False)));
    Item.AddPair('width',  TJSONNumber.Create(Cams[i].Width));
    Item.AddPair('height', TJSONNumber.Create(Cams[i].Height));
    Result.AddElement(Item);
  end;
end;

// Constroi micsJson + speakersJson com o campo `track` ja computado pela
// funcao centralizada em OBSEngine. Single source of truth: UI nao
// re-implementa o agrupamento.
// Insertion sort estavel dos 4 arrays paralelos (Idxs, Tracks,
// Enabled, Default) pela chave Track ascendente. Track=0 (device
// desabilitado, fora de qualquer faixa) e tratado como +infinito,
// indo pro fim da lista. Usado pra exibir mics/speakers na UI
// seguindo a numeracao das tracks — coerente com a legenda.
procedure SortAudioParallelByTrack(
  var AIdxs, ATracks: TArray<Integer>;
  var AEnabled, ADefault: TArray<Boolean>);
var
  i, j, n: Integer;
  tIdx, tTrack: Integer;
  tEn, tDef: Boolean;

  function Key(ATrack: Integer): Integer;
  begin
    if ATrack = 0 then Result := MaxInt else Result := ATrack;
  end;

begin
  n := Length(ATracks);
  if n < 2 then Exit;
  for i := 1 to n - 1 do
  begin
    tIdx := AIdxs[i]; tTrack := ATracks[i];
    tEn := AEnabled[i]; tDef := ADefault[i];
    j := i;
    while (j > 0) and (Key(ATracks[j-1]) > Key(tTrack)) do
    begin
      AIdxs[j]    := AIdxs[j-1];
      ATracks[j]  := ATracks[j-1];
      AEnabled[j] := AEnabled[j-1];
      ADefault[j] := ADefault[j-1];
      Dec(j);
    end;
    AIdxs[j]    := tIdx;
    ATracks[j]  := tTrack;
    AEnabled[j] := tEn;
    ADefault[j] := tDef;
  end;
end;

procedure BuildAudioJsonWithTracks(out AMicsJson, ASpkJson: TJSONArray);
var
  Devs: TAudioDeviceInfoArray;
  MicIdxs, SpkIdxs: TArray<Integer>;
  MicEnabled, MicDefault, SpkEnabled, SpkDefault: TArray<Boolean>;
  MicTracks, SpkTracks: TArray<Integer>;
  TotalTracks: Integer;
  i, k: Integer;
  Item: TJSONObject;
  Id: string;
begin
  AMicsJson := TJSONArray.Create;
  ASpkJson := TJSONArray.Create;
  InitAudio;
  Devs := EnumerateAudioDevices;

  // Separa mics e spks. Reordena cada lista pra que o default fique
  // primeiro (Windows default = top da UI). Particionamento estavel:
  // 1a passada pega so defaults; 2a passada pega o resto na ordem
  // original. Mics e speakers seguem a mesma regra (user pediu).
  for i := 0 to High(Devs) do
    if (Devs[i].Kind = adkInput) and Devs[i].IsDefault then
    begin
      SetLength(MicIdxs, Length(MicIdxs) + 1);
      MicIdxs[High(MicIdxs)] := i;
    end;
  for i := 0 to High(Devs) do
    if (Devs[i].Kind = adkInput) and (not Devs[i].IsDefault) then
    begin
      SetLength(MicIdxs, Length(MicIdxs) + 1);
      MicIdxs[High(MicIdxs)] := i;
    end;
  for i := 0 to High(Devs) do
    if (Devs[i].Kind = adkOutput) and Devs[i].IsDefault then
    begin
      SetLength(SpkIdxs, Length(SpkIdxs) + 1);
      SpkIdxs[High(SpkIdxs)] := i;
    end;
  for i := 0 to High(Devs) do
    if (Devs[i].Kind = adkOutput) and (not Devs[i].IsDefault) then
    begin
      SetLength(SpkIdxs, Length(SpkIdxs) + 1);
      SpkIdxs[High(SpkIdxs)] := i;
    end;

  SetLength(MicEnabled, Length(MicIdxs));
  SetLength(MicDefault, Length(MicIdxs));
  SetLength(SpkEnabled, Length(SpkIdxs));
  SetLength(SpkDefault, Length(SpkIdxs));
  for k := 0 to High(MicIdxs) do
  begin
    Id := MicIdFromName(Devs[MicIdxs[k]].Name);
    MicEnabled[k] := GetSourceEnabled(Id, True);
    MicDefault[k] := Devs[MicIdxs[k]].IsDefault;
  end;
  for k := 0 to High(SpkIdxs) do
  begin
    Id := OutIdFromName(Devs[SpkIdxs[k]].Name);
    SpkEnabled[k] := GetSourceEnabled(Id, True);
    SpkDefault[k] := Devs[SpkIdxs[k]].IsDefault;
  end;

  ComputeAudioTrackAssignments(MicEnabled, MicDefault, SpkEnabled, SpkDefault,
    MicTracks, SpkTracks, TotalTracks);

  // Reordena os 4 arrays paralelos (Idxs, Tracks, Enabled, Default)
  // pra que a lista exibida na UI siga a numeracao das tracks
  // ascendente. Sem isso, quando o ComputeAudioTrackAssignments faz
  // o post-processing movendo a faixa agrupada pro fim (track 6),
  // dispositivos com track menor (4, 5) apareciam DEPOIS dos
  // agrupados (6) na UI — contradizendo a legenda. Disabled (track=0)
  // vao pro fim ("inativos" agrupados na cauda da lista).
  //
  // Insertion sort estavel — arrays sao pequenos (<10 devices tipico),
  // O(n^2) e irrelevante. Stable: empates (ex: varios devices na
  // mesma track agrupada) mantem ordem de enumeracao.
  SortAudioParallelByTrack(MicIdxs, MicTracks, MicEnabled, MicDefault);
  SortAudioParallelByTrack(SpkIdxs, SpkTracks, SpkEnabled, SpkDefault);

  for k := 0 to High(MicIdxs) do
  begin
    i := MicIdxs[k];
    Id := MicIdFromName(Devs[i].Name);
    Item := TJSONObject.Create;
    Item.AddPair('id',   Id);
    Item.AddPair('name', Devs[i].Name);
    Item.AddPair('info', '');
    Item.AddPair('enabled',     TJSONBool.Create(MicEnabled[k]));
    Item.AddPair('isDefault',   TJSONBool.Create(MicDefault[k]));
    Item.AddPair('isBluetooth', TJSONBool.Create(Devs[i].IsBluetooth));
    Item.AddPair('track',       TJSONNumber.Create(MicTracks[k]));
    AMicsJson.AddElement(Item);
  end;
  for k := 0 to High(SpkIdxs) do
  begin
    i := SpkIdxs[k];
    Id := OutIdFromName(Devs[i].Name);
    Item := TJSONObject.Create;
    Item.AddPair('id',   Id);
    Item.AddPair('name', Devs[i].Name);
    Item.AddPair('info', '');
    Item.AddPair('enabled',     TJSONBool.Create(SpkEnabled[k]));
    Item.AddPair('isDefault',   TJSONBool.Create(SpkDefault[k]));
    Item.AddPair('isBluetooth', TJSONBool.Create(Devs[i].IsBluetooth));
    Item.AddPair('track',       TJSONNumber.Create(SpkTracks[k]));
    ASpkJson.AddElement(Item);
  end;
end;

function ListRecordings(const ADir: string): TStringDynArray;
// Lista todos os arquivos de video da pasta de gravacao. Cobre os
// formatos que o OBS gera (mkv, mp4, mov, ts, fragmented mp4, flv).
var
  All: TStringDynArray;
  i, n: Integer;
  Ext: string;
begin
  SetLength(Result, 0);
  if (ADir = '') or (not TDirectory.Exists(ADir)) then Exit;
  All := TDirectory.GetFiles(ADir, '*.*', TSearchOption.soTopDirectoryOnly);
  n := 0;
  SetLength(Result, Length(All));
  for i := 0 to High(All) do
  begin
    Ext := LowerCase(ExtractFileExt(All[i]));
    if MatchStr(Ext, RECORDING_EXTS) then
    begin
      Result[n] := All[i];
      Inc(n);
    end;
  end;
  SetLength(Result, n);
end;

function IsRecordingExt(const APath: string): Boolean;
// True se a extensao e de um formato de gravacao conhecido.
begin
  Result := MatchStr(LowerCase(ExtractFileExt(APath)), RECORDING_EXTS);
end;

function IsPathInRecordDir(const APath: string): Boolean;
// Defesa em profundidade: so operamos em arquivos DENTRO da pasta de
// gravacao. Toda gravacao legitima vem de ListRecordings(RecordDir), entao
// o path enviado pela UI sempre cai aqui. Bloqueia mensagens forjadas
// (ex.: via XSS ou navegacao indevida do WebView) de mirar arquivos
// arbitrarios do disco — sem isto, open_recording -> ShellExecute('open')
// executaria qualquer .exe, e delete/rename atingiriam qualquer arquivo.
var
  Base, Full: string;
begin
  Result := False;
  if (APath = '') or (RecordDir = '') then Exit;
  try
    Base := IncludeTrailingPathDelimiter(
      TPath.GetFullPath(ExcludeTrailingPathDelimiter(RecordDir)));
    Full := TPath.GetFullPath(APath);
  except
    Exit;
  end;
  // StartsText = case-insensitive (Windows). Base tem delimitador final,
  // entao "C:\Vids\" nao casa com um vizinho "C:\VidsOutro\rec.mkv".
  Result := StartsText(Base, Full);
end;

function FormatBytesShort(ABytes: Int64): string;
const
  KB = Int64(1024);
  MB = KB * 1024;
  GB = MB * 1024;
begin
  if ABytes >= GB then
    Result := Format('%.1f GB', [ABytes / GB])
  else if ABytes >= MB then
    Result := Format('%d MB', [ABytes div MB])
  else if ABytes >= KB then
    Result := Format('%d KB', [ABytes div KB])
  else
    Result := Format('%d B', [ABytes]);
end;

function BuildRecordingsArray: TJSONArray;
var
  Files: TStringDynArray;
  i: Integer;
  Item: TJSONObject;
  FilePath, FileName: string;
  FSize: Int64;
  FDate: TDateTime;
  CachedDur: Integer;
  CachedThumb: string;
begin
  Result := TJSONArray.Create;
  if (RecordDir = '') or (not TDirectory.Exists(RecordDir)) then Exit;

  Files := ListRecordings(RecordDir);

  for i := 0 to High(Files) do
  begin
    FilePath := Files[i];
    // Esconde o arquivo da gravacao em andamento — ele aparece com
    // tamanho parcial ate o muxer finalizar. PushRecordingAdded ja
    // adiciona ele na lista quando a gravacao termina.
    if RecordingActive and (LastRecordingPath <> '') and
       SameText(FilePath, LastRecordingPath) then Continue;
    FileName := ExtractFileName(FilePath);
    try
      FSize := TFile.GetSize(FilePath);
      FDate := TFile.GetLastWriteTime(FilePath);
    except
      Continue;
    end;

    Item := TJSONObject.Create;
    Item.AddPair('id',       FilePath);
    Item.AddPair('name',     ChangeFileExt(FileName, ''));
    Item.AddPair('date',     FormatDateTime('yyyy-mm-dd"T"hh:nn:ss', FDate));
    Item.AddPair('size',     TJSONNumber.Create(FSize));
    Item.AddPair('sizeText', FormatBytesShort(FSize));

    // Le metadata cacheada (instantaneo). Probe/thumb via libavformat
    // roda em background depois pra preencher os que faltam.
    CachedDur := 0;
    CachedThumb := '';
    GetCachedMeta(FilePath, CachedDur, CachedThumb);
    if CachedDur > 0 then
      Item.AddPair('duration', TJSONNumber.Create(CachedDur))
    else
      Item.AddPair('duration', '');
    if CachedThumb <> '' then
      Item.AddPair('thumb', CachedThumb);

    Result.AddElement(Item);
  end;
end;

// =====================================================================
// Push de estado
// =====================================================================

// Forward — definida mais abaixo, mas chamada por PushInit/PushRecordings.
function GetRecordDirFreeBytes: Int64; forward;

procedure PushInit(AIncludeAudio: Boolean = True);
var
  Init: TJSONObject;
  Bundle: TJSONObject;
begin
  // Se AIncludeAudio=False: pula a enumeracao WASAPI (que pode demorar
  // 30s+ em maquinas sem mic / audio service ruim). Caller deve depois
  // disparar enumeracao em worker thread e push audio_sources_refreshed.
  Init := TJSONObject.Create;
  Init.AddPair('type', 'init');
  // Bundle de traducoes — JS usa pra montar a propria T() function.
  // Caso nao tenha bundle (lang folder ausente), JS cai pros literais
  // hardcoded no HTML como fallback.
  Bundle := OBSLang.GetCurrentBundle;
  if Bundle <> nil then
    Init.AddPair('i18n', Bundle);
  Init.AddPair('language', OBSLang.CurrentLanguage);
  Init.AddPair('monitors',   BuildMonitorsFromWin);
  if AIncludeAudio then
  begin
    var MicsJ: TJSONArray; var SpksJ: TJSONArray;
    BuildAudioJsonWithTracks(MicsJ, SpksJ);
    Init.AddPair('mics',     MicsJ);
    Init.AddPair('speakers', SpksJ);
  end
  else
  begin
    Init.AddPair('mics',     TJSONArray.Create);
    Init.AddPair('speakers', TJSONArray.Create);
  end;
  Init.AddPair('webcams',    BuildWebcamsFromWin);
  Init.AddPair('recordings', BuildRecordingsArray);
  Init.AddPair('recordDir',  RecordDir);
  Init.AddPair('freeBytes',  TJSONNumber.Create(GetRecordDirFreeBytes));
  PostOwned(Init);
end;

procedure PushRecordingState;
var
  Obj: TJSONObject;
  Elapsed: Integer;
begin
  Elapsed := 0;
  if RecordingActive then
    Elapsed := Integer((GetTickCount - RecordingStartTickMs) div 1000);

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'recording_state');
  Obj.AddPair('active', TJSONBool.Create(RecordingActive));
  Obj.AddPair('elapsed', TJSONNumber.Create(Elapsed));
  PostOwned(Obj);
end;

// Forward — definida mais abaixo (depende do OBSPlayer).
procedure ScanSingleRecordingMeta(const APath: string); forward;

procedure PushRecordingAdded(const AFilePath: string; ADurationSec: Integer);
var
  Obj, Item: TJSONObject;
  FSize: Int64;
  FDate: TDateTime;
begin
  Log('PushRecordingAdded: path="%s" duration=%d', [AFilePath, ADurationSec]);
  if not TFile.Exists(AFilePath) then
  begin
    Log('PushRecordingAdded: arquivo NAO existe (cancelado).');
    Exit;
  end;

  // Sem Sleep: esta funcao agora so e chamada por OnEngineRecordingStopped,
  // disparado pelo sinal "stop" do output — que significa que o muxer ja
  // escreveu o trailer/cues e o arquivo esta COMPLETO (mesma garantia que
  // o OBS usa pra dar AutoRemux na hora). GetSize ja le o tamanho final.
  try
    FSize := TFile.GetSize(AFilePath);
    FDate := TFile.GetLastWriteTime(AFilePath);
  except
    on E: Exception do
    begin
      Log('PushRecordingAdded: erro lendo arquivo: %s', [E.Message]);
      Exit;
    end;
  end;
  Log('PushRecordingAdded: size=%d bytes', [FSize]);

  Item := TJSONObject.Create;
  Item.AddPair('id',       AFilePath);
  Item.AddPair('name',     ChangeFileExt(ExtractFileName(AFilePath), ''));
  Item.AddPair('date',     FormatDateTime('yyyy-mm-dd"T"hh:nn:ss', FDate));
  Item.AddPair('size',     TJSONNumber.Create(FSize));
  Item.AddPair('sizeText', FormatBytesShort(FSize));
  Item.AddPair('duration', TJSONNumber.Create(ADurationSec));

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'recording_added');
  Obj.AddPair('item', Item);
  // Re-envia espaco livre — gravacao nova consumiu disco, UI deve
  // atualizar o "X GB livre" + acender o icone de aviso se cruzou
  // o limite (5GB).
  Obj.AddPair('freeBytes', TJSONNumber.Create(GetRecordDirFreeBytes));
  PostOwned(Obj);

  // Gera thumb (duracao ja temos) em background — chega depois via
  // recording_meta e atualiza o card.
  ScanSingleRecordingMeta(AFilePath);
end;

// =====================================================================
// Init: garante OBS rodando, conecta, monta scene
// =====================================================================

function DetectSystemTheme: string;
// Le a preferencia "Apps mode" do Windows 10+ pra resolver o tema
// quando o config esta em modo 'system' (default na 1a execucao).
// Registry: HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\
//   Personalize\AppsUseLightTheme (DWORD)
//   0 = dark, 1 = light
// Fallback 'light' em Windows < 10, registry corrompido, etc.
const
  KEY_PATH = 'Software\Microsoft\Windows\CurrentVersion\Themes\Personalize';
  VAL_NAME = 'AppsUseLightTheme';
var
  Key: HKEY;
  DataType, DataSize, Value: DWORD;
begin
  Result := 'light';
  if RegOpenKeyExW(HKEY_CURRENT_USER, KEY_PATH, 0, KEY_QUERY_VALUE, Key)
     <> ERROR_SUCCESS then Exit;
  try
    DataSize := SizeOf(Value);
    if (RegQueryValueExW(Key, VAL_NAME, nil, @DataType, @Value, @DataSize)
        = ERROR_SUCCESS) and (DataType = REG_DWORD) then
    begin
      if Value = 0 then Result := 'dark';
    end;
  finally
    RegCloseKey(Key);
  end;
end;

procedure PushTheme;
// Valores possiveis em config.json -> "theme":
//   'system' = segue o tema do SO (resolvido em runtime pela registry)
//   'dark'   = escolha explicita do user (gravado pelo HandleSetTheme)
//   'light'  = escolha explicita do user
//
// 1a execucao: sem 'theme' no config -> grava 'system' (marcador
// "estou seguindo o Windows, nao foi escolha do user") e empurra o
// tema resolvido pra UI. Se user trocar pelo Settings, vira 'dark' ou
// 'light' explicito (HandleSetTheme).
var
  Obj: TJSONObject;
  ThemeCfg, Resolved: string;
begin
  ThemeCfg := GetConfigStr('theme', '');
  if ThemeCfg = '' then
  begin
    // 1a execucao — registra 'system' pra deixar claro no config
    // que NAO foi uma escolha do user.
    SetConfigStr('theme', 'system');
    ThemeCfg := 'system';
    Log('PushTheme: 1a execucao, marcado theme="system" no config.');
  end;

  if ThemeCfg = 'system' then
    Resolved := DetectSystemTheme
  else if (ThemeCfg = 'dark') or (ThemeCfg = 'light') then
    Resolved := ThemeCfg
  else
  begin
    // Valor invalido (edicao manual? versao futura?) — usa SO.
    Log('PushTheme: theme="%s" invalido — fallback pro SO.', [ThemeCfg]);
    Resolved := DetectSystemTheme;
  end;
  Log('PushTheme: cfg="%s" resolved="%s"', [ThemeCfg, Resolved]);

  // Aplica o tema no popup do menu de bandeja (popup preto so no dark).
  try OBSUI.ApplyTrayMenuTheme(SameText(Resolved, 'dark')); except end;

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'theme');
  Obj.AddPair('theme', Resolved);
  PostOwned(Obj);
end;

procedure HandleSetTheme(const ATheme: string);
begin
  if (ATheme <> 'dark') and (ATheme <> 'light') then Exit;
  SetConfigStr('theme', ATheme);
  // Atualiza o tema do menu de bandeja na hora (sem esperar restart).
  try OBSUI.ApplyTrayMenuTheme(SameText(ATheme, 'dark')); except end;
end;

function ConsumeFirstRunFlag: Boolean;
// Le HKCU\Software\NoOBS\FirstRun (escrito pelo instalador na 1a
// instalacao). Se = 1, retorna True e apaga o valor — assim so abre
// o modal de Configuracoes uma unica vez.
const
  REG_KEY = 'Software\NoOBS';
  REG_NAME = 'FirstRun';
var
  Key: HKEY;
  DataType, DataSize, Value: DWORD;
begin
  Result := False;
  if RegOpenKeyExW(HKEY_CURRENT_USER, REG_KEY, 0,
    KEY_QUERY_VALUE or KEY_SET_VALUE, Key) <> ERROR_SUCCESS then Exit;
  try
    DataSize := SizeOf(Value);
    if (RegQueryValueExW(Key, REG_NAME, nil, @DataType, @Value, @DataSize) =
        ERROR_SUCCESS) and (DataType = REG_DWORD) and (Value = 1) then
    begin
      Result := True;
      RegDeleteValueW(Key, REG_NAME);
    end;
  finally
    RegCloseKey(Key);
  end;
end;

procedure PushOpenSettings;
// Solicita que a UI abra o modal de Configuracoes (1a execucao apos instalacao).
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'open_settings');
  PostOwned(Obj);
end;

procedure PushAppIcon; forward;

procedure PushInitPending;
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'init_pending');
  PostOwned(Obj);
end;

procedure PushRecordingMeta(const APath: string; ADuration: Integer;
  const AThumbUrl: string);
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'recording_meta');
  Obj.AddPair('id', APath);
  Obj.AddPair('duration', TJSONNumber.Create(ADuration));
  if AThumbUrl <> '' then
    Obj.AddPair('thumb', AThumbUrl);
  PostOwned(Obj);
end;

procedure ScanSingleRecordingMeta(const APath: string);
// Worker thread pra um arquivo so. Usado quando termina uma gravacao.
var
  PathCopy: string;
begin
  if IsShuttingDown then Exit;
  if not FFmpegLibAvailable then Exit;
  PathCopy := APath;
  TThread.CreateAnonymousThread(
    procedure
    var
      Dur: Integer;
      ThumbUrl: string;
    begin
      if IsShuttingDown then Exit;
      Dur := 0;
      ThumbUrl := '';
      try
        EnsureRecordingMeta(PathCopy, Dur, ThumbUrl);
      except
        on E: Exception do
          Log('ScanSingleMeta: exception em %s: %s [%s]',
            [ExtractFileName(PathCopy), E.Message, E.ClassName]);
      end;
      if IsShuttingDown then Exit;
      if (Dur > 0) or (ThumbUrl <> '') then
        TThread.Queue(nil,
          procedure
          begin
            PushRecordingMeta(PathCopy, Dur, ThumbUrl);
          end);
    end).Start;
end;

procedure ProcessSingleMetaSync(const APath: string);
// Roda em worker thread (chamada por ScanRecordingsMeta). Cada
// chamada cria seu proprio frame, evitando captura compartilhada
// de variaveis do loop pela closure.
var
  Dur: Integer;
  ThumbUrl: string;
begin
  Dur := 0;
  ThumbUrl := '';
  try
    EnsureRecordingMeta(APath, Dur, ThumbUrl);
  except
    on E: Exception do
      Log('ProcessSingleMeta: exception em %s: %s [%s]',
        [ExtractFileName(APath), E.Message, E.ClassName]);
  end;
  if (Dur > 0) or (ThumbUrl <> '') then
    TThread.Queue(nil,
      procedure
      begin
        PushRecordingMeta(APath, Dur, ThumbUrl);
      end);
end;

procedure CleanupLegacyCache;
// Versoes antigas do NoOBS guardavam cache em <rec-dir>\.cache\.
// Agora vai pra %LOCALAPPDATA%\NoOBS\cache. Remove a pasta legada
// pra nao acumular lixo.
var
  Legacy: string;
begin
  if RecordDir = '' then Exit;
  Legacy := IncludeTrailingPathDelimiter(RecordDir) + '.cache';
  if DirectoryExists(Legacy) then
  try
    TDirectory.Delete(Legacy, True);
    Log('Cache legado removido: %s', [Legacy]);
  except
    on E: Exception do
      Log('   . falha removendo cache legado: %s', [E.Message]);
  end;
end;

procedure ScanRecordingsMeta;
// Em worker thread: pra cada gravacao, garante duracao + thumb
// (libavformat, cacheado) e empurra `recording_meta` por arquivo.
// UI atualiza os cards conforme chegam. Tambem faz GC do cache
// removendo arquivos cuja gravacao original ja nao existe.
var
  Files: TStringDynArray;
  LivePaths: TArray<string>;
  i: Integer;
begin
  if (RecordDir = '') or (not TDirectory.Exists(RecordDir)) then Exit;
  if not FFmpegLibAvailable then Exit;

  Files := ListRecordings(RecordDir);

  // Snapshot dos paths vivos pro GC.
  SetLength(LivePaths, Length(Files));
  for i := 0 to High(Files) do
    LivePaths[i] := Files[i];

  TThread.CreateAnonymousThread(
    procedure
    var
      j: Integer;
    begin
      if IsShuttingDown then Exit;
      try CleanupLegacyCache; except end;
      // GC primeiro pra liberar espaco antes de gerar caches novos.
      try GarbageCollectCache(LivePaths); except end;

      // Ordena por mtime DESC: gera thumbnails dos mais recentes
      // primeiro, que sao os que o user mais provavelmente vai
      // querer abrir/ver. Sem isso, o cache antigo eh processado
      // primeiro e os videos novos (sem thumb) ficam por ultimo.
      TArray.Sort<string>(Files, TComparer<string>.Construct(
        function(const A, B: string): Integer
        var DA, DB: TDateTime;
        begin
          DA := 0; DB := 0;
          try DA := TFile.GetLastWriteTime(A); except end;
          try DB := TFile.GetLastWriteTime(B); except end;
          if      DA > DB then Result := -1
          else if DA < DB then Result :=  1
          else                 Result :=  0;
        end));

      for j := 0 to High(Files) do
      begin
        if IsShuttingDown then Exit;
        // Pula a gravacao em andamento: muxer ainda nao escreveu o
        // trailer EBML, avformat_open_input falha e polui o log.
        // PushRecordingAdded gera a thumb assim que ela terminar.
        if RecordingActive and (LastRecordingPath <> '') and
           SameText(Files[j], LastRecordingPath) then Continue;
        ProcessSingleMetaSync(Files[j]);
      end;
    end).Start;
end;

// Forward — definida logo abaixo da implementacao da thread.
procedure PushMonitorThumbs; forward;
// Definida em ~linha 2800. Forward aqui porque DoInit (~1990) chama
// PushSettings pra que o UI tenha as configs desde o boot (sem isso,
// settings so chegavam quando o user abria o modal de Configuracoes).
procedure PushSettings; forward;

// ----------------------------------------------------------------------
// TThumbTimerThread
// ----------------------------------------------------------------------

constructor TThumbTimerThread.Create(AIntervalMs: Cardinal);
begin
  FIntervalMs := AIntervalMs;
  FreeOnTerminate := False;
  inherited Create(False);
end;

procedure TThumbTimerThread.Execute;
var
  Step, Slept: Cardinal;
  WasSuspended: Boolean;
begin
  WasSuspended := False;
  while not Terminated do
  begin
    if Terminated or IsShuttingDown then Break;

    // Captura PRIMEIRO, dorme DEPOIS — assim a 1a thumb aparece quase
    // instantaneo (em vez de esperar 500ms antes de qualquer captura).
    // Suspende enquanto o player de video esta aberto (BitBlt + JPEG
    // encode e caro e os previews estao escondidos atras do modal).
    if PlayerOpen then
    begin
      if not WasSuspended then
      begin
        Log('ThumbThread: SUSPENSO (player aberto)');
        WasSuspended := True;
      end;
    end
    else
    begin
      if WasSuspended then
      begin
        Log('ThumbThread: RETOMADO (player fechado)');
        WasSuspended := False;
      end;
      try PushMonitorThumbs; except end;
    end;

    // Sleep em pedacos de 100ms pra terminar rapido no shutdown.
    Slept := 0;
    while (Slept < FIntervalMs) and (not Terminated) do
    begin
      Step := FIntervalMs - Slept;
      if Step > 100 then Step := 100;
      Sleep(Step);
      Inc(Slept, Step);
    end;
  end;
end;

procedure PushMonitorThumbs;
// Captura screenshot de cada monitor, armazena JPEG em memoria no
// OBSPlayer e empurra URLs HTTP pro JS. Sem base64 no IPC — o
// WebView2 busca o JPEG direto pelo HTTP local.
//
// Em gravacao, itera sobre o SNAPSHOT do inicio da gravacao (slots
// fixos) e matcheia cada slot ao monitor atual por DeviceName. Se o
// monitor sumiu (foi desplugado), manda thumb vazia pra UI mostrar
// preto em vez de ficar congelada com a ultima imagem.
//
// Fora de gravacao, usa o estado atual direto (DoRefreshMonitors
// reage a WM_DISPLAYCHANGE e re-renderiza os slots).
var
  CurrentMons, WorkMons: TMonitorInfoArray;
  Recording: Boolean;
begin
  if IsShuttingDown or ThumbBusy then Exit;
  ThumbBusy := True;
  CurrentMons := EnumerateMonitors;
  Recording := RecordingActive and (Length(RecordingMonitorsSnapshot) > 0);
  if Recording then
    WorkMons := RecordingMonitorsSnapshot
  else
    WorkMons := CurrentMons;

  TThread.CreateAnonymousThread(
    procedure
    var
      i, k, MatchIdx: Integer;
      LocalArr: TArray<TPair<string, string>>;  // id, url ('' = monitor sumiu)
      Jpeg: TBytes;
      Id, Url: string;
    begin
      try
        SetLength(LocalArr, Length(WorkMons));
        for i := 0 to High(WorkMons) do
        begin
          if IsShuttingDown then
          begin
            ThumbBusy := False;
            Exit;
          end;
          Id := MonitorIdFromIndex(WorkMons[i].Index);

          // Procura o monitor ATUAL que corresponde a esse slot.
          // DeviceName (\\.\DISPLAY1 etc) e estavel enquanto o monitor
          // existe — quando o user desplugar, o nome some da enum.
          MatchIdx := -1;
          for k := 0 to High(CurrentMons) do
            if SameText(CurrentMons[k].DeviceName, WorkMons[i].DeviceName) then
            begin MatchIdx := k; Break; end;

          if MatchIdx >= 0 then
          begin
            // Monitor ainda existe — captura com coords atuais.
            Jpeg := CaptureMonitorAsJpeg(CurrentMons[MatchIdx], 480, 270);
            if Length(Jpeg) > 0 then
              SetMonitorThumb(Id, Jpeg);
            Url := GetMonitorThumbUrl(Id);
          end
          else
          begin
            // Monitor sumiu — sinaliza pra UI limpar o preview.
            // Url vazia + envio explicito do item com thumb=''.
            Url := '';
          end;
          LocalArr[i] := TPair<string, string>.Create(Id, Url);
        end;

        if IsShuttingDown then
        begin
          ThumbBusy := False;
          Exit;
        end;

        TThread.Queue(nil,
          procedure
          var
            j: Integer;
            Arr: TJSONArray;
            Item, Obj: TJSONObject;
          begin
            try
              Arr := TJSONArray.Create;
              for j := 0 to High(LocalArr) do
              begin
                // Inclui mesmo com Url vazia (sinal pro UI de "monitor
                // sumiu, limpa a imagem").
                Item := TJSONObject.Create;
                Item.AddPair('id',    LocalArr[j].Key);
                Item.AddPair('thumb', LocalArr[j].Value);
                Arr.AddElement(Item);
              end;
              if Arr.Count = 0 then
              begin
                Arr.Free;
                Exit;
              end;
              Obj := TJSONObject.Create;
              Obj.AddPair('type', 'monitor_thumbs');
              Obj.AddPair('items', Arr);
              PostOwned(Obj);
            finally
              ThumbBusy := False;
            end;
          end);
      except
        TThread.Queue(nil, procedure begin ThumbBusy := False; end);
      end;
    end).Start;
end;

function GetRecordDirFreeBytes: Int64;
// Espaco livre no volume onde RecordDir esta. -1 se nao conseguiu ler
// (caminho invalido, sem permissao, drive removivel desconectado).
// GetDiskFreeSpaceExW retorna espaco disponivel pro USUARIO atual
// (respeita cotas) — mais util que o total free.
var
  FreeAvail: TULargeInteger;
  Probe: string;
begin
  Result := -1;
  if RecordDir = '' then Exit;
  // GetDiskFreeSpaceExW aceita qualquer caminho dentro do volume —
  // usamos o proprio RecordDir. Adiciona trailing slash pra ser seguro
  // com APIs que querem path-de-diretorio. Overload com pointers:
  // passamos nil pros campos que nao usamos (total e total free).
  Probe := IncludeTrailingPathDelimiter(RecordDir);
  if GetDiskFreeSpaceExW(PWideChar(Probe), @FreeAvail, nil, nil) then
    Result := Int64(FreeAvail);
end;

procedure PushRecordings;
// Envia so a lista de gravacoes — UI ja popula o painel direito
// enquanto o OBS ainda esta subindo.
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'recordings_loaded');
  Obj.AddPair('recordings', BuildRecordingsArray);
  Obj.AddPair('recordDir',  RecordDir);
  Obj.AddPair('freeBytes',  TJSONNumber.Create(GetRecordDirFreeBytes));
  PostOwned(Obj);
end;

var
  LastMetersTickMs: Cardinal = 0;
  LastDeviceChangeTickMs: Cardinal = 0;
  PendingAudioRefresh: Boolean = False;
  PendingMonitorRefresh: Boolean = False;
  PendingWebcamRefresh: Boolean = False;
  // Re-entrance guards POR TIPO. Compartilhar uma flag global fazia
  // o monitor refresh ser silenciosamente ignorado quando o audio
  // refresh ainda estava em worker thread (audio seta a flag, monitor
  // ve True e desiste — banner do monitor nao some).
  AudioRefreshInProgress:   Boolean = False;
  MonitorRefreshInProgress: Boolean = False;
  LastMonitorCount: Integer = -1;     // pra detectar mudanca real
  MonitorRetryAttempts: Integer = 0;  // tentativas extras pos-evento

  // Signature determinista do estado de audio que a UI conhece. Usado
  // pra deduplicar refreshes — eventos de IMMNotificationClient vem em
  // rajada (mic + speaker do mesmo headset, varios papeis default,
  // OnDeviceStateChanged sem mudanca real, etc), e nao queremos mostrar
  // o banner "dispositivos alterados" se a lista visivel pela UI segue
  // identica. So atualiza quando push 'audio_sources_refreshed' vai pra
  // UI — durante gravacao continua "antiga" ate o stop aplicar o
  // refresh adiado. Vazia ate o DoInit popular.
  LastAppliedAudioSig: string = '';
  // Snapshots dos ultimos pushes — usados pra calcular o diff de
  // mudancas (adicionado / removido / padrao trocado) e incluir
  // detalhes na notificacao "Dispositivos atualizados" da UI.
  LastAppliedAudioDevs:    TAudioDeviceInfoArray;
  LastAppliedMonitorsArr:  TMonitorInfoArray;
  LastAppliedWebcamsArr:   TWebcamInfoArray;

function BuildAudioChangesArray(const AOld, ANew: TAudioDeviceInfoArray): TJSONArray;
// Compara duas snapshots de dispositivos de audio e produz uma lista de
// strings em portugues descrevendo o que mudou. Comparacao por DeviceId.
//   "Microfone adicionado: <nome>"
//   "Microfone removido: <nome>"
//   "Padrao de microfone: <nome>"     (so se ambas snapshots tem default
//                                       da mesma categoria e o id mudou)
//   ... idem pra Alto-falante
//
// Devices recem-adicionados que ja sao default geram so o "adicionado"
// (mais natural — implicito que e novo default).
var
  i: Integer;
  Found: Boolean;
  function KindWord(K: TAudioDeviceKind): string;
  begin
    if K = adkInput then Result := 'Microfone' else Result := 'Alto-falante';
  end;
  function FindByIdAndKind(const AArr: TAudioDeviceInfoArray;
    const AId: string; AKind: TAudioDeviceKind): Integer;
  var k: Integer;
  begin
    for k := 0 to High(AArr) do
      if (AArr[k].Kind = AKind) and SameText(AArr[k].DeviceId, AId) then Exit(k);
    Result := -1;
  end;
  function DefaultIdOf(const AArr: TAudioDeviceInfoArray;
    AKind: TAudioDeviceKind): string;
  var k: Integer;
  begin
    for k := 0 to High(AArr) do
      if (AArr[k].Kind = AKind) and AArr[k].IsDefault then
        Exit(AArr[k].DeviceId);
    Result := '';
  end;
  function NameOfId(const AArr: TAudioDeviceInfoArray;
    const AId: string; AKind: TAudioDeviceKind): string;
  var k: Integer;
  begin
    k := FindByIdAndKind(AArr, AId, AKind);
    if k >= 0 then Result := AArr[k].Name else Result := '';
  end;
  procedure CheckDefaultOf(AKind: TAudioDeviceKind);
  var OldId, NewId, NewName: string;
  begin
    OldId := DefaultIdOf(AOld, AKind);
    NewId := DefaultIdOf(ANew, AKind);
    if (OldId = '') or (NewId = '') then Exit;
    if SameText(OldId, NewId) then Exit;
    // Se o novo default e um device recem-adicionado, ja foi reportado
    // como "adicionado" — pula pra evitar duplicidade.
    if FindByIdAndKind(AOld, NewId, AKind) < 0 then Exit;
    NewName := NameOfId(ANew, NewId, AKind);
    Result.Add('Padrão de ' + LowerCase(KindWord(AKind)) + ': ' + NewName);
  end;
begin
  Result := TJSONArray.Create;

  // Removidos: em AOld mas nao em ANew (mesmo Kind+DeviceId).
  for i := 0 to High(AOld) do
  begin
    Found := FindByIdAndKind(ANew, AOld[i].DeviceId, AOld[i].Kind) >= 0;
    if not Found then
      Result.Add(KindWord(AOld[i].Kind) + ' removido: ' + AOld[i].Name);
  end;

  // Adicionados: em ANew mas nao em AOld.
  for i := 0 to High(ANew) do
  begin
    Found := FindByIdAndKind(AOld, ANew[i].DeviceId, ANew[i].Kind) >= 0;
    if not Found then
      Result.Add(KindWord(ANew[i].Kind) + ' adicionado: ' + ANew[i].Name);
  end;

  // Default trocado (so se o novo default ja existia em AOld — devices
  // adicionados ja foram reportados acima).
  CheckDefaultOf(adkInput);
  CheckDefaultOf(adkOutput);
end;

function BuildMonitorChangesArray(const AOld, ANew: TMonitorInfoArray): TJSONArray;
// Compara snapshots de monitores. Identificador unico = DeviceName
// (string do Windows tipo "\\.\DISPLAY1"). Nome amigavel via FriendlyName.
var
  i, j: Integer;
  Found: Boolean;
  function NameFor(const M: TMonitorInfo): string;
  begin
    if M.FriendlyName <> '' then Result := M.FriendlyName
    else Result := M.DeviceName;
  end;
begin
  Result := TJSONArray.Create;
  for i := 0 to High(AOld) do
  begin
    Found := False;
    for j := 0 to High(ANew) do
      if SameText(AOld[i].DeviceName, ANew[j].DeviceName) then
      begin Found := True; Break; end;
    if not Found then
      Result.Add('Monitor removido: ' + NameFor(AOld[i]));
  end;
  for i := 0 to High(ANew) do
  begin
    Found := False;
    for j := 0 to High(AOld) do
      if SameText(ANew[i].DeviceName, AOld[j].DeviceName) then
      begin Found := True; Break; end;
    if not Found then
      Result.Add('Monitor adicionado: ' + NameFor(ANew[i]));
  end;
end;

function BuildWebcamChangesArray(const AOld, ANew: TWebcamInfoArray): TJSONArray;
// Compara snapshots de webcams. Identificador = DeviceId (moniker).
var
  i, j: Integer;
  Found: Boolean;
begin
  Result := TJSONArray.Create;
  for i := 0 to High(AOld) do
  begin
    Found := False;
    for j := 0 to High(ANew) do
      if SameText(AOld[i].DeviceId, ANew[j].DeviceId) then
      begin Found := True; Break; end;
    if not Found then
      Result.Add('Webcam removida: ' + AOld[i].Name);
  end;
  for i := 0 to High(ANew) do
  begin
    Found := False;
    for j := 0 to High(AOld) do
      if SameText(ANew[i].DeviceId, AOld[j].DeviceId) then
      begin Found := True; Break; end;
    if not Found then
      Result.Add('Webcam adicionada: ' + ANew[i].Name);
  end;
end;

function BuildAudioSignature(const ADevs: TAudioDeviceInfoArray): string;
// Composicao determinista do estado de audio. Inclui IsDefault pra
// detectar troca de default sem mudanca de lista (user altera o
// dispositivo padrao no Windows -> IsDefault muda pra outro device).
// Ordena por (Kind, DeviceId) pra que ordem de enumeracao do WASAPI
// nao afete a comparacao.
var
  Tmp: TArray<string>;
  i: Integer;
begin
  if Length(ADevs) = 0 then Exit('');
  SetLength(Tmp, Length(ADevs));
  for i := 0 to High(ADevs) do
    Tmp[i] := Format('%d|%s|%d|%s',
      [Integer(ADevs[i].Kind), ADevs[i].DeviceId,
       Ord(ADevs[i].IsDefault), ADevs[i].Name]);
  TArray.Sort<string>(Tmp);
  Result := string.Join(';', Tmp);
end;

procedure PushRefreshBusy(ABusy: Boolean; const AWhat: string);
// Mostra/esconde overlay de loading na sidebar enquanto um refresh
// (audio ou monitores) esta acontecendo. Como a chamada websocket
// roda na main thread e pode demorar (ate 10s no pior caso), o
// overlay da feedback visual durante o congelamento.
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'refresh_busy');
  Obj.AddPair('busy', TJSONBool.Create(ABusy));
  Obj.AddPair('what', AWhat);
  PostOwned(Obj);
end;

procedure PushMonitorChanged(APending: Boolean);
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'monitor_changed');
  Obj.AddPair('pending', TJSONBool.Create(APending));
  PostOwned(Obj);
end;

procedure DoRefreshMonitors;
var
  Init: TJSONObject;
  NewCount: Integer;
  Mons: TMonitorInfoArray;
begin
  if MonitorRefreshInProgress then
  begin
    Log('DoRefreshMonitors: outro refresh de monitor em andamento, ignorando.');
    Exit;
  end;
  MonitorRefreshInProgress := True;
  PushRefreshBusy(True, 'monitors');
  try
    try
      Mons := EnumerateMonitors;
      NewCount := Length(Mons);
      Init := TJSONObject.Create;
      Init.AddPair('type', 'monitors_refreshed');
      Init.AddPair('monitors', BuildMonitorsFromWin);
      // Diff vs snapshot anterior pra UI detalhar o que mudou no toast.
      Init.AddPair('changes',
        BuildMonitorChangesArray(LastAppliedMonitorsArr, Mons));
      PostOwned(Init);
      LastAppliedMonitorsArr := Mons;
    except
      on E: Exception do
      begin
        Log('DoRefreshMonitors: falhou: %s', [E.Message]);
        NewCount := -1;
      end;
    end;

    if (NewCount >= 0) and (NewCount = LastMonitorCount) and
       (MonitorRetryAttempts < 2) then
    begin
      Inc(MonitorRetryAttempts);
      Log('MonitorRefresh: contagem inalterada (%d), retry %d em 2s',
        [NewCount, MonitorRetryAttempts]);
      if MainWindowHandle <> 0 then
        SetTimer(MainWindowHandle, TIMER_MONITOR_REFRESH, 2000, nil);
    end
    else
    begin
      LastMonitorCount := NewCount;
      MonitorRetryAttempts := 0;
    end;

  finally
    MonitorRefreshInProgress := False;
    PushRefreshBusy(False, 'monitors');
  end;
end;

procedure OnDisplayChange;
// WM_DISPLAYCHANGE chega na main thread (WindowProc). Debounce igual
// ao audio: cada chamada reagenda o timer; refresh so dispara apos
// 800ms sem mudanca.
begin
  if RecordingActive then
  begin
    if not PendingMonitorRefresh then
    begin
      PendingMonitorRefresh := True;
      PushMonitorChanged(True);
    end;
    Exit;
  end;
  if MainWindowHandle <> 0 then
  begin
    KillTimer(MainWindowHandle, TIMER_MONITOR_REFRESH);
    SetTimer(MainWindowHandle, TIMER_MONITOR_REFRESH,
      MONITOR_REFRESH_DEBOUNCE_MS, nil);
  end;
end;

procedure DoRefreshWebcams;
// Re-enumera webcams via DirectShow e empurra a lista atualizada
// pra UI. Disparado por WM_DEVICECHANGE (USB plug/unplug).
var
  Init: TJSONObject;
  Cams: TWebcamInfoArray;
begin
  try
    Cams := EnumerateWebcams;
    Init := TJSONObject.Create;
    Init.AddPair('type', 'webcams_refreshed');
    Init.AddPair('webcams', BuildWebcamsFromWin);
    // Diff vs snapshot anterior — UI usa pra mostrar "Webcam adicionada:
    // <nome>" / "Webcam removida: <nome>" no toast.
    Init.AddPair('changes',
      BuildWebcamChangesArray(LastAppliedWebcamsArr, Cams));
    PostOwned(Init);
    LastAppliedWebcamsArr := Cams;
    Log('DoRefreshWebcams: pushed.');
  except
    on E: Exception do
      Log('DoRefreshWebcams falhou: %s', [E.Message]);
  end;
end;

procedure OnDeviceNodeChange;
// WM_DEVICECHANGE (DBT_DEVNODES_CHANGED) chega na main thread.
// Dispara pra qualquer mudanca de hardware — USB plug/unplug, etc.
// Usamos pra detectar webcam (DirectShow nao tem callback nativo).
// Debounce 800ms agrega rajadas de eventos do mesmo plug/unplug.
// Nome diferente de OnDeviceChange pra evitar colisao com o callback
// do OBSAudioWatch (que recebe parametros).
begin
  Log('WM_DEVICECHANGE recebido (DBT_DEVNODES_CHANGED).');
  if RecordingActive then
  begin
    // Durante gravacao, marca pendente e aplica apos stop. Sources de
    // monitor/webcam ja sao bloqueados pelo SetSourceEnabled enquanto
    // grava, entao nao ha como fazer refresh no meio.
    PendingWebcamRefresh := True;
    Exit;
  end;
  if MainWindowHandle <> 0 then
  begin
    KillTimer(MainWindowHandle, TIMER_WEBCAM_REFRESH);
    SetTimer(MainWindowHandle, TIMER_WEBCAM_REFRESH,
      WEBCAM_REFRESH_DEBOUNCE_MS, nil);
  end;
end;

procedure LogDeviceSnapshot(const ATag: string;
  const ADevs: TAudioDeviceInfoArray);
// Dump da lista de dispositivos enumerados pra facilitar diagnostico
// de duplicacao / sumico apos hot-plug. Lista por kind + flag default.
// Detecta nomes duplicados (caso 2 USB iguais ou WASAPI inconsistente)
// e emite warning explicito no log.
var
  i, j: Integer;
  NMics, NSpks: Integer;
  Names: TArray<string>;
  Dup: Boolean;
begin
  NMics := 0; NSpks := 0;
  for i := 0 to High(ADevs) do
    if ADevs[i].Kind = adkInput then Inc(NMics)
    else Inc(NSpks);
  Log('%s snapshot: %d mic(s), %d spk(s)', [ATag, NMics, NSpks]);
  SetLength(Names, 0);
  for i := 0 to High(ADevs) do
  begin
    if ADevs[i].Kind = adkInput then
      Log('   IN  "%s"%s', [ADevs[i].Name,
        IfThen(ADevs[i].IsDefault, ' [default]', '')])
    else
      Log('   OUT "%s"%s', [ADevs[i].Name,
        IfThen(ADevs[i].IsDefault, ' [default]', '')]);
    // Detecta nome duplicado dentro do mesmo kind.
    Dup := False;
    for j := 0 to High(Names) do
      if SameText(Names[j], Format('%d:%s', [Integer(ADevs[i].Kind), ADevs[i].Name])) then
      begin Dup := True; Break; end;
    if Dup then
      Log('   ^ AVISO: nome duplicado neste kind — UI vai mostrar 2 cards iguais.')
    else
    begin
      SetLength(Names, Length(Names) + 1);
      Names[High(Names)] := Format('%d:%s', [Integer(ADevs[i].Kind), ADevs[i].Name]);
    end;
  end;
end;

// Forward — definida mais abaixo, mas chamada de dentro da queued
// procedure de DoRefreshAudio quando o user esta gravando (sinaliza
// banner "dispositivos alterados").
procedure PushAudioDeviceChanged(APending: Boolean); forward;

procedure DoRefreshAudio;
// Re-enumera audio devices via WASAPI e empurra a lista atualizada
// pra UI. Disparado por OBSAudioWatch ao detectar hot-plug (USB
// connect/disconnect).
//
// IMPORTANTE: enumeracao WASAPI roda em worker thread porque pode
// bloquear 60s+ quando o Windows Audio Service esta doente — caso
// classico: remover o ultimo mic conectado. RefreshAudioDevices +
// EnumerateAudioDevices ficam no worker; so o BuildAudioJsonWithTracks
// (que ja le do cache) roda no UI thread via TThread.Queue.
begin
  if AudioRefreshInProgress then
  begin
    Log('DoRefreshAudio: outro refresh de audio em andamento, ignorando.');
    Exit;
  end;
  AudioRefreshInProgress := True;
  PushRefreshBusy(True, 'audio');
  Log('DoRefreshAudio: disparando worker.');

  TThread.CreateAnonymousThread(
    procedure
    var
      Init: TJSONObject;
      T0, TPhase: UInt64;
      Devs: TAudioDeviceInfoArray;
      NewSig: string;
    begin
      T0 := GetTickCount64;
      try
        try
          if IsShuttingDown then Exit;
          // CoInitializeEx no worker — IMMDeviceEnumerator precisa.
          TPhase := GetTickCount64;
          try InitAudio; except on E: Exception do
            Log('DoRefreshAudio: InitAudio falhou: %s', [E.Message]); end;
          Log('DoRefreshAudio: InitAudio em %dms.',
            [GetTickCount64 - TPhase]);
          if IsShuttingDown then Exit;

          // Invalida cache do WinAudioMeter — proxima EnumerateAudioDevices
          // re-enumera fresco (pega devices novos, dropa devices removidos).
          TPhase := GetTickCount64;
          RefreshAudioDevices;
          Log('DoRefreshAudio: RefreshAudioDevices em %dms.',
            [GetTickCount64 - TPhase]);
          if IsShuttingDown then Exit;

          // Forca enumeracao agora (no worker, nao no UI) — popula cache.
          // Esse e o ponto que costuma travar quando o audio service
          // do Windows esta doente (ex.: removeu o ultimo mic).
          TPhase := GetTickCount64;
          Devs := EnumerateAudioDevices;
          Log('DoRefreshAudio: EnumerateAudioDevices em %dms (%d device(s)).',
            [GetTickCount64 - TPhase, Length(Devs)]);
          LogDeviceSnapshot('DoRefreshAudio', Devs);
          if IsShuttingDown then Exit;

          NewSig := BuildAudioSignature(Devs);

          TThread.Queue(nil,
            procedure
            var
              MicsJ, SpksJ: TJSONArray;
            begin
              if IsShuttingDown then Exit;
              try
                // Dedup: se a lista (incluindo defaults) e identica ao que
                // a UI ja conhece, ignora — o evento que disparou esse
                // refresh nao trouxe nada relevante. Cobre os casos
                // espurios (state change interno, role secundario, etc).
                if NewSig = LastAppliedAudioSig then
                begin
                  Log('DoRefreshAudio: signature inalterada, sem push pra UI.');
                  // Mudanca revertida (estado atual == o que a UI ja mostra):
                  // FECHA o banner e limpa o flag. PushAudioDeviceChanged(False)
                  // e incondicional de proposito — o HandleRecordStop ja zera
                  // PendingAudioRefresh ANTES de chamar DoRefreshAudio, entao
                  // um `if PendingAudioRefresh` aqui nunca fecharia o banner no
                  // pos-stop e ele ficaria preso quando a gravacao terminava no
                  // mesmo estado de antes. Fechar banner ja escondido e no-op.
                  PendingAudioRefresh := False;
                  PushAudioDeviceChanged(False);
                  Exit;
                end;

                // Durante gravacao nao atualiza a lista de audio na UI —
                // o user ta no meio de uma gravacao, ver a lista mudando
                // confunde. So mostra o banner "dispositivos alterados,
                // refresh apos parar". Aplicacao real fica adiada pra
                // depois do stop (que chama DoRefreshAudio novamente).
                if RecordingActive then
                begin
                  if not PendingAudioRefresh then
                  begin
                    PendingAudioRefresh := True;
                    PushAudioDeviceChanged(True);
                  end;
                  Exit;
                end;

                Init := TJSONObject.Create;
                Init.AddPair('type', 'audio_sources_refreshed');
                BuildAudioJsonWithTracks(MicsJ, SpksJ);
                Init.AddPair('mics',     MicsJ);
                Init.AddPair('speakers', SpksJ);
                // Diff vs snapshot anterior — descreve em portugues o
                // que mudou (adicionado / removido / padrao trocado).
                // UI usa pra montar o body do toast.
                Init.AddPair('changes',
                  BuildAudioChangesArray(LastAppliedAudioDevs, Devs));
                PostOwned(Init);
                LastAppliedAudioSig := NewSig;
                LastAppliedAudioDevs := Devs;
                if PendingAudioRefresh then PendingAudioRefresh := False;
              except
                on E: Exception do
                  Log('DoRefreshAudio (UI): %s', [E.Message]);
              end;
            end);
        except
          on E: Exception do
            Log('DoRefreshAudio falhou: %s', [E.Message]);
        end;
      finally
        Log('DoRefreshAudio: worker terminou em %dms.',
          [GetTickCount64 - T0]);
        TThread.Queue(nil,
          procedure
          begin
            AudioRefreshInProgress := False;
            PushRefreshBusy(False, 'audio');
          end);
      end;
    end).Start;
end;

procedure PushAudioDeviceChanged(APending: Boolean);
// pending=true: gravacao ativa, refresh adiado, mostrar banner.
// pending=false: refresh ja foi aplicado, fechar banner se aberto.
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'audio_device_changed');
  Obj.AddPair('pending', TJSONBool.Create(APending));
  PostOwned(Obj);
end;

procedure OnDeviceChange(AKind: TAudioDeviceChangeKind; const ADeviceId: string);
// Callback chamado pela thread do WASAPI. Eventos de hot-plug vem em
// rajada (mic + speaker do mesmo headset, varios papeis default, etc).
// Em vez de throttle (que perde eventos), debounce: a cada evento
// reagendamos um timer; o refresh so dispara quando para de chegar
// evento por AUDIO_REFRESH_DEBOUNCE_MS.
const
  KIND_NAMES: array[TAudioDeviceChangeKind] of string = (
    'added', 'removed', 'stateChanged', 'defaultChanged'
  );
var
  ShortId, FriendlyName: string;
begin
  ShortId := ADeviceId;
  if Length(ShortId) > 40 then ShortId := Copy(ShortId, 1, 37) + '...';
  FriendlyName := '';
  try FriendlyName := ResolveDeviceName(ADeviceId); except end;
  if FriendlyName <> '' then
    Log('AudioWatch: %s "%s" (id="%s")',
      [KIND_NAMES[AKind], FriendlyName, ShortId])
  else
    Log('AudioWatch: %s id="%s"', [KIND_NAMES[AKind], ShortId]);

  // Sempre debounce — independente do kind (incluindo adcDefaultChanged,
  // disparado quando o user troca o dispositivo padrao no Painel de Som
  // do Windows) e independente de estar gravando. A decisao "banner vs
  // refresh vs nada" e tomada em DoRefreshAudio depois do worker
  // enumerar e comparar signature com LastAppliedAudioSig — assim nao
  // notificamos quando nada relevante mudou (state change interno do
  // WASAPI, property change espuria, role secundario do mesmo device,
  // etc).
  TThread.Queue(nil,
    procedure
    begin
      if MainWindowHandle <> 0 then
      begin
        KillTimer(MainWindowHandle, TIMER_AUDIO_REFRESH);
        SetTimer(MainWindowHandle, TIMER_AUDIO_REFRESH,
          AUDIO_REFRESH_DEBOUNCE_MS, nil);
      end;
    end);
end;

procedure PushAudioMetersFromWin;
// Le peak por device via WASAPI IAudioMeterInformation. Substitui o
// evento InputVolumeMeters do OBS — funciona sem OBS rodando.
var
  Levels: TAudioLevelArray;
  Devs: TAudioDeviceInfoArray;
  i, j: Integer;
  Arr: TJSONArray;
  Item, Obj: TJSONObject;
  Id, DeviceName: string;
begin
  InitAudio;
  Levels := ReadPeakLevels;
  if Length(Levels) = 0 then Exit;
  Devs := EnumerateAudioDevices;

  Arr := TJSONArray.Create;
  for i := 0 to High(Levels) do
  begin
    DeviceName := '';
    for j := 0 to High(Devs) do
      if SameText(Devs[j].DeviceId, Levels[i].DeviceId) then
      begin
        DeviceName := Devs[j].Name;
        if Devs[j].Kind = adkInput then
          Id := MicIdFromName(DeviceName)
        else
          Id := OutIdFromName(DeviceName);
        Break;
      end;
    if DeviceName = '' then Continue;
    Item := TJSONObject.Create;
    Item.AddPair('id', Id);
    // level = peak total (compatibilidade — meter atual usa esse).
    Item.AddPair('level', TJSONNumber.Create(Levels[i].PeakLevel));
    // L/R individuais — UI renderiza dois barras se canais>=2.
    Item.AddPair('left',  TJSONNumber.Create(Levels[i].PeakLeft));
    Item.AddPair('right', TJSONNumber.Create(Levels[i].PeakRight));
    Item.AddPair('channels', TJSONNumber.Create(Levels[i].Channels));
    Arr.AddElement(Item);
  end;

  if Arr.Count = 0 then
  begin
    Arr.Free;
    Exit;
  end;

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'audio_meters');
  Obj.AddPair('items', Arr);
  PostOwned(Obj);
end;

// HandleOBSEvent, WriteBootstrapBeforeLaunch removidos — sem websocket.

procedure OnRecordDirChanged;
// Callback do OBSRecordWatch quando o Windows detecta arquivo
// adicionado/excluido/renomeado na pasta de gravacoes. Re-lista e
// dispara scan de meta pra gerar thumbs dos arquivos novos.
begin
  PushRecordings;
  ScanRecordingsMeta;
end;

procedure ApplyHotkeyFromConfig;
// Le 'hotkey' do config e (re)registra. Default = "Pause" (tecla
// PAUSE/BREAK — improvavel de conflitar com outros apps).
// Config string vazia ('') = nenhum atalho global.
const
  DEFAULT_HOTKEY = 'Pause/Break';
var
  Spec: string;
  HK: THotkeySpec;
  Ok: Boolean;
  Reason: string;
begin
  // Sempre desregistra antes — handler tolera nao-registrado.
  UnregisterGlobalHotkey(HK_RECORD_TOGGLE);
  UnregisterGlobalHotkey(HK_RECORD_TOGGLE_ALT);

  // Le do config. Se a chave nunca foi setada, usa default. Se foi
  // explicitamente setada como vazia, nao registra nada.
  Spec := GetConfigStr('hotkey', DEFAULT_HOTKEY);
  if Spec.Trim = '' then
  begin
    Log('Hotkey: desativada (config vazia).');
    Exit;
  end;

  HK := ParseHotkey(Spec);
  if not HK.Valid then
  begin
    Log('Hotkey: spec invalido "%s" — ignorado.', [Spec]);
    Exit;
  end;

  // Bloqueia combinacoes reservadas pelo Windows (RegisterHotKey nunca
  // dispararia mesmo). Safety net — UI ja bloqueia antes de mandar.
  if IsReservedHotkey(HK.Modifiers, HK.Vk, Reason) then
  begin
    Log('Hotkey: "%s" e reservado pelo Windows (%s) — ignorado.',
      [Spec, Reason]);
    Exit;
  end;

  Ok := RegisterGlobalHotkey(HK_RECORD_TOGGLE, HK.Modifiers, HK.Vk);
  if Ok then
    Log('Hotkey: registrado "%s".', [Spec])
  else
    Log('Hotkey: RegisterHotKey falhou pra "%s" (outro app pode estar usando).',
      [Spec]);

  // Pegadinha do Windows: Ctrl+Pause nao gera VK_PAUSE — vira VK_CANCEL
  // ($03), a funcao "Break" da tecla. Pra atalho Ctrl+Pause funcionar
  // precisamos registrar TAMBEM com VK_CANCEL num ID alternativo.
  // OnHotkey trata os dois IDs igual.
  if (HK.Vk = VK_PAUSE) and ((HK.Modifiers and MOD_CONTROL) <> 0) then
  begin
    if RegisterGlobalHotkey(HK_RECORD_TOGGLE_ALT, HK.Modifiers, VK_CANCEL) then
      Log('Hotkey: alias Ctrl+Break (VK_CANCEL) registrado pra cobrir Ctrl+Pause.')
    else
      Log('Hotkey: alias Ctrl+Break (VK_CANCEL) falhou — Ctrl+Pause pode nao disparar.');
  end;
end;

procedure HandleValidateHotkey(const ASpec: string);
// Valida um spec de hotkey e responde pra UI via mensagem
// 'hotkey_validation_result'. Frontend usa pra mostrar erro antes
// de fechar o modal de configuracoes. Centraliza a regra (lista de
// reservados etc) no backend — UI nao precisa saber as combinacoes.
//
// Resposta JSON:
//   { type: 'hotkey_validation_result', hotkey: '...', ok: true/false, reason: '...' }
var
  HK: THotkeySpec;
  Reason, Spec: string;
  Obj: TJSONObject;
begin
  Spec := ASpec.Trim;
  Reason := '';

  if Spec <> '' then
  begin
    HK := ParseHotkey(Spec);
    if not HK.Valid then
      Reason := 'settings.hotkey.invalidSpec'
    else
      IsReservedHotkey(HK.Modifiers, HK.Vk, Reason);
  end;

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'hotkey_validation_result');
  Obj.AddPair('hotkey', Spec);
  Obj.AddPair('ok', TJSONBool.Create(Reason = ''));
  // Reason e uma CHAVE i18n (vazia = ok). Resolve pro idioma atual antes
  // de mandar pra UI, que exibe o texto direto no toast.
  if Reason <> '' then Reason := OBSLang.T(Reason);
  Obj.AddPair('reason', Reason);
  PostOwned(Obj);
end;

procedure HandleSetHotkey(const ASpec: string);
// ASpec vazio = limpa o atalho. String valida = registra novo.
// Persiste em config e aplica imediato.
var
  HK: THotkeySpec;
  Normalized: string;
  Reason: string;
begin
  Normalized := ASpec.Trim;
  if Normalized <> '' then
  begin
    HK := ParseHotkey(Normalized);
    if not HK.Valid then
    begin
      Log('HandleSetHotkey: spec invalido "%s" — ignorado.', [Normalized]);
      Exit;
    end;
    // Bloqueia combinacoes reservadas pelo Windows. UI ja deveria ter
    // bloqueado antes de chegar aqui (espelho do RESERVED_HOTKEYS em
    // index.html), mas garantimos pra edicao manual de config.json.
    if IsReservedHotkey(HK.Modifiers, HK.Vk, Reason) then
    begin
      Log('HandleSetHotkey: "%s" reservado pelo Windows (%s) — ignorado.',
        [Normalized, Reason]);
      Exit;
    end;
    // Re-formata canonicamente (Ctrl antes de Shift antes de Alt etc).
    Normalized := FormatHotkey(HK.Modifiers, HK.Vk);
  end;
  SetConfigStr('hotkey', Normalized);
  ApplyHotkeyFromConfig;
end;

procedure DoInit;
// OBS so sobe quando o usuario clica "Iniciar Gravacao". Init pega
// monitores via Win32 e audio via WASAPI — UI funciona toda via APIs
// nativas. Enumeracao WASAPI roda em worker thread porque maquinas
// sem mic / com audio driver mal podem fazer COM calls levarem 30s+.
begin
  Log('DoInit: chamado (Initialized=%s)', [BoolToStr(Initialized, True)]);
  if Initialized then
  begin
    PushInit;
    Exit;
  end;
  if DoInitStarted then
  begin
    // Segundo 'ready' enquanto o init async ainda nao terminou (reload
    // da pagina/re-navegacao). NAO re-roda backend init — so re-empurra
    // o basico pra UI repintar. Evita timers/watchers duplicados.
    Log('DoInit: ja em andamento — re-push leve, sem re-init.');
    PushTheme;
    PushInit(False);
    Exit;
  end;
  DoInitStarted := True;
  Log('DoInit: inicio');

  // Marca que a UI JS esta viva — daqui pra frente, push de
  // 'show_notification' pra UI funciona. Antes disso usa NIF_INFO
  // balloon como fallback.
  UIReady := True;

  // Carrega bundle de traducoes antes de qualquer push pra UI (PushInit
  // serializa o bundle pra JS). 1a execucao detecta locale do Windows.
  try OBSLang.InitLanguage; except on E: Exception do
    Log('OBSLang: InitLanguage falhou: %s', [E.Message]); end;

  PushTheme;
  PushAppIcon;

  try StartPlayerServer; except on E: Exception do
    Log('Player: falha ao subir servidor: %s', [E.Message]); end;
  Log('DoInit: PlayerServer ok');

  if RecordDir = '' then
  begin
    RecordDir := GetConfigStr('recordDir', '');
    if RecordDir = '' then
      RecordDir := GetEnvironmentVariable('USERPROFILE') + '\Videos';
  end;
  PushRecordings;
  ScanRecordingsMeta;
  Log('DoInit: recordings ok');

  // Push UI imediato com audio vazio (AIncludeAudio=False). A
  // enumeracao WASAPI roda em worker thread porque pode bloquear
  // 30s+ em maquinas sem mic ou com audio service em estado ruim.
  // Quando terminar, push refresh.
  PushInit(False);
  // Sem isso, currentPlaySoundOnRecord (e outras settings) ficam nos
  // defaults JS ate o user abrir o modal de configuracoes. Resultado:
  // som de inicio/fim de gravacao nao toca na primeira gravacao se
  // estiver habilitado, ate o user ter aberto Settings uma vez.
  PushSettings;
  PushRecordingState;
  // Snapshots iniciais — sem isso o 1o hot-plug de monitor/webcam
  // compararia contra lista vazia e reportaria "todos adicionados".
  try LastAppliedMonitorsArr := EnumerateMonitors; except end;
  try LastAppliedWebcamsArr  := EnumerateWebcams;  except end;
  Log('DoInit: UI pushed');

  TThread.CreateAnonymousThread(
    procedure
    var
      Init: TJSONObject;
      Devs: TAudioDeviceInfoArray;
      InitSig: string;
    begin
      if IsShuttingDown then Exit;
      try InitAudio; except end;
      if IsShuttingDown then Exit;
      try
        Devs := EnumerateAudioDevices;
        LogDeviceSnapshot('DoInit', Devs);
        InitSig := BuildAudioSignature(Devs);
      except
        on E: Exception do Log('DoInit: erro ao enumerar audio: %s', [E.Message]);
      end;
      if IsShuttingDown then Exit;
      TThread.Queue(nil, procedure
      var
        MicsJ, SpksJ: TJSONArray;
      begin
        if IsShuttingDown then Exit;
        try
          Init := TJSONObject.Create;
          Init.AddPair('type', 'audio_sources_refreshed');
          // silent=true: nao mostra toast "Dispositivos atualizados"
          // (esse e o load inicial, nao foi um hot-plug do usuario).
          Init.AddPair('silent', TJSONBool.Create(True));
          BuildAudioJsonWithTracks(MicsJ, SpksJ);
          Init.AddPair('mics',     MicsJ);
          Init.AddPair('speakers', SpksJ);
          PostOwned(Init);
          // Captura signature + lista inicial pra dedup e diff de
          // eventos futuros. Sem isso, o 1o hot-plug compararia contra
          // estado vazio e reportaria "todos os dispositivos atuais
          // foram adicionados".
          LastAppliedAudioSig  := InitSig;
          LastAppliedAudioDevs := Devs;
          Log('DoInit: audio enumeration completa');
        except
          on E: Exception do
            Log('DoInit: erro ao publicar audio: %s', [E.Message]);
        end;
      end);
    end).Start;

  // Captura de thumbs em thread propria (independe do WM_TIMER que e
  // suprimido durante modal sizemove loop — drag/resize de janela).
  if ThumbThread = nil then
    ThumbThread := TThumbTimerThread.Create(THUMB_TICK_MS);

  // Audio meters continuam via WM_TIMER (ok pausar durante drag — UI
  // de meters nao e foco enquanto o user move a janela).
  SetTimer(MainWindowHandle, TIMER_AUDIO_METER, AUDIO_METER_MS, nil);

  // Hot-plug de audio (continua funcionando sem OBS).
  try OBSAudioWatch.Start(OnDeviceChange); except end;

  // Detector de bloqueio de tela. Sempre ativo (custo desprezivel —
  // hidden message window + PeekMessage). O config 'stopOnLock' e
  // consultado dentro do callback, nao na construcao — assim o user
  // pode ligar/desligar a feature sem reiniciar o app.
  if LockEventHolder = nil then
    LockEventHolder := TLockEventHolder.Create;
  if LockDetector = nil then
  begin
    try
      LockDetector := TMachineLockDetector.Create;
      LockDetector.OnLockStateChanged := LockEventHolder.OnLockChanged;
      Log('LockDetector: iniciado.');
    except
      on E: Exception do
      begin
        Log('LockDetector: falha ao iniciar: %s', [E.Message]);
        if LockDetector <> nil then FreeAndNil(LockDetector);
      end;
    end;
  end;

  // Watcher da pasta de gravacoes — refresh automatico quando o user
  // adiciona/exclui arquivo via Explorer ou outro app.
  try OBSRecordWatch.Start(RecordDir, OnRecordDirChanged); except end;

  // Warmup do libobs: agenda init com delay pra que a UI renderize
  // primeiro. Sem isso, a 1a gravacao espera ~300ms enquanto obs.dll
  // carrega + plugins + D3D11 device. Com warmup, ela e instantanea.
  SetTimer(MainWindowHandle, TIMER_OBS_WARMUP, OBS_WARMUP_DELAY_MS, nil);

  // Atalho global configuravel. Default = Pause se config vazio.
  // Falha de registro nao e fatal — outro app pode ter o mesmo atalho.
  ApplyHotkeyFromConfig;

  Initialized := True;
  Log('DoInit: pronto (sem OBS — sobe na hora da gravacao).');

  // Se "Minimizar pra bandeja ao fechar" esta ativo, garante o icone
  // na bandeja mesmo com a janela aberta (sinaliza que [X] vai pra
  // bandeja). Em modo /tray o icone ja foi instalado em OBSUI.Run.
  if GetConfigBool('closeToTray', False) then
    OBSUI.EnsureTrayIcon;

  // 1a execucao apos instalacao: abre o modal de Configuracoes pra
  // o user ajustar preferencias antes de comecar a usar.
  if ConsumeFirstRunFlag then
  begin
    Log('DoInit: primeira execucao detectada — abrindo Configuracoes.');
    PushOpenSettings;
  end;
end;

// =====================================================================
// Comandos vindos do JS
// =====================================================================

procedure HandleToggleSource(const AId: string; AEnabled: Boolean);
var
  IsMonitor, IsAudio: Boolean;
  Init: TJSONObject;
  MicsJ, SpksJ: TJSONArray;
begin
  IsMonitor := StartsText(PFX_MONITOR, AId) or StartsText(PFX_WEBCAM, AId);
  IsAudio   := StartsText(PFX_MIC, AId) or StartsText(PFX_OUT, AId);

  if RecordingActive and IsMonitor then
  begin
    PostError(OBSLang.T('error.cantChangeSourcesWhileRecording'));
    Exit;
  end;

  SetSourceEnabled(AId, AEnabled);

  if RecordingActive and IsAudio and (Engine <> nil) then
    try Engine.SetSourceMuted(AId, not AEnabled); except end;

  // Audio toggle altera o agrupamento (default + total enabled mudam a
  // atribuicao de tracks). Empurra refresh silencioso pra UI atualizar
  // cores/legenda. Single source of truth: o calculo so existe aqui.
  if IsAudio then
  begin
    try
      Init := TJSONObject.Create;
      Init.AddPair('type', 'audio_sources_refreshed');
      Init.AddPair('silent', TJSONBool.Create(True));
      BuildAudioJsonWithTracks(MicsJ, SpksJ);
      Init.AddPair('mics',     MicsJ);
      Init.AddPair('speakers', SpksJ);
      PostOwned(Init);
    except
      on E: Exception do
        Log('HandleToggleSource: refresh falhou: %s', [E.Message]);
    end;
  end;
end;

// Forward — MaybeNotifyRecord e implementada junto das outras de
// settings (mais abaixo) mas e chamada pelo Handle{Start,Stop}.
procedure MaybeNotifyRecord(const ATitle, AMessage: string); forward;

procedure OnEngineRecordingStopped(const AOutputPath: string);
// Callback registrado em Engine.OnStopped. Roda na MAIN thread quando o
// output emitiu "stop" — gravacao terminou de verdade, arquivo completo.
// E aqui (nao no HandleRecordStop) que salvamos a meta e adicionamos o
// card, porque so agora o arquivo esta integro (mesma logica do
// RecordingStop do frontend do OBS). Tambem desarma o timeout.
var
  Meta: TRecordingMeta;
begin
  KillTimer(MainWindowHandle, TIMER_STOP_TIMEOUT);
  Log('OnEngineRecordingStopped: path="%s"', [AOutputPath]);
  if AOutputPath = '' then Exit;

  // Persiste layout (canvas + monitores/webcams) + duracao em <hash>.json
  // antes do PushRecordingAdded, pra o ScanSingleRecordingMeta (worker) ja
  // achar o layout pronto. CurrentLayout segue valido — ReleaseRecordingObjects
  // nao o limpa, e um novo recording fica bloqueado ate o stop concluir.
  if Engine <> nil then
  begin
    Meta := Default(TRecordingMeta);
    Meta.DurationSec := LastRecordingDuration;
    Meta.Layout := Engine.CurrentLayout;
    try
      OBSPlayer.SaveRecordingMeta(AOutputPath, Meta);
    except
      on E: Exception do
        Log('SaveRecordingMeta falhou: %s', [E.Message]);
    end;
  end;

  PushRecordingAdded(AOutputPath, LastRecordingDuration);
end;

procedure HandleRecordStart;
var
  OutputPath: string;
  T0, TStep: UInt64;
begin
  if RecordingActive then Exit;

  // Desarma idle hibernate — gravacao em curso = nao hibernar.
  if MainWindowHandle <> 0 then
    KillTimer(MainWindowHandle, TIMER_HIBERNATE_IDLE);

  // Stop anterior ainda finalizando (sinal "stop" pendente)? Conclui agora
  // — libera os objetos e adiciona o card da gravacao anterior — antes de
  // montar a nova. Evita o "Ja esta gravando" se o user clicar start logo
  // apos parar.
  if (Engine <> nil) and Engine.IsStopping then
  begin
    if MainWindowHandle <> 0 then
      KillTimer(MainWindowHandle, TIMER_STOP_TIMEOUT);
    try Engine.ForceCompleteStop; except end;
  end;

  T0 := GetTickCount64;
  Log('HandleRecordStart: inicio.');
  PushRefreshBusy(True, 'starting');
  try
    TStep := GetTickCount64;
    if Engine = nil then
    begin
      Engine := TOBSEngine.Create;
      Engine.OnStopped := OnEngineRecordingStopped;
    end;
    Engine.EnsureInitialized;
    Log('HandleRecordStart: EnsureInitialized em %dms.',
      [GetTickCount64 - TStep]);

    OutputPath := IncludeTrailingPathDelimiter(RecordDir)
      + 'NoOBS_' + FormatDateTime('yyyy-mm-dd_hh-nn-ss', Now) + '.mkv';

    TStep := GetTickCount64;
    Engine.BuildAndStartRecording(OutputPath);
    Log('HandleRecordStart: BuildAndStartRecording em %dms.',
      [GetTickCount64 - TStep]);

    RecordingActive := True;
    LastRecordingPath := OutputPath;
    LastRecordingDuration := 0;
    RecordingStartTickMs := GetTickCount;
    // Snapshot dos monitores no inicio da gravacao — usado pelo
    // PushMonitorThumbs pra manter os slots de preview fixos durante
    // a gravacao mesmo se o user desplugar/replugar monitor.
    RecordingMonitorsSnapshot := WinPreview.EnumerateMonitors;
    SetTimer(MainWindowHandle, TIMER_RECORDING_TICK, RECORDING_TICK_MS, nil);
    // Bolinha vermelha no icone da bandeja e da janela (taskbar) —
    // sinalizacao visual de "gravando" mesmo com a janela escondida.
    try OBSTray.SetTrayRecording(True); except end;
    try OBSUI.SetWindowIconRecording(True); except end;
    PushRecordingState;
    // Auto-minimizar se o user pediu — UI da lugar pra outras janelas
    // durante a gravacao. Hotkey global continua funcionando pra parar.
    // Destino depende do master 'closeToTray':
    //   closeToTray=ON  → some pra bandeja (HideToTray)
    //   closeToTray=OFF → minimiza pra taskbar (visivel na barra)
    // Lembra se a janela estava visivel pra restaurar no stop (se o
    // user ja tinha minimizado manualmente, nao queremos forcar abrir).
    WindowWasVisibleBeforeRecord := False;
    if GetConfigBool('minimizeOnRecord', False) then
    begin
      WindowWasVisibleBeforeRecord :=
        (MainWindowHandle <> 0) and IsWindowVisible(MainWindowHandle);
      if WindowWasVisibleBeforeRecord then
      begin
        if GetConfigBool('closeToTray', False) then
          OBSUI.HideToTray
        else
          OBSUI.MinimizeToTaskbar;
      end;
    end;
    // Notificacao na bandeja (so dispara se o tray esta visivel).
    MaybeNotifyRecord('NoOBS', OBSLang.T('record.started'));

    // Indicador via LED Scroll Lock — opcional, default off. Pisca a
    // 1Hz enquanto a gravacao ativa. Util quando o app esta na bandeja
    // e o user nao tem feedback visual da UI. Apagamos sempre ao parar
    // (em HandleRecordStop), independente do estado em que estava no
    // momento que ligamos.
    if GetConfigBool('scrollLockIndicator', False) then
    begin
      Log('HandleRecordStart: ativando blink do Scroll Lock.');
      OBSScrollLock.SetScrollLockState(True);  // comeca aceso
      SetTimer(MainWindowHandle, TIMER_SCROLL_LOCK_BLINK,
        SCROLL_LOCK_BLINK_INTERVAL_MS, nil);
    end;

    Log('HandleRecordStart: total %dms.', [GetTickCount64 - T0]);
  except
    on E: Exception do
    begin
      RecordingActive := False;
      // Reverte os icones caso ja tenhamos trocado pra "recording"
      // antes do erro (defensivo — no-op se nunca trocou).
      try OBSTray.SetTrayRecording(False); except end;
      try OBSUI.SetWindowIconRecording(False); except end;
      PostError(OBSLang.T('error.recordStartFailed', ['error', E.Message]));
      PushRecordingState;
      Log('HandleRecordStart: FALHOU apos %dms: %s',
        [GetTickCount64 - T0, E.Message]);
    end;
  end;
  PushRefreshBusy(False, 'starting');
end;

procedure HandleRecordStop; forward;

procedure OnHotkey(AHotkeyId: Integer);
// Callback de WM_HOTKEY chamado por OBSUI.WindowProc. Roda no UI
// thread (mensagem da janela) — pode chamar Start/Stop direto.
begin
  Log('Hotkey: id=%d disparado.', [AHotkeyId]);
  case AHotkeyId of
    HK_RECORD_TOGGLE, HK_RECORD_TOGGLE_ALT:
      if RecordingActive then HandleRecordStop
      else HandleRecordStart;
  end;
end;

procedure OnWindowHiddenForHibernate;
// Disparado por OBSUI.HideToTray ou .MinimizeToTaskbar. Arma o timer
// de idle hibernate — apos HIBERNATE_IDLE_DELAY_MS sem interacao, o
// app se re-spawna em /hibernate pra liberar recursos. Skip se:
//   - Gravando (durante gravacao janela pode estar minimizada mas
//     NAO queremos hibernar)
//   - Config 'hibernate' desativada (master switch — user prefere
//     manter full mode em segundo plano)
begin
  if RecordingActive then Exit;
  if MainWindowHandle = 0 then Exit;
  // Default False: hibernar so faz sentido com closeToTray ON (janela
  // some pra bandeja). Sem isso, fechar a janela ja encerra o app e
  // nao tem cenario pra hibernar. UI gateia o toggle (so habilita com
  // closeToTray ON), mas defensivo aqui tambem.
  if not GetConfigBool('hibernate', False) then
  begin
    Log('OnWindowHidden: hibernate desativada no config — skip.');
    Exit;
  end;
  Log('OnWindowHidden: armando timer de idle hibernate (%dms).',
    [HIBERNATE_IDLE_DELAY_MS]);
  KillTimer(MainWindowHandle, TIMER_HIBERNATE_IDLE);
  SetTimer(MainWindowHandle, TIMER_HIBERNATE_IDLE,
    HIBERNATE_IDLE_DELAY_MS, nil);
end;

procedure OnWindowRestoredForHibernate;
// Disparado por OBSUI.RestoreFromTray. Faz duas coisas:
//
//  1. Cancela o timer de idle hibernate — user voltou a interagir
//     com a janela, nao queremos respawnar como /hibernate.
//
//  2. Re-empurra estado de gravacao pra UI. Defensivo: se a gravacao
//     foi iniciada via hotkey/tray enquanto a janela estava escondida,
//     o PushRecordingState original pode nao ter "pego" no DOM:
//       - WebView2 as vezes throttle/dropa msgs com window hidden
//       - Em spawn /start-record (vindo de hibernate), o PushRecordingState
//         compete com PushInit/PushSettings/PushRefreshBusy na ordem de
//         processamento JS — race conhecido
//     Re-enviar aqui garante que ao abrir a janela o user ve o estado
//     correto (botao "Parar", borda vermelha, timer rolando). Push
//     e idempotente — se DOM ja esta certo, applyRecordingState e no-op.
begin
  if MainWindowHandle = 0 then Exit;
  KillTimer(MainWindowHandle, TIMER_HIBERNATE_IDLE);
  try PushRecordingState; except end;
end;

procedure HandleRecordStop;
var
  OutputPath: string;
  Elapsed: Integer;
begin
  if not RecordingActive then
  begin
    Log('HandleRecordStop: chamado mas RecordingActive=False — no-op.');
    Exit;
  end;
  Log('HandleRecordStop: inicio.');

  // Sinaliza pra UI tocar o som de parada AGORA — Engine.StopRecording
  // pode demorar centenas de ms flushing buffers do MKV, e o user nao
  // quer ouvir o "ding" so depois disso (parece travado). UI debouncea
  // pra nao tocar duas vezes se o stop veio do click no botao (que ja
  // toca preemptivamente).
  if GetConfigBool('playSoundOnRecord', False) then
  begin
    var SndObj := TJSONObject.Create;
    SndObj.AddPair('type', 'recording_stopping');
    PostOwned(SndObj);
  end;

  KillTimer(MainWindowHandle, TIMER_RECORDING_TICK);

  // Para o blink do Scroll Lock e garante LED apagado, independente
  // do estado em que estava nesse instante do ciclo de piscar.
  KillTimer(MainWindowHandle, TIMER_SCROLL_LOCK_BLINK);
  if GetConfigBool('scrollLockIndicator', False) then
    OBSScrollLock.SetScrollLockState(False);
  Elapsed := Integer((GetTickCount - RecordingStartTickMs) div 1000);

  // Caminho do arquivo ja e conhecido (geramos no start). O arquivo so
  // estara COMPLETO quando o sinal "stop" do output disparar — por isso
  // o card e o SaveRecordingMeta acontecem no callback OnEngineRecordingStopped,
  // nao aqui (ver HandleRecordStart: Engine.OnStopped).
  OutputPath := '';
  if Engine <> nil then
    OutputPath := Engine.OutputPath;

  RecordingActive := False;
  LastRecordingPath := OutputPath;
  LastRecordingDuration := Elapsed;
  // Restaura icones (remove a bolinha vermelha).
  try OBSTray.SetTrayRecording(False); except end;
  try OBSUI.SetWindowIconRecording(False); except end;
  PushRecordingState;

  // Pede o stop de forma ASSINCRONA (igual ao OBS: obs_output_stop e
  // retorna). Quando o output terminar de verdade, o sinal "stop"
  // dispara -> OnEngineRecordingStopped faz meta + card. Sem poll/Sleep,
  // a UI nao trava. Timeout de seguranca caso o sinal nunca chegue.
  if (Engine <> nil) and Engine.IsRecording then
  begin
    try
      Engine.RequestStop;
      SetTimer(MainWindowHandle, TIMER_STOP_TIMEOUT, STOP_TIMEOUT_MS, nil);
    except
      on E: Exception do
        Log('RequestStop falhou: %s', [E.Message]);
    end;
  end;

  // Notifica fim da gravacao (so se em tray e config permite).
  MaybeNotifyRecord('NoOBS',
    OBSLang.T('record.finished',
      ['min', IntToStr(Elapsed div 60), 'sec', IntToStr(Elapsed mod 60)]));

  // Se o auto-minimize escondeu a janela na hora do start, restaura.
  // O user esperava continuar olhando o NoOBS apos parar a gravacao
  // (via hotkey ou tray menu). Caso o user ja tivesse minimizado
  // manual, esse flag esta False e a janela continua na bandeja.
  if WindowWasVisibleBeforeRecord then
  begin
    WindowWasVisibleBeforeRecord := False;
    OBSUI.RestoreFromTray;
  end
  else
  begin
    // Janela continua escondida apos gravacao — arma idle hibernate.
    // Sem isso, app fica em modo full em segundo plano consumindo RAM
    // ate o user reabrir manualmente.
    OnWindowHiddenForHibernate;
  end;

  if PendingAudioRefresh then
  begin
    PendingAudioRefresh := False;
    DoRefreshAudio;
  end;
  if PendingMonitorRefresh then
  begin
    PendingMonitorRefresh := False;
    DoRefreshMonitors;
  end;
  if PendingWebcamRefresh then
  begin
    PendingWebcamRefresh := False;
    DoRefreshWebcams;
  end;
  Log('HandleRecordStop: fim (retornando ao message loop).');
end;

procedure HandleRenameRecording(const AOldPath, ANewName: string);
var
  Dir, Ext, Sanitized, NewPath: string;
  Suffix: Integer;
const
  ILLEGAL: array[0..8] of Char = ('\', '/', ':', '*', '?', '"', '<', '>', '|');
var
  i: Integer;
  Obj: TJSONObject;
begin
  if (AOldPath = '') or (ANewName = '') then Exit;
  if not IsPathInRecordDir(AOldPath) then
  begin
    Log('HandleRenameRecording: path fora da pasta de gravacao, ignorado: %s',
      [AOldPath]);
    Exit;
  end;
  if not TFile.Exists(AOldPath) then
  begin
    PostError(OBSLang.T('error.fileNotFound'));
    Exit;
  end;

  Sanitized := Trim(ANewName);
  for i := 0 to High(ILLEGAL) do
    Sanitized := StringReplace(Sanitized, ILLEGAL[i], '_', [rfReplaceAll]);
  if Sanitized = '' then Exit;

  Dir := ExtractFilePath(AOldPath);
  Ext := ExtractFileExt(AOldPath);

  NewPath := Dir + Sanitized + Ext;
  Suffix := 2;
  while TFile.Exists(NewPath) and not SameText(NewPath, AOldPath) do
  begin
    NewPath := Dir + Sanitized + Format(' (%d)', [Suffix]) + Ext;
    Inc(Suffix);
  end;

  if SameText(NewPath, AOldPath) then Exit; // sem mudanca

  if not RenameFile(AOldPath, NewPath) then
  begin
    PostError(OBSLang.T('error.renameFailed'));
    Exit;
  end;

  // Migra o cache (thumb/duracao/mp4/json/audio tracks) do hash antigo
  // pro novo — senao ficaria orfao ate o proximo GC e a gravacao
  // renomeada regeneraria tudo do zero.
  try OBSPlayer.RenameCacheEntries(AOldPath, NewPath); except end;

  // Notifica UI da mudanca de path/nome.
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'recording_renamed');
  Obj.AddPair('oldId', AOldPath);
  Obj.AddPair('newId', NewPath);
  Obj.AddPair('newName', ChangeFileExt(ExtractFileName(NewPath), ''));
  PostOwned(Obj);
end;

procedure HandleOpenRecordDir;
begin
  if (RecordDir = '') or (not DirectoryExists(RecordDir)) then
  begin
    PostError(OBSLang.T('error.folderNotFound'));
    Exit;
  end;
  ShellExecute(0, 'open', PChar(RecordDir), nil, nil, SW_SHOWNORMAL);
end;

procedure HandleOpenRecording(const APath: string);
begin
  // So abre arquivos de gravacao DENTRO da pasta de gravacao.
  // ShellExecute('open') num path arbitrario executaria .exe/.bat/.lnk
  // com o handler default — a UI nunca manda isso, mas e defesa em
  // profundidade contra mensagem forjada (XSS/navegacao).
  if (not IsPathInRecordDir(APath)) or (not IsRecordingExt(APath)) then
  begin
    Log('HandleOpenRecording: rejeitado (fora da pasta ou ext nao-midia): %s',
      [APath]);
    Exit;
  end;
  if not TFile.Exists(APath) then
  begin
    PostError(OBSLang.T('error.fileNotFound'));
    Exit;
  end;
  ShellExecute(0, 'open', PChar(APath), nil, nil, SW_SHOW);
end;

procedure HandleOpenUrl(const AUrl: string);
// Abre URL no browser padrao via ShellExecute. So aceita http(s) pra
// evitar abuso (UI poderia mandar file:// etc).
begin
  if (AUrl = '') then Exit;
  if not (AUrl.StartsWith('http://') or AUrl.StartsWith('https://')) then
  begin
    Log('HandleOpenUrl: URL rejeitada (nao http/https): %s', [AUrl]);
    Exit;
  end;
  ShellExecute(0, 'open', PChar(AUrl), nil, nil, SW_SHOWNORMAL);
end;

procedure PushPlayPending(const APath: string);
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'play_pending');
  Obj.AddPair('id', APath);
  PostOwned(Obj);
end;

function ResolveRecordingStartClockSec(const APath: string): Integer;
// Segundos desde a meia-noite (0..86399) da hora em que a gravacao
// COMECOU. O player usa pra mostrar a hora-do-relogio correspondente a
// posicao atual (inicio + offset). Retorna -1 se nao der pra determinar.
//
// Fontes, em ordem de preferencia:
//   1. Nome NoOBS_yyyy-mm-dd_hh-nn-ss(.ext) — instante exato do start (o
//      nome e gerado com Now() em HandleRecordStart). Sobrevive copia,
//      nao sobrevive rename.
//   2. Data de CRIACAO do arquivo — o muxer cria o arquivo no inicio da
//      gravacao, entao creation time ~= start. Sobrevive rename; copia
//      entre volumes reseta. Cobre renomeados e arquivos importados.
var
  Name: string;
  Y, Mo, D, H, Mi, S: Integer;
  Hh, Mm, Ss, Ms: Word;
begin
  // 1. Parse do nome NoOBS_YYYY-MM-DD_HH-NN-SS (25 chars no minimo).
  Name := ChangeFileExt(ExtractFileName(APath), '');
  if (Length(Name) >= 25) and StartsText('NoOBS_', Name) then
    if TryStrToInt(Copy(Name, 7, 4), Y) and
       TryStrToInt(Copy(Name, 12, 2), Mo) and
       TryStrToInt(Copy(Name, 15, 2), D) and
       TryStrToInt(Copy(Name, 18, 2), H) and
       TryStrToInt(Copy(Name, 21, 2), Mi) and
       TryStrToInt(Copy(Name, 24, 2), S) and
       (Y >= 2000) and (Mo >= 1) and (Mo <= 12) and (D >= 1) and (D <= 31) and
       (H >= 0) and (H <= 23) and (Mi >= 0) and (Mi <= 59) and
       (S >= 0) and (S <= 59) then
      Exit(H * 3600 + Mi * 60 + S);

  // 2. Data de criacao do arquivo (TFile.GetCreationTime ja vem em local).
  try
    DecodeTime(TFile.GetCreationTime(APath), Hh, Mm, Ss, Ms);
    Result := Hh * 3600 + Mm * 60 + Ss;
  except
    Result := -1;
  end;
end;

procedure PushPlayUrl(const APath, AUrl, AMode: string);
// AMode: 'direct' (arquivo original) ou 'transcoded' (cache MP4 H.264).
// UI usa pra saber se pode pedir transcode em caso de falha.
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'play_url');
  Obj.AddPair('id', APath);
  Obj.AddPair('name', ChangeFileExt(ExtractFileName(APath), ''));
  Obj.AddPair('url', AUrl);
  Obj.AddPair('mode', AMode);
  // Hora-do-relogio do inicio da gravacao (segundos desde meia-noite) pra
  // o player exibir o horario real na posicao atual. -1 = desconhecido.
  Obj.AddPair('startClockSec',
    TJSONNumber.Create(ResolveRecordingStartClockSec(APath)));
  PostOwned(Obj);
end;

procedure HandlePlayRecording(const APath: string);
// Tentativa rapida: serve o arquivo original direto. Se o WebView2
// tiver codec (HEVC Extensions instaladas, ou H.264), vai tocar sem
// nenhum custo. Se falhar, JS dispara request_transcode.
var
  Url: string;
begin
  if APath = '' then Exit;
  if not IsPathInRecordDir(APath) then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError(OBSLang.T('error.fileNotFound'));
    Exit;
  end;
  Url := GetDirectUrl(APath);
  if Url = '' then
  begin
    PostError(OBSLang.T('error.prepareUrlFailed'));
    Exit;
  end;
  PushPlayUrl(APath, Url, 'direct');
end;

procedure HandleRequestTranscode(const APath: string);
// JS chama isso depois que o player falhou tocando o arquivo original.
// Faz remux via libavformat (worker thread) e devolve URL do MP4.
begin
  if APath = '' then Exit;
  if not IsPathInRecordDir(APath) then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError(OBSLang.T('error.fileNotFound'));
    Exit;
  end;
  if not FFmpegLibAvailable then
  begin
    PostError(OBSLang.T('error.mediaLibUnavailable'));
    Exit;
  end;

  PushPlayPending(APath);
  TThread.CreateAnonymousThread(
    procedure
    var
      Url, ErrMsg: string;
    begin
      if IsShuttingDown then Exit;
      Url := '';
      ErrMsg := '';
      try
        Url := GetTranscodedUrl(APath);
      except
        on E: Exception do ErrMsg := E.Message;
      end;
      if IsShuttingDown then Exit;
      TThread.Queue(nil,
        procedure
        begin
          if Url <> '' then
            PushPlayUrl(APath, Url, 'transcoded')
          else
            PostError(OBSLang.T('error.transcodeFailed') +
              IfThen(ErrMsg <> '', ' ' + ErrMsg, ''));
        end);
    end).Start;
end;

procedure HandleRequestVideoInfo(const APath: string);
// Probe via libavformat roda em worker thread (10-50ms tipicamente,
// mas pode picar em arquivos grandes/remotos). UI mostra loading.
begin
  if APath = '' then Exit;
  if not IsPathInRecordDir(APath) then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError(OBSLang.T('error.fileNotFound'));
    Exit;
  end;
  if not FFmpegLibAvailable then
  begin
    PostError(OBSLang.T('error.mediaLibUnavailable'));
    Exit;
  end;

  TThread.CreateAnonymousThread(
    procedure
    var
      Report: TProbeReport;
      Ok: Boolean;
      i: Integer;
      Obj, StreamObj: TJSONObject;
      Streams: TJSONArray;
      S: TStreamInfo;
      CachedStr: string;
      Cached: TJSONValue;
      CachedObj: TJSONObject;
      Vsz: TJSONValue;
      CurSz: Int64;
    begin
      if IsShuttingDown then Exit;

      // Cache hit? O Probe (find_stream_info) e caro em arquivos grandes —
      // o resultado fica cacheado no <hash>.json e e reusado na reabertura.
      // Valida pelo tamanho do arquivo (gravacao e imutavel, mas defensivo).
      CachedStr := OBSPlayer.LoadMetaSubObjectJson(APath, 'videoInfo');
      if CachedStr <> '' then
      begin
        Cached := TJSONObject.ParseJSONValue(CachedStr);
        if Cached is TJSONObject then
        begin
          CachedObj := TJSONObject(Cached);
          Vsz := CachedObj.GetValue('size');
          try CurSz := TFile.GetSize(APath); except CurSz := -1; end;
          if (Vsz is TJSONNumber) and (CurSz > 0) and
             (TJSONNumber(Vsz).AsInt64 = CurSz) then
          begin
            // O cache guarda id/fileName de quando foi criado. Apos um
            // rename, o conteudo (hash) e o mesmo mas o caminho mudou —
            // sobrescreve pra bater com o request atual. Sem isso, a UI ve
            // data.id != currentId e entra em LOOP re-pedindo (cache hit =
            // resposta instantanea = loop apertado, disco a 8 MB/s).
            CachedObj.RemovePair('id').Free;
            CachedObj.AddPair('id', APath);
            CachedObj.RemovePair('fileName').Free;
            CachedObj.AddPair('fileName', ExtractFileName(APath));
            TThread.Queue(nil, procedure begin PostOwned(CachedObj); end);
            Exit;  // CachedObj passa a ser do PostOwned
          end;
          CachedObj.Free;  // stale — reprobe
        end
        else if Cached <> nil then Cached.Free;
      end;

      Ok := False;
      try Ok := Probe(APath, Report); except end;
      if IsShuttingDown then Exit;
      if not Ok then
      begin
        TThread.Queue(nil, procedure begin
          PostError(OBSLang.T('error.probeFailed')); end);
        Exit;
      end;

      Obj := TJSONObject.Create;
      Obj.AddPair('type', 'video_info');
      Obj.AddPair('id', APath);
      Obj.AddPair('fileName', ExtractFileName(APath));
      Obj.AddPair('format', Report.Format);
      Obj.AddPair('duration', TJSONNumber.Create(Report.Duration));
      Obj.AddPair('bitrate', TJSONNumber.Create(Report.BitRate));
      Obj.AddPair('size', TJSONNumber.Create(Report.Size));
      Streams := TJSONArray.Create;
      for i := 0 to High(Report.Streams) do
      begin
        S := Report.Streams[i];
        StreamObj := TJSONObject.Create;
        StreamObj.AddPair('index', TJSONNumber.Create(S.Index));
        StreamObj.AddPair('kind', S.Kind);
        StreamObj.AddPair('codec', S.Codec);
        StreamObj.AddPair('title', S.Title);
        StreamObj.AddPair('bitrate', TJSONNumber.Create(S.BitRate));
        StreamObj.AddPair('duration', TJSONNumber.Create(S.Duration));
        if S.Kind = 'video' then
        begin
          StreamObj.AddPair('width', TJSONNumber.Create(S.Width));
          StreamObj.AddPair('height', TJSONNumber.Create(S.Height));
          // FPS — 0 quando desconhecido (UI esconde a linha nesse caso).
          StreamObj.AddPair('frameRate', TJSONNumber.Create(S.FrameRate));
        end
        else if S.Kind = 'audio' then
        begin
          StreamObj.AddPair('channels', TJSONNumber.Create(S.Channels));
          StreamObj.AddPair('sampleRate', TJSONNumber.Create(S.SampleRate));
        end;
        Streams.AddElement(StreamObj);
      end;
      Obj.AddPair('streams', Streams);

      // Layout (canvas + regioes de monitor/webcam) salvo em <hash>.json.
      // UI usa pra montar o seletor "Visualizacao" no painel de info do
      // player — permite zoom em um monitor especifico ao tocar.
      // Gravacoes antigas sem .json: Meta vem zerado, UI cai pro modo
      // "Tela cheia" sem seletor.
      var Meta := Default(TRecordingMeta);
      if OBSPlayer.LoadRecordingMeta(APath, Meta) and
         (Length(Meta.Layout.Regions) > 0) then
      begin
        var LayoutObj := TJSONObject.Create;
        LayoutObj.AddPair('canvasW', TJSONNumber.Create(Meta.Layout.CanvasW));
        LayoutObj.AddPair('canvasH', TJSONNumber.Create(Meta.Layout.CanvasH));
        var RegArr := TJSONArray.Create;
        for i := 0 to High(Meta.Layout.Regions) do
        begin
          var R := TJSONObject.Create;
          R.AddPair('name', Meta.Layout.Regions[i].Name);
          R.AddPair('kind', Meta.Layout.Regions[i].Kind);
          R.AddPair('x', TJSONNumber.Create(Meta.Layout.Regions[i].X));
          R.AddPair('y', TJSONNumber.Create(Meta.Layout.Regions[i].Y));
          R.AddPair('w', TJSONNumber.Create(Meta.Layout.Regions[i].W));
          R.AddPair('h', TJSONNumber.Create(Meta.Layout.Regions[i].H));
          RegArr.AddElement(R);
        end;
        LayoutObj.AddPair('regions', RegArr);
        Obj.AddPair('layout', LayoutObj);
      end;

      // Cacheia a mensagem montada (probe + layout) no <hash>.json pra
      // acelerar reaberturas. ToJSON nao consome Obj.
      OBSPlayer.SaveMetaSubObjectJson(APath, 'videoInfo', Obj.ToJSON);
      TThread.Queue(nil, procedure begin PostOwned(Obj); end);
    end).Start;
end;

procedure HandleRequestWaveform(const APath: string; ABuckets: Integer);
// Calcula peaks da 1a faixa de audio via libav (em worker thread) e
// envia pra UI como JSON. UI renderiza as barras embaixo do seek bar.
// O resultado e cacheado no <hash>.json (chave 'waveform') — decodar o
// audio inteiro custa ~500ms-2s, entao reaberturas vem do cache.
begin
  if APath = '' then Exit;
  if not IsPathInRecordDir(APath) then Exit;
  if not TFile.Exists(APath) then Exit;
  // Clamp dos dois lados: < 1 → 100 (default); teto pra uma mensagem
  // forjada nao pedir um array de bilhoes de buckets (OOM).
  if ABuckets <= 0 then ABuckets := 100
  else if ABuckets > 20000 then ABuckets := 20000;

  TThread.CreateAnonymousThread(
    procedure
    var
      Peaks: TArray<Single>;
      Ok: Boolean;
      i: Integer;
      Obj: TJSONObject;
      Arr: TJSONArray;
      CachedStr: string;
      Cached: TJSONValue;
      CachedObj: TJSONObject;
      Vb: TJSONValue;
    begin
      if IsShuttingDown then Exit;

      // Cache hit? (so reusa se o numero de buckets bate com o pedido).
      CachedStr := OBSPlayer.LoadMetaSubObjectJson(APath, 'waveform');
      if CachedStr <> '' then
      begin
        Cached := TJSONObject.ParseJSONValue(CachedStr);
        if Cached is TJSONObject then
        begin
          CachedObj := TJSONObject(Cached);
          Vb := CachedObj.GetValue('buckets');
          if (Vb is TJSONNumber) and (TJSONNumber(Vb).AsInt = ABuckets) then
          begin
            // Sobrescreve o id pro caminho atual (apos rename, o cache
            // guarda o id antigo e o JS ignoraria a resposta — waveform
            // nao renderizaria). Os peaks sao do conteudo, validos.
            CachedObj.RemovePair('id').Free;
            CachedObj.AddPair('id', APath);
            TThread.Queue(nil, procedure begin PostOwned(CachedObj); end);
            Exit;  // CachedObj passa a ser do PostOwned
          end;
          CachedObj.Free;  // contagem de buckets diferente — recomputa
        end
        else if Cached <> nil then Cached.Free;
      end;

      Ok := False;
      try Ok := FFmpegOps.ComputeAudioPeaks(APath, ABuckets, Peaks); except end;
      if IsShuttingDown then Exit;
      if not Ok then
      begin
        Log('Waveform: ComputeAudioPeaks falhou pra "%s"', [APath]);
        Exit;
      end;

      Obj := TJSONObject.Create;
      Obj.AddPair('type', 'waveform_ready');
      Obj.AddPair('id', APath);
      Obj.AddPair('buckets', TJSONNumber.Create(ABuckets)); // pra validar o cache
      Arr := TJSONArray.Create;
      for i := 0 to High(Peaks) do
        Arr.AddElement(TJSONNumber.Create(Peaks[i]));
      Obj.AddPair('peaks', Arr);

      // Cacheia no <hash>.json pra acelerar reaberturas.
      OBSPlayer.SaveMetaSubObjectJson(APath, 'waveform', Obj.ToJSON);
      TThread.Queue(nil, procedure begin PostOwned(Obj); end);
    end).Start;
end;

procedure HandleRequestAudioTracks(const APath: string);
// Extrai todas as audio tracks (uma vez, ~500ms-2s) e devolve URLs.
// JS cria audio elements sincronizados ao video element pra mixagem
// per-track em tempo real.
begin
  if APath = '' then Exit;
  if not IsPathInRecordDir(APath) then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError(OBSLang.T('error.fileNotFound'));
    Exit;
  end;

  TThread.CreateAnonymousThread(
    procedure
    var
      Urls: TArray<string>;
      Ok: Boolean;
      i: Integer;
      Obj: TJSONObject;
      Arr: TJSONArray;
    begin
      if IsShuttingDown then Exit;
      Ok := False;
      try Ok := GetAudioTrackUrls(APath, Urls); except end;
      if IsShuttingDown then Exit;
      if not Ok then
      begin
        TThread.Queue(nil, procedure begin
          PostError(OBSLang.T('error.extractAudioFailed')); end);
        Exit;
      end;

      Obj := TJSONObject.Create;
      Obj.AddPair('type', 'audio_tracks_ready');
      Obj.AddPair('id', APath);
      Arr := TJSONArray.Create;
      for i := 0 to High(Urls) do
        Arr.AddElement(TJSONString.Create(Urls[i]));
      Obj.AddPair('urls', Arr);

      TThread.Queue(nil, procedure begin PostOwned(Obj); end);
    end).Start;
end;

function DeleteToRecycleBin(const APath: string): Boolean;
// Retry com backoff curto pra cobrir o caso comum: logo apos uma
// gravacao terminar, a worker thread de thumbnail (ScanSingleRecordingMeta)
// abre o arquivo via libav com FILE_SHARE_READ — bloqueia DELETE
// ate fechar. SHFileOperation falha com sharing violation e o user
// veria "Falha ao excluir". Geracao de thumb dura ~200-500ms; 1.5s
// de retry cobre praticamente todos os casos sem virar UI travada.
const
  MAX_RETRIES = 6;
  RETRY_DELAY_MS = 250;
var
  ShOp: TSHFileOpStructW;
  Buf: array of WideChar;
  Len, Attempt: Integer;
  Rc: Integer;
begin
  // pFrom de SHFileOperation exige path duplo-NUL-terminado.
  Len := Length(APath);
  SetLength(Buf, Len + 2);
  if Len > 0 then
    Move(PWideChar(APath)^, Buf[0], Len * SizeOf(WideChar));
  Buf[Len]     := #0;
  Buf[Len + 1] := #0;

  Result := False;
  for Attempt := 0 to MAX_RETRIES - 1 do
  begin
    ZeroMemory(@ShOp, SizeOf(ShOp));
    ShOp.Wnd    := 0;
    ShOp.wFunc  := FO_DELETE;
    ShOp.pFrom  := @Buf[0];
    // ALLOWUNDO = vai pra Lixeira (recuperavel) em vez de delete permanente.
    // SILENT/NOCONFIRMATION/NOERRORUI = sem dialogos do shell.
    ShOp.fFlags := FOF_ALLOWUNDO or FOF_NOCONFIRMATION
                or FOF_SILENT or FOF_NOERRORUI;
    Rc := SHFileOperationW(ShOp);
    if Rc = 0 then
    begin
      Result := True;
      if Attempt > 0 then
        Log('DeleteToRecycleBin: ok apos %d retries (%dms).',
          [Attempt, Attempt * RETRY_DELAY_MS]);
      Exit;
    end;
    // Falhou — provavelmente sharing violation enquanto worker thread
    // ainda processa o arquivo. Tenta de novo em RETRY_DELAY_MS.
    if Attempt = 0 then
      Log('DeleteToRecycleBin: 1a tentativa falhou (rc=%d) — retrying.', [Rc]);
    Sleep(RETRY_DELAY_MS);
  end;
  Log('DeleteToRecycleBin: desistiu apos %d tentativas (rc=%d).',
    [MAX_RETRIES, Rc]);
end;

procedure PushSettings;
// Manda config atual pra UI (so o que e configuravel hoje).
var
  Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'settings');
  Obj.AddPair('recordDir', RecordDir);
  Obj.AddPair('codec', GetConfigStr('codec', 'auto'));
  Obj.AddPair('hotkey', GetConfigStr('hotkey', 'Pause/Break'));
  Obj.AddPair('autostart', TJSONBool.Create(OBSAutostart.IsAutoStartEnabled));
  Obj.AddPair('closeToTray',
    TJSONBool.Create(GetConfigBool('closeToTray', False)));
  Obj.AddPair('minimizeOnRecord',
    TJSONBool.Create(GetConfigBool('minimizeOnRecord', False)));
  Obj.AddPair('notifyOnRecord',
    TJSONBool.Create(GetConfigBool('notifyOnRecord', False)));
  Obj.AddPair('scrollLockIndicator',
    TJSONBool.Create(GetConfigBool('scrollLockIndicator', False)));
  Obj.AddPair('playSoundOnRecord',
    TJSONBool.Create(GetConfigBool('playSoundOnRecord', False)));
  Obj.AddPair('stopOnLock',
    TJSONBool.Create(GetConfigBool('stopOnLock', False)));
  Obj.AddPair('hibernate',
    TJSONBool.Create(GetConfigBool('hibernate', False)));
  Obj.AddPair('recordingQuality',
    TJSONNumber.Create(GetConfigInt('recordingQuality', 0)));
  // recordingFps: 0 = nao configurado → UI interpreta como 30 (default
  // do NoOBS — mais compacto que o 60fps do OBS Studio).
  // maxMonitorHz: taxa maxima dos monitores conectados agora — UI usa como
  // limite superior do slider de fps. Chamada rapida (Win32 apenas).
  Obj.AddPair('recordingFps',
    TJSONNumber.Create(GetConfigInt('recordingFps', 30)));
  // recordingKeyframeSec: intervalo de keyframe (1..10s, default 2). Afeta a
  // precisao da divisao de video no player (stream copy so corta em I-frame).
  Obj.AddPair('recordingKeyframeSec',
    TJSONNumber.Create(GetConfigInt('recordingKeyframeSec', 2)));
  Obj.AddPair('maxMonitorHz',
    TJSONNumber.Create(WinPreview.GetMaxMonitorRefreshRate));
  // Idioma atual ativo (codigo resolvido — 'pt-BR', 'en', etc.).
  Obj.AddPair('language', OBSLang.CurrentLanguage);
  // Valor salvo no config ('', 'auto', 'pt-BR', ...) — UI usa pra decidir
  // se o dropdown deve mostrar "Automatico (sistema)" como selecao atual
  // ou um idioma fixo.
  Obj.AddPair('languagePref', GetConfigStr('language', ''));
  Obj.AddPair('availableLanguages', OBSLang.GetAvailableLanguages);
  PostOwned(Obj);
end;

procedure HandleSetAutostart(AEnable: Boolean);
begin
  // Registro guarda apenas o flag /autostart como marcador de origem
  // — comportamento (tray vs visivel) e decidido pelo app em runtime
  // lendo 'closeToTray'. Toggle do closeToTray nao precisa reescrever
  // a entrada do Run.
  OBSAutostart.SetAutoStart(AEnable);
  Log('Autostart: %s', [BoolToStr(AEnable, True)]);
end;

procedure HandleSetCloseToTray(AEnable: Boolean);
begin
  SetConfigBool('closeToTray', AEnable);
  Log('CloseToTray: %s', [BoolToStr(AEnable, True)]);
  // Reflete na bandeja imediato: com closeToTray ON queremos icone
  // visivel mesmo com a janela aberta (sinaliza que [X] vai pra bandeja).
  // Com OFF e janela visivel, remove o icone.
  if AEnable then
    OBSUI.EnsureTrayIcon
  else if MainWindowHandle <> 0 then
  begin
    // So remove se a janela esta visivel (caso contrario o app esta
    // minimizado na bandeja e tirar o icone deixaria o user sem acesso).
    if IsWindowVisible(MainWindowHandle) then
      OBSUI.RemoveTrayIcon;
  end;
  // Autostart NAO precisa ser re-escrito — a entrada do Run guarda
  // apenas /autostart (marcador), e o comportamento e lido daqui no
  // proximo boot.
end;

procedure HandleSetMinimizeOnRecord(AEnable: Boolean);
begin
  SetConfigBool('minimizeOnRecord', AEnable);
  Log('MinimizeOnRecord: %s', [BoolToStr(AEnable, True)]);
end;

procedure HandleSetNotifyOnRecord(AEnable: Boolean);
begin
  SetConfigBool('notifyOnRecord', AEnable);
  Log('NotifyOnRecord: %s', [BoolToStr(AEnable, True)]);
end;

procedure HandleSetRecordingQuality(ALevel: Integer);
begin
  // Clampa pra range valido — UI envia -2..+2 mas defensivo contra
  // edicao manual de config.json ou mensagem malformada.
  if ALevel < -2 then ALevel := -2;
  if ALevel >  2 then ALevel :=  2;
  SetConfigInt('recordingQuality', ALevel);
  Log('RecordingQuality: %d', [ALevel]);
end;

procedure HandleSetRecordingFps(AFps: Integer);
begin
  // Minimo 10 fps. Sem maximo fixo — user pode ter monitor 360 Hz.
  if AFps < 10 then AFps := 10;
  SetConfigInt('recordingFps', AFps);
  Log('RecordingFps: %d fps', [AFps]);
end;

procedure HandleSetRecordingKeyframe(ASec: Integer);
begin
  // Clampa 1..10s — UI manda nesse range, defensivo contra config.json
  // editado a mao. Aplicado na criacao do encoder (OBSEncoder.keyint_sec).
  if ASec < 1  then ASec := 1;
  if ASec > 10 then ASec := 10;
  SetConfigInt('recordingKeyframeSec', ASec);
  Log('RecordingKeyframe: %ds', [ASec]);
end;

procedure HandleSetLanguage(const ACode: string);
// ACode aceita:
//   ''     -> 'auto' (segue locale do Windows)
//   'auto' -> idem
//   'pt-BR', 'en', 'es', ...
// Persiste a preferencia + carrega o bundle + push 'language_changed'
// pra UI repintar tudo.
var
  Normalized, Resolved: string;
  Obj: TJSONObject;
  Bundle: TJSONObject;
begin
  Normalized := Trim(ACode);
  SetConfigStr('language', Normalized);
  Log('Language: pref="%s"', [Normalized]);
  // Re-init resolve auto/manual + carrega.
  OBSLang.InitLanguage;
  Resolved := OBSLang.CurrentLanguage;

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'language_changed');
  Obj.AddPair('language', Resolved);
  Obj.AddPair('languagePref', Normalized);
  Bundle := OBSLang.GetCurrentBundle;
  if Bundle <> nil then Obj.AddPair('i18n', Bundle);
  PostOwned(Obj);
end;

procedure HandleSetHibernate(AEnable: Boolean);
begin
  SetConfigBool('hibernate', AEnable);
  Log('Hibernate: %s', [BoolToStr(AEnable, True)]);
  if not AEnable then
  begin
    // Desativando: cancela qualquer timer idle pendente — janela
    // escondida agora continua escondida sem virar hibernate.
    if MainWindowHandle <> 0 then
      KillTimer(MainWindowHandle, TIMER_HIBERNATE_IDLE);
  end
  else
  begin
    // Ativando: se a janela ja esta escondida + nao gravando, arma
    // imediato (caso contrario user teria que reabrir+esconder pra
    // disparar). Reusa OnWindowHiddenForHibernate que ja checa esses
    // estados.
    if (MainWindowHandle <> 0) and
       (not IsWindowVisible(MainWindowHandle) or
        IsIconic(MainWindowHandle)) then
      OnWindowHiddenForHibernate;
  end;
end;

procedure HandleSetScrollLockIndicator(AEnable: Boolean);
begin
  SetConfigBool('scrollLockIndicator', AEnable);
  Log('ScrollLockIndicator: %s', [BoolToStr(AEnable, True)]);
  // Se gravando agora, aplica/remove o blink imediatamente em vez
  // de esperar a proxima gravacao.
  if not RecordingActive then Exit;
  if AEnable then
  begin
    OBSScrollLock.SetScrollLockState(True);
    SetTimer(MainWindowHandle, TIMER_SCROLL_LOCK_BLINK,
      SCROLL_LOCK_BLINK_INTERVAL_MS, nil);
  end
  else
  begin
    KillTimer(MainWindowHandle, TIMER_SCROLL_LOCK_BLINK);
    OBSScrollLock.SetScrollLockState(False);
  end;
end;

procedure HandleSetPlaySoundOnRecord(AEnable: Boolean);
begin
  SetConfigBool('playSoundOnRecord', AEnable);
  Log('PlaySoundOnRecord: %s', [BoolToStr(AEnable, True)]);
end;

procedure HandleSetStopOnLock(AEnable: Boolean);
begin
  SetConfigBool('stopOnLock', AEnable);
  Log('StopOnLock: %s', [BoolToStr(AEnable, True)]);
end;

procedure TLockEventHolder.OnLockChanged(Sender: TObject; ALocked: Boolean);
// Callback do TMachineLockDetector — roda na thread INTERNA do detector
// (a que da PeekMessage da hidden message-window). Nao toca libobs aqui
// — Queue pra main pra respeitar a invariante "libobs so na main thread"
// (pegadinha #3). HandleRecordStop tambem mexe em timers WM_* e UI push,
// que tem que ser main.
begin
  Log('LockDetector: sessao %s.',
    [IfThen(ALocked, 'bloqueada', 'desbloqueada')]);
  // So agimos no LOCK. Unlock e informativo (no log).
  if not ALocked then Exit;
  // Config gate — feature e opt-in, mesmo com detector rodando.
  if not GetConfigBool('stopOnLock', False) then Exit;

  TThread.Queue(nil,
    procedure
    begin
      if IsShuttingDown then Exit;
      if not RecordingActive then Exit;
      Log('LockDetector: parando gravacao por bloqueio de tela.');
      HandleRecordStop;
    end);
end;

procedure MaybeNotifyRecord(const ATitle, AMessage: string);
// Mostra notificacao via Web Notifications API (`new Notification(...)`
// no JS da UI) se o user pediu (config notifyOnRecord = True). A UI
// se encarrega de criar o toast, dar setTimeout(close, N) pra remover
// da Central de Notificacoes, e usar tag fixa pra que notificacoes
// novas substituam as antigas em vez de empilhar.
//
// Por que WebView2 e nao WinRT direto: o Chromium ja gerencia AUMID
// e Action Center nativamente, e `notification.close()` remove da
// Central de forma deterministica — coisa que WinRT toast em Delphi
// (vtable manual, IReference<DateTime>) e fragil. Mesma abordagem
// usada pelo conversa-web/Vue rodando dentro do WebView2.
var
  Obj: TJSONObject;
begin
  if not GetConfigBool('notifyOnRecord', False) then Exit;

  // UI ainda nao subiu — caso classico do /start-record vindo da
  // hibernacao, onde a gravacao comeca antes do WebView2 terminar
  // init. Fallback: balloon de tray icon (NIF_INFO). Requer que o
  // icone de tray exista; OBSUI instala on-demand quando vai
  // hibernar/minimizar pra tray, mas no /start-record talvez nao
  // tenha. ShowBalloon e idempotente — se sem tray, e no-op.
  if not UIReady then
  begin
    OBSTray.ShowBalloon(ATitle, AMessage);
    Exit;
  end;

  Obj := TJSONObject.Create;
  Obj.AddPair('type',  'show_notification');
  Obj.AddPair('title', ATitle);
  Obj.AddPair('body',  AMessage);
  PostOwned(Obj);
end;

procedure HandleSetCodec(const ACodec: string);
// Valores aceitos: auto | av1-hw | hevc-hw | h264-hw | h264-sw.
// OBSEncoder.SelectVideoEncoder consulta config 'codec' em cada
// gravacao; mudanca aqui afeta a proxima gravacao.
const
  VALID: array[0..4] of string = ('auto', 'av1-hw', 'hevc-hw', 'h264-hw', 'h264-sw');
var
  i: Integer;
  Ok: Boolean;
begin
  Ok := False;
  for i := 0 to High(VALID) do
    if SameText(ACodec, VALID[i]) then begin Ok := True; Break; end;
  if not Ok then Exit;
  SetConfigStr('codec', ACodec);
  Log('Codec preferido alterado para: %s', [ACodec]);
end;

function PickFolder(const AInitial: string): string;
// Dialogo nativo de "selecionar pasta" via IFileOpenDialog.
var
  Dlg: IFileOpenDialog;
  Item: IShellItem;
  Opts: DWORD;
  PathPtr: PWideChar;
  InitialItem: IShellItem;
begin
  Result := '';
  if Failed(CoCreateInstance(CLSID_FileOpenDialog, nil, CLSCTX_INPROC_SERVER,
    IFileOpenDialog, Dlg)) then Exit;

  Dlg.GetOptions(Opts);
  Dlg.SetOptions(Opts or FOS_PICKFOLDERS or FOS_FORCEFILESYSTEM
    or FOS_PATHMUSTEXIST);
  Dlg.SetTitle('Selecione a pasta de saida das gravacoes');

  if (AInitial <> '') and DirectoryExists(AInitial) then
  begin
    if Succeeded(SHCreateItemFromParsingName(PChar(AInitial), nil,
      IShellItem, InitialItem)) then
      Dlg.SetFolder(InitialItem);
  end;

  if Failed(Dlg.Show(MainWindowHandle)) then Exit;
  if Failed(Dlg.GetResult(Item)) then Exit;
  if Succeeded(Item.GetDisplayName(SIGDN_FILESYSPATH, PathPtr)) then
  begin
    Result := PathPtr;
    CoTaskMemFree(PathPtr);
  end;
end;

procedure HandlePickRecordDir;
var
  Picked: string;
  Obj: TJSONObject;
begin
  Picked := PickFolder(RecordDir);
  if Picked = '' then Exit;
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'record_dir_picked');
  Obj.AddPair('path', Picked);
  PostOwned(Obj);
end;

procedure HandleSetRecordDir(const APath: string);
var
  NewPath: string;
begin
  // String vazia = restaurar pro default (USERPROFILE\Videos).
  // Usado pelo botao "Restaurar padrao" das configuracoes.
  if APath = '' then
  begin
    NewPath := GetEnvironmentVariable('USERPROFILE') + '\Videos';
    SetConfigStr('recordDir', '');
    RecordDir := NewPath;
    Log('Pasta de gravacao restaurada pro padrao: %s', [NewPath]);
    PushSettings;
    PushRecordings;
    ScanRecordingsMeta;
    try OBSRecordWatch.UpdateDir(NewPath); except end;
    Exit;
  end;
  if not DirectoryExists(APath) then
  begin
    PostError(OBSLang.T('error.folderNotFound'));
    Exit;
  end;

  SetConfigStr('recordDir', APath);
  RecordDir := APath;
  Log('Pasta de gravacao alterada para: %s', [APath]);
  PushSettings;
  PushRecordings;  // re-lista do novo dir
  ScanRecordingsMeta;
  // Re-aponta o watcher pra nova pasta.
  try OBSRecordWatch.UpdateDir(APath); except end;
end;

procedure HandleDeleteRecording(const APath: string);
// Async pra nao travar a UI: DeleteToRecycleBin pode bloquear ate ~1.5s
// se o arquivo estiver sendo lido pela worker de thumbnail (sharing
// violation com retry interno). Bulk-delete de varios arquivos
// rodando sync na main thread = UI freeze inaceitavel (10 arquivos =
// 15s). Spawn worker, faz a deleta com retry, e o push de
// 'recording_removed' (+ GC + freeBytes) volta pra main via Queue.
//
// A UI ja removeu o card otimisticamente quando o user clicou —
// nao tem nada visivel pendente esperando essa promise.
var
  PathCopy: string;
begin
  if APath = '' then Exit;
  if not IsPathInRecordDir(APath) then
  begin
    Log('HandleDeleteRecording: path fora da pasta de gravacao, ignorado: %s',
      [APath]);
    Exit;
  end;
  if not TFile.Exists(APath) then
  begin
    // Pode ser race: bulk-delete chamou pra um arquivo que outro
    // delete (ou GC ou shell) ja removeu. Trata como sucesso silencioso.
    Log('HandleDeleteRecording: arquivo ja nao existe: %s', [APath]);
    Exit;
  end;
  PathCopy := APath;

  TThread.CreateAnonymousThread(
    procedure
    var
      Ok: Boolean;
    begin
      Ok := DeleteToRecycleBin(PathCopy);
      TThread.Queue(nil,
        procedure
        var
          Obj: TJSONObject;
        begin
          if IsShuttingDown then Exit;
          if not Ok then
          begin
            PostError(OBSLang.T('error.deleteFailed',
              ['file', ExtractFileName(PathCopy)]));
            Exit;
          end;
          // Limpa cache desse arquivo agora — GC pegaria no proximo
          // start, mas sem custo fazer aqui.
          try
            GarbageCollectCache(ListRecordings(RecordDir));
          except end;

          Obj := TJSONObject.Create;
          Obj.AddPair('type', 'recording_removed');
          Obj.AddPair('id', PathCopy);
          // Delete liberou espaco — refresh do indicador na UI.
          Obj.AddPair('freeBytes', TJSONNumber.Create(GetRecordDirFreeBytes));
          PostOwned(Obj);
        end);
    end).Start;
end;

function MakeSplitPath(const AOrig: string; APart: Integer): string;
// <dir>\<base> - <part>.<ext>, com sufixo " (N)" se ja existir.
var
  Dir, Base, Ext, Cand: string;
  N: Integer;
begin
  Dir := ExtractFilePath(AOrig);
  Base := ChangeFileExt(ExtractFileName(AOrig), '');
  Ext := ExtractFileExt(AOrig);
  Cand := Dir + Format('%s - %d%s', [Base, APart, Ext]);
  N := 2;
  while TFile.Exists(Cand) do
  begin
    Cand := Dir + Format('%s - %d (%d)%s', [Base, APart, N, Ext]);
    Inc(N);
  end;
  Result := Cand;
end;

procedure PushSplitPending;
var Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'split_pending');
  PostOwned(Obj);
end;

procedure PushSplitDone(AOk: Boolean);
var Obj: TJSONObject;
begin
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'split_done');
  Obj.AddPair('ok', TJSONBool.Create(AOk));
  PostOwned(Obj);
end;

procedure HandleSplitRecording(const APath: string; APosSec: Double);
// Divide a gravacao em DUAS partes no keyframe mais proximo de APosSec
// (stream copy via FFmpegOps, sem reencode). O original vai pra lixeira
// (recuperavel). Roda em worker — split de arquivo grande leva segundos.
var
  PathCopy: string;
  PosCopy: Double;
begin
  if APath = '' then Exit;
  if not IsPathInRecordDir(APath) then
  begin
    Log('HandleSplitRecording: path fora da pasta, ignorado: %s', [APath]);
    Exit;
  end;
  if not TFile.Exists(APath) then
  begin
    PostError(OBSLang.T('error.fileNotFound'));
    Exit;
  end;
  if not FFmpegLibAvailable then
  begin
    PostError(OBSLang.T('error.mediaLibUnavailable'));
    Exit;
  end;
  if APosSec <= 0 then Exit;  // inicio do video: nada a dividir

  // Espaco em disco: as duas partes somam ~o tamanho do original (stream
  // copy). Como o original so vai pra lixeira DEPOIS (continua ocupando ate
  // esvaziar), o pico exige ~o tamanho do original livre. Checa ANTES de
  // tentar — senao o corte falharia no meio com o disco cheio, gerando
  // arquivos parciais. Folga: +5% +16MB (headers/cues das 2 partes + respiro).
  var OrigSize: Int64 := 0;
  try OrigSize := TFile.GetSize(APath); except end;
  var FreeBytes: Int64 := GetRecordDirFreeBytes;
  var Needed: Int64 := OrigSize + (OrigSize div 20) + 16 * 1024 * 1024;
  if (OrigSize > 0) and (FreeBytes >= 0) and (FreeBytes < Needed) then
  begin
    Log('HandleSplitRecording: espaco insuficiente — precisa ~%d, livre %d.',
      [Needed, FreeBytes]);
    PostError(OBSLang.T('error.splitNoSpace',
      ['needed', FormatBytesShort(Needed), 'free', FormatBytesShort(FreeBytes)]));
    Exit;
  end;

  PathCopy := APath;
  PosCopy := APosSec;
  PushSplitPending;

  TThread.CreateAnonymousThread(
    procedure
    var
      PathA, PathB: string;
      Outcome: TSplitOutcome;
      NoCutPoint: Boolean;
      SizeA, SizeB: Int64;
    begin
      if IsShuttingDown then Exit;
      PathA := MakeSplitPath(PathCopy, 1);
      PathB := MakeSplitPath(PathCopy, 2);

      Outcome := soError;
      try
        Outcome := SplitFileAtKeyframe(PathCopy, PathA, PathB, PosCopy);
      except
        on E: Exception do
          Log('HandleSplitRecording: excecao: %s', [E.Message]);
      end;

      // Mesmo com soOk, confirma que as duas partes sairam com bytes.
      if Outcome = soOk then
      begin
        SizeA := 0; SizeB := 0;
        try
          SizeA := TFile.GetSize(PathA);
          SizeB := TFile.GetSize(PathB);
        except end;
        if (SizeA <= 0) or (SizeB <= 0) then Outcome := soError;
      end;

      if Outcome <> soOk then
      begin
        // Limpa qualquer parte parcial que tenha sobrado.
        try if TFile.Exists(PathA) then TFile.Delete(PathA); except end;
        try if TFile.Exists(PathB) then TFile.Delete(PathB); except end;
        NoCutPoint := (Outcome = soNoCutPoint);
        TThread.Queue(nil,
          procedure
          begin
            if IsShuttingDown then Exit;
            PushSplitDone(False);
            // "Sem ponto de corte" (keyframe) ganha dica especifica em vez
            // da falha generica.
            if NoCutPoint then
              PostError(OBSLang.T('error.splitNoCutPoint'))
            else
              PostError(OBSLang.T('error.splitFailed'));
          end);
        Exit;
      end;

      // Preserva o layout de monitores/webcams (canvas + regioes) do original
      // nas duas partes. A divisao e stream copy, entao a disposicao no canvas
      // e IDENTICA — so o tempo muda; o seletor de monitor do player precisa
      // disso. Le ANTES de mover o original pra lixeira / rodar o GC (que apaga
      // o <hash>.json dele). DurationSec fica 0 e o ScanSingleRecordingMeta
      // (via PushRecordingAdded) calcula a duracao real de cada parte
      // PRESERVANDO este layout (EnsureRecordingMeta so sobrescreve a duracao).
      var OrigMeta: TRecordingMeta;
      if OBSPlayer.LoadRecordingMeta(PathCopy, OrigMeta) and
         (Length(OrigMeta.Layout.Regions) > 0) then
      begin
        var PartMeta: TRecordingMeta := Default(TRecordingMeta);
        PartMeta.Layout := OrigMeta.Layout;
        try OBSPlayer.SaveRecordingMeta(PathA, PartMeta); except end;
        try OBSPlayer.SaveRecordingMeta(PathB, PartMeta); except end;
      end;

      // Sucesso: original pra lixeira (recuperavel).
      DeleteToRecycleBin(PathCopy);

      TThread.Queue(nil,
        procedure
        var
          Obj: TJSONObject;
        begin
          if IsShuttingDown then Exit;
          // Remove o card do original.
          Obj := TJSONObject.Create;
          Obj.AddPair('type', 'recording_removed');
          Obj.AddPair('id', PathCopy);
          PostOwned(Obj);
          // Cache orfao do original removido aqui (GC pegaria no proximo
          // start de qualquer forma).
          try GarbageCollectCache(ListRecordings(RecordDir)); except end;
          // Adiciona as duas partes (duracao=0 → ScanSingleRecordingMeta
          // preenche thumb + duracao em background).
          PushRecordingAdded(PathA, 0);
          PushRecordingAdded(PathB, 0);
          // Fecha o player (o original nao existe mais) + toast de sucesso.
          PushSplitDone(True);
        end);
    end).Start;
end;

// =====================================================================
// Dispatch publico
// =====================================================================

procedure Dispatch(const AJsonMsg: string);
var
  Root: TJSONValue;
  Obj: TJSONObject;
  MsgType: string;
begin
  Root := TJSONObject.ParseJSONValue(AJsonMsg);
  if not (Root is TJSONObject) then
  begin
    Log('Dispatch: payload nao e JSON object — descartado.');
    if Root <> nil then Root.Free;
    Exit;
  end;
  Obj := TJSONObject(Root);
  try
   try
    MsgType := GetStrField(Obj, 'type');
    if (MsgType <> 'ui_log') and (MsgType <> 'player_state') then
      Log('Dispatch: type="%s"', [MsgType]);
    if MsgType = 'ui_log' then
      Log('UI: %s', [GetStrField(Obj, 'message')])
    else if MsgType = 'ready' then
      DoInit
    else if MsgType = 'toggle_source' then
      HandleToggleSource(GetStrField(Obj, 'id'), GetBoolField(Obj, 'enabled'))
    else if MsgType = 'record_start' then
      HandleRecordStart
    else if MsgType = 'record_stop' then
      HandleRecordStop
    else if MsgType = 'rename_recording' then
      HandleRenameRecording(GetStrField(Obj, 'id'), GetStrField(Obj, 'newName'))
    else if MsgType = 'open_recording' then
      HandleOpenRecording(GetStrField(Obj, 'id'))
    else if MsgType = 'open_url' then
      HandleOpenUrl(GetStrField(Obj, 'url'))
    else if MsgType = 'open_record_dir' then
      HandleOpenRecordDir
    else if MsgType = 'play_recording' then
      HandlePlayRecording(GetStrField(Obj, 'id'))
    else if MsgType = 'request_transcode' then
      HandleRequestTranscode(GetStrField(Obj, 'id'))
    else if MsgType = 'request_video_info' then
      HandleRequestVideoInfo(GetStrField(Obj, 'id'))
    else if MsgType = 'request_audio_tracks' then
      HandleRequestAudioTracks(GetStrField(Obj, 'id'))
    else if MsgType = 'request_waveform' then
      HandleRequestWaveform(GetStrField(Obj, 'id'), GetIntField(Obj, 'buckets'))
    else if MsgType = 'delete_recording' then
      HandleDeleteRecording(GetStrField(Obj, 'id'))
    else if MsgType = 'split_recording' then
      HandleSplitRecording(GetStrField(Obj, 'id'), GetIntField(Obj, 'posMs') / 1000)
    else if MsgType = 'pick_record_dir' then
      HandlePickRecordDir
    else if MsgType = 'set_record_dir' then
      HandleSetRecordDir(GetStrField(Obj, 'path'))
    else if MsgType = 'set_codec' then
      HandleSetCodec(GetStrField(Obj, 'codec'))
    else if MsgType = 'set_hotkey' then
      HandleSetHotkey(GetStrField(Obj, 'hotkey'))
    else if MsgType = 'validate_hotkey' then
      HandleValidateHotkey(GetStrField(Obj, 'hotkey'))
    else if MsgType = 'set_autostart' then
      HandleSetAutostart(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_close_to_tray' then
      HandleSetCloseToTray(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_minimize_on_record' then
      HandleSetMinimizeOnRecord(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_notify_on_record' then
      HandleSetNotifyOnRecord(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_scroll_lock_indicator' then
      HandleSetScrollLockIndicator(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_play_sound_on_record' then
      HandleSetPlaySoundOnRecord(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_stop_on_lock' then
      HandleSetStopOnLock(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_hibernate' then
      HandleSetHibernate(GetBoolField(Obj, 'enabled'))
    else if MsgType = 'set_recording_quality' then
      HandleSetRecordingQuality(GetIntField(Obj, 'level'))
    else if MsgType = 'set_recording_fps' then
      HandleSetRecordingFps(GetIntField(Obj, 'fps'))
    else if MsgType = 'set_recording_keyframe' then
      HandleSetRecordingKeyframe(GetIntField(Obj, 'sec'))
    else if MsgType = 'set_language' then
      HandleSetLanguage(GetStrField(Obj, 'language'))
    else if MsgType = 'get_settings' then
      PushSettings
    else if MsgType = 'set_theme' then
      HandleSetTheme(GetStrField(Obj, 'theme'))
    else if MsgType = 'toggle_fullscreen' then
      OBSUI.ToggleFullscreen
    else if MsgType = 'tray_show' then
      // Disparado pelo clique numa notificacao do Windows (handler
      // show_notification no UI). Restaura a janela esteja ela na
      // bandeja ou minimizada na taskbar.
      OBSUI.RestoreFromTray
    else if MsgType = 'player_state' then
    begin
      // UI avisa que o modal de player abriu/fechou. Suspende audio
      // meters e thumb capture enquanto aberto pra evitar trabalho
      // desnecessario competindo com o video player.
      PlayerOpen := GetBoolField(Obj, 'open');
      Log('PlayerOpen=%s', [BoolToStr(PlayerOpen, True)]);
    end;
   except
     // Barreira: um handler que lanca excecao NAO pode escapar pro
     // WindowProc/DispatchMessage — isso mata o message pump e o app.
     // Loga e segue (convencao do projeto: falha isolada nao trava o app).
     on E: Exception do
       Log('Dispatch: excecao no handler type="%s": %s [%s]',
         [MsgType, E.Message, E.ClassName]);
   end;
  finally
    Obj.Free;
  end;
end;

procedure PushAppIcon;
// Envia o icone do app como data URL. Extrai o HICON do recurso
// MAINICON (criado automaticamente pelo Delphi a partir do icon.ico
// vinculado ao projeto) e converte pra PNG com canal alpha explicito.
//
// Pegadinhas evitadas:
//  1. Nao usar DrawIconEx — escreve pixels pre-multiplicados (alpha
//     misturado no RGB), o PNG resultante fica com a base escurecida.
//  2. Nao usar TPngImage.Assign(TBitmap) — perde o canal alpha mesmo
//     com pf32bit + afDefined.
//
// Abordagem correta: GetIconInfo retorna o hbmColor (bitmap interno
// do icone com BGRA cru). GetDIBits le os bytes diretos sem mexer
// no alpha. Copia pra TPngImage criado como COLOR_RGBALPHA, com a
// Scanline (RGB) e AlphaScanline (canal alpha) preenchidas separadas.
const
  REQUESTED_SIZE = 256;  // .ico tem 256 — pedimos esse pra qualidade maxima
var
  HIco: HICON;
  IconInfo: TIconInfo;
  BmInfo: Winapi.Windows.TBitmap;
  Dib: TBitmapInfo;
  Pixels: array of TRGBQuad;
  W, H, X, Y: Integer;
  ScreenDC: HDC;
  Png: TPngImage;
  PngRow: PRGBTriple;
  AlphaRow: PByteArray;
  Src: PRGBQuad;
  Stream: TMemoryStream;
  Bytes: TBytes;
  Base64: string;
  Obj: TJSONObject;
begin
  HIco := LoadImage(HInstance, 'MAINICON', IMAGE_ICON,
    REQUESTED_SIZE, REQUESTED_SIZE, LR_DEFAULTCOLOR);
  if HIco = 0 then HIco := LoadIcon(HInstance, 'MAINICON');
  if HIco = 0 then
  begin
    Log('PushAppIcon: MAINICON nao encontrado.');
    Exit;
  end;

  try
    ZeroMemory(@IconInfo, SizeOf(IconInfo));
    if not GetIconInfo(HIco, IconInfo) then Exit;
    try
      if IconInfo.hbmColor = 0 then Exit;  // icone monocromatico — ignora

      // Descobre o tamanho real do bitmap de cor do icone.
      ZeroMemory(@BmInfo, SizeOf(BmInfo));
      if GetObject(IconInfo.hbmColor, SizeOf(BmInfo), @BmInfo) = 0 then Exit;
      W := BmInfo.bmWidth;
      H := BmInfo.bmHeight;
      if (W <= 0) or (H <= 0) then Exit;

      // Configura DIB de destino: 32-bit, top-down (biHeight negativo),
      // sem compressao. GetDIBits copia exatamente os bytes BGRA do
      // bitmap do icone — pra icones 32-bit modernos, o byte de alpha
      // (rgbReserved) ja vem com os valores corretos do .ico, sem
      // pre-multiplicacao.
      ZeroMemory(@Dib, SizeOf(Dib));
      Dib.bmiHeader.biSize        := SizeOf(TBitmapInfoHeader);
      Dib.bmiHeader.biWidth       := W;
      Dib.bmiHeader.biHeight      := -H;  // negativo = top-down
      Dib.bmiHeader.biPlanes      := 1;
      Dib.bmiHeader.biBitCount    := 32;
      Dib.bmiHeader.biCompression := BI_RGB;

      SetLength(Pixels, W * H);
      ScreenDC := GetDC(0);
      try
        if GetDIBits(ScreenDC, IconInfo.hbmColor, 0, H,
             @Pixels[0], Dib, DIB_RGB_COLORS) = 0 then Exit;
      finally
        ReleaseDC(0, ScreenDC);
      end;

      // Cria PNG com canal alpha explicito (COLOR_RGBALPHA).
      Png := TPngImage.CreateBlank(COLOR_RGBALPHA, 8, W, H);
      try
        for Y := 0 to H - 1 do
        begin
          PngRow   := PRGBTriple(Png.Scanline[Y]);
          AlphaRow := PByteArray(Png.AlphaScanline[Y]);
          Src      := @Pixels[Y * W];
          for X := 0 to W - 1 do
          begin
            PngRow^.rgbtBlue  := Src^.rgbBlue;
            PngRow^.rgbtGreen := Src^.rgbGreen;
            PngRow^.rgbtRed   := Src^.rgbRed;
            AlphaRow^[X]      := Src^.rgbReserved;
            Inc(PngRow);
            Inc(Src);
          end;
        end;

        Stream := TMemoryStream.Create;
        try
          Png.SaveToStream(Stream);
          Stream.Position := 0;
          SetLength(Bytes, Stream.Size);
          if Stream.Size > 0 then
            Stream.ReadBuffer(Bytes[0], Stream.Size);
          Base64 := TNetEncoding.Base64.EncodeBytesToString(Bytes);
          Base64 := StringReplace(Base64, #13#10, '', [rfReplaceAll]);
          Base64 := StringReplace(Base64, #10,    '', [rfReplaceAll]);
        finally
          Stream.Free;
        end;
      finally
        Png.Free;
      end;
    finally
      // GetIconInfo aloca hbmColor e hbmMask — temos que liberar.
      if IconInfo.hbmColor <> 0 then DeleteObject(IconInfo.hbmColor);
      if IconInfo.hbmMask  <> 0 then DeleteObject(IconInfo.hbmMask);
    end;
  finally
    DestroyIcon(HIco);
  end;

  if Base64 = '' then Exit;
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'app_icon');
  Obj.AddPair('dataUrl', 'data:image/png;base64,' + Base64);
  PostOwned(Obj);
end;

procedure PushEncoderCaps;
// Detecta encoders disponiveis e envia pra UI: quais codecs sao
// suportados + logo do vendor do GPU. UI usa pra habilitar opcoes
// no select e mostrar o icone (AMD/NVIDIA/INTEL).
var
  Caps: TEncoderCaps;
  Obj: TJSONObject;
  VendorStr, VendorLogo: string;
begin
  try
    Caps := DetectEncoderCaps;
  except
    Exit;
  end;

  case Caps.Vendor of
    // VendorLogo e uma URL relativa servida da pasta ui\ pelo virtual host
    // (resolve contra https://noobs.app/ -> https://noobs.app/nvidia.png).
    gvNvidia: begin VendorStr := 'nvidia'; VendorLogo := 'nvidia.png'; end;
    gvAmd:    begin VendorStr := 'amd';    VendorLogo := 'amd.png'; end;
    gvIntel:  begin VendorStr := 'intel';  VendorLogo := 'intel.png'; end;
  else
    begin VendorStr := ''; VendorLogo := ''; end;
  end;

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'encoder_caps');
  Obj.AddPair('av1Hw', TJSONBool.Create(Caps.Av1Hw));
  Obj.AddPair('hevcHw', TJSONBool.Create(Caps.HevcHw));
  Obj.AddPair('h264Hw', TJSONBool.Create(Caps.H264Hw));
  Obj.AddPair('h264Sw', TJSONBool.Create(Caps.H264Sw));
  Obj.AddPair('vendor', VendorStr);
  Obj.AddPair('vendorLogo', VendorLogo);
  PostOwned(Obj);
  Log('Encoder caps: av1-hw=%s hevc-hw=%s h264-hw=%s h264-sw=%s vendor=%s',
    [BoolToStr(Caps.Av1Hw, True), BoolToStr(Caps.HevcHw, True),
     BoolToStr(Caps.H264Hw, True), BoolToStr(Caps.H264Sw, True), VendorStr]);
end;

procedure OnTimer(ATimerId: UINT_PTR);
begin
 try
  if ATimerId = TIMER_RECORDING_TICK then
  begin
    if RecordingActive then
      PushRecordingState;
  end
  else if ATimerId = TIMER_AUDIO_REFRESH then
  begin
    if AudioRefreshInProgress then Exit; // continua agendado, tenta no proximo tick
    KillTimer(MainWindowHandle, TIMER_AUDIO_REFRESH);
    Log('TIMER_AUDIO_REFRESH: debounce fired.');
    // SEM guard de RecordingActive: o DoRefreshAudio ja trata gravacao
    // internamente — durante a gravacao ele NAO atualiza a lista (que
    // esta congelada), mas mostra o banner "dispositivos alterados" e
    // marca PendingAudioRefresh pra aplicar no stop. Enumeracao roda em
    // worker, entao nao trava a UI nem mexe no engine. O guard antigo
    // pulava tudo e o usuario nao via mudanca nenhuma durante a gravacao.
    DoRefreshAudio;
  end
  else if ATimerId = TIMER_MONITOR_REFRESH then
  begin
    if MonitorRefreshInProgress then Exit;
    KillTimer(MainWindowHandle, TIMER_MONITOR_REFRESH);
    // Monitor/webcam ja tratam gravacao no proprio handler de evento
    // (OnDisplayChange/OnDeviceNodeChange: banner + defer, sem armar o
    // timer durante gravacao), entao mantemos o guard aqui.
    if not RecordingActive then
      DoRefreshMonitors;
  end
  else if ATimerId = TIMER_WEBCAM_REFRESH then
  begin
    KillTimer(MainWindowHandle, TIMER_WEBCAM_REFRESH);
    Log('TIMER_WEBCAM_REFRESH: debounce fired.');
    if not RecordingActive then
      DoRefreshWebcams;
  end
  else if ATimerId = TIMER_AUDIO_METER then
  begin
    // Suspende meters quando o player de video esta aberto — a sidebar
    // com os bars de nivel esta escondida atras do modal e atualiza-la
    // so gera reflow inutil + concorre por CPU com o video.
    if not PlayerOpen then
      PushAudioMetersFromWin;
  end
  else if ATimerId = TIMER_OBS_WARMUP then
  begin
    // One-shot — desliga antes de chamar (init bloqueia main thread
    // por ~300ms, evita disparar de novo se algo enroscar).
    KillTimer(MainWindowHandle, TIMER_OBS_WARMUP);
    if (Engine = nil) and (not RecordingActive) then
    begin
      try
        Engine := TOBSEngine.Create;
        Engine.OnStopped := OnEngineRecordingStopped;
        Engine.EnsureInitialized;
        Log('libobs: warmup pronto — proxima gravacao sera instantanea.');
        // Apos warmup, libobs ja conhece os encoders. Detecta + envia
        // pra UI poder mostrar logo do GPU e habilitar/desabilitar
        // opcoes no select de codec.
        PushEncoderCaps;
      except
        on E: Exception do
        begin
          Log('libobs: warmup falhou (gravacao vai inicializar sob demanda): %s',
            [E.Message]);
          if Engine <> nil then FreeAndNil(Engine);
        end;
      end;
    end;

    // /start-record: o hibernate spawnou esse processo apos o user
    // apertar a hotkey de gravacao. Dispara o start agora que libobs
    // esta warmed up. Se warmup falhou, HandleRecordStart fara init
    // sob demanda — paga ~300ms mas funciona.
    if OBSUI.StartRecordRequested and (not RecordingActive) then
    begin
      Log('TIMER_OBS_WARMUP: /start-record — disparando gravacao.');
      HandleRecordStart;
    end;
  end
  else if ATimerId = TIMER_HIBERNATE_IDLE then
  begin
    // 1 min sem janela visivel + sem gravacao: re-spawna em modo
    // /hibernate pra liberar RAM/GPU. One-shot — KillTimer antes
    // pra nao re-disparar caso o spawn demore.
    KillTimer(MainWindowHandle, TIMER_HIBERNATE_IDLE);
    if RecordingActive then
    begin
      Log('TIMER_HIBERNATE_IDLE: gravando, hibernacao adiada.');
      Exit;
    end;
    if OBSUI.MainWindowHandle <> 0 then
    begin
      if IsWindowVisible(OBSUI.MainWindowHandle) and
         not IsIconic(OBSUI.MainWindowHandle) then
      begin
        Log('TIMER_HIBERNATE_IDLE: janela visivel, ignorando.');
        Exit;
      end;
    end;
    Log('TIMER_HIBERNATE_IDLE: respawning como /hibernate.');
    OBSUI.SpawnHibernateAndExit;
  end
  else if ATimerId = TIMER_SCROLL_LOCK_BLINK then
  begin
    // Pisca o LED enquanto gravando. Se nao esta gravando mais (race
    // com HandleRecordStop), apaga e desarma. Sem log no toggle pra
    // nao poluir — 1 entry por segundo seria muito.
    if not RecordingActive then
    begin
      KillTimer(MainWindowHandle, TIMER_SCROLL_LOCK_BLINK);
      OBSScrollLock.SetScrollLockState(False);
      Exit;
    end;
    OBSScrollLock.ToggleScrollLock;
  end
  else if ATimerId = TIMER_STOP_TIMEOUT then
  begin
    // O sinal "stop" do output nunca chegou no prazo. One-shot — desarma
    // e forca a finalizacao (ForceCompleteStop dispara OnEngineRecordingStopped).
    KillTimer(MainWindowHandle, TIMER_STOP_TIMEOUT);
    Log('TIMER_STOP_TIMEOUT: sinal "stop" nao chegou — forcando finalizacao.');
    if Engine <> nil then
      try Engine.ForceCompleteStop; except on E: Exception do
        Log('ForceCompleteStop falhou: %s', [E.Message]); end;
  end;
 except
   // Barreira: timers rodam via WM_TIMER no WindowProc. Uma excecao
   // (ex.: falha WASAPI transitoria no meter de 100ms) escaparia pro
   // DispatchMessage e mataria o pump. Loga e segue.
   on E: Exception do
     Log('OnTimer: excecao no timer %d: %s [%s]',
       [Int64(ATimerId), E.Message, E.ClassName]);
 end;
end;

procedure Shutdown;
var
  Wait: DWORD;
begin
  Log('Shutdown: inicio');
  // Sinaliza pra todos os workers (capture, ffprobe, transcode...)
  // abortarem cedo. Atomico + memory barrier via TInterlocked — todos
  // os workers veem a mudanca imediatamente, sem race nem cache stale.
  SignalShutdown;
  // Atalhos globais — libera a combinacao pra outros apps usarem.
  UnregisterGlobalHotkey(HK_RECORD_TOGGLE);
  UnregisterGlobalHotkey(HK_RECORD_TOGGLE_ALT);
  if MainWindowHandle <> 0 then
  begin
    KillTimer(MainWindowHandle, TIMER_RECORDING_TICK);
    KillTimer(MainWindowHandle, TIMER_AUDIO_REFRESH);
    KillTimer(MainWindowHandle, TIMER_MONITOR_REFRESH);
    KillTimer(MainWindowHandle, TIMER_AUDIO_METER);
    KillTimer(MainWindowHandle, TIMER_OBS_WARMUP);
    KillTimer(MainWindowHandle, TIMER_HIBERNATE_IDLE);
    KillTimer(MainWindowHandle, TIMER_SCROLL_LOCK_BLINK);
    KillTimer(MainWindowHandle, TIMER_STOP_TIMEOUT);
  end;
  // Garante que o LED nao fique aceso se o app crashar/fechar mid-blink.
  try OBSScrollLock.SetScrollLockState(False); except end;
  Log('Shutdown: timers off');

  if ThumbThread <> nil then
  begin
    ThumbThread.Terminate;
    // Sleep granular de 100ms, no maximo 2s de espera total.
    Wait := WaitForSingleObject(ThumbThread.Handle, 2000);
    if Wait = WAIT_TIMEOUT then
    begin
      Log('Shutdown: ThumbThread nao parou em 2s — abandonando.');
      ThumbThread := nil;
    end
    else
      FreeAndNil(ThumbThread);
  end;
  Log('Shutdown: ThumbThread ok');

  try OBSRecordWatch.Stop; except end;
  Log('Shutdown: RecordWatch ok');

  try OBSAudioWatch.Stop; except end;
  Log('Shutdown: AudioWatch ok');

  // Detector de bloqueio: destructor termina + waits o TThread.
  if LockDetector <> nil then
  begin
    try FreeAndNil(LockDetector); except end;
  end;
  if LockEventHolder <> nil then
    FreeAndNil(LockEventHolder);
  Log('Shutdown: LockDetector ok');
  try StopPlayerServer; except end;
  Log('Shutdown: PlayerServer ok');

  if Engine <> nil then
  begin
    if Engine.IsRecording then
      try Engine.StopRecording; except end;
    try Engine.Teardown; except end;
    FreeAndNil(Engine);
  end;
  Log('Shutdown: Engine ok');

  Initialized := False;
  DoInitStarted := False;
  Log('Shutdown: fim');
end;

function ShouldHideOnClose: Boolean;
// WM_CLOSE chama esse callback pra decidir entre minimizar pra bandeja
// ou fechar de verdade. Regras:
//   - 'closeToTray' ON: user pediu pra app ficar rodando na bandeja
//   - Gravando: nunca interromper a gravacao por engano
//   - Caso contrario: fecha normal
begin
  Result := GetConfigBool('closeToTray', False) or RecordingActive;
end;

// Toggle de gravacao usado pelo menu do tray (item "Iniciar/Parar
// gravacao"). Mesmo comportamento da hotkey global.
procedure ToggleRecordFromTray;
begin
  if RecordingActive then HandleRecordStop
  else HandleRecordStart;
end;

function IsRecording: Boolean;
begin
  Result := RecordingActive;
end;

initialization
  // Liga a UI a este bridge sem criar dependencia direta de OBSUI -> OBSBridge.
  OBSUI.OnUIMessage           := Dispatch;
  OBSUI.OnUITimer             := OnTimer;
  OBSUI.OnUIDisplayChange     := OnDisplayChange;
  OBSUI.OnUIDeviceChange      := OnDeviceNodeChange;
  OBSUI.OnUIHotkey            := OnHotkey;
  OBSUI.OnUIShouldHideOnClose := ShouldHideOnClose;
  OBSUI.OnUIWindowHidden      := OnWindowHiddenForHibernate;
  OBSUI.OnUIWindowRestored    := OnWindowRestoredForHibernate;
  // Tray menu — item "Iniciar/Parar gravacao" usa esses callbacks.
  OBSTray.OnToggleRecord      := ToggleRecordFromTray;
  OBSTray.OnIsRecording       := IsRecording;

finalization
  Shutdown;

end.

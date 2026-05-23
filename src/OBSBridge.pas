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
  System.IOUtils,
  System.Classes,
  System.NetEncoding,
  Vcl.Graphics,
  Vcl.Imaging.PngImage,
  OBSUI,
  OBSLog,
  OBSScene,
  OBSPlayer,
  OBSConfig,
  OBSAudioWatch,
  OBSProbe,
  OBSRecordWatch,
  System.SyncObjs,
  FFmpegLib,
  NoOBSTypes,
  OBSEncoder,
  OBSAudioTracks,
  OBSEngine,
  OBSHotkey,
  OBSAutostart,
  OBSTray,
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
  OBS_WARMUP_DELAY_MS   = 1500;  // tempo pra UI renderizar antes do init

  PFX_MONITOR = 'NoOBS Monitor ';
  PFX_MIC     = 'NoOBS Mic - ';
  PFX_OUT     = 'NoOBS Out - ';
  PFX_WEBCAM  = 'NoOBS Webcam - ';

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
  // True se a janela estava visivel quando o auto-minimize on record
  // disparou. Usado pra restaurar a janela quando a gravacao parar
  // (via hotkey ou tray menu). Caso o user tenha minimizado manual
  // antes de gravar, esse flag fica False e nao restauramos.
  WindowWasVisibleBeforeRecord: Boolean = False;
  ThumbBusy: Boolean = False;       // evita pile-up se o tick anterior atrasar
  ThumbThread: TThumbTimerThread = nil;
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

  for k := 0 to High(MicIdxs) do
  begin
    i := MicIdxs[k];
    Id := MicIdFromName(Devs[i].Name);
    Item := TJSONObject.Create;
    Item.AddPair('id',   Id);
    Item.AddPair('name', Devs[i].Name);
    Item.AddPair('info', '');
    Item.AddPair('enabled',   TJSONBool.Create(MicEnabled[k]));
    Item.AddPair('isDefault', TJSONBool.Create(MicDefault[k]));
    Item.AddPair('track',     TJSONNumber.Create(MicTracks[k]));
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
    Item.AddPair('enabled',   TJSONBool.Create(SpkEnabled[k]));
    Item.AddPair('isDefault', TJSONBool.Create(SpkDefault[k]));
    Item.AddPair('track',     TJSONNumber.Create(SpkTracks[k]));
    ASpkJson.AddElement(Item);
  end;
end;

function ListRecordings(const ADir: string): TStringDynArray;
// Lista todos os arquivos de video da pasta de gravacao. Cobre os
// formatos que o OBS gera (mkv, mp4, mov, ts, fragmented mp4, flv).
const
  EXTS: array[0..6] of string = (
    '.mkv', '.mp4', '.mov', '.m4v', '.ts', '.flv', '.webm'
  );
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
    if MatchStr(Ext, EXTS) then
    begin
      Result[n] := All[i];
      Inc(n);
    end;
  end;
  SetLength(Result, n);
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

procedure PushInit(AIncludeAudio: Boolean = True);
var
  Init: TJSONObject;
begin
  // Se AIncludeAudio=False: pula a enumeracao WASAPI (que pode demorar
  // 30s+ em maquinas sem mic / audio service ruim). Caller deve depois
  // disparar enumeracao em worker thread e push audio_sources_refreshed.
  Init := TJSONObject.Create;
  Init.AddPair('type', 'init');
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

  // Pequena espera: o OBS pode estar finalizando o muxer do MKV
  // (cues/seek index sao escritos apos o ultimo cluster). Sem esse
  // settle, GetSize pode retornar tamanho parcial / 0.
  Sleep(500);

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
  PostOwned(Obj);

  // Gera thumb (duracao ja temos) em background — chega depois via
  // recording_meta e atualiza o card.
  ScanSingleRecordingMeta(AFilePath);
end;

// =====================================================================
// Init: garante OBS rodando, conecta, monta scene
// =====================================================================

procedure PushTheme;
var
  Obj: TJSONObject;
  Theme: string;
begin
  Theme := GetConfigStr('theme', 'dark');
  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'theme');
  Obj.AddPair('theme', Theme);
  PostOwned(Obj);
end;

procedure HandleSetTheme(const ATheme: string);
begin
  if (ATheme <> 'dark') and (ATheme <> 'light') then Exit;
  SetConfigStr('theme', ATheme);
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
      PostOwned(Init);
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
begin
  try
    Init := TJSONObject.Create;
    Init.AddPair('type', 'webcams_refreshed');
    Init.AddPair('webcams', BuildWebcamsFromWin);
    PostOwned(Init);
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
                  // Se o banner ja estava marcado durante gravacao mas a
                  // mudanca foi revertida, limpa o flag — UI ja reflete
                  // o estado atual, nao precisa refresh pos-stop.
                  if PendingAudioRefresh then PendingAudioRefresh := False;
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
                PostOwned(Init);
                LastAppliedAudioSig := NewSig;
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
  DEFAULT_HOTKEY = 'Pause';
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
      Reason := 'Atalho inválido — selecione pelo menos uma tecla principal ' +
                '(letra, número, F1-F12, Pause, etc).'
    else
      IsReservedHotkey(HK.Modifiers, HK.Vk, Reason);
  end;

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'hotkey_validation_result');
  Obj.AddPair('hotkey', Spec);
  Obj.AddPair('ok', TJSONBool.Create(Reason = ''));
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
  if Initialized then
  begin
    PushInit;
    Exit;
  end;
  Log('DoInit: inicio');

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
  PushRecordingState;
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
          // Captura signature inicial pra dedup de eventos futuros —
          // sem isso o 1o hot-plug compararia contra '' e sempre
          // mostraria banner mesmo que o estado fosse identico.
          LastAppliedAudioSig := InitSig;
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
    PostError('Nao da pra alterar fontes de video durante a gravacao.');
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

procedure HandleRecordStart;
var
  OutputPath: string;
  T0, TStep: UInt64;
begin
  if RecordingActive then Exit;

  T0 := GetTickCount64;
  Log('HandleRecordStart: inicio.');
  PushRefreshBusy(True, 'starting');
  try
    TStep := GetTickCount64;
    if Engine = nil then
      Engine := TOBSEngine.Create;
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
    MaybeNotifyRecord('NoOBS', 'Gravação iniciada.');
    Log('HandleRecordStart: total %dms.', [GetTickCount64 - T0]);
  except
    on E: Exception do
    begin
      RecordingActive := False;
      PostError('Falha ao iniciar gravacao: ' + E.Message);
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

procedure HandleRecordStop;
var
  OutputPath: string;
  Elapsed: Integer;
begin
  if not RecordingActive then Exit;

  KillTimer(MainWindowHandle, TIMER_RECORDING_TICK);
  Elapsed := Integer((GetTickCount - RecordingStartTickMs) div 1000);

  OutputPath := '';
  try
    if Engine <> nil then
      OutputPath := Engine.StopRecording;
  except
    on E: Exception do
      Log('StopRecord falhou: %s', [E.Message]);
  end;

  RecordingActive := False;
  LastRecordingPath := OutputPath;
  LastRecordingDuration := Elapsed;
  PushRecordingState;
  if OutputPath <> '' then
    PushRecordingAdded(OutputPath, Elapsed);

  // Notifica fim da gravacao (so se em tray e config permite).
  MaybeNotifyRecord('NoOBS',
    Format('Gravação finalizada (%dm %ds).', [Elapsed div 60, Elapsed mod 60]));

  // Se o auto-minimize escondeu a janela na hora do start, restaura.
  // O user esperava continuar olhando o NoOBS apos parar a gravacao
  // (via hotkey ou tray menu). Caso o user ja tivesse minimizado
  // manual, esse flag esta False e a janela continua na bandeja.
  if WindowWasVisibleBeforeRecord then
  begin
    WindowWasVisibleBeforeRecord := False;
    OBSUI.RestoreFromTray;
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
  if not TFile.Exists(AOldPath) then
  begin
    PostError('Arquivo nao encontrado: ' + AOldPath);
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
    PostError('Falha ao renomear arquivo.');
    Exit;
  end;

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
    PostError('Pasta de gravacoes nao encontrada.');
    Exit;
  end;
  ShellExecute(0, 'open', PChar(RecordDir), nil, nil, SW_SHOWNORMAL);
end;

procedure HandleOpenRecording(const APath: string);
begin
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
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
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
    Exit;
  end;
  Url := GetDirectUrl(APath);
  if Url = '' then
  begin
    PostError('Falha ao preparar URL do video.');
    Exit;
  end;
  PushPlayUrl(APath, Url, 'direct');
end;

procedure HandleRequestTranscode(const APath: string);
// JS chama isso depois que o player falhou tocando o arquivo original.
// Faz remux via libavformat (worker thread) e devolve URL do MP4.
begin
  if APath = '' then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
    Exit;
  end;
  if not FFmpegLibAvailable then
  begin
    PostError('Biblioteca de media (libavformat) nao disponivel.');
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
            PostError('Falha ao transcodar video' +
              IfThen(ErrMsg <> '', ': ' + ErrMsg, '.'));
        end);
    end).Start;
end;

procedure HandleRequestVideoInfo(const APath: string);
// Probe via libavformat roda em worker thread (10-50ms tipicamente,
// mas pode picar em arquivos grandes/remotos). UI mostra loading.
begin
  if APath = '' then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
    Exit;
  end;
  if not FFmpegLibAvailable then
  begin
    PostError('Biblioteca de media (libavformat) nao disponivel.');
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
    begin
      if IsShuttingDown then Exit;
      Ok := False;
      try Ok := Probe(APath, Report); except end;
      if IsShuttingDown then Exit;
      if not Ok then
      begin
        TThread.Queue(nil, procedure begin
          PostError('Falha ao inspecionar video.'); end);
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
        end
        else if S.Kind = 'audio' then
        begin
          StreamObj.AddPair('channels', TJSONNumber.Create(S.Channels));
          StreamObj.AddPair('sampleRate', TJSONNumber.Create(S.SampleRate));
        end;
        Streams.AddElement(StreamObj);
      end;
      Obj.AddPair('streams', Streams);

      TThread.Queue(nil, procedure begin PostOwned(Obj); end);
    end).Start;
end;

procedure HandleRequestAudioTracks(const APath: string);
// Extrai todas as audio tracks (uma vez, ~500ms-2s) e devolve URLs.
// JS cria audio elements sincronizados ao video element pra mixagem
// per-track em tempo real.
begin
  if APath = '' then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
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
          PostError('Falha ao extrair faixas de audio.'); end);
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
var
  ShOp: TSHFileOpStructW;
  Buf: array of WideChar;
  Len: Integer;
begin
  // pFrom de SHFileOperation exige path duplo-NUL-terminado.
  Len := Length(APath);
  SetLength(Buf, Len + 2);
  if Len > 0 then
    Move(PWideChar(APath)^, Buf[0], Len * SizeOf(WideChar));
  Buf[Len]     := #0;
  Buf[Len + 1] := #0;

  ZeroMemory(@ShOp, SizeOf(ShOp));
  ShOp.Wnd    := 0;
  ShOp.wFunc  := FO_DELETE;
  ShOp.pFrom  := @Buf[0];
  // ALLOWUNDO = vai pra Lixeira (recuperavel) em vez de delete permanente.
  // SILENT/NOCONFIRMATION/NOERRORUI = sem dialogos do shell.
  ShOp.fFlags := FOF_ALLOWUNDO or FOF_NOCONFIRMATION
              or FOF_SILENT or FOF_NOERRORUI;
  Result := SHFileOperationW(ShOp) = 0;
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
  Obj.AddPair('hotkey', GetConfigStr('hotkey', 'Pause'));
  Obj.AddPair('autostart', TJSONBool.Create(OBSAutostart.IsAutoStartEnabled));
  Obj.AddPair('closeToTray',
    TJSONBool.Create(GetConfigBool('closeToTray', False)));
  Obj.AddPair('minimizeOnRecord',
    TJSONBool.Create(GetConfigBool('minimizeOnRecord', False)));
  Obj.AddPair('notifyOnRecord',
    TJSONBool.Create(GetConfigBool('notifyOnRecord', False)));
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
    PostError('Pasta nao existe: ' + APath);
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
var
  Obj: TJSONObject;
begin
  if APath = '' then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
    Exit;
  end;
  if not DeleteToRecycleBin(APath) then
  begin
    PostError('Falha ao excluir o arquivo.');
    Exit;
  end;

  // Limpa cache desse arquivo agora — GC pegaria no proximo start, mas
  // sem custo fazer aqui.
  try
    GarbageCollectCache(ListRecordings(RecordDir));
  except end;

  Obj := TJSONObject.Create;
  Obj.AddPair('type', 'recording_removed');
  Obj.AddPair('id', APath);
  PostOwned(Obj);
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
    if Root <> nil then Root.Free;
    Exit;
  end;
  Obj := TJSONObject(Root);
  try
    MsgType := GetStrField(Obj, 'type');
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
    else if MsgType = 'delete_recording' then
      HandleDeleteRecording(GetStrField(Obj, 'id'))
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

function LoadResourceAsDataUrl(const AResName, AMime: string): string;
// Le um recurso RCDATA do .exe e devolve uma data URL (base64) usavel
// como src de <img> no WebView. Retorna '' se o recurso nao existir.
var
  Stream: TResourceStream;
  Bytes: TBytes;
  Base64: string;
begin
  Result := '';
  if FindResource(HInstance, PChar(AResName), RT_RCDATA) = 0 then Exit;
  try
    Stream := TResourceStream.Create(HInstance, AResName, RT_RCDATA);
    try
      SetLength(Bytes, Stream.Size);
      if Stream.Size > 0 then Stream.ReadBuffer(Bytes[0], Stream.Size);
    finally
      Stream.Free;
    end;
    Base64 := TNetEncoding.Base64.EncodeBytesToString(Bytes);
    // Remove quebras de linha que TNetEncoding adiciona.
    Base64 := StringReplace(Base64, #13#10, '', [rfReplaceAll]);
    Base64 := StringReplace(Base64, #10, '', [rfReplaceAll]);
    Result := 'data:' + AMime + ';base64,' + Base64;
  except
    Result := '';
  end;
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
    gvNvidia: begin VendorStr := 'nvidia'; VendorLogo := LoadResourceAsDataUrl('NVIDIA', 'image/png'); end;
    gvAmd:    begin VendorStr := 'amd';    VendorLogo := LoadResourceAsDataUrl('AMD',    'image/png'); end;
    gvIntel:  begin VendorStr := 'intel';  VendorLogo := LoadResourceAsDataUrl('INTEL',  'image/png'); end;
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
    if not RecordingActive then
      DoRefreshAudio;
  end
  else if ATimerId = TIMER_MONITOR_REFRESH then
  begin
    if MonitorRefreshInProgress then Exit;
    KillTimer(MainWindowHandle, TIMER_MONITOR_REFRESH);
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
  end;
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
  // Tray menu — item "Iniciar/Parar gravacao" usa esses callbacks.
  OBSTray.OnToggleRecord      := ToggleRecordFromTray;
  OBSTray.OnIsRecording       := IsRecording;

finalization
  Shutdown;

end.

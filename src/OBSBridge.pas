(*
  OBSBridge — ponte entre a UI HTML (via OBSUI) e o motor de gravacao
  (via LibOBSEngine).

  Fluxo:
    JS  -> OBSUI.OnUIMessage -> Dispatch -> handlers -> LibOBSEngine
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
  OBSUI,
  OBSLog,
  OBSScene,
  OBSPlayer,
  OBSConfig,
  OBSAudioWatch,
  OBSProbe,
  OBSRecordWatch,
  System.SyncObjs,
  LibOBSEngine,
  WinPreview,
  WinAudioMeter,
  WinWebcam;

const
  TIMER_RECORDING_TICK = 7001;
  RECORDING_TICK_MS    = 1000;

  THUMB_TICK_MS        = 1000;  // intervalo normal (TThumbTimerThread)
  THUMB_BURST_MS       = 400;   // intervalo durante burst pos-refresh

  TIMER_AUDIO_REFRESH  = 7003;
  AUDIO_REFRESH_DEBOUNCE_MS = 800;

  TIMER_AUDIO_METER    = 7005;
  AUDIO_METER_MS       = 100;

  TIMER_MONITOR_REFRESH = 7004;
  MONITOR_REFRESH_DEBOUNCE_MS = 2500;

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
    FNormalMs: Cardinal;
    FBurstMs: Cardinal;
    FBurstUntilTick: Cardinal;
  public
    constructor Create(ANormalMs, ABurstMs: Cardinal);
    procedure RequestBurst(ADurationMs: Cardinal);
    procedure Execute; override;
  end;

var
  Engine: TLibOBSEngine = nil;
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
  ThumbBusy: Boolean = False;       // evita pile-up se o tick anterior atrasar
  ThumbThread: TThumbTimerThread = nil;
  LastRecordingPath: string = '';
  LastRecordingDuration: Integer = 0;

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
    Thumb := CaptureMonitorAsDataUrl(Mons[i], 320, 180);
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

function BuildAudioFromWin(AKind: TAudioDeviceKind): TJSONArray;
var
  Devs: TAudioDeviceInfoArray;
  i: Integer;
  Item: TJSONObject;
  Id: string;
begin
  Result := TJSONArray.Create;
  InitAudio;
  Devs := EnumerateAudioDevices;
  for i := 0 to High(Devs) do
  begin
    if Devs[i].Kind <> AKind then Continue;
    if AKind = adkInput then
      Id := MicIdFromName(Devs[i].Name)
    else
      Id := OutIdFromName(Devs[i].Name);
    Item := TJSONObject.Create;
    Item.AddPair('id',   Id);
    Item.AddPair('name', Devs[i].Name);
    Item.AddPair('info', '');
    Item.AddPair('enabled', TJSONBool.Create(GetSourceEnabled(Id, True)));
    Result.AddElement(Item);
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

    // Le metadata cacheada (instantaneo). ffmpeg roda em background
    // depois pra preencher os que faltam.
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

procedure PushInit;
var
  Init: TJSONObject;
begin
  // PushInit agora le sources direto de Win32/WASAPI — OBS pode estar
  // dormindo (sobe so durante gravacao). Se OBS estiver vivo, podemos
  // tambem tentar as APIs de scene, mas o caminho default e Win-side.
  Init := TJSONObject.Create;
  Init.AddPair('type', 'init');
  Init.AddPair('monitors',   BuildMonitorsFromWin);
  Init.AddPair('mics',       BuildAudioFromWin(adkInput));
  Init.AddPair('speakers',   BuildAudioFromWin(adkOutput));
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
  if not FFmpegAvailable then Exit;
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
      try EnsureRecordingMeta(PathCopy, Dur, ThumbUrl); except end;
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
  try EnsureRecordingMeta(APath, Dur, ThumbUrl); except end;
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
// (ffmpeg cacheado) e empurra `recording_meta` por arquivo. UI
// atualiza os cards conforme chegam. Tambem faz GC do cache
// removendo arquivos cuja gravacao original ja nao existe.
var
  Files: TStringDynArray;
  LivePaths: TArray<string>;
  i: Integer;
begin
  if (RecordDir = '') or (not TDirectory.Exists(RecordDir)) then Exit;
  if not FFmpegAvailable then Exit;

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
        ProcessSingleMetaSync(Files[j]);
      end;
    end).Start;
end;

// Forward — definida logo abaixo da implementacao da thread.
procedure PushMonitorThumbs; forward;

// ----------------------------------------------------------------------
// TThumbTimerThread
// ----------------------------------------------------------------------

constructor TThumbTimerThread.Create(ANormalMs, ABurstMs: Cardinal);
begin
  FNormalMs := ANormalMs;
  FBurstMs := ABurstMs;
  FBurstUntilTick := 0;
  FreeOnTerminate := False;
  inherited Create(False);
end;

procedure TThumbTimerThread.RequestBurst(ADurationMs: Cardinal);
begin
  // Atomico: write de Cardinal alinhado e thread-safe em x64.
  FBurstUntilTick := GetTickCount + ADurationMs;
end;

procedure TThumbTimerThread.Execute;
var
  Interval, Step, Slept: Cardinal;
begin
  while not Terminated do
  begin
    if (FBurstUntilTick <> 0) and (GetTickCount < FBurstUntilTick) then
      Interval := FBurstMs
    else
    begin
      FBurstUntilTick := 0;
      Interval := FNormalMs;
    end;

    // Sleep em pedacos de 100ms pra terminar rapido no shutdown.
    Slept := 0;
    while (Slept < Interval) and (not Terminated) do
    begin
      Step := Interval - Slept;
      if Step > 100 then Step := 100;
      Sleep(Step);
      Inc(Slept, Step);
    end;
    if Terminated or IsShuttingDown then Break;

    try PushMonitorThumbs; except end;
  end;
end;

procedure PushMonitorThumbs;
// Captura screenshot de cada monitor e empurra como array {id, thumb}.
// Chamado pelo timer a cada 1s. Captura + JPEG + base64 sao caros pra
// monitores grandes (4K), entao roda numa worker thread pra nao travar
// a UI. ThumbBusy garante que so 1 captura roda em paralelo (skipa o
// tick se o anterior nao terminou).
var
  Mons: TMonitorInfoArray;
begin
  if IsShuttingDown or ThumbBusy then Exit;
  ThumbBusy := True;
  Mons := EnumerateMonitors; // rapido — so EnumDisplayMonitors

  TThread.CreateAnonymousThread(
    procedure
    var
      i: Integer;
      LocalArr: TArray<TPair<string, string>>;  // id, thumb
      Thumb, Id: string;
    begin
      try
        SetLength(LocalArr, Length(Mons));
        for i := 0 to High(Mons) do
        begin
          if IsShuttingDown then
          begin
            ThumbBusy := False;
            Exit;
          end;
          Id := MonitorIdFromIndex(Mons[i].Index);
          Thumb := CaptureMonitorAsDataUrl(Mons[i], 320, 180);
          LocalArr[i] := TPair<string, string>.Create(Id, Thumb);
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
                if LocalArr[j].Value = '' then Continue;
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
        // Se a captura quebrar, libera mesmo assim.
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
  RefreshInProgress: Boolean = False; // re-entrance guard pros refreshes
  LastMonitorCount: Integer = -1;     // pra detectar mudanca real
  MonitorRetryAttempts: Integer = 0;  // tentativas extras pos-evento

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
  if RefreshInProgress then
  begin
    Log('DoRefreshMonitors: outro refresh em andamento, ignorando.');
    Exit;
  end;
  RefreshInProgress := True;
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

    // Burst de 5s: captura a cada 400ms pra novos sources poderem
    // mostrar imagem assim que renderizam.
    if ThumbThread <> nil then ThumbThread.RequestBurst(5000);
  finally
    RefreshInProgress := False;
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

procedure DoRefreshAudio;
// Re-enumera audio devices via WASAPI e empurra a lista atualizada
// pra UI. Phase 3: OBS nao esta rodando (so durante gravacao), entao
// nao mexe em scene items — proxima gravacao usa a lista nova via
// BuildRecordingScene. Disparado por OBSAudioWatch ao detectar hot-
// plug (USB connect/disconnect).
var
  Init: TJSONObject;
begin
  if RefreshInProgress then
  begin
    Log('DoRefreshAudio: outro refresh em andamento, ignorando.');
    Exit;
  end;
  RefreshInProgress := True;
  PushRefreshBusy(True, 'audio');
  try
    try
      // Invalida cache do WinAudioMeter — proxima EnumerateAudioDevices
      // re-enumera fresco (pega devices novos, dropa devices removidos).
      RefreshAudioDevices;
      Init := TJSONObject.Create;
      Init.AddPair('type', 'audio_sources_refreshed');
      Init.AddPair('mics',     BuildAudioFromWin(adkInput));
      Init.AddPair('speakers', BuildAudioFromWin(adkOutput));
      PostOwned(Init);
    except
      on E: Exception do
        Log('DoRefreshAudio falhou: %s', [E.Message]);
    end;
  finally
    RefreshInProgress := False;
    PushRefreshBusy(False, 'audio');
  end;
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
begin
  if AKind = adcDefaultChanged then Exit;
  TThread.Queue(nil,
    procedure
    begin
      // Mostra banner imediato se em gravacao; refresh fica adiado
      // pra depois do stop. Sem gravacao, agenda timer pra disparar
      // o refresh apos a rajada de eventos.
      if RecordingActive then
      begin
        if not PendingAudioRefresh then
        begin
          PendingAudioRefresh := True;
          PushAudioDeviceChanged(True);
        end;
        Exit;
      end;
      // Reagenda — KillTimer + SetTimer com mesmo ID reseta o relogio.
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
    Item.AddPair('level', TJSONNumber.Create(Levels[i].PeakLevel));
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

procedure DoInit;
// Phase 3: OBS NAO e iniciado aqui. So sobe quando o usuario clica
// em "Iniciar Gravacao". Init pega monitores via Win32 e audio via
// WASAPI — UI funciona toda via APIs nativas.
begin
  if Initialized then
  begin
    PushInit;
    Exit;
  end;

  PushTheme;

  try StartPlayerServer; except on E: Exception do
    Log('Player: falha ao subir servidor: %s', [E.Message]); end;

  if RecordDir = '' then
  begin
    RecordDir := GetConfigStr('recordDir', '');
    if RecordDir = '' then
      RecordDir := GetEnvironmentVariable('USERPROFILE') + '\Videos';
  end;
  PushRecordings;
  ScanRecordingsMeta;

  // Sources via Win32 / WASAPI.
  InitAudio;
  PushInit;
  PushRecordingState;

  // Captura de thumbs em thread propria (independe do WM_TIMER que e
  // suprimido durante modal sizemove loop — drag/resize de janela).
  if ThumbThread = nil then
    ThumbThread := TThumbTimerThread.Create(THUMB_TICK_MS, THUMB_BURST_MS);

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

  Initialized := True;
  Log('DoInit: pronto (sem OBS — sobe na hora da gravacao).');
end;

// =====================================================================
// Comandos vindos do JS
// =====================================================================

procedure HandleToggleSource(const AId: string; AEnabled: Boolean);
var
  IsMonitor, IsAudio: Boolean;
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
end;

procedure HandleRecordStart;
var
  OutputPath: string;
begin
  if RecordingActive then Exit;

  PushRefreshBusy(True, 'starting');
  try
    if Engine = nil then
      Engine := TLibOBSEngine.Create;
    Engine.EnsureInitialized;

    OutputPath := IncludeTrailingPathDelimiter(RecordDir)
      + 'NoOBS_' + FormatDateTime('yyyy-mm-dd_hh-nn-ss', Now) + '.mkv';
    Engine.BuildAndStartRecording(OutputPath);

    RecordingActive := True;
    LastRecordingPath := OutputPath;
    LastRecordingDuration := 0;
    RecordingStartTickMs := GetTickCount;
    SetTimer(MainWindowHandle, TIMER_RECORDING_TICK, RECORDING_TICK_MS, nil);
    PushRecordingState;
  except
    on E: Exception do
    begin
      RecordingActive := False;
      PostError('Falha ao iniciar gravacao: ' + E.Message);
      PushRecordingState;
    end;
  end;
  PushRefreshBusy(False, 'starting');
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
// Roda ffmpeg (worker thread) e devolve URL transcodada.
begin
  if APath = '' then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
    Exit;
  end;
  if not FFmpegAvailable then
  begin
    PostError('ffmpeg.exe nao encontrado na pasta do app.');
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
// ffprobe roda em worker thread (pode levar 100-500ms). UI mostra
// loading enquanto isso.
begin
  if APath = '' then Exit;
  if not TFile.Exists(APath) then
  begin
    PostError('Arquivo nao encontrado.');
    Exit;
  end;
  if not FFprobeAvailable then
  begin
    PostError('ffprobe.exe nao encontrado.');
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
          PostError('Falha ao inspecionar video com ffprobe.'); end);
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
  PostOwned(Obj);
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
begin
  if APath = '' then Exit;
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
    else if MsgType = 'open_record_dir' then
      HandleOpenRecordDir
    else if MsgType = 'play_recording' then
      HandlePlayRecording(GetStrField(Obj, 'id'))
    else if MsgType = 'request_transcode' then
      HandleRequestTranscode(GetStrField(Obj, 'id'))
    else if MsgType = 'request_video_info' then
      HandleRequestVideoInfo(GetStrField(Obj, 'id'))
    else if MsgType = 'delete_recording' then
      HandleDeleteRecording(GetStrField(Obj, 'id'))
    else if MsgType = 'pick_record_dir' then
      HandlePickRecordDir
    else if MsgType = 'set_record_dir' then
      HandleSetRecordDir(GetStrField(Obj, 'path'))
    else if MsgType = 'get_settings' then
      PushSettings
    else if MsgType = 'set_theme' then
      HandleSetTheme(GetStrField(Obj, 'theme'))
    else if MsgType = 'toggle_fullscreen' then
      OBSUI.ToggleFullscreen;
  finally
    Obj.Free;
  end;
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
    if RefreshInProgress then Exit; // continua agendado, tenta no proximo tick
    KillTimer(MainWindowHandle, TIMER_AUDIO_REFRESH);
    if not RecordingActive then
      DoRefreshAudio;
  end
  else if ATimerId = TIMER_MONITOR_REFRESH then
  begin
    if RefreshInProgress then Exit;
    KillTimer(MainWindowHandle, TIMER_MONITOR_REFRESH);
    if not RecordingActive then
      DoRefreshMonitors;
  end
  else if ATimerId = TIMER_AUDIO_METER then
  begin
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
        Engine := TLibOBSEngine.Create;
        Engine.EnsureInitialized;
        Log('libobs: warmup pronto — proxima gravacao sera instantanea.');
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

initialization
  // Liga a UI a este bridge sem criar dependencia direta de OBSUI -> OBSBridge.
  OBSUI.OnUIMessage       := Dispatch;
  OBSUI.OnUITimer         := OnTimer;
  OBSUI.OnUIDisplayChange := OnDisplayChange;

finalization
  Shutdown;

end.

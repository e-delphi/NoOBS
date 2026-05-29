{
  OBSPlayer — servidor HTTP local + libavformat pra tocar gravacoes
  dentro do WebView2.

  Por que isso existe:
    WebView2 nao toca arquivos locais via file:// por padrao, e MKV
    nao toca direto no Chromium. A solucao e remuxar pra MP4
    (sem reencode, so troca de container) e servir pelo localhost
    com suporte a Range (essencial pra seek).

  Fluxo:
    1. Startup: HTTP server sobe em 127.0.0.1:porta-livre.
    2. UI pede play -> GetPlayUrl(path):
       a. Calcula path do cache (<localappdata>\NoOBS\cache\<hash>.mp4).
       b. Se nao existe, chama FFmpegOps.RemuxFile (MKV->MP4 in-process).
       c. Devolve "http://127.0.0.1:porta/<token>.mp4".
    3. WebView2 pede o arquivo -> server serve com Range.

  Threading: remux/extracao de audio/thumb sao via DLL e in-process,
  mas ainda demoram dezenas-centenas de ms — chamar de worker thread
  pra nao travar a UI.
}
unit OBSPlayer;

interface

uses
  System.SysUtils,
  NoOBSTypes;

procedure StartPlayerServer;
procedure StopPlayerServer;

// Armazena JPEG em memoria pra um monitor (id = "NoOBS Monitor N").
// Chamado pela worker thread de thumbs. Thread-safe.
procedure SetMonitorThumb(const AId: string; const AJpeg: TBytes);

// URL do thumb de um monitor via HTTP (ex: /thumb/mon0.jpg?v=123).
// Retorna '' se o server nao subiu ou nao ha thumb pro id.
function GetMonitorThumbUrl(const AId: string): string;

// URL "direta" — serve o arquivo original sem transcode. Pode falhar
// no player se o codec nao for suportado pelo WebView2 (ex: HEVC sem
// HEVC Video Extensions instalado). Instantaneo, sem ffmpeg.
function GetDirectUrl(const APath: string): string;

// URL "transcodada" — garante MP4/H.264 jogavel. Pode demorar (ffmpeg).
// Use de worker thread se chamada vier da main pra evitar travar a UI.
function GetTranscodedUrl(const APath: string): string;

// Garante metadata cacheada (duracao em segundos + thumb JPG) para
// uma gravacao. Usa libavformat. Devolve True se ok. Worker thread.
function EnsureRecordingMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string): Boolean;

// So le o cache (instantaneo, sem ffmpeg). Devolve duracao=0 e thumb=''
// se nao houver. Pra usar na main thread sem travar.
procedure GetCachedMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string);

// ---- Metadata unificada (.json em <hash>.json) ----
// Le/escreve duracao + layout (canvas + regions de cada monitor/webcam).
// Bridge salva o layout em HandleRecordStop; player le via
// HandleRequestVideoInfo pra construir o seletor de monitor.
function LoadRecordingMeta(const APath: string;
  out AMeta: TRecordingMeta): Boolean;
procedure SaveRecordingMeta(const APath: string;
  const AMeta: TRecordingMeta);

// Remove arquivos de cache que nao pertencem a nenhuma das gravacoes
// listadas. ALivePaths sao os paths das gravacoes que ainda existem.
procedure GarbageCollectCache(const ALivePaths: TArray<string>);

// Migra os arquivos de cache (<hash>.dur/.jpg/.mp4/.json, <hash>_aN.m4a)
// do hash do path antigo pro hash do novo apos um rename. Sem isso o
// cache fica orfao (so limpo no proximo GC) e a gravacao renomeada perde
// thumb/duracao ate regenerar. No-op se os hashes coincidirem.
procedure RenameCacheEntries(const AOldPath, ANewPath: string);

// Extrai todas as audio tracks da gravacao em arquivos separados (m4a)
// e devolve URLs servidas pelo HTTP server. Idempotente: se ja extraiu
// antes, retorna direto do cache (~ms). Primeira extracao usa ffmpeg
// -c copy (sem reencode), ~500ms-2s pra gravacao de 10 min.
// Retorna False se ffmpeg falhar ou arquivo nao existir.
function GetAudioTrackUrls(const APath: string;
  out AUrls: TArray<string>): Boolean;

// Pasta raiz do cache (%LOCALAPPDATA%\NoOBS\cache\) — exposta pra
// OBSUI mapear como virtual host (https://cache.noobs.app/...) usado
// pela waveform. Fetch via HTTP local seria bloqueado por mixed content
// (UI roda em origem https://).
function CacheRootDir: string;

implementation

uses
  Winapi.Windows,
  System.Classes,
  System.IOUtils,
  System.Hash,
  System.StrUtils,
  System.JSON,
  System.Generics.Collections,
  System.SyncObjs,
  IdContext,
  IdCustomHTTPServer,
  IdHTTPServer,
  IdGlobal,
  OBSLog,
  OBSProbe,
  FFmpegLib,
  FFmpegOps;

type
  // OnCommandGet exige method-of-object. Esta classe e um trampolim.
  TPlayerServerHandler = class
    procedure HandleGet(AContext: TIdContext;
      ARequest: TIdHTTPRequestInfo; AResponse: TIdHTTPResponseInfo);
  end;

var
  Handler: TPlayerServerHandler = nil;

var
  Server: TIdHTTPServer = nil;
  ServerPort: Integer = 0;

  // Mapa token (basename do cache, sem ext) -> path absoluto do MP4
  // no disco. Garante que so servimos o que registramos.
  TokenMap: TDictionary<string, string> = nil;
  TokenLock: TCriticalSection = nil;

  // Thumbs volateis de monitor: id -> JPEG bytes. Atualizado a cada
  // ~1s pela worker thread de captura. Servido em /thumb/<slot>.jpg.
  ThumbMap: TDictionary<string, TBytes> = nil;
  ThumbVersion: Integer = 0;
  ThumbLock: TCriticalSection = nil;

function ExeDir: string;
begin
  Result := ExtractFilePath(ParamStr(0));
end;

function CacheRootDir: string;
// Cache centralizado em %LOCALAPPDATA%\NoOBS\cache.
var
  AppData: string;
begin
  AppData := GetEnvironmentVariable('LOCALAPPDATA');
  if AppData = '' then AppData := GetEnvironmentVariable('APPDATA');
  if AppData = '' then AppData := ExeDir;
  Result := IncludeTrailingPathDelimiter(AppData) + 'NoOBS\cache\';
end;

// FFmpegAvailable removida — callers usam FFmpegLib.FFmpegLibAvailable
// diretamente. Sem fallback pra ffmpeg.exe (nao temos mais).

// RunFFmpegCapture/DetectVideoCodec removidos — migrados pra libav.
// Probe (OBSProbe) e RemuxFile/ExtractAudioTracks/ExtractFrameJpeg
// (FFmpegLib) cobrem todos os casos sem fork de processo.

function CacheDirFor(const ASourcePath: string): string;
begin
  Result := CacheRootDir;
  if not DirectoryExists(Result) then
    ForceDirectories(Result);
end;

function HashName(const APath: string): string;
// Hash curto pra usar como nome de cache + URL token. Evita conflitos
// e nao expoe nome real na URL.
var
  Bytes: TBytes;
  i: Integer;
begin
  Bytes := THashSHA1.GetHashBytes(APath.ToLower);
  Result := '';
  for i := 0 to 9 do
    Result := Result + IntToHex(Bytes[i], 2);
  Result := LowerCase(Result);
end;

procedure RenameCacheEntries(const AOldPath, ANewPath: string);
var
  Dir, OldHash, NewHash, Tail, NewName: string;
  Files: TArray<string>;
  F: string;
begin
  Dir := CacheDirFor(AOldPath);
  if not DirectoryExists(Dir) then Exit;
  OldHash := HashName(AOldPath);
  NewHash := HashName(ANewPath);
  if SameText(OldHash, NewHash) then Exit;
  try
    // <hash>* cobre .dur/.jpg/.mp4/.json e _aN.m4a (todos prefixados
    // pelo hash de 20 chars hex). Colisao com outro arquivo e improvavel
    // (SHA1 de paths distintos).
    Files := TDirectory.GetFiles(Dir, OldHash + '*');
  except
    on E: Exception do
    begin
      Log('RenameCacheEntries: GetFiles falhou: %s', [E.Message]);
      Exit;
    end;
  end;
  for F in Files do
  begin
    // Tail = tudo apos o hash (ex.: ".dur", ".jpg", "_a1.m4a").
    Tail := Copy(ExtractFileName(F), Length(OldHash) + 1, MaxInt);
    NewName := IncludeTrailingPathDelimiter(Dir) + NewHash + Tail;
    try
      if TFile.Exists(NewName) then TFile.Delete(NewName);
      TFile.Move(F, NewName);
    except
      on E: Exception do
        Log('RenameCacheEntries: move %s falhou: %s',
          [ExtractFileName(F), E.Message]);
    end;
  end;
end;

procedure SetMonitorThumb(const AId: string; const AJpeg: TBytes);
begin
  if ThumbLock = nil then Exit;
  ThumbLock.Enter;
  try
    ThumbMap.AddOrSetValue(AId, AJpeg);
    Inc(ThumbVersion);
  finally
    ThumbLock.Leave;
  end;
end;

function GetMonitorThumbUrl(const AId: string): string;
var
  Ver: Integer;
begin
  Result := '';
  if (Server = nil) or (ThumbLock = nil) then Exit;
  ThumbLock.Enter;
  try
    if not ThumbMap.ContainsKey(AId) then Exit;
    Ver := ThumbVersion;
  finally
    ThumbLock.Leave;
  end;
  Result := Format('http://127.0.0.1:%d/thumb/%s.jpg?v=%d',
    [ServerPort, AId, Ver]);
end;

function EnsureCachedMp4(const APath: string): string;
// Remux MKV -> MP4 via libavformat (sem ffmpeg.exe). Sempre `-c copy`
// equivalente — apenas troca de container. WebView2/Chromium toca:
//   - H.264 em MP4: sempre
//   - HEVC em MP4: precisa de HEVC Video Extensions ou hardware decode.
// Se HEVC nao tocar, isso e responsabilidade do browser; nao tem como
// resolver sem fazer transcode (que vamos evitar pelo custo).
var
  CacheDir, CacheFile: string;
  SrcSize, CacheSize: Int64;
  T0: UInt64;
begin
  Result := '';
  if not FileExists(APath) then Exit;
  if not FFmpegLibAvailable then
  begin
    Log('Player: libavformat indisponivel — nao da pra preparar cache.');
    Exit;
  end;

  CacheDir := CacheDirFor(APath);
  CacheFile := IncludeTrailingPathDelimiter(CacheDir) +
    HashName(APath) + '.mp4';

  // Se cache existe e original nao mudou desde a criacao, reusa.
  if FileExists(CacheFile) then
  begin
    try
      SrcSize := TFile.GetSize(APath);
      CacheSize := TFile.GetSize(CacheFile);
    except
      SrcSize := -1;
      CacheSize := 0;
    end;
    if (CacheSize > 0) and (SrcSize > 0) then
      Exit(CacheFile);
    // Cache invalido — apaga.
    try TFile.Delete(CacheFile); except end;
  end;

  T0 := GetTickCount64;
  if not RemuxFile(APath, CacheFile) then
  begin
    Log('Player: remux falhou para %s', [ExtractFileName(APath)]);
    Exit;
  end;
  Log('Player: remux em %dms -> %s',
    [GetTickCount64 - T0, ExtractFileName(CacheFile)]);
  Result := CacheFile;
end;

function MakeUrl(const ATokenSuffix, AFilePath, AExt: string): string;
// Registra um token -> arquivo no mapa e devolve a URL.
var
  Token: string;
begin
  Result := '';
  if Server = nil then Exit;
  if not FileExists(AFilePath) then Exit;
  Token := HashName(AFilePath) + ATokenSuffix;
  TokenLock.Enter;
  try
    TokenMap.AddOrSetValue(Token, AFilePath);
  finally
    TokenLock.Leave;
  end;
  Result := Format('http://127.0.0.1:%d/v/%s%s', [ServerPort, Token, AExt]);
end;

// ParseDurationFromFFmpeg removida — duracao agora vem do Probe()
// (libavformat), sem precisar parsear stderr.

function MetaFilePath(const APath: string): string;
begin
  Result := IncludeTrailingPathDelimiter(CacheDirFor(APath)) +
    HashName(APath) + '.json';
end;

function LoadRecordingMeta(const APath: string;
  out AMeta: TRecordingMeta): Boolean;
// Le <hash>.json. True se conseguiu parsear.
var
  MetaFile, Content: string;
  Root, V: TJSONValue;
  Obj, CanvasObj, RegObj: TJSONObject;
  Arr: TJSONArray;
  i: Integer;
begin
  Result := False;
  AMeta := Default(TRecordingMeta);

  MetaFile := MetaFilePath(APath);
  if not FileExists(MetaFile) then Exit;

  try
    Content := TFile.ReadAllText(MetaFile, TEncoding.UTF8);
    Root := TJSONObject.ParseJSONValue(Content);
    if Root is TJSONObject then
    try
      Obj := TJSONObject(Root);
      V := Obj.GetValue('duration');
      if V is TJSONNumber then
        AMeta.DurationSec := TJSONNumber(V).AsInt;
      V := Obj.GetValue('canvas');
      if V is TJSONObject then
      begin
        CanvasObj := TJSONObject(V);
        if CanvasObj.GetValue('w') is TJSONNumber then
          AMeta.Layout.CanvasW := TJSONNumber(CanvasObj.GetValue('w')).AsInt;
        if CanvasObj.GetValue('h') is TJSONNumber then
          AMeta.Layout.CanvasH := TJSONNumber(CanvasObj.GetValue('h')).AsInt;
      end;
      V := Obj.GetValue('monitors');
      if V is TJSONArray then
      begin
        Arr := TJSONArray(V);
        SetLength(AMeta.Layout.Regions, Arr.Count);
        for i := 0 to Arr.Count - 1 do
        begin
          if not (Arr.Items[i] is TJSONObject) then Continue;
          RegObj := TJSONObject(Arr.Items[i]);
          if RegObj.GetValue('name') is TJSONString then
            AMeta.Layout.Regions[i].Name := TJSONString(RegObj.GetValue('name')).Value;
          if RegObj.GetValue('kind') is TJSONString then
            AMeta.Layout.Regions[i].Kind := TJSONString(RegObj.GetValue('kind')).Value;
          if RegObj.GetValue('x') is TJSONNumber then
            AMeta.Layout.Regions[i].X := TJSONNumber(RegObj.GetValue('x')).AsInt;
          if RegObj.GetValue('y') is TJSONNumber then
            AMeta.Layout.Regions[i].Y := TJSONNumber(RegObj.GetValue('y')).AsInt;
          if RegObj.GetValue('w') is TJSONNumber then
            AMeta.Layout.Regions[i].W := TJSONNumber(RegObj.GetValue('w')).AsInt;
          if RegObj.GetValue('h') is TJSONNumber then
            AMeta.Layout.Regions[i].H := TJSONNumber(RegObj.GetValue('h')).AsInt;
        end;
      end;
      Result := True;
    finally
      Root.Free;
    end
    else if Root <> nil then Root.Free;
  except
    on E: Exception do
      Log('LoadRecordingMeta: erro lendo %s: %s',
        [ExtractFileName(MetaFile), E.Message]);
  end;
end;

procedure SaveRecordingMeta(const APath: string;
  const AMeta: TRecordingMeta);
var
  MetaFile: string;
  Obj, CanvasObj, RegObj: TJSONObject;
  Arr: TJSONArray;
  i: Integer;
begin
  MetaFile := MetaFilePath(APath);
  try
    ForceDirectories(ExtractFilePath(MetaFile));
    Obj := TJSONObject.Create;
    try
      Obj.AddPair('duration', TJSONNumber.Create(AMeta.DurationSec));
      if (AMeta.Layout.CanvasW > 0) and (AMeta.Layout.CanvasH > 0) then
      begin
        CanvasObj := TJSONObject.Create;
        CanvasObj.AddPair('w', TJSONNumber.Create(AMeta.Layout.CanvasW));
        CanvasObj.AddPair('h', TJSONNumber.Create(AMeta.Layout.CanvasH));
        Obj.AddPair('canvas', CanvasObj);
      end;
      if Length(AMeta.Layout.Regions) > 0 then
      begin
        Arr := TJSONArray.Create;
        for i := 0 to High(AMeta.Layout.Regions) do
        begin
          RegObj := TJSONObject.Create;
          RegObj.AddPair('name', AMeta.Layout.Regions[i].Name);
          RegObj.AddPair('kind', AMeta.Layout.Regions[i].Kind);
          RegObj.AddPair('x', TJSONNumber.Create(AMeta.Layout.Regions[i].X));
          RegObj.AddPair('y', TJSONNumber.Create(AMeta.Layout.Regions[i].Y));
          RegObj.AddPair('w', TJSONNumber.Create(AMeta.Layout.Regions[i].W));
          RegObj.AddPair('h', TJSONNumber.Create(AMeta.Layout.Regions[i].H));
          Arr.AddElement(RegObj);
        end;
        Obj.AddPair('monitors', Arr);
      end;
      TFile.WriteAllText(MetaFile, Obj.ToJSON, TEncoding.UTF8);
    finally
      Obj.Free;
    end;
  except
    on E: Exception do
      Log('SaveRecordingMeta: erro escrevendo %s: %s',
        [ExtractFileName(MetaFile), E.Message]);
  end;
end;

function EnsureRecordingMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string): Boolean;
// Duracao + thumbnail via libavformat/libavcodec — sem ffmpeg.exe.
// Duracao vem do Probe() (ja usa libav). Thumb extraido via
// ExtractFrameJpeg() (decode + swscale + mjpeg encode).
var
  CacheDir, ThumbFile, Token: string;
  Meta: TRecordingMeta;
  SeekTs: Integer;
  Report: TProbeReport;
begin
  Result := False;
  ADurationSec := 0;
  AThumbUrl := '';
  if not FileExists(APath) then Exit;
  if not FFmpegLibAvailable then Exit;

  CacheDir := CacheDirFor(APath);
  Token := HashName(APath);
  ThumbFile := IncludeTrailingPathDelimiter(CacheDir) + Token + '.jpg';

  LoadRecordingMeta(APath, Meta);
  ADurationSec := Meta.DurationSec;

  if ADurationSec = 0 then
  begin
    if Probe(APath, Report) then
      ADurationSec := Round(Report.Duration);
    if ADurationSec > 0 then
    begin
      Meta.DurationSec := ADurationSec;
      SaveRecordingMeta(APath, Meta);
    end;
  end;

  // Remove thumb cacheado se ficou vazio/quebrado de uma corrida
  // anterior — caso contrario FileExists segue True e a gente pula
  // a geracao, deixando img tag quebrada no UI eternamente.
  if FileExists(ThumbFile) then
    try
      if TFile.GetSize(ThumbFile) < 100 then
      begin
        Log('Player: thumb cacheado vazio/curto, regenerando: %s',
          [ExtractFileName(ThumbFile)]);
        TFile.Delete(ThumbFile);
      end;
    except end;

  if not FileExists(ThumbFile) then
  begin
    SeekTs := 1;
    if ADurationSec > 10 then SeekTs := ADurationSec div 10;
    // Altura 240 do thumb — card e ~212x122 e usa object-fit:cover.
    // 16:9 vira 427x240, ultra-wide 16:4 vira ~995x240 (resolucao
    // suficiente pra qualquer aspect sem upscaling feio).
    if not ExtractFrameJpeg(APath, ThumbFile, SeekTs, 240) then
      Log('Player: thumbnail falhou para %s', [ExtractFileName(APath)]);
  end;

  if FileExists(ThumbFile) then
    AThumbUrl := MakeUrl('-thumb', ThumbFile, '.jpg');

  Result := (ADurationSec > 0) or (AThumbUrl <> '');
end;

procedure GetCachedMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string);
var
  CacheDir, ThumbFile, Token: string;
  Meta: TRecordingMeta;
begin
  ADurationSec := 0;
  AThumbUrl := '';
  if not FileExists(APath) then Exit;
  CacheDir := CacheRootDir;
  if not DirectoryExists(CacheDir) then Exit;

  Token := HashName(APath);
  ThumbFile := IncludeTrailingPathDelimiter(CacheDir) + Token + '.jpg';

  LoadRecordingMeta(APath, Meta);
  ADurationSec := Meta.DurationSec;

  if FileExists(ThumbFile) and (Server <> nil) then
    AThumbUrl := MakeUrl('-thumb', ThumbFile, '.jpg');
end;

function GetDirectUrl(const APath: string): string;
var
  Ext: string;
begin
  Ext := LowerCase(ExtractFileExt(APath));
  Result := MakeUrl('-direct', APath, Ext);
end;

procedure GarbageCollectCache(const ALivePaths: TArray<string>);
var
  Live: TDictionary<string, Boolean>;
  Dir: string;
  Files: TArray<string>;
  i: Integer;
  Base, Hash: string;
  Removed: Integer;
begin
  Dir := CacheRootDir;
  if not DirectoryExists(Dir) then Exit;

  // Hashes vivos = hash de cada path atual.
  Live := TDictionary<string, Boolean>.Create;
  try
    for i := 0 to High(ALivePaths) do
      Live.AddOrSetValue(HashName(ALivePaths[i]), True);

    Files := System.IOUtils.TDirectory.GetFiles(Dir);
    Removed := 0;
    for i := 0 to High(Files) do
    begin
      // Nome cache: <hash>.<ext>. Hash = nome sem extensao.
      Base := ChangeFileExt(ExtractFileName(Files[i]), '');
      Hash := LowerCase(Base);
      if not Live.ContainsKey(Hash) then
      begin
        try
          System.IOUtils.TFile.Delete(Files[i]);
          Inc(Removed);
        except end;
      end;
    end;
    if Removed > 0 then
      Log('Cache GC: %d arquivo(s) orfao(s) removido(s).', [Removed]);
  finally
    Live.Free;
  end;
end;

function GetTranscodedUrl(const APath: string): string;
var
  CacheFile: string;
begin
  Result := '';
  CacheFile := EnsureCachedMp4(APath);
  if CacheFile = '' then Exit;
  Result := MakeUrl('-tx', CacheFile, '.mp4');
end;

function GetAudioTrackUrls(const APath: string;
  out AUrls: TArray<string>): Boolean;
// Extrai cada faixa de audio como .m4a (AAC stream copy) via
// libavformat — sem fork de ffmpeg.exe. Cacheia por hash do path.
var
  Report: TProbeReport;
  CacheDir, Token, TrackFile: string;
  TrackFiles: TArray<string>;
  AudioStreams: TStreamArray;
  i, TrackCount, NeedExtract: Integer;
  T0: UInt64;
begin
  Result := False;
  SetLength(AUrls, 0);
  if not FileExists(APath) then Exit;
  if not FFmpegLibAvailable then Exit;

  if not Probe(APath, Report) then Exit;
  AudioStreams := Report.AudioStreams;
  TrackCount := Length(AudioStreams);
  if TrackCount = 0 then Exit;

  CacheDir := CacheDirFor(APath);
  if not DirectoryExists(CacheDir) then ForceDirectories(CacheDir);
  Token := HashName(APath);

  // Constroi lista de arquivos esperados + checa cache.
  SetLength(TrackFiles, TrackCount);
  NeedExtract := 0;
  for i := 0 to TrackCount - 1 do
  begin
    TrackFiles[i] := IncludeTrailingPathDelimiter(CacheDir) +
      Format('%s_a%d.m4a', [Token, i]);
    if not FileExists(TrackFiles[i]) then Inc(NeedExtract);
  end;

  if NeedExtract > 0 then
  begin
    T0 := GetTickCount64;
    Log('Player: extraindo %d audio tracks de %s',
      [TrackCount, ExtractFileName(APath)]);
    if not ExtractAudioTracks(APath, TrackFiles) then
    begin
      Log('Player: extract audio falhou.');
      Exit;
    end;
    Log('Player: extract em %dms.', [GetTickCount64 - T0]);
  end;

  // Monta URLs (todas ja registradas no token map via MakeUrl).
  SetLength(AUrls, TrackCount);
  for i := 0 to TrackCount - 1 do
  begin
    TrackFile := TrackFiles[i];
    AUrls[i] := MakeUrl(Format('-a%d', [i]), TrackFile, '.m4a');
  end;
  Result := True;
end;

// =====================================================================
// HTTP server: serve o arquivo de cache com Range
// =====================================================================

procedure ServeFileWithRange(AReq: TIdHTTPRequestInfo;
  AResp: TIdHTTPResponseInfo; const AFilePath: string);
var
  FS: TFileStream;
  TotalSize, RangeStart, RangeEnd, ContentLen, SuffixLen: Int64;
  RangeHdr, S, StartStr: string;
  P: Integer;
begin
  if not FileExists(AFilePath) then
  begin
    AResp.ResponseNo := 404;
    AResp.ContentText := 'not found';
    Exit;
  end;

  FS := TFileStream.Create(AFilePath, fmOpenRead or fmShareDenyWrite);
  try
    TotalSize := FS.Size;
    RangeStart := 0;
    RangeEnd := TotalSize - 1;

    RangeHdr := AReq.RawHeaders.Values['Range'];
    if (RangeHdr <> '') and StartsText('bytes=', RangeHdr) then
    begin
      // formato "bytes=START-END", "bytes=START-" ou "bytes=-N" (sufixo)
      S := Copy(RangeHdr, 7, MaxInt);
      P := Pos('-', S);
      if P > 0 then
      begin
        StartStr := Copy(S, 1, P - 1);
        if StartStr = '' then
        begin
          // Range de sufixo "bytes=-N" = ultimos N bytes (RFC 7233).
          // Antes era interpretado como start=0 -> servia os bytes errados.
          SuffixLen := StrToInt64Def(Copy(S, P + 1, MaxInt), 0);
          if SuffixLen > 0 then
          begin
            if SuffixLen > TotalSize then SuffixLen := TotalSize;
            RangeStart := TotalSize - SuffixLen;
            RangeEnd := TotalSize - 1;
          end;
        end
        else
        begin
          RangeStart := StrToInt64Def(StartStr, 0);
          if P < Length(S) then
            RangeEnd := StrToInt64Def(Copy(S, P + 1, MaxInt), TotalSize - 1);
        end;
      end;
      if RangeEnd >= TotalSize then RangeEnd := TotalSize - 1;
      if RangeStart > RangeEnd then
      begin
        AResp.ResponseNo := 416;
        AResp.CustomHeaders.Values['Content-Range'] :=
          Format('bytes */%d', [TotalSize]);
        Exit;
      end;
      AResp.ResponseNo := 206;
      AResp.CustomHeaders.Values['Content-Range'] :=
        Format('bytes %d-%d/%d', [RangeStart, RangeEnd, TotalSize]);
    end
    else
      AResp.ResponseNo := 200;

    ContentLen := RangeEnd - RangeStart + 1;
    // Content-Type por extensao — Chromium e mais permissivo se vier
    // o tipo certo. video/x-matroska pra .mkv, video/mp4 pra .mp4 etc.
    case IndexStr(LowerCase(ExtractFileExt(AFilePath)),
                  ['.mp4', '.m4v', '.mkv', '.webm', '.mov', '.jpg',
                   '.jpeg', '.png', '.m4a', '.aac']) of
      0, 1: AResp.ContentType := 'video/mp4';
      2:    AResp.ContentType := 'video/x-matroska';
      3:    AResp.ContentType := 'video/webm';
      4:    AResp.ContentType := 'video/quicktime';
      5, 6: AResp.ContentType := 'image/jpeg';
      7:    AResp.ContentType := 'image/png';
      8:    AResp.ContentType := 'audio/mp4';
      9:    AResp.ContentType := 'audio/aac';
    else
      AResp.ContentType := 'application/octet-stream';
    end;
    AResp.CustomHeaders.Values['Accept-Ranges'] := 'bytes';
    AResp.CustomHeaders.Values['Cache-Control'] := 'no-store';
    AResp.ContentLength := ContentLen;

    FS.Position := RangeStart;

    // Indy fecha o ContentStream pra gente.
    AResp.ContentStream := TMemoryStream.Create;
    TMemoryStream(AResp.ContentStream).CopyFrom(FS, ContentLen);
    AResp.ContentStream.Position := 0;
  except
    on E: Exception do
    begin
      FS.Free;
      AResp.ResponseNo := 500;
      AResp.ContentText := 'erro: ' + E.Message;
      Exit;
    end;
  end;
  FS.Free;
end;

procedure ServeMonitorThumb(const AId: string;
  AResp: TIdHTTPResponseInfo);
var
  Jpeg: TBytes;
  MS: TMemoryStream;
begin
  if ThumbLock = nil then
  begin
    AResp.ResponseNo := 503;
    Exit;
  end;
  ThumbLock.Enter;
  try
    if not ThumbMap.TryGetValue(AId, Jpeg) then
    begin
      AResp.ResponseNo := 404;
      AResp.ContentText := 'no thumb';
      Exit;
    end;
  finally
    ThumbLock.Leave;
  end;
  if Length(Jpeg) = 0 then
  begin
    AResp.ResponseNo := 404;
    Exit;
  end;
  MS := TMemoryStream.Create;
  MS.WriteBuffer(Jpeg[0], Length(Jpeg));
  MS.Position := 0;
  AResp.ResponseNo := 200;
  AResp.ContentType := 'image/jpeg';
  AResp.CustomHeaders.Values['Cache-Control'] := 'no-store';
  AResp.ContentLength := MS.Size;
  AResp.ContentStream := MS;
end;

procedure TPlayerServerHandler.HandleGet(AContext: TIdContext;
  ARequest: TIdHTTPRequestInfo; AResponse: TIdHTTPResponseInfo);
var
  Doc, Token, FilePath: string;
  P: Integer;
begin
  Doc := ARequest.Document;

  // CORS pra todos os responses. WebView2 com NavigateToString tem
  // origin "null"; sem ACAO=*, MediaElementAudioSourceNode marca o
  // recurso como "tainted" e o GainNode produz SILENCIO (mesmo que
  // o <video>/<audio> sem Web Audio tocasse normalmente). Setamos
  // crossOrigin="anonymous" no JS — daqui o browser exige header
  // de CORS no response. '*' satisfaz any-origin pra requests sem
  // credenciais (que e o caso, tudo aqui e localhost sem cookie).
  // Range requests passam transparente — Chromium so usa ACAO pra
  // habilitar Web Audio, nao afeta o playback regular.
  AResponse.CustomHeaders.Values['Access-Control-Allow-Origin'] := '*';

  // /thumb/<id>.jpg — thumbs volateis de monitor
  if StartsText('/thumb/', Doc) then
  begin
    Token := Copy(Doc, 8, MaxInt);
    P := Pos('.', Token);
    if P > 0 then Token := Copy(Token, 1, P - 1);
    ServeMonitorThumb(Token, AResponse);
    Exit;
  end;

  // /v/<token>.ext — arquivos registrados
  if not StartsText('/v/', Doc) then
  begin
    AResponse.ResponseNo := 404;
    AResponse.ContentText := 'not found';
    Exit;
  end;
  Token := Copy(Doc, 4, MaxInt);
  P := Pos('.', Token);
  if P > 0 then Token := Copy(Token, 1, P - 1);

  // Guard de shutdown: HandleGet roda numa worker thread do Indy. Se uma
  // requisicao chega entre Server.Active:=False e o FreeAndNil(TokenLock)
  // do StopPlayerServer, usar o lock liberado causa AV. Mesmo padrao de
  // ServeMonitorThumb (que ja checa ThumbLock=nil).
  if (TokenLock = nil) or (TokenMap = nil) then
  begin
    AResponse.ResponseNo := 503;
    Exit;
  end;

  TokenLock.Enter;
  try
    if not TokenMap.TryGetValue(Token, FilePath) then FilePath := '';
  finally
    TokenLock.Leave;
  end;

  if FilePath = '' then
  begin
    AResponse.ResponseNo := 404;
    AResponse.ContentText := 'unknown token';
    Exit;
  end;

  ServeFileWithRange(ARequest, AResponse, FilePath);
end;

// =====================================================================
// Lifecycle
// =====================================================================

procedure StartPlayerServer;
begin
  if Server <> nil then Exit;

  TokenLock := TCriticalSection.Create;
  TokenMap := TDictionary<string, string>.Create;

  ThumbLock := TCriticalSection.Create;
  ThumbMap := TDictionary<string, TBytes>.Create;
  ThumbVersion := 0;

  Handler := TPlayerServerHandler.Create;
  Server := TIdHTTPServer.Create(nil);
  Server.OnCommandGet := Handler.HandleGet;
  // Bindings: deixa Indy escolher porta livre via DefaultPort=0.
  Server.DefaultPort := 0;
  Server.Bindings.Clear;
  with Server.Bindings.Add do
  begin
    IP := '127.0.0.1';
    Port := 0;
  end;
  Server.Active := True;

  // A porta efetiva fica nas Bindings depois do Active=True.
  if Server.Bindings.Count > 0 then
    ServerPort := Server.Bindings[0].Port;

  Log('Player: HTTP server em 127.0.0.1:%d', [ServerPort]);
end;

procedure StopPlayerServer;
begin
  if Server <> nil then
  begin
    try Server.Active := False; except end;
    FreeAndNil(Server);
  end;
  FreeAndNil(Handler);
  FreeAndNil(TokenMap);
  FreeAndNil(TokenLock);
  FreeAndNil(ThumbMap);
  FreeAndNil(ThumbLock);
  ServerPort := 0;
end;

end.

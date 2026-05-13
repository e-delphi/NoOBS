{
  OBSPlayer — servidor HTTP local + ffmpeg pra tocar gravacoes
  dentro do WebView2.

  Por que isso existe:
    WebView2 nao toca arquivos locais via file:// por padrao, e MKV
    com HEVC quase nunca toca direto no Chromium. A solucao e remux
    pra MP4 (rapido, sem reencode quando o codec ja e compativel) e
    servir pelo localhost com suporte a Range (essencial pra seek).

  Fluxo:
    1. Startup: HTTP server sobe em 127.0.0.1:porta-livre.
    2. UI pede play -> GetPlayUrl(path):
       a. Calcula path do cache (<rec-dir>\.cache\<basename>.mp4).
       b. Se nao existe, roda ffmpeg pra remuxar (-c copy) ou
          transcodar (HEVC -> H.264) baseado no codec.
       c. Devolve "http://127.0.0.1:porta/<token>.mp4".
    3. WebView2 pede o arquivo -> server serve com Range.

  Threading: ffmpeg pode ser pesado se transcodar. Execute em worker
  thread; UI fica responsiva.
}
unit OBSPlayer;

interface

uses
  System.SysUtils;

procedure StartPlayerServer;
procedure StopPlayerServer;

// URL "direta" — serve o arquivo original sem transcode. Pode falhar
// no player se o codec nao for suportado pelo WebView2 (ex: HEVC sem
// HEVC Video Extensions instalado). Instantaneo, sem ffmpeg.
function GetDirectUrl(const APath: string): string;

// URL "transcodada" — garante MP4/H.264 jogavel. Pode demorar (ffmpeg).
// Use de worker thread se chamada vier da main pra evitar travar a UI.
function GetTranscodedUrl(const APath: string): string;

// Verifica se ffmpeg.exe existe ao lado do NoOBS.exe.
function FFmpegAvailable: Boolean;

// Garante metadata cacheada (duracao em segundos + thumb JPG) para
// uma gravacao. Roda ffmpeg se ainda nao tem cache. Devolve True se ok.
// Use de worker thread.
function EnsureRecordingMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string): Boolean;

// So le o cache (instantaneo, sem ffmpeg). Devolve duracao=0 e thumb=''
// se nao houver. Pra usar na main thread sem travar.
procedure GetCachedMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string);

// Remove arquivos de cache que nao pertencem a nenhuma das gravacoes
// listadas. ALivePaths sao os paths das gravacoes que ainda existem.
procedure GarbageCollectCache(const ALivePaths: TArray<string>);

implementation

uses
  Winapi.Windows,
  System.Classes,
  System.IOUtils,
  System.Hash,
  System.StrUtils,
  System.Generics.Collections,
  System.SyncObjs,
  IdContext,
  IdCustomHTTPServer,
  IdHTTPServer,
  IdGlobal,
  OBSLog;

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

function FFmpegPath: string;
// ffmpeg.exe mora ao lado do NoOBS.exe (mesma pasta bin\64bit do OBS).
begin
  Result := ExeDir + 'ffmpeg.exe';
  if not FileExists(Result) then Result := '';
end;

function FFmpegAvailable: Boolean;
begin
  Result := FFmpegPath <> '';
end;

// =====================================================================
// ffmpeg helpers
// =====================================================================

function RunFFmpegCapture(const AArgs: string; out AStdErr: string;
  out AExitCode: DWORD): Boolean;
// Roda ffmpeg capturando stderr (onde ele escreve info do arquivo).
const
  BUF_SIZE = 4096;
var
  Sec: TSecurityAttributes;
  ReadH, WriteH: THandle;
  StartInfo: TStartupInfo;
  ProcInfo: TProcessInformation;
  CmdLine: string;
  CmdBuf: array[0..2047] of Char;
  Buf: array[0..BUF_SIZE - 1] of AnsiChar;
  BytesRead: DWORD;
  SS: TStringStream;
begin
  Result := False;
  AStdErr := '';
  AExitCode := 1;

  Sec.nLength := SizeOf(Sec);
  Sec.bInheritHandle := True;
  Sec.lpSecurityDescriptor := nil;
  if not CreatePipe(ReadH, WriteH, @Sec, 0) then Exit;

  // O lado read nao deve ser herdado.
  SetHandleInformation(ReadH, HANDLE_FLAG_INHERIT, 0);

  ZeroMemory(@StartInfo, SizeOf(StartInfo));
  StartInfo.cb := SizeOf(StartInfo);
  StartInfo.dwFlags := STARTF_USESTDHANDLES or STARTF_USESHOWWINDOW;
  StartInfo.wShowWindow := SW_HIDE;
  StartInfo.hStdOutput := WriteH;
  StartInfo.hStdError  := WriteH;
  StartInfo.hStdInput  := GetStdHandle(STD_INPUT_HANDLE);

  CmdLine := '"' + FFmpegPath + '" ' + AArgs;
  StrLCopy(CmdBuf, PChar(CmdLine), High(CmdBuf));

  SS := TStringStream.Create('', TEncoding.UTF8);
  try
    if not CreateProcess(nil, CmdBuf, nil, nil, True,
      CREATE_NO_WINDOW, nil, PChar(ExeDir), StartInfo, ProcInfo) then
    begin
      CloseHandle(ReadH);
      CloseHandle(WriteH);
      Exit;
    end;
    // Fecha o write da nossa parte; do contrario ReadFile bloqueia
    // sempre que o filho fizer flush.
    CloseHandle(WriteH);

    while ReadFile(ReadH, Buf, BUF_SIZE, BytesRead, nil) and (BytesRead > 0) do
      SS.WriteData(@Buf[0], BytesRead);

    WaitForSingleObject(ProcInfo.hProcess, INFINITE);
    GetExitCodeProcess(ProcInfo.hProcess, AExitCode);
    CloseHandle(ProcInfo.hProcess);
    CloseHandle(ProcInfo.hThread);
    CloseHandle(ReadH);

    AStdErr := SS.DataString;
    Result := AExitCode = 0;
  finally
    SS.Free;
  end;
end;

function DetectVideoCodec(const APath: string): string;
// Le o stderr do ffmpeg -i <file>. Procura "Video: <codec>".
var
  StdErr, Lower: string;
  Code: DWORD;
  P, Q: Integer;
begin
  Result := '';
  RunFFmpegCapture('-hide_banner -i "' + APath + '"', StdErr, Code);
  // ffmpeg sai com codigo != 0 quando nao tem output, mas a info ta no stderr.
  Lower := LowerCase(StdErr);
  P := Pos('video: ', Lower);
  if P = 0 then Exit;
  Inc(P, Length('video: '));
  Q := P;
  while (Q <= Length(Lower)) and (Lower[Q] <> ' ') and
        (Lower[Q] <> ',') do
    Inc(Q);
  Result := Copy(Lower, P, Q - P);
end;

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

function EnsureCachedMp4(const APath: string): string;
// Garante que existe um MP4 jogavel e retorna o path do MP4.
var
  CacheDir, CacheFile, Codec, Args: string;
  StdErr: string;
  Code: DWORD;
  SrcSize, CacheSize: Int64;
begin
  Result := '';
  if not FileExists(APath) then Exit;
  if not FFmpegAvailable then
  begin
    Log('Player: ffmpeg.exe nao encontrado — nao da pra preparar cache.');
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

  Codec := DetectVideoCodec(APath);
  Log('Player: codec detectado="%s" para %s', [Codec, ExtractFileName(APath)]);

  // Estrategia:
  //   - h264: remux puro (-c copy). Instantaneo.
  //   - hevc/h265 ou outro: transcoda video pra H.264, copia audio.
  // Sempre +faststart (move moov pro inicio) pra streaming e seek imediato.
  if (Codec = 'h264') or (Codec = 'avc1') then
    Args := '-y -hide_banner -loglevel error -i "' + APath +
      '" -c copy -movflags +faststart "' + CacheFile + '"'
  else
    Args := '-y -hide_banner -loglevel error -i "' + APath +
      '" -c:v libx264 -preset veryfast -crf 22 -c:a aac -b:a 192k ' +
      '-movflags +faststart "' + CacheFile + '"';

  Log('Player: ffmpeg %s', [Args]);
  if not RunFFmpegCapture(Args, StdErr, Code) then
  begin
    Log('Player: ffmpeg falhou (code=%d): %s', [Code, StdErr]);
    Exit;
  end;

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

function ParseDurationFromFFmpeg(const AStdErr: string): Integer;
var
  P, Q: Integer;
  Token, HH, MM, SS: string;
begin
  Result := 0;
  P := Pos('Duration:', AStdErr);
  if P = 0 then Exit;
  Inc(P, Length('Duration:'));
  while (P <= Length(AStdErr)) and (AStdErr[P] = ' ') do Inc(P);
  Q := P;
  while (Q <= Length(AStdErr)) and (AStdErr[Q] <> ',') and
        (AStdErr[Q] <> #13) and (AStdErr[Q] <> #10) do Inc(Q);
  Token := Trim(Copy(AStdErr, P, Q - P));
  if Length(Token) < 7 then Exit;
  HH := Copy(Token, 1, 2);
  MM := Copy(Token, 4, 2);
  SS := Copy(Token, 7, 2);
  Result := StrToIntDef(HH, 0) * 3600 +
            StrToIntDef(MM, 0) * 60 +
            StrToIntDef(SS, 0);
end;

function EnsureRecordingMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string): Boolean;
var
  CacheDir, ThumbFile, DurFile, StdErr, Token, Args: string;
  Code: DWORD;
  Lines: TStringList;
  SeekTs: Integer;
begin
  Result := False;
  ADurationSec := 0;
  AThumbUrl := '';
  if not FileExists(APath) then Exit;
  if not FFmpegAvailable then Exit;

  CacheDir := CacheDirFor(APath);
  Token := HashName(APath);
  ThumbFile := IncludeTrailingPathDelimiter(CacheDir) + Token + '.jpg';
  DurFile   := IncludeTrailingPathDelimiter(CacheDir) + Token + '.dur';

  if FileExists(DurFile) then
  begin
    Lines := TStringList.Create;
    try
      try
        Lines.LoadFromFile(DurFile);
        if Lines.Count > 0 then
          ADurationSec := StrToIntDef(Trim(Lines[0]), 0);
      except end;
    finally
      Lines.Free;
    end;
  end;

  if ADurationSec = 0 then
  begin
    RunFFmpegCapture('-hide_banner -i "' + APath + '"', StdErr, Code);
    ADurationSec := ParseDurationFromFFmpeg(StdErr);
    if ADurationSec > 0 then
      try TFile.WriteAllText(DurFile, IntToStr(ADurationSec)); except end;
  end;

  if not FileExists(ThumbFile) then
  begin
    SeekTs := 1;
    if ADurationSec > 10 then SeekTs := ADurationSec div 10;
    Args := Format(
      '-y -hide_banner -loglevel error -ss %d -i "%s" ' +
      '-frames:v 1 -vf "scale=320:-1" -q:v 5 "%s"',
      [SeekTs, APath, ThumbFile]);
    RunFFmpegCapture(Args, StdErr, Code);
  end;

  if FileExists(ThumbFile) then
    AThumbUrl := MakeUrl('-thumb', ThumbFile, '.jpg');

  Result := (ADurationSec > 0) or (AThumbUrl <> '');
end;

procedure GetCachedMeta(const APath: string;
  out ADurationSec: Integer; out AThumbUrl: string);
var
  CacheDir, ThumbFile, DurFile, Token: string;
  Lines: TStringList;
begin
  ADurationSec := 0;
  AThumbUrl := '';
  if not FileExists(APath) then Exit;
  CacheDir := CacheRootDir;
  if not DirectoryExists(CacheDir) then Exit;

  Token := HashName(APath);
  ThumbFile := IncludeTrailingPathDelimiter(CacheDir) + Token + '.jpg';
  DurFile   := IncludeTrailingPathDelimiter(CacheDir) + Token + '.dur';

  if FileExists(DurFile) then
  begin
    Lines := TStringList.Create;
    try
      try Lines.LoadFromFile(DurFile); except end;
      if Lines.Count > 0 then
        ADurationSec := StrToIntDef(Trim(Lines[0]), 0);
    finally
      Lines.Free;
    end;
  end;

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

// =====================================================================
// HTTP server: serve o arquivo de cache com Range
// =====================================================================

procedure ServeFileWithRange(AReq: TIdHTTPRequestInfo;
  AResp: TIdHTTPResponseInfo; const AFilePath: string);
var
  FS: TFileStream;
  TotalSize, RangeStart, RangeEnd, ContentLen: Int64;
  RangeHdr, S: string;
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
      // formato "bytes=START-END" ou "bytes=START-"
      S := Copy(RangeHdr, 7, MaxInt);
      P := Pos('-', S);
      if P > 0 then
      begin
        RangeStart := StrToInt64Def(Copy(S, 1, P - 1), 0);
        if P < Length(S) then
          RangeEnd := StrToInt64Def(Copy(S, P + 1, MaxInt), TotalSize - 1);
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
                  ['.mp4', '.m4v', '.mkv', '.webm', '.mov', '.jpg', '.jpeg', '.png']) of
      0, 1: AResp.ContentType := 'video/mp4';
      2:    AResp.ContentType := 'video/x-matroska';
      3:    AResp.ContentType := 'video/webm';
      4:    AResp.ContentType := 'video/quicktime';
      5, 6: AResp.ContentType := 'image/jpeg';
      7:    AResp.ContentType := 'image/png';
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

procedure TPlayerServerHandler.HandleGet(AContext: TIdContext;
  ARequest: TIdHTTPRequestInfo; AResponse: TIdHTTPResponseInfo);
var
  Doc, Token, FilePath: string;
  P: Integer;
begin
  Doc := ARequest.Document;

  // Espera /v/<token>.mp4
  if not StartsText('/v/', Doc) then
  begin
    AResponse.ResponseNo := 404;
    AResponse.ContentText := 'not found';
    Exit;
  end;
  Token := Copy(Doc, 4, MaxInt);
  P := Pos('.', Token);
  if P > 0 then Token := Copy(Token, 1, P - 1);

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
  ServerPort := 0;
end;

end.

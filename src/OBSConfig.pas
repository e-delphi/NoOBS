(*
  OBSConfig — preferencias do app salvas em JSON.

  Local: %LOCALAPPDATA%\NoOBS\config.json
  Estrutura atual: { "theme": "dark" | "light" }

  Funciona via get/set tipados; nada de TIniFile pra deixar facil
  expandir (qualquer JSON valido vale).
*)
unit OBSConfig;

interface

uses
  System.JSON;

function GetConfigStr(const AKey, ADefault: string): string;
procedure SetConfigStr(const AKey, AValue: string);
function ConfigFilePath: string;
// Limpa cache em memoria — proxima leitura recarrega do disco.
// Util pra testes que mexem com config.json fora-de-banda.
procedure ResetConfigCache;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.IOUtils,
  System.Classes,
  System.SyncObjs;

var
  ConfigLock: TCriticalSection = nil;
  CachedJson: TJSONObject = nil;

function ConfigDir: string;
var
  Base: string;
begin
  Base := GetEnvironmentVariable('LOCALAPPDATA');
  if Base = '' then Base := GetEnvironmentVariable('APPDATA');
  Result := IncludeTrailingPathDelimiter(Base) + 'NoOBS';
end;

function ConfigFilePath: string;
begin
  Result := IncludeTrailingPathDelimiter(ConfigDir) + 'config.json';
end;

procedure EnsureLoaded;
var
  Content: string;
  Parsed: TJSONValue;
begin
  if CachedJson <> nil then Exit;

  if FileExists(ConfigFilePath) then
  begin
    try
      Content := TFile.ReadAllText(ConfigFilePath, TEncoding.UTF8);
      Parsed := TJSONObject.ParseJSONValue(Content);
      if Parsed is TJSONObject then
        CachedJson := TJSONObject(Parsed)
      else if Parsed <> nil then
        Parsed.Free;
    except
      // arquivo corrompido — descarta e comeca do zero.
      CachedJson := nil;
    end;
  end;

  if CachedJson = nil then
    CachedJson := TJSONObject.Create;
end;

procedure WriteToDisk;
var
  Stream: TFileStream;
  Bytes: TBytes;
  Dir: string;
begin
  if CachedJson = nil then Exit;
  Dir := ConfigDir;
  if not DirectoryExists(Dir) then
    ForceDirectories(Dir);
  try
    Bytes := TEncoding.UTF8.GetBytes(CachedJson.ToJSON);
    Stream := TFileStream.Create(ConfigFilePath, fmCreate);
    try
      if Length(Bytes) > 0 then
        Stream.WriteBuffer(Bytes[0], Length(Bytes));
    finally
      Stream.Free;
    end;
  except
    on E: Exception do ; // falha de IO nao crasha o app
  end;
end;

function GetConfigStr(const AKey, ADefault: string): string;
var
  V: TJSONValue;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    V := CachedJson.GetValue(AKey);
    if V <> nil then
      Result := V.Value
    else
      Result := ADefault;
  finally
    ConfigLock.Leave;
  end;
end;

procedure SetConfigStr(const AKey, AValue: string);
var
  Pair: TJSONPair;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    Pair := CachedJson.RemovePair(AKey);
    if Pair <> nil then Pair.Free;
    CachedJson.AddPair(AKey, AValue);
    WriteToDisk;
  finally
    ConfigLock.Leave;
  end;
end;

procedure ResetConfigCache;
begin
  ConfigLock.Enter;
  try
    if CachedJson <> nil then
      FreeAndNil(TJSONObject(CachedJson));
  finally
    ConfigLock.Leave;
  end;
end;

initialization
  ConfigLock := TCriticalSection.Create;

finalization
  if CachedJson <> nil then
    FreeAndNil(TJSONObject(CachedJson));
  if ConfigLock <> nil then
    FreeAndNil(ConfigLock);

end.

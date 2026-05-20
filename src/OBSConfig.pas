(*
  OBSConfig — preferencias do app salvas em JSON.

  Local: %LOCALAPPDATA%\NoOBS\config.json

  Estrutura:
    {
      "version":   1,
      "theme":     "dark" | "light",
      "recordDir": "<path>",
      "monitors":  { "<index>": bool, ... },
      "mics":      { "<device>": bool, ... },
      "speakers":  { "<device>": bool, ... },
      "webcams":   { "<device>": bool, ... }
    }

  "version" e o discriminador de schema. Se o arquivo nao tem version
  ou e diferente de CURRENT_VERSION, descarta e comeca do zero —
  evita migrar configs incompativeis. Futuras evolucoes incrementam o
  numero (e, se desejar, podem implementar upgrade aqui).

  Acesso via get/set tipados. Boolean armazenado como TJSONBool real.
  Saida pretty-printed pra inspecao manual.
*)
unit OBSConfig;

interface

uses
  System.JSON;

// Strings de topo: theme, recordDir.
function GetConfigStr(const AKey, ADefault: string): string;
procedure SetConfigStr(const AKey, AValue: string);

function GetConfigBool(const AKey: string; ADefault: Boolean): Boolean;
procedure SetConfigBool(const AKey: string; AValue: Boolean);

// Toggle de source: ACategory = 'monitors'/'mics'/'speakers'/'webcams',
// AId = indice (monitor) ou nome do dispositivo.
function GetSourceBool(const ACategory, AId: string;
  ADefault: Boolean): Boolean;
procedure SetSourceBool(const ACategory, AId: string; AValue: Boolean);

function ConfigFilePath: string;

// Limpa cache em memoria — proxima leitura recarrega do disco.
procedure ResetConfigCache;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.IOUtils,
  System.Classes,
  System.SyncObjs;

const
  // Incrementar quando mudar o schema do config.json de forma
  // incompativel. Arquivos com version != CURRENT_VERSION sao
  // descartados na carga (user reconfigura).
  CURRENT_VERSION = 1;

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

// ----------------------------------------------------------------------
// Pretty-print
// ----------------------------------------------------------------------

function PrettyJson(AObj: TJSONObject): string;
begin
  // TJSONAncestor.Format(2) gera output indentado.
  // (Caracteres nao-ASCII viram \uXXXX por design do System.JSON —
  // legibilidade do JSON em si nao e prejudicada, parser le de volta
  // normalmente.)
  if AObj = nil then Exit('{}');
  try
    Result := AObj.Format(2);
  except
    Result := AObj.ToJSON;
  end;
end;

// ----------------------------------------------------------------------
// Load / save
// ----------------------------------------------------------------------

procedure WriteToDisk; forward;

function GetVersion(AObj: TJSONObject): Integer;
var V: TJSONValue;
begin
  Result := 0;
  if AObj = nil then Exit;
  V := AObj.GetValue('version');
  if V is TJSONNumber then Result := TJSONNumber(V).AsInt;
end;

procedure EnsureLoaded;
var
  Content: string;
  Parsed: TJSONValue;
  CreatedFresh: Boolean;
begin
  if CachedJson <> nil then Exit;

  CreatedFresh := False;

  // Tenta carregar. Qualquer falha (parse, IO, version incompativel)
  // descarta e comeca zerado.
  if FileExists(ConfigFilePath) then
  begin
    try
      Content := TFile.ReadAllText(ConfigFilePath, TEncoding.UTF8);
      Parsed := TJSONObject.ParseJSONValue(Content);
      if Parsed is TJSONObject then
      begin
        if GetVersion(TJSONObject(Parsed)) = CURRENT_VERSION then
          CachedJson := TJSONObject(Parsed)
        else
          Parsed.Free; // schema incompativel, descarta
      end
      else if Parsed <> nil then
        Parsed.Free;
    except
      CachedJson := nil;
    end;
  end;

  if CachedJson = nil then
  begin
    CachedJson := TJSONObject.Create;
    CachedJson.AddPair('version', TJSONNumber.Create(CURRENT_VERSION));
    CreatedFresh := True;
  end;

  // Sobrescreve o arquivo antigo (incompativel/corrompido) com o
  // novo formato zerado. Sem isso, toda inicializacao detectaria o
  // mesmo arquivo invalido ate o user tocar em algo.
  if CreatedFresh then WriteToDisk;
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
    Bytes := TEncoding.UTF8.GetBytes(PrettyJson(CachedJson));
    Stream := TFileStream.Create(ConfigFilePath, fmCreate);
    try
      if Length(Bytes) > 0 then
        Stream.WriteBuffer(Bytes[0], Length(Bytes));
    finally
      Stream.Free;
    end;
  except
    on E: Exception do ;
  end;
end;

// ----------------------------------------------------------------------
// Public API
// ----------------------------------------------------------------------

function GetConfigStr(const AKey, ADefault: string): string;
var
  V: TJSONValue;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    V := CachedJson.GetValue(AKey);
    if (V <> nil) and (V is TJSONString) then
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

function GetConfigBool(const AKey: string; ADefault: Boolean): Boolean;
var
  V: TJSONValue;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    V := CachedJson.GetValue(AKey);
    if V is TJSONBool then
      Result := TJSONBool(V).AsBoolean
    else
      Result := ADefault;
  finally
    ConfigLock.Leave;
  end;
end;

procedure SetConfigBool(const AKey: string; AValue: Boolean);
var
  Pair: TJSONPair;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    Pair := CachedJson.RemovePair(AKey);
    if Pair <> nil then Pair.Free;
    CachedJson.AddPair(AKey, TJSONBool.Create(AValue));
    WriteToDisk;
  finally
    ConfigLock.Leave;
  end;
end;

function GetSourceBool(const ACategory, AId: string;
  ADefault: Boolean): Boolean;
var
  Cat: TJSONValue;
  V: TJSONValue;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    Cat := CachedJson.GetValue(ACategory);
    if not (Cat is TJSONObject) then Exit(ADefault);
    V := TJSONObject(Cat).GetValue(AId);
    if V = nil then Exit(ADefault);
    if V is TJSONBool then Exit(TJSONBool(V).AsBoolean);
    Result := ADefault;
  finally
    ConfigLock.Leave;
  end;
end;

procedure SetSourceBool(const ACategory, AId: string; AValue: Boolean);
var
  CatJson: TJSONValue;
  CatObj: TJSONObject;
  Pair: TJSONPair;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    CatJson := CachedJson.GetValue(ACategory);
    if CatJson is TJSONObject then
      CatObj := TJSONObject(CatJson)
    else
    begin
      // Remove valor invalido (string solta etc.) e cria objeto novo.
      Pair := CachedJson.RemovePair(ACategory);
      if Pair <> nil then Pair.Free;
      CatObj := TJSONObject.Create;
      CachedJson.AddPair(ACategory, CatObj);
    end;
    Pair := CatObj.RemovePair(AId);
    if Pair <> nil then Pair.Free;
    CatObj.AddPair(AId, TJSONBool.Create(AValue));
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
  // Carrega proativamente no startup do processo — assim o check de
  // version (e o reset zerado em caso de incompatibilidade) acontece
  // antes de qualquer codigo tocar nas configs. Sem isso, dependeriamos
  // do primeiro Get/Set tardio pra disparar o EnsureLoaded.
  try EnsureLoaded; except end;

finalization
  if CachedJson <> nil then
    FreeAndNil(TJSONObject(CachedJson));
  if ConfigLock <> nil then
    FreeAndNil(ConfigLock);

end.

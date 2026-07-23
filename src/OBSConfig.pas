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

function GetConfigInt(const AKey: string; ADefault: Integer): Integer;
procedure SetConfigInt(const AKey: string; AValue: Integer);

// Toggle de source: ACategory = 'monitors'/'mics'/'speakers'/'webcams',
// AId = indice (monitor) ou nome do dispositivo.
function GetSourceBool(const ACategory, AId: string;
  ADefault: Boolean): Boolean;
procedure SetSourceBool(const ACategory, AId: string; AValue: Boolean);

// "Oculto": o dispositivo some da lista da tela inicial E nao entra na
// gravacao. Namespace paralelo ('hidden_mics' etc.) de proposito — ocultar
// NAO mexe no enabled, entao desocultar devolve o device com a mesma
// preferencia de gravacao que ele tinha antes.
function GetSourceHidden(const ACategory, AId: string): Boolean;
procedure SetSourceHidden(const ACategory, AId: string; AValue: Boolean);
// "Ativo pra gravacao" = marcado E nao oculto. Os caminhos de gravacao
// (OBSEngine/OBSScene) devem usar ESTA funcao, nunca GetSourceBool direto:
// senao um device oculto continuaria sendo gravado invisivelmente.
function GetSourceActive(const ACategory, AId: string;
  ADefault: Boolean): Boolean;

// Perfil de gravacao AUTOMATICA: subconjunto dos dispositivos habilitados
// que entram na gravacao quando ela e disparada pelo monitor de microfone
// (pegadinha #47). Namespace paralelo 'auto_<categoria>', espelho do
// 'hidden_<categoria>'. Default True: enquanto o usuario nao desmarca nada,
// a auto-gravacao grava exatamente o que a manual gravaria.
function GetSourceAutoRecord(const ACategory, AId: string): Boolean;
procedure SetSourceAutoRecord(const ACategory, AId: string; AValue: Boolean);
// "Ativo na auto-gravacao" = GetSourceActive E marcado no perfil auto. A
// engine usa ESTA no lugar de GetSourceActive quando a gravacao nasceu do
// watcher de mic.
function GetSourceActiveForAuto(const ACategory, AId: string;
  ADefault: Boolean): Boolean;

function ConfigFilePath: string;

// Limpa cache em memoria — proxima leitura recarrega do disco.
procedure ResetConfigCache;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.IOUtils,
  System.Classes,
  System.SyncObjs,
  OBSLog;

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

function JsonNumAsIntClamped(V: TJSONValue; ADefault: Integer): Integer;
// Le um TJSONNumber como Integer com range-check. AsInt cru trunca/faz
// wrap silencioso de valores fora de Int32 (ex.: "recordingFps":
// 9999999999 viraria lixo, ate negativo). Le como Int64 e clampa; se a
// conversao falhar (numero gigante/fracionario invalido), devolve default.
begin
  Result := ADefault;
  if not (V is TJSONNumber) then Exit;
  try
    var I64: Int64 := TJSONNumber(V).AsInt64;
    if I64 < Low(Integer) then Result := Low(Integer)
    else if I64 > High(Integer) then Result := High(Integer)
    else Result := Integer(I64);
  except
    Result := ADefault;
  end;
end;

function GetVersion(AObj: TJSONObject): Integer;
begin
  Result := 0;
  if AObj = nil then Exit;
  Result := JsonNumAsIntClamped(AObj.GetValue('version'), 0);
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
  Dir, FinalPath, TmpPath: string;
begin
  if CachedJson = nil then Exit;
  Dir := ConfigDir;
  if not DirectoryExists(Dir) then
    ForceDirectories(Dir);
  FinalPath := ConfigFilePath;
  TmpPath := FinalPath + '.tmp';
  try
    Bytes := TEncoding.UTF8.GetBytes(PrettyJson(CachedJson));
    // Escreve num temp e renomeia por cima (atomico via MoveFileEx) — sem
    // isso, uma escrita interrompida deixava config.json truncado, e o
    // fmCreate exclusivo colidia silenciosamente quando full + /hibernate
    // gravavam juntos (pegadinha #36). Falha agora e LOGADA, nao engolida.
    Stream := TFileStream.Create(TmpPath, fmCreate);
    try
      if Length(Bytes) > 0 then
        Stream.WriteBuffer(Bytes[0], Length(Bytes));
    finally
      Stream.Free;
    end;
    if not MoveFileEx(PChar(TmpPath), PChar(FinalPath),
         MOVEFILE_REPLACE_EXISTING or MOVEFILE_WRITE_THROUGH) then
      raise Exception.CreateFmt('MoveFileEx falhou (err=%d)', [GetLastError]);
  except
    on E: Exception do
    begin
      Log('OBSConfig: falha ao gravar "%s": %s', [FinalPath, E.Message]);
      try if FileExists(TmpPath) then TFile.Delete(TmpPath); except end;
    end;
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

function GetConfigInt(const AKey: string; ADefault: Integer): Integer;
var
  V: TJSONValue;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    V := CachedJson.GetValue(AKey);
    Result := JsonNumAsIntClamped(V, ADefault);
  finally
    ConfigLock.Leave;
  end;
end;

procedure SetConfigInt(const AKey: string; AValue: Integer);
var
  Pair: TJSONPair;
begin
  ConfigLock.Enter;
  try
    EnsureLoaded;
    Pair := CachedJson.RemovePair(AKey);
    if Pair <> nil then Pair.Free;
    CachedJson.AddPair(AKey, TJSONNumber.Create(AValue));
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

// Prefixo da categoria paralela de ocultos: 'mics' -> 'hidden_mics'.
function HiddenCategory(const ACategory: string): string;
begin
  Result := 'hidden_' + ACategory;
end;

function GetSourceHidden(const ACategory, AId: string): Boolean;
begin
  Result := GetSourceBool(HiddenCategory(ACategory), AId, False);
end;

procedure SetSourceHidden(const ACategory, AId: string; AValue: Boolean);
begin
  SetSourceBool(HiddenCategory(ACategory), AId, AValue);
end;

function GetSourceActive(const ACategory, AId: string;
  ADefault: Boolean): Boolean;
begin
  // Chamadas sequenciais (nao aninhadas) — cada uma pega/solta o ConfigLock.
  Result := GetSourceBool(ACategory, AId, ADefault) and
            not GetSourceHidden(ACategory, AId);
end;

// Prefixo da categoria paralela do perfil de auto-gravacao:
// 'mics' -> 'auto_mics'. Espelho de HiddenCategory.
function AutoRecordCategory(const ACategory: string): string;
begin
  Result := 'auto_' + ACategory;
end;

function GetSourceAutoRecord(const ACategory, AId: string): Boolean;
begin
  // Default True: o dispositivo habilitado entra no perfil auto ate o
  // usuario desmarca-lo explicitamente na aba Comportamento.
  Result := GetSourceBool(AutoRecordCategory(ACategory), AId, True);
end;

procedure SetSourceAutoRecord(const ACategory, AId: string; AValue: Boolean);
begin
  SetSourceBool(AutoRecordCategory(ACategory), AId, AValue);
end;

function GetSourceActiveForAuto(const ACategory, AId: string;
  ADefault: Boolean): Boolean;
begin
  // Ativo na auto-gravacao = NAO oculto (menu Dispositivos) E marcado no
  // perfil auto (aba Comportamento). NAO exige o 'enabled' da tela inicial:
  // o perfil de auto-gravacao e uma selecao propria, independente da
  // gravacao manual. ADefault (default do enabled manual) e ignorado de
  // proposito — o default do perfil auto e True (ver GetSourceAutoRecord).
  Result := (not GetSourceHidden(ACategory, AId)) and
            GetSourceAutoRecord(ACategory, AId);
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

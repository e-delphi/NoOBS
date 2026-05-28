(*
  OBSLang - sistema de internacionalizacao (i18n) do NoOBS.

  Formato:
    JSON aninhado por idioma em lang\<code>.json. Padrao alinhado com
    o i18next:
      - chaves com namespace via dot-notation ('settings.title')
      - interpolacao via {{var}} no valor
      - fallback automatico pro idioma definido em meta.fallback
      - chave inexistente retorna a propria chave (sinalizador visual)

  Localizacao dos arquivos:
    Sempre <ExeDir>\lang\<code>.json — em dev e em producao. A propria
    pasta exe\bin\64bit\lang\ no repo e a fonte de verdade (nao existe
    duplicata na raiz). NoOBS.exe roda de la, instalador copia de la.

  Estado:
    LoadLanguage(code) carrega um idioma na memoria. CurrentLanguage()
    retorna o codigo ativo. T(key) faz lookup com interpolacao.
    AvailableLanguages enumera arquivos na pasta lang.

  Detecao automatica:
    Primeira execucao (config sem 'language') usa GetUserDefaultLocaleName
    do Windows pra escolher o idioma — match exato ('pt-BR'), depois por
    prefixo de 2 letras ('pt' -> 'pt-BR'), fallback final pra 'en'.

  Backend vs UI:
    O Delphi consome via T(key). O JSON inteiro do idioma atual e
    serializado pra UI via OBSBridge.PushInit, que o JS usa pra montar
    sua propria funcao T() — single source of truth.
*)
unit OBSLang;

interface

uses
  System.JSON;

// Carrega um idioma na memoria. Retorna False se o arquivo nao existir
// ou tiver JSON invalido (e nao troca o idioma ativo nesse caso).
function LoadLanguage(const ACode: string): Boolean;

// Auto-detecta + carrega: usa config 'language' (vazia = sistema), com
// fallback pro Windows locale e por fim 'en'.
procedure InitLanguage;

// Codigo do idioma atualmente carregado (ex.: 'pt-BR', 'en').
function CurrentLanguage: string;

// Faz lookup com dot-notation. Aceita opcoes de interpolacao via
// TArray<string> de pares [chave, valor, chave, valor, ...].
//   T('settings.title')
//   T('record.finished', ['min', '5', 'sec', '32'])
function T(const AKey: string): string; overload;
function T(const AKey: string; const AArgs: array of string): string; overload;

// Retorna o JSON inteiro do idioma atual (clonado — caller libera).
// Usado por OBSBridge pra mandar a tabela pra UI.
function GetCurrentBundle: TJSONObject;

// Enumera idiomas disponiveis lendo a pasta lang\. Retorna array de
// objetos: { code, name, nativeName }.
function GetAvailableLanguages: TJSONArray;

// Pasta onde os arquivos .json moram (resolvida em runtime).
function LangFolder: string;

// Forca recarga da pasta lang (apos editar arquivos sem reiniciar).
procedure ReloadLanguage;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.IOUtils,
  System.Classes,
  System.Generics.Collections,
  System.SyncObjs,
  OBSConfig,
  OBSLog;

const
  DEFAULT_LANGUAGE = 'en';

var
  GLock: TCriticalSection = nil;
  // Bundle ativo (idioma escolhido pelo user, ja resolvido).
  GCurrentBundle: TJSONObject = nil;
  GCurrentCode: string = '';
  // Bundle de fallback — geralmente 'en' — consultado quando GCurrentBundle
  // nao tem a chave. Reaproveitado se o idioma atual ja for 'en'.
  GFallbackBundle: TJSONObject = nil;
  GFallbackCode: string = '';
  // Cache da pasta resolvida (evita ExpandFileName em cada chamada).
  GLangFolderCache: string = '';

// ---------------------------------------------------------------------
// Resolucao de paths
// ---------------------------------------------------------------------

function ResolveLangFolder: string;
// Pasta unica em <ExeDir>\lang\. No repo, exe\bin\64bit\lang\ E a
// pasta source-controlled (sem duplicacao na raiz). NoOBS.exe roda
// daquele diretorio tanto em dev quanto em prod, entao um unico path
// resolve os dois cenarios. Se nao existir, GetAvailableLanguages
// retorna lista vazia e OBSStartupCheck loga aviso.
begin
  Result := IncludeTrailingPathDelimiter(
    IncludeTrailingPathDelimiter(ExtractFilePath(ParamStr(0))) + 'lang');
end;

function LangFolder: string;
begin
  if GLangFolderCache = '' then
    GLangFolderCache := ResolveLangFolder;
  Result := GLangFolderCache;
end;

// ---------------------------------------------------------------------
// Carga de JSON
// ---------------------------------------------------------------------

function LoadJsonFile(const APath: string): TJSONObject;
var
  Content: string;
  Parsed: TJSONValue;
begin
  Result := nil;
  if not FileExists(APath) then Exit;
  try
    Content := TFile.ReadAllText(APath, TEncoding.UTF8);
  except
    on E: Exception do
    begin
      Log('OBSLang: erro lendo "%s": %s', [APath, E.Message]);
      Exit;
    end;
  end;
  try
    Parsed := TJSONObject.ParseJSONValue(Content);
  except
    on E: Exception do
    begin
      Log('OBSLang: parse JSON falhou em "%s": %s', [APath, E.Message]);
      Exit;
    end;
  end;
  if Parsed is TJSONObject then
    Result := TJSONObject(Parsed)
  else
  begin
    if Parsed <> nil then Parsed.Free;
    Log('OBSLang: "%s" nao e objeto JSON na raiz.', [APath]);
  end;
end;

function PathForCode(const ACode: string): string;
begin
  Result := LangFolder + ACode + '.json';
end;

// ---------------------------------------------------------------------
// Detecao de locale do Windows
// ---------------------------------------------------------------------

function DetectWindowsLocale: string;
// Le o locale do user (ex.: 'pt-BR', 'en-US', 'es-ES'). Documentado a
// retornar string BCP-47 (idioma-Regiao).
var
  Buf: array[0..85] of WideChar;
  Len: Integer;
begin
  Result := DEFAULT_LANGUAGE;
  Len := GetUserDefaultLocaleName(@Buf[0], Length(Buf));
  if Len > 0 then
    Result := WideCharToString(@Buf[0]);
end;

function FindMatchingLanguage(const ALocale: string): string;
// Match em 3 etapas:
//   1. Exato: 'pt-BR.json' existe? usa ele.
//   2. Prefixo 2 letras: 'pt-BR' procura 'pt.json' ou qualquer
//      'pt-*.json' (escolhe o primeiro encontrado).
//   3. Fallback final: DEFAULT_LANGUAGE.
var
  Folder, Prefix, Code, FileName: string;
  Files: TArray<string>;
  i: Integer;
begin
  Folder := LangFolder;

  // 1. Exato.
  if FileExists(Folder + ALocale + '.json') then
    Exit(ALocale);

  // 2. Prefixo. 'pt-BR' -> prefixo 'pt'.
  if Length(ALocale) >= 2 then
  begin
    Prefix := LowerCase(Copy(ALocale, 1, 2));
    if FileExists(Folder + Prefix + '.json') then
      Exit(Prefix);
    if DirectoryExists(Folder) then
    begin
      try
        Files := TDirectory.GetFiles(Folder, Prefix + '-*.json');
      except
        SetLength(Files, 0);
      end;
      for i := 0 to High(Files) do
      begin
        FileName := ExtractFileName(Files[i]);
        Code := ChangeFileExt(FileName, '');
        Exit(Code);
      end;
    end;
  end;

  // 3. Default.
  Result := DEFAULT_LANGUAGE;
end;

// ---------------------------------------------------------------------
// Lookup com dot-notation
// ---------------------------------------------------------------------

function LookupKey(ABundle: TJSONObject; const AKey: string): TJSONValue;
// Resolve 'settings.quality.hint.0' descendo na arvore JSON.
var
  Parts: TArray<string>;
  i: Integer;
  Current: TJSONValue;
  Obj: TJSONObject;
begin
  Result := nil;
  if (ABundle = nil) or (AKey = '') then Exit;
  Parts := AKey.Split(['.']);
  Current := ABundle;
  for i := 0 to High(Parts) do
  begin
    if not (Current is TJSONObject) then Exit(nil);
    Obj := TJSONObject(Current);
    Current := Obj.GetValue(Parts[i]);
    if Current = nil then Exit(nil);
  end;
  Result := Current;
end;

function InterpolateArgs(const ATemplate: string;
  const AArgs: array of string): string;
// Substitui {{nome}} pelos pares (chave, valor) recebidos. Sem args =
// passa direto.
var
  i: Integer;
  Key: string;
begin
  Result := ATemplate;
  i := 0;
  while i + 1 <= High(AArgs) do
  begin
    Key := '{{' + AArgs[i] + '}}';
    Result := StringReplace(Result, Key, AArgs[i + 1], [rfReplaceAll]);
    Inc(i, 2);
  end;
end;

// ---------------------------------------------------------------------
// API publica
// ---------------------------------------------------------------------

function CurrentLanguage: string;
begin
  GLock.Enter;
  try
    Result := GCurrentCode;
  finally
    GLock.Leave;
  end;
end;

function LoadLanguage(const ACode: string): Boolean;
var
  Path, FallbackCode: string;
  Bundle, Fallback: TJSONObject;
  MetaVal: TJSONValue;
  MetaObj: TJSONObject;
begin
  Result := False;
  if ACode = '' then Exit;
  Path := PathForCode(ACode);
  Bundle := LoadJsonFile(Path);
  if Bundle = nil then
  begin
    Log('OBSLang: nao encontrou "%s".', [Path]);
    Exit;
  end;

  // Resolve fallback declarado no meta. Se igual ao atual, nao carrega
  // duplicado (a engine usa o mesmo bundle pra lookups secundarios).
  FallbackCode := DEFAULT_LANGUAGE;
  MetaVal := Bundle.GetValue('meta');
  if MetaVal is TJSONObject then
  begin
    MetaObj := TJSONObject(MetaVal);
    var V := MetaObj.GetValue('fallback');
    if (V <> nil) and (V is TJSONString) then
      FallbackCode := V.Value;
  end;

  Fallback := nil;
  if not SameText(FallbackCode, ACode) then
    Fallback := LoadJsonFile(PathForCode(FallbackCode));

  GLock.Enter;
  try
    if GCurrentBundle <> nil then FreeAndNil(GCurrentBundle);
    if GFallbackBundle <> nil then FreeAndNil(GFallbackBundle);
    GCurrentBundle := Bundle;
    GCurrentCode := ACode;
    GFallbackBundle := Fallback;
    if Fallback <> nil then GFallbackCode := FallbackCode
    else GFallbackCode := '';
  finally
    GLock.Leave;
  end;
  Log('OBSLang: idioma carregado "%s" (fallback="%s").',
    [ACode, GFallbackCode]);
  Result := True;
end;

procedure InitLanguage;
var
  Configured, Detected, Chosen: string;
begin
  Configured := GetConfigStr('language', '');
  if Configured = '' then
  begin
    Detected := DetectWindowsLocale;
    Chosen := FindMatchingLanguage(Detected);
    Log('OBSLang: 1a execucao — locale do Windows="%s" -> idioma="%s".',
      [Detected, Chosen]);
  end
  else
  begin
    // Valor 'auto' tambem cai na detecao do sistema (sem persistir override).
    if SameText(Configured, 'auto') then
    begin
      Detected := DetectWindowsLocale;
      Chosen := FindMatchingLanguage(Detected);
    end
    else
      Chosen := Configured;
  end;
  if not LoadLanguage(Chosen) then
  begin
    // Bundle escolhido falhou — tenta default.
    if not SameText(Chosen, DEFAULT_LANGUAGE) then
    begin
      Log('OBSLang: fallback pra "%s".', [DEFAULT_LANGUAGE]);
      LoadLanguage(DEFAULT_LANGUAGE);
    end;
  end;
end;

procedure ReloadLanguage;
var
  Code: string;
begin
  Code := CurrentLanguage;
  GLangFolderCache := '';
  if Code <> '' then LoadLanguage(Code)
  else InitLanguage;
end;

function T(const AKey: string): string;
begin
  Result := T(AKey, []);
end;

function T(const AKey: string; const AArgs: array of string): string;
var
  Value: TJSONValue;
begin
  GLock.Enter;
  try
    Value := LookupKey(GCurrentBundle, AKey);
    if (Value = nil) and (GFallbackBundle <> nil) then
      Value := LookupKey(GFallbackBundle, AKey);
  finally
    GLock.Leave;
  end;

  if Value is TJSONString then
    Result := TJSONString(Value).Value
  else if Value is TJSONNumber then
    Result := TJSONNumber(Value).ToString
  else if Value is TJSONBool then
    Result := BoolToStr(TJSONBool(Value).AsBoolean, True)
  else
    // Chave nao encontrada — retorna a propria chave entre colchetes pra
    // que o tradutor identifique facil o que falta.
    Exit('[' + AKey + ']');

  if Length(AArgs) > 0 then
    Result := InterpolateArgs(Result, AArgs);
end;

function GetCurrentBundle: TJSONObject;
var
  S: string;
begin
  // Clona via serialize+parse — TJSONObject nao tem Clone util e nao
  // queremos compartilhar o ponteiro com o caller (caller poderia liberar).
  Result := nil;
  GLock.Enter;
  try
    if GCurrentBundle = nil then Exit;
    S := GCurrentBundle.ToJSON;
  finally
    GLock.Leave;
  end;
  try
    var Parsed := TJSONObject.ParseJSONValue(S);
    if Parsed is TJSONObject then
      Result := TJSONObject(Parsed)
    else if Parsed <> nil then
      Parsed.Free;
  except
    Result := nil;
  end;
end;

function GetAvailableLanguages: TJSONArray;
// Enumera lang\*.json e devolve um array com { code, name, nativeName }
// pra UI montar o dropdown. Ordena alfabeticamente pelo code pra ordem
// estavel entre execucoes.
var
  Folder: string;
  Files: TArray<string>;
  i: Integer;
  Code, Path: string;
  Bundle: TJSONObject;
  Meta: TJSONValue;
  MetaObj: TJSONObject;
  Item: TJSONObject;
  NameV, NativeV: TJSONValue;
begin
  Result := TJSONArray.Create;
  Folder := LangFolder;
  if not DirectoryExists(Folder) then
  begin
    Log('OBSLang: pasta lang nao existe em "%s".', [Folder]);
    Exit;
  end;
  try
    Files := TDirectory.GetFiles(Folder, '*.json');
  except
    SetLength(Files, 0);
  end;
  TArray.Sort<string>(Files);
  for i := 0 to High(Files) do
  begin
    Path := Files[i];
    Code := ChangeFileExt(ExtractFileName(Path), '');
    Bundle := LoadJsonFile(Path);
    if Bundle = nil then Continue;
    try
      Item := TJSONObject.Create;
      Item.AddPair('code', Code);
      Meta := Bundle.GetValue('meta');
      if Meta is TJSONObject then
      begin
        MetaObj := TJSONObject(Meta);
        NameV := MetaObj.GetValue('name');
        NativeV := MetaObj.GetValue('nativeName');
        if (NameV <> nil) and (NameV is TJSONString) then
          Item.AddPair('name', NameV.Value)
        else
          Item.AddPair('name', Code);
        if (NativeV <> nil) and (NativeV is TJSONString) then
          Item.AddPair('nativeName', NativeV.Value)
        else
          Item.AddPair('nativeName', Code);
      end
      else
      begin
        Item.AddPair('name', Code);
        Item.AddPair('nativeName', Code);
      end;
      Result.AddElement(Item);
    finally
      Bundle.Free;
    end;
  end;
end;

initialization
  GLock := TCriticalSection.Create;

finalization
  if GCurrentBundle <> nil then FreeAndNil(GCurrentBundle);
  if GFallbackBundle <> nil then FreeAndNil(GFallbackBundle);
  if GLock <> nil then FreeAndNil(GLock);

end.

(*
  OBSStartupCheck - validacao dos arquivos necessarios na inicializacao
  do app. Chamado por OBSUI.Run antes de criar a janela.

  Distincao:
    - CRITICOS: app nao pode rodar sem. Falta → MessageBox + Halt.
    - RECOMENDADOS: feature degrada, mas app sobe. Falta → log.

  Layout esperado (relativo ao .exe):
    NoOBS.exe (em <root>\exe\bin\64bit\)
    obs.dll, WebView2Loader.dll, avcodec-*.dll, avformat-*.dll,
    avutil-*.dll, swscale-*.dll, libobs-d3d11.dll, ...
    ..\..\obs-plugins\64bit\*.dll
    ..\..\data\libobs\
    ..\..\data\obs-plugins\<plugin>\
*)
unit OBSStartupCheck;

interface

type
  TMissingKind = (mkCritical, mkRecommended);

  TMissingFile = record
    Path: string;
    Kind: TMissingKind;
    Reason: string;
  end;

  TMissingFileArray = TArray<TMissingFile>;

// Verifica todos os arquivos esperados. Retorna lista vazia se tudo OK.
function ValidateRuntime: TMissingFileArray;

// Conveniencia: roda ValidateRuntime, separa criticos/recomendados,
// loga recomendados, mostra MessageBox + Halt se houver critico faltando.
// Retorna False se algum critico faltou (caller deve sair sem criar janela).
function EnforceRuntime: Boolean;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.IOUtils,
  OBSLog;

function ExeDir: string;
begin
  Result := ExtractFilePath(ParamStr(0));
end;

function ResolveRel(const ARelative: string): string;
begin
  Result := ExpandFileName(ExeDir + ARelative);
end;

procedure AddMissing(var AArr: TMissingFileArray; const APath, AReason: string;
  AKind: TMissingKind);
var
  Entry: TMissingFile;
begin
  Entry.Path := APath;
  Entry.Reason := AReason;
  Entry.Kind := AKind;
  SetLength(AArr, Length(AArr) + 1);
  AArr[High(AArr)] := Entry;
end;

procedure CheckFile(var AArr: TMissingFileArray;
  const APath, AReason: string; AKind: TMissingKind);
begin
  if not FileExists(APath) then
    AddMissing(AArr, APath, AReason, AKind);
end;

procedure CheckDir(var AArr: TMissingFileArray;
  const APath, AReason: string; AKind: TMissingKind);
begin
  if not DirectoryExists(APath) then
    AddMissing(AArr, APath, AReason, AKind);
end;

function ValidateRuntime: TMissingFileArray;
var
  PluginBin, PluginData, DataLibobs, LangDir: string;
begin
  SetLength(Result, 0);

  // -- Criticos: app nao funciona sem -------------------------------------
  CheckFile(Result, ExeDir + 'obs.dll',
    'Core libobs — sem ele nao da pra gravar.', mkCritical);
  CheckFile(Result, ExeDir + 'WebView2Loader.dll',
    'Loader do WebView2 — UI nao abre sem isso.', mkCritical);

  // Bibliotecas FFmpeg — caminho unico pra todas as operacoes de
  // media (probe/remux/extract/thumb). Sem elas o app nao consegue
  // ler gravacoes nem gerar thumbs/info. Nao temos fallback pra exe.
  CheckFile(Result, ExeDir + 'avcodec-61.dll',
    'avcodec — decoders/encoders. Sem isso nada de media funciona.',
    mkCritical);
  CheckFile(Result, ExeDir + 'avformat-61.dll',
    'avformat — leitura/escrita de containers (MKV/MP4). Critico.',
    mkCritical);
  CheckFile(Result, ExeDir + 'avutil-59.dll',
    'avutil — utilitarios comuns do FFmpeg.', mkCritical);
  CheckFile(Result, ExeDir + 'swscale-8.dll',
    'swscale — conversao de pixel format/scale (necessario p/ thumbs).',
    mkCritical);

  // Pasta de data do libobs (effects, locale).
  DataLibobs := ResolveRel('..\..\data\libobs');
  CheckDir(Result, DataLibobs,
    'Data do libobs (shaders/effects). Sem isso o video nao renderiza.',
    mkCritical);

  // Pasta de traducoes — unica, sempre em <ExeDir>\lang\ (sem fallback
  // de dev: a propria pasta exe\bin\64bit\lang\ no repo e a fonte de
  // verdade). Recomendada, nao critica — sem ela o app sobe com chaves
  // literais ("[settings.title]") mas ainda funciona.
  LangDir := ExeDir + 'lang';
  if not DirectoryExists(LangDir) then
    AddMissing(Result, LangDir,
      'Pasta de traducoes (lang\). Sem ela o app exibe chaves brutas — UI ' +
      'continua funcional mas sem textos traduzidos.', mkRecommended);

  // -- Recomendados: feature degrada se faltar ----------------------------
  PluginBin  := ResolveRel('..\..\obs-plugins\64bit');
  PluginData := ResolveRel('..\..\data\obs-plugins');

  CheckDir(Result, PluginBin,
    'Pasta dos plugins — sem ela nada de captura/encoder funciona.',
    mkCritical);
  CheckDir(Result, PluginData,
    'Data dos plugins — config padrao de cada plugin.', mkRecommended);

  // Plugins individuais — cada um habilita uma feature.
  CheckFile(Result, IncludeTrailingPathDelimiter(PluginBin) + 'win-wasapi.dll',
    'Captura de audio (WASAPI). Sem isso gravacao fica muda.',
    mkRecommended);
  CheckFile(Result, IncludeTrailingPathDelimiter(PluginBin) + 'win-capture.dll',
    'Captura de monitor. Sem isso so grava audio.', mkRecommended);
  CheckFile(Result, IncludeTrailingPathDelimiter(PluginBin) + 'win-dshow.dll',
    'Captura de webcam. Sem isso webcam nao aparece.', mkRecommended);
  CheckFile(Result, IncludeTrailingPathDelimiter(PluginBin) + 'obs-ffmpeg.dll',
    'Muxer/encoder de audio. Sem isso o arquivo final nao escreve.',
    mkCritical);
  CheckFile(Result, IncludeTrailingPathDelimiter(PluginBin) + 'obs-x264.dll',
    'Encoder de video fallback (CPU). Sem ele e sem NVENC, nao grava.',
    mkRecommended);
end;

function EnforceRuntime: Boolean;
var
  Missing: TMissingFileArray;
  HasCritical: Boolean;
  i: Integer;
  MsgText, KindLbl: string;
  Box: UINT;
begin
  Result := True;
  Missing := ValidateRuntime;
  if Length(Missing) = 0 then
  begin
    Log('Startup: todos os arquivos esperados encontrados.');
    Exit;
  end;

  HasCritical := False;
  Log('Startup: %d arquivo(s) faltando:', [Length(Missing)]);
  for i := 0 to High(Missing) do
  begin
    if Missing[i].Kind = mkCritical then KindLbl := 'CRITICO'
    else KindLbl := 'aviso';
    Log('   [%s] %s — %s',
      [KindLbl, Missing[i].Path, Missing[i].Reason]);
    if Missing[i].Kind = mkCritical then
      HasCritical := True;
  end;

  if not HasCritical then Exit;

  MsgText := 'NoOBS nao pode iniciar — arquivos essenciais nao foram ' +
             'encontrados na pasta do app:' + sLineBreak + sLineBreak;
  for i := 0 to High(Missing) do
  begin
    if Missing[i].Kind <> mkCritical then Continue;
    MsgText := MsgText + '  - ' + ExtractFileName(Missing[i].Path) +
      sLineBreak;
  end;
  MsgText := MsgText + sLineBreak +
    'Confirme que o app foi extraido completo (incluindo as pastas ' +
    'obs-plugins\ e data\). Detalhes em ' +
    '%LOCALAPPDATA%\NoOBS\NoOBS.log.';

  Box := MB_OK or MB_ICONERROR or MB_TOPMOST;
  MessageBox(0, PChar(MsgText), 'NoOBS - Arquivos ausentes', Box);
  Result := False;
end;

end.

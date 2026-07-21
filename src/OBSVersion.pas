{
  OBSVersion — versão do app e comparação semântica de versões.

  A versão vem de src\Version.inc, GERADO pelo gen-version.bat (pre-build)
  a partir de `git describe --tags`. Nunca editar Version.inc na mão: ele
  é sobrescrito a cada build e está no .gitignore.

  Formatos possíveis vindos do git describe:
    v0.15.0-beta                  release: build exatamente na tag, limpo
    v0.15.0-beta-1-gd7e4f73       1 commit depois da tag
    v0.15.0-beta-1-gd7e4f73-dirty ainda com alterações não commitadas
    d7e4f73                       repo sem tags (--always) — NÃO é versão

  IsDevBuild detecta os 3 últimos casos. O OBSUpdate usa isso pra NÃO
  checar atualização em build de desenvolvimento: esse build está à frente
  da última tag por definição, então avisar "há versão nova" seria sempre
  falso positivo. É essa regra que torna impossível o app ficar avisando
  à toa por versão desatualizada — o build "esquecido" é justamente o sujo.
}
unit OBSVersion;

interface

// Versão crua como o git describe devolveu (ex.: 'v0.15.0-beta').
function CurrentVersion: string;
// Versão pra exibir na UI: sem o 'v' inicial; build de dev mantém o sufixo
// pra ficar evidente que não é um release.
function DisplayVersion: string;
// True se o build NÃO está exatamente numa tag limpa (ou não há versão).
function IsDevBuild: Boolean;
// -1 se A < B, 0 se iguais, 1 se A > B. Compara MAJOR.MINOR.PATCH numérico
// e trata pré-lançamento pela regra semver (1.0.0-beta < 1.0.0).
function CompareVersions(const A, B: string): Integer;
// True se ARemote é mais novo que a versão atual. False em qualquer dúvida
// (versão local inválida, build de dev, string vazia) — nunca avisa por erro.
function IsRemoteNewer(const ARemote: string): Boolean;

implementation

uses
  System.SysUtils, System.StrUtils;

{$I Version.inc}   // declara: const APP_VERSION = '...';

type
  TSemVer = record
    Major, Minor, Patch: Integer;
    PreRelease: string;   // '' = release final
    Valid: Boolean;
  end;

function CurrentVersion: string;
begin
  Result := APP_VERSION;
end;

// Sufixo do git describe: '-<n>-g<hash>' e/ou '-dirty'.
function HasDescribeSuffix(const S: string): Boolean;
var
  i, p: Integer;
  Seg: string;
  Parts: TArray<string>;
begin
  if EndsText('-dirty', S) then Exit(True);
  // Procura um segmento no formato 'g<hex>' precedido por um segmento
  // numérico — o par que o git describe acrescenta fora da tag.
  Parts := S.Split(['-']);
  for i := 1 to High(Parts) do
  begin
    Seg := Parts[i];
    if (Length(Seg) >= 2) and (Seg[1] = 'g') then
    begin
      // o anterior precisa ser só dígitos (contagem de commits)
      p := 0;
      if TryStrToInt(Parts[i - 1], p) then Exit(True);
    end;
  end;
  Result := False;
end;

// Aceita 'v1.2.3', '1.2.3', '1.2.3-beta'. Ignora sufixo de describe.
function ParseSemVer(const S: string): TSemVer;
var
  T, Core, Pre: string;
  Parts: TArray<string>;
begin
  Result := Default(TSemVer);
  T := Trim(S);
  if T = '' then Exit;
  if StartsText('v', T) then Delete(T, 1, 1);

  // Remove o sufixo do describe pra não poluir o pré-lançamento.
  if EndsText('-dirty', T) then
    T := Copy(T, 1, Length(T) - Length('-dirty'));
  Parts := T.Split(['-']);
  // Reconstrói: core = Parts[0]; pré-lançamento = os segmentos seguintes
  // até (exclusive) o par '<n>-g<hash>'.
  Core := Parts[0];
  Pre := '';
  var i := 1;
  while i <= High(Parts) do
  begin
    var n: Integer;
    if (i + 1 <= High(Parts)) and TryStrToInt(Parts[i], n) and
       (Length(Parts[i + 1]) >= 2) and (Parts[i + 1][1] = 'g') then
      Break;   // daqui pra frente é sufixo do describe
    if Pre <> '' then Pre := Pre + '-';
    Pre := Pre + Parts[i];
    Inc(i);
  end;

  Parts := Core.Split(['.']);
  if Length(Parts) < 2 then Exit;   // exige ao menos MAJOR.MINOR
  if not TryStrToInt(Parts[0], Result.Major) then Exit;
  if not TryStrToInt(Parts[1], Result.Minor) then Exit;
  if Length(Parts) >= 3 then
  begin
    if not TryStrToInt(Parts[2], Result.Patch) then Exit;
  end
  else
    Result.Patch := 0;
  Result.PreRelease := Pre;
  Result.Valid := True;
end;

function IsDevBuild: Boolean;
var
  V: TSemVer;
begin
  V := ParseSemVer(APP_VERSION);
  Result := (not V.Valid) or HasDescribeSuffix(APP_VERSION);
end;

function DisplayVersion: string;
begin
  Result := APP_VERSION;
  if StartsText('v', Result) then Delete(Result, 1, 1);
end;

function CompareVersions(const A, B: string): Integer;
var
  VA, VB: TSemVer;
begin
  VA := ParseSemVer(A);
  VB := ParseSemVer(B);
  if not (VA.Valid and VB.Valid) then Exit(0);

  // if/else explicito e nao IfThen: o IfThen de System.StrUtils so tem
  // sobrecarga pra string; a versao numerica vive em System.Math.
  if VA.Major <> VB.Major then
    if VA.Major < VB.Major then Exit(-1) else Exit(1);
  if VA.Minor <> VB.Minor then
    if VA.Minor < VB.Minor then Exit(-1) else Exit(1);
  if VA.Patch <> VB.Patch then
    if VA.Patch < VB.Patch then Exit(-1) else Exit(1);

  // Mesmo core: regra semver — quem TEM pré-lançamento é menor.
  // (1.0.0-beta < 1.0.0). Dois pré-lançamentos comparam alfabeticamente.
  if (VA.PreRelease = '') and (VB.PreRelease = '') then Exit(0);
  if VA.PreRelease = '' then Exit(1);
  if VB.PreRelease = '' then Exit(-1);
  Result := CompareText(VA.PreRelease, VB.PreRelease);
  if Result < 0 then Result := -1
  else if Result > 0 then Result := 1;
end;

function IsRemoteNewer(const ARemote: string): Boolean;
begin
  // Qualquer dúvida => False. Nunca avisar por erro é melhor do que avisar
  // à toa: um falso positivo destrói a confiança no aviso.
  if Trim(ARemote) = '' then Exit(False);
  if IsDevBuild then Exit(False);
  if not ParseSemVer(ARemote).Valid then Exit(False);
  Result := CompareVersions(APP_VERSION, ARemote) < 0;
end;

end.

@echo off
REM ---------------------------------------------------------------------
REM  make-installer.bat - gera o instalador NSIS ja com a versao no nome.
REM
REM  Uso:
REM      make-installer.bat                  versao = tag do git
REM      make-installer.bat v0.22.0.0-beta   versao explicita
REM
REM  Saida: NoOBS-<versao>.exe na raiz do projeto (ex.: NoOBS-v0.22.0-beta.exe).
REM
REM  O nome sai certo de primeira: o .nsi tem OutFile "${OUTFILE}" com um
REM  default, e este .bat passa /DOUTFILE=... pro makensis. Nada de renomear
REM  depois - se o build falhar, nao sobra arquivo com nome de release.
REM
REM  A versao vem da MESMA fonte do gen-version.bat (git describe), entao o
REM  nome do instalador e o APP_VERSION compilado no exe nao divergem.
REM  Build fora de uma tag vem com sufixo (-N-g<sha> / -dirty) e o sufixo
REM  ENTRA no nome de proposito: build de dev nao pode parecer release.
REM
REM  Este .bat NAO compila o Delphi (a Community Edition nao compila por
REM  linha de comando). Compile em Release/Win64 no RAD Studio antes.
REM
REM  A versao Rust usa o make-installer-rust.bat, que compila e chama este
REM  com NOOBS_RUNTIME (pasta do runtime) e NOOBS_FLAVOR (sufixo do nome).
REM ---------------------------------------------------------------------
setlocal EnableDelayedExpansion
cd /d "%~dp0"

REM --- 1) Localiza o makensis --------------------------------------------
set "MAKENSIS="
if defined NSIS_HOME if exist "%NSIS_HOME%\makensis.exe" set "MAKENSIS=%NSIS_HOME%\makensis.exe"
if not defined MAKENSIS if exist "%ProgramFiles(x86)%\NSIS\makensis.exe" set "MAKENSIS=%ProgramFiles(x86)%\NSIS\makensis.exe"
if not defined MAKENSIS if exist "%ProgramFiles%\NSIS\makensis.exe" set "MAKENSIS=%ProgramFiles%\NSIS\makensis.exe"
if not defined MAKENSIS for %%m in (makensis.exe) do if not "%%~$PATH:m"=="" set "MAKENSIS=%%~$PATH:m"
if not defined MAKENSIS (
  echo [make-installer] ERRO: makensis.exe nao encontrado.
  echo                  Instale o NSIS ^(nsis.sourceforge.io^) ou defina NSIS_HOME.
  exit /b 1
)

REM --- 2) O exe precisa existir ------------------------------------------
REM  Sem ele o pacote sairia "valido" e sem o aplicativo dentro.
REM  NOOBS_RUNTIME / NOOBS_FLAVOR vem do make-installer-rust.bat: a pasta
REM  do runtime (rust\exe) e o sufixo do nome. Sem eles, exe\ do Delphi.
set "RUNTIME=exe"
if defined NOOBS_RUNTIME set "RUNTIME=%NOOBS_RUNTIME%"
set "APPEXE=%RUNTIME%\bin\64bit\NoOBS.exe"
set "FLAVOR="
if defined NOOBS_FLAVOR set "FLAVOR=-%NOOBS_FLAVOR%"
if not exist "%APPEXE%" (
  echo [make-installer] ERRO: %APPEXE% nao existe.
  if defined NOOBS_RUNTIME (
    echo                  Rode cargo build --release e rust\sync-runtime.bat antes.
  ) else (
    echo                  Compile em Release/Win64 no RAD Studio antes de empacotar.
  )
  exit /b 1
)

REM --- 3) Versao ----------------------------------------------------------
set "VER=%~1"
if not defined VER for /f "delims=" %%v in ('git describe --tags --always --dirty 2^>nul') do set "VER=%%v"

REM  Fallback: le o Version.inc que o gen-version.bat escreveu no ultimo
REM  build (linha: const APP_VERSION = 'v0.22.0-beta';).
if not defined VER if exist "src\Version.inc" for /f "tokens=2 delims='" %%v in ('type "src\Version.inc"') do set "VER=%%v"

if not defined VER (
  echo [make-installer] ERRO: nao consegui determinar a versao.
  echo                  Passe explicitamente: make-installer.bat v0.22.0-beta
  exit /b 1
)

REM  Normaliza o "v" inicial pro nome ficar sempre no mesmo formato.
if /i not "!VER:~0,1!"=="v" set "VER=v!VER!"

REM --- 4) Avisos (nao impedem o build) ------------------------------------
set "DEV="
echo(!VER!| findstr /r /c:"-dirty$" /c:"-g[0-9a-f][0-9a-f]*$" >nul && set "DEV=1"
if defined DEV echo [make-installer] AVISO: build de desenvolvimento ^(fora de uma tag limpa^).

REM  O exe carrega o APP_VERSION do momento em que foi COMPILADO. Se a tag
REM  mudou depois disso, o nome do instalador e a tela "Sobre" divergem.
REM  So vale pro Delphi: o Rust tira a versao do git describe no build.
set "INCVER="
if not defined NOOBS_RUNTIME if exist "src\Version.inc" for /f "tokens=2 delims='" %%v in ('type "src\Version.inc"') do set "INCVER=%%v"
if defined INCVER if /i not "v!INCVER!"=="!VER!" if /i not "!INCVER!"=="!VER!" (
  echo [make-installer] AVISO: o exe foi compilado como !INCVER! e o instalador vai sair como !VER!.
  echo                  Recompile no RAD Studio se quiser os dois iguais.
)

REM --- 5) Gera ------------------------------------------------------------
REM  Apaga um arquivo homonimo antes: se o makensis falhar, a sobra de uma
REM  execucao anterior pareceria o instalador recem-gerado.
set "OUT=NoOBS-!VER!!FLAVOR!.exe"
if exist "!OUT!" erase /q "!OUT!"

echo [make-installer] gerando !OUT! com !APPEXE! ...
"%MAKENSIS%" /DOUTFILE=!OUT! "/DRUNTIME=!RUNTIME!" installer.nsi
if errorlevel 1 (
  echo [make-installer] ERRO: makensis falhou.
  exit /b 1
)
if not exist "!OUT!" (
  echo [make-installer] ERRO: makensis terminou sem gerar !OUT!.
  exit /b 1
)

for %%f in ("!OUT!") do set /a SZ=%%~zf/1048576
echo [make-installer] OK: !OUT! ^(!SZ! MB^)
endlocal
exit /b 0

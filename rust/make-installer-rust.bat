@echo off
REM ---------------------------------------------------------------------
REM  make-installer-rust.bat - gera o instalador NSIS da versao RUST.
REM
REM  Uso:
REM      make-installer-rust.bat                  versao = tag do git
REM      make-installer-rust.bat v0.22.0.0-beta   versao explicita
REM
REM  Saida: NoOBS-<versao>-rust.exe nesta pasta (rust\).
REM
REM  Independente do projeto Delphi da raiz: compila (build.bat), empacota
REM  o exe\ DESTA pasta com o installer.nsi DESTA pasta.
REM
REM  A versao vem do git describe - a mesma que o build.rs compila no exe.
REM  Build fora de uma tag limpa vem com sufixo (-N-g<sha> / -dirty) e o
REM  sufixo ENTRA no nome de proposito: build de dev nao pode parecer release.
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
  echo [make-installer-rust] ERRO: makensis.exe nao encontrado.
  echo                       Instale o NSIS ^(nsis.sourceforge.io^) ou defina NSIS_HOME.
  exit /b 1
)

REM --- 2) Compila e poe o exe em exe\bin\64bit\ ---------------------------
call "%~dp0build.bat"
if errorlevel 1 exit /b 1

REM --- 3) Versao ----------------------------------------------------------
set "VER=%~1"
if not defined VER for /f "delims=" %%v in ('git describe --tags --always --dirty 2^>nul') do set "VER=%%v"
if not defined VER (
  echo [make-installer-rust] ERRO: nao consegui determinar a versao.
  echo                       Passe explicitamente: make-installer-rust.bat v0.22.0-beta
  exit /b 1
)
if /i not "!VER:~0,1!"=="v" set "VER=v!VER!"

set "DEV="
echo(!VER!| findstr /r /c:"-dirty$" /c:"-g[0-9a-f][0-9a-f]*$" >nul && set "DEV=1"
if defined DEV echo [make-installer-rust] AVISO: build de desenvolvimento ^(fora de uma tag limpa^).

REM --- 4) Gera ------------------------------------------------------------
REM  Apaga um arquivo homonimo antes: se o makensis falhar, a sobra de uma
REM  execucao anterior pareceria o instalador recem-gerado.
set "OUT=NoOBS-!VER!-rust.exe"
if exist "!OUT!" erase /q "!OUT!"

echo [make-installer-rust] gerando !OUT! ...
"%MAKENSIS%" /DOUTFILE=!OUT! installer.nsi
if errorlevel 1 (
  echo [make-installer-rust] ERRO: makensis falhou.
  exit /b 1
)
if not exist "!OUT!" (
  echo [make-installer-rust] ERRO: makensis terminou sem gerar !OUT!.
  exit /b 1
)

for %%f in ("!OUT!") do set /a SZ=%%~zf/1048576
echo [make-installer-rust] OK: rust\!OUT! ^(!SZ! MB^)
endlocal
exit /b 0

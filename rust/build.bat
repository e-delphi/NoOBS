@echo off
REM ---------------------------------------------------------------------
REM  build.bat - compila o NoOBS em Rust e poe o exe no runtime.
REM
REM  Uso:
REM      build.bat           cargo build --release
REM      build.bat debug     cargo build (mais rapido, sem otimizacao)
REM
REM  Saida: exe\bin\64bit\NoOBS.exe (desta pasta rust\), ao lado da obs.dll,
REM  das DLLs do FFmpeg, da ui\ e do lang\ - e de la que o app roda.
REM
REM  Sem WebView2Loader.dll: no alvo MSVC o webview2-com-sys liga o
REM  WebView2LoaderStatic.lib, entao o loader ja vai dentro do exe.
REM ---------------------------------------------------------------------
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
  echo [build] ERRO: cargo nao encontrado. Instale o Rust ^(rustup.rs^).
  exit /b 1
)
if not exist "exe\bin\64bit\obs.dll" (
  echo [build] ERRO: exe\bin\64bit\obs.dll nao existe - falta o runtime do OBS.
  echo         Copie o OBS portatil ^(32.1.2^) pra exe\ desta pasta: bin\64bit, data, obs-plugins.
  exit /b 1
)

set "PROFILE=release"
set "FLAGS=--release"
if /i "%~1"=="debug" (
  set "PROFILE=debug"
  set "FLAGS="
)

echo [build] cargo build %FLAGS% ...
cargo build %FLAGS%
if errorlevel 1 (
  echo [build] ERRO: cargo build falhou.
  exit /b 1
)

copy /y "target\%PROFILE%\noobs.exe" "exe\bin\64bit\NoOBS.exe" >nul
if errorlevel 1 (
  echo [build] ERRO: nao consegui copiar o exe ^(o NoOBS de exe\bin\64bit esta aberto?^).
  exit /b 1
)
echo [build] OK: exe\bin\64bit\NoOBS.exe ^(%PROFILE%^)
exit /b 0

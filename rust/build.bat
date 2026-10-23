@echo off
REM ---------------------------------------------------------------------
REM  build.bat - compila o NoOBS em Rust e poe o exe no runtime.
REM
REM  Uso:
REM      build.bat           cargo build --release
REM      build.bat debug     cargo build (mais rapido, sem otimizacao)
REM
REM  Saida: exe\NoOBS.exe (desta pasta rust\), ao lado das DLLs do FFmpeg -
REM  e de la que o app roda. O runtime e SO isso: a interface (ui\, Slint) e
REM  as traducoes (lang\) vao compiladas dentro do exe.
REM ---------------------------------------------------------------------
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
  echo [build] ERRO: cargo nao encontrado. Instale o Rust ^(rustup.rs^).
  exit /b 1
)
if not exist "exe\avcodec-61.dll" (
  echo [build] ERRO: exe\avcodec-61.dll nao existe - faltam as DLLs do FFmpeg.
  echo         Copie pra exe\ desta pasta, do OBS portatil 32.1.2 ^(bin\64bit^):
  echo         avcodec-61 avformat-61 avutil-59 swscale-8 swresample-5
  echo         libx264-164 zlib srt librist ^(.dll^).
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

copy /y "target\%PROFILE%\noobs.exe" "exe\NoOBS.exe" >nul
if errorlevel 1 (
  echo [build] ERRO: nao consegui copiar o exe ^(o NoOBS de exe\ esta aberto?^).
  exit /b 1
)
echo [build] OK: exe\NoOBS.exe ^(%PROFILE%^)
exit /b 0

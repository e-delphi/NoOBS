@echo off
REM ---------------------------------------------------------------------
REM  sync-runtime.bat - monta rust\exe\, o runtime da versao Rust.
REM
REM  Uso (depois do cargo build):
REM      rust\sync-runtime.bat            usa target\release\noobs.exe
REM      rust\sync-runtime.bat debug      usa target\debug\noobs.exe
REM
REM  rust\exe\ tem o MESMO layout do exe\ do Delphi (bin\64bit, data,
REM  obs-plugins) e e uma COPIA dele, fora do git:
REM    - sem o NoOBS.exe do Delphi (entra o do Rust, com o mesmo nome);
REM    - sem o WebView2Loader.dll: o exe do Rust liga o loader estatico
REM      (WebView2LoaderStatic.lib, via webview2-com-sys).
REM  A UI (ui\) e as traducoes (lang\) continuam com a fonte da verdade em
REM  exe\bin\64bit\: edite la e rode este script de novo. O robocopy /MIR
REM  so copia o que mudou e apaga o que sumiu da origem.
REM
REM  Rodar: rust\exe\bin\64bit\NoOBS.exe (feche o NoOBS do Delphi antes:
REM  os dois usam o mesmo mutex de instancia unica).
REM ---------------------------------------------------------------------
setlocal
cd /d "%~dp0"

set "PROFILE=release"
if /i "%~1"=="debug" set "PROFILE=debug"
set "SRCEXE=target\%PROFILE%\noobs.exe"
if not exist "%SRCEXE%" (
  echo [sync-runtime] ERRO: %SRCEXE% nao existe. Rode cargo build ^(--release^) antes.
  exit /b 1
)
if not exist "..\exe\bin\64bit\obs.dll" (
  echo [sync-runtime] ERRO: ..\exe\bin\64bit\obs.dll nao existe ^(runtime do OBS ausente^).
  exit /b 1
)

REM  /XF tambem protege esses nomes no destino (o /MIR nao os apaga).
robocopy "..\exe" "exe" /MIR /NFL /NDL /NJH /NJS /NP /R:1 /W:1 ^
  /XF NoOBS.exe noobs-*.exe NoOBS-*.exe WebView2Loader.dll >nul
if errorlevel 8 (
  echo [sync-runtime] ERRO: robocopy falhou ^(%errorlevel%^).
  exit /b 1
)
REM  Sobra de um sync antigo, de quando o loader ainda era copiado.
if exist "exe\bin\64bit\WebView2Loader.dll" del /q "exe\bin\64bit\WebView2Loader.dll"

copy /y "%SRCEXE%" "exe\bin\64bit\NoOBS.exe" >nul
if errorlevel 1 (
  echo [sync-runtime] ERRO: nao consegui copiar o exe ^(o NoOBS de rust\exe esta aberto?^).
  exit /b 1
)
echo [sync-runtime] OK: rust\exe\bin\64bit\NoOBS.exe ^(%PROFILE%^)
exit /b 0

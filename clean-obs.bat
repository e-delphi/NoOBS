@echo off
REM ============================================================
REM  clean-obs.bat
REM
REM  Limpa o OBS portatil bundled em exe\ removendo plugins,
REM  binarios do OBS Studio (UI), CEF, .pdb's e Qt — coisas que
REM  o NoOBS nao usa porque chama libobs direto via obs.dll.
REM
REM  Plugins MANTIDOS (LibOBSEngine.LoadModules WANTED list):
REM    obs-ffmpeg     - encoder ffmpeg_aac + output ffmpeg_muxer
REM    obs-x264       - encoder CPU fallback
REM    obs-nvenc      - encoder hardware NVIDIA (opcional)
REM    win-capture    - source monitor_capture (gravar tela)
REM    win-dshow      - source dshow_input (webcam)
REM    win-wasapi     - sources wasapi_input/output_capture (audio)
REM
REM  Idempotente: pode rodar quantas vezes quiser.
REM ============================================================

cd /d %~dp0
set ROOT=%cd%\exe
if not exist "%ROOT%" (
  echo [ERRO] Pasta nao encontrada: %ROOT%
  exit /b 1
)

set BIN=%ROOT%\bin\64bit
set PLUGINS=%ROOT%\obs-plugins\64bit
set DATAP=%ROOT%\data\obs-plugins
set CONFIG=%ROOT%\config

echo == Limpando OBS bundled em %ROOT% ==

REM Plugins nao usados (.dll + .pdb + pasta data correspondente).
REM obs-filters/transitions/outputs/rtmp-services antes eram obrigatorios
REM pro init do OBS Studio carregar a cena default. Com libobs direto
REM montamos nossa cena, entao podem sair tambem.
for %%P in (
  aja
  aja-output-ui
  decklink
  decklink-captions
  decklink-output-ui
  frontend-tools
  image-source
  nv-filters
  obs-browser
  obs-filters
  obs-outputs
  obs-qsv11
  obs-text
  obs-transitions
  obs-vst
  obs-webrtc
  obs-websocket
  rtmp-services
  text-freetype2
  vlc-video
  coreaudio-encoder
) do (
  if exist "%PLUGINS%\%%P.dll" (
    echo  - plugin: %%P
    del /q "%PLUGINS%\%%P.dll" 2>nul
  )
  if exist "%PLUGINS%\%%P.pdb" del /q "%PLUGINS%\%%P.pdb" 2>nul
  if exist "%DATAP%\%%P" rmdir /s /q "%DATAP%\%%P" 2>nul
)

REM Sobras do obs-browser (CEF) — pesado, ~300MB
for %%F in (
  obs-browser-page.exe
  chrome_100_percent.pak
  chrome_200_percent.pak
  chrome_elf.dll
  icudtl.dat
  libcef.dll
  libEGL.dll
  libGLESv2.dll
  resources.pak
  v8_context_snapshot.bin
) do (
  if exist "%PLUGINS%\%%F" (
    echo  - arquivo: %%F
    del /q "%PLUGINS%\%%F" 2>nul
  )
)
if exist "%PLUGINS%\locales" (
  echo  - pasta: locales
  rmdir /s /q "%PLUGINS%\locales" 2>nul
)

REM Binarios da UI do OBS Studio — nao subimos o obs64.exe, so usamos
REM obs.dll. Tambem nao precisamos do ffplay (so ffmpeg + ffprobe).
for %%F in (
  obs64.exe
  ffplay.exe
  obs-frontend-api.dll
  obs-scripting.dll
  lua51.dll
  libobs-opengl.dll
) do (
  if exist "%BIN%\%%F" (
    echo  - bin: %%F
    del /q "%BIN%\%%F" 2>nul
  )
)

REM Qt nao e necessario — nao carregamos plugins que usam Qt
REM (obs-websocket, frontend-tools, obs-browser ja foram removidos).
for %%F in (
  Qt6Core.dll
  Qt6Gui.dll
  Qt6Network.dll
  Qt6Svg.dll
  Qt6Widgets.dll
  Qt6Xml.dll
) do (
  if exist "%BIN%\%%F" (
    echo  - Qt: %%F
    del /q "%BIN%\%%F" 2>nul
  )
)

REM Pasta vazia que o OBS deixa em bin\ (legado obs-websocket).
if exist "%BIN%\obs-websocket" (
  echo  - bin: obs-websocket\ (pasta vazia)
  rmdir /s /q "%BIN%\obs-websocket" 2>nul
)

REM Symbols de debug (.pdb)
echo  - .pdb em obs-plugins\64bit
del /q "%PLUGINS%\*.pdb" 2>nul
echo  - .pdb em bin\64bit
del /q "%BIN%\*.pdb" 2>nul

REM config\ inteiro — restos da arquitetura antiga (basic.ini,
REM user.ini, sentinels, plugin_config). NoOBS via libobs direto
REM passa settings in-memory via obs_data_t, nao escreve nada.
if exist "%CONFIG%" (
  echo  - config\ (legado)
  rmdir /s /q "%CONFIG%" 2>nul
)

REM Temas Qt — sem Qt, sem temas.
if exist "%ROOT%\data\obs-studio\themes" (
  echo  - data\obs-studio\themes
  rmdir /s /q "%ROOT%\data\obs-studio\themes" 2>nul
)

echo.
echo OBS bundled limpo. Pode zipar exe\ e distribuir.
echo == Concluido ==
pause

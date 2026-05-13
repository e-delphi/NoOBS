# NoOBS

![NoOBS](app.png)

Gravador de tela em Delphi usando **libobs (obs.dll) diretamente in-process**.
Interface web em WebView2.

Foi desenhado pra ser intuitivo: nada de processo OBS separado, nada de
websocket, nada de config files. O `NoOBS.exe` carrega `obs.dll` na
primeira gravação e usa a API C do libobs direto.

---

## Requisitos

Versão atualizada do OBS disponível em <https://obsproject.com/download>.
Baixe a versão Zip e extraia o conteúdo em `exe/` (achatado, sem o
`obs/` intermediário). Após extrair, rode `clean-obs.bat` pra remover
plugins/binaries que o NoOBS não usa.

Versão major do FFmpeg deve casar com as DLLs `avcodec-XX` que o OBS
portátil traz em `exe/bin/64bit/` (atualmente FFmpeg **7.x**):
<https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2024-09-30-15-36>.
Baixe **`ffmpeg-n7.1-win64-gpl-shared-7.1.zip`** (build **shared**, não
static), extraia só `bin/ffmpeg.exe` e `bin/ffprobe.exe` pra
`exe/bin/64bit/` — reusa as DLLs já presentes lá.

```
NoOBS/
├── src/                       ← código Delphi (.pas)
├── ui/                        ← index.html (compilado como RCDATA)
├── exe/                       ← runtime (build output + OBS bundled)
│   ├── bin/64bit/
│   │   ├── NoOBS.exe          ← app
│   │   ├── WebView2Loader.dll
│   │   ├── obs.dll            ← libobs
│   │   ├── libobs-d3d11.dll
│   │   ├── obs-ffmpeg-mux.exe
│   │   ├── ffmpeg.exe, ffprobe.exe
│   │   └── av*.dll, sw*.dll, libx264-164.dll, ...
│   ├── data/
│   │   ├── libobs/*.effect    ← shaders
│   │   └── obs-plugins/<plugin>/*
│   └── obs-plugins/64bit/*.dll
├── NoOBS.dpr, NoOBS.dproj
└── clean-obs.bat
```

---

## Recursos

- **Captura unificada**: composição de todos os monitores num só canvas,
  em um único arquivo. Layout compacto (lado a lado, sem buracos pretos).
- **Webcams**: enumeradas via DirectShow, posicionadas à direita do
  bounding dos monitores. Forçadas em MJPEG@30 (modo universal).
- **Audio-only**: marcar só mic/speaker gera MKV com canvas preto 800×600
  (apenas pro container ser válido) + audio tracks normais.
- **Áudio multi-track**: track 1 = mix de tudo; tracks 2..6 = isoladas
  por dispositivo (mics e saídas). Toggle de cada fonte na UI.
- **Encoder HEVC quando disponível**: tenta `obs_nvenc_hevc_tex`,
  `h265_texture_amf`, `obs_qsv11_hevc`, etc., na ordem. Cai pra
  `obs_x264` (CPU) se nada de hardware estiver disponível.
- **Canvas até 8192**: limite hard do NVENC em GPUs Turing+. Clamp
  proporcional mantendo aspect.
- **Telas visual**: mini-mapa do layout do desktop com thumbnail ao
  vivo de cada monitor (atualizado 1×/s, via Win32 `BitBlt` em thread
  própria — não trava a UI durante captura).
- **Hot-plug nativo**:
  - Áudio: `IMMNotificationClient` detecta mic/speaker plugado/desplugado.
  - Monitor: `WM_DISPLAYCHANGE` reage a mudanças de display.
  - Refresh automático ou banner amarelo se gravação ativa.
- **Player embutido**: clique numa gravação abre modal com controles
  customizados, seek com Range, zoom + pan no vídeo, transcode HEVC→H.264
  sob demanda via ffmpeg local.
- **Painel de informações do vídeo**: botão circular `ⓘ` no player abre
  painel lateral com dados extraídos via ffprobe (container, duração,
  bitrate, codec, resolução, faixas de áudio com codec/canais/sample rate).
- **Lista de gravações**: cards com thumbnail e duração (cacheados em
  `%LOCALAPPDATA%\NoOBS\cache`), agrupados por período, busca,
  renomeação inline, exclusão pra Lixeira.
- **Seleção múltipla**: check em cada card + check do período inteiro,
  exclusão em lote via context menu.
- **Notificações em toast** pra erros (não bloqueia a UI).
- **Tema claro/escuro** persistido em `%LOCALAPPDATA%\NoOBS\config.json`,
  com sincronia de title bar do Windows (`DwmSetWindowAttribute`).
- **Splash nativo** durante a inicialização do WebView2.

---

## Arquitetura

```
┌─────────────────────────────────────────────────────────────────────┐
│ NoOBS.exe                                                           │
│                                                                     │
│  ┌─────────┐    ┌────────────┐    ┌─────────────────────────────┐  │
│  │ OBSUI   │◄──►│ OBSBridge  │◄──►│ LibOBSEngine (motor)        │  │
│  │WebView2 │    │ Dispatcher │    └─────────────┬───────────────┘  │
│  └─────────┘    │            │                  │                  │
│                 │            │                  ▼                  │
│                 │            │           ┌─────────────┐           │
│                 │            │           │ obs.dll     │           │
│                 │            │           │ (libobs)    │           │
│                 │            │           └─────────────┘           │
│                 │            │    ┌─────────────────────────────┐  │
│                 │            │◄──►│ OBSPlayer (HTTP+ffmpeg)     │  │
│                 │            │    └─────────────────────────────┘  │
│                 │            │    ┌─────────────────────────────┐  │
│                 │            │◄──►│ OBSProbe (ffprobe wrapper)  │  │
│                 │            │    └─────────────────────────────┘  │
│                 │            │    ┌─────────────────────────────┐  │
│                 │            │◄──►│ OBSAudioWatch (hot-plug)    │  │
│                 │            │    └─────────────────────────────┘  │
│                 │            │    ┌─────────────────────────────┐  │
│                 └────────────┘    │ OBSConfig (JSON prefs)      │  │
│                                   └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

| Unit             | Responsabilidade                                                                |
|------------------|---------------------------------------------------------------------------------|
| `OBSUI`          | Host WebView2, message pump, splash, sync de tema                              |
| `OBSBridge`      | Dispatcher de mensagens UI↔Engine, timers, eventos                             |
| `LibOBS`         | Bindings Delphi para `obs.dll` (tipos opacos, structs, funcoes cdecl)          |
| `LibOBSEngine`   | Motor de gravação: init libobs, scene, sources, encoders, output MKV           |
| `OBSScene`       | Tipos puros (TOBSMonitor, TAudioDevice) + `ComputeCanvas` + filtros            |
| `OBSPlayer`      | Servidor HTTP local (Range support) + integração ffmpeg (transcode/thumb)      |
| `OBSProbe`       | Wrapper sobre ffprobe pra extrair metadata de vídeos (codec, faixas, etc.)     |
| `OBSAudioWatch`  | `IMMNotificationClient` puro Delphi pra hot-plug de áudio                      |
| `OBSConfig`      | Preferências em JSON                                                            |
| `OBSLog`         | Log centralizado em `%LOCALAPPDATA%\NoOBS\NoOBS.log`                           |
| `WinPreview`     | Win32 `EnumDisplayMonitors` + `BitBlt` pra capturar thumbs                     |
| `WinAudioMeter`  | WASAPI `IAudioMeterInformation` pra peak por device                            |
| `WinWebcam`      | DirectShow pra enumerar webcams                                                |

---

## Build

**Requisitos**:
- RAD Studio 12+ (Delphi). Compilador `dcc64.exe`.
- Win64 target.
- OBS portátil descompactado em `exe/` (ver "Requisitos" acima).
- ffmpeg + ffprobe em `exe/bin/64bit/` (ver "FFmpeg" abaixo).

### FFmpeg

Não vem versionado no repo. Baixar manualmente:

1. Acesse o release **autobuild-2024-09-30-15-36** do BtbN:
   <https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2024-09-30-15-36>
2. Baixe **`ffmpeg-n7.1-win64-gpl-shared-7.1.zip`**.
3. Extraia o conteúdo da pasta `bin/` do zip direto pra `exe/bin/64bit/`
   (só adicionando `ffmpeg.exe` e `ffprobe.exe` — DLLs já estão lá).

**Por que essa versão exata:** o build precisa ser **shared** (não static)
e na **major 7.x**, igual às DLLs `avcodec-61` / `avformat-61` / `avutil-59` /
`swresample-5` / `swscale-8` que o OBS bundled traz em `bin/64bit/`.
Outras versões (8.x = avcodec-62) não vão carregar.

Se um dia o OBS bundled subir de major (7→8), troque o ffmpeg/ffprobe
por build shared da major correspondente.

### Compilar

Abra `NoOBS.dproj` no RAD Studio e build em Release/Win64
(`Project → Build NoOBS` ou `Shift+F9`). Ou via linha de comando:

```bat
"C:\Program Files (x86)\Embarcadero\Studio\23.0\bin\rsvars.bat"
msbuild NoOBS.dproj /t:Build /p:config=Release /p:platform=Win64
```

Saída: `exe/bin/64bit/NoOBS.exe`.

A UI HTML é compilada como recurso `RCDATA "UI"` (ver `NoOBSResource.rc`)
e carregada via `WebView.NavigateToString` — sem dependência de arquivo
em runtime.

### Limpeza do OBS bundled

OBS Studio vem com muita coisa que o NoOBS não usa (UI Qt, browser CEF,
plugins extras, .pdb's, etc.). Após extrair o OBS portátil em `exe/`,
rode:

```bat
clean-obs.bat
```

Idempotente. Remove plugins não-usados, binários do OBS UI, Qt,
`.pdb`'s. Pode reduzir o pacote em centenas de MB.

---

## Layout do disco em runtime

```
exe/
├── bin/64bit/
│   ├── NoOBS.exe
│   ├── WebView2Loader.dll
│   ├── obs.dll, libobs-d3d11.dll
│   ├── obs-ffmpeg-mux.exe       ← helper pro output MKV
│   ├── ffmpeg.exe, ffprobe.exe
│   └── av*.dll, sw*.dll, libx264-164.dll, ...
├── data/
│   ├── libobs/*.effect          ← shaders D3D11
│   └── obs-plugins/<plugin>/*   ← locale, etc
└── obs-plugins/64bit/*.dll      ← plugins do libobs

%LOCALAPPDATA%/NoOBS/
├── NoOBS.log                    ← log único, append entre sessões
├── config.json                  ← preferências (theme, etc.)
└── cache/
    ├── <hash>.dur
    ├── <hash>.jpg
    └── <hash>.mp4

%LOCALAPPDATA%/TNoOBS/           ← user data folder do WebView2
```

---

## Convenções

- Comentários em **português**.
- Arquivos `.pas` salvos em **UTF-8 com BOM** (Delphi exige pra parsear
  acentos corretamente).
- Sem dependências externas além da RTL Delphi e WebView2.
- Chamadas pra `obs.dll` (via `LibOBS`) **só na main thread** — libobs
  espera single-threaded API access.

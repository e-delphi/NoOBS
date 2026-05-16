# NoOBS

![NoOBS](app.png)

Gravador de tela em Delphi usando **libobs (obs.dll) diretamente in-process**.
Interface web em WebView2.

Foi desenhado pra ser intuitivo: nada de processo OBS separado, nada de
websocket, nada de config files. O `NoOBS.exe` carrega `obs.dll` na
primeira gravação e usa a API C do libobs direto. Todas as operações de
mídia (probe, remux, extração de áudio, thumbnail) usam `avformat-61` /
`avcodec-61` / `avutil-59` / `swscale-8` diretamente in-process.

---

## Requisitos

Versão atualizada do OBS disponível em <https://obsproject.com/download>.
Baixe a versão Zip e extraia o conteúdo em `exe/` (achatado, sem o
`obs/` intermediário). Após extrair, rode `clean-obs.bat` pra remover
plugins/binaries que o NoOBS não usa.

O OBS portátil já traz as DLLs FFmpeg que o NoOBS precisa:
`avcodec-61.dll`, `avformat-61.dll`, `avutil-59.dll`, `swscale-8.dll`.

```
NoOBS/
├── src/                       ← código Delphi (.pas)
├── ui/                        ← index.html (compilado como RCDATA)
├── exe/                       ← runtime (build output + OBS bundled)
│   ├── bin/64bit/
│   │   ├── NoOBS.exe          ← app
│   │   ├── WebView2Loader.dll
│   │   ├── obs.dll, libobs-d3d11.dll
│   │   ├── obs-ffmpeg-mux.exe ← helper invocado pelo libobs no muxer MKV
│   │   ├── avcodec-61.dll, avformat-61.dll, avutil-59.dll, swscale-8.dll
│   │   └── swresample-5.dll, libx264-164.dll, ...
│   ├── data/
│   │   ├── libobs/*.effect    ← shaders
│   │   └── obs-plugins/<plugin>/*
│   └── obs-plugins/64bit/*.dll
├── NoOBS.dpr, NoOBS.dproj
└── clean-obs.bat
```

---

## Recursos

### Gravação
- **Captura unificada**: composição de todos os monitores num só canvas,
  em um único arquivo. Layout compacto (lado a lado, sem buracos pretos).
- **Webcams**: enumeradas via DirectShow, posicionadas à direita do
  bounding dos monitores. Forçadas em MJPEG@30 (modo universal).
- **Audio-only**: marcar só mic/speaker gera MKV com canvas preto 800×600
  (apenas pro container ser válido) + audio tracks normais.
- **Áudio multi-track**: track 1 = mix de tudo; tracks 2..6 = isoladas
  por dispositivo (mics e saídas). Toggle de cada fonte na UI.
- **Seletor de codec**: HEVC/H.264 + Auto (hardware-first) ou Software.
  Logo do fabricante da GPU (AMD/NVIDIA/Intel) é detectado e mostrado
  nas configurações via `obs_enum_encoder_types`.
- **Encoder HEVC quando disponível**: tenta `obs_nvenc_hevc_tex`,
  `h265_texture_amf`, `obs_qsv11_hevc`, etc., na ordem. Cai pra
  `obs_x264` (CPU) se nada de hardware estiver disponível.
- **Canvas até 8192**: limite hard do NVENC em GPUs Turing+. Clamp
  proporcional mantendo aspect.
- **Atalho global Ctrl+Shift+F9**: inicia/para gravação de qualquer
  lugar do sistema (via `RegisterHotKey` + `WM_HOTKEY`).

### Interface
- **Telas visual**: mini-mapa do layout do desktop com thumbnail ao
  vivo de cada monitor (atualizado 1×/s, via Win32 `BitBlt` em thread
  própria — não trava a UI durante captura).
- **Hot-plug nativo**:
  - Áudio: `IMMNotificationClient` detecta mic/speaker plugado/desplugado.
  - Monitor: `WM_DISPLAYCHANGE` reage a mudanças de display.
  - Refresh automático ou banner amarelo se gravação ativa.
- **Stereo meters L+R**: barras separadas por canal via
  `IAudioMeterInformation::GetChannelsPeakValues`. Devices mono mostram
  uma barra única.
- **Indicadores de faixa de áudio**: barra colorida de 3px à esquerda
  de cada device indicando em que track (2-6) ele vai gravar. Legenda
  no rodapé da sidebar mostra só as cores em uso.
- **Tema claro/escuro** persistido em `%LOCALAPPDATA%\NoOBS\config.json`,
  com sincronia de title bar do Windows (`DwmSetWindowAttribute`).
- **Splash nativo** durante a inicialização do WebView2.
- **Notificações em toast** pra erros (não bloqueia a UI).

### Player
- **Player embutido**: clique numa gravação abre modal com controles
  customizados, seek com Range, zoom + pan no vídeo.
- **Multi-track playback**: vídeo principal + áudios por track em
  `<audio>` slaves sincronizados (drift check 200ms). Slider de volume
  por faixa no painel de info.
- **Cache automático** de MP4 remuxado em `%LOCALAPPDATA%\NoOBS\cache`
  pra tocar MKV no WebView2 (Chromium não toca MKV direto). Remux é
  só troca de container, sem reencode.
- **Painel de informações do vídeo**: botão circular `ⓘ` no player abre
  painel lateral com dados extraídos via libavformat (container, duração,
  bitrate, codec, resolução, faixas de áudio com codec/canais/sample rate).

### Gerenciamento
- **Lista de gravações**: cards com thumbnail e duração (cacheados em
  `%LOCALAPPDATA%\NoOBS\cache`), agrupados por período, busca,
  renomeação inline, exclusão pra Lixeira.
- **Seleção múltipla**: check em cada card + check do período inteiro,
  exclusão em lote via context menu.
- **Validação de runtime**: na inicialização, verifica presença das
  DLLs críticas (`obs.dll`, `avcodec-61.dll`, etc.). Se faltar algo,
  mostra MessageBox claro e aborta.

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
│                 │            │◄──►│ OBSPlayer (HTTP local)      │  │
│                 │            │    │   └─► FFmpegLib (libav DLL) │  │
│                 │            │    └─────────────────────────────┘  │
│                 │            │    ┌─────────────────────────────┐  │
│                 │            │◄──►│ OBSProbe (via libavformat)  │  │
│                 │            │    └─────────────────────────────┘  │
│                 │            │    ┌─────────────────────────────┐  │
│                 │            │◄──►│ OBSAudioWatch (hot-plug)    │  │
│                 │            │    └─────────────────────────────┘  │
│                 │            │    ┌─────────────────────────────┐  │
│                 └────────────┘    │ OBSConfig (JSON prefs v1)   │  │
│                                   └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

| Unit                | Responsabilidade                                                                |
|---------------------|---------------------------------------------------------------------------------|
| `OBSUI`             | Host WebView2, message pump, splash, sync de tema, hotkey global               |
| `OBSBridge`         | Dispatcher de mensagens UI↔Engine, timers, eventos                             |
| `LibOBS`            | Bindings Delphi para `obs.dll` (tipos opacos, structs, funcoes cdecl)          |
| `LibOBSEngine`      | Motor de gravação: init libobs, scene, sources, encoders, output MKV           |
| `OBSScene`          | Tipos puros (TOBSMonitor, TAudioDevice) + `ComputeCanvas` + filtros            |
| `OBSStartupCheck`   | Verifica DLLs/pastas obrigatórias antes de criar janela                        |
| `FFmpegLib`         | Bindings Delphi para libavformat/avcodec/avutil/swscale + helpers de remux     |
| `OBSPlayer`         | Servidor HTTP local (Range support), remux MKV→MP4, extração de audio tracks   |
| `OBSProbe`          | Inspeção de mídia via libavformat (codec, faixas, duração, bitrate)            |
| `OBSAudioWatch`     | `IMMNotificationClient` puro Delphi pra hot-plug de áudio                      |
| `OBSConfig`         | Preferências em JSON com versionamento                                          |
| `OBSLog`            | Log centralizado em `%LOCALAPPDATA%\NoOBS\NoOBS.log`                           |
| `WinPreview`        | Win32 `EnumDisplayMonitors` + `BitBlt` pra capturar thumbs                     |
| `WinAudioMeter`     | WASAPI `IAudioMeterInformation` pra peak L+R por device                        |
| `WinWebcam`         | DirectShow pra enumerar webcams                                                |

---

## Build

**Requisitos**:
- RAD Studio 12+ (Delphi). Compilador `dcc64.exe`.
- Win64 target.
- OBS portátil descompactado em `exe/` (ver "Requisitos" acima).

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
│   ├── obs-ffmpeg-mux.exe       ← helper que o libobs invoca pra muxar MKV
│   ├── avcodec-61.dll, avformat-61.dll, avutil-59.dll, swscale-8.dll
│   ├── swresample-5.dll
│   └── libx264-164.dll, ...
├── data/
│   ├── libobs/*.effect          ← shaders D3D11
│   └── obs-plugins/<plugin>/*   ← locale, etc
└── obs-plugins/64bit/*.dll      ← plugins do libobs

%LOCALAPPDATA%/NoOBS/
├── NoOBS.log                    ← log único, append entre sessões
├── config.json                  ← preferências (theme, etc.)
└── cache/
    ├── <hash>.dur               ← duração em segundos (texto)
    ├── <hash>.jpg               ← thumbnail (via libav decode+swscale+mjpeg)
    ├── <hash>.mp4               ← MP4 remuxado (via libavformat)
    └── <hash>_aN.m4a            ← audio track N isolada (via libavformat)

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
- Chamadas pra libav (via `FFmpegLib`) **podem rodar em worker thread**
  — sempre rodamos remux/extract/thumb em workers pra não travar a UI.
- Strings passadas pra libobs/libav: **sempre UTF-8** (`UTF8Encode` /
  `FFmpegLib.ToUtf8`). Strings recebidas: **sempre UTF-8** (`UTF8ToString`).

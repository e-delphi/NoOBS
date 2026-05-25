# NoOBS

Gravador de tela em Delphi com OBS Studio embarcado e UI em WebView2.

Este CLAUDE.md é a fonte da verdade pra arquitetura, convenções e
"pegadinhas" do projeto. **Antes de mexer no código, leia até o final.**

---

## Princípio fundamental

**libobs inicializa em warmup ~1.5s após app abrir e persiste até
fechar.** `TIMER_OBS_WARMUP` em `OBSBridge.DoInit` agenda o init com
delay pra UI renderizar primeiro. Resultado: 1ª gravação é
instantânea (sem espera de ~300ms pra obs.dll + plugins + D3D11).
Se warmup falhar, gravação inicializa sob demanda no clique.

Entre gravações, o core libobs fica vivo (plugins carregados, GPU
inicializada), mas scene/encoders/output são destruídos e recriados
a cada sessão. A UI funciona inteira via Win32 (monitor preview) e
WASAPI (audio meters) direto — sem processo separado, sem websocket.

**Toda manipulação de mídia (probe, remux, audio extract, thumbnail)
usa libav diretamente via DLL** — `avformat-61`, `avcodec-61`,
`avutil-59`, `swscale-8`. Operações ficam in-process, sem fork de
processo.

**NoOBS.exe roda dentro de `exe\bin\64bit\` (ao lado de obs.dll).**
Isso simplifica drasticamente o init: sem `SetDllDirectory`, sem
preload de DLLs, sem CWD trickery. Toda a resolução de paths (DLLs,
plugins, shaders relativos a `../../data/libobs/`, helpers via
`os_get_executable_path_ptr`) funciona naturalmente porque o exe está
exatamente no lugar onde libobs espera.

---

## Stack

- **RAD Studio 12+ (Delphi)**, target **Win64**, `{$APPTYPE GUI}`.
- **WebView2** via `Winapi.WebView2` + `WebView2Loader.dll`.
- **libobs** (`obs.dll`) carregada in-process via `external delayed`.
- **libavformat/avcodec/avutil/swscale** (FFmpeg 7.x) carregadas
  in-process via `external delayed`. Bundled pelo OBS portátil.
- **Indy** (`TIdHTTPServer`) — RTL.
- **HTML/CSS/JS puro** (sem framework) embutido como `RCDATA "UI"`.

## Estrutura do projeto

```
src/      ← .pas (todo código Delphi)
ui/       ← index.html (compilado como RCDATA)
exe/      ← runtime: build output em bin/64bit/ + OBS bundled
NoOBS.dpr, NoOBS.dproj
clean-obs.bat
```

---

## Arquitetura

Arquitetura em 4 camadas:

1. **Bindings raw** — declarações `external` das DLLs:
   - `LibOBS` — obs.dll
   - `FFmpegLib` — libavformat/avcodec/avutil/swscale + structs + acessors low-level
2. **Wrappers altos** — API Delphi limpa sobre as DLLs:
   - `OBSEngine` — TOBSEngine class (init, scene, sources, output MKV)
   - `FFmpegOps` — RemuxFile, ExtractAudioTracks, ExtractFrameJpeg
3. **Domínio do app** — lógica de negócio:
   - `OBSEncoder` — seleção de codec (AV1/HEVC/H264/x264)
   - `OBSAudioTracks` — atribuição de tracks + enum de audio devices
   - `OBSPlayer`, `OBSProbe`, `OBSConfig`, `OBSLog`, ...
4. **Orquestração** — UI + dispatch:
   - `OBSUI`, `OBSBridge`

Tipos compartilhados: `NoOBSTypes` (TGpuVendor, TEncoderCaps, TObsAudioDev).

| Unit                | Papel                                                                              |
|---------------------|------------------------------------------------------------------------------------|
| `OBSUI`             | Host WebView2, janela, message pump, splash, sync de tema, hotkey global (`WM_HOTKEY`) |
| `OBSBridge`         | Dispatcher central UI ↔ Delphi. Timers. Lifecycle de gravação via OBSEngine     |
| `LibOBS`            | Bindings Delphi para obs.dll (tipos opacos, structs, enums, funções cdecl)         |
| `OBSEngine`         | Motor de gravação: init libobs, scene, sources, output MKV (TOBSEngine class)      |
| `OBSEncoder`        | Detecção/seleção de encoder de vídeo (AV1/HEVC/H264 hw, x264 sw)                   |
| `OBSAudioTracks`    | `ComputeAudioTrackAssignments` (single source of truth) + `BuildTrackNames` + enum de devices via obs_properties |
| `OBSScene`          | Tipos puros (TOBSMonitor, TAudioDevice) + `ComputeCanvas` + `FilterEnabledMonitors`|
| `OBSStartupCheck`   | Valida presença de obs.dll, libav, WebView2 antes de criar janela                  |
| `OBSSingleInstance` | Literais de mutex/window-message compartilhados entre full e hibernate (Pegadinha #36) |
| `NoOBSTypes`        | Tipos compartilhados entre 2+ units (TGpuVendor, TEncoderCaps, TObsAudioDev)       |
| `FFmpegLib`         | **Bindings raw** das DLLs libav* + structs ABI + acessors low-level + helpers básicos (ToUtf8, ScanDurationByPackets) |
| `FFmpegOps`         | **Wrappers altos**: `RemuxFile`, `ExtractAudioTracks`, `ExtractFrameJpeg`          |
| `OBSPlayer`         | `TIdHTTPServer` em 127.0.0.1:porta-livre + cache de MP4 remuxado + extração de audio tracks |
| `OBSProbe`          | Inspeção de mídia via libavformat (codec, faixas, bitrate, duration com packet-scan fallback) |
| `OBSAudioWatch`     | `IMMNotificationClient` em Delphi puro pra detectar hot-plug de áudio              |
| `OBSConfig`         | Preferências em JSON com discriminator de versão (`%LOCALAPPDATA%\NoOBS\config.json`) |
| `OBSLog`            | Log centralizado em `%LOCALAPPDATA%\NoOBS\NoOBS.log`, append, thread-safe          |
| `WinPreview`        | **Win32**: `EnumDisplayMonitors` + `BitBlt` pra capturar thumb de cada monitor     |
| `WinAudioMeter`     | **WASAPI**: `IMMDeviceEnumerator` + `IAudioMeterInformation` pra peak L+R por device |
| `WinWebcam`         | **DirectShow**: enumera webcams com friendly name e resolução                      |

A UI HTML/CSS/JS está em `ui/index.html`. É compilada via `NoOBSResource.rc`
em `NoOBS.dres` e carregada por `WebView.NavigateToString`. Não há
dependência de arquivo em runtime.

### Mensageria UI ↔ Delphi

JS → Delphi: `window.chrome.webview.postMessage(jsonString)` →
`OBSUI.WebMessageReceived` → `OnUIMessage` → `OBSBridge.Dispatch`.

Delphi → JS: `OBSUI.PostJSON(jsonString)` →
`WebView.PostWebMessageAsJson` → `Bridge.handlers[type]`.

---

## Fluxo de gravação

```
App start (OBSUI.Run):
  • Single-instance mutex
  • EnforceRuntime (OBSStartupCheck): valida obs.dll, WebView2Loader,
    avformat-61.dll, avcodec-61.dll, avutil-59.dll, swscale-8.dll,
    obs-plugins/64bit/*. Se faltar critico → MessageBox + exit.
  • CoInitialize, DPI awareness, registra wndclass, cria janela
  • DoInit (em OBSBridge):
    • PushTheme
    • StartPlayerServer (HTTP local pra recordings/thumbs)
    • Lê RecordDir do config.json (fallback: %USERPROFILE%\Videos)
    • PushRecordings (cards via meta cacheada)
    • PushInit (sem audio enum ainda — WASAPI pode bloquear)
    • Worker thread: enumera audio (WASAPI) e empurra audio_sources_refreshed
    • ScanRecordingsMeta (worker: libav pra thumbs/duracoes faltantes)
    • Inicia TThumbTimerThread (captura de thumbs em thread propria)
    • Liga timer: TIMER_AUDIO_METER (100ms)
    • OBSAudioWatch.Start (hot-plug)
    • OBSRecordWatch.Start (file watcher na pasta de gravacoes)
    • Agenda TIMER_OBS_WARMUP (one-shot, 1500ms)
    • RegisterGlobalHotkey(HK_RECORD_TOGGLE, Ctrl+Shift+F9)

1.5s depois (TIMER_OBS_WARMUP):
  • Engine.EnsureInitialized (obs_startup, modules, video, audio)
  • DetectEncoderCaps → PushEncoderCaps (UI mostra logo GPU)
  → libobs pronto, 1a gravacao instantanea.

User toggle de monitor/mic/speaker:
  • SetSourceEnabled(id, enabled) em config.json
  • Se gravando e é audio: Engine.SetSourceMuted(id, not enabled)
  → Monitores/webcams: bloqueado durante gravação.

User clica "Iniciar Gravação" (ou hotkey Ctrl+Shift+F9):
  • PushRefreshBusy(True, 'starting')
  • Engine.EnsureInitialized (no-op se ja warm)
  • Gera OutputPath: RecordDir + 'NoOBS_yyyy-mm-dd_hh-nn-ss.mkv'
  • Engine.BuildAndStartRecording(OutputPath):
    - Enumera monitores (Win32) → FilterEnabledMonitors
    - Enumera webcams (DirectShow) → filtra por enabled
    - Enumera audio (libobs wasapi properties, com timeout 3s)
    - ComputeCanvas (bounding box side-by-side)
    - obs_reset_video com canvas calculado
    - Cria scene + sources de monitor/webcam/audio
    - Resolve monitor_id via obs_properties
    - Configura track bitmask (1 mix + N isolados, max 6)
    - SelectVideoEncoder: lê config codec, dispatch AV1/HEVC/H264/SW
    - Cria output ffmpeg_muxer (MKV) + obs_output_start
  • PushRecordingState

User clica "Parar Gravação":
  • Engine.StopRecording:
    - obs_output_stop (poll obs_output_active até parar)
    - Release: output, encoders, sources, scene
    - Core libobs permanece vivo
  • PushRecordingAdded
  → Pronto pra próxima gravação.

User clica em uma gravação pra tocar:
  • HandleRequestVideoInfo → Probe via libavformat (worker thread)
  • UI usa GetDirectUrl (HTTP local serve MKV)
  • Se falhar (chromium nao toca MKV), JS chama HandleRequestTranscode
    → EnsureCachedMp4 → FFmpegLib.RemuxFile (libavformat -c copy)
    → URL do MP4 cacheado
  • Painel de info: HandleRequestAudioTracks → FFmpegLib.ExtractAudioTracks
    → URLs M4A por track pra slaves <audio>

App exit (Shutdown):
  • UnregisterGlobalHotkey
  • Para workers (thumb thread, recordwatch, audiowatch, playerserver)
  • Engine.Teardown → obs_shutdown (com timeout 5s em worker thread)
```

---

## Pegadinhas conhecidas (já encontramos, não repita)

### 1. UTF-8 BOM em `.pas`

Delphi parsea `.pas` como cp1252 quando não há BOM. Acentos viram
mojibake e comentários com `é`/`ç`/`—` quebram blocos `{ }`.

**Sempre** salvar fontes Delphi com **UTF-8 BOM**. Se o Write tool não
adiciona, rode:

```ps
$path = 'C:\...\Foo.pas'
$content = [System.IO.File]::ReadAllText($path)
$utf8bom = New-Object System.Text.UTF8Encoding($true)
[System.IO.File]::WriteAllText($path, $content, $utf8bom)
```

### 2. Block comment com `{` interno

Comentário `{ JSON exemplo: { "k": "v" } }` quebra: o segundo `{` abre
novo bloco. **Use `(* ... *)`** pra comentários que contêm `{`/`}`.

### 3. Threading — main thread only para libobs

Todas as chamadas libobs devem ser feitas da main thread. `TThread.Queue`
pra voltar à main quando necessário.

**libav (FFmpegLib) é diferente**: pode rodar em worker thread, e
**deve** rodar — sempre fazemos remux/extract/thumb em workers.

### 4. `WakeMainThread` + `CheckSynchronize`

NoOBS usa `GetMessage`/`DispatchMessage` puro Win32 (sem VCL.Forms).
Isso significa que `TThread.Queue` não é drenado automaticamente.
`OBSUI.Run` configura:
- `WakeMainThread := Wakeup.Wake` (que faz `PostMessage(WM_NULL)`).
- `CheckSynchronize` após cada `DispatchMessage`.

Não remova essas linhas — sem elas, todo `TThread.Queue` trava.

### 5. `RegisterClass` ambíguo

`System.Classes.RegisterClass` (TPersistentClass) sombreia
`Winapi.Windows.RegisterClass` (WNDCLASS). Em OBSUI sempre escrever:

```pascal
Winapi.Windows.RegisterClass(wc);
```

### 6. Closures dentro de loop

```pascal
for i := 0 to High(Files) do
  TThread.Queue(nil, procedure begin DoSomething(Files[i]); end);
```

A captura é por referência. Quando a queued proc roda, `i` já foi
avançado. **Solução**: chamar uma procedure separada que recebe
`const APath: string` (novo stack frame = nova captura).

### 7. **NVENC tem limite hard de 8192 por dimensão**

GPUs Turing+ (NVENC HEVC moderno) rejeitam canvas > 8192 com
`NV_ENC_ERR_INVALID_PARAM "Width greater than supported value"`.

**Solução** em `OBSEngine` (via `ENCODER_MAX_DIM = 8192`):
clampa canvas proporcionalmente antes de `obs_reset_video`.
Dimensões ímpares ajustadas com `if Odd then Dec`.

### 8. **Audio-only: canvas preto 800×600**

Marcar só mic/speaker (0 monitor + 0 webcam): MKV exige stream de
vídeo válido. Solução: canvas preto 800×600 (OBS renderiza frame preto
contínuo). Audio tracks gravam normal. Fallback em
`OBSEngine.BuildAndStartRecording` quando `(W=0 or H=0)`.

### 9. **GDI handle leak em captura de tela**

`VclBmp.Handle := HBitmapManual` causa double-free quando o bitmap
ainda está SelectObject'ado em um DC. Esgota GDI handles em segundos
(timer de 1s).

**Solução** em `WinPreview.CaptureMonitorAsDataUrl`: usar `TBitmap`
puro com `SetSize` + `Canvas.Handle` — Delphi gerencia tudo
internamente.

### 10. **`MKV` é o único formato seguro contra queda de energia**

MP4 escreve cabeçalho no fim do arquivo — se travar, perde tudo.
MKV é frame-by-frame recuperável. `OBSEngine` usa `ffmpeg_muxer`
com path `.mkv`. Pra player, remuxamos sob demanda pra MP4 (cache).

### 11. **Canvas baseado em monitores enabled, não todos**

Canvas e bounding sempre consideram só os monitores marcados em
`enabled.NoOBS Monitor N` no `config.json`. `FilterEnabledMonitors`
é chamado em `OBSEngine.BuildAndStartRecording` antes de computar
bounding e criar sources.

Layout = compacto side-by-side (sum widths × max height).

### 12. **Mic meter precisa de `IAudioClient` ativo**

`IAudioMeterInformation.GetPeakValue` em endpoint de captura
(microfone) retorna **0** se não houver sessão de captura aberta. Em
endpoint de render (alto-falante) sempre funciona porque o Windows
mantém o mix do sistema.

`WinAudioMeter.StartCaptureForMeter`: ativa um `IAudioClient` shared
por mic, dá `Start` (não consumimos o buffer — deixa transbordar). O
meter passa a retornar valores reais.

### 13. **Plugins minimos pro libobs**

Plugins carregados (whitelist em `OBSEngine.LoadModules.WANTED`):
`obs-ffmpeg` (encoder áudio + muxer MKV), `obs-x264` (CPU encoder
fallback), `obs-nvenc` (HEVC/H264 NVIDIA, opcional), `win-capture`
(monitor_capture), `win-dshow` (webcam), `win-wasapi` (mics +
speakers loopback).

Usa `obs_open_module` + `obs_init_module` por plugin (não
`obs_load_all_modules`) — assim filtra plugins problemáticos como
`obs-websocket` (crash sem callbacks de frontend).

### 14. Encoders que viraram "Obsoleto" em OBS 31+

`jim_hevc_nvenc` foi marcado obsoleto em favor de `obs_nvenc_hevc_tex`.
`OBSEncoder.SelectVideoEncoder` testa o novo primeiro, com
fallback chain AV1 → HEVC → H.264 hardware → x264 CPU.

OBS recente retorna handle "phantom" pra encoder ID não registrado —
`EncoderTypeExists` valida via `obs_enum_encoder_types` antes de
criar.

### 15. **Versão major do FFmpeg precisa casar com OBS bundled**

`FFmpegLib` declara as DLLs por nome versionado: `avcodec-61.dll`,
`avformat-61.dll`, `avutil-59.dll`, `swscale-8.dll`. Essas vêm do
OBS portátil. Se o OBS subir de major (61→62, etc.), atualizar as
constantes `LIB_*` em `FFmpegLib.pas` e revalidar offsets de struct
(pegadinha #26).

### 16. WebView2 e `file://`

`NavigateToString` (que usamos) não tem origem de URL. `file://` e
URLs relativas não funcionam. Tudo que precisa ser servido (vídeos,
thumbs) vai pelo `OBSPlayer` que sobe um HTTP em 127.0.0.1:ephemeral.

### 17. Port 0 = ephemeral

`TIdHTTPServer.Bindings[0].Port := 0` faz o OS escolher porta livre.
Nunca colide. A porta efetiva fica em `Server.Bindings[0].Port` após
`Active := True`.

### 18. **obs_video_info struct alignment**

`gpu_conversion` é `ByteBool` (1 byte) seguido de 3 bytes de padding
(`_pad0: array[0..2] of Byte`) antes de `colorspace: Integer`.
Necessário pra casar com o layout C/MSVC x64.

### 19. **DLL delayed loading**

`LibOBS.pas` e `FFmpegLib.pas` declaram todas as funções como
`external '<dll>' delayed`. A DLL só é carregada na 1ª chamada. Como
`NoOBS.exe` mora em `bin\64bit\` junto das DLLs, o LoadLibrary
resolve naturalmente. `OBSStartupCheck` valida presença antes da
janela aparecer.

### 20. **monitor_id via obs_properties**

Para criar uma source `monitor_capture` com o monitor correto, é
necessário resolver o `monitor_id` interno do OBS (varia entre
reinstalls, drivers, etc). `OBSEngine.ResolveMonitorId` cria uma
source temporária, enumera a property list de `monitor_id`, e faz
match pelo sufixo `@ X,Y` na descrição.

### 21. **WM_TIMER suprimido em modal sizemove**

Quando o usuário arrasta/redimensiona a janela, o Windows entra num
loop modal interno que prioriza mouse-tracking e **suprime
`WM_TIMER`**. Timers via `SetTimer` simplesmente não disparam durante
o drag.

**Solução** pra captura de thumbs: `TThumbTimerThread` em
`OBSBridge` — thread própria com loop `Sleep` + invocação de
`PushMonitorThumbs`. Independe da message queue, continua tocando
durante drag.

`TIMER_AUDIO_METER` continua via `WM_TIMER` (meters durante drag
não importam — UI nem está sendo olhada).

### 22. **Captura de thumb em worker thread**

`BitBlt` + `TJPEGImage.SaveToStream` + base64 de monitor 4K leva
50-200ms — trava a UI se rodar na main. `PushMonitorThumbs` spawna
uma `TThread.CreateAnonymousThread` pra capturar + encodar; só o
`PostJSON` final volta pra main via `TThread.Queue`. `ThumbBusy`
(volatile bool) evita pile-up se uma captura demora mais que o
intervalo do tick.

### 23. **`textContent =` apaga filhos no DOM**

Em `ui/index.html`, ao atualizar o conteúdo de uma thumb depois que
a geração termina, NÃO fazer `thumb.textContent = ''` pra limpar —
isso apaga também o `.rec-check` e `.duration` que já estavam ali
como filhos. Use `placeholder.remove()` pra remover só o que precisa.

### 24. **`for i := 0 to Count - 1 do` com Cardinal/NativeUInt**

Padrão clássico de bug: `Count` vem de uma API externa (FFmpeg,
WASAPI) como `Cardinal`/`NativeUInt`. Se `Count = 0`, então
`Count - 1` underflowa pra `$FFFFFFFF` (~4 bilhões). Em Debug
(`{$Q+}`) dispara `EIntOverflow`; em Release o loop roda 4 bilhões
de vezes chamando API com índices inválidos — **freeze de minutos**.

**Solução**: sempre `if Count = 0 then Exit;` antes do loop quando
`Count` é tipo unsigned.

Locais já corrigidos: `WinAudioMeter.RebuildCache`,
`OBSEngine.ResolveMonitorId`, `OBSAudioTracks.EnumerateObsAudioDevicesRaw`.

### 25. **`av_frame_*` está em libavutil, NÃO libavcodec**

Erro clássico de portar bindings FFmpeg pra Delphi: declarar
`av_frame_alloc/free/unref` apontando pra `avcodec-61.dll`. Resultado:
`STATUS_DELAY_LOAD_FAILED (C06D007F)` ao tentar usar.

**Mapeamento correto das libs**:
| Família | DLL |
|---|---|
| `avformat_*`, `av_read_frame`, `av_seek_frame`, `avio_*` | libavformat |
| `avcodec_*`, `av_packet_*` | libavcodec |
| `av_frame_*` ⚠️ | **libavutil** |
| `av_dict_*`, `av_opt_*`, `av_image_*`, `av_log_*`, `av_rescale_q`, `av_free*` | libavutil |
| `sws_*` | libswscale |

### 26. **AVFormatContext offsets validados pra FFmpeg 7.x**

`AVFormatContext` NÃO é ABI-stable; layout específico de cada major
do libavformat. Pra FFmpeg 7.x (avformat-61):

| Campo | Offset | Tipo |
|---|---|---|
| `iformat*` | 8 | const AVInputFormat* |
| `pb*` | 32 | AVIOContext* |
| `nb_streams` | **44** | unsigned int |
| `streams**` | **48** | AVStream** |
| `duration` | 72 | int64_t |
| `bit_rate` | 80 | int64_t |

Se trocar major do libavformat (61→62), validar contra `avformat.h`
do novo release. Constantes em `FFmpegLib.pas` (`OFFS_*`).

### 27. **MKV de OBS pode ter `duration = 0` no global**

OBS escreve a tag `Duration` no EBML SegmentInfo só no
`av_write_trailer`. Se a gravação foi interrompida abruptamente
(crash, kill), o trailer não é escrito e tanto `AVFormatContext.duration`
quanto `AVStream.duration` ficam 0.

**Fallback** em `OBSProbe.Probe`: `FFmpegLib.ScanDurationByPackets`
varre todos os pacotes do arquivo e pega o `max(pts + duration)`.
É O(N pacotes) mas funciona pra qualquer arquivo. Cacheado em
`<hash>.dur` depois.

### 28. **`av_opt_set_int` não expõe "width"/"height" pra AVCodecContext**

A tabela de opções do AVCodecContext expõe `video_size` (string
`"WxH"`), `bit_rate`, `time_base`, etc — mas **não** `width` e
`height` individualmente. Setar via `av_opt_set_int(ctx, "width", N, 0)`
falha silenciosamente, encoder abre com 0×0, `avcodec_open2` erra.

**Solução** em `FFmpegLib.ExtractFrameJpeg`: usa `AVCodecParameters`
(ABI-stable, com campos diretos) + `avcodec_parameters_to_context`.
`time_base` ainda vai via `av_opt_set_q` (essa opção existe).

### 29. **UTF-8 nas strings que vão pra/vem do FFmpeg**

FFmpeg armazena e expõe TODAS as strings (metadata, paths, codec
names) em UTF-8 — independente do locale. Conversão via
`AnsiString(string)` usa `DefaultSystemCodePage` (cp1252 em pt-BR),
quebra acentos.

**Solução**:
- Escrita: `FFmpegLib.ToUtf8(s)` → `UTF8String` → `PAnsiChar`
- Leitura: `UTF8ToString(PAnsiChar)` → `string`
- Arquivos: `CreateFileW` (não `CreateFileA`) pra paths com acentos

`OBSEngine.ToAnsi` / `.FromAnsi` também usam UTF-8 (libobs segue
a mesma convenção do FFmpeg).

### 30. **WASAPI bloqueia quando o audio service está doente**

Remover o último mic conectado deixa o Windows Audio Service num
estado ruim: `IMMDeviceEnumerator.EnumAudioEndpoints` pode bloquear
**60+ segundos**. Se rodar na main thread, app trava.

**Solução**: `OBSBridge.DoRefreshAudio` roda toda enumeração WASAPI
(`InitAudio` + `RefreshAudioDevices` + `EnumerateAudioDevices`) em
worker thread; só `BuildAudioFromWin` (que lê do cache populado)
volta pra main via `TThread.Queue`.

`FFmpegLib.EnumerateObsAudioDevices` (usado pelo
`BuildAndStartRecording`) tem timeout de 3s — se travar, retorna
lista vazia e gravação inicia sem áudio em vez de pendurar a UI.

### 31. **`obs_shutdown` pode travar indefinidamente**

Se threads internas de áudio/render do libobs estão com trabalho
pendente, `obs_shutdown` pode esperar pra sempre. Em fechamento do
app isso impede o processo de morrer.

**Solução**: `OBSEngine.Teardown` chama `obs_shutdown` em worker
thread com `WaitForSingleObject(5000)`. Se timeout, abandona e segue
— o `ExitProcess` no fim do programa Delphi mata threads zumbi.

### 32. **`obs_set_output_source` antes de `obs_startup` → AV**

`ReleaseRecordingObjects` é chamado de `BuildAndStartRecording`,
`StopRecording` e `Teardown`. Se `EnsureInitialized` nunca rodou
(app fechou sem gravar) e `Teardown` chama `ReleaseRecordingObjects`,
o loop de `obs_set_output_source(i, nil)` faz dereference de
estruturas internas do libobs não inicializadas → AV.

**Solução**: guard `if not FInitialized then Exit;` no topo de
`ReleaseRecordingObjects`. Também `try/except` em volta de cada call
libobs de cleanup pra defensividade extra.

### 33. **Hotkey global precisa de `RegisterHotKey`+`WM_HOTKEY`**

`OBSUI.RegisterGlobalHotkey(id, modifiers, vk)` chama
`Winapi.Windows.RegisterHotKey`. Window proc trata `WM_HOTKEY` →
chama callback `OnUIHotkey` (registrado pelo `OBSBridge`).

Combinação pode falhar se outro app registrou globalmente
(`RegisterHotKey` retorna `False`). Não é fatal — app sobe sem o
atalho, só não funciona até o conflict sair.

`UnregisterGlobalHotkey` no `Shutdown` libera o registro.

### 34. **Validação de runtime na inicialização**

`OBSStartupCheck.EnforceRuntime` verifica antes de criar a janela:
- **Críticos** (sem isso o app não funciona): `obs.dll`,
  `WebView2Loader.dll`, `avcodec-61.dll`, `avformat-61.dll`,
  `avutil-59.dll`, `swscale-8.dll`, `data\libobs\`,
  `obs-plugins\64bit\`, `obs-ffmpeg.dll`
- **Recomendados** (degrada feature): outros plugins individuais

Se faltar crítico, mostra `MessageBox` claro listando o que falta e
aponta pro log. Sem isso, app crasha mais tarde com erro obscuro.

### 35. **FPU exception mask: Delphi default quebra libobs/libav**

Delphi por padrão habilita `EInvalidOp`, `EZeroDivide` e `EOverflow`
na FPU (mask = `[exDenormalized, exUnderflow, exPrecision]`). Já
libobs, libav, D3D11 e drivers de GPU assumem o default do Windows
(**todas** mascaradas) e rotineiramente produzem NaN/Inf em cálculos
internos — matrizes de projeção, scale 0/0 enquanto source assíncrona
inicializa, etc. Comportamento normal para C, mas em Delphi o flag
fica pendente na FPU.

Quando o controle volta pro Delphi, **qualquer** operação FP
posterior (até em outra unit, muito depois) dispara `EInvalidOp`
"Invalid floating point operation" com stack trace enganoso. Sintoma
clássico: gravação "Falha ao iniciar: Invalid floating point
operation" disparada N segundos depois, apontando linha aleatória
de aritmética Single inocente.

**Solução** em `initialization` de `OBSEngine.pas`:

```pascal
SetExceptionMask(exAllArithmeticExceptions);
```

`System.Math` precisa estar nas uses. Roda antes de qualquer chamada
pra obs.dll/libav (initialization de unit roda antes do `begin` do
.dpr). Mesma máscara cobre todas as threads do processo.

### 36. **Single-instance entre modos full e hibernate — strings devem casar**

NoOBS roda em dois modos no mesmo exe (full e `/hibernate`). Pra
que um modo enxergue o outro (single-instance + WM_SHOW_INSTANCE pra
promover hibernate→full), os dois precisam usar **exatamente os
mesmos literais** em:

- `CreateMutex(nil, False, MUTEX_NAME)` — nomes diferentes = dois
  kernel objects diferentes = mutex não detecta a outra instância.
- `RegisterWindowMessage(SHOW_MSG_NAME)` — strings diferentes
  retornam UINTs diferentes = mensagem enviada por um modo é
  ignorada pelo outro.

Já aconteceu: o `CLASS_NAME` do OBSUI foi renomeado de
`TNoOBSWindow` pra `TNoOBS`, mas o `MUTEX_NAME` literal em
`OBSHibernate.pas` continuou com o nome antigo. Resultado: full e
hibernate coexistiam (mutex distinto), e a hotkey/tray de um não
acordava o outro.

**Solução**: `OBSSingleInstance.pas` exporta as constantes; ambas as
units fazem `uses OBSSingleInstance`. Nunca duplicar esses literais.

---

## Caches

| Cache                                   | Conteúdo                                  |
|-----------------------------------------|-------------------------------------------|
| `%LOCALAPPDATA%\NoOBS\config.json`      | Preferências (versionado, theme, recordDir, enabled, codec) |
| `%LOCALAPPDATA%\NoOBS\NoOBS.log`        | Log único, append                         |
| `%LOCALAPPDATA%\NoOBS\cache\<hash>.dur` | Duração da gravação (texto, segundos)     |
| `%LOCALAPPDATA%\NoOBS\cache\<hash>.jpg` | Thumbnail (gerado via libav decode+sws+mjpeg) |
| `%LOCALAPPDATA%\NoOBS\cache\<hash>.mp4` | MP4 remuxado (libavformat `-c copy` equivalente) |
| `%LOCALAPPDATA%\NoOBS\cache\<hash>_aN.m4a` | Audio track isolada N (libavformat extract) |

`<hash>` = primeiros 10 bytes hex do SHA1 do path original.

GC: a cada `ScanRecordingsMeta` (no startup) os arquivos cuja
gravação original não existe mais são removidos. Também roda após
`HandleDeleteRecording`. Thumbs com tamanho < 100 bytes (sinal de
geração quebrada) são apagados e regenerados.

---

## Configuração persistida (`config.json`)

JSON com discriminator `"version": 1`. Se versão não bate, app
descarta e reescreve do zero (sem migração — preferências são
recuperáveis manualmente).

| Chave                            | Valor                                              |
|----------------------------------|----------------------------------------------------|
| `version`                        | `1` (discriminator)                                |
| `theme`                          | `"dark"` ou `"light"`                              |
| `recordDir`                      | path absoluto                                      |
| `codec`                          | `"auto"`, `"av1-hw"`, `"hevc-hw"`, `"h264-hw"`, `"h264-sw"` |
| `sources.monitors[name]`         | `true` / `false` (default: `true`)                 |
| `sources.mics[name]`             | `true` / `false` (default: `true`)                 |
| `sources.speakers[name]`         | `true` / `false` (default: `true`)                 |
| `sources.webcams[name]`          | `true` / `false` (default: `false`)                |

---

## Build

Abrir `NoOBS.dproj` no RAD Studio e compilar em Release/Win64
(`Shift+F9`). Ou via msbuild:

```bat
"C:\Program Files (x86)\Embarcadero\Studio\23.0\bin\rsvars.bat"
msbuild NoOBS.dproj /t:Build /p:config=Release /p:platform=Win64
```

Saída: `exe\bin\64bit\NoOBS.exe`.

Logs em `%LOCALAPPDATA%\NoOBS\NoOBS.log`. Tail em PowerShell:

```ps
Get-Content $env:LOCALAPPDATA\NoOBS\NoOBS.log -Wait -Tail 50
```

---

## Convenções de código

- **Comentários**: português.
- **Cabeçalho** em cada `.pas`: bloco `{...}` ou `(* ... *)` com
  descrição da unit.
- **Constantes mágicas** ficam no início da `implementation`, com
  comentário explicando.
- **Erros**: `Log()` + segue. Não trava o app por causa de uma falha
  isolada.
- **Mensagens UI ↔ Delphi**: campo `type` é o discriminator.
- **Strings pra FFI** (libobs/libav): UTF-8 sempre. Use
  `UTF8Encode`/`UTF8ToString` (ou `FFmpegLib.ToUtf8`), nunca
  `AnsiString(s)` direto que depende da locale.

---

## Quando fizer mudanças

1. **Bug de comportamento**: corrigir mantendo invariantes. Update
   CLAUDE.md se a correção descobre uma nova "pegadinha".
2. **Feature nova**: adicionar a unit nova ou estender existente.
   Update README.md (lista de recursos) e CLAUDE.md (arquitetura) se
   muda topologia.
3. **Mudança de protocolo UI↔Delphi**: documentar tipo novo no
   cabeçalho de `OBSBridge.pas` e no objeto `Bridge.handlers` em
   `ui/index.html`.
4. **Sempre rebuild antes de declarar pronto**: msbuild deve
   terminar com **0 Aviso(s) 0 Erro(s)**.
5. **Se mexeu em canvas/format/audio/encoder**: faça uma gravação
   real de teste e valide o `.mkv` abrindo no player do NoOBS
   (que invoca Probe via libavformat).
6. **Se mexeu em offsets de struct libav**: validar contra o header
   `.h` do release exato da major (avformat-61 = FFmpeg 7.x).

---

## Não faça

- **NÃO use** `TThread.Synchronize` em vez de `TThread.Queue` (pode
  deadlock).
- **NÃO use** `file://` no WebView — bloqueado.
- **NÃO chame** funções libobs de worker thread — main thread only.
- **NÃO permita** canvas > 8192 — NVENC rejeita. Clamp obrigatório
  via `ENCODER_MAX_DIM` em `OBSEngine`.
- **NÃO esqueça** o BOM em `.pas` novos.
- **NÃO bloqueie** a main thread por mais de ~1s sem mostrar overlay
  via `PushRefreshBusy(True, ...)` antes.
- **NÃO inicialize** libobs DIRETO no `DoInit` — pode travar o
  startup do app. Use o `TIMER_OBS_WARMUP` (one-shot ~1.5s depois)
  pra que a UI renderize antes do init bloquear ~300ms.
- **NÃO compute** canvas baseado em todos os monitores — só os
  enabled em config.json.
- **NÃO atribua** HBITMAP manual a `TBitmap.Handle` enquanto o bitmap
  está selecionado em DC — vaza GDI.
- **NÃO esqueça** padding em structs C (`obs_video_info` tem `_pad0`
  após `gpu_conversion`).
- **NÃO faça** `for i := 0 to Count - 1 do` quando `Count` é
  Cardinal/NativeUInt sem checar `Count = 0` antes — underflow.
- **NÃO converta** strings pra `AnsiString` diretamente antes de
  passar pro FFmpeg/libobs — use `UTF8Encode`/`ToUtf8` (pegadinha #29).
- **NÃO declare** `av_frame_*` em `avcodec-61.dll` — está em
  `avutil-59.dll` (pegadinha #25).
- **NÃO chame** `obs_set_output_source` ou outras funções libobs
  antes de `obs_startup` — `ReleaseRecordingObjects` deve checar
  `FInitialized` primeiro (pegadinha #32).
- **NÃO use** `av_opt_set_int(ctx, "width", ...)` em AVCodecContext —
  use `AVCodecParameters` + `avcodec_parameters_to_context`
  (pegadinha #28).

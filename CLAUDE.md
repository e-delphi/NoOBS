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

**NoOBS.exe roda dentro de `exe\bin\64bit\` (ao lado de obs.dll).**
Isso simplifica drasticamente o init: sem `SetDllDirectory`, sem
preload de DLLs, sem CWD trickery, sem cópia de helpers
(`obs-ffmpeg-mux.exe`). Toda a resolução de paths (DLLs, plugins,
shaders relativos a `../../data/libobs/`, helpers via
`os_get_executable_path_ptr`) funciona naturalmente porque o exe está
exatamente no lugar onde libobs espera.

---

## Stack

- **RAD Studio 12+ (Delphi)**, target **Win64**, `{$APPTYPE GUI}`.
- **WebView2** via `Winapi.WebView2` + `WebView2Loader.dll`.
- **libobs** (`obs.dll`) carregada in-process via `external delayed`.
- **Indy** (`TIdHTTPServer`, `TIdTCPClient`) — RTL.
- **OBS libs** em `exe/bin/64bit/` (obs.dll, plugins, ffmpeg).
- **ffmpeg + ffprobe** em `exe/bin/64bit/`.
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

| Unit             | Papel                                                                              |
|------------------|------------------------------------------------------------------------------------|
| `OBSUI`          | Host WebView2, janela, message pump, splash nativo, sync de tema na title bar      |
| `OBSBridge`      | Dispatcher central UI ↔ Delphi. Timers. Lifecycle de gravação via LibOBSEngine     |
| `LibOBS`         | Bindings Delphi para obs.dll (tipos opacos, structs, enums, funções cdecl)         |
| `LibOBSEngine`   | Motor de gravação: init libobs, scene, sources, encoders, output MKV               |
| `OBSScene`       | Tipos puros (TOBSMonitor, TAudioDevice) + `ComputeCanvas` + `FilterEnabledMonitors`|
| `OBSPlayer`      | `TIdHTTPServer` em 127.0.0.1:porta-livre + ffmpeg pra transcode/thumb com cache    |
| `OBSProbe`       | Wrapper sobre ffprobe pra extrair metadata de vídeos (codec, faixas, bitrate)      |
| `OBSAudioWatch`  | `IMMNotificationClient` em Delphi puro pra detectar hot-plug de áudio              |
| `OBSConfig`      | Preferências em JSON (`%LOCALAPPDATA%\NoOBS\config.json`)                          |
| `OBSLog`         | Log centralizado em `%LOCALAPPDATA%\NoOBS\NoOBS.log`, append, thread-safe          |
| `WinPreview`     | **Win32**: `EnumDisplayMonitors` + `BitBlt` pra capturar thumb de cada monitor     |
| `WinAudioMeter`  | **WASAPI**: `IMMDeviceEnumerator` + `IAudioMeterInformation` pra peak por device   |
| `WinWebcam`      | **DirectShow**: enumera webcams com friendly name e resolução                      |

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
App start (DoInit):
  • PushTheme
  • StartPlayerServer (HTTP local pra recordings/thumbs)
  • Lê RecordDir do config.json (fallback: %USERPROFILE%\Videos)
  • PushRecordings (cards via meta cacheada)
  • ScanRecordingsMeta (worker: ffmpeg pra thumbs/duracoes faltantes)
  • PushInit com sources via Win32 (monitores) e WASAPI (audio)
  • Inicia TThumbTimerThread (captura de thumbs em thread propria,
    nao depende de WM_TIMER que e suprimido durante modal sizemove)
  • Liga timer: TIMER_AUDIO_METER (100ms)
  • OBSAudioWatch.Start (hot-plug)
  • OBSRecordWatch.Start (file watcher na pasta de gravacoes)
  • Agenda TIMER_OBS_WARMUP (one-shot, 1500ms)
  → libobs ainda nao inicializou.

1.5s depois (TIMER_OBS_WARMUP):
  • Engine.EnsureInitialized (obs_startup, modules, video, audio)
  • ~300ms blocking — UI ja renderizou
  → libobs pronto, 1a gravacao instantanea.

User toggle de monitor/mic/speaker:
  • SetSourceEnabled(id, enabled) em config.json
  • Se gravando e é audio: Engine.SetSourceMuted(id, not enabled)
  → Monitores/webcams: bloqueado durante gravação.

User clica "Iniciar Gravação" (HandleRecordStart):
  • PushRefreshBusy(True, 'starting')
  • Engine.EnsureInitialized (1ª vez: obs_startup, load modules,
    obs_reset_video/audio)
  • Gera OutputPath: RecordDir + 'NoOBS_yyyy-mm-dd_hh-nn-ss.mkv'
  • Engine.BuildAndStartRecording(OutputPath):
    - Enumera monitores (Win32) → FilterEnabledMonitors
    - Enumera webcams (DirectShow) → filtra por enabled
    - Enumera audio (WASAPI) → filtra por enabled
    - ComputeCanvas (bounding box side-by-side)
    - obs_reset_video com canvas calculado
    - Cria scene + sources de monitor/webcam/audio
    - Resolve monitor_id via obs_properties
    - Configura track bitmask (1 mix + N isolados, max 6)
    - Seleciona encoder: HEVC > H.264 hardware > x264 CPU
    - Cria output ffmpeg_muxer (MKV) + obs_output_start
  • PushRecordingState
  • PushRefreshBusy(False, 'starting')

User clica "Parar Gravação" (HandleRecordStop):
  • Engine.StopRecording:
    - obs_output_stop (poll obs_output_active até parar)
    - Release: output, encoders, sources, scene
    - Core libobs permanece vivo
    - Retorna OutputPath
  • PushRecordingAdded
  → Pronto pra próxima gravação (sem re-init).

App exit (Shutdown):
  • Engine.Teardown → obs_shutdown
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
avançado. **Solução**: chamar uma procedure separada que recebe `const APath: string`
(novo stack frame = nova captura).

### 7. **NVENC tem limite hard de 8192 por dimensão**

GPUs Turing+ (NVENC HEVC moderno) rejeitam canvas > 8192 com
`NV_ENC_ERR_INVALID_PARAM "Width greater than supported value"`.

**Solução** em `LibOBSEngine` (via `ENCODER_MAX_DIM = 8192`):
clampa canvas proporcionalmente antes de `obs_reset_video`.
Dimensões ímpares ajustadas com `if Odd then Dec`.

### 8. **Audio-only: canvas preto 800×600**

Marcar só mic/speaker (0 monitor + 0 webcam): MKV exige stream de
vídeo válido. Solução: canvas preto 800×600 (OBS renderiza frame preto
contínuo). Audio tracks gravam normal. Fallback em
`LibOBSEngine.BuildAndStartRecording` quando `(W=0 or H=0)`.

### 9. **GDI handle leak em captura de tela**

`VclBmp.Handle := HBitmapManual` causa double-free quando o bitmap
ainda está SelectObject'ado em um DC. Esgota GDI handles em segundos
(timer de 1s).

**Solução** em `WinPreview.CaptureMonitorAsDataUrl`: usar `TBitmap`
puro com `SetSize` + `Canvas.Handle` — Delphi gerencia tudo
internamente.

### 10. **`MKV` é o único formato seguro contra queda de energia**

MP4 escreve cabeçalho no fim do arquivo — se travar, perde tudo.
MKV é frame-by-frame recuperável. `LibOBSEngine` usa `ffmpeg_muxer`
com path `.mkv`.

### 11. **Canvas baseado em monitores enabled, não todos**

Canvas e bounding sempre consideram só os monitores marcados em
`enabled.NoOBS Monitor N` no `config.json`. `FilterEnabledMonitors`
é chamado em `LibOBSEngine.BuildAndStartRecording` antes de computar
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

Plugins carregados (whitelist em `LibOBSEngine.LoadModules.WANTED`):
`obs-ffmpeg` (encoder áudio + muxer MKV), `obs-x264` (CPU encoder
fallback), `obs-nvenc` (HEVC/H264 NVIDIA, opcional), `win-capture`
(monitor_capture), `win-dshow` (webcam), `win-wasapi` (mics +
speakers loopback).

Usa `obs_open_module` + `obs_init_module` por plugin (não
`obs_load_all_modules`) — assim filtra plugins problemáticos como
`obs-websocket` (crash sem callbacks de frontend).

### 14. Encoders que viraram "Obsoleto" em OBS 31+

`jim_hevc_nvenc` foi marcado obsoleto em favor de `obs_nvenc_hevc_tex`.
`LibOBSEngine.SelectVideoEncoder` testa o novo primeiro, com
fallback chain HEVC → H.264 hardware → x264 CPU.

OBS recente retorna handle "phantom" pra encoder ID não registrado —
`EncoderTypeExists` valida via `obs_enum_encoder_types` antes de
criar.

### 15. **ffmpeg/ffprobe vivem ao lado do NoOBS.exe**

Os binários ficam em `bin/64bit/` junto do `NoOBS.exe`, `obs.dll`, e
das DLLs `avcodec/avformat/avutil/sw*`. A versão major do FFmpeg
precisa **casar** com a do OBS bundled (atualmente 7.x = avcodec-61).

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

`LibOBS.pas` declara todas as funções como `external 'obs.dll' delayed`.
A DLL só é carregada na 1ª chamada. Como `NoOBS.exe` mora em
`bin\64bit\` junto da `obs.dll`, o LoadLibrary resolve naturalmente
— sem `SetDllDirectory`/`AppendObsBinToPath` mágica.

### 20. **monitor_id via obs_properties**

Para criar uma source `monitor_capture` com o monitor correto, é
necessário resolver o `monitor_id` interno do OBS (varia entre
reinstalls, drivers, etc). `LibOBSEngine.ResolveMonitorId` cria uma
source temporária, enumera a property list de `monitor_id`, e faz
match pelo sufixo `@ X,Y` na descrição.

### 21. **WM_TIMER suprimido em modal sizemove**

Quando o usuário arrasta/redimensiona a janela, o Windows entra num
loop modal interno (`DefWindowProc` → `WM_SYSCOMMAND` com
`SC_MOVE`/`SC_SIZE`) que prioriza mouse-tracking e **suprime
`WM_TIMER`**. Timers via `SetTimer` simplesmente não disparam durante
o drag.

**Solução** pra captura de thumbs (que precisa ser contínua):
`TThumbTimerThread` em `OBSBridge` — thread própria com loop
`Sleep` + invocação de `PushMonitorThumbs`. Independe da message
queue, continua tocando durante drag. A entrega ao WebView2 ainda
depende da main thread (gated pelo modal loop), mas a captura fica
pronta encodada — o primeiro frame pós-drag é fresco em vez de
esperar mais 1s.

`TIMER_AUDIO_METER` continua via `WM_TIMER` (meters durante drag
não importam — UI nem está sendo olhada).

### 22. **Captura de thumb em worker thread**

`BitBlt` + `TJPEGImage.SaveToStream` + base64 de monitor 4K leva
50-200ms — trava a UI se rodar na main. `PushMonitorThumbs` spawna
uma `TThread.CreateAnonymousThread` pra capturar + encodar; só o
`PostJSON` final volta pra main via `TThread.Queue`. `ThumbBusy`
(volatile bool) evita pile-up se uma captura demora mais que o
intervalo do tick.

VCL `TBitmap` / `TJPEGImage` / `TNetEncoding.Base64` são todos
thread-safe quando criados localmente no worker.

### 23. **`textContent =` apaga filhos no DOM**

Em `ui/index.html`, ao atualizar o conteúdo de uma thumb depois que
o ffmpeg gera a imagem, NÃO fazer `thumb.textContent = ''` pra
limpar — isso apaga também o `.rec-check` e `.duration` que já
estavam ali como filhos. Use `placeholder.remove()` pra remover só
o que precisa, ou itere por children.

---

## Caches

| Cache                                   | Conteúdo                                  |
|-----------------------------------------|-------------------------------------------|
| `%LOCALAPPDATA%\NoOBS\config.json`      | Preferências (theme, recordDir, enabled.*) |
| `%LOCALAPPDATA%\NoOBS\NoOBS.log`        | Log único, append                         |
| `%LOCALAPPDATA%\NoOBS\cache\<hash>.dur` | Duração da gravação (texto, segundos)     |
| `%LOCALAPPDATA%\NoOBS\cache\<hash>.jpg` | Thumbnail (320×180)                       |
| `%LOCALAPPDATA%\NoOBS\cache\<hash>.mp4` | MP4 transcodado (só quando precisa tocar) |

`<hash>` = primeiros 10 bytes hex do SHA1 do path original.

GC: a cada `ScanRecordingsMeta` (no startup) os arquivos cuja
gravação original não existe mais são removidos. Também roda após
`HandleDeleteRecording`.

---

## Configuração persistida (`config.json`)

| Chave                            | Valor                                              |
|----------------------------------|----------------------------------------------------|
| `theme`                          | `"dark"` ou `"light"`                              |
| `recordDir`                      | path absoluto                                      |
| `enabled.NoOBS Monitor N`        | `"true"` ou `"false"` (default: `true`)            |
| `enabled.NoOBS Mic - <X>`        | `"true"` ou `"false"` (default: `true`)            |
| `enabled.NoOBS Out - <X>`        | `"true"` ou `"false"` (default: `true`)            |
| `enabled.NoOBS Webcam - <Name>`  | `"true"` ou `"false"` (default: `false` — opt-in)  |

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
   real de teste e valide o `.mkv` com `ffprobe -i <arquivo>`.

---

## Não faça

- **NÃO use** `TThread.Synchronize` em vez de `TThread.Queue` (pode
  deadlock).
- **NÃO use** `file://` no WebView — bloqueado.
- **NÃO chame** funções libobs de worker thread — main thread only.
- **NÃO permita** canvas > 8192 — NVENC rejeita. Clamp obrigatório
  via `ENCODER_MAX_DIM` em `LibOBSEngine`.
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

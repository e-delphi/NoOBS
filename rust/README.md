# NoOBS em Rust (sem Tauri)

Porte do projeto Delphi (`../src/*.pas`) para Rust. Mesma arquitetura em 4
camadas, mesma janela Win32 + WebView2 criada à mão (`windows` +
`webview2-com`), mesma UI em `exe\bin\64bit\ui\` e mesmo protocolo
`postMessage` UI ↔ backend — a UI não muda.

> Este porte **não foi compilado** (pedido da issue #1: foco na conversão).
> Ao compilar no Windows (`cargo build --release` em `rust/`), espere ajustes
> de assinatura nas versões de `windows`/`webview2-com`.

## Mapa Delphi → Rust

| Delphi                | Rust (`rust/src/`)        | Estado |
|-----------------------|---------------------------|--------|
| `OBSSingleInstance`   | `single_instance.rs`      | portado |
| `NoOBSTypes`          | `types.rs`                | portado |
| `OBSScene`            | `scene.rs` (+ testes)     | portado |
| `OBSConfig`           | `config.rs`               | portado |
| `OBSLog`              | `log.rs` (`log!`)         | portado |
| `OBSLang`             | `lang.rs` (+ testes)      | portado |
| `OBSEncoder`          | `encoder.rs` (+ testes)   | portado (tabelas, limites, plano de settings, `select_video_encoder`, `detect_encoder_caps`, `predict_video_encoder_id`, overrides de keyframe/qualidade) |
| `LibOBS`              | `ffi/obs.rs`              | portado (tabela de funções + `ObsData` RAII) |
| `FFmpegLib`           | `ffi/ffmpeg.rs`           | portado (structs, offsets, scan de duração, fps); faltam decode/encode/swscale |
| `OBSProbe`            | `probe.rs`                | portado |
| `OBSRtwq`             | `rtwq.rs`                 | portado |
| `OBSStartupCheck`     | `startup_check.rs`        | portado |
| `OBSUI`               | `ui.rs` + `mainthread.rs` | portado (janela, WebView2, hotkey, ciclo de vida); faltam bandeja, splash com texto, dark menu |
| `OBSBridge`           | `bridge.rs` + `bridge/recording.rs` + `bridge/devices.rs` | em andamento: init + warmup, tema, idioma, open_url; **gravação** (`record_start`/`record_stop`, stop assíncrono #41, tique de 1 s, nome por modelo, caps de encoder, hotkey padrão); **tela inicial** (`init` com monitores/mics/saídas/webcams, `audio_sources_refreshed`, refresh por hot-plug com diff e dedup por assinatura, medidores de nível, mudo do endpoint, prévias dos monitores, toggles/ocultar/perfil automático). Pendências marcadas como `TODO(<Unit>)` no código, uma por unit ainda não portada |
| `OBSEngine`           | `engine.rs` (+ testes)    | portado inteiro: init/módulos, grafo de captura, gravação, stop por sinal, `replay_buffer` (#62), janela (#66), teardown + watchdog |
| `OBSAudioTracks`      | `audio_tracks.rs` (+ testes) | portado (atribuição de faixas, nomes, enumeração com timeout) |
| `WinPreview`          | `win_preview.rs`          | portado: monitores (+ taxa de atualização) e prévias BitBlt → JPEG (RAII de GDI, pegadinha #9) |
| `WinAudioMeter`       | `win_audio_meter.rs`      | portado: enumeração com cache, medidor por canal, sessão de captura por mic (#12), mudo do endpoint (#59), política de abertura |
| `WinWebcam`           | `win_webcam.rs` (+ testes)| portado (DirectShow, filtro de câmeras de software, nome por fabricante) |
| `OBSAudioWatch`       | `audio_watch.rs`          | portado (`IMMNotificationClient` via `#[implement]`) |
| `OBSAudioFilters`     | — | a portar |
| `FFmpegOps`, `FFmpegExport`, `ExportCaptions`    | — | a portar |
| `OBSPlayer`, `OBSFileMeta`, `OBSTranscribe`, `OBSLocalAsr`, `OBSNumbersPt` | — | a portar |
| `OBSHibernate`, `OBSTray`, `OBSElevate`, `OBSHotkey`, `OBSAutostart`, `OBSUpdate`, `Win*` | — | a portar |

## Como o Rust mudou o desenho (sem mudar o comportamento)

- **`TThread.Queue` + `CheckSynchronize`** → `mainthread::queue` + `WM_APP_WAKE`
  (closures `move`: some a pegadinha #6 de captura em laço).
- **`obs_data_*` + `try/finally`** → `ObsData` com `Drop`.
- **`external ... delayed`** → `libloading` + macro `dll_api!`: tabela de
  funções por DLL, carregada na primeira chamada; símbolo ausente falha cedo.
- **`ApplyConstantQuality`** → `plan_constant_quality` pura (devolve a lista de
  settings), testável sem a `obs.dll`; `apply_constant_quality` só aplica.
- **Cadeia `if MsgType = ... else if`** → `match` no campo `type`.
- **Underflow de `Count - 1` (pegadinha #24)** não existe: `for i in 0..n` com
  `n = 0` é vazio.
- **Regras que seguem valendo**: libobs só na main thread; UTF-8 nas strings
  para FFI; `obs_shutdown` na mesma thread do `obs_startup`; RTWQ antes do
  `CoInitialize`; BOM não se aplica (Rust é UTF-8).

Os testes unitários (`cargo test`) cobrem canvas, i18n e as tabelas de
qualidade/limites de encoder.

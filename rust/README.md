# NoOBS em Rust (sem Tauri)

Porte do projeto Delphi (`../src/*.pas`) para Rust. Mesma arquitetura em 4
camadas, mesma janela Win32 + WebView2 criada à mão (`windows` +
`webview2-com`), mesma UI em `exe\bin\64bit\ui\` e mesmo protocolo
`postMessage` UI ↔ backend — a UI não muda.

> Compila no Windows (`cargo build` em `rust/`; o `build.rs` usa o `rc.exe`
> do Windows SDK para embutir o ícone) e roda ao lado da `obs.dll`, em
> `exe\bin\64bit\`. `cargo test` cobre o que não depende das DLLs.

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
| `FFmpegLib`           | `ffi/ffmpeg.rs`           | portado inteiro: bindings de avformat/avcodec/avutil/swscale, structs/offsets (#26/#40), fallbacks de duração/bitrate/fps, RAII (`Input`/`Packet`/`Frame`/`CodecCtx`/`Sws`/`Dict`) |
| `OBSProbe`            | `probe.rs`                | portado |
| `OBSRtwq`             | `rtwq.rs`                 | portado |
| `OBSStartupCheck`     | `startup_check.rs`        | portado |
| `OBSUI`               | `ui.rs` + `mainthread.rs` | portado inteiro: janela, WebView2, splash com título, início escondido (off-screen até o WebView2 subir), fechar pra bandeja, restaurar contornando o anti-roubo de foco, overlay de gravando na taskbar (`ITaskbarList3`), menus no tema, tela cheia, título ao vivo, salto pra hibernação |
| `OBSBridge`           | `bridge.rs` + `bridge/recording.rs` + `bridge/devices.rs` + `bridge/library.rs` + `bridge/shell.rs` (bandeja, hibernação, atalho, janela) + `bridge/settings.rs` (`get_settings` e os `set_*` de gravação, pasta, biblioteca, auto-gravação por mic e atualização) + `bridge/folders.rs` (abrir/criar/renomear/excluir/mover pastas com migração do cache — #55 — e união de gravações) | em andamento (a galeria — listar, navegar, scan de miniaturas, tocar, info, waveform, keyframes, faixas, renomear, abrir, excluir — está em `library.rs`): init + warmup, tema, idioma, open_url; **gravação** (`record_start`/`record_stop`/`record_pause`, stop assíncrono #41, pausa por `obs_output_pause` com o relógio descontando a pausa, tique de 1 s, nome por modelo, caps de encoder, hotkey padrão); **tela inicial** (`init` com monitores/mics/saídas/webcams, `audio_sources_refreshed`, refresh por hot-plug com diff e dedup por assinatura, medidores de nível, mudo do endpoint, prévias dos monitores, toggles/ocultar/perfil automático). Pendências marcadas como `TODO(<Unit>)` no código, uma por unit ainda não portada |
| `OBSEngine`           | `engine.rs` (+ testes)    | portado inteiro: init/módulos, grafo de captura, gravação, stop por sinal, `replay_buffer` (#62), janela (#66), teardown + watchdog |
| `OBSAudioTracks`      | `audio_tracks.rs` (+ testes) | portado (atribuição de faixas, nomes, enumeração com timeout) |
| `WinPreview`          | `win_preview.rs`          | portado: monitores (+ taxa de atualização) e prévias BitBlt → JPEG (RAII de GDI, pegadinha #9) |
| `WinAudioMeter`       | `win_audio_meter.rs`      | portado: enumeração com cache, medidor por canal, sessão de captura por mic (#12), mudo do endpoint (#59), política de abertura |
| `WinWebcam`           | `win_webcam.rs` (+ testes)| portado (DirectShow, filtro de câmeras de software, nome por fabricante) |
| `OBSAudioWatch`       | `audio_watch.rs`          | portado (`IMMNotificationClient` via `#[implement]`) |
| `OBSAudioFilters`     | `audio_filters.rs` (+ `bridge/filters.rs`) | portado: lista/edição genérica das propriedades da INSTÂNCIA, cadeia por ordem de inserção, teste A/B pela fonte `noobs_audio_feed`; `get/set/reset/test_audio_filters` ligados |
| `FFmpegOps`           | `ffmpeg_ops.rs` (+ testes)| portado inteiro: remux, união, emenda do buffer por conteúdo (#62), extração de faixas, miniatura JPEG (#42) / PNG exato, picos de waveform, PCM mono em blocos, grade de keyframes (#61) |
| `OBSPlayer`           | `player.rs` (+ testes)    | portado inteiro: servidor HTTP (`TcpListener`, Range, CORS restrito), cache por hash do path (#55), `<hash>.json` sob lock, GC, rename |
| `OBSRecordWatch`      | `record_watch.rs`         | portado (`FindFirstChangeNotification`, debounce, `update_dir` com snapshot do callback — #54) |
| `FFmpegExport`, `ExportCaptions` (+ `TimeStretch`, `bridge/export.rs`) | `ffmpeg_export.rs`, `export_captions.rs`, `time_stretch.rs` | portado (falta a transcrição herdada, que entra com o OBSTranscribe) |
| `OBSFileMeta` | `file_meta.rs` | portado (bloco no fim do MKV/MP4, worker com pendentes persistidos, importação no `ensure_recording_meta`) |
| `OBSNumbersPt` | `numbers_pt.rs` | portado |
| `OBSLocalAsr` | `local_asr.rs` (+ `win_http.rs` WinHTTP com cancelamento, `win_proc.rs` `RunHidden`) | portado (instalação com retomada/SHA-256/espelho, servidor sob Job Object, reamostrador, pipeline de 2 passadas); falta a fila que o chama (OBSTranscribe) |
| `OBSTranscribe` | `transcribe.rs` (+ `transcribe/merge.rs`, `bridge/transcribe.rs`) | portado: fila persistida/reordenável/manual×automática/pausa, espera do servidor, 2 motores, dedup de eco por palavra, transcrição herdada pela exportação, diagnóstico WSL→Docker→container, falantes e busca. Handlers de `transcribe_*`, `local_asr_*`, `root_folders` ligados ao dispatch; o push de `transcribeHost`/`transcribeEngine` no `settings` entra com o PushSettings (etapa 9) |
| `OBSTray`, `OBSRecordIcon` | `tray.rs`, `record_icon.rs` | portado (ícone, menu, balão, ícone de gravando por DIB 32-bit) |
| `OBSHibernate` | `hibernate.rs` | portado (bandeja + atalho + watchers; promove a completo com `/start-record`, `/start-record-mic`, `/start-replay`) |
| `OBSHotkey` | `hotkey.rs` (+ testes) | portado (parse/format, pontuação OEM — #43, reservados) |
| `OBSAutostart`, `OBSElevate` | `autostart.rs`, `elevate.rs` | portado |
| `WinMicWatch`, `WinProcWatch` | `mic_watch.rs`, `proc_watch.rs` (+ testes) | portado (usados pela hibernação; o bridge os liga junto com o buffer e a auto-gravação) |
| `OBSVersion`, `OBSUpdate` | `version.rs` (+ testes), `update.rs` | portado (versão do `git describe` pelo `build.rs`; checagem diária ancorada às 6 h pelo WinHTTP) |
| `WinRecIndicator` | `rec_indicator.rs` (+ testes) | portado (overlay excluído da captura — #49 —, modos gravação/buffer, pausa, sai da frente de tela cheia, clique opcional, bolinha por GDI+) |
| `NoOBSLockDetector` | `lock_detector.rs` | portado (janela só-mensagens + `WTSRegisterSessionNotification`) |
| `WinWindows`, `WinGpuUsage` | — | a portar |

## Como o Rust mudou o desenho (sem mudar o comportamento)

- **`TThread.Queue` + `CheckSynchronize`** → `mainthread::queue` + `WM_APP_WAKE`
  (closures `move`: some a pegadinha #6 de captura em laço).
- **`obs_data_*` + `try/finally`** → `ObsData` com `Drop`.
- **`external ... delayed`** → `libloading` + macro `dll_api!`: tabela de
  funções por DLL, carregada na primeira chamada; símbolo ausente falha cedo.
- **`ApplyConstantQuality`** → `plan_constant_quality` pura (devolve a lista de
  settings), testável sem a `obs.dll`; `apply_constant_quality` só aplica.
- **Cadeia `if MsgType = ... else if`** → `match` no campo `type`.
- **Callbacks do `ui` para o bridge** saem de um `Mutex<Hooks>`: copie o
  ponteiro e chame FORA da trava. Chamar dentro travou a main thread na 1ª
  gravação pelo atalho (o handler esconde a janela, e o `hide_to_tray` lê
  outro hook): o `Mutex` do Rust não é reentrante.
- **Underflow de `Count - 1` (pegadinha #24)** não existe: `for i in 0..n` com
  `n = 0` é vazio.
- **Regras que seguem valendo**: libobs só na main thread; UTF-8 nas strings
  para FFI; `obs_shutdown` na mesma thread do `obs_startup`; RTWQ antes do
  `CoInitialize`; BOM não se aplica (Rust é UTF-8).

Os testes unitários (`cargo test`) cobrem canvas, i18n, as tabelas de
qualidade/limites de encoder, faixas de áudio, atalhos e o filtro de apps.

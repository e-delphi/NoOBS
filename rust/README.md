# NoOBS em Rust (sem Tauri, sem OBS)

Porte do projeto Delphi (`../src/*.pas`) para Rust, com motor de captura
PRÓPRIO no lugar da libobs: Direct3D 11 + Windows Graphics Capture (telas,
janela, área), Media Foundation (webcam), WASAPI (microfones, som do
sistema, som de um app) e os encoders do libavcodec — `docs/motor-proprio.md`.
Mesma janela Win32 + WebView2 criada à mão (`windows` + `webview2-com`) e
mesmo protocolo `postMessage` UI ↔ backend.

> Compila no Windows (`cargo build` em `rust/`; o `build.rs` usa o `rc.exe`
> do Windows SDK para embutir o ícone e embute a UI e as traduções).
> `cargo test` cobre o que não grava a tela.

## Compilar, rodar e empacotar

Esta pasta é um projeto **independente** do Delphi da raiz: nada aqui lê
arquivos de fora de `rust\`, e o Delphi não lê nada daqui.

```
rust\
  src\                 código Rust
  ui\                  a interface (HTML/CSS/JS) — vai DENTRO do exe
  lang\                traduções (pt-BR/en/es) — vão DENTRO do exe
  exe\                 runtime de desenvolvimento: NoOBS.exe + DLLs do FFmpeg
  build.bat            compila e põe o exe em exe\NoOBS.exe
  make-installer-rust.bat, installer.nsi, icon.ico
```

```bat
build.bat
exe\NoOBS.exe
```

`build.bat debug` usa o build de debug (compila mais rápido).
`make-installer-rust.bat` chama o `build.bat` e gera
`NoOBS-<versão>-rust.exe` nesta pasta (precisa do NSIS).

- **O runtime é o `NoOBS.exe` e 9 DLLs do FFmpeg**: `avcodec-61`,
  `avformat-61`, `avutil-59`, `swscale-8` e as dependências delas
  (`swresample-5`, `libx264-164`, `zlib`, `srt`, `librist`). Nada de
  `obs.dll`, plugins, `data\` ou pastas ao lado (47 MB, eram 81).
- **A UI e as traduções vão dentro do exe** (`build.rs` → `assets.rs`). As
  traduções são lidas da memória, e a UI também: o WebView2 pede cada arquivo
  de `https://noobs.app/*` e o app responde com os bytes do exe
  (`WebResourceRequested` em `ui::start_navigate`) — origin `https://` real
  (pegadinha #16), nada escrito no disco. Editar a UI pede `cargo build` (o
  `build.rs` vigia `ui\` e `lang\`).
- **Sem `WebView2Loader.dll`**: no alvo MSVC o `webview2-com-sys` liga o
  `WebView2LoaderStatic.lib`, então o loader já vai dentro do exe.
- **No git**: código, `ui\`, `lang\`, scripts, `installer.nsi`, `icon.ico`.
  A UI e as traduções são uma cópia própria: mudança feita na UI do Delphi
  não chega aqui sozinha (e vice-versa).
- **Fora do git**: `exe\` inteiro. Numa máquina nova, copie as 9 DLLs do
  `bin\64bit\` do OBS portátil 32.1.2 pra `exe\` (o `build.bat` avisa).
- **Instalado**: o app fica em `%LOCALAPPDATA%\NoOBS\bin\` (só o exe e as
  DLLs). Instalar por cima de uma versão com a libobs apaga o `bin\64bit\`,
  o `data\` e o `obs-plugins\` antigos.

As duas versões instalam no mesmo lugar e com o mesmo nome: um instalador
substitui o outro. Rodando lado a lado, feche uma antes de abrir a outra —
usam o mesmo mutex de instância única, o mesmo `config.json` e o mesmo cache.

## Testes (sempre fora do app)

Nenhum código de teste vai no `NoOBS.exe`:

- `cargo test --release` — unidade, as mensagens da página que não gravam e
  a mídia com o FFmpeg real sobre as amostras de `tests/fixtures/media/`.
- `cargo test --release --features app-tests --test mensagens [-- filtro]` —
  as mensagens da página pelo dispatch, gravando a tela de verdade (gravar,
  pausa, fontes, janela, área, buffer, teste de qualidade, som do sistema).
- `cargo test --release --features app-tests --test captura [-- filtro]` —
  o motor de captura direto (cor, encoders, sincronia, buffer, pausa...).

A feature `app-tests` só torna os módulos públicos pros executáveis de
`tests/` — ver `modulos!` em `src/lib.rs`.

## Mapa Delphi → Rust

| Delphi                | Rust (`rust/src/`)        | Estado |
|-----------------------|---------------------------|--------|
| `OBSSingleInstance`   | `single_instance.rs`      | portado |
| `NoOBSTypes`          | `types.rs`                | portado |
| `OBSScene`            | `scene.rs`                | portado (o `ComputeCanvas` ficou de fora: sem uso também no Delphi) |
| `OBSConfig`           | `config.rs`               | portado |
| `OBSLog`              | `log.rs` (`log!`)         | portado |
| `OBSLang`             | `lang.rs` (+ testes)      | portado |
| `OBSEncoder`          | `encoder.rs` (+ testes), `capture/caps.rs` | nível 0..10 → CRF do x264 (traduzido por encoder pelo `apply_quality_options` da exportação), taxa do áudio, override do teste de qualidade; a detecção abre e codifica um quadro em cada encoder do libavcodec (`capture/caps.rs`) |
| `LibOBS`, `OBSRtwq`   | —                         | saíram com a libobs (motor próprio) |
| `FFmpegLib`           | `ffi/ffmpeg.rs`           | portado inteiro: bindings de avformat/avcodec/avutil/swscale, structs/offsets (#26/#40), fallbacks de duração/bitrate/fps, RAII (`Input`/`Packet`/`Frame`/`CodecCtx`/`Sws`/`Dict`) |
| `OBSProbe`            | `probe.rs`                | portado |
| `OBSStartupCheck`     | `startup_check.rs`        | confere as 9 DLLs do FFmpeg |
| `OBSUI`               | `ui.rs` + `mainthread.rs` | portado inteiro: janela, WebView2, splash com título, início escondido (off-screen até o WebView2 subir), fechar pra bandeja, restaurar contornando o anti-roubo de foco, overlay de gravando na taskbar (`ITaskbarList3`), menus no tema, tela cheia, título ao vivo, salto pra hibernação |
| `OBSBridge`           | `bridge.rs` + `bridge/recording.rs` + `bridge/devices.rs` + `bridge/library.rs` + `bridge/shell.rs` (bandeja, hibernação, atalho, janela) + `bridge/settings.rs` (`get_settings` e os `set_*` de gravação, pasta, biblioteca, auto-gravação por mic e atualização) + `bridge/folders.rs` (abrir/criar/renomear/excluir/mover pastas com migração do cache — #55 — e união de gravações) | portado (a galeria — listar, navegar, scan de miniaturas, tocar, info, waveform, keyframes, faixas, renomear, abrir, excluir — está em `library.rs`): init + warmup, tema, idioma, open_url; **gravação** (`record_start`/`record_stop`/`record_pause`, stop assíncrono #41, pausa com o relógio descontando a pausa, tique de 1 s, nome por modelo, caps de encoder, hotkey padrão); **tela inicial** (`init` com monitores/mics/saídas/webcams, `audio_sources_refreshed`, refresh por hot-plug com diff e dedup por assinatura, medidores de nível, mudo do endpoint, prévias dos monitores, toggles/ocultar/perfil automático). Todos os tipos de mensagem do `Dispatch` do Delphi têm handler |
| `OBSEngine`           | `engine.rs` (interface `CaptureEngine`), `native_engine.rs`, `capture/*` | motor próprio: telas, janela (#66), área, webcams, áudio (inclusive o de UM app), gravação, pausa, buffer em memória com trecho salvo sem buraco nem repetição — `docs/motor-proprio.md` |
| `OBSAudioTracks`      | `audio_tracks.rs` (+ testes) | atribuição de faixas SEM o teto de 6 da libobs (cada dispositivo marcado na sua faixa), nomes |
| `WinPreview`          | `win_preview.rs`          | portado: monitores (+ taxa de atualização) e prévias BitBlt → JPEG (RAII de GDI, pegadinha #9) |
| `WinAudioMeter`       | `win_audio_meter.rs`      | portado: enumeração com cache, medidor por canal, sessão de captura por mic (#12), mudo do endpoint (#59), política de abertura |
| `WinWebcam`           | `win_webcam.rs` (+ testes)| portado (DirectShow, filtro de câmeras de software, nome por fabricante) |
| `OBSAudioWatch`       | `audio_watch.rs`          | portado (`IMMNotificationClient` via `#[implement]`) |
| `OBSAudioFilters`     | —                         | saiu: os filtros eram do plugin `obs-filters` |
| `FFmpegOps`           | `ffmpeg_ops.rs` (+ testes)| remux, união, extração de faixas, miniatura JPEG (#42) / PNG exato, picos de waveform, PCM mono em blocos, grade de keyframes (#61) |
| `OBSPlayer`           | `player.rs` (+ testes)    | portado inteiro: servidor HTTP (`TcpListener`, Range, CORS restrito), cache por hash do path (#55), `<hash>.json` sob lock, GC, rename |
| `OBSRecordWatch`      | `record_watch.rs`         | portado (`FindFirstChangeNotification`, debounce, `update_dir` com snapshot do callback — #54) |
| `FFmpegExport`, `ExportCaptions` (+ `TimeStretch`, `bridge/export.rs`) | `ffmpeg_export.rs`, `export_captions.rs`, `time_stretch.rs` | portado (com a transcrição herdada) |
| `OBSFileMeta` | `file_meta.rs` | portado (bloco no fim do MKV/MP4, worker com pendentes persistidos, importação no `ensure_recording_meta`) |
| `OBSNumbersPt` | `numbers_pt.rs` | portado |
| `OBSLocalAsr` | `local_asr.rs` (+ `win_http.rs` WinHTTP com cancelamento, `win_proc.rs` `RunHidden`) | portado (instalação com retomada/SHA-256/espelho, servidor sob Job Object, reamostrador, pipeline de 2 passadas) |
| `OBSTranscribe` | `transcribe.rs` (+ `transcribe/merge.rs`, `bridge/transcribe.rs`) | portado: fila persistida/reordenável/manual×automática/pausa, espera do servidor, 2 motores, dedup de eco por palavra, transcrição herdada pela exportação, diagnóstico WSL→Docker→container, falantes e busca. Handlers de `transcribe_*`, `local_asr_*`, `root_folders` ligados ao dispatch |
| `OBSTray`, `OBSRecordIcon` | `tray.rs`, `record_icon.rs` | portado (ícone, menu, balão, ícone de gravando por DIB 32-bit) |
| `OBSHibernate` | `hibernate.rs` | portado (bandeja + atalho + watchers; promove a completo com `/start-record`, `/start-record-mic`, `/start-replay`) |
| `OBSHotkey` | `hotkey.rs` (+ testes) | portado (parse/format, pontuação OEM — #43, reservados) |
| `OBSAutostart`, `OBSElevate` | `autostart.rs`, `elevate.rs` | portado |
| `WinMicWatch`, `WinProcWatch` | `mic_watch.rs`, `proc_watch.rs` (+ testes) | portado (usados pela hibernação; o bridge os liga junto com o buffer e a auto-gravação) |
| `OBSVersion`, `OBSUpdate` | `version.rs` (+ testes), `update.rs` | portado (versão do `git describe` pelo `build.rs`; checagem diária ancorada às 6 h pelo WinHTTP) |
| `WinRecIndicator` | `rec_indicator.rs` (+ testes) | portado (overlay excluído da captura — #49 —, modos gravação/buffer, pausa, sai da frente de tela cheia, clique opcional, bolinha por GDI+) |
| `NoOBSLockDetector` | `lock_detector.rs` | portado (janela só-mensagens + `WTSRegisterSessionNotification`) |
| `WinGpuUsage` | `gpu_usage.rs` (+ testes) | portado (PDH por motor, leitura quebrada descartada) |
| Buffer em memória (seção do `OBSBridge`) | `bridge/replay.rs` | (#62): ligar/desligar, salvar (o buffer esvazia, corte num keyframe forçado), gravação que continua o buffer num arquivo só, atalho próprio, indicador no modo buffer, apps que ligam sozinhos, limites por RAM, logs de RAM e desempenho |
| `WinWindows` | `win_windows.rs` (+ testes, `bridge/window_record.rs`) | portado (#66): janelas da barra de tarefas, UWP pela filha de outro processo, string `titulo:classe:exe` escapada como o OBS fazia, prévia por `PrintWindow(PW_RENDERFULLCONTENT)` recortada na área cliente; `list_capture_windows`/`record_window` com som do app |
| Teste de qualidade (seção do `OBSBridge`) | `bridge/video_test.rs` | portado (#64): amostra de 5 s no nível máximo pelo caminho da gravação, 11 níveis refeitos pela exportação (MP4 + PNG do meio), cancelável, segura a hibernação |

## Recursos novos (só na versão Rust)

A partir da v0.44, recursos novos entram só aqui; o Delphi é a versão legada.

### Capturar uma área da tela

O botão "Capturar..." (antes "Gravar uma janela") abre o seletor com duas
abas: **Janela** (o de sempre) e **Área**.

- **Seleção** (`area_select.rs`): uma janela layered do tamanho da área de
  trabalho virtual escurece todos os monitores; o usuário arrasta o
  retângulo e soltar já começa a gravar. Esc ou o botão direito cancelam.
  O "furo" no escuro é uma cor-chave (transparente de verdade) e o
  `WM_PAINT` pinta só a região que mudou — um buffer do tamanho da tela
  seriam ~66 MB por quadro em dois monitores 4K. Um 2º `WM_LBUTTONDOWN` no
  meio do arrasto (chega quando o clique ativa a janela) é ignorado.
- **Gravação** (`capture/layout.rs`, `area_layout`): uma captura por
  monitor (TODOS, marcados ou não: a área pode ir pra qualquer um), cada
  uma recortada no pedaço da área que cai nela. O canvas é o tamanho da
  área no início (múltiplo de 16, como a janela — #66). Mover/redimensionar
  no meio só troca recorte/posição/escala (`update_area`); o vídeo NÃO muda
  de tamanho e a área nova é encaixada mantendo a proporção.
- **Moldura** (`area_frame.rs`): borda vermelha por fora da área e um botão
  no canto, ambos fora da captura (`WDA_EXCLUDEFROMCAPTURE`). O botão é
  atravessável pelo mouse (`WS_EX_TRANSPARENT`) e só deixa de ser enquanto
  **Ctrl+Shift** estão pressionados (lidos a cada 50 ms) — só Ctrl seria
  acionado por acidente com o Ctrl+clique comum de outros apps.
  Ctrl+Shift+clique liga a edição: um véu sobre a área recebe o mouse
  (meio move, bordas/cantos redimensionam, setas movem 1 px / Shift 10 px)
  e Ctrl+Shift+clique, Enter ou Esc terminam. Com jogo em tela cheia na
  frente tudo some, como o indicador (#49).
- Não continua o buffer em memória (a cena dele são as telas), igual à
  gravação de janela.
- **Área salva + atalho + atraso** (`bridge/area.rs`, aba **Área** das
  Configurações, `js/area.js`):
  - "Definir área" abre a MESMA seleção, mas só guarda o retângulo em
    `areaSaved` (`{x,y,w,h}`, px físicos). O seletor "Capturar... → Área"
    ganha o botão "Gravar a área salva".
  - `areaHotkey` (padrão `Ctrl+Shift+Pause/Break`) grava a área salva; de
    novo, para. Só é registrado COM uma área salva — sem ela a combinação
    volta a ser dos outros programas. Não pode repetir o atalho de gravar
    nem o do buffer. Ctrl+Pause chega como `VK_CANCEL`: registra o alias
    (ids 104/105), como os outros atalhos.
  - Na hibernação o atalho também vale: ela promove com `/start-area` (não
    `/start-record-area`: o `cmd.contains("/start-record")` pegaria também)
    e o warmup grava a área salva. O flag entra no `hidden_start` e na
    bandeja, como o `/start-replay` (#62).
  - A área salva é recortada pela área de trabalho virtual de AGORA: monitor
    desligado não vira pedaço preto; fora de todas as telas = aviso.
  - `areaDelaySec` (0 = desligado, padrão): a moldura aparece âmbar com os
    segundos no botão (GDI+ `DrawString`) e a gravação começa no zero
    (`TIMER_AREA_COUNTDOWN`). Vale pro atalho e pela seleção. Durante a
    contagem a edição fica travada, e o atalho da área, o de gravar e o
    botão de gravar CANCELAM. A tela principal mostra a contagem na linha
    de status (`area_countdown`).

## Como o Rust mudou o desenho

- **`TThread.Queue` + `CheckSynchronize`** → `mainthread::queue` + `WM_APP_WAKE`
  (closures `move`: some a pegadinha #6 de captura em laço).
- **`external ... delayed`** → `libloading` + macro `dll_api!`: tabela de
  funções por DLL, carregada na primeira chamada; símbolo ausente falha cedo.
- **libobs** → motor próprio (`capture/*`) atrás da interface
  `engine::CaptureEngine`. Some o que era limitação dela: o teto de 6 faixas
  de áudio, o buffer que não esvazia ao salvar (rotação de saída + emenda
  dos arquivos), as corridas ao destruir fontes WASAPI (#68), a RTWQ (#48).
- **Cadeia `if MsgType = ... else if`** → `match` no campo `type`.
- **Callbacks do `ui` para o bridge** saem de um `Mutex<Hooks>`: copie o
  ponteiro e chame FORA da trava. Chamar dentro travou a main thread na 1ª
  gravação pelo atalho (o handler esconde a janela, e o `hide_to_tray` lê
  outro hook): o `Mutex` do Rust não é reentrante.
- **Underflow de `Count - 1` (pegadinha #24)** não existe: `for i in 0..n` com
  `n = 0` é vazio.
- **Regras que seguem valendo**: motor e UI só na main thread (o trabalho
  pesado roda nas threads do motor e volta por evento); UTF-8 nas strings
  para FFI; objeto COM em `thread_local` sai antes do `CoUninitialize`.

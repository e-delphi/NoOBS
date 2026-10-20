# Motor de captura próprio (substituiu a libobs)

Decisão (outubro de 2026): trocar a `obs.dll` e os plugins por um motor em
Rust, usando a fonte do OBS 32.1.2 como referência. Motivo: o NoOBS usa uma
fatia pequena da libobs e herda os bugs e as convenções dela (thread única,
RTWQ, deduplicação de monitoramento, destruição adiada…). **Os filtros de áudio
(`obs-filters`) saem** e não voltam no motor novo.

Os testes da frente 4 (hoje `tests/mensagens.rs`) foram a régua: o motor
novo passou pelos mesmos cenários pelo `dispatch`. **Desde a fase 7 a libobs
não existe mais no app** (ver o fim deste documento).

## O que o motor faz (e só isso)

| Peça | Como | Referência no OBS |
|---|---|---|
| Relógio | QPC em ns, um só pro processo | `os_gettime_ns`, `video_sleep` (`obs-video.c`) |
| Captura de monitor e de janela | Windows Graphics Capture (WGC), pool *free-threaded* consultado pelo próprio laço de vídeo (sem evento, sem dispatcher) | `libobs-winrt/winrt-capture.cpp` |
| Captura de monitor (reserva) | DXGI Desktop Duplication | `libobs-d3d11/d3d11-duplicator.cpp` |
| Webcam | Media Foundation | `win-dshow` |
| Composição | D3D11: cada fonte desenhada no canvas com recorte/escala | `obs-scene.c`, `default.effect` |
| Cor | canvas BGRA → NV12 BT.709 limitado no shader | `format_conversion.effect` |
| Codificação de vídeo | **libavcodec** (`h264_amf`, `hevc_nvenc`, `av1_amf`… com quadros D3D11 sem sair da placa; `libx264`/`libsvtav1` com cópia pra memória) | `texture-amf.cpp`, `nvenc*.c` |
| Áudio | WASAPI: microfone, som do sistema (loopback), som de um app (process loopback) | `win-wasapi.cpp` |
| Mixagem | blocos de 1024 amostras a 48 kHz alinhados ao relógio; 1 faixa mista + uma isolada por dispositivo, SEM teto (a libobs parava em 6) | `audio-io.c`, `obs-audio.c` |
| Codificação de áudio | AAC do libavcodec | `obs-ffmpeg-audio-encoders.c` |
| Gravação | libavformat (Matroska) | `obs-ffmpeg-mux.c` |
| Buffer em memória | anel de pacotes cortado por keyframe | `replay_buffer` (`obs-ffmpeg-mux.c`) |
| Pausa | descarta pacotes e desloca o relógio da saída | `obs_output_pause` |

## Desenho (SOLID)

```
src/capture/
  mod.rs          fachada pública (CaptureEngine) — o que o bridge enxerga
  clock.rs        Clock: QPC → ns
  gpu/            Device (D3D11 + contexto), Texture, Compositor, ColorConverter
  video/          trait VideoSource + WgcSource (monitor/janela), DxgiSource, WebcamSource
  audio/          trait AudioSource + WasapiSource; AudioMixer
  encode/         trait VideoEncoder / AudioEncoder + implementações libavcodec
  sink/           trait PacketSink + MkvSink, ReplayRing
  pipeline.rs     Session: liga fontes → composição → encoders → sinks, em threads próprias
```

- **S**: cada tipo faz uma coisa (capturar, compor, converter cor, codificar,
  gravar). A `Session` só orquestra.
- **O**: fonte, encoder ou saída nova = uma implementação nova do trait; a
  `Session` não muda. (Ex.: o buffer e a gravação são dois `PacketSink`
  pendurados nos MESMOS encoders — é assim que a gravação continua o buffer.)
- **L**: toda `VideoSource` entrega uma textura no mesmo formato; todo
  `VideoEncoder` aceita o mesmo `GpuFrame`. Quem precisa de memória (x264)
  faz a cópia DENTRO da própria implementação.
- **I**: traits pequenos (`VideoSource` não sabe de áudio; `PacketSink` não
  sabe de encoder).
- **D**: o `bridge` depende da fachada `CaptureEngine`, não do pipeline nem das
  implementações. Durante a migração os dois motores implementam a mesma
  fachada e a escolha é uma chave (`captureEngine`: `obs` | `native`).

### Threads

Sem regra de "só na main": o motor tem as próprias threads.

- **vídeo**: a cada tique da grade (fps fixo, relógio QPC): pega a última
  textura de cada fonte, compõe, converte, codifica, entrega os pacotes.
  Tique perdido não é recuperado (o próximo quadro leva o pts certo da grade —
  como o `video_sleep` do OBS).
- **áudio**: uma thread por dispositivo WASAPI (eventos do próprio WASAPI) +
  a thread do mixer, que a cada 1024 amostras de relógio mistura e codifica.
- **saída**: os sinks recebem pacotes por canal; o MKV escreve numa thread
  própria (disco lento não segura a captura).

O contexto D3D11 é usado só pela thread de vídeo. O WGC usa pool
*free-threaded* e é CONSULTADO por ela (`TryGetNextFrame`), sem callback —
a corrida de destruição que existe no OBS (quadro chegando numa thread
enquanto a captura é desmontada em outra) não tem como acontecer.

## Fases

1. **Tela → MKV**: relógio, D3D11, WGC de monitor, composição lado a lado,
   encoder por libavcodec com quadro D3D11, MKV. Teste fora do app
   (`tests/captura.rs`) que grava e confere pelo `probe`.
2. Áudio: WASAPI (mic + loopback), mixer, AAC, faixas isoladas, sincronia
   (medir o atraso áudio/vídeo com um clique audível + flash).
3. Janela (WGC de janela + som do app), área (recorte/escala no compositor,
   mover no meio), webcam.
4. Buffer em memória, salvar trecho, gravação que continua o buffer, pausa.
5. Encoders: todos os codecs, calibração de qualidade (as tabelas do
   `encoder.rs` valem pros IDs do libobs; os do libavcodec já estão
   calibrados em `ffmpeg_export.rs`), teste de qualidade.
6. Fachada `CaptureEngine`, chave `captureEngine`, rodar a suíte `msg-*`
   com o motor novo; depois tirar a libobs do runtime e do instalador.

## Andamento

| Fase | Situação |
|---|---|
| 1. Tela → MKV | ✅ `src/capture/` (clock, gpu, video, encode, sink, pipeline) + `tests/captura.rs` |
| 2. Áudio | ✅ `audio` (WASAPI), `mixer`, AAC; faixas isoladas e sincronia medidas |
| 3. Som de app, webcam, área móvel | ✅ `AudioDevice::App(pid)`, `webcam` (Media Foundation), `layout::area_layout` + `Session::set_placement` |
| 4. Buffer, pausa | ✅ `output` (fila, corte, pausa — puro), `router` (gravador com comandos); sincronia A/V não remedida (ver abaixo) |
| 5. Codecs e calibração | ✅ `caps` (detecção sem libobs + aquecimento), escolha do codec do app, AV1 por software (SVT); NVENC sem placa pra testar |
| 6. Troca no app | ✅ `engine::CaptureEngine` (os dois motores atrás dela) + chave `captureEngine`; suíte `msg-*` 12/12 com o motor próprio |
| 7. Sem libobs | ✅ motor próprio é o único; runtime = exe + 9 DLLs do FFmpeg; UI e traduções dentro do exe; faixas de áudio sem teto |

```
cargo test --release --features app-tests --test captura            # tudo (~20 s)
cargo test --release --features app-tests --test captura -- monitor # filtro (vírgula = vários)
```

Medido nesta máquina (RX 9070 XT, 2 monitores 4K), fase 1:

| Cenário | Resultado |
|---|---|
| `cor` | BGRA → NV12 idêntico ao OBS: branco 235/128/128, preto 16/128/128, vermelho 63/102/240 |
| `encoder-*` | `h264_amf`, `hevc_amf`, `av1_amf` com o quadro NA GPU; `libx264` por cópia; NVENC pula sem NVIDIA |
| `monitor` | 3840x2160 @30 por `h264_amf`: **0,7 ms/quadro** na thread de vídeo, 0 atrasados (x264: 10 ms) |
| `dois-monitores` | lado a lado, escalado pra 4096 (teto do H.264, #7): 0,8 ms/quadro |
| `area` | recorte 640x360 @60: 0 atrasados |
| `janela` / `janela-fecha` | área cliente; a janela FECHAR no meio não derruba nada — o espaço fica preto e o arquivo fecha íntegro |
| `audio-faixas` | mistura + microfone + som do sistema = 3 faixas AAC 48 kHz estéreo, com nome, da duração do vídeo |
| `audio-loopback` | o loopback enxerga o som de outro processo |
| `audio-app` | loopback de UM processo: o helper que bipa sai com pico 0,80 (antes do volume do sistema); outro helper, mudo, sai 0,0000 |
| `webcam` | 1ª câmera (Media Foundation, convertida pra RGB32 pelo Windows) encaixada em 640x480, 0 atrasados |
| `area-move` | área sobre uma janela PRETA, movida aos 1,2 s pra uma BRANCA: vídeo 320x240 o tempo todo, claro a partir de 1,23 s |
| `gravar-depois` | sessão sobe sem gravar; duas gravações no meio: 2,00 s e 1,00 s, cada uma começando em 0 |
| `pausa` | 1,5 s + 2 s pausado + 1,5 s = 3,07 s de arquivo |
| `buffer-trechos` | dois trechos seguidos: 3,03 + 2,06 = 5,08 s em ~5 s de relógio (sem buraco nem repetição) |
| `buffer-limite` | limite de 2 s depois de 4 s: 2,03 s salvos |
| `buffer-continua` | gravar a partir do buffer: UM arquivo de 5,00 s (3 s guardados + 2 s ao vivo); o buffer segue guardando depois |
| `caps` | detecção sem libobs: fabricante pelo DXGI + 1 quadro codificado de verdade em cada encoder; aqui: AMF H.264/HEVC/AV1, x264, SVT-AV1 |
| `codec-*` | cada escolha do app (`auto`, `h264-hw`, `hevc-hw`, `av1-hw`, `h264-sw`, `av1-sw`) grava no codec certo, 1280x720 @30, 0 quadros atrasados (SVT-AV1 incluído) |
| `qualidade-*` | nível 0 → 5 → 10 sobre uma janela de RUÍDO: AMF 385 → 2477 → 5025 KB; x264 347 → 2657 → 4745 KB |
| `latencia-wgc` | só a captura, sem nada nosso: quanto o Windows leva pra entregar uma mudança na tela (<30 ms com a tela acesa; ~250 ms com ela apagada — aí a sincronia é PULADA) |
| `audio-sincronia` | janela pisca e bipa juntas: no arquivo o som cai **9 a 26 ms ANTES** do quadro claro (a imagem chega 1–2 quadros depois pelo compositor do Windows; o áudio é estável em 1,841 s) |

### Armadilhas da fase 1 (já resolvidas)

- **`AVCodecContext.hw_frames_ctx` fica em 552**, logo antes do
  `hw_device_ctx` (560, já medido). Uma conta pela vizinhança do
  `max_pixels` dava 536 e estava errada (o `max_pixels` mora em 792). O
  código só escreve depois de conferir a âncora `extra_hw_frames` (AVOption)
  em 572 e o campo vazio — escrever às cegas teria corrompido o contexto em
  silêncio.
- **Pool de quadros D3D11 do FFmpeg exige bind flag**: NV12 em array com
  `BindFlags = 0` dá `E_INVALIDARG` no `av_hwframe_ctx_init` ("Could not
  create the texture"). `D3D11_BIND_RENDER_TARGET` resolve.
- Os demais offsets do `hwcontext` (avutil-59, sem o `internal` do 6.x) são
  conferidos em tempo de execução contra valores conhecidos (`type` do
  device = D3D11VA, `device_ref` do frames apontando pro device) antes de
  qualquer escrita.

### Áudio (fase 2)

- Cada fonte WASAPI roda numa thread própria, consulta a cada 5 ms (sem
  evento e **sem RTWQ** — foi a RTWQ do `win-wasapi` que trouxe o áudio mudo
  da #48 e a corrupção de memória que a frente 4 achou) e guarda as amostras
  num anel marcado com o instante QPC que o próprio WASAPI dá a cada pacote
  (`pu64QPCPosition`, em 100 ns da escala do QPC: ×100 = o `clock::now_ns`).
- O formato é pedido já em 48 kHz float estéreo (`AUTOCONVERTPCM`): o Windows
  reamostra, não há resampler nosso.
- O anel se REALINHA quando o instante do pacote e a contagem de amostras
  divergem mais de 20 ms: buraco vira silêncio (loopback sem nada tocando não
  entrega pacote), sobra é cortada (relógio do dispositivo adiantado).
- O mixer anda 150 ms atrás do relógio, em blocos de 1024 (o `frame_size` do
  AAC); cada fonte é lida UMA vez por bloco e somada em todas as faixas da
  máscara (bit 0 = mistura).
- **O loopback vem DEPOIS do volume do sistema**: com o volume em ~2,5% um
  bipe de 0,8 chegou como 0,02 (é o Windows; o OBS grava igual). O teste de
  sincronia acha o início do som relativo ao pico, não por limiar fixo.
- `ns_to_frames` ARREDONDA: `frames_to_ns` trunca, e sem arredondar 100
  amostras → 2.083.333 ns → 99 amostras na volta (um teste de unidade pegou).

### Fase 3

- **Som de um app**: `ActivateAudioInterfaceAsync` no dispositivo virtual
  `VAD\Process_Loopback` com `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE`
  (Windows 10 2004+; antes disso a ativação falha e a fonte fica de fora).
  O dispositivo virtual não tem formato de mistura nem conversão: aceita o
  48 kHz float pedido e SÓ funciona por evento (`EVENTCALLBACK` +
  `SetEventHandle`). O callback de conclusão é um objeto COM nosso
  (`#[implement]`), ágil por padrão.
- **O modo inclui a árvore de FILHOS**: o PID do próprio processo de teste
  "ouvia" os helpers que ele lançou. A referência de silêncio do teste é um
  segundo helper mudo.
- **Loopback de processo vem ANTES do volume do sistema** (0,80 de 0,80); o
  loopback do dispositivo vem DEPOIS (0,02 com o volume baixo).
- **Webcam** por Media Foundation (o OBS usa DirectShow): o `IMFSourceReader`
  com `ENABLE_VIDEO_PROCESSING` entrega RGB32 seja qual for o formato da câmera;
  o `Lock2D` dá a 1ª linha e o passo (negativo = de baixo pra cima). Uma
  thread lê no ritmo da câmera; a fonte só sobe o quadro quando ele muda.
- **Área móvel**: uma fonte de monitor por monitor (como no motor antigo); o
  `area_layout` dá recorte e destino de cada uma (área cruzando monitores =
  um pedaço por monitor; redimensionada = encaixe com tarja, sem distorcer);
  `Session::set_placement` troca recorte/destino e vale no quadro seguinte.
  Destino de largura 0 = fonte não desenhada.

### Fase 4

- **Buffer = fila de pacotes já codificados** (`output::ReplayRing`), aparada
  por tempo e memória sempre num keyframe. Salvar um trecho força um keyframe
  AGORA: tudo antes dele vai pro arquivo (numa thread à parte), tudo depois
  fica na fila. Sem buraco e sem a sobreposição de até 1 s da #62. O áudio de
  antes do corte que chega atrasado (~150 ms, o mixer anda atrás do relógio)
  ainda entra no trecho: o corte só fecha quando cada faixa de áudio passou dele.
- **Gravar continuando o buffer** = a gravação recebe a fila inteira como
  começo e segue com os pacotes ao vivo: um arquivo só, sem a emenda
  (`SpliceContinuation`) que a libobs obrigava.
- **Pausa** (`output::PauseGate`): guarda CADA intervalo pausado
  `[início, keyframe da retomada)` e decide pelo INSTANTE do pacote, não pela
  ordem de chegada. Com estado único, o áudio de antes da pausa que chegava
  depois da retomada levava o desconto novo, o tempo andava pra trás e o
  muxer recusava ("non monotonically increasing dts").
- **O AMF do FFmpeg 7.1 não força keyframe** — o `amfenc.c` só lê o tipo do
  pacote de SAÍDA, e não existe `forced_idr` (a opção sobra no dicionário).
  O `AvVideoEncoder` cumpre o contrato `key = true` esvaziando e REABRINDO o
  codificador (começa sempre por IDR), com o mesmo pool de quadros da GPU;
  confere que o SPS/PPS reaberto é idêntico ao do cabeçalho (foi, em todas).
  x264/NVENC usam `forced-idr`.
- **A gravação segura o que chega antes do 1º keyframe**: o encoder de vídeo
  retém os primeiros quadros por centenas de ms (lookahead do x264, latência do
  AMF) e o áudio do mesmo instante chega antes deles. Descartar dava arquivo
  com o áudio começando ~280 ms depois do vídeo (achado lendo os primeiros
  pacotes de cada faixa).
- **Tela apagada atrasa a captura em ~250 ms** (compositor do Windows; medido
  sem nada nosso no caminho). Os testes mantêm a tela acesa
  (`SetThreadExecutionState`) e pulam a sincronia quando a latência passa de
  100 ms. ⚠️ Por isso a sincronia A/V NÃO foi remedida depois da fase 4: rodar
  `-- audio-sincronia` de novo com a tela ligada e os dois monitores.

### Fase 5

- **Detecção de encoders sem libobs** (`capture::caps`): fabricante pelo
  `VendorId` do adaptador do nosso dispositivo e, pra cada candidato, abrir E
  codificar um quadro preto. Só existir no build não basta (o NVENC existe em
  toda máquina) e só abrir também não: o AMF só se inicializa de verdade no 1º
  quadro — sem isso a 1ª gravação do processo perdia ~0,5 s de quadros. A
  detecção, rodando no arranque, é também o AQUECIMENTO.
- **O AMF de HEVC/AV1 recusa quadro pequeno** (`Init() failed with error 5`
  em 256x144) e grava normal em tamanho de tela: o teste de abertura é em
  1280x720, senão a máquina "não teria" HEVC/AV1.
- **Escolha e qualidade reusam a exportação**: preferência do app → nome do
  libavcodec por `resolve_export_encoder` (mesma ordem do "auto" da gravação:
  H.264 hw → x264 → AV1 hw → HEVC hw; AV1 por software nunca no automático) e
  nível 0..10 → CRF de referência (`encoder::quality_level_to_crf`) → o
  parâmetro calibrado de cada encoder (`apply_quality_options`, #51b). As
  tabelas `Q_*` do `encoder.rs` são dos IDs do libobs e saem junto com ele.
- **Limite de canvas** (#7) por nome do libavcodec: `h264_*` (hardware) 4096,
  o resto 8192 (`caps::clamp_canvas`, proporcional e em pares).
- **SVT-AV1 só aceita YUV420P**: a cópia pra memória separa o UV intercalado
  do NV12 em U e V. 1280x720 @30 sem quadro atrasado.
- O log do vídeo diz o PIOR quadro de cada etapa (fontes, composição,
  encoder): um atraso intermitente do 1º quadro foi visto 2 vezes e não
  voltou em 12 rodadas depois do aquecimento — se voltar, o log diz onde.

### Fase 6 — o motor dentro do app

- **`engine::CaptureEngine`** é a interface que o Bridge enxerga (DIP). Dois
  implementam: `engine::Engine` (libobs, repassando pros próprios métodos) e
  `native_engine::NativeEngine` (o próprio). A instância global virou
  `Box<dyn CaptureEngine>`; o `with_engine` entrega `&mut dyn CaptureEngine`
  e os eventos (fim de gravação, trecho salvo) pela mesma fila de antes.
  Só tem o que o Bridge usa (ISP): `teardown` e `replay_save_elapsed_ms`
  ficaram de fora porque ninguém chamava.
- **Chave `captureEngine`** no `config.json`: `native` = motor próprio;
  qualquer outra coisa (ou ausente) = libobs. Lida no `ensure_created` (vale
  no próximo início; sem controle na tela por enquanto).
- O `NativeEngine` repete as regras de montagem do `build_capture_graph`
  (monitores habilitados lado a lado, área por monitor, janela em múltiplo
  de 16, webcams no fim, TODAS as fontes de áudio — desmarcadas mudas —,
  faixas por `compute_audio_track_assignments`, nomes `NoOBS Mic/Out - …`).
  Parar e salvar trecho continuam assíncronos: uma thread faz o trabalho e o
  evento chega na main.
- **Diferenças que o Bridge precisou saber** (métodos da interface):
  `continuation_is_single_file` — gravar continuando o buffer já sai num
  arquivo só: sem trecho-prefixo, sem timeout esperando por ele, sem
  descontar o tempo do buffer da duração; `encoder_caps` /
  `predict_encoder_id` — a detecção é do motor; `uses_libobs` — filtros de
  áudio e o locale do plugin só existem com a libobs (no motor próprio a aba
  mostra "indisponível"; chamar a libobs sem ela ter subido derrubaria o app).
- **Semântica alinhada com a libobs**: continuar o buffer ENCERRA o buffer pro
  Bridge (ele religa quando a gravação para). Mantê-lo ativo dava "Ja esta
  gravando" no religar.
- O teste de qualidade usava "garanta os filtros" como atalho pra "garanta o
  motor de pé" — trocado pelo que ele queria.
- `describe_encoder_id` marcava `libsvtav1` como hardware: prefixo `lib` =
  software (convenção do libavcodec).

Com `OBSTEST_MOTOR=native`, a suíte de mensagens (então `tests/obs.rs`)
passou 12/12; pularam os 2 cenários de filtros (saíram de propósito) e o
`msg-som-sistema` (o volume do sistema desta máquina deixa o tom abaixo do
limiar — a libobs pulava igual). Com a libobs a mesma suíte TRAVOU uma vez
em `obs_reset_video` depois de gravar janela (o travamento intermitente
antigo, no caminho dela); com o motor próprio o mesmo par de cenários passa.

### Fase 7 — a libobs saiu

- **Código**: saíram `ffi/obs.rs`, `rtwq.rs`, `audio_filters.rs`,
  `bridge/filters.rs`, o `engine::Engine` inteiro (o `engine.rs` ficou só com
  a interface, os pedidos e o motor vivo da sessão) e a parte libobs do
  `encoder.rs` (seleção, `plan_constant_quality`, tabelas `Q_*` dos IDs da
  libobs). A interface perdeu o que só existia por causa dela:
  `uses_libobs`, `as_obs`, `continuation_is_single_file`, o "commit" adiado
  do corte do trecho e o trecho-prefixo — e com eles a emenda por conteúdo
  (`ffmpeg_ops::splice_continuation`, as amostras `buffer.mkv` +
  `continuacao.mkv`) e o `TIMER_REPLAY_SAVE_COMMIT`. A chave `captureEngine`
  deixou de existir (ignorada se estiver no `config.json`).
- **Faixas de áudio sem teto**: `compute_audio_track_assignments` dá uma
  faixa a cada dispositivo marcado (padrões primeiro), sem agrupar; o mixer
  troca a máscara `u32` por uma lista de faixas; a legenda da tela principal
  pinta as faixas por uma paleta em ciclo (`trackColor` em `displays.js`).
- **Runtime**: `NoOBS.exe` + `avcodec-61`, `avformat-61`, `avutil-59`,
  `swscale-8` e as dependências delas (`swresample-5`, `libx264-164`,
  `zlib`, `srt`, `librist`) — conferidas lendo as importações das DLLs. Sem
  `obs.dll`, `libobs-d3d11`, `w32-pthreads`, plugins, `data\`, Qt
  (`platforms\`, `styles\`…), `avfilter`/`avdevice`, `libcurl`,
  `datachannel`. A UI (`rust\ui\`) e as traduções (`rust\lang\`) vão dentro
  do exe (`build.rs` gera a lista; `assets.rs` serve): as traduções da
  memória, a UI extraída uma vez em `%LOCALAPPDATA%\NoOBS\ui\<hash>\` — o
  WebView2 só mapeia um origin `https://` real a partir de uma PASTA (#16).
- **Instalador**: o app vai pra `$INSTDIR\bin\` (só o exe e as DLLs) e a
  instalação apaga o `bin\64bit\`, o `data\` e o `obs-plugins\` antigos.
- **Testes**: a feature virou `app-tests`; `tests/obs.rs` virou
  `tests/mensagens.rs`, só com as mensagens pelo dispatch (os cenários que
  chamavam a libobs direto saíram — o `tests/captura.rs` cobre o motor
  direto), num processo só, achando as DLLs por `SetDllDirectoryW` (antes ele
  se copiava pra perto da `obs.dll`). O `msg-gravar` confere que o arquivo
  tem uma faixa por dispositivo marcado.

```
cargo test --release --features app-tests --test mensagens
cargo test --release --features app-tests --test mensagens -- msg-buffer,msg-area
```

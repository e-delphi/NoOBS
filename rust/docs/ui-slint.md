# UI em Slint (sai o WebView2)

Plano pra trocar a interface HTML/CSS/JS servida no WebView2 por uma
interface nativa em [Slint](https://slint.dev), na branch **`rust-slint`**
(a `rust` segue como a linha que gera instalador até a troca ser mesclada).

Licença: o NoOBS é GPLv3 e o Slint pode ser usado sob GPLv3. Se um dia o
código for fechado, a licença royalty-free cobre um app desktop (exige o
crédito `AboutSlint` na tela "Sobre").

## Decisões

1. **Conversão completa, sem convivência.** O WebView2 sai logo na fundação:
   `webview2-com`, `rust/ui/`, `ui::ensure_webview`/`destroy_webview`, a
   checagem do runtime no `startup_check` e no instalador. Não existe
   `--ui=web` nem chave no config. Enquanto a branch não termina, telas ainda
   não portadas simplesmente não existem nela.
2. **Chamadas diretas, sem JSON** entre interface e core (detalhe abaixo).
3. **Visual idêntico à versão web**: mesmas cores, medidas, raios, sombras,
   fontes, ícones, textos e animações. A web é a referência e a conferência é
   por captura de tela lado a lado (abaixo).
4. **Sem hibernação.** Ela existia pra descarregar o WebView2 (~100–200 MB e
   vários processos). Com a UI nativa, "fechar pra bandeja" só esconde a
   janela. Sai tudo: chave `hibernate`, `set_hibernate`,
   `TIMER_HIBERNATE_IDLE`, `enter_light`/`is_light`, a dica "hibernando" da
   bandeja, o arranque já hibernando e a opção nas Configurações. Os
   observadores (microfone, programas do buffer) passam a rodar sempre no
   processo normal, que é como já rodam com a janela aberta.

## Por que dá: o player já foi provado

A parte arriscada era o vídeo, que hoje vem de graça do `<video>` do
Chromium. A prova de conceito em `rust/poc-slint` (crate solto, fora do app)
toca as gravações reais:

| Gravação | Resultado |
|---|---|
| AV1 1080p60 | 60 q/s, nenhum descartado |
| AV1 4K30 / 5K30 | 30 q/s, nenhum descartado |
| seek exato aleatório em 4K | ~120 ms |
| janela / 1º quadro | ~230 ms / ~300–400 ms |

O caminho que funcionou: decode na placa (D3D11VA, o mesmo da exportação,
#51l) → **D3D11 Video Processor** converte NV12→RGBA e reduz pro tamanho da
tela na própria placa → só essa imagem volta pra memória → `slint::Image`.
Converter na CPU (cópia do 4K inteiro + swscale) custava ~20 ms por quadro
(teto ~48 fps); na placa, 6–8 ms (teto ~130 fps). Cor: entrada BT.709 de
faixa limitada; o swscale sem `setColorspaceDetails` usaria BT.601 e
desbota.

## Tamanho do trabalho

A UI atual (`rust/ui`) tem ~19 mil linhas: `index.html` 1850, CSS ~5,5 mil,
JS ~12 mil. Os maiores: `export.js` 2372, `player.js` 2212, `settings.js`
1406. O Rust fala com ela por **114 tipos de mensagem** (`bridge::dispatch`)
e ~70 pontos de `post_json`. Todo esse protocolo é substituído.

## Interface ↔ core: chamadas diretas

Hoje: página → `postMessage(json)` → `bridge::dispatch` → `match type` →
handler em `src/bridge/*.rs`; e handler → `post_json(json)` → página →
`Bridge.handlers[type]`.

Depois:

- **Interface → core.** Cada `callback` da `.slint` é ligado no Rust direto
  à função do handler, com argumentos tipados (`ui.on_record_toggle(||
  recording::handle_record_toggle())`). O `dispatch` e o parse de JSON somem;
  os handlers de `src/bridge/*.rs` ficam, perdendo só o desempacotar do JSON.
- **Core → interface.** Cada `post_json` vira uma chamada a um trait
  **`View`** com um método tipado por evento (`recording_state(&RecState)`,
  `audio_meters(&[Meter])`, `recording_added(&RecCard)`…). Os dados viram
  structs Rust (os `json!` de hoje dizem quais campos existem).
  - Na janela, o `View` é implementado pela UI Slint: atualiza propriedades e
    `VecModel`s (atualização por linha com `set_row_data`, não o modelo
    inteiro — medidores e prévias chegam 10×/s).
  - Nos testes, um `View` falso grava as chamadas. É o que mantém a suíte
    `tests/mensagens.rs`: ela passa a chamar os handlers direto e conferir o
    que chegou no `View` falso, em vez de mandar JSON e ler JSON.
- Core e Slint rodam na main thread (regra do projeto). Worker devolve pela
  `mainthread::queue` de sempre e de lá chama o `View`.
- Agrupar em structs é decisão por evento: onde a página recebia um objeto
  grande (`init`, `settings`), o `View` recebe a struct inteira; onde recebia
  um valor solto, recebe o valor.

## Visual idêntico: como garantir

- **Tokens primeiro.** As variáveis CSS do `base.css` (cores do claro e do
  escuro, raios, espaçamentos, sombras) viram um global `Theme` com os
  MESMOS valores. Nenhuma cor escrita solta numa `.slint`.
- **Fonte.** A pilha do CSS (`-apple-system, "Inter", "Segoe UI", …`) cai no
  **Segoe UI** no Windows (Inter não vem instalada); mono = `Cascadia Code`
  / `Consolas`. Mesmos tamanhos e pesos.
- **Ícones.** Os SVGs inline do `index.html` viram arquivos `.svg` (o Slint
  desenha SVG), com a cor trocada por `colorize`.
- **Animações.** As 78 `transition` e 11 `@keyframes` viram `animate` /
  `animation`, com a mesma duração e curva.
- **Conferência por captura.** Cada tela é comparada com a versão web no
  mesmo tamanho de janela, nos dois temas: o NoOBS instalado (web) e a
  branch, capturas lado a lado. Tela só conta como pronta depois disso.
- **Diferença conhecida: `backdrop-filter: blur`** (6 usos: controles do
  player, overlays das Configurações e do cartão de gravar). O Slint não
  desfoca o que está atrás. Saída prevista: fundo do mesmo tom com a mesma
  opacidade final, que é o que o olho percebe nesses pontos (o blur ali é
  sobre vídeo/fundo escuro). Se não ficar igual, decidir caso a caso.

## Pegadinhas já previstas

- **Laço de mensagens.** Hoje é `GetMessage`/`DispatchMessage` puro, com a
  janela principal recebendo `WM_APP_WAKE` (`mainthread`), `WM_TIMER`
  (timers do bridge), `WM_HOTKEY` e a bandeja. O Slint (winit) roda o
  próprio laço, mas ele também despacha as mensagens de toda janela da
  thread. Então: uma **janela Win32 oculta** fica com wake/timers/atalhos/
  bandeja e a janela do Slint só desenha. `mainthread::queue` não muda.
  Os HWND que hoje são a janela principal (dono do `RegisterHotKey`, alvo do
  `SetTimer`, selo da taskbar `SetOverlayIcon`, `DwmSetWindowAttribute` do
  modo escuro) precisam ser revistos um a um: o selo e o tema da barra de
  título são da janela VISÍVEL (a do Slint, HWND via
  `slint::winit_030` / `raw-window-handle`).
- **i18n.** O `@tr` do Slint quer gettext. Pra manter os `lang/*.json` (fonte
  única): um global `T` com `pure callback tr(int, string) -> string`, onde
  o `int` é uma revisão que sobe na troca de idioma — sem ele o Slint não
  reavalia os textos. Interpolação `{{var}}` fica no Rust.
- **Sem equivalente pronto no Slint**, pra fazer à mão: dica (tooltip) com
  atraso, toast, arrastar card pra pasta / pra lixeira, régua virtualizada
  da linha do tempo, forma de onda (desenhar em `Image` no Rust, não em
  `Path` com milhares de pontos), moldura de recorte com alças, campo de
  nome editável no próprio card.
- **Player sem o `<video>`** precisa do que o Chromium dava: áudio
  (WASAPI, relógio mestre no áudio), faixas isoladas em sincronia, volume
  por faixa, velocidade com tom mantido (o `TTimeStretch` da exportação já
  existe), avanço por saltos (#61, a grade de keyframes já vem do backend),
  zoom/região. O servidor HTTP local do `player.rs` deixa de servir o
  player: fica só se algo mais usar (conferir; senão sai junto).
- **Notificação do Windows.** A página usava a Notification API; vira toast
  nativo do Windows ou o balão da bandeja (o que a web mostrava).

## Fases

Cada fase é um ou mais commits na `rust-slint`, compilando e com os testes
passando.

### Estado

- **Fase 0 — feita.** Sem WebView2 (`webview2-com`, `rust/ui` web, servidor
  de arquivos da página) e sem hibernação. Janela Slint (`ui/app.slint`,
  `src/app_view.rs`) + janela Win32 oculta (`src/ui.rs`). Trait `View`
  (`src/view.rs`) com tema, idioma, aviso, erro, abrir Configurações e
  janela escondida. Globais `Theme` (tokens do `base.css`) e `T` (textos).
  Toast, dica e diálogo de confirmação prontos (`ui/widgets/`), ainda sem
  conferência visual lado a lado — nenhuma tela os dispara até a fase 1.
  Colunas da casca vazias.
- **Fase 1 — feita.** Tela principal em Slint (`ui/screens/`, presenter
  `src/app_main.rs`): cartão de gravar (botão com anéis, brilho, pulso,
  arco de finalizando, contagem, onda vermelha), pausa, linha do buffer,
  microfones/saídas com medidor e legenda das faixas, bloco de telas com
  prévia ao vivo (RGB direto, sem JPEG nem data URL), seletor "Capturar..."
  (abas Janela/Área, busca, som do app, áreas salvas, modo "várias janelas
  casaram o atalho"). Sons de início/fim gerados em memória
  (`src/sounds.rs`). Conferido por captura contra a web: cartão, fontes e
  telas. **Não conferidos lado a lado**: o seletor (o WebView2 ignora clique
  por mensagem, então não há captura dele aberto na web — conferido contra o
  `record.css`), a contagem e a onda (pedem gravação de verdade).

- **Fase 2 — feita.** Biblioteca (`ui/screens/library*.slint`, presenter
  `src/app_library.rs`): a coluna da direita rola inteira (telas + gravações)
  com o cabeçalho e o título de cada grupo grudando; pastas, caminho, grupos
  por período, busca (inclusive na transcrição), seleção com a ordem da
  união, menu de contexto nos três alvos, renomear no próprio card,
  recortar/colar (e Ctrl+X/Ctrl+V/Backspace), arrastar pra pasta, pro
  caminho e pra lixeira, excluir, unir. A biblioteca chega tipada
  (`View::library` e vizinhos), sem JSON. Conferido por captura contra a web
  (cabeçalho, grupos e cards alinhados ao pixel).

- **Fase 3 — feita.** Player próprio (`src/playback/`: vídeo decodificado e
  convertido na placa, áudio de todas as faixas misturado por tempo com o
  volume de cada uma, WSOLA acima de 1×, relógio pelo consumo do WASAPI,
  saltos por keyframe de 8× pra cima), presenter `src/app_player.rs`, telas
  `ui/screens/player*.slint`: barra com forma de onda, volume vertical,
  velocidade, zoom por roda e arrasto, tela cheia, selo "PAUSADO", painel de
  informações (seções, "Visualização" por região, volume por faixa) e de
  transcrição (busca com destaque, turno ativo seguindo o vídeo, renomear
  falante), legenda em blocos de duas linhas e o menu do CC. Os dados chegam
  tipados (`View::video_info`, `waveform`, `transcript`). Conferido por
  captura contra a web com as duas janelas no tamanho de maximizada (3840×2088
  a 150%): cabeçalho, controles e os dois painéis.
- **Fase 4 — feita.** Configurações (`ui/screens/settings*.slint`, controles
  de formulário em `ui/widgets/form.slint`, presenter `src/app_settings.rs`):
  as 11 abas, seletor com lista própria (o popup do `<select>`), sliders com
  marcas, prévia do indicador, atalhos (modo, modificadores, tecla, atraso,
  área, título), dispositivos e o perfil da gravação automática, pastas
  ocultas, motor local e fila de transcrição (arrastar pra reordenar),
  atualizações, "Sobre" e o "Restaurar padrões". O teste de qualidade
  compara QUADROS parados (os PNG de cada nível) com divisor e zoom; o
  comparador da web também tocava os MP4 dos níveis — **ainda não portado**.
  Na tela principal entraram o selo da transcrição e o aviso verde de versão
  nova. O core avisa por métodos tipados do `View` (`transcribe_state`,
  `local_asr_state`, `hotkeys_state`, `video_test`, `update_result`…); o
  JSON antigo só existe no gravador dos testes. Conferido aba por aba contra
  a web, as duas no tamanho de maximizada. Sem a hibernação (saiu na fase 0).
- **Fase 5 — feita.** Exportação (`ui/screens/export*.slint`, presenter
  `src/app_export.rs`): telas (com "só áudio"), prévia pelo motor do player
  ABERTO PARADO (`Playback::open_paused`; tocando, ele manda no cursor),
  composição das telas escolhidas, recorte com alças/sombra/terços/medida,
  zoom e deslocamento da prévia (Ctrl+roda, botão do meio, Shift+arrasto),
  legenda (CC) com os blocos do player, linha do tempo com cortes, trecho
  tirado listrado, forma de onda de alta resolução, régua e rolagem, faixas
  de áudio (o volume por faixa do motor toca as marcadas), resolução,
  codificador, redimensionamento, velocidade, fps, qualidade, legenda
  gravada, nome/formato, a exportação e a prévia gerada (abre no player por
  cima). O pedido vai tipado (`bridge::export::ExportRequest`) e o progresso
  volta pelo `View` (`export_progress`/`export_done`/`export_preview_*`).
  "Exportar" e o menu da biblioteca ligados; a barra de progresso mora no
  card da origem, com o × de cancelar. Conferido contra a web com as duas
  janelas maximizadas (formulário a 1–2 px) e exportando de verdade.

### Como comparar sem tocar na área de trabalho

Scripts em PowerShell, todos só sobre a janela do app: captura por
`PrintWindow(PW_RENDERFULLCONTENT)` (funciona coberta e fora da tela),
clique por `PostMessage(WM_LBUTTONDOWN/UP)` em coordenadas da captura
(menos a borda e a barra de título; também botão direito, duplo clique, roda e
arrasto com WM_MOUSEMOVE + MK_LBUTTON) e, pra não pular na frente do usuário,
`ShowWindow(SW_SHOWNOACTIVATE)` + `SetWindowPos` pra x = −4000. A janela do
WebView2 não aceita o clique por mensagem: a web se dirige pelo CDP
(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` e
`Runtime.evaluate`; reabrir sem a variável no fim). Teclado: o winit ignora
WM_CHAR solto — vai WM_KEYDOWN por tecla (VK_A… sai minúsculo).

**Compare as duas maximizadas**: `SetWindowPos` fora da tela com o tamanho da
área de trabalho (3840×2088 aqui) — a responsividade tem que ser a mesma.

**Hover por mensagem não se sustenta**: o winit liga o `TrackMouseEvent`, e
como o cursor de verdade está fora da janela o Windows manda `WM_MOUSELEAVE`
logo depois — o hover cai. Pra clicar num item de menu que só existe com
hover, mande MOVE + DOWN + UP sem pausa entre eles (chegam antes do LEAVE).
E o DOWN sem MOVE antes usa a ÚLTIMA posição do mouse, não a do lParam.

### O que a fase 1 ensinou

- **Medidas do CSS no Slint.** `font-weight: 500` sai Semibold no Chromium
  com Segoe UI (não há peso 500): no Slint é 600. `line-height` vira
  `line-height-factor` = valor do CSS / 1,33 (a altura de linha natural do
  Segoe UI). Elemento sem `y` fora de layout é centralizado na vertical, ao
  contrário do fluxo do HTML.
- **Camada por cima com tamanho EXPLÍCITO.** Um modal sem `x/y/width/height`
  manda o tamanho preferido dele pra cima e a janela vira restrita a ele:
  ao abrir o seletor a janela ENCOLHIA. Vale pro modal, pro `FocusScope`
  dele e pra área de rolagem de dentro.
- **Texto que quebra linha dentro de linha horizontal é medido como uma
  linha só.** O botão de baixo subia por cima do texto. Saída: largura fixa
  no texto (o `max-width` da web, que ele sempre preenche).
- **Sombra colorida translúcida debaixo de um fundo preto translúcido vira
  pontos brancos** no renderizador Skia (medido por eliminação: some sem o
  fundo do modal ou sem o `drop-shadow` do brilho do botão). Com modal
  aberto o brilho do botão de gravar sai — debaixo de 55% de preto ele mal
  aparece na web.
- **Hover não é reavaliado quando algo cobre o alvo**; só no próximo
  movimento do mouse. A dica do botão "Capturar..." ficava presa por cima
  do modal (e a do "Fechar" depois de fechar). `Hints.top-layer`: só dispara
  a dica da camada aberta, e abrir/fechar um modal esconde a que estiver na
  tela.
- **Rótulo atrás da prévia.** Na web a `<img>` posicionada cobre o nome do
  monitor; no Slint o rótulo só aparece sem prévia.

### O que a fase 0 ensinou

- **O HWND do winit só existe dentro do laço.** `show()` antes do
  `run_event_loop` não cria a janela; posição, tamanho, maximizar, ícone e
  barra escura vão em `ui::finish_show`, que tenta de 16 em 16 ms até o HWND
  aparecer. Antes disso o Slint também trocaria o tamanho pelo preferido do
  layout.
- **Perguntas de qualquer thread vão pelo HWND, não pelo Slint.** O estado
  do Slint é da main; a thread de prévias perguntava "janela visível?" e
  ouvia sempre "não". `ui::is_window_visible`/`is_window_shown`/
  `main_window` leem o HWND guardado num atômico.
- **`Cargo.lock`: `gpu-allocator` preso no `windows` 0.62.** O renderer
  Skia puxa o `wgpu` 30, que precisa do `gpu-allocator` com `windows` 0.62;
  o cargo o resolvia pra 0.61 (a do app, dentro da faixa `>=0.53, <=0.62`
  dele) e o `wgpu-hal` não compilava. A entrada foi editada à mão no lock;
  um `cargo update` que a desfaça traz o erro de volta (tipos
  `ID3D12Device` "diferentes").
- **Layout que depende da largura da janela vira laço.** A janela tira o
  mínimo dela dos filhos; colunas cuja largura depende de `root.width`
  ficam posicionadas à mão, fora de layout.
- **Medir janela pelo PowerShell mente em DPI alto**: o processo dele não é
  DPI-aware e recebe coordenadas virtualizadas (1133×800 a 150% = 1700×1200).
- **Não apague `%LOCALAPPDATA%\TNoOBS` enquanto a versão web existir.** A
  1ª versão da fase 0 apagava (dados do WebView2, "resto da versão web") e
  a versão web instalada na mesma máquina parou de abrir. E não volta
  sozinha: com a pasta criada DO ZERO com a DACL protegida que o
  `secure_user_data_folder` da versão web aplica (`D:P(A;OICI;FA;;;OW)
  (A;OICI;FA;;;SY)`), o navegador do WebView2 trava antes de escrever
  qualquer arquivo (`0x80080005`, `0x800700AA`, ou o controller nunca
  volta). Medido: a mesma versão com a pasta padrão do WebView2 sobe na
  hora. Antes funcionava só porque a pasta já existia, criada por versões
  antigas. **Isso é um bug da versão publicada (`rust`)**: instalação nova
  não tem a pasta. Remendo nesta máquina: `%LOCALAPPDATA%\TNoOBS` virou um
  ARQUIVO marcador, e a versão web cai na pasta padrão
  (`bin\NoOBS.exe.WebView2`). Apagar o marcador quando a Slint substituir a
  web; o instalador da fase 6 limpa os dois.
- **Código morto durante a migração.** Handlers só alcançados pelo
  `bridge::dispatch` ficam sem quem chame no app (231 avisos). `lib.rs` tem
  um `allow(dead_code)` SÓ no build do app (os testes continuam avisando) —
  sai na fase 6.

### O que a fase 2 ensinou

- **O `Flickable` arrasta o conteúdo com o mouse** (gesto de toque): arrastar
  um card rolava a lista. `mouse-drag-pan-enabled: false` no `ScrollArea`
  desliga só o arrasto do mouse; roda e toque continuam.
- **Sem `x`, o Slint centraliza também na horizontal** (não só o `y`): a
  camada fixa do cabeçalho, 8 px mais estreita que o painel, ficou 4 px à
  direita.
- **Uma área de toque por card.** Hover não atravessa: com a área do card por
  cima, as dos selos nunca recebiam hover (e por baixo, o card perdia o hover
  sobre eles). Os selos, a caixa de seleção e o número da ordem testam a
  posição do ponteiro do card.
- **`position: sticky` é uma camada por cima da rolagem**: cabeçalho e
  títulos são desenhados fora do `Flickable`, na posição
  `min(max(natural − rolagem, topo), fim do grupo − altura)`.
- **Só existem os cards à vista** (com 400 px de folga): a posição de cada um
  sai das linhas antes do grupo (`rows-before`, que o Rust recalcula quando o
  número de colunas muda). A miniatura é decodificada numa worker (`image`,
  só JPEG) quando o card aparece, e ficam no máximo 120 na memória (~400 KB
  cada na placa); a que sai é tirada do modelo também.
- **Miniatura é arquivo, não URL**: `player::ensure_recording_meta` /
  `get_cached_meta` devolvem o caminho do `<hash>.jpg` (o servidor HTTP sai
  com o player).

### O que a fase 3 ensinou

- **Renderizador: Skia por Direct3D 12, não OpenGL.** Com o
  `renderer-skia-opengl` a janela parou de desenhar de um dia pro outro (e o
  commit da fase 2, que funcionava, também): a thread principal ficava presa
  no `wglSwapBuffers` dentro do driver OpenGL da AMD (`atio6axx.dll`, pilha
  pelo `cdb -pv`). A janela respondia (`IsHungAppWindow` = falso) e não havia
  erro nenhum. O `renderer-skia` (wgpu → D3D12, com OpenGL e software de
  reserva) desenha na mesma máquina, no mesmo estado do driver.
- **O cache tem que continuar legível pela versão web.** O `videoInfo` do
  `<hash>.json` é a mensagem pronta que a web manda direto pra tela; sem o
  `"type": "video_info"` ela ficava em "Analisando arquivo..." pra sempre.
  O Slint grava com o campo.
- **Hover passa pro TouchArea de cima.** Um menu que abre com
  `botão.has-hover || menu.has-hover` FECHA quando o mouse chega num item: o
  item fica com o hover e a área do menu o perde. Some-se o hover dos itens
  (`changed has-hover` contando) — no menu do CC e na trilha do volume.
- **`StyledText` sempre quebra linha**, mas sem largura explícita fica com
  a largura do texto numa linha e centralizado. E não tem `line-height`: a
  altura é escalada (1,45 / 1,33) pra o turno ocupar o espaço da web.
- **Filho mais baixo num `HorizontalLayout` vai pro topo**, o `y` é
  ignorado: o "PAUSADO" (24 px) ficava acima do centro do × (32 px). Ele
  ganha a altura da linha e se posiciona dentro dela.
- **O × do Chromium com `line-height: 1` fica ~1,5–2 px acima** do centro
  que o Slint calcula; deslocado à mão.

### O que a fase 4 ensinou

- **`<input>`, `<select>` e `<button>` não herdam a fonte da página** no
  Chromium: saem em Arial 13 px. Os controles do formulário usam Arial
  (`Form.control-font`); rótulos, abas e dicas, Segoe UI. E `font-weight: 500`
  em Arial é normal (não há semibold): só no Segoe UI o 500 vira 600.
- **Campo escondido ainda conta margem na web.** O
  `.settings-field + .settings-field { margin-top: 10px }` vale mesmo com o
  irmão anterior em `display: none` (de outra aba): toda aba menos a Geral
  começa 10 px mais baixo.
- **`<hr>` não tem o `max-width` dos campos**: os divisores atravessam o
  corpo inteiro (`St.rule-w`).
- **`?:` trunca a escala.** `zoom == 0 ? ajustado : zoom` deu 0 com o ajustado
  em 0,3 — mesmo com os dois ramos float. Trocado por `max()`/`min()`. Vale
  conferir na tela todo `?:` que devolve fração.
- **Altura que depende da própria largura sai zero no layout**: o layout mede
  antes de dar largura. O comparador tira a altura da largura da COLUNA
  (`St.col-w`), que vem de fora.
- **`Image::load_from_path` não decodifica arquivo** nesta configuração do
  Slint (sem os formatos padrão): os PNG do teste vão pelo crate `image`. As
  `@image-url` da `.slint` (os logos de GPU) funcionam.
- **`PopupWindow` não tem `closed`**; e o `StyledText` desenha o `` `code` ``
  minúsculo e com fundo — a dica do nome do arquivo usa texto normal.
- **O cache e as mensagens do core com JSON só nos testes**: os empurrões das
  Configurações viraram métodos tipados do `View`; o gravador reproduz o JSON
  antigo (`settings_json`, pastas sumidas sem `count`…) e os testes seguiram
  iguais, exceto o do teste de qualidade, que agora confere o PNG no disco.

### O que a fase 5 ensinou

- **O estado da tela que vem da `.slint` não pode ser zerado na abertura.**
  O tamanho do quadro e da linha do tempo chegam por `changed` quando a
  janela muda — antes de qualquer abertura. O reset do `open_for` os apagava
  e a prévia ficava 2×2 (preta) e a linha do tempo sem imagem.
- **Laço de layout com a barra de rolagem**: descontar os 8 px só "quando
  rola" faz a largura da coluna depender da altura do conteúdo, que depende
  da largura. O formulário é sempre mais alto que a janela: a barra fica
  sempre descontada (como a web, na prática).
- **O `preferred-height` do layout fica aquém** com dicas que quebram linha:
  os botões do fim ficavam cortados. A altura rolável sai da posição de um
  marcador vazio no fim do layout (`end-mark`), com o layout em
  `height: self.preferred-height` (sem isso o marcador entra em laço com a
  área rolável).
- **`flex-wrap` de pílulas**: o Slint não mede texto do lado do Rust e não
  quebra linha de filhos. As linhas saem quebradas no Rust medindo com GDI
  (`win::text_width`, Segoe UI 12 px) e cada linha é um `HorizontalLayout`.
- **O TouchArea do card fica por cima de tudo**: o × do rodapé de exportação
  é testado por posição no clique do card, como a caixa de seleção.
- **Na web, só as pílulas montadas por `_mkCheck` ganham `.checked`**: as
  caixas soltas ("Manter o áudio original", "Juntar as faixas") ficam com a
  borda comum mesmo marcadas (`EChk.plain`). E a caixa nativa desativada é
  cinza (`CheckBox.disabled`), não a verde apagada.
- **Diferença aceita**: na web o rodapé de progresso alonga o card; aqui ele
  fica sobre a base da miniatura (a grade tem altura de linha fixa). E com a
  prévia aproximada, a folga de 14 px do palco fica preta (na web o vídeo
  aproximado aparecia nela) — o motor só desenha dentro do quadro.

| # | Fase | Substitui | Risco |
|---|---|---|---|
| 0 | **Fundação**: tira WebView2 e hibernação; janela Slint + janela Win32 oculta; trait `View` + `View` falso dos testes; globais `Theme` e `T`; casca da janela (cabeçalho, navegação), toast, diálogo de confirmação, dica | `index.html` (casca), `bridge.js`, `i18n.js`, `widgets.js`, `base/layout/widgets.css` | médio (laço de mensagens) |
| 1 | **Tela principal**: cartão de gravar/pausar, buffer, telas/mics/saídas/webcams com prévia e medidor, dispositivos ocultos, gravar janela, área, contagem | `record`, `displays`, `replay`, `windowpicker`, `area`, `hotkey` | baixo |
| 2 | **Biblioteca**: cards com prévia, grupos por data, pastas, navegação, busca (inclusive na transcrição), seleção, menu de contexto, renomear, arrastar | `recordings`, `folders`, `main` (menu) | médio (arrastar) |
| 3 | **Player**: o da prova de conceito + áudio e faixas, velocidade, saltos, forma de onda, zoom/região, painel de informações, transcrição ao lado e legenda | `player` | **alto** (áudio) |
| 4 | **Configurações**: todas as abas, editor genérico dos filtros de áudio, teste de qualidade, transcrição (fila, diagnóstico, motor local), atalhos | `settings`, `audiofilters`, `videotest`, `transcribe`, `hotkeys` | médio (volume) |
| 5 | **Exportação**: prévia com composição, recorte com alças e zoom, linha do tempo com cortes e forma de onda, faixas, CC, progresso no card da origem | `export` | alto (volume e interação) |
| 6 | **Fechamento**: instalador sem WebView2, `startup_check`, README e CLAUDE.md (protocolo, hibernação, pegadinhas que mudaram), sai o `bridge::dispatch`/`post_json` e o `allow(dead_code)` do `lib.rs`, conferência visual final de todas as telas | — | baixo |

A ordem põe primeiro o que destrava o resto (0), depois o que se usa todo
dia (1, 2). O player vem antes das Configurações porque é o risco técnico
que sobrou; se o áudio empacar, é melhor saber na fase 3 do que no fim.

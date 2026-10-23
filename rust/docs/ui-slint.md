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

| # | Fase | Substitui | Risco |
|---|---|---|---|
| 0 | **Fundação**: tira WebView2 e hibernação; janela Slint + janela Win32 oculta; trait `View` + `View` falso dos testes; globais `Theme` e `T`; casca da janela (cabeçalho, navegação), toast, diálogo de confirmação, dica | `index.html` (casca), `bridge.js`, `i18n.js`, `widgets.js`, `base/layout/widgets.css` | médio (laço de mensagens) |
| 1 | **Tela principal**: cartão de gravar/pausar, buffer, telas/mics/saídas/webcams com prévia e medidor, dispositivos ocultos, gravar janela, área, contagem | `record`, `displays`, `replay`, `windowpicker`, `area`, `hotkey` | baixo |
| 2 | **Biblioteca**: cards com prévia, grupos por data, pastas, navegação, busca (inclusive na transcrição), seleção, menu de contexto, renomear, arrastar, progresso de exportação no card | `recordings`, `folders`, `main` (menu) | médio (arrastar) |
| 3 | **Player**: o da prova de conceito + áudio e faixas, velocidade, saltos, forma de onda, zoom/região, painel de informações, transcrição ao lado e legenda | `player` | **alto** (áudio) |
| 4 | **Configurações**: todas as abas, editor genérico dos filtros de áudio, teste de qualidade, transcrição (fila, diagnóstico, motor local), atalhos | `settings`, `audiofilters`, `videotest`, `transcribe`, `hotkeys` | médio (volume) |
| 5 | **Exportação**: prévia com composição, recorte com alças e zoom, linha do tempo com cortes e forma de onda, faixas, CC | `export` | alto (volume e interação) |
| 6 | **Fechamento**: instalador sem WebView2, `startup_check`, README e CLAUDE.md (protocolo, hibernação, pegadinhas que mudaram), conferência visual final de todas as telas | — | baixo |

A ordem põe primeiro o que destrava o resto (0), depois o que se usa todo
dia (1, 2). O player vem antes das Configurações porque é o risco técnico
que sobrou; se o áudio empacar, é melhor saber na fase 3 do que no fim.

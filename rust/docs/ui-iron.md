# UI no iron (sai o Slint)

Troca da interface Slint por uma interface no [iron](../../../iron), na branch
**`rust-iron`** (a `rust-slint` fica como estava até a troca terminar). A
referência visual é a versão Slint (que por sua vez copiou a web ao pixel):
`git show rust-slint:rust/ui/...` e os presenters `rust-slint:rust/src/app_*.rs`.

## Decisões

1. **Projeto iron com forms.** O NoOBS é um projeto do iron: `NoOBS.ironproj`,
   `forms/` e `themes/` na raiz do repositório e o código em `rust/` (a pasta da
   linguagem). As telas são forms JSON, abertos e editados na IDE do iron; o
   Rust fica nos handlers (`src/forms/`, gerados pela IDE) e nos presenters.
2. **A fronteira não muda.** O core só fala com a interface pelo trait `View`
   (`src/view.rs`) e a interface chama o core direto. Troca só a camada de
   apresentação: `ui/*.slint`, `app_view.rs`, `app_*.rs` e a parte da janela
   em `ui.rs`. A janela Win32 oculta (wake, timers, atalhos, bandeja, 2ª
   instância, instalador) fica como estava.
3. **Tema próprio no NoOBS.** `themes/noobs-dark.json` e `noobs-light.json`
   partem do Iron Dark / Iron Light (`"base"`) com os MESMOS tokens do
   `theme.slint` (que eram os do `base.css` da web). Tokens novos do NoOBS
   (`surface_3`, `border_2`, `text_3`, `warning_*`...) ficam no próprio tema.
4. **O que faltar no iron entra no iron**, genérico (não um controle "do
   NoOBS"), com teste e documentação lá.

## Como o `View` chega no `Ui`

O `Ui` do iron pertence ao laço de eventos. Cada método do `View` enfileira
uma função com o `Ui` (`app_view::with_ui`), que o iron roda logo em seguida
(pelo `Poster`), na ordem de chegada. As workers continuam chegando pelo
`view::call` → `mainthread::queue`, como antes.

A janela principal é o form `frmMain`. O que precisa do Win32 (barra de título
escura, ícone, selo de gravando, trazer à frente) usa o HWND dele
(`iron::platform::win::hwnd`). Mostrar e esconder vão pelo iron
(`WindowCommand::Show`/`Hide`); ela nasce escondida (`set_start_hidden`) e o
1º `restore_from_tray` põe no lugar e mostra (ou deixa na bandeja no
`/autostart`).

## Fases

As mesmas do porte para o Slint. Cada fase compila e passa nos testes.

- **Fase 0 — feita.** Sem Slint (`slint`, `slint-build`, `ui/`, os presenters).
  Projeto iron (`NoOBS.ironproj`, `forms/main.json` com a casca: coluna de
  fontes de 400 px e a área de gravações), tema do NoOBS, `View` pelo iron
  (tema, aviso, erro; o resto chega nas fases seguintes), janela pelo iron
  com bandeja, 2ª instância e encerramento pelo instalador conferidos. No
  iron entraram: `WindowCommand::Hide`/`Show`/`SetFullscreen`,
  `set_start_hidden`, `win::hwnd`, `Ui::set_timeout` e o `"base"` dos temas
  em JSON.
- **Fase 1.** Tela principal: cartão de gravar (botão com anéis, pulso,
  contagem, onda), pausa, buffer, microfones/saídas com medidor, telas com
  prévia ao vivo, seletor "Capturar...". Widgets: aviso empilhado (toast),
  dica, diálogo de confirmação.
- **Fase 2.** Biblioteca: pastas, caminho, grupos por período, busca, seleção,
  menu de contexto, renomear no card, arrastar, excluir, unir.
- **Fase 3.** Player: quadro do motor próprio (`playback`, agora em
  `RgbaBuffer`) num controle do iron, forma de onda, volume, velocidade, zoom,
  tela cheia, painéis de informações e transcrição, legenda.
- **Fase 4.** Configurações: as 11 abas, atalhos, dispositivos, transcrição,
  atualizações, "Sobre".
- **Fase 5.** Exportação: prévia, recorte, linha do tempo com cortes, faixas,
  codificador, progresso.
- **Fase 6.** Limpeza: avisos de código morto, README, instalador, memória
  medida.

O Slint saiu inteiro na fase 0: `slint`, `slint-build`, `ui/`, os presenters,
a prova de conceito `poc-slint/`, o `vendor/wgpu-hal` e o `docs/ui-slint.md`.

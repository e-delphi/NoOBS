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
- **Fase 1 — feita.** Tela principal (`forms/main.json`, presenters
  `app_main.rs` e `app_picker.rs`): cartão de gravar (`RecordButton`:
  pulso, pausa, contagem, finalizando), cronômetro, buffer em memória,
  microfones/saídas com faixa colorida e medidor estéreo, legenda das faixas,
  bloco "Telas" com a prévia ao vivo (`ScreenLayout`), avisos empilhados
  (`notify`) e o seletor "Capturar..." (abas Janela/Área, busca, miniaturas,
  som do app, áreas salvas, timelapse). Conferido por captura contra a versão
  Slint. No iron entraram `RecordButton`, `ScreenLayout`, `notify`, o
  `LevelMeter` estéreo com degradê e o `letter_spacing`/`text_case` do
  `Label`. **Falta**: a onda vermelha ao começar a gravar, o aviso de versão
  nova e o selo da transcrição (entram com as Configurações), o Esc fechando
  o seletor e a conferência gravando de verdade (cores, cronômetro, pausa).
- **Fase 2 — feita.** Biblioteca (`lyLibrary` no `forms/main.json`, presenter
  `app_library.rs`): cabeçalho com contagem e espaço (aviso de pouco espaço),
  busca (também na transcrição), pastas, caminho (`Breadcrumb`), grupos por
  período com "selecionar o grupo", cartões `MediaCard` com miniatura, duração
  e selos, seleção na ordem da união, menu de contexto, recortar/colar
  (Ctrl+X/Ctrl+V), renomear no cartão, excluir e unir com `confirm`.
  Conferido gravando de verdade (a gravação nova entra em "Hoje"). **Falta**:
  arrastar pra pasta/lixeira e o menu do fundo (pasta nova, colar, voltar).
- **Fase 3 — feita.** Player (`pnlPlayer` no `forms/main.json`, presenter
  `app_player.rs`): o quadro do motor próprio no `FrameView`, forma de onda
  com a parte tocada e busca, tempo e relógio, volume geral e por faixa,
  velocidade (com os saltos de keyframe), zoom pela roda e arrasto, tela
  cheia, controles que somem com o mouse parado (e o cursor junto), teclado
  (Esc, espaço, setas, M, F, 0, C), painel de informações com a
  "Visualização" por região e o de transcrição com busca e legenda.
  Conferido tocando uma gravação de verdade. **Falta**: renomear o falante na
  transcrição e o indicador de pausa no meio do palco.
- **Fase 4 — feita.** Configurações (`pnlSettings` no `forms/main.json`,
  presenter `app_settings.rs`): a barra lateral e o cabeçalho vêm do form; as
  linhas das 11 abas (Geral, Gravação, Arquivos, Vídeo, Áudio, Buffer,
  Atalhos, Dispositivos, Transcrição, Atualizações, Sobre) são montadas no
  código por um construtor de linha (título, dica quebrada medida com a fonte
  do estilo, controle à direita). Cada mudança vai direto pro core, sem
  Salvar; sliders e o campo de programas do buffer com envio adiado. Teste de
  qualidade de vídeo, fila e motor de transcrição, atalhos com validação de
  combinação, "Restaurar padrões" pelo `confirm` do iron. Na tela principal
  entraram o aviso de versão nova (`pnlUpdate`) e o selo da fila de
  transcrição (`lblTrChip`). Esc fecha. No iron: `Ui::measure_text_wrapped` e
  o `font_of` com a fonte do estilo.
- **Fase 5.** Exportação: prévia, recorte, linha do tempo com cortes, faixas,
  codificador, progresso.
- **Fase 6.** Limpeza: avisos de código morto, README, instalador, memória
  medida.

O Slint saiu inteiro na fase 0: `slint`, `slint-build`, `ui/`, os presenters,
a prova de conceito `poc-slint/`, o `vendor/wgpu-hal` e o `docs/ui-slint.md`.

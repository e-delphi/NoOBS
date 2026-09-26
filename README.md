# NoOBS

<p align="center">
  <img src="app-black.png" alt="NoOBS — tema escuro" width="49%">
  <img src="app-white.png" alt="NoOBS — tema claro" width="49%">
</p>

Gravador de tela com o OBS Studio embarcado. Toda a potência do OBS, sem
instalar nem configurar nada — só abrir e gravar.

---

## Por que o NoOBS

- **Zero configuração.** Sem cenas, sem fontes, sem perfis. Abriu, seus
  monitores e microfones já estão lá, prontos.

- **Some quando não está usando.** Depois de 1 minuto na bandeja, cai pra
  **~5 MB de RAM** — só o ícone e o atalho global. Volta sozinho na hora
  de gravar.

- **Todos os monitores num arquivo só.** Lado a lado, sem faixa preta de
  monitor que ficou de fora.

- **Cada microfone e alto-falante na própria faixa.** Faixa 1 é a mistura
  de tudo; as outras ficam isoladas por dispositivo, até 6. Abre no
  DaVinci ou no Premiere com os nomes certos, pronto pra separar voz de
  áudio do sistema.

- **Arquivos que não desperdiçam espaço.** Grava por qualidade, não por
  taxa fixa: tela parada quase não ocupa nada, e as cenas de movimento
  gastam o que precisarem.

- **Junta e exporta sem sair do app.** Unir não reencoda — é instantâneo
  e sem perda. A exportação recorta trechos,
  escolhe monitores e reduz resolução, com prévia do que está saindo. Para
  enquadrar, é só arrastar as bordas da prévia: o que ficar dentro da
  moldura é o que vai pro arquivo. Fechou a tela? Ela continua em segundo
  plano, com a barra de progresso no card da gravação e um botão de
  cancelar ali mesmo.

- **Organiza em pastas, sem abrir o Explorer.** Crie pastas na própria
  lista, arraste uma gravação pra dentro ou use recortar e colar. Excluir
  uma pasta avisa quantas gravações vão junto.

- **Grava chamadas sozinho.** Detecta quando o Teams, o Meet ou o
  WhatsApp abre o microfone, começa a gravar e para quando a chamada
  acaba. Funciona até com o app hibernando.

- **Guarda os últimos minutos sem gravar no disco.** Ligue o buffer em
  memória e jogue: quando acontecer algo que valha guardar, um atalho
  salva o trecho como gravação normal e o buffer recomeça — dá pra salvar
  vários pedaços seguidos, e a emenda entre eles se sobrepõe em vez de
  perder um pedaço. Ele liga sozinho quando você abre um jogo da sua
  lista (mesmo com o NoOBS hibernando), e um indicador discreto mostra
  quanto já está guardado — clicar nele salva o trecho. O tempo e a
  memória guardados são seus, com o máximo limitado ao que cabe na
  máquina. Começou a gravar com o buffer ligado? A gravação já sai com o
  que ele tinha guardado, emendada sem perder nem repetir um quadro.

- **Transcreve e legenda na sua placa de vídeo.** Instala com um clique
  na aba de Transcrição, sem Docker: o NoOBS baixa o motor e os modelos
  (~3,7 GB, uma vez só) e transcreve na GPU — AMD, NVIDIA ou Intel. Cada
  palavra sai marcada no instante exato em que foi dita, e números por
  extenso viram algarismos. Com faixas isoladas, cada fala leva o nome do
  dispositivo de onde veio. Prefere um servidor? A Transcritor API (Docker
  ou outro computador) continua como opção, e separa os falantes.

- **À prova de queda de energia.** Grava em MKV, recuperável quadro a
  quadro. Um travamento não leva a gravação junto.

Também tem tema claro/escuro acompanhando o Windows, interface em
português, inglês e espanhol, atalho global, ícone na bandeja, início com
o Windows, player embutido com zoom, velocidade e forma de onda, e
exclusão sempre pela lixeira. Enquanto você grava ou o buffer está
ligado, o NoOBS segura os trabalhos pesados em segundo plano (como a
transcrição) pra não disputar a máquina com o que está sendo capturado.

---

## Instalação

Baixe a versão mais recente em
[Releases](https://github.com/e-delphi/NoOBS/releases/latest).

O instalador oferece **iniciar com o Windows** (marcado por padrão) e
**atalho na área de trabalho** (desmarcado). Na primeira execução o app
abre nas Configurações pra você escolher a pasta de gravação e o atalho.

---

## Terceiros

Este software utiliza os seguintes componentes open-source:

- **OBS Studio** — GPL v2+ — https://github.com/obsproject/obs-studio
- **FFmpeg** — LGPL v2.1+ / GPL v2+ — https://ffmpeg.org
- **WebView2** — Microsoft Software License — UI HTML embutida via runtime do Edge

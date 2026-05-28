# NoOBS

<p align="center">
  <img src="app-black.png" alt="NoOBS — tema escuro" width="49%">
  <img src="app-white.png" alt="NoOBS — tema claro" width="49%">
</p>

Gravador de tela simples e direto, sem complicação. Uma interface
pensada pra ser fácil de usar que aproveita toda a potência do OBS
— você não precisa instalar nem configurar o OBS, tudo já vem pronto.
Só abrir e gravar.

> **Premium e leve.** Modo hibernação que cai pra ~5 MB de RAM
> enquanto não está gravando. Tema acompanha o Windows na primeira
> execução. UI focada só em gravar.

---

## Recursos

### Gravação
| Recurso | Descrição |
|---|---|
| Captura multi-monitor | Grava todos os seus monitores ao mesmo tempo em um único arquivo, lado a lado |
| Webcam | Detecta suas webcams automaticamente e permite adicionar na gravação |
| Áudio separado por dispositivo | Cada microfone e alto-falante em faixas independentes, facilitando edição depois |
| Gravação só de áudio | Se nenhum monitor ou webcam estiver selecionado, o áudio ainda é gravado |
| Detecção em tempo real | Reconhece quando você conecta ou desconecta microfones, alto-falantes, monitores e webcams — inclusive troca de dispositivo padrão no Painel de Som |
| Codec automático | Detecta sua placa de vídeo e usa o melhor encoder disponível (H.264 hardware → x264 → AV1 → HEVC) — priorizando compatibilidade. Default é H.264 hardware |
| Qualidade ajustável | Slider de 5 níveis pra balancear tamanho de arquivo e qualidade visual |
| Faixa "Mix" + isoladas | Faixa 1 é o mix de tudo; faixas 2–6 individuais por dispositivo (até 6 no total) |
| Dispositivo padrão preservado | O microfone e alto-falante padrão do Windows sempre ficam em faixas individuais quando possível |

### Interface
| Recurso | Descrição |
|---|---|
| Tema claro/escuro | Segue o tema do sistema na primeira execução; alternável depois nas Configurações |
| Botão de gravação cinematográfico | Botão circular com halo pulsante, timer com centésimos de segundo e onda radial vermelha ao iniciar |
| Preview ao vivo | Miniaturas dos monitores e webcams atualizando a 2 FPS, identifica facilmente o que será gravado |
| Legenda de faixas de áudio | Mostra qual dispositivo está em qual faixa do vídeo, com cores distintas |
| Indicador de dispositivo padrão | Bolinha verde nos dispositivos definidos como padrão no Windows |
| Espaço em disco visível | Painel de gravações mostra quanto disco está usado e quanto sobra — alerta laranja abaixo de 5 GB |
| Notificações detalhadas | Avisa o que mudou (dispositivo adicionado, removido, padrão trocado) com o nome do dispositivo |
| Tela "Sobre" via F1 | Informações do projeto e link do repositório com um atalho |

### Atalhos e automação
| Recurso | Descrição |
|---|---|
| Atalho global configurável | Padrão: tecla `Pause`. Configurável com até 4 teclas (Ctrl+Shift+Alt+G, etc.) |
| Som de início/fim | Opção pra tocar uma sequência curta de duas notas ao iniciar (ascendente) e parar (descendente) a gravação — discreto, confirmação auditiva |
| Indicador no LED Scroll Lock | Opção pra piscar o LED de Scroll Lock do teclado enquanto grava — útil quando o app está na bandeja |
| Bandeja do sistema | Ícone próximo ao relógio com menu para iniciar/parar gravação, abrir e fechar. Ícone troca pra uma bolinha vermelha enquanto grava |
| Iniciar com Windows | Abre minimizado na bandeja ao logar, gravação fica pronta no atalho global. Sincronizado com o Task Manager — respeita o estado de "Desabilitado" do Windows |
| Parar ao bloquear o computador | Opção que finaliza automaticamente a gravação quando o Windows bloqueia (Win+L, troca de usuário, bloqueio automático) |
| Modo hibernação | Após 1 min na bandeja (ou cold-start via autostart), o app libera todos os recursos e fica em ~5 MB de RAM, só com tray icon + hotkey. Volta automaticamente ao normal quando precisar |
| Minimizar ao gravar | Opção pra esconder a janela automaticamente quando a gravação começa |
| Fechamento inteligente | Botão [X] minimiza pra bandeja se estiver gravando ou se "iniciar com Windows" estiver ativo |

### Reprodução e gerenciamento
| Recurso | Descrição |
|---|---|
| Player embutido | Assista suas gravações direto no app, com zoom e controles de reprodução |
| Volume com boost até 200% | Sliders master e por-faixa vão de 0% a 200%; barra vermelha acima de 100% indica amplificação. Pra cada vídeo o volume volta automático a 100% |
| Atalhos rápidos de volume | Tooltip mostra o `%` ao vivo durante o drag; double-click no slider reseta pra 100% |
| Volume por faixa | Ajuste o volume de cada microfone e alto-falante separadamente ao assistir; faixas individuais podem ser mutadas |
| Navegação quadro a quadro | Setas `←` / `→` no player pausado avançam um quadro por vez (1/30s); tocando, pulam ±5 s |
| Feedback visual de pause | Pulso curto no centro ao pausar + badge persistente "PAUSADO" no header + seek bar fica cinza durante o pause |
| Informações do vídeo | Detalhes técnicos da gravação: resolução, duração, codec, bitrate, faixas de áudio |
| Lista com miniaturas | Cards com thumbnail, duração e tamanho de cada gravação. Click na miniatura abre o player; duplo-click no nome ou tamanho renomeia o arquivo |
| Forma de onda do áudio | Barras de intensidade renderizadas embaixo da seek bar — visão rápida de onde tem áudio alto no vídeo |
| Busca e gerenciamento | Filtrar, renomear, excluir em lote (botão de exclusão na header fica vermelho quando há seleção) |
| Compatível com editores | MKV padrão com metadata correta de nome de faixa — abre direto no DaVinci, Premiere, etc. |

---

## Instalação

Baixe a versão mais recente em [Releases](https://github.com/e-delphi/NoOBS/releases).

O instalador inclui opções (desmarcadas por padrão):

- **Iniciar com o Windows** — sobe o NoOBS automaticamente no logon. Combinado com a opção "Minimizar para bandeja", o app inicia em modo hibernação consumindo só ~5 MB.
- **Atalho na área de trabalho** — cria um atalho do NoOBS no Desktop.

Na **primeira execução** após instalação, o app abre direto na tela de Configurações pra você ajustar pasta de gravação, atalho e demais opções antes da primeira gravação.

---

## Terceiros

Este software utiliza os seguintes componentes open-source:

- **OBS Studio** — GPL v2+ — https://github.com/obsproject/obs-studio
- **FFmpeg** — LGPL v2.1+ / GPL v2+ — https://ffmpeg.org
- **WebView2** — Microsoft Software License — UI HTML embutida via runtime do Edge

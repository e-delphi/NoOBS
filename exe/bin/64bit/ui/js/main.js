function startRecordingAnimation(elapsedSec) {
  _recAnimStart = Date.now() - (Math.max(0, elapsedSec) * 1000);
  if (_recAnimInterval) clearInterval(_recAnimInterval);
  _renderTimer(Date.now() - _recAnimStart);
  _recAnimInterval = setInterval(() => {
    _renderTimer(Date.now() - _recAnimStart);
  }, 50);
}

function stopRecordingAnimation() {
  if (_recAnimInterval) {
    clearInterval(_recAnimInterval);
    _recAnimInterval = null;
  }
  _renderTimer(0);
}

// Estado anterior — usado pra detectar transicao (false→true = inicio,
// true→false = parou) e disparar o som de feedback. Inicia null pra que
// o primeiro recording_state recebido apos o boot (geralmente active=false)
// NAO dispare playStop.
let _lastRecordingActive = null;

function applyRecordingState(active, elapsedSec) {
  const btn = document.getElementById('recordBtn');
  const label = document.getElementById('recordLabel');
  const card = document.getElementById('recCard');
  const statusText = document.getElementById('recStatusText');

  // Feedback sonoro de inicio — so quando active vira true, primeira
  // vez (nao no boot onde _lastRecordingActive=null). O som de PARADA
  // nao roda aqui — toca preemptivamente em toggleRecord (click) ou
  // pelo handler recording_stopping (hotkey/tray), porque
  // Engine.StopRecording pode levar centenas de ms flushing buffers
  // e o user sente que "travou" se o ding so vier no fim.
  if (_lastRecordingActive !== null && _lastRecordingActive !== active &&
      active && Settings && Settings.currentPlaySoundOnRecord) {
    RecordingSounds.playStart();
  }
  _lastRecordingActive = active;

  if (active) {
    btn.classList.add('recording');
    card.classList.add('recording');
    document.body.classList.add('recording');
    label.textContent = T('record.stop');
    statusText.textContent = T('record.statusRecording');
    startRecordingAnimation(elapsedSec || 0);
  } else {
    btn.classList.remove('recording');
    card.classList.remove('recording');
    document.body.classList.remove('recording');
    label.textContent = T('record.start');
    statusText.textContent = T('record.statusReady');
    stopRecordingAnimation();
  }
  updateRecordButtonAvailability();
}

// Habilita/desabilita o botao de gravar com base na presenca de algum
// dispositivo (monitor, webcam, mic ou speaker) habilitado. Durante
// gravacao ativa o botao fica sempre habilitado pra permitir o stop.
function updateRecordButtonAvailability() {
  const btn = document.getElementById('recordBtn');
  if (!btn) return;
  if (btn.classList.contains('recording')) {
    btn.disabled = false;
    btn.removeAttribute('title');
    return;
  }
  const mons = (Displays.monitors || []).filter(m => m.enabled).length;
  const cams = (Displays.webcams  || []).filter(c => c.enabled).length;
  const mics = document.querySelectorAll('#micList .source-item.selected').length;
  const spks = document.querySelectorAll('#spkList .source-item.selected').length;
  const anyEnabled = (mons + cams + mics + spks) > 0;
  btn.disabled = !anyEnabled;
  if (!anyEnabled)
    btn.title = T('record.needDevice');
  else
    btn.removeAttribute('title');
}

// =====================================================================
// Busca
// =====================================================================
function onSearch() {
  const q = (document.getElementById('searchInput').value || '').toLowerCase().trim();
  const cards = document.querySelectorAll('#recGrid .rec-card');
  let visible = 0;
  cards.forEach(card => {
    const text = card.textContent.toLowerCase();
    const match = q === '' || text.includes(q);
    card.style.display = match ? '' : 'none';
    if (match) visible++;
  });
  // Esconde grupos cujos cards estao todos filtrados.
  document.querySelectorAll('#recGrid .rec-group').forEach(g => {
    const hasVisible = !!g.querySelector('.rec-card:not([style*="display: none"])');
    g.style.display = hasVisible ? '' : 'none';
  });
  document.getElementById('emptyState').style.display =
    (visible === 0 && q !== '') ? 'block' : 'none';
  document.getElementById('recGrid').style.display =
    (visible === 0 && q !== '') ? 'none' : '';
}

// =====================================================================
// Tema
// =====================================================================
function notifyTitlebarTheme(theme) {
  // Canal "dark"/"light" simples lido pelo OBSUI pra colorir a
  // barra de titulo do Windows (DwmSetWindowAttribute).
  try { window.chrome.webview.postMessage(theme); } catch (e) {}
}
// Aplica o tema vindo do backend (pull inicial em DoInit/PushTheme).
// Trocas vem do toggle dentro do modal de Configuracoes (Settings.setTheme).
function applyTheme(theme) {
  if (theme !== 'dark' && theme !== 'light') return;
  document.documentElement.setAttribute('data-theme', theme);
  notifyTitlebarTheme(theme);
}

// =====================================================================
// Context menu (right-click em rec-card)
// =====================================================================
function showCtxMenu(clientX, clientY, recordingId) {
  const menu = document.getElementById('ctxMenu');
  menu.dataset.target = recordingId;
  // Se ha selecao multipla e o card clicado faz parte dela, o menu
  // opera sobre TODOS os selecionados. Caso contrario, opera so no
  // card clicado (e mantemos a selecao intacta).
  const selectedAll = RecSelection.all();
  const useSelection = selectedAll.length > 1 &&
                       RecSelection.has(recordingId);
  menu.dataset.bulk = useSelection ? '1' : '0';
  const deleteItem = menu.querySelector('[data-action="delete"]');
  if (deleteItem) {
    deleteItem.textContent = useSelection
      ? T('recordings.deleteNShort', { count: selectedAll.length })
      : T('common.delete');
  }
  menu.style.display = 'block';
  // Posiciona — clamp para nao escapar da janela.
  const w = menu.offsetWidth, h = menu.offsetHeight;
  const maxX = window.innerWidth  - w - 4;
  const maxY = window.innerHeight - h - 4;
  menu.style.left = Math.min(clientX, maxX) + 'px';
  menu.style.top  = Math.min(clientY, maxY) + 'px';
}
function hideCtxMenu() {
  const menu = document.getElementById('ctxMenu');
  menu.style.display = 'none';
  menu.dataset.target = '';
}
function initCtxMenu() {
  const menu = document.getElementById('ctxMenu');
  // Acao: excluir
  menu.querySelector('[data-action="delete"]').addEventListener('click', () => {
    const id = menu.dataset.target;
    const bulk = menu.dataset.bulk === '1';
    hideCtxMenu();
    if (bulk) {
      // Mesma logica do botao da header (header → bulkDeleteSelected).
      bulkDeleteSelected();
      return;
    }
    if (!id) return;
    const card = document.querySelector(`#recGrid .rec-card[data-id="${CSS.escape(id)}"]`);
    const name = card ? (card.querySelector('.when')?.textContent || id) : id;
    Confirm.open({
      title: T('recordings.confirmDeleteSingleTitle'),
      message: T('recordings.confirmDeleteNamed', { name: name }),
      okLabel: T('common.delete'),
      onOk: () => {
        // Limpa selecao otimisticamente — evita race com o file watcher
        // que pode rebuildar a lista antes do recording_removed chegar.
        if (RecSelection.ids.delete(id)) {
          RecSelection._syncGroups();
          RecSelection._syncMode();
        }
        Bridge.send('delete_recording', { id });
      }
    });
  });
  // Fecha em click fora
  document.addEventListener('mousedown', (e) => {
    if (!e.target.closest('.ctx-menu')) hideCtxMenu();
  });
  // Fecha em Esc
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') hideCtxMenu();
  });
  // Fecha em scroll/resize pra nao ficar flutuando
  window.addEventListener('resize', hideCtxMenu);
  document.addEventListener('scroll', hideCtxMenu, true);
  // Re-fit do canvas composite quando a janela redimensiona.
  window.addEventListener('resize', () => Player._fitCompositeToStage());
}

// =====================================================================
// Boot
// =====================================================================
// Suprime o menu padrao do browser em toda a UI. Apenas cards de
// gravacao tem context menu proprio (Excluir).
document.addEventListener('contextmenu', (e) => {
  if (!e.target.closest('.rec-card')) e.preventDefault();
});

// Bloqueia zoom do browser — Ctrl+scroll, Ctrl + / -, Ctrl 0.
window.addEventListener('wheel', (e) => {
  if (e.ctrlKey) e.preventDefault();
}, { passive: false });
window.addEventListener('keydown', (e) => {
  if (!e.ctrlKey) return;
  const k = e.key;
  if (k === '+' || k === '-' || k === '=' || k === '0') e.preventDefault();
});

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('searchInput').addEventListener('input', onSearch);
  initCtxMenu();
  Confirm.init();
  Hint.init();
  Player.init();
  Displays.init();
  Bridge.init();
});

// Bloqueia drag-and-save de imagens em qualquer lugar do app
// (complementa o CSS user-drag: none). Captura no document pra
// pegar elementos criados dinamicamente (thumbs de gravacao,
// previews de monitor, logos, etc).
document.addEventListener('dragstart', (e) => {
  if (e.target && e.target.tagName === 'IMG') e.preventDefault();
});

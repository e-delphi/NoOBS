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
    btn.dataset.hint = T('record.needDevice');
  else
    btn.removeAttribute('title');
}

// =====================================================================
// Busca
// =====================================================================
function onSearch() {
  const q = (document.getElementById('searchInput').value || '').toLowerCase().trim();
  // Pastas entram no filtro junto com os cards: buscar sem elas deixaria
  // uma linha de pastas irrelevantes acima do unico resultado. O card de
  // "voltar" tambem some — durante a busca a navegacao nao e o assunto.
  const cards = document.querySelectorAll('#recGrid .rec-card, #recGrid .rec-folder');
  let visible = 0;
  cards.forEach(card => {
    const isUp = card.classList.contains('up');
    const text = card.textContent.toLowerCase();
    const match = (q === '') ? true : (!isUp && text.includes(q));
    card.style.display = match ? '' : 'none';
    if (match) visible++;
  });
  // Esconde grupos cujos cards estao todos filtrados.
  document.querySelectorAll('#recGrid .rec-group').forEach(g => {
    const hasVisible = !!g.querySelector(
      '.rec-card:not([style*="display: none"]), .rec-folder:not([style*="display: none"])');
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

// Modo "Sistema": segue mudancas de tema do SO ao vivo. O WebView dispara
// 'change' no prefers-color-scheme quando o Windows troca claro<->escuro.
// So reage se o usuario esta em 'system'; reenvia set_theme('system') pro
// backend re-resolver pela registry + atualizar o menu da bandeja.
try {
  const _mq = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)');
  if (_mq && _mq.addEventListener) {
    _mq.addEventListener('change', () => {
      if (typeof Settings !== 'undefined' && Settings.currentThemePref === 'system')
        Settings.setTheme('system');
    });
  }
} catch (e) {}

// =====================================================================
// Context menu (right-click na biblioteca)
// =====================================================================
// Três alvos, um só menu: card de gravação, card de pasta e o fundo da
// lista. Os itens são montados na hora — antes eram markup fixo no HTML,
// mas com pastas o conjunto de ações muda por alvo e ainda depende de
// haver ou não um recorte pendente.
function confirmDeleteOneRecording(id) {
  const card = document.querySelector(
    `#recGrid .rec-card[data-id="${cssEscape(id)}"]`);
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
}

function ctxMenuItems(ctx) {
  const items = [];
  const canPaste = RecFolders.canPaste();
  const pasteHint = canPaste ? '' : T('recordings.nothingCut');

  if (ctx.kind === 'recording') {
    // Com selecao multipla que inclui o card clicado, o menu opera sobre
    // TODOS os selecionados; senao so sobre ele.
    const ids = RecFolders.targetIds(ctx.id);
    const bulk = ids.length > 1;
    // Exportar opera sobre UMA gravacao (recorte, regioes e faixas sao
    // do arquivo). Em lote o menu fala das N selecionadas — deixar
    // "Exportar" ativo ali sugeriria exportar todas, quando exportaria
    // so a clicada.
    const expBlocked = bulk || Export.running;
    items.push({
      label: T('export.menuItem'),
      disabled: expBlocked,
      hint: expBlocked ? T('export.selectOne') : '',
      run: () => Export.openFor(ctx.id)
    });
    items.push({
      label: bulk ? T('recordings.cutN', { count: ids.length }) : T('recordings.cut'),
      run: () => RecFolders.cut(ids)
    });
    items.push({ sep: true });
    items.push({
      label: bulk ? T('recordings.deleteNShort', { count: ids.length })
                  : T('common.delete'),
      danger: true,
      run: () => { if (bulk) bulkDeleteSelected(); else confirmDeleteOneRecording(ctx.id); }
    });
    return items;
  }

  if (ctx.kind === 'folder') {
    items.push({ label: T('recordings.folderOpen'), run: () => RecFolders.open(ctx.id) });
    items.push({ label: T('common.rename'),         run: () => RecFolders.renameById(ctx.id) });
    items.push({ label: T('recordings.cut'),        run: () => RecFolders.cut([ctx.id]) });
    items.push({
      label: T('recordings.pasteInto'),
      disabled: !canPaste,
      hint: pasteHint,
      run: () => RecFolders.paste(ctx.id)
    });
    items.push({ sep: true });
    items.push({
      label: T('common.delete'), danger: true,
      run: () => RecFolders.remove(ctx.id)
    });
    return items;
  }

  items.push({ label: T('recordings.newFolder'), run: () => RecFolders.create() });
  items.push({
    label: T('recordings.paste'),
    disabled: !canPaste,
    hint: pasteHint,
    run: () => RecFolders.paste('')
  });
  if (!RecFolders.atRoot) {
    items.push({ sep: true });
    items.push({ label: T('recordings.folderUp'), run: () => RecFolders.goUp() });
  }
  return items;
}

function showCtxMenu(clientX, clientY, ctx) {
  // Compat: chamadas antigas passavam so o id da gravacao.
  if (typeof ctx === 'string') ctx = { kind: 'recording', id: ctx };
  ctx = ctx || { kind: 'empty', id: '' };
  const menu = document.getElementById('ctxMenu');
  menu.innerHTML = '';
  ctxMenuItems(ctx).forEach(spec => {
    if (spec.sep) {
      const s = document.createElement('div');
      s.className = 'ctx-sep';
      menu.appendChild(s);
      return;
    }
    const el = document.createElement('div');
    el.className = 'ctx-item' + (spec.danger ? ' danger' : '');
    el.textContent = spec.label;
    if (spec.disabled) el.dataset.disabled = 'true';
    if (spec.hint) el.dataset.hint = spec.hint;
    el.addEventListener('click', () => {
      // Desabilitado nao e so visual: sem esta guarda o clique ainda
      // rodaria a acao que o estado desabilitado existe pra evitar.
      if (el.dataset.disabled === 'true') return;
      hideCtxMenu();
      spec.run();
    });
    menu.appendChild(el);
  });
  menu.style.display = 'block';
  // Posiciona — clamp para nao escapar da janela.
  const w = menu.offsetWidth, h = menu.offsetHeight;
  menu.style.left = Math.max(4, Math.min(clientX, window.innerWidth  - w - 4)) + 'px';
  menu.style.top  = Math.max(4, Math.min(clientY, window.innerHeight - h - 4)) + 'px';
}

function hideCtxMenu() {
  const menu = document.getElementById('ctxMenu');
  menu.style.display = 'none';
}

function initCtxMenu() {
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
  // O teto de largura da prévia da exportação é derivado da altura da
  // janela (Export._syncStageAspect) — sem recalcular, redimensionar a
  // janela deixaria a prévia alta demais ou pequena à toa.
  window.addEventListener('resize', () => {
    try { Export._syncStageAspect(); } catch (e) {}
  });
}

// Atalhos da biblioteca. So valem com o foco na lista: dentro de um
// campo de texto Ctrl+X/Ctrl+V sao do texto, e com player/exportacao/
// configuracoes abertos a biblioteca esta atras de uma tela cheia.
function libraryShortcutsAllowed() {
  const a = document.activeElement;
  if (a) {
    const t = (a.tagName || '').toUpperCase();
    if (t === 'INPUT' || t === 'TEXTAREA' || a.isContentEditable) return false;
  }
  const blockers = ['playerOverlay', 'exportOverlay', 'settingsOverlay', 'confirmOverlay'];
  for (let i = 0; i < blockers.length; i++) {
    const el = document.getElementById(blockers[i]);
    if (el && el.classList.contains('visible')) return false;
  }
  return true;
}

// =====================================================================
// Boot
// =====================================================================
// Suprime o menu padrao do browser em toda a UI. Cards de gravacao e de
// pasta tem handler proprio (com stopPropagation), entao aqui so chega o
// que caiu no VAZIO da biblioteca — que ganha o menu de "Nova pasta /
// Colar", o unico jeito de criar pasta ou colar sem mirar num card.
document.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  const inLibrary = e.target.closest('.recordings') &&
                    !e.target.closest('.displays-block') &&
                    !e.target.closest('input, textarea, button');
  if (inLibrary) showCtxMenu(e.clientX, e.clientY, { kind: 'empty', id: '' });
});

// Recortar/colar e voltar uma pasta pelo teclado. Espelham o menu de
// contexto — quem descobriu a acao ali espera o atalho.
document.addEventListener('keydown', (e) => {
  if (!libraryShortcutsAllowed()) return;
  const k = e.key;
  if (e.ctrlKey && (k === 'x' || k === 'X')) {
    const ids = RecSelection.all();
    if (ids.length) { e.preventDefault(); RecFolders.cut(ids); }
  } else if (e.ctrlKey && (k === 'v' || k === 'V')) {
    if (RecFolders.canPaste()) { e.preventDefault(); RecFolders.paste(''); }
  } else if ((e.altKey && k === 'ArrowLeft') ||
             (!e.ctrlKey && !e.altKey && k === 'Backspace')) {
    if (!RecFolders.atRoot) { e.preventDefault(); RecFolders.goUp(); }
  }
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
  Export.init();
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

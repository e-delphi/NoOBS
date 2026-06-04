// =====================================================================
// Render: recordings
// =====================================================================
function formatDuration(sec) {
  if (!sec || sec <= 0) return '';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) return `${String(h).padStart(2,'0')}:${String(m).padStart(2,'0')}:${String(s).padStart(2,'0')}`;
  return `${String(m).padStart(2,'0')}:${String(s).padStart(2,'0')}`;
}

function formatWhen(isoOrText) {
  if (!isoOrText) return '';
  const d = new Date(isoOrText);
  if (isNaN(d.getTime())) return isoOrText;
  const today = new Date();
  const yest  = new Date(); yest.setDate(today.getDate() - 1);
  const sameDay = (a, b) =>
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate();
  const hhmm = `${String(d.getHours()).padStart(2,'0')}:${String(d.getMinutes()).padStart(2,'0')}`;
  if (sameDay(d, today)) return `${T('dateGroups.today')}, ${hhmm}`;
  if (sameDay(d, yest))  return `${T('dateGroups.yesterday')}, ${hhmm}`;
  const dd = String(d.getDate()).padStart(2,'0');
  const mm = String(d.getMonth() + 1).padStart(2,'0');
  const yy = d.getFullYear();
  return `${dd}/${mm}/${yy}, ${hhmm}`;
}

function formatFullDate(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  const pad = n => String(n).padStart(2, '0');
  return `${pad(d.getDate())}/${pad(d.getMonth()+1)}/${d.getFullYear()} ` +
         `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

// =====================================================================
// Selecao multipla de gravacoes
// =====================================================================
// SVG do check (viewBox 16, posicao do "v" geometricamente centralizada).
const CHECK_SVG =
  '<svg viewBox="0 0 16 16" aria-hidden="true">' +
  '<polyline points="3.5,8.5 6.8,11.8 12.5,5.5"/>' +
  '</svg>';

const RecSelection = {
  ids: new Set(),
  has(id) { return this.ids.has(id); },
  toggle(id) {
    if (this.ids.has(id)) this.ids.delete(id);
    else this.ids.add(id);
    this._syncCard(id);
    this._syncGroups();
    this._syncMode();
  },
  setMany(ids, selected) {
    ids.forEach(id => {
      if (selected) this.ids.add(id); else this.ids.delete(id);
      this._syncCard(id);
    });
    this._syncGroups();
    this._syncMode();
  },
  clear() {
    const old = Array.from(this.ids);
    this.ids.clear();
    old.forEach(id => this._syncCard(id));
    this._syncGroups();
    this._syncMode();
  },
  size() { return this.ids.size; },
  all() { return Array.from(this.ids); },
  _syncCard(id) {
    const card = document.querySelector(
      `#recGrid .rec-card[data-id="${cssEscape(id)}"]`);
    if (!card) return;
    if (this.ids.has(id)) card.dataset.selected = 'true';
    else delete card.dataset.selected;
  },
  _syncGroups() {
    document.querySelectorAll('#recGrid .rec-group').forEach(g => {
      const cards = g.querySelectorAll('.rec-card');
      let selectedCount = 0;
      cards.forEach(c => { if (c.dataset.selected === 'true') selectedCount++; });
      const check = g.querySelector('.rec-group-check');
      if (!check) return;
      if (selectedCount === 0) check.dataset.state = 'none';
      else if (selectedCount === cards.length) check.dataset.state = 'all';
      else check.dataset.state = 'some';
    });
  },
  _syncMode() {
    document.body.classList.toggle('rec-select-mode', this.ids.size > 0);
    // Habilita/desabilita o botao de "excluir selecionadas" na header
    // de Gravacoes — vermelho quando ha 1+ selecionado.
    const delBtn = document.getElementById('deleteSelectedBtn');
    if (delBtn) {
      const n = this.ids.size;
      delBtn.disabled = (n === 0);
      delBtn.title = n === 0
        ? T('recordings.selectToDelete')
        : (n === 1 ? T('recordings.deleteOne')
                   : T('recordings.deleteN', { count: n }));
    }
  }
};

// Bulk delete reutilizado pelo menu de contexto (item "Excluir" em
// modo selecao) e pelo botao da header de Gravacoes. Otimistic UI: tira
// os cards do DOM agora pra evitar "piscadas" enquanto o backend
// processa um delete por vez. Pushes 'recording_removed' que chegam
// depois sao no-op (card ja nao existe).
function bulkDeleteSelected() {
  const ids = RecSelection.all();
  if (ids.length === 0) return;
  Confirm.open({
    title: T('recordings.confirmDeleteTitle'),
    message: ids.length === 1
      ? T('recordings.confirmDeleteOne')
      : T('recordings.confirmDeleteN', { count: ids.length }),
    okLabel: T('common.delete'),
    onOk: () => {
      ids.forEach(rid => {
        const card = document.querySelector(
          `#recGrid .rec-card[data-id="${cssEscape(rid)}"]`);
        if (!card) return;
        const group = card.closest('.rec-group');
        card.remove();
        if (group && !group.querySelector('.rec-card')) group.remove();
      });
      RecSelection.clear();
      recalcRecMetaFromDom();
      ids.forEach(rid => Bridge.send('delete_recording', { id: rid }));
    }
  });
}

function buildRecCard(item) {
  const card = document.createElement('div');
  card.className = 'rec-card';
  card.dataset.id = item.id;
  // Restaura estado visual de selecao apos rebuild (renderRecordings
  // limpa o grid e refaz tudo — sem isso card selecionado perde o
  // highlight quando o file watcher dispara um refresh).
  if (RecSelection.has(item.id)) card.dataset.selected = 'true';
  if (item.size) card.dataset.size = item.size;
  const fullDate = formatFullDate(item.date);
  if (fullDate) card.title = fullDate;
  card.onclick = (e) => {
    // Click no checkbox: toggle de selecao, nao abre o video.
    if (e.target.closest('.rec-check')) {
      e.stopPropagation();
      RecSelection.toggle(card.dataset.id);
      return;
    }
    // Click no BODY inteiro (nome + tamanho) e reservado pra dblclick =
    // rename. So a thumb (imagem/duracao no topo) abre o player. Sem
    // isso, era facil pedir play sem querer mirando no texto.
    if (e.target.closest('.body')) return;
    // Em modo selecao, click no card tambem alterna a selecao
    // (evita ter que mirar na bolinha de 20px).
    if (RecSelection.size() > 0) {
      RecSelection.toggle(card.dataset.id);
      return;
    }
    Bridge.send('play_recording', { id: card.dataset.id });
  };
  card.oncontextmenu = (e) => {
    e.preventDefault();
    showCtxMenu(e.clientX, e.clientY, card.dataset.id);
  };

  const thumb = document.createElement('div');
  thumb.className = 'thumb';
  // Checkbox de selecao no canto superior direito.
  const check = document.createElement('div');
  check.className = 'rec-check';
  check.title = T('recordings.select');
  check.innerHTML = CHECK_SVG;
  thumb.appendChild(check);
  if (item.thumb) {
    const img = document.createElement('img');
    img.className = 'thumb-img';
    img.src = item.thumb;
    img.alt = '';
    thumb.appendChild(img);
  } else {
    // Placeholder via child node — textContent apaga o .rec-check ja adicionado.
    const ph = document.createElement('span');
    ph.className = 'thumb-placeholder';
    ph.textContent = '▶';
    thumb.appendChild(ph);
  }
  if (item.duration && item.duration > 0) {
    const dur = document.createElement('span');
    dur.className = 'duration';
    dur.textContent = formatDuration(item.duration);
    thumb.appendChild(dur);
  }

  const body = document.createElement('div');
  body.className = 'body';

  const when = document.createElement('div');
  when.className = 'when';
  when.textContent = item.name || formatWhen(item.date);

  const size = document.createElement('div');
  size.className = 'size';
  size.textContent = item.sizeText || '';

  body.appendChild(when);
  body.appendChild(size);
  // Dblclick em qualquer parte do body (nome OU tamanho) → edita o
  // nome. Antes estava so no .when — agora o tamanho tambem serve de
  // alvo, consistente com o single-click do body que tambem nao abre
  // o player. Sempre passa o `when` element pro editName (que ele e
  // o unico contenteditable; o size e display-only).
  body.ondblclick = (ev) => editName(ev, when, card.dataset.id);
  card.appendChild(thumb);
  card.appendChild(body);
  return card;
}

// Define a qual periodo uma data pertence. Labels traduzidos via I18n —
// 'months' e array indexado por month (0-11) lido com I18n.get(). Sem
// cache pra sobreviver a troca de idioma em runtime (language_changed
// re-renderiza recordings, e cada periodKey() consulta o bundle ativo).
const MONTHS_FALLBACK = ['Janeiro','Fevereiro','Março','Abril','Maio','Junho',
  'Julho','Agosto','Setembro','Outubro','Novembro','Dezembro'];
function periodKey(date) {
  const d = new Date(date);
  if (isNaN(d.getTime())) return { key: 'zzz-unknown', label: T('dateGroups.noDate'), order: 0 };
  const today = new Date();
  today.setHours(0,0,0,0);
  const y = new Date(d); y.setHours(0,0,0,0);
  const diff = (today - y) / 86400000; // dias
  if (diff <= 0) return { key: '00-hoje',   label: T('dateGroups.today'),     order: 0 };
  if (diff < 2)  return { key: '01-ontem',  label: T('dateGroups.yesterday'), order: 1 };
  if (diff < 7)  return { key: '02-semana', label: T('dateGroups.thisWeek'),  order: 2 };
  // Agrupa por mês/ano. Order = 1000000 - (Y*100 + M) pra desc.
  const ym = d.getFullYear() * 100 + d.getMonth();
  const months = I18n.get('months');
  const monthName = (Array.isArray(months) ? months[d.getMonth()]
                                           : null) || MONTHS_FALLBACK[d.getMonth()];
  return {
    key:   `99-${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}`,
    label: `${monthName} ${d.getFullYear()}`,
    order: 1000000 - ym
  };
}

function renderRecordings(items) {
  // Cache pra re-renderizar em troca de idioma (date groups + months
  // dependem do bundle ativo). Sem cache, language_changed nao tem
  // como repintar a lista.
  if (Array.isArray(items)) window._lastRecordingsItems = items;
  const grid = document.getElementById('recGrid');
  grid.innerHTML = '';
  const arr = (items || []).slice();
  arr.sort((a, b) => (b.date || '').localeCompare(a.date || ''));

  // Agrupa preservando ordem (arr ja vem mais novo -> mais antigo).
  const groups = []; // [{ key, label, order, items: [] }]
  const byKey = new Map();
  arr.forEach(item => {
    const p = periodKey(item.date);
    let g = byKey.get(p.key);
    if (!g) {
      g = { ...p, items: [] };
      byKey.set(p.key, g);
      groups.push(g);
    }
    g.items.push(item);
  });
  groups.sort((a, b) => a.order - b.order);

  groups.forEach(g => {
    const wrap = document.createElement('div');
    wrap.className = 'rec-group';
    wrap.dataset.groupKey = g.key;
    const h = buildGroupTitle(g.label);
    const inner = document.createElement('div');
    inner.className = 'rec-group-grid';
    g.items.forEach(item => inner.appendChild(buildRecCard(item)));
    wrap.appendChild(h);
    wrap.appendChild(inner);
    grid.appendChild(wrap);
  });
  // Limpa selecoes orfas (ids que nao tem mais card correspondente —
  // ex: arquivo deletado externamente, watcher trouxe lista nova).
  pruneOrphanSelections();
  // Re-sincroniza estado visual da selecao apos re-render.
  RecSelection._syncGroups();
  RecSelection._syncMode();

  updateRecMeta(arr);
}

function pruneOrphanSelections() {
  // Remove do RecSelection ids que nao tem card correspondente no DOM.
  // Sem isso, RecSelection.size() permanece > 0 apos rebuild que sumiu
  // com o card — clicks em outros cards entram em "modo selecao" em
  // vez de tocar o video.
  if (RecSelection.ids.size === 0) return;
  const orphans = [];
  RecSelection.ids.forEach(id => {
    if (!document.querySelector(
      `#recGrid .rec-card[data-id="${cssEscape(id)}"]`))
      orphans.push(id);
  });
  if (orphans.length === 0) return;
  orphans.forEach(id => RecSelection.ids.delete(id));
}

// Cria o <h3> do grupo com o titulo a esquerda e a bolinha de
// "selecionar todos do periodo" a direita.
function buildGroupTitle(label) {
  const h = document.createElement('h3');
  h.className = 'rec-group-title';
  const span = document.createElement('span');
  span.textContent = label;
  const check = document.createElement('div');
  check.className = 'rec-group-check';
  check.title = T('recordings.selectAllInGroup');
  check.dataset.state = 'none';
  check.innerHTML = CHECK_SVG;
  check.onclick = (e) => {
    e.stopPropagation();
    const group = check.closest('.rec-group');
    if (!group) return;
    const cards = group.querySelectorAll('.rec-card');
    const ids = Array.from(cards).map(c => c.dataset.id);
    // Se todos ja selecionados, des-seleciona; senao seleciona todos.
    const allSelected = check.dataset.state === 'all';
    RecSelection.setMany(ids, !allSelected);
  };
  h.appendChild(span);
  h.appendChild(check);
  return h;
}

function addRecordingCard(item) {
  const grid = document.getElementById('recGrid');
  const p = periodKey(item.date);
  let group = grid.querySelector(`.rec-group[data-group-key="${p.key}"]`);
  if (!group) {
    // Cria grupo "Hoje" no topo.
    group = document.createElement('div');
    group.className = 'rec-group';
    group.dataset.groupKey = p.key;
    const h = buildGroupTitle(p.label);
    const inner = document.createElement('div');
    inner.className = 'rec-group-grid';
    group.appendChild(h);
    group.appendChild(inner);
    grid.insertBefore(group, grid.firstChild);
  }
  const inner = group.querySelector('.rec-group-grid');
  const card = buildRecCard(item);
  inner.insertBefore(card, inner.firstChild);
  recalcRecMetaFromDom();
  RecSelection._syncGroups();
}

function renameRecordingCard(oldId, newId, newName) {
  const card = document.querySelector(`.rec-card[data-id="${cssEscape(oldId)}"]`);
  if (!card) return;
  card.dataset.id = newId;
  const when = card.querySelector('.when');
  if (when) when.textContent = newName;
}

function removeRecordingCard(id) {
  const card = document.querySelector(`.rec-card[data-id="${cssEscape(id)}"]`);
  if (card) {
    const group = card.closest('.rec-group');
    card.remove();
    if (group && !group.querySelector('.rec-card')) group.remove();
  }
  // Tira do set de selecao (silencioso — UI ja sumiu).
  if (RecSelection.ids.delete(id)) {
    RecSelection._syncGroups();
    RecSelection._syncMode();
  }
  recalcRecMetaFromDom();
}

// Espaco livre no disco da pasta de gravacoes — empurrado pelo backend
// junto com 'recordings_loaded'. -1 = nao foi possivel ler. Guardamos
// como state global pra que recalcRecMetaFromDom (disparado em deletes
// individuais) tambem renderize esse campo.
let _lastFreeBytes = -1;
const LOW_DISK_THRESHOLD = 5 * 1024 * 1024 * 1024; // 5 GB

// SVG inline do icone de aviso (triangulo laranja com !). Tamanho 13px
// pra casar com a altura da linha do meta. Wrap em span com data-hint
// pra disparar o tooltip custom (Hint) ao inves do nativo do browser.
// Funcao (nao const): T() precisa rodar em tempo de render, depois que o
// bundle de i18n chegou do backend — um const top-level seria avaliado
// antes do bundle carregar e congelaria a string no idioma de fallback.
function lowDiskIcon() {
  return '<span data-hint="' + T('recordings.lowSpaceHint') + '" ' +
    'style="display:inline-flex;vertical-align:middle">' +
    '<svg width="13" height="13" viewBox="0 0 24 24" ' +
    'fill="none" stroke="#f59e0b" stroke-width="2.2" stroke-linecap="round" ' +
    'stroke-linejoin="round" style="vertical-align:-2px;margin-left:5px" ' +
    'aria-label="' + T('recordings.lowSpace') + '">' +
    '<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>' +
    '<line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>' +
    '</svg></span>';
}

// Mesma logica de OBSEncoder.GetEncoderMaxDimension (Delphi) e do
// Settings._syncCodecMaxRes — extraida em helper top-level pra ser
// reusada em mais de um lugar. Se mudar aqui, mudar nos outros.
function codecMaxDim(codec, caps) {
  caps = caps || {};
  if (codec === 'h264-hw') return 4096;
  if (codec === 'h264-sw' || codec === 'hevc-hw' || codec === 'av1-hw') return 8192;
  // 'auto' ou desconhecido: chain prioriza h264-hw, entao limite = 4096
  // quando h264-hw esta presente nas caps.
  return caps.h264Hw ? 4096 : 8192;
}

// Triangulo de aviso (mesmo visual do LOW_DISK_ICON) com tooltip custom
// via data-hint no wrapper. Mostrado ao lado da resolucao agregada das
// telas selecionadas quando ela excede o limite do codec atual.
function codecLimitWarningIcon(maxDim) {
  const tip = T('warning.canvasOverflow', { max: maxDim });
  return '<span data-hint="' + escapeHtml(tip) + '" ' +
    'style="display:inline-flex;vertical-align:middle">' +
    '<svg width="13" height="13" viewBox="0 0 24 24" ' +
    'fill="none" stroke="#f59e0b" stroke-width="2.2" stroke-linecap="round" ' +
    'stroke-linejoin="round" style="vertical-align:-2px;margin-left:5px" ' +
    'aria-label="' + escapeHtml(tip) + '">' +
    '<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>' +
    '<line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>' +
    '</svg></span>';
}

function buildRecMetaHtml(fileCount, totalBytes, freeBytes) {
  const sep = '<span class="meta-sep">|</span>';
  // Pluralizacao via chaves _one / _other (padrao i18next).
  const fileKey = fileCount === 1 ? 'recordings.fileCount_one'
                                  : 'recordings.fileCount_other';
  let html = T(fileKey, { count: fileCount });
  if (totalBytes > 0)
    html += sep + T('recordings.spaceUsed', { size: formatBytes(totalBytes) });
  if (freeBytes >= 0) {
    html += sep + T('recordings.spaceFree', { size: formatBytes(freeBytes) });
    if (freeBytes < LOW_DISK_THRESHOLD) html += lowDiskIcon();
  }
  return html;
}

function updateRecMeta(arr) {
  const meta = document.getElementById('recMeta');
  if (!meta) return;
  const n = arr && arr.length ? arr.length : 0;
  let total = 0;
  if (Array.isArray(arr)) arr.forEach(it => { if (it && it.size) total += it.size; });
  meta.innerHTML = buildRecMetaHtml(n, total, _lastFreeBytes);
}

function recalcRecMetaFromDom() {
  const cards = document.querySelectorAll('#recGrid .rec-card');
  let total = 0;
  cards.forEach(c => { total += parseInt(c.dataset.size || '0', 10) || 0; });
  const meta = document.getElementById('recMeta');
  if (!meta) return;
  meta.innerHTML = buildRecMetaHtml(cards.length, total, _lastFreeBytes);
}

function formatBytes(b) {
  const KB = 1024, MB = KB * 1024, GB = MB * 1024;
  if (b >= GB) return (b / GB).toFixed(1) + ' GB';
  if (b >= MB) return Math.round(b / MB) + ' MB';
  if (b >= KB) return Math.round(b / KB) + ' KB';
  return b + ' B';
}

// Junta a lista de mudancas (pre-formatadas em portugues pelo backend
// — vide Build*ChangesArray em OBSBridge.pas) em uma string compacta
// pra exibir no body do toast "Dispositivos atualizados". Mostra ate
// 3 itens; o resto vira "…e mais N".
function formatDeviceChanges(changes) {
  if (!Array.isArray(changes) || changes.length === 0) return '';
  const MAX = 3;
  if (changes.length <= MAX) return changes.join(' · ');
  return changes.slice(0, MAX).join(' · ') +
    ' · ' + T('toast.andMore', { count: changes.length - MAX });
}

function formatBitrate(bps) {
  if (!bps || bps <= 0) return '—';
  if (bps >= 1000000) return (bps / 1000000).toFixed(2) + ' Mbps';
  if (bps >= 1000)    return Math.round(bps / 1000) + ' kbps';
  return bps + ' bps';
}

function formatFps(fps) {
  // Mantem casas decimais em taxas NTSC (29.97, 59.94) que aparecem
  // como fracoes nao-inteiras. Taxas exatas (30, 60, 120) ficam limpas.
  // Tolerancia 0.01 absorve ruido de ponto flutuante quando avg_frame_rate
  // ja vem como inteiro mas o backend perdeu precisao na divisao.
  if (!fps || fps <= 0) return '—';
  const rounded = Math.round(fps);
  if (Math.abs(fps - rounded) < 0.01) return rounded + ' fps';
  return fps.toFixed(2) + ' fps';
}

// SVG do botao de mutar/desmutar uma faixa de audio do player.
// Quando muted=true mostra speaker-X (cortado); senao speaker-com-ondas.
function MUTE_ICON_SVG(muted) {
  if (muted) {
    return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
      'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
      '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" ' +
      'fill="currentColor" stroke="none"/>' +
      '<line x1="23" y1="9" x2="17" y2="15"/>' +
      '<line x1="17" y1="9" x2="23" y2="15"/></svg>';
  }
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
    'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
    '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" ' +
    'fill="currentColor" stroke="none"/>' +
    '<path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>' +
    '<path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>';
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function infoRow(key, val) {
  return '<div class="player-info-row">' +
    '<span class="player-info-key">' + escapeHtml(key) + '</span>' +
    '<span class="player-info-val">' + val + '</span></div>';
}

// Usa CSS.escape nativo (lida com ] , espacos, ids iniciando com digito
// etc.); fallback so se indisponivel.
function cssEscape(s) {
  return (window.CSS && CSS.escape)
    ? CSS.escape(String(s))
    : String(s).replace(/["\\]/g, '\\$&');
}

// =====================================================================
// Editor de nome (rename de gravacao)
// =====================================================================
function editName(event, el, id) {
  event.stopPropagation();
  if (el.classList.contains('editing')) return;
  const original = el.textContent;
  el.classList.add('editing');
  el.contentEditable = 'true';
  el.focus();

  const range = document.createRange();
  range.selectNodeContents(el);
  const sel = window.getSelection();
  sel.removeAllRanges();
  sel.addRange(range);

  const finish = (commit) => {
    el.removeEventListener('blur', onBlur);
    el.removeEventListener('keydown', onKey);
    el.contentEditable = 'false';
    el.classList.remove('editing');
    if (!commit) { el.textContent = original; return; }
    // Sanitiza no cliente: remove quebras e caracteres ilegais de nome de
    // arquivo (\ / : * ? " < > |) e limita o tamanho. O backend tambem
    // sanitiza (HandleRenameRecording), mas isto evita round-trip e da
    // feedback imediato no DOM com o nome que sera realmente usado.
    const newName = el.textContent
      .replace(/[\r\n]+/g, ' ')
      .replace(/[\\/:*?"<>|]/g, '')
      .trim()
      .slice(0, 150);
    if (newName === '' || newName === original) {
      el.textContent = original;
      return;
    }
    el.textContent = newName;
    Bridge.send('rename_recording', { id, newName });
  };
  const onBlur = () => finish(true);
  const onKey = (ev) => {
    if (ev.key === 'Enter') { ev.preventDefault(); el.blur(); }
    else if (ev.key === 'Escape') { ev.preventDefault(); finish(false); }
  };
  el.addEventListener('blur', onBlur);
  el.addEventListener('keydown', onKey);
}


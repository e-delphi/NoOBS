// =====================================================================
// Capturar... (seletor: uma janela OU uma area da tela)
// =====================================================================
// Duas abas. JANELA e o seletor de sempre (abaixo). AREA so explica e
// manda `select_capture_area`: quem escurece os monitores e deixa arrastar
// o retangulo e o backend (janela nativa, area_select.rs) — o WebView nao
// cobre os outros monitores. Soltar o mouse ja comeca a gravar; a moldura
// da gravacao (area_frame.rs) libera mover/redimensionar com Ctrl+Shift+clique no
// botao do canto. A aba usada por ultimo fica em localStorage.
//
// O botao ao lado do buffer abre uma grade com as janelas abertas. O
// backend (OBSBridge.HandleListCaptureWindows) manda a lista na hora
// (`capture_windows`) e depois as previas uma a uma
// (`capture_window_thumb`, URL do HTTP local) — PrintWindow custa dezenas
// de ms por janela e a grade nao pode esperar todas.
//
// Clicar numa janela manda `record_window` com o id; o backend grava SO
// ela (fonte window_capture do OBS), com o audio de sempre. Nada aqui
// guarda a identidade da janela: o id so aponta pra ultima lista do
// backend. Pegadinha #66.
//
// "Gravar o som do app" vai junto (`appAudio`): o som SO daquele programa
// numa faixa propria. A escolha e preferencia de uso desta maquina
// (localStorage, como a legenda do player), nao do config.json. O backend
// diz no `capture_windows` se o Windows tem captura por processo.
const WindowPicker = {
  items: [],
  thumbs: {},      // id -> url (chega depois da lista)
  seq: 0,          // rodada corrente (pushes de rodada velha sao ignorados)
  loading: false,
  AUDIO_KEY: 'noobs.windowPicker.appAudio',
  MODE_KEY: 'noobs.windowPicker.mode',
  mode: 'window',   // 'window' | 'area'

  _savedMode() {
    try { return localStorage.getItem(this.MODE_KEY) === 'area' ? 'area' : 'window'; }
    catch (_) { return 'window'; }
  },

  // Troca de aba. Na de janela a lista e pedida na primeira vez (abrir direto
  // na aba de area nao gasta o PrintWindow de todas as janelas a toa).
  setMode(mode) {
    this.mode = mode === 'area' ? 'area' : 'window';
    try { localStorage.setItem(this.MODE_KEY, this.mode); } catch (_) {}
    const isArea = this.mode === 'area';
    const tabs = [['windowPickerModeWindow', !isArea], ['windowPickerModeArea', isArea]];
    for (const [id, on] of tabs) {
      const el = document.getElementById(id);
      if (el) el.setAttribute('aria-selected', on ? 'true' : 'false');
    }
    const winPane = document.getElementById('windowPickerWindowPane');
    const areaPane = document.getElementById('windowPickerAreaPane');
    if (winPane) winPane.hidden = isArea;
    if (areaPane) areaPane.hidden = !isArea;
    const refresh = document.getElementById('windowPickerRefresh');
    if (refresh) refresh.hidden = isArea;
    const sub = document.getElementById('windowPickerSub');
    if (sub) sub.textContent = T(isArea ? 'windowPicker.areaSubtitle' : 'windowPicker.subtitle');
    if (isArea) {
      const btn = document.getElementById('windowPickerAreaBtn');
      if (btn) setTimeout(() => btn.focus(), 30);
    } else {
      if (!this.items.length && !this.loading) this.refresh();
      const search = document.getElementById('windowPickerSearch');
      if (search) setTimeout(() => search.focus(), 30);
    }
  },

  // Fecha o modal e deixa o backend escurecer os monitores. O modal tem que
  // sumir ANTES: senao ele apareceria por baixo do escuro, no meio da tela.
  selectArea() {
    this.close();
    Bridge.send('select_capture_area');
  },

  _audioWanted() {
    try { return localStorage.getItem(this.AUDIO_KEY) === '1'; }
    catch (_) { return false; }
  },

  onAudioChange(on) {
    try { localStorage.setItem(this.AUDIO_KEY, on ? '1' : '0'); } catch (_) {}
  },

  _syncAudio(supported) {
    const wrap = document.getElementById('windowPickerAudioWrap');
    const cb = document.getElementById('windowPickerAudio');
    if (!wrap || !cb) return;
    if (supported !== undefined) wrap.hidden = !supported;
    cb.checked = this._audioWanted();
  },

  open() {
    if (document.body.classList.contains('recording')) return;
    const ov = document.getElementById('windowPickerOverlay');
    if (!ov) return;
    // Sem isso o botao fica com o foco e, ao fechar com Esc, o navegador
    // desenha o anel azul de foco em volta dele.
    const btn = document.getElementById('windowRecordBtn');
    if (btn) btn.blur();
    const search = document.getElementById('windowPickerSearch');
    if (search) search.value = '';
    this._syncAudio();
    ov.classList.add('visible');
    // Lista nova a cada abertura (janelas abrem/fecham entre uma e outra).
    this.items = [];
    this.setMode(this._savedMode());
  },

  close() {
    const ov = document.getElementById('windowPickerOverlay');
    if (ov) ov.classList.remove('visible');
  },

  isOpen() {
    const ov = document.getElementById('windowPickerOverlay');
    return !!(ov && ov.classList.contains('visible'));
  },

  refresh() {
    this.loading = true;
    this.items = [];
    this.thumbs = {};
    this.render();
    Bridge.send('list_capture_windows');
  },

  onList(data) {
    if (!data) return;
    this.seq = data.seq || 0;
    this.items = Array.isArray(data.items) ? data.items : [];
    this.thumbs = {};
    this.loading = false;
    this._syncAudio(data.appAudio !== false);
    this.render();
  },

  onThumb(data) {
    if (!data || data.seq !== this.seq || !data.id) return;
    this.thumbs[data.id] = data.thumb || '';
    // So troca a imagem do cartao — refazer a grade a cada previa faria o
    // hover/foco piscar.
    const grid = document.getElementById('windowPickerGrid');
    if (!grid) return;
    const card = grid.querySelector(`.winpick-item[data-id="${cssEscape(data.id)}"]`);
    if (card) this._fillThumb(card.querySelector('.winpick-thumb'), data.thumb);
  },

  pick(id) {
    if (!id) return;
    this.close();
    if (typeof triggerRecordRipple === 'function') triggerRecordRipple();
    const wrap = document.getElementById('windowPickerAudioWrap');
    const cb = document.getElementById('windowPickerAudio');
    const appAudio = !!(cb && cb.checked && !(wrap && wrap.hidden));
    Bridge.send('record_window', { id, appAudio });
  },

  _placeholderSvg() {
    return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
      'stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">' +
      '<rect x="3" y="4" width="18" height="16" rx="2"/>' +
      '<line x1="3" y1="9" x2="21" y2="9"/></svg>';
  },

  _fillThumb(box, url) {
    if (!box) return;
    if (!url) {
      box.innerHTML = this._placeholderSvg();
      return;
    }
    const img = document.createElement('img');
    img.alt = '';
    img.draggable = false;
    img.src = url;
    img.onerror = () => { box.innerHTML = this._placeholderSvg(); };
    box.innerHTML = '';
    box.appendChild(img);
  },

  render() {
    const grid = document.getElementById('windowPickerGrid');
    const empty = document.getElementById('windowPickerEmpty');
    if (!grid || !empty) return;
    const q = ((document.getElementById('windowPickerSearch') || {}).value || '')
      .trim().toLowerCase();
    const list = q
      ? this.items.filter(w => (w.title || '').toLowerCase().includes(q) ||
                               (w.exe || '').toLowerCase().includes(q))
      : this.items;

    grid.innerHTML = '';
    if (this.loading) {
      empty.hidden = false;
      empty.innerHTML = '<span></span><span class="loading-dots"></span>';
      empty.firstChild.textContent = T('windowPicker.loading');
      return;
    }
    if (!list.length) {
      empty.hidden = false;
      empty.textContent = this.items.length ? T('windowPicker.noMatch')
                                            : T('windowPicker.empty');
      return;
    }
    empty.hidden = true;

    for (const w of list) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'winpick-item';
      btn.dataset.id = w.id;
      btn.setAttribute('aria-label', T('windowPicker.pick') + ': ' + (w.title || ''));
      btn.onclick = () => this.pick(w.id);

      const thumb = document.createElement('div');
      thumb.className = 'winpick-thumb';
      this._fillThumb(thumb, this.thumbs[w.id] || '');

      const name = document.createElement('div');
      name.className = 'winpick-name';
      name.textContent = w.title || '';
      name.title = w.title || '';

      const info = document.createElement('div');
      info.className = 'winpick-info';
      info.textContent = (w.exe || '') + (w.w && w.h ? ` · ${w.w}×${w.h}` : '');

      btn.append(thumb, name, info);
      grid.appendChild(btn);
    }
  },
};

// Esc fecha; Enter no campo de busca grava a primeira janela da lista.
document.addEventListener('keydown', (e) => {
  if (!WindowPicker.isOpen()) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    WindowPicker.close();
  } else if (e.key === 'Enter' && WindowPicker.mode === 'area' &&
             !(e.target && e.target.closest && e.target.closest('.winpick-modes'))) {
    e.preventDefault();
    WindowPicker.selectArea();
  } else if (e.key === 'Enter' && e.target && e.target.id === 'windowPickerSearch') {
    const first = document.querySelector('#windowPickerGrid .winpick-item');
    if (first) {
      e.preventDefault();
      WindowPicker.pick(first.dataset.id);
    }
  }
});

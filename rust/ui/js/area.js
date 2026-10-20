// =====================================================================
// Captura de area (aba "Area" das Configuracoes) — bridge/area.rs.
//
// Tres coisas: a AREA SALVA (escolhida pela mesma selecao do
// "Capturar... -> Area", so que guardada em vez de gravar), o ATALHO que
// grava ela (padrao Ctrl+Shift+Pause/Break; o backend so o registra com uma
// area salva) e o ATRASO antes de comecar (0 = desligado). O estado vem
// inteiro do backend em `area_state`; a contagem do atraso, em
// `area_countdown`.
// =====================================================================
const AreaCapture = {
  state: { saved: null, hotkey: '', delay: 0, delayMax: 60 },
  currentHotkey: '',   // ultimo atalho aceito (pra nao reenviar o mesmo)
  countdown: 0,
  DEFAULT_HOTKEY: 'Ctrl+Shift+Pause/Break',
  HOTKEY_PREFIX: 'settingsAreaHotkey',
  DELAY_OPTIONS: [0, 3, 5, 10, 15, 30],

  applyState(data) {
    if (!data) return;
    this.state = Object.assign({}, this.state, data);
    this.currentHotkey = this.state.hotkey || '';
    this._renderSavedButton();
    this.renderPreviews();
    const modal = document.getElementById('settingsOverlay');
    if (modal && modal.classList.contains('visible')) this.loadIntoSettings();
  },

  // ---- Previa: monitores + area salva destacada --------------------------
  // Monitores e area estao no MESMO sistema de coordenadas (pixels fisicos
  // da area de trabalho virtual), entao a previa e so uma regra de tres: a
  // caixa recebe o aspect-ratio do conjunto e cada peca vai em %, sem
  // recalcular quando a janela muda de tamanho. As imagens sao as da tela
  // inicial (`monitor_thumbs`), inclusive de monitores ocultos.
  _thumbs: {},

  _monitors() {
    const all = (typeof Devices !== 'undefined' && Devices.all && Devices.all.monitors) || [];
    return all.filter(m => m.width > 0 && m.height > 0);
  },

  renderPreviews() {
    this._renderPreview('settingsAreaPreview');
    this._renderPreview('windowPickerAreaPreview');
    // No seletor, a previa substitui o desenho explicativo.
    const pv = document.getElementById('windowPickerAreaPreview');
    const art = document.querySelector('.winpick-area-art');
    if (pv && art) art.style.display = pv.hidden ? '' : 'none';
  },

  _renderPreview(boxId) {
    const box = document.getElementById(boxId);
    if (!box) return;
    const a = this.state.saved;
    const mons = this._monitors();
    if (!a || !mons.length) {
      box.hidden = true;
      box.innerHTML = '';
      return;
    }
    // Limites: todos os monitores + a propria area (pode ter ficado
    // parcialmente fora se um monitor saiu).
    let x0 = a.x, y0 = a.y, x1 = a.x + a.w, y1 = a.y + a.h;
    for (const m of mons) {
      x0 = Math.min(x0, m.x); y0 = Math.min(y0, m.y);
      x1 = Math.max(x1, m.x + m.width); y1 = Math.max(y1, m.y + m.height);
    }
    const bw = x1 - x0, bh = y1 - y0;
    const place = (el, x, y, w, h) => {
      el.style.left = ((x - x0) / bw * 100) + '%';
      el.style.top = ((y - y0) / bh * 100) + '%';
      el.style.width = (w / bw * 100) + '%';
      el.style.height = (h / bh * 100) + '%';
    };
    // Monitor onde fica (a maior parte d)a area: ganha o destaque no nome.
    const inter = m => Math.max(0, Math.min(a.x + a.w, m.x + m.width) - Math.max(a.x, m.x)) *
                       Math.max(0, Math.min(a.y + a.h, m.y + m.height) - Math.max(a.y, m.y));
    let host = null, best = 0;
    for (const m of mons) { const v = inter(m); if (v > best) { best = v; host = m; } }

    box.hidden = false;
    box.style.aspectRatio = bw + ' / ' + bh;
    box.innerHTML = '';
    for (const m of mons) {
      const el = document.createElement('div');
      el.className = 'area-preview-mon' + (m === host ? ' host' : '');
      el.dataset.id = m.id;
      place(el, m.x, m.y, m.width, m.height);
      const url = this._thumbs[m.id] || m.thumb || '';
      if (url) {
        const img = document.createElement('img');
        img.alt = '';
        img.draggable = false;
        img.src = url;
        el.appendChild(img);
      }
      if (mons.length > 1) {
        const name = document.createElement('span');
        name.className = 'area-preview-name';
        name.textContent = m.name || '';
        el.appendChild(name);
      }
      box.appendChild(el);
    }
    const r = document.createElement('div');
    r.className = 'area-preview-rect';
    place(r, a.x, a.y, a.w, a.h);
    const size = document.createElement('span');
    size.className = 'area-preview-size';
    size.textContent = a.w + ' × ' + a.h;
    r.appendChild(size);
    box.appendChild(r);
  },

  // Tique das previas (a cada ~500 ms): troca so as imagens, sem refazer.
  updateThumbs(items) {
    for (const it of items) this._thumbs[it.id] = it.thumb || '';
    for (const boxId of ['settingsAreaPreview', 'windowPickerAreaPreview']) {
      const box = document.getElementById(boxId);
      if (!box || box.hidden) continue;
      box.querySelectorAll('.area-preview-mon').forEach(el => {
        const url = this._thumbs[el.dataset.id];
        if (url === undefined) return;
        let img = el.querySelector('img');
        if (!url) { if (img) img.remove(); return; }
        if (!img) {
          img = document.createElement('img');
          img.alt = '';
          img.draggable = false;
          el.insertBefore(img, el.firstChild);
        }
        if (img.getAttribute('src') !== url) img.src = url;
      });
    }
  },

  _savedText() {
    const a = this.state.saved;
    if (!a) return T('settings.area.none');
    return T('settings.area.savedValue', { w: a.w, h: a.h, x: a.x, y: a.y });
  },

  loadIntoSettings() {
    const txt = document.getElementById('settingsAreaSaved');
    if (txt) {
      txt.textContent = this._savedText();
      txt.classList.toggle('empty', !this.state.saved);
    }
    const def = document.getElementById('settingsAreaDefine');
    if (def) def.textContent = T(this.state.saved ? 'settings.area.redefine' : 'settings.area.define');
    const clr = document.getElementById('settingsAreaClear');
    if (clr) clr.hidden = !this.state.saved;
    this._fillDelay(this.state.delay || 0);
    this.renderPreviews();
    Settings._loadHotkeyIntoUi(this.currentHotkey, this.HOTKEY_PREFIX);
  },

  _fillDelay(value) {
    const sel = document.getElementById('settingsAreaDelay');
    if (!sel) return;
    const opts = this.DELAY_OPTIONS.slice();
    // Valor do config fora da lista (JSON editado a mao) continua escolhivel.
    if (!opts.includes(value)) { opts.push(value); opts.sort((a, b) => a - b); }
    sel.innerHTML = '';
    for (const v of opts) {
      const o = document.createElement('option');
      o.value = String(v);
      o.textContent = v === 0 ? T('settings.area.delayOff')
                              : T('settings.area.delaySec', { sec: v });
      sel.appendChild(o);
    }
    sel.value = String(value);
  },

  // A selecao escurece os monitores: as Configuracoes ficam embaixo, e o
  // `area_state` que volta (salva ou cancelada) atualiza a aba.
  define() { Bridge.send('define_capture_area', {}); },
  clear() { Bridge.send('clear_capture_area', {}); },

  onDelayChange(v) {
    Bridge.send('set_area_delay', { sec: parseInt(v, 10) || 0 });
  },

  onHotkeyChange() {
    Settings._updateHotkeyPreview(this.HOTKEY_PREFIX);
    this._commitHotkey();
  },
  // Mesmo fluxo do atalho do buffer: nao pode repetir o de gravar nem o do
  // buffer (o Windows so aceita um registro por combinacao). O backend
  // confere de novo e devolve o estado.
  async _commitHotkey() {
    const hotkey = Settings._readHotkeyFromUi(this.HOTKEY_PREFIX);
    if (hotkey === this.currentHotkey) return;
    if (hotkey !== '') {
      const taken = [Settings.currentHotkey,
                     typeof Replay !== 'undefined' ? Replay.currentHotkey : ''];
      if (taken.includes(hotkey)) {
        Toast.show(T('toast.invalidHotkey'), T('settings.area.hotkeyConflict'),
          { warn: true, ttl: 7000 });
        return;
      }
      const validation = await validateHotkeyWithBackend(hotkey);
      if (!validation.ok) {
        Toast.show(T('toast.invalidHotkey'), validation.reason +
          ' ' + T('settings.hotkey.chooseAnother'), { warn: true, ttl: 7000 });
        return;
      }
    }
    Bridge.send('set_area_hotkey', { hotkey });
    this.currentHotkey = hotkey;
  },

  // "Restaurar padroes": atalho e atraso voltam; a area salva e escolha do
  // usuario e fica.
  restoreDefaults() {
    Settings._loadHotkeyIntoUi(this.DEFAULT_HOTKEY, this.HOTKEY_PREFIX);
    this._commitHotkey();
    if ((this.state.delay || 0) !== 0) this.onDelayChange(0);
  },

  // Botao "Gravar a area salva" do seletor Capturar... -> Area.
  _renderSavedButton() {
    const btn = document.getElementById('windowPickerSavedAreaBtn');
    if (!btn) return;
    const a = this.state.saved;
    btn.hidden = !a;
    if (a) btn.textContent = T('windowPicker.savedArea', { w: a.w, h: a.h });
  },

  // Contagem do atraso: a linha de status da tela principal diz quanto falta
  // e que clicar em gravar cancela.
  onCountdown(sec) {
    this.countdown = sec || 0;
    const st = document.getElementById('recStatusText');
    if (!st) return;
    if (this.countdown > 0)
      st.textContent = T('record.areaCountdown', { sec: this.countdown });
    else if (!_lastRecordingActive)
      st.textContent = T('record.statusReady');
  },

  // Troca de idioma: textos montados aqui.
  relabel() {
    this._renderSavedButton();
    if (this.countdown > 0) this.onCountdown(this.countdown);
    const modal = document.getElementById('settingsOverlay');
    if (modal && modal.classList.contains('visible')) this.loadIntoSettings();
  },
};

// =====================================================================
// Aba Atalhos — lista de atalhos globais (bridge/hotkeys.rs).
//
// Cada linha e uma combinacao e o que ela faz:
//   record  grava as telas
//   window  grava a janela cujo titulo CONTEM o texto (varias casando: abre
//           o seletor so com elas; nenhuma: avisa)
//   area    grava o retangulo salvo na linha, ou abre a selecao na hora
//   replay  salva o trecho do buffer (so ativo com o buffer ligado)
// Os de gravacao aceitam um atraso (contagem antes de comecar) e, apertados
// de novo, param a gravacao / cancelam a contagem.
//
// A lista vive no backend: a cada mudanca a tela manda a lista INTEIRA
// (`set_hotkeys`) e redesenha com o `hotkeys_state` que volta — que traz
// tambem a situacao de cada linha (repetida, reservada, em uso por outro
// programa...). Combinacao repetida e recusada aqui antes de mandar.
// =====================================================================
const Hotkeys = {
  items: [],
  status: [],
  delayMax: 60,
  defining: null,
  DELAY_OPTIONS: [0, 3, 5, 10, 15, 30],
  MODES: ['record', 'window', 'area', 'replay'],
  _pending: false,   // estado chegou com um campo de texto em edicao

  applyState(data) {
    if (!data) return;
    this.items = Array.isArray(data.items) ? data.items : [];
    this.status = Array.isArray(data.status) ? data.status : [];
    this.delayMax = data.delayMax || 60;
    this.defining = (data.defining === undefined) ? null : data.defining;
    this.render();
    this.renderPickerAreas();
  },

  _send() {
    Bridge.send('set_hotkeys', { items: this.items });
  },

  _isEditing() {
    const a = document.activeElement;
    const list = document.getElementById('hotkeysList');
    return !!(a && list && list.contains(a) && a.tagName === 'INPUT' && a.type === 'text');
  },

  // ---- Linhas ------------------------------------------------------------
  render() {
    const list = document.getElementById('hotkeysList');
    if (!list) return;
    // Um push no meio da digitacao de um titulo refaria o campo e levaria o
    // cursor (pegadinha #56): espera o blur.
    if (this._isEditing()) { this._pending = true; return; }
    this._pending = false;
    list.innerHTML = '';
    if (!this.items.length) {
      const empty = document.createElement('div');
      empty.className = 'settings-hint hk-empty';
      empty.textContent = T('hotkeys.empty');
      list.appendChild(empty);
      return;
    }
    this.items.forEach((it, i) => list.appendChild(this._row(it, i)));
  },

  _row(it, i) {
    const row = document.createElement('div');
    row.className = 'hk-row';
    row.dataset.index = i;

    // Cabecalho: modo + combinacao + remover.
    const head = document.createElement('div');
    head.className = 'hk-row-head';
    const mode = document.createElement('select');
    mode.className = 'settings-input hk-mode';
    for (const m of this.MODES) {
      const o = document.createElement('option');
      o.value = m;
      o.textContent = T('hotkeys.mode.' + m);
      mode.appendChild(o);
    }
    mode.value = it.mode;
    mode.onchange = () => this.setMode(i, mode.value);
    head.appendChild(mode);
    head.appendChild(this._builder(i));
    const rm = document.createElement('button');
    rm.type = 'button';
    rm.className = 'hk-remove';
    rm.title = T('hotkeys.remove');
    rm.setAttribute('aria-label', T('hotkeys.remove'));
    rm.textContent = '×';
    rm.onclick = () => this.remove(i);
    head.appendChild(rm);
    row.appendChild(head);

    // Corpo: o que cada modo precisa.
    const body = document.createElement('div');
    body.className = 'hk-row-body';
    if (it.mode === 'window') {
      const title = document.createElement('input');
      title.type = 'text';
      title.className = 'settings-input hk-title';
      title.placeholder = T('hotkeys.titlePlaceholder');
      title.value = it.title || '';
      title.onchange = () => this.update(i, { title: title.value.trim() });
      title.onblur = () => { if (this._pending) setTimeout(() => this.render(), 0); };
      body.appendChild(this._labeled('hotkeys.titleLabel', title));
      const audio = document.createElement('label');
      audio.className = 'settings-check-row hk-audio';
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = !!it.appAudio;
      cb.onchange = () => this.update(i, { appAudio: cb.checked });
      const sp = document.createElement('span');
      sp.textContent = T('windowPicker.appAudio');
      audio.append(cb, sp);
      body.appendChild(audio);
    } else if (it.mode === 'area') {
      const wrap = document.createElement('div');
      wrap.className = 'hk-area';
      const txt = document.createElement('span');
      txt.className = 'area-saved-text' + (it.area ? '' : ' empty');
      txt.textContent = it.area
        ? T('hotkeys.areaValue', { w: it.area.w, h: it.area.h, x: it.area.x, y: it.area.y })
        : T('hotkeys.areaAsk');
      const def = document.createElement('button');
      def.type = 'button';
      def.className = 'settings-btn';
      def.textContent = this.defining === i ? T('hotkeys.areaChoosing')
                      : T(it.area ? 'hotkeys.areaRedefine' : 'hotkeys.areaDefine');
      def.disabled = this.defining !== null;
      def.onclick = () => Bridge.send('define_hotkey_area', { index: i });
      wrap.append(txt, def);
      if (it.area) {
        const clr = document.createElement('button');
        clr.type = 'button';
        clr.className = 'settings-btn';
        clr.textContent = T('hotkeys.areaClear');
        clr.onclick = () => this.update(i, { area: null });
        wrap.appendChild(clr);
      }
      body.appendChild(wrap);
      const pv = document.createElement('div');
      pv.className = 'area-preview hk-area-preview';
      pv.hidden = true;
      body.appendChild(pv);
      AreaCapture.renderPreview(pv, it.area);
    }
    if (it.mode !== 'replay') {
      const sel = document.createElement('select');
      sel.className = 'settings-input hk-delay';
      const value = it.delaySec || 0;
      const opts = this.DELAY_OPTIONS.filter(v => v <= this.delayMax);
      // Valor do config fora da lista (JSON editado a mao) continua escolhivel.
      if (!opts.includes(value)) { opts.push(value); opts.sort((a, b) => a - b); }
      for (const v of opts) {
        const o = document.createElement('option');
        o.value = String(v);
        o.textContent = v === 0 ? T('hotkeys.delayOff') : T('hotkeys.delaySec', { sec: v });
        sel.appendChild(o);
      }
      sel.value = String(value);
      sel.onchange = () => this.update(i, { delaySec: parseInt(sel.value, 10) || 0 });
      body.appendChild(this._labeled('hotkeys.delayLabel', sel));
    }
    if (body.childNodes.length) row.appendChild(body);

    const st = this.status[i] || '';
    if (st) {
      const msg = document.createElement('div');
      msg.className = 'settings-hint hk-status' + (st === 'inactive' ? '' : ' warn');
      msg.textContent = T('hotkeys.status.' + st);
      row.appendChild(msg);
    }
    return row;
  },

  _labeled(key, control) {
    const l = document.createElement('label');
    l.className = 'hk-field';
    const s = document.createElement('span');
    s.className = 'hk-field-label';
    s.textContent = T(key);
    l.append(s, control);
    return l;
  },

  // Mesmo construtor das Configuracoes (checkboxes + tecla), com o prefixo
  // da linha nos ids — os helpers do Settings fazem o resto.
  _builder(i) {
    const prefix = 'hkRow' + i;
    const b = document.createElement('div');
    b.className = 'hotkey-builder';
    const mods = document.createElement('div');
    mods.className = 'hotkey-mods';
    for (const [m, key] of [['Ctrl', 'ctrl'], ['Shift', 'shift'], ['Alt', 'alt'], ['Win', 'win']]) {
      const l = document.createElement('label');
      l.className = 'hotkey-mod';
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.id = prefix + m;
      cb.onchange = () => this.onHotkeyChange(i);
      const s = document.createElement('span');
      s.textContent = T('settings.hotkey.' + key);
      l.append(cb, s);
      mods.appendChild(l);
    }
    const plus = document.createElement('span');
    plus.className = 'hotkey-plus';
    plus.textContent = '+';
    const key = document.createElement('select');
    key.className = 'settings-input hotkey-key';
    key.id = prefix + 'Key';
    key.onchange = () => this.onHotkeyChange(i);
    b.append(mods, plus, key);
    // Os ids so existem depois de entrar no DOM: carrega no proximo tique.
    setTimeout(() => {
      if (document.getElementById(prefix + 'Key'))
        Settings._loadHotkeyIntoUi(this.items[i] ? this.items[i].hotkey : '', prefix);
    }, 0);
    return b;
  },

  async onHotkeyChange(i) {
    const prefix = 'hkRow' + i;
    const spec = Settings._readHotkeyFromUi(prefix);
    const it = this.items[i];
    if (!it || spec === it.hotkey) return;
    const back = () => Settings._loadHotkeyIntoUi(it.hotkey, prefix);
    // So modificadores marcados ainda nao e combinacao: espera a tecla.
    const keySel = document.getElementById(prefix + 'Key');
    if (spec && keySel && !keySel.value) return;
    if (spec) {
      const other = this.items.findIndex((x, j) => j !== i && x.hotkey &&
                                         x.hotkey.toLowerCase() === spec.toLowerCase());
      if (other >= 0) {
        Toast.show(T('toast.invalidHotkey'),
          T('hotkeys.duplicate', { mode: T('hotkeys.mode.' + this.items[other].mode) }),
          { warn: true, ttl: 7000 });
        back();
        return;
      }
      const validation = await validateHotkeyWithBackend(spec);
      if (!validation.ok) {
        Toast.show(T('toast.invalidHotkey'), validation.reason +
          ' ' + T('settings.hotkey.chooseAnother'), { warn: true, ttl: 7000 });
        back();
        return;
      }
    }
    this.update(i, { hotkey: spec });
  },

  update(i, patch) {
    if (!this.items[i]) return;
    this.items[i] = Object.assign({}, this.items[i], patch);
    this._send();
  },

  setMode(i, mode) {
    const it = this.items[i];
    if (!it) return;
    // Campos de um modo nao vazam pro outro (o backend descartaria igual).
    const next = { mode, hotkey: it.hotkey };
    if (mode === 'window') { next.title = ''; next.appAudio = false; }
    if (mode === 'area') next.area = null;
    if (mode !== 'replay') next.delaySec = it.delaySec || 0;
    this.items[i] = next;
    this._send();
  },

  add() {
    this.items.push({ mode: 'record', hotkey: '', delaySec: 0 });
    this._send();
  },

  remove(i) {
    this.items.splice(i, 1);
    this._send();
  },

  restoreDefaults() { Bridge.send('reset_hotkeys'); },

  // Monitores mudaram: refaz as previas das linhas de area.
  renderPreviews() {
    document.querySelectorAll('#hotkeysList .hk-row').forEach(row => {
      const it = this.items[+row.dataset.index];
      const pv = row.querySelector('.hk-area-preview');
      if (it && pv) AreaCapture.renderPreview(pv, it.area);
    });
  },

  // "Capturar... -> Area": um botao por atalho de area com retangulo salvo.
  renderPickerAreas() {
    const box = document.getElementById('windowPickerSavedAreas');
    if (!box) return;
    box.innerHTML = '';
    this.items.forEach((it, i) => {
      if (it.mode !== 'area' || !it.area) return;
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'export-btn winpick-area-btn';
      btn.textContent = T('windowPicker.savedArea', { w: it.area.w, h: it.area.h }) +
                        (it.hotkey ? ' · ' + it.hotkey : '');
      btn.onclick = () => { WindowPicker.close(); Bridge.send('trigger_hotkey', { index: i }); };
      box.appendChild(btn);
    });
    box.hidden = !box.childNodes.length;
  },

  relabel() {
    this.render();
    this.renderPickerAreas();
  },
};

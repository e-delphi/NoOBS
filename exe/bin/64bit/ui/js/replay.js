// =====================================================================
// Buffer em memoria (replay buffer)
// =====================================================================
// Com o buffer ligado o backend grava continuamente na RAM (saida
// replay_buffer da libobs) e guarda so os ultimos N segundos / M MB. O
// atalho global ou o botao "Salvar trecho" transformam o que esta guardado
// numa gravacao normal da biblioteca e ESVAZIAM o buffer — salvar de novo em
// seguida so traz a continuacao, com os dois trechos se SOBREPONDO por ate um
// keyframe (nunca com buraco). Ver OBSBridge "Buffer em memoria" e
// TOBSEngine.SaveReplay.
//
// Buffer e gravacao manual sao um de cada vez: gravar desliga o buffer e,
// ao terminar, ele volta sozinho se continuar ligado. Por isso o estado
// "ligado" (enabled, a vontade do usuario nesta sessao) e distinto de
// "ativo" (guardando agora).
//
// O estado chega inteiro no push `replay_state` — nada e deduzido aqui.
const Replay = {
  // enabled = ligado NESTA sessao (o botao da tela principal);
  // autoStart = ligar sozinho ao abrir o app (Configuracoes). Sao duas
  // coisas distintas de proposito — ver HandleSetReplayAutoStart.
  state: { enabled: false, autoStart: false, active: false, saving: false,
           sinceMs: 0, maxSec: 300, maxMb: 2048, maxMbLimit: 2048, hotkey: '',
           apps: '', indicator: true, indicatorCorner: 'top-right',
           indicatorOpacity: 90 },
  _receivedAt: 0,     // performance.now() do ultimo push (base do relogio)
  _ticker: null,
  currentHotkey: '',  // ultimo atalho aceito (pra nao reenviar o mesmo)

  // Opcoes dos seletores das Configuracoes. Valores fora da lista (config
  // editado a mao) entram como opcao extra em _fillSelect.
  SEC_OPTIONS: [30, 60, 120, 300, 600, 900, 1800, 3600],
  // Degraus de 1 GB ate 8 e de 2 em 2 depois: entre 8 e 16 direto o salto
  // era grande demais pra uma escolha que custa RAM. Tudo acima do teto da
  // maquina (maxMbLimit) e cortado, e o ultimo item e sempre o proprio
  // teto ("Máximo (N GB)") — ver _mbOptions.
  MB_OPTIONS: [512, 1024, 2048, 3072, 4096, 5120, 6144, 7168, 8192, 10240,
               12288, 14336, 16384, 20480, 24576, 28672],
  DEFAULT_MAX_SEC: 300,
  DEFAULT_MAX_MB: 2048,
  DEFAULT_HOTKEY: 'Ctrl+Shift+F10',
  HOTKEY_PREFIX: 'settingsReplayHotkey',

  applyState(data) {
    if (!data) return;
    this.state = Object.assign({}, this.state, data);
    this._receivedAt = performance.now();
    this.currentHotkey = this.state.hotkey || '';
    this.render();
    // Relogio de "quanto ja esta guardado" anda sozinho entre pushes: o
    // backend so manda estado quando algo muda.
    if (this.state.active && !this._ticker)
      this._ticker = setInterval(() => this.render(), 1000);
    else if (!this.state.active && this._ticker) {
      clearInterval(this._ticker);
      this._ticker = null;
    }
    // Configuracoes abertas: reflete limites/atalho (ex.: clamp do backend).
    const modal = document.getElementById('settingsOverlay');
    if (modal && modal.classList.contains('visible')) this.loadIntoSettings();
  },

  // Segundos guardados agora: desde o ultimo push + o que ja tinha, limitado
  // ao teto de tempo (o de memoria o front nao tem como saber).
  _heldSec() {
    const ms = this.state.sinceMs + (performance.now() - this._receivedAt);
    return Math.min(Math.floor(ms / 1000), this.state.maxSec || 0);
  },

  _fmt(sec) {
    const m = Math.floor(sec / 60), s = sec % 60;
    return m + ':' + String(s).padStart(2, '0');
  },

  render() {
    const status = document.getElementById('replayStatus');
    const btn = document.getElementById('replaySaveBtn');
    const row = document.getElementById('replayRow');
    if (!status || !btn || !row) return;
    const st = this.state;
    const recording = document.body.classList.contains('recording');
    // .selected = ligado (toggle verde, igual as linhas de dispositivo);
    // .active = guardando agora (bolinha pulsando).
    row.classList.toggle('selected', !!st.enabled);
    row.classList.toggle('active', !!st.active);
    row.setAttribute('aria-checked', st.enabled ? 'true' : 'false');
    let text;
    if (st.active) {
      text = st.saving
        ? T('replay.statusSaving')
        : T('replay.statusHolding', { held: this._fmt(this._heldSec()),
                                      max: this._fmt(st.maxSec || 0) });
    } else if (st.enabled && recording) {
      text = T('replay.statusPaused');
    } else {
      text = T('replay.statusOff', { max: this._fmt(st.maxSec || this.DEFAULT_MAX_SEC) });
    }
    status.textContent = text;
    btn.hidden = !st.active;
    btn.disabled = !!st.saving;
    btn.dataset.hint = st.hotkey
      ? T('replay.saveHint', { spec: st.hotkey })
      : T('replay.saveHintNoKey');
  },

  toggle() {
    this.setEnabled(!this.state.enabled);
  },

  setEnabled(on) {
    Bridge.send('set_replay_enabled', { enabled: !!on });
  },

  save() {
    if (!this.state.active || this.state.saving) return;
    Bridge.send('save_replay');
  },

  onSaved(data) {
    if (!data) return;
    const d = +data.durationSec || 0;
    Toast.show(T('replay.saved'),
      T('replay.savedBody', { name: data.name || '', dur: this._fmt(d) }),
      { ttl: 5000 });
  },

  // ---- Configuracoes ------------------------------------------------------

  _fillSelect(id, values, current, label) {
    const sel = document.getElementById(id);
    if (!sel) return;
    const list = values.slice();
    if (current && !list.includes(current)) {
      list.push(current);
      list.sort((a, b) => a - b);
    }
    sel.innerHTML = '';
    list.forEach(v => {
      const o = document.createElement('option');
      o.value = String(v);
      o.textContent = label(v);
      sel.appendChild(o);
    });
    sel.value = String(current);
  },
  _secLabel(v) {
    return v < 60 ? T('settings.replay.seconds', { n: v })
                  : T('settings.replay.minutes', { n: Math.round(v / 60) });
  },
  _mbLabel(v) {
    // GB/MB sao iguais nos tres idiomas; so o separador decimal muda. O teto
    // da maquina raramente da GB redondo (RAM instalada menos a reserva),
    // dai uma casa decimal quando nao e inteiro.
    if (v < 1024) return v + ' MB';
    const gb = v / 1024;
    const lang = I18n.language || undefined;
    return (Number.isInteger(gb) ? gb.toLocaleString(lang)
                                 : gb.toLocaleString(lang, { maximumFractionDigits: 1 }))
           + ' GB';
  },

  // Opcoes de memoria desta maquina: os degraus abaixo do teto, mais o teto
  // como ultimo item. O teto vem do backend (RAM instalada menos a reserva
  // de 4 GB) — passar dele poria a maquina pra paginar, e no limite a libobs
  // morre sem tratar falta de memoria.
  _mbOptions() {
    const limit = this.state.maxMbLimit || this.DEFAULT_MAX_MB;
    return this.MB_OPTIONS.filter(v => v < limit).concat([limit]);
  },

  onAutoStartChange(on) {
    Bridge.send('set_replay_autostart', { autoStart: !!on });
  },

  // Programas que ligam o buffer sozinho (WinProcWatch no backend).
  onAppsChange(v) {
    clearTimeout(this._appsTimer);
    Bridge.send('set_replay_apps', { apps: v || '' });
  },
  // `change` so dispara ao sair do campo — quem digita o nome do jogo e
  // fecha as Configuracoes direto (ou aperta Esc) perderia o que digitou.
  // Daí o `input` com atraso: cada envio reinicia a thread do watcher no
  // backend, entao nao da pra mandar a cada tecla.
  _appsTimer: null,
  onAppsInput(v) {
    clearTimeout(this._appsTimer);
    this._appsTimer = setTimeout(() => this.onAppsChange(v), 600);
  },

  onIndicatorChange(on) {
    Bridge.send('set_replay_indicator', { enabled: !!on });
    this._syncIndicatorVisibility(!!on);
  },
  onIndicatorCornerChange(v) {
    Bridge.send('set_replay_indicator_corner', { corner: v });
  },
  // Slider ao vivo: o backend aplica na janela do overlay sem recriar.
  onIndicatorOpacityInput() {
    const pct = this._indicatorOpacityPct();
    this._syncIndicatorOpacityLabel();
    Bridge.send('set_replay_indicator_opacity', { opacity: pct });
  },
  _indicatorOpacityPct() {
    const s = document.getElementById('settingsReplayIndicatorOpacity');
    return s ? (parseInt(s.value, 10) || 90) : 90;
  },
  _syncIndicatorOpacityLabel() {
    const pct = this._indicatorOpacityPct();
    const v = document.getElementById('settingsReplayIndicatorOpacityValue');
    if (v) v.textContent = pct + '%';
    // Previa acompanha o slider (mesma ideia da do indicador de gravacao).
    const p = document.getElementById('replayIndicatorPreview');
    if (p) p.style.opacity = (pct / 100).toFixed(2);
  },
  // Canto/opacidade so fazem sentido com o indicador ligado.
  _syncIndicatorVisibility(on) {
    const wrap = document.getElementById('settingsReplayIndicatorWrap');
    if (wrap) wrap.style.display = on ? '' : 'none';
  },

  loadIntoSettings() {
    const auto = document.getElementById('settingsReplayAutoStart');
    if (auto) auto.checked = !!this.state.autoStart;
    const apps = document.getElementById('settingsReplayApps');
    // NAO sobrescreve enquanto o usuario digita: o push chega a qualquer
    // momento (mesma armadilha da pegadinha #56).
    if (apps && document.activeElement !== apps) apps.value = this.state.apps || '';
    const ind = document.getElementById('settingsReplayIndicator');
    if (ind) ind.checked = !!this.state.indicator;
    const corner = document.getElementById('settingsReplayIndicatorCorner');
    if (corner) corner.value = this.state.indicatorCorner || 'top-right';
    const op = document.getElementById('settingsReplayIndicatorOpacity');
    if (op) op.value = this.state.indicatorOpacity || 90;
    this._syncIndicatorOpacityLabel();
    this._syncIndicatorVisibility(!!this.state.indicator);
    this._fillSelect('settingsReplayMaxSec', this.SEC_OPTIONS,
      this.state.maxSec || this.DEFAULT_MAX_SEC, v => this._secLabel(v));
    this._fillSelect('settingsReplayMaxMb', this._mbOptions(),
      this.state.maxMb || this.DEFAULT_MAX_MB, v => this._mbOptionLabel(v));
    Settings._loadHotkeyIntoUi(this.currentHotkey, this.HOTKEY_PREFIX);
  },

  _mbOptionLabel(v) {
    const limit = this.state.maxMbLimit || this.DEFAULT_MAX_MB;
    return v >= limit ? T('settings.replay.memoryMax', { size: this._mbLabel(v) })
                      : this._mbLabel(v);
  },

  onLimitsChange() {
    const sec = parseInt(document.getElementById('settingsReplayMaxSec').value, 10) || 0;
    const mb = parseInt(document.getElementById('settingsReplayMaxMb').value, 10) || 0;
    Bridge.send('set_replay_limits', { maxSec: sec, maxMb: mb });
  },

  onHotkeyChange() {
    Settings._updateHotkeyPreview(this.HOTKEY_PREFIX);
    this._commitHotkey();
  },
  // Mesmo fluxo do atalho de gravar (Settings._commitHotkey), com a regra a
  // mais: nao pode repetir o atalho de gravar — o Windows so deixa um
  // registro por combinacao, e o segundo simplesmente nao dispararia.
  async _commitHotkey() {
    const hotkey = Settings._readHotkeyFromUi(this.HOTKEY_PREFIX);
    if (hotkey === this.currentHotkey) return;
    if (hotkey !== '') {
      if (hotkey === Settings.currentHotkey) {
        Toast.show(T('toast.invalidHotkey'), T('replay.hotkeyConflict'),
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
    Bridge.send('set_replay_hotkey', { hotkey });
    this.currentHotkey = hotkey;
  },

  // Chamado pelo "Restaurar padroes" das Configuracoes, depois dos outros.
  restoreDefaults() {
    const auto = document.getElementById('settingsReplayAutoStart');
    if (auto && auto.checked) { auto.checked = false; this.onAutoStartChange(false); }
    // Indicador volta ligado (padrao) e a lista de programas fica VAZIA: e
    // uma escolha do usuario, nao uma preferencia com padrao sensato.
    const ind = document.getElementById('settingsReplayIndicator');
    if (ind && !ind.checked) { ind.checked = true; this.onIndicatorChange(true); }
    const corner = document.getElementById('settingsReplayIndicatorCorner');
    if (corner && corner.value !== 'top-right') {
      corner.value = 'top-right'; this.onIndicatorCornerChange('top-right');
    }
    const op = document.getElementById('settingsReplayIndicatorOpacity');
    if (op && String(op.value) !== '90') { op.value = 90; this.onIndicatorOpacityInput(); }
    const apps = document.getElementById('settingsReplayApps');
    if (apps && apps.value !== '') { apps.value = ''; this.onAppsChange(''); }
    this._fillSelect('settingsReplayMaxSec', this.SEC_OPTIONS,
      this.DEFAULT_MAX_SEC, v => this._secLabel(v));
    this._fillSelect('settingsReplayMaxMb', this._mbOptions(),
      this.DEFAULT_MAX_MB, v => this._mbOptionLabel(v));
    this.onLimitsChange();
    Settings._loadHotkeyIntoUi(this.DEFAULT_HOTKEY, this.HOTKEY_PREFIX);
    this._commitHotkey();
  },
};

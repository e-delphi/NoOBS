// =====================================================================
// About modal
// =====================================================================
const About = {
  open()  { document.getElementById('aboutOverlay').classList.add('visible'); },
  close() { document.getElementById('aboutOverlay').classList.remove('visible'); },
  openUrl(url) { Bridge.send('open_url', { url }); },
};
// Fecha o About clicando fora do modal ou com Esc.
document.getElementById('aboutOverlay').addEventListener('click', (e) => {
  if (e.target.id === 'aboutOverlay') About.close();
});
document.addEventListener('keydown', (e) => {
  const ov = document.getElementById('aboutOverlay');
  if (ov.classList.contains('visible') && e.key === 'Escape') {
    e.preventDefault();
    About.close();
    return;
  }
  // F1 abre a tela "Sobre". Ignora se o foco esta num input editavel
  // (ex.: caixa de busca, campo de pasta) pra nao roubar o atalho do
  // navegador embutido (que normalmente nao faz nada aqui de qualquer
  // jeito, mas e' a higiene certa).
  if (e.key === 'F1' && !ov.classList.contains('visible')) {
    const tag = (document.activeElement && document.activeElement.tagName) || '';
    const isEditable = tag === 'INPUT' || tag === 'TEXTAREA' ||
      (document.activeElement && document.activeElement.isContentEditable);
    if (!isEditable) {
      e.preventDefault();
      About.open();
    }
  }
});

// Fecha as Configuracoes clicando no overlay (fora do modal) ou com Esc.
// Match com o About (mesma UX). Guard contra Confirm/About em cima —
// se algum modal "filho" esta aberto, ESC dele tem prioridade.
document.getElementById('settingsOverlay').addEventListener('click', (e) => {
  if (e.target.id === 'settingsOverlay') Settings.close();
});
document.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  const sov = document.getElementById('settingsOverlay');
  if (!sov.classList.contains('visible')) return;
  // Se um modal sobreposto (Confirm ou About) esta aberto, deixa ele
  // tratar o ESC — os handlers desses ja fecham primeiro.
  const cov = document.getElementById('confirmOverlay');
  const aov = document.getElementById('aboutOverlay');
  if ((cov && cov.classList.contains('visible')) ||
      (aov && aov.classList.contains('visible'))) return;
  e.preventDefault();
  Settings.close();
});

const Settings = {
  currentRecDir: '',
  currentCodec: 'auto',
  currentHotkey: '',
  currentAutostart: false,
  currentCloseToTray: false,
  currentMinimizeOnRecord: false,
  currentNotifyOnRecord: false,
  currentScrollLockIndicator: false,
  currentPlaySoundOnRecord: false,
  currentStopOnLock: false,
  currentHibernate: false,
  currentRecordingQuality: 0,
  currentRecordingFps: 30,
  maxMonitorHz: 60,
  // Predefinicoes do slider de fps — computadas a partir de maxMonitorHz.
  // Regra: 20, 30, e depois multiplos de 30 ate o max. Se max nao for
  // multiplo de 30, descarta o penultimo (o maior multiplo abaixo de max)
  // e adiciona o proprio max no fim.
  //   60Hz   -> [20, 30, 60]
  //   144Hz  -> [20, 30, 60, 90, 144]            (120 pulado)
  //   165Hz  -> [20, 30, 60, 90, 120, 165]       (150 pulado)
  //   240Hz  -> [20, 30, 60, 90, 120, 150, 180, 210, 240]
  // Slider.value = indice nesse array (0..N-1, step=1).
  fpsPresets: [20, 30, 60],
  currentLanguage: '',        // codigo ativo resolvido ('pt-BR', 'en'...)
  currentLanguagePref: '',    // pref salvo ('', 'auto', ou codigo fixo)
  availableLanguages: [],     // [{code, name, nativeName}, ...]
  open() {
    this._populateLanguageOptions();
    document.getElementById('settingsLanguage').value =
      this.currentLanguagePref || '';
    document.getElementById('settingsRecDir').value = this.currentRecDir || '';
    document.getElementById('settingsCodec').value = this.currentCodec || 'auto';
    this._loadHotkeyIntoUi(this.currentHotkey || '');
    document.getElementById('settingsAutostart').checked = !!this.currentAutostart;
    document.getElementById('settingsCloseToTray').checked = !!this.currentCloseToTray;
    document.getElementById('settingsMinimizeOnRecord').checked = !!this.currentMinimizeOnRecord;
    document.getElementById('settingsNotifyOnRecord').checked = !!this.currentNotifyOnRecord;
    document.getElementById('settingsScrollLockIndicator').checked = !!this.currentScrollLockIndicator;
    document.getElementById('settingsPlaySoundOnRecord').checked = !!this.currentPlaySoundOnRecord;
    document.getElementById('settingsStopOnLock').checked = !!this.currentStopOnLock;
    document.getElementById('settingsHibernate').checked = !!this.currentHibernate;
    document.getElementById('settingsRecordingQuality').value = String(this.currentRecordingQuality | 0);
    this._syncQualityHint();
    // Computa presets baseados no maxMonitorHz atual + aplica no slider
    // (indice + labels + ticks). Idempotente.
    this.fpsPresets = this._computeFpsPresets();
    this._applyFpsPresetsToSlider();
    this._populateFpsTicks();
    this._syncFpsHint();
    this._syncMinimizeOnRecordLabel();
    this._syncNotifyOnRecordEnabled();
    this._syncHibernateEnabled();
    this._syncCodecMaxRes();
    this._updateThemeButtons();
    document.getElementById('settingsOverlay').classList.add('visible');
    // Reseta o scroll pro topo a cada abertura — senao reabrir mantem a
    // posicao da sessao anterior (ficava no fim se o user havia rolado ate
    // la). O overlay usa opacity/visibility (nao display:none), entao o
    // .settings-body sempre esta no layout e scrollTop=0 aplica na hora.
    const body = document.querySelector('#settingsOverlay .settings-body');
    if (body) body.scrollTop = 0;
    Bridge.send('get_settings');
  },
  close() {
    document.getElementById('settingsOverlay').classList.remove('visible');
  },
  pickFolder() {
    Bridge.send('pick_record_dir');
  },
  // Aplica o tema imediato (preview) e persiste no backend. Sem botao
  // Salvar — tema sempre commita na hora pra dar feedback instantaneo.
  setTheme(theme) {
    if (theme !== 'dark' && theme !== 'light') return;
    document.documentElement.setAttribute('data-theme', theme);
    notifyTitlebarTheme(theme);
    Bridge.send('set_theme', { theme });
    this._updateThemeButtons();
  },
  _updateThemeButtons() {
    const current = document.documentElement.getAttribute('data-theme') || 'dark';
    document.querySelectorAll('.theme-toggle-btn').forEach(btn => {
      btn.classList.toggle('active', btn.dataset.theme === current);
    });
  },
  async save() {
    try {
      const language = document.getElementById('settingsLanguage').value || '';
      const path = document.getElementById('settingsRecDir').value.trim();
      const codec = document.getElementById('settingsCodec').value;
      const hotkey = this._readHotkeyFromUi();
      const autostart = document.getElementById('settingsAutostart').checked;
      const closeToTray = document.getElementById('settingsCloseToTray').checked;
      const minimizeOnRecord = document.getElementById('settingsMinimizeOnRecord').checked;
      const notifyOnRecord = document.getElementById('settingsNotifyOnRecord').checked;
      const scrollLockIndicator = document.getElementById('settingsScrollLockIndicator').checked;
      const playSoundOnRecord = document.getElementById('settingsPlaySoundOnRecord').checked;
      const stopOnLock = document.getElementById('settingsStopOnLock').checked;
      const hibernate = document.getElementById('settingsHibernate').checked;
      const recordingQuality = parseInt(document.getElementById('settingsRecordingQuality').value, 10) | 0;
      // Slider snap nas predefinicoes — _currentFpsFromSlider mapeia
      // indice -> fps direto da lista, garantindo valor valido [20..max].
      const recordingFps = this._currentFpsFromSlider();
      console.log('[Settings.save]', { path, codec, hotkey, autostart, closeToTray,
        minimizeOnRecord, notifyOnRecord, scrollLockIndicator, hibernate,
        recordingQuality, recordingFps,
        prev: { recDir: this.currentRecDir, codec: this.currentCodec, hotkey: this.currentHotkey,
                autostart: this.currentAutostart, closeToTray: this.currentCloseToTray,
                minimizeOnRecord: this.currentMinimizeOnRecord,
                notifyOnRecord: this.currentNotifyOnRecord,
                scrollLockIndicator: this.currentScrollLockIndicator,
                hibernate: this.currentHibernate,
                recordingQuality: this.currentRecordingQuality,
                recordingFps: this.currentRecordingFps } });

      // Valida hotkey via backend (regra centralizada em OBSHotkey/OBSBridge).
      // Cobre tanto spec invalido (so modificadores) quanto combinacoes
      // reservadas pelo Windows. Se rejeitado, mostra Toast e mantem o
      // modal aberto pro user corrigir.
      if (hotkey !== '') {
        const validation = await validateHotkeyWithBackend(hotkey);
        if (!validation.ok) {
          Toast.show(T('toast.invalidHotkey'), validation.reason +
            ' ' + T('settings.hotkey.chooseAnother'), { warn: true, ttl: 7000 });
          return;
        }
      }

      // path vazio = restaurar pro default (USERPROFILE\Videos). So
      // envia se mudou — evita rebuild desnecessario da lista de gravacoes.
      if (path !== this.currentRecDir)
        Bridge.send('set_record_dir', { path });
      if (codec) Bridge.send('set_codec', { codec });
      // Hotkey: salva sempre (string vazia = desativa atalho).
      if (hotkey !== this.currentHotkey)
        Bridge.send('set_hotkey', { hotkey });
      if (autostart !== this.currentAutostart)
        Bridge.send('set_autostart', { enabled: autostart });
      if (closeToTray !== this.currentCloseToTray)
        Bridge.send('set_close_to_tray', { enabled: closeToTray });
      if (minimizeOnRecord !== this.currentMinimizeOnRecord)
        Bridge.send('set_minimize_on_record', { enabled: minimizeOnRecord });
      if (notifyOnRecord !== this.currentNotifyOnRecord)
        Bridge.send('set_notify_on_record', { enabled: notifyOnRecord });
      if (scrollLockIndicator !== this.currentScrollLockIndicator)
        Bridge.send('set_scroll_lock_indicator', { enabled: scrollLockIndicator });
      if (playSoundOnRecord !== this.currentPlaySoundOnRecord)
        Bridge.send('set_play_sound_on_record', { enabled: playSoundOnRecord });
      if (stopOnLock !== this.currentStopOnLock)
        Bridge.send('set_stop_on_lock', { enabled: stopOnLock });
      if (hibernate !== this.currentHibernate)
        Bridge.send('set_hibernate', { enabled: hibernate });
      if (recordingQuality !== this.currentRecordingQuality)
        Bridge.send('set_recording_quality', { level: recordingQuality });
      if (recordingFps !== this.currentRecordingFps)
        Bridge.send('set_recording_fps', { fps: recordingFps });
      if (language !== this.currentLanguagePref)
        Bridge.send('set_language', { language });
      // Atualiza cache local imediato pra evitar reenvio de uma mesma
      // mudanca na proxima save (e evitar flash de valor antigo num
      // re-open antes da resposta do get_settings).
      this.currentRecDir = path || this.currentRecDir;
      if (codec) this.currentCodec = codec;
      this.currentHotkey = hotkey;
      this.currentAutostart = autostart;
      this.currentCloseToTray = closeToTray;
      this.currentMinimizeOnRecord = minimizeOnRecord;
      this.currentNotifyOnRecord = notifyOnRecord;
      this.currentScrollLockIndicator = scrollLockIndicator;
      this.currentPlaySoundOnRecord = playSoundOnRecord;
      this.currentStopOnLock = stopOnLock;
      this.currentHibernate = hibernate;
      this.currentRecordingQuality = recordingQuality;
      this.currentRecordingFps = recordingFps;
      this.currentLanguagePref = language;
      // Atualiza o icone de aviso na barra lateral caso o codec novo
      // tenha um limite diferente do anterior (ex.: h264-hw → av1-hw
      // remove o warning de canvas grande).
      if (typeof Displays !== 'undefined' && Displays._updateCount)
        Displays._updateCount();
      this.close();
      Toast.show(T('toast.saved'), '', { ttl: 2000 });
    } catch (err) {
      console.error('[Settings.save] erro:', err);
      Toast.show(T('toast.errorSaving'), String(err && err.message || err),
        { warn: true, ttl: 6000 });
    }
  },
  applySettings(data) {
    this.currentRecDir = data.recordDir || '';
    const prevCodec = this.currentCodec;
    this.currentCodec = data.codec || 'auto';
    // Sincroniza o icone de aviso se o codec efetivo mudou no boot
    // ou apos um get_settings com config diferente.
    if (prevCodec !== this.currentCodec &&
        typeof Displays !== 'undefined' && Displays._updateCount)
      Displays._updateCount();
    this.currentHotkey = data.hotkey || '';
    this.currentAutostart = !!data.autostart;
    this.currentCloseToTray = !!data.closeToTray;
    this.currentMinimizeOnRecord = !!data.minimizeOnRecord;
    this.currentNotifyOnRecord = !!data.notifyOnRecord;
    this.currentScrollLockIndicator = !!data.scrollLockIndicator;
    // playSoundOnRecord: default false — feature opt-in. !!data.* cobre
    // tanto o caso "undefined" (configa nova/limpa) quanto false explicito.
    this.currentPlaySoundOnRecord = !!data.playSoundOnRecord;
    // stopOnLock: default false — opt-in. Quando ON, Windows lock event
    // (Win+L etc.) chama HandleRecordStop no backend.
    this.currentStopOnLock = !!data.stopOnLock;
    // hibernate: default false — so faz sentido com closeToTray ON, e
    // gateamos a UI pra forcar isso. !!data.hibernate cobre o caso
    // undefined naturalmente (resulta em false).
    this.currentHibernate = !!data.hibernate;
    // recordingQuality: -2..+2, default 0
    let rq = parseInt(data.recordingQuality, 10);
    if (!Number.isFinite(rq)) rq = 0;
    if (rq < -2) rq = -2;
    if (rq > 2)  rq = 2;
    this.currentRecordingQuality = rq;
    // recordingFps: >= 10, default 30 (padrao do NoOBS).
    let fps = parseInt(data.recordingFps, 10);
    if (!Number.isFinite(fps) || fps < 10) fps = 30;
    this.currentRecordingFps = fps;
    // maxMonitorHz: taxa maxima detectada no backend (Win32 EnumDisplaySettings).
    let maxHz = parseInt(data.maxMonitorHz, 10);
    if (!Number.isFinite(maxHz) || maxHz < 10) maxHz = 60;
    this.maxMonitorHz = maxHz;
    // i18n: idioma ativo + pref salvo (vazio/'auto' = automatico). Lista
    // de idiomas disponiveis vem do backend (enumerou pasta lang\).
    this.currentLanguage = data.language || '';
    this.currentLanguagePref = data.languagePref || '';
    if (Array.isArray(data.availableLanguages))
      this.availableLanguages = data.availableLanguages;
    // Repopula dropdown — preserva selecao se o modal estiver aberto.
    this._populateLanguageOptions();
    const langEl = document.getElementById('settingsLanguage');
    if (langEl) langEl.value = this.currentLanguagePref || '';
    const inp = document.getElementById('settingsRecDir');
    if (inp) inp.value = this.currentRecDir;
    const sel = document.getElementById('settingsCodec');
    if (sel) sel.value = this.currentCodec;
    this._loadHotkeyIntoUi(this.currentHotkey);
    const as = document.getElementById('settingsAutostart');
    if (as) as.checked = this.currentAutostart;
    const ct = document.getElementById('settingsCloseToTray');
    if (ct) ct.checked = this.currentCloseToTray;
    const mr = document.getElementById('settingsMinimizeOnRecord');
    if (mr) mr.checked = this.currentMinimizeOnRecord;
    const nr = document.getElementById('settingsNotifyOnRecord');
    if (nr) nr.checked = this.currentNotifyOnRecord;
    const sl = document.getElementById('settingsScrollLockIndicator');
    if (sl) sl.checked = this.currentScrollLockIndicator;
    const ps = document.getElementById('settingsPlaySoundOnRecord');
    if (ps) ps.checked = this.currentPlaySoundOnRecord;
    const sol = document.getElementById('settingsStopOnLock');
    if (sol) sol.checked = this.currentStopOnLock;
    const hb = document.getElementById('settingsHibernate');
    if (hb) hb.checked = this.currentHibernate;
    const rqEl = document.getElementById('settingsRecordingQuality');
    if (rqEl) rqEl.value = String(this.currentRecordingQuality);
    this._syncQualityHint();
    // Recomputa presets pra refletir o maxMonitorHz que acabou de chegar.
    this.fpsPresets = this._computeFpsPresets();
    this._applyFpsPresetsToSlider();
    this._populateFpsTicks();
    this._syncFpsHint();
    this._syncMinimizeOnRecordLabel();
    this._syncNotifyOnRecordEnabled();
    this._syncHibernateEnabled();
    this._syncCodecMaxRes();
  },
  setPickedPath(path) {
    document.getElementById('settingsRecDir').value = path;
  },
  // Cascata de dependencias:
  //   closeToTray (master tray) → muda label do minimizeOnRecord
  //                              (vai pra bandeja vs minimiza taskbar)
  //                            → habilita/desabilita hibernate
  //                              (hibernar so faz sentido com bandeja ON)
  //   minimizeOnRecord → habilita/desabilita notifyOnRecord
  //                      (notify so faz sentido se app fica escondido)
  onTrayChange() {
    this._syncMinimizeOnRecordLabel();
    this._syncHibernateEnabled();
  },
  onMinimizeOnRecordChange() {
    this._syncNotifyOnRecordEnabled();
  },
  onQualityChange() {
    this._syncQualityHint();
  },
  onFpsChange() {
    this._syncFpsHint();
  },
  onLanguageChange() {
    // Mudanca local — so commita no backend quando o user clica Salvar.
    // Aqui apenas guardamos a selecao pra que outros _sync* reflitam.
    // (O backend escolhe 'auto' quando o valor enviado e '' ou 'auto'.)
  },
  // Reconstroi as <option> do dropdown de idioma a partir de
  // availableLanguages, preservando a opcao "Automatico" como primeira.
  // Idempotente: pode ser chamada toda vez que o modal abre ou que
  // applySettings traz uma lista nova.
  _populateLanguageOptions() {
    const sel = document.getElementById('settingsLanguage');
    if (!sel) return;
    // Preserva opcao "auto" (com data-i18n) — limpa as demais.
    const autoOpt = sel.querySelector('option[value=""]');
    sel.innerHTML = '';
    if (autoOpt) sel.appendChild(autoOpt);
    const list = Array.isArray(this.availableLanguages) ? this.availableLanguages : [];
    list.forEach(l => {
      if (!l || !l.code) return;
      const opt = document.createElement('option');
      opt.value = l.code;
      // nativeName fica mais natural pro user (ex.: "Português (Brasil)")
      // que o name em ingles ("Portuguese (Brazil)"). Fallback pro code.
      opt.textContent = l.nativeName || l.name || l.code;
      sel.appendChild(opt);
    });
  },
  onCodecChange() {
    this._syncCodecMaxRes();
  },
  // Espelha OBSEncoder.GetEncoderMaxDimension (Delphi) — qualquer
  // mudanca de logica de limite tem que ser refletida aqui pra UI
  // mostrar a mesma coisa que o backend vai aplicar.
  //   h264-hw  → 4096 — universal em NVIDIA NVENC, AMD AMF, Intel QSV
  //              (limite do encoder hw, nao do hardware em si)
  //   h264-sw  → 8192 (x264 nao tem limite pratico)
  //   hevc-hw  → 8192
  //   av1-hw   → 8192
  //   auto     → 4096 se caps tem h264-hw (vai bater nele primeiro
  //              no fallback chain), senao 8192
  _syncCodecMaxRes() {
    const codec = document.getElementById('settingsCodec').value;
    const el = document.getElementById('settingsCodecMaxRes');
    if (!el) return;
    const caps = this.encoderCaps || {};
    let dim;
    if (codec === 'h264-hw') dim = 4096;
    else if (codec === 'h264-sw' || codec === 'hevc-hw' || codec === 'av1-hw') dim = 8192;
    else /* auto */ dim = caps.h264Hw ? 4096 : 8192;
    el.textContent = T('settings.codec.maxRes', { w: dim, h: dim });
  },
  _syncQualityHint() {
    const el = document.getElementById('settingsRecordingQuality');
    const hint = document.getElementById('settingsQualityHint');
    if (!el || !hint) return;
    const v = parseInt(el.value, 10) | 0;
    // Chave estilo 'settings.quality.hint.-2' .. '.2' — match com o JSON.
    if (v >= -2 && v <= 2) hint.textContent = T('settings.quality.hint.' + v);
    else hint.textContent = '';
    this._syncSliderFill(el);
  },
  // Atualiza a CSS var --val (0..1) usada pelo linear-gradient da track
  // pra colorir a parte preenchida em verde. Chamado pelos _sync*Hint
  // que ja sao invocados em todo lugar que o slider muda — drag, open,
  // applySettings, restoreDefaults.
  _syncSliderFill(slider) {
    if (!slider) return;
    const min = parseFloat(slider.min) || 0;
    const max = parseFloat(slider.max) || 100;
    const val = parseFloat(slider.value) || 0;
    const ratio = (max > min) ?
      Math.max(0, Math.min(1, (val - min) / (max - min))) : 0;
    slider.style.setProperty('--val', String(ratio));
  },
  _syncFpsHint() {
    const hint = document.getElementById('settingsFpsHint');
    if (!hint) return;
    // Le o fps via helper (slider.value -> indice -> fps). Bucketiza
    // pela mesma faixa de antes — agora os buckets coincidem com os
    // presets (20→low, 30→smooth, 60→good, 90/120→high, 150+→veryHigh).
    const v = this._currentFpsFromSlider();
    let bucket;
    if (v <= 24)       bucket = 'low';
    else if (v <= 30)  bucket = 'smooth';
    else if (v <= 60)  bucket = 'good';
    else if (v <= 120) bucket = 'high';
    else               bucket = 'veryHigh';
    hint.textContent = T('settings.fps.hint.' + bucket, { fps: v });
    this._syncSliderFill(document.getElementById('settingsRecordingFps'));
  },
  // Calcula as predefinicoes validas dado o maxMonitorHz atual. Ver
  // comentario da propriedade fpsPresets pra regra completa.
  _computeFpsPresets() {
    const max = Math.max(20, this.maxMonitorHz || 60);
    const list = [20, 30];
    for (let v = 60; v <= max; v += 30) list.push(v);
    // Dedup + filtro pelo range (caso max < 30 ou == 20).
    let presets = [...new Set(list)].filter(v => v <= max);
    if (presets.length === 0) return [max];
    // Se max nao bateu num multiplo de 30, pula o ultimo da lista
    // (penultimo em relacao a onde max vai entrar) e injeta max.
    // Mantem >= 2 elementos pra nao colapsar quando max e baixo.
    if (presets[presets.length - 1] !== max) {
      if (presets.length >= 3) presets.pop();
      presets.push(max);
    }
    return presets;
  },
  // Acha o indice do preset mais proximo de um valor de fps livre.
  // Usado pra mapear config (qualquer inteiro >= 10) -> posicao do slider.
  _fpsToIndex(fps) {
    if (!Array.isArray(this.fpsPresets) || this.fpsPresets.length === 0) return 0;
    let best = 0;
    let bestDiff = Infinity;
    this.fpsPresets.forEach((p, i) => {
      const d = Math.abs(p - fps);
      if (d < bestDiff) { best = i; bestDiff = d; }
    });
    return best;
  },
  // Le o valor atual do slider e retorna o fps correspondente (vira
  // direto da lista de presets — slider.value E o indice).
  _currentFpsFromSlider() {
    const el = document.getElementById('settingsRecordingFps');
    if (!el || !Array.isArray(this.fpsPresets) || this.fpsPresets.length === 0)
      return this.currentRecordingFps || 30;
    const idx = Math.max(0, Math.min(this.fpsPresets.length - 1,
      parseInt(el.value, 10) | 0));
    return this.fpsPresets[idx];
  },
  // Configura o slider (min/max/step/value) + labels laterais a partir
  // de fpsPresets. Centralizado pra reusar em open()/applySettings()/
  // restoreDefaults() sem duplicar logica.
  _applyFpsPresetsToSlider() {
    const slider = document.getElementById('settingsRecordingFps');
    if (slider) {
      slider.min = 0;
      slider.max = String(Math.max(0, this.fpsPresets.length - 1));
      slider.step = 1;
      slider.value = String(this._fpsToIndex(this.currentRecordingFps));
    }
    // Os rotulos das pontas sao estaticos e localizados ("Menos fluido" /
    // "Mais fluido", via data-i18n no HTML) — mais claros pro usuario leigo
    // que os antigos "20 fps" / "max fps". Os numeros exatos continuam
    // visiveis nos ticks abaixo do slider e no hint. Por isso NAO setamos
    // textContent aqui (sobrescreveria a traducao do I18n.apply).
  },
  // Tick por preset, espacados igualmente (cada step do slider = 1 tick).
  // Container .fps-ticks vive dentro de .slider-with-ticks, que tem a
  // mesma largura do input (width: 100%). Entao "100%" no calc abaixo
  // ja e a largura util do slider — basta compensar o raio do thumb
  // (7px = metade da width: 14px do ::-webkit-slider-thumb):
  //   left = 7px + ratio * (100% - 14px)
  // Independente de label-width, gap, ou outros offsets externos do row.
  _populateFpsTicks() {
    const el = document.getElementById('fpsTicks');
    if (!el) return;
    const presets = Array.isArray(this.fpsPresets) ? this.fpsPresets : [];
    if (presets.length < 2) { el.innerHTML = ''; return; }
    el.innerHTML = presets.map((v, i) => {
      const ratio = i / (presets.length - 1);
      const left = `calc(7px + ${ratio} * (100% - 14px))`;
      // Sem at-start/at-end — todos os ticks centralizados via translateX(-50%)
      // na CSS, que e o jeito certo de alinhar com o thumb (centro a centro,
      // nao borda a borda).
      return `<span class="fps-tick" style="left: ${left}">${v}</span>`;
    }).join('');
  },

  _syncMinimizeOnRecordLabel() {
    const tray = document.getElementById('settingsCloseToTray').checked;
    const lbl = document.getElementById('settingsMinimizeOnRecordLabel');
    const hint = document.getElementById('settingsMinimizeOnRecordHint');
    if (lbl) lbl.textContent = T(tray
      ? 'settings.minimizeOnRecord.labelTray'
      : 'settings.minimizeOnRecord.labelTaskbar');
    if (hint) hint.textContent = T(tray
      ? 'settings.minimizeOnRecord.hintTray'
      : 'settings.minimizeOnRecord.hintTaskbar');
  },
  _syncNotifyOnRecordEnabled() {
    const minOnRec = document.getElementById('settingsMinimizeOnRecord').checked;
    const notif = document.getElementById('settingsNotifyOnRecord');
    const row = document.getElementById('settingsNotifyOnRecordRow');
    if (!notif || !row) return;
    notif.disabled = !minOnRec;
    row.classList.toggle('disabled', !minOnRec);
    // Forca uncheck quando desabilitado pra evitar estado "ON mas oculto".
    if (!minOnRec) notif.checked = false;
  },
  // Hibernate so faz sentido com closeToTray ON — sem bandeja, fechar
  // a janela ja encerra o app e nao tem cenario pra hibernar. Mesma
  // logica do notifyOnRecord (uncheck forcado pra evitar estado oculto).
  _syncHibernateEnabled() {
    const tray = document.getElementById('settingsCloseToTray').checked;
    const hib = document.getElementById('settingsHibernate');
    const row = document.getElementById('settingsHibernateRow');
    if (!hib || !row) return;
    hib.disabled = !tray;
    row.classList.toggle('disabled', !tray);
    if (!tray) hib.checked = false;
  },
  restoreDefaults() {
    // Reseta APENAS os campos do modal (UI) — nao salva nada. User
    // revisa e clica Salvar pra confirmar, ou Cancelar pra descartar.
    // Pede confirmacao antes pra evitar reset acidental.
    // Tema fica de fora — ja eh aplicado/salvo instantaneo via os
    // botoes de toggle no header, nao faz parte do fluxo de Salvar.
    Confirm.open({
      title: T('settings.buttons.reset'),
      message: T('toast.restoreConfirmMessage'),
      okLabel: T('toast.restoreConfirmOk'),
      cancelLabel: T('common.cancel'),
      danger: false,
      onOk: () => {
        // Defaults (espelhados do backend OBSConfig.GetConfigBool/Str
        // e do OBSBridge HandleSet*).
        //
        // recordingFps default = 30 (padrao do NoOBS — mais compacto
        // que o 60fps do OBS Studio, suficiente pra screencast). User
        // pode subir manualmente ate o Hz do monitor mais rapido.
        document.getElementById('settingsLanguage').value = '';
        document.getElementById('settingsRecDir').value = '';
        document.getElementById('settingsCodec').value = 'auto';
        this._loadHotkeyIntoUi('Pause/Break');
        document.getElementById('settingsAutostart').checked = false;
        document.getElementById('settingsCloseToTray').checked = false;
        document.getElementById('settingsMinimizeOnRecord').checked = false;
        document.getElementById('settingsNotifyOnRecord').checked = false;
        document.getElementById('settingsScrollLockIndicator').checked = false;
        document.getElementById('settingsPlaySoundOnRecord').checked = false;
        document.getElementById('settingsStopOnLock').checked = false;
        document.getElementById('settingsHibernate').checked = false;
        document.getElementById('settingsRecordingQuality').value = '0';
        // FPS: snap pra 30 (preset garantido a existir se max >= 30).
        // Atualiza currentRecordingFps ANTES de _applyFpsPresetsToSlider
        // pra que o slider sente nessa posicao.
        this.currentRecordingFps = 30;
        this.fpsPresets = this._computeFpsPresets();
        this._applyFpsPresetsToSlider();
        this._populateFpsTicks();
        this._syncMinimizeOnRecordLabel();
        this._syncNotifyOnRecordEnabled();
        this._syncHibernateEnabled();
        this._syncQualityHint();
        this._syncFpsHint();
        this._syncCodecMaxRes();
        Toast.show(T('toast.fieldsReset'),
          T('toast.fieldsResetBody'), { ttl: 4000 });
      },
    });
  },

  // -------- Hotkey via checkboxes (modificadores) + dropdown (tecla) --------
  // 4 checkboxes (Ctrl/Shift/Alt/Win) controlam os modificadores.
  // 1 dropdown lista as teclas principais (F1-F12, letras, numeros, etc).
  // Combinacao final: [mods marcados na ordem canonica] + tecla principal.

  onHotkeyChange() {
    this._updateHotkeyPreview();
  },

  _makeOpt(val, label) {
    const o = document.createElement('option');
    o.value = val;
    o.textContent = label;
    return o;
  },

  // Constroi as opcoes do dropdown de tecla principal. Roda 1x quando
  // o modal abre (ou no _loadHotkeyIntoUi inicial).
  _buildHotkeyKeyDropdown() {
    const sel = document.getElementById('settingsHotkeyKey');
    if (!sel) return;
    // Preserva selecao atual se ja foi construido antes (re-abrir modal).
    const prevValue = sel.value;
    sel.innerHTML = '';
    sel.appendChild(this._makeOpt('', T('settings.hotkey.notDefined')));
    HOTKEY_KEY_GROUPS.forEach(group => {
      const og = document.createElement('optgroup');
      og.label = T(group.labelKey);
      // Item pode ser string (value == label) ou objeto { k, labelKey }
      // (value = k canonico, label traduzido). Ver HOTKEY_KEY_GROUPS.
      group.keys.forEach(key => {
        if (typeof key === 'string') { og.appendChild(this._makeOpt(key, key)); return; }
        // objeto: { k, labelKey } (rotulo traduzido via T) ou { k, label }
        // (rotulo literal, ex. "Num +", igual em todos os idiomas).
        const text = key.labelKey ? T(key.labelKey) : (key.label || key.k);
        og.appendChild(this._makeOpt(key.k, text));
      });
      sel.appendChild(og);
    });
    if (prevValue) sel.value = prevValue;
  },

  // Le checkboxes + dropdown e monta a spec final no formato canonico
  // do backend: Ctrl+Shift+Alt+Win+Tecla.
  _readHotkeyFromUi() {
    const parts = [];
    if (document.getElementById('settingsHotkeyCtrl').checked)  parts.push('Ctrl');
    if (document.getElementById('settingsHotkeyShift').checked) parts.push('Shift');
    if (document.getElementById('settingsHotkeyAlt').checked)   parts.push('Alt');
    if (document.getElementById('settingsHotkeyWin').checked)   parts.push('Win');
    const key = document.getElementById('settingsHotkeyKey').value;
    if (key) parts.push(key);
    return parts.join('+');
  },

  // Carrega uma spec ("Ctrl+Shift+F9" / "Pause" / "") nos controles.
  _loadHotkeyIntoUi(spec) {
    const parts = (spec || '').split('+').map(s => s.trim()).filter(Boolean);
    document.getElementById('settingsHotkeyCtrl').checked  = parts.includes('Ctrl');
    document.getElementById('settingsHotkeyShift').checked = parts.includes('Shift');
    document.getElementById('settingsHotkeyAlt').checked   = parts.includes('Alt');
    document.getElementById('settingsHotkeyWin').checked   = parts.includes('Win');
    const main = parts.find(p => !isHotkeyModifier(p)) || '';
    this._buildHotkeyKeyDropdown();
    document.getElementById('settingsHotkeyKey').value = main;
    this._updateHotkeyPreview();
  },

  _updateHotkeyPreview() {
    const spec = this._readHotkeyFromUi();
    const el = document.getElementById('settingsHotkeyPreview');
    if (!el) return;
    // Bundle traz HTML simples nesses 2 hints (code/b). T() faz interpolacao
    // {{spec}} pro atalho atual; o noKey nao tem var.
    if (spec)
      el.innerHTML = T('settings.hotkey.activeHint', { spec });
    else
      el.innerHTML = T('settings.hotkey.noKey');
  },
  // caps = { av1Hw, hevcHw, h264Hw, h264Sw, vendor, vendorLogo }
  encoderCaps: null,
  applyEncoderCaps(caps) {
    this.encoderCaps = caps || {};
    // Caps mudou — 'auto' pode passar de 4096 (com h264-hw) pra 8192
    // (sem) ou vice-versa, entao re-sincroniza o texto de max res.
    this._syncCodecMaxRes();
    // O icone de aviso ao lado da resolucao agregada das telas
    // depende de caps (pro modo 'auto'). Re-renderiza o meta.
    if (typeof Displays !== 'undefined' && Displays._updateCount)
      Displays._updateCount();
    const sel = document.getElementById('settingsCodec');
    if (!sel) return;
    // Habilita/desabilita cada opcao baseado nas caps detectadas.
    Array.from(sel.options).forEach(opt => {
      let avail = true;
      if (opt.value === 'av1-hw') avail = !!caps.av1Hw;
      else if (opt.value === 'hevc-hw') avail = !!caps.hevcHw;
      else if (opt.value === 'h264-hw') avail = !!caps.h264Hw;
      else if (opt.value === 'h264-sw') avail = !!caps.h264Sw;
      opt.disabled = !avail;
      // Label dinamico mostrando vendor pra hw options.
      if (opt.value === 'av1-hw' || opt.value === 'hevc-hw' || opt.value === 'h264-hw') {
        // AV1 / HEVC / H.264 sao nomes de padrao (nao traduzem). O sufixo
        // " — hardware", o vendor e "(indisponível)" vem do i18n.
        const base = opt.value === 'av1-hw' ? 'AV1' :
                     opt.value === 'hevc-hw' ? 'HEVC / H.265' : 'H.264';
        const tag = caps.vendor === 'nvidia' ? T('settings.codec.vendorNvidia') :
                    caps.vendor === 'amd'    ? T('settings.codec.vendorAmd') :
                    caps.vendor === 'intel'  ? T('settings.codec.vendorIntel') : '';
        opt.textContent = base + T('settings.codec.hwSuffix') +
          (avail ? tag : ' ' + T('settings.codec.unavailable'));
      } else if (opt.value === 'h264-sw') {
        opt.textContent = T('settings.codec.h264Sw') +
          (avail ? '' : ' ' + T('settings.codec.unavailable'));
      }
    });
    // Logo do GPU ao lado do label.
    const logo = document.getElementById('settingsGpuLogo');
    if (logo) {
      if (caps.vendorLogo) {
        logo.src = caps.vendorLogo;
        logo.dataset.show = '1';
      } else {
        logo.removeAttribute('src');
        delete logo.dataset.show;
      }
    }
  }
};


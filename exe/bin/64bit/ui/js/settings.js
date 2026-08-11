// =====================================================================
// About — deixou de ser modal proprio: virou a aba "Sobre" das
// Configuracoes. `open()` sobrou como ponto de entrada unico (F1 e
// qualquer chamada externa) pra abrir as Configuracoes ja nessa aba.
// =====================================================================
const About = {
  open() {
    Settings.open();
    Settings.showTab('about');
  },
  openUrl(url) { Bridge.send('open_url', { url }); },
};

// =====================================================================
// Updates — aba "Atualizações". A checagem em si e o Delphi que faz
// (OBSUpdate, worker thread); aqui so disparamos e exibimos o resultado.
// =====================================================================
const Updates = {
  _lastUrl: '',

  checkNow() {
    const st = document.getElementById('settingsUpdateStatus');
    const bt = document.getElementById('settingsCheckNowBtn');
    if (st) { st.textContent = T('settings.updates.checking'); st.className = 'settings-hint'; }
    // Desabilita ate a resposta: a checagem e assincrona e cliques
    // repetidos so enfileirariam requisicoes (o backend ja ignora
    // concorrentes, mas o feedback tem que ser visivel).
    if (bt) bt.disabled = true;
    Bridge.send('check_updates', {});
  },

  // Banner verde na tela inicial. É o único lugar que o usuário comum vê
  // sem ir procurar — a aba "Atualizações" só é aberta por quem já
  // desconfia que existe versão nova.
  //
  // Dispensa vale só para a sessão: não persiste em config de propósito.
  // Gravar "já dispensou a v0.21" faria quem clicou no X uma vez nunca
  // mais ser avisado daquela versão, e o custo de reaparecer no próximo
  // arranque é baixo (um X).
  showBanner(data) {
    const el = document.getElementById('updateBanner');
    const body = document.getElementById('updateBannerBody');
    if (!el || !body) return;
    body.textContent = T('banner.updateBody', { version: data.latest || '' });
    if (data.url) {
      body.appendChild(document.createTextNode(' '));
      const a = document.createElement('span');
      a.className = 'about-link';
      a.setAttribute('role', 'link');
      a.tabIndex = 0;
      a.textContent = T('settings.updates.openPage');
      a.onclick = () => About.openUrl(data.url);
      body.appendChild(a);
    }
    el.classList.add('show');
    const close = document.getElementById('updateBannerClose');
    if (close) close.onclick = () => el.classList.remove('show');
  },

  applyResult(data) {
    // O banner vem ANTES de qualquer guarda de elemento da tela de
    // Configuracoes: ele tem que aparecer mesmo que aquela tela nao
    // esteja montada, que e justamente o caso normal.
    this._lastUrl = data.url || '';
    if (data.ok && data.hasUpdate) this.showBanner(data);

    const st = document.getElementById('settingsUpdateStatus');
    const bt = document.getElementById('settingsCheckNowBtn');
    if (bt) bt.disabled = false;
    if (!st) return;

    if (!data.ok) {
      // Falha e silenciosa por design — so informa quem pediu explicitamente.
      st.className = 'settings-hint';
      st.textContent = T('settings.updates.failed');
      return;
    }
    if (data.hasUpdate) {
      st.className = 'settings-hint update-available';
      st.textContent = T('settings.updates.available', { version: data.latest });
      if (this._lastUrl) {
        st.appendChild(document.createTextNode(' '));
        const a = document.createElement('span');
        a.className = 'about-link';
        a.setAttribute('role', 'link');
        a.tabIndex = 0;
        a.textContent = T('settings.updates.openPage');
        a.onclick = () => About.openUrl(Updates._lastUrl);
        st.appendChild(a);
      }
    } else {
      st.className = 'settings-hint';
      st.textContent = T('settings.updates.upToDate');
    }
  },
};
document.addEventListener('keydown', (e) => {
  // F1 abre a tela "Sobre". Ignora se o foco esta num input editavel
  // (ex.: caixa de busca, campo de pasta) pra nao roubar o atalho do
  // navegador embutido (que normalmente nao faz nada aqui de qualquer
  // jeito, mas e' a higiene certa).
  if (e.key !== 'F1') return;
  const tag = (document.activeElement && document.activeElement.tagName) || '';
  const isEditable = tag === 'INPUT' || tag === 'TEXTAREA' ||
    (document.activeElement && document.activeElement.isContentEditable);
  if (isEditable) return;
  e.preventDefault();
  About.open();
});

// Fecha as Configuracoes clicando no overlay (fora do modal) ou com Esc.
// Match com o About (mesma UX). Guard contra Confirm/About em cima —
// se algum modal "filho" esta aberto, ESC dele tem prioridade.
// So fecha quando o gesto INTEIRO (mousedown E mouseup) cai na overlay — senao
// selecionar texto num input e arrastar o mouse pra fora do formulario fecharia
// o modal (o 'click' resultante tem a overlay como alvo).
(() => {
  const sov = document.getElementById('settingsOverlay');
  let downOnOverlay = false;
  sov.addEventListener('mousedown', (e) => { downOnOverlay = (e.target === sov); });
  sov.addEventListener('mouseup', (e) => {
    if (downOnOverlay && e.target === sov) Settings.close();
    downOnOverlay = false;
  });
})();

// Commit IMEDIATO: qualquer 'change' num input do modal aplica na hora (sem
// botao Salvar). 'change' dispara no blur/Enter pra textos e no toggle pra
// checkboxes/dropdowns/sliders — nao a cada tecla. A hotkey tem handler proprio
// (onHotkeyChange -> _commitHotkey); aqui o commit() so ignora ela.
document.getElementById('settingsOverlay').addEventListener('change', (e) => {
  if (typeof Settings !== 'undefined' && Settings.commit) Settings.commit();
  // Ligar/desligar a auto-gravacao mostra/esconde a lista de dispositivos do
  // perfil na hora (change so dispara no toggle do checkbox — basta aqui).
  if (e.target && e.target.id === 'settingsAutoRecordOnMic' &&
      typeof Settings !== 'undefined' && Settings._syncAutoRecordDevicesVisibility)
    Settings._syncAutoRecordDevicesVisibility();
  // Idem pro seletor de canto do indicador de tela.
  if (e.target && e.target.id === 'settingsRecIndicator' &&
      typeof Settings !== 'undefined' && Settings._syncRecIndicatorVisibility)
    Settings._syncRecIndicatorVisibility();
});
// Show/hide do campo de excecoes ao vivo enquanto digita os apps monitorados
// ('change' so dispara no blur; aqui queremos resposta imediata a cada tecla).
document.getElementById('settingsOverlay').addEventListener('input', (e) => {
  if (e.target && e.target.id === 'settingsAutoRecordMicApps' &&
      typeof Settings !== 'undefined' && Settings._syncAutoRecordExceptVisibility)
    Settings._syncAutoRecordExceptVisibility();
});
document.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  const sov = document.getElementById('settingsOverlay');
  if (!sov.classList.contains('visible')) return;
  // Se o Confirm esta sobreposto, deixa ele tratar o ESC — o handler dele
  // fecha primeiro. (O About nao entra mais aqui: virou uma aba, nao um
  // modal por cima.)
  const cov = document.getElementById('confirmOverlay');
  if (cov && cov.classList.contains('visible')) return;
  e.preventDefault();
  Settings.close();
});

const Settings = {
  // Aba ativa do menu lateral. Lembrada entre aberturas do modal (so na
  // sessao, nao vai pro config) — reabrir cai na aba onde o user parou.
  currentTab: 'general',
  currentRecDir: '',
  currentThemePref: 'system',   // 'system' | 'dark' | 'light' (modo do tema)
  currentCodec: 'auto',
  currentWindowTitle: 'NoOBS',
  currentFilenamePattern: '{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}',
  currentHotkey: '',
  currentAutostart: false,
  currentCloseToTray: false,
  currentMinimizeOnRecord: false,
  currentNotifyOnRecord: false,
  currentScrollLockIndicator: false,
  currentRecIndicator: false,
  currentRecIndicatorCorner: 'top-right',
  currentRecIndicatorOpacity: 90,
  currentPlaySoundOnRecord: false,
  currentStopOnLock: false,
  currentHibernate: false,
  currentAutoRecordOnMic: false,
  currentAutoRecordMicApps: '',
  currentAutoRecordMicExcept: '',
  currentCheckUpdates: true,
  currentRecordingQuality: 5,
  currentRecordingKeyframe: 2,
  currentLibraryThumbs: 'auto',   // 'auto' | 'always' | 'off'
  recordDirCloud: false,          // pasta de gravacao parece estar no OneDrive
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
    document.getElementById('settingsWindowTitle').value = this.currentWindowTitle || '';
    document.getElementById('settingsFilenamePattern').value = this.currentFilenamePattern || '';
    this._renderFilenamePreview();
    this._loadHotkeyIntoUi(this.currentHotkey || '');
    document.getElementById('settingsAutostart').checked = !!this.currentAutostart;
    document.getElementById('settingsCloseToTray').checked = !!this.currentCloseToTray;
    document.getElementById('settingsMinimizeOnRecord').checked = !!this.currentMinimizeOnRecord;
    document.getElementById('settingsNotifyOnRecord').checked = !!this.currentNotifyOnRecord;
    document.getElementById('settingsScrollLockIndicator').checked = !!this.currentScrollLockIndicator;
    document.getElementById('settingsRecIndicator').checked = !!this.currentRecIndicator;
    document.getElementById('settingsRecIndicatorCorner').value = this.currentRecIndicatorCorner || 'top-right';
    document.getElementById('settingsRecIndicatorOpacity').value = this.currentRecIndicatorOpacity || 90;
    this._syncRecIndicatorOpacityLabel();
    document.getElementById('settingsPlaySoundOnRecord').checked = !!this.currentPlaySoundOnRecord;
    document.getElementById('settingsStopOnLock').checked = !!this.currentStopOnLock;
    document.getElementById('settingsHibernate').checked = !!this.currentHibernate;
    document.getElementById('settingsAutoRecordOnMic').checked = !!this.currentAutoRecordOnMic;
    document.getElementById('settingsAutoRecordMicApps').value = this.currentAutoRecordMicApps || '';
    document.getElementById('settingsAutoRecordMicExcept').value = this.currentAutoRecordMicExcept || '';
    document.getElementById('settingsCheckUpdates').checked = !!this.currentCheckUpdates;
    document.getElementById('settingsRecordingQuality').value = String(this.currentRecordingQuality | 0);
    this._syncQualityHint();
    // Computa presets baseados no maxMonitorHz atual + aplica no slider
    // (indice + labels + ticks). Idempotente.
    this.fpsPresets = this._computeFpsPresets();
    this._applyFpsPresetsToSlider();
    this._populateFpsTicks();
    this._syncFpsHint();
    document.getElementById('settingsRecordingKeyframe').value =
      String(this.currentRecordingKeyframe | 0);
    this._populateKeyframeTicks();
    this._syncKeyframeHint();
    document.getElementById('settingsLibraryThumbs').value = this.currentLibraryThumbs;
    this._syncLibraryHint();
    this._syncMinimizeOnRecordLabel();
    this._syncNotifyOnRecordEnabled();
    this._syncHibernateEnabled();
    this._syncAutoRecordExceptVisibility();
    this._syncAutoRecordDevicesVisibility();
    this._syncRecIndicatorVisibility();
    this._syncCodecMaxRes();
    this._updateThemeButtons();
    this.showTab(this.currentTab);
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
  setTheme(mode) {
    if (mode !== 'dark' && mode !== 'light' && mode !== 'system') return;
    this.currentThemePref = mode;
    // data-theme e sempre 'dark'/'light' (resolvido). Pro 'system', resolve
    // pelo tema do SO (WebView reporta via prefers-color-scheme); o backend
    // confirma pela registry no push seguinte.
    const resolved = (mode === 'system') ? this._osTheme() : mode;
    document.documentElement.setAttribute('data-theme', resolved);
    notifyTitlebarTheme(resolved);
    Bridge.send('set_theme', { theme: mode });
    this._updateThemeButtons();
  },
  // Tema do SO via WebView (prefers-color-scheme). Fallback 'dark'.
  _osTheme() {
    try {
      return (window.matchMedia &&
              window.matchMedia('(prefers-color-scheme: dark)').matches) ? 'dark' : 'light';
    } catch (e) { return 'dark'; }
  },
  // Chamado pelo handler de 'theme' do backend: guarda o MODO ('system'/
  // 'dark'/'light') e destaca o botao certo.
  applyThemePref(pref) {
    this.currentThemePref =
      (pref === 'dark' || pref === 'light' || pref === 'system') ? pref : 'system';
    this._updateThemeButtons();
  },
  _updateThemeButtons() {
    const pref = this.currentThemePref || 'system';
    // Um unico ativo: limpa TODOS e marca so o botao do modo atual. Assim
    // "sistema" nunca fica aceso junto com "escuro"/"claro", e qualquer estado
    // sujo (ex.: highlight antigo por tema resolvido) se auto-corrige.
    document.querySelectorAll('.theme-toggle-btn').forEach(btn => {
      btn.classList.remove('active');
    });
    const active = document.querySelector('.theme-toggle-btn[data-theme="' + pref + '"]');
    if (active) active.classList.add('active');
  },
  // Aplica as configuracoes IMEDIATAMENTE (sem botao Salvar). O listener de
  // 'change' do modal chama isto a cada alteracao — le todos os campos e envia
  // so os que mudaram (diff contra current*). A hotkey e tratada a parte
  // (onHotkeyChange -> _commitHotkey), pois exige validacao async no backend.
  commit() {
    try {
      const language = document.getElementById('settingsLanguage').value || '';
      const path = document.getElementById('settingsRecDir').value.trim();
      const codec = document.getElementById('settingsCodec').value;
      const windowTitle = document.getElementById('settingsWindowTitle').value.trim();
      const filenamePattern = document.getElementById('settingsFilenamePattern').value.trim();
      const autostart = document.getElementById('settingsAutostart').checked;
      const closeToTray = document.getElementById('settingsCloseToTray').checked;
      const minimizeOnRecord = document.getElementById('settingsMinimizeOnRecord').checked;
      const notifyOnRecord = document.getElementById('settingsNotifyOnRecord').checked;
      const scrollLockIndicator = document.getElementById('settingsScrollLockIndicator').checked;
      const recIndicator = document.getElementById('settingsRecIndicator').checked;
      const recIndicatorCorner = document.getElementById('settingsRecIndicatorCorner').value;
      const recIndicatorOpacity = parseInt(document.getElementById('settingsRecIndicatorOpacity').value, 10) || 90;
      const playSoundOnRecord = document.getElementById('settingsPlaySoundOnRecord').checked;
      const stopOnLock = document.getElementById('settingsStopOnLock').checked;
      const hibernate = document.getElementById('settingsHibernate').checked;
      const autoRecordOnMic = document.getElementById('settingsAutoRecordOnMic').checked;
      const autoRecordMicApps = document.getElementById('settingsAutoRecordMicApps').value.trim();
      const autoRecordMicExcept = document.getElementById('settingsAutoRecordMicExcept').value.trim();
      const checkUpdates = document.getElementById('settingsCheckUpdates').checked;
      const recordingQuality = parseInt(document.getElementById('settingsRecordingQuality').value, 10) | 0;
      // Slider snap nas predefinicoes — _currentFpsFromSlider mapeia
      // indice -> fps direto da lista, garantindo valor valido [20..max].
      const recordingFps = this._currentFpsFromSlider();
      const recordingKeyframe = parseInt(document.getElementById('settingsRecordingKeyframe').value, 10) | 0;
      const libraryThumbs = document.getElementById('settingsLibraryThumbs').value;

      // path vazio = restaurar pro default (USERPROFILE\Videos). So
      // envia se mudou — evita rebuild desnecessario da lista de gravacoes.
      if (path !== this.currentRecDir)
        Bridge.send('set_record_dir', { path });
      if (codec) Bridge.send('set_codec', { codec });
      // Titulo da janela: vazio = backend usa 'NoOBS'. Nome do arquivo: vazio =
      // backend usa o modelo padrao.
      if (windowTitle !== this.currentWindowTitle)
        Bridge.send('set_window_title', { title: windowTitle });
      if (filenamePattern !== this.currentFilenamePattern)
        Bridge.send('set_filename_pattern', { pattern: filenamePattern });
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
      if (recIndicator !== this.currentRecIndicator)
        Bridge.send('set_rec_indicator', { enabled: recIndicator });
      if (recIndicatorCorner !== this.currentRecIndicatorCorner)
        Bridge.send('set_rec_indicator_corner', { corner: recIndicatorCorner });
      if (recIndicatorOpacity !== this.currentRecIndicatorOpacity)
        Bridge.send('set_rec_indicator_opacity', { opacity: recIndicatorOpacity });
      if (playSoundOnRecord !== this.currentPlaySoundOnRecord)
        Bridge.send('set_play_sound_on_record', { enabled: playSoundOnRecord });
      if (stopOnLock !== this.currentStopOnLock)
        Bridge.send('set_stop_on_lock', { enabled: stopOnLock });
      if (hibernate !== this.currentHibernate)
        Bridge.send('set_hibernate', { enabled: hibernate });
      if (autoRecordOnMic !== this.currentAutoRecordOnMic)
        Bridge.send('set_auto_record_on_mic', { enabled: autoRecordOnMic });
      if (autoRecordMicApps !== this.currentAutoRecordMicApps)
        Bridge.send('set_auto_record_mic_apps', { apps: autoRecordMicApps });
      if (autoRecordMicExcept !== this.currentAutoRecordMicExcept)
        Bridge.send('set_auto_record_mic_except', { apps: autoRecordMicExcept });
      if (recordingQuality !== this.currentRecordingQuality)
        Bridge.send('set_recording_quality', { level: recordingQuality });
      if (recordingFps !== this.currentRecordingFps)
        Bridge.send('set_recording_fps', { fps: recordingFps });
      if (recordingKeyframe !== this.currentRecordingKeyframe)
        Bridge.send('set_recording_keyframe', { sec: recordingKeyframe });
      if (libraryThumbs !== this.currentLibraryThumbs)
        Bridge.send('set_library_thumbs', { mode: libraryThumbs });
      if (language !== this.currentLanguagePref)
        Bridge.send('set_language', { language });
      // Atualiza cache local imediato pra evitar reenvio de uma mesma
      // mudanca na proxima save (e evitar flash de valor antigo num
      // re-open antes da resposta do get_settings).
      this.currentRecDir = path || this.currentRecDir;
      if (codec) this.currentCodec = codec;
      this.currentWindowTitle = windowTitle || 'NoOBS';
      this.currentFilenamePattern = filenamePattern || this.currentFilenamePattern;
      this.currentAutostart = autostart;
      this.currentCloseToTray = closeToTray;
      this.currentMinimizeOnRecord = minimizeOnRecord;
      this.currentNotifyOnRecord = notifyOnRecord;
      this.currentScrollLockIndicator = scrollLockIndicator;
      this.currentRecIndicator = recIndicator;
      this.currentRecIndicatorCorner = recIndicatorCorner;
      this.currentRecIndicatorOpacity = recIndicatorOpacity;
      this.currentPlaySoundOnRecord = playSoundOnRecord;
      this.currentStopOnLock = stopOnLock;
      this.currentHibernate = hibernate;
      this.currentAutoRecordOnMic = autoRecordOnMic;
      this.currentAutoRecordMicApps = autoRecordMicApps;
      this.currentAutoRecordMicExcept = autoRecordMicExcept;
      this.currentCheckUpdates = checkUpdates;
      this.currentRecordingQuality = recordingQuality;
      this.currentRecordingFps = recordingFps;
      this.currentRecordingKeyframe = recordingKeyframe;
      this.currentLibraryThumbs = libraryThumbs;
      this.currentLanguagePref = language;
      // Atualiza o icone de aviso na barra lateral caso o codec novo
      // tenha um limite diferente do anterior (ex.: h264-hw → av1-hw
      // remove o warning de canvas grande).
      if (typeof Displays !== 'undefined' && Displays._updateCount)
        Displays._updateCount();
    } catch (err) {
      console.error('[Settings.commit] erro:', err);
      Toast.show(T('toast.errorSaving'), String(err && err.message || err),
        { warn: true, ttl: 6000 });
    }
  },
  applySettings(data) {
    // Com o modal ABERTO, a UI e a fonte da verdade dos campos. O backend
    // pode ecoar um snapshot no MEIO de um lote de alteracoes: o
    // HandleSetRecordDir chama PushSettings assim que grava a pasta, e o
    // commit() envia set_record_dir ANTES de set_codec/set_recording_quality
    // — entao esse snapshot ainda traz codec/qualidade velhos. Reaplicar
    // isso jogava valores antigos por cima do que o usuario tinha acabado
    // de definir (sintoma: "Restaurar padroes" so aparecia depois de fechar
    // e reabrir a tela). Aqui so absorvemos o recordDir, que e o unico campo
    // que o backend RESOLVE sozinho (vazio -> USERPROFILE\Videos).
    const overlay = document.getElementById('settingsOverlay');
    if (overlay && overlay.classList.contains('visible')) {
      this.currentRecDir = data.recordDir || '';
      const rd = document.getElementById('settingsRecDir');
      if (rd) rd.value = this.currentRecDir;
      return;
    }
    this.currentRecDir = data.recordDir || '';
    const prevCodec = this.currentCodec;
    this.currentCodec = data.codec || 'auto';
    // O dialogo de exportacao pre-seleciona o mesmo codec das Configuracoes.
    if (typeof Export !== 'undefined') Export.setDefaultEncoder(this.currentCodec);
    this.currentWindowTitle = data.windowTitle || 'NoOBS';
    this.currentFilenamePattern =
      data.filenamePattern || '{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}';
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
    this.currentRecIndicator = !!data.recIndicator;
    this.currentRecIndicatorCorner = data.recIndicatorCorner || 'top-right';
    this.currentRecIndicatorOpacity =
      (typeof data.recIndicatorOpacity === 'number') ? data.recIndicatorOpacity : 90;
    // playSoundOnRecord: default true (backend GetConfigBool). O backend sempre
    // envia o valor real; !!data.* so protege contra ausencia acidental.
    this.currentPlaySoundOnRecord = !!data.playSoundOnRecord;
    // stopOnLock: default true. Quando ON, Windows lock event (Win+L etc.)
    // chama HandleRecordStop no backend.
    this.currentStopOnLock = !!data.stopOnLock;
    // hibernate: default true — so faz sentido com closeToTray ON, e gateamos
    // a UI pra forcar isso (ambos vem ON por padrao, entao consistente).
    this.currentHibernate = !!data.hibernate;
    this.currentAutoRecordOnMic = !!data.autoRecordOnMic;
    this.currentAutoRecordMicApps = data.autoRecordMicApps || '';
    this.currentAutoRecordMicExcept = data.autoRecordMicExcept || '';
    // checkUpdates: default TRUE (chave ausente = ligado)
    this.currentCheckUpdates = (data.checkUpdates !== false);
    const av = document.getElementById('settingsAppVersion');
    if (av && data.appVersion) av.textContent = data.appVersion;
    // recordingQuality: nivel do slider 0..10, default 5. O backend ja
    // manda migrado da escala antiga (-4..+2) — ver
    // OBSEncoder.GetRecordingQualityLevel.
    let rq = parseInt(data.recordingQuality, 10);
    if (!Number.isFinite(rq)) rq = 5;
    if (rq < 0)  rq = 0;
    if (rq > 10) rq = 10;
    this.currentRecordingQuality = rq;
    // recordingFps: >= 10, default 30 (padrao do NoOBS).
    let fps = parseInt(data.recordingFps, 10);
    if (!Number.isFinite(fps) || fps < 10) fps = 30;
    this.currentRecordingFps = fps;
    // recordingKeyframeSec: 1..10, default 2.
    let kf = parseInt(data.recordingKeyframeSec, 10);
    if (!Number.isFinite(kf)) kf = 2;
    if (kf < 1)  kf = 1;
    if (kf > 10) kf = 10;
    this.currentRecordingKeyframe = kf;
    // libraryThumbs: 'auto' | 'always' | 'off', default 'auto'.
    this.currentLibraryThumbs =
      ['auto', 'always', 'off'].includes(data.libraryThumbs) ? data.libraryThumbs : 'auto';
    this.recordDirCloud = !!data.recordDirCloud;
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
    const wt = document.getElementById('settingsWindowTitle');
    if (wt) wt.value = this.currentWindowTitle;
    const fp = document.getElementById('settingsFilenamePattern');
    if (fp) fp.value = this.currentFilenamePattern;
    this._renderFilenamePreview();
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
    const ri = document.getElementById('settingsRecIndicator');
    if (ri) ri.checked = this.currentRecIndicator;
    const ric = document.getElementById('settingsRecIndicatorCorner');
    if (ric) ric.value = this.currentRecIndicatorCorner || 'top-right';
    const rio = document.getElementById('settingsRecIndicatorOpacity');
    if (rio) rio.value = this.currentRecIndicatorOpacity || 90;
    this._syncRecIndicatorOpacityLabel();
    const ps = document.getElementById('settingsPlaySoundOnRecord');
    if (ps) ps.checked = this.currentPlaySoundOnRecord;
    const sol = document.getElementById('settingsStopOnLock');
    if (sol) sol.checked = this.currentStopOnLock;
    const hb = document.getElementById('settingsHibernate');
    if (hb) hb.checked = this.currentHibernate;
    const arm = document.getElementById('settingsAutoRecordOnMic');
    if (arm) arm.checked = this.currentAutoRecordOnMic;
    const arma = document.getElementById('settingsAutoRecordMicApps');
    if (arma) arma.value = this.currentAutoRecordMicApps;
    const arme = document.getElementById('settingsAutoRecordMicExcept');
    if (arme) arme.value = this.currentAutoRecordMicExcept;
    const rqEl = document.getElementById('settingsRecordingQuality');
    if (rqEl) rqEl.value = String(this.currentRecordingQuality);
    this._syncQualityHint();
    // Recomputa presets pra refletir o maxMonitorHz que acabou de chegar.
    this.fpsPresets = this._computeFpsPresets();
    this._applyFpsPresetsToSlider();
    this._populateFpsTicks();
    this._syncFpsHint();
    const kfEl = document.getElementById('settingsRecordingKeyframe');
    if (kfEl) kfEl.value = String(this.currentRecordingKeyframe);
    this._populateKeyframeTicks();
    this._syncKeyframeHint();
    const ltEl = document.getElementById('settingsLibraryThumbs');
    if (ltEl) ltEl.value = this.currentLibraryThumbs;
    this._syncLibraryHint();
    this._syncMinimizeOnRecordLabel();
    this._syncNotifyOnRecordEnabled();
    this._syncHibernateEnabled();
    this._syncAutoRecordExceptVisibility();
    this._syncAutoRecordDevicesVisibility();
    this._syncRecIndicatorVisibility();
    this._syncCodecMaxRes();
  },
  setPickedPath(path) {
    document.getElementById('settingsRecDir').value = path;
    // Set programatico nao dispara 'change' — aplica na hora manualmente.
    this.commit();
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
  onKeyframeChange() {
    this._syncKeyframeHint();
  },
  onLibraryThumbsChange() {
    this._syncLibraryHint();
  },
  _syncLibraryHint() {
    const el = document.getElementById('settingsLibraryThumbs');
    const hint = document.getElementById('settingsLibraryHint');
    if (!el || !hint) return;
    const v = el.value;
    const key = (v === 'always') ? 'always' : (v === 'off') ? 'off' : 'auto';
    let txt = T('settings.library.hint.' + key);
    // Reforca o aviso de nuvem quando a pasta de gravacao esta no OneDrive
    // (so faz diferenca pros modos que geram: auto/always).
    if (this.recordDirCloud && v !== 'off')
      txt += ' ' + T('settings.library.cloudNote');
    hint.textContent = txt;
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
  // Titulo da janela: sem preview/commit ao vivo — o valor e lido no save().
  // Handler existe so pra o oninput do input nao referenciar algo indefinido.
  onWindowTitleChange() {},
  onFilenamePatternChange() {
    this._renderFilenamePreview();
  },
  // Prévia ao vivo do nome do arquivo — espelha ApplyFilenamePattern (Delphi):
  // substitui os codigos {AAAA}{MM}{DD}{HH}{NN}{SS} (case-insensitive) pela
  // data/hora ATUAL, remove chaves sobrando, sanitiza caracteres invalidos.
  _buildFilenameFromPattern(pattern, d) {
    const p2 = n => String(n).padStart(2, '0');
    const p3 = n => String(n).padStart(3, '0');
    let s = String(pattern || '');
    s = s.replace(/\{AAAA\}/gi, String(d.getFullYear()))
         .replace(/\{MM\}/gi,   p2(d.getMonth() + 1))
         .replace(/\{DD\}/gi,   p2(d.getDate()))
         .replace(/\{HH\}/gi,   p2(d.getHours()))
         .replace(/\{NN\}/gi,   p2(d.getMinutes()))
         .replace(/\{SS\}/gi,   p2(d.getSeconds()))
         .replace(/\{ZZZ\}/gi,  p3(d.getMilliseconds()));
    s = s.replace(/[{}]/g, '');            // chaves remanescentes (codigo invalido)
    s = s.replace(/[\\/:*?"<>|]/g, '_').trim();
    if (!s)
      s = 'NoOBS_' + d.getFullYear() + '-' + p2(d.getMonth() + 1) + '-' +
          p2(d.getDate()) + '_' + p2(d.getHours()) + '-' + p2(d.getMinutes()) +
          '-' + p2(d.getSeconds());
    return s + '.mkv';
  },
  _renderFilenamePreview() {
    const inp = document.getElementById('settingsFilenamePattern');
    const out = document.getElementById('settingsFilenamePreview');
    if (!inp || !out) return;
    const name = this._buildFilenameFromPattern(inp.value, new Date());
    out.textContent = T('settings.filename.previewLabel', { name });
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
    else if (codec === 'h264-sw' || codec === 'hevc-hw' ||
             codec === 'av1-hw' || codec === 'av1-sw') dim = 8192;
    else /* auto */ dim = caps.h264Hw ? 4096 : 8192;
    el.textContent = T('settings.codec.maxRes', { w: dim, h: dim });
    // O aviso de CPU depende exatamente das mesmas coisas que a resolucao
    // maxima (codec escolhido + caps), e esta funcao ja e chamada nos 6
    // pontos onde qualquer uma das duas muda — incluindo a chegada das
    // caps depois do warmup do libobs. Pendurar aqui evita repetir a
    // chamada em todos eles e esquecer de um.
    this._syncCodecCpuWarn();
  },
  // Aviso de uso de CPU do AV1 por software. Só o 'av1-sw' codifica na
  // CPU disputando a máquina com o usuário — o 'h264-sw' também roda em
  // CPU, mas é barato o bastante pra não merecer alarme.
  _syncCodecCpuWarn() {
    const sel = document.getElementById('settingsCodec');
    const warn = document.getElementById('settingsCodecCpuWarn');
    if (!sel || !warn) return;
    warn.hidden = (sel.value !== 'av1-sw');
  },
  _syncQualityHint() {
    const el = document.getElementById('settingsRecordingQuality');
    const hint = document.getElementById('settingsQualityHint');
    if (!el || !hint) return;
    const v = parseInt(el.value, 10) | 0;
    // Chave estilo 'settings.quality.hint.0' .. '.10' — match com o JSON.
    // A legenda descreve a QUALIDADE, sem numero de taxa: a gravacao roda
    // em qualidade constante, entao quem manda no tamanho e o conteudo —
    // nao existe taxa fixa pra anunciar (era o que a legenda antiga fazia).
    hint.textContent = (v >= 0 && v <= 10) ? T('settings.quality.hint.' + v) : '';
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
  _syncKeyframeHint() {
    const el = document.getElementById('settingsRecordingKeyframe');
    const hint = document.getElementById('settingsKeyframeHint');
    if (!el || !hint) return;
    const v = parseInt(el.value, 10) | 0;
    let bucket;
    if (v <= 1)      bucket = 'precise';
    else if (v <= 2) bucket = 'balanced';
    else if (v <= 5) bucket = 'medium';
    else             bucket = 'spaced';
    hint.textContent = T('settings.keyframe.hint.' + bucket, { sec: v });
    this._syncSliderFill(el);
  },
  // Ticks de referencia (1..10s) posicionados pelo VALOR real no range
  // 1..10 — mesma matematica de alinhamento ao thumb dos outros sliders
  // (left = 7px + ratio*(100% - 14px)).
  _populateKeyframeTicks() {
    const el = document.getElementById('keyframeTicks');
    if (!el) return;
    const ticks = Array.from({ length: 10 }, (_, i) => i + 1);
    const min = 1, max = 10;
    el.innerHTML = ticks.map(v => {
      const ratio = (v - min) / (max - min);
      const left = `calc(7px + ${ratio} * (100% - 14px))`;
      return `<span class="fps-tick" style="left: ${left}">${v}s</span>`;
    }).join('');
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
  // O campo de excecoes so faz sentido quando NAO ha lista de monitorados
  // (monitora tudo, menos as excecoes). Com apps monitorados especificos, a
  // propria lista ja e o filtro e as excecoes nao se aplicam (o backend
  // tambem as ignora nesse caso), entao esconde pra nao confundir. NAO limpa
  // o valor — se o user esvaziar os monitorados de novo, as excecoes voltam.
  // Troca a aba visivel do menu lateral. Os campos das outras abas continuam
  // NO DOM (so com display:none) — e por isso que os getElementById, o
  // listener delegado de 'change' (commit imediato) e os _sync* de
  // dependencia seguem funcionando sem nenhuma alteracao de logica.
  showTab(name) {
    this.currentTab = name;
    // Campos E divisores por aba: ambos carregam data-panel e alternam junto.
    document.querySelectorAll('.settings-field[data-panel], .settings-divider[data-panel]').forEach(f => {
      f.style.display = (f.dataset.panel === name) ? '' : 'none';
    });
    document.querySelectorAll('.settings-tab').forEach(t => {
      t.classList.toggle('active', t.dataset.tab === name);
    });
    // Cada aba comeca do topo — senao herda o scroll da aba anterior.
    const body = document.getElementById('settingsBody');
    if (body) body.scrollTop = 0;
    // Lista de dispositivos montada sob demanda.
    if (name === 'devices' && typeof Devices !== 'undefined') Devices.render();
    // Idem pro perfil de auto-gravacao: ao entrar na aba Comportamento,
    // re-sincroniza visibilidade + renderiza da Devices.all atual (pega
    // hot-plug que ocorreu enquanto o usuario estava em outra aba).
    if (name === 'behavior') this._syncAutoRecordDevicesVisibility();
  },
  _syncAutoRecordExceptVisibility() {
    const apps = document.getElementById('settingsAutoRecordMicApps');
    const wrap = document.getElementById('settingsAutoRecordExceptWrap');
    if (!apps || !wrap) return;
    wrap.style.display = (apps.value.trim() === '') ? '' : 'none';
  },
  // O seletor de canto do indicador de tela so aparece com o indicador ligado.
  _syncRecIndicatorVisibility() {
    const on = document.getElementById('settingsRecIndicator');
    const wrap = document.getElementById('settingsRecIndicatorCornerWrap');
    if (!on || !wrap) return;
    wrap.style.display = on.checked ? '' : 'none';
  },
  // Atualiza o rotulo "NN%" ao lado do slider de opacidade. Chamado pelo
  // oninput do range (ao vivo) e ao abrir/aplicar as Config.
  _syncRecIndicatorOpacityLabel() {
    const s = document.getElementById('settingsRecIndicatorOpacity');
    const v = document.getElementById('settingsRecIndicatorOpacityValue');
    const pct = s ? (parseInt(s.value, 10) || 90) : 90;
    if (v) v.textContent = pct + '%';
    // Previa: a opacidade da pilha acompanha o slider (demonstra a transparencia).
    const p = document.getElementById('recIndicatorPreview');
    if (p) p.style.opacity = (pct / 100).toFixed(2);
  },
  // A lista de dispositivos do perfil so faz sentido com a auto-gravacao
  // ligada — mostra/esconde igual ao campo de excecoes. Ao mostrar, renderiza
  // (reusa Devices.all, ja populado pelos pushes de dispositivo).
  _syncAutoRecordDevicesVisibility() {
    const on = document.getElementById('settingsAutoRecordOnMic');
    const wrap = document.getElementById('settingsAutoRecordDevicesWrap');
    if (!on || !wrap) return;
    wrap.style.display = on.checked ? '' : 'none';
    if (on.checked && typeof AutoDevices !== 'undefined') AutoDevices.render();
  },
  restoreDefaults() {
    // Reseta os campos e APLICA na hora (commit no fim) — nao ha botao
    // Salvar. Por isso pede confirmacao antes: sem etapa de revisao, o
    // clique e irreversivel.
    // Tema fica de fora: e aplicado/salvo instantaneo pelos botoes de
    // toggle da aba Geral, fora deste fluxo.
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
        document.getElementById('settingsWindowTitle').value = 'NoOBS';
        document.getElementById('settingsFilenamePattern').value =
          '{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}';
        this._loadHotkeyIntoUi('Pause/Break');
        // Defaults novos: iniciar com Windows, minimizar p/ bandeja, minimizar
        // ao gravar, som de inicio/fim, parar ao bloquear e hibernar vem LIGADOS.
        // notifyOnRecord e scrollLockIndicator seguem desligados (opt-in).
        document.getElementById('settingsAutostart').checked = true;
        document.getElementById('settingsCloseToTray').checked = true;
        document.getElementById('settingsMinimizeOnRecord').checked = true;
        document.getElementById('settingsNotifyOnRecord').checked = false;
        document.getElementById('settingsScrollLockIndicator').checked = false;
        document.getElementById('settingsRecIndicator').checked = false;
        document.getElementById('settingsRecIndicatorCorner').value = 'top-right';
        document.getElementById('settingsRecIndicatorOpacity').value = 90;
        this._syncRecIndicatorOpacityLabel();
        document.getElementById('settingsPlaySoundOnRecord').checked = true;
        document.getElementById('settingsStopOnLock').checked = true;
        document.getElementById('settingsHibernate').checked = true;
        document.getElementById('settingsAutoRecordOnMic').checked = false;
        document.getElementById('settingsAutoRecordMicApps').value = '';
        document.getElementById('settingsAutoRecordMicExcept').value = '';
        document.getElementById('settingsCheckUpdates').checked = true;
        document.getElementById('settingsRecordingQuality').value = '5';
        // FPS: snap pra 30 (preset garantido a existir se max >= 30).
        // Atualiza currentRecordingFps ANTES de _applyFpsPresetsToSlider
        // pra que o slider sente nessa posicao.
        this.currentRecordingFps = 30;
        this.fpsPresets = this._computeFpsPresets();
        this._applyFpsPresetsToSlider();
        this._populateFpsTicks();
        // Keyframe: default 2s.
        document.getElementById('settingsRecordingKeyframe').value = '2';
        this._populateKeyframeTicks();
        // Previas da biblioteca: default 'auto'.
        document.getElementById('settingsLibraryThumbs').value = 'auto';
        this._syncLibraryHint();
        this._syncMinimizeOnRecordLabel();
        this._syncNotifyOnRecordEnabled();
        this._syncHibernateEnabled();
        this._syncAutoRecordExceptVisibility();
        this._syncRecIndicatorVisibility();
        this._syncQualityHint();
        this._syncFpsHint();
        this._syncKeyframeHint();
        this._syncCodecMaxRes();
        this._renderFilenamePreview();
        // Dispositivos ocultos nao tem campo no DOM — o reset generico acima
        // nao os alcanca. Desfaz explicitamente (volta todos a visiveis).
        if (typeof Devices !== 'undefined') Devices.showAll();
        // FPS é o único campo em que o reset precisa escrever currentX ANTES
        // (o slider guarda um ÍNDICE de preset, então _applyFpsPresetsToSlider
        // depende de currentRecordingFps pra saber onde sentar). Só que isso
        // derrota o guard do commit: ele compara slider (30) com currentX (30),
        // conclui "não mudou" e nunca envia set_recording_fps — o backend ficava
        // com o valor antigo e reabrir o app o trazia de volta.
        // Zerar aqui, DEPOIS de todos os _sync*, força o commit a enviar.
        this.currentRecordingFps = -1;
        // Aplica os defaults IMEDIATAMENTE (nao ha botao Salvar).
        this.commit();
        this._commitHotkey();
        // Re-sincroniza a aba visivel: os campos de outras abas estao com
        // display:none na hora do reset, e alguns controles (fill dos
        // sliders, hints) so recalculam direito quando visiveis.
        this.showTab(this.currentTab);
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
    this._commitHotkey();
  },
  // Aplica a hotkey imediatamente (async: valida no backend). Separada do
  // commit() geral. Se invalida, avisa e NAO aplica (currentHotkey fica).
  async _commitHotkey() {
    const hotkey = this._readHotkeyFromUi();
    if (hotkey === this.currentHotkey) return;
    if (hotkey !== '') {
      const validation = await validateHotkeyWithBackend(hotkey);
      if (!validation.ok) {
        Toast.show(T('toast.invalidHotkey'), validation.reason +
          ' ' + T('settings.hotkey.chooseAnother'), { warn: true, ttl: 7000 });
        return;
      }
    }
    Bridge.send('set_hotkey', { hotkey });
    this.currentHotkey = hotkey;
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
      else if (opt.value === 'av1-sw') avail = !!caps.av1Sw;
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
      } else if (opt.value === 'av1-sw') {
        opt.textContent = T('settings.codec.av1Sw') +
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


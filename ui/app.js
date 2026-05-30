// =====================================================================
// Bridge — comunicacao com o host Delphi (OBSBridge.pas)
// =====================================================================
// =====================================================================
// I18n — sistema de traducoes (carregado do bundle Delphi via Bridge)
//
// Padrao alinhado com i18next: chaves com namespace via dot-notation
// ('settings.title'), interpolacao {{var}}, fallback automatico.
//
// Uso no HTML:
//   <span data-i18n="settings.title">Configurações</span>
//   <button data-i18n-title="header.close">×</button>
//   <input data-i18n-placeholder="recordings.search">
//   <button data-i18n-aria="record.ariaLabel">REC</button>
//
// Uso no JS:
//   T('toast.saved')                 -> "Configurações salvas"
//   T('record.finished', {min: 5})   -> "Gravação finalizada (5m 0s)."
//
// O texto inicial no HTML serve de fallback enquanto o bundle nao
// chegou (1o paint antes do 'init' message).
// =====================================================================
const I18n = {
  bundle: null,    // objeto JSON do idioma ativo
  language: '',    // codigo do idioma ('pt-BR', 'en', ...)
  setBundle(bundle, language) {
    this.bundle = bundle || null;
    this.language = language || '';
    document.documentElement.setAttribute('lang', this.language || 'pt-BR');
    this.apply(document);
  },
  // Recupera um valor cru — string, array, ou objeto — pra casos como
  // months[] ou hints que ficam em sub-objetos.
  get(key) { return this._lookup(key); },
  // Lookup string com interpolacao. Chave ausente => '[key]' (sinal pro
  // tradutor identificar o que falta). Aceita 2o param como objeto
  // { name: 'Eduardo', count: 3 }.
  t(key, args) {
    const v = this._lookup(key);
    if (v == null) return '[' + key + ']';
    if (typeof v !== 'string') return String(v);
    if (!args) return v;
    return v.replace(/\{\{(\w+)\}\}/g, (m, k) =>
      (args[k] !== undefined && args[k] !== null) ? String(args[k]) : m);
  },
  _lookup(key) {
    if (!this.bundle || !key) return null;
    const parts = String(key).split('.');
    let cur = this.bundle;
    for (let i = 0; i < parts.length; i++) {
      if (cur == null || typeof cur !== 'object') return null;
      cur = cur[parts[i]];
    }
    return (cur === undefined) ? null : cur;
  },
  // Aplica traducao a todos os elementos com data-i18n* dentro de root.
  // Walk separado por tipo de atributo pra precisao.
  apply(root) {
    root = root || document;
    root.querySelectorAll('[data-i18n]').forEach(el => {
      const k = el.getAttribute('data-i18n');
      if (k) el.textContent = this.t(k);
    });
    root.querySelectorAll('[data-i18n-html]').forEach(el => {
      // Variante que aceita HTML simples (<b>, <i>, <code>) — usado em
      // textos longos do About modal. Limita risco mantendo bundles
      // sob controle dos devs (nao vem de user input).
      const k = el.getAttribute('data-i18n-html');
      if (k) el.innerHTML = this.t(k);
    });
    root.querySelectorAll('[data-i18n-title]').forEach(el => {
      const k = el.getAttribute('data-i18n-title');
      if (k) el.setAttribute('title', this.t(k));
    });
    root.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
      const k = el.getAttribute('data-i18n-placeholder');
      if (k) el.setAttribute('placeholder', this.t(k));
    });
    root.querySelectorAll('[data-i18n-aria]').forEach(el => {
      const k = el.getAttribute('data-i18n-aria');
      if (k) el.setAttribute('aria-label', this.t(k));
    });
    // data-hint = tooltip custom do modulo Hint (nao e o title nativo).
    root.querySelectorAll('[data-i18n-hint]').forEach(el => {
      const k = el.getAttribute('data-i18n-hint');
      if (k) el.setAttribute('data-hint', this.t(k));
    });
    // Renderiza um array do bundle como <li> dentro do elemento. Usado em
    // listas de tamanho variavel (About: features, builtWith).
    root.querySelectorAll('[data-i18n-list]').forEach(el => {
      const k = el.getAttribute('data-i18n-list');
      const arr = this.get(k);
      if (Array.isArray(arr))
        el.innerHTML = arr.map(item => '<li>' + item + '</li>').join('');
    });
  },
};
const T = (k, a) => I18n.t(k, a);

const Bridge = {
  send(type, payload = {}) {
    try {
      window.chrome.webview.postMessage(JSON.stringify({ type, ...payload }));
    } catch (e) {
      console.warn('Bridge.send falhou:', e);
    }
  },
  init() {
    if (!window.chrome || !window.chrome.webview) {
      console.warn('chrome.webview nao disponivel — modo standalone.');
      return;
    }
    window.chrome.webview.addEventListener('message', (e) => {
      const data = (typeof e.data === 'string') ? JSON.parse(e.data) : e.data;
      if (!data || !data.type) return;
      const handler = Bridge.handlers[data.type];
      if (handler) handler(data);
      else console.log('Bridge: tipo nao tratado:', data.type, data);
    });
    Bridge.send('ready');
  },
  handlers: {
    init(data) {
      // i18n vem ANTES de qualquer render — quem usar T() ja pega o
      // bundle carregado. Sem bundle, T() retorna '[key]' e os textos
      // hardcoded no HTML servem de fallback ate o user trocar idioma.
      if (data.i18n) I18n.setBundle(data.i18n, data.language || '');
      Displays.monitors = data.monitors || [];
      Displays.webcams  = data.webcams  || [];
      Displays.render();
      renderSources('mic', 'micList', 'micCount', data.mics);
      renderSources('spk', 'spkList', 'spkCount', data.speakers);
      // freeBytes antes de renderRecordings — updateRecMeta usa.
      if (typeof data.freeBytes === 'number') _lastFreeBytes = data.freeBytes;
      renderRecordings(data.recordings);
      if (data.recordDir !== undefined) Settings.currentRecDir = data.recordDir;
      hideLoading();
      updateRecordButtonAvailability();
    },
    language_changed(data) {
      // Backend trocou de idioma — re-aplica bundle. Como apply() faz
      // walk pelos data-i18n, todos os elementos marcados se atualizam
      // sem reload da pagina.
      if (data.i18n) I18n.setBundle(data.i18n, data.language || '');
      // Atualiza textos dinamicos que dependem de funcoes JS (re-render
      // de listas, botoes com label condicional, etc).
      try { Settings._syncMinimizeOnRecordLabel(); } catch (e) {}
      try { Settings._syncQualityHint(); } catch (e) {}
      try { Settings._syncFpsHint(); } catch (e) {}
      try { Settings._syncCodecMaxRes(); } catch (e) {}
      try { Settings._updateHotkeyPreview(); } catch (e) {}
      try { Displays.render(); } catch (e) {}
      // Re-renderiza a legenda de faixas (textos compostos em JS, sem
      // data-i18n por terem placeholders dinamicos como "Faixa N").
      try { buildTrackLegend(); } catch (e) {}
      // Re-render do estado de gravacao pra atualizar label/status do
      // botao de gravar (T('record.start'/'stop'/'statusReady'/...)).
      // Re-aplica estado de gravacao pra atualizar label (T('record.start'/stop'))
      // e statusText. Usa cache _lastRecordingActive — true/false/null pre-boot.
      try {
        if (_lastRecordingActive !== null)
          applyRecordingState(_lastRecordingActive, 0);
      } catch (e) {}
      try { updateRecordButtonAvailability(); } catch (e) {}
      // Re-render recordings — date groups (Hoje/Ontem/...) + nomes de
      // mes ficam stale; cache populado no proprio renderRecordings.
      try {
        if (typeof renderRecordings === 'function' &&
            window._lastRecordingsItems) {
          renderRecordings(window._lastRecordingsItems);
        }
      } catch (e) {}
      // Re-render do meta (X arquivos · Y usados · Z livres) — buildRecMetaHtml
      // usa T() pluralizado. recalcRecMetaFromDom le do DOM atual.
      try { recalcRecMetaFromDom(); } catch (e) {}
      Toast.show(T('toast.languageChanged'), '', { ttl: 1800 });
    },
    init_pending(data) { showLoading(data && data.message); },
    window_hidden() {
      // Backend escondeu a janela main (minimize pra bandeja, etc).
      // Fecha o player se estiver aberto — quando o user reabrir o
      // app, comeca limpo em vez de reencontrar o player fantasma.
      const ov = document.getElementById('playerOverlay');
      if (ov && ov.classList.contains('visible')) {
        try { Player.close(); } catch (e) {}
      }
    },
    // Notificacao do Windows pedida pelo Delphi (gravacao iniciou/parou).
    //
    // Usa a Web Notifications API do Chromium (WebView2 expoe nativamente
    // e o host ja aprovou a permissao via TPermissionRequestedHandler).
    // O Chromium se vira com AUMID + Action Center, sem precisarmos
    // mexer com WinRT em Delphi.
    //
    // tag fixa "noobs-record": notificacoes novas SUBSTITUEM a anterior
    // (em vez de empilhar na Central). renotify: true forca o popup a
    // reaparecer mesmo com mesma tag.
    //
    // setTimeout + close(): remove a notificacao da Central de
    // Notificacoes depois do popup ter sumido — usuario nao quer ver
    // historico de "Gravacao iniciada/parou" se acumulando ali.
    // (Inspirado no padrao do conversa-web/sound.ts.)
    show_notification(data) {
      if (!('Notification' in window)) return;
      if (Notification.permission !== 'granted') {
        // Permissao deveria estar 'granted' pelo handler do host. Se
        // por algum motivo nao estiver, pede agora (no-op se 'denied').
        Notification.requestPermission().then(p => {
          if (p === 'granted') Bridge.handlers.show_notification(data);
        });
        return;
      }
      try {
        if (Bridge._lastNotif) {
          try { Bridge._lastNotif.onclose = null; Bridge._lastNotif.close(); } catch (e) {}
        }
        const opts = {
          body: data.body || '',
          tag: 'noobs-record',
          renotify: true,
          silent: false,
        };
        // Icone do app (data URL ja recebido via app_icon push do
        // backend). Aparece tanto no popup quanto na Central. Se ainda
        // nao chegou, o toast sai sem icone — sem fallback de URL
        // remota, que poderia atrasar/falhar o display.
        if (Bridge._appIconDataUrl) opts.icon = Bridge._appIconDataUrl;

        const n = new Notification(data.title || 'NoOBS', opts);
        Bridge._lastNotif = n;

        // Clique no popup (ou na entrada na Central) restaura a janela
        // principal — usuario espera "clicou na notificacao, app abre".
        n.onclick = () => {
          try { n.close(); } catch (e) {}
          Bridge.send('tray_show');
        };

        // Auto-fecha apos 5s pra nao acumular na Central de Notificacoes.
        setTimeout(() => {
          try { n.close(); } catch (e) {}
          if (Bridge._lastNotif === n) Bridge._lastNotif = null;
        }, 5000);
      } catch (e) {
        console.warn('show_notification falhou:', e);
      }
    },
    app_icon(data) {
      // Backend extraiu o icon.ico do exe e mandou como data URL.
      // Guarda no Bridge pra reuso (modal Sobre + icone das notificacoes
      // do Windows). Esconde o SVG fallback no modal.
      if (data && data.dataUrl) {
        Bridge._appIconDataUrl = data.dataUrl;
      }
      const img = document.getElementById('aboutAppIcon');
      const fb  = document.getElementById('aboutAppIconFallback');
      if (img && data && data.dataUrl) {
        img.src = data.dataUrl;
        img.style.display = '';
        if (fb) fb.style.display = 'none';
      }
    },
    open_settings() {
      // Pedido do backend (1a execucao apos instalacao). Pequeno delay
      // pra modal abrir depois dos pushes iniciais terem renderizado.
      setTimeout(() => Settings.open(), 200);
    },
    hotkey_validation_result(data) {
      // Resposta do backend pra validateHotkeyWithBackend(). Resolve a
      // Promise pendente (se houver) com { ok, reason }.
      if (_pendingHotkeyValidation) {
        const cb = _pendingHotkeyValidation;
        _pendingHotkeyValidation = null;
        cb({ ok: !!data.ok, reason: data.reason || '' });
      }
    },
    theme(data) { applyTheme(data.theme); },
    recordings_loaded(data) {
      // Espaco livre vem ANTES de renderRecordings pra que updateRecMeta
      // (chamada dentro do render) ja pegue o valor novo.
      if (typeof data.freeBytes === 'number') _lastFreeBytes = data.freeBytes;
      renderRecordings(data.recordings);
      if (data.recordDir !== undefined) Settings.currentRecDir = data.recordDir;
    },
    settings(data) { Settings.applySettings(data); },
    record_dir_picked(data) { Settings.setPickedPath(data.path); },
    monitor_thumbs(data) { updateMonitorThumbs(data.items); },
    audio_meters(data) { updateAudioMeters(data.items); },
    audio_device_changed(data) {
      const banner = document.getElementById('audioRefreshBanner');
      if (banner) banner.classList.toggle('show', !!data.pending);
    },
    audio_sources_refreshed(data) {
      renderSources('mic', 'micList', 'micCount', data.mics);
      renderSources('spk', 'spkList', 'spkCount', data.speakers);
      const banner = document.getElementById('audioRefreshBanner');
      if (banner) banner.classList.remove('show');
      // silent=true vem do load inicial do app; toast so faz sentido
      // pra hot-plug subsequente (user plugou/desplugou dispositivo).
      if (!data.silent)
        Toast.show(T('toast.devicesUpdated'),
          formatDeviceChanges(data.changes),
          { warn: true, ttl: 4500 });
      updateRecordButtonAvailability();
    },
    monitor_changed(data) {
      const banner = document.getElementById('monitorRefreshBanner');
      if (banner) banner.classList.toggle('show', !!data.pending);
    },
    refresh_busy(data) {
      if (data.busy) {
        const msg = data.what === 'monitors' ? T('toast.updatingMonitors')
                  : data.what === 'audio'    ? T('toast.updatingAudio')
                  : data.what === 'starting' ? T('toast.startingRecord')
                  : T('toast.updating');
        showLoading(msg);
      } else {
        hideLoading();
      }
    },
    monitors_refreshed(data) {
      Displays.monitors = data.monitors || [];
      Displays.render();
      const banner = document.getElementById('monitorRefreshBanner');
      if (banner) banner.classList.remove('show');
      // So mostra toast se houve mudanca real (lista de changes nao vazia)
      // — refresh pode ter sido disparado por WM_DISPLAYCHANGE sem
      // mudanca visivel (ex.: troca de orientacao reportada como evento).
      const changes = data.changes || [];
      if (changes.length > 0)
        Toast.show(T('toast.monitorsUpdated'),
          formatDeviceChanges(changes),
          { warn: true, ttl: 4500 });
      updateRecordButtonAvailability();
    },
    webcams_refreshed(data) {
      Displays.webcams = data.webcams || [];
      Displays.render();
      // Mesmo titulo que audio_sources_refreshed pra que o dedup do
      // Toast.show coalesce as duas notificacoes quando hot-plug USB
      // dispara refresh de audio + webcam ao mesmo tempo.
      const changes = data.changes || [];
      if (changes.length > 0)
        Toast.show(T('toast.devicesUpdated'),
          formatDeviceChanges(changes),
          { warn: true, ttl: 4500 });
      updateRecordButtonAvailability();
    },
    recording_meta(data) { updateRecordingMeta(data); },
    play_pending(data) { Player.showPending(data.id); },
    play_url(data) { Player.play(data.url, data.name, data.mode, data.id); },
    video_info(data) { Player.renderInfo(data); },
    audio_tracks_ready(data) { Player.onAudioTracksReady(data); },
    waveform_ready(data) { Waveform.onReady(data); },
    encoder_caps(data) { Settings.applyEncoderCaps(data); },
    recording_state(data) { applyRecordingState(data.active, data.elapsed); },
    // Sinal cedo do backend ao comecar HandleRecordStop — tocamos o som
    // de parada AGORA em vez de esperar o flush dos buffers do MKV.
    // Cobre o caso da hotkey/tray, onde o UI nao passa pelo toggleRecord.
    recording_stopping() {
      if (Settings && Settings.currentPlaySoundOnRecord)
        RecordingSounds.playStop();
    },
    recording_added(data) {
      if (typeof data.freeBytes === 'number') _lastFreeBytes = data.freeBytes;
      addRecordingCard(data.item);
    },
    recording_renamed(data) { renameRecordingCard(data.oldId, data.newId, data.newName); },
    recording_removed(data) {
      if (typeof data.freeBytes === 'number') _lastFreeBytes = data.freeBytes;
      removeRecordingCard(data.id);
    },
    error(data) {
      console.error('Bridge error:', data.message);
      // Toast nao-bloqueante. loadingText nao serve porque o loading
      // some logo apos o erro (recording_state=false volta o estado
      // normal) e a mensagem desaparece com ele.
      Toast.show(T('toast.errorTitle'), data.message || T('toast.unknownError'),
                 { warn: true, ttl: 8000 });
    }
  }
};

// =====================================================================
// Render: sources (monitor / mic / speaker)
// =====================================================================
function updateRecordingMeta(data) {
  if (!data || !data.id) return;
  const card = document.querySelector(`#recGrid .rec-card[data-id="${CSS.escape(data.id)}"]`);
  if (!card) return;
  const thumb = card.querySelector('.thumb');
  if (!thumb) return;

  // Thumb: substitui o placeholder "▶" por <img>. textContent=''
  // apagaria o .rec-check / .duration; entao removemos so o placeholder.
  if (data.thumb) {
    let img = thumb.querySelector('img');
    if (!img) {
      const ph = thumb.querySelector('.thumb-placeholder');
      if (ph) ph.remove();
      img = document.createElement('img');
      img.alt = '';
      img.className = 'thumb-img';
      thumb.insertBefore(img, thumb.firstChild);
    }
    if (img.src !== data.thumb) img.src = data.thumb;
  }

  // Duracao: cria/atualiza badge.
  if (data.duration && data.duration > 0) {
    let dur = thumb.querySelector('.duration');
    if (!dur) {
      dur = document.createElement('span');
      dur.className = 'duration';
      thumb.appendChild(dur);
    }
    dur.textContent = formatDuration(data.duration);
  }
}

// Converte amplitude linear 0..1 pra largura 0..1 numa curva dB-friendly.
// -60dB vira 0%, 0dB vira 100% (clamp). Silencio absoluto = 0.
function ampToBar(v) {
  if (!v || v <= 0) return 0;
  const db = 20 * Math.log10(v);
  return Math.max(0, Math.min(1, (db + 60) / 60));
}

function updateAudioMeters(items) {
  if (!items) return;
  items.forEach(it => {
    const card = document.querySelector(`.source-item[data-id="${CSS.escape(it.id)}"]`);
    if (!card) return;
    const meter = card.querySelector('.source-meter');
    if (!meter) return;
    const ch = (it.channels && it.channels >= 2) ? 2 : 1;
    meter.dataset.channels = String(ch);

    const fillL = meter.querySelector('.source-meter-track[data-ch="l"] .source-meter-fill');
    const fillR = meter.querySelector('.source-meter-track[data-ch="r"] .source-meter-fill');

    // Backend manda left/right separados. Fallback: usa level.
    const vL = ampToBar(typeof it.left  === 'number' ? it.left  : it.level);
    const vR = ampToBar(typeof it.right === 'number' ? it.right : it.level);

    // Cover desliza da esquerda (sinal zero = cobre tudo) pra direita
    // (sinal 1.0 = cover colapsado, gradient inteiro visivel).
    if (fillL) fillL.style.left = (vL * 100) + '%';
    if (fillR) fillR.style.left = (vR * 100) + '%';
  });
}

function updateMonitorThumbs(items) {
  if (!items || !items.length) return;
  Displays.updateThumbs(items);
}

// =====================================================================
// Render: Displays (monitores + webcams num layout visual proporcional)
// =====================================================================
const Displays = {
  monitors: [],
  webcams:  [],
  _observer: null,

  init() {
    const root = document.getElementById('displayLayout');
    if (!root || this._observer || typeof ResizeObserver === 'undefined') return;
    // Re-renderiza quando o container muda de tamanho (maximizar,
    // resize de janela, mudanca de aside). Sem isso os rects ficam
    // calculados no tamanho antigo e desalinhados.
    let raf = 0;
    this._observer = new ResizeObserver(() => {
      // rAF coalesce — evita render em rajada durante o drag de resize.
      if (raf) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => { raf = 0; this.render(); });
    });
    this._observer.observe(root);
  },

  render() {
    const root = document.getElementById('displayLayout');
    if (!root) return;
    root.innerHTML = '';

    const monW = (m) => Number(m.width)  || 0;
    const monH = (m) => Number(m.height) || 0;
    const monX = (m) => Number(m.x) || 0;
    const monY = (m) => Number(m.y) || 0;

    if (this.monitors.length === 0 && this.webcams.length === 0) {
      this._updateCount();
      return;
    }

    // 1. Bounding dos monitores em coordenadas do desktop.
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    this.monitors.forEach(m => {
      const x = monX(m), y = monY(m), w = monW(m), h = monH(m);
      if (x < minX) minX = x;
      if (y < minY) minY = y;
      if (x + w > maxX) maxX = x + w;
      if (y + h > maxY) maxY = y + h;
    });
    if (!isFinite(minX)) { minX = 0; minY = 0; maxX = 0; maxY = 0; }

    // 2. Posiciona webcams a direita do bounding dos monitores, em y=0,
    //    empilhadas horizontalmente. Reflete o que BuildRecordingScene
    //    faz com elas no canvas final.
    const camRects = [];
    let camX = maxX;
    this.webcams.forEach(c => {
      const w = Number(c.width)  || 1280;
      const h = Number(c.height) || 720;
      camRects.push({ cam: c, x: camX, y: 0, w, h });
      camX += w;
    });
    if (camRects.length > 0) {
      if (camX > maxX) maxX = camX;
      // y=0 ja esta dentro de [minY, maxY]; altura da cam pode
      // ultrapassar maxY se monitores fossem menores — improvavel,
      // mas trata por seguranca.
      camRects.forEach(r => { if (r.y + r.h > maxY) maxY = r.y + r.h; });
    }

    const totalW = Math.max(1, maxX - minX);
    const totalH = Math.max(1, maxY - minY);
    const boxW = root.clientWidth  || 360;
    const boxH = root.clientHeight || 200;
    const pad = 8; // margem interna pra rects nao colarem na borda
    const availW = boxW - pad * 2;
    const availH = boxH - pad * 2;
    // Escala uniforme pra manter aspect ratio e caber dentro.
    const scale = Math.min(availW / totalW, availH / totalH);
    // Centraliza o conteudo no box.
    const offsetX = pad + (availW - totalW * scale) / 2;
    const offsetY = pad + (availH - totalH * scale) / 2;

    // 3. Cria rect pra cada monitor.
    this.monitors.forEach(m => {
      const rect = document.createElement('div');
      rect.className = 'display-rect';
      rect.dataset.type = 'monitor';
      rect.dataset.id = m.id;
      rect.dataset.kind = 'monitor';
      rect.dataset.enabled = m.enabled ? 'true' : 'false';
      rect.style.left   = (offsetX + (monX(m) - minX) * scale) + 'px';
      rect.style.top    = (offsetY + (monY(m) - minY) * scale) + 'px';
      rect.style.width  = (monW(m) * scale) + 'px';
      rect.style.height = (monH(m) * scale) + 'px';
      // <img> filho — preview ao vivo. updateThumbs so troca o src.
      const img = document.createElement('img');
      img.className = 'display-thumb';
      img.alt = '';
      if (m.thumb) img.src = m.thumb;
      rect.appendChild(img);
      const label = document.createElement('div');
      label.className = 'display-label';
      label.textContent = m.name || T('display.monitor');
      rect.appendChild(label);
      this._appendCheck(rect);
      rect.onclick = () => this._toggle(rect, m, 'monitor');
      root.appendChild(rect);
    });

    // 4. Cria rect pra cada webcam.
    camRects.forEach(r => {
      const c = r.cam;
      const rect = document.createElement('div');
      rect.className = 'display-rect';
      rect.dataset.type = 'webcam';
      rect.dataset.id = c.id;
      rect.dataset.kind = 'webcam';
      rect.dataset.enabled = c.enabled ? 'true' : 'false';
      rect.style.left   = (offsetX + (r.x - minX) * scale) + 'px';
      rect.style.top    = (offsetY + (r.y - minY) * scale) + 'px';
      rect.style.width  = (r.w * scale) + 'px';
      rect.style.height = (r.h * scale) + 'px';
      // Icone de camera no centro.
      const icon = document.createElementNS('http://www.w3.org/2000/svg','svg');
      icon.setAttribute('class', 'display-cam-icon');
      icon.setAttribute('viewBox', '0 0 24 24');
      icon.setAttribute('fill', 'none');
      icon.setAttribute('stroke', 'rgba(255,255,255,0.85)');
      icon.setAttribute('stroke-width', '2');
      icon.setAttribute('stroke-linecap', 'round');
      icon.setAttribute('stroke-linejoin', 'round');
      icon.innerHTML =
        '<path d="M23 7l-7 5 7 5V7z"/>' +
        '<rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>';
      rect.appendChild(icon);
      const label = document.createElement('div');
      label.className = 'display-label';
      label.textContent = c.name || T('display.webcam');
      label.style.position = 'absolute';
      label.style.bottom = '4px';
      label.style.left = '4px';
      rect.appendChild(label);
      this._appendCheck(rect);
      rect.onclick = () => this._toggle(rect, c, 'webcam');
      root.appendChild(rect);
    });

    this._updateCount();
  },

  // Atualiza so o background-image dos rects de monitor existentes —
  // chamado pelo timer do backend a cada 1s pra mostrar preview ao
  // vivo sem reconstruir o DOM (que causaria flicker).
  updateThumbs(items) {
    const root = document.getElementById('displayLayout');
    if (!root) return;
    const byId = {};
    root.querySelectorAll('.display-rect').forEach(r => {
      byId[r.dataset.id] = r;
    });
    items.forEach(it => {
      const m = this.monitors.find(x => x.id === it.id);
      const rect = byId[it.id];
      if (!rect) return;
      // thumb vazia = backend sinalizou que o monitor sumiu (desplug
      // durante gravacao). Remove o <img> e o cache pra mostrar
      // o placeholder/fundo preto em vez da ultima imagem capturada.
      if (!it.thumb) {
        if (m) m.thumb = '';
        const stale = rect.querySelector('img.display-thumb');
        if (stale) stale.remove();
        return;
      }
      if (m) m.thumb = it.thumb;
      let img = rect.querySelector('img.display-thumb');
      if (!img) {
        img = document.createElement('img');
        img.className = 'display-thumb';
        img.alt = '';
        // <img> primeiro na ordem do DOM pra ficar atras de label/check.
        rect.insertBefore(img, rect.firstChild);
      }
      if (img.src !== it.thumb) img.src = it.thumb;
    });
  },

  _appendCheck(rect) {
    const check = document.createElement('div');
    check.className = 'display-check';
    check.innerHTML = CHECK_SVG;
    rect.appendChild(check);
  },

  _toggle(rect, item, kind) {
    const newEnabled = rect.dataset.enabled !== 'true';
    rect.dataset.enabled = newEnabled ? 'true' : 'false';
    item.enabled = newEnabled;
    Bridge.send('toggle_source', {
      kind: kind,
      id: item.id,
      enabled: newEnabled
    });
    this._updateCount();
    updateRecordButtonAvailability();
  },

  _updateCount() {
    const total = this.monitors.length + this.webcams.length;
    const sel = this.monitors.filter(m => m.enabled).length
              + this.webcams.filter(c => c.enabled).length;
    const el = document.getElementById('monCount');
    if (el) el.textContent = `${sel} / ${total}`;

    // Resolucao total = bounding dos itens habilitados (mesma logica
    // do BuildRecordingScene/canvas). Layout side-by-side: largura =
    // soma das larguras, altura = max das alturas. Mostra "—" se nada
    // selecionado.
    //
    // Quando a resolucao agregada excede o limite do codec atual,
    // injeta um icone de aviso (triangulo laranja) com tooltip via
    // <title> explicando que o canvas vai ser reduzido.
    const meta = document.getElementById('monMeta');
    if (meta) {
      let w = 0, h = 0;
      this.monitors.forEach(m => {
        if (!m.enabled) return;
        w += Number(m.width)  || 0;
        const mh = Number(m.height) || 0;
        if (mh > h) h = mh;
      });
      this.webcams.forEach(c => {
        if (!c.enabled) return;
        w += Number(c.width)  || 0;
        const ch = Number(c.height) || 0;
        if (ch > h) h = ch;
      });
      if (w > 0 && h > 0) {
        let html = `${w}×${h}`;
        const codec = (Settings && Settings.currentCodec) || 'auto';
        const caps = (Settings && Settings.encoderCaps) || {};
        const maxDim = codecMaxDim(codec, caps);
        if (w > maxDim || h > maxDim) html += codecLimitWarningIcon(maxDim);
        meta.innerHTML = html;
      } else {
        meta.textContent = '';
      }
    }
  }
};

// Atribuicao de tracks e calculada exclusivamente no Delphi
// (LibOBSEngine.ComputeAudioTrackAssignments). UI recebe item.track ja
// pronto em audio_sources_refreshed e renderiza direto. Sem
// re-implementacao duplicada aqui — single source of truth.

// Rebuild da legenda a partir do DOM. data-track ja vem aplicado
// no render via item.track do JSON.
function refreshTrackColors() {
  buildTrackLegend();
}

// Monta a legenda das cores no rodape da sidebar. Le data-track dos
// dispositivos ja renderizados, agrupa por numero de track e mostra
// so o que tiver presente. Track 1 (mix) nao aparece — devices nunca
// tem data-track="1" (mix nao colore o bar).
//
// Quando varios devices compartilham a mesma track (grouping ativado
// porque excedeu 5 sources), mostra contagem em vez de listar nomes.
function buildTrackLegend() {
  const legend = document.getElementById('trackLegend');
  if (!legend) return;

  // Coleta info por track: nomes, se contem default, kind (mic/spk).
  // trackInfo[t] = { names: [...], hasDefault: bool, kinds: Set }
  const trackInfo = new Map();
  const disabledNames = [];
  ['micList', 'spkList'].forEach(listId => {
    const list = document.getElementById(listId);
    if (!list) return;
    const kindShort = listId === 'micList' ? 'mic' : 'spk';
    list.querySelectorAll('.source-item').forEach(el => {
      const nameEl = el.querySelector('.source-name');
      const name = nameEl ? (nameEl.textContent || '').trim() : '';
      if (!name) return;
      const t = parseInt(el.dataset.track || '0', 10);
      const isDefault = el.dataset.default === 'true' &&
                        el.classList.contains('selected');
      if (t >= 2) {
        if (!trackInfo.has(t))
          trackInfo.set(t, { names: [], hasDefault: false, kinds: new Set() });
        const info = trackInfo.get(t);
        info.names.push(name);
        info.kinds.add(kindShort);
        if (isDefault) info.hasDefault = true;
      } else if (!el.classList.contains('selected')) {
        // Sem track + nao selecionado = desabilitado pelo user, fora
        // de qualquer faixa de gravacao.
        disabledNames.push(name);
      }
    });
  });

  const hasDefault = document.querySelector(
    '#micList .source-default-dot, #spkList .source-default-dot');

  if (trackInfo.size === 0 && disabledNames.length === 0 && !hasDefault) {
    legend.hidden = true;
    legend.innerHTML = '';
    return;
  }

  // Categoriza tracks pela ordem que o user pediu:
  //   1. Padrão (item explicativo)
  //   2. Faixa 1 (Mix)
  //   3. Tracks com dispositivo padrão (mics e outputs misturados)
  //   4. Tracks de microfone (sem default)
  //   5. Tracks de output (sem default, inclui agrupadas)
  //   6. Sem faixa (item explicativo)
  const allTracks = [...trackInfo.keys()].sort((a, b) => a - b);
  const defaultTracks = allTracks.filter(t => trackInfo.get(t).hasDefault);
  const micTracks = allTracks.filter(t =>
    !trackInfo.get(t).hasDefault &&
    trackInfo.get(t).kinds.has('mic') &&
    !trackInfo.get(t).kinds.has('spk'));
  const spkTracks = allTracks.filter(t =>
    !trackInfo.get(t).hasDefault &&
    trackInfo.get(t).kinds.has('spk'));

  function renderTrack(t) {
    const info = trackInfo.get(t);
    const names = info.names;
    let label;
    if (names.length === 1) label = names[0];
    else if (names.length === 2) label = names.join(', ');
    else label = T('tracks.devicesGrouped_other', { count: names.length });
    return '<div class="track-legend-item" data-track="' + t + '" data-hint="' +
           escapeHtml(names.join(', ')) + '">' +
           '<span class="track-legend-color"></span>' +
           '<span class="track-legend-name">' +
           '<span class="track-legend-track">' + T('tracks.track', { n: t }) + '</span>' +
           escapeHtml(label) +
           '</span></div>';
  }

  let html = '<div class="track-legend-title">' + T('tracks.title') + '</div>';

  // 1. Padrão (explicativo) — quando ha device default visivel.
  if (hasDefault) {
    html += '<div class="track-legend-item" data-track="default" ' +
            'data-hint="' + escapeHtml(T('tracks.defaultHint')) + '">' +
            '<span class="track-legend-color"></span>' +
            '<span class="track-legend-name">' +
            '<span class="track-legend-track">' + T('tracks.default') + '</span>' +
            escapeHtml(T('tracks.defaultDevice')) +
            '</span></div>';
  }
  // 2. Faixa 1 (Mix) — quando ha alguma track ativa.
  if (allTracks.length > 0) {
    html += '<div class="track-legend-item" data-track="mix" ' +
            'data-hint="' + escapeHtml(T('tracks.mixHint')) + '">' +
            '<span class="track-legend-color"></span>' +
            '<span class="track-legend-name">' +
            '<span class="track-legend-track">' + T('tracks.mix') + '</span>' +
            escapeHtml(T('tracks.mixLabel')) +
            '</span></div>';
  }
  // 3. Tracks dos dispositivos padrão.
  defaultTracks.forEach(t => { html += renderTrack(t); });
  // 4. Tracks de microfone (sem default).
  micTracks.forEach(t => { html += renderTrack(t); });
  // 5. Tracks de output (sem default).
  spkTracks.forEach(t => { html += renderTrack(t); });
  // 6. Sem faixa (desabilitados).
  if (disabledNames.length > 0) {
    const label = disabledNames.length === 1
      ? disabledNames[0]
      : T('tracks.devicesDisabled_other', { count: disabledNames.length });
    html += '<div class="track-legend-item" data-track="off" data-hint="' +
            escapeHtml(disabledNames.join(', ')) + '">' +
            '<span class="track-legend-color"></span>' +
            '<span class="track-legend-name">' +
            '<span class="track-legend-track">' + T('tracks.noTrack') + '</span>' +
            escapeHtml(label) +
            '</span></div>';
  }
  legend.innerHTML = html;
  legend.hidden = false;
}

function renderSources(kindShort, listId, countId, items) {
  const list = document.getElementById(listId);
  list.innerHTML = '';
  const arr = items || [];
  // Tracks sao re-calculados a partir do estado enabled lido do DOM
  // (refreshTrackColors no fim). Suporta toggles em tempo real.
  arr.forEach((item, idx) => {
    const div = document.createElement('div');
    div.className = 'source-item' + (item.enabled ? ' selected' : '');
    div.dataset.id = item.id;
    if (item.isDefault) div.dataset.default = 'true';
    div.dataset.kind = kindShort === 'mon' ? 'monitor'
                     : kindShort === 'mic' ? 'mic'
                     : kindShort === 'cam' ? 'webcam' : 'speaker';
    // Track-color bar pro lado esquerdo (so mics e speakers). O numero
    // vem direto do JSON — computado no Delphi (single source of truth).
    if ((kindShort === 'mic' || kindShort === 'spk') &&
        typeof item.track === 'number' && item.track > 0) {
      div.dataset.track = String(item.track);
    }
    div.onclick = () => onToggleSource(div);

    if (item.thumb) {
      const thumb = document.createElement('div');
      thumb.className = 'source-thumb';
      const img = document.createElement('img');
      img.src = item.thumb;
      img.alt = '';
      thumb.appendChild(img);
      div.appendChild(thumb);
    }

    const meta = document.createElement('div');
    meta.className = 'source-meta';
    const name = document.createElement('div');
    name.className = 'source-name';
    if (item.isDefault) {
      const dot = document.createElement('span');
      dot.className = 'source-default-dot';
      dot.title = T('sources.defaultDevice');
      name.appendChild(dot);
    }
    // Dispositivos Bluetooth ganham dois icones:
    //   1) Logo BT em azul — informativo, "isto eh um device Bluetooth"
    //   2) Triangulo ambar — sinaliza o problema de qualidade HFP, hint
    //      detalhado explica a limitacao (texto varia pra mic vs spk).
    // Ambos com vertical-align: middle (mesma estrategia da bolinha
    // default) pra centralizar com o nome.
    if (item.isBluetooth) {
      const bt = document.createElement('span');
      bt.className = 'source-bt-icon';
      bt.setAttribute('data-hint', T('sources.btDevice'));
      bt.innerHTML =
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
        'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M6.5 6.5l11 11L12 23V1l5.5 5.5-11 11"/></svg>';
      name.appendChild(bt);

      const warn = document.createElement('span');
      warn.className = 'source-bt-warn';
      warn.setAttribute('data-hint', (kindShort === 'mic')
        ? T('sources.btMicWarn') : T('sources.btSpeakerWarn'));
      // Mesmo SVG do LOW_DISK_ICON: triangulo com ! interno.
      warn.innerHTML =
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
        'stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>' +
        '<line x1="12" y1="9" x2="12" y2="13"/>' +
        '<line x1="12" y1="17" x2="12.01" y2="17"/></svg>';
      name.appendChild(warn);
    }
    name.appendChild(document.createTextNode(item.name || item.id));
    meta.appendChild(name);
    if (item.info) {
      const info = document.createElement('div');
      info.className = 'source-info';
      info.textContent = item.info;
      meta.appendChild(info);
    }
    if (kindShort === 'mic' || kindShort === 'spk') {
      // Estrutura L+R sempre — atributo data-channels controla layout:
      // "1" esconde R e estende L; "2" mostra os dois lado a lado.
      // updateAudioMeters seta channels conforme o que o WASAPI reporta.
      const meter = document.createElement('div');
      meter.className = 'source-meter';
      meter.dataset.channels = '2';
      const trackL = document.createElement('div');
      trackL.className = 'source-meter-track';
      trackL.dataset.ch = 'l';
      const fillL = document.createElement('div');
      fillL.className = 'source-meter-fill';
      trackL.appendChild(fillL);
      const trackR = document.createElement('div');
      trackR.className = 'source-meter-track';
      trackR.dataset.ch = 'r';
      const fillR = document.createElement('div');
      fillR.className = 'source-meter-fill';
      trackR.appendChild(fillR);
      meter.appendChild(trackL);
      meter.appendChild(trackR);
      meta.appendChild(meter);
    }

    const tg = document.createElement('div');
    tg.className = 'toggle';

    div.appendChild(meta);
    div.appendChild(tg);
    list.appendChild(div);
  });
  updateCount(listId, countId);
  // Re-aplica cores de track em ambas as listas — garante que mudou
  // de contagem (mic ou spk) atualize tracks ja desenhados na outra.
  refreshTrackColors();
}

function onToggleSource(el) {
  el.classList.toggle('selected');
  const enabled = el.classList.contains('selected');
  Bridge.send('toggle_source', {
    kind: el.dataset.kind,
    id: el.dataset.id,
    enabled
  });
  const section = el.closest('.section');
  const countEl = section.querySelector('.count');
  if (countEl) {
    const total = section.querySelectorAll('.source-item').length;
    const sel = section.querySelectorAll('.source-item.selected').length;
    countEl.textContent = `${sel} / ${total}`;
  }
  // Toggle de mic/speaker reorganiza tracks (agrupamento por numero
  // de enabled). O recalculo acontece no Delphi: HandleToggleSource
  // empurra audio_sources_refreshed silencioso → UI re-renderiza com
  // os novos data-track. Aqui so atualiza visual local imediato.
  updateRecordButtonAvailability();
}

function updateCount(listId, countId) {
  const list = document.getElementById(listId);
  const total = list.querySelectorAll('.source-item').length;
  const sel = list.querySelectorAll('.source-item.selected').length;
  document.getElementById(countId).textContent = `${sel} / ${total}`;
}

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

function cssEscape(s) { return String(s).replace(/["\\]/g, '\\$&'); }

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
    const newName = el.textContent.replace(/[\r\n]+/g, ' ').trim();
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

// =====================================================================
// Gravacao (start/stop)
// =====================================================================
// Onda radial vermelha que sai do centro do botao de gravar e cobre
// a tela inteira. Disparada apenas ao INICIAR gravacao (nao no stop)
// — feedback visual imediato pra o click. Independe do backend
// confirmar, entao se a gravacao falhar o efeito ja rolou (toast de
// erro aparece logo depois explicando).
function triggerRecordRipple() {
  const btn = document.getElementById('recordBtn');
  if (!btn) {
    Bridge.send('ui_log', { message: 'ripple: recordBtn nao encontrado' });
    return;
  }
  const r = btn.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  const cy = r.top  + r.height / 2;

  const dx = Math.max(cx, window.innerWidth  - cx);
  const dy = Math.max(cy, window.innerHeight - cy);
  const maxRadius = Math.hypot(dx, dy);
  // Anel posicionado em ~94% do raio (94px no nao-escalado).
  // scale = maxRadius / 94 pra anel chegar ate o canto + 1 de margem.
  const scale = (maxRadius / 94) + 1;

  Bridge.send('ui_log', { message:
    `ripple: cx=${cx.toFixed(0)} cy=${cy.toFixed(0)} maxR=${maxRadius.toFixed(0)} scale=${scale.toFixed(2)}` });

  const ripple = document.createElement('div');
  ripple.className = 'record-ripple';
  ripple.style.left = cx + 'px';
  ripple.style.top  = cy + 'px';
  document.body.appendChild(ripple);

  // Web Animations API — mais explicito que CSS transition/animation.
  // Sem depender de reflow timing nem de var() em scale() (que tinha
  // problemas em algumas versoes do WebView2). Os keyframes recebem
  // o scale calculado direto como valor literal.
  if (typeof ripple.animate === 'function') {
    const anim = ripple.animate(
      [
        { transform: 'translate(-50%, -50%) scale(0)',   opacity: 1 },
        { transform: 'translate(-50%, -50%) scale(' + scale.toFixed(2) + ')', opacity: 0 }
      ],
      {
        duration: 1800,
        easing:   'cubic-bezier(0.16, 1, 0.3, 1)',
        fill:     'forwards'
      }
    );
    anim.onfinish = () => { if (ripple.parentNode) ripple.remove(); };
  } else {
    // Fallback ultra-defensivo — WebView2 modernos tem WAAPI. Aqui so
    // pra nao deixar o elemento orfao se Algo Estranho™ acontecer.
    Bridge.send('ui_log', { message: 'ripple: WAAPI indisponivel, removendo' });
    setTimeout(() => ripple.remove(), 100);
  }
}

function toggleRecord() {
  const isRecording = document.body.classList.contains('recording');
  // Ripple so no start. No stop nao faz sentido visualmente — a UI
  // ja sinaliza fim removendo o pulse vermelho.
  if (!isRecording) triggerRecordRipple();
  // Som de parada preemptivo — toca imediato ao clicar, em vez de
  // esperar o backend terminar Engine.StopRecording (que pode levar
  // centenas de ms flushing buffers). O backend tambem manda um
  // recording_stopping, mas o debounce em RecordingSounds.playStop
  // evita disparar duas vezes.
  if (isRecording && Settings && Settings.currentPlaySoundOnRecord)
    RecordingSounds.playStop();
  Bridge.send(isRecording ? 'record_stop' : 'record_start');
}

// =====================================================================
// Loading overlay
// =====================================================================
function showLoading(message) {
  const ov = document.getElementById('loadingOverlay');
  const tx = document.getElementById('loadingText');
  if (tx) {
    tx.innerHTML = '<span></span><span class="loading-dots"></span>';
    tx.firstChild.textContent = message || T('common.loading');
  }
  if (ov) ov.classList.remove('hidden');
}
function hideLoading() {
  const ov = document.getElementById('loadingOverlay');
  if (ov) ov.classList.add('hidden');
}

// =====================================================================
// Hint (tooltip custom — substitui o `title` nativo)
//
// Como usar:
//   <button data-hint="Texto explicativo aqui">Botão</button>
//
// Comportamento:
//   - Aparece ~400ms apos o mouse parar sobre o elemento.
//   - Some imediato quando o mouse sai (ou scroll/blur/mousedown).
//   - Auto-posiciona acima/abaixo do alvo conforme caiba na viewport.
//   - Tema (dark/light) herdado do <html data-theme="...">.
// =====================================================================
const Hint = {
  el: null,
  showTimer: 0,
  current: null,  // elemento sob o cursor que disparou o hint
  SHOW_DELAY_MS: 400,

  init() {
    this.el = document.getElementById('hint');
    if (!this.el) return;
    document.addEventListener('mouseover',  (e) => this._onOver(e));
    document.addEventListener('mouseout',   (e) => this._onOut(e));
    document.addEventListener('mousedown',  ()  => this.hide());
    document.addEventListener('scroll',     ()  => this.hide(), true);
    window.addEventListener('blur',         ()  => this.hide());
    window.addEventListener('resize',       ()  => this.hide());
  },

  _onOver(e) {
    const target = e.target.closest('[data-hint]');
    if (!target) return;
    if (target === this.current) return;
    this.current = target;
    clearTimeout(this.showTimer);
    const text = target.getAttribute('data-hint');
    if (!text) return;
    this.showTimer = setTimeout(() => this._show(target, text), this.SHOW_DELAY_MS);
  },

  _onOut(e) {
    const target = e.target.closest('[data-hint]');
    if (!target) return;
    // Ignora se o mouse foi pra um descendente do mesmo target —
    // continuamos "dentro" do hint area.
    if (e.relatedTarget && target.contains(e.relatedTarget)) return;
    this.current = null;
    clearTimeout(this.showTimer);
    this.hide();
  },

  _show(target, text) {
    if (!this.el) return;
    this.el.textContent = text;
    // Torna visivel pra medir tamanho real, mas sem flash — opacity 0
    // ate posicionarmos.
    this.el.style.visibility = 'hidden';
    this.el.classList.add('visible');
    const hintRect = this.el.getBoundingClientRect();
    const rect = target.getBoundingClientRect();
    const pad = 6;
    const margin = 6;
    // Default: abaixo, centralizado horizontalmente.
    let top  = rect.bottom + pad;
    let left = rect.left + (rect.width - hintRect.width) / 2;
    // Se vai estourar pra baixo, posiciona acima.
    if (top + hintRect.height + margin > window.innerHeight)
      top = rect.top - hintRect.height - pad;
    // Clamp horizontal.
    if (left < margin) left = margin;
    if (left + hintRect.width > window.innerWidth - margin)
      left = window.innerWidth - hintRect.width - margin;
    // Clamp vertical (caso ambos os lados nao caibam).
    if (top < margin) top = margin;
    this.el.style.left = left + 'px';
    this.el.style.top  = top  + 'px';
    this.el.style.visibility = '';
  },

  hide() {
    clearTimeout(this.showTimer);
    if (this.el) this.el.classList.remove('visible');
    this.current = null;
  }
};

// =====================================================================
// RecordingSounds — feedback sonoro de inicio/fim de gravacao via
// Web Audio API (sem arquivos de audio externos).
//
// Notas escolhidas: C5 (523.25 Hz) + G5 (783.99 Hz) — intervalo de
// quinta justa, agradavel e nao-disruptivo. Inicio toca C5→G5 (sensacao
// ascendente / "abrir"); fim toca G5→C5 (descendente / "fechar").
// Envelope ADSR rapido (5ms attack, 50ms decay pra 70%, release linear)
// pra evitar clicks audiveis e dar uma sensacao "natural" de bell tone.
// Volume final 0.12 (de 1.0) — audivel mas nao incomoda.
//
// AudioContext eh lazy-init na primeira chamada — o autoplay policy do
// Chromium pode suspender ate o user interagir com a pagina, mas como
// gravacao so dispara via click no botao OU hotkey global (que tambem
// conta como interacao pelo WebView2), na pratica funciona desde a 1a.
// =====================================================================
const RecordingSounds = {
  ctx: null,
  // Timestamp do ultimo playStop pra debounce — clicar no botao toca
  // preemptivamente, e o backend depois manda recording_stopping; sem
  // debounce, sound tocaria duas vezes sobreposto.
  _lastStopMs: 0,

  _ensureContext() {
    if (!this.ctx) {
      try {
        const AC = window.AudioContext || window.webkitAudioContext;
        if (!AC) return null;
        this.ctx = new AC();
      } catch (e) {
        return null;
      }
    }
    if (this.ctx.state === 'suspended') {
      try { this.ctx.resume(); } catch (e) {}
    }
    return this.ctx;
  },

  _playNote(freq, startTime, duration, volume) {
    const ctx = this.ctx;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = 'sine';
    osc.frequency.value = freq;
    // ADSR: 0→vol em 5ms, decay pra 70% em 45ms, release linear ate 0.
    gain.gain.setValueAtTime(0, startTime);
    gain.gain.linearRampToValueAtTime(volume,        startTime + 0.005);
    gain.gain.linearRampToValueAtTime(volume * 0.70, startTime + 0.050);
    gain.gain.linearRampToValueAtTime(0,             startTime + duration);
    osc.connect(gain).connect(ctx.destination);
    osc.start(startTime);
    osc.stop(startTime + duration + 0.02);
  },

  // C5 → G5 — ascendente, sensacao de inicio/abertura.
  playStart() {
    const ctx = this._ensureContext();
    if (!ctx) return;
    const t0 = ctx.currentTime;
    this._playNote(523.25, t0,        0.16, 0.12);
    this._playNote(783.99, t0 + 0.10, 0.20, 0.12);
  },

  // G5 → C5 — descendente, sensacao de fim/fechamento.
  // Debounce de 800ms — se o stop veio do click no botao, ja tocamos
  // preemptivamente; o recording_stopping do backend chegaria logo
  // depois e tocaria de novo sem o debounce.
  playStop() {
    const now = Date.now();
    if (now - this._lastStopMs < 800) return;
    this._lastStopMs = now;
    const ctx = this._ensureContext();
    if (!ctx) return;
    const t0 = ctx.currentTime;
    this._playNote(783.99, t0,        0.16, 0.12);
    this._playNote(523.25, t0 + 0.10, 0.20, 0.12);
  }
};

// =====================================================================
// Waveform — barras de onda sonora abaixo do seek bar do player.
//
// Arquitetura: backend (Delphi) extrai os peaks da 1a faixa de audio
// via libav em worker thread (~500ms-2s pra gravacao tipica) e manda
// pra UI como JSON. Sem fetch/decode no JS, evitando os problemas de
// mixed content (UI em https vs HTTP local) e CORS (origens diferentes
// entre subdomains do virtual host).
//
// Fluxo:
//   1) Player.play() seta currentRecId + dispara request_waveform.
//   2) Backend computa peaks via FFmpegOps.ComputeAudioPeaks em thread
//      separada — nao bloqueia main nem video playback.
//   3) waveform_ready chega com array de N floats em [0..1] — Waveform
//      so renderiza.
//   4) Cacheado por recId no JS — proxima abertura da mesma gravacao,
//      render instantaneo sem novo round-trip pro backend.
//
// Custo runtime: zero. Barras sao DOM estatico painted pelo compositor.
// =====================================================================
const Waveform = {
  cache: new Map(),     // recId → array de peaks (resolucao de armazenamento)
  currentRecId: null,
  // Resolucao de ARMAZENAMENTO (nao de exibicao). O backend decodifica o
  // audio uma vez e compacta nesta quantidade; o cache guarda isso. Na
  // hora de renderizar, _render faz downsample pra largura REAL em pixels
  // (PX_PER_BAR), entao a densidade visual fica consistente em qualquer
  // duracao e qualquer largura (janela/fullscreen) SEM recomputar. 2000 e
  // mais que qualquer seek bar em px, entao nunca falta detalhe na tela.
  BUCKETS: 2000,
  // Largura alvo de cada barra (barra + gap) em px. ~2px da o aspecto de
  // waveform continuo estilo SoundCloud.
  PX_PER_BAR: 2,

  // Chamado em Player.play quando troca de gravacao. Se ja temos peaks
  // em cache, renderiza instantaneo. Senao dispara request_waveform.
  reset(recId) {
    this.currentRecId = recId;
    const el = document.getElementById('playerWaveform');
    if (!el) return;
    el.innerHTML = '';
    if (recId && this.cache.has(recId)) {
      this._render(this.cache.get(recId));
    } else {
      el.hidden = true;
    }
  },

  // Backend manda waveform_ready. Cacheamos e renderizamos.
  onReady(data) {
    if (!data || !data.id || !Array.isArray(data.peaks)) return;
    this.cache.set(data.id, data.peaks);
    if (this.currentRecId === data.id) this._render(data.peaks);
  },

  _render(storedPeaks) {
    const el = document.getElementById('playerWaveform');
    if (!el || !storedPeaks || storedPeaks.length === 0) return;
    // Mostra ANTES de medir: o elemento usa [hidden] (display:none), e
    // display:none zera clientWidth. O overlay do player e opacity/
    // visibility (sempre no layout), entao com o waveform visivel a
    // largura ja reflete o seek bar.
    el.hidden = false;
    // Downsample da resolucao de ARMAZENAMENTO (BUCKETS) pra quantidade de
    // barras que cabem na largura REAL do elemento — densidade visual
    // consistente em qualquer duracao/largura.
    const peaks = this._downsampleToWidth(storedPeaks, el);
    // Normaliza pelo pico maximo (ocupa altura total) pra audio
    // baixo tambem mostrar barras visiveis.
    let maxPeak = 0;
    for (let i = 0; i < peaks.length; i++)
      if (peaks[i] > maxPeak) maxPeak = peaks[i];
    if (maxPeak < 0.0001) maxPeak = 0.0001;

    const widthPct = 100 / peaks.length;
    // Bar width = 88% do slot (deixa um gap minimo entre barras).
    const barWidth = widthPct * 0.88;
    // Curva gamma (pow 0.5 = sqrt) na razao peak/max — amplifica
    // pequenas variacoes de amplitude que sao tipicas em audio com
    // AGC ou compressao. Sem isso, RMS na maioria das gravacoes (fala
    // razoavelmente uniforme) gera barras quase iguais. Com gamma 0.5:
    //   ratio 0.10 → bar 32% (era 10%)
    //   ratio 0.50 → bar 71% (era 50%)
    //   ratio 1.00 → bar 100% (igual)
    // Altura minima 4% pra silencios totais terem linha visivel.
    let html = '';
    for (let i = 0; i < peaks.length; i++) {
      const ratio = peaks[i] / maxPeak;
      const h = Math.max(4, Math.sqrt(ratio) * 100);
      html += '<div class="wf-bar" style="' +
        'left:' + (i * widthPct).toFixed(3) + '%;' +
        'width:' + barWidth.toFixed(3) + '%;' +
        'height:' + h.toFixed(1) + '%' +
        '"></div>';
    }
    el.innerHTML = html;
    el.hidden = false;
  },

  // Agrega o array guardado (max por slot) pro numero de barras que cabem
  // na largura atual. Max-of-max e idempotente, entao nao perde picos. Se
  // a tela comporta mais barras que o stored, usa o stored como esta (nao
  // da pra inventar detalhe).
  _downsampleToWidth(stored, el) {
    let w = el.clientWidth || el.offsetWidth || 0;
    if (w <= 0) w = 800;  // ainda sem layout: fallback razoavel
    const target = Math.max(1, Math.floor(w / this.PX_PER_BAR));
    if (target >= stored.length) return stored;
    const out = new Array(target);
    for (let i = 0; i < target; i++) {
      const start = Math.floor(i * stored.length / target);
      let end = Math.floor((i + 1) * stored.length / target);
      if (end <= start) end = start + 1;
      let m = 0;
      for (let j = start; j < end; j++) if (stored[j] > m) m = stored[j];
      out[i] = m;
    }
    return out;
  },

  // Re-renderiza a partir do cache pra largura nova (resize/fullscreen).
  // NAO recomputa no backend — so re-agrega o array ja guardado.
  relayout() {
    if (!this.currentRecId) return;
    const el = document.getElementById('playerWaveform');
    if (!el || el.hidden) return;
    const peaks = this.cache.get(this.currentRecId);
    if (peaks) this._render(peaks);
  },

  hide() {
    this.currentRecId = null;
    const el = document.getElementById('playerWaveform');
    if (el) { el.hidden = true; el.innerHTML = ''; }
  }
};

// Re-agrega o waveform pra nova largura em resize/fullscreen (debounce).
// So age se o player estiver aberto (relayout checa currentRecId/hidden).
window.addEventListener('resize', () => {
  clearTimeout(Waveform._resizeTimer);
  Waveform._resizeTimer = setTimeout(() => Waveform.relayout(), 120);
});

// =====================================================================
// Toast (notificacao nao-bloqueante)
// =====================================================================
const Toast = {
  show(title, msg, opts) {
    const stack = document.getElementById('toastStack');
    if (!stack) return;
    const ttl = (opts && opts.ttl) || 6000;
    // Dedup: se ja existe um toast com mesmo titulo visivel, renova
    // o TTL em vez de criar um novo (evita empilhar duas notificacoes
    // identicas quando hot-plug USB dispara refresh de audio + webcam).
    const existing = stack.querySelector('.toast[data-title="' +
      CSS.escape(title || '') + '"]');
    if (existing) {
      if (existing._dismissTimer) clearTimeout(existing._dismissTimer);
      existing._dismissTimer = setTimeout(() => {
        existing.classList.remove('show');
        setTimeout(() => existing.remove(), 220);
      }, ttl);
      return;
    }
    const el = document.createElement('div');
    el.className = 'toast' + (opts && opts.warn ? ' warn' : '');
    el.dataset.title = title || '';
    if (title) {
      const t = document.createElement('div');
      t.className = 'toast-title';
      t.textContent = title;
      el.appendChild(t);
    }
    if (msg) {
      const m = document.createElement('div');
      m.className = 'toast-msg';
      m.textContent = msg;
      el.appendChild(m);
    }
    stack.appendChild(el);
    requestAnimationFrame(() => el.classList.add('show'));
    el._dismissTimer = setTimeout(() => {
      el.classList.remove('show');
      setTimeout(() => el.remove(), 220);
    }, ttl);
  }
};

// =====================================================================
// Confirm modal (generico)
// =====================================================================
const Confirm = {
  _onOk: null,
  init() {
    document.getElementById('confirmCancel').onclick = () => this.close();
    document.getElementById('confirmOk').onclick = () => {
      const cb = this._onOk;
      this.close();
      if (cb) cb();
    };
    document.getElementById('confirmOverlay').addEventListener('click', (e) => {
      if (e.target.id === 'confirmOverlay') this.close();
    });
    document.addEventListener('keydown', (e) => {
      const ov = document.getElementById('confirmOverlay');
      if (!ov.classList.contains('visible')) return;
      if (e.key === 'Escape') { this.close(); e.preventDefault(); }
      else if (e.key === 'Enter') {
        const cb = this._onOk;
        this.close();
        if (cb) cb();
        e.preventDefault();
      }
    });
  },
  open(opts) {
    const { title, message, okLabel, cancelLabel, danger, onOk } = opts || {};
    document.getElementById('confirmTitle').textContent = title || T('common.confirm');
    document.getElementById('confirmMsg').textContent   = message || '';
    document.getElementById('confirmOk').textContent    = okLabel || T('common.delete');
    document.getElementById('confirmCancel').textContent = cancelLabel || T('common.cancel');
    const ok = document.getElementById('confirmOk');
    ok.classList.toggle('danger', danger !== false);
    this._onOk = onOk || null;
    document.getElementById('confirmOverlay').classList.add('visible');
  },
  close() {
    document.getElementById('confirmOverlay').classList.remove('visible');
    this._onOk = null;
  }
};

// =====================================================================
// Settings modal
// =====================================================================
// Teclas principais disponiveis no dropdown de atalho. Os nomes batem
// EXATAMENTE com o que OBSHotkey.ParseHotkey/NameToVk espera no Delphi.
// Mudancas aqui precisam de mudanca correspondente la. Modificadores
// (Ctrl/Shift/Alt/Win) sao checkboxes a parte, fora desta lista.
// labelKey (nao label): o nome do grupo e resolvido via T() no render do
// dropdown (em _buildHotkeyDropdown), pois esta const e avaliada antes
// do bundle de i18n chegar do backend.
const HOTKEY_KEY_GROUPS = [
  { labelKey: 'settings.hotkey.groups.function', keys: [
      'F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12']
  },
  { labelKey: 'settings.hotkey.groups.letters', keys: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('') },
  { labelKey: 'settings.hotkey.groups.numbers', keys: '0123456789'.split('') },
  { labelKey: 'settings.hotkey.groups.special', keys: [
      'Space','Enter','Tab','Esc','Insert','Delete','Home','End',
      'PageUp','PageDown','Backspace','Pause/Break']
  },
  { labelKey: 'settings.hotkey.groups.arrows', keys: ['Left','Right','Up','Down'] },
  { labelKey: 'settings.hotkey.groups.numpad', keys: [
      'Numpad0','Numpad1','Numpad2','Numpad3','Numpad4','Numpad5',
      'Numpad6','Numpad7','Numpad8','Numpad9',
      'NumpadAdd','NumpadSubtract','NumpadMultiply','NumpadDivide','NumpadDecimal']
  },
  { labelKey: 'settings.hotkey.groups.chars', keys: ['-','=',',','.',';','/','`','[','\\',']',"'"] },
  // Teclas de midia (controle de reproducao em teclados multimidia). Aqui
  // cada item e um objeto { k, labelKey } em vez de string: o VALOR (k) e o
  // nome canonico que o backend entende ('MediaPlayPause' etc.), e o LABEL
  // visivel e traduzido via labelKey (com simbolo). Normalmente usadas SEM
  // modificador.
  { labelKey: 'settings.hotkey.groups.media', keys: [
      { k: 'MediaPlayPause', labelKey: 'settings.hotkey.media.playPause' },
      { k: 'MediaStop',      labelKey: 'settings.hotkey.media.stop' },
      { k: 'MediaPrev',      labelKey: 'settings.hotkey.media.prev' },
      { k: 'MediaNext',      labelKey: 'settings.hotkey.media.next' },
  ]},
];
const HOTKEY_MODIFIERS = ['Ctrl','Shift','Alt','Win'];
function isHotkeyModifier(k) { return HOTKEY_MODIFIERS.includes(k); }

// Validacao de hotkey e centralizada no backend (OBSHotkey.IsReservedHotkey +
// HandleValidateHotkey em OBSBridge). Frontend manda 'validate_hotkey' e
// espera 'hotkey_validation_result' { hotkey, ok, reason }. O backend e
// a single source of truth — lista de combinacoes reservadas nao se
// duplica aqui.
let _pendingHotkeyValidation = null;
function validateHotkeyWithBackend(hotkey) {
  return new Promise((resolve) => {
    // Cancela validacao anterior (se o user clicou Salvar duas vezes).
    if (_pendingHotkeyValidation) _pendingHotkeyValidation({ ok: true, reason: '' });
    _pendingHotkeyValidation = resolve;
    Bridge.send('validate_hotkey', { hotkey });
    // Timeout safety: se o backend nao responder em 2s, deixa passar
    // (fail-open). Evita travar a UI se algo der errado.
    setTimeout(() => {
      if (_pendingHotkeyValidation === resolve) {
        _pendingHotkeyValidation = null;
        resolve({ ok: true, reason: '' });
      }
    }, 2000);
  });
}

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
    const minLbl = document.getElementById('settingsFpsMinLabel');
    if (minLbl) minLbl.textContent = this.fpsPresets[0] + ' fps';
    const maxLbl = document.getElementById('settingsFpsMaxLabel');
    if (maxLbl)
      maxLbl.textContent = this.fpsPresets[this.fpsPresets.length - 1] + ' fps';
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
        if (typeof key === 'string')
          og.appendChild(this._makeOpt(key, key));
        else
          og.appendChild(this._makeOpt(key.k, T(key.labelKey)));
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

// =====================================================================
// Player modal
// =====================================================================
const Player = {
  pendingId: null,
  currentId: null,
  currentMode: null,    // 'direct' | 'transcoded'
  triedTranscode: false,
  // Estado do zoom/pan
  zoom: 1,
  panX: 0,
  panY: 0,
  dragging: false,
  dragStartX: 0,
  dragStartY: 0,
  dragStartPanX: 0,
  dragStartPanY: 0,
  // Velocidade de execucao do video — sincronizada com os audio slaves
  // via 'ratechange' listener (applyRateAudios).
  playbackSpeed: 1,
  init() {
    const v   = document.getElementById('playerVideo');
    const ov  = document.getElementById('playerOverlay');
    const seek = document.getElementById('playerSeek');
    const vol  = document.getElementById('playerVolume');

    // Render inicial do slider master — sem isso o gradient nasce
    // sem --vp/--vp-mid (CSS fallback = 0%, bar fica toda cinza ate
    // o 1o play() chamar updateVolUi).
    setVolBarVars(vol, parseInt(vol.value, 10));

    document.getElementById('playerClose').onclick = () => Player.close();
    ov.addEventListener('click', (e) => { if (e.target === ov) Player.close(); });

    document.getElementById('playerPlay').onclick = () => Player.togglePlay();
    document.getElementById('playerMute').onclick = () => Player.toggleMute();
    document.getElementById('playerInfo').onclick = () => Player.toggleInfoPanel();
    document.getElementById('playerInfoClose').onclick = () => Player.closeInfoPanel();
    document.getElementById('playerFs').onclick = () => Player.toggleFullscreen();

    // Speed selector: botao toggla menu; click numa opcao aplica
    // playbackRate e fecha. Click fora do wrap fecha sem aplicar.
    const speedBtn  = document.getElementById('playerSpeedBtn');
    const speedMenu = document.getElementById('playerSpeedMenu');
    speedBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      const open = !speedMenu.hidden;
      speedMenu.hidden = open;
      speedBtn.classList.toggle('active', !open);
    });
    speedMenu.querySelectorAll('.player-speed-option').forEach(opt => {
      opt.addEventListener('click', (e) => {
        e.stopPropagation();
        const rate = parseFloat(opt.dataset.speed);
        Player.setPlaybackSpeed(rate);
        speedMenu.hidden = true;
        speedBtn.classList.remove('active');
      });
    });
    // Click em qualquer outro lugar fecha o menu.
    document.addEventListener('click', () => {
      if (!speedMenu.hidden) {
        speedMenu.hidden = true;
        speedBtn.classList.remove('active');
      }
    });

    v.addEventListener('timeupdate', () => Player.onTimeUpdate());
    v.addEventListener('loadedmetadata', () => Player.onTimeUpdate());
    v.addEventListener('play',  () => { Player.updatePlayIcon(); Player.startTimeRaf(); Player.updatePauseOverlay(); });
    v.addEventListener('pause', () => { Player.updatePlayIcon(); Player.stopTimeRaf(); Player.updatePauseOverlay(); });
    v.addEventListener('ended', () => { Player.updatePlayIcon(); Player.stopTimeRaf(); Player.updatePauseOverlay(); });
    v.addEventListener('volumechange', () => Player.updateVolUi());
    v.addEventListener('canplay', () => {
      document.getElementById('playerLoading').classList.remove('visible');
    });
    v.addEventListener('error', () => {
      // Se o arquivo original nao tocou (codec nao suportado pelo
      // WebView2, tipico HEVC), pede transcode pro Delphi e tenta de
      // novo. So tenta uma vez por sessao do player.
      if (Player.currentMode === 'direct' && !Player.triedTranscode &&
          Player.currentId) {
        Player.triedTranscode = true;
        const ld = document.getElementById('playerLoading');
        const ldt = document.getElementById('playerLoadingText');
        if (ld) ld.classList.add('visible');
        if (ldt) ldt.textContent = T('error.unsupportedCodec');
        Bridge.send('request_transcode', { id: Player.currentId });
        return;
      }
      const t = document.getElementById('playerLoadingText');
      if (t) t.textContent = T('error.loadVideo');
      const ld = document.getElementById('playerLoading');
      if (ld) ld.classList.add('visible');
    });

    seek.addEventListener('input', () => {
      if (!isFinite(v.duration)) return;
      v.currentTime = (seek.value / 1000) * v.duration;
    });
    vol.addEventListener('input', () => {
      // Slider value e 0..200; masterVolume vira 0..2.0 (1.0 = "natural"
      // 100%, 2.0 = boost maximo). AudioContext as vezes nasce suspenso
      // ate o 1o user gesture — esse input event JA conta como gesture,
      // mas resume() e seguro chamar idempotente.
      Player.masterVolume = vol.value / 100;
      Player.masterMuted = Player.masterVolume === 0;
      Player.ensureAudioGraph();
      Player.applyVolumes();
      Player.updateVolUi();
    });
    // Tooltip + dblclick=reset → 100% (compartilhado com os sliders
    // per-faixa via VolTooltip.wire).
    VolTooltip.wire(vol);

    // Sync audio elements ao seek/play/pause/ratechange do video.
    v.addEventListener('seeked', () => Player.syncAudios(true));
    v.addEventListener('play',  () => Player.syncAudios(false));
    v.addEventListener('pause', () => Player.pauseAudios());
    v.addEventListener('ratechange', () => Player.applyRateAudios());

    document.addEventListener('keydown', (e) => {
      if (!ov.classList.contains('visible')) return;
      if (e.key === 'Escape') {
        // ESC em fullscreen: sai do fullscreen, nao fecha o player.
        if (document.body.dataset.fs === '1') {
          Player.toggleFullscreen();
          e.preventDefault();
          return;
        }
        Player.close();
        e.preventDefault();
      }
      else if (e.key === ' ') { Player.togglePlay(); e.preventDefault(); }
      else if (e.key === 'm' || e.key === 'M') { Player.toggleMute(); }
      else if (e.key === 'f' || e.key === 'F') { Player.toggleFullscreen(); }
      else if (e.key === '0') { Player.resetZoom(); }
      else if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        // Pausado: avanca/retrocede QUADRO A QUADRO. NoOBS grava a
        // 30fps por padrao (OBSEngine.fps_num=30), entao 1/30s ≈ 1
        // frame. Pra videos vindos de fora com framerate diferente
        // (60fps, etc.) o passo nao bate exato em 1 frame, mas
        // browser snap pra frame mais proxima — UX continua sendo
        // "uma seta = um passo discreto pequeno".
        //
        // Tocando: pula 5s (atalho clássico de scrubbing). Mantem a
        // distincao porque user pausado quer precisao (revisar frame
        // exato), user tocando quer navegacao rapida.
        //
        // Audio slaves sincronizam via o listener 'seeked' que ja
        // existe (syncAudios(true) forca todos pro novo currentTime).
        const dir  = (e.key === 'ArrowRight') ? +1 : -1;
        const step = v.paused ? (1/30) : 5;
        const dur  = v.duration || 0;
        const next = (v.currentTime || 0) + dir * step;
        v.currentTime = Math.max(0, Math.min(next, dur));
        e.preventDefault();
      }
    });

    // Zoom: scroll do mouse no stage (sem ctrl). O bloqueio global de
    // ctrl+wheel nao afeta isso porque la so prevDefault se ctrlKey.
    const stage = document.getElementById('playerStage');
    stage.addEventListener('wheel', (e) => Player.onWheel(e), { passive: false });
    stage.addEventListener('mousedown', (e) => Player.onDragStart(e));
    stage.addEventListener('mousemove', (e) => Player.onDragMove(e));
    stage.addEventListener('mouseup',   ()  => Player.onDragEnd());
    stage.addEventListener('mouseleave',()  => Player.onDragEnd());
    // Click no video = play/pause; dblclick = fullscreen.
    // Defer de 250ms no click pra dar tempo do dblclick chegar e
    // cancelar (sem isso, dblclick pausaria e despausaria o video).
    stage.addEventListener('click', (e) => Player.onStageClick(e));
    stage.addEventListener('dblclick', () => {
      if (Player.clickTimer) {
        clearTimeout(Player.clickTimer);
        Player.clickTimer = null;
      }
      Player.toggleFullscreen();
    });

    // Auto-hide de header + controles apos 2s sem mover o mouse.
    // Em fullscreen + cursor proximo as bordas (top/left/right) o
    // delay cai pra 1s — esconde rapido quando user "parqueia" o
    // mouse na lateral.
    const player = document.querySelector('.player');
    if (player) {
      player.addEventListener('mousemove', (e) => Player.markActive(e));
      player.addEventListener('mouseleave', () => Player.scheduleIdle(200));
    }
    // Pausar/dar play tambem reseta o estado (em pause, mostra sempre).
    v.addEventListener('play',  () => Player.markActive());
    v.addEventListener('pause', () => Player.markActive());
  },

  // --- Auto-hide de controles ---
  idleTimer: null,
  EDGE_MARGIN_PX: 40,  // distancia das bordas que conta como "edge zone"
  markActive(evt) {
    const player = document.querySelector('.player');
    if (!player) return;
    player.classList.remove('idle');
    if (this.idleTimer) { clearTimeout(this.idleTimer); this.idleTimer = null; }
    const v = document.getElementById('playerVideo');
    // Pausado mantem visivel — so agenda esconder se esta tocando.
    if (!v || v.paused || v.ended) return;

    // Em fullscreen, se o cursor esta proximo das bordas (top/left/right),
    // usa delay curto (1s) — usuario provavelmente "parqueou" o mouse
    // na lateral e nao quer mais ver controles.
    let delay = 2000;
    if (document.body.dataset.fs === '1' && evt) {
      const r = player.getBoundingClientRect();
      const m = this.EDGE_MARGIN_PX;
      const nearTop   = (evt.clientY - r.top)    <= m;
      const nearLeft  = (evt.clientX - r.left)   <= m;
      const nearRight = (r.right - evt.clientX)  <= m;
      if (nearTop || nearLeft || nearRight) delay = 1000;
    }
    this.scheduleIdle(delay);
  },
  scheduleIdle(ms) {
    if (this.idleTimer) clearTimeout(this.idleTimer);
    this.idleTimer = setTimeout(() => {
      const player = document.querySelector('.player');
      const v = document.getElementById('playerVideo');
      // Re-checa estado no fire — pode ter pausado no meio do timer.
      if (player && v && !v.paused && !v.ended) player.classList.add('idle');
    }, ms);
  },

  // --- Click no stage: play/pause (defer 250ms pra deixar dblclick passar) ---
  clickTimer: null,
  onStageClick(e) {
    // Se o mouse arrastou (pan quando zoom > 1), nao toggle play/pause.
    // dragStartX/Y sao registrados no onDragStart.
    if (this.zoom > 1.001) {
      const dx = Math.abs(e.clientX - this.dragStartX);
      const dy = Math.abs(e.clientY - this.dragStartY);
      if (dx > 5 || dy > 5) return;
    }
    if (this.clickTimer) clearTimeout(this.clickTimer);
    this.clickTimer = setTimeout(() => {
      this.clickTimer = null;
      this.togglePlay();
    }, 250);
  },

  toggleFullscreen() {
    // Pede pro Delphi mudar a JANELA HOST pra borderless ocupando a
    // tela inteira. Fullscreen API do WebView2 so afetaria a area do
    // WebView dentro da janela, nao a janela em si.
    Bridge.send('toggle_fullscreen', {});
    // Mantem CSS local em sync pra responsividade visual (header sem
    // border-radius, etc.) — alternamos um data-attr no <body>.
    const entering = document.body.dataset.fs !== '1';
    document.body.dataset.fs = entering ? '1' : '';
    // Troca o icone do botao (4 setas pra fora -> pra dentro).
    const ic = document.getElementById('playerFsIcon');
    if (ic) {
      ic.innerHTML = entering
        ? '<path d="M5 16h3v3h2v-5H5v2zm3-8H5v2h5V5H8v3zm6 11h2v-3h3v-2h-5v5zm2-11V5h-2v5h5V8h-3z"/>'
        : '<path d="M7 14H5v5h5v-2H7v-3zm-2-4h2V7h3V5H5v5zm12 7h-3v2h5v-5h-2v3zM14 5v2h3v3h2V5h-5z"/>';
    }
  },

  applyTransform() {
    const v = document.getElementById('playerVideo');
    if (!v) return;
    const tf = `translate(${this.panX}px, ${this.panY}px) scale(${this.zoom})`;
    v.style.transform = tf;
    // Em modo composite (2+ regioes), o video esta escondido e o canvas
    // e que aparece — aplicar o mesmo transform pra zoom/pan funcionarem
    // visualmente.
    const c = document.getElementById('playerComposite');
    if (c) c.style.transform = tf;
    const stage = document.getElementById('playerStage');
    const badge = document.getElementById('playerZoomBadge');
    if (stage) stage.classList.toggle('zoomed', this.zoom > 1.001);
    if (badge) badge.textContent = Math.round(this.zoom * 100) + '%';
  },

  clampPan() {
    // Quando zoom > 1, evita que o video saia totalmente do stage.
    const stage = document.getElementById('playerStage');
    if (!stage) return;
    const r = stage.getBoundingClientRect();
    const w = r.width  * this.zoom;
    const h = r.height * this.zoom;
    // Pan limites: video deve ocupar pelo menos 1/3 do stage de cada lado.
    const minX = Math.min(0, r.width  - w);
    const minY = Math.min(0, r.height - h);
    if (this.panX > 0) this.panX = 0;
    if (this.panY > 0) this.panY = 0;
    if (this.panX < minX) this.panX = minX;
    if (this.panY < minY) this.panY = minY;
  },

  onWheel(e) {
    e.preventDefault();
    const stage = e.currentTarget;
    const r = stage.getBoundingClientRect();
    const cx = e.clientX - r.left;
    const cy = e.clientY - r.top;
    // Ponto na "coordenada do video" antes do zoom.
    const vx = (cx - this.panX) / this.zoom;
    const vy = (cy - this.panY) / this.zoom;
    // Aplica delta — wheel up = zoom in.
    const factor = (e.deltaY < 0) ? 1.15 : 1 / 1.15;
    let z = this.zoom * factor;
    if (z < 1)   z = 1;
    if (z > 8)   z = 8;
    this.zoom = z;
    // Reposiciona pan pra manter o ponto sob o cursor.
    this.panX = cx - vx * this.zoom;
    this.panY = cy - vy * this.zoom;
    this.clampPan();
    this.applyTransform();
  },

  onDragStart(e) {
    if (this.zoom <= 1.001) return;
    if (e.button !== 0) return;
    this.dragging = true;
    this.dragStartX = e.clientX;
    this.dragStartY = e.clientY;
    this.dragStartPanX = this.panX;
    this.dragStartPanY = this.panY;
    document.getElementById('playerStage').classList.add('dragging');
    e.preventDefault();
  },
  onDragMove(e) {
    if (!this.dragging) return;
    this.panX = this.dragStartPanX + (e.clientX - this.dragStartX);
    this.panY = this.dragStartPanY + (e.clientY - this.dragStartY);
    this.clampPan();
    this.applyTransform();
  },
  onDragEnd() {
    if (!this.dragging) return;
    this.dragging = false;
    document.getElementById('playerStage').classList.remove('dragging');
  },

  resetZoom() {
    this.zoom = 1; this.panX = 0; this.panY = 0;
    this.applyTransform();
  },

  // Aplica taxa de reproducao no <video> — listener 'ratechange' propaga
  // pros audio slaves. Atualiza o label do botao e marca a opcao ativa
  // no menu pra refletir o estado atual.
  setPlaybackSpeed(rate) {
    if (!isFinite(rate) || rate <= 0) return;
    this.playbackSpeed = rate;
    const v = document.getElementById('playerVideo');
    if (v) v.playbackRate = rate;
    // Label: 1× / 0,5× / 1,5× — virgula como separador decimal (pt-BR).
    const lbl = document.getElementById('playerSpeedLabel');
    if (lbl) {
      lbl.textContent = (rate === 1)
        ? '1×'
        : String(rate).replace('.', ',') + '×';
    }
    // Marca opcao selecionada no menu.
    document.querySelectorAll('.player-speed-option').forEach(o => {
      o.classList.toggle('selected',
        Math.abs(parseFloat(o.dataset.speed) - rate) < 0.001);
    });
  },

  // Calcula display size do canvas composite preservando aspect ratio
  // dentro do stage (object-fit:contain manual). Chamado quando o
  // composite e (re)montado e em window resize.
  _fitCompositeToStage() {
    const c = document.getElementById('playerComposite');
    const stage = document.getElementById('playerStage');
    if (!c || !stage || c.hidden) return;
    if (!c.width || !c.height) return;
    const r = stage.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return;
    const stageAspect = r.width / r.height;
    const canvasAspect = c.width / c.height;
    let w, h;
    if (canvasAspect > stageAspect) {
      // canvas mais "wide" que stage — limita pela largura
      w = r.width;
      h = r.width / canvasAspect;
    } else {
      // canvas mais "tall" — limita pela altura
      h = r.height;
      w = r.height * canvasAspect;
    }
    c.style.width  = w + 'px';
    c.style.height = h + 'px';
  },
  showPending(id) {
    this.pendingId = id;
    const ov = document.getElementById('playerOverlay');
    const ld = document.getElementById('playerLoading');
    const ldt = document.getElementById('playerLoadingText');
    const v = document.getElementById('playerVideo');
    v.removeAttribute('src'); v.load();
    document.getElementById('playerTitle').textContent = '...';
    if (ldt) ldt.textContent = T('player.loading');
    ld.classList.add('visible');
    ov.classList.add('visible');
    // Avisa o backend pra suspender audio meters e thumb capture
    // enquanto o player estiver visivel — a UI atras esta escondida.
    Bridge.send('player_state', { open: true });
  },
  play(url, name, mode, id) {
    const ov = document.getElementById('playerOverlay');
    // "Fresh open" = overlay nao estava visivel antes desta chamada.
    // Distincto de "video novo" — user pode reabrir o mesmo video
    // (caso B) ou pode dar play em outro card sem fechar (caso C, raro).
    // Tambem distinto de reload pra transcode (caso D, wasVisible=true
    // e id igual) — la NAO queremos resetar nada.
    const wasVisible = ov.classList.contains('visible');
    if (!wasVisible) ov.classList.add('visible');
    // Idempotente — caso play() seja chamado sem showPending antes
    // (backend pode pular pending pra videos ja prontos no cache).
    Bridge.send('player_state', { open: true });
    const changed = (this.currentId !== id);
    if (changed) {
      this.triedTranscode = false;
      this.infoLoaded = null;
      // Reseta multi-track audio do video anterior.
      this.stopDriftCheck();
      this.audioEls.forEach(a => {
        try { a.pause(); a.src=''; a.load(); } catch(e){}
        try { if (a._noobs_source) a._noobs_source.disconnect(); } catch(e){}
        try { if (a._noobs_gain)   a._noobs_gain.disconnect();   } catch(e){}
        a._noobs_source = null;
        a._noobs_gain   = null;
      });
      this.audioEls = [];
      this.audiosRequested = false;
      this.audiosReadyForId = null;
      this.trackVolumes = null;
      // Reseta seletor de visualizacao — gravacao nova abre em "Tela
      // cheia" por padrao mesmo se a anterior estava em zoom.
      this.currentLayout = null;
      this.selectedRegionIndex = -1;
      this.applyRegionView();
    }
    // Reset do MASTER em TODA abertura fresh (mesmo reabrindo o
    // mesmo video). User pediu: "ao abrir um video sempre comecar
    // no 100%". Reload pra transcode (wasVisible=true) NAO reseta —
    // a recuperacao do video nao deve perder a configuracao do user.
    if (!wasVisible || changed) {
      this.masterVolume = 1.0;
      this.masterMuted = false;
      this.applyVolumes();
    }
    this.currentId = id;
    this.currentMode = mode || 'direct';
    this.pendingId = null;
    // Se o painel de info ja esta aberto e o video mudou, recarrega.
    if (changed &&
        document.getElementById('playerInfoPanel').classList.contains('open'))
      this.requestInfo();
    const v = document.getElementById('playerVideo');
    document.getElementById('playerTitle').textContent = name || '';
    // crossOrigin DEVE vir ANTES do src: o video master recebe attachGain
    // (MediaElementAudioSourceNode -> GainNode) quando o slider de volume
    // entra em cena. Sem crossOrigin="anonymous", o elemento fica "tainted"
    // por CORS e o GainNode produz SILENCIO. O servidor HTTP local retorna
    // Access-Control-Allow-Origin: * (ver OBSPlayer.HandleGet), entao o
    // load nao quebra. Mesma logica usada nos slaves de audio das tracks.
    v.crossOrigin = 'anonymous';
    v.src = url;
    v.load();
    // Waveform: reseta state. Se cacheado, renderiza instantaneo;
    // senao dispara request_waveform em background (defer pra video
    // ter prioridade na thread).
    if (changed) {
      Waveform.reset(id);
      if (!Waveform.cache.has(id)) {
        setTimeout(() => {
          if (Player.currentId === id)
            Bridge.send('request_waveform',
              { id: id, buckets: Waveform.BUCKETS });
        }, 100);
      }
    }
    // Esconde indicador de pause durante o load (evita flash se o video
    // antigo havia ficado pausado). Tira tambem a classe .flash do
    // central pra cancelar uma animacao em andamento — sem isso, abrir
    // outro video no meio do flash deixaria o pulso continuar fora de
    // contexto. Eventos play/pause atualizam dai.
    const _ind = document.getElementById('playerPauseIndicator');
    if (_ind) _ind.classList.remove('flash');
    const _bd  = document.getElementById('playerPauseBadge');
    if (_bd) _bd.classList.remove('visible');
    // Limpa tambem o estado "paused" do container — sem isso, a seek
    // bar nasceria cinza por um frame ate o evento 'play' disparar.
    const _pl = document.querySelector('.player');
    if (_pl) _pl.classList.remove('paused');
    v.play().catch(() => Player.updatePauseOverlay());
    // Sincroniza o icone de volume — o video padrao abre em 1.0 mas
    // nenhum volumechange dispara automaticamente, entao o icone
    // ficava preso no SVG default do HTML.
    this.updateVolUi();
  },
  close() {
    const ov = document.getElementById('playerOverlay');
    const v = document.getElementById('playerVideo');
    try { v.pause(); } catch(e) {}
    v.removeAttribute('src');
    v.load();
    ov.classList.remove('visible');
    this.pendingId = null;
    this.currentId = null;
    this.currentMode = null;
    this.triedTranscode = false;
    this.closeInfoPanel();
    this.resetZoom();
    // Reseta selecao de monitor/regiao + layout cacheado + flag de info
    // carregada. Sem isso, abrir o mesmo (ou outro) video em seguida
    // herda a selecao anterior na aba "Visualizacao" do painel de info
    // — bug visivel: usuario abre video, escolhe um monitor especifico,
    // fecha, reabre, e o player ja inicia naquela regiao em vez de tela
    // cheia. infoLoaded=null forca o re-render do painel mesmo se for
    // o mesmo video (caso contrario toggleInfoPanel skip o requestInfo
    // pq infoLoaded === currentId, e o DOM mantem o radio do monitor
    // antigo marcado).
    this.selectedRegions = new Set();
    this.currentLayout = null;
    this.infoLoaded = null;
    // Reseta velocidade pra 1x — novo video comeca em ritmo normal
    // mesmo se o ultimo ficou em 2x ou 0.5x.
    this.setPlaybackSpeed(1);
    // Cancela qualquer decode/fetch de waveform em andamento (peaks ja
    // cacheados ficam — proxima vez que abrir essa gravacao, render
    // instantaneo).
    Waveform.hide();
    // Sai de fullscreen se estiver — fechar player em fullscreen
    // deixaria a janela borderless toda preta sem onde clicar.
    if (document.body.dataset.fs === '1') this.toggleFullscreen();
    // Limpa estado de auto-hide (controles visiveis no proximo open).
    if (this.idleTimer) { clearTimeout(this.idleTimer); this.idleTimer = null; }
    if (this.clickTimer) { clearTimeout(this.clickTimer); this.clickTimer = null; }
    this.stopTimeRaf();
    this.stopDriftCheck();
    // Para e descarta audio elements (libera memoria/network).
    // Desconecta MediaElementSource/GainNode antes — sem isso o
    // audioCtx mantem referencias vivas pra cada slave ja criado,
    // acumulando memoria a cada open/close. O <video> persiste (e o
    // mesmo elemento) — nao desconecta o dele.
    this.audioEls.forEach(a => {
      try { a.pause(); a.src=''; a.load(); } catch(e){}
      try { if (a._noobs_source) a._noobs_source.disconnect(); } catch(e){}
      try { if (a._noobs_gain)   a._noobs_gain.disconnect();   } catch(e){}
      a._noobs_source = null;
      a._noobs_gain   = null;
    });
    this.audioEls = [];
    this.audiosRequested = false;
    this.audiosReadyForId = null;
    this.trackVolumes = null;
    const player = document.querySelector('.player');
    if (player) player.classList.remove('idle');
    // Retoma os updates de meter/thumb que tinham sido suspendidos.
    Bridge.send('player_state', { open: false });
  },

  // -- Painel lateral de informacoes (ffprobe) --
  infoLoaded: null,        // id do video pra qual ja temos dados
  infoLoading: false,
  infoRequestedId: null,   // id do ultimo request_video_info (guard anti-loop)
  toggleInfoPanel() {
    const panel = document.getElementById('playerInfoPanel');
    const btn = document.getElementById('playerInfo');
    if (panel.classList.contains('open')) {
      this.closeInfoPanel();
      return;
    }
    panel.classList.add('open');
    panel.setAttribute('aria-hidden', 'false');
    btn.classList.add('active');

    // Se o video atual mudou desde a ultima carga, pede de novo.
    if (this.currentId && this.infoLoaded !== this.currentId) {
      this.requestInfo();
    }
  },
  closeInfoPanel() {
    const panel = document.getElementById('playerInfoPanel');
    const btn = document.getElementById('playerInfo');
    panel.classList.remove('open');
    panel.setAttribute('aria-hidden', 'true');
    if (btn) btn.classList.remove('active');
  },
  requestInfo() {
    if (!this.currentId) return;
    if (this.infoLoading) return;
    this.infoLoading = true;
    this.infoRequestedId = this.currentId;  // guard anti-loop (ver renderInfo)
    const body = document.getElementById('playerInfoBody');
    body.innerHTML =
      '<div class="player-info-loading">' +
      '<div class="spin"></div><span>' + T('player.analyzing') + '</span></div>';
    Bridge.send('request_video_info', { id: this.currentId });
  },
  renderInfo(data) {
    if (!data) return;
    if (data.id !== this.currentId) {
      // Resposta com id != video atual. Libera o lock (senao infoLoading
      // ficaria preso e o painel nunca carregaria).
      this.infoLoading = false;
      // Re-pede SO se o video atual mudou desde o ultimo request (resposta
      // velha de outro video, trocado durante o load). Se o id divergente
      // veio do PROPRIO request atual (ex.: backend devolvendo id de cache
      // desatualizado), NAO re-pede — evitaria um loop apertado.
      if (this.currentId && this.currentId !== this.infoRequestedId &&
          document.getElementById('playerInfoPanel').classList.contains('open'))
        this.requestInfo();
      return;
    }
    this.infoLoading = false;
    this.infoLoaded = data.id;
    const body = document.getElementById('playerInfoBody');

    const videoStreams = (data.streams || []).filter(s => s.kind === 'video');
    const audioStreams = (data.streams || []).filter(s => s.kind === 'audio');

    let html = '';

    // Geral
    html += '<div class="player-info-section">';
    html += '<div class="player-info-section-title">' + T('player.info.file') + '</div>';
    html += infoRow(T('player.info.name'), escapeHtml(data.fileName || ''));
    html += infoRow(T('player.info.container'), escapeHtml(data.format || ''));
    html += infoRow(T('player.info.duration'), formatDuration(Math.round(data.duration || 0)));
    html += infoRow(T('player.info.size'), formatBytes(data.size || 0));
    if (data.bitrate > 0)
      html += infoRow(T('player.info.totalBitrate'), formatBitrate(data.bitrate));
    html += '</div>';

    // Video
    if (videoStreams.length > 0) {
      html += '<div class="player-info-section">';
      html += '<div class="player-info-section-title">' + T('player.info.video') + '</div>';
      videoStreams.forEach((s, idx) => {
        if (videoStreams.length > 1)
          html += '<div class="player-info-track-title">' +
                  T('player.info.stream', { n: idx + 1 }) + '</div>';
        html += infoRow(T('player.info.codec'), escapeHtml((s.codec || '').toUpperCase()));
        if (s.width && s.height)
          html += infoRow(T('player.info.resolution'), s.width + ' × ' + s.height);
        // FPS — so renderiza se conhecido (backend manda 0 quando avg_frame_rate
        // nao foi populado pelo container). NTSC etc. mantem casas decimais
        // (29,97); taxas inteiras (30, 60) ficam sem decimal.
        if (s.frameRate > 0)
          html += infoRow(T('player.info.frameRate'), formatFps(s.frameRate));
        if (s.bitrate > 0)
          html += infoRow(T('player.info.bitrate'), formatBitrate(s.bitrate));
      });
      html += '</div>';
    }

    // Visualizacao — seletor multi-select de monitor / webcam.
    // "Tela cheia" e um radio (limpa tudo). Cada regiao e um checkbox
    // que pode combinar com outras. Layout vem do <hash>.json salvo
    // no fim da gravacao. Gravacoes antigas sem .json: secao nao aparece.
    this.currentLayout = (data.layout && Array.isArray(data.layout.regions) &&
      data.layout.regions.length > 0) ? data.layout : null;
    if (!(this.selectedRegions instanceof Set)) this.selectedRegions = new Set();
    if (this.currentLayout) {
      const regs = this.currentLayout.regions;
      // Limpa indices que nao existem mais (ex.: regravou com menos monitores).
      this.selectedRegions.forEach(i => { if (i >= regs.length) this.selectedRegions.delete(i); });
      const isFullscreen = this.selectedRegions.size === 0;
      html += '<div class="player-info-section">';
      html += '<div class="player-info-section-title">' + T('player.info.visualization') + '</div>';
      // "Tela cheia" — radio. Selecionado quando NAO ha regiao escolhida.
      html += this._renderViewOption(-1, T('player.info.fullView'), 'full',
        isFullscreen, /*isRadio*/ true);
      regs.forEach((r, i) => {
        html += this._renderViewOption(i, r.name || T('player.info.region', { n: i+1 }),
          r.kind || 'monitor',
          this.selectedRegions.has(i), /*isRadio*/ false);
      });
      html += '</div>';
    } else {
      this.selectedRegions.clear();
    }

    // Audio (multi-track) com volume slider por faixa
    if (audioStreams.length > 0) {
      html += '<div class="player-info-section">';
      const audioKey = audioStreams.length === 1
        ? 'player.info.audio_one' : 'player.info.audio_other';
      html += '<div class="player-info-section-title">' +
              T(audioKey, { count: audioStreams.length }) + '</div>';
      // Inicializa volumes (Track 0 = 100, demais = 0) se ainda nao
      // foi feito pra este video.
      if (!Array.isArray(this.trackVolumes) ||
          this.trackVolumes.length !== audioStreams.length) {
        this.trackVolumes = audioStreams.map((_, i) => i === 0 ? 1.0 : 0.0);
      }
      audioStreams.forEach((s, idx) => {
        const volPct = Math.round((this.trackVolumes[idx] || 0) * 100);
        const isMuted = volPct === 0;
        // Usa o title gravado nos metadata se houver; fallback "Faixa N".
        const label = (s.title && s.title.length > 0) ? s.title :
          (idx === 0 ? T('player.info.trackMix', { n: idx + 1 })
                     : T('player.info.track', { n: idx + 1 }));
        html += '<div class="player-info-track' +
                (isMuted ? ' muted' : '') + '" data-track-row="' + idx + '">';
        html += '<div class="player-info-track-title">' +
                escapeHtml(label) + '</div>';
        html += infoRow(T('player.info.codec'), escapeHtml((s.codec || '').toUpperCase()));
        if (s.channels)
          html += infoRow(T('player.info.channels'),
            s.channels === 1 ? T('player.info.channelsMono') :
            s.channels === 2 ? T('player.info.channelsStereo') :
            T('player.info.channelsN', { count: s.channels }));
        if (s.sampleRate)
          html += infoRow(T('player.info.sampleRate'),
            (s.sampleRate / 1000).toFixed(1) + ' kHz');
        if (s.bitrate > 0)
          html += infoRow(T('player.info.bitrate'), formatBitrate(s.bitrate));
        // Slider de volume da faixa + botao de mutar.
        html += '<div class="player-info-vol-row">';
        html += '<button class="player-info-mute' +
                (isMuted ? ' muted' : '') + '" ' +
                'data-track-mute="' + idx + '" ' +
                'title="' + (isMuted ? T('player.unmute') : T('player.muteShort')) + '">' +
                MUTE_ICON_SVG(isMuted) + '</button>';
        html += '<input type="range" class="player-info-vol" ' +
                'data-track="' + idx + '" min="0" max="200" ' +
                'value="' + volPct + '" title="' + T('player.info.trackVolume') + '">';
        html += '<span class="player-info-vol-val" data-track-val="' +
                idx + '">' + volPct + '%</span>';
        html += '</div>';
        html += '</div>';
      });
      html += '</div>';
    }

    body.innerHTML = html;

    // Sliders de volume por faixa — wire-up apos render.
    // Mantem o slider sincronizado com a classe .muted da row pai
    // (quando user arrasta de/pra zero pelo proprio slider, sem clicar
    // no botao de mutar).
    const syncMuteState = (idx, isMuted) => {
      const row    = body.querySelector('[data-track-row="' + idx + '"]');
      const muteBt = body.querySelector('[data-track-mute="' + idx + '"]');
      if (row) row.classList.toggle('muted', isMuted);
      if (muteBt) {
        muteBt.classList.toggle('muted', isMuted);
        muteBt.title = isMuted ? T('player.unmute') : T('player.muteShort');
        muteBt.innerHTML = MUTE_ICON_SVG(isMuted);
      }
    };
    body.querySelectorAll('.player-info-vol').forEach(slider => {
      const idx = parseInt(slider.dataset.track, 10);
      // Render inicial: posiciona --vp/--vp-mid corretamente pro valor
      // que ja veio do trackVolumes[idx] (pode ser 100% no track 0).
      setVolBarVars(slider, parseInt(slider.value, 10));
      slider.addEventListener('input', () => {
        const raw = parseInt(slider.value, 10);  // 0..200
        const vol = raw / 100;                    // 0..2.0
        setVolBarVars(slider, raw);
        const label = body.querySelector(
          '[data-track-val="' + idx + '"]');
        if (label) label.textContent = raw + '%';
        // Garante AudioContext acordado (input event = user gesture).
        Player.ensureAudioGraph();
        Player.setTrackVolume(idx, vol);
        syncMuteState(idx, vol === 0);
        // User arrastando o slider apaga o "volume restaurado" do
        // mute button — proximo unmute via botao volta pra 100, nao
        // pro valor que o user acabou de mexer.
        if (vol > 0) delete slider.dataset.preMute;
      });
      // Tooltip flutuante + dblclick=reset 100%. Idempotente —
      // re-renders do painel re-passam pelos mesmos elementos, mas
      // a flag _noobs_voltt_wired evita listeners duplicados.
      VolTooltip.wire(slider);
    });

    // Botoes de mutar — toggle: muta zera volume guardando o valor
    // anterior em data-preMute; desmuta restaura pra preMute ou 100%.
    body.querySelectorAll('.player-info-mute').forEach(btn => {
      btn.addEventListener('click', (e) => {
        e.preventDefault();
        const idx = parseInt(btn.dataset.trackMute, 10);
        const slider = body.querySelector(
          '.player-info-vol[data-track="' + idx + '"]');
        if (!slider) return;
        const curVol = parseInt(slider.value, 10);
        let nextVol;
        if (curVol > 0) {
          // Mutar: guarda volume atual e zera.
          slider.dataset.preMute = String(curVol);
          nextVol = 0;
        } else {
          // Desmutar: restaura pre-mute, ou 100 se nunca foi setado.
          nextVol = parseInt(slider.dataset.preMute || '100', 10);
          delete slider.dataset.preMute;
        }
        slider.value = String(nextVol);
        setVolBarVars(slider, nextVol);
        const label = body.querySelector(
          '[data-track-val="' + idx + '"]');
        if (label) label.textContent = nextVol + '%';
        Player.setTrackVolume(idx, nextVol / 100);
        syncMuteState(idx, nextVol === 0);
      });
    });

    // Wire-up das opcoes de "Visualizacao". Click no "Tela cheia"
    // (idx=-1) limpa o set. Click numa regiao (idx>=0) toggla — se
    // o set ficar vazio, "Tela cheia" auto-ativa.
    body.querySelectorAll('.player-view-option').forEach(opt => {
      opt.addEventListener('click', () => {
        const idx = parseInt(opt.dataset.viewIdx, 10);
        if (!(Player.selectedRegions instanceof Set))
          Player.selectedRegions = new Set();
        if (idx === -1) {
          Player.selectedRegions.clear();
        } else {
          if (Player.selectedRegions.has(idx)) Player.selectedRegions.delete(idx);
          else Player.selectedRegions.add(idx);
        }
        // Refresca visual dos checks + radio.
        const fullSelected = Player.selectedRegions.size === 0;
        body.querySelectorAll('.player-view-option').forEach(o => {
          const i = parseInt(o.dataset.viewIdx, 10);
          o.classList.toggle('selected',
            i === -1 ? fullSelected : Player.selectedRegions.has(i));
        });
        // Mudou a area visivel — zoom/pan anterior nao faz mais sentido.
        Player.resetZoom();
        Player.applyRegionView();
      });
    });
    // Reaplica view (caso video tenha sido recarregado e o estado
    // precise ser persistido).
    this.applyRegionView();
  },

  // Renderiza um item da lista "Visualizacao". `idx` = -1 pra tela
  // cheia ou 0+ pra index em currentLayout.regions. `kind` controla
  // o icone (monitor / webcam / full). `isRadio` muda o estilo do
  // indicador (circulo vs check).
  _renderViewOption(idx, label, kind, selected, isRadio) {
    const iconSvg =
      kind === 'webcam'  ? '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3.5"/><circle cx="12" cy="12" r="9"/></svg>' :
      kind === 'monitor' ? '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="14" rx="2"/><line x1="8" y1="22" x2="16" y2="22"/><line x1="12" y1="18" x2="12" y2="22"/></svg>' :
      /* full */          '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>';
    return '<div class="player-view-option' +
      (selected ? ' selected' : '') + '" data-view-idx="' + idx + '">' +
      '<span class="player-view-check' + (isRadio ? ' radio' : '') + '"></span>' +
      '<span class="player-view-icon">' + iconSvg + '</span>' +
      '<span class="player-view-label">' + escapeHtml(label) + '</span>' +
      '</div>';
  },

  // Aplica o modo de visualizacao no player. Tres caminhos:
  //   0 regioes  → tela cheia (object-view-box: none, sem canvas)
  //   1 regiao   → object-view-box CSS (nativo, zero overhead)
  //   2+ regioes → composicao via <canvas> RAF (sem buracos entre monitores)
  applyRegionView() {
    const v = document.getElementById('playerVideo');
    const stage = document.getElementById('playerStage');
    if (!v) return;
    const set = this.selectedRegions instanceof Set ? this.selectedRegions : new Set();
    const regs = this.currentLayout ? this.currentLayout.regions : null;

    // Para qualquer mudanca, cancela RAF se rodando (sera recriado abaixo se preciso).
    if (this.compositeRaf) {
      cancelAnimationFrame(this.compositeRaf);
      this.compositeRaf = null;
    }

    if (!regs || set.size === 0) {
      // Modo tela cheia.
      v.style.objectViewBox = '';
      v.style.objectFit = 'contain';
      stage.classList.remove('compositing');
      const c = document.getElementById('playerComposite');
      if (c) c.hidden = true;
      return;
    }

    if (set.size === 1) {
      // Single region — usa object-view-box (nativo).
      const i = set.values().next().value;
      const reg = regs[i];
      const cw = this.currentLayout.canvasW;
      const ch = this.currentLayout.canvasH;
      if (reg && cw && ch) {
        const top    = (reg.y                / ch * 100).toFixed(2);
        const right  = ((cw - reg.x - reg.w) / cw * 100).toFixed(2);
        const bottom = ((ch - reg.y - reg.h) / ch * 100).toFixed(2);
        const left   = (reg.x                / cw * 100).toFixed(2);
        v.style.objectViewBox = `inset(${top}% ${right}% ${bottom}% ${left}%)`;
        v.style.objectFit = 'contain';
      }
      stage.classList.remove('compositing');
      const c1 = document.getElementById('playerComposite');
      if (c1) c1.hidden = true;
      return;
    }

    // 2+ regioes — canvas mode.
    v.style.objectViewBox = '';   // limpa object-view-box do video escondido
    stage.classList.add('compositing');
    const c = document.getElementById('playerComposite');
    if (!c) return;
    c.hidden = false;

    // Ordena por X (left-to-right) pra ficar previsivel.
    const ordered = [...set].map(i => regs[i]).filter(Boolean)
      .sort((a, b) => a.x - b.x);
    // Composite: lado a lado horizontalmente. Largura = soma das ws,
    // altura = max das hs. Monitores menores ficam centralizados verticalmente
    // (letterbox preto natural por causa do clear).
    const sumW = ordered.reduce((s, r) => s + r.w, 0);
    const maxH = ordered.reduce((s, r) => Math.max(s, r.h), 0);
    if (c.width !== sumW)  c.width  = sumW;
    if (c.height !== maxH) c.height = maxH;
    // Letterbox manual: object-fit:contain nao funciona em <canvas>,
    // entao computamos o tamanho display pra preservar aspect dentro do
    // stage. fitCompositeToStage e chamado tambem em resize do window.
    this._fitCompositeToStage();
    const ctx = c.getContext('2d');

    const draw = () => {
      // Se o video ainda nao tem dados suficientes pra desenhar, pula
      // o frame — readyState >= 2 (HAVE_CURRENT_DATA).
      if (v.readyState >= 2) {
        ctx.fillStyle = '#000';
        ctx.fillRect(0, 0, c.width, c.height);
        let dx = 0;
        for (const r of ordered) {
          // Webcam: alinha no topo (dy=0) pra preservar a geometria
          // original da gravacao (webcam costuma ficar no topo do canvas
          // junto ao monitor principal). Monitor: centraliza vertical
          // pra letterbox simetrico quando alturas diferem.
          const dy = (r.kind === 'webcam') ? 0
                                           : Math.round((maxH - r.h) / 2);
          ctx.drawImage(v, r.x, r.y, r.w, r.h, dx, dy, r.w, r.h);
          dx += r.w;
        }
      }
      this.compositeRaf = requestAnimationFrame(draw);
    };
    this.compositeRaf = requestAnimationFrame(draw);
  },

  togglePlay() {
    const v = document.getElementById('playerVideo');
    if (v.paused || v.ended) v.play(); else v.pause();
  },
  updatePlayIcon() {
    const v = document.getElementById('playerVideo');
    const ic = document.getElementById('playerPlayIcon');
    if (v.paused || v.ended)
      ic.innerHTML = '<path d="M8 5v14l11-7z"/>';
    else
      ic.innerHTML = '<path d="M6 5h4v14H6zM14 5h4v14h-4z"/>';
  },
  updatePauseOverlay() {
    // Padrao "YouTube + Netflix":
    //   - Indicador CENTRAL e efemero: pulsa 1.2s no momento do
    //     pause/play e some, confirmando a acao sem bloquear o quadro.
    //   - Badge no canto (top-left) e persistente enquanto pausado,
    //     dando feedback de estado discreto.
    // Re-trigger da animacao precisa de remove → force reflow →
    // re-add (caso contrario o browser nao reinicia a animacao se a
    // classe ja estava la, ex: usuario pausa duas vezes rapido).
    const v   = document.getElementById('playerVideo');
    const ind = document.getElementById('playerPauseIndicator');
    const bd  = document.getElementById('playerPauseBadge');
    if (!v) return;
    const isPaused = v.paused || v.ended;
    if (ind) {
      ind.classList.remove('flash');
      if (isPaused) {
        void ind.offsetWidth; // force reflow → reinicia a animacao
        ind.classList.add('flash');
      }
    }
    if (bd) {
      bd.classList.toggle('visible', isPaused);
    }
    // Classe .paused no container do player → seek bar muda pra cinza
    // (override CSS scoped). Sem isso, a barra continuaria verde
    // durante pause, contradizendo o "estou parado".
    const playerEl = document.querySelector('.player');
    if (playerEl) playerEl.classList.toggle('paused', isPaused);
  },
  toggleMute() {
    this.masterMuted = !this.masterMuted;
    this.applyVolumes();
    this.updateVolUi();
  },
  updateVolUi() {
    const v = document.getElementById('playerVideo');
    const vol = document.getElementById('playerVolume');
    const ic = document.getElementById('playerVolIcon');
    // Master volume e o slider em si — nao mais lido de v.volume
    // (que agora e o produto master * trackVolumes[0]).
    const pct = this.masterMuted ? 0 :
      Math.round((this.masterVolume == null ? 1 : this.masterVolume) * 100);
    vol.value = pct;
    // Slider range 0..200 → --vp e --vp-mid em % da largura (0..100%).
    // helper centralizado pra a mesma logica vir pros per-track sliders.
    setVolBarVars(vol, pct);
    const mute = this.masterMuted || pct === 0;
    if (mute)
      ic.innerHTML = '<path d="M16.5 12A4.5 4.5 0 0 0 14 7.97v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51A8.92 8.92 0 0 0 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06a8.99 8.99 0 0 0 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"/>';
    else if (pct < 50)
      ic.innerHTML = '<path d="M7 9v6h4l5 5V4l-5 5H7zm9.5 3a4.5 4.5 0 0 0-2.5-4.03v8.05A4.5 4.5 0 0 0 16.5 12z"/>';
    else
      ic.innerHTML = '<path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3a4.5 4.5 0 0 0-2.5-4.03v8.05A4.5 4.5 0 0 0 16.5 12zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77S18.01 4.14 14 3.23z"/>';
  },
  onTimeUpdate() {
    const v = document.getElementById('playerVideo');
    const seek = document.getElementById('playerSeek');
    const tm = document.getElementById('playerTime');
    const cur = v.currentTime || 0;
    const dur = isFinite(v.duration) ? v.duration : 0;
    const p = dur > 0 ? (cur / dur) * 1000 : 0;
    seek.value = p;
    seek.style.setProperty('--p', (dur > 0 ? (cur / dur) * 100 : 0) + '%');
    tm.textContent = fmtTime(cur) + ' / ' + fmtTime(dur);
  },

  // -- Atualizacao da barra de progresso via rAF (60fps).
  // O evento 'timeupdate' do video so dispara ~4x/segundo, deixando a
  // barra com aparencia "saltada". rAF interpola direto de
  // video.currentTime que o browser atualiza internamente em alta freq.
  rafId: null,
  startTimeRaf() {
    if (this.rafId) return;
    const tick = () => {
      const v = document.getElementById('playerVideo');
      if (!v || v.paused || v.ended) { this.rafId = null; return; }
      this.onTimeUpdate();
      this.rafId = requestAnimationFrame(tick);
    };
    this.rafId = requestAnimationFrame(tick);
  },
  stopTimeRaf() {
    if (this.rafId) { cancelAnimationFrame(this.rafId); this.rafId = null; }
  },

  // ===== Mixagem multi-track de audio =====
  // Track 1 (mix) = audio nativo do <video>. Tracks 2..N = elementos
  // <audio> sincronizados ao video. Volume final por track =
  // master * trackVolumes[idx]. Audio elements sao criados sob demanda
  // (primeira vez que o user mexe num slider de track > 0).
  trackVolumes: null,           // ex: [1.0, 0, 0, 0, 0]
  masterVolume: 1.0,
  masterMuted: false,
  audioEls: [],                 // <audio> pras tracks 2..N (indice 0 = track 2)
  audiosRequested: false,       // ja pediu extracao pro Delphi?
  audiosReadyForId: null,       // id pra qual ja temos URLs
  driftRaf: null,
  // Seletor de visualizacao (monitor/webcam) — layout vem do .json
  // salvo no fim da gravacao. Set vazio = "tela cheia"; com 1+ indices
  // ativa o modo regiao.
  //   1 indice  → object-view-box (recorte CSS nativo, zero CPU)
  //   2+ indices → composicao via <canvas> + RAF + drawImage (sem buracos)
  currentLayout: null,
  selectedRegions: null,        // Set<number> | null
  compositeRaf: null,           // requestAnimationFrame handle

  setTrackVolume(idx, vol) {
    if (!Array.isArray(this.trackVolumes)) this.trackVolumes = [];
    this.trackVolumes[idx] = vol;
    // Track 1 (idx=0) controla o volume do <video>. Aplica imediato.
    this.applyVolumes();
    // Se mexeu numa track > 0 e ainda nao temos audio elements, pede
    // a extracao agora. O reproducao do video nao para.
    if (idx > 0 && vol > 0 && !this.audiosRequested && this.currentId) {
      this.audiosRequested = true;
      Bridge.send('request_audio_tracks', { id: this.currentId });
    }
  },

  onAudioTracksReady(data) {
    if (!data || data.id !== this.currentId) return;
    if (this.audiosReadyForId === data.id) return; // ja setup
    const urls = data.urls || [];
    this.audiosReadyForId = data.id;
    // Limpa qualquer audio anterior (defensive). Inclui desconexao
    // dos gain nodes pra nao acumular no audioCtx se onAudioTracksReady
    // dispara duas vezes (race entre clicks rapidos).
    this.audioEls.forEach(a => {
      try { a.pause(); a.src=''; } catch(e){}
      try { if (a._noobs_source) a._noobs_source.disconnect(); } catch(e){}
      try { if (a._noobs_gain)   a._noobs_gain.disconnect();   } catch(e){}
      a._noobs_source = null;
      a._noobs_gain   = null;
    });
    this.audioEls = [];
    // Cria audio elements pras tracks 2..N (skipa idx 0 = video).
    // COM crossOrigin="anonymous": o servidor HTTP local retorna
    // Access-Control-Allow-Origin: * (ver OBSPlayer.HandleGet), o que
    // permite o GainNode sem taintar/silenciar. WebView2 com
    // NavigateToString tem origin "null"; ACAO=* satisfaz o CORS.
    const log = (msg) => Bridge.send('ui_log', { message: 'audio: ' + msg });
    for (let i = 1; i < urls.length; i++) {
      // crossOrigin DEVE ser setado ANTES do src (que ja foi via
      // new Audio(url) ali) — Chromium ignora mudanca depois que o
      // request comecou. Reseta forcando src vazio → seta crossOrigin
      // → re-atribui src. Necessario pra MediaElementAudioSourceNode
      // nao "taintar" e silenciar quando o GainNode entra em cena.
      const a = new Audio();
      a.crossOrigin = 'anonymous';
      a.src = urls[i];
      a.preload = 'auto';
      const idx = i;
      a.addEventListener('error', () => {
        log('track ' + idx + ' ERROR ' +
          (a.error ? 'code=' + a.error.code : 'unknown') +
          ' url=' + urls[idx]);
      });
      a.addEventListener('canplay', () => log('track ' + idx + ' canplay'));
      a.addEventListener('loadedmetadata', () =>
        log('track ' + idx + ' loadedmetadata dur=' + a.duration));
      this.audioEls.push(a);
    }
    log('created ' + this.audioEls.length + ' audio element(s)');
    // Sync inicial: posiciona no mesmo currentTime do video, da play
    // se o video estiver tocando.
    this.syncAudios(true);
    this.applyVolumes();
    this.startDriftCheck();
  },

  syncAudios(forceSeek) {
    const v = document.getElementById('playerVideo');
    if (!v || this.audioEls.length === 0) return;
    const log = (m) => Bridge.send('ui_log', { message: 'audio-sync: ' + m });
    this.audioEls.forEach((a, i) => {
      if (forceSeek || Math.abs(a.currentTime - v.currentTime) > 0.05)
        a.currentTime = v.currentTime;
      a.playbackRate = v.playbackRate;
      if (!v.paused && a.paused) {
        a.play().then(
          () => log('track ' + (i+1) + ' play OK vol=' + a.volume),
          (err) => log('track ' + (i+1) + ' play REJEITADO ' + (err && err.name) + ': ' + (err && err.message))
        );
      }
    });
  },
  pauseAudios() {
    this.audioEls.forEach(a => { try { a.pause(); } catch(e){} });
  },
  applyRateAudios() {
    const v = document.getElementById('playerVideo');
    if (!v) return;
    this.audioEls.forEach(a => { a.playbackRate = v.playbackRate; });
  },
  startDriftCheck() {
    if (this.driftRaf) return;
    let lastCheck = 0;
    const check = (t) => {
      const v = document.getElementById('playerVideo');
      if (!v || this.audioEls.length === 0) { this.driftRaf = null; return; }
      // Check a cada ~200ms (nao todo frame).
      if (t - lastCheck > 200) {
        lastCheck = t;
        if (!v.paused && !v.ended) {
          this.audioEls.forEach(a => {
            const drift = a.currentTime - v.currentTime;
            if (Math.abs(drift) > 0.10)
              a.currentTime = v.currentTime;
          });
        }
      }
      this.driftRaf = requestAnimationFrame(check);
    };
    this.driftRaf = requestAnimationFrame(check);
  },
  stopDriftCheck() {
    if (this.driftRaf) { cancelAnimationFrame(this.driftRaf); this.driftRaf = null; }
  },
  // ===== Web Audio: GainNode pra volume > 100% =====
  // HTMLMediaElement.volume e clampado em [0, 1] pelo spec — sem
  // GainNode, "200%" nao existiria. Lazy: AudioContext so e criado
  // quando algo realmente precisa (slider mexido ou audio slave
  // criado). MediaElementAudioSourceNode pode ser criado UMA vez
  // por elemento — guardamos no proprio elemento via _noobs_gain.
  audioCtx: null,
  ensureAudioGraph() {
    if (!this.audioCtx) {
      const AC = window.AudioContext || window.webkitAudioContext;
      if (!AC) return; // browser muito antigo — fallback nativo
      try { this.audioCtx = new AC(); }
      catch (e) { return; }
    }
    // Pode estar suspended por autoplay policy ate o user gesture.
    // resume() e idempotente e ja temos gesture (o input do slider).
    if (this.audioCtx.state === 'suspended') {
      this.audioCtx.resume().catch(() => {});
    }
  },
  // Anexa MediaElementSource→GainNode→destination no elemento, se
  // ainda nao tem. Retorna o GainNode (ou null se falhou). Cacheado
  // em el._noobs_gain — chamadas subsequentes sao no-op O(1).
  attachGain(el) {
    if (!el) return null;
    if (el._noobs_gain) return el._noobs_gain;
    if (!this.audioCtx) {
      this.ensureAudioGraph();
      if (!this.audioCtx) return null;
    }
    try {
      const src  = this.audioCtx.createMediaElementSource(el);
      const gain = this.audioCtx.createGain();
      src.connect(gain).connect(this.audioCtx.destination);
      el._noobs_source = src;
      el._noobs_gain   = gain;
      return gain;
    } catch (e) {
      // Tipico: createMediaElementSource ja chamado pra este element,
      // OU elemento ainda nao tem audio (load nao terminou). Loga e
      // segue com fallback nativo — slider funciona ate 100% via .volume.
      Bridge.send('ui_log',
        { message: 'audio-graph: attach falhou ' + (e && e.message || e) });
      return null;
    }
  },
  applyVolumes() {
    const v = document.getElementById('playerVideo');
    if (!v) return;
    const tv = this.trackVolumes || [1.0];
    const master = this.masterMuted ? 0 :
      (this.masterVolume == null ? 1 : this.masterVolume);
    // Track 0 = audio nativo do <video>. Tenta gain node primeiro;
    // se conseguiu, .volume fica em 1.0 (gain controla tudo). Caso
    // contrario fallback pra .volume nativo (clampado em [0,1]).
    const trk0 = tv[0] == null ? 1 : tv[0];
    const vGain = this.attachGain(v);
    if (vGain) {
      v.volume = 1;
      // setTargetAtTime suaviza mudancas rapidas no slider sem zipper noise.
      try {
        vGain.gain.setTargetAtTime(
          Math.max(0, master * trk0), this.audioCtx.currentTime, 0.01);
      } catch (e) { vGain.gain.value = Math.max(0, master * trk0); }
    } else {
      v.volume = Math.max(0, Math.min(1, master * trk0));
    }
    v.muted = false;
    this.audioEls.forEach((a, i) => {
      const trackIdx = i + 1;
      const tvol = tv[trackIdx] == null ? 0 : tv[trackIdx];
      const aGain = this.attachGain(a);
      if (aGain) {
        a.volume = 1;
        try {
          aGain.gain.setTargetAtTime(
            Math.max(0, master * tvol), this.audioCtx.currentTime, 0.01);
        } catch (e) { aGain.gain.value = Math.max(0, master * tvol); }
      } else {
        a.volume = Math.max(0, Math.min(1, master * tvol));
      }
    });
  }
};

// =========================================================================
// Helper: atualiza --vp e --vp-mid do gradient pra um slider 0..200.
//   --vp     = posicao do thumb em % da largura (val/2 → 0..100%)
//   --vp-mid = limite verde/branco→vermelho (min(val,100)/2)
// Tambem alterna --thumb-color pra vermelho quando boost ativo.
// =========================================================================
function setVolBarVars(slider, val0_200) {
  const v = Math.max(0, Math.min(200, val0_200 | 0));
  const pct = v / 2;                    // 0..100 (% da largura)
  const mid = Math.min(v, 100) / 2;     // 0..50  (limite cor1→cor2)
  slider.style.setProperty('--vp',     pct + '%');
  slider.style.setProperty('--vp-mid', mid + '%');
  slider.style.setProperty('--thumb-color', v > 100 ? 'var(--danger-2, #ef4444)' : '');
}

// =========================================================================
// VolTooltip — hint flutuante que mostra o % atual durante interacao
// com os sliders de volume (master + per-faixa). Singleton: um unico
// <div> reutilizado, posicionado via JS sobre o thumb do slider ativo.
//
// Eventos:
//   pointerdown → mostra (com valor atual)
//   input       → atualiza texto + posicao (durante drag/keyboard)
//   pointerup   → schedule hide com pequeno delay (leitura final)
//   focus       → mostra (keyboard nav)
//   blur        → hide
//   dblclick    → reseta pra 100% (dispara 'input' programaticamente
//                 pra os outros handlers reagirem: gain + gradient +
//                 label de % na linha do per-faixa)
//
// pointerup e ouvido tambem na window — sem isso, drag que termina
// FORA do slider (mouse arrastado pra longe) deixaria o tooltip
// visivel pra sempre.
// =========================================================================
const VolTooltip = {
  el: null,
  active: null,
  hideTimer: 0,
  globalWired: false,

  _ensure() {
    if (this.el) return this.el;
    let t = document.getElementById('volTooltip');
    if (!t) {
      t = document.createElement('div');
      t.id = 'volTooltip';
      t.className = 'vol-tooltip';
      t.hidden = true;
      document.body.appendChild(t);
    }
    this.el = t;
    if (!this.globalWired) {
      window.addEventListener('pointerup',     () => this._onGlobalUp());
      window.addEventListener('pointercancel', () => this._onGlobalUp());
      window.addEventListener('resize', () => {
        if (this.active) this._position(this.active);
      });
      this.globalWired = true;
    }
    return t;
  },

  show(slider) {
    const t = this._ensure();
    clearTimeout(this.hideTimer);
    const v = parseInt(slider.value, 10) || 0;
    t.textContent = v + '%';
    t.classList.toggle('boost', v > 100);
    t.hidden = false;
    this._position(slider);
  },

  _position(slider) {
    const t = this.el;
    if (!t || t.hidden) return;
    const rect = slider.getBoundingClientRect();
    const min = parseFloat(slider.min) || 0;
    const max = parseFloat(slider.max) || 100;
    const val = parseFloat(slider.value) || 0;
    const ratio = (max > min) ? (val - min) / (max - min) : 0;
    // thumbW aproximado — Chromium renderiza ~11px no master e ~10px
    // no per-faixa. A diferenca eh perceptivelmente irrelevante pro
    // posicionamento do tooltip; 11 da centralizacao boa nos dois casos.
    const thumbW = 11;
    const innerW = Math.max(0, rect.width - thumbW);
    const x = rect.left + thumbW/2 + ratio * innerW;
    t.style.left = x + 'px';
    t.style.top  = rect.top + 'px';
  },

  hide(delayMs) {
    clearTimeout(this.hideTimer);
    const doHide = () => { if (this.el) this.el.hidden = true; };
    if (delayMs && delayMs > 0)
      this.hideTimer = setTimeout(doHide, delayMs);
    else
      doHide();
  },

  _onGlobalUp() {
    if (this.active) {
      this.hide(400);
      this.active = null;
    }
  },

  // Wire-up de UM slider (idempotente — flag no proprio elemento
  // evita listeners duplicados se o painel de info for re-renderizado).
  wire(slider) {
    if (!slider || slider._noobs_voltt_wired) return;
    slider._noobs_voltt_wired = true;
    this._ensure();

    slider.addEventListener('pointerdown', () => {
      this.active = slider;
      // pointerdown roda ANTES do native value update do click-on-track,
      // entao o primeiro show pega o valor antigo. O input event que
      // dispara em sequencia (1-2ms depois) ja atualiza o texto.
      this.show(slider);
    });
    slider.addEventListener('input', () => {
      this.show(slider);
      // Keyboard nav (setas, page up/down): nao tem pointerdown ativo,
      // entao auto-hide curtinho depois da ultima tecla.
      if (!this.active) this.hide(800);
    });
    slider.addEventListener('focus', () => this.show(slider));
    slider.addEventListener('blur',  () => this.hide(200));
    slider.addEventListener('dblclick', (e) => {
      e.preventDefault();
      slider.value = '100';
      // Dispara 'input' programaticamente pra os handlers ja
      // existentes (gain, gradient, label) atualizarem em cascata.
      // bubbles=true caso algum dia adicionemos delegation.
      slider.dispatchEvent(new Event('input', { bubbles: true }));
      this.show(slider);
      this.hide(900);
    });
  }
};
function fmtTime(s) {
  s = Math.max(0, Math.round(s || 0));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), ss = s % 60;
  const pad = (n) => n.toString().padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(ss)}` : `${pad(m)}:${pad(ss)}`;
}

// Timer com centesimos animado por JS. Backend so manda elapsed em
// segundos a cada tick (~1s), mas pra o display ficar fluido renderizamos
// localmente baseado em Date.now() - startTime. A cada 'recording_state'
// vindo do backend re-sincronizamos startTime, evitando drift.
let _recAnimStart = 0;
let _recAnimInterval = null;

function _renderTimer(ms) {
  const total = Math.floor(ms / 1000);
  const h = String(Math.floor(total / 3600)).padStart(2, '0');
  const m = String(Math.floor((total % 3600) / 60)).padStart(2, '0');
  const s = String(total % 60).padStart(2, '0');
  const cs = String(Math.floor((ms % 1000) / 10)).padStart(2, '0');
  const main = document.getElementById('recTimerMain');
  const msEl = document.getElementById('recTimerMs');
  if (main) main.textContent = `${h}:${m}:${s}`;
  if (msEl) msEl.textContent = `.${cs}`;
}

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

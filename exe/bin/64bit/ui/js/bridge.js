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
      let data;
      try {
        data = (typeof e.data === 'string') ? JSON.parse(e.data) : e.data;
      } catch (err) {
        console.warn('Bridge: mensagem JSON invalida ignorada:', err);
        return;
      }
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
    play_url(data) { Player.play(data.url, data.name, data.mode, data.id, data.startClockSec); },
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


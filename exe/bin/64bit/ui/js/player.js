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
    // 'seeking' PAUSA os slaves na hora: sem isso eles continuam tocando o
    // trecho ANTIGO durante o gap do seek do <video> e, quando 'seeked'
    // chega e re-sincroniza, da uma "repetida"/eco de alguns ms. Pausados,
    // o 'seeked' reposiciona (currentTime no ponto novo) e retoma limpo.
    v.addEventListener('seeking', () => Player.pauseAudios());
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
    // permite o GainNode sem taintar/silenciar. A UI roda em
    // https://noobs.app e o audio vem de http://127.0.0.1 (origem cross);
    // ACAO=* satisfaz o CORS.
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


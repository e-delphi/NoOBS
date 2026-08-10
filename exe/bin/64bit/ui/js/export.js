// =====================================================================
// Export — modal de exportacao (recorte + regioes + escala + audio)
// =====================================================================
// Script comum (NAO module), como os outros js/ — escopo global, exigido
// pelos handlers inline do HTML (onclick="Export...").
//
// Os dados vem de duas mensagens que JA existiam:
//   video_info    — duracao, bitrate, streams de audio, layout do canvas
//   encoder_caps  — campo exportEncoders[] (encoders do LIBAVCODEC que
//                   servem nesta maquina; ver OBSBridge.PushEncoderCaps)
// Nada de mensagem nova pra abrir o dialogo.

// Faixa do controle de qualidade, na escala do x264 (0 = sem perdas,
// 51 = pior). Tem que casar com EXPORT_CRF_* do FFmpegExport.pas e com
// os limites do <input type="range"> do index.html.
const CRF_MIN = 0;
const CRF_MAX = 51;
const CRF_DEFAULT = 23;

const Export = {
  currentId: null,
  durationSec: 0,
  srcW: 0,
  srcH: 0,
  srcFps: 0,             // taxa de quadros da origem (0 = desconhecida)
  layout: null,          // { canvasW, canvasH, regions: [...] }
  audioStreams: [],      // [{ index, title, channels }]
  selectedRegions: null, // Set<number> — vazio = todas
  selectedAudio: null,   // Set<number> — indices de STREAM do arquivo
  parts: [],             // [{start,end,keep}] cobrindo a duracao inteira
  playSec: 0,            // posicao do cursor na linha do tempo
  zoom: 1,               // quantas telas a duracao inteira ocupa
  encoders: [],          // [{ id, name, hardware }]
  defaultEncoder: 'auto',
  running: false,
  _loading: false,

  // ---- ciclo de vida ------------------------------------------------

  openForSelected() {
    const ids = RecSelection.all();
    if (ids.length !== 1) return;
    this.openFor(ids[0]);
  },

  openFor(id) {
    if (!id || this.running) return;
    this.currentId = id;
    this._loading = true;
    this.selectedRegions = new Set();
    this.selectedAudio = new Set();
    this.durationSec = 0;
    this.parts = [];
    this.playSec = 0;

    const ov = document.getElementById('exportOverlay');
    ov.classList.remove('running');
    ov.classList.add('visible');
    this.running = false;
    this._setProgress(0);

    // Nome sugerido a partir do card (que ja mostra o nome amigavel).
    const card = document.querySelector(
      `#recGrid .rec-card[data-id="${CSS.escape(id)}"]`);
    const base = card ? (card.querySelector('.when')?.textContent || '') : '';
    document.getElementById('exportName').value =
      (base || T('export.defaultName')) + ' - ' + T('export.nameSuffix');

    // Zera o que depende do arquivo enquanto o probe nao volta.
    document.getElementById('exportRegions').innerHTML = '';
    document.getElementById('exportAudio').innerHTML = '';
    document.getElementById('exportPreview').innerHTML = '';
    document.getElementById('exportParts').innerHTML = '';
    document.getElementById('exportRunBtn').disabled = true;
    const wrap = document.querySelector('.export-preview-video');
    if (wrap) wrap.classList.remove('ready');
    const msg = document.getElementById('exportVideoMsg');
    if (msg) msg.textContent = T('export.previewLoading');

    this._renderEncoders();
    this._syncContainerHint();
    this._syncRunButton();
    this._wirePreview();
    this._syncPlayButtons();
    // Duas mensagens, ambas ja existentes: o probe (duracao/faixas/layout)
    // e a URL do arquivo pra previa — a mesma que o player usa.
    Bridge.send('request_video_info', { id: id });
    Bridge.send('play_recording', { id: id });
  },

  close() {
    if (this.running) return;   // durante a exportacao so o Cancelar sai
    const ov = document.getElementById('exportOverlay');
    ov.classList.remove('visible');
    // Solta o arquivo: com o <video> segurando a URL, o servidor HTTP
    // mantem o handle aberto e dividir/excluir a gravacao falharia.
    const v = document.getElementById('exportVideo');
    if (v) { try { v.pause(); } catch (e) {} v.removeAttribute('src'); v.load(); }
    const wrap = document.querySelector('.export-preview-video');
    if (wrap) wrap.classList.remove('ready');
    // A tela cobre a janela inteira: sem rolar o corpo pro topo, reabrir
    // mostra o formulario no meio de onde o usuario tinha parado.
    const body = document.querySelector('.export-body');
    if (body) body.scrollTop = 0;
    this.currentId = null;
  },

  // ---- dados do arquivo ---------------------------------------------

  // True quando a TELA de exportacao esta aberta neste id — o bridge usa
  // pra decidir entre entregar a resposta pro player ou pra ca.
  //
  // Testa a tela estar visivel, e nao um flag de "carregando": video_info
  // e play_url voltam em ordem imprevisivel, e um flag zerado pela
  // primeira resposta mandaria a segunda pro player, que abriria por cima.
  isWaitingFor(id) {
    const ov = document.getElementById('exportOverlay');
    return !!id && this.currentId === id &&
           !!ov && ov.classList.contains('visible');
  },

  onVideoInfo(data) {
    if (!data || data.id !== this.currentId) return;
    this._loading = false;

    this.durationSec = data.duration || 0;
    this.srcW = 0;
    this.srcH = 0;
    this.srcFps = 0;
    this.audioStreams = [];
    (data.streams || []).forEach(s => {
      if (s.kind === 'video' && !this.srcW) {
        this.srcW = s.width || 0;
        this.srcH = s.height || 0;
        this.srcFps = s.frameRate || 0;
      } else if (s.kind === 'audio') {
        this.audioStreams.push(s);
      }
    });

    this.layout = (data.layout && Array.isArray(data.layout.regions) &&
                   data.layout.regions.length > 0) ? data.layout : null;

    // Faixa 1 (o mix) marcada por padrao.
    if (this.audioStreams.length > 0)
      this.selectedAudio.add(this.audioStreams[0].index);

    this._resetParts();
    this._wireTimeline();
    this._applyZoom();
    this._renderTicks();
    this._renderParts();
    this._renderRegions();
    this._renderAudio();
    this._renderResolutions();
    this._renderScale();
    this._renderFps();
    this._renderPreview();
    this._syncQualityValue();
    document.getElementById('exportRunBtn').disabled = false;
  },

  // ---- encoders ------------------------------------------------------

  applyEncoderCaps(data) {
    if (!data) return;
    if (Array.isArray(data.exportEncoders)) this.encoders = data.exportEncoders;
    this._renderEncoders();
  },

  setDefaultEncoder(codec) {
    this.defaultEncoder = codec || 'auto';
    this._renderEncoders();
  },

  _renderEncoders() {
    const sel = document.getElementById('exportEncoder');
    if (!sel) return;
    const prev = sel.value;
    sel.innerHTML = '';
    // 'av1-sw' so aparece AQUI, nao nas Configuracoes: a exportacao aceita
    // encoder de software pra qualquer formato (o usuario escolheu esperar),
    // enquanto a gravacao evita software pra nao competir com a maquina.
    const labels = {
      'h264-hw': T('settings.codec.h264Hw'),
      'h264-sw': T('settings.codec.h264Sw'),
      'av1-hw':  T('settings.codec.av1Hw'),
      'av1-sw':  T('settings.codec.av1Sw'),
      'hevc-hw': T('settings.codec.hevcHw')
    };
    if (this.encoders.length === 0) {
      // Ainda nao chegou o encoder_caps (warmup do libobs). 'auto' deixa
      // o backend resolver — nunca fica sem opcao.
      const o = document.createElement('option');
      o.value = 'auto';
      o.textContent = T('settings.codec.auto');
      sel.appendChild(o);
      return;
    }
    this.encoders.forEach(e => {
      const o = document.createElement('option');
      o.value = e.id;
      o.textContent = labels[e.id] || e.id;
      sel.appendChild(o);
    });
    // Default: o codec das Configuracoes, se estiver na lista.
    const want = (prev && prev !== 'auto') ? prev : this.defaultEncoder;
    if (want && [...sel.options].some(o => o.value === want)) sel.value = want;
  },

  // ---- trechos (cortes) ----------------------------------------------
  //
  // Modelo: `parts` cobre a duracao INTEIRA, em ordem e sem buraco. Cada
  // parte tem { start, end, keep }. Cortar divide a parte sob o cursor em
  // duas; clicar numa parte alterna se ela entra no resultado. O que vai
  // pro backend sao as partes com keep=true, com as vizinhas fundidas.

  _resetParts() {
    this.parts = [{ start: 0, end: this.durationSec, keep: true }];
    this.playSec = 0;
    this.zoom = 1;
  },

  // ---- zoom da linha do tempo ---------------------------------------
  //
  // zoom = quantas telas a duracao inteira ocupa. 1 = tudo cabe na
  // largura; 60 = cada tela mostra 1/60 do video. O teto e calculado pra
  // que no maximo sobrem ~2s por tela — mais que isso nao ajuda a mirar.

  _maxZoom() {
    return Math.max(1, Math.min(2000, this.durationSec / 2));
  },

  // Zoom mantendo `anchorSec` parado no mesmo ponto da tela. Sem essa
  // ancora, aproximar joga o trecho de interesse pra fora da vista.
  setZoom(z, anchorSec) {
    const sc = document.getElementById('exportTimelineScroll');
    if (!sc) return;
    const prev = this.zoom || 1;
    const next = Math.max(1, Math.min(this._maxZoom(), z));
    if (Math.abs(next - prev) < 0.0001) return;

    const view = sc.clientWidth || 1;
    const anchor = (typeof anchorSec === 'number')
      ? anchorSec : this._visibleCenterSec();
    // Onde a ancora esta na tela agora (px a partir da borda esquerda).
    const anchorOffset = (this.durationSec > 0)
      ? (anchor / this.durationSec) * (view * prev) - sc.scrollLeft
      : 0;

    this.zoom = next;
    this._applyZoom();
    // Recoloca a ancora no mesmo offset.
    if (this.durationSec > 0) {
      sc.scrollLeft = (anchor / this.durationSec) * (view * next) - anchorOffset;
    }
    this._renderTicks();
    this._syncPlayhead();
  },

  zoomBy(factor, anchorSec) {
    this.setZoom((this.zoom || 1) * factor, anchorSec);
  },

  resetZoom() {
    this.setZoom(1, 0);
    const sc = document.getElementById('exportTimelineScroll');
    if (sc) sc.scrollLeft = 0;
  },

  _applyZoom() {
    const pct = ((this.zoom || 1) * 100) + '%';
    const tl = document.getElementById('exportTimeline');
    const tk = document.getElementById('exportTicks');
    if (tl) tl.style.width = pct;
    if (tk) tk.style.width = pct;
    const lvl = document.getElementById('exportZoomLevel');
    if (lvl) lvl.textContent = (this.zoom < 10 ? this.zoom.toFixed(1) : Math.round(this.zoom)) + '×';
  },

  _visibleCenterSec() {
    const sc = document.getElementById('exportTimelineScroll');
    if (!sc || this.durationSec <= 0) return 0;
    const total = (sc.clientWidth || 1) * (this.zoom || 1);
    return (sc.scrollLeft + (sc.clientWidth || 0) / 2) / total * this.durationSec;
  },

  _secAtClientX(clientX) {
    const tl = document.getElementById('exportTimeline');
    if (!tl) return 0;
    const r = tl.getBoundingClientRect();
    if (r.width <= 0) return 0;
    // r.left ja acompanha o scroll, entao isso vale zoomado tambem.
    return (clientX - r.left) / r.width * this.durationSec;
  },

  // Marcadores de tempo com passo escolhido pra dar ~90px entre eles no
  // zoom atual. Sem isso a regua fica ilegivel (ou vazia demais).
  //
  // Renderiza SO a janela visivel (mais uma tela de folga de cada lado) e
  // refaz ao rolar: no zoom maximo de um video de 2h a regua inteira
  // daria 7000+ divs, e o navegador engasga a cada aproximada.
  _renderTicks() {
    const box = document.getElementById('exportTicks');
    const sc = document.getElementById('exportTimelineScroll');
    if (!box || !sc || this.durationSec <= 0) return;
    const view = sc.clientWidth || 600;
    const totalPx = view * (this.zoom || 1);
    const STEPS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];
    const minPx = 90;
    let step = STEPS[STEPS.length - 1];
    for (const s of STEPS) {
      if (s / this.durationSec * totalPx >= minPx) { step = s; break; }
    }

    const secPerPx = this.durationSec / totalPx;
    const from = Math.max(0, (sc.scrollLeft - view) * secPerPx);
    const to = Math.min(this.durationSec, (sc.scrollLeft + view * 2) * secPerPx);

    box.innerHTML = '';
    const first = Math.ceil(from / step) * step;
    for (let t = first; t <= to; t += step) {
      // Pula o marcador colado no fim: o rotulo dele passaria da largura
      // da barra e criaria rolagem por 2-3px do nada.
      if (t / this.durationSec > 0.985) break;
      const d = document.createElement('div');
      d.className = 'export-tick';
      d.style.left = (t / this.durationSec * 100) + '%';
      d.textContent = this._fmtTime(t);
      box.appendChild(d);
    }
    this._ticksStep = step;
  },

  cutHere() {
    const t = this.playSec;
    // Corte na borda nao divide nada.
    const i = this.parts.findIndex(p => t > p.start + 0.05 && t < p.end - 0.05);
    if (i < 0) return;
    const p = this.parts[i];
    this.parts.splice(i, 1,
      { start: p.start, end: t, keep: p.keep },
      { start: t, end: p.end, keep: p.keep });
    this._renderParts();
  },

  // Indice do trecho sob o cursor. A barra inteira e area de arrastar,
  // entao incluir/tirar age SEMPRE na posicao do cursor — o que tambem
  // resolve alternar trechos curtos demais pra acertar com o mouse.
  _partAtCursor() {
    const t = this.playSec;
    if (!this.parts || this.parts.length === 0) return -1;
    for (let i = 0; i < this.parts.length; i++) {
      const p = this.parts[i];
      if (t >= p.start && t < p.end) return i;
    }
    return this.parts.length - 1;   // cursor no fim exato
  },

  togglePartAtCursor() {
    this.togglePart(this._partAtCursor());
  },

  // Divisao mais proxima do cursor. O indice i vale pela fronteira entre
  // parts[i-1] e parts[i], entao vai de 1 ate parts.length-1. -1 = ainda
  // nao ha divisao nenhuma.
  _nearestCutIndex() {
    if (!this.parts || this.parts.length < 2) return -1;
    let best = -1, bestDist = Infinity;
    for (let i = 1; i < this.parts.length; i++) {
      const d = Math.abs(this.parts[i].start - this.playSec);
      if (d < bestDist) { bestDist = d; best = i; }
    }
    return best;
  },

  // Desfaz uma divisao, juntando os dois trechos vizinhos num so.
  //
  // O trecho resultante ENTRA no vídeo se qualquer um dos dois entrava:
  // apagar uma divisao nao pode ser um jeito disfarcado de descartar
  // conteudo que estava marcado pra ficar.
  removeCutAtCursor() {
    const i = this._nearestCutIndex();
    if (i < 0) return;
    const left = this.parts[i - 1], right = this.parts[i];
    this.parts.splice(i - 1, 2,
      { start: left.start, end: right.end, keep: left.keep || right.keep });
    this._renderParts();
  },

  togglePart(i) {
    const p = this.parts[i];
    if (!p) return;
    // Nao deixa zerar tudo — sem nenhum trecho nao ha o que exportar.
    if (p.keep && this.parts.filter(x => x.keep).length === 1) {
      Toast.show(T('toast.errorTitle'), T('export.needOnePart'),
                 { warn: true, ttl: 4000 });
      return;
    }
    p.keep = !p.keep;
    this._renderParts();
  },

  // Trechos mantidos, com vizinhos colados fundidos — menos seeks no
  // backend e uma emenda a menos no encoder.
  keptSegments() {
    const out = [];
    (this.parts || []).forEach(p => {
      if (!p.keep) return;
      const last = out[out.length - 1];
      if (last && Math.abs(last.end - p.start) < 0.001) last.end = p.end;
      else out.push({ start: p.start, end: p.end });
    });
    return out;
  },

  keptDuration() {
    return this.keptSegments().reduce((a, s) => a + (s.end - s.start), 0);
  },

  _renderParts() {
    const box = document.getElementById('exportParts');
    const cuts = document.getElementById('exportCuts');
    box.innerHTML = '';
    cuts.innerHTML = '';
    const dur = Math.max(0.001, this.durationSec);
    (this.parts || []).forEach((p, i) => {
      const d = document.createElement('div');
      d.className = 'export-part ' + (p.keep ? 'keep' : 'drop');
      d.style.width = ((p.end - p.start) / dur * 100) + '%';
      const len = p.end - p.start;
      // Rotulo so quando cabe — parte curta vira sopa de letrinha.
      if (len / dur > 0.12) d.textContent = this._fmtTime(len);
      box.appendChild(d);

      // Marca de corte em cada divisa interna (a ultima nao tem).
      if (i < this.parts.length - 1) {
        const c = document.createElement('div');
        c.className = 'export-cut';
        c.style.left = (p.end / dur * 100) + '%';
        cuts.appendChild(c);
      }
    });
    this._syncPlayhead();
  },

  // Move so o ESTADO do cursor (barra, campo de tempo, botoes). Usado
  // quando quem manda e o proprio video tocando — mexer no currentTime
  // aqui brigaria com a reproducao.
  _setPlaySec(sec, keepView) {
    this.playSec = Math.max(0, Math.min(this.durationSec, sec));
    this._syncPlayhead();
    if (!keepView) this._scrollPlayheadIntoView();
  },

  // Move o cursor E reposiciona o video.
  _seekTo(sec, keepView) {
    this._setPlaySec(sec, keepView);
    const v = document.getElementById('exportVideo');
    if (v && v.readyState >= 1 && isFinite(v.duration)) {
      try { v.currentTime = this.playSec; } catch (e) {}
    }
  },

  // ---- reproducao da previa ------------------------------------------

  togglePlay() {
    const v = document.getElementById('exportVideo');
    if (!v || !v.src || v.readyState < 2) return;
    if (v.paused) {
      // O play so acontece por gesto do usuario, entao tocar com som e
      // permitido pela politica de autoplay.
      const p = v.play();
      if (p && p.catch) p.catch(() => {});
    } else v.pause();
  },

  toggleMute() {
    const v = document.getElementById('exportVideo');
    if (!v) return;
    v.muted = !v.muted;
    // Sair do mudo com volume zerado nao faria som nenhum.
    if (!v.muted && v.volume === 0) {
      v.volume = 1;
      document.getElementById('exportVolume').value = 100;
    }
    this._syncPlayButtons();
  },

  _syncPlayButtons() {
    const v = document.getElementById('exportVideo');
    const play = document.getElementById('exportPlayBtn');
    const mute = document.getElementById('exportMuteBtn');
    if (!v) return;
    const usable = !!v.src && v.readyState >= 2;
    if (play) {
      play.disabled = !usable;
      play.classList.toggle('playing', !v.paused);
      play.dataset.hint = v.paused ? T('export.play') : T('export.pause');
    }
    if (mute) {
      mute.disabled = !usable;
      const silent = v.muted || v.volume === 0;
      mute.classList.toggle('muted', silent);
      mute.dataset.hint = silent ? T('export.unmute') : T('export.mute');
    }
  },

  _wirePreview() {
    if (this._previewWired) return;
    this._previewWired = true;
    const v = document.getElementById('exportVideo');
    const vol = document.getElementById('exportVolume');
    const wrap = document.querySelector('.export-preview-video');

    // Enquanto toca, quem manda no cursor e o video.
    v.addEventListener('timeupdate', () => {
      if (v.paused) return;
      this._setPlaySec(v.currentTime);
    });
    v.addEventListener('play', () => this._syncPlayButtons());
    v.addEventListener('pause', () => this._syncPlayButtons());
    v.addEventListener('ended', () => this._syncPlayButtons());
    v.addEventListener('volumechange', () => this._syncPlayButtons());

    // "Pronto" sai de eventos PERSISTENTES, nao de um `loadeddata` de
    // disparo unico: se ele nao vier (midia em cache, troca de arquivo,
    // recarga), os controles ficariam desabilitados pra sempre.
    const ready = () => {
      wrap.classList.add('ready');
      this._syncPlayButtons();
    };
    v.addEventListener('loadeddata', ready);
    v.addEventListener('canplay', ready);
    v.addEventListener('durationchange', () => this._syncPlayButtons());
    v.addEventListener('emptied', () => {
      wrap.classList.remove('ready');
      this._syncPlayButtons();
    });
    v.addEventListener('error', () => {
      // Sem previa a exportacao continua funcionando — o codec pode nao
      // ser suportado pelo WebView2 (HEVC/AV1 sem os codecs instalados).
      wrap.classList.remove('ready');
      const msg = document.getElementById('exportVideoMsg');
      if (msg) msg.textContent = T('export.previewUnavailable');
      this._syncPlayButtons();
    });

    vol.addEventListener('input', () => {
      v.volume = (+vol.value || 0) / 100;
      if (v.volume > 0) v.muted = false;
      this._syncPlayButtons();
    });

    // Espaco toca/pausa — mas nunca enquanto o foco esta num campo de
    // texto, senao digitar espaco no nome do arquivo pausaria a previa.
    document.addEventListener('keydown', (ev) => {
      if (ev.key !== ' ' && ev.code !== 'Space') return;
      const ov = document.getElementById('exportOverlay');
      if (!ov || !ov.classList.contains('visible')) return;
      const el = document.activeElement;
      if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' ||
                 el.tagName === 'SELECT' || el.isContentEditable)) return;
      ev.preventDefault();
      this.togglePlay();
    });
  },

  _syncPlayhead() {
    const dur = Math.max(0.001, this.durationSec);
    const ph = document.getElementById('exportPlayhead');
    if (ph) ph.style.left = (this.playSec / dur * 100) + '%';
    const t = document.getElementById('exportPlayTime');
    if (t && document.activeElement !== t) t.value = this._fmtTime(this.playSec);
    const tot = document.getElementById('exportTotalTime');
    if (tot) tot.textContent = this._fmtTime(this.durationSec);
    const len = document.getElementById('exportRangeLen');
    if (len) len.textContent =
      T('export.rangeLength', { len: this._fmtTime(this.keptDuration()) });
    this._syncPartButtons();
  },

  // O botao age no trecho sob o cursor, entao o rotulo dele conta o que
  // vai acontecer — e a parte correspondente ganha um realce.
  _syncPartButtons() {
    const idx = this._partAtCursor();
    const p = (idx >= 0) ? this.parts[idx] : null;
    const btn = document.getElementById('exportTogglePartBtn');
    if (btn) {
      btn.disabled = !p;
      btn.textContent = (p && p.keep) ? T('export.dropPart') : T('export.keepPart');
    }
    const cut = document.getElementById('exportCutBtn');
    if (cut) {
      // Dividir na borda de um trecho nao produz nada.
      const t = this.playSec;
      cut.disabled = !p || !(t > p.start + 0.05 && t < p.end - 0.05);
    }
    const nodes = document.querySelectorAll('#exportParts .export-part');
    nodes.forEach((n, i) => n.classList.toggle('at-cursor', i === idx));

    // Remover divisao: age na marca mais proxima, que fica destacada.
    const cutIdx = this._nearestCutIndex();
    const un = document.getElementById('exportUncutBtn');
    if (un) {
      un.disabled = cutIdx < 0;
      un.dataset.hint = (cutIdx < 0) ? T('export.removeCutNone')
        : T('export.removeCutAt', { at: this._fmtTime(this.parts[cutIdx].start) });
    }
    document.querySelectorAll('#exportCuts .export-cut').forEach((n, i) =>
      n.classList.toggle('target', i === cutIdx - 1));
  },

  _wireTimeline() {
    if (this._timelineWired) return;
    this._timelineWired = true;
    const tl = document.getElementById('exportTimeline');
    const sc = document.getElementById('exportTimelineScroll');
    const seekFromEvent = (ev, keepView) =>
      this._seekTo(this._secAtClientX(ev.clientX), keepView);

    let dragging = false;
    tl.addEventListener('mousedown', (ev) => { dragging = true; seekFromEvent(ev, true); });
    window.addEventListener('mousemove', (ev) => { if (dragging) seekFromEvent(ev, true); });
    window.addEventListener('mouseup', () => { dragging = false; });

    // Roda do mouse sobre a barra = zoom, ancorado no ponto sob o cursor
    // do mouse. passive:false porque precisamos do preventDefault — senao
    // o formulario rolaria junto.
    sc.addEventListener('wheel', (ev) => {
      ev.preventDefault();
      this.zoomBy(ev.deltaY < 0 ? 1.25 : 1 / 1.25, this._secAtClientX(ev.clientX));
    }, { passive: false });

    // Ctrl +/- e Ctrl+0. O main.js ja bloqueia o zoom do proprio WebView
    // nessas teclas; aqui damos a elas um significado util quando a tela
    // de exportacao esta aberta.
    document.addEventListener('keydown', (ev) => {
      if (!ev.ctrlKey) return;
      const ov = document.getElementById('exportOverlay');
      if (!ov || !ov.classList.contains('visible')) return;
      if (ev.key === '+' || ev.key === '=') {
        this.zoomBy(1.6, this.playSec); ev.preventDefault();
      } else if (ev.key === '-' || ev.key === '_') {
        this.zoomBy(1 / 1.6, this.playSec); ev.preventDefault();
      } else if (ev.key === '0') {
        this.resetZoom(); ev.preventDefault();
      }
    });

    // A regua e virtualizada: rolar precisa redesenhar a janela visivel.
    // Um rAF de guarda evita refazer isso varias vezes no mesmo quadro.
    let pending = false;
    sc.addEventListener('scroll', () => {
      if (pending) return;
      pending = true;
      requestAnimationFrame(() => { pending = false; this._renderTicks(); });
    });

    // Reajusta a regua quando a janela muda de largura (o passo depende
    // da largura util).
    window.addEventListener('resize', () => {
      if (this.currentId) { this._applyZoom(); this._renderTicks(); }
    });

    document.getElementById('exportPlayTime').addEventListener('change', () => {
      const v = this._parseTime(document.getElementById('exportPlayTime').value);
      if (v === null) { this._syncPlayhead(); return; }
      this._seekTo(v);
    });
  },

  // Mantem o cursor dentro da area visivel quando ele sai por causa do
  // zoom ou de um seek pelo campo de tempo.
  _scrollPlayheadIntoView() {
    const sc = document.getElementById('exportTimelineScroll');
    if (!sc || this.durationSec <= 0) return;
    const view = sc.clientWidth || 0;
    const total = view * (this.zoom || 1);
    const x = this.playSec / this.durationSec * total;
    const margin = Math.min(60, view * 0.15);
    if (x < sc.scrollLeft + margin) sc.scrollLeft = Math.max(0, x - margin);
    else if (x > sc.scrollLeft + view - margin) sc.scrollLeft = x - view + margin;
  },

  // ---- previa --------------------------------------------------------

  onPlayUrl(data) {
    const v = document.getElementById('exportVideo');
    if (!v || !data || !data.url) return;
    // Os listeners de estado ficam em _wirePreview (persistentes); aqui
    // so trocamos a fonte. Um seek pra posicao atual assim que houver
    // metadata deixa o quadro certo na tela.
    this._wirePreview();
    v.src = data.url;
    v.addEventListener('loadedmetadata',
      () => this._seekTo(this.playSec), { once: true });
  },

  _parseTime(txt) {
    const parts = String(txt || '').trim().split(':').map(p => p.trim());
    if (parts.some(p => p === '' || isNaN(+p))) return null;
    let sec = 0;
    parts.forEach(p => { sec = sec * 60 + (+p); });
    return Math.round(sec);
  },

  _fmtTime(sec) {
    sec = Math.max(0, Math.round(sec));
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    const s = sec % 60;
    const pad = n => String(n).padStart(2, '0');
    return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
  },

  // ---- regioes -------------------------------------------------------

  _renderRegions() {
    const field = document.getElementById('exportRegionsField');
    const box = document.getElementById('exportRegions');
    box.innerHTML = '';
    // Gravacao antiga sem <hash>.json: nao ha layout, exporta o canvas
    // inteiro e a secao nem aparece (mesmo criterio do player).
    if (!this.layout) { field.style.display = 'none'; return; }
    field.style.display = '';

    box.appendChild(this._mkCheck(
      T('export.allRegions'), this.selectedRegions.size === 0,
      () => { this.selectedRegions.clear(); this._afterRegionChange(); }));

    this.layout.regions.forEach((r, i) => {
      box.appendChild(this._mkCheck(
        r.name || T('export.regionN', { n: i + 1 }),
        this.selectedRegions.has(i),
        () => {
          if (this.selectedRegions.has(i)) this.selectedRegions.delete(i);
          else this.selectedRegions.add(i);
          this._afterRegionChange();
        }));
    });
  },

  _afterRegionChange() {
    this._renderRegions();
    this._renderResolutions();
    this._renderPreview();
    // Tirar uma regiao muda a composicao, e com ela o "esta reduzindo?".
    this._renderScale();
  },

  // Tamanho nativo da composicao — mesma regra do backend
  // (BuildCompRegions): soma das larguras x maior altura.
  _composedSize() {
    if (!this.layout || this.selectedRegions.size === 0)
      return { w: this.srcW, h: this.srcH };
    let w = 0, h = 0;
    this._orderedRegions().forEach(r => { w += r.w; h = Math.max(h, r.h); });
    return { w: w || this.srcW, h: h || this.srcH };
  },

  _orderedRegions() {
    const out = [];
    this.selectedRegions.forEach(i => {
      const r = this.layout && this.layout.regions[i];
      if (r) out.push(r);
    });
    out.sort((a, b) => a.x - b.x);
    return out;
  },

  // Desenha o arranjo final em ESCALA — a proporcao de cada monitor e a
  // largura total saem do tamanho real, entao a previa mostra de fato
  // como as regioes vao se encaixar. Antes era tudo em %, o que esticava
  // a caixa pela coluna inteira e achatava os monitores.
  _renderPreview() {
    const box = document.getElementById('exportPreview');
    box.innerHTML = '';
    box.style.width = '';

    let regs = this._orderedRegions();
    let full = false;
    if (regs.length === 0) {
      // "Todos": uma caixa so, com a proporcao do canvas inteiro.
      if (!this.srcW || !this.srcH) {
        const d = document.createElement('div');
        d.className = 'export-preview-empty';
        d.textContent = T('export.previewFull');
        box.appendChild(d);
        return;
      }
      regs = [{ w: this.srcW, h: this.srcH }];
      full = true;
    }

    const PREF_H = 88;          // altura confortavel da previa
    const CHROME = 14 + regs.length * 3;   // padding + gaps
    const totalW = regs.reduce((a, r) => a + r.w, 0) || 1;
    const maxH = regs.reduce((a, r) => Math.max(a, r.h), 1);

    // Nao pode passar da largura util da coluna; quando passa, a altura
    // cede junto pra manter a proporcao.
    const avail = Math.max(160, (box.parentElement
      ? box.parentElement.clientWidth : 660) - CHROME);
    let scale = PREF_H / maxH;
    if (totalW * scale > avail) scale = avail / totalW;

    regs.forEach(r => {
      const d = document.createElement('div');
      d.className = 'export-preview-item';
      d.style.width = Math.max(8, Math.round(r.w * scale)) + 'px';
      d.style.height = Math.max(8, Math.round(r.h * scale)) + 'px';
      // Rotulo so cabe em caixa razoavel — senao vira sopa de letrinha.
      if (r.w * scale >= 62) d.textContent = full
        ? T('export.previewFull') : `${r.w}×${r.h}`;
      box.appendChild(d);
    });
  },

  // ---- resolucao -----------------------------------------------------

  _renderResolutions() {
    const sel = document.getElementById('exportResolution');
    const prev = +sel.value || 0;
    const comp = this._composedSize();
    sel.innerHTML = '';

    const add = (h, label) => {
      const o = document.createElement('option');
      o.value = String(h);
      o.textContent = label;
      sel.appendChild(o);
    };
    add(0, T('export.resOriginal', { w: comp.w, h: comp.h }));
    // Nunca oferece aumentar — upscale so inventa peso de arquivo.
    [1440, 1080, 720, 480].forEach(h => {
      if (h < comp.h) add(h, h + 'p');
    });
    if ([...sel.options].some(o => +o.value === prev)) sel.value = String(prev);

    document.getElementById('exportResolutionHint').textContent =
      T('export.resolutionHint');
  },

  _outputSize() {
    const comp = this._composedSize();
    const target = +document.getElementById('exportResolution').value || 0;
    if (!target || target >= comp.h) return comp;
    const scale = target / comp.h;
    return {
      w: Math.max(2, Math.round(comp.w * scale) & ~1),
      h: Math.max(2, Math.round(comp.h * scale) & ~1)
    };
  },

  // ---- redimensionamento ---------------------------------------------
  //
  // Escolha do algoritmo de reamostragem do swscale. So faz sentido quando
  // a saida e MENOR que a composicao: em 1:1 o swscale nao reamostra, e os
  // tres dariam a mesma imagem no mesmo tempo. Por isso o campo some.
  //
  // Os numeros da legenda sao medidos na swscale-8 empacotada, reduzindo
  // 3840x2160 -> 1920x1080 (ver pegadinha #52 do CLAUDE.md).

  _isResizing() {
    const comp = this._composedSize();
    const out = this._outputSize();
    return out.w !== comp.w || out.h !== comp.h;
  },

  _scaleAlgo() {
    const sel = document.getElementById('exportScale');
    const v = sel ? sel.value : '';
    return (v === 'bilinear' || v === 'area') ? v : 'bicubic';
  },

  _renderScale() {
    const field = document.getElementById('exportScaleField');
    const sel = document.getElementById('exportScale');
    if (!field || !sel) return;
    // Sem gravação aberta nao ha composicao pra medir (selectedRegions
    // ainda e null): o language_changed chega aqui com a tela fechada.
    if (!this.currentId) { field.style.display = 'none'; return; }
    if (!this._isResizing()) { field.style.display = 'none'; return; }
    field.style.display = '';

    // Preserva a escolha ao re-renderizar (troca de resolucao/regiao).
    const prev = this._scaleAlgo();
    sel.innerHTML = '';
    ['bicubic', 'bilinear', 'area'].forEach(id => {
      const o = document.createElement('option');
      o.value = id;
      o.textContent = T('export.scaleOpt.' + id);
      sel.appendChild(o);
    });
    sel.value = prev;
    this._syncScaleHint();
  },

  _syncScaleHint() {
    const el = document.getElementById('exportScaleHint');
    if (el) el.textContent = T('export.scaleHint.' + this._scaleAlgo());
  },

  // ---- audio ---------------------------------------------------------

  _mixIndex() {
    return this.audioStreams.length > 0 ? this.audioStreams[0].index : -1;
  },

  _renderAudio() {
    const field = document.getElementById('exportAudioField');
    const box = document.getElementById('exportAudio');
    box.innerHTML = '';
    if (this.audioStreams.length === 0) { field.style.display = 'none'; return; }
    field.style.display = '';

    const mixIdx = this._mixIndex();
    const hasIsolated = [...this.selectedAudio].some(i => i !== mixIdx);
    const hasMix = this.selectedAudio.has(mixIdx);

    this.audioStreams.forEach((s, i) => {
      const isMix = s.index === mixIdx;
      const label = s.title || (isMix ? T('export.trackMix', { n: i + 1 })
                                      : T('export.trackN', { n: i + 1 }));
      // A faixa 1 e a mixagem de TUDO: marcada junto com uma isolada, o
      // mesmo audio entraria duas vezes. Por isso as duas se excluem.
      const blocked = isMix ? hasIsolated : hasMix;
      box.appendChild(this._mkCheck(label, this.selectedAudio.has(s.index),
        () => {
          if (this.selectedAudio.has(s.index)) this.selectedAudio.delete(s.index);
          else this.selectedAudio.add(s.index);
          this._renderAudio();
        }, blocked));
    });

    // Mixar so faz sentido com 2+ faixas selecionadas.
    const mixWrap = document.getElementById('exportMixWrap');
    const canMix = this.selectedAudio.size > 1;
    mixWrap.classList.toggle('disabled', !canMix);
    const cb = document.getElementById('exportMixAudio');
    cb.disabled = !canMix;
    if (!canMix) cb.checked = false;
  },

  // ---- qualidade ------------------------------------------------------
  //
  // Um controle so: CRF, na escala do x264 — 0 = sem perdas (arquivo
  // enorme), 51 = pior. O backend traduz esse numero pro controle de
  // qualidade constante de cada encoder (cq no NVENC, qvbr_quality_level
  // no AMF, quality no Media Foundation), sempre em bitrate VARIAVEL.
  //
  // Por isso nao ha estimativa de bitrate nem de tamanho aqui: com
  // qualidade constante quem manda no tamanho e o conteudo, e recortar
  // uma regiao ou baixar a resolucao ja economiza sozinho — nao ha
  // nenhuma escala manual por area pra espelhar.

  _rangeSec() {
    return this.keptDuration();
  },

  // A barra corre ao CONTRARIO da escala: extremo esquerdo = CRF 51 (pior),
  // extremo direito = CRF 0 (sem perdas). Assim arrastar pra direita
  // melhora a qualidade, e o preenchimento da barra cresce junto — com a
  // escala crua acontecia o oposto (barra cheia = pior imagem).
  //
  // Por isso o value do <input> e a POSICAO, nao o CRF. Toda leitura do
  // controle passa por aqui; nada mais deve ler o .value direto.
  _crf() {
    const el = document.getElementById('exportQuality');
    const pos = el ? +el.value : (CRF_MAX - CRF_DEFAULT);
    if (!Number.isFinite(pos)) return CRF_DEFAULT;
    return Math.max(CRF_MIN, Math.min(CRF_MAX, CRF_MAX - Math.round(pos)));
  },

  // Faixa descritiva do CRF corrente, pra o usuario nao precisar decorar
  // a escala. Os limites sao os do x264, que e a referencia da escala.
  _crfBandKey(crf) {
    if (crf <= 17) return 'export.crfBandLossless';
    if (crf <= 23) return 'export.crfBandHigh';
    if (crf <= 30) return 'export.crfBandBalanced';
    if (crf <= 40) return 'export.crfBandSmall';
    return 'export.crfBandLow';
  },

  // ---- taxa de quadros ------------------------------------------------
  //
  // Faixa: 20 ate a taxa da ORIGEM. Fonte com 20fps ou menos nao tem o
  // que reduzir, entao o controle some.

  _renderFps() {
    const field = document.getElementById('exportFpsField');
    const sl = document.getElementById('exportFps');
    const src = Math.round(this.srcFps || 0);
    if (!src || src <= 20) { field.style.display = 'none'; return; }
    field.style.display = '';
    sl.min = 20;
    sl.max = src;
    sl.value = src;
    this._syncFpsValue();
  },

  _targetFps() {
    const sl = document.getElementById('exportFps');
    const src = Math.round(this.srcFps || 0);
    if (!src || src <= 20) return 0;          // 0 = mantem a da origem
    return Math.max(20, Math.min(src, +sl.value || src));
  },

  _syncFpsValue() {
    const el = document.getElementById('exportFpsValue');
    if (el) el.textContent = T('export.fpsValue', { fps: this._targetFps() ||
                                                    Math.round(this.srcFps || 0) });
  },

  _syncQualityValue() {
    const val = document.getElementById('exportQualityValue');
    const hint = document.getElementById('exportQualityHint');
    if (!val) return;
    const crf = this._crf();
    val.textContent = T('export.qualityValue', { crf: crf });
    if (hint)
      hint.textContent = T('export.qualityHint',
                           { band: T(this._crfBandKey(crf)) });
  },

  // ---- execucao ------------------------------------------------------

  // Botao unico: exporta quando parado, cancela quando rodando. Sair sem
  // exportar e o botao de voltar do cabecalho.
  primaryAction() {
    if (this.running) this.cancel();
    else this.run();
  },

  _syncRunButton() {
    const btn = document.getElementById('exportRunBtn');
    if (!btn) return;
    btn.textContent = this.running ? T('common.cancel') : T('export.action');
  },

  run() {
    if (this.running || !this.currentId) return;
    const segs = this.keptSegments();
    if (segs.length === 0 || this.keptDuration() <= 0) return;

    const msg = {
      id: this.currentId,
      name: document.getElementById('exportName').value,
      segments: segs.map(s => ({ startMs: Math.round(s.start * 1000),
                                 endMs: Math.round(s.end * 1000) })),
      regions: [...this.selectedRegions],
      targetHeight: +document.getElementById('exportResolution').value || 0,
      fps: this._targetFps(),
      encoder: document.getElementById('exportEncoder').value || 'auto',
      // CRF cru (0..51). O backend traduz pro controle nativo do encoder
      // escolhido; nao ha alvo de bitrate na exportacao.
      crf: this._crf(),
      // Sem reducao de resolucao o campo esta escondido: manda o default,
      // que e o que o backend usaria de qualquer jeito.
      scaleAlgo: this._isResizing() ? this._scaleAlgo() : 'bicubic',
      audioStreams: [...this.selectedAudio],
      mixTrackIndex: this._mixIndex(),
      mixAudio: document.getElementById('exportMixAudio').checked,
      container: document.getElementById('exportContainer').value || 'mp4'
    };

    // Tocando a previa durante a exportacao seria disputa boba de CPU
    // (e o som atrapalha quem esta esperando).
    const v = document.getElementById('exportVideo');
    if (v) { try { v.pause(); } catch (e) {} }

    this.running = true;
    document.getElementById('exportOverlay').classList.add('running');
    this._syncRunButton();
    this._setProgress(0);
    Bridge.send('export_recording', msg);
  },

  cancel() {
    if (!this.running) return;
    Bridge.send('cancel_export', {});
    const btn = document.getElementById('exportRunBtn');
    if (btn) btn.disabled = true;   // evita duplo clique no cancelamento
    document.getElementById('exportProgressText').textContent =
      T('export.canceling');
  },

  onProgress(pct) {
    if (!this.running) return;
    this._setProgress(pct);
  },

  _setProgress(pct) {
    const p = Math.max(0, Math.min(100, pct || 0));
    document.getElementById('exportProgressFill').style.width = p + '%';
    document.getElementById('exportProgressText').textContent =
      T('export.progress', { pct: p.toFixed(0) });
  },

  onDone(data) {
    this.running = false;
    const ov = document.getElementById('exportOverlay');
    ov.classList.remove('running');
    const btn = document.getElementById('exportRunBtn');
    if (btn) btn.disabled = false;
    this._syncRunButton();
    if (data && data.ok) {
      ov.classList.remove('visible');
      this.currentId = null;
      RecSelection.clear();
      Toast.show(T('toast.exportDone'), T('toast.exportDoneMsg'), { ttl: 6000 });
    } else if (data && data.canceled) {
      Toast.show(T('toast.exportCanceled'), T('toast.exportCanceledMsg'),
                 { ttl: 4000 });
    }
    // Falha "de verdade" ja veio como toast de erro pelo handler `error`.
  },

  // ---- helpers -------------------------------------------------------

  _mkCheck(label, checked, onToggle, disabled) {
    const el = document.createElement('label');
    el.className = 'export-check' + (checked ? ' checked' : '') +
                   (disabled ? ' disabled' : '');
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = !!checked;
    cb.disabled = !!disabled;
    cb.addEventListener('change', () => onToggle());
    const sp = document.createElement('span');
    sp.textContent = label;
    el.appendChild(cb);
    el.appendChild(sp);
    return el;
  },

  // MP4 abre em qualquer lugar; MKV é o formato nativo do app e aceita
  // HEVC/AV1 sem drama, mas nem todo player/site engole.
  _syncContainerHint() {
    const el = document.getElementById('exportContainerHint');
    if (!el) return;
    const v = document.getElementById('exportContainer').value || 'mp4';
    el.textContent = T(v === 'mkv' ? 'export.containerHintMkv'
                                   : 'export.containerHintMp4');
  },

  init() {
    const c = document.getElementById('exportContainer');
    if (c) c.addEventListener('change', () => this._syncContainerHint());
    // Mudar a resolucao pode ligar/desligar o campo de redimensionamento.
    const r = document.getElementById('exportResolution');
    if (r) r.addEventListener('change', () => this._renderScale());
    const sc = document.getElementById('exportScale');
    if (sc) sc.addEventListener('change', () => this._syncScaleHint());
    const q = document.getElementById('exportQuality');
    if (q) q.addEventListener('input', () => this._syncQualityValue());
    const f = document.getElementById('exportFps');
    if (f) f.addEventListener('input', () => this._syncFpsValue());
    // Sem clique-fora-fecha: e uma TELA, nao um dialogo — nao existe
    // "fora". Sai pelo botao de voltar ou por Esc.
    document.addEventListener('keydown', (ev) => {
      if (ev.key !== 'Escape') return;
      const o = document.getElementById('exportOverlay');
      if (o && o.classList.contains('visible') && !this.running) {
        this.close();
        ev.preventDefault();
      }
    });
  }
};

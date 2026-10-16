// =====================================================================
// Teste de qualidade de video (Configuracoes -> Video)
// =====================================================================
//
// O backend grava alguns segundos da tela pelo caminho da gravacao, no nivel
// maximo, e reproduz cada nivel 0..10 a partir dessa amostra (ver o bloco
// "Teste de qualidade de video" no OBSBridge). Aqui so se mostra o resultado:
// o tamanho que cada nivel daria por hora e o clipe de cada um, tocando lado a
// lado com um divisor, no MESMO instante — o mesmo raciocinio do A/B do teste
// de audio, que mantem o ponto ao alternar.
//
// Mensagens: test_video_quality / cancel_video_test -> push video_test.

const VideoTest = {
  running: false,
  capturing: false,
  _countdown: 0,
  _timer: null,
  _pct: 0,
  _level: -1,
  result: null,     // push 'done' do backend
  a: 5,             // nivel do lado esquerdo
  b: 10,            // nivel do lado direito (abre no 10, a referencia)
  split: 0.5,       // posicao do divisor, fracao da largura visivel
  zoom: 'fit',      // 'fit' | 1 | 2 | 3
  _drag: null,
  _splitDrag: false,
  _lastDragMoved: false,
  playing: false,
  _t: 0,            // instante do comparador (o A manda; o B segue)
  _raf: 0,
  _still: false,    // true = video nao toca aqui; compara quadros parados

  start() {
    if (this.running) return;
    // Solta os clipes do teste anterior: a mesma pasta vai ser reescrita.
    this._unload();
    this.running = true;
    this.capturing = true;
    this._countdown = 5;
    this._pct = 0;
    this._level = -1;
    this._sync();
    Bridge.send('test_video_quality');
  },

  cancel() {
    if (!this.running) return;
    Bridge.send('cancel_video_test');
  },

  onState(data) {
    if (!data) return;
    const st = data.state;
    if (st === 'recording') {
      this.running = true;
      this.capturing = true;
      this._countdown = data.seconds || 5;
      clearInterval(this._timer);
      this._timer = setInterval(() => {
        this._countdown = Math.max(0, this._countdown - 1);
        this._sync();
      }, 1000);
    } else if (st === 'encoding') {
      this.running = true;
      this.capturing = false;
      clearInterval(this._timer);
      this._pct = data.pct | 0;
      this._level = (typeof data.level === 'number') ? data.level : -1;
    } else if (st === 'done') {
      this.running = false;
      this.capturing = false;
      clearInterval(this._timer);
      this.result = data;
      // Comeca comparando o nivel escolhido hoje com o 10, que faz o papel
      // de referencia (quem ja usa o 10 compara com o 9).
      const cur = (typeof data.currentLevel === 'number') ? data.currentLevel : 5;
      this.a = cur === 10 ? 9 : cur;
      this.b = 10;
      this.split = 0.5;
      // Parado no meio do clipe: o mesmo instante do quadro PNG de reserva.
      this.playing = false;
      this._still = false;
      this._t = (data.duration || 0) / 2;
    } else if (st === 'canceled' || st === 'error') {
      this.running = false;
      this.capturing = false;
      clearInterval(this._timer);
      if (st === 'error')
        Toast.show(T('toast.errorTitle'), data.error || T('settings.videoTest.failed'),
          { warn: true, ttl: 6000 });
    }
    this._sync();
    if (st === 'done') this._render();
  },

  // ---- numeros ----------------------------------------------------

  _perHour(bytes) {
    const d = (this.result && this.result.duration) || 0;
    if (!(bytes > 0) || !(d > 0)) return 0;
    return bytes / d * 3600;
  },

  _fmtSize(bytes) {
    const loc = (typeof I18n !== 'undefined' && I18n.language) || undefined;
    if (bytes >= 1e9) return (bytes / 1e9).toLocaleString(loc, { maximumFractionDigits: 1 }) + ' GB';
    if (bytes >= 1e6) return (bytes / 1e6).toLocaleString(loc, { maximumFractionDigits: 0 }) + ' MB';
    return Math.max(1, Math.round(bytes / 1e3)).toLocaleString(loc) + ' KB';
  },

  _fmtMbps(bytes) {
    const d = (this.result && this.result.duration) || 0;
    if (!(bytes > 0) || !(d > 0)) return '';
    const loc = (typeof I18n !== 'undefined' && I18n.language) || undefined;
    return (bytes * 8 / d / 1e6).toLocaleString(loc, { maximumFractionDigits: 1 }) + ' Mbps';
  },

  _levelData(level) {
    if (!this.result || !Array.isArray(this.result.levels)) return null;
    return this.result.levels.find(l => l.level === level) || null;
  },

  _image(which) {
    const lv = which === 'a' ? this.a : this.b;
    const d = this._levelData(lv);
    return d && d.image;
  },

  _label(level) {
    return T('settings.videoTest.levelN', { n: level });
  },

  // ---- interface --------------------------------------------------

  _sync() {
    const btn = document.getElementById('vtTestBtn');
    const cancel = document.getElementById('vtCancelBtn');
    const status = document.getElementById('vtStatus');
    const bar = document.getElementById('vtProgress');
    const fill = document.getElementById('vtProgressFill');
    const res = document.getElementById('vtResult');
    if (btn) {
      btn.disabled = this.running;
      btn.textContent = T(this.result ? 'settings.videoTest.again' : 'settings.videoTest.run');
    }
    if (cancel) {
      cancel.hidden = !this.running;
      cancel.textContent = T('settings.videoTest.cancel');
    }
    if (status) {
      let txt = '';
      if (this.capturing) txt = T('settings.videoTest.capturing', { sec: this._countdown });
      else if (this.running)
        txt = this._level >= 0
          ? T('settings.videoTest.encoding', { n: this._level, pct: this._pct })
          : T('settings.videoTest.preparing');
      status.textContent = txt;
      status.hidden = !txt;
      status.classList.toggle('recording', this.capturing);
    }
    if (bar) bar.hidden = !this.running || this.capturing;
    if (fill) fill.style.width = Math.max(0, Math.min(100, this._pct)) + '%';
    // Testando de novo: o resultado anterior some pra nao ser lido como o novo.
    if (res) res.hidden = !this.result || this.running;
  },

  _render() {
    const r = this.result;
    if (!r) return;
    const meta = document.getElementById('vtMeta');
    if (meta) {
      meta.textContent = T('settings.videoTest.meta', {
        w: r.width, h: r.height, fps: Math.round(r.fps || 0),
        sec: (r.duration || 0).toLocaleString(
          (typeof I18n !== 'undefined' && I18n.language) || undefined,
          { minimumFractionDigits: 1, maximumFractionDigits: 1 }),
        encoder: r.encoder || ''
      });
    }
    this._renderLevels();
    this._renderViewer();
  },

  _renderLevels() {
    const box = document.getElementById('vtLevels');
    const r = this.result;
    if (!box || !r) return;
    box.innerHTML = '';
    const levels = (r.levels || []).slice().sort((x, y) => y.level - x.level);
    const max = Math.max(1, ...levels.map(l => this._perHour(l.bytes)));
    const cur = this._levelData(r.currentLevel);
    const curH = cur ? this._perHour(cur.bytes) : 0;
    levels.forEach(l => {
      const h = this._perHour(l.bytes);
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'vtest-row';
      row.classList.toggle('current', l.level === r.currentLevel);
      row.classList.toggle('sel-a', l.level === this.a);
      row.classList.toggle('sel-b', l.level === this.b);
      row.title = T('settings.videoTest.rowHint');

      const name = document.createElement('span');
      name.className = 'vtest-name';
      name.textContent = this._label(l.level);
      row.appendChild(name);

      const track = document.createElement('span');
      track.className = 'vtest-track';
      const fill = document.createElement('span');
      fill.className = 'vtest-fill';
      fill.style.width = (h > 0 ? Math.max(1.5, h / max * 100) : 0) + '%';
      track.appendChild(fill);
      row.appendChild(track);

      const size = document.createElement('span');
      size.className = 'vtest-size';
      size.textContent = h > 0
        ? T('settings.videoTest.perHour', { size: this._fmtSize(h) })
        : '—';
      size.title = this._fmtMbps(l.bytes);
      row.appendChild(size);

      // Relacao com o nivel atual: e a pergunta que o usuario tem ("quanto
      // a mais vai custar subir um nivel?").
      const rel = document.createElement('span');
      rel.className = 'vtest-rel';
      if (l.level === r.currentLevel) rel.textContent = T('settings.videoTest.current');
      else if (h > 0 && curH > 0) {
        const f = h / curH;
        const loc = (typeof I18n !== 'undefined' && I18n.language) || undefined;
        rel.textContent = '×' + f.toLocaleString(loc, { maximumFractionDigits: f < 1 ? 2 : 1 });
      }
      row.appendChild(rel);

      // Clique escolhe o A; com Shift (ou botao direito), o B.
      row.addEventListener('click', (e) => this.pick(l.level, e.shiftKey ? 'b' : 'a'));
      row.addEventListener('contextmenu', (e) => { e.preventDefault(); this.pick(l.level, 'b'); });
      box.appendChild(row);
    });
  },

  pick(level, slot) {
    if (slot === 'b') this.b = level; else this.a = level;
    this._renderLevels();
    this._renderViewer();
  },

  swap() {
    const t = this.a; this.a = this.b; this.b = t;
    this._renderLevels();
    this._renderViewer();
  },

  // Setas: sobe/desce o nivel do A; com Shift, o do B.
  step(dir, slot) {
    if (!this.result) return;
    const cur = slot === 'b' ? this.b : this.a;
    this.pick(Math.max(0, Math.min(10, cur + dir)), slot);
  },

  setZoom(z) {
    this.zoom = z;
    this._renderViewer();
  },

  // Usa o nivel A como qualidade da gravacao: mesmo caminho do slider (o
  // commit da tela de Configuracoes e imediato).
  useA() {
    const el = document.getElementById('settingsRecordingQuality');
    if (!el || this.a < 0) return;
    el.value = String(this.a);
    Settings.onQualityChange();
    Settings.commit();
    if (this.result) this.result.currentLevel = this.a;
    this._renderLevels();
    this._renderViewer();
    Toast.show(T('settings.videoTest.applied', { n: this.a }));
  },

  // ---- midia ------------------------------------------------------
  //
  // Cada lado tem um <video> (o clipe do nivel, pra ver em MOVIMENTO — parado
  // nao se percebe o que o encoder faz com o que muda) e um <img> de reserva
  // (o quadro PNG do meio do clipe). A imagem so entra se o video nao tocar:
  // HEVC, por exemplo, depende de extensao instalada no Windows. O A manda no
  // relogio; o B segue o A.

  _video(which) {
    const lv = which === 'a' ? this.a : this.b;
    const d = this._levelData(lv);
    return d && d.video;
  },

  _el(which) {
    const kind = this._still ? 'Img' : 'Vid';
    return document.getElementById('vt' + kind + (which === 'a' ? 'A' : 'B'));
  },

  _fps() {
    return (this.result && this.result.fps > 0) ? this.result.fps : 30;
  },

  _dur() {
    const v = document.getElementById('vtVidA');
    if (v && isFinite(v.duration) && v.duration > 0) return v.duration;
    return (this.result && this.result.duration) || 0;
  },

  // Troca o arquivo de um video mantendo o instante (e o play, se tocava).
  _load(v, url) {
    if (!v) return;
    if ((v.getAttribute('src') || '') === (url || '')) return;
    const t = this._t, wasPlaying = this.playing;
    if (!url) { v.removeAttribute('src'); v.load(); return; }
    v.setAttribute('src', url);
    v.addEventListener('loadedmetadata', () => {
      try { v.currentTime = t; } catch (e) {}
      if (wasPlaying && this.playing) v.play().catch(() => {});
    }, { once: true });
  },

  // Solta os arquivos: o proximo teste reescreve a mesma pasta.
  _unload() {
    this.pause();
    ['vtVidA', 'vtVidB'].forEach(id => {
      const v = document.getElementById(id);
      if (v) { v.removeAttribute('src'); v.load(); }
    });
  },

  togglePlay() {
    if (this.playing) this.pause(); else this.play();
  },

  play() {
    if (this._still || !this.result) return;
    const a = document.getElementById('vtVidA'), b = document.getElementById('vtVidB');
    if (!a || !b) return;
    this.playing = true;
    try { b.currentTime = a.currentTime; } catch (e) {}
    a.play().catch(() => {});
    b.play().catch(() => {});
    cancelAnimationFrame(this._raf);
    const tick = () => {
      if (!this.playing) return;
      this._t = a.currentTime;
      // B escorrega do A com o tempo (dois decoders): puxa de volta quando a
      // diferenca passa de meio quadro — antes de dar pra ver.
      if (!b.seeking && Math.abs(b.currentTime - a.currentTime) > 0.5 / this._fps())
        try { b.currentTime = a.currentTime; } catch (e) {}
      this._syncTime();
      this._raf = requestAnimationFrame(tick);
    };
    this._raf = requestAnimationFrame(tick);
    this._syncTime();
  },

  pause() {
    const a = document.getElementById('vtVidA'), b = document.getElementById('vtVidB');
    const was = this.playing;
    this.playing = false;
    cancelAnimationFrame(this._raf);
    if (a) a.pause();
    if (b) b.pause();
    // Parado, os dois TEM que estar no mesmo quadro: e ai que se compara.
    if (was && a) this.setTime(a.currentTime);
    else this._syncTime();
  },

  setTime(t) {
    const d = this._dur();
    t = Math.max(0, Math.min(d > 0 ? d - 0.001 : t, t));
    this._t = t;
    ['vtVidA', 'vtVidB'].forEach(id => {
      const v = document.getElementById(id);
      if (v && v.getAttribute('src')) try { v.currentTime = t; } catch (e) {}
    });
    this._syncTime();
  },

  seekFrac(f) {
    this.setTime(f * this._dur());
  },

  // Quadro a quadro (setas ← →): pausa e anda 1/fps.
  stepFrame(dir) {
    if (this._still || !this.result) return;
    if (this.playing) this.pause();
    this.setTime(this._t + dir / this._fps());
  },

  _syncTime() {
    const d = this._dur();
    const seek = document.getElementById('vtSeek');
    const time = document.getElementById('vtTime');
    const btn = document.getElementById('vtPlayBtn');
    if (seek && d > 0) {
      if (document.activeElement !== seek)
        seek.value = String(Math.round(this._t / d * 1000));
      // Preenchimento na cor do app ate o ponteiro (mesmo --val dos
      // outros sliders das Configuracoes).
      seek.style.setProperty('--val', String((+seek.value || 0) / 1000));
    }
    if (time) {
      const loc = (typeof I18n !== 'undefined' && I18n.language) || undefined;
      const f = (x) => x.toLocaleString(loc, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
      time.textContent = f(this._t) + ' / ' + f(d) + ' s';
    }
    if (btn) {
      btn.textContent = this.playing ? '⏸' : '▶';
      btn.title = T(this.playing ? 'settings.videoTest.pause' : 'settings.videoTest.play');
    }
  },

  // Um dos videos nao tocou (codec sem suporte no WebView2): cai pro quadro
  // parado, que sempre abre.
  _onVideoError() {
    if (this._still) return;
    this._still = true;
    this.pause();
    this._renderViewer();
  },

  _renderViewer() {
    const r = this.result;
    if (!r) return;
    const stage = document.getElementById('vtStage');
    const tagA = document.getElementById('vtTagA');
    const tagB = document.getElementById('vtTagB');
    const use = document.getElementById('vtUseBtn');
    const swap = document.getElementById('vtSwapBtn');
    const player = document.getElementById('vtPlayer');
    const still = document.getElementById('vtStillNote');
    // Um seletor de nivel em cada lado, por cima da imagem.
    this._fillTag(tagA, 'A', this.a);
    this._fillTag(tagB, 'B', this.b);
    if (swap) swap.textContent = T('settings.videoTest.swap');
    if (use) {
      use.textContent = T('settings.videoTest.use', { n: this.a });
      use.disabled = this.a < 0 || this.a === r.currentLevel;
    }
    document.querySelectorAll('#vtZoom .settings-btn').forEach(b => {
      b.classList.toggle('active', String(b.dataset.zoom) === String(this.zoom));
    });
    if (!stage) return;

    // Sem video de um dos lados (teste antigo, remux que falhou): quadro parado.
    if (!this._video('a') || !this._video('b')) this._still = true;
    if (player) player.hidden = this._still;
    if (still) still.hidden = !this._still;

    const vA = document.getElementById('vtVidA'), vB = document.getElementById('vtVidB');
    const iA = document.getElementById('vtImgA'), iB = document.getElementById('vtImgB');
    [vA, vB].forEach(v => { if (v) v.hidden = this._still; });
    [iA, iB].forEach(i => { if (i) i.hidden = !this._still; });
    if (this._still) {
      // As duas camadas tem o mesmo tamanho: trocar o nivel de um lado nao
      // mexe no scroll, e a comparacao continua no mesmo ponto.
      const ua = this._image('a') || '', ub = this._image('b') || '';
      if (iA && iA.getAttribute('src') !== ua) iA.setAttribute('src', ua);
      if (iB && iB.getAttribute('src') !== ub) iB.setAttribute('src', ub);
    } else {
      this._load(vA, this._video('a'));
      this._load(vB, this._video('b'));
    }

    stage.classList.toggle('fit', this.zoom === 'fit');
    let w = '';
    if (this.zoom !== 'fit') {
      // Tamanho REAL em pixels da tela: o artefato de compressao so aparece
      // 1:1 (ou ampliado, com pixelated). Reduzida, qualquer nivel parece bom.
      const dpr = window.devicePixelRatio || 1;
      w = ((r.width || 0) * this.zoom / dpr) + 'px';
    }
    [vA, vB, iA, iB].forEach(el => { if (el) el.style.width = w; });
    this._syncSplit();
    this._syncTime();
  },

  _fillTag(sel, letter, level) {
    if (!sel) return;
    if (sel.options.length !== 11 || sel.dataset.lang !== (I18n.language || '')) {
      sel.innerHTML = '';
      for (let n = 10; n >= 0; n--) {
        const o = document.createElement('option');
        o.value = String(n);
        o.textContent = letter + ' · ' + this._label(n);
        sel.appendChild(o);
      }
      sel.dataset.lang = I18n.language || '';
    }
    sel.value = String(level);
  },

  // Divisor: fica parado na VISTA (fracao da largura visivel), nao na
  // imagem — rolando, o corte acompanha. O A e o lado esquerdo.
  _syncSplit() {
    const stage = document.getElementById('vtStage');
    const top = this._el('a');
    const div = document.getElementById('vtDivider');
    if (!stage || !top) return;
    const x = this.split * stage.clientWidth;
    const cut = stage.scrollLeft + x;
    const right = Math.max(0, top.offsetWidth - cut);
    top.style.clipPath = 'inset(0 ' + right + 'px 0 0)';
    if (div) {
      div.style.left = x + 'px';
      div.style.height = stage.clientHeight + 'px';
    }
  },

  // Tela saindo de vista (outra aba, Configuracoes fechadas): nao deixa
  // video tocando por tras.
  onLeave() {
    this.pause();
  },

  _wireStage() {
    const stage = document.getElementById('vtStage');
    const view = document.getElementById('vtView');
    const div = document.getElementById('vtDivider');
    if (!stage || stage._wired) return;
    stage._wired = true;

    // Arrastar a imagem desloca a vista (ampliada ela passa da largura).
    stage.addEventListener('mousedown', (e) => {
      if (e.button !== 0 || this.zoom === 'fit') return;
      this._drag = { x: e.clientX, y: e.clientY, sl: stage.scrollLeft, st: stage.scrollTop, moved: false };
      stage.classList.add('dragging');
      e.preventDefault();
    });
    // Arrastar o divisor move o corte entre A e B.
    if (div) div.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      this._splitDrag = true;
      if (view) view.classList.add('splitting');
      e.preventDefault();
      e.stopPropagation();
    });
    window.addEventListener('mousemove', (e) => {
      if (this._splitDrag) {
        const rect = stage.getBoundingClientRect();
        const w = stage.clientWidth || 1;
        this.split = Math.max(0, Math.min(1, (e.clientX - rect.left) / w));
        this._syncSplit();
        return;
      }
      if (!this._drag) return;
      if (Math.abs(e.clientX - this._drag.x) + Math.abs(e.clientY - this._drag.y) > 3)
        this._drag.moved = true;
      stage.scrollLeft = this._drag.sl - (e.clientX - this._drag.x);
      stage.scrollTop = this._drag.st - (e.clientY - this._drag.y);
    });
    window.addEventListener('mouseup', () => {
      if (this._splitDrag) {
        this._splitDrag = false;
        if (view) view.classList.remove('splitting');
      }
      if (!this._drag) return;
      this._lastDragMoved = this._drag.moved;
      this._drag = null;
      stage.classList.remove('dragging');
    });
    stage.addEventListener('scroll', () => this._syncSplit());
    window.addEventListener('resize', () => this._syncSplit());
    ['vtImgA', 'vtImgB', 'vtVidA', 'vtVidB'].forEach(id => {
      const el = document.getElementById(id);
      if (!el) return;
      el.addEventListener(el.tagName === 'VIDEO' ? 'loadedmetadata' : 'load',
        () => { this._syncSplit(); this._syncTime(); });
      if (el.tagName === 'VIDEO')
        el.addEventListener('error', () => { if (el.getAttribute('src')) this._onVideoError(); });
    });
    // Segunda rede pro sincronismo: o laco de requestAnimationFrame do play()
    // para quando a pagina nao esta sendo desenhada (janela coberta ou
    // minimizada), e o B escorregava ~0,2 s. O timeupdate continua chegando.
    const vA = document.getElementById('vtVidA'), vB = document.getElementById('vtVidB');
    if (vA && vB) vA.addEventListener('timeupdate', () => {
      if (!this.playing) return;
      this._t = vA.currentTime;
      if (!vB.seeking && Math.abs(vB.currentTime - vA.currentTime) > 0.5 / this._fps())
        try { vB.currentTime = vA.currentTime; } catch (e) {}
      this._syncTime();
    });

    // Clique na imagem nao faz nada: zoom so pelos botoes, tocar/pausar so
    // pelo botao do player (ou Espaco).

    // Botoes do resultado nao ficam com o foco ao clicar: senao o Espaco
    // (tocar/pausar) tambem "apertaria" o ultimo botao clicado.
    const res = document.getElementById('vtResult');
    if (res) res.addEventListener('mousedown', (e) => {
      if (e.target.closest && e.target.closest('button')) e.preventDefault();
    });

    // Teclado com a aba Video aberta e um resultado na tela, fora de campos
    // (o proprio slider de qualidade usa as setas):
    //   ↑ ↓ nivel A (Shift: B) · ← → quadro a quadro · Espaco toca/pausa
    document.addEventListener('keydown', (e) => {
      const keys = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', ' '];
      if (!keys.includes(e.key)) return;
      if (!this.result || this.running) return;
      const ov = document.getElementById('settingsOverlay');
      if (!ov || !ov.classList.contains('visible')) return;
      if (typeof Settings !== 'undefined' && Settings.currentTab !== 'recording') return;
      const t = e.target;
      if (t && (t.isContentEditable || /^(INPUT|SELECT|TEXTAREA)$/.test(t.tagName))) return;
      e.preventDefault();
      if (e.key === ' ') this.togglePlay();
      else if (e.key === 'ArrowLeft') this.stepFrame(-1);
      else if (e.key === 'ArrowRight') this.stepFrame(1);
      else this.step(e.key === 'ArrowUp' ? 1 : -1, e.shiftKey ? 'b' : 'a');
    });
  },

  init() {
    this._wireStage();
    this._sync();
  },

  onLanguageChanged() {
    this._sync();
    this._render();
  }
};

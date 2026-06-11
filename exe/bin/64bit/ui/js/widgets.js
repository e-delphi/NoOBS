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
    // Sanitiza cada elemento: o array vira parte de um style string em
    // innerHTML (_render). Garante numeros finitos >= 0 — um valor
    // nao-numerico (cache corrompido / mensagem forjada) viraria
    // "height:NaN%" ou pior. Coercao defensiva na entrada.
    const clean = data.peaks.map(p =>
      (typeof p === 'number' && isFinite(p) && p >= 0) ? p : 0);
    this.cache.set(data.id, clean);
    if (this.currentRecId === data.id) this._render(clean);
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
  // Fecha um toast imediatamente (clique do usuário ou fim do TTL).
  // Idempotente — clique + timeout podem chamar; só o 1o age de fato.
  _dismiss(el) {
    if (!el) return;
    if (el._dismissTimer) { clearTimeout(el._dismissTimer); el._dismissTimer = null; }
    el.classList.remove('show');
    setTimeout(() => el.remove(), 220);
  },
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
      existing._dismissTimer = setTimeout(() => this._dismiss(existing), ttl);
      return;
    }
    const el = document.createElement('div');
    el.className = 'toast' + (opts && opts.warn ? ' warn' : '');
    el.dataset.title = title || '';
    // Clicar fecha o toast na hora — libera o que estiver embaixo dele
    // (ex.: o botão de fechar do player, que fica no mesmo canto).
    el.title = T('toast.clickToClose');
    el.addEventListener('click', () => this._dismiss(el));
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
    el._dismissTimer = setTimeout(() => this._dismiss(el), ttl);
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


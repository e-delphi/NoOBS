// =====================================================================
// Transcrição — fila e progresso (aba Configurações → Transcrição)
// =====================================================================
// O trabalho pesado é do backend (OBSTranscribe): fila de um item por
// vez contra a Transcritor API. Aqui só desenhamos o estado.
//
// ATENÇÃO ao que a barra significa: a API NÃO tem rota de progresso — o
// POST /transcribe só responde quando termina. Então o preenchimento vem
// de decorrido/estimado, e a estimativa sai da duração da gravação
// (~1,5× mais rápido que tempo real, medido no README da API). É uma
// previsão declarada, não uma medição — por isso o texto diz "~".

const Transcribe = {
  running: false,
  queue: 0,
  total: 0,
  done: 0,
  failed: 0,
  current: '',
  elapsed: 0,
  estimate: 0,
  lastError: '',
  pending: 0,
  _tick: null,

  // ---- ações ---------------------------------------------------------

  test() {
    const el = document.getElementById('settingsTranscribeHost');
    const status = document.getElementById('settingsTranscribeStatus');
    if (status) status.textContent = T('settings.transcribe.testing');
    Bridge.send('test_transcribe_host', { host: el ? el.value.trim() : '' });
  },

  onHealth(data) {
    const status = document.getElementById('settingsTranscribeStatus');
    if (!status) return;
    status.textContent = data && data.ok
      ? T('settings.transcribe.ok')
      : T('settings.transcribe.fail', { error: (data && data.error) || '' });
  },

  transcribePending() {
    Bridge.send('transcribe_pending', {});
  },

  cancel() {
    Bridge.send('cancel_transcribe', {});
  },

  // Menu de contexto da gravação.
  transcribeOne(id) {
    if (!id) return;
    Bridge.send('transcribe_recording', { id: id });
    Toast.show(T('toast.transcribeQueued'), '', { ttl: 2500 });
  },

  // ---- estado --------------------------------------------------------

  applyState(data) {
    if (!data) return;
    this.running   = !!data.running;
    this.queue     = data.queue || 0;
    this.total     = data.total || 0;
    this.done      = data.done || 0;
    this.failed    = data.failed || 0;
    this.current   = data.current || '';
    this.elapsed   = data.elapsed || 0;
    this.estimate  = data.estimate || 0;
    this.lastError = data.lastError || '';
    this.render();
    // Enquanto roda, o decorrido anda sozinho: o backend só reempurra o
    // estado quando algo MUDA (item começa, termina, falha), não a cada
    // segundo — 40 minutos de push por segundo seria ruído puro.
    this._syncTick();
  },

  applyPending(data) {
    this.pending = (data && data.count) || 0;
    this.render();
  },

  _syncTick() {
    if (this.running && !this._tick) {
      this._tick = setInterval(() => {
        this.elapsed++;
        this.render();
      }, 1000);
    } else if (!this.running && this._tick) {
      clearInterval(this._tick);
      this._tick = null;
    }
  },

  // ---- desenho -------------------------------------------------------

  _fmt(sec) {
    sec = Math.max(0, Math.round(sec || 0));
    const m = Math.floor(sec / 60), s = sec % 60;
    if (m >= 60) {
      const h = Math.floor(m / 60);
      return `${h}:${String(m % 60).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
    }
    return `${m}:${String(s).padStart(2, '0')}`;
  },

  render() {
    const box  = document.getElementById('transcribeProgress');
    const fill = document.getElementById('transcribeFill');
    const line = document.getElementById('transcribeLine');
    const cancelBtn  = document.getElementById('settingsTranscribeCancelBtn');
    const pendingBtn = document.getElementById('settingsTranscribePendingBtn');

    if (cancelBtn) cancelBtn.disabled = !this.running && this.queue === 0;
    if (pendingBtn) {
      pendingBtn.disabled = this.pending === 0;
      pendingBtn.textContent = this.pending > 0
        ? T('settings.transcribe.pendingN', { count: this.pending })
        : T('settings.transcribe.pendingNone');
    }
    if (!box || !line) return;

    if (!this.running && this.queue === 0) {
      // Fila vazia. Se um lote acabou de terminar, mostra o resultado em
      // vez de sumir sem dizer nada.
      if (this.total > 0) {
        box.hidden = false;
        if (fill) fill.style.width = '100%';
        line.textContent = this.failed > 0
          ? T('settings.transcribe.doneWithFailures',
              { done: this.done, total: this.total, failed: this.failed })
          : T('settings.transcribe.doneAll', { total: this.total });
      } else {
        box.hidden = true;
      }
      return;
    }

    box.hidden = false;
    // "3 de 7" conta o item em curso: o usuário pensa em "estou no 3º",
    // não em "2 terminaram".
    const pos = Math.min(this.done + this.failed + 1, this.total || 1);
    let txt = T('settings.transcribe.progress',
      { pos: pos, total: this.total || pos, name: this.current || '—' });
    if (this.estimate > 0) {
      txt += ' · ' + T('settings.transcribe.eta',
        { elapsed: this._fmt(this.elapsed), estimate: this._fmt(this.estimate) });
      if (fill) {
        // Trava em 97%: passar de 100% seria mentira, e cravar 100% antes
        // de terminar faz parecer travado. A estimativa erra pra menos com
        // frequência (servidor mais lento que o medido).
        const pct = Math.min(97, (this.elapsed / this.estimate) * 100);
        fill.style.width = pct.toFixed(1) + '%';
      }
    } else {
      txt += ' · ' + T('settings.transcribe.elapsed', { elapsed: this._fmt(this.elapsed) });
      if (fill) fill.style.width = '0%';
    }
    if (this.failed > 0 && this.lastError)
      txt += ' · ' + T('settings.transcribe.failedN', { failed: this.failed });
    line.textContent = txt;
  }
};

// =====================================================================
// Transcrição — fila e progresso (aba Configurações → Transcrição)
// =====================================================================
// O trabalho pesado é do backend (OBSTranscribe): fila de um item por
// vez contra a Transcritor API. Aqui só desenhamos o estado.
//
// O progresso é REAL: a API trabalha com jobs (POST /jobs devolve um id
// na hora, GET /jobs/{id} dá estágio, percentual e ETA) e o percentual
// vem do trecho de áudio já coberto pelos segmentos. Nada aqui é
// estimado por nós.
//
// Duas consequências no desenho: a barra pode chegar a 100% sem mentir,
// e ela anda AOS SALTOS — o Whisper processa em janelas de 30s, então em
// áudio curto são poucos degraus. O ETA só aparece a partir de ~10%,
// onde a extrapolação da API passa a fazer sentido.

const Transcribe = {
  running: false,
  queue: 0,
  total: 0,
  done: 0,
  failed: 0,
  current: '',
  elapsed: 0,
  progress: -1,   // 0..1 vindo da API; -1 = ainda sem número
  eta: -1,        // segundos restantes; -1 = a API ainda não arrisca
  _etaShown: -1,  // o ETA que vai pra TELA: conta pra baixo (ver _syncEta)
  _etaFor: '',    // de qual gravação ele é — trocou de item, recomeça
  stage: '',
  lastError: '',
  lastErrorName: '',   // qual gravação falhou — "1 com falha" sozinho não diz
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
    const failedBefore = this.failed;
    this.running   = !!data.running;
    this.queue     = data.queue || 0;
    this.total     = data.total || 0;
    this.done      = data.done || 0;
    this.failed    = data.failed || 0;
    this.current   = data.current || '';
    this.elapsed   = data.elapsed || 0;
    this.progress  = (typeof data.progress === 'number') ? data.progress : -1;
    this.eta       = (typeof data.eta === 'number') ? data.eta : -1;
    this.stage     = data.stage || '';
    this._syncEta();
    this.lastError = data.lastError || '';
    this.lastErrorName = data.lastErrorName || '';
    // A transcrição é disparada pelo menu da gravação, e daí o usuário
    // vai fazer outra coisa — ninguém fica olhando esta aba. Sem o toast,
    // a falha só existia numa linha que não estava na tela de ninguém.
    // Só na BORDA (o contador subiu), senão cada push repetiria o aviso.
    if (this.failed > failedBefore && this.lastError) {
      Toast.show(
        T('toast.transcribeFailed', { name: this.lastErrorName || '—' }),
        this.lastError, { warn: true, ttl: 12000 });
    }
    this.render();
    // O decorrido anda sozinho entre um push e outro: o backend só
    // reempurra quando algo MUDA (etapa, percentual, ETA), e o percentual
    // fica parado por dezenas de segundos entre as janelas do Whisper.
    this._syncTick();
  },

  applyPending(data) {
    this.pending = (data && data.count) || 0;
    this.render();
  },

  // O "faltam" só DESCE.
  //
  // A API extrapola o ETA a partir do progresso, e o progresso anda aos
  // SALTOS (o Whisper processa em janelas de 30s). Entre um salto e o
  // outro o decorrido cresce com o progresso PARADO, então a conta do
  // servidor sobe — e o usuário via "faltam 2:10" virar "faltam 2:40".
  //
  // Então o número do servidor é adotado quando ENCURTA o que já está na
  // tela, e ignorado quando alonga; entre pushes, o tique local desconta
  // 1s. Chegando a zero, o render cai no decorrido — que é o que já
  // acontecia antes de haver ETA, e é honesto: melhor dizer há quanto
  // tempo corre do que ficar preso num "faltam ~0:00" que não anda.
  _syncEta() {
    // Item novo (ou fila parada) recomeça do zero: o ETA de uma
    // gravação não diz nada sobre a próxima.
    const key = this.running ? (this.current || '') : '';
    if (key !== this._etaFor) {
      this._etaFor = key;
      this._etaShown = -1;
    }
    if (this.eta > 0) {
      this._etaShown = this._etaShown < 0
        ? this.eta : Math.min(this._etaShown, this.eta);
    }
  },

  _syncTick() {
    if (this.running && !this._tick) {
      this._tick = setInterval(() => {
        this.elapsed++;
        // Sem isto o "faltam" ficaria congelado entre um push e outro:
        // o backend só reempurra quando algum número MUDA.
        if (this._etaShown > 0) this._etaShown--;
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

  // O MOTIVO da falha, em linha própria. Fora da caixa de progresso
  // porque aquela some quando a fila esvazia — e o erro precisa
  // continuar legível depois que o lote terminou.
  _renderError() {
    const el = document.getElementById('transcribeError');
    if (!el) return;
    if (!this.lastError) { el.hidden = true; el.textContent = ''; return; }
    el.hidden = false;
    el.textContent = this.lastErrorName
      ? T('settings.transcribe.errorNamed',
          { name: this.lastErrorName, error: this.lastError })
      : T('settings.transcribe.error', { error: this.lastError });
  },

  render() {
    this._renderError();
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

    // Etapa: traduzida a partir do código da API ('transcribing', …), não
    // do `stage_label` que ela manda pronto — aquele vem só em português
    // e a UI tem três idiomas.
    const st = this.stage ? T('settings.transcribe.stage.' + this.stage) : '';
    if (st && st.charAt(0) !== '[') txt += ' · ' + st;

    // Barra com o percentual REAL. Sem teto artificial: quando a API diz
    // 100%, é 100%.
    if (fill) {
      fill.style.width = this.progress >= 0
        ? (this.progress * 100).toFixed(1) + '%' : '0%';
    }
    if (this.progress >= 0)
      txt += ' · ' + Math.round(this.progress * 100) + '%';

    // Só com ETA > 0: em "faltam ~0:00" o número não informa nada, e é
    // o que a API devolve no instante em que termina. O valor aqui é o
    // de _syncEta (que só desce), NUNCA o `eta` cru do servidor.
    if (this._etaShown > 0)
      txt += ' · ' + T('settings.transcribe.remaining',
        { eta: this._fmt(this._etaShown) });
    else
      txt += ' · ' + T('settings.transcribe.elapsed', { elapsed: this._fmt(this.elapsed) });

    // Só a CONTAGEM aqui; o motivo vai na linha de erro, que tem
    // espaço pra ele sem estourar esta linha de status.
    if (this.failed > 0)
      txt += ' · ' + T('settings.transcribe.failedN', { failed: this.failed });
    line.textContent = txt;
  }
};

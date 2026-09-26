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
  track: 0,       // faixa em transcrição (0-based) quando são várias
  trackCount: 1,  // 1 = faixa única; a UI não mostra nada nesse caso
  lastError: '',
  lastErrorName: '',   // qual gravação falhou — "1 com falha" sozinho não diz
  _lastToastError: '', // último motivo já avisado por toast (ver applyState)
  pending: 0,
  waiting: false,   // fila parada esperando o servidor (não é falha)
  pendingCloud: 0,  // ficaram de fora: só na nuvem (não baixamos em lote)
  queueItems: [],  // [{id,name,duration,current}] na ordem de execução
  _dragId: null,   // item sendo arrastado AGORA (trava o re-render)
  _pendingQueueRender: false,
  _tick: null,

  // ---- ações ---------------------------------------------------------

  test() {
    const el = document.getElementById('settingsTranscribeHost');
    const status = document.getElementById('settingsTranscribeStatus');
    if (status) status.textContent = T('settings.transcribe.testing');
    Bridge.send('test_transcribe_host', { host: el ? el.value.trim() : '' });
  },

  onHealth(data) {
    // O "Testar" sabe a mesma coisa que o diagnóstico: servidor de pé
    // esconde o painel de etapas; fora do ar, mostra o que falta.
    if (data && data.ok) TranscribeSetup.apply({ seq: TranscribeSetup._seq, serverOk: true });
    else if (TranscribeSetup.isHidden()) TranscribeSetup.check();
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
    this.track      = data.track || 0;
    this.trackCount = data.trackCount || 1;
    this._syncEta();
    this.lastError = data.lastError || '';
    this.lastErrorName = data.lastErrorName || '';
    // A transcrição é disparada pelo menu da gravação, e daí o usuário
    // vai fazer outra coisa — ninguém fica olhando esta aba. Sem o toast,
    // a falha só existia numa linha que não estava na tela de ninguém.
    // Só na BORDA (o contador subiu), senão cada push repetiria o aviso.
    //
    // E só quando o MOTIVO é novo: com o servidor fora do ar o lote
    // inteiro falha pelo mesmo motivo, um item atrás do outro, e o
    // Toast só deduplica por título — que aqui traz o nome da gravação,
    // então seriam N avisos empilhados dizendo a mesma coisa. A contagem
    // corrente continua na linha vermelha, que é o lugar dela.
    if (this.failed > failedBefore && this.lastError &&
        this.lastError !== this._lastToastError) {
      this._lastToastError = this.lastError;
      Toast.show(
        T('toast.transcribeFailed', { name: this.lastErrorName || '—' }),
        this.lastError, { warn: true, ttl: 12000 });
    }
    // Lote novo (o backend zerou os contadores) reabre o aviso: o mesmo
    // motivo numa segunda tentativa é informação, não repetição.
    if (this.failed === 0) this._lastToastError = '';

    // SERVIDOR FORA DO AR não é falha: o backend devolve o item pra fila e
    // tenta de novo sozinho. Sem toast de propósito — a espera aparece só
    // na aba de Transcrição, junto do diagnóstico do servidor.
    this.waiting = !!data.waiting;
    this.render();
    // O decorrido anda sozinho entre um push e outro: o backend só
    // reempurra quando algo MUDA (etapa, percentual, ETA), e o percentual
    // fica parado por dezenas de segundos entre as janelas do Whisper.
    this._syncTick();
  },

  applyPending(data) {
    this.pending = (data && data.count) || 0;
    // Pendentes que o backend deixou de fora por estarem só na nuvem.
    this.pendingCloud = (data && data.cloud) || 0;
    this.render();
  },

  applyQueue(data) {
    this.queueItems = (data && data.items) || [];
    this.renderQueue();
  },

  // Tira da fila. NÃO apaga a gravação — ela volta a contar como
  // pendente e pode ser enfileirada de novo depois.
  removeFromQueue(id) {
    if (!id) return;
    Bridge.send('remove_transcribe_item', { id: id });
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
    // A chave inclui a FAIXA: cada faixa é um job novo com estimativa
    // própria, e sem reancorar aqui o "só desce" prenderia o número da
    // faixa anterior pelo resto da gravação.
    const key = this.running ? (this.current || '') + '#' + this.track : '';
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

  // ---- fila (lista arrastável) ---------------------------------------

  // A lista vem PRONTA do backend, na ordem de execução, com o item em
  // curso na frente. Nada é deduzido aqui: a ordem é do OBSTranscribe, e
  // reordenar é mandar a nova posição e esperar o push de volta.
  renderQueue() {
    const box = document.getElementById('transcribeQueue');
    if (!box) return;

    // Arrastando: o DOM está no meio de um gesto e re-renderizar mataria
    // o drag (mesma armadilha da edição de nome — pegadinha #56). O push
    // que chegar agora é reaplicado no dragend.
    if (this._dragId) { this._pendingQueueRender = true; return; }

    const items = this.queueItems || [];
    box.textContent = '';
    box.hidden = items.length === 0;
    if (!items.length) return;

    items.forEach((it) => {
      const row = document.createElement('div');
      row.className = 'tq-item' + (it.current ? ' current' : '');
      row.dataset.id = it.id;

      const grip = document.createElement('span');
      grip.className = 'tq-grip';
      grip.textContent = it.current ? '●' : '⠿';
      row.appendChild(grip);

      const name = document.createElement('span');
      name.className = 'tq-name';
      name.textContent = it.name || '';
      name.title = it.name || '';
      row.appendChild(name);

      const dur = document.createElement('span');
      dur.className = 'tq-dur';
      // Duração 0 = ainda não lida do arquivo. O backend dispara o probe
      // ao montar a lista e reempurra quando termina, então o traço é
      // temporário — não é "gravação sem duração".
      dur.textContent = it.duration > 0 ? this._fmt(it.duration) : '—';
      row.appendChild(dur);

      if (it.current) {
        const badge = document.createElement('span');
        badge.className = 'tq-badge';
        badge.textContent = T('settings.transcribe.queueNow');
        row.appendChild(badge);
      } else {
        row.draggable = true;
        this._wireDrag(row);
        const del = document.createElement('button');
        del.className = 'tq-del';
        del.type = 'button';
        del.textContent = '×';
        del.title = T('settings.transcribe.queueRemove');
        del.setAttribute('aria-label', del.title);
        del.draggable = false;   // o botão não inicia o arrasto da linha
        del.addEventListener('mousedown', (e) => e.stopPropagation());
        del.addEventListener('click', (e) => {
          e.stopPropagation();
          this.removeFromQueue(it.id);
        });
        row.appendChild(del);
      }

      box.appendChild(row);
    });
  },

  _wireDrag(row) {
    row.addEventListener('dragstart', (e) => {
      this._dragId = row.dataset.id;
      row.classList.add('dragging');
      try {
        e.dataTransfer.effectAllowed = 'move';
        // Alguns alvos só aceitam o drop se houver dado; o conteúdo em si
        // não é lido por ninguém (mesma razão do RecFolders).
        e.dataTransfer.setData('text/plain', row.dataset.id);
      } catch (err) {}
      e.stopPropagation();
    });

    row.addEventListener('dragover', (e) => {
      if (!this._dragId || row.dataset.id === this._dragId) return;
      e.preventDefault();   // sem isto o navegador recusa o drop
      e.stopPropagation();
      try { e.dataTransfer.dropEffect = 'move'; } catch (err) {}
      const r = row.getBoundingClientRect();
      const after = (e.clientY - r.top) > r.height / 2;
      this._clearDropMarks();
      row.classList.add(after ? 'drop-after' : 'drop-before');
    });

    row.addEventListener('dragleave', () => {
      row.classList.remove('drop-before', 'drop-after');
    });

    row.addEventListener('drop', (e) => {
      e.preventDefault();
      e.stopPropagation();
      const after = row.classList.contains('drop-after');
      this._clearDropMarks();
      this._dropOn(row.dataset.id, after);
    });

    row.addEventListener('dragend', () => {
      row.classList.remove('dragging');
      this._clearDropMarks();
      this._dragId = null;
      // Um push pode ter chegado durante o gesto (o item em curso terminou,
      // por exemplo). Aplica agora, que o DOM já está livre.
      if (this._pendingQueueRender) {
        this._pendingQueueRender = false;
        this.renderQueue();
      }
    });
  },

  _clearDropMarks() {
    const box = document.getElementById('transcribeQueue');
    if (!box) return;
    box.querySelectorAll('.drop-before, .drop-after')
       .forEach(el => el.classList.remove('drop-before', 'drop-after'));
  },

  // Posição final na ESPERA — o backend indexa a fila de espera, que não
  // inclui o item em curso, então a linha dele sai da conta.
  _dropOn(targetId, after) {
    const dragId = this._dragId;
    if (!dragId || !targetId || dragId === targetId) return;
    const waiting = (this.queueItems || []).filter(it => !it.current);
    const from = waiting.findIndex(it => it.id === dragId);
    let to = waiting.findIndex(it => it.id === targetId);
    if (from < 0 || to < 0) return;
    if (after) to++;
    // Tirar o item da posição antiga desloca em 1 tudo que vem depois.
    if (from < to) to--;
    if (to === from) return;
    Bridge.send('move_transcribe_item', { id: dragId, to: to });
  },

  // Gravações que não entram na conta nem no lote por estarem só na
  // nuvem. Sem este aviso, uma biblioteca inteira no OneDrive mostraria
  // "Nada pendente" com dezenas de gravações sem transcrição — parece
  // defeito, e o usuário não teria como saber o porquê.
  _renderCloudHint() {
    const el = document.getElementById('transcribeCloudHint');
    if (!el) return;
    if (!this.pendingCloud) { el.hidden = true; el.textContent = ''; return; }
    el.hidden = false;
    el.textContent = T('settings.transcribe.pendingCloud',
      { count: this.pendingCloud });
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
    this._renderCloudHint();
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

    // Faixas isoladas: a gravação vira N transcrições. Sem dizer qual
    // está rodando, a barra parece travada por minutos a fio.
    if (this.trackCount > 1)
      txt += ' · ' + T('settings.transcribe.trackN',
        { n: this.track + 1, total: this.trackCount });

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


// =====================================================================
// Diagnóstico do servidor de transcrição (aba Configurações → Transcrição)
// =====================================================================
// O backend descobre EM ETAPAS por que o servidor não responde (rodando
// wsl.exe e docker.exe) e manda o resultado cru. Aqui só decidimos o que
// dizer: a tela mostra o PRIMEIRO degrau que falta, com a instrução dele e
// o comando pronto pra copiar — não uma lista de coisas que talvez estejam
// erradas.
//
// Ordem das etapas e por quê:
//   servidor responde  -> pronto; nada mais importa
//   host remoto        -> Docker local não tem nada a ver com isso
//   WSL                -> o Docker Desktop precisa dele
//   Docker instalado / em execução
//   container          -> ausente: `docker run`; PARADO: `docker start`.
//                         Mandar `docker run` com um container parado
//                         criaria um segundo brigando pela mesma porta.

const TRANSCRITOR_IMAGE = 'eduardo20041995/transcritor-api:latest';

const TranscribeSetup = {
  _seq: 0,
  _last: null,

  check() {
    const el = document.getElementById('settingsTranscribeHost');
    const host = el ? el.value.trim() : '';
    this._seq++;
    this._last = { checking: true };
    this._render(this._last);
    Bridge.send('check_transcribe_setup', { host: host, seq: this._seq });
  },

  apply(data) {
    // Resposta de uma verificação já superada (o usuário clicou de novo,
    // ou trocou de aba e voltou): descarta, senão o estado velho venceria.
    if (!data || data.seq !== this._seq) return;
    this._last = data;
    this._render(data);
  },

  // Troca de idioma: redesenha com os textos novos, sem verificar de novo.
  rerender() { if (this._last) this._render(this._last); },

  isHidden() {
    const box = document.getElementById('transcribeSetup');
    return !box || box.hidden;
  },

  action(name) { Bridge.send('transcribe_setup_action', { action: name }); },

  async copy(text, btn) {
    let ok = false;
    try { await navigator.clipboard.writeText(text); ok = true; } catch (e) {}
    if (!ok) {
      // Sem permissão de clipboard: o caminho antigo ainda funciona.
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try { ok = document.execCommand('copy'); } catch (e) {}
      ta.remove();
    }
    if (btn && ok) {
      btn.textContent = T('settings.transcribe.setup.copied');
      btn.disabled = true;
      setTimeout(() => {
        btn.textContent = T('settings.transcribe.setup.copy');
        btn.disabled = false;
      }, 1500);
    }
  },

  // ---- desenho ------------------------------------------------------

  _row(body, state, text) {
    const r = document.createElement('div');
    r.className = 'ts-row ' + state;
    const i = document.createElement('span');
    i.className = 'ts-icon';
    i.textContent = state === 'ok' ? '✓' : state === 'fail' ? '✕' : '…';
    const t = document.createElement('span');
    t.textContent = text;
    r.appendChild(i);
    r.appendChild(t);
    body.appendChild(r);
  },

  _detail(body) {
    const d = document.createElement('div');
    d.className = 'ts-detail';
    body.appendChild(d);
    return d;
  },

  _p(parent, text) {
    const p = document.createElement('p');
    p.textContent = text;
    parent.appendChild(p);
  },

  _cmd(parent, command) {
    const box = document.createElement('div');
    box.className = 'ts-cmd';
    const code = document.createElement('code');
    code.textContent = command;
    const btn = document.createElement('button');
    btn.className = 'settings-btn';
    btn.type = 'button';
    btn.textContent = T('settings.transcribe.setup.copy');
    btn.onclick = () => this.copy(command, btn);
    box.appendChild(code);
    box.appendChild(btn);
    parent.appendChild(box);
  },

  _button(parent, label, action) {
    const wrap = document.createElement('div');
    wrap.className = 'ts-actions';
    const btn = document.createElement('button');
    btn.className = 'settings-btn';
    btn.type = 'button';
    btn.textContent = label;
    btn.onclick = () => this.action(action);
    wrap.appendChild(btn);
    parent.appendChild(wrap);
  },

  _render(d) {
    const body = document.getElementById('transcribeSetupBody');
    const btn = document.getElementById('transcribeSetupCheckBtn');
    if (!body) return;
    body.textContent = '';
    if (btn) btn.disabled = !!d.checking;
    const S = (k, a) => T('settings.transcribe.setup.' + k, a);

    // SERVIDOR NO AR: o painel inteiro some — as etapas existem pra
    // orientar o que falta, e não falta nada. Verificando, fica como
    // estava: quem já estava escondido não pisca "verificando…" a cada
    // abertura da aba (o caso comum é o servidor estar de pé), e quem
    // mostrava uma falha continua visível enquanto re-verifica.
    const box = document.getElementById('transcribeSetup');
    if (d.checking) { this._row(body, 'wait', S('checking')); return; }
    if (box) box.hidden = !!d.serverOk;
    if (d.serverOk) return;
    if (!d.isLocal) {
      this._row(body, 'fail', S('remote'));
      this._p(this._detail(body), S('remoteHow', { error: d.serverError || '' }));
      return;
    }

    // WSL só entra na conversa enquanto o Docker não roda: com ele de pé,
    // o WSL obviamente está ok (ou o Docker usa outro backend).
    if (!d.dockerRunning) {
      this._row(body, d.wslOk ? 'ok' : 'fail', S(d.wslOk ? 'wslOk' : 'wslMissing'));
      if (!d.wslOk) {
        const det = this._detail(body);
        this._p(det, S('wslHow'));
        this._cmd(det, 'wsl --install');
        this._p(det, S('wslAfter'));
        return;
      }
    }

    this._row(body, d.dockerInstalled ? 'ok' : 'fail',
      S(d.dockerInstalled ? 'dockerOk' : 'dockerMissing'));
    if (!d.dockerInstalled) {
      const det = this._detail(body);
      this._p(det, S('dockerHow'));
      this._button(det, S('dockerDownload'), 'openDockerSite');
      this._p(det, S('dockerAfter'));
      return;
    }

    this._row(body, d.dockerRunning ? 'ok' : 'fail',
      S(d.dockerRunning ? 'runningOk' : 'runningMissing'));
    if (!d.dockerRunning) {
      const det = this._detail(body);
      this._p(det, S('runningHow'));
      if (d.canStartDocker) this._button(det, S('openDocker'), 'startDockerDesktop');
      return;
    }

    const port = d.port || 8000;
    if (!d.containerId) {
      this._row(body, 'fail', S('containerMissing'));
      const det = this._detail(body);
      this._p(det, S('containerHow'));
      this._cmd(det, 'docker run --restart=always -d -p ' + port +
        ':8000 -v transcritor-dados:/data ' + TRANSCRITOR_IMAGE);
      this._p(det, S('containerAfter'));
      return;
    }

    // Alguém usa a porta, mas não é o transcritor: não adianta mandar
    // `docker start` num container que não é o nosso.
    if (!/transcritor/i.test(d.containerImage || '')) {
      this._row(body, 'fail', S('portBusy', { port: port, image: d.containerImage }));
      this._p(this._detail(body), S('portBusyHow'));
      return;
    }

    const noRestart = (d.restartPolicy || '') !== 'always';
    const updateCmd = 'docker update --restart=always ' + d.containerId;

    if (d.containerState !== 'running') {
      this._row(body, 'fail', S('containerStopped', { status: d.containerStatus }));
      const det = this._detail(body);
      this._p(det, S('containerStartHow'));
      // Duas linhas, não `a && b`: o `&&` não existe no PowerShell 5, que é
      // onde muita gente vai colar. Colar várias linhas roda uma por vez.
      this._cmd(det, (noRestart ? updateCmd + '\n' : '') + 'docker start ' + d.containerId);
      if (noRestart) this._p(det, S('containerRestartWhy'));
      return;
    }

    this._row(body, 'ok', S('containerRunning', { status: d.containerStatus }));
    this._row(body, 'wait', S('serverLoading'));
    const det = this._detail(body);
    this._p(det, S('serverLoadingHow'));
    if (noRestart) {
      this._p(det, S('containerRestartTip'));
      this._cmd(det, updateCmd);
    }
  }
};


// =====================================================================
// Motor local (aba Configurações → Transcrição)
// =====================================================================
// "Onde transcrever": nesta máquina (OBSLocalAsr — Qwen3 + audio.cpp na
// placa de vídeo, instalado por aqui, sem Docker) ou num servidor da
// Transcritor API. O estado da instalação é do backend (local_asr_state);
// aqui só se desenha.
//
// O RÁDIO segue o fluxo das Configurações: só vale no Salvar (Settings
// manda set_transcribe_engine). A exceção é o backend trocar sozinho —
// terminar a instalação liga o motor local, porque quem instala quer usar
// — e aí o push traz `engine` e o rádio acompanha.
const LocalAsr = {
  state: null,        // último local_asr_state
  _confirmRemove: false,

  apply(data) {
    this.state = data;
    if (data.removeRefused)
      Toast.show(T('settings.transcribe.local.removeBusy'), '', { ttl: 4000, warn: true });
    // O backend mudou o motor (instalação terminou / motor removido):
    // o rádio e o "valor salvo" acompanham, senão o próximo Salvar
    // desfaria a troca.
    if (data.engine && typeof Settings !== 'undefined' &&
        data.engine !== Settings.currentTranscribeEngine) {
      Settings.currentTranscribeEngine = data.engine;
      this.setEngine(data.engine);
    }
    this.render();
  },

  // Motor marcado na TELA (pode ainda não ter sido salvo).
  selectedEngine() {
    const local = document.getElementById('settingsTranscribeEngineLocal');
    return local && local.checked ? 'local' : 'server';
  },

  setEngine(engine) {
    const local = document.getElementById('settingsTranscribeEngineLocal');
    const server = document.getElementById('settingsTranscribeEngineServer');
    if (local) local.checked = engine === 'local';
    if (server) server.checked = engine !== 'local';
    this.render();
  },

  onEngineChange() {
    this._confirmRemove = false;
    this.render();
    // Voltou pro servidor: o diagnóstico dele passa a importar de novo.
    if (this.selectedEngine() === 'server' && typeof TranscribeSetup !== 'undefined')
      TranscribeSetup.check();
  },

  install() { Bridge.send('local_asr_install', {}); },
  cancel()  { Bridge.send('local_asr_cancel', {}); },

  remove() {
    // Dois cliques: apagar 3,7 GB por engano custaria outro download.
    if (!this._confirmRemove) {
      this._confirmRemove = true;
      this.render();
      return;
    }
    this._confirmRemove = false;
    Bridge.send('local_asr_remove', {});
  },

  _size(bytes) {
    const gb = (bytes || 0) / 1e9;
    let txt;
    try {
      txt = new Intl.NumberFormat(I18n.language || 'pt-BR',
        { maximumFractionDigits: gb >= 10 ? 0 : 1 }).format(gb);
    } catch (e) { txt = gb.toFixed(1); }
    return txt + ' GB';
  },

  _button(parent, label, onClick) {
    const btn = document.createElement('button');
    btn.className = 'settings-btn';
    btn.type = 'button';
    btn.textContent = label;
    btn.onclick = onClick;
    parent.appendChild(btn);
    return btn;
  },

  render() {
    const isLocal = this.selectedEngine() === 'local';
    const box = document.getElementById('localAsrBox');
    const serverBox = document.getElementById('transcribeServerBox');
    if (serverBox) serverBox.hidden = isLocal;
    if (!box) return;
    box.hidden = !isLocal;
    if (!isLocal) return;

    const status = document.getElementById('localAsrStatus');
    const detail = document.getElementById('localAsrDetail');
    const actions = document.getElementById('localAsrActions');
    const bar = document.getElementById('localAsrBar');
    const fill = document.getElementById('localAsrFill');
    const L = (k, a) => T('settings.transcribe.local.' + k, a);
    const d = this.state || { status: 'missing', total: 0, done: 0 };
    status.className = 'local-asr-status';
    status.textContent = '';
    detail.textContent = '';
    actions.textContent = '';
    bar.hidden = true;

    if (d.status === 'installing') {
      const st = d.stage ? L('stages.' + d.stage) : '';
      status.textContent = L('installing', { stage: st });
      bar.hidden = false;
      const pct = d.total > 0 ? Math.min(100, d.done * 100 / d.total) : 0;
      fill.style.width = pct.toFixed(1) + '%';
      detail.textContent = L('progress', { done: this._size(d.done), total: this._size(d.total) }) +
        ' · ' + L('resumeHint');
      this._button(actions, L('cancel'), () => this.cancel());
      return;
    }

    if (d.status === 'ready') {
      status.classList.add('ok');
      status.textContent = d.device ? L('ready', { device: d.device }) : L('readyCpu');
      detail.textContent = L('readyHow', { size: this._size(d.total) });
      const btn = this._button(actions,
        this._confirmRemove ? L('removeConfirm') : L('remove'), () => this.remove());
      if (this._confirmRemove) btn.classList.add('danger');
      return;
    }

    // 'missing' ou 'error': o botão instala — e, depois de uma falha,
    // RETOMA o que já tinha baixado.
    if (d.status === 'error') {
      status.classList.add('fail');
      status.textContent = L('error', { error: d.error || '' });
    } else {
      status.textContent = L('missing');
    }
    detail.textContent = L('missingHow', { size: this._size(d.total) });
    this._button(actions, d.status === 'error' ? L('retry') : L('install'), () => this.install());
  }
};

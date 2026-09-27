// =====================================================================
// Filtros de audio do microfone (aba Audio das Configuracoes)
// =====================================================================
//
// A lista e o editor sao GENERICOS: o backend (OBSAudioFilters) descreve
// cada filtro do plugin obs-filters como {id, name, enabled, props:[...]},
// e aqui cada propriedade vira o controle do tipo dela (slider, numero,
// lista, caixa). Os rotulos vem do proprio plugin, ja no idioma do app —
// por isso nao ha chave de traducao por campo. A unica coisa nossa por
// filtro e a frase curta que diz pra que ele serve (settings.audioFilters.
// desc.<id>), que o plugin nao tem.
//
// Nada e deduzido aqui: toda mudanca vai pro backend e o filtro volta
// inteiro (mudar um campo pode mostrar ou esconder outros).
//
// TESTE A/B: o backend grava alguns segundos do microfone em duas copias,
// uma crua e outra com os filtros, e devolve as duas como data: URL. Nada
// entra na biblioteca.

const AudioFilters = {
  filters: [],
  mics: [],
  ready: false,
  loaded: false,
  testSec: 6,
  testing: false,
  _open: new Set(),        // ids com os ajustes abertos (sobrevive a re-render)
  _audio: null,
  _playing: '',
  _countdown: 0,
  _timer: 0,
  _clips: null,

  // Chamado ao entrar na aba Audio.
  request() {
    this._syncTestUi();
    this.render();
    Bridge.send('get_audio_filters', {});
  },

  apply(data) {
    this.ready = !!(data && data.ready);
    this.loaded = true;
    this.filters = Array.isArray(data && data.filters) ? data.filters : [];
    this.mics = Array.isArray(data && data.mics) ? data.mics : [];
    if (data && data.testSec) this.testSec = data.testSec;
    this._renderMics();
    this.render();
    this._syncTestUi();
  },

  // Um filtro so voltou (depois de ligar/mudar/restaurar).
  applyOne(data) {
    const f = data && data.filter;
    if (!f) return;
    const i = this.filters.findIndex(x => x.id === f.id);
    if (i >= 0) this.filters[i] = f; else this.filters.push(f);
    const row = document.querySelector('.afilter[data-id="' + CSS.escape(f.id) + '"]');
    if (row) row.replaceWith(this._renderFilter(f));
    else this.render();
    this._syncSummary();
    this._syncTestUi();
  },

  render() {
    const box = document.getElementById('afList');
    if (!box) return;
    box.innerHTML = '';
    if (!this.loaded) {
      box.appendChild(this._p(T('settings.audioFilters.loading'), 'settings-hint'));
      return;
    }
    if (!this.ready || this.filters.length === 0) {
      box.appendChild(this._p(T('settings.audioFilters.unavailable'), 'settings-warn'));
      return;
    }
    this.filters.forEach(f => box.appendChild(this._renderFilter(f)));
    this._syncSummary();
  },

  _syncSummary() {
    const el = document.getElementById('afSummary');
    if (!el) return;
    const n = this.filters.filter(f => f.enabled).length;
    el.textContent = n === 0 ? T('settings.audioFilters.noneOn')
                             : T('settings.audioFilters.nOn', { count: n });
  },

  _p(text, cls) {
    const d = document.createElement('div');
    d.className = cls || '';
    d.textContent = text;
    return d;
  },

  _renderFilter(f) {
    const row = document.createElement('div');
    row.className = 'afilter' + (f.enabled ? ' on' : '');
    row.dataset.id = f.id;

    const head = document.createElement('div');
    head.className = 'afilter-head';

    const lab = document.createElement('label');
    lab.className = 'settings-check-row';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = !!f.enabled;
    // Nao deixa o change subir pro listener do modal (commit de TODAS as
    // configuracoes a cada clique aqui seria trabalho a toa).
    cb.addEventListener('change', (e) => {
      e.stopPropagation();
      Bridge.send('set_audio_filter', { id: f.id, op: 'enable', value: cb.checked });
    });
    const name = document.createElement('span');
    name.textContent = f.name || f.id;
    lab.appendChild(cb);
    lab.appendChild(name);
    head.appendChild(lab);

    const hasProps = Array.isArray(f.props) && f.props.length > 0;
    if (hasProps) {
      const tog = document.createElement('button');
      tog.type = 'button';
      tog.className = 'settings-btn afilter-toggle';
      const open = this._open.has(f.id);
      tog.textContent = open ? T('settings.audioFilters.hideSettings')
                             : T('settings.audioFilters.showSettings');
      tog.addEventListener('click', () => {
        if (this._open.has(f.id)) this._open.delete(f.id); else this._open.add(f.id);
        row.replaceWith(this._renderFilter(f));
      });
      head.appendChild(tog);
    }
    row.appendChild(head);

    // Frase nossa, quando temos uma pra esse id.
    const desc = I18n.get('settings.audioFilters.desc.' + f.id);
    if (typeof desc === 'string' && desc)
      row.appendChild(this._p(desc, 'settings-hint afilter-desc'));

    if (hasProps && this._open.has(f.id)) {
      const body = document.createElement('div');
      body.className = 'afilter-body';
      this._renderProps(f, f.props, body);
      const reset = document.createElement('button');
      reset.type = 'button';
      reset.className = 'settings-btn afilter-reset';
      reset.textContent = T('settings.audioFilters.reset');
      reset.addEventListener('click', () =>
        Bridge.send('set_audio_filter', { id: f.id, op: 'reset' }));
      body.appendChild(reset);
      row.appendChild(body);
    }
    return row;
  },

  _renderProps(f, props, parent) {
    props.forEach(p => {
      if (p.type === 'group') {
        const g = document.createElement('div');
        g.className = 'afilter-group';
        if (p.label) g.appendChild(this._p(p.label, 'afilter-group-title'));
        this._renderProps(f, p.props || [], g);
        parent.appendChild(g);
        return;
      }
      if (p.type === 'info') {
        parent.appendChild(this._p(p.label, 'settings-hint'));
        return;
      }
      const field = document.createElement('div');
      field.className = 'afilter-prop';
      if (p.hint) field.title = p.hint;
      const send = (value) =>
        Bridge.send('set_audio_filter', { id: f.id, op: 'value', key: p.name, value: value });

      if (p.type === 'bool') {
        const lab = document.createElement('label');
        lab.className = 'settings-check-row';
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.checked = !!p.value;
        cb.disabled = p.enabled === false;
        cb.addEventListener('change', (e) => { e.stopPropagation(); send(cb.checked); });
        const sp = document.createElement('span');
        sp.textContent = p.label;
        lab.appendChild(cb);
        lab.appendChild(sp);
        field.appendChild(lab);
      } else if (p.type === 'list') {
        field.appendChild(this._p(p.label, 'afilter-prop-label'));
        const sel = document.createElement('select');
        sel.className = 'settings-input';
        sel.disabled = p.enabled === false;
        (p.items || []).forEach((it, i) => {
          const o = document.createElement('option');
          o.value = String(i);
          o.textContent = it.name;
          if (it.value === p.value) o.selected = true;
          sel.appendChild(o);
        });
        sel.addEventListener('change', (e) => {
          e.stopPropagation();
          const it = (p.items || [])[+sel.value];
          if (it) send(it.value);
        });
        field.appendChild(sel);
      } else if (p.type === 'int' || p.type === 'float') {
        field.appendChild(this._numberControl(p, send));
      } else {
        return;
      }
      parent.appendChild(field);
    });
  },

  // Slider (quando o plugin pede slider) com o valor ao lado, ou campo de
  // numero. Manda so ao SOLTAR (change), nao a cada pixel do arrasto.
  _numberControl(p, send) {
    const wrap = document.createElement('div');
    wrap.appendChild(this._p(p.label, 'afilter-prop-label'));
    const line = document.createElement('div');
    line.className = 'afilter-num';
    const isInt = p.type === 'int';
    const step = p.step > 0 ? p.step : (isInt ? 1 : 0.1);
    const decimals = isInt ? 0 : Math.max(0, Math.min(3,
      (String(step).split('.')[1] || '').length));
    const fmt = (v) => Number(v).toFixed(decimals) + (p.suffix || '');
    const clamp = (v) => Math.max(p.min, Math.min(p.max, v));

    if (p.slider) {
      const r = document.createElement('input');
      r.type = 'range';
      r.min = p.min; r.max = p.max; r.step = step;
      r.value = p.value;
      r.disabled = p.enabled === false;
      const val = document.createElement('span');
      val.className = 'afilter-num-val';
      val.textContent = fmt(p.value);
      const fill = () => {
        const span = (p.max - p.min) || 1;
        r.style.setProperty('--val', String((+r.value - p.min) / span));
      };
      fill();
      r.addEventListener('input', () => { val.textContent = fmt(r.value); fill(); });
      r.addEventListener('change', (e) => { e.stopPropagation(); send(+r.value); });
      line.appendChild(r);
      line.appendChild(val);
    } else {
      const n = document.createElement('input');
      n.type = 'number';
      n.className = 'settings-input';
      n.min = p.min; n.max = p.max; n.step = step;
      n.value = Number(p.value).toFixed(decimals);
      n.disabled = p.enabled === false;
      n.addEventListener('change', (e) => {
        e.stopPropagation();
        let v = parseFloat(n.value);
        if (!Number.isFinite(v)) { n.value = Number(p.value).toFixed(decimals); return; }
        v = clamp(v);
        n.value = v.toFixed(decimals);
        send(isInt ? Math.round(v) : v);
      });
      line.appendChild(n);
      if (p.suffix) line.appendChild(this._p(p.suffix.trim(), 'afilter-num-val'));
    }
    wrap.appendChild(line);
    return wrap;
  },

  // ---- teste A/B -----------------------------------------------------

  _renderMics() {
    const sel = document.getElementById('afTestMic');
    if (!sel) return;
    const prev = sel.value;
    sel.innerHTML = '';
    this.mics.forEach(m => {
      const o = document.createElement('option');
      o.value = m.id;
      o.textContent = m.isDefault ? T('settings.audioFilters.micDefault', { name: m.name })
                                  : m.name;
      sel.appendChild(o);
    });
    if (prev && this.mics.some(m => m.id === prev)) sel.value = prev;
  },

  test() {
    if (this.testing) return;
    this.stop();
    this._clips = null;
    const sel = document.getElementById('afTestMic');
    this.testing = true;
    this._countdown = this.testSec;
    this._syncTestUi();
    Bridge.send('test_audio_filters', { device: sel ? sel.value : '' });
  },

  onTest(data) {
    if (!data) return;
    if (data.state === 'recording') {
      this.testing = true;
      this._countdown = data.seconds || this.testSec;
      clearInterval(this._timer);
      this._timer = setInterval(() => {
        this._countdown = Math.max(0, this._countdown - 1);
        this._syncTestUi();
      }, 1000);
    } else if (data.state === 'done') {
      this.testing = false;
      clearInterval(this._timer);
      this._clips = {
        original: data.original, filtered: data.filtered,
        originalPeakDb: data.originalPeakDb, filteredPeakDb: data.filteredPeakDb,
        originalRmsDb: data.originalRmsDb, filteredRmsDb: data.filteredRmsDb,
        filters: data.filters | 0
      };
      // Toca o resultado com filtros direto: e o que o usuario quer ouvir.
      this.play('filtered');
    } else if (data.state === 'filtered' && this._clips) {
      // Mudou um filtro: o backend refez a versao filtrada a partir do MESMO
      // trecho. Se ela esta tocando, troca o som sem voltar pro comeco — e
      // assim que se compara um ajuste com o outro.
      this._clips.filtered = data.filtered;
      this._clips.filteredPeakDb = data.filteredPeakDb;
      this._clips.filteredRmsDb = data.filteredRmsDb;
      this._clips.filters = data.filters | 0;
      if (this._playing === 'filtered') this.play('filtered', true);
    } else if (data.state === 'error') {
      this.testing = false;
      clearInterval(this._timer);
      Toast.show(T('toast.errorTitle'), data.error || T('settings.audioFilters.testFailed'),
        { warn: true, ttl: 5000 });
    }
    this._syncTestUi();
  },

  // Alternar entre Original e Com filtros no meio da reproducao continua do
  // MESMO ponto: comparar o mesmo instante e o que torna a diferenca audivel.
  // AReload = a versao filtrada foi refeita e deve seguir tocando.
  play(which, AReload) {
    if (!this._clips || !this._clips[which]) return;
    if (this._playing === which && !AReload) { this.stop(); return; }
    const at = (this._audio && this._playing && !this._audio.ended)
      ? this._audio.currentTime : 0;
    this.stop();
    const a = new Audio(this._clips[which]);
    this._audio = a;
    this._playing = which;
    a.addEventListener('ended', () => {
      if (this._audio !== a) return;
      this._playing = ''; this._syncTestUi();
    });
    const start = () => {
      if (at > 0 && at < (a.duration || 0)) a.currentTime = at;
      a.play().catch(() => { if (this._audio === a) { this._playing = ''; this._syncTestUi(); } });
    };
    if (at > 0) a.addEventListener('loadedmetadata', start, { once: true });
    else start();
    this._syncTestUi();
  },

  stop() {
    if (this._audio) { try { this._audio.pause(); } catch (e) {} }
    this._audio = null;
    this._playing = '';
  },

  // Saindo da aba: nao deixa som tocando atras de outra tela.
  onLeave() {
    this.stop();
    this._syncTestUi();
  },

  // Nivel MEDIO e pico. O medio e o que a supressao de ruido mexe (os picos
  // sao a voz, que ela preserva) — so o pico fazia parecer que nada mudou.
  _level(rms, peak) {
    if (typeof peak !== 'number' || peak <= -99) return T('settings.audioFilters.silence');
    const r = (typeof rms === 'number' && rms > -99) ? rms.toFixed(1) : '−∞';
    return T('settings.audioFilters.level', { rms: r, peak: peak.toFixed(1) });
  },

  _syncTestUi() {
    const btn = document.getElementById('afTestBtn');
    const status = document.getElementById('afTestStatus');
    const res = document.getElementById('afTestResult');
    if (btn) {
      btn.disabled = this.testing || !this.ready;
      btn.textContent = T('settings.audioFilters.test', { sec: this.testSec });
    }
    if (status) {
      // Teste sem nenhum filtro ligado devolve duas copias iguais — o aviso
      // existe porque isso foi lido como "os filtros nao funcionam".
      let txt = '', warn = false;
      if (this.testing) txt = T('settings.audioFilters.testing', { sec: this._countdown });
      else if (this._clips && this._clips.filters === 0) {
        txt = T('settings.audioFilters.testedWithout'); warn = true;
      } else if (this._clips) txt = T('settings.audioFilters.compare', { count: this._clips.filters });
      else if (this.ready) txt = T('settings.audioFilters.howTo');
      status.textContent = txt;
      status.classList.toggle('recording', this.testing);
      status.classList.toggle('warn', warn);
    }
    if (res) {
      res.hidden = !this._clips || this.testing;
      if (this._clips) {
        const ob = document.getElementById('afPlayOriginal');
        const fb = document.getElementById('afPlayFiltered');
        if (ob) {
          ob.textContent = (this._playing === 'original' ? '■ ' : '▶ ') +
            T('settings.audioFilters.original');
          ob.classList.toggle('active', this._playing === 'original');
        }
        if (fb) {
          fb.textContent = (this._playing === 'filtered' ? '■ ' : '▶ ') +
            T('settings.audioFilters.filtered');
          fb.classList.toggle('active', this._playing === 'filtered');
        }
        const op = document.getElementById('afPeakOriginal');
        const fp = document.getElementById('afPeakFiltered');
        if (op) op.textContent = this._level(this._clips.originalRmsDb, this._clips.originalPeakDb);
        if (fp) fp.textContent = this._level(this._clips.filteredRmsDb, this._clips.filteredPeakDb);
      }
    }
  },

  // Troca de idioma: os rotulos dos campos vem do plugin, entao a lista
  // precisa voltar do backend.
  onLanguageChanged() {
    this._renderMics();
    this._syncTestUi();
    const panel = document.getElementById('afList');
    if (panel && panel.offsetParent !== null) this.request();
    else this.render();
  }
};

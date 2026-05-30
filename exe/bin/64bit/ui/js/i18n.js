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


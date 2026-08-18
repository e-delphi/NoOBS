// =====================================================================
// Pastas da biblioteca de gravações
// =====================================================================
// A lista de gravações virou navegável: as subpastas da pasta de
// gravação aparecem como cards antes dos grupos de data, e o usuário
// organiza arrastando um vídeo pra cima de uma pasta ou por
// recortar/colar no menu de contexto.
//
// O estado (onde estamos, o que tem aqui) NÃO é deduzido no cliente:
// chega inteiro no MESMO push que traz a lista (`init` /
// `recordings_loaded`), pra nunca divergir do que o backend acabou de
// listar. Toda operação de disco é do backend — aqui só há gesto.

const FOLDER_SVG =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
  'stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">' +
  '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>' +
  '</svg>';

const FOLDER_UP_SVG =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
  'stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">' +
  '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>' +
  '<polyline points="12 17 12 11"/><polyline points="9.5 13.5 12 11 14.5 13.5"/>' +
  '</svg>';

// Separador de caminho do Windows. Via escape unicode de propósito:
// uma barra invertida literal aqui é fácil de perder numa edição.
const PATH_SEP = '\u005C';

const RecFolders = {
  currentDir: '',
  parentDir : '',
  atRoot    : true,
  breadcrumb: [],
  items     : [],
  // Ids recortados esperando um "Colar". Só um recorte por vez —
  // recortar de novo substitui, como no Explorer.
  clipboard : [],
  // Ids sendo arrastados AGORA. O `dragover` não tem acesso aos dados do
  // dataTransfer (só o `drop` tem), então a decisão de aceitar ou não o
  // alvo precisa sair daqui.
  _dragIds  : null,

  // -------------------------------------------------------------
  // Estado vindo do backend
  // -------------------------------------------------------------
  apply(data) {
    if (!data) return;
    if (typeof data.currentDir === 'string') this.currentDir = data.currentDir;
    if (typeof data.parentDir  === 'string') this.parentDir  = data.parentDir;
    if (typeof data.atRoot     === 'boolean') this.atRoot    = data.atRoot;
    if (Array.isArray(data.breadcrumb)) this.breadcrumb = data.breadcrumb;
    if (Array.isArray(data.folders))    this.items      = data.folders;
    this.renderPath();
  },

  find(id) {
    const t = String(id || '').toLowerCase();
    return this.items.find(f => String(f.id).toLowerCase() === t) || null;
  },

  // -------------------------------------------------------------
  // Navegação
  // -------------------------------------------------------------
  open(id) {
    // Recorte pendente sobrevive à navegação de propósito: recortar aqui
    // e colar lá é justamente o fluxo que o menu de contexto oferece.
    Bridge.send('open_folder', { id: id || '' });
  },
  goUp() {
    if (this.atRoot) return;
    this.open(this.parentDir);
  },

  // -------------------------------------------------------------
  // Caminho (breadcrumb) acima da grade
  // -------------------------------------------------------------
  renderPath() {
    const bar = document.getElementById('recPath');
    if (!bar) return;
    bar.innerHTML = '';
    // Na raiz não há caminho a mostrar — a barra some pra não roubar
    // altura da grade.
    bar.hidden = this.atRoot;
    if (this.atRoot) return;

    const back = document.createElement('button');
    back.className = 'path-back';
    back.type = 'button';
    back.dataset.hint = T('recordings.folderUp');
    back.innerHTML =
      '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" ' +
      'stroke="currentColor" stroke-width="2.2" stroke-linecap="round" ' +
      'stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>';
    back.onclick = () => this.goUp();
    this._wireDrop(back, this.parentDir);
    bar.appendChild(back);

    this.breadcrumb.forEach((c, i) => {
      if (i > 0) {
        const sep = document.createElement('span');
        sep.className = 'path-sep';
        sep.textContent = '/';
        bar.appendChild(sep);
      }
      const el = document.createElement('span');
      el.className = 'path-crumb' + (i === this.breadcrumb.length - 1 ? ' current' : '');
      // A raiz é a pasta configurada nas Configurações; mostrar o nome
      // real dela ("Vídeos") confunde com uma subpasta qualquer.
      el.textContent = c.root ? T('recordings.libraryRoot') : c.name;
      el.dataset.hint = c.id;
      if (i < this.breadcrumb.length - 1) {
        el.onclick = () => this.open(c.root ? '' : c.id);
        this._wireDrop(el, c.id);
      }
      bar.appendChild(el);
    });
  },

  // -------------------------------------------------------------
  // Grupo de pastas, no topo da grade
  // -------------------------------------------------------------
  renderInto(grid) {
    if (!grid) return;
    // Raiz sem nenhuma subpasta: nada a desenhar. Fora da raiz o grupo
    // aparece sempre, porque o card de "voltar" mora nele.
    if (this.atRoot && this.items.length === 0) return;

    const wrap = document.createElement('div');
    wrap.className = 'rec-group rec-folders-group';
    wrap.dataset.groupKey = 'folders';

    const h = document.createElement('h3');
    h.className = 'rec-group-title';
    const span = document.createElement('span');
    span.textContent = T('recordings.foldersGroup');
    h.appendChild(span);

    const inner = document.createElement('div');
    inner.className = 'rec-group-grid';
    if (!this.atRoot) inner.appendChild(this._buildUpCard());
    this.items.forEach(f => inner.appendChild(this._buildCard(f)));

    wrap.appendChild(h);
    wrap.appendChild(inner);
    grid.appendChild(wrap);
  },

  _metaText(f) {
    const n = f.count || 0;
    const key = n === 1 ? 'recordings.fileCount_one' : 'recordings.fileCount_other';
    let s = T(key, { count: n });
    if (n > 0 && f.sizeText) s += ' · ' + f.sizeText;
    return s;
  },

  _buildUpCard() {
    const el = document.createElement('div');
    el.className = 'rec-folder up';
    el.dataset.id = this.parentDir;
    el.dataset.hint = T('recordings.folderUpHint');
    el.onclick = () => this.goUp();
    const icon = document.createElement('div');
    icon.className = 'folder-icon';
    icon.innerHTML = FOLDER_UP_SVG;
    const body = document.createElement('div');
    body.className = 'folder-body';
    const name = document.createElement('div');
    name.className = 'folder-name';
    name.textContent = '..';
    const meta = document.createElement('div');
    meta.className = 'folder-meta';
    meta.textContent = T('recordings.folderUp');
    body.appendChild(name);
    body.appendChild(meta);
    el.appendChild(icon);
    el.appendChild(body);
    // Soltar aqui move pra pasta de cima — é o único jeito de tirar algo
    // de dentro de uma pasta sem sair dela.
    this._wireDrop(el, this.parentDir);
    return el;
  },

  _buildCard(f) {
    const el = document.createElement('div');
    el.className = 'rec-folder';
    el.dataset.id = f.id;
    el.draggable = true;
    el.dataset.hint = f.id;
    el.onclick = () => {
      // Enquanto o nome está sendo editado o card não navega — senão o
      // clique pra posicionar o cursor no texto abriria a pasta.
      if (el.dataset.editing === 'true') return;
      this.open(f.id);
    };
    el.oncontextmenu = (e) => {
      e.preventDefault();
      e.stopPropagation();
      showCtxMenu(e.clientX, e.clientY, { kind: 'folder', id: f.id });
    };

    const icon = document.createElement('div');
    icon.className = 'folder-icon';
    icon.innerHTML = FOLDER_SVG;

    const body = document.createElement('div');
    body.className = 'folder-body';
    const name = document.createElement('div');
    name.className = 'folder-name';
    name.textContent = f.name;
    const meta = document.createElement('div');
    meta.className = 'folder-meta';
    meta.textContent = this._metaText(f);
    body.appendChild(name);
    body.appendChild(meta);

    el.appendChild(icon);
    el.appendChild(body);

    this._wireDrag(el, f.id);
    this._wireDrop(el, f.id);
    if (this.clipboard.indexOf(f.id) >= 0) el.classList.add('cut');
    return el;
  },

  // -------------------------------------------------------------
  // Criar / renomear / excluir
  // -------------------------------------------------------------
  create() {
    // O backend cria com o nome padrão (deduplicando "(2)", "(3)"…) e
    // responde `folder_created`; só então entramos na edição do nome —
    // o card precisa existir no DOM pra receber o cursor.
    Bridge.send('create_folder', { name: T('recordings.newFolderName') });
  },

  onCreated(id) {
    const el = document.querySelector(
      '#recGrid .rec-folder[data-id="' + cssEscape(id) + '"]');
    if (!el) return;
    try { el.scrollIntoView({ block: 'nearest' }); } catch (e) {}
    this.beginRename(el);
  },

  renameById(id) {
    const el = document.querySelector(
      '#recGrid .rec-folder[data-id="' + cssEscape(id) + '"]');
    if (el) this.beginRename(el);
  },

  // Edição no próprio card, igual ao rename de gravação (editName em
  // recordings.js). Duplicado de propósito: aquele fala `rename_recording`
  // e mexe no `.when`; aqui o alvo é `.folder-name` e a mensagem é outra.
  beginRename(card) {
    const el = card && card.querySelector('.folder-name');
    if (!el || el.classList.contains('editing')) return;
    const id = card.dataset.id;
    const original = el.textContent;
    card.dataset.editing = 'true';
    card.draggable = false;   // arrastar seleciona texto durante a edição
    el.classList.add('editing');
    el.contentEditable = 'true';
    el.focus();

    const range = document.createRange();
    range.selectNodeContents(el);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);

    const finish = (commit) => {
      el.removeEventListener('blur', onBlur);
      el.removeEventListener('keydown', onKey);
      el.contentEditable = 'false';
      el.classList.remove('editing');
      card.draggable = true;
      // O clique que tira o foco também chega no card; sair do modo de
      // edição só no próximo tick evita que ele navegue pra pasta.
      setTimeout(() => { delete card.dataset.editing; }, 0);
      if (!commit) { el.textContent = original; finishRecordingsRender(false); return; }
      const newName = el.textContent
        .replace(/[\r\n]+/g, ' ')
        .replace(/[\u005C/:*?"<>|]/g, '')
        .trim()
        .slice(0, 150);
      if (newName === '' || newName === original) {
        el.textContent = original;
        finishRecordingsRender(false);
        return;
      }
      el.textContent = newName;
      Bridge.send('rename_folder', { id, newName });
      finishRecordingsRender(true);
    };
    const onBlur = () => finish(true);
    const onKey = (ev) => {
      ev.stopPropagation();
      if (ev.key === 'Enter') { ev.preventDefault(); el.blur(); }
      else if (ev.key === 'Escape') { ev.preventDefault(); finish(false); }
    };
    el.addEventListener('blur', onBlur);
    el.addEventListener('keydown', onKey);
  },

  remove(id) {
    const f = this.find(id);
    const name = f ? f.name : id;
    const count = f ? (f.count || 0) : 0;
    Confirm.open({
      title: T('recordings.confirmDeleteFolderTitle'),
      // Pasta com conteúdo é o caso que precisa de aviso: o que vai pra
      // Lixeira não é "uma pasta", são N gravações junto com ela.
      message: count === 0
        ? T('recordings.confirmDeleteFolderEmpty', { name: name })
        : T(count === 1 ? 'recordings.confirmDeleteFolder_one'
                        : 'recordings.confirmDeleteFolder_other',
            { name: name, count: count }),
      okLabel: T('common.delete'),
      icon: 'delete',
      danger: true,
      onOk: () => {
        this.clipboard = this.clipboard.filter(x => x !== id);
        Bridge.send('delete_folder', { id: id });
      }
    });
  },

  // -------------------------------------------------------------
  // Recortar / colar
  // -------------------------------------------------------------
  cut(ids) {
    ids = (ids || []).filter(Boolean);
    if (!ids.length) return;
    this._clearCutMarks();
    this.clipboard = ids.slice();
    this._markCut();
    Toast.show(T('toast.cutTitle'),
      ids.length === 1 ? T('toast.cutOne') : T('toast.cutN', { count: ids.length }),
      { ttl: 3500 });
  },

  canPaste() { return this.clipboard.length > 0; },

  paste(target) {
    if (!this.clipboard.length) return;
    const ids = this.clipboard.slice();
    this.clipboard = [];
    this._clearCutMarks();
    // target vazio = pasta que está aberta ("Colar aqui").
    Bridge.send('move_items', { ids: ids, target: target || '' });
  },

  _markCut() {
    this.clipboard.forEach(id => {
      const esc = cssEscape(id);
      const el = document.querySelector(
        '#recGrid .rec-card[data-id="' + esc + '"], ' +
        '#recGrid .rec-folder[data-id="' + esc + '"]');
      if (el) el.classList.add('cut');
    });
  },

  _clearCutMarks() {
    document.querySelectorAll('#recGrid .cut').forEach(el => el.classList.remove('cut'));
  },

  // Ids sobre os quais uma ação do menu de contexto deve agir: a seleção
  // inteira quando o item clicado faz parte dela, senão só ele. Mesma
  // regra que o "Excluir" do menu já usava.
  targetIds(id) {
    if (typeof RecSelection !== 'undefined' &&
        RecSelection.size() > 1 && RecSelection.has(id))
      return RecSelection.all();
    return [id];
  },

  // -------------------------------------------------------------
  // Arrastar e soltar
  // -------------------------------------------------------------
  // Chamado por buildRecCard (recordings.js) e pelos cards de pasta.
  _wireDrag(el, id) {
    el.draggable = true;
    el.addEventListener('dragstart', (e) => {
      // Arrastar durante a edição do nome é seleção de texto, não move.
      if (el.dataset.editing === 'true') { e.preventDefault(); return; }
      this._dragIds = this.targetIds(id);
      el.classList.add('dragging');
      try {
        e.dataTransfer.effectAllowed = 'move';
        // Sem setData o Chromium cancela o arrasto antes do primeiro
        // dragover. O conteúdo em si não é lido por ninguém.
        e.dataTransfer.setData('text/plain', this._dragIds.join('\n'));
      } catch (err) {}
    });
    el.addEventListener('dragend', () => {
      el.classList.remove('dragging');
      this._dragIds = null;
      document.querySelectorAll('.drop-target')
        .forEach(t => t.classList.remove('drop-target'));
    });
  },

  // Alvos: cards de pasta, o card de "voltar" e cada pedaço do caminho.
  _wireDrop(el, targetId) {
    el.addEventListener('dragover', (e) => {
      if (!this._canDrop(targetId)) return;
      e.preventDefault();
      try { e.dataTransfer.dropEffect = 'move'; } catch (err) {}
      el.classList.add('drop-target');
    });
    el.addEventListener('dragleave', () => el.classList.remove('drop-target'));
    el.addEventListener('drop', (e) => {
      el.classList.remove('drop-target');
      if (!this._canDrop(targetId)) return;
      e.preventDefault();
      const ids = this._dragIds.slice();
      this._dragIds = null;
      // Solto: a seleção que existia não descreve mais o que está na
      // tela (os cards vão sumir daqui), então limpa.
      if (typeof RecSelection !== 'undefined') RecSelection.clear();
      Bridge.send('move_items', { ids: ids, target: targetId || '' });
    });
  },

  _canDrop(targetId) {
    const ids = this._dragIds;
    if (!ids || !ids.length) return false;
    const t = String(targetId || '').toLowerCase();
    if (!t) return false;
    for (let i = 0; i < ids.length; i++) {
      const s = String(ids[i]).toLowerCase();
      // A própria pasta, ou uma pasta dentro dela mesma: mover faria a
      // árvore sumir dentro da origem. O backend também recusa.
      if (s === t) return false;
      if (t.indexOf(s + PATH_SEP) === 0) return false;
      // Já está no destino — o alvo não deve nem acender.
      const cut = s.lastIndexOf(PATH_SEP);
      if (cut > 0 && s.slice(0, cut) === t) return false;
    }
    return true;
  }
};

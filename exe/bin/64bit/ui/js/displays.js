// =====================================================================
// Render: sources (monitor / mic / speaker)
// =====================================================================
function updateRecordingMeta(data) {
  if (!data || !data.id) return;
  const card = document.querySelector(`#recGrid .rec-card[data-id="${CSS.escape(data.id)}"]`);
  if (!card) return;
  const thumb = card.querySelector('.thumb');
  if (!thumb) return;

  // Thumb: substitui o placeholder "▶" por <img>. textContent=''
  // apagaria o .rec-check / .duration; entao removemos so o placeholder.
  if (data.thumb) {
    let img = thumb.querySelector('img');
    if (!img) {
      const ph = thumb.querySelector('.thumb-placeholder');
      if (ph) ph.remove();
      img = document.createElement('img');
      img.alt = '';
      img.className = 'thumb-img';
      thumb.insertBefore(img, thumb.firstChild);
    }
    if (img.src !== data.thumb) img.src = data.thumb;
  }

  // Duracao: cria/atualiza badge.
  if (data.duration && data.duration > 0) {
    let dur = thumb.querySelector('.duration');
    if (!dur) {
      dur = document.createElement('span');
      dur.className = 'duration';
      thumb.appendChild(dur);
    }
    dur.textContent = formatDuration(data.duration);
  }
}

// Converte amplitude linear 0..1 pra largura 0..1 numa curva dB-friendly.
// -60dB vira 0%, 0dB vira 100% (clamp). Silencio absoluto = 0.
function ampToBar(v) {
  if (!v || v <= 0) return 0;
  const db = 20 * Math.log10(v);
  return Math.max(0, Math.min(1, (db + 60) / 60));
}

function updateAudioMeters(items) {
  if (!items) return;
  items.forEach(it => {
    const card = document.querySelector(`.source-item[data-id="${CSS.escape(it.id)}"]`);
    if (!card) return;
    const meter = card.querySelector('.source-meter');
    if (!meter) return;
    const ch = (it.channels && it.channels >= 2) ? 2 : 1;
    meter.dataset.channels = String(ch);

    const fillL = meter.querySelector('.source-meter-track[data-ch="l"] .source-meter-fill');
    const fillR = meter.querySelector('.source-meter-track[data-ch="r"] .source-meter-fill');

    // Backend manda left/right separados. Fallback: usa level.
    const vL = ampToBar(typeof it.left  === 'number' ? it.left  : it.level);
    const vR = ampToBar(typeof it.right === 'number' ? it.right : it.level);

    // Cover desliza da esquerda (sinal zero = cobre tudo) pra direita
    // (sinal 1.0 = cover colapsado, gradient inteiro visivel).
    if (fillL) fillL.style.left = (vL * 100) + '%';
    if (fillR) fillR.style.left = (vR * 100) + '%';
  });
}

function updateMonitorThumbs(items) {
  if (!items || !items.length) return;
  Displays.updateThumbs(items);
}

// =====================================================================
// Render: Displays (monitores + webcams num layout visual proporcional)
// =====================================================================
const Displays = {
  monitors: [],
  webcams:  [],
  _observer: null,

  init() {
    const root = document.getElementById('displayLayout');
    if (!root || this._observer || typeof ResizeObserver === 'undefined') return;
    // Re-renderiza quando o container muda de tamanho (maximizar,
    // resize de janela, mudanca de aside). Sem isso os rects ficam
    // calculados no tamanho antigo e desalinhados.
    let raf = 0;
    this._observer = new ResizeObserver(() => {
      // rAF coalesce — evita render em rajada durante o drag de resize.
      if (raf) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => { raf = 0; this.render(); });
    });
    this._observer.observe(root);
  },

  render() {
    const root = document.getElementById('displayLayout');
    if (!root) return;
    root.innerHTML = '';

    const monW = (m) => Number(m.width)  || 0;
    const monH = (m) => Number(m.height) || 0;
    const monX = (m) => Number(m.x) || 0;
    const monY = (m) => Number(m.y) || 0;

    if (this.monitors.length === 0 && this.webcams.length === 0) {
      this._updateCount();
      return;
    }

    // 1. Bounding dos monitores em coordenadas do desktop.
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    this.monitors.forEach(m => {
      const x = monX(m), y = monY(m), w = monW(m), h = monH(m);
      if (x < minX) minX = x;
      if (y < minY) minY = y;
      if (x + w > maxX) maxX = x + w;
      if (y + h > maxY) maxY = y + h;
    });
    if (!isFinite(minX)) { minX = 0; minY = 0; maxX = 0; maxY = 0; }

    // 2. Posiciona webcams a direita do bounding dos monitores, em y=0,
    //    empilhadas horizontalmente. Reflete o que BuildRecordingScene
    //    faz com elas no canvas final.
    const camRects = [];
    let camX = maxX;
    this.webcams.forEach(c => {
      const w = Number(c.width)  || 1280;
      const h = Number(c.height) || 720;
      camRects.push({ cam: c, x: camX, y: 0, w, h });
      camX += w;
    });
    if (camRects.length > 0) {
      if (camX > maxX) maxX = camX;
      // y=0 ja esta dentro de [minY, maxY]; altura da cam pode
      // ultrapassar maxY se monitores fossem menores — improvavel,
      // mas trata por seguranca.
      camRects.forEach(r => { if (r.y + r.h > maxY) maxY = r.y + r.h; });
    }

    const totalW = Math.max(1, maxX - minX);
    const totalH = Math.max(1, maxY - minY);
    const boxW = root.clientWidth  || 360;
    const boxH = root.clientHeight || 200;
    const pad = 8; // margem interna pra rects nao colarem na borda
    const availW = boxW - pad * 2;
    const availH = boxH - pad * 2;
    // Escala uniforme pra manter aspect ratio e caber dentro.
    const scale = Math.min(availW / totalW, availH / totalH);
    // Centraliza o conteudo no box.
    const offsetX = pad + (availW - totalW * scale) / 2;
    const offsetY = pad + (availH - totalH * scale) / 2;

    // 3. Cria rect pra cada monitor.
    this.monitors.forEach(m => {
      const rect = document.createElement('div');
      rect.className = 'display-rect';
      rect.dataset.type = 'monitor';
      rect.dataset.id = m.id;
      rect.dataset.kind = 'monitor';
      rect.dataset.enabled = m.enabled ? 'true' : 'false';
      rect.style.left   = (offsetX + (monX(m) - minX) * scale) + 'px';
      rect.style.top    = (offsetY + (monY(m) - minY) * scale) + 'px';
      rect.style.width  = (monW(m) * scale) + 'px';
      rect.style.height = (monH(m) * scale) + 'px';
      // <img> filho — preview ao vivo. updateThumbs so troca o src.
      const img = document.createElement('img');
      img.className = 'display-thumb';
      img.alt = '';
      if (m.thumb) img.src = m.thumb;
      rect.appendChild(img);
      const label = document.createElement('div');
      label.className = 'display-label';
      label.textContent = m.name || T('display.monitor');
      rect.appendChild(label);
      this._appendCheck(rect);
      rect.onclick = () => this._toggle(rect, m, 'monitor');
      root.appendChild(rect);
    });

    // 4. Cria rect pra cada webcam.
    camRects.forEach(r => {
      const c = r.cam;
      const rect = document.createElement('div');
      rect.className = 'display-rect';
      rect.dataset.type = 'webcam';
      rect.dataset.id = c.id;
      rect.dataset.kind = 'webcam';
      rect.dataset.enabled = c.enabled ? 'true' : 'false';
      rect.style.left   = (offsetX + (r.x - minX) * scale) + 'px';
      rect.style.top    = (offsetY + (r.y - minY) * scale) + 'px';
      rect.style.width  = (r.w * scale) + 'px';
      rect.style.height = (r.h * scale) + 'px';
      // Icone de camera no centro.
      const icon = document.createElementNS('http://www.w3.org/2000/svg','svg');
      icon.setAttribute('class', 'display-cam-icon');
      icon.setAttribute('viewBox', '0 0 24 24');
      icon.setAttribute('fill', 'none');
      icon.setAttribute('stroke', 'rgba(255,255,255,0.85)');
      icon.setAttribute('stroke-width', '2');
      icon.setAttribute('stroke-linecap', 'round');
      icon.setAttribute('stroke-linejoin', 'round');
      icon.innerHTML =
        '<path d="M23 7l-7 5 7 5V7z"/>' +
        '<rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>';
      rect.appendChild(icon);
      const label = document.createElement('div');
      label.className = 'display-label';
      label.textContent = c.name || T('display.webcam');
      label.style.position = 'absolute';
      label.style.bottom = '4px';
      label.style.left = '4px';
      rect.appendChild(label);
      this._appendCheck(rect);
      rect.onclick = () => this._toggle(rect, c, 'webcam');
      root.appendChild(rect);
    });

    this._updateCount();
  },

  // Atualiza so o background-image dos rects de monitor existentes —
  // chamado pelo timer do backend a cada 1s pra mostrar preview ao
  // vivo sem reconstruir o DOM (que causaria flicker).
  updateThumbs(items) {
    const root = document.getElementById('displayLayout');
    if (!root) return;
    const byId = {};
    root.querySelectorAll('.display-rect').forEach(r => {
      byId[r.dataset.id] = r;
    });
    items.forEach(it => {
      const m = this.monitors.find(x => x.id === it.id);
      const rect = byId[it.id];
      if (!rect) return;
      // thumb vazia = backend sinalizou que o monitor sumiu (desplug
      // durante gravacao). Remove o <img> e o cache pra mostrar
      // o placeholder/fundo preto em vez da ultima imagem capturada.
      if (!it.thumb) {
        if (m) m.thumb = '';
        const stale = rect.querySelector('img.display-thumb');
        if (stale) stale.remove();
        return;
      }
      if (m) m.thumb = it.thumb;
      let img = rect.querySelector('img.display-thumb');
      if (!img) {
        img = document.createElement('img');
        img.className = 'display-thumb';
        img.alt = '';
        // <img> primeiro na ordem do DOM pra ficar atras de label/check.
        rect.insertBefore(img, rect.firstChild);
      }
      if (img.src !== it.thumb) img.src = it.thumb;
    });
  },

  _appendCheck(rect) {
    const check = document.createElement('div');
    check.className = 'display-check';
    check.innerHTML = CHECK_SVG;
    rect.appendChild(check);
  },

  _toggle(rect, item, kind) {
    const newEnabled = rect.dataset.enabled !== 'true';
    rect.dataset.enabled = newEnabled ? 'true' : 'false';
    item.enabled = newEnabled;
    Bridge.send('toggle_source', {
      kind: kind,
      id: item.id,
      enabled: newEnabled
    });
    this._updateCount();
    updateRecordButtonAvailability();
  },

  _updateCount() {
    const total = this.monitors.length + this.webcams.length;
    const sel = this.monitors.filter(m => m.enabled).length
              + this.webcams.filter(c => c.enabled).length;
    const el = document.getElementById('monCount');
    if (el) el.textContent = `${sel} / ${total}`;

    // Resolucao total = bounding dos itens habilitados (mesma logica
    // do BuildRecordingScene/canvas). Layout side-by-side: largura =
    // soma das larguras, altura = max das alturas. Mostra "—" se nada
    // selecionado.
    //
    // Quando a resolucao agregada excede o limite do codec atual,
    // injeta um icone de aviso (triangulo laranja) com tooltip via
    // <title> explicando que o canvas vai ser reduzido.
    const meta = document.getElementById('monMeta');
    if (meta) {
      let w = 0, h = 0;
      this.monitors.forEach(m => {
        if (!m.enabled) return;
        w += Number(m.width)  || 0;
        const mh = Number(m.height) || 0;
        if (mh > h) h = mh;
      });
      this.webcams.forEach(c => {
        if (!c.enabled) return;
        w += Number(c.width)  || 0;
        const ch = Number(c.height) || 0;
        if (ch > h) h = ch;
      });
      if (w > 0 && h > 0) {
        let html = `${w}×${h}`;
        const codec = (Settings && Settings.currentCodec) || 'auto';
        const caps = (Settings && Settings.encoderCaps) || {};
        const maxDim = codecMaxDim(codec, caps);
        if (w > maxDim || h > maxDim) html += codecLimitWarningIcon(maxDim);
        meta.innerHTML = html;
      } else {
        meta.textContent = '';
      }
    }
  }
};

// Atribuicao de tracks e calculada exclusivamente no Delphi
// (LibOBSEngine.ComputeAudioTrackAssignments). UI recebe item.track ja
// pronto em audio_sources_refreshed e renderiza direto. Sem
// re-implementacao duplicada aqui — single source of truth.

// Rebuild da legenda a partir do DOM. data-track ja vem aplicado
// no render via item.track do JSON.
function refreshTrackColors() {
  buildTrackLegend();
}

// Monta a legenda das cores no rodape da sidebar. Le data-track dos
// dispositivos ja renderizados, agrupa por numero de track e mostra
// so o que tiver presente. Track 1 (mix) nao aparece — devices nunca
// tem data-track="1" (mix nao colore o bar).
//
// Quando varios devices compartilham a mesma track (grouping ativado
// porque excedeu 5 sources), mostra contagem em vez de listar nomes.
function buildTrackLegend() {
  const legend = document.getElementById('trackLegend');
  if (!legend) return;

  // Coleta info por track: nomes, se contem default, kind (mic/spk).
  // trackInfo[t] = { names: [...], hasDefault: bool, kinds: Set }
  const trackInfo = new Map();
  const disabledNames = [];
  ['micList', 'spkList'].forEach(listId => {
    const list = document.getElementById(listId);
    if (!list) return;
    const kindShort = listId === 'micList' ? 'mic' : 'spk';
    list.querySelectorAll('.source-item').forEach(el => {
      const nameEl = el.querySelector('.source-name');
      const name = nameEl ? (nameEl.textContent || '').trim() : '';
      if (!name) return;
      const t = parseInt(el.dataset.track || '0', 10);
      const isDefault = el.dataset.default === 'true' &&
                        el.classList.contains('selected');
      if (t >= 2) {
        if (!trackInfo.has(t))
          trackInfo.set(t, { names: [], hasDefault: false, kinds: new Set() });
        const info = trackInfo.get(t);
        info.names.push(name);
        info.kinds.add(kindShort);
        if (isDefault) info.hasDefault = true;
      } else if (!el.classList.contains('selected')) {
        // Sem track + nao selecionado = desabilitado pelo user, fora
        // de qualquer faixa de gravacao.
        disabledNames.push(name);
      }
    });
  });

  const hasDefault = document.querySelector(
    '#micList .source-default-dot, #spkList .source-default-dot');

  if (trackInfo.size === 0 && disabledNames.length === 0 && !hasDefault) {
    legend.hidden = true;
    legend.innerHTML = '';
    return;
  }

  // Categoriza tracks pela ordem que o user pediu:
  //   1. Padrão (item explicativo)
  //   2. Faixa 1 (Mix)
  //   3. Tracks com dispositivo padrão (mics e outputs misturados)
  //   4. Tracks de microfone (sem default)
  //   5. Tracks de output (sem default, inclui agrupadas)
  //   6. Sem faixa (item explicativo)
  const allTracks = [...trackInfo.keys()].sort((a, b) => a - b);
  const defaultTracks = allTracks.filter(t => trackInfo.get(t).hasDefault);
  const micTracks = allTracks.filter(t =>
    !trackInfo.get(t).hasDefault &&
    trackInfo.get(t).kinds.has('mic') &&
    !trackInfo.get(t).kinds.has('spk'));
  const spkTracks = allTracks.filter(t =>
    !trackInfo.get(t).hasDefault &&
    trackInfo.get(t).kinds.has('spk'));

  function renderTrack(t) {
    const info = trackInfo.get(t);
    const names = info.names;
    let label;
    if (names.length === 1) label = names[0];
    else if (names.length === 2) label = names.join(', ');
    else label = T('tracks.devicesGrouped_other', { count: names.length });
    return '<div class="track-legend-item" data-track="' + t + '" data-hint="' +
           escapeHtml(names.join(', ')) + '">' +
           '<span class="track-legend-color"></span>' +
           '<span class="track-legend-name">' +
           '<span class="track-legend-track">' + T('tracks.track', { n: t }) + '</span>' +
           escapeHtml(label) +
           '</span></div>';
  }

  let html = '<div class="track-legend-title">' + T('tracks.title') + '</div>';

  // 1. Padrão (explicativo) — quando ha device default visivel.
  if (hasDefault) {
    html += '<div class="track-legend-item" data-track="default" ' +
            'data-hint="' + escapeHtml(T('tracks.defaultHint')) + '">' +
            '<span class="track-legend-color"></span>' +
            '<span class="track-legend-name">' +
            '<span class="track-legend-track">' + T('tracks.default') + '</span>' +
            escapeHtml(T('tracks.defaultDevice')) +
            '</span></div>';
  }
  // 2. Faixa 1 (Mix) — quando ha alguma track ativa.
  if (allTracks.length > 0) {
    html += '<div class="track-legend-item" data-track="mix" ' +
            'data-hint="' + escapeHtml(T('tracks.mixHint')) + '">' +
            '<span class="track-legend-color"></span>' +
            '<span class="track-legend-name">' +
            '<span class="track-legend-track">' + T('tracks.mix') + '</span>' +
            escapeHtml(T('tracks.mixLabel')) +
            '</span></div>';
  }
  // 3. Tracks dos dispositivos padrão.
  defaultTracks.forEach(t => { html += renderTrack(t); });
  // 4. Tracks de microfone (sem default).
  micTracks.forEach(t => { html += renderTrack(t); });
  // 5. Tracks de output (sem default).
  spkTracks.forEach(t => { html += renderTrack(t); });
  // 6. Sem faixa (desabilitados).
  if (disabledNames.length > 0) {
    const label = disabledNames.length === 1
      ? disabledNames[0]
      : T('tracks.devicesDisabled_other', { count: disabledNames.length });
    html += '<div class="track-legend-item" data-track="off" data-hint="' +
            escapeHtml(disabledNames.join(', ')) + '">' +
            '<span class="track-legend-color"></span>' +
            '<span class="track-legend-name">' +
            '<span class="track-legend-track">' + T('tracks.noTrack') + '</span>' +
            escapeHtml(label) +
            '</span></div>';
  }
  legend.innerHTML = html;
  legend.hidden = false;
}

function renderSources(kindShort, listId, countId, items) {
  const list = document.getElementById(listId);
  list.innerHTML = '';
  const arr = items || [];
  // Tracks sao re-calculados a partir do estado enabled lido do DOM
  // (refreshTrackColors no fim). Suporta toggles em tempo real.
  arr.forEach((item, idx) => {
    const div = document.createElement('div');
    div.className = 'source-item' + (item.enabled ? ' selected' : '');
    div.dataset.id = item.id;
    if (item.isDefault) div.dataset.default = 'true';
    div.dataset.kind = kindShort === 'mon' ? 'monitor'
                     : kindShort === 'mic' ? 'mic'
                     : kindShort === 'cam' ? 'webcam' : 'speaker';
    // Track-color bar pro lado esquerdo (so mics e speakers). O numero
    // vem direto do JSON — computado no Delphi (single source of truth).
    if ((kindShort === 'mic' || kindShort === 'spk') &&
        typeof item.track === 'number' && item.track > 0) {
      div.dataset.track = String(item.track);
    }
    div.onclick = () => onToggleSource(div);

    if (item.thumb) {
      const thumb = document.createElement('div');
      thumb.className = 'source-thumb';
      const img = document.createElement('img');
      img.src = item.thumb;
      img.alt = '';
      thumb.appendChild(img);
      div.appendChild(thumb);
    }

    const meta = document.createElement('div');
    meta.className = 'source-meta';
    const name = document.createElement('div');
    name.className = 'source-name';
    if (item.isDefault) {
      const dot = document.createElement('span');
      dot.className = 'source-default-dot';
      dot.title = T('sources.defaultDevice');
      name.appendChild(dot);
    }
    // Dispositivos Bluetooth ganham dois icones:
    //   1) Logo BT em azul — informativo, "isto eh um device Bluetooth"
    //   2) Triangulo ambar — sinaliza o problema de qualidade HFP, hint
    //      detalhado explica a limitacao (texto varia pra mic vs spk).
    // Ambos com vertical-align: middle (mesma estrategia da bolinha
    // default) pra centralizar com o nome.
    if (item.isBluetooth) {
      const bt = document.createElement('span');
      bt.className = 'source-bt-icon';
      bt.setAttribute('data-hint', T('sources.btDevice'));
      bt.innerHTML =
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
        'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M6.5 6.5l11 11L12 23V1l5.5 5.5-11 11"/></svg>';
      name.appendChild(bt);

      const warn = document.createElement('span');
      warn.className = 'source-bt-warn';
      warn.setAttribute('data-hint', (kindShort === 'mic')
        ? T('sources.btMicWarn') : T('sources.btSpeakerWarn'));
      // Mesmo SVG do LOW_DISK_ICON: triangulo com ! interno.
      warn.innerHTML =
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
        'stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>' +
        '<line x1="12" y1="9" x2="12" y2="13"/>' +
        '<line x1="12" y1="17" x2="12.01" y2="17"/></svg>';
      name.appendChild(warn);
    }
    name.appendChild(document.createTextNode(item.name || item.id));
    meta.appendChild(name);
    if (item.info) {
      const info = document.createElement('div');
      info.className = 'source-info';
      info.textContent = item.info;
      meta.appendChild(info);
    }
    if (kindShort === 'mic' || kindShort === 'spk') {
      // Estrutura L+R sempre — atributo data-channels controla layout:
      // "1" esconde R e estende L; "2" mostra os dois lado a lado.
      // updateAudioMeters seta channels conforme o que o WASAPI reporta.
      const meter = document.createElement('div');
      meter.className = 'source-meter';
      meter.dataset.channels = '2';
      const trackL = document.createElement('div');
      trackL.className = 'source-meter-track';
      trackL.dataset.ch = 'l';
      const fillL = document.createElement('div');
      fillL.className = 'source-meter-fill';
      trackL.appendChild(fillL);
      const trackR = document.createElement('div');
      trackR.className = 'source-meter-track';
      trackR.dataset.ch = 'r';
      const fillR = document.createElement('div');
      fillR.className = 'source-meter-fill';
      trackR.appendChild(fillR);
      meter.appendChild(trackL);
      meter.appendChild(trackR);
      meta.appendChild(meter);
    }

    const tg = document.createElement('div');
    tg.className = 'toggle';

    div.appendChild(meta);
    div.appendChild(tg);
    list.appendChild(div);
  });
  updateCount(listId, countId);
  // Re-aplica cores de track em ambas as listas — garante que mudou
  // de contagem (mic ou spk) atualize tracks ja desenhados na outra.
  refreshTrackColors();
}

function onToggleSource(el) {
  el.classList.toggle('selected');
  const enabled = el.classList.contains('selected');
  Bridge.send('toggle_source', {
    kind: el.dataset.kind,
    id: el.dataset.id,
    enabled
  });
  const section = el.closest('.section');
  const countEl = section.querySelector('.count');
  if (countEl) {
    const total = section.querySelectorAll('.source-item').length;
    const sel = section.querySelectorAll('.source-item.selected').length;
    countEl.textContent = `${sel} / ${total}`;
  }
  // Toggle de mic/speaker reorganiza tracks (agrupamento por numero
  // de enabled). O recalculo acontece no Delphi: HandleToggleSource
  // empurra audio_sources_refreshed silencioso → UI re-renderiza com
  // os novos data-track. Aqui so atualiza visual local imediato.
  updateRecordButtonAvailability();
}

function updateCount(listId, countId) {
  const list = document.getElementById(listId);
  const total = list.querySelectorAll('.source-item').length;
  const sel = list.querySelectorAll('.source-item.selected').length;
  document.getElementById(countId).textContent = `${sel} / ${total}`;
}


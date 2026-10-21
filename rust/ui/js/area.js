// =====================================================================
// Area da tela: previa de uma area sobre os monitores + contagem regressiva.
//
// A previa (monitores com as imagens da tela inicial + o retangulo
// destacado) e usada pelos atalhos de area da aba Atalhos (hotkeys.js). A
// contagem (`record_countdown`, bridge/countdown.rs) vale pra qualquer
// atalho com atraso: a linha de status da tela principal diz quanto falta.
// =====================================================================
const AreaCapture = {
  countdown: 0,

  // Monitores e area estao no MESMO sistema de coordenadas (pixels fisicos
  // da area de trabalho virtual), entao a previa e so uma regra de tres: a
  // caixa recebe o aspect-ratio do conjunto e cada peca vai em %, sem
  // recalcular quando a janela muda de tamanho. As imagens sao as da tela
  // inicial (`monitor_thumbs`), inclusive de monitores ocultos.
  _thumbs: {},

  _monitors() {
    const all = (typeof Devices !== 'undefined' && Devices.all && Devices.all.monitors) || [];
    return all.filter(m => m.width > 0 && m.height > 0);
  },

  // Desenha a previa da area `a` ({x,y,w,h}) dentro de `box`. Sem area ou
  // sem monitores conhecidos, a caixa some.
  renderPreview(box, a) {
    if (!box) return;
    const mons = this._monitors();
    if (!a || !mons.length) {
      box.hidden = true;
      box.innerHTML = '';
      return;
    }
    // Limites: todos os monitores + a propria area (pode ter ficado
    // parcialmente fora se um monitor saiu).
    let x0 = a.x, y0 = a.y, x1 = a.x + a.w, y1 = a.y + a.h;
    for (const m of mons) {
      x0 = Math.min(x0, m.x); y0 = Math.min(y0, m.y);
      x1 = Math.max(x1, m.x + m.width); y1 = Math.max(y1, m.y + m.height);
    }
    const bw = x1 - x0, bh = y1 - y0;
    const place = (el, x, y, w, h) => {
      el.style.left = ((x - x0) / bw * 100) + '%';
      el.style.top = ((y - y0) / bh * 100) + '%';
      el.style.width = (w / bw * 100) + '%';
      el.style.height = (h / bh * 100) + '%';
    };
    // Monitor onde fica (a maior parte d)a area: ganha o destaque no nome.
    const inter = m => Math.max(0, Math.min(a.x + a.w, m.x + m.width) - Math.max(a.x, m.x)) *
                       Math.max(0, Math.min(a.y + a.h, m.y + m.height) - Math.max(a.y, m.y));
    let host = null, best = 0;
    for (const m of mons) { const v = inter(m); if (v > best) { best = v; host = m; } }

    box.hidden = false;
    box.style.aspectRatio = bw + ' / ' + bh;
    box.innerHTML = '';
    for (const m of mons) {
      const el = document.createElement('div');
      el.className = 'area-preview-mon' + (m === host ? ' host' : '');
      el.dataset.id = m.id;
      place(el, m.x, m.y, m.width, m.height);
      const url = this._thumbs[m.id] || m.thumb || '';
      if (url) {
        const img = document.createElement('img');
        img.alt = '';
        img.draggable = false;
        img.src = url;
        el.appendChild(img);
      }
      if (mons.length > 1) {
        const name = document.createElement('span');
        name.className = 'area-preview-name';
        name.textContent = m.name || '';
        el.appendChild(name);
      }
      box.appendChild(el);
    }
    const r = document.createElement('div');
    r.className = 'area-preview-rect';
    place(r, a.x, a.y, a.w, a.h);
    const size = document.createElement('span');
    size.className = 'area-preview-size';
    size.textContent = a.w + ' × ' + a.h;
    r.appendChild(size);
    box.appendChild(r);
  },

  // Monitores mudaram (lista de dispositivos): refaz as previas abertas.
  renderPreviews() {
    if (typeof Hotkeys !== 'undefined') Hotkeys.renderPreviews();
  },

  // Tique das previas (a cada ~500 ms): troca so as imagens, sem refazer.
  updateThumbs(items) {
    for (const it of items) this._thumbs[it.id] = it.thumb || '';
    document.querySelectorAll('.area-preview:not([hidden])').forEach(box => {
      box.querySelectorAll('.area-preview-mon').forEach(el => {
        const url = this._thumbs[el.dataset.id];
        if (url === undefined) return;
        let img = el.querySelector('img');
        if (!url) { if (img) img.remove(); return; }
        if (!img) {
          img = document.createElement('img');
          img.alt = '';
          img.draggable = false;
          el.insertBefore(img, el.firstChild);
        }
        if (img.getAttribute('src') !== url) img.src = url;
      });
    });
  },

  // Contagem do atraso: o botao de gravar mostra os segundos no lugar do
  // icone e um anel em volta que esvazia ate zero (animacao CSS de `total`
  // segundos, comecando no ponto certo se a contagem ja estava andando); a
  // linha de status diz quanto falta e que clicar em gravar cancela.
  countdownTotal: 0,
  onCountdown(sec, total) {
    const prev = this.countdown;
    this.countdown = sec || 0;
    if (total) this.countdownTotal = total;
    const btn = document.getElementById('recordBtn');
    const num = document.getElementById('recordCountdownNum');
    const label = document.getElementById('recordLabel');
    if (btn && num) {
      if (this.countdown > 0) {
        const tot = Math.max(this.countdownTotal, this.countdown);
        if (!btn.classList.contains('counting') || this.countdown > prev) {
          // Contagem nova: reinicia a animacao do anel (tirar a classe e
          // forcar o reflow e o que faz o navegador recomecar do zero).
          btn.classList.remove('counting');
          void btn.offsetWidth;
          btn.style.setProperty('--cd-total', tot + 's');
          btn.style.setProperty('--cd-delay', -(tot - this.countdown) + 's');
          btn.classList.add('counting');
        }
        num.textContent = String(this.countdown);
        num.classList.remove('tick');
        void num.offsetWidth;
        num.classList.add('tick');
        if (label) label.textContent = T('record.countdownCancel');
      } else if (btn.classList.contains('counting')) {
        btn.classList.remove('counting');
        num.textContent = '';
        if (label && !_lastRecordingActive) label.textContent = T('record.start');
      }
    }
    const st = document.getElementById('recStatusText');
    if (!st) return;
    if (this.countdown > 0)
      st.textContent = T('record.countdown', { sec: this.countdown });
    else if (!_lastRecordingActive)
      st.textContent = T('record.statusReady');
  },

  // Troca de idioma: textos montados aqui.
  relabel() {
    if (this.countdown > 0) this.onCountdown(this.countdown);
  },
};

// =====================================================================
// Gravacao (start/stop)
// =====================================================================
// Onda radial vermelha que sai do centro do botao de gravar e cobre
// a tela inteira. Disparada apenas ao INICIAR gravacao (nao no stop)
// — feedback visual imediato pra o click. Independe do backend
// confirmar, entao se a gravacao falhar o efeito ja rolou (toast de
// erro aparece logo depois explicando).
function triggerRecordRipple() {
  const btn = document.getElementById('recordBtn');
  if (!btn) {
    Bridge.send('ui_log', { message: 'ripple: recordBtn nao encontrado' });
    return;
  }
  const r = btn.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  const cy = r.top  + r.height / 2;

  const dx = Math.max(cx, window.innerWidth  - cx);
  const dy = Math.max(cy, window.innerHeight - cy);
  const maxRadius = Math.hypot(dx, dy);
  // Anel posicionado em ~94% do raio (94px no nao-escalado).
  // scale = maxRadius / 94 pra anel chegar ate o canto + 1 de margem.
  const scale = (maxRadius / 94) + 1;

  Bridge.send('ui_log', { message:
    `ripple: cx=${cx.toFixed(0)} cy=${cy.toFixed(0)} maxR=${maxRadius.toFixed(0)} scale=${scale.toFixed(2)}` });

  const ripple = document.createElement('div');
  ripple.className = 'record-ripple';
  ripple.style.left = cx + 'px';
  ripple.style.top  = cy + 'px';
  document.body.appendChild(ripple);

  // Web Animations API — mais explicito que CSS transition/animation.
  // Sem depender de reflow timing nem de var() em scale() (que tinha
  // problemas em algumas versoes do WebView2). Os keyframes recebem
  // o scale calculado direto como valor literal.
  if (typeof ripple.animate === 'function') {
    const anim = ripple.animate(
      [
        { transform: 'translate(-50%, -50%) scale(0)',   opacity: 1 },
        { transform: 'translate(-50%, -50%) scale(' + scale.toFixed(2) + ')', opacity: 0 }
      ],
      {
        duration: 1800,
        easing:   'cubic-bezier(0.16, 1, 0.3, 1)',
        fill:     'forwards'
      }
    );
    anim.onfinish = () => { if (ripple.parentNode) ripple.remove(); };
  } else {
    // Fallback ultra-defensivo — WebView2 modernos tem WAAPI. Aqui so
    // pra nao deixar o elemento orfao se Algo Estranho™ acontecer.
    Bridge.send('ui_log', { message: 'ripple: WAAPI indisponivel, removendo' });
    setTimeout(() => ripple.remove(), 100);
  }
}

function toggleRecord() {
  const isRecording = document.body.classList.contains('recording');
  // Ripple so no start. No stop nao faz sentido visualmente — a UI
  // ja sinaliza fim removendo o pulse vermelho.
  if (!isRecording) triggerRecordRipple();
  // Som de parada preemptivo — toca imediato ao clicar, em vez de
  // esperar o backend terminar Engine.StopRecording (que pode levar
  // centenas de ms flushing buffers). O backend tambem manda um
  // recording_stopping, mas o debounce em RecordingSounds.playStop
  // evita disparar duas vezes.
  if (isRecording && Settings && Settings.currentPlaySoundOnRecord)
    RecordingSounds.playStop();
  Bridge.send(isRecording ? 'record_stop' : 'record_start');
}

// =====================================================================
// Loading overlay
// =====================================================================
function showLoading(message) {
  const ov = document.getElementById('loadingOverlay');
  const tx = document.getElementById('loadingText');
  if (tx) {
    tx.innerHTML = '<span></span><span class="loading-dots"></span>';
    tx.firstChild.textContent = message || T('common.loading');
  }
  if (ov) ov.classList.remove('hidden');
}
function hideLoading() {
  const ov = document.getElementById('loadingOverlay');
  if (ov) ov.classList.add('hidden');
}


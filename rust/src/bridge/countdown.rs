//! Contagem regressiva antes de gravar — o `delaySec` dos atalhos.
//!
//! Vale pros três alvos (telas, janela, área). Na área a moldura já aparece
//! no lugar, âmbar, com os segundos no botão; nos outros a linha de status da
//! tela principal diz quanto falta (`record_countdown`). Durante a contagem,
//! qualquer atalho de gravação e o botão de gravar CANCELAM em vez de começar
//! outra coisa.

use std::cell::RefCell;

use crate::engine::{AreaRect, WindowCapture};
use crate::{area_frame, log, ui};

use super::{area, recording, window_record};

pub const TIMER_COUNTDOWN: usize = 7019;

/// O que gravar quando a contagem chegar a zero.
pub enum Target {
    Screens,
    Window(WindowCapture),
    Area(AreaRect),
}

struct State {
    target: Target,
    left: u32,
    total: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Começa agora (`delay` 0) ou arma a contagem.
pub fn start(target: Target, delay: u32) {
    if recording::recording_active() || active() {
        return;
    }
    if delay == 0 {
        start_now(target, false);
        return;
    }
    if let Target::Area(a) = &target {
        // Moldura já no lugar: o usuário vê o que vai entrar enquanto espera.
        area_frame::show(*a, area::on_area_changed);
        area_frame::set_countdown(Some(delay));
    }
    let what = match &target {
        Target::Screens => "telas".to_owned(),
        Target::Window(w) => format!("janela \"{}\"", w.title),
        Target::Area(a) => format!("area {}x{} em ({}, {})", a.w, a.h, a.x, a.y),
    };
    STATE.with(|s| *s.borrow_mut() = Some(State { target, left: delay, total: delay }));
    ui::set_timer(TIMER_COUNTDOWN, 1000);
    push(delay, delay);
    log!("Contagem: gravacao em {delay}s ({what}).");
}

fn start_now(target: Target, frame_shown: bool) {
    match target {
        Target::Screens => recording::handle_record_start(),
        Target::Window(w) => window_record::start_window(w),
        Target::Area(a) => area::start_area(a, frame_shown),
    }
}

pub fn active() -> bool {
    STATE.with(|s| s.borrow().is_some())
}

/// `total` deixa a tela desenhar o anel do botão de gravar regredindo
/// (e acertar o ponto dele se a página abriu no meio da contagem).
fn push(sec: u32, total: u32) {
    crate::view::call(move |v| v.record_countdown(sec, total));
}

/// Cancela a contagem em curso. `true` = havia uma.
pub fn cancel() -> bool {
    let Some(st) = STATE.with(|s| s.borrow_mut().take()) else { return false };
    ui::kill_timer(TIMER_COUNTDOWN);
    if matches!(st.target, Target::Area(_)) {
        area_frame::hide();
    }
    push(0, st.total);
    log::log("Contagem: cancelada.");
    true
}

pub fn on_timer(id: usize) -> bool {
    if id != TIMER_COUNTDOWN {
        return false;
    }
    let left = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let st = s.as_mut()?;
        st.left = st.left.saturating_sub(1);
        Some((st.left, st.total))
    });
    match left {
        None => ui::kill_timer(TIMER_COUNTDOWN),
        Some((0, total)) => {
            ui::kill_timer(TIMER_COUNTDOWN);
            push(0, total);
            if let Some(st) = STATE.with(|s| s.borrow_mut().take()) {
                start_now(st.target, true);
            }
        }
        Some((n, total)) => {
            if STATE.with(|s| matches!(s.borrow().as_ref().map(|s| &s.target), Some(Target::Area(_)))) {
                area_frame::set_countdown(Some(n));
            }
            push(n, total);
        }
    }
    true
}

pub fn shutdown() {
    STATE.with(|s| s.borrow_mut().take());
    ui::kill_timer(TIMER_COUNTDOWN);
}

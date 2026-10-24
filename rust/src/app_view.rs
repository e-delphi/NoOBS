//! A janela Slint: implementa o [`View`] (core → interface) e liga os
//! callbacks da `.slint` direto nas funções do core (interface → core), sem
//! JSON (docs/ui-slint.md).
//!
//! Só a main thread toca aqui. As workers chegam pelo `view::call`, que
//! enfileira na main.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::view::{AudioMeter, Device, MonitorThumb, PendingKind, RecState, ReplayState, View, WindowItem, WindowMatches};
use crate::{app_main, lang, log};

slint::include_modules!();

/// Tempo de vida padrão de um aviso (o `ttl` do `Toast.show` da web).
const TOAST_TTL_MS: u32 = 6000;
/// O `setTimeout(remove, 220)` da web: deixa a animação de saída acabar.
const TOAST_REMOVE_MS: u64 = 220;

/// Avisos na tela. O desenho é da `.slint`; aqui ficam o tempo de vida e a
/// regra de não repetir: um aviso com o MESMO título já visível só renova o
/// prazo (o hot-plug de USB dispara o refresh de áudio e o de webcam juntos).
struct Toasts {
    model: Rc<VecModel<ToastItem>>,
    next_id: i32,
    timers: Vec<(i32, slint::Timer)>,
}

impl Toasts {
    fn new() -> Self {
        Self { model: Rc::new(VecModel::default()), next_id: 1, timers: Vec::new() }
    }
}

thread_local! {
    static TOASTS: RefCell<Toasts> = RefCell::new(Toasts::new());
    static WINDOW: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
}

fn find_toast(model: &VecModel<ToastItem>, id: i32) -> Option<usize> {
    (0..model.row_count()).find(|&i| model.row_data(i).is_some_and(|t| t.id == id))
}

/// Arma (ou rearma) o fim do aviso `id`.
fn arm_toast_timer(id: i32, ttl_ms: u32) {
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::SingleShot, Duration::from_millis(ttl_ms as u64), move || dismiss_toast(id));
    TOASTS.with(|t| {
        let mut t = t.borrow_mut();
        t.timers.retain(|(i, _)| *i != id);
        t.timers.push((id, timer));
    });
}

pub fn show_toast(title: &str, msg: &str, warn: bool, ttl_ms: u32) {
    let ttl = if ttl_ms == 0 { TOAST_TTL_MS } else { ttl_ms };
    let existing = TOASTS.with(|t| {
        let t = t.borrow();
        (0..t.model.row_count()).filter_map(|i| t.model.row_data(i)).find(|it| it.shown && it.title == title).map(|it| it.id)
    });
    if let Some(id) = existing {
        arm_toast_timer(id, ttl);
        return;
    }
    let (id, model) = TOASTS.with(|t| {
        let mut t = t.borrow_mut();
        let id = t.next_id;
        t.next_id += 1;
        (id, t.model.clone())
    });
    model.push(ToastItem { id, title: title.into(), msg: msg.into(), warn, shown: false });
    // Nasce escondido e aparece no tique seguinte: é o que dispara a animação
    // de entrada (o `requestAnimationFrame(() => add('show'))` da web).
    slint::Timer::single_shot(Duration::from_millis(16), move || {
        TOASTS.with(|t| {
            let t = t.borrow();
            if let Some(i) = find_toast(&t.model, id) {
                if let Some(mut it) = t.model.row_data(i) {
                    it.shown = true;
                    t.model.set_row_data(i, it);
                }
            }
        });
    });
    arm_toast_timer(id, ttl);
}

/// Some com o aviso (clique ou fim do prazo). Idempotente.
fn dismiss_toast(id: i32) {
    TOASTS.with(|t| {
        let mut t = t.borrow_mut();
        t.timers.retain(|(i, _)| *i != id);
        if let Some(i) = find_toast(&t.model, id) {
            if let Some(mut it) = t.model.row_data(i) {
                if !it.shown {
                    return;
                }
                it.shown = false;
                t.model.set_row_data(i, it);
            }
        }
    });
    slint::Timer::single_shot(Duration::from_millis(TOAST_REMOVE_MS), move || {
        TOASTS.with(|t| {
            let t = t.borrow();
            if let Some(i) = find_toast(&t.model, id) {
                t.model.remove(i);
            }
        });
    });
}

/// A janela, se existir.
pub fn window() -> Option<AppWindow> {
    WINDOW.with(|w| w.borrow().as_ref().and_then(|w| w.upgrade()))
}

pub struct SlintView;

impl View for SlintView {
    fn theme(&self, dark: bool, _pref: &str) {
        if let Some(w) = window() {
            w.global::<Theme>().set_dark(dark);
        }
    }
    fn language_changed(&self, _language: &str, _pref: &str) {
        if let Some(w) = window() {
            let t = w.global::<T>();
            t.set_rev(t.get_rev() + 1);
            crate::app_main::on_language_changed(&w);
        }
    }
    fn toast(&self, title: &str, msg: &str, warn: bool, ttl_ms: u32) {
        show_toast(title, msg, warn, ttl_ms);
    }
    fn error(&self, message: &str) {
        let msg = if message.is_empty() { lang::t("toast.unknownError") } else { message.to_owned() };
        show_toast(&lang::t("toast.errorTitle"), &msg, true, 8000);
    }
    fn open_settings(&self) {
        // As Configurações entram na fase 4 (docs/ui-slint.md).
        log::log("View: open_settings (tela ainda nao portada).");
    }
    fn window_hidden(&self) {
        // Fecha o player quando ele existir (fase 3).
    }

    fn monitors(&self, list: &[Device], changes: &[String]) {
        if let Some(w) = window() {
            app_main::on_monitors(&w, list, changes);
        }
    }
    fn webcams(&self, list: &[Device], changes: &[String]) {
        if let Some(w) = window() {
            app_main::on_webcams(&w, list, changes);
        }
    }
    fn audio_sources(&self, mics: &[Device], speakers: &[Device], silent: bool, changes: &[String]) {
        if let Some(w) = window() {
            app_main::on_audio_sources(&w, mics, speakers, silent, changes);
        }
    }
    fn audio_meters(&self, items: &[AudioMeter]) {
        app_main::on_audio_meters(items);
    }
    fn monitor_thumbs(&self, items: &[MonitorThumb]) {
        if let Some(w) = window() {
            app_main::on_monitor_thumbs(&w, items);
        }
    }
    fn device_change_pending(&self, kind: PendingKind, pending: bool) {
        if let Some(w) = window() {
            app_main::on_device_change_pending(&w, kind, pending);
        }
    }
    fn refresh_busy(&self, busy: bool, what: &str) {
        if let Some(w) = window() {
            app_main::on_refresh_busy(&w, busy, what);
        }
    }
    fn recording_state(&self, st: &RecState) {
        if let Some(w) = window() {
            app_main::on_recording_state(&w, st);
        }
    }
    fn recording_finalizing(&self, active: bool) {
        if let Some(w) = window() {
            app_main::on_recording_finalizing(&w, active);
        }
    }
    fn recording_stopping(&self) {
        // Atalho/bandeja: o clique na tela já tocou (o debounce evita o 2º).
        app_main::play_stop_sound();
    }
    fn replay_state(&self, st: &ReplayState) {
        if let Some(w) = window() {
            app_main::on_replay_state(&w, st);
        }
    }
    fn replay_saved(&self, _id: &str, name: &str, duration_sec: i32) {
        app_main::on_replay_saved(name, duration_sec);
    }
    fn replay_restarted(&self) {
        app_main::on_replay_restarted();
    }
    fn capture_windows(&self, seq: u32, items: &[WindowItem], app_audio: bool) {
        app_main::on_capture_windows(seq, items, app_audio);
    }
    fn capture_window_thumb(&self, seq: u32, id: &str, width: u32, height: u32, rgb: &[u8]) {
        app_main::on_capture_window_thumb(seq, id, width, height, rgb);
    }
    fn window_picker_matches(&self, m: &WindowMatches) {
        app_main::on_window_picker_matches(m);
    }
    fn record_countdown(&self, sec: u32, total: u32) {
        if let Some(w) = window() {
            app_main::on_record_countdown(&w, sec, total);
        }
    }
}

/// Cria a janela (escondida), registra o [`View`] e liga os callbacks.
pub fn create() -> Option<AppWindow> {
    let w = match AppWindow::new() {
        Ok(w) => w,
        Err(e) => {
            log!("UI: falha ao criar a janela Slint: {e}");
            return None;
        }
    };
    WINDOW.with(|cell| *cell.borrow_mut() = Some(w.as_weak()));

    // Textos: `T.s(key)` lê do `lang` em vigor; o `rev` é só a dependência.
    w.global::<T>().on_lookup(|_rev, key| lang::t(key.as_str()).into());
    w.global::<Theme>().set_dark(crate::bridge::resolved_theme() == "dark");

    let model = TOASTS.with(|t| t.borrow().model.clone());
    w.set_toasts(ModelRc::from(model));
    w.on_toast_clicked(dismiss_toast);

    w.global::<Confirm>().on_accepted(|action, arg| {
        log!("Confirm: \"{action}\" ({arg}) — sem tratador ainda.");
    });
    app_main::setup(&w);

    crate::view::set(Rc::new(SlintView));
    Some(w)
}

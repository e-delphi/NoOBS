//! Captura de uma ÁREA da tela.
//!
//! * "Capturar... → Área": escurece os monitores, o usuário arrasta e, ao
//!   soltar, a gravação começa (`area_select` + `area_frame`).
//! * **Área salva** (Configurações → Área): uma área escolhida uma vez e
//!   guardada em `areaSaved`, gravada pelo atalho `areaHotkey` (padrão
//!   Ctrl+Shift+Pause/Break). O atalho só é registrado COM uma área salva —
//!   sem ela, a combinação continua sendo dos outros programas.
//! * **Atraso** (`areaDelaySec`, 0 = desligado): entre escolher a área e a
//!   gravação começar, a moldura aparece âmbar com os segundos que faltam no
//!   botão. Vale pros dois caminhos. Na contagem, o atalho da área, o de
//!   gravar e o botão de gravar CANCELAM em vez de começar outra coisa.
//!
//! Na hibernação o atalho também vale: ela lança o modo completo com
//! `/start-area`, que grava a área salva depois do warmup (como o
//! `/start-record`).

use std::cell::RefCell;

use serde_json::{json, Value};

use crate::engine::{self, AreaRect};
use crate::{area_frame, area_select, config, hotkey, lang, log, ui};

use super::{post_error, post_json, recording};

pub const TIMER_AREA_COUNTDOWN: usize = 7019;
pub const HK_AREA_RECORD: i32 = 104;
pub const HK_AREA_RECORD_ALT: i32 = 105;
pub const DEFAULT_AREA_HOTKEY: &str = "Ctrl+Shift+Pause/Break";
/// Teto do atraso, em segundos.
const AREA_DELAY_MAX: i32 = 60;

struct Countdown {
    area: AreaRect,
    left: u32,
}

thread_local! {
    static COUNTDOWN: RefCell<Option<Countdown>> = const { RefCell::new(None) };
}

// ---------------------------------------------------------------------------
// Configuração
// ---------------------------------------------------------------------------

/// Área salva, como guardada (pode estar fora da tela se os monitores mudaram).
pub fn saved_area() -> Option<AreaRect> {
    let v = config::get_config_json("areaSaved")?;
    let get = |k: &str| v.get(k).and_then(Value::as_i64).map(|n| n as i32);
    let a = AreaRect { x: get("x")?, y: get("y")?, w: get("w")?, h: get("h")? };
    (a.w >= area_select::MIN_SIZE && a.h >= area_select::MIN_SIZE).then_some(a)
}

fn delay_sec() -> u32 {
    config::get_config_int("areaDelaySec", 0).clamp(0, AREA_DELAY_MAX) as u32
}

pub(super) fn hotkey_spec() -> String {
    config::get_config_str("areaHotkey", DEFAULT_AREA_HOTKEY)
}

/// A área salva recortada pela área de trabalho virtual de AGORA: um monitor
/// desligado não pode virar um pedaço preto (ou o vídeo inteiro preto).
fn clamp_to_screen(a: AreaRect) -> Option<AreaRect> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let l = a.x.max(vx);
    let t = a.y.max(vy);
    let r = (a.x + a.w).min(vx + vw);
    let b = (a.y + a.h).min(vy + vh);
    let (w, h) = (r - l, b - t);
    (w >= area_select::MIN_SIZE && h >= area_select::MIN_SIZE).then_some(AreaRect { x: l, y: t, w, h })
}

pub fn push_area_state() {
    let saved = saved_area().map(|a| json!({ "x": a.x, "y": a.y, "w": a.w, "h": a.h }));
    post_json(json!({
        "type": "area_state",
        "saved": saved,
        "hotkey": hotkey_spec(),
        "delay": delay_sec(),
        "delayMax": AREA_DELAY_MAX,
    }));
}

// ---------------------------------------------------------------------------
// Atalho
// ---------------------------------------------------------------------------

fn unregister_hotkey() {
    ui::unregister_global_hotkey(HK_AREA_RECORD);
    ui::unregister_global_hotkey(HK_AREA_RECORD_ALT);
}

/// Só com uma área SALVA: sem ela o atalho não teria o que gravar.
fn apply_hotkey() {
    unregister_hotkey();
    if saved_area().is_none() {
        log::log("Area: sem area salva — atalho da area nao registrado.");
        return;
    }
    let spec = hotkey_spec();
    if spec.trim().is_empty() {
        log::log("Area: atalho desativado (config vazia).");
        return;
    }
    let Some(hk) = hotkey::parse_hotkey(&spec) else {
        log!("Area: atalho invalido \"{spec}\" — ignorado.");
        return;
    };
    if let Some(reason) = hotkey::reserved_reason(hk.modifiers, hk.vk) {
        log!("Area: atalho \"{spec}\" e reservado pelo Windows ({reason}) — ignorado.");
        return;
    }
    if ui::register_global_hotkey(HK_AREA_RECORD, hk.modifiers, hk.vk) {
        log!("Area: atalho registrado \"{spec}\".");
    } else {
        log!("Area: RegisterHotKey falhou pra \"{spec}\" (outro app, ou outro atalho do NoOBS, ja usa a combinacao).");
    }
    // Ctrl+Pause chega como VK_CANCEL (Ctrl+Break).
    if hk.vk == hotkey::VK_PAUSE && hk.modifiers & hotkey::MOD_CTRL != 0 {
        ui::register_global_hotkey(HK_AREA_RECORD_ALT, hk.modifiers, hotkey::VK_CANCEL);
    }
}

/// `true` = o id era do atalho da área.
pub fn on_hotkey(id: i32) -> bool {
    if !matches!(id, HK_AREA_RECORD | HK_AREA_RECORD_ALT) {
        return false;
    }
    // Mesmo atalho de novo: na contagem cancela, gravando para (como o de gravar).
    if cancel_countdown() {
        return true;
    }
    if recording::recording_active() {
        recording::handle_record_stop();
        return true;
    }
    record_saved_area();
    true
}

// ---------------------------------------------------------------------------
// Gravar (já ou depois da contagem)
// ---------------------------------------------------------------------------

/// `select_capture_area`: escurece os monitores pro usuário arrastar a área.
/// Soltar começa a gravação; Esc/botão direito cancelam (`capture_area_canceled`).
pub fn handle_select_capture_area() {
    if recording::recording_active() || area_select::is_active() || countdown_active() {
        return;
    }
    if !area_select::start(on_area_selected) {
        post_json(json!({ "type": "capture_area_canceled" }));
    }
}

fn on_area_selected(area: Option<AreaRect>) {
    let Some(a) = area else {
        post_json(json!({ "type": "capture_area_canceled" }));
        return;
    };
    if recording::recording_active() {
        return;
    }
    begin(a);
}

/// Grava a área salva (atalho, `/start-area` ou o botão do seletor).
pub fn record_saved_area() {
    if recording::recording_active() || area_select::is_active() || countdown_active() {
        return;
    }
    let Some(saved) = saved_area() else {
        log::log("Area: pedido de gravar a area salva sem area salva.");
        post_error(lang::t("error.areaNotSaved"));
        return;
    };
    let Some(a) = clamp_to_screen(saved) else {
        log!("Area: area salva {}x{} em ({}, {}) fora da tela atual.", saved.w, saved.h, saved.x, saved.y);
        post_error(lang::t("error.areaOffScreen"));
        return;
    };
    log!("Area: gravando a area salva {}x{} em ({}, {}).", a.w, a.h, a.x, a.y);
    begin(a);
}

/// Começa agora, ou arma a contagem regressiva do atraso.
fn begin(a: AreaRect) {
    let d = delay_sec();
    if d == 0 {
        start_now(a, false);
        return;
    }
    COUNTDOWN.with(|c| *c.borrow_mut() = Some(Countdown { area: a, left: d }));
    // Moldura já no lugar: o usuário vê o que vai entrar enquanto espera.
    area_frame::show(a, on_area_changed);
    area_frame::set_countdown(Some(d));
    ui::set_timer(TIMER_AREA_COUNTDOWN, 1000);
    push_countdown(d);
    log!("Area: gravacao em {d}s ({}x{} em ({}, {})).", a.w, a.h, a.x, a.y);
}

fn start_now(a: AreaRect, frame_shown: bool) {
    if recording::recording_active() {
        if frame_shown {
            area_frame::hide();
        }
        return;
    }
    recording::set_recording_area(Some(a));
    recording::handle_record_start();
    if recording::recording_active() {
        if frame_shown {
            area_frame::set_countdown(None);
        } else {
            // Moldura com o botão de mover/redimensionar (Ctrl+Shift+clique).
            area_frame::show(a, on_area_changed);
        }
    } else {
        // Não subiu (recusada ou falhou): a próxima gravação volta às telas.
        recording::set_recording_area(None);
        if frame_shown {
            area_frame::hide();
        }
    }
}

fn countdown_active() -> bool {
    COUNTDOWN.with(|c| c.borrow().is_some())
}

fn push_countdown(sec: u32) {
    post_json(json!({ "type": "area_countdown", "sec": sec }));
}

/// Cancela a contagem em curso. `true` = havia uma.
pub fn cancel_countdown() -> bool {
    if COUNTDOWN.with(|c| c.borrow_mut().take()).is_none() {
        return false;
    }
    ui::kill_timer(TIMER_AREA_COUNTDOWN);
    area_frame::hide();
    push_countdown(0);
    log::log("Area: contagem cancelada.");
    true
}

pub fn on_timer(id: usize) -> bool {
    if id != TIMER_AREA_COUNTDOWN {
        return false;
    }
    let next = COUNTDOWN.with(|c| {
        let mut c = c.borrow_mut();
        let cd = c.as_mut()?;
        cd.left = cd.left.saturating_sub(1);
        if cd.left == 0 {
            c.take().map(|cd| (cd.area, 0))
        } else {
            Some((cd.area, cd.left))
        }
    });
    match next {
        None => ui::kill_timer(TIMER_AREA_COUNTDOWN),
        Some((a, 0)) => {
            ui::kill_timer(TIMER_AREA_COUNTDOWN);
            push_countdown(0);
            start_now(a, true);
        }
        Some((_, left)) => {
            area_frame::set_countdown(Some(left));
            push_countdown(left);
        }
    }
    true
}

/// A moldura mudou a área no meio da gravação: o engine troca recorte e
/// posição das capturas na hora (o tamanho do vídeo não muda). No fim do
/// arrasto, o rótulo da tela ("Área 1280×720") acompanha.
fn on_area_changed(area: AreaRect, done: bool) {
    let applied = engine::with_engine(|e| e.update_area(area)).unwrap_or(false);
    if !applied {
        return;
    }
    recording::update_recording_area(area);
    if done {
        log!("Area: agora {}x{} em ({}, {}).", area.w, area.h, area.x, area.y);
        recording::push_recording_state();
    }
}

// ---------------------------------------------------------------------------
// Handlers das Configurações
// ---------------------------------------------------------------------------

/// `define_capture_area`: a mesma seleção, mas só GUARDA a área.
pub fn handle_define_area() {
    if area_select::is_active() || countdown_active() {
        return;
    }
    if !area_select::start_with_hint(on_area_defined, "areaSelect.hintSave") {
        push_area_state();
    }
}

fn on_area_defined(area: Option<AreaRect>) {
    if let Some(a) = area {
        config::set_config_json("areaSaved", Some(json!({ "x": a.x, "y": a.y, "w": a.w, "h": a.h })));
        log!("Area: area salva {}x{} em ({}, {}).", a.w, a.h, a.x, a.y);
        apply_hotkey();
    }
    // Cancelada também responde: a UI sai do "escolhendo".
    push_area_state();
}

pub fn handle_clear_area() {
    config::set_config_json("areaSaved", None);
    log::log("Area: area salva removida.");
    apply_hotkey();
    push_area_state();
}

/// Não pode repetir o atalho de gravar nem o do buffer: o Windows aceita um
/// registro por combinação, e o segundo simplesmente não dispararia.
pub fn handle_set_area_hotkey(spec: &str) {
    let mut normalized = spec.trim().to_owned();
    if !normalized.is_empty() {
        let Some(hk) = hotkey::parse_hotkey(&normalized) else {
            log!("HandleSetAreaHotkey: spec invalido \"{normalized}\" — ignorado.");
            return;
        };
        if let Some(reason) = hotkey::reserved_reason(hk.modifiers, hk.vk) {
            log!("HandleSetAreaHotkey: \"{normalized}\" reservado ({reason}) — ignorado.");
            return;
        }
        normalized = hotkey::format_hotkey(hk.modifiers, hk.vk);
        let taken = [config::get_config_str("hotkey", "Pause/Break"), super::replay::hotkey_spec()];
        if taken.iter().any(|t| normalized.eq_ignore_ascii_case(t)) {
            log!("HandleSetAreaHotkey: \"{normalized}\" ja e outro atalho do NoOBS — ignorado.");
            post_error(lang::t("error.areaHotkeyConflict"));
            push_area_state();
            return;
        }
    }
    config::set_config_str("areaHotkey", &normalized);
    apply_hotkey();
    push_area_state();
}

pub fn handle_set_area_delay(sec: i32) {
    let sec = sec.clamp(0, AREA_DELAY_MAX);
    config::set_config_int("areaDelaySec", sec);
    log!("Area: atraso = {sec}s.");
    push_area_state();
}

// ---------------------------------------------------------------------------
// Ciclo de vida
// ---------------------------------------------------------------------------

pub fn do_init() {
    apply_hotkey();
    push_area_state();
}

pub fn shutdown() {
    COUNTDOWN.with(|c| c.borrow_mut().take());
    ui::kill_timer(TIMER_AREA_COUNTDOWN);
    unregister_hotkey();
}

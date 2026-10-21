//! Captura de uma ÁREA da tela.
//!
//! * "Capturar... → Área": escurece os monitores, o usuário arrasta e, ao
//!   soltar, a gravação começa (`area_select` + `area_frame`).
//! * Atalhos de área (aba Atalhos, `bridge::hotkeys`): gravam um retângulo
//!   salvo no próprio atalho, ou abrem a mesma seleção — com o atraso do
//!   atalho (`bridge::countdown`).
//!
//! A moldura da gravação deixa mover/redimensionar a área no meio
//! (Ctrl+Shift+clique no botão do canto).

use serde_json::json;

use crate::engine::{self, AreaRect};
use crate::{area_frame, area_select, log};

use super::{countdown, post_json, recording};

/// O retângulo recortado pela área de trabalho virtual de AGORA: um monitor
/// desligado não pode virar um pedaço preto (ou o vídeo inteiro preto).
pub fn clamp_to_screen(a: AreaRect) -> Option<AreaRect> {
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

/// `select_capture_area`: escurece os monitores pro usuário arrastar a área.
/// Soltar começa a gravação; Esc/botão direito cancelam (`capture_area_canceled`).
pub fn handle_select_capture_area() {
    if recording::recording_active() || area_select::is_active() || countdown::active() {
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
    countdown::start(countdown::Target::Area(a), 0);
}

/// Grava a área agora. `frame_shown` = a moldura já está na tela (veio da
/// contagem regressiva).
pub(super) fn start_area(a: AreaRect, frame_shown: bool) {
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

/// A moldura mudou a área no meio da gravação: o engine troca recorte e
/// posição das capturas na hora (o tamanho do vídeo não muda). No fim do
/// arrasto, o rótulo da tela ("Área 1280×720") acompanha.
pub(super) fn on_area_changed(area: AreaRect, done: bool) {
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

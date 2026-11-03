//! Integração do Bridge com o shell do Windows (partes do `OBSBridge.pas`):
//! bandeja e as preferências de janela (título, fechar pra bandeja,
//! autostart, elevação…).
//!
//! Tudo na main thread.

use crate::{autostart, config, engine, log, tray, ui};

use super::recording;

/// Liga a bandeja e a janela ao Bridge (parte do `initialization` do Delphi).
pub fn install() {
    tray::set_callbacks(tray::Callbacks {
        on_command: None,
        on_toggle_record: Some(toggle_record_from_tray),
        on_is_recording: Some(recording::recording_active),
    });
}

/// Item "Iniciar/Parar gravação" do menu da bandeja: mesmo efeito do atalho.
fn toggle_record_from_tray() {
    if recording::recording_active() {
        recording::handle_record_stop();
    } else {
        recording::handle_record_start();
    }
}

/// Buffer em memória ligado conta como gravando para fechar.
fn replay_active() -> bool {
    engine::with_engine(|e| e.is_replay_active()).unwrap_or(false)
}

/// `WM_CLOSE`: bandeja em vez de fechar? Com `closeToTray`, gravando (nunca
/// interromper por engano) ou com o buffer ligado (fechar jogaria fora o que
/// está guardado).
pub fn should_hide_on_close() -> bool {
    config::get_config_bool("closeToTray", true) || recording::recording_active() || replay_active()
}

/// Janela voltou da bandeja/taskbar: desarma o ócio, sai da hibernação e reempurra o
/// estado de gravação (o que mudou com ela escondida, pelo atalho ou pela bandeja).
pub fn on_window_restored() {
    ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
    super::leave_light();
    recording::push_recording_state();
}

// ---------------------------------------------------------------------------
// Hibernação por ócio
// ---------------------------------------------------------------------------

/// Quanto tempo na bandeja até hibernar (e de quanto em quanto tempo confere de novo).
const HIBERNATE_IDLE_DELAY_MS: u32 = 60_000;

/// Janela foi pra bandeja: arma o ócio (repete a cada minuto enquanto ela estiver
/// escondida). Com `hibernate` desligado, nada.
pub fn on_window_hidden() {
    if !config::get_config_bool("hibernate", true) {
        log::log("OnWindowHidden: hibernacao desligada no config.");
        return;
    }
    log!("OnWindowHidden: hiberna depois de {HIBERNATE_IDLE_DELAY_MS} ms de ocio.");
    ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
    ui::set_timer(super::TIMER_HIBERNATE_IDLE, HIBERNATE_IDLE_DELAY_MS);
}

/// `TIMER_HIBERNATE_IDLE` (repete): gravando, com o buffer ligado ou no teste de vídeo,
/// espera o próximo; senão hiberna. Já hibernando, só solta de novo o que uma gravação
/// escondida religou.
pub fn on_hibernate_idle() {
    if ui::is_window_shown() || !config::get_config_bool("hibernate", true) {
        ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
        return;
    }
    if recording::recording_active() || replay_active() || super::video_test::busy() {
        return;
    }
    if super::light_has_work() {
        super::enter_light();
    }
}

/// "Liberar memória após 1 minuto na bandeja" (aba Geral).
pub fn handle_set_hibernate(enable: bool) {
    config::set_config_bool("hibernate", enable);
    log!("Hibernate: {enable}");
    if !enable {
        ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
    } else if !ui::is_window_visible() {
        on_window_hidden();
    }
}

// ---------------------------------------------------------------------------
// Preferências de janela/bandeja
// ---------------------------------------------------------------------------

/// O registro guarda só o marcador `/autostart`; o comportamento vem do config.
pub fn handle_set_autostart(enable: bool) {
    autostart::set_autostart(enable);
}

/// Vale no próximo início: elevar ESTE processo exigiria reabrir no meio de
/// uma gravação ou do buffer.
pub fn handle_set_run_as_admin(enable: bool) {
    config::set_config_bool("runAsAdmin", enable);
    log!("Elevate: abrir como administrador = {enable} (no proximo inicio).");
}

/// Reflete na bandeja na hora. Desligando com a janela ESCONDIDA o ícone
/// fica: tirá-lo deixaria o usuário sem como voltar.
pub fn handle_set_close_to_tray(enable: bool) {
    config::set_config_bool("closeToTray", enable);
    log!("CloseToTray: {enable}");
    if enable {
        ui::ensure_tray_icon();
    } else if ui::is_window_visible() {
        // Minimizada conta: ela continua acessível pela taskbar.
        ui::remove_tray_icon();
    }
}

pub fn handle_set_config_bool(key: &str, enable: bool) {
    config::set_config_bool(key, enable);
    log!("{key}: {enable}");
}

/// Vazio volta pro padrão `NoOBS`; aplica ao vivo (barra + bandeja).
pub fn handle_set_window_title(title: &str) {
    let t = match title.trim() {
        "" => "NoOBS",
        t => t,
    };
    config::set_config_str("windowTitle", t);
    log!("WindowTitle: \"{t}\"");
    ui::apply_window_title();
}


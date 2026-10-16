//! Integração do Bridge com o shell do Windows (partes do `OBSBridge.pas`):
//! bandeja, hibernação por ócio, notificações, atalho global configurável e as
//! preferências de janela (título, fechar pra bandeja, autostart, elevação…).
//!
//! Tudo na main thread.

use std::cell::Cell;

use serde_json::json;

use crate::{autostart, config, engine, hotkey, lang, log, tray, ui};

use super::{export, post_json, recording, transcribe};

/// 1 min sem janela visível e sem trabalho: relança como `/hibernate`.
const HIBERNATE_IDLE_DELAY_MS: u32 = 60_000;

thread_local! {
    /// A UI mandou `ready`: antes disso o `show_notification` não chega e a
    /// notificação cai no balão da bandeja (gravação vinda da hibernação).
    static UI_READY: Cell<bool> = const { Cell::new(false) };
}

pub fn set_ui_ready() {
    UI_READY.with(|r| r.set(true));
}

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

/// Buffer em memória ligado conta como gravando para fechar e hibernar.
fn replay_active() -> bool {
    engine::with_engine(|e| e.is_replay_active()).unwrap_or(false)
}

/// `WM_CLOSE`: bandeja em vez de fechar? Com `closeToTray`, gravando (nunca
/// interromper por engano) ou com o buffer ligado (fechar jogaria fora o que
/// está guardado).
pub fn should_hide_on_close() -> bool {
    config::get_config_bool("closeToTray", true) || recording::recording_active() || replay_active()
}

/// Notificação de início/fim/pausa (`notifyOnRecord`, padrão desligado). Pela
/// Notification API da UI (o Chromium cuida da Central de Notificações); antes
/// de a UI subir, balão da bandeja.
pub fn maybe_notify_record(title: &str, message: &str) {
    if !config::get_config_bool("notifyOnRecord", false) {
        return;
    }
    if !UI_READY.with(|r| r.get()) {
        tray::show_balloon(title, message);
        return;
    }
    post_json(json!({ "type": "show_notification", "title": title, "body": message }));
}

// ---------------------------------------------------------------------------
// Hibernação por ócio
// ---------------------------------------------------------------------------

/// Janela foi pra bandeja/taskbar: arma o timer de ócio. Não arma gravando,
/// com o buffer ligado ou com `hibernate` desligado.
pub fn on_window_hidden() {
    if recording::recording_active() || replay_active() {
        return;
    }
    if !config::get_config_bool("hibernate", true) {
        log::log("OnWindowHidden: hibernate desativada no config — skip.");
        return;
    }
    log!("OnWindowHidden: armando timer de idle hibernate ({HIBERNATE_IDLE_DELAY_MS}ms).");
    ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
    ui::set_timer(super::TIMER_HIBERNATE_IDLE, HIBERNATE_IDLE_DELAY_MS);
}

/// Janela voltou: desarma o ócio e reempurra o estado de gravação — com a
/// janela escondida o WebView2 às vezes perde mensagens (pegadinha #38).
pub fn on_window_restored() {
    ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
    recording::push_recording_state();
}

/// `TIMER_HIBERNATE_IDLE`. One-shot; trabalho longo em worker deste processo
/// RE-ARMA em vez de desistir (hibernar é sair do processo — a worker morreria
/// junto), para hibernar assim que terminar.
pub fn on_hibernate_idle() {
    ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
    if recording::recording_active() {
        log::log("TIMER_HIBERNATE_IDLE: gravando, hibernacao adiada.");
        return;
    }
    // Sem re-armar: desligar o buffer com a janela escondida arma de novo.
    if replay_active() {
        log::log("TIMER_HIBERNATE_IDLE: buffer em memoria ligado, hibernacao adiada.");
        return;
    }
    if transcribe::busy_for_hibernation() || export::export_busy() {
        log!(
            "TIMER_HIBERNATE_IDLE: trabalho em andamento (transcricao/instalacao={}, exportacao={}), hibernacao adiada.",
            transcribe::busy_for_hibernation(),
            export::export_busy()
        );
        ui::set_timer(super::TIMER_HIBERNATE_IDLE, HIBERNATE_IDLE_DELAY_MS);
        return;
    }
    if ui::is_window_shown() {
        log::log("TIMER_HIBERNATE_IDLE: janela visivel, ignorando.");
        return;
    }
    log::log("TIMER_HIBERNATE_IDLE: respawning como /hibernate.");
    ui::spawn_hibernate_and_exit();
}

// ---------------------------------------------------------------------------
// Atalho global configurável (OBSHotkey)
// ---------------------------------------------------------------------------

/// Lê `hotkey` e (re)registra. Padrão `Pause/Break`; vazio = nenhum atalho.
pub fn apply_hotkey_from_config() {
    ui::unregister_global_hotkey(recording::HK_RECORD_TOGGLE);
    ui::unregister_global_hotkey(recording::HK_RECORD_TOGGLE_ALT);
    let spec = config::get_config_str("hotkey", "Pause/Break");
    if spec.trim().is_empty() {
        log::log("Hotkey: desativada (config vazia).");
        return;
    }
    let Some(hk) = hotkey::parse_hotkey(&spec) else {
        log!("Hotkey: spec invalido \"{spec}\" — ignorado.");
        return;
    };
    // Rede de segurança: a UI já bloqueia antes de mandar.
    if let Some(reason) = hotkey::reserved_reason(hk.modifiers, hk.vk) {
        log!("Hotkey: \"{spec}\" e reservado pelo Windows ({reason}) — ignorado.");
        return;
    }
    if ui::register_global_hotkey(recording::HK_RECORD_TOGGLE, hk.modifiers, hk.vk) {
        log!("Hotkey: registrado \"{spec}\".");
    } else {
        log!("Hotkey: RegisterHotKey falhou pra \"{spec}\" (outro app pode estar usando).");
    }
    // Ctrl+Pause não gera VK_PAUSE: vira VK_CANCEL ("Break"). Registra os dois.
    if hk.vk == hotkey::VK_PAUSE && hk.modifiers & hotkey::MOD_CTRL != 0 {
        if ui::register_global_hotkey(recording::HK_RECORD_TOGGLE_ALT, hk.modifiers, hotkey::VK_CANCEL) {
            log::log("Hotkey: alias Ctrl+Break (VK_CANCEL) registrado pra cobrir Ctrl+Pause.");
        } else {
            log::log("Hotkey: alias Ctrl+Break (VK_CANCEL) falhou — Ctrl+Pause pode nao disparar.");
        }
    }
}

/// Valida um atalho para a UI (`hotkey_validation_result`), com o motivo já
/// traduzido. A regra (lista de reservados) fica toda no backend.
pub fn handle_validate_hotkey(spec: &str) {
    let spec = spec.trim();
    let reason = if spec.is_empty() {
        None
    } else {
        match hotkey::parse_hotkey(spec) {
            None => Some("settings.hotkey.invalidSpec"),
            Some(hk) => hotkey::reserved_reason(hk.modifiers, hk.vk),
        }
    };
    post_json(json!({
        "type": "hotkey_validation_result",
        "hotkey": spec,
        "ok": reason.is_none(),
        "reason": reason.map(lang::t).unwrap_or_default(),
    }));
}

/// Vazio limpa; válido é gravado na forma canônica e aplicado na hora.
pub fn handle_set_hotkey(spec: &str) {
    let mut normalized = spec.trim().to_owned();
    if !normalized.is_empty() {
        let Some(hk) = hotkey::parse_hotkey(&normalized) else {
            log!("HandleSetHotkey: spec invalido \"{normalized}\" — ignorado.");
            return;
        };
        if let Some(reason) = hotkey::reserved_reason(hk.modifiers, hk.vk) {
            log!("HandleSetHotkey: \"{normalized}\" reservado pelo Windows ({reason}) — ignorado.");
            return;
        }
        normalized = hotkey::format_hotkey(hk.modifiers, hk.vk);
    }
    config::set_config_str("hotkey", &normalized);
    apply_hotkey_from_config();
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
    } else if ui::is_window_shown() {
        ui::remove_tray_icon();
    }
}

pub fn handle_set_hibernate(enable: bool) {
    config::set_config_bool("hibernate", enable);
    log!("Hibernate: {enable}");
    if !enable {
        ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
    } else if !ui::is_window_shown() {
        // Já escondida e sem gravar: arma agora, sem exigir reabrir/esconder.
        on_window_hidden();
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

/// Notificação do Windows clicada / "abrir" vindo da UI.
pub fn handle_tray_show() {
    ui::restore_from_tray();
}

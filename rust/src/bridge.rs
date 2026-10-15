//! Despachante central UI ↔ Rust (porte do esqueleto de `OBSBridge.pas`).
//!
//! JS → Rust: `window.chrome.webview.postMessage(jsonString)` →
//! `ui::on_web_message` → [`dispatch`]. Rust → JS: [`post_json`] →
//! `PostWebMessageAsJson` → `Bridge.handlers[type]` no `bridge.js`.
//!
//! O `if MsgType = '...' then ... else if ...` de ~150 ramos do Delphi vira um
//! `match` sobre o campo `type`. O protocolo (nomes de mensagem e campos) NÃO
//! muda: a UI em `exe\bin\64bit\ui\` é a mesma.
//!
//! ## Estado do porte
//! Implementados: `ready`/init, tema, idioma, toggles de dispositivo,
//! `open_url`, `ui_log`. Os demais tipos caem em [`not_ported`], que registra
//! no log e devolve um erro à UI — os handlers de gravação/exportação/
//! transcrição dependem do `OBSEngine`/`FFmpegOps`/`FFmpegExport`/
//! `OBSTranscribe`, ainda não portados (ver `rust/README.md`).

use serde_json::{json, Value};

use crate::{config, lang, log, mainthread, player, record_watch, ui};

mod devices;
pub mod export;
mod library;
mod recording;

use std::sync::atomic::{AtomicBool, Ordering};

/// Idle de hibernação: 1 min sem janela visível e sem gravação (porte futuro
/// do `OBSHibernate`); o id já existe porque a gravação o desarma.
pub const TIMER_HIBERNATE_IDLE: usize = 7008;

/// Lido por TODAS as workers: abortam cedo quando o app está fechando. Um
/// `AtomicBool` com `SeqCst` dá a atomicidade e a barreira de memória que o
/// Delphi buscava com `TInterlocked`.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

pub fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

/// Liga a UI a este bridge sem dependência direta de `ui` → `bridge` (o
/// `initialization` do Delphi). Chamado uma vez, antes de `ui::run`.
pub fn install_hooks() {
    recording::install();
    ui::set_hooks(ui::Hooks {
        on_timer: Some(on_timer),
        on_hotkey: Some(recording::on_hotkey),
        on_display_change: Some(devices::on_display_change),
        on_device_change: Some(devices::on_device_node_change),
        on_should_hide_on_close: Some(should_hide_on_close),
        on_shutdown: Some(shutdown),
    });
}

/// `OnTimer`: cada módulo reconhece os próprios ids; `false` = não é dele.
fn on_timer(id: usize) {
    if recording::on_timer(id) || devices::on_timer(id) {
        return;
    }
    on_timer_other(id);
}

/// `WM_CLOSE`: minimizar pra bandeja em vez de fechar? Sim com `closeToTray`
/// ligado ou gravando (nunca interromper a gravação por engano).
fn should_hide_on_close() -> bool {
    config::get_config_bool("closeToTray", true) || recording::recording_active()
}

/// Encerra o que está vivo; idempotente (o `ShutdownDone` do Delphi).
pub fn shutdown() {
    // Chamado de DOIS lugares no Delphi (o Run e a finalization); aqui basta o
    // hook, mas a guarda fica: repetir o teardown do libobs seria AV.
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    log::log("Shutdown: inicio");
    devices::shutdown();
    log::log("Shutdown: dispositivos off");
    record_watch::stop();
    log::log("Shutdown: RecordWatch ok");
    recording::shutdown();
    player::stop_player_server();
    log::log("Shutdown: fim");
}

/// Timers dos módulos ainda não portados caem aqui.
fn on_timer_other(id: usize) {
    log!("OnTimer: id={id} sem tratador (módulo ainda não portado).");
}

/// Envia um objeto JSON à UI (qualquer thread: vai pela fila da main).
pub fn post_json(value: Value) {
    mainthread::queue(move || ui::post_json_now(&value.to_string()));
}

/// Mostra erro na UI (`type: "error"`), já traduzido.
pub fn post_error(message: impl Into<String>) {
    post_json(json!({ "type": "error", "message": message.into() }));
}

fn str_field<'a>(obj: &'a Value, key: &str) -> &'a str {
    obj.get(key).and_then(Value::as_str).unwrap_or("")
}

fn bool_field(obj: &Value, key: &str, default: bool) -> bool {
    obj.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// Entrada única das mensagens vindas do JS.
pub fn dispatch(raw: &str) {
    let Ok(msg) = serde_json::from_str::<Value>(raw) else {
        log::log("Dispatch: payload não é JSON — descartado.");
        return;
    };
    if !msg.is_object() {
        log::log("Dispatch: payload não é JSON object — descartado.");
        return;
    }
    let ty = str_field(&msg, "type");
    if ty != "ui_log" && ty != "player_state" {
        log!("Dispatch: type=\"{ty}\"");
    }

    match ty {
        "ui_log" => log!("UI: {}", str_field(&msg, "message")),
        "ready" => do_init(),
        "record_start" => recording::handle_record_start(),
        "record_stop" => recording::handle_record_stop(),
        "set_theme" => handle_set_theme(str_field(&msg, "theme")),
        "set_language" => handle_set_language(str_field(&msg, "language")),
        "toggle_source" => devices::handle_toggle_source(str_field(&msg, "id"), bool_field(&msg, "enabled", false)),
        "set_source_hidden" => devices::handle_set_source_hidden(str_field(&msg, "id"), bool_field(&msg, "hidden", false)),
        "set_auto_source" => devices::handle_set_auto_source(str_field(&msg, "id"), bool_field(&msg, "enabled", true)),
        "player_state" => {
            // O modal do player abriu/fechou: suspende medidores e prévias
            // enquanto aberto (a sidebar está atrás dele).
            let open = bool_field(&msg, "open", false);
            devices::set_player_open_all(open);
            log!("PlayerOpen={open}");
        }
        "rename_recording" => library::handle_rename_recording(str_field(&msg, "id"), str_field(&msg, "newName")),
        "open_recording" => library::handle_open_recording(str_field(&msg, "id")),
        "open_record_dir" => library::handle_open_record_dir(),
        "play_recording" => library::handle_play_recording(str_field(&msg, "id")),
        "request_transcode" => library::handle_request_transcode(str_field(&msg, "id")),
        "request_video_info" => library::handle_request_video_info(str_field(&msg, "id")),
        "request_audio_tracks" => library::handle_request_audio_tracks(str_field(&msg, "id")),
        "request_waveform" => library::handle_request_waveform(
            str_field(&msg, "id"),
            msg.get("buckets").and_then(Value::as_i64).unwrap_or(0),
            bool_field(&msg, "hi", false),
        ),
        "request_keyframes" => library::handle_request_keyframes(str_field(&msg, "id")),
        "export_recording" => export::handle_export_recording(&msg),
        "cancel_export" => export::handle_cancel_export(),
        "delete_recording" => library::handle_delete_recording(str_field(&msg, "id")),
        "open_url" => handle_open_url(str_field(&msg, "url")),
        other => not_ported(other),
    }
}

fn not_ported(ty: &str) {
    log!("Dispatch: type=\"{ty}\" ainda não portado para Rust.");
}

// ---------------------------------------------------------------------------
// init / tema / idioma
// ---------------------------------------------------------------------------

/// `DoInit`: tema + `init` + dispositivos (áudio em worker, prévias, medidores,
/// hot-plug) + warmup do libobs. O servidor do player, a biblioteca e os
/// watchers de pasta/mic/apps entram com os módulos que os têm.
fn do_init() {
    push_theme();
    // HTTP local das prévias/vídeos; falhar aqui não derruba o app.
    player::start_player_server();
    log::log("DoInit: PlayerServer ok");
    // Lista e metadados (duração/miniatura) da pasta de gravação.
    library::push_recordings();
    library::scan_recordings_meta();
    log::log("DoInit: recordings ok");
    devices::do_init();
    // Refresh automático quando o usuário mexe na pasta pelo Explorer.
    library::start_record_watch();
    // Warmup do libobs com delay: a UI renderiza primeiro e a 1ª gravação fica
    // instantânea. NUNCA inicializar o libobs direto aqui (pode travar o
    // startup). Falha do warmup → a gravação inicializa sob demanda.
    recording::schedule_warmup();
    // Atalho global (falha de registro não é fatal — pegadinha #33).
    recording::apply_hotkey_from_config();
    log::log("DoInit: pronto (sem OBS — sobe no warmup ou na hora da gravacao).");
}

/// `HKCU\...\Themes\Personalize\AppsUseLightTheme`: 0 = dark, 1 = light.
fn detect_system_theme() -> &'static str {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    if rc.is_ok() && value == 0 { "dark" } else { "light" }
}

/// Resolve o tema configurado: `system` (segue o SO), `dark` ou `light`.
pub fn resolved_theme() -> String {
    let mut cfg = config::get_config_str("theme", "");
    if cfg.is_empty() {
        // 1ª execução: marca "estou seguindo o Windows", não foi escolha do user.
        cfg = "system".into();
        config::set_config_str("theme", &cfg);
    }
    match cfg.as_str() {
        "dark" | "light" => cfg,
        _ => detect_system_theme().to_owned(),
    }
}

pub fn push_theme() {
    let pref = config::get_config_str("theme", "system");
    let resolved = resolved_theme();
    log!("PushTheme: cfg=\"{pref}\" resolved=\"{resolved}\"");
    ui::set_dark_mode(resolved == "dark");
    post_json(json!({ "type": "theme", "theme": resolved, "pref": pref }));
}

fn handle_set_theme(theme: &str) {
    if !matches!(theme, "dark" | "light" | "system") {
        return;
    }
    config::set_config_str("theme", theme);
    push_theme();
}

fn handle_set_language(code: &str) {
    let pref = code.trim();
    config::set_config_str("language", pref);
    log!("Language: pref=\"{pref}\"");
    lang::init_language();
    post_json(json!({
        "type": "language_changed",
        "language": lang::current_language(),
        "languagePref": pref,
        "i18n": lang::current_bundle(),
    }));
}

// ---------------------------------------------------------------------------
// misc
// ---------------------------------------------------------------------------

/// Só http(s): a UI poderia mandar `file://` etc.
fn handle_open_url(url: &str) {
    if url.is_empty() {
        return;
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        log!("HandleOpenUrl: URL rejeitada (não http/https): {url}");
        return;
    }
    ui::shell_open(url);
}

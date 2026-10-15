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

use crate::{config, lang, log, mainthread, ui};

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
        "set_theme" => handle_set_theme(str_field(&msg, "theme")),
        "set_language" => handle_set_language(str_field(&msg, "language")),
        "toggle_source" => handle_toggle_source(str_field(&msg, "id"), bool_field(&msg, "enabled", false)),
        "set_source_hidden" => handle_set_source_hidden(str_field(&msg, "id"), bool_field(&msg, "hidden", false)),
        "set_auto_source" => handle_set_auto_source(str_field(&msg, "id"), bool_field(&msg, "enabled", true)),
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

/// `DoInit` mínimo: tema + `init` (bundle i18n). O restante (servidor do
/// player, enumeração de áudio em worker, timers, warmup do libobs 1,5 s
/// depois) entra com os módulos de domínio.
fn do_init() {
    push_theme();
    push_init();
}

fn push_init() {
    post_json(json!({
        "type": "init",
        "i18n": lang::current_bundle(),
        "language": lang::current_language(),
        "languagePref": config::get_config_str("language", ""),
        "languages": lang::available_languages(),
        "recordDir": config::get_config_str("recordDir", ""),
    }));
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
// dispositivos
// ---------------------------------------------------------------------------

/// Prefixos de id que a UI usa (`PFX_*` do Delphi): `monitor:0`, `mic:Nome`…
fn split_source_id(id: &str) -> Option<(&'static str, &str)> {
    const PREFIXES: [(&str, &str); 4] = [
        ("monitor:", "monitors"),
        ("webcam:", "webcams"),
        ("mic:", "mics"),
        ("out:", "speakers"),
    ];
    PREFIXES
        .iter()
        .find_map(|(p, cat)| id.strip_prefix(p).map(|rest| (*cat, rest)))
}

fn handle_toggle_source(id: &str, enabled: bool) {
    let Some((cat, name)) = split_source_id(id) else { return };
    // Monitores/webcams mudam o canvas: bloqueado durante a gravação.
    // (RecordingActive virá do módulo do engine.)
    config::set_source_bool(cat, name, enabled);
}

fn handle_set_source_hidden(id: &str, hidden: bool) {
    if let Some((cat, name)) = split_source_id(id) {
        config::set_source_hidden(cat, name, hidden);
    }
}

fn handle_set_auto_source(id: &str, enabled: bool) {
    if let Some((cat, name)) = split_source_id(id) {
        config::set_source_auto_record(cat, name, enabled);
    }
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

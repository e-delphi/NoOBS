//! O `dispatch` em si e as mensagens soltas do `bridge.rs` (tema, idioma,
//! link externo, log da página, player aberto).

use serde_json::json;

use crate::config;
use crate::testkit::{self, begin, send, send_raw};

// ---- dispatch ---------------------------------------------------------------

#[test]
fn json_invalido_nao_derruba_nem_responde() {
    let _s = begin();
    for raw in ["", "{", "não é json", "[1,2,3]", "42", "\"set_theme\"", "null"] {
        send_raw(raw);
    }
    assert!(testkit::sent().is_empty(), "{:?}", testkit::sent());
}

#[test]
fn tipo_desconhecido_ou_ausente_e_ignorado() {
    let _s = begin();
    send(json!({ "type": "nao_existe" }));
    send(json!({ "sem": "tipo" }));
    send(json!({ "type": 123 }));
    assert!(testkit::sent().is_empty());
}

#[test]
fn campo_com_tipo_errado_cai_no_padrao() {
    let _s = begin();
    // `theme` numérico vira "" → recusado; o config não muda.
    send(json!({ "type": "set_theme", "theme": 1 }));
    assert_eq!(config::get_config_str("theme", "nenhum"), "nenhum");
    // `enabled` como texto vale o padrão do handler (false).
    send(json!({ "type": "set_stop_on_lock", "enabled": "sim" }));
    assert!(!config::get_config_bool("stopOnLock", true));
}

#[test]
fn ui_log_so_registra() {
    let _s = begin();
    send(json!({ "type": "ui_log", "message": "olá da página" }));
    send(json!({ "type": "ui_log" }));
    assert!(testkit::sent().is_empty());
}

#[test]
fn player_state_aberto_e_fechado_nao_responde() {
    let _s = begin();
    send(json!({ "type": "player_state", "open": true }));
    send(json!({ "type": "player_state", "open": false }));
    send(json!({ "type": "player_state" }));
    assert!(testkit::sent().is_empty());
}

// ---- tema -------------------------------------------------------------------

#[test]
fn tema_aceita_os_tres_valores() {
    let _s = begin();
    for t in ["dark", "light", "system"] {
        testkit::clear_sent();
        send(json!({ "type": "set_theme", "theme": t }));
        assert_eq!(config::get_config_str("theme", ""), t);
        let msg = testkit::last_of("theme").expect("push do tema");
        assert_eq!(msg["pref"], t);
        // `system` resolve pro tema do Windows: sempre um dos dois.
        assert!(msg["theme"] == "dark" || msg["theme"] == "light", "{msg}");
    }
}

#[test]
fn tema_invalido_nao_muda_nada() {
    let _s = begin();
    send(json!({ "type": "set_theme", "theme": "dark" }));
    testkit::clear_sent();
    for t in ["", "DARK", "azul", " dark"] {
        send(json!({ "type": "set_theme", "theme": t }));
    }
    assert_eq!(config::get_config_str("theme", ""), "dark");
    assert!(testkit::sent_of("theme").is_empty());
}

// ---- idioma -----------------------------------------------------------------

#[test]
fn idioma_troca_e_manda_o_bundle() {
    let _s = begin();
    send(json!({ "type": "set_language", "language": "en" }));
    let msg = testkit::last_of("language_changed").expect("language_changed");
    assert_eq!(msg["language"], "en");
    assert_eq!(msg["languagePref"], "en");
    assert_eq!(msg["i18n"]["meta"]["code"], "en");
    assert_eq!(config::get_config_str("language", "?"), "en");

    send(json!({ "type": "set_language", "language": "es" }));
    assert_eq!(testkit::last_of("language_changed").unwrap()["language"], "es");
}

#[test]
fn idioma_vazio_volta_ao_automatico() {
    let _s = begin();
    send(json!({ "type": "set_language", "language": "  " }));
    let msg = testkit::last_of("language_changed").unwrap();
    assert_eq!(msg["languagePref"], "");
    // Automático = um dos idiomas que existem (o do Windows ou o inglês).
    let lang = msg["language"].as_str().unwrap();
    assert!(["pt-BR", "en", "es"].contains(&lang), "{lang}");
}

#[test]
fn idioma_inexistente_fica_no_padrao() {
    let _s = begin();
    send(json!({ "type": "set_language", "language": "xx" }));
    let msg = testkit::last_of("language_changed").unwrap();
    // A preferência é guardada como veio, mas o idioma ativo é um que existe.
    assert_eq!(msg["languagePref"], "xx");
    assert_ne!(msg["language"], "xx");
    assert!(msg["i18n"].is_object());
}

// ---- link externo -----------------------------------------------------------

#[test]
fn link_externo_so_http_e_https() {
    let _s = begin();
    for url in [
        "https://github.com/x",
        "http://exemplo.com",
        "file:///C:/Windows/System32/cmd.exe",
        "C:\\Windows\\notepad.exe",
        "javascript:alert(1)",
        "ftp://x",
        "",
    ] {
        send(json!({ "type": "open_url", "url": url }));
    }
    assert_eq!(
        testkit::effects(),
        vec!["shell_open:https://github.com/x", "shell_open:http://exemplo.com"]
    );
}

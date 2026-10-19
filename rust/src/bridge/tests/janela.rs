//! Janela, bandeja, atalho de gravar e preferências de comportamento
//! (`bridge/shell.rs` + os `set_rec_indicator*` de `bridge/recording.rs`).
//!
//! Sem janela principal no teste: bandeja, tela cheia e registro do atalho
//! viram no-op — o que se confere é que não quebram e o que vai pro config.

use serde_json::json;

use crate::config;
use crate::lang;
use crate::testkit::{self, begin, send};

#[test]
fn bandeja_e_tela_cheia_sem_janela_nao_quebram() {
    let _s = begin();
    send(json!({ "type": "tray_show" }));
    send(json!({ "type": "toggle_fullscreen" }));
    send(json!({ "type": "toggle_fullscreen" }));
    assert!(testkit::sent().is_empty());
}

#[test]
fn titulo_da_janela() {
    let _s = begin();
    send(json!({ "type": "set_window_title", "title": "  Meu Gravador  " }));
    assert_eq!(config::get_config_str("windowTitle", ""), "Meu Gravador");
    // Vazio (ou só espaço) volta pro padrão.
    for t in ["", "   "] {
        send(json!({ "type": "set_window_title", "title": t }));
        assert_eq!(config::get_config_str("windowTitle", ""), "NoOBS");
    }
    // Acento e emoji passam como vieram.
    send(json!({ "type": "set_window_title", "title": "Gravação 🎬" }));
    assert_eq!(config::get_config_str("windowTitle", ""), "Gravação 🎬");
    send(json!({ "type": "set_window_title" }));
    assert_eq!(config::get_config_str("windowTitle", ""), "NoOBS");
}

// ---- atalho de gravar -------------------------------------------------------

#[test]
fn atalho_valido_fica_na_forma_canonica() {
    let _s = begin();
    for (entrada, esperado) in [
        ("ctrl+shift+f9", "Ctrl+Shift+F9"),
        ("  Alt+F10 ", "Alt+F10"),
        ("Pause", "Pause/Break"),
        ("Ctrl+Pause/Break", "Ctrl+Pause/Break"),
        ("Ctrl+-", "Ctrl+-"),
        ("Ctrl+;", "Ctrl+;"),
        ("Win+Shift+Numpad5", "Shift+Win+Numpad5"),
    ] {
        send(json!({ "type": "set_hotkey", "hotkey": entrada }));
        assert_eq!(config::get_config_str("hotkey", "?"), esperado, "entrada {entrada:?}");
    }
}

#[test]
fn atalho_vazio_desliga() {
    let _s = begin();
    send(json!({ "type": "set_hotkey", "hotkey": "F9" }));
    send(json!({ "type": "set_hotkey", "hotkey": "" }));
    assert_eq!(config::get_config_str("hotkey", "?"), "");
}

#[test]
fn atalho_invalido_ou_reservado_nao_muda_o_config() {
    let _s = begin();
    send(json!({ "type": "set_hotkey", "hotkey": "F9" }));
    for spec in ["Ctrl+Shift", "Ctrl+Foo", "+++", "Alt+Tab", "Win+L", "Ctrl+Alt+Delete", "Alt+F4", "Win+Shift+S"] {
        send(json!({ "type": "set_hotkey", "hotkey": spec }));
        assert_eq!(config::get_config_str("hotkey", "?"), "F9", "spec {spec:?}");
    }
}

#[test]
fn atalho_de_gravar_nao_repete_buffer_nem_area() {
    let _s = begin();
    config::set_config_str("replayHotkey", "Ctrl+F10");
    config::set_config_str("areaHotkey", "Ctrl+F11");
    send(json!({ "type": "set_hotkey", "hotkey": "F9" }));
    for (spec, msg) in [("ctrl+f10", "error.replayHotkeyConflict"), ("Ctrl+F11", "error.areaHotkeyConflict")] {
        testkit::clear_sent();
        send(json!({ "type": "set_hotkey", "hotkey": spec }));
        assert_eq!(config::get_config_str("hotkey", "?"), "F9", "{spec}");
        assert_eq!(testkit::errors(), vec![lang::t(msg)], "{spec}");
    }
    // Os PADRÕES dos outros também contam (nunca configurados).
    config::set_config_json("replayHotkey", None);
    config::set_config_json("areaHotkey", None);
    for padrao in ["Ctrl+Shift+F10", "Ctrl+Shift+Pause/Break"] {
        send(json!({ "type": "set_hotkey", "hotkey": padrao }));
        assert_eq!(config::get_config_str("hotkey", "?"), "F9", "{padrao}");
    }
}

#[test]
fn validar_atalho_responde_com_o_motivo() {
    let _s = begin();
    let caso = |spec: &str| {
        testkit::clear_sent();
        send(json!({ "type": "validate_hotkey", "hotkey": spec }));
        testkit::last_of("hotkey_validation_result").expect("resposta")
    };
    let ok = caso("Ctrl+Shift+F9");
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["reason"], "");

    let vazio = caso("   ");
    assert_eq!(vazio["ok"], true, "vazio = desligar, é válido");
    assert_eq!(vazio["hotkey"], "");

    let invalido = caso("Ctrl+Shift");
    assert_eq!(invalido["ok"], false);
    assert_eq!(invalido["reason"], lang::t("settings.hotkey.invalidSpec"));

    for (spec, chave) in [
        ("Alt+Tab", "altTab"),
        ("Win+L", "winL"),
        ("Ctrl+Alt+Delete", "ctrlAltDel"),
        ("Win+Up", "winArrows"),
        ("Ctrl+Esc", "ctrlEsc"),
    ] {
        let r = caso(spec);
        assert_eq!(r["ok"], false, "{spec}");
        assert_eq!(r["reason"], lang::t(&format!("settings.hotkey.reserved.{chave}")), "{spec}");
        // O motivo sai traduzido, nunca a chave crua.
        assert!(!r["reason"].as_str().unwrap().starts_with('['), "{spec}: {}", r["reason"]);
    }
}

// ---- preferências -----------------------------------------------------------

#[test]
fn autostart_vai_pro_registro_so_como_efeito() {
    let _s = begin();
    send(json!({ "type": "set_autostart", "enabled": true }));
    send(json!({ "type": "set_autostart", "enabled": false }));
    send(json!({ "type": "set_autostart" }));
    assert_eq!(testkit::effects(), vec!["autostart:true", "autostart:false", "autostart:false"]);
    // Não é preferência do config (o instalador é o dono — pegadinha #46).
    assert!(config::get_config_json("autostart").is_none());
}

#[test]
fn preferencias_booleanas_vao_pro_config() {
    let _s = begin();
    for (tipo, chave) in [
        ("set_run_as_admin", "runAsAdmin"),
        ("set_close_to_tray", "closeToTray"),
        ("set_hibernate", "hibernate"),
        ("set_minimize_on_record", "minimizeOnRecord"),
        ("set_notify_on_record", "notifyOnRecord"),
        ("set_play_sound_on_record", "playSoundOnRecord"),
        ("set_stop_on_lock", "stopOnLock"),
        ("set_rec_indicator", "recIndicator"),
        ("set_rec_indicator_clickable", "recIndicatorClickable"),
        ("set_fullscreen_detect", "fullscreenDetect"),
    ] {
        for v in [true, false, true] {
            send(json!({ "type": tipo, "enabled": v }));
            assert_eq!(config::get_config_bool(chave, !v), v, "{tipo} = {v}");
        }
    }
    assert!(testkit::sent().is_empty(), "nenhuma delas responde: {:?}", testkit::sent());
}

#[test]
fn campo_ausente_usa_o_padrao_de_cada_handler() {
    let _s = begin();
    // Padrão false no dispatch...
    send(json!({ "type": "set_close_to_tray" }));
    assert!(!config::get_config_bool("closeToTray", true));
    // ...e true nos dois que nascem ligados.
    send(json!({ "type": "set_rec_indicator_clickable" }));
    assert!(config::get_config_bool("recIndicatorClickable", false));
    send(json!({ "type": "set_fullscreen_detect" }));
    assert!(config::get_config_bool("fullscreenDetect", false));
}

#[test]
fn canto_e_opacidade_do_indicador() {
    let _s = begin();
    for c in ["top-left", "top-right", "bottom-left", "bottom-right"] {
        send(json!({ "type": "set_rec_indicator_corner", "corner": c }));
        assert_eq!(config::get_config_str("recIndicatorCorner", ""), c);
    }
    for pct in [20, 55, 100] {
        send(json!({ "type": "set_rec_indicator_opacity", "opacity": pct }));
        assert_eq!(config::get_config_int("recIndicatorOpacity", 0), pct);
    }
    // Sem o campo: 90, o padrão.
    send(json!({ "type": "set_rec_indicator_opacity" }));
    assert_eq!(config::get_config_int("recIndicatorOpacity", 0), 90);
    // Fora da faixa do slider: limitado (abaixo de 20% o overlay some).
    for (entrada, gravado) in [(0, 20), (-50, 20), (19, 20), (101, 100), (500, 100)] {
        send(json!({ "type": "set_rec_indicator_opacity", "opacity": entrada }));
        assert_eq!(config::get_config_int("recIndicatorOpacity", 0), gravado, "entrada {entrada}");
    }
}

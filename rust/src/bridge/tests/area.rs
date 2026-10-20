//! Captura de área (`bridge/area.rs`): área salva, atalho, atraso e a
//! seleção. A seleção de verdade escurece os monitores; aqui ela fica "aberta"
//! até `area_select::test_finish` dizer o que o usuário arrastou.
//!
//! Gravar de fato a área é dos testes externos (`tests/`). Aqui só os caminhos que param
//! antes: cancelar, recusar, guardar.

use serde_json::json;

use crate::area_select;
use crate::config;
use crate::engine::AreaRect;
use crate::lang;
use crate::testkit::{self, begin, send};

fn state() -> serde_json::Value {
    testkit::clear_sent();
    send(json!({ "type": "get_area_state" }));
    testkit::last_of("area_state").expect("area_state")
}

#[test]
fn estado_de_instalacao_nova() {
    let _s = begin();
    let st = state();
    assert!(st["saved"].is_null());
    assert_eq!(st["hotkey"], "Ctrl+Shift+Pause/Break");
    assert_eq!(st["delay"], 0);
    assert_eq!(st["delayMax"], 60);
}

// ---- atraso -----------------------------------------------------------------

#[test]
fn atraso_fica_entre_0_e_60() {
    let _s = begin();
    for (entrada, gravado) in [(3, 3), (0, 0), (-5, 0), (60, 60), (61, 60), (999, 60), (10, 10)] {
        send(json!({ "type": "set_area_delay", "sec": entrada }));
        assert_eq!(config::get_config_int("areaDelaySec", -1), gravado, "entrada {entrada}");
        assert_eq!(testkit::last_of("area_state").unwrap()["delay"], gravado);
    }
    send(json!({ "type": "set_area_delay" }));
    assert_eq!(state()["delay"], 0);
}

#[test]
fn atraso_editado_a_mao_tambem_e_limitado() {
    let _s = begin();
    config::set_config_int("areaDelaySec", 5000);
    assert_eq!(state()["delay"], 60);
    config::set_config_int("areaDelaySec", -1);
    assert_eq!(state()["delay"], 0);
}

// ---- atalho -----------------------------------------------------------------

#[test]
fn atalho_da_area_normaliza() {
    let _s = begin();
    send(json!({ "type": "set_area_hotkey", "hotkey": "ctrl+alt+f8" }));
    assert_eq!(config::get_config_str("areaHotkey", ""), "Ctrl+Alt+F8");
    assert_eq!(testkit::last_of("area_state").unwrap()["hotkey"], "Ctrl+Alt+F8");
    // Vazio = sem atalho.
    send(json!({ "type": "set_area_hotkey", "hotkey": "" }));
    assert_eq!(config::get_config_str("areaHotkey", "?"), "");
}

#[test]
fn atalho_da_area_nao_repete_os_outros() {
    let _s = begin();
    config::set_config_str("hotkey", "Ctrl+F9");
    config::set_config_str("replayHotkey", "Ctrl+F10");
    send(json!({ "type": "set_area_hotkey", "hotkey": "Alt+F1" }));
    for (spec, de_quem) in [("ctrl+f9", "gravar"), ("Ctrl+F10", "buffer")] {
        testkit::clear_sent();
        send(json!({ "type": "set_area_hotkey", "hotkey": spec }));
        assert_eq!(config::get_config_str("areaHotkey", ""), "Alt+F1", "conflito com o de {de_quem}");
        assert_eq!(testkit::errors(), vec![lang::t("error.areaHotkeyConflict")]);
        // A tela volta a mostrar o atalho que ficou.
        assert_eq!(testkit::last_of("area_state").unwrap()["hotkey"], "Alt+F1");
    }
}

#[test]
fn atalho_da_area_invalido_ou_reservado_e_ignorado() {
    let _s = begin();
    send(json!({ "type": "set_area_hotkey", "hotkey": "Alt+F1" }));
    for spec in ["Ctrl+Shift", "lixo", "Alt+Tab", "Win+D"] {
        send(json!({ "type": "set_area_hotkey", "hotkey": spec }));
        assert_eq!(config::get_config_str("areaHotkey", ""), "Alt+F1", "{spec}");
    }
}

// ---- área salva -------------------------------------------------------------

#[test]
fn definir_area_abre_a_selecao_e_guarda() {
    let _s = begin();
    send(json!({ "type": "define_capture_area" }));
    assert_eq!(testkit::effects(), vec!["area_select:areaSelect.hintSave"]);
    assert!(area_select::is_active());
    // Com a seleção aberta, um segundo pedido não abre outra.
    send(json!({ "type": "define_capture_area" }));
    assert_eq!(testkit::effects().len(), 1);

    testkit::clear_sent();
    assert!(area_select::test_finish(Some(AreaRect { x: 100, y: 50, w: 1280, h: 720 })));
    let st = testkit::last_of("area_state").unwrap();
    assert_eq!(st["saved"], json!({ "x": 100, "y": 50, "w": 1280, "h": 720 }));
    assert_eq!(config::get_config_json("areaSaved").unwrap()["w"], 1280);
}

#[test]
fn definir_area_cancelado_nao_guarda_mas_responde() {
    let _s = begin();
    send(json!({ "type": "define_capture_area" }));
    testkit::clear_sent();
    area_select::test_finish(None);
    assert!(testkit::last_of("area_state").unwrap()["saved"].is_null());
    assert!(config::get_config_json("areaSaved").is_none());
}

#[test]
fn redefinir_troca_a_area_e_remover_apaga() {
    let _s = begin();
    for r in [AreaRect { x: 0, y: 0, w: 640, h: 480 }, AreaRect { x: -1920, y: 10, w: 800, h: 600 }] {
        send(json!({ "type": "define_capture_area" }));
        area_select::test_finish(Some(r));
        let saved = state()["saved"].clone();
        assert_eq!(saved, json!({ "x": r.x, "y": r.y, "w": r.w, "h": r.h }));
    }
    send(json!({ "type": "clear_capture_area" }));
    assert!(testkit::last_of("area_state").unwrap()["saved"].is_null());
    assert!(config::get_config_json("areaSaved").is_none());
    // Remover sem nada salvo também responde.
    testkit::clear_sent();
    send(json!({ "type": "clear_capture_area" }));
    assert!(testkit::last_of("area_state").is_some());
}

#[test]
fn area_salva_corrompida_conta_como_nenhuma() {
    let _s = begin();
    for ruim in [
        json!({ "x": 0, "y": 0, "w": 10, "h": 10 }),     // menor que o mínimo (32)
        json!({ "x": 0, "y": 0, "w": 640 }),             // falta campo
        json!({ "x": "0", "y": 0, "w": 640, "h": 480 }), // tipo errado
        json!([1, 2, 3, 4]),
    ] {
        config::set_config_json("areaSaved", Some(ruim.clone()));
        assert!(state()["saved"].is_null(), "{ruim}");
    }
}

// ---- gravar (só os caminhos que param antes do motor) ------------------------

#[test]
fn gravar_area_salva_sem_area_avisa() {
    let _s = begin();
    send(json!({ "type": "record_saved_area" }));
    assert_eq!(testkit::errors(), vec![lang::t("error.areaNotSaved")]);
}

#[test]
fn gravar_area_salva_fora_das_telas_avisa() {
    let _s = begin();
    config::set_config_json("areaSaved", Some(json!({ "x": -900000, "y": -900000, "w": 640, "h": 480 })));
    send(json!({ "type": "record_saved_area" }));
    assert_eq!(testkit::errors(), vec![lang::t("error.areaOffScreen")]);
}

#[test]
fn selecionar_area_e_cancelar() {
    let _s = begin();
    send(json!({ "type": "select_capture_area" }));
    assert_eq!(testkit::effects(), vec!["area_select:areaSelect.hint"]);
    // Aberta: outro pedido não abre outra seleção.
    send(json!({ "type": "select_capture_area" }));
    assert_eq!(testkit::effects().len(), 1);
    area_select::test_finish(None);
    assert_eq!(testkit::sent_of("capture_area_canceled").len(), 1);
}

#[test]
fn cancelar_contagem_sem_contagem_nao_faz_nada() {
    let _s = begin();
    send(json!({ "type": "cancel_area_countdown" }));
    assert!(testkit::sent().is_empty());
}

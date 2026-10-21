//! Aba Atalhos (`bridge/hotkeys.rs`): a lista, a migração das chaves antigas,
//! a situação de cada item e os caminhos de execução que param antes do motor
//! (contagem, avisos, seleção). Sem janela principal no teste, o
//! `RegisterHotKey` falha: os itens que chegam até ele ficam "busy".

use serde_json::{json, Value};

use crate::area_select;
use crate::config;
use crate::engine::AreaRect;
use crate::lang;
use crate::testkit::{self, begin, send};

fn state() -> Value {
    testkit::clear_sent();
    send(json!({ "type": "get_hotkeys" }));
    testkit::last_of("hotkeys_state").expect("hotkeys_state")
}

fn set(items: Value) -> Value {
    testkit::clear_sent();
    send(json!({ "type": "set_hotkeys", "items": items }));
    testkit::last_of("hotkeys_state").expect("hotkeys_state")
}

#[test]
fn instalacao_nova_tem_gravar_e_salvar_trecho() {
    let _s = begin();
    let st = state();
    assert_eq!(
        st["items"],
        json!([
            { "mode": "record", "hotkey": "Pause/Break", "delaySec": 0 },
            { "mode": "replay", "hotkey": "Ctrl+Shift+F10" }
        ])
    );
    assert_eq!(st["delayMax"], 60);
    // Buffer desligado: o de salvar trecho fica inativo, não com erro.
    assert_eq!(st["status"][1], "inactive");
    assert!(config::get_config_json("hotkeys").is_some(), "a lista vai pro config");
}

#[test]
fn chaves_antigas_viram_itens_e_somem() {
    let _s = begin();
    config::set_config_str("hotkey", "ctrl+f9");
    config::set_config_str("replayHotkey", "");
    config::set_config_str("areaHotkey", "Ctrl+Pause/Break");
    config::set_config_json("areaSaved", Some(json!({ "x": -1683, "y": 1114, "w": 642, "h": 465 })));
    config::set_config_int("areaDelaySec", 3);
    let st = state();
    assert_eq!(
        st["items"],
        json!([
            { "mode": "record", "hotkey": "Ctrl+F9", "delaySec": 0 },
            { "mode": "area", "hotkey": "Ctrl+Pause/Break", "area": { "x": -1683, "y": 1114, "w": 642, "h": 465 }, "delaySec": 3 }
        ])
    );
    for k in ["hotkey", "replayHotkey", "areaHotkey", "areaSaved", "areaDelaySec"] {
        assert_eq!(config::get_config_str(k, "-"), "-", "{k}");
        assert!(config::get_config_json(k).is_none(), "{k}");
    }
}

#[test]
fn lista_gravada_normaliza_e_marca_problemas() {
    let _s = begin();
    let st = set(json!([
        { "mode": "record", "hotkey": "ctrl+shift+f9" },
        { "mode": "window", "hotkey": "Ctrl+Shift+F9", "title": "  nova guia ", "appAudio": true, "delaySec": 999 },
        { "mode": "area", "hotkey": "Alt+Tab", "area": null },
        { "mode": "window", "hotkey": "lixo" },
        { "mode": "replay", "hotkey": "F8", "delaySec": 5 },
        { "mode": "nao-existe", "hotkey": "F7" }
    ]));
    let items = st["items"].as_array().unwrap();
    assert_eq!(items.len(), 5, "modo desconhecido sai");
    assert_eq!(items[0]["hotkey"], "Ctrl+Shift+F9");
    assert_eq!(items[1]["title"], "nova guia");
    assert_eq!(items[1]["appAudio"], true);
    assert_eq!(items[1]["delaySec"], 60);
    assert_eq!(items[3]["hotkey"], "");
    assert!(items[4].get("delaySec").is_none());
    assert_eq!(st["status"], json!(["busy", "duplicate", "reserved", "empty", "inactive"]));
    assert_eq!(config::get_config_json("hotkeys").unwrap(), st["items"]);
}

#[test]
fn restaurar_padroes() {
    let _s = begin();
    set(json!([{ "mode": "area", "hotkey": "F2", "area": null }]));
    testkit::clear_sent();
    send(json!({ "type": "reset_hotkeys" }));
    let st = testkit::last_of("hotkeys_state").unwrap();
    assert_eq!(st["items"][0]["mode"], "record");
    assert_eq!(st["items"][1]["mode"], "replay");
}

#[test]
fn dica_do_buffer_mostra_o_primeiro_atalho_de_salvar() {
    let _s = begin();
    set(json!([{ "mode": "record", "hotkey": "F9" }, { "mode": "replay", "hotkey": "alt+f10" }]));
    let st = testkit::last_of("replay_state").expect("replay_state reenviado");
    assert_eq!(st["hotkey"], "Alt+F10");
}

// ---- área do atalho -----------------------------------------------------------

#[test]
fn definir_area_de_um_item() {
    let _s = begin();
    set(json!([{ "mode": "record", "hotkey": "F9" }, { "mode": "area", "hotkey": "F10", "area": null }]));
    send(json!({ "type": "define_hotkey_area", "index": 1 }));
    assert_eq!(testkit::effects(), vec!["area_select:areaSelect.hintSave"]);
    assert_eq!(testkit::last_of("hotkeys_state").unwrap()["defining"], 1);
    // Aberta: outro pedido não abre outra seleção.
    send(json!({ "type": "define_hotkey_area", "index": 1 }));
    assert_eq!(testkit::effects().len(), 1);
    testkit::clear_sent();
    area_select::test_finish(Some(AreaRect { x: 100, y: 50, w: 1280, h: 720 }));
    let st = testkit::last_of("hotkeys_state").unwrap();
    assert_eq!(st["items"][1]["area"], json!({ "x": 100, "y": 50, "w": 1280, "h": 720 }));
    assert!(st["defining"].is_null());
    assert_eq!(config::get_config_json("hotkeys").unwrap()[1]["area"]["w"], 1280);
}

#[test]
fn definir_area_cancelado_ou_item_errado_nao_muda_nada() {
    let _s = begin();
    set(json!([{ "mode": "record", "hotkey": "F9" }, { "mode": "area", "hotkey": "F10", "area": null }]));
    send(json!({ "type": "define_hotkey_area", "index": 1 }));
    testkit::clear_sent();
    area_select::test_finish(None);
    assert!(testkit::last_of("hotkeys_state").unwrap()["items"][1]["area"].is_null());
    // Índice fora da lista: nem abre a seleção.
    send(json!({ "type": "define_hotkey_area", "index": 7 }));
    assert_eq!(testkit::effects().len(), 1);
    // Item que não é de área: abre, mas o retângulo não entra nele.
    send(json!({ "type": "define_hotkey_area", "index": 0 }));
    area_select::test_finish(Some(AreaRect { x: 0, y: 0, w: 640, h: 480 }));
    assert!(state()["items"][0].get("area").is_none());
}

// ---- executar (só os caminhos que param antes do motor) ----------------------

#[test]
fn gravar_com_atraso_conta_e_o_mesmo_atalho_cancela() {
    let _s = begin();
    set(json!([{ "mode": "record", "hotkey": "F9", "delaySec": 5 }]));
    testkit::clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    assert_eq!(testkit::last_of("record_countdown").unwrap()["sec"], 5);
    testkit::clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    assert_eq!(testkit::last_of("record_countdown").unwrap()["sec"], 0);
    // Cancelar sem contagem: nada.
    testkit::clear_sent();
    send(json!({ "type": "cancel_countdown" }));
    assert!(testkit::sent().is_empty());
}

#[test]
fn botao_de_gravar_cancela_a_contagem() {
    let _s = begin();
    set(json!([{ "mode": "record", "hotkey": "F9", "delaySec": 3 }]));
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    testkit::clear_sent();
    send(json!({ "type": "record_start" }));
    assert_eq!(testkit::last_of("record_countdown").unwrap()["sec"], 0);
}

#[test]
fn area_salva_conta_com_a_moldura() {
    let _s = begin();
    set(json!([{ "mode": "area", "hotkey": "F9", "area": { "x": 0, "y": 0, "w": 400, "h": 300 }, "delaySec": 2 }]));
    testkit::clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    assert_eq!(testkit::last_of("record_countdown").unwrap()["sec"], 2);
    send(json!({ "type": "cancel_countdown" }));
}

#[test]
fn area_fora_das_telas_avisa() {
    let _s = begin();
    set(json!([{ "mode": "area", "hotkey": "F9", "area": { "x": -900000, "y": -900000, "w": 640, "h": 480 } }]));
    testkit::clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    assert_eq!(testkit::errors(), vec![lang::t("error.areaOffScreen")]);
}

#[test]
fn area_sem_retangulo_abre_a_selecao_e_conta_depois() {
    let _s = begin();
    set(json!([{ "mode": "area", "hotkey": "F9", "area": null, "delaySec": 4 }]));
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    assert_eq!(testkit::effects(), vec!["area_select:areaSelect.hint"]);
    testkit::clear_sent();
    area_select::test_finish(Some(AreaRect { x: 0, y: 0, w: 300, h: 200 }));
    assert_eq!(testkit::last_of("record_countdown").unwrap()["sec"], 4);
    send(json!({ "type": "cancel_countdown" }));
    // Cancelada: avisa a tela, sem contagem.
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    testkit::clear_sent();
    area_select::test_finish(None);
    assert_eq!(testkit::sent_of("capture_area_canceled").len(), 1);
    assert!(testkit::last_of("record_countdown").is_none());
}

#[test]
fn janela_sem_titulo_avisa() {
    let _s = begin();
    set(json!([{ "mode": "window", "hotkey": "F9", "title": "  " }]));
    testkit::clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    assert_eq!(testkit::errors(), vec![lang::t("hotkeys.noTitle")]);
}

#[test]
fn janela_que_nao_existe_avisa() {
    let _s = begin();
    let titulo = "zz-noobs-janela-que-nao-existe-zz";
    set(json!([{ "mode": "window", "hotkey": "F9", "title": titulo }]));
    testkit::clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    assert!(testkit::wait_for("error", 10_000).is_some(), "a busca roda numa worker");
    assert_eq!(testkit::errors(), vec![lang::t_with("hotkeys.noWindow", &[("title", titulo)])]);
}

#[test]
fn salvar_trecho_com_buffer_desligado_nao_faz_nada() {
    let _s = begin();
    set(json!([{ "mode": "replay", "hotkey": "F9" }]));
    testkit::clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    send(json!({ "type": "trigger_hotkey", "index": 9 }));
    assert!(testkit::errors().is_empty());
    assert!(testkit::last_of("record_countdown").is_none());
}

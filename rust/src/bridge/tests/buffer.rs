//! Preferências do buffer em memória (`bridge/replay.rs`) com o buffer
//! DESLIGADO — ligar/salvar passa pelo motor (testes externos, `tests/`).

use serde_json::json;

use crate::config;
use crate::testkit::{self, begin, send};

fn state() -> serde_json::Value {
    testkit::clear_sent();
    send(json!({ "type": "get_replay_state" }));
    testkit::last_of("replay_state").expect("replay_state")
}

#[test]
fn estado_de_instalacao_nova() {
    let _s = begin();
    let st = state();
    assert_eq!(st["enabled"], false);
    assert_eq!(st["active"], false);
    assert_eq!(st["saving"], false);
    assert_eq!(st["autoStart"], false);
    assert_eq!(st["intoRecording"], true);
    assert_eq!(st["maxSec"], 300);
    assert_eq!(st["hotkey"], "Ctrl+Shift+F10");
    assert_eq!(st["apps"], "");
    assert_eq!(st["indicator"], true);
    assert_eq!(st["indicatorCorner"], "top-right");
    assert_eq!(st["indicatorOpacity"], 90);
    assert_eq!(st["indicatorClickable"], true);
    // 2048 MB, ou o teto da máquina se ela tiver menos RAM livre que isso.
    let limit = st["maxMbLimit"].as_i64().unwrap();
    assert!(limit >= 128);
    assert_eq!(st["maxMb"].as_i64().unwrap(), 2048.min(limit));
}

#[test]
fn preferencias_booleanas() {
    let _s = begin();
    for (tipo, campo, chave, json_campo) in [
        ("set_replay_indicator", "enabled", "replayIndicator", "indicator"),
        ("set_replay_indicator_clickable", "enabled", "replayIndicatorClickable", "indicatorClickable"),
        ("set_replay_autostart", "autoStart", "replayAutoStart", "autoStart"),
        ("set_replay_into_recording", "enabled", "replayIntoRecording", "intoRecording"),
    ] {
        for v in [false, true] {
            testkit::clear_sent();
            send(json!({ "type": tipo, campo: v }));
            assert_eq!(config::get_config_bool(chave, !v), v, "{tipo}");
            assert_eq!(testkit::last_of("replay_state").expect(tipo)[json_campo], v, "{tipo}");
        }
    }
}

#[test]
fn canto_e_opacidade_do_indicador() {
    let _s = begin();
    send(json!({ "type": "set_replay_indicator_corner", "corner": "bottom-left" }));
    assert_eq!(testkit::last_of("replay_state").unwrap()["indicatorCorner"], "bottom-left");
    for (entrada, gravado) in [(20, 20), (5, 20), (0, 20), (150, 100), (100, 100), (65, 65)] {
        send(json!({ "type": "set_replay_indicator_opacity", "opacity": entrada }));
        assert_eq!(config::get_config_int("replayIndicatorOpacity", 0), gravado, "entrada {entrada}");
    }
}

#[test]
fn limites_de_tempo_e_memoria() {
    let _s = begin();
    let limit = state()["maxMbLimit"].as_i64().unwrap();
    for (sec, mostrado) in [(60, 60), (5, 10), (3600, 3600), (99999, 3600)] {
        send(json!({ "type": "set_replay_limits", "maxSec": sec, "maxMb": 0 }));
        assert_eq!(testkit::last_of("replay_state").unwrap()["maxSec"], mostrado, "sec {sec}");
    }
    for (mb, mostrado) in [(512, 512.min(limit)), (1, 128), (10_000_000, limit)] {
        send(json!({ "type": "set_replay_limits", "maxSec": 0, "maxMb": mb }));
        assert_eq!(testkit::last_of("replay_state").unwrap()["maxMb"].as_i64().unwrap(), mostrado, "mb {mb}");
    }
    // Zero (ou ausente) mantém o que estava.
    send(json!({ "type": "set_replay_limits", "maxSec": 120, "maxMb": 256 }));
    send(json!({ "type": "set_replay_limits" }));
    let st = testkit::last_of("replay_state").unwrap();
    assert_eq!(st["maxSec"], 120);
    assert_eq!(st["maxMb"].as_i64().unwrap(), 256.min(limit));
}

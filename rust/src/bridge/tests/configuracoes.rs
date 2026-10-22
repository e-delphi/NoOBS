//! Tela de Configurações (`bridge/settings.rs`): o push `settings` e os
//! `set_*` que não chegam em DLL.

use serde_json::json;

use crate::config;
use crate::testkit::{self, begin, send};

fn settings() -> serde_json::Value {
    testkit::clear_sent();
    send(json!({ "type": "get_settings" }));
    testkit::last_of("settings").expect("push settings")
}

#[test]
fn settings_de_instalacao_nova() {
    let s = begin();
    let st = settings();
    assert_eq!(st["recordDir"], s.rec.to_string_lossy().as_ref());
    assert_eq!(st["windowTitle"], "NoOBS");
    assert_eq!(st["codec"], "auto");
    assert_eq!(st["filenamePattern"], "{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}");
    assert_eq!(st["recordingQuality"], 5);
    assert_eq!(st["recordingFps"], 30);
    assert_eq!(st["recordingKeyframeSec"], 2);
    assert_eq!(st["audioBitrate"], 192);
    assert_eq!(st["libraryThumbs"], "auto");
    assert_eq!(st["transcribeLanguage"], "app");
    for (k, v) in [
        ("closeToTray", true),
        ("minimizeOnRecord", true),
        ("recIndicator", false),
        ("recIndicatorClickable", true),
        ("playSoundOnRecord", true),
        ("stopOnLock", true),
        ("hibernate", true),
        ("autoRecordOnMic", false),
        ("checkUpdates", true),
        ("runAsAdmin", false),
        ("fullscreenDetect", true),
        ("transcribeOnStop", true),
        ("muteWhenDeviceMuted", true),
    ] {
        assert_eq!(st[k], v, "{k}");
    }
    assert!(st["availableLanguages"].as_array().is_some_and(|a| a.len() >= 3));
    assert!(st["maxMonitorHz"].as_i64().is_some());
    // O push do buffer vai junto, na mesma ida.
    assert!(testkit::last_of("replay_state").is_some());
}

#[test]
fn settings_refletem_o_config() {
    let _s = begin();
    config::set_config_str("windowTitle", "X");
    config::set_config_int("recordingFps", 144);
    config::set_config_bool("stopOnLock", false);
    config::set_config_str("transcribeLanguage", "  EN ");
    let st = settings();
    assert_eq!(st["windowTitle"], "X");
    assert_eq!(st["recordingFps"], 144);
    assert_eq!(st["stopOnLock"], false);
    assert_eq!(st["transcribeLanguage"], "en");
}

#[test]
fn qualidade_da_escala_antiga_migra() {
    let _s = begin();
    // Escala -4..+2 (pegadinha #53): 0 → 5, -4 → 1, +2 → 9.
    for (antigo, novo) in [(0, 5), (-4, 1), (2, 9)] {
        config::set_config_int("recordingQuality", antigo);
        assert_eq!(settings()["recordingQuality"], novo, "antigo {antigo}");
    }
    // A chave nova vence a antiga.
    config::set_config_int("recordingQualityLevel", 7);
    assert_eq!(settings()["recordingQuality"], 7);
}

#[test]
fn keyframe_fica_entre_1_e_10() {
    let _s = begin();
    for (entrada, gravado) in [(0, 1), (-3, 1), (1, 1), (4, 4), (10, 10), (11, 10), (999, 10)] {
        send(json!({ "type": "set_recording_keyframe", "sec": entrada }));
        assert_eq!(config::get_config_int("recordingKeyframeSec", 0), gravado, "entrada {entrada}");
    }
    send(json!({ "type": "set_recording_keyframe" }));
    assert_eq!(config::get_config_int("recordingKeyframeSec", 0), 1);
}

#[test]
fn modelo_do_nome_do_arquivo() {
    let _s = begin();
    send(json!({ "type": "set_filename_pattern", "pattern": "  NoOBS {AAAA}-{MM} {CODEC}  " }));
    assert_eq!(config::get_config_str("filenamePattern", ""), "NoOBS {AAAA}-{MM} {CODEC}");
    for vazio in ["", "    "] {
        send(json!({ "type": "set_filename_pattern", "pattern": vazio }));
        assert_eq!(config::get_config_str("filenamePattern", ""), "{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}");
    }
    // Caractere ilegal é guardado cru: quem limpa é a montagem do nome.
    send(json!({ "type": "set_filename_pattern", "pattern": "a:b*c" }));
    assert_eq!(config::get_config_str("filenamePattern", ""), "a:b*c");
}

#[test]
fn escolher_pasta_abre_o_dialogo_e_cancelado_nao_responde() {
    let s = begin();
    send(json!({ "type": "pick_record_dir" }));
    assert_eq!(testkit::effects(), vec![format!("pick_folder:{}", s.rec.to_string_lossy())]);
    assert!(testkit::sent_of("record_dir_picked").is_empty());
}

#[test]
fn auto_gravacao_por_microfone() {
    let _s = begin();
    send(json!({ "type": "set_auto_record_mic_apps", "apps": "  teams, whatsapp  " }));
    assert_eq!(config::get_config_str("autoRecordMicApps", ""), "teams, whatsapp");
    send(json!({ "type": "set_auto_record_mic_except", "apps": " steam " }));
    assert_eq!(config::get_config_str("autoRecordMicExcept", ""), "steam");
    send(json!({ "type": "set_auto_record_mic_except", "apps": "" }));
    assert_eq!(config::get_config_str("autoRecordMicExcept", "?"), "");

    // Liga (sobe o vigia do microfone) e desliga (derruba) — os dois só no config
    // e na thread do vigia, sem resposta pra página.
    send(json!({ "type": "set_auto_record_on_mic", "enabled": true }));
    assert!(config::get_config_bool("autoRecordOnMic", false));
    assert!(crate::mic_watch::is_running());
    send(json!({ "type": "set_auto_record_on_mic", "enabled": false }));
    assert!(!config::get_config_bool("autoRecordOnMic", true));
    assert!(!crate::mic_watch::is_running());
}

#[test]
fn atualizacoes() {
    let _s = begin();
    send(json!({ "type": "check_updates" }));
    assert_eq!(testkit::effects(), vec!["check_updates"]);
    send(json!({ "type": "set_check_updates", "enabled": false }));
    assert!(!config::get_config_bool("checkUpdates", true));
    send(json!({ "type": "set_check_updates", "enabled": true }));
    assert!(config::get_config_bool("checkUpdates", false));
}

#[test]
fn restaurar_limpa_ocultos_e_perfil_automatico_ate_de_desconectados() {
    let _s = begin();
    config::set_config_json("hiddenFolders", Some(json!(["captures"])));
    config::set_source_hidden("mics", "Microfone que nao esta ligado", true);
    config::set_source_hidden("monitors", "2", true);
    config::set_source_auto_record("speakers", "Caixa desligada", false);
    send(json!({ "type": "reset_hidden_lists" }));
    assert!(config::get_config_json("hiddenFolders").is_none());
    assert!(!config::get_source_hidden("mics", "Microfone que nao esta ligado"));
    assert!(!config::get_source_hidden("monitors", "2"));
    assert!(config::get_source_auto_record("speakers", "Caixa desligada"));
    // A galeria e a lista de pastas da aba Arquivos são refeitas.
    assert!(testkit::last_of("root_folders").is_some());
}

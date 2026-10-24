//! Gravações na biblioteca (`bridge/library.rs`): renomear, excluir, abrir,
//! e o perfil da auto-gravação (`bridge/devices.rs`). Tocar, transcodificar e
//! ler mídia passam pelo FFmpeg (frente 2).
//!
//! Os "vídeos" aqui são arquivos de bytes quaisquer com extensão de mídia:
//! nenhum destes handlers abre o conteúdo.

use serde_json::json;

use crate::config;
use crate::lang;
use crate::testkit::{self, begin, send};

// ---- renomear ---------------------------------------------------------------

#[test]
fn renomear_mantem_a_extensao_e_responde() {
    let s = begin();
    let old = s.file("a.mkv", b"x");
    send(json!({ "type": "rename_recording", "id": old, "newName": "Reunião de segunda" }));
    let r = testkit::last_of("recording_renamed").expect("recording_renamed");
    assert_eq!(r["oldId"], old.as_str());
    assert_eq!(r["newId"], s.path("Reunião de segunda.mkv"));
    assert_eq!(r["newName"], "Reunião de segunda");
    assert!(s.exists("Reunião de segunda.mkv"));
    assert!(!s.exists("a.mkv"));
}

#[test]
fn renomear_limpa_caracteres_ilegais() {
    let s = begin();
    let old = s.file("a.mp4", b"x");
    send(json!({ "type": "rename_recording", "id": old, "newName": "  a:b/c*d?e  " }));
    assert_eq!(testkit::last_of("recording_renamed").unwrap()["newName"], "a_b_c_d_e");
    assert!(s.exists("a_b_c_d_e.mp4"));
}

#[test]
fn renomear_para_nome_existente_ganha_sufixo() {
    let s = begin();
    s.file("Aula.mkv", b"1");
    s.file("Aula (2).mkv", b"2");
    let old = s.file("x.mkv", b"3");
    send(json!({ "type": "rename_recording", "id": old, "newName": "Aula" }));
    assert_eq!(testkit::last_of("recording_renamed").unwrap()["newName"], "Aula (3)");
    assert_eq!(std::fs::read(s.path("Aula.mkv")).unwrap(), b"1", "o existente não é sobrescrito");
}

#[test]
fn renomear_para_o_mesmo_nome_nao_faz_nada() {
    let s = begin();
    let old = s.file("Igual.mkv", b"x");
    send(json!({ "type": "rename_recording", "id": old, "newName": "Igual" }));
    assert!(testkit::sent().is_empty());
    assert!(s.exists("Igual.mkv"));
}

#[test]
fn renomear_recusa_o_que_nao_deve() {
    let s = begin();
    let fora = std::env::temp_dir().join(format!("noobs-fora-{}.mkv", std::process::id()));
    std::fs::write(&fora, b"x").unwrap();
    let dentro = s.file("ok.mkv", b"x");

    // Fora da pasta de gravação: ignorado em silêncio, arquivo intacto.
    send(json!({ "type": "rename_recording", "id": fora.to_string_lossy(), "newName": "y" }));
    assert!(fora.exists());
    // Nome vazio, nome que vira vazio, id vazio.
    send(json!({ "type": "rename_recording", "id": dentro, "newName": "" }));
    send(json!({ "type": "rename_recording", "id": "", "newName": "y" }));
    assert!(testkit::sent().is_empty(), "{:?}", testkit::sent());
    // Escapar da pasta com ".." não passa.
    let escape = format!("{}\\..\\..\\x.mkv", s.rec.to_string_lossy());
    send(json!({ "type": "rename_recording", "id": escape, "newName": "y" }));
    assert!(testkit::sent_of("recording_renamed").is_empty());

    // Arquivo que não existe (dentro da pasta): avisa.
    send(json!({ "type": "rename_recording", "id": s.path("sumiu.mkv"), "newName": "y" }));
    assert_eq!(testkit::errors(), vec![lang::t("error.fileNotFound")]);
    let _ = std::fs::remove_file(fora);
}

#[test]
fn abrir_pasta_de_gravacao() {
    let s = begin();
    send(json!({ "type": "open_record_dir" }));
    assert_eq!(testkit::effects(), vec![format!("shell_open:{}", s.rec.to_string_lossy())]);
    // Pasta configurada que não existe mais: avisa, não abre nada.
    config::set_config_str("recordDir", &s.path("nao-existe"));
    send(json!({ "type": "open_record_dir" }));
    assert_eq!(testkit::effects().len(), 1);
    assert_eq!(testkit::errors(), vec![lang::t("error.folderNotFound")]);
}

// ---- excluir ----------------------------------------------------------------

#[test]
fn excluir_manda_pra_lixeira_e_avisa() {
    let s = begin();
    let f = s.file("apagar.mkv", b"x");
    send(json!({ "type": "delete_recording", "id": f }));
    let r = testkit::wait_for("recording_removed", 5000).expect("recording_removed");
    assert_eq!(r["id"], f.as_str());
    assert!(r["freeBytes"].as_i64().is_some_and(|b| b > 0));
    assert!(!s.exists("apagar.mkv"));
    assert_eq!(testkit::effects(), vec![format!("recycle:{f}")]);
}

#[test]
fn excluir_o_que_nao_pode_nao_faz_nada() {
    let s = begin();
    let fora = std::env::temp_dir().join(format!("noobs-fora-del-{}.mkv", std::process::id()));
    std::fs::write(&fora, b"x").unwrap();
    send(json!({ "type": "delete_recording", "id": fora.to_string_lossy() }));
    send(json!({ "type": "delete_recording", "id": "" }));
    // Já apagado (outro delete chegou antes): sucesso silencioso.
    send(json!({ "type": "delete_recording", "id": s.path("ja-foi.mkv") }));
    testkit::settle(300);
    assert!(fora.exists());
    assert!(testkit::effects().is_empty());
    assert!(testkit::sent().is_empty(), "{:?}", testkit::sent());
    let _ = std::fs::remove_file(fora);
}

#[test]
fn cancelar_exportacao_sem_exportacao() {
    let _s = begin();
    send(json!({ "type": "cancel_export" }));
    assert!(testkit::sent().is_empty());
}

// ---- perfil da auto-gravação ------------------------------------------------

#[test]
fn perfil_da_auto_gravacao_por_dispositivo() {
    let _s = begin();
    send(json!({ "type": "set_auto_source", "id": "NoOBS Mic - Microfone (USB)", "enabled": false }));
    assert!(!config::get_source_auto_record("mics", "Microfone (USB)"));
    send(json!({ "type": "set_auto_source", "id": "NoOBS Mic - Microfone (USB)", "enabled": true }));
    assert!(config::get_source_auto_record("mics", "Microfone (USB)"));
    send(json!({ "type": "set_auto_source", "id": "NoOBS Monitor 1", "enabled": true }));
    assert!(config::get_source_auto_record("monitors", "1"));
    // Id que não é de dispositivo: nada muda, nada quebra.
    send(json!({ "type": "set_auto_source", "id": "qualquer coisa", "enabled": true }));
    send(json!({ "type": "set_auto_source" }));
    assert!(testkit::sent().is_empty());
}

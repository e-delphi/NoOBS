//! Transcrição (`bridge/transcribe.rs`) sem rodar transcrição de verdade:
//! configuração, fila (só os caminhos de recusa — enfileirar extrai áudio pelo
//! FFmpeg), motor local, falantes, painel do player, busca e pastas
//! ocultas.

use serde_json::json;

use crate::config;
use crate::testkit::{self, begin, send};
use crate::transcribe;

// ---- configuração -----------------------------------------------------------

#[test]
fn idioma_da_transcricao() {
    let _s = begin();
    for (entrada, gravado) in [("app", "app"), ("auto", "auto"), ("pt", "pt"), (" EN ", "en"), ("yue", "yue")] {
        send(json!({ "type": "set_transcribe_language", "language": entrada }));
        assert_eq!(config::get_config_str("transcribeLanguage", ""), gravado, "{entrada:?}");
    }
    // O que vai pra API: idioma da interface em "app", nada em "auto".
    send(json!({ "type": "set_transcribe_language", "language": "app" }));
    assert_eq!(transcribe::resolve_transcribe_language(), "pt");
    send(json!({ "type": "set_transcribe_language", "language": "auto" }));
    assert_eq!(transcribe::resolve_transcribe_language(), "");

    for ruim in ["portugues", "p", "p1", "pt-br", "", "ab-c", "çã"] {
        send(json!({ "type": "set_transcribe_language", "language": ruim }));
        assert_eq!(config::get_config_str("transcribeLanguage", ""), "auto", "{ruim:?} devia ser recusado");
    }
}

#[test]
fn transcrever_ao_parar() {
    let _s = begin();
    send(json!({ "type": "set_transcribe_on_stop", "enabled": false }));
    assert!(!config::get_config_bool("transcribeOnStop", true));
    send(json!({ "type": "set_transcribe_on_stop", "enabled": true }));
    assert!(config::get_config_bool("transcribeOnStop", false));
}

// ---- motor local ------------------------------------------------------------

#[test]
fn motor_local_estado_instalar_cancelar() {
    let _s = begin();
    send(json!({ "type": "get_local_asr_state" }));
    let st = testkit::last_of("local_asr_state").unwrap();
    // A pasta temporária não tem o motor instalado.
    assert_eq!(st["status"], "missing");
    assert_eq!(st["removeRefused"], false);
    send(json!({ "type": "local_asr_install" }));
    assert_eq!(testkit::effects(), vec!["local_asr_install"]);
    send(json!({ "type": "local_asr_cancel" }));
}

#[test]
fn remover_motor_local_nao_instalado() {
    let _s = begin();
    send(json!({ "type": "local_asr_remove" }));
    // Nada a apagar: ou não responde, ou responde com o estado.
    testkit::settle(300);
    for st in testkit::sent_of("local_asr_state") {
        assert_eq!(st["status"], "missing");
    }
}

// ---- fila: estado e recusas -------------------------------------------------

#[test]
fn estado_da_fila_vazia() {
    let _s = begin();
    send(json!({ "type": "get_transcribe_state" }));
    assert!(testkit::last_of("transcribe_state").is_some());
    let p = testkit::last_of("transcribe_pending").unwrap();
    assert_eq!(p["count"], 0);
    assert_eq!(p["cloud"], 0);
    assert!(testkit::last_of("transcribe_queue").is_some());
}

#[test]
fn fila_recusa_caminhos_de_fora() {
    let _s = begin();
    for tipo in ["transcribe_recording", "remove_transcribe_item"] {
        send(json!({ "type": tipo, "id": "C:\\Windows\\notepad.exe" }));
    }
    send(json!({ "type": "move_transcribe_item", "id": "C:\\x.mkv", "to": 0 }));
    send(json!({ "type": "cancel_transcribe" }));
    assert!(testkit::sent().is_empty(), "{:?}", testkit::sent());
}

#[test]
fn tirar_da_fila_o_que_nao_esta_nela() {
    let s = begin();
    let f = s.file("a.mkv", b"x");
    send(json!({ "type": "remove_transcribe_item", "id": f }));
    send(json!({ "type": "move_transcribe_item", "id": f, "to": 3 }));
    assert!(testkit::sent_of("transcribe_pending").is_empty());
}

// ---- painel do player, falantes e busca ------------------------------------

fn write_transcript(path: &str, turns: serde_json::Value, text: &str) {
    let tp = transcribe::transcript_path(path);
    std::fs::create_dir_all(tp.parent().unwrap()).unwrap();
    std::fs::write(&tp, json!({ "turns": turns }).to_string()).unwrap();
    std::fs::write(transcribe::transcript_text_path(path), text).unwrap();
}

#[test]
fn transcricao_inexistente() {
    let s = begin();
    let f = s.file("nunca.mkv", b"x");
    send(json!({ "type": "request_transcript", "id": f }));
    let r = testkit::wait_for("transcript", 5000).unwrap();
    assert_eq!(r["transcribed"], false);
    assert_eq!(r["has"], false);
    assert_eq!(r["turns"], json!([]));
}

#[test]
fn transcricao_sem_fala_e_diferente_de_nao_transcrita() {
    let s = begin();
    let f = s.file("mudo.mkv", b"x");
    write_transcript(&f, json!([]), "");
    send(json!({ "type": "request_transcript", "id": f }));
    let r = testkit::wait_for("transcript", 5000).unwrap();
    assert_eq!(r["transcribed"], true);
    assert_eq!(r["has"], false);
}

#[test]
fn transcricao_manda_so_turnos_com_texto() {
    let s = begin();
    let f = s.file("fala.mkv", b"x");
    write_transcript(
        &f,
        json!([
            { "speaker": "SPEAKER_00", "start": 1.5, "end": 3.0, "text": "  Bom dia  " },
            { "speaker": "SPEAKER_01", "start": 3.0, "end": 4.0, "text": "   " },
            { "start": 5.0, "end": 6.0, "text": "sem falante" },
            "lixo",
            { "speaker": "SPEAKER_00", "text": "sem tempo" }
        ]),
        "Bom dia sem falante sem tempo",
    );
    send(json!({ "type": "request_transcript", "id": f }));
    let r = testkit::wait_for("transcript", 5000).unwrap();
    assert_eq!(r["has"], true);
    let turns = r["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[0], json!({ "speaker": "SPEAKER_00", "start": 1.5, "end": 3.0, "text": "Bom dia" }));
    assert_eq!(turns[1]["speaker"], "");
    assert_eq!(turns[2]["start"], 0.0);
}

#[test]
fn transcricao_json_corrompido_vira_vazia() {
    let s = begin();
    let f = s.file("ruim.mkv", b"x");
    let tp = transcribe::transcript_path(&f);
    std::fs::create_dir_all(tp.parent().unwrap()).unwrap();
    std::fs::write(&tp, "{ isto não é json").unwrap();
    send(json!({ "type": "request_transcript", "id": f }));
    let r = testkit::wait_for("transcript", 5000).unwrap();
    assert_eq!(r["transcribed"], true);
    assert_eq!(r["turns"], json!([]));
}

#[test]
fn transcricao_de_fora_nao_responde() {
    let _s = begin();
    send(json!({ "type": "request_transcript", "id": "C:\\Windows\\x.mkv" }));
    testkit::settle(200);
    assert!(testkit::sent().is_empty());
}

#[test]
fn nomes_de_falante() {
    let s = begin();
    let f = s.file("conversa.mkv", b"x");
    let nomes = || {
        testkit::clear_sent();
        send(json!({ "type": "request_transcript", "id": f }));
        testkit::wait_for("transcript", 5000).unwrap()["speakers"].clone()
    };
    send(json!({ "type": "set_speaker_name", "id": f, "speaker": "SPEAKER_00", "name": "  Eduardo  " }));
    send(json!({ "type": "set_speaker_name", "id": f, "speaker": "SPEAKER_01", "name": "Ana" }));
    assert_eq!(nomes(), json!({ "SPEAKER_00": "Eduardo", "SPEAKER_01": "Ana" }));
    // Nome vazio volta ao rótulo padrão (sai do mapa).
    send(json!({ "type": "set_speaker_name", "id": f, "speaker": "SPEAKER_01", "name": "" }));
    assert_eq!(nomes(), json!({ "SPEAKER_00": "Eduardo" }));
    // Até 60 caracteres.
    let longo = "x".repeat(100);
    send(json!({ "type": "set_speaker_name", "id": f, "speaker": "SPEAKER_00", "name": longo }));
    assert_eq!(nomes()["SPEAKER_00"].as_str().unwrap().chars().count(), 60);
    // Falante vazio ou gravação de fora: ignorado.
    send(json!({ "type": "set_speaker_name", "id": f, "speaker": " ", "name": "X" }));
    send(json!({ "type": "set_speaker_name", "id": "C:\\x.mkv", "speaker": "SPEAKER_00", "name": "X" }));
    assert_eq!(nomes().as_object().unwrap().len(), 1);
}

#[test]
fn busca_no_texto_das_transcricoes() {
    let s = begin();
    let a = s.file("a.mkv", b"x");
    let b = s.file("b.mkv", b"x");
    let _c = s.file("c.mkv", b"x");
    write_transcript(&a, json!([]), "Reunião sobre o ORÇAMENTO de 2026");
    write_transcript(&b, json!([]), "conversa sobre férias");
    let busca = |q: &str| {
        testkit::clear_sent();
        send(json!({ "type": "search_transcripts", "query": q }));
        let r = testkit::wait_for("transcript_search", 5000).unwrap();
        let mut ids: Vec<String> = r["ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect();
        ids.sort();
        ids
    };
    assert_eq!(busca("orçamento"), vec![a.clone()]);
    assert_eq!(busca("  SOBRE "), vec![a.clone(), b.clone()]);
    assert_eq!(busca("inexistente"), Vec::<String>::new());
    // Vazio não varre nada.
    assert_eq!(busca("   "), Vec::<String>::new());
}

// ---- pastas ocultas ---------------------------------------------------------

#[test]
fn pastas_da_raiz_com_contagem() {
    let s = begin();
    s.file("Aulas\\a.mkv", b"x");
    s.file("Aulas\\sub\\b.mp4", b"x");
    s.dir("Vazia");
    send(json!({ "type": "get_root_folders" }));
    let mut f: Vec<_> = testkit::last_of("root_folders").unwrap()["folders"].as_array().unwrap().clone();
    f.sort_by_key(|v| v["name"].as_str().unwrap().to_owned());
    assert_eq!(f[0], json!({ "name": "Aulas", "hidden": false, "count": 2 }));
    assert_eq!(f[1], json!({ "name": "Vazia", "hidden": false, "count": 0 }));
}

#[test]
fn ocultar_e_mostrar_pasta() {
    let s = begin();
    s.file("Captures\\x.mp4", b"x");
    send(json!({ "type": "set_folder_hidden", "name": "Captures", "hidden": true }));
    assert_eq!(config::get_config_json("hiddenFolders").unwrap(), json!(["captures"]));
    // Uma ida manda as três listas.
    for t in ["recordings_loaded", "transcribe_pending", "root_folders"] {
        assert!(testkit::last_of(t).is_some(), "{t}");
    }
    let rf = testkit::last_of("root_folders").unwrap();
    assert_eq!(rf["folders"][0]["hidden"], true);
    // Oculta não aparece nos cards da raiz.
    assert!(testkit::last_of("recordings_loaded").unwrap()["folders"].as_array().unwrap().is_empty());

    // Mesmo nome com outra caixa não duplica.
    send(json!({ "type": "set_folder_hidden", "name": "CAPTURES", "hidden": true }));
    assert_eq!(config::get_config_json("hiddenFolders").unwrap(), json!(["captures"]));
    send(json!({ "type": "set_folder_hidden", "name": "captures", "hidden": false }));
    assert_eq!(config::get_config_json("hiddenFolders").unwrap(), json!([]));
}

#[test]
fn pasta_oculta_que_sumiu_continua_listada() {
    let _s = begin();
    send(json!({ "type": "set_folder_hidden", "name": "NVIDIA", "hidden": true }));
    let rf = testkit::last_of("root_folders").unwrap();
    assert_eq!(rf["folders"], json!([{ "name": "nvidia", "hidden": true, "missing": true }]));
}

#[test]
fn ocultar_pasta_com_nome_invalido() {
    let _s = begin();
    for nome in ["", "   ", "a\\b", "..\\x"] {
        send(json!({ "type": "set_folder_hidden", "name": nome, "hidden": true }));
    }
    assert!(config::get_config_json("hiddenFolders").is_none());
    assert!(testkit::sent().is_empty());
}

#[test]
fn chaves_do_servidor_antigo_saem_do_config() {
    let _s = begin();
    config::set_config_str("transcribeHost", "http://srv:8000");
    config::set_config_str("transcribeEngine", "server");
    super::super::transcribe::do_init();
    assert_eq!(config::get_config_str("transcribeHost", "-"), "-");
    assert_eq!(config::get_config_str("transcribeEngine", "-"), "-");
    // O estado do motor local não fala mais de "engine".
    send(json!({ "type": "get_local_asr_state" }));
    assert!(testkit::last_of("local_asr_state").unwrap().get("engine").is_none());
    // As mensagens do servidor não existem mais (caem no "não portado", sem resposta).
    testkit::clear_sent();
    for ty in ["set_transcribe_host", "test_transcribe_host", "check_transcribe_setup", "transcribe_setup_action", "set_transcribe_engine"] {
        send(json!({ "type": ty, "host": "http://127.0.0.1:1", "engine": "server", "action": "openDockerSite" }));
    }
    assert!(testkit::sent().is_empty());
    assert!(testkit::effects().is_empty());
}

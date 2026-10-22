//! Frente 4 (parte FFmpeg): as mensagens da página que chegam no FFmpeg,
//! mandadas pelo `dispatch` com as amostras versionadas copiadas pra pasta de
//! gravação da sessão. Confere a resposta, o que ficou no disco e — pras URLs
//! do player — baixa o arquivo de verdade pelo servidor HTTP local.
//!
//! A transcrição vai contra um motor FALSO (`transcribe::test_engine`, só no
//! `cargo test`): é o único jeito de passar pela extração de áudio, pela escolha
//! das faixas e pela montagem dos turnos sem os 3,7 GB do motor local.

use std::sync::Mutex;

use serde_json::{json, Value};

use crate::config;
use crate::lang;
use crate::testkit::{self, begin, send};

fn ids(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

// ---- player -----------------------------------------------------------------

#[test]
fn tocar_serve_o_arquivo_original() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "tocar.mkv");
    send(json!({ "type": "play_recording", "id": f }));
    let r = testkit::last_of("play_url").expect("play_url");
    assert_eq!(r["mode"], "direct");
    assert_eq!(r["name"], "tocar");
    assert!(r["startClockSec"].as_i64().is_some_and(|c| (0..86_400).contains(&c)));
    let url = r["url"].as_str().unwrap();
    let (st, body) = testkit::http_get(url, None);
    assert_eq!(st, 200);
    assert_eq!(body, std::fs::read(&f).unwrap(), "o arquivo inteiro, byte a byte");
    // Pedaço (o <video> faz seek por Range).
    let (st, body) = testkit::http_get(url, Some("bytes=100-199"));
    assert_eq!(st, 206);
    assert_eq!(body, std::fs::read(&f).unwrap()[100..200].to_vec());
}

#[test]
fn tocar_recusa_e_avisa() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    send(json!({ "type": "play_recording", "id": "C:\\Windows\\notepad.exe" }));
    assert!(testkit::sent_of("play_url").is_empty() && testkit::errors().is_empty());
    send(json!({ "type": "play_recording", "id": s.path("sumiu.mkv") }));
    assert_eq!(testkit::errors(), vec![lang::t("error.fileNotFound")]);
}

#[test]
fn transcodificar_entrega_mp4() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "remux.mkv");
    send(json!({ "type": "request_transcode", "id": f }));
    assert!(testkit::last_of("play_pending").is_some(), "avisa que vai demorar");
    let r = testkit::wait_for("play_url", 15_000).expect("play_url");
    assert_eq!(r["mode"], "transcoded");
    let (st, body) = testkit::http_get(r["url"].as_str().unwrap(), Some("bytes=0-63"));
    assert_eq!(st, 206);
    assert_eq!(&body[4..8], b"ftyp", "é MP4");
    // De novo: sai do cache, mesma URL.
    testkit::clear_sent();
    send(json!({ "type": "request_transcode", "id": f }));
    assert_eq!(testkit::wait_for("play_url", 15_000).unwrap()["url"], r["url"]);
}

#[test]
fn transcodificar_o_que_nao_e_video_avisa() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.file("falso.mkv", "isto não é vídeo".repeat(100).as_bytes());
    send(json!({ "type": "request_transcode", "id": f }));
    assert!(testkit::wait_for("error", 15_000).is_some());
    assert_eq!(testkit::errors(), vec![lang::t("error.transcodeFailed")]);
}

// ---- painel de informações --------------------------------------------------

#[test]
fn informacoes_do_video() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_6faixas.mkv", "info.mkv");
    send(json!({ "type": "request_video_info", "id": f }));
    let r = testkit::wait_for("video_info", 15_000).expect("video_info");
    assert_eq!(r["id"], f.as_str());
    assert_eq!(r["fileName"], "info.mkv");
    assert!((r["duration"].as_f64().unwrap() - 3.0).abs() < 0.15);
    let streams = r["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 7);
    assert_eq!(streams[0]["kind"], "video");
    assert_eq!(streams[0]["codec"], "h264");
    assert_eq!((streams[0]["width"].as_i64(), streams[0]["height"].as_i64()), (Some(320), Some(180)));
    assert!((streams[0]["frameRate"].as_f64().unwrap() - 30.0).abs() < 0.01);
    let titulos: Vec<_> = streams[1..].iter().map(|s| s["title"].as_str().unwrap().to_owned()).collect();
    assert_eq!(titulos, ["Mix", "Microfone (USB)", "Alto-falantes", "Headset", "Webcam", "Linha"]);
    assert_eq!(streams[1]["sampleRate"], 48_000);

    // Segunda vez: do cache (`videoInfo` no <hash>.json), mesma resposta.
    testkit::clear_sent();
    send(json!({ "type": "request_video_info", "id": f }));
    let r2 = testkit::wait_for("video_info", 5000).unwrap();
    assert_eq!(r2["streams"], r["streams"]);
}

#[test]
fn informacoes_apos_renomear_trazem_o_id_novo() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_sem_bframes.mkv", "antes.mkv");
    send(json!({ "type": "request_video_info", "id": f }));
    testkit::wait_for("video_info", 15_000).unwrap();
    send(json!({ "type": "rename_recording", "id": f, "newName": "depois" }));
    let novo = s.path("depois.mkv");
    testkit::clear_sent();
    send(json!({ "type": "request_video_info", "id": novo }));
    let r = testkit::wait_for("video_info", 5000).unwrap();
    // O cache migrou com o rename e responde com o caminho NOVO (senão a UI
    // re-pediria em laço).
    assert_eq!(r["id"], novo.as_str());
    assert_eq!(r["fileName"], "depois.mkv");
}

#[test]
fn informacoes_de_arquivo_ruim() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.file("ruim.mkv", "x".repeat(2000).as_bytes());
    send(json!({ "type": "request_video_info", "id": f }));
    testkit::wait_for("error", 15_000).expect("erro");
    assert_eq!(testkit::errors(), vec![lang::t("error.probeFailed")]);
    send(json!({ "type": "request_video_info", "id": "C:\\fora.mkv" }));
    testkit::settle(200);
    assert_eq!(testkit::errors().len(), 1);
}

// ---- waveform e keyframes ---------------------------------------------------

#[test]
fn waveform_com_limites() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "onda.mkv");
    for (pedido, hi, esperado) in [(0, false, 100), (-5, false, 100), (50, false, 50), (999_999, false, 20_000), (30_000, true, 30_000), (999_999, true, 150_000)] {
        testkit::clear_sent();
        send(json!({ "type": "request_waveform", "id": f, "buckets": pedido, "hi": hi }));
        let r = testkit::wait_for("waveform_ready", 20_000).unwrap_or_else(|| panic!("{pedido} {hi}"));
        assert_eq!(r["buckets"], esperado, "{pedido} hi={hi}");
        assert_eq!(r["hi"], hi);
        let p = r["peaks"].as_array().unwrap();
        assert_eq!(p.len(), esperado as usize);
        assert!(p.iter().all(|v| (0.0..=1.0).contains(&v.as_f64().unwrap())));
    }
}

#[test]
fn waveform_sem_arquivo_nao_responde() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    send(json!({ "type": "request_waveform", "id": s.path("nada.mkv"), "buckets": 10 }));
    testkit::settle(300);
    assert!(testkit::sent_of("waveform_ready").is_empty() && testkit::errors().is_empty(), "sem aviso: a waveform é opcional");
}

#[test]
fn keyframes_do_player() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "kf.mkv");
    send(json!({ "type": "request_keyframes", "id": f }));
    let r = testkit::wait_for("keyframes", 10_000).unwrap();
    assert_eq!(r["times"], json!([0.0, 1.0, 2.0, 3.0]));
    assert_eq!(r["fps"], 30.0);
    let g = s.sample("interrompido.mkv", "kf-interrompido.mkv");
    testkit::clear_sent();
    send(json!({ "type": "request_keyframes", "id": g }));
    assert_eq!(testkit::wait_for("keyframes", 10_000).unwrap()["times"], json!([0.0, 1.0]));
}

// ---- faixas de áudio do player ----------------------------------------------

#[test]
fn faixas_isoladas_viram_urls() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_6faixas.mkv", "faixas.mkv");
    send(json!({ "type": "request_audio_tracks", "id": f }));
    let r = testkit::wait_for("audio_tracks_ready", 20_000).expect("audio_tracks_ready");
    let urls = ids(&r["urls"]);
    assert_eq!(urls.len(), 6);
    // A mistura (0) toca no <video>: não é extraída.
    assert_eq!(urls[0], "");
    for u in &urls[1..] {
        let (st, body) = testkit::http_get(u, Some("bytes=0-31"));
        assert_eq!(st, 206, "{u}");
        assert_eq!(&body[4..8], b"ftyp", "M4A");
    }
}

// ---- prévia e duração da biblioteca -----------------------------------------

#[test]
fn abrir_pasta_gera_previa_e_duracao() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let pasta = s.dir("Aulas");
    let f = s.sample("h264_2faixas.mkv", "Aulas\\aula.mkv");
    send(json!({ "type": "open_folder", "id": pasta }));
    let r = testkit::wait_for("recording_meta", 20_000).expect("recording_meta");
    assert_eq!(r["id"], f.as_str());
    assert_eq!(r["duration"], 4);
    let (st, body) = testkit::http_get(r["thumb"].as_str().unwrap(), None);
    assert_eq!(st, 200);
    assert_eq!(&body[..2], b"\xFF\xD8", "miniatura JPEG");
}

#[test]
fn modo_da_previa_desligado_nao_gera() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    s.sample("h264_sem_bframes.mkv", "x.mkv");
    send(json!({ "type": "set_library_thumbs", "mode": "off" }));
    testkit::settle(1500);
    assert!(testkit::sent_of("recording_meta").is_empty());
    assert_eq!(config::get_config_str("libraryThumbs", ""), "off");
    send(json!({ "type": "set_library_thumbs", "mode": "always" }));
    assert!(testkit::wait_for("recording_meta", 20_000).is_some());
    // Valor desconhecido: auto.
    send(json!({ "type": "set_library_thumbs", "mode": "qualquer" }));
    assert_eq!(config::get_config_str("libraryThumbs", ""), "auto");
}

#[test]
fn trocar_pasta_de_gravacao_lista_e_escaneia() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let nova = s.dir("OutraPasta");
    s.sample("uniao_a.mkv", "OutraPasta\\v.mkv");
    send(json!({ "type": "set_record_dir", "path": nova }));
    assert_eq!(config::get_config_str("recordDir", ""), nova);
    assert_eq!(testkit::last_of("settings").unwrap()["recordDir"], nova.as_str());
    let l = testkit::last_of("recordings_loaded").unwrap();
    assert_eq!(l["recordings"].as_array().unwrap().len(), 1);
    assert!(testkit::wait_for("recording_meta", 20_000).is_some());
    // Pasta que não existe: avisa e mantém a atual.
    send(json!({ "type": "set_record_dir", "path": s.path("nao-existe") }));
    assert_eq!(testkit::errors(), vec![lang::t("error.folderNotFound")]);
    assert_eq!(config::get_config_str("recordDir", ""), nova);
}

// ---- exportação -------------------------------------------------------------

fn exportar(msg: Value, timeout_ms: u64) -> Value {
    send(msg);
    testkit::wait_for("export_done", timeout_ms).expect("export_done")
}

#[test]
fn exportar_como_a_pagina_manda() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "Aulas\\origem.mkv");
    let r = exportar(
        json!({
            "type": "export_recording", "id": f, "name": "Recorte da aula",
            "segments": [{ "startMs": 0, "endMs": 1000 }, { "startMs": 2000, "endMs": 3500 }],
            "encoder": "h264-sw", "crf": 30, "container": "mp4",
            "audioStreams": [1, 2], "originalAudio": true, "targetHeight": 90,
        }),
        60_000,
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["sourceId"], f.as_str());
    // Na MESMA pasta da origem, com o nome pedido.
    let out = r["path"].as_str().unwrap().to_owned();
    assert_eq!(out, s.path("Aulas\\Recorte da aula.mp4"));
    let rep = crate::probe::probe(std::path::Path::new(&out)).unwrap();
    assert!((rep.duration - 2.5).abs() < 0.2, "{}", rep.duration);
    assert_eq!(rep.video_stream().unwrap().height, 90);
    assert_eq!(rep.audio_track_count(), 2);
    assert!(!testkit::sent_of("export_progress").is_empty(), "progresso ao longo do caminho");
    // Nenhum .part sobrando.
    assert!(std::fs::read_dir(s.rec.join("Aulas")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")));
}

#[test]
fn previa_da_exportacao_fica_fora_da_biblioteca_e_toca_no_player() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "origem.mkv");
    let pedir = |from_ms: i64| {
        testkit::clear_sent();
        send(json!({
            "type": "export_recording", "id": f, "name": "Recorte",
            "segments": [{ "startMs": 0, "endMs": 4000 }],
            "encoder": "h264-sw", "crf": 30, "container": "mp4", "audioStreams": [1], "mixTrackIndex": 1,
            "preview": { "fromMs": from_ms, "sec": 5 },
        }));
        testkit::wait_for("export_preview_done", 60_000).expect("export_preview_done")
    };
    // Do cursor (1 s) até o fim da gravação de 4 s: 3 s, menos que os 5 pedidos.
    let r = pedir(1000);
    assert_eq!(r["ok"], true, "{r}");
    let out = r["path"].as_str().unwrap().to_owned();
    assert!(!out.to_lowercase().starts_with(&s.rec.to_string_lossy().to_lowercase()), "fora da pasta de gravação: {out}");
    let rep = crate::probe::probe(std::path::Path::new(&out)).unwrap();
    assert!((rep.duration - 3.0).abs() < 0.3, "{}", rep.duration);
    // Prévia não é exportação: sem card, sem barra no card da origem.
    assert!(testkit::sent_of("export_done").is_empty());
    assert!(testkit::sent_of("recording_added").is_empty());
    assert!(testkit::sent_of("export_progress").is_empty());
    assert!(!testkit::sent_of("export_preview_progress").is_empty());
    // O player da galeria toca.
    testkit::clear_sent();
    send(json!({ "type": "play_recording", "id": out }));
    assert_eq!(testkit::last_of("play_url").expect("play_url")["id"], out.as_str());
    // Mexer no arquivo, não: renomear/excluir são só da biblioteca.
    send(json!({ "type": "delete_recording", "id": out }));
    assert!(std::path::Path::new(&out).exists());
    // Prévia nova substitui a anterior.
    let r2 = pedir(0);
    assert_eq!(r2["ok"], true, "{r2}");
    let out2 = r2["path"].as_str().unwrap().to_owned();
    let n = std::fs::read_dir(std::path::Path::new(&out2).parent().unwrap()).unwrap().count();
    assert_eq!(n, 1, "só a prévia mais recente fica");
}

#[test]
fn exportar_so_audio_sai_mp3_e_nome_repetido_ganha_sufixo() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "a.mkv");
    for esperado in ["Som.mp3", "Som (2).mp3"] {
        testkit::clear_sent();
        let r = exportar(
            json!({ "type": "export_recording", "id": f, "name": "Som", "noVideo": true,
                    "segments": [{ "startMs": 0, "endMs": 4000 }], "audioStreams": [1], "mixTrackIndex": 1 }),
            60_000,
        );
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["path"], s.path(esperado));
    }
}

#[test]
fn exportar_recusa_pedido_invalido() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_2faixas.mkv", "b.mkv");
    for (msg, erro) in [
        (json!({ "type": "export_recording", "id": f, "segments": [] }), "error.exportBadRange"),
        (json!({ "type": "export_recording", "id": f, "segments": [{ "startMs": 3000, "endMs": 1000 }] }), "error.exportBadRange"),
        (json!({ "type": "export_recording", "id": f, "noVideo": true, "audioStreams": [], "segments": [{ "startMs": 0, "endMs": 1000 }] }), "error.exportNothing"),
        (json!({ "type": "export_recording", "id": "C:\\fora.mkv", "segments": [{ "startMs": 0, "endMs": 1000 }] }), "error.fileNotFound"),
        (json!({ "type": "export_recording", "id": s.path("sumiu.mkv"), "segments": [{ "startMs": 0, "endMs": 1000 }] }), "error.fileNotFound"),
    ] {
        testkit::clear_sent();
        send(msg);
        let done = testkit::last_of("export_done").expect("export_done");
        assert_eq!(done["ok"], false);
        assert_eq!(testkit::errors(), vec![lang::t(erro)], "{erro}");
    }
}

#[test]
fn cancelar_exportacao_em_curso() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let f = s.sample("h264_6faixas.mkv", "longa.mkv");
    send(json!({ "type": "export_recording", "id": f, "name": "cancelada", "encoder": "av1-sw", "crf": 0,
                 "segments": [{ "startMs": 0, "endMs": 3000 }], "audioStreams": [1] }));
    // Uma segunda enquanto a primeira roda: recusada.
    send(json!({ "type": "export_recording", "id": f, "segments": [{ "startMs": 0, "endMs": 1000 }] }));
    assert!(testkit::errors().contains(&lang::t("error.exportBusy")));
    send(json!({ "type": "cancel_export" }));
    let r = testkit::wait_for("export_done", 60_000).unwrap();
    // Cancelada (ou, se terminou antes do pedido chegar, concluída — nunca falha).
    assert!(r["canceled"] == true || r["ok"] == true, "{r}");
    if r["canceled"] == true {
        assert!(!s.exists("cancelada.mp4"));
    }
}

// ---- união ------------------------------------------------------------------

#[test]
fn unir_gravacoes_pela_pagina() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let a = s.sample("uniao_a.mkv", "parte1.mkv");
    let b = s.sample("uniao_b.mkv", "parte2.mkv");
    send(json!({ "type": "merge_recordings", "ids": [a, b] }));
    assert!(testkit::last_of("merge_pending").is_some());
    let r = testkit::wait_for("merge_done", 30_000).expect("merge_done");
    assert_eq!(r["ok"], true, "{:?}", testkit::errors());
    // O " - U.mkv" na raiz; as partes foram pra Lixeira (no teste: apagadas).
    let unida: Vec<_> = std::fs::read_dir(&s.rec).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(" - U.mkv")).collect();
    assert_eq!(unida.len(), 1, "{unida:?}");
    assert!(!s.exists("parte1.mkv") && !s.exists("parte2.mkv"));
    let rep = crate::probe::probe(&s.rec.join(&unida[0])).unwrap();
    assert!((rep.duration - 4.0).abs() < 0.2);
}

#[test]
fn unir_incompativeis_avisa_e_mantem_as_partes() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let a = s.sample("h264_2faixas.mkv", "x1.mkv");
    let b = s.sample("av1.mkv", "x2.mkv");
    send(json!({ "type": "merge_recordings", "ids": [a, b] }));
    let r = testkit::wait_for("merge_done", 30_000).unwrap();
    assert_eq!(r["ok"], false);
    assert_eq!(testkit::errors(), vec![lang::t("error.mergeIncompatible")]);
    assert!(s.exists("x1.mkv") && s.exists("x2.mkv"));
    // Uma só: nada acontece.
    testkit::clear_sent();
    send(json!({ "type": "merge_recordings", "ids": [s.path("x1.mkv")] }));
    assert!(testkit::sent_of("merge_pending").is_empty() && testkit::sent_of("merge_done").is_empty());
}

// ---- transcrição contra um motor falso --------------------------------------

/// O que o motor falso recebeu: os bytes de cada áudio, na ordem.
static CALLS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Pronto na hora, com palavras DIFERENTES por chamada — palavras iguais no
/// mesmo instante seriam tiradas como eco.
fn fake_engine(audio: &str) -> crate::local_asr::Outcome {
    let n = {
        let mut c = CALLS.lock().unwrap();
        c.push(std::fs::read(audio).unwrap_or_default());
        c.len()
    };
    let (w1, w2) = (format!("palavra{n}a"), format!("palavra{n}b"));
    let t0 = 0.3 + n as f64 * 0.1;
    let words = json!([
        { "word": w1, "start": t0, "end": t0 + 0.4, "score": 0.9 },
        { "word": w2, "start": t0 + 0.5, "end": t0 + 0.9, "score": 0.9 }
    ]);
    let text = format!("{w1} {w2}");
    let result = json!({
        "language": "pt",
        "text": text,
        "turns": [{ "speaker": null, "start": t0, "end": t0 + 0.9, "text": text }],
        "segments": [{ "start": t0, "end": t0 + 0.9, "text": text, "speaker": null, "words": words }]
    });
    crate::local_asr::Outcome::Done(result.to_string())
}

/// Liga o motor falso enquanto viver.
struct FakeEngine;

fn fake_engine_on() -> FakeEngine {
    CALLS.lock().unwrap().clear();
    crate::transcribe::test_engine::set(Some(fake_engine));
    FakeEngine
}

impl Drop for FakeEngine {
    fn drop(&mut self) {
        crate::transcribe::test_engine::set(None);
    }
}

fn calls() -> Vec<Vec<u8>> {
    CALLS.lock().unwrap().clone()
}

fn transcrita(path: &str) -> bool {
    crate::transcribe::has_transcript(path)
}

#[test]
fn transcrever_pela_mistura() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let _engine = fake_engine_on();
    super::super::transcribe::do_init();
    let f = s.sample("h264_2faixas.mkv", "conversa.mkv");
    send(json!({ "type": "transcribe_recording", "id": f }));
    let ok = (0..300).any(|_| {
        testkit::settle(100);
        transcrita(&f)
    });
    assert!(ok, "não transcreveu em 30 s; chamadas={} estado={:?}", calls().len(), testkit::last_of("transcribe_state"));
    // Mistura + 1 isolada: vai SÓ a mistura (uma isolada repetiria o texto).
    let ups = calls();
    assert_eq!(ups.len(), 1);
    // O que o motor recebeu é áudio AAC de verdade, com a duração da gravação.
    let m4a = testkit::sandbox_root().join("audio-mistura.m4a");
    std::fs::write(&m4a, &ups[0]).unwrap();
    let rep = crate::probe::probe(&m4a).expect("audio legível");
    assert!(rep.video_stream().is_none());
    assert_eq!(rep.audio_track_count(), 1);
    assert!((rep.duration - 4.0).abs() < 0.2, "{}", rep.duration);
    // O painel do player lê os turnos do cache.
    testkit::clear_sent();
    send(json!({ "type": "request_transcript", "id": f }));
    let t = testkit::wait_for("transcript", 5000).unwrap();
    assert_eq!(t["has"], true);
    assert!(t["turns"][0]["text"].as_str().unwrap().contains("palavra1a"), "{t}");
    // E a busca acha.
    send(json!({ "type": "search_transcripts", "query": "PALAVRA1B" }));
    assert_eq!(ids(&testkit::wait_for("transcript_search", 5000).unwrap()["ids"]), vec![f.clone()]);
    send(json!({ "type": "cancel_transcribe" }));
}

#[test]
fn transcrever_por_faixa_pula_a_muda_e_nomeia_pelo_dispositivo() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let _engine = fake_engine_on();
    super::super::transcribe::do_init();
    let f = s.sample("h264_6faixas.mkv", "reuniao.mkv");
    send(json!({ "type": "transcribe_recording", "id": f }));
    let ok = (0..600).any(|_| {
        testkit::settle(100);
        transcrita(&f)
    });
    assert!(ok, "não transcreveu em 60 s");
    // 5 isoladas, uma muda ("Alto-falantes"): vão 4 pro motor.
    assert_eq!(calls().len(), 4);
    send(json!({ "type": "request_transcript", "id": f }));
    let t = testkit::wait_for("transcript", 5000).unwrap();
    let falantes: std::collections::BTreeSet<String> = t["turns"].as_array().unwrap().iter().map(|x| x["speaker"].as_str().unwrap().to_owned()).collect();
    assert_eq!(
        falantes.into_iter().collect::<Vec<_>>(),
        ["Headset", "Linha", "Microfone (USB)", "Webcam"],
        "falante = nome do dispositivo da faixa"
    );
    send(json!({ "type": "cancel_transcribe" }));
}

#[test]
fn transcrever_pendentes_e_estado_da_fila() {
    let s = begin();
    if !testkit::media() {
        return;
    }
    let _engine = fake_engine_on();
    super::super::transcribe::do_init();
    let a = s.sample("uniao_a.mkv", "p1.mkv");
    let b = s.sample("uniao_b.mkv", "Pasta\\p2.mkv");
    send(json!({ "type": "get_transcribe_state" }));
    assert_eq!(testkit::last_of("transcribe_pending").unwrap()["count"], 2, "a árvore inteira conta");
    send(json!({ "type": "transcribe_pending" }));
    let ok = (0..400).any(|_| {
        testkit::settle(100);
        transcrita(&a) && transcrita(&b)
    });
    assert!(ok, "não transcreveu as pendentes");
    testkit::clear_sent();
    send(json!({ "type": "get_transcribe_state" }));
    assert_eq!(testkit::last_of("transcribe_pending").unwrap()["count"], 0);
    send(json!({ "type": "cancel_transcribe" }));
}

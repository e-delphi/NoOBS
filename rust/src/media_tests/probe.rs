//! `probe::probe`: o que o painel de informações e a biblioteca leem de cada
//! arquivo.

use super::amostras::path;
use super::need_dlls;
use super::util::{copy_as, near, probe};
use crate::probe::probe as raw_probe;

#[test]
fn gravacao_comum_h264_duas_faixas() {
    need_dlls!();
    let f = path("h264_2faixas.mkv");
    let r = probe(&f);
    assert!(r.format.contains("matroska"), "{}", r.format);
    near(r.duration, 4.0, 0.1, "duração");
    assert_eq!(r.size, std::fs::metadata(&f).unwrap().len());
    assert!(r.bit_rate > 0);
    let v = r.video_stream().expect("vídeo");
    assert_eq!((v.codec.as_str(), v.width, v.height), ("h264", 320, 180));
    near(v.frame_rate, 30.0, 0.01, "fps");
    let audio: Vec<_> = r.audio_streams().collect();
    assert_eq!(audio.len(), 2);
    assert_eq!(audio[0].title, "Mix");
    assert_eq!(audio[1].title, "Microfone (USB)");
    for a in audio {
        assert_eq!((a.codec.as_str(), a.channels, a.sample_rate), ("aac", 2, 48_000));
    }
}

#[test]
fn seis_faixas_com_titulos() {
    need_dlls!();
    let r = probe(&path("h264_6faixas.mkv"));
    assert_eq!(r.audio_track_count(), 6);
    let titulos: Vec<_> = r.audio_streams().map(|s| s.title.clone()).collect();
    assert_eq!(titulos, ["Mix", "Microfone (USB)", "Alto-falantes", "Headset", "Webcam", "Linha"]);
    near(r.duration, 3.0, 0.1, "duração");
}

#[test]
fn cada_codec_de_video() {
    need_dlls!();
    for (nome, codec, w, h, dur) in [
        ("h264_sem_bframes.mkv", "h264", 320, 180, 2.0),
        ("av1.mkv", "av1", 320, 180, 2.0),
        ("hevc.mkv", "hevc", 640, 360, 2.0),
    ] {
        let f = path(nome);
        if !std::path::Path::new(&f).exists() {
            eprintln!("{nome} não existe (placa sem esse encoder quando as amostras foram geradas)");
            continue;
        }
        let r = probe(&f);
        let v = r.video_stream().unwrap_or_else(|| panic!("{nome} sem vídeo"));
        assert_eq!((v.codec.as_str(), v.width, v.height), (codec, w, h), "{nome}");
        near(r.duration, dur, 0.1, nome);
    }
}

#[test]
fn mp4() {
    need_dlls!();
    let r = probe(&path("h264.mp4"));
    assert!(r.format.contains("mp4"), "{}", r.format);
    near(r.duration, 4.0, 0.1, "duração");
    assert_eq!(r.audio_track_count(), 2);
    near(r.video_stream().unwrap().frame_rate, 30.0, 0.01, "fps");
}

#[test]
fn interrompido_sem_duracao_no_cabecalho_varre_os_pacotes() {
    need_dlls!();
    // Sem trailer: o cabeçalho não tem a duração e o último cluster se perdeu.
    // A duração sai da varredura dos pacotes (#27): menos que os 2,4 s
    // escritos, mais que zero.
    let r = probe(&path("interrompido.mkv"));
    assert!(r.duration > 1.0 && r.duration <= 2.4, "{}", r.duration);
    assert!(r.bit_rate > 0, "bitrate pela conta tamanho/duração");
    assert!(r.video_stream().is_some());
}

#[test]
fn caminho_com_acento_e_espaco() {
    need_dlls!();
    // UTF-8 até o FFmpeg (#29): cp1252 quebraria o "ç" e o "ã".
    let f = copy_as(&path("h264_sem_bframes.mkv"), "gravação de reunião ç.mkv");
    near(probe(&f).duration, 2.0, 0.1, "duração");
}

#[test]
fn o_que_nao_e_video_devolve_nada() {
    need_dlls!();
    let txt = super::out("texto.mkv");
    std::fs::write(&txt, "isto não é um vídeo".repeat(100)).unwrap();
    let vazio = super::out("vazio.mkv");
    std::fs::write(&vazio, b"").unwrap();
    for p in [txt.as_str(), vazio.as_str(), "C:\\nao\\existe.mkv", ""] {
        assert!(raw_probe(std::path::Path::new(p)).is_none(), "{p:?}");
    }
}

#[test]
fn arquivo_truncado_no_meio_ainda_le() {
    need_dlls!();
    // Copiado pela metade (download interrompido, disco cheio).
    let bytes = std::fs::read(path("h264_2faixas.mkv")).unwrap();
    let f = super::out("metade.mkv");
    std::fs::write(&f, &bytes[..bytes.len() / 2]).unwrap();
    let r = probe(&f);
    assert!(r.video_stream().is_some());
    assert!(r.duration > 0.0);
}

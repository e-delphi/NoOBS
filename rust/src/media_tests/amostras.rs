//! As amostras versionadas em `tests/fixtures/media/` e o teste (ignorado) que
//! as gera. Pequenas de propósito: 320×180 a 30 fps, poucos segundos, AAC a
//! 32 kbps.
//!
//! | Arquivo | O que imita |
//! |---|---|
//! | `h264_2faixas.mkv` | gravação comum: H.264 com B-frames (x264, #50), mistura + 1 isolada, 4 s |
//! | `h264_6faixas.mkv` | teto de faixas do OBS: mistura + 5 isoladas (uma muda), 3 s |
//! | `h264_sem_bframes.mkv` | encoder sem B-frames (AMF), 2 s |
//! | `av1.mkv` | AV1 (codec padrão em placa nova), 2 s |
//! | `hevc.mkv` | HEVC 640×360 (o encoder da placa recusa menos), só se a placa codifica HEVC, 2 s |
//! | `h264.mp4` | a mesma gravação comum, em MP4 (o que a exportação produz) |
//! | `interrompido.mkv` | gravação cortada no meio: sem duração no cabeçalho e sem índice (#27) |
//! | `buffer.mkv` + `continuacao.mkv` | trecho do buffer e a gravação que continuou dele, com 1 s em comum (#62) |
//! | `uniao_a.mkv` + `uniao_b.mkv` | duas gravações iguais em formato, pra unir |
//!
//! Regerar (sobrescreve): `cargo test --release gerar_amostras -- --ignored`.

use std::path::PathBuf;

use super::gen::{self, AudioSpec, Spec, VideoSpec};

pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("media")
}

/// Caminho de uma amostra (existe ou não — `hevc.mkv` depende da máquina que gerou).
pub fn path(name: &str) -> String {
    dir().join(name).to_string_lossy().into_owned()
}

const X264: &[(&str, &str)] = &[("crf", "32"), ("preset", "veryfast")];

fn x264(bframes: i32) -> VideoSpec {
    VideoSpec { encoder: "libx264", w: 320, h: 180, fps: 30, bframes, opts: X264, seed: 0 }
}

fn mix(freq: f32) -> AudioSpec {
    AudioSpec { title: "Mix", freq }
}

#[test]
#[ignore = "gera as amostras versionadas; rode a mao quando o formato mudar"]
fn gerar_amostras() {
    assert!(super::dlls(), "FFmpeg indisponivel");
    std::fs::create_dir_all(dir()).unwrap();

    // Gravação comum: H.264 com B-frames, mistura + microfone isolado.
    let comum = gen::encode(&Spec {
        seconds: 4.0,
        video: Some(x264(2)),
        audio: vec![mix(440.0), AudioSpec { title: "Microfone (USB)", freq: 880.0 }],
    })
    .unwrap();
    gen::mux(&comum, &path("h264_2faixas.mkv"), "matroska", &gen::all(&comum), false, true).unwrap();
    gen::mux(&comum, &path("h264.mp4"), "mp4", &gen::all(&comum), false, true).unwrap();
    // Interrompida: 60% dos pacotes e nada de trailer.
    let corte: Vec<_> = gen::all(&comum).into_iter().filter(|p| comum.sec(p) < 2.4).collect();
    gen::mux(&comum, &path("interrompido.mkv"), "matroska", &corte, false, false).unwrap();

    // Buffer + continuação: o buffer vai até 3 s; a gravação começa no keyframe
    // de 2 s. Os dois com o relógio zerado no próprio começo, como a libobs faz.
    let buf: Vec<_> = gen::all(&comum).into_iter().filter(|p| comum.sec(p) < 3.0).collect();
    gen::mux(&comum, &path("buffer.mkv"), "matroska", &buf, true, true).unwrap();
    let k = comum.pkts.iter().find(|p| p.stream == 0 && p.key && comum.sec(p) >= 1.9).expect("keyframe de 2 s");
    let k_dts = k.dts;
    let k_sec = comum.sec(k);
    let cont: Vec<_> = gen::all(&comum)
        .into_iter()
        .filter(|p| if p.stream == 0 { p.dts >= k_dts } else { comum.sec(p) >= k_sec })
        .collect();
    gen::mux(&comum, &path("continuacao.mkv"), "matroska", &cont, true, true).unwrap();

    // Seis faixas: o teto do OBS (mistura + 5 isoladas, uma muda).
    let seis = gen::encode(&Spec {
        seconds: 3.0,
        video: Some(x264(2)),
        audio: vec![
            mix(440.0),
            AudioSpec { title: "Microfone (USB)", freq: 660.0 },
            AudioSpec { title: "Alto-falantes", freq: 0.0 },
            AudioSpec { title: "Headset", freq: 550.0 },
            AudioSpec { title: "Webcam", freq: 990.0 },
            AudioSpec { title: "Linha", freq: 330.0 },
        ],
    })
    .unwrap();
    gen::mux(&seis, &path("h264_6faixas.mkv"), "matroska", &gen::all(&seis), false, true).unwrap();

    let sem_b = gen::encode(&Spec { seconds: 2.0, video: Some(x264(0)), audio: vec![mix(440.0)] }).unwrap();
    gen::mux(&sem_b, &path("h264_sem_bframes.mkv"), "matroska", &gen::all(&sem_b), false, true).unwrap();

    // Sementes diferentes: nenhum quadro em comum (a emenda não pode achar encontro).
    for (nome, freq, seed) in [("uniao_a.mkv", 440.0, 0), ("uniao_b.mkv", 660.0, 1)] {
        let e = gen::encode(&Spec { seconds: 2.0, video: Some(VideoSpec { seed, ..x264(2) }), audio: vec![mix(freq)] }).unwrap();
        gen::mux(&e, &path(nome), "matroska", &gen::all(&e), false, true).unwrap();
    }

    let av1 = gen::encode(&Spec {
        seconds: 2.0,
        video: Some(VideoSpec { encoder: "libsvtav1", w: 320, h: 180, fps: 30, bframes: 0, opts: &[("crf", "50"), ("preset", "10")], seed: 0 }),
        audio: vec![mix(440.0)],
    })
    .unwrap();
    gen::mux(&av1, &path("av1.mkv"), "matroska", &gen::all(&av1), false, true).unwrap();

    // HEVC só existe por hardware neste build (#51f): gera se a placa deixar.
    // Mesmo modo da exportação (#51b): CQP no AMF, qualidade constante no NVENC.
    let hevc: [(&str, &[(&str, &str)]); 3] = [
        ("hevc_amf", &[("rc", "cqp"), ("qp_i", "30"), ("qp_p", "30")]),
        ("hevc_nvenc", &[("rc", "vbr"), ("cq", "30"), ("b", "0")]),
        ("hevc_mf", &[]),
    ];
    for (enc, opts) in hevc {
        let r = gen::encode(&Spec {
            seconds: 2.0,
            video: Some(VideoSpec { encoder: enc, w: 640, h: 360, fps: 30, bframes: 0, opts, seed: 0 }),
            audio: vec![mix(440.0)],
        });
        if let Ok(e) = r {
            gen::mux(&e, &path("hevc.mkv"), "matroska", &gen::all(&e), false, true).unwrap();
            eprintln!("hevc.mkv gerado com {enc}");
            break;
        }
    }

    for f in std::fs::read_dir(dir()).unwrap().flatten() {
        eprintln!("{:>8} {}", f.metadata().unwrap().len(), f.file_name().to_string_lossy());
    }
}

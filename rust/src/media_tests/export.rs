//! `ffmpeg_export`: a exportação (o único caminho que recodifica) e a escolha
//! de encoder. Tudo com `libx264`/`libsvtav1` (software, existe em qualquer
//! máquina); os de hardware só quando a placa deixa.

use std::sync::atomic::AtomicI32;

use super::amostras::path;
use super::need_dlls;
use super::util::{hashes, near, of_kind, of_stream, packets, png_size, probe};
use crate::export_captions::CaptionTurn;
use crate::ffi::ffmpeg::AVMEDIA_TYPE_VIDEO;
use crate::ffmpeg_export::*;
use crate::ffmpeg_ops::extract_frame_png;
use crate::types::{EncoderCaps, RecordingRegion};

/// Exportação "padrão" da gravação comum: MP4, x264, CRF 23, as duas faixas
/// copiadas, o arquivo inteiro (como UM trecho: a lista nunca vem vazia — o
/// bridge recusa antes, e o `export_video` conta com isso).
fn base(src: &str, dst_name: &str) -> ExportOptions {
    let dur = crate::probe::probe(std::path::Path::new(src)).map_or(10.0, |r| r.duration);
    ExportOptions {
        segments: vec![ExportSegment { start_sec: 0.0, end_sec: dur }],
        src_path: src.to_owned(),
        dst_path: super::out(dst_name),
        container: "mp4".into(),
        speed: 1.0,
        encoder_name: "libx264".into(),
        scale_algo: "bicubic".into(),
        crf: 23,
        audio_streams: vec![1, 2],
        ..Default::default()
    }
}

fn run(o: &ExportOptions) -> ExportResult {
    export_video(o, None, None)
}

fn comum() -> String {
    path("h264_2faixas.mkv")
}

// ---- o básico ---------------------------------------------------------------

#[test]
fn exporta_inteiro_em_mp4() {
    need_dlls!();
    let o = base(&comum(), "inteiro.mp4");
    assert_eq!(run(&o), ExportResult::Ok);
    let r = probe(&o.dst_path);
    assert!(r.format.contains("mp4"));
    let v = r.video_stream().unwrap();
    assert_eq!((v.codec.as_str(), v.width, v.height), ("h264", 320, 180));
    near(v.frame_rate, 30.0, 0.05, "fps");
    near(r.duration, 4.0, 0.15, "duração");
    // Faixas copiadas com o NOME (o MP4 guarda no handler_name).
    let titulos: Vec<_> = r.audio_streams().map(|s| s.title.clone()).collect();
    assert_eq!(titulos, ["Mix", "Microfone (USB)"]);
    // Áudio copiado = os mesmos pacotes da origem, byte a byte e sem repetição,
    // menos a pré-rolagem do AAC (pts −21 ms), que fica antes do trecho.
    let (src, dst) = (packets(&comum()), packets(&o.dst_path));
    for s in [1, 2] {
        let esperado: Vec<_> = of_stream(&src, s).into_iter().filter(|p| p.pts >= 0.0).collect();
        assert_eq!(hashes(&of_stream(&dst, s)), hashes(&esperado), "faixa {s}");
    }
    assert_eq!(of_kind(&dst, AVMEDIA_TYPE_VIDEO).len(), 120, "4 s × 30 fps, nenhum quadro perdido");
}

#[test]
fn exporta_em_mkv() {
    need_dlls!();
    let mut o = base(&comum(), "inteiro.mkv");
    o.container = "matroska".into();
    assert_eq!(run(&o), ExportResult::Ok);
    let r = probe(&o.dst_path);
    assert!(r.format.contains("matroska"));
    assert_eq!(r.audio_streams().map(|s| s.title.clone()).collect::<Vec<_>>(), ["Mix", "Microfone (USB)"]);
}

#[test]
fn origem_de_cada_codec() {
    need_dlls!();
    for (nome, w, h) in [("av1.mkv", 320, 180), ("hevc.mkv", 640, 360), ("h264.mp4", 320, 180), ("h264_sem_bframes.mkv", 320, 180)] {
        let src = path(nome);
        if !std::path::Path::new(&src).exists() {
            continue;
        }
        let mut o = base(&src, &format!("de-{nome}.mp4"));
        o.audio_streams = vec![1];
        assert_eq!(run(&o), ExportResult::Ok, "{nome}");
        let v = probe(&o.dst_path);
        assert_eq!((v.video_stream().unwrap().width, v.video_stream().unwrap().height), (w, h), "{nome}");
    }
}

#[test]
fn gravacao_interrompida_exporta() {
    need_dlls!();
    let o = base(&path("interrompido.mkv"), "de-interrompido.mp4");
    assert_eq!(run(&o), ExportResult::Ok);
    assert!(probe(&o.dst_path).duration > 1.0);
}

// ---- trechos ----------------------------------------------------------------

#[test]
fn trechos_emendados_sem_buraco() {
    need_dlls!();
    let mut o = base(&comum(), "trechos.mp4");
    o.segments = vec![ExportSegment { start_sec: 0.0, end_sec: 1.0 }, ExportSegment { start_sec: 2.0, end_sec: 3.5 }];
    assert_eq!(run(&o), ExportResult::Ok);
    let r = probe(&o.dst_path);
    near(r.duration, 2.5, 0.15, "1 s + 1,5 s");
    let n = of_kind(&packets(&o.dst_path), AVMEDIA_TYPE_VIDEO).len() as i64;
    assert!((n - 75).abs() <= 2, "{n} quadros (≈75)");
}

#[test]
fn trecho_no_meio_do_gop() {
    need_dlls!();
    // 1,5 s cai entre keyframes (1 s e 2 s): decodifica do keyframe e descarta.
    let mut o = base(&comum(), "meio-gop.mp4");
    o.segments = vec![ExportSegment { start_sec: 1.5, end_sec: 2.5 }];
    assert_eq!(run(&o), ExportResult::Ok);
    near(probe(&o.dst_path).duration, 1.0, 0.12, "duração");
    // O 1º quadro da saída é o de 1,5 s da origem, não o keyframe de 1 s.
    let (a, b) = (super::out("meio-gop-saida.png"), super::out("meio-gop-origem.png"));
    assert!(extract_frame_png(&o.dst_path, &a, 0.0));
    assert!(extract_frame_png(&comum(), &b, 1.5));
    assert_eq!(png_size(&a), png_size(&b));
}

// ---- qualidade, tamanho, fps, escala, recorte -------------------------------

#[test]
fn crf_mais_alto_arquivo_menor() {
    need_dlls!();
    let mut tamanhos = Vec::new();
    for crf in [0, 23, 51, -10, 99] {
        let mut o = base(&comum(), &format!("crf{crf}.mp4"));
        o.crf = crf;
        o.audio_streams = vec![];
        assert_eq!(run(&o), ExportResult::Ok, "crf {crf}");
        tamanhos.push(std::fs::metadata(&o.dst_path).unwrap().len());
    }
    assert!(tamanhos[0] > tamanhos[1] && tamanhos[1] > tamanhos[2], "{tamanhos:?}");
    // Fora da faixa vira o extremo (−10 → 0, 99 → 51).
    assert_eq!(tamanhos[3], tamanhos[0]);
    assert_eq!(tamanhos[4], tamanhos[2]);
}

#[test]
fn reduz_resolucao_e_nunca_amplia() {
    need_dlls!();
    for (alvo, esperado, algo) in [(90, (160, 90), "bicubic"), (90, (160, 90), "bilinear"), (90, (160, 90), "area"), (1080, (320, 180), "bicubic"), (0, (320, 180), "bicubic")] {
        let mut o = base(&comum(), &format!("alt{alvo}-{algo}.mp4"));
        o.target_height = alvo;
        o.scale_algo = algo.into();
        assert_eq!(run(&o), ExportResult::Ok);
        let v = probe(&o.dst_path);
        let v = v.video_stream().unwrap();
        assert_eq!((v.width, v.height), esperado, "alvo {alvo} {algo}");
    }
}

#[test]
fn taxa_de_quadros_menor() {
    need_dlls!();
    for (fps, quadros) in [(15, 60), (10, 40), (30, 120), (999, 120)] {
        let mut o = base(&comum(), &format!("fps{fps}.mp4"));
        o.target_fps = fps;
        assert_eq!(run(&o), ExportResult::Ok, "{fps}");
        let n = of_kind(&packets(&o.dst_path), AVMEDIA_TYPE_VIDEO).len() as i64;
        assert!((n - quadros).abs() <= 2, "fps {fps}: {n} quadros (≈{quadros})");
    }
}

#[test]
fn recorte_de_regiao() {
    need_dlls!();
    let reg = |x, y, w, h| RecordingRegion { name: "r".into(), kind: "monitor".into(), x, y, w, h };
    for (regioes, esperado) in [
        (vec![reg(0, 0, 160, 180)], (160, 180)),
        // Ímpar vira par (YUV420, #51c).
        (vec![reg(11, 7, 101, 51)], (100, 50)),
        // Passando da borda: cortado no canvas.
        (vec![reg(300, 0, 200, 180)], (20, 180)),
        // Duas regiões lado a lado.
        (vec![reg(160, 0, 160, 180), reg(0, 0, 100, 90)], (260, 180)),
    ] {
        let mut o = base(&comum(), &format!("rec-{}x{}.mp4", esperado.0, esperado.1));
        o.regions = regioes;
        assert_eq!(run(&o), ExportResult::Ok, "{esperado:?}");
        let r = probe(&o.dst_path);
        let v = r.video_stream().unwrap();
        assert_eq!((v.width, v.height), esperado);
    }
}

#[test]
fn regiao_degenerada_falha() {
    need_dlls!();
    let mut o = base(&comum(), "degenerada.mp4");
    o.regions = vec![RecordingRegion { name: "r".into(), kind: "monitor".into(), x: 0, y: 0, w: 1, h: 1 }];
    assert_ne!(run(&o), ExportResult::Ok);
}

// ---- áudio ------------------------------------------------------------------

#[test]
fn faixas_escolhidas_mixadas_e_nenhuma() {
    need_dlls!();
    let src = path("h264_6faixas.mkv");
    // Três isoladas copiadas.
    let mut o = base(&src, "tres.mp4");
    o.audio_streams = vec![2, 4, 6];
    assert_eq!(run(&o), ExportResult::Ok);
    assert_eq!(probe(&o.dst_path).audio_streams().map(|s| s.title.clone()).collect::<Vec<_>>(), ["Microfone (USB)", "Headset", "Linha"]);
    // Mixadas numa só, com o título pedido.
    let mut o = base(&src, "mix.mp4");
    o.audio_streams = vec![2, 4];
    o.mix_audio = true;
    o.mix_title = "Mistura".into();
    assert_eq!(run(&o), ExportResult::Ok);
    let r = probe(&o.dst_path);
    assert_eq!(r.audio_track_count(), 1);
    assert_eq!(r.audio_streams().next().unwrap().title, "Mistura");
    // Nenhuma.
    let mut o = base(&src, "sem-audio.mp4");
    o.audio_streams = vec![];
    assert_eq!(run(&o), ExportResult::Ok);
    assert_eq!(probe(&o.dst_path).audio_track_count(), 0);
    // Índices inválidos (vídeo, fora do arquivo) são ignorados.
    let mut o = base(&src, "invalidas.mp4");
    o.audio_streams = vec![0, 99, -1, 1];
    assert_eq!(run(&o), ExportResult::Ok);
    assert_eq!(probe(&o.dst_path).audio_track_count(), 1);
}

#[test]
fn so_audio_sai_em_mp3() {
    need_dlls!();
    let mut o = base(&comum(), "so-audio.mp3");
    o.container = "mp3".into();
    o.no_video = true;
    o.audio_streams = vec![1];
    o.mix_audio = true;
    o.audio_codec = "libmp3lame".into();
    o.audio_bitrate = 128_000;
    assert_eq!(run(&o), ExportResult::Ok);
    let r = probe(&o.dst_path);
    assert!(r.video_stream().is_none());
    assert_eq!(r.audio_track_count(), 1);
    assert_eq!(r.audio_streams().next().unwrap().codec, "mp3");
    near(r.duration, 4.0, 0.1, "duração");
}

// ---- velocidade (#51m) ------------------------------------------------------

#[test]
fn acelerado_encurta_video_e_audio() {
    need_dlls!();
    for speed in [2.0, 1.5, 8.0] {
        let mut o = base(&comum(), &format!("x{speed}.mp4"));
        o.speed = speed;
        assert_eq!(run(&o), ExportResult::Ok, "{speed}×");
        let r = probe(&o.dst_path);
        near(r.duration, 4.0 / speed, 0.15, &format!("{speed}× duração"));
        // Áudio recodificado (o tempo muda), as duas faixas continuam.
        assert_eq!(r.audio_track_count(), 2, "{speed}×");
        for a in r.audio_streams() {
            near(a.duration, 4.0 / speed, 0.2, &format!("{speed}× áudio"));
        }
    }
}

#[test]
fn velocidade_fora_da_faixa_e_limitada() {
    need_dlls!();
    for (speed, nome) in [(0.0, "zero"), (-3.0, "negativa")] {
        let mut o = base(&comum(), &format!("vel-{nome}.mp4"));
        o.speed = speed;
        assert_eq!(run(&o), ExportResult::Ok, "{nome}");
        near(probe(&o.dst_path).duration, 4.0, 0.15, "vale 1×");
    }
}

// ---- legenda gravada (#51i) -------------------------------------------------

#[test]
fn legenda_gravada_muda_so_os_quadros_do_trecho() {
    need_dlls!();
    let mut com = base(&comum(), "com-legenda.mp4");
    com.crf = 0;
    com.captions = vec![CaptionTurn { start_sec: 1.0, end_sec: 2.0, text: "Olá, isto é uma legenda".into() }];
    let mut sem = base(&comum(), "sem-legenda.mp4");
    sem.crf = 0;
    assert_eq!(run(&com), ExportResult::Ok);
    assert_eq!(run(&sem), ExportResult::Ok);
    let quadro = |f: &str, sec: f64, n: &str| {
        let p = super::out(n);
        assert!(extract_frame_png(f, &p, sec));
        std::fs::read(p).unwrap()
    };
    assert_ne!(quadro(&com.dst_path, 1.5, "c15.png"), quadro(&sem.dst_path, 1.5, "s15.png"), "no ar");
    assert_eq!(quadro(&com.dst_path, 3.0, "c30.png"), quadro(&sem.dst_path, 3.0, "s30.png"), "fora do ar");
}

#[test]
fn legenda_vazia_ou_fora_do_video_nao_atrapalha() {
    need_dlls!();
    let mut o = base(&comum(), "legenda-estranha.mp4");
    o.captions = vec![
        CaptionTurn { start_sec: 50.0, end_sec: 60.0, text: "depois do fim".into() },
        CaptionTurn { start_sec: 1.0, end_sec: 0.5, text: "invertida".into() },
        CaptionTurn { start_sec: 0.0, end_sec: 1.0, text: "   ".into() },
    ];
    assert_eq!(run(&o), ExportResult::Ok);
}

// ---- progresso, cancelamento, erros -----------------------------------------

#[test]
fn progresso_sobe_ate_cem() {
    need_dlls!();
    let o = base(&comum(), "progresso.mp4");
    let vistos = std::sync::Mutex::new(Vec::new());
    let cb = |p: f64| vistos.lock().unwrap().push(p);
    assert_eq!(export_video(&o, Some(&cb), None), ExportResult::Ok);
    let v = vistos.into_inner().unwrap();
    assert!(!v.is_empty());
    assert!(v.windows(2).all(|w| w[1] >= w[0]), "só sobe: {v:?}");
    assert!(v.iter().all(|p| (0.0..=100.0).contains(p)));
    assert!(*v.last().unwrap() >= 95.0, "{v:?}");
}

#[test]
fn cancelada_antes_de_comecar() {
    need_dlls!();
    let o = base(&comum(), "cancelada.mp4");
    let flag = AtomicI32::new(1);
    assert_eq!(export_video(&o, None, Some(&flag)), ExportResult::Canceled);
}

#[test]
fn erros_de_entrada() {
    need_dlls!();
    let mut o = base("C:\\nao\\existe.mkv", "x.mp4");
    assert_eq!(run(&o), ExportResult::Error);
    o = base(&comum(), "y.mp4");
    o.dst_path = String::new();
    assert_eq!(run(&o), ExportResult::Error);
    o = base(&comum(), "z.mp4");
    o.encoder_name = "encoder_que_nao_existe".into();
    assert_eq!(run(&o), ExportResult::NoEncoder);
    // Trecho todo depois do fim do arquivo: nada pra exportar.
    o = base(&comum(), "w.mp4");
    o.segments = vec![ExportSegment { start_sec: 100.0, end_sec: 200.0 }];
    assert_ne!(run(&o), ExportResult::Ok);
}

// ---- encoders ---------------------------------------------------------------

#[test]
fn av1_por_software() {
    need_dlls!();
    let mut o = base(&comum(), "av1-sw.mp4");
    o.encoder_name = "libsvtav1".into();
    o.crf = 40;
    assert_eq!(run(&o), ExportResult::Ok);
    assert_eq!(probe(&o.dst_path).video_stream().unwrap().codec, "av1");
}

#[test]
fn encoders_de_hardware_quando_a_placa_deixa() {
    need_dlls!();
    for enc in ["h264_amf", "hevc_amf", "av1_amf", "h264_nvenc", "hevc_nvenc", "av1_nvenc"] {
        let mut o = base(&comum(), &format!("hw-{enc}.mp4"));
        o.encoder_name = enc.into();
        match run(&o) {
            ExportResult::Ok => {
                let codec = probe(&o.dst_path).video_stream().unwrap().codec.clone();
                assert_eq!(codec, enc.split('_').next().unwrap(), "{enc}");
            }
            // Placa de outro fabricante (ou sem o codec): recusa limpa.
            ExportResult::NoEncoder | ExportResult::Error => eprintln!("{enc}: indisponivel nesta maquina"),
            ExportResult::Canceled => panic!("{enc}: cancelada sem pedido"),
        }
    }
}

#[test]
fn lista_e_escolha_de_encoders() {
    need_dlls!();
    let sem_placa = EncoderCaps::default();
    let ids: Vec<_> = list_export_encoders(&sem_placa).into_iter().map(|e| (e.id, e.libav_name, e.hardware)).collect();
    assert_eq!(ids, [("h264-sw".into(), "libx264".into(), false), ("av1-sw".into(), "libsvtav1".into(), false)]);
    assert_eq!(resolve_export_encoder("auto", &sem_placa), "libx264");
    assert_eq!(resolve_export_encoder("", &sem_placa), "libx264");
    assert_eq!(resolve_export_encoder("AV1-SW", &sem_placa), "libsvtav1");
    // Pedido que esta máquina não tem: cai no x264.
    assert_eq!(resolve_export_encoder("hevc-hw", &sem_placa), "libx264");
    assert_eq!(resolve_export_encoder("lixo", &sem_placa), "libx264");

    let amd = EncoderCaps { h264_hw: true, hevc_hw: true, av1_hw: true, vendor: crate::types::GpuVendor::Amd, ..Default::default() };
    let nomes: Vec<_> = list_export_encoders(&amd).into_iter().map(|e| e.libav_name).collect();
    assert_eq!(nomes, ["h264_amf", "libx264", "av1_amf", "libsvtav1", "hevc_amf"]);
    // Com H.264 por hardware, o automático é ele.
    assert_eq!(resolve_export_encoder("auto", &amd), "h264_amf");
    assert_eq!(resolve_export_encoder("hevc-hw", &amd), "hevc_amf");
}

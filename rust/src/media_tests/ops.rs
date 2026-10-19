//! `ffmpeg_ops`: as operações que só copiam pacotes (remux, faixas, união,
//! emenda do buffer) e as que decodificam pouco (miniatura, quadro exato,
//! waveform, áudio pra transcrição, índice de keyframes).

use super::amostras::path;
use super::need_dlls;
use super::util::{copy_as, hashes, jpeg_size, near, of_kind, of_stream, packets, png_size, probe};
use crate::ffi::ffmpeg::{AVMEDIA_TYPE_AUDIO, AVMEDIA_TYPE_VIDEO};
use crate::ffmpeg_ops::*;

// ---- remux ------------------------------------------------------------------

#[test]
fn remux_mkv_para_mp4_copia_todos_os_pacotes() {
    need_dlls!();
    let src = path("h264_2faixas.mkv");
    let dst = super::out("remux.mp4");
    assert!(remux_file(&src, &dst));
    let (a, b) = (packets(&src), packets(&dst));
    for s in 0..3 {
        assert_eq!(hashes(&of_stream(&a, s)), hashes(&of_stream(&b, s)), "stream {s}: bytes iguais, na mesma ordem");
    }
    let r = probe(&dst);
    assert!(r.format.contains("mp4"));
    near(r.duration, 4.0, 0.1, "duração");
    assert_eq!(r.audio_streams().map(|s| s.title.clone()).collect::<Vec<_>>(), ["Mix", "Microfone (USB)"]);
}

#[test]
fn remux_de_cada_codec_e_container() {
    need_dlls!();
    for (src, ext) in [("av1.mkv", "mp4"), ("hevc.mkv", "mp4"), ("h264.mp4", "mkv"), ("h264_6faixas.mkv", "mp4")] {
        let src = path(src);
        if !std::path::Path::new(&src).exists() {
            continue;
        }
        let dst = super::out(&format!("remux-{}.{ext}", std::path::Path::new(&src).file_stem().unwrap().to_string_lossy()));
        assert!(remux_file(&src, &dst), "{src}");
        assert_eq!(packets(&src).len(), packets(&dst).len(), "{src}");
    }
}

#[test]
fn remux_de_gravacao_interrompida_gera_arquivo_fechado() {
    need_dlls!();
    let dst = super::out("interrompido-remux.mp4");
    assert!(remux_file(&path("interrompido.mkv"), &dst));
    // O MP4 novo tem a duração no cabeçalho (é o que o player precisa).
    assert!(probe(&dst).duration > 1.0);
}

#[test]
fn remux_recusa_entrada_ou_saida_invalida() {
    need_dlls!();
    let txt = super::out("nao-video.mkv");
    std::fs::write(&txt, "x".repeat(1000)).unwrap();
    assert!(!remux_file("C:\\nao\\existe.mkv", &super::out("x.mp4")));
    assert!(!remux_file(&txt, &super::out("y.mp4")));
    assert!(!remux_file(&path("h264_2faixas.mkv"), "C:\\pasta\\que\\nao\\existe\\z.mp4"));
}

// ---- faixas de áudio --------------------------------------------------------

#[test]
fn extrair_faixas_isoladas_pulando_a_mistura() {
    need_dlls!();
    let src = path("h264_6faixas.mkv");
    let outs: Vec<String> = (1..6).map(|i| super::out(&format!("faixa{i}.m4a"))).collect();
    assert!(extract_audio_tracks(&src, &outs, 1, None));
    let origem = packets(&src);
    for (j, o) in outs.iter().enumerate() {
        let r = probe(o);
        assert_eq!(r.audio_track_count(), 1, "{o}");
        assert!(r.video_stream().is_none());
        // Stream (j+2) da origem: vídeo é o 0, a mistura o 1.
        assert_eq!(hashes(&packets(o)), hashes(&of_stream(&origem, j as i32 + 2)), "faixa {}", j + 1);
    }
}

#[test]
fn extrair_todas_ou_so_algumas() {
    need_dlls!();
    let src = path("h264_6faixas.mkv");
    let todas: Vec<String> = (0..6).map(|i| super::out(&format!("todas{i}.m4a"))).collect();
    assert!(extract_audio_tracks(&src, &todas, 0, None));
    assert!(todas.iter().all(|o| std::path::Path::new(o).exists()));
    // Menos saídas que faixas: só as pedidas.
    let duas = vec![super::out("so1.m4a"), super::out("so2.m4a")];
    assert!(extract_audio_tracks(&src, &duas, 0, None));
    assert_eq!(hashes(&packets(&duas[1])), hashes(&of_stream(&packets(&src), 2)));
}

#[test]
fn extrair_faixas_recusa_e_cancela() {
    need_dlls!();
    let src = path("h264_2faixas.mkv");
    assert!(!extract_audio_tracks(&src, &[], 0, None));
    assert!(!extract_audio_tracks("C:\\nao\\existe.mkv", &[super::out("x.m4a")], 0, None));
    let cancela = || true;
    assert!(!extract_audio_tracks(&src, &[super::out("cancelada.m4a")], 0, Some(&cancela)));
}

// ---- união ------------------------------------------------------------------

#[test]
fn unir_duas_gravacoes() {
    need_dlls!();
    let (a, b) = (path("uniao_a.mkv"), path("uniao_b.mkv"));
    let dst = super::out("unida.mkv");
    assert_eq!(merge_files(&[a.clone(), b.clone()], &dst), Ok(()));
    let r = probe(&dst);
    near(r.duration, 4.0, 0.15, "duração = soma");
    let (pa, pb, pu) = (packets(&a), packets(&b), packets(&dst));
    let mut esperado = hashes(&of_kind(&pa, AVMEDIA_TYPE_VIDEO));
    esperado.extend(hashes(&of_kind(&pb, AVMEDIA_TYPE_VIDEO)));
    assert_eq!(hashes(&of_kind(&pu, AVMEDIA_TYPE_VIDEO)), esperado, "vídeo de A depois o de B, sem perda");
    assert_eq!(of_kind(&pu, AVMEDIA_TYPE_AUDIO).len(), of_kind(&pa, AVMEDIA_TYPE_AUDIO).len() + of_kind(&pb, AVMEDIA_TYPE_AUDIO).len());
    // O relógio anda pra frente o tempo todo (a 2ª parte é deslocada).
    let v: Vec<f64> = of_kind(&pu, AVMEDIA_TYPE_VIDEO).iter().filter(|p| p.key).map(|p| p.pts).collect();
    assert!(v.windows(2).all(|w| w[1] > w[0]), "{v:?}");
}

#[test]
fn unir_tres_e_a_mesma_duas_vezes() {
    need_dlls!();
    let a = path("uniao_a.mkv");
    let dst = super::out("tripla.mkv");
    assert_eq!(merge_files(&[a.clone(), path("uniao_b.mkv"), a.clone()], &dst), Ok(()));
    near(probe(&dst).duration, 6.0, 0.2, "duração");
}

#[test]
fn unir_formatos_diferentes_e_recusado() {
    need_dlls!();
    // Faixas diferentes (2 vs 1 de áudio) e codec diferente (H.264 vs AV1).
    assert_eq!(merge_files(&[path("h264_2faixas.mkv"), path("uniao_a.mkv")], &super::out("m1.mkv")), Err(true));
    assert_eq!(merge_files(&[path("uniao_a.mkv"), path("av1.mkv")], &super::out("m2.mkv")), Err(true));
}

#[test]
fn unir_com_entrada_invalida() {
    need_dlls!();
    let a = path("uniao_a.mkv");
    assert_eq!(merge_files(&[a.clone()], &super::out("m3.mkv")), Err(false), "uma só");
    assert_eq!(merge_files(&[a.clone(), a.clone()], ""), Err(false), "sem destino");
    assert!(merge_files(&[a.clone(), "C:\\nao\\existe.mkv".into()], &super::out("m4.mkv")).is_err());
}

// ---- emenda do buffer com a gravação (#62) ----------------------------------

#[test]
fn emenda_buffer_e_continuacao_sem_buraco_nem_repeticao() {
    need_dlls!();
    let dst = super::out("emendada.mkv");
    let info = splice_continuation(&path("buffer.mkv"), &path("continuacao.mkv"), &dst, 1000).expect("emenda");
    assert!(info.contains("repetido"), "{info}");
    // Os dois foram cortados do MESMO fluxo: emendados, têm que ser exatamente
    // a gravação inteira — cada pacote uma vez só, na ordem.
    let (inteira, emendada) = (packets(&path("h264_2faixas.mkv")), packets(&dst));
    for s in 0..3 {
        assert_eq!(hashes(&of_stream(&emendada, s)), hashes(&of_stream(&inteira, s)), "stream {s}");
    }
    near(probe(&dst).duration, 4.0, 0.1, "duração");
}

#[test]
fn emenda_com_lead_desconhecido_ou_errado() {
    need_dlls!();
    // O lead só desempata keyframes idênticos; aqui cada quadro é único.
    for lead in [0, -1, 1000, 1_000_000] {
        let dst = super::out(&format!("emenda-lead{lead}.mkv"));
        assert!(splice_continuation(&path("buffer.mkv"), &path("continuacao.mkv"), &dst, lead).is_ok(), "lead {lead}");
        near(probe(&dst).duration, 4.0, 0.1, "duração");
    }
}

#[test]
fn emenda_sem_ponto_de_encontro_e_recusada() {
    need_dlls!();
    // Ordem trocada, gravações sem nada em comum, faixas diferentes, inexistente.
    for (head, tail) in [
        (path("continuacao.mkv"), path("buffer.mkv")),
        (path("uniao_a.mkv"), path("uniao_b.mkv")),
        (path("buffer.mkv"), path("h264_6faixas.mkv")),
        (path("buffer.mkv"), "C:\\nao\\existe.mkv".to_owned()),
    ] {
        let r = splice_continuation(&head, &tail, &super::out("nao-emendada.mkv"), 1000);
        assert!(r.is_err(), "{head} + {tail}: {r:?}");
    }
}

// ---- miniatura e quadro exato -----------------------------------------------

#[test]
fn miniatura_jpeg_na_altura_pedida() {
    need_dlls!();
    let src = path("h264_2faixas.mkv");
    for (h, esperado) in [(0, (426, 240)), (90, (160, 90)), (180, (320, 180)), (101, (178, 101))] {
        let dst = super::out(&format!("thumb{h}.jpg"));
        assert!(extract_frame_jpeg(&src, &dst, 1, h), "altura {h}");
        assert_eq!(jpeg_size(&dst), esperado, "altura {h}");
    }
}

#[test]
fn miniatura_de_cada_codec_e_instante() {
    need_dlls!();
    for nome in ["h264_2faixas.mkv", "h264_sem_bframes.mkv", "av1.mkv", "hevc.mkv", "h264.mp4", "interrompido.mkv"] {
        let src = path(nome);
        if !std::path::Path::new(&src).exists() {
            continue;
        }
        // Começo, meio e bem depois do fim (fica no último keyframe).
        for ts in [0, 1, 999] {
            let dst = super::out(&format!("thumb-{nome}-{ts}.jpg"));
            assert!(extract_frame_jpeg(&src, &dst, ts, 120), "{nome} @ {ts}s");
            assert_eq!(jpeg_size(&dst).1, 120, "{nome} @ {ts}s");
        }
    }
}

#[test]
fn miniatura_de_arquivo_invalido_falha() {
    need_dlls!();
    let txt = super::out("nao-video2.mkv");
    std::fs::write(&txt, "x".repeat(1000)).unwrap();
    assert!(!extract_frame_jpeg(&txt, &super::out("t.jpg"), 0, 120));
    assert!(!extract_frame_jpeg("C:\\nao\\existe.mkv", &super::out("t2.jpg"), 0, 120));
    // Só áudio: não tem quadro.
    let m4a = super::out("so-audio.m4a");
    assert!(extract_audio_tracks(&path("uniao_a.mkv"), &[m4a.clone()], 0, None));
    assert!(!extract_frame_jpeg(&m4a, &super::out("t3.jpg"), 0, 120));
}

#[test]
fn quadro_exato_em_png() {
    need_dlls!();
    let src = path("h264_2faixas.mkv");
    let q = |sec: f64, nome: &str| {
        let dst = super::out(nome);
        assert!(extract_frame_png(&src, &dst, sec), "{sec}");
        assert_eq!(png_size(&dst), (320, 180));
        std::fs::read(&dst).unwrap()
    };
    let (a, a2, b, c) = (q(1.0, "q1.png"), q(1.0, "q1b.png"), q(1.5, "q15.png"), q(1.0 + 1.0 / 30.0, "q1mais.png"));
    assert_eq!(a, a2, "mesmo instante, mesmo quadro");
    assert_ne!(a, b, "instantes diferentes, quadros diferentes");
    // Um quadro depois (no MEIO do GOP, com B-frames): ainda assim outro quadro.
    assert_ne!(a, c, "quadro exato, não o keyframe do seek");
    // Depois do fim: o último quadro.
    assert_eq!(q(999.0, "qfim.png"), q(3.98, "qultimo.png"));
}

#[test]
fn quadro_exato_de_cada_codec() {
    need_dlls!();
    for (nome, w, h) in [("av1.mkv", 320, 180), ("hevc.mkv", 640, 360), ("h264.mp4", 320, 180)] {
        let src = path(nome);
        if !std::path::Path::new(&src).exists() {
            continue;
        }
        let dst = super::out(&format!("q-{nome}.png"));
        assert!(extract_frame_png(&src, &dst, 0.5), "{nome}");
        assert_eq!(png_size(&dst), (w, h), "{nome}");
    }
    assert!(!extract_frame_png("C:\\nao\\existe.mkv", &super::out("q.png"), 0.0));
}

// ---- waveform e áudio da transcrição ----------------------------------------

#[test]
fn picos_da_waveform() {
    need_dlls!();
    let src = path("h264_2faixas.mkv");
    for n in [1, 50, 100, 4000] {
        let p = compute_audio_peaks(&src, n, None).expect("picos");
        assert_eq!(p.len(), n);
        assert!(p.iter().all(|v| (0.0..=1.0).contains(v)), "faixa 0..1");
        // Seno a 0,3 de amplitude em todo o arquivo; o AAC a 32 kbps estoura a
        // onda até ~0,42 (medido), por isso a faixa larga.
        let max = p.iter().cloned().fold(0.0f32, f32::max);
        assert!((0.25..=0.5).contains(&max), "pico {max}");
    }
}

#[test]
fn picos_recusa_e_cancela() {
    need_dlls!();
    let src = path("h264_2faixas.mkv");
    assert!(compute_audio_peaks(&src, 0, None).is_none());
    assert!(compute_audio_peaks("C:\\nao\\existe.mkv", 10, None).is_none());
    let cancela = || true;
    assert!(compute_audio_peaks(&src, 10, Some(&cancela)).is_none());
}

#[test]
fn audio_mono_em_blocos() {
    need_dlls!();
    let mut total = 0usize;
    let mut taxa = 0;
    let mut pico = 0.0f32;
    assert!(decode_audio_mono(&path("h264_2faixas.mkv"), &mut |b, r| {
        total += b.len();
        taxa = r;
        pico = b.iter().fold(pico, |m, v| m.max(v.abs()));
        true
    }));
    assert_eq!(taxa, 48_000);
    near(total as f64 / 48_000.0, 4.0, 0.05, "segundos de áudio");
    assert!((0.25..=0.5).contains(&pico), "pico {pico}");
}

#[test]
fn audio_mono_interrompido_pelo_callback() {
    need_dlls!();
    let mut blocos = 0;
    assert!(!decode_audio_mono(&path("h264_2faixas.mkv"), &mut |_, _| {
        blocos += 1;
        blocos < 3
    }));
    assert_eq!(blocos, 3);
    assert!(!decode_audio_mono("C:\\nao\\existe.mkv", &mut |_, _| true));
}

// ---- índice de keyframes ----------------------------------------------------

#[test]
fn keyframes_pelo_indice() {
    need_dlls!();
    let (k, fps) = list_video_keyframes(&path("h264_2faixas.mkv")).expect("índice");
    near(fps, 30.0, 0.01, "fps");
    assert_eq!(k.len(), 4, "{k:?}");
    for (i, t) in k.iter().enumerate() {
        near(*t, i as f64, 0.05, "keyframe a cada 1 s");
    }
}

#[test]
fn keyframes_de_gravacao_interrompida_e_sem_video() {
    need_dlls!();
    // Interrompida: sem Cues (vão no trailer). O demuxer varre os clusters e
    // acha os keyframes que sobraram — o de 2 s se perdeu com o último cluster.
    let (k, _) = list_video_keyframes(&path("interrompido.mkv")).expect("índice parcial");
    assert_eq!(k, vec![0.0, 1.0]);
    assert!(list_video_keyframes("C:\\nao\\existe.mkv").is_none());
    // Só áudio: sem vídeo, sem keyframe.
    let m4a = super::out("so-audio-kf.m4a");
    assert!(extract_audio_tracks(&path("uniao_a.mkv"), &[m4a.clone()], 0, None));
    assert!(list_video_keyframes(&m4a).is_none());
    // A cópia com outro nome lê igual (o índice vem do arquivo, não do cache).
    let copia = copy_as(&path("uniao_a.mkv"), "uniao-copia.mkv");
    assert_eq!(list_video_keyframes(&copia).map(|(k, _)| k.len()), Some(2));
}

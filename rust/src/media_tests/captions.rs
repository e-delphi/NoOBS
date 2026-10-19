//! `export_captions::CaptionBurner`: a legenda gravada no vídeo exportado
//! (#51i), direto num quadro YUV 4:2:0, sem passar pela exportação.

use super::need_dlls;
use crate::export_captions::{CaptionBurner, CaptionTurn};
use crate::ffi::ffmpeg::*;

fn turn(start: f64, end: f64, text: &str) -> CaptionTurn {
    CaptionTurn { start_sec: start, end_sec: end, text: text.into() }
}

/// Quadro YUV420P preto (Y 16, croma neutro).
fn frame(w: i32, h: i32) -> Frame {
    let a = api().unwrap();
    let mut f = Frame::alloc().unwrap();
    f.width = w;
    f.height = h;
    f.format = AV_PIX_FMT_YUV420P;
    assert!(unsafe { (a.util.av_frame_get_buffer)(f.ptr(), 0) } >= 0);
    unsafe {
        for p in 0..3 {
            let (pw, ph) = if p == 0 { (w, h) } else { (w / 2, h / 2) };
            for y in 0..ph as usize {
                std::slice::from_raw_parts_mut(f.data[p].add(y * f.linesize[p] as usize), pw as usize).fill(if p == 0 { 16 } else { 128 });
            }
        }
    }
    f
}

/// Os três planos, linha a linha (sem o preenchimento do linesize).
fn planes(f: &Frame) -> Vec<u8> {
    let mut v = Vec::new();
    unsafe {
        for p in 0..3 {
            let (pw, ph) = if p == 0 { (f.width, f.height) } else { (f.width / 2, f.height / 2) };
            for y in 0..ph as usize {
                v.extend_from_slice(std::slice::from_raw_parts(f.data[p].add(y * f.linesize[p] as usize), pw as usize));
            }
        }
    }
    v
}

/// Linhas do plano Y que mudaram em relação ao quadro preto.
fn changed_rows(f: &Frame) -> Vec<usize> {
    let mut rows = Vec::new();
    unsafe {
        for y in 0..f.height as usize {
            let row = std::slice::from_raw_parts(f.data[0].add(y * f.linesize[0] as usize), f.width as usize);
            if row.iter().any(|&p| p != 16) {
                rows.push(y);
            }
        }
    }
    rows
}

#[test]
fn texto_curto_um_bloco_desenhado_embaixo() {
    need_dlls!();
    let mut b = CaptionBurner::new(1280, 720, &[turn(1.0, 3.0, "Bom dia")]).unwrap();
    assert_eq!(b.chunk_count(), 1);
    let mut f = frame(1280, 720);
    b.blend_at(2.0, &mut f);
    let rows = changed_rows(&f);
    assert!(!rows.is_empty(), "texto no ar desenha");
    // No terço de baixo, acima da borda (6% de margem).
    assert!(*rows.first().unwrap() > 720 * 2 / 3, "{rows:?}");
    assert!(*rows.last().unwrap() < 720 - 720 * 6 / 100 + 10, "{rows:?}");
    // Texto branco sobre preto: o pico do Y sobe bem.
    let max = unsafe { (0..720).flat_map(|y| std::slice::from_raw_parts(f.data[0].add(y * f.linesize[0] as usize), 1280).to_vec()).max().unwrap() };
    assert!(max > 200, "{max}");
}

#[test]
fn fora_do_tempo_nao_mexe_no_quadro() {
    need_dlls!();
    let mut b = CaptionBurner::new(640, 360, &[turn(1.0, 2.0, "Olá")]).unwrap();
    for t in [0.0, 0.999, 2.5, 100.0, -1.0] {
        let mut f = frame(640, 360);
        let antes = planes(&f);
        b.blend_at(t, &mut f);
        assert_eq!(planes(&f), antes, "t={t}");
    }
}

#[test]
fn fala_longa_vira_varios_blocos_de_duas_linhas() {
    need_dlls!();
    let longo = "Então eu acho que a melhor saída aqui é adiar tudo pra semana que vem, porque ninguém conseguiu revisar o orçamento a tempo e a reunião com o cliente ficou pra quinta-feira de manhã logo cedo.";
    let curto = CaptionBurner::new(1280, 720, &[turn(0.0, 10.0, "Sim")]).unwrap();
    let b = CaptionBurner::new(1280, 720, &[turn(0.0, 10.0, longo)]).unwrap();
    assert_eq!(curto.chunk_count(), 1);
    assert!(b.chunk_count() >= 2, "{}", b.chunk_count());
    // Cada bloco cabe em duas linhas: desenhado, nunca passa de ~3 alturas de linha.
    let mut b = b;
    for t in [0.5, 5.0, 9.5] {
        let mut f = frame(1280, 720);
        b.blend_at(t, &mut f);
        let rows = changed_rows(&f);
        assert!(!rows.is_empty(), "t={t}");
        let altura = rows.last().unwrap() - rows.first().unwrap();
        // fonte 4,5% de 720 = 32 px; duas linhas + contorno < 3 linhas.
        assert!(altura < 32 * 3, "t={t}: {altura}px");
    }
}

#[test]
fn blocos_diferentes_desenham_diferente() {
    need_dlls!();
    let mut b = CaptionBurner::new(640, 360, &[turn(0.0, 1.0, "Primeira fala"), turn(1.0, 2.0, "Segunda, bem diferente")]).unwrap();
    let (mut f1, mut f2) = (frame(640, 360), frame(640, 360));
    b.blend_at(0.5, &mut f1);
    b.blend_at(1.5, &mut f2);
    assert_ne!(planes(&f1), planes(&f2));
    // Voltar ao primeiro (rasteriza de novo) dá o mesmo desenho.
    let mut f3 = frame(640, 360);
    b.blend_at(0.5, &mut f3);
    assert_eq!(planes(&f1), planes(&f3));
}

#[test]
fn turnos_vazios_ou_invertidos_nao_aparecem() {
    need_dlls!();
    let b = CaptionBurner::new(640, 360, &[turn(0.0, 1.0, "   "), turn(0.0, 1.0, "")]).unwrap();
    assert_eq!(b.chunk_count(), 0);
    let b = CaptionBurner::new(640, 360, &[]).unwrap();
    assert_eq!(b.chunk_count(), 0);
    // Invertido (fim antes do início): vira bloco de duração zero, que nunca
    // entra no ar — e não tapa a fala válida que começou antes dele.
    let mut so_a = CaptionBurner::new(640, 360, &[turn(0.0, 5.0, "A")]).unwrap();
    let mut com_x = CaptionBurner::new(640, 360, &[turn(0.0, 5.0, "A"), turn(2.0, 1.0, "invertido")]).unwrap();
    for t in [1.0, 2.0, 3.0] {
        let (mut f1, mut f2) = (frame(640, 360), frame(640, 360));
        so_a.blend_at(t, &mut f1);
        com_x.blend_at(t, &mut f2);
        assert_eq!(planes(&f1), planes(&f2), "t={t}");
    }
}

#[test]
fn sobrepostos_vale_o_que_comecou_por_ultimo() {
    need_dlls!();
    // Regra do player (#60p): no trecho em comum aparece o que começou depois.
    let mut so_b = CaptionBurner::new(640, 360, &[turn(1.0, 3.0, "B")]).unwrap();
    let mut ambos = CaptionBurner::new(640, 360, &[turn(0.0, 3.0, "A comprido"), turn(1.0, 3.0, "B")]).unwrap();
    let (mut f1, mut f2) = (frame(640, 360), frame(640, 360));
    so_b.blend_at(2.0, &mut f1);
    ambos.blend_at(2.0, &mut f2);
    assert_eq!(planes(&f1), planes(&f2));
}

#[test]
fn tamanhos_extremos_e_texto_unicode() {
    need_dlls!();
    for (w, h) in [(320, 180), (3840, 2160), (7680, 2160), (160, 90)] {
        let mut b = CaptionBurner::new(w, h, &[turn(0.0, 1.0, "Ação — ñ, ü, 日本語, 🎬")]).unwrap();
        let mut f = frame(w, h);
        b.blend_at(0.5, &mut f);
        assert!(!changed_rows(&f).is_empty(), "{w}x{h}");
    }
}

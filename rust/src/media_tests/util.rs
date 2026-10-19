//! Conferência dos arquivos produzidos: lê os pacotes de volta pelo próprio
//! FFmpeg (o `probe` sozinho não diria se um pacote sumiu ou repetiu) e mede
//! PNG/JPEG pelo cabeçalho.

use std::path::Path;

use sha1::{Digest, Sha1};

use crate::ffi::ffmpeg::*;
use crate::probe::{self, ProbeReport};

#[derive(Debug, Clone)]
pub struct Pk {
    pub stream: i32,
    pub kind: i32,
    pub pts: f64,
    pub key: bool,
    /// SHA-1 dos bytes do pacote: identidade, pra conferir cópia sem perda.
    pub hash: [u8; 20],
}

/// Todos os pacotes do arquivo, na ordem de leitura.
pub fn packets(path: &str) -> Vec<Pk> {
    let a = api().expect("ffmpeg");
    let input = Input::open(path).expect("abrir");
    assert!(input.find_stream_info());
    let mut out = Vec::new();
    let mut pkt = Packet::alloc().unwrap();
    unsafe {
        while (a.format.av_read_frame)(input.ptr(), pkt.ptr()) >= 0 {
            let p = &*pkt.ptr();
            let st = input.stream(p.stream_index as u32).unwrap();
            let tb = st.time_base;
            let kind = (*st.codecpar).codec_type;
            let data = std::slice::from_raw_parts(p.data, p.size as usize);
            out.push(Pk {
                stream: p.stream_index,
                kind,
                pts: if p.pts == AV_NOPTS_VALUE { f64::NAN } else { p.pts as f64 * tb.num as f64 / tb.den as f64 },
                key: p.flags & AV_PKT_FLAG_KEY != 0,
                hash: Sha1::digest(data).into(),
            });
            pkt.unref();
        }
    }
    out
}

pub fn of_kind(pks: &[Pk], kind: i32) -> Vec<Pk> {
    pks.iter().filter(|p| p.kind == kind).cloned().collect()
}

pub fn of_stream(pks: &[Pk], stream: i32) -> Vec<Pk> {
    pks.iter().filter(|p| p.stream == stream).cloned().collect()
}

pub fn hashes(pks: &[Pk]) -> Vec<[u8; 20]> {
    pks.iter().map(|p| p.hash).collect()
}

pub fn probe(path: &str) -> ProbeReport {
    probe::probe(Path::new(path)).unwrap_or_else(|| panic!("probe falhou: {path}"))
}

/// `|a - b| <= tol`, com mensagem.
pub fn near(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} != {b} (±{tol})");
}

/// (largura, altura) do IHDR.
pub fn png_size(path: &str) -> (u32, u32) {
    let b = std::fs::read(path).unwrap();
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n", "não é PNG");
    (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()))
}

/// (largura, altura) do SOF do JPEG.
pub fn jpeg_size(path: &str) -> (u32, u32) {
    let b = std::fs::read(path).unwrap();
    assert_eq!(&b[..2], b"\xFF\xD8", "não é JPEG");
    let mut i = 2;
    while i + 9 < b.len() {
        assert_eq!(b[i], 0xFF, "marcador quebrado em {i}");
        let m = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if (0xC0..=0xC3).contains(&m) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return (w, h);
        }
        i += 2 + len;
    }
    panic!("JPEG sem SOF");
}

/// Copia uma amostra pra um nome novo na pasta de saída.
pub fn copy_as(fixture: &str, name: &str) -> String {
    let dst = super::out(name);
    std::fs::copy(fixture, &dst).unwrap();
    dst
}

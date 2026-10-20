//! Mixer: a cada bloco de 1024 amostras do relógio, monta as faixas. Cada
//! fonte tem a LISTA das faixas em que entra (índice 0 = a mistura; os outros
//! = isoladas, na ordem do `audio_tracks::compute_audio_track_assignments`).
//! Lista e não máscara de bits: o número de faixas não tem teto (a libobs
//! parava em 6, o `MAX_AUDIO_MIXES`).
//!
//! Referência: `libobs/obs-audio.c` (`audio_callback`/`mix_audio`) e
//! `media-io/audio-io.c`. Sem a deduplicação de monitoramento nem o
//! "buffering" dinâmico: o mixer anda com um atraso FIXO atrás do relógio
//! (`MIX_DELAY_NS`), tempo de sobra pro WASAPI entregar os pacotes.

use super::audio::{AudioSource, CHANNELS};

/// Amostras por bloco (= `frame_size` do AAC).
pub const BLOCK_FRAMES: usize = 1024;
/// Quanto o mixer anda atrás do relógio.
pub const MIX_DELAY_NS: u64 = 150_000_000;

/// Uma fonte e as faixas em que ela entra.
pub struct MixInput {
    pub source: std::sync::Arc<dyn AudioSource>,
    /// Índices das faixas (0 = mistura) em que a fonte entra.
    pub tracks: Vec<usize>,
}

pub struct AudioMixer {
    inputs: Vec<MixInput>,
    tracks: usize,
    tmp: Vec<f32>,
}

impl AudioMixer {
    pub fn new(inputs: Vec<MixInput>, tracks: usize) -> Self {
        Self { inputs, tracks, tmp: vec![0.0; BLOCK_FRAMES * CHANNELS] }
    }

    pub fn track_count(&self) -> usize {
        self.tracks
    }

    pub fn set_muted(&self, name: &str, muted: bool) -> bool {
        let mut found = false;
        for i in self.inputs.iter().filter(|i| i.source.name() == name) {
            i.source.set_muted(muted);
            found = true;
        }
        found
    }

    /// Mistura o bloco que começa em `t0_ns`: uma saída intercalada por faixa.
    /// Cada fonte é lida UMA vez (a leitura consome o anel) e somada em
    /// todas as faixas dela.
    pub fn mix(&mut self, t0_ns: u64, out: &mut [Vec<f32>]) {
        for o in out.iter_mut() {
            o.clear();
            o.resize(BLOCK_FRAMES * CHANNELS, 0.0);
        }
        for input in &self.inputs {
            self.tmp.fill(0.0);
            input.source.mix_into(t0_ns, &mut self.tmp);
            for &t in input.tracks.iter().filter(|&&t| t < self.tracks) {
                for (d, s) in out[t].iter_mut().zip(&self.tmp) {
                    *d += *s;
                }
            }
        }
    }
}

/// Início do bloco `k` a partir de `start_ns` (conta inteira, sem acumular erro).
pub fn block_start_ns(start_ns: u64, k: u64) -> u64 {
    start_ns + (k as u128 * BLOCK_FRAMES as u128 * 1_000_000_000 / super::audio::SAMPLE_RATE as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Fonte de teste: valor constante, conta as leituras.
    struct Const {
        name: String,
        v: f32,
        reads: Mutex<u32>,
        muted: std::sync::atomic::AtomicBool,
    }
    impl AudioSource for Const {
        fn name(&self) -> &str {
            &self.name
        }
        fn mix_into(&self, _t0: u64, out: &mut [f32]) {
            *self.reads.lock().unwrap() += 1;
            if !self.muted.load(std::sync::atomic::Ordering::Relaxed) {
                for s in out.iter_mut() {
                    *s += self.v;
                }
            }
        }
        fn set_muted(&self, m: bool) {
            self.muted.store(m, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn src(name: &str, v: f32) -> std::sync::Arc<dyn AudioSource> {
        std::sync::Arc::new(Const { name: name.into(), v, reads: Mutex::new(0), muted: Default::default() })
    }

    #[test]
    fn mistura_e_isoladas() {
        // Faixa 0 = mistura dos dois; faixa 1 = mic; faixa 2 = sistema.
        let mut m = AudioMixer::new(vec![MixInput { source: src("mic", 0.25), tracks: vec![0, 1] }, MixInput { source: src("sys", 0.5), tracks: vec![0, 2] }], 3);
        let mut out = vec![Vec::new(), Vec::new(), Vec::new()];
        m.mix(0, &mut out);
        assert!(out[0].iter().all(|&s| s == 0.75));
        assert!(out[1].iter().all(|&s| s == 0.25));
        assert!(out[2].iter().all(|&s| s == 0.5));
        assert_eq!(out[0].len(), BLOCK_FRAMES * CHANNELS);
    }

    #[test]
    fn mudo_por_nome() {
        let mut m = AudioMixer::new(vec![MixInput { source: src("mic", 0.25), tracks: vec![0] }, MixInput { source: src("sys", 0.5), tracks: vec![0] }], 1);
        assert!(m.set_muted("mic", true));
        assert!(!m.set_muted("nada", true));
        let mut out = vec![Vec::new()];
        m.mix(0, &mut out);
        assert!(out[0].iter().all(|&s| s == 0.5));
    }

    #[test]
    fn blocos_sem_erro_acumulado() {
        // 48000 / 1024 não é inteiro: o bloco 46875 cai EXATAMENTE em 1000 s.
        assert_eq!(block_start_ns(0, 46_875), 1_000_000_000_000);
        assert_eq!(block_start_ns(10, 1), 10 + 21_333_333);
    }
}

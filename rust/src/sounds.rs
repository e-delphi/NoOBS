//! Sons de começar e parar a gravação — o `RecordingSounds` da versão web
//! (Web Audio), refeito com as mesmas notas e o mesmo envelope:
//! C5 (523,25 Hz) e G5 (783,99 Hz), quinta justa. Começar = C5→G5 (sobe),
//! parar = G5→C5 (desce). Cada nota: ataque de 5 ms até o volume, cai pra 70%
//! em 50 ms, e solta linear até zero no fim; a 2ª começa 100 ms depois da 1ª.
//! Volume 0,12.
//!
//! O WAV é montado uma vez e tocado da memória (`PlaySoundW` + `SND_MEMORY` +
//! `SND_ASYNC`): sem arquivo no disco e sem travar quem chama.

use std::sync::OnceLock;

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

const RATE: u32 = 48_000;
const VOLUME: f32 = 0.12;

/// Uma nota com o envelope da web, somada em `buf` a partir de `start_s`.
fn add_note(buf: &mut [f32], freq: f32, start_s: f32, dur_s: f32) {
    let start = (start_s * RATE as f32) as usize;
    // `osc.stop(start + duration + 0.02)`: 20 ms de folga já em silêncio.
    let n = ((dur_s + 0.02) * RATE as f32) as usize;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = if t < 0.005 {
            VOLUME * t / 0.005
        } else if t < 0.050 {
            VOLUME * (1.0 - 0.30 * (t - 0.005) / 0.045)
        } else if t < dur_s {
            VOLUME * 0.70 * (1.0 - (t - 0.050) / (dur_s - 0.050))
        } else {
            0.0
        };
        if let Some(s) = buf.get_mut(start + i) {
            *s += env * (2.0 * std::f32::consts::PI * freq * t).sin();
        }
    }
}

/// WAV PCM 16 bits mono com as duas notas.
fn build(first: f32, second: f32) -> Vec<u8> {
    let total_s = 0.10 + 0.20 + 0.03;
    let mut buf = vec![0.0f32; (total_s * RATE as f32) as usize];
    add_note(&mut buf, first, 0.0, 0.16);
    add_note(&mut buf, second, 0.10, 0.20);
    let data_len = (buf.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in buf {
        w.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    w
}

const C5: f32 = 523.25;
const G5: f32 = 783.99;

fn play(wav: &'static [u8]) {
    #[cfg(test)]
    {
        let _ = wav;
        return;
    }
    #[allow(unreachable_code)]
    unsafe {
        // Com SND_MEMORY o "nome" é o ponteiro pros bytes do WAV.
        let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

/// C5 → G5: sobe, "abrir".
pub fn play_start() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    play(WAV.get_or_init(|| build(C5, G5)));
}

/// G5 → C5: desce, "fechar".
pub fn play_stop() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    play(WAV.get_or_init(|| build(G5, C5)));
}

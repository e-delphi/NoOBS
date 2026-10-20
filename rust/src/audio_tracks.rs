//! Atribuição de faixas de áudio (porte de `OBSAudioTracks.pas`, SEM o teto
//! de 6 faixas da libobs — `MAX_AUDIO_MIXES`).
//!
//! Sem estado. `compute_audio_track_assignments` é a "single source of truth"
//! da ordem: tanto a gravação real quanto a lista que vai pra UI a chamam; o
//! JS não reimplementa nada.

use crate::lang;
use crate::types::AudioDev;

/// Faixa de cada mic/saída (0 = desmarcado: só religa na mistura) e o total
/// usado (inclui o Mix).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackAssignment {
    pub mic_tracks: Vec<i32>,
    pub out_tracks: Vec<i32>,
    pub total_tracks: i32,
}

/// Ordem física das faixas no MKV:
/// 1 = Mix · 2 = mic padrão · 3 = saída padrão · depois os outros mics e as
/// outras saídas, cada um na SUA faixa. Sem teto: o MKV guarda quantas faixas
/// houver, e o mixer soma por lista (a libobs parava em 6 e agrupava o
/// excesso numa faixa só).
pub fn compute_audio_track_assignments(
    mic_enabled: &[bool],
    mic_default: &[bool],
    out_enabled: &[bool],
    out_default: &[bool],
) -> TrackAssignment {
    let mut mic_tracks = vec![0i32; mic_enabled.len()];
    let mut out_tracks = vec![0i32; out_enabled.len()];
    let default_mic = (0..mic_enabled.len()).find(|&j| mic_enabled[j] && mic_default.get(j).copied().unwrap_or(false));
    let default_out = (0..out_enabled.len()).find(|&j| out_enabled[j] && out_default.get(j).copied().unwrap_or(false));

    let mut next = 2;
    let mut take = || {
        let t = next;
        next += 1;
        t
    };
    if let Some(j) = default_mic {
        mic_tracks[j] = take();
    }
    if let Some(j) = default_out {
        out_tracks[j] = take();
    }
    for j in 0..mic_enabled.len() {
        if mic_enabled[j] && Some(j) != default_mic {
            mic_tracks[j] = take();
        }
    }
    for j in 0..out_enabled.len() {
        if out_enabled[j] && Some(j) != default_out {
            out_tracks[j] = take();
        }
    }
    TrackAssignment { mic_tracks, out_tracks, total_tracks: next - 1 }
}

/// Nomes humanos das faixas (índice 0 = Mix). Viram o `title` da stream no MKV,
/// no idioma ativo na hora da gravação.
pub fn build_track_names(
    total_tracks: i32,
    mics: &[AudioDev],
    outputs: &[AudioDev],
    mic_tracks: &[i32],
    out_tracks: &[i32],
) -> Vec<String> {
    if total_tracks <= 0 {
        return Vec::new();
    }
    let n = total_tracks as usize;
    let mut result: Vec<String> = (1..=n).map(|k| lang::t_with("tracks.track", &[("n", &k.to_string())])).collect();
    result[0] = lang::t("tracks.mixName");
    let named = mics.iter().zip(mic_tracks).chain(outputs.iter().zip(out_tracks));
    for (dev, &track) in named {
        if track >= 2 && (track as usize) <= n {
            result[track as usize - 1] = dev.name.clone();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_primeiro() {
        let a = compute_audio_track_assignments(&[true, true], &[false, true], &[true], &[true]);
        // 2 = mic padrão, 3 = saída padrão (sempre antes dos outros), 4 = o outro mic.
        assert_eq!(a.mic_tracks, vec![4, 2]);
        assert_eq!(a.out_tracks, vec![3]);
        assert_eq!(a.total_tracks, 4);
    }

    #[test]
    fn desabilitado_fica_so_no_mix() {
        let a = compute_audio_track_assignments(&[false], &[true], &[], &[]);
        assert_eq!(a.mic_tracks, vec![0]);
        assert_eq!(a.total_tracks, 1);
    }

    #[test]
    fn sem_teto_cada_dispositivo_na_sua_faixa() {
        // 1 mic padrão + 9 saídas: 11 faixas (a libobs agrupava a partir da 6ª).
        let outs = vec![true; 9];
        let a = compute_audio_track_assignments(&[true], &[true], &outs, &[false; 9]);
        assert_eq!(a.mic_tracks, vec![2]);
        assert_eq!(a.out_tracks, (3..=11).collect::<Vec<_>>());
        assert_eq!(a.total_tracks, 11);
    }

    #[test]
    fn nomes_pela_faixa() {
        let d = |n: &str| AudioDev { name: n.into(), device_id: n.into() };
        let a = compute_audio_track_assignments(&[true], &[true], &[true, false], &[false, false]);
        let n = build_track_names(a.total_tracks, &[d("Mic")], &[d("Fone"), d("HDMI")], &a.mic_tracks, &a.out_tracks);
        assert_eq!(n.len(), 3);
        assert_eq!(&n[1..], ["Mic", "Fone"]);
    }
}

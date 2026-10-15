//! Atribuição de faixas de áudio e enumeração de dispositivos via libobs
//! (porte de `OBSAudioTracks.pas`).
//!
//! Sem estado. `compute_audio_track_assignments` é a "single source of truth"
//! do agrupamento: tanto a gravação real quanto a lista que vai pra UI a
//! chamam; o JS não reimplementa nada.

use std::ffi::CString;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::ffi::cstr_to_string;
use crate::ffi::obs::api;
use crate::types::ObsAudioDev;
use crate::{lang, log};

pub fn count_true(a: &[bool]) -> usize {
    a.iter().filter(|b| **b).count()
}

/// Lê a lista `device_id` da fonte `kind` (`wasapi_input_capture` /
/// `wasapi_output_capture`). Pegadinha #24: contagem unsigned zerada não
/// underflowa em Rust (`0..0` é vazio), mas o `return` antecipado fica.
fn enumerate_raw(kind: &str) -> Vec<ObsAudioDev> {
    let Ok(a) = api() else { return Vec::new() };
    let kind_c = CString::new(kind).unwrap_or_default();
    let key = CString::new("device_id").unwrap_or_default();
    let mut out = Vec::new();
    unsafe {
        let props = (a.obs_get_source_properties)(kind_c.as_ptr());
        if props.is_null() {
            return out;
        }
        let prop = (a.obs_properties_get)(props, key.as_ptr());
        if !prop.is_null() {
            let count = (a.obs_property_list_item_count)(prop);
            for i in 0..count {
                // Strings do libobs são UTF-8 (pegadinha #29).
                let name = cstr_to_string((a.obs_property_list_item_name)(prop, i));
                let value = cstr_to_string((a.obs_property_list_item_string)(prop, i));
                if value == "default" {
                    continue;
                }
                out.push(ObsAudioDev { name, device_id: value });
            }
        }
        (a.obs_properties_destroy)(props);
    }
    out
}

/// Enumera com timeout de 3 s: o `get_properties` do win-wasapi chama o WASAPI
/// do Windows e, com o serviço de áudio doente, pode travar 60 s+. Roda em
/// worker (com COM MTA, pedido pelo `CoCreateInstance` do plugin) enquanto a
/// main espera; sem acesso concorrente ao libobs, pois ela fica bloqueada.
/// Estourou o prazo → lista vazia e a thread fica vazada de propósito.
pub fn enumerate_obs_audio_devices(kind: &str) -> Vec<ObsAudioDev> {
    const TIMEOUT: Duration = Duration::from_millis(3000);
    let t0 = Instant::now();
    log!("   enum {kind}: iniciando (timeout={}ms)...", TIMEOUT.as_millis());
    let (tx, rx) = mpsc::channel();
    let kind_owned = kind.to_owned();
    std::thread::spawn(move || {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
        let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        let r = std::panic::catch_unwind(|| enumerate_raw(&kind_owned)).unwrap_or_default();
        if com_ok {
            unsafe { CoUninitialize() };
        }
        let _ = tx.send(r);
    });
    match rx.recv_timeout(TIMEOUT) {
        Ok(list) => {
            log!("   enum {kind}: {} device(s) em {}ms.", list.len(), t0.elapsed().as_millis());
            list
        }
        Err(_) => {
            log!("   enum {kind}: TIMEOUT apos {}ms — sem audio (WASAPI travado).", t0.elapsed().as_millis());
            Vec::new()
        }
    }
}

/// Faixa de cada mic/saída (0 = só Mix) e o total usado (inclui o Mix).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackAssignment {
    pub mic_tracks: Vec<i32>,
    pub out_tracks: Vec<i32>,
    pub total_tracks: i32,
}

/// Ordem física das faixas no MKV:
/// 1 = Mix · 2 = mic default · 3 = saída default · depois mics individuais,
/// saídas individuais · a faixa AGRUPADA (2+ devices) vai sempre pro fim.
/// Defaults ficam individuais; o agrupamento só atinge os não-default.
/// `reserved_slots`: faixas isoladas que os devices não podem usar (som do
/// app na gravação de janela, pegadinha #66) — ficam depois da última devolvida.
pub fn compute_audio_track_assignments(
    mic_enabled: &[bool],
    mic_default: &[bool],
    out_enabled: &[bool],
    out_default: &[bool],
    reserved_slots: i32,
) -> TrackAssignment {
    const ISOLATED_SLOTS: i32 = 5; // faixas 2..6
    let mut mic_tracks = vec![0i32; mic_enabled.len()];
    let mut out_tracks = vec![0i32; out_enabled.len()];

    let enabled_mics = count_true(mic_enabled) as i32;
    let enabled_outs = count_true(out_enabled) as i32;

    let default_mic = (0..mic_enabled.len()).find(|&j| mic_enabled[j] && mic_default[j]);
    let default_out = (0..out_enabled.len()).find(|&j| out_enabled[j] && out_default[j]);

    let other_mics = enabled_mics - default_mic.is_some() as i32;
    let other_outs = enabled_outs - default_out.is_some() as i32;
    let mut remaining = ISOLATED_SLOTS - reserved_slots;
    if default_mic.is_some() {
        remaining -= 1;
    }
    if default_out.is_some() {
        remaining -= 1;
    }
    let other_total = other_mics + other_outs;

    let mut group_other_mics = false;
    let mut output_group_size = 0;
    if remaining > 0 {
        if other_total > remaining {
            let surplus = other_total - remaining;
            if other_outs > surplus {
                // Agrupa só o excesso+1 saídas (poupa exatamente `surplus`).
                output_group_size = surplus + 1;
            } else {
                output_group_size = other_outs;
                if other_mics + 1 > remaining {
                    group_other_mics = true;
                }
            }
        }
    } else {
        // Sem slot pra nenhum não-default: agrupa tudo o que sobrou.
        if other_outs > 0 {
            output_group_size = other_outs;
        }
        if other_mics > 0 {
            group_other_mics = true;
        }
    }

    let mut next = 2;
    if let Some(j) = default_mic {
        mic_tracks[j] = next;
        next += 1;
    }
    if let Some(j) = default_out {
        out_tracks[j] = next;
        next += 1;
    }

    if other_mics > 0 {
        if group_other_mics {
            let group = next;
            next += 1;
            for j in 0..mic_enabled.len() {
                if Some(j) != default_mic && mic_enabled[j] {
                    mic_tracks[j] = group;
                }
            }
        } else {
            for j in 0..mic_enabled.len() {
                if Some(j) != default_mic && mic_enabled[j] {
                    mic_tracks[j] = next;
                    next += 1;
                }
            }
        }
    }

    if other_outs > 0 {
        if output_group_size > 0 {
            let group = next;
            next += 1;
            let mut grouped = 0;
            for j in 0..out_enabled.len() {
                if Some(j) != default_out && out_enabled[j] && grouped < output_group_size {
                    out_tracks[j] = group;
                    grouped += 1;
                }
            }
            for j in 0..out_enabled.len() {
                if Some(j) != default_out && out_enabled[j] && out_tracks[j] == 0 {
                    out_tracks[j] = next;
                    next += 1;
                }
            }
        } else {
            for j in 0..out_enabled.len() {
                if Some(j) != default_out && out_enabled[j] {
                    out_tracks[j] = next;
                    next += 1;
                }
            }
        }
    }

    let total_tracks = (next - 1).min(6);

    // Pós-processamento: faixas agrupadas ao FINAL (individuais 2..N, grupos N+1..).
    let mut count = [0i32; 7];
    for t in mic_tracks.iter().chain(out_tracks.iter()) {
        if (2..=6).contains(t) {
            count[*t as usize] += 1;
        }
    }
    let individuals: Vec<usize> = (2..=6).filter(|&t| count[t] == 1).collect();
    let groups: Vec<usize> = (2..=6).filter(|&t| count[t] >= 2).collect();
    if !groups.is_empty() {
        let mut remap: [i32; 7] = [0, 1, 2, 3, 4, 5, 6];
        let mut new_track = 2;
        for t in individuals.iter().chain(groups.iter()) {
            remap[*t] = new_track;
            new_track += 1;
        }
        for t in mic_tracks.iter_mut().chain(out_tracks.iter_mut()) {
            if *t > 0 {
                *t = remap[*t as usize];
            }
        }
    }

    TrackAssignment { mic_tracks, out_tracks, total_tracks }
}

/// Nomes humanos das faixas (índice 0 = Mix). Viram o `title` da stream no MKV,
/// no idioma ativo na hora da gravação.
pub fn build_track_names(
    total_tracks: i32,
    mics: &[ObsAudioDev],
    outputs: &[ObsAudioDev],
    mic_tracks: &[i32],
    out_tracks: &[i32],
) -> Vec<String> {
    if total_tracks <= 0 {
        return Vec::new();
    }
    let n = total_tracks as usize;
    let mut result = vec![String::new(); n];
    result[0] = lang::t("tracks.mixName");

    // track_devs[k] = devices da faixa k+1; kind: 0 nenhum, 1 mic, 2 saída, 3 misto.
    let mut track_devs: Vec<Vec<&str>> = vec![Vec::new(); n];
    let mut kind = vec![0u8; n];
    for (j, m) in mics.iter().enumerate() {
        let idx = mic_tracks.get(j).copied().unwrap_or(0) - 1;
        if idx >= 1 && (idx as usize) < n {
            let i = idx as usize;
            track_devs[i].push(&m.name);
            kind[i] = match kind[i] { 0 => 1, 2 => 3, k => k };
        }
    }
    for (j, o) in outputs.iter().enumerate() {
        let idx = out_tracks.get(j).copied().unwrap_or(0) - 1;
        if idx >= 1 && (idx as usize) < n {
            let i = idx as usize;
            track_devs[i].push(&o.name);
            kind[i] = match kind[i] { 0 => 2, 1 => 3, k => k };
        }
    }

    for j in 1..n {
        result[j] = match track_devs[j].len() {
            0 => lang::t_with("tracks.track", &[("n", &(j + 1).to_string())]),
            1 => track_devs[j][0].to_owned(),
            count => {
                let combined = track_devs[j].join(" + ");
                if combined.chars().count() <= 80 {
                    combined
                } else {
                    let key = match kind[j] {
                        1 => "tracks.micsGroup",
                        2 => "tracks.outputsGroup",
                        _ => "tracks.groupedAudio",
                    };
                    lang::t_with(key, &[("count", &count.to_string())])
                }
            }
        };
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_primeiro() {
        let a = compute_audio_track_assignments(&[true, true], &[false, true], &[true], &[true], 0);
        assert_eq!(a.mic_tracks, vec![3, 2]);
        assert_eq!(a.out_tracks, vec![4]);
        assert_eq!(a.total_tracks, 4);
    }

    #[test]
    fn desabilitado_fica_so_no_mix() {
        let a = compute_audio_track_assignments(&[false], &[true], &[], &[], 0);
        assert_eq!(a.mic_tracks, vec![0]);
        assert_eq!(a.total_tracks, 1);
    }

    #[test]
    fn excesso_agrupa_saidas_no_fim() {
        // 1 mic default + 6 saídas: 4 slots livres, grupo de 3 saídas na última.
        let outs = vec![true; 6];
        let a = compute_audio_track_assignments(&[true], &[true], &outs, &vec![false; 6], 0);
        assert_eq!(a.mic_tracks, vec![2]);
        assert_eq!(a.total_tracks, 6);
        assert_eq!(a.out_tracks.iter().filter(|t| **t == 6).count(), 2);
        assert!(a.out_tracks.iter().all(|t| (2..=6).contains(t)));
    }

    #[test]
    fn slot_reservado_ao_app() {
        let a = compute_audio_track_assignments(&[true], &[true], &[], &[], 1);
        assert_eq!(a.total_tracks, 2);
    }
}

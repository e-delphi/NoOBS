//! Uso da placa de vídeo POR MOTOR pelos contadores do Windows (PDH,
//! `\GPU Engine(*)\Utilization Percentage`) — porte de `WinGpuUsage.pas`.
//!
//! Serve ao monitor de desempenho: com o buffer ou a gravação ligados, uma
//! linha no log a cada 30 s diz quanto o JOGO usa da placa (motor 3D) e
//! quanto o NoOBS usa (motor de codificação). É o que separa "o buffer pesou"
//! de "a placa já estava no limite".
//!
//! Cada instância é um par processo × motor
//! (`pid_<pid>_luid_<adaptador>_phys_0_eng_<n>_engtype_<tipo>`). Somamos por
//! (adaptador, tipo) e mostramos o adaptador mais ocupado. O nome do tipo é
//! do DRIVER (AMD: "Video Codec Engine"; NVIDIA: "VideoEncode"), então a linha
//! lista os mais usados pelo nome que vier.
//!
//! O contador é uma TAXA: cada leitura é a média desde a anterior. ~1,5 ms
//! com ~400 instâncias — barato pra main thread a cada 30 s.

use std::cell::Cell;
use std::collections::HashMap;

use windows::core::w;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    PDH_FMT, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
};

use crate::log;

/// A soma de motores pode passar de 100 (o crate não exporta este flag).
const PDH_FMT_NOCAP100: PDH_FMT = PDH_FMT(0x0000_8000);
/// Motores mostrados na linha.
const MAX_ENGINES: usize = 4;
/// Abaixo disso não entra.
const MIN_PCT: f64 = 1.0;

thread_local! {
    static QUERY: Cell<isize> = const { Cell::new(0) };
    static COUNTER: Cell<isize> = const { Cell::new(0) };
    static FAILED: Cell<bool> = const { Cell::new(false) };
}

/// Abre a consulta e faz a 1ª leitura (a base da média). Idempotente.
pub fn start() {
    if QUERY.with(Cell::get) != 0 || FAILED.with(Cell::get) {
        return;
    }
    unsafe {
        let mut q = PDH_HQUERY::default();
        let r = PdhOpenQueryW(None, 0, &mut q);
        if r != 0 {
            FAILED.with(|f| f.set(true));
            log!("GpuUsage: PdhOpenQuery falhou (${r:x}).");
            return;
        }
        let mut c = PDH_HCOUNTER::default();
        let r = PdhAddEnglishCounterW(q, w!("\\GPU Engine(*)\\Utilization Percentage"), 0, &mut c);
        if r != 0 {
            // Windows anterior ao 10 1709 ou driver que não publica: o monitor
            // segue sem esta parte.
            log!("GpuUsage: contador de GPU indisponivel (${r:x}).");
            let _ = PdhCloseQuery(q);
            FAILED.with(|f| f.set(true));
            return;
        }
        let _ = PdhCollectQueryData(q);
        QUERY.with(|x| x.set(q.0 as isize));
        COUNTER.with(|x| x.set(c.0 as isize));
    }
}

pub fn stop() {
    let q = QUERY.with(|x| x.replace(0));
    COUNTER.with(|x| x.set(0));
    if q != 0 {
        unsafe {
            let _ = PdhCloseQuery(PDH_HQUERY(q as *mut _));
        }
    }
}

/// `pid`, adaptador e tipo de motor do nome da instância.
fn parse_instance(name: &str) -> Option<(u32, &str, &str)> {
    let rest = name.strip_prefix("pid_")?;
    let (pid, rest) = rest.split_once("_luid_")?;
    let (luid, _) = rest.split_once("_phys_")?;
    let (_, eng) = name.split_once("_engtype_")?;
    if eng.is_empty() {
        return None;
    }
    Some((pid.parse().ok()?, luid, eng))
}

/// Resume as instâncias: motores do adaptador mais ocupado, mais usados
/// primeiro, ex. "3D 87% (NoOBS 2%), Video Codec Engine 14% (NoOBS 14%)".
fn summarize(samples: &[(String, f64)], own_pid: u32) -> String {
    let mut total: HashMap<(&str, &str), f64> = HashMap::new();
    let mut own: HashMap<(&str, &str), f64> = HashMap::new();
    let mut by_luid: HashMap<&str, f64> = HashMap::new();
    for (name, v) in samples {
        let Some((pid, luid, eng)) = parse_instance(name) else { continue };
        // Uma instância é UM motor de UM processo: acima de 100% é leitura
        // quebrada (visto: 6149686312366% com status OK, durante um jogo).
        if !(0.0..=100.5).contains(v) {
            continue;
        }
        *total.entry((luid, eng)).or_default() += v;
        if pid == own_pid {
            *own.entry((luid, eng)).or_default() += v;
        }
        *by_luid.entry(luid).or_default() += v;
    }
    let Some((best, _)) = by_luid.iter().max_by(|a, b| a.1.total_cmp(b.1)) else { return String::new() };
    let mut rows: Vec<(&str, f64, f64)> = total
        .iter()
        .filter(|((l, _), t)| l == best && **t >= MIN_PCT)
        .map(|((_, e), t)| (*e, *t, own.get(&(*best, *e)).copied().unwrap_or(0.0)))
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    let s = rows
        .iter()
        .take(MAX_ENGINES)
        .map(|(e, t, o)| format!("{e} {t:.0}% (NoOBS {o:.0}%)"))
        .collect::<Vec<_>>()
        .join(", ");
    if s.is_empty() { "ociosa".into() } else { s }
}

/// Lê o intervalo desde a leitura anterior. Vazio se não há contadores.
pub fn sample(own_pid: u32) -> String {
    let (q, c) = (QUERY.with(Cell::get), COUNTER.with(Cell::get));
    if q == 0 {
        return String::new();
    }
    unsafe {
        if PdhCollectQueryData(PDH_HQUERY(q as *mut _)) != 0 {
            return String::new();
        }
        let counter = PDH_HCOUNTER(c as *mut _);
        let fmt = PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100.0);
        let (mut size, mut count) = (0u32, 0u32);
        if PdhGetFormattedCounterArrayW(counter, fmt, &mut size, &mut count, None) != PDH_MORE_DATA {
            return String::new();
        }
        // Buffer alinhado para os itens (nomes vêm no fim do mesmo bloco).
        let n_items = (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
        let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = vec![Default::default(); n_items.max(1)];
        if PdhGetFormattedCounterArrayW(counter, fmt, &mut size, &mut count, Some(buf.as_mut_ptr())) != 0 {
            return String::new();
        }
        let samples: Vec<(String, f64)> = buf[..count as usize]
            .iter()
            .filter(|it| it.FmtValue.CStatus == 0 && !it.szName.is_null())
            .map(|it| (it.szName.to_string().unwrap_or_default(), it.FmtValue.Anonymous.doubleValue))
            .collect();
        summarize(&samples, own_pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_da_instancia() {
        let n = "pid_1234_luid_0x00000000_0x0000D1E2_phys_0_eng_3_engtype_Video Codec Engine";
        assert_eq!(parse_instance(n), Some((1234, "0x00000000_0x0000D1E2", "Video Codec Engine")));
        assert_eq!(parse_instance("lixo"), None);
    }

    #[test]
    fn resumo_pega_o_adaptador_mais_ocupado_e_descarta_leitura_quebrada() {
        let s = vec![
            ("pid_1_luid_A_phys_0_eng_0_engtype_3D".to_string(), 80.0),
            ("pid_9_luid_A_phys_0_eng_1_engtype_VideoEncode".to_string(), 12.0),
            ("pid_1_luid_B_phys_0_eng_0_engtype_3D".to_string(), 5.0),
            ("pid_1_luid_A_phys_0_eng_0_engtype_3D".to_string(), 6.1e12),
        ];
        assert_eq!(summarize(&s, 9), "3D 80% (NoOBS 0%), VideoEncode 12% (NoOBS 12%)");
    }
}

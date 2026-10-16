//! Detecta se algum programa de uma lista está RODANDO, para ligar/desligar o
//! buffer em memória sozinho — porte de `WinProcWatch.pas` (pegadinha #62).
//!
//! Varre os processos (ToolHelp32) a cada 2 s e compara o nome do executável
//! com o filtro do usuário por "contém", sem caixa: `valorant` casa com
//! `VALORANT-Win64-Shipping.exe`. O callback roda NA THREAD do watcher e só na
//! mudança de estado — o consumidor marshala para a main (modo full) ou posta
//! mensagem para a janela (hibernação).
//!
//! Diferença deliberada para o watcher de mic: aqui a PRIMEIRA leitura também
//! dispara. "O jogo está aberto" é um ESTADO: abrir o NoOBS com o jogo já
//! rodando tem que ligar o buffer, e nada se perde ligando.
//!
//! Lista VAZIA = recurso desligado (o thread nem sobe). Aqui "vazio" não pode
//! ser "qualquer app", senão o buffer ligaria sempre.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use crate::log;

/// Um jogo abrindo leva bem mais que isso, e a varredura custa alguns ms.
const POLL: Duration = Duration::from_millis(2000);
/// Fatia do sono para o stop responder rápido no shutdown.
const SLICE: Duration = Duration::from_millis(100);

pub type Callback = fn(bool);

struct Watcher {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

static WATCHER: Mutex<Option<Watcher>> = Mutex::new(None);
/// Guardado ANTES das saídas antecipadas do `start`: com a lista vazia não há
/// thread, mas o `update_filter` (usuário digitando a lista com o app aberto)
/// precisa dele para subir — senão o recurso só valeria no próximo arranque
/// (a pegadinha #54 noutra forma).
static CALLBACK: Mutex<Option<Callback>> = Mutex::new(None);

/// Divide o filtro em nomes minúsculos (aceita `,` `;` e quebras de linha).
pub(crate) fn parse_apps(filter: &str) -> Vec<String> {
    filter
        .split([',', ';', '\r', '\n'])
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// `name` (já minúsculo) contém algum item? Lista vazia = NÃO casa.
pub(crate) fn app_in_list(name: &str, list: &[String]) -> bool {
    list.iter().any(|w| !w.is_empty() && name.contains(w.as_str()))
}

/// Nome (minúsculo) do próprio executável — o NoOBS nunca se monitora.
pub(crate) fn own_exe_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_default()
}

fn poll_running(wanted: &[String], own: &str) -> bool {
    if wanted.is_empty() {
        return false;
    }
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut pe = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                let len = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len());
                let name = String::from_utf16_lossy(&pe.szExeFile[..len]).to_lowercase();
                if name != own && app_in_list(&name, wanted) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        found
    }
}

/// Inicia (no-op se já rodando ou com a lista vazia).
pub fn start(apps_filter: &str, callback: Callback) {
    *CALLBACK.lock().unwrap_or_else(|p| p.into_inner()) = Some(callback);
    let mut w = WATCHER.lock().unwrap_or_else(|p| p.into_inner());
    if w.is_some() {
        return;
    }
    let apps = parse_apps(apps_filter);
    if apps.is_empty() {
        return;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let handle = std::thread::spawn(move || {
        let own = own_exe_name();
        let mut last: Option<bool> = None;
        while !stop2.load(Ordering::SeqCst) {
            let running = poll_running(&apps, &own);
            // Primeira leitura TAMBÉM dispara (ver o cabeçalho).
            if last != Some(running) {
                last = Some(running);
                callback(running);
            }
            let mut slept = Duration::ZERO;
            while slept < POLL && !stop2.load(Ordering::SeqCst) {
                std::thread::sleep(SLICE);
                slept += SLICE;
            }
        }
    });
    *w = Some(Watcher { stop, handle });
    log!("WinProcWatch: iniciado (monitorar=\"{apps_filter}\").");
}

/// Para e espera a thread. NÃO esquece o callback (o `update_filter` usa).
pub fn stop() {
    let w = WATCHER.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(w) = w {
        w.stop.store(true, Ordering::SeqCst);
        let _ = w.handle.join();
        log::log("WinProcWatch: parado.");
    }
}

/// Troca o filtro com o app aberto: para e sobe de novo com o callback guardado.
pub fn update_filter(apps_filter: &str) {
    // Snapshot ANTES do stop (pegadinha #54).
    let cb = *CALLBACK.lock().unwrap_or_else(|p| p.into_inner());
    stop();
    if let Some(cb) = cb {
        start(apps_filter, cb);
    }
}

pub fn is_running() -> bool {
    WATCHER.lock().unwrap_or_else(|p| p.into_inner()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtro_aceita_separadores_e_ignora_caixa() {
        assert_eq!(parse_apps(" Valorant ; cs2,\r\nTeams "), vec!["valorant", "cs2", "teams"]);
        assert!(parse_apps("  ,; ").is_empty());
    }

    #[test]
    fn contem_e_lista_vazia_nao_casa() {
        let l = parse_apps("valorant");
        assert!(app_in_list("valorant-win64-shipping.exe", &l));
        assert!(!app_in_list("notepad.exe", &l));
        assert!(!app_in_list("qualquer.exe", &[]));
    }
}

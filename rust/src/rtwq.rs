//! Plataforma RTWQ (Real-Time Work Queue) — porte de `OBSRtwq.pas`.
//!
//! O plugin `win-wasapi` agenda a entrega dos buffers de captura pela RTWQ
//! mas NÃO inicializa a plataforma: assume que o HOST fez isso (o frontend do
//! OBS chama `RtwqStartup` no topo do `main`). O NoOBS substitui o frontend,
//! então a responsabilidade é nossa. Sem isso: a 1ª gravação de cada sessão
//! sai sem o áudio do sistema e o `obs_shutdown` trava (pegadinhas #48/#31).
//!
//! `start_rtwq` ANTES do `CoInitialize` e de qualquer fonte WASAPI;
//! `stop_rtwq` por último, depois do `obs_shutdown`. `RtwqStartup` é
//! refcontado (par com `RtwqShutdown`), como o `CoInitialize`.

use libloading::Library;
use std::sync::Mutex;

use crate::log;

type RtwqFn = unsafe extern "system" fn() -> i32;

/// A DLL fica carregada enquanto a plataforma está ativa; `Some` também é a
/// guarda de idempotência.
static MODULE: Mutex<Option<Library>> = Mutex::new(None);

pub fn start_rtwq() {
    let mut guard = MODULE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_some() {
        return;
    }
    // Carrega de verdade (não só "procura o handle"): assim o GetModuleHandle
    // do plugin a enxerga e escolhe o caminho RTWQ, agora inicializado.
    let lib = match unsafe { Library::new("RTWorkQ.dll") } {
        Ok(l) => l,
        Err(e) => {
            // Sem RTWQ o plugin usa a thread de captura clássica: não é fatal.
            log!("RTWQ: RTWorkQ.dll indisponível ({e}) — win-wasapi usará a thread de captura.");
            return;
        }
    };
    let hr = match unsafe { lib.get::<RtwqFn>(b"RtwqStartup\0") } {
        Ok(f) => unsafe { f() },
        Err(_) => {
            log!("RTWQ: RtwqStartup ausente na DLL — abortando init da plataforma.");
            return;
        }
    };
    if hr < 0 {
        log!("RTWQ: RtwqStartup falhou (0x{:08x}).", hr as u32);
        return;
    }
    log!("RTWQ: plataforma inicializada (RtwqStartup ok).");
    *guard = Some(lib);
}

pub fn stop_rtwq() {
    let mut guard = MODULE.lock().unwrap_or_else(|p| p.into_inner());
    let Some(lib) = guard.take() else { return };
    if let Ok(f) = unsafe { lib.get::<RtwqFn>(b"RtwqShutdown\0") } {
        unsafe { f() };
    }
    drop(lib); // FreeLibrary
    log!("RTWQ: plataforma finalizada (RtwqShutdown).");
}

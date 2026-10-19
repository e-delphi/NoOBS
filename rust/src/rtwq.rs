//! Plataforma RTWQ (Real-Time Work Queue) — porte de `OBSRtwq.pas`.
//!
//! O plugin `win-wasapi` agenda a entrega dos buffers de captura pela RTWQ
//! mas NÃO inicializa a plataforma: assume que o HOST fez isso (o frontend do
//! OBS chama `RtwqStartup` no topo do `main`). O NoOBS substitui o frontend,
//! então a responsabilidade é nossa. Sem isso: a 1ª gravação de cada sessão
//! sai sem o áudio do sistema e o `obs_shutdown` trava (pegadinhas #48/#31).
//!
//! **Mas o NoOBS NÃO carrega a RTWQ por conta própria.** O plugin escolhe o
//! modelo de captura por fonte, no construtor, por `GetModuleHandle(RTWorkQ.dll)`:
//! sem a DLL no processo ele usa a thread de captura clássica, e o caminho RTWQ
//! do plugin tem uma corrida ao destruir a fonte — o `Stop()` volta antes de o
//! RTWQ terminar de liberar o item de trabalho, e o `Release` do callback
//! embutido (`--refCount`) escreve na fonte já apagada: heap corrompido,
//! travamento ou queda intermitentes depois de parar uma gravação. Medido: com
//! RTWQ, heap de depuração quebrou na 3ª/6ª rodada; sem, 20 de 20 limpas.
//!
//! E a DLL APARECE sozinha: a webcam (DirectShow → Media Foundation, `mfplat.dll`
//! importa a `RTWorkQ.dll`) a carrega — medido. Então [`hide_from_wasapi_plugin`]
//! desvia, só na tabela de importação do `win-wasapi.dll`, o `GetModuleHandleW`:
//! pra `"RTWorkQ.dll"` ele responde "não carregada" e o plugin usa a thread de
//! captura SEMPRE, com ou sem webcam. Se o desvio não puder ser instalado,
//! [`ensure_if_preloaded`] (chamado antes de criar cada fonte do plugin) sobe a
//! plataforma quando outra coisa já carregou a DLL: o plugin vai escolher o
//! caminho RTWQ, e sem a plataforma iniciada ele sai mudo e trava o
//! `obs_shutdown` (#48) — a corrida acima é o mal menor.
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

/// O desvio do `GetModuleHandleW` do `win-wasapi.dll` está instalado.
static HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `GetModuleHandleW` original (o endereço que estava na tabela de importação).
static REAL_GET_MODULE_HANDLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

type GetModuleHandleWFn = unsafe extern "system" fn(*const u16) -> isize;

/// O `GetModuleHandleW` que o `win-wasapi.dll` passa a chamar: igual ao do
/// Windows, menos pra `RTWorkQ.dll`, que fica "não carregada".
unsafe extern "system" fn get_module_handle_for_wasapi(name: *const u16) -> isize {
    if !name.is_null() {
        let mut n = 0usize;
        while unsafe { *name.add(n) } != 0 && n < 260 {
            n += 1;
        }
        let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name, n) });
        if s.eq_ignore_ascii_case("RTWorkQ.dll") || s.eq_ignore_ascii_case("RTWorkQ") {
            static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                log!("RTWQ: win-wasapi perguntou pela RTWorkQ.dll — respondido 'ausente' (captura pela thread do plugin).");
            }
            return 0;
        }
    }
    let real = REAL_GET_MODULE_HANDLE.load(std::sync::atomic::Ordering::SeqCst);
    if real == 0 {
        return 0;
    }
    let f: GetModuleHandleWFn = unsafe { std::mem::transmute(real) };
    unsafe { f(name) }
}

/// Depois de carregar o plugin `win-wasapi`: troca, na tabela de importação
/// DELE, o `GetModuleHandleW` pelo [`get_module_handle_for_wasapi`]. `true` =
/// instalado (o plugin nunca mais usa o caminho RTWQ). Idempotente.
pub fn hide_from_wasapi_plugin() -> bool {
    use std::sync::atomic::Ordering;
    if HIDDEN.load(Ordering::SeqCst) {
        return true;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualProtect(addr: *mut core::ffi::c_void, size: usize, new: u32, old: *mut u32) -> i32;
    }
    const PAGE_READWRITE: u32 = 0x04;
    let Ok(module) = (unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(windows::core::w!("win-wasapi.dll")) }) else {
        log!("RTWQ: win-wasapi.dll nao esta carregado — desvio nao instalado.");
        return false;
    };
    let base = module.0 as *const u8;
    // PE: DOS → NT → diretório de importação (índice 1 do DataDirectory).
    let ok = unsafe {
        let rd32 = |off: usize| std::ptr::read_unaligned(base.add(off) as *const u32) as usize;
        let nt = rd32(0x3c);
        let opt = nt + 24;
        if std::ptr::read_unaligned(base.add(opt) as *const u16) != 0x20b {
            false // não é PE32+
        } else {
            let imp = rd32(opt + 112 + 8);
            let mut found = false;
            let mut d = imp;
            while imp != 0 && rd32(d + 12) != 0 {
                let dll = std::ffi::CStr::from_ptr(base.add(rd32(d + 12)) as *const core::ffi::c_char).to_string_lossy();
                if dll.eq_ignore_ascii_case("KERNEL32.dll") {
                    let (names, slots) = (rd32(d), rd32(d + 16));
                    let mut i = 0usize;
                    loop {
                        let thunk = std::ptr::read_unaligned(base.add(names + i * 8) as *const u64);
                        if thunk == 0 {
                            break;
                        }
                        if thunk >> 63 == 0 {
                            let fname = std::ffi::CStr::from_ptr(base.add(thunk as usize + 2) as *const core::ffi::c_char);
                            if fname.to_bytes() == b"GetModuleHandleW" {
                                let slot = base.add(slots + i * 8) as *mut usize;
                                let mut old = 0u32;
                                if VirtualProtect(slot as *mut _, 8, PAGE_READWRITE, &mut old) != 0 {
                                    REAL_GET_MODULE_HANDLE.store(std::ptr::read_volatile(slot), Ordering::SeqCst);
                                    std::ptr::write_volatile(slot, get_module_handle_for_wasapi as *const () as usize);
                                    VirtualProtect(slot as *mut _, 8, old, &mut old);
                                    found = true;
                                }
                                break;
                            }
                        }
                        i += 1;
                    }
                }
                d += 20;
            }
            found
        }
    };
    if ok {
        HIDDEN.store(true, Ordering::SeqCst);
        log!("RTWQ: win-wasapi passa a ver a RTWorkQ.dll como ausente — captura sempre pela thread do plugin.");
    } else {
        log!("RTWQ: nao achei o GetModuleHandleW na importacao do win-wasapi — desvio nao instalado (fica o RtwqStartup sob demanda).");
    }
    ok
}

/// Antes de criar uma fonte do `win-wasapi`: se a `RTWorkQ.dll` já está no
/// processo (carregada por outro caminho), o plugin vai usar o caminho RTWQ —
/// então a plataforma tem que estar de pé (#48). Sem a DLL, não faz nada: o
/// plugin usa a thread de captura, que não tem a corrida ao destruir a fonte.
pub fn ensure_if_preloaded() {
    // Com o desvio instalado o plugin nem enxerga a DLL: nada a fazer.
    if HIDDEN.load(std::sync::atomic::Ordering::SeqCst) || MODULE.lock().unwrap_or_else(|p| p.into_inner()).is_some() {
        return;
    }
    let loaded = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(windows::core::w!("RTWorkQ.dll")) }.is_ok();
    if loaded {
        log!("RTWQ: RTWorkQ.dll ja carregada por outro modulo — iniciando a plataforma (o win-wasapi vai usar o caminho RTWQ).");
        start_rtwq();
    }
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

//! File watcher da pasta de gravações pela API nativa do Windows
//! (`FindFirstChangeNotification`) — porte de `OBSRecordWatch.pas`. Dispara o
//! callback quando o usuário adiciona/exclui/renomeia arquivo pelo Explorer (ou
//! outro app) e o NoOBS atualiza a lista sozinho.
//!
//! Filtra só `FILE_NAME`/`DIR_NAME` — não reage a escrita durante a gravação
//! (seria `FILE_NOTIFY_CHANGE_SIZE`). Thread própria com `WaitForMultipleObjects`
//! (stop + mudança): o stop responde em ~ms porque sinaliza o evento.
//!
//! O watcher NÃO é recursivo: ao navegar para uma subpasta o Bridge re-aponta
//! com [`update_dir`].

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_DIR_NAME,
    FILE_NOTIFY_CHANGE_FILE_NAME,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE};

use crate::{log, mainthread};

/// Roda na main thread, via `mainthread::queue`.
pub type Callback = Arc<dyn Fn() + Send + Sync>;

struct Watch {
    stop_event: HANDLE,
    thread: Option<JoinHandle<()>>,
}
// O HANDLE do evento é só sinalizado de outra thread.
unsafe impl Send for Watch {}

struct State {
    watch: Option<Watch>,
    callback: Option<Callback>,
}

static STATE: Mutex<State> = Mutex::new(State { watch: None, callback: None });

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

fn spawn(dir: String, callback: Callback) -> Option<Watch> {
    if dir.is_empty() || !std::path::Path::new(&dir).is_dir() {
        return None;
    }
    let stop_event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.ok()?;
    let stop_raw = stop_event.0 as isize;
    let thread = std::thread::spawn(move || {
        let stop = HANDLE(stop_raw as *mut _);
        let wide: Vec<u16> = dir.encode_utf16().chain(std::iter::once(0)).collect();
        let change = match unsafe {
            FindFirstChangeNotificationW(PCWSTR(wide.as_ptr()), false, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME)
        } {
            Ok(h) if !h.is_invalid() => h,
            _ => {
                log!("OBSRecordWatch: FindFirstChangeNotification falhou em {dir}");
                return;
            }
        };
        log!("OBSRecordWatch: monitorando {dir}");
        let handles = [stop, change];
        loop {
            let w = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
            if w.0 != WAIT_OBJECT_0.0 + 1 {
                break; // stop (ou erro)
            }
            // Debounce: junta rajadas (copiar vários arquivos de uma vez) e só
            // dispara depois de 400 ms quieto.
            let mut stopped = false;
            loop {
                let w = unsafe { WaitForMultipleObjects(&handles, false, 400) };
                if w == WAIT_OBJECT_0 {
                    stopped = true;
                    break;
                }
                if w == WAIT_TIMEOUT {
                    break;
                }
                unsafe {
                    let _ = FindNextChangeNotification(change);
                }
            }
            if stopped {
                break;
            }
            // O callback só dispara se o watcher ainda for o vigente (um Stop
            // deliberado zera `callback`): evita rodar com o Bridge em teardown.
            let cb = callback.clone();
            mainthread::queue(move || {
                if state().callback.is_some() {
                    cb();
                }
            });
            unsafe {
                let _ = FindNextChangeNotification(change);
            }
        }
        unsafe {
            let _ = FindCloseChangeNotification(change);
        }
        log!("OBSRecordWatch: parou de monitorar {dir}");
    });
    Some(Watch { stop_event, thread: Some(thread) })
}

fn halt(w: Watch) {
    unsafe {
        let _ = SetEvent(w.stop_event);
    }
    if let Some(t) = w.thread {
        // O Delphi esperava 2 s e abandonava a thread se não saísse (cleanup pelo
        // SO). O join é imediato porque o evento acorda o WaitForMultipleObjects.
        let _ = t.join();
    }
    unsafe {
        let _ = CloseHandle(w.stop_event);
    }
}

pub fn start(dir: &str, callback: Callback) {
    stop();
    state().callback = Some(callback.clone());
    let w = spawn(dir.to_owned(), callback);
    state().watch = w;
}

/// Limpa o callback ANTES de sinalizar: evita executar callback em estado
/// parcialmente desmontado.
pub fn stop() {
    let w = {
        let mut s = state();
        s.callback = None;
        s.watch.take()
    };
    if let Some(w) = w {
        halt(w);
    }
}

/// Re-aponta para outra pasta. Guarda o callback ANTES do `stop`, que o zera de
/// propósito: lido depois dele seria sempre `None` e o watcher nunca voltaria a
/// subir — a pasta nova ficava sem vigilância pelo resto da sessão (pegadinha #54).
pub fn update_dir(new_dir: &str) {
    let cb = state().callback.clone();
    let w = {
        let mut s = state();
        s.watch.take()
    };
    if let Some(w) = w {
        halt(w);
    }
    let Some(cb) = cb else { return };
    state().callback = Some(cb.clone());
    let w = spawn(new_dir.to_owned(), cb);
    state().watch = w;
}

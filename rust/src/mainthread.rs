//! Fila de trabalho para a main thread — o equivalente de `TThread.Queue` +
//! `WakeMainThread` + `CheckSynchronize` do Delphi (pegadinha #4).
//!
//! O app usa um loop Win32 puro (`GetMessage`/`DispatchMessage`), então nada
//! drena sozinho o que as workers enfileiram: `queue` guarda o closure e posta
//! `WM_APP_WAKE` na janela; o `WndProc` chama `drain`. Sem isso, toda tarefa
//! enfileirada ficaria parada até a próxima mensagem qualquer do Windows.
//!
//! Regra do projeto: libobs e UI só na main thread; libav pode (e deve) rodar
//! em worker e usar `queue` para devolver o resultado.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Mensagem privada que acorda o `GetMessage`.
pub const WM_APP_WAKE: u32 = WM_APP + 1;

type Job = Box<dyn FnOnce() + Send + 'static>;

static QUEUE: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
static WAKE_HWND: AtomicIsize = AtomicIsize::new(0);

/// Registra a janela que recebe o `WM_APP_WAKE`.
pub fn set_wake_window(hwnd: HWND) {
    WAKE_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
}

/// Enfileira `job` para rodar na main thread. Seguro de qualquer thread.
///
/// Closures capturam por MOVE: não existe a armadilha do Delphi de capturar
/// a variável de um laço por referência (pegadinha #6).
pub fn queue(job: impl FnOnce() + Send + 'static) {
    QUEUE.lock().unwrap_or_else(|p| p.into_inner()).push_back(Box::new(job));
    let hwnd = WAKE_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_APP_WAKE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Roda tudo que está pendente. Só o `WndProc` chama.
pub fn drain() {
    loop {
        // Solta o lock antes de rodar: o job pode enfileirar outro.
        let job = QUEUE.lock().unwrap_or_else(|p| p.into_inner()).pop_front();
        match job {
            // Falha numa tarefa não derruba o app nem as seguintes (o
            // CheckSynchronize do Delphi engole a exceção do mesmo jeito). O
            // hook de panic já registrou o motivo no log.
            Some(job) => {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                    crate::log::log("MainThread: tarefa enfileirada falhou (ver PANIC acima) — seguindo.");
                }
            }
            None => break,
        }
    }
}

//! Bloqueio de tela (Win+L, bloqueio automático, troca de usuário) — porte
//! de `NoOBSLockDetector.pas`.
//!
//! Uma janela só-mensagens numa thread própria registrada no
//! `WTSRegisterSessionNotification`; o callback roda NA THREAD do detector e
//! só na mudança de estado. Quem consome marshala para a main.
//!
//! O Delphi tinha um double-start de `TThread` aqui que impedia o detector de
//! subir (pegadinha #45); com `std::thread` não existe esse estado.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use std::thread::JoinHandle;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::log;

const WTS_SESSION_LOCK: usize = 7;
const WTS_SESSION_UNLOCK: usize = 8;

static HWND_: AtomicIsize = AtomicIsize::new(0);
static LOCKED: AtomicBool = AtomicBool::new(false);
static CALLBACK: Mutex<Option<fn(bool)>> = Mutex::new(None);
static THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

fn changed(locked: bool) {
    if LOCKED.swap(locked, Ordering::SeqCst) == locked {
        return;
    }
    if let Some(cb) = *CALLBACK.lock().unwrap_or_else(|p| p.into_inner()) {
        cb(locked);
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_WTSSESSION_CHANGE => {
            match wparam.0 {
                WTS_SESSION_LOCK => changed(true),
                WTS_SESSION_UNLOCK => changed(false),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = WTSUnRegisterSessionNotification(hwnd);
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Sobe o detector (no-op se já rodando).
pub fn start(callback: fn(bool)) {
    *CALLBACK.lock().unwrap_or_else(|p| p.into_inner()) = Some(callback);
    let mut t = THREAD.lock().unwrap_or_else(|p| p.into_inner());
    if t.is_some() {
        return;
    }
    *t = Some(std::thread::spawn(|| unsafe {
        let hinst = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: w!("MachineLockDetectorWindow"),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("MachineLockDetectorWindow"),
            w!("MachineLockDetector"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst),
            None,
        ) else {
            log::log("LockDetector: CreateWindowEx falhou.");
            return;
        };
        if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).is_err() {
            log::log("LockDetector: WTSRegisterSessionNotification falhou.");
            let _ = DestroyWindow(hwnd);
            return;
        }
        HWND_.store(hwnd.0 as isize, Ordering::SeqCst);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        HWND_.store(0, Ordering::SeqCst);
    }));
    log::log("LockDetector: iniciado.");
}

pub fn stop() {
    let h = HWND_.swap(0, Ordering::SeqCst);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    if let Some(t) = THREAD.lock().unwrap_or_else(|p| p.into_inner()).take() {
        let _ = t.join();
    }
}

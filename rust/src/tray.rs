//! Ícone na bandeja do sistema (porte de `OBSTray.pas`).
//!
//! * clique esquerdo/duplo: restaura a janela;
//! * clique direito: menu — Iniciar/Parar gravação, Abrir, Iniciar com o
//!   Windows, Fechar;
//! * o `WM_CLOSE` da janela principal vai pra bandeja em vez de fechar (a
//!   decisão é do bridge, via hook do `ui`).
//!
//! Tudo aqui roda na main thread (é chamado do `WndProc` e do bridge).

use std::cell::RefCell;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_LARGE_ICON, NIIF_USER, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, PostMessageW, SetForegroundWindow,
    TrackPopupMenu, HICON, MF_CHECKED, MF_SEPARATOR, MF_STRING, SM_CXSMICON, TPM_RIGHTBUTTON, WM_LBUTTONDBLCLK,
    WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_USER,
};

use crate::{autostart, lang, log, record_icon, win};

pub const WM_TRAYICON: u32 = WM_USER + 100;
pub const ID_TRAY_SHOW: i32 = 9001;
pub const ID_TRAY_AUTOSTART: i32 = 9002;
pub const ID_TRAY_QUIT: i32 = 9003;
pub const ID_TRAY_TOGGLE_RECORD: i32 = 9004;

/// Callbacks que o `ui` (Abrir/Fechar) e o bridge (item de gravação) ligam.
#[derive(Clone, Copy, Default)]
pub struct Callbacks {
    pub on_command: Option<fn(i32)>,
    pub on_toggle_record: Option<fn()>,
    pub on_is_recording: Option<fn() -> bool>,
}

#[derive(Default)]
struct State {
    hwnd: Option<HWND>,
    added: bool,
    base_icon: Option<HICON>,
    recording_icon: Option<HICON>,
    recording_now: bool,
    cb: Callbacks,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// O `ui` liga Abrir/Fechar; o bridge, o item de gravação. Mescla: quem não
/// passa um campo não apaga o do outro.
pub fn set_callbacks(cb: Callbacks) {
    with(|s| {
        if cb.on_command.is_some() {
            s.cb.on_command = cb.on_command;
        }
        if cb.on_toggle_record.is_some() {
            s.cb.on_toggle_record = cb.on_toggle_record;
        }
        if cb.on_is_recording.is_some() {
            s.cb.on_is_recording = cb.on_is_recording;
        }
    });
}

/// `szTip`/`szInfo`… são arrays fixos de UTF-16: copia com clamp e zero final.
fn copy_wide(dst: &mut [u16], text: &str) {
    let mut n = 0;
    let max = dst.len() - 1;
    for (d, c) in dst.iter_mut().take(max).zip(text.encode_utf16()) {
        *d = c;
        n += 1;
    }
    dst[n] = 0;
}

fn base_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() }
}

pub fn is_installed() -> bool {
    with(|s| s.added)
}

pub fn install(hwnd: HWND, tooltip: &str) {
    if with(|s| s.added) {
        return;
    }
    let mut d = base_data(hwnd);
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    d.uCallbackMessage = WM_TRAYICON;
    d.hIcon = win::main_icon();
    copy_wide(&mut d.szTip, tooltip);
    let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &d) }.as_bool();
    with(|s| {
        s.hwnd = Some(hwnd);
        s.added = ok;
    });
    if !ok {
        log::log("Tray: Shell_NotifyIconW(NIM_ADD) falhou.");
        return;
    }
    // Rede de segurança: o ícone pode nascer DEPOIS de a gravação começar (o
    // start chama `set_recording(true)`, e sem ícone isso é no-op). Com
    // `closeToTray` o ícone já nasce no `do_init`, como no Delphi; isto cobre
    // o resto (ligar a bandeja nas Configurações no meio de uma gravação, ou
    // um ícone recriado depois).
    if with(|s| s.cb.on_is_recording).is_some_and(|f| f()) {
        set_recording(true);
    }
}

pub fn remove() {
    let Some(hwnd) = with(|s| s.hwnd.filter(|_| s.added)) else { return };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &base_data(hwnd));
    }
    with(|s| {
        s.added = false;
        // Um ícone novo nasce sem a bolinha: o próximo set_recording reaplica.
        s.recording_now = false;
    });
}

/// Só o tooltip (`NIM_MODIFY`); nunca adiciona — evitaria ícone duplicado.
pub fn update_tooltip(tooltip: &str) {
    let Some(hwnd) = with(|s| s.hwnd.filter(|_| s.added)) else { return };
    let mut d = base_data(hwnd);
    d.uFlags = NIF_TIP;
    copy_wide(&mut d.szTip, tooltip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
    }
}

/// Balão (`NIF_INFO`): fallback de notificação quando a UI HTML ainda não
/// subiu (gravação vinda da hibernação). No-op sem ícone.
pub fn show_balloon(title: &str, message: &str) {
    let Some(hwnd) = with(|s| s.hwnd.filter(|_| s.added)) else { return };
    let mut d = base_data(hwnd);
    d.uFlags = NIF_INFO;
    copy_wide(&mut d.szInfoTitle, title);
    copy_wide(&mut d.szInfo, message);
    d.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
    }
}

/// Troca o ícone entre normal e "gravando" (bolinha vermelha). Idempotente.
pub fn set_recording(on: bool) {
    let Some(hwnd) = with(|s| s.hwnd.filter(|_| s.added)) else { return };
    if with(|s| std::mem::replace(&mut s.recording_now, on) == on) {
        return;
    }
    let icon = with(|s| {
        let base = *s.base_icon.get_or_insert_with(win::main_icon);
        if s.recording_icon.is_none() {
            let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
            s.recording_icon = record_icon::create_recording_overlay_icon(base, size);
        }
        if on { s.recording_icon.unwrap_or(base) } else { base }
    });
    let mut d = base_data(hwnd);
    d.uFlags = NIF_ICON;
    d.hIcon = icon;
    let ok = unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) }.as_bool();
    log!("Tray: icone de gravando {} (ok={ok}).", if on { "ligado" } else { "desligado" });
}

fn show_menu(hwnd: HWND) {
    let cb = with(|s| s.cb);
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let item = |id: i32, key: &str, flags| {
            let text = win::wide(&lang::t(key));
            let _ = AppendMenuW(menu, flags, id as usize, windows::core::PCWSTR(text.as_ptr()));
        };
        // Item de gravação só com os callbacks do bridge ligados.
        if let (Some(_), Some(is_rec)) = (cb.on_toggle_record, cb.on_is_recording) {
            item(ID_TRAY_TOGGLE_RECORD, if is_rec() { "record.stop" } else { "record.start" }, MF_STRING);
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        }
        item(ID_TRAY_SHOW, "common.open", MF_STRING);
        let auto = if autostart::is_autostart_enabled() { MF_STRING | MF_CHECKED } else { MF_STRING };
        item(ID_TRAY_AUTOSTART, "settings.autostart.label", auto);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        item(ID_TRAY_QUIT, "header.close", MF_STRING);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // SetForegroundWindow + WM_NULL depois: o truque clássico para o menu
        // fechar quando o usuário clica fora.
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

/// `WM_TRAYICON` (o `lParam` traz a mensagem de mouse).
pub fn handle_message(hwnd: HWND, lparam: LPARAM) {
    match lparam.0 as u32 {
        WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
            if let Some(f) = with(|s| s.cb.on_command) {
                f(ID_TRAY_SHOW);
            }
        }
        WM_RBUTTONUP => show_menu(hwnd),
        _ => {}
    }
}

/// `WM_COMMAND` com id >= 9000.
pub fn handle_command(id: i32) {
    let cb = with(|s| s.cb);
    match id {
        ID_TRAY_SHOW | ID_TRAY_QUIT => {
            if let Some(f) = cb.on_command {
                f(id);
            }
        }
        ID_TRAY_AUTOSTART => autostart::set_autostart(!autostart::is_autostart_enabled()),
        ID_TRAY_TOGGLE_RECORD => {
            if let Some(f) = cb.on_toggle_record {
                f();
            }
        }
        _ => {}
    }
}

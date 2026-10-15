//! Pequenos helpers Win32 compartilhados (strings largas, MessageBox, caminhos).

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, MB_TOPMOST};

/// `&str` → UTF-16 terminado em zero.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn message_box(title: &str, text: &str) {
    unsafe {
        MessageBoxW(
            Some(HWND::default()),
            PCWSTR(HSTRING::from(text).as_ptr()),
            PCWSTR(HSTRING::from(title).as_ptr()),
            MB_OK | MB_ICONERROR | MB_TOPMOST,
        );
    }
}

/// Linha de comando em minúsculas, para os flags `/autostart`, `/hibernate`…
pub fn cmdline_lower() -> String {
    std::env::args().collect::<Vec<_>>().join(" ").to_lowercase()
}

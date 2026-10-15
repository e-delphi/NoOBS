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

/// Separador decimal do locale do usuário (`,` em pt-BR, `.` em en-US). O Delphi
/// formatava tamanhos com o `FormatSettings` do sistema, e a UI exibe como vier.
pub fn decimal_separator() -> char {
    use std::sync::OnceLock;
    use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SDECIMAL};
    static SEP: OnceLock<char> = OnceLock::new();
    *SEP.get_or_init(|| {
        let mut buf = [0u16; 8];
        // LOCALE_NAME_USER_DEFAULT é NULL.
        let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_SDECIMAL, Some(&mut buf)) };
        if n > 1 {
            char::from_u32(buf[0] as u32).unwrap_or('.')
        } else {
            '.'
        }
    })
}

/// Troca o `.` de um número já formatado pelo separador decimal do usuário.
pub fn localize_decimal(s: &str) -> String {
    let sep = decimal_separator();
    if sep == '.' { s.to_owned() } else { s.replace('.', &sep.to_string()) }
}

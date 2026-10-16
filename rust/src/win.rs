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

/// Ícone do app (`MAINICON`, embutido pelo `build.rs`); `IDI_APPLICATION` se
/// faltar. Ícone de recurso é compartilhado: não precisa de `DestroyIcon`.
pub fn main_icon() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    use windows::core::w;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{LoadIconW, IDI_APPLICATION};
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).ok();
        LoadIconW(hinst, w!("MAINICON"))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default()
    }
}

/// `HKCU\...\Themes\Personalize\AppsUseLightTheme`: 0 = escuro.
pub fn detect_system_theme() -> &'static str {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    if rc.is_ok() && value == 0 { "dark" } else { "light" }
}

/// Funções NÃO documentadas do `uxtheme.dll` (ordinais 133/135/136) que põem
/// os menus do sistema (inclusive o `TrackPopupMenu` da bandeja) no tema
/// escuro sem owner-draw. Mesma estratégia do OBSUI/OBSHibernate.
struct UxDark {
    allow_dark_for_window: Option<unsafe extern "system" fn(HWND, i32) -> i32>,
    set_preferred_app_mode: Option<unsafe extern "system" fn(i32) -> i32>,
    flush_menu_themes: Option<unsafe extern "system" fn()>,
}

fn ux_dark() -> &'static UxDark {
    use std::sync::OnceLock;
    use windows::core::{w, PCSTR};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    static UX: OnceLock<UxDark> = OnceLock::new();
    UX.get_or_init(|| unsafe {
        let Ok(m) = LoadLibraryW(w!("uxtheme.dll")) else {
            return UxDark { allow_dark_for_window: None, set_preferred_app_mode: None, flush_menu_themes: None };
        };
        let by_ord = |n: usize| GetProcAddress(m, PCSTR(n as *const u8));
        UxDark {
            allow_dark_for_window: by_ord(133).map(|f| std::mem::transmute(f)),
            set_preferred_app_mode: by_ord(135).map(|f| std::mem::transmute(f)),
            flush_menu_themes: by_ord(136).map(|f| std::mem::transmute(f)),
        }
    })
}

/// Popup de menu segue o tema do app: FORCE_DARK (2) ou FORCE_LIGHT (3).
pub fn apply_menu_theme(hwnd: Option<HWND>, dark: bool) {
    let ux = ux_dark();
    unsafe {
        if let Some(f) = ux.set_preferred_app_mode {
            f(if dark { 2 } else { 3 });
        }
        if let (Some(f), Some(h)) = (ux.allow_dark_for_window, hwnd) {
            f(h, dark as i32);
        }
        if let Some(f) = ux.flush_menu_themes {
            f();
        }
    }
}

/// Caminho do próprio exe (vazio se o Windows não disser).
pub fn exe_path() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
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

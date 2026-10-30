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

/// Ícone do app como PNG com alfa (porte do `PushAppIcon` do Delphi): vai pra
/// UI como data URL — logo do "Sobre" e ícone das notificações. `None` se o
/// recurso não carregar.
///
/// Os pixels saem do `hbmColor` do `GetIconInfo` por `GetDIBits`, com o alfa
/// do `.ico` intacto. `DrawIconEx` não serve: escreve RGB pré-multiplicado e a
/// imagem sai com as bordas escurecidas.
#[allow(dead_code)]
pub fn main_icon_png(size: i32) -> Option<Vec<u8>> {
    main_icon_rgba(size).map(|(w, h, rgba)| encode_png_rgba(w, h, &rgba))
}

/// O ícone do app em RGBA (largura, altura, pixels): a logo do "Sobre".
pub fn main_icon_rgba(size: i32) -> Option<(u32, u32, Vec<u8>)> {
    use windows::core::w;
    use windows::Win32::Foundation::HINSTANCE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, GetIconInfo, LoadImageW, HICON, ICONINFO, IMAGE_ICON, LR_DEFAULTCOLOR,
    };
    unsafe {
        let hinst = GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0));
        // 256 é o maior tamanho do .ico: pede ele pra ter qualidade máxima.
        let h = LoadImageW(hinst, w!("MAINICON"), IMAGE_ICON, size, size, LR_DEFAULTCOLOR).ok()?;
        let hicon = HICON(h.0);
        let mut info = ICONINFO::default();
        let ok = GetIconInfo(hicon, &mut info).is_ok();
        let result = (|| {
            if !ok || info.hbmColor.is_invalid() {
                return None; // monocromático: sem cor pra mostrar
            }
            let mut bm = BITMAP::default();
            if GetObjectW(HGDIOBJ(info.hbmColor.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
                return None;
            }
            let (wd, ht) = (bm.bmWidth, bm.bmHeight);
            if wd <= 0 || ht <= 0 {
                return None;
            }
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: wd,
                    biHeight: -ht, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bgra = vec![0u8; (wd * ht * 4) as usize];
            let dc = GetDC(None);
            let lines = GetDIBits(dc, info.hbmColor, 0, ht as u32, Some(bgra.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            if lines == 0 {
                return None;
            }
            // Ícone antigo sem canal alfa vem com alfa 0 em tudo: sem isto a
            // imagem sairia inteira transparente.
            let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
            let rgba: Vec<u8> = bgra
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0], if has_alpha { p[3] } else { 255 }])
                .collect();
            Some((wd as u32, ht as u32, rgba))
        })();
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        let _ = DestroyIcon(hicon);
        result
    }
}

/// PNG RGBA 8 bits mínimo (sem filtro por linha). O projeto não tem crate de
/// imagem; o `flate2` já entra pelo `file_meta`.
pub fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::{Compression, Crc};
    use std::io::Write;

    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc = Crc::new();
        crc.update(kind);
        crc.update(data);
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
    }

    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in rgba.chunks_exact(stride) {
        raw.push(0); // filtro "None"
        raw.extend_from_slice(row);
    }
    let mut z = ZlibEncoder::new(Vec::new(), Compression::best());
    let _ = z.write_all(&raw);
    let idat = z.finish().unwrap_or_default();

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA, deflate, sem filtro adaptativo, sem entrelaçamento

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", &[]);
    out
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
/// escuro sem owner-draw. Mesma estratégia do OBSUI.
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

/// Linha de comando em minúsculas, para os flags `/autostart`, `/tray`…
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

/// Largura do texto em px (Segoe UI no tamanho `px`), pra quebrar linhas de
/// pílulas como o `flex-wrap` da web (medido do lado do Rust, sem a tela).
pub fn text_width(text: &str, px: i32, weight: i32) -> i32 {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject, GetTextExtentPoint32W, SelectObject, CLIP_DEFAULT_PRECIS,
        DEFAULT_CHARSET, DEFAULT_PITCH, FF_SWISS, FONT_QUALITY, HGDIOBJ, OUT_TT_PRECIS,
    };
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return (text.chars().count() as f32 * px as f32 * 0.55) as i32;
        }
        let font = CreateFontW(
            -px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            FONT_QUALITY(4),
            (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
            windows::core::w!("Segoe UI"),
        );
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let w: Vec<u16> = text.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &w, &mut sz);
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteDC(dc);
        sz.cx
    }
}

#[cfg(test)]
mod png_tests {
    use super::encode_png_rgba;

    #[test]
    fn png_tem_estrutura_e_pixels_certos() {
        // 2x2: vermelho, verde opaco / azul meio transparente, transparente.
        let px = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 0, 0, 0, 0];
        let png = encode_png_rgba(2, 2, &px);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 2);
        assert_eq!(&png[24..26], &[8, 6]); // 8 bits, RGBA
        // CRC do IHDR confere.
        let mut crc = flate2::Crc::new();
        crc.update(&png[12..29]);
        assert_eq!(crc.sum(), u32::from_be_bytes(png[29..33].try_into().unwrap()));
        // IDAT descompacta para as linhas com o byte de filtro na frente.
        let len = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
        assert_eq!(&png[37..41], b"IDAT");
        let mut raw = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&png[41..41 + len]), &mut raw).unwrap();
        assert_eq!(raw, [&[0u8][..], &px[..8], &[0], &px[8..]].concat());
        assert!(png.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]));
    }
}

/// Build do Windows pelo registro (`CurrentBuildNumber`): o `GetVersionEx`
/// mente sem manifesto de compatibilidade.
pub fn windows_build() -> u32 {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let mut buf = [0u16; 32];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return 0;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n]).trim().parse().unwrap_or(0)
}

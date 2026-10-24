//! Janelas de aplicativo que podem ser gravadas (porte de `WinWindows.pas`,
//! pegadinha #66).
//!
//! Alimenta o seletor "Gravar uma janela": enumera as janelas de topo que
//! aparecem na barra de tarefas e tira uma prévia de cada uma com
//! `PrintWindow(PW_RENDERFULLCONTENT)` — funciona com a janela coberta, porque
//! o DWM entrega a superfície dela.
//!
//! A identidade da janela é `titulo:classe:exe`, com `#` e `:` escapados como
//! o OBS montava (`window-helpers.c`, `encode_dstr`): é por ela que o motor
//! reencontra a janela na hora de gravar.

use std::ffi::c_void;

use windows::core::BOOL;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, HDC,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Desenha pelo DWM (8.1+): sem isso navegadores/Electron saem pretos.
const PW_RENDERFULLCONTENT: u32 = 2;

/// Mesma lista do OBS (`internal_microsoft_exes_exact`): janelas do shell que
/// aparecem na enumeração mas não são "aplicativos".
const INTERNAL_EXES: [&str; 15] = [
    "startmenuexperiencehost.exe",
    "applicationframehost.exe",
    "peopleexperiencehost.exe",
    "shellexperiencehost.exe",
    "microsoft.notes.exe",
    "systemsettings.exe",
    "textinputhost.exe",
    "searchapp.exe",
    "video.ui.exe",
    "searchui.exe",
    "lockapp.exe",
    "cortana.exe",
    "gamebar.exe",
    "tabtip.exe",
    "time.exe",
];

#[derive(Debug, Clone)]
pub struct CaptureWindow {
    /// Janela de topo (a da barra de tarefas).
    pub handle: isize,
    pub title: String,
    pub class_name: String,
    /// Só o nome do arquivo (ex.: `chrome.exe`); `unknown` se ilegível.
    pub exe: String,
    /// `titulo:classe:exe` escapado, para o `window_capture`.
    pub spec: String,
    /// Área CLIENTE em pixels físicos.
    pub width: i32,
    pub height: i32,
}

impl CaptureWindow {
    /// Nome do app para a faixa de som: o exe sem extensão; sem exe legível
    /// (processo elevado), o título.
    pub fn app_name(&self) -> String {
        let stem = self.exe.rsplit_once('.').map(|(s, _)| s).unwrap_or(&self.exe);
        if stem.is_empty() || stem.eq_ignore_ascii_case("unknown") { self.title.clone() } else { stem.to_owned() }
    }
}

/// `#` ANTES de `:` (a ordem do `encode_dstr`), senão o `#` do `#3A` seria
/// escapado de novo.
pub fn encode_part(s: &str) -> String {
    s.replace('#', "#22").replace(':', "#3A")
}

fn wnd_text(h: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(h);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(h, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..n])
    }
}

fn wnd_class(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn wnd_pid(h: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

/// O OBS cai em `unknown` quando não abre o processo (elevado, protegido):
/// repetimos, senão a string não casaria com a dele.
fn process_exe_name(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return "unknown".into() };
        let mut buf = [0u16; 261];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return "unknown".into();
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        full.rsplit('\\').next().unwrap_or("unknown").to_owned()
    }
}

/// Em outra área de trabalho virtual.
fn is_cloaked(h: HWND) -> bool {
    let mut c = 0u32;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut c as *mut u32 as *mut c_void, 4) }.is_ok() && c != 0
}

fn is_internal_exe(exe: &str) -> bool {
    let l = exe.to_lowercase();
    INTERNAL_EXES.contains(&l.as_str()) || l.starts_with("windowsinternal")
}

/// App da loja: a janela de topo é do `ApplicationFrameHost`; o app de verdade
/// é uma FILHA de outro processo (`ms_get_uwp_actual_window`).
fn uwp_actual_window(frame: HWND) -> Option<HWND> {
    let frame_pid = wnd_pid(frame);
    let mut child = unsafe { FindWindowExW(Some(frame), None, None, None) }.ok()?;
    loop {
        if wnd_pid(child) != frame_pid {
            return Some(child);
        }
        child = unsafe { FindWindowExW(Some(frame), Some(child), None, None) }.ok()?;
    }
}

/// Área cliente (o que o `window_capture` grava com `client_area`).
/// Minimizada: o tamanho que ela terá restaurada (aproximação com borda).
pub fn capture_window_size(hwnd: isize) -> Option<(i32, i32)> {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if hwnd == 0 || !IsWindow(Some(h)).as_bool() {
            return None;
        }
        if IsIconic(h).as_bool() {
            let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
            GetWindowPlacement(h, &mut wp).ok()?;
            let r = wp.rcNormalPosition;
            return Some((r.right - r.left, r.bottom - r.top));
        }
        let mut r = RECT::default();
        GetClientRect(h, &mut r).ok()?;
        Some((r.right - r.left, r.bottom - r.top))
    }
}

struct EnumCtx {
    list: Vec<CaptureWindow>,
    self_pid: u32,
}

unsafe extern "system" fn enum_proc(h: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut EnumCtx);
    if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() || is_cloaked(h) {
        return true.into();
    }
    let st = GetWindowLongPtrW(h, GWL_STYLE);
    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
    if ex & WS_EX_TOOLWINDOW.0 as isize != 0 || st & WS_CHILD.0 as isize != 0 {
        return true.into();
    }
    // Com dono (diálogo, popup) não é da barra de tarefas.
    if GetWindow(h, GW_OWNER).is_ok_and(|o| !o.0.is_null()) && ex & WS_EX_APPWINDOW.0 as isize == 0 {
        return true.into();
    }
    if wnd_pid(h) == ctx.self_pid {
        return true.into();
    }
    let Some((w, ht)) = capture_window_size(h.0 as isize) else { return true.into() };
    if w < 32 || ht < 32 {
        return true.into();
    }
    let cls = wnd_class(h);
    let ident = if cls.eq_ignore_ascii_case("ApplicationFrameWindow") || cls.eq_ignore_ascii_case("WinUIDesktopWin32WindowClass") {
        // App da loja suspenso: nada pra gravar.
        match uwp_actual_window(h) {
            Some(c) => c,
            None => return true.into(),
        }
    } else {
        h
    };
    let exe = process_exe_name(wnd_pid(ident));
    if is_internal_exe(&exe) {
        return true.into();
    }
    let mut title = wnd_text(ident);
    if title.is_empty() {
        title = wnd_text(h);
    }
    // Sem título o usuário não saberia o que escolheu.
    if title.trim().is_empty() {
        return true.into();
    }
    let class_name = wnd_class(ident);
    let spec = format!("{}:{}:{}", encode_part(&title), encode_part(&class_name), encode_part(&exe));
    ctx.list.push(CaptureWindow { handle: h.0 as isize, title, class_name, exe, spec, width: w, height: ht });
    true.into()
}

/// Janelas graváveis na ordem Z (a mais à frente primeiro).
pub fn enumerate_capture_windows() -> Vec<CaptureWindow> {
    let mut ctx = EnumCtx { list: Vec::new(), self_pid: unsafe { GetCurrentProcessId() } };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut EnumCtx as isize));
    }
    ctx.list
}

struct DcGuard(HDC, bool);
impl Drop for DcGuard {
    fn drop(&mut self) {
        unsafe {
            if self.1 {
                ReleaseDC(None, self.0);
            } else {
                let _ = DeleteDC(self.0);
            }
        }
    }
}
struct ObjGuard(HGDIOBJ);
impl Drop for ObjGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

fn dib(dc: HDC, w: i32, h: i32) -> Option<(ObjGuard, *mut u8)> {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bmp = unsafe { CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    if bits.is_null() {
        return None;
    }
    Some((ObjGuard(HGDIOBJ(bmp.0)), bits as *mut u8))
}

/// Área cliente reduzida pra caber em `max_w` × `max_h`, em RGB 8 bits (linhas
/// de cima pra baixo) com o tamanho. `None` se não der (janela sumiu, travada,
/// tamanho zero). Dezenas de ms: em worker.
pub fn capture_window_thumb_rgb(hwnd: isize, max_w: i32, max_h: i32) -> Option<(Vec<u8>, u32, u32)> {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if hwnd == 0 || !IsWindow(Some(h)).as_bool() || IsIconic(h).as_bool() {
            return None;
        }
        // O PrintWindow manda mensagem pra thread da janela: travada, o pedido
        // ficaria preso junto.
        if IsHungAppWindow(h).as_bool() {
            return None;
        }
        let mut wr = RECT::default();
        if GetWindowRect(h, &mut wr).is_err() {
            return None;
        }
        let (cw, ch) = capture_window_size(hwnd)?;
        let (ww, wh) = (wr.right - wr.left, wr.bottom - wr.top);
        if cw <= 0 || ch <= 0 || ww <= 0 || wh <= 0 {
            return None;
        }
        // Onde a área cliente começa dentro da janela: a prévia recorta só ela.
        let mut org = POINT::default();
        let _ = ClientToScreen(h, &mut org);
        let (ox, oy) = (org.x - wr.left, org.y - wr.top);
        let scale = (max_w as f64 / cw as f64).min(max_h as f64 / ch as f64).min(1.0);
        let (tw, th) = (((cw as f64 * scale).round() as i32).max(1), ((ch as f64 * scale).round() as i32).max(1));

        let screen = DcGuard(GetDC(None), true);
        let full_dc = DcGuard(CreateCompatibleDC(Some(screen.0)), false);
        let thumb_dc = DcGuard(CreateCompatibleDC(Some(screen.0)), false);
        let (full_bmp, _) = dib(full_dc.0, ww, wh)?;
        let (thumb_bmp, thumb_bits) = dib(thumb_dc.0, tw, th)?;
        let of = SelectObject(full_dc.0, full_bmp.0);
        let ot = SelectObject(thumb_dc.0, thumb_bmp.0);
        let mut rgb = Vec::new();
        if PrintWindow(h, full_dc.0, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool() {
            SetStretchBltMode(thumb_dc.0, HALFTONE);
            let _ = SetBrushOrgEx(thumb_dc.0, 0, 0, None);
            if StretchBlt(thumb_dc.0, 0, 0, tw, th, Some(full_dc.0), ox, oy, cw, ch, SRCCOPY).as_bool() {
                let px = std::slice::from_raw_parts(thumb_bits, (tw * th * 4) as usize);
                rgb.reserve((tw * th * 3) as usize);
                for p in px.chunks_exact(4) {
                    rgb.extend_from_slice(&[p[2], p[1], p[0]]);
                }
            }
        }
        SelectObject(full_dc.0, of);
        SelectObject(thumb_dc.0, ot);
        (!rgb.is_empty()).then_some((rgb, tw as u32, th as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identidade_escapa_na_ordem_do_obs() {
        assert_eq!(encode_part("a:b#c"), "a#3Ab#22c");
        // '#' primeiro: o '#' do '#3A' não pode virar '#223A'.
        assert_eq!(encode_part("x:"), "x#3A");
    }

    #[test]
    fn nome_do_app() {
        let w = CaptureWindow { handle: 0, title: "Doc".into(), class_name: String::new(), exe: "WINWORD.EXE".into(), spec: String::new(), width: 0, height: 0 };
        assert_eq!(w.app_name(), "WINWORD");
        let u = CaptureWindow { exe: "unknown".into(), ..w };
        assert_eq!(u.app_name(), "Doc");
    }
}

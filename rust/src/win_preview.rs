//! Monitores físicos via Win32 (porte de `WinPreview.pas`). A captura das
//! prévias (BitBlt → JPEG) entra com a tela inicial; aqui fica a enumeração,
//! que o engine e a UI usam como fonte única do índice de cada monitor.

use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW, ENUM_CURRENT_SETTINGS, HDC,
    HMONITOR, MONITORINFOEXW,
};

use crate::scene::ObsMonitor;

/// `MONITORINFOF_PRIMARY` (winuser.h) — a `windows` crate não o exporta.
const MONITORINFOF_PRIMARY: u32 = 1;

#[derive(Debug, Clone, Default)]
pub struct MonitorInfo {
    pub index: i32,
    pub device_name: String,
    pub friendly_name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub is_primary: bool,
    /// Hz (ex.: 60, 120, 144, 240).
    pub refresh_rate: i32,
}

unsafe extern "system" fn monitor_enum_proc(hmon: HMONITOR, _hdc: HDC, _rc: *mut RECT, lparam: LPARAM) -> BOOL {
    let list = &mut *(lparam.0 as *mut Vec<MonitorInfo>);
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(hmon, &mut mi as *mut MONITORINFOEXW as *mut _).as_bool() {
        let len = mi.szDevice.iter().position(|c| *c == 0).unwrap_or(mi.szDevice.len());
        let device_name = String::from_utf16_lossy(&mi.szDevice[..len]);
        let r = mi.monitorInfo.rcMonitor;

        // Taxa ativa do monitor. dmDisplayFrequency 0 ou 1 = "hardware default"
        // (documentado), não uma taxa real → cai no fallback de 60 Hz.
        let mut dm = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
        let refresh_rate = if EnumDisplaySettingsW(
            windows::core::PCWSTR(mi.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut dm,
        )
        .as_bool()
            && dm.dmDisplayFrequency > 1
        {
            dm.dmDisplayFrequency as i32
        } else {
            60
        };

        list.push(MonitorInfo {
            index: list.len() as i32,
            friendly_name: device_name.clone(), // sem nome amigável via Win32 simples
            device_name,
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
            is_primary: mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            refresh_rate,
        });
    }
    BOOL(1)
}

/// Monitores conectados, na ordem do Windows (o `index` é o da UI e do
/// `monitor_capture`).
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    let mut list: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

/// Maior taxa de atualização entre os monitores (fallback 60 Hz).
pub fn max_monitor_refresh_rate() -> i32 {
    enumerate_monitors().iter().map(|m| m.refresh_rate).fold(60, i32::max)
}

/// `MonitorsFromWinPreview` (OBSScene): o mesmo inventário no formato do canvas.
pub fn monitors_from_win_preview() -> Vec<ObsMonitor> {
    enumerate_monitors()
        .into_iter()
        .map(|m| ObsMonitor {
            index: m.index,
            name: m.friendly_name,
            width: m.width,
            height: m.height,
            position_x: m.x,
            position_y: m.y,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Prévias (BitBlt do desktop → JPEG)
// ---------------------------------------------------------------------------

use std::ffi::c_void;

use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, SetStretchBltMode,
    StretchBlt, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, HBITMAP, HGDIOBJ, SRCCOPY,
};

/// Objetos GDI com liberação garantida (RAII). A pegadinha #9 do Delphi
/// (double-free / vazamento de handle com a atribuição manual de `HBITMAP`)
/// não tem como acontecer: cada handle tem UM dono, e o `Drop` desfaz na ordem
/// inversa da criação.
struct ScreenDc(HDC);
impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe { ReleaseDC(None, self.0) };
    }
}
struct MemDc(HDC);
impl Drop for MemDc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}
struct Dib(HBITMAP);
impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0 .0));
        }
    }
}

/// Captura o monitor escalado para `thumb_w` × `thumb_h` e devolve RGB 8 bits
/// (3 bytes por pixel, linhas de cima pra baixo). Vazio se algo falhar.
fn capture_monitor_rgb(m: &MonitorInfo, thumb_w: i32, thumb_h: i32) -> Vec<u8> {
    if thumb_w <= 0 || thumb_h <= 0 || m.width <= 0 || m.height <= 0 {
        return Vec::new();
    }
    unsafe {
        let screen = GetDC(None);
        if screen.0.is_null() {
            return Vec::new();
        }
        let screen = ScreenDc(screen);
        let mem = CreateCompatibleDC(Some(screen.0));
        if mem.0.is_null() {
            return Vec::new();
        }
        let mem = MemDc(mem);

        // DIB 32 bpp top-down (altura negativa): as linhas já saem na ordem do JPEG.
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: thumb_w,
                biHeight: -thumb_h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(Some(mem.0), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            return Vec::new();
        };
        let bmp = Dib(bmp);
        if bits.is_null() {
            return Vec::new();
        }
        let old = SelectObject(mem.0, HGDIOBJ(bmp.0 .0));
        SetStretchBltMode(mem.0, HALFTONE);
        let ok = StretchBlt(mem.0, 0, 0, thumb_w, thumb_h, Some(screen.0), m.x, m.y, m.width, m.height, SRCCOPY).as_bool();
        let mut rgb = Vec::new();
        if ok {
            let px = std::slice::from_raw_parts(bits as *const u8, (thumb_w * thumb_h * 4) as usize);
            rgb.reserve((thumb_w * thumb_h * 3) as usize);
            for p in px.chunks_exact(4) {
                rgb.extend_from_slice(&[p[2], p[1], p[0]]); // BGRA → RGB
            }
        }
        SelectObject(mem.0, old); // desfaz antes de o DIB ser destruído
        rgb
    }
}

/// JPEG (qualidade 65) do monitor. Roda em worker: BitBlt + encode de um 4K leva
/// 50–200 ms e travaria a UI na main (pegadinha #22).
pub fn capture_monitor_as_jpeg(m: &MonitorInfo, thumb_w: i32, thumb_h: i32) -> Vec<u8> {
    let rgb = capture_monitor_rgb(m, thumb_w, thumb_h);
    if rgb.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, 65);
    if enc.encode(&rgb, thumb_w as u16, thumb_h as u16, jpeg_encoder::ColorType::Rgb).is_err() {
        return Vec::new();
    }
    out
}

/// `data:image/jpeg;base64,...` — usado uma única vez, no `init`, para o thumb
/// já vir inline no JSON inicial.
pub fn capture_monitor_as_data_url(m: &MonitorInfo, thumb_w: i32, thumb_h: i32) -> String {
    use base64::Engine as _;
    let jpeg = capture_monitor_as_jpeg(m, thumb_w, thumb_h);
    if jpeg.is_empty() {
        return String::new();
    }
    format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg))
}

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

//! Indicador NA TELA, excluído da própria gravação (porte de
//! `WinRecIndicator.pas`, pegadinha #49).
//!
//! Uma janelinha overlay (topmost, layered) com uma bolinha e um tempo, em
//! dois estados que nunca coexistem (o buffer para enquanto a gravação manual
//! roda):
//! * [`Mode::Recording`] — bolinha VERMELHA pulsando + tempo gravado;
//!   pausada, duas barras âmbar fixas e o tempo parado. Clicar para.
//! * [`Mode::Buffer`] — bolinha VERDE fixa + "guardado / teto". Clicar salva.
//!   Não pulsa de propósito: de canto de olho, confundir "guardando na RAM"
//!   com "gravando em disco" é o erro caro.
//!
//! O truque é `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`: o usuário vê,
//! a captura (WGC/DXGI) mostra o desktop atrás. Exige Windows 10 2004+; se
//! falhar, NÃO mostra — senão apareceria dentro da gravação.
//!
//! Com app em TELA CHEIA na frente fica escondido: uma janela por cima tira o
//! jogo da apresentação direta e custa ~40% de FPS (medido). O tique de 500 ms
//! o traz de volta quando o jogo sai da frente.
//!
//! Main thread (depende do loop do `ui::run` para pintar e para o timer).

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::GdiPlus::{
    GdipCreateFromHDC, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipFillEllipse,
    GdipSetSmoothingMode, GdiplusStartup, GdiplusStartupInput, GpBrush, GpGraphics, GpSolidFill,
    SmoothingModeAntiAlias,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::log;

const CLASS_NAME: PCWSTR = w!("TNoOBSRecIndicator");
const TIMER_ID: usize = 1;
/// Pulso da bolinha (1 Hz) + reavalia tempo e tela cheia.
const TICK_MS: u32 = 500;
/// Distância do canto, em px lógicos.
const MARGIN_DIP: i32 = 24;
/// Largura do pill na gravação; o do buffer cabe "12:34 / 60:00".
const W_DIP: i32 = 116;
const W_BUF_DIP: i32 = 140;
const H_DIP: i32 = 34;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Valor do config → canto; qualquer outro = superior direito.
pub fn parse_corner(s: &str) -> Corner {
    match s.to_ascii_lowercase().as_str() {
        "top-left" => Corner::TopLeft,
        "bottom-left" => Corner::BottomLeft,
        "bottom-right" => Corner::BottomRight,
        _ => Corner::TopRight,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Recording,
    Buffer,
}

struct State {
    hwnd: Option<HWND>,
    start: Instant,
    corner: Corner,
    dot_on: bool,
    registered: bool,
    dpi: i32,
    mode: Mode,
    max_sec: i32,
    clickable: bool,
    /// Escondido porque um app em tela cheia está na frente.
    fullscreen_hidden: bool,
    /// Pausada (só na gravação): o tempo para em `pause_at`.
    pause_at: Option<Instant>,
    on_click: Option<fn()>,
    gdiplus_token: usize,
}

thread_local! {
    static S: RefCell<State> = RefCell::new(State {
        hwnd: None,
        start: Instant::now(),
        corner: Corner::TopRight,
        dot_on: true,
        registered: false,
        dpi: 96,
        mode: Mode::Recording,
        max_sec: 0,
        clickable: true,
        fullscreen_hidden: false,
        pause_at: None,
        on_click: None,
        gdiplus_token: 0,
    });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    S.with(|s| f(&mut s.borrow_mut()))
}

/// Config `fullscreenDetect` (padrão ligado). Desligado, ninguém se esconde
/// nem pausa por tela cheia.
static FULLSCREEN_DETECT: AtomicBool = AtomicBool::new(true);

pub fn set_fullscreen_detect(enabled: bool) {
    FULLSCREEN_DETECT.store(enabled, Ordering::SeqCst);
}

/// Disparado (na main, DENTRO do `WndProc` do overlay) no clique. O bridge
/// decide pelo estado — parar ou salvar — e age DIFERIDO.
pub fn set_on_click(f: fn()) {
    with(|s| s.on_click = Some(f));
}

/// App em TELA CHEIA na frente? (jogo, inclusive sem borda; vídeo em tela
/// cheia). Qualquer thread. Tela cheia = a janela EM FOCO cobre o monitor
/// inteiro, ou D3D exclusivo. NÃO o `QUNS_BUSY`: ele conta qualquer janela
/// sempre-por-cima do tamanho do monitor — um app de marca d'água deixava o
/// indicador sumido e as prévias congeladas para sempre. Marca d'água nunca
/// tem o foco; jogo sempre tem.
pub fn fullscreen_app_active() -> bool {
    if !FULLSCREEN_DETECT.load(Ordering::SeqCst) {
        return false;
    }
    unsafe {
        if SHQueryUserNotificationState().ok() == Some(QUNS_RUNNING_D3D_FULL_SCREEN) {
            return true;
        }
        let fg = GetForegroundWindow();
        if fg.0.is_null() || !IsWindowVisible(fg).as_bool() || IsIconic(fg).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return false;
        }
        // Área de trabalho e barra de tarefas também são do tamanho do monitor.
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, &mut cls) as usize;
        let cls = String::from_utf16_lossy(&cls[..n]);
        if ["Progman", "WorkerW", "Shell_TrayWnd"].iter().any(|c| cls.eq_ignore_ascii_case(c)) {
            return false;
        }
        // Maximizada com barra de título: com a barra de tarefas oculta ela
        // cobre o monitor, mas navegador maximizado não é jogo.
        if IsZoomed(fg).as_bool() && (GetWindowLongW(fg, GWL_STYLE) as u32 & WS_CAPTION.0) == WS_CAPTION.0 {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        if mon.is_invalid() {
            return false;
        }
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

/// 20..100% → alfa 51..255 (abaixo de 20% seria invisível demais).
fn opacity_to_alpha(pct: i32) -> u8 {
    mul_div(pct.clamp(20, 100), 255, 100) as u8
}

/// `MulDiv` do Win32 (o que o Delphi usava): arredonda ao inteiro mais
/// próximo em vez de truncar — em 120 DPI, 6 px lógicos são 8, não 7.
fn mul_div(v: i32, num: i32, den: i32) -> i32 {
    let p = v as i64 * num as i64;
    let half = den as i64 / 2;
    (if p >= 0 { (p + half) / den as i64 } else { (p - half) / den as i64 }) as i32
}

fn sx(dpi: i32, v: i32) -> i32 {
    mul_div(v, dpi, 96)
}

fn elapsed_str(s: &State) -> String {
    let now = s.pause_at.unwrap_or_else(Instant::now);
    let secs = now.saturating_duration_since(s.start).as_secs();
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
}

/// "guardado / teto" (m:ss). O guardado satura no teto — é o que o buffer faz.
fn held_str(s: &State) -> String {
    let mut secs = s.start.elapsed().as_secs() as i64;
    if s.max_sec > 0 {
        secs = secs.min(s.max_sec as i64);
    }
    let max = s.max_sec as i64;
    format!("{}:{:02} / {}:{:02}", secs / 60, secs % 60, max / 60, max % 60)
}

/// GDI+ só para a bolinha com antialiasing (o `Ellipse` do GDI serrilha).
/// Iniciado sob demanda; `false` = indisponível (cai no GDI).
pub(crate) fn ensure_gdiplus() -> bool {
    if with(|s| s.gdiplus_token) != 0 {
        return true;
    }
    let input = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
    let mut token = 0usize;
    let ok = unsafe { GdiplusStartup(&mut token, &input, std::ptr::null_mut()) }.0 == 0;
    if ok {
        with(|s| s.gdiplus_token = token);
    }
    ok
}

fn fill_circle_aa(dc: HDC, cx: i32, cy: i32, rad: i32, (r, g, b): (u8, u8, u8)) -> bool {
    if !ensure_gdiplus() {
        return false;
    }
    unsafe {
        let mut gfx: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(dc, &mut gfx).0 != 0 {
            return false;
        }
        let _ = GdipSetSmoothingMode(gfx, SmoothingModeAntiAlias);
        let argb = 0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32;
        let mut brush: *mut GpSolidFill = std::ptr::null_mut();
        let ok = GdipCreateSolidFill(argb, &mut brush).0 == 0;
        if ok {
            let _ = GdipFillEllipse(gfx, brush as *mut GpBrush, (cx - rad) as f32, (cy - rad) as f32, (2 * rad) as f32, (2 * rad) as f32);
            let _ = GdipDeleteBrush(brush as *mut GpBrush);
        }
        let _ = GdipDeleteGraphics(gfx);
        ok
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

fn paint_to(dc: HDC, w: i32, h: i32) {
    let (mode, dot_on, paused, dpi, text) = with(|s| {
        let text = if s.mode == Mode::Buffer { held_str(s) } else { elapsed_str(s) };
        (s.mode, s.dot_on, s.pause_at.is_some(), s.dpi, text)
    });
    unsafe {
        // Fundo escuro do pill (os cantos vêm do SetWindowRgn).
        let bg = CreateSolidBrush(rgb(24, 24, 27));
        FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        let dot = match (mode, dot_on) {
            (Mode::Buffer, _) => (16, 185, 129),
            (_, true) => (240, 62, 62),
            (_, false) => (110, 34, 34),
        };
        let (rad, cx, cy) = (sx(dpi, 6), sx(dpi, 17), h / 2);
        if paused && mode == Mode::Recording {
            // Pausada: duas barras âmbar FIXAS (pulsar diria "gravando").
            let b = CreateSolidBrush(rgb(245, 158, 11));
            FillRect(dc, &RECT { left: cx - rad, top: cy - rad, right: cx - rad + sx(dpi, 4), bottom: cy + rad }, b);
            FillRect(dc, &RECT { left: cx + rad - sx(dpi, 4), top: cy - rad, right: cx + rad, bottom: cy + rad }, b);
            let _ = DeleteObject(HGDIOBJ(b.0));
        } else if !fill_circle_aa(dc, cx, cy, rad, dot) {
            let b = CreateSolidBrush(rgb(dot.0, dot.1, dot.2));
            let ob = SelectObject(dc, HGDIOBJ(b.0));
            let op = SelectObject(dc, GetStockObject(NULL_PEN));
            let _ = Ellipse(dc, cx - rad, cy - rad, cx + rad, cy + rad);
            SelectObject(dc, ob);
            SelectObject(dc, op);
            let _ = DeleteObject(HGDIOBJ(b.0));
        }

        let font = CreateFontW(-sx(dpi, 15), 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32, w!("Segoe UI"));
        let of = SelectObject(dc, HGDIOBJ(font.0));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, rgb(240, 240, 240));
        let mut r = RECT { left: cx + rad + sx(dpi, 8), top: 0, right: w - sx(dpi, 6), bottom: h };
        let mut t: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(dc, &mut t, &mut r, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
        SelectObject(dc, of);
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
}

/// `WM_PAINT` com double buffer, para o repaint de 500 ms não piscar.
fn do_paint(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right, rc.bottom);
        let mem = CreateCompatibleDC(Some(dc));
        let bmp = CreateCompatibleBitmap(dc, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        paint_to(mem, w, h);
        let _ = BitBlt(dc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

/// Esconde com tela cheia na frente, volta quando ela sai. Só na TRANSIÇÃO.
fn sync_fullscreen(hwnd: HWND) {
    let fs = fullscreen_app_active();
    if with(|s| std::mem::replace(&mut s.fullscreen_hidden, fs) == fs) {
        return;
    }
    unsafe {
        if fs {
            let _ = ShowWindow(hwnd, SW_HIDE);
            log::log("WinRecIndicator: escondido (app em tela cheia em primeiro plano).");
        } else {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
            log::log("WinRecIndicator: de volta (sem app em tela cheia).");
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            do_paint(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            sync_fullscreen(hwnd);
            // Pulso só na gravação; no buffer a bolinha fica acesa.
            let hidden = with(|s| {
                s.dot_on = s.mode == Mode::Buffer || !s.dot_on;
                s.fullscreen_hidden
            });
            if !hidden {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        // Clicar nunca rouba o foco da janela ativa.
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_SETCURSOR => {
            unsafe {
                let _ = SetCursor(LoadCursorW(None, IDC_HAND).ok());
            }
            LRESULT(1)
        }
        WM_LBUTTONUP => {
            // Quem decide é o bridge, pelo estado, e DIFERIDO: agir aqui
            // destruiria esta janela dentro do próprio WndProc.
            if let Some(f) = with(|s| s.on_click) {
                f();
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn hinstance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default() }
}

fn ensure_class() {
    if with(|s| std::mem::replace(&mut s.registered, true)) {
        return;
    }
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&wc);
    }
}

fn position_window(hwnd: HWND, w: i32, h: i32) {
    let (corner, dpi) = with(|s| (s.corner, s.dpi));
    unsafe {
        // Área de trabalho do monitor PRINCIPAL (sem a barra de tarefas).
        let mut wa = RECT::default();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut wa as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_err() {
            wa = RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) };
        }
        let m = sx(dpi, MARGIN_DIP);
        let (x, y) = match corner {
            Corner::TopLeft => (wa.left + m, wa.top + m),
            Corner::BottomLeft => (wa.left + m, wa.bottom - h - m),
            Corner::BottomRight => (wa.right - w - m, wa.bottom - h - m),
            Corner::TopRight => (wa.right - w - m, wa.top + m),
        };
        // Com jogo em tela cheia, só posiciona e deixa ESCONDIDA: era aqui que
        // salvar/trocar de modo trazia o overlay pra cima do jogo.
        if fullscreen_app_active() {
            if !with(|s| std::mem::replace(&mut s.fullscreen_hidden, true)) {
                log::log("WinRecIndicator: escondido (app em tela cheia em primeiro plano).");
            }
            let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOACTIVATE | SWP_NOZORDER | SWP_HIDEWINDOW);
        } else {
            with(|s| s.fullscreen_hidden = false);
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }
}

/// Opacidade ao vivo (slider das Configurações). No-op sem overlay.
pub fn set_opacity(pct: i32) {
    if let Some(hwnd) = with(|s| s.hwnd) {
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), opacity_to_alpha(pct), LWA_ALPHA);
        }
    }
}

/// `WS_EX_TRANSPARENT` numa janela LAYERED = o hit-test PULA a janela e o
/// clique cai no que está embaixo (o overlay pode cobrir um botão útil).
fn apply_clickable() {
    let Some((hwnd, clickable)) = with(|s| s.hwnd.map(|h| (h, s.clickable))) else { return };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let t = WS_EX_TRANSPARENT.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, if clickable { ex & !t } else { ex | t });
    }
}

pub fn set_clickable(clickable: bool) {
    with(|s| s.clickable = clickable);
    apply_clickable();
}

/// Pausada: o tempo congela e a bolinha vira o símbolo de pausa. `start` é o
/// início JÁ descontado da pausa (o bridge o empurra pra frente ao retomar).
pub fn set_paused(paused: bool, start: Instant) {
    let hwnd = with(|s| {
        s.start = start;
        if paused && s.pause_at.is_none() {
            s.pause_at = Some(Instant::now());
        } else if !paused {
            s.pause_at = None;
        }
        s.hwnd
    });
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

/// Mostra no canto do monitor principal contando desde `start`. Já visível:
/// reposiciona e reaplica, inclusive trocando de modo. `max_sec` é o teto do
/// buffer (só no modo buffer). Sem exclusão de captura no Windows, não mostra.
pub fn show(corner: Corner, start: Instant, opacity_pct: i32, mode: Mode, max_sec: i32, clickable: bool) {
    let dpi = unsafe { GetDpiForSystem() }.max(1) as i32;
    let existing = with(|s| {
        s.start = start;
        s.corner = corner;
        s.dot_on = true;
        // Mostrar de novo é sempre "correndo"; quem está pausado reaplica.
        s.pause_at = None;
        s.mode = mode;
        s.max_sec = max_sec;
        s.clickable = clickable;
        s.dpi = dpi;
        s.hwnd
    });
    let w = sx(dpi, if mode == Mode::Buffer { W_BUF_DIP } else { W_DIP });
    let h = sx(dpi, H_DIP);
    unsafe {
        if let Some(hwnd) = existing {
            // Trocar de modo muda a LARGURA: a região arredondada tem que ser
            // refeita junto, senão a antiga recorta o pill novo.
            let _ = SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, sx(dpi, 10), sx(dpi, 10));
            SetWindowRgn(hwnd, Some(rgn), false);
            position_window(hwnd, w, h);
            set_opacity(opacity_pct);
            apply_clickable();
            let _ = InvalidateRect(Some(hwnd), None, false);
            return;
        }
        ensure_class();
        // Nasce SEM WS_EX_TRANSPARENT (recebe o clique); o apply_clickable liga
        // o "atravessar" se o usuário desligou o clique.
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            CLASS_NAME,
            w!(""),
            WS_POPUP,
            0,
            0,
            w,
            h,
            None,
            None,
            Some(hinstance()),
            None,
        ) else {
            log::log("WinRecIndicator: CreateWindowEx falhou.");
            return;
        };
        // EXCLUI DA CAPTURA. Sem isso (Windows < 2004) o overlay apareceria
        // dentro da gravação: melhor não mostrar.
        if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).is_err() {
            log::log("WinRecIndicator: SetWindowDisplayAffinity falhou — Windows sem suporte a exclusao de captura; indicador NAO sera mostrado.");
            let _ = DestroyWindow(hwnd);
            return;
        }
        with(|s| s.hwnd = Some(hwnd));
        set_opacity(opacity_pct);
        apply_clickable();
        // A janela assume a posse da região.
        let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, sx(dpi, 10), sx(dpi, 10));
        SetWindowRgn(hwnd, Some(rgn), false);
        position_window(hwnd, w, h);
        SetTimer(Some(hwnd), TIMER_ID, TICK_MS, None);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    log!("WinRecIndicator: overlay mostrado (modo={mode:?}, canto={corner:?}, opac={opacity_pct}%, clique={clickable}).");
}

/// Esconde e destrói. Idempotente.
pub fn hide() {
    let Some(hwnd) = with(|s| {
        s.fullscreen_hidden = false;
        s.hwnd.take()
    }) else {
        return;
    };
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
        let _ = DestroyWindow(hwnd);
    }
    log::log("WinRecIndicator: overlay escondido.");
}

pub fn is_showing() -> bool {
    with(|s| s.hwnd.is_some())
}

/// Modo do overlay visível (só faz sentido com [`is_showing`]).
pub fn current_mode() -> Mode {
    with(|s| s.mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canto_invalido_vira_superior_direito() {
        assert_eq!(parse_corner("Bottom-Left"), Corner::BottomLeft);
        assert_eq!(parse_corner("meio"), Corner::TopRight);
    }

    #[test]
    fn opacidade_limitada() {
        assert_eq!(opacity_to_alpha(5), 51);
        assert_eq!(opacity_to_alpha(100), 255);
        assert_eq!(opacity_to_alpha(150), 255);
        assert_eq!(opacity_to_alpha(90), 230);
        assert_eq!(sx(120, 6), 8);
    }
}

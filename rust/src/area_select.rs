//! Seleção de ÁREA da tela ("Capturar... → Área").
//!
//! Escurece TODOS os monitores com uma janela só, do tamanho da área de
//! trabalho virtual, e o usuário arrasta o retângulo que quer gravar. Soltar o
//! botão confirma; Esc ou o botão direito cancelam.
//!
//! A janela é layered com DOIS atributos ao mesmo tempo: alfa (o escuro
//! translúcido) e uma cor-chave, que vira transparente de verdade — é ela que
//! "fura" o escuro no retângulo escolhido, então o usuário vê a área nítida.
//!
//! Sem buffer do tamanho da tela: em dois monitores 4K seriam ~66 MB por
//! quadro. O `WM_PAINT` pinta direto, só a região invalidada (o retângulo
//! antigo e o novo), e nunca pinta o escuro por cima do furo — por isso não
//! pisca.
//!
//! Coordenadas: pixels FÍSICOS (o processo é per-monitor DPI aware v2), na
//! área de trabalho virtual (origem no monitor principal, pode ser negativa).
//!
//! Main thread.

use std::cell::RefCell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::engine::AreaRect;
use crate::{lang, log, mainthread};

const CLASS_NAME: PCWSTR = w!("TNoOBSAreaSelect");
/// Cor-chave: vira transparente (o "furo" do escuro). Magenta puro não aparece
/// em nenhum desenho nosso.
const KEY: COLORREF = COLORREF(0x00FF_00FF);
/// Opacidade do escuro (0..255).
const DIM_ALPHA: u8 = 150;
/// Menor área aceita, em px físicos: arrastar menos que isso é clique perdido.
pub const MIN_SIZE: i32 = 32;

pub type OnSelected = fn(Option<AreaRect>);

#[derive(Default)]
struct State {
    registered: bool,
    hwnd: Option<HWND>,
    /// Canto superior esquerdo da área de trabalho virtual (cliente → tela).
    origin: (i32, i32),
    /// Onde o arrasto começou (cliente).
    anchor: Option<POINT>,
    /// Seleção atual (cliente), normalizada.
    sel: Option<RECT>,
    /// Onde vai a instrução antes do 1º clique (cliente): o monitor do cursor.
    hint_rect: RECT,
    hint_visible: bool,
    on_selected: Option<OnSelected>,
    /// Chave do texto de instrução (gravar agora ou só salvar a área).
    hint_key: &'static str,
    font: HFONT,
    dpi: i32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn hinstance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default() }
}

fn sx(dpi: i32, v: i32) -> i32 {
    (v * dpi + 48) / 96
}

/// Seleção aberta agora?
pub fn is_active() -> bool {
    #[cfg(test)]
    if TEST_PENDING.with(|p| p.get().is_some()) {
        return true;
    }
    with(|s| s.hwnd.is_some())
}

// Teste: a seleção de verdade escurece todos os monitores e espera o mouse.
// Aqui ela só fica "aberta" até o teste dizer o que o usuário escolheu.
#[cfg(test)]
thread_local! {
    static TEST_PENDING: std::cell::Cell<Option<OnSelected>> = const { std::cell::Cell::new(None) };
}

/// Teste: termina a seleção aberta como se o usuário tivesse arrastado
/// (`Some`) ou apertado Esc (`None`). `false` se não havia seleção aberta.
#[cfg(test)]
pub fn test_finish(result: Option<AreaRect>) -> bool {
    match TEST_PENDING.with(|p| p.take()) {
        Some(cb) => {
            cb(result);
            true
        }
        None => false,
    }
}

/// Abre a seleção. `false` se já há uma aberta ou a janela não pôde nascer.
pub fn start(on_selected: OnSelected) -> bool {
    start_with_hint(on_selected, "areaSelect.hint")
}

/// Como [`start`], com outra instrução na tela (ex.: definir a área salva).
pub fn start_with_hint(on_selected: OnSelected, hint_key: &'static str) -> bool {
    if is_active() {
        return false;
    }
    #[cfg(test)]
    {
        crate::testkit::record_effect(format!("area_select:{hint_key}"));
        TEST_PENDING.with(|p| p.set(Some(on_selected)));
        return true;
    }
    #[allow(unreachable_code)]
    ensure_class();
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let dpi = GetDpiForSystem().max(96) as i32;

        // Instrução no monitor onde o cursor está (é pra onde o usuário olha).
        let mut cur = POINT::default();
        let _ = GetCursorPos(&mut cur);
        let mon = MonitorFromPoint(cur, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let m = mi.rcMonitor;
        let hint = RECT { left: m.left - vx, top: m.top - vy, right: m.right - vx, bottom: m.bottom - vy };

        let font = CreateFontW(-sx(dpi, 18), 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, DEFAULT_PITCH.0 as u32, w!("Segoe UI"));

        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS_NAME,
            w!("NoOBS - area"),
            WS_POPUP,
            vx,
            vy,
            vw,
            vh,
            None,
            None,
            Some(hinstance()),
            None,
        ) else {
            log::log("AreaSelect: CreateWindowEx falhou.");
            let _ = DeleteObject(HGDIOBJ(font.0));
            return false;
        };
        let _ = SetLayeredWindowAttributes(hwnd, KEY, DIM_ALPHA, LWA_ALPHA | LWA_COLORKEY);
        with(|s| {
            s.hwnd = Some(hwnd);
            s.origin = (vx, vy);
            s.anchor = None;
            s.sel = None;
            s.hint_rect = hint;
            s.hint_visible = true;
            s.on_selected = Some(on_selected);
            s.hint_key = hint_key;
            s.font = font;
            s.dpi = dpi;
        });
        let _ = ShowWindow(hwnd, SW_SHOW);
        // Foco pra o Esc funcionar. Sem ele (trava de primeiro plano do
        // Windows) o botão direito ainda cancela.
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        log!("AreaSelect: aberta ({vw}x{vh} em {vx},{vy}).");
    }
    true
}

/// Fecha e avisa quem abriu. O callback sai pela fila da main: ele começa a
/// gravação (pesado) e não pode rodar dentro do `WndProc` desta janela.
fn finish(result: Option<AreaRect>) {
    let (hwnd, cb, font) = with(|s| (s.hwnd.take(), s.on_selected.take(), std::mem::take(&mut s.font)));
    if let Some(h) = hwnd {
        unsafe {
            let _ = ReleaseCapture();
            let _ = DestroyWindow(h);
        }
    }
    if !font.is_invalid() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
    }
    match result {
        Some(a) => log!("AreaSelect: area escolhida {}x{} em ({}, {}).", a.w, a.h, a.x, a.y),
        None => log::log("AreaSelect: cancelada."),
    }
    if let Some(cb) = cb {
        mainthread::queue(move || cb(result));
    }
}

fn norm(a: POINT, b: POINT) -> RECT {
    RECT { left: a.x.min(b.x), top: a.y.min(b.y), right: a.x.max(b.x), bottom: a.y.max(b.y) }
}

fn inflate(r: RECT, d: i32) -> RECT {
    RECT { left: r.left - d, top: r.top - d, right: r.right + d, bottom: r.bottom + d }
}

/// Faixa do rótulo "1280 × 720", acima da seleção (ou abaixo, colada no topo).
fn label_rect(sel: RECT, dpi: i32) -> RECT {
    let (w, h, gap) = (sx(dpi, 120), sx(dpi, 28), sx(dpi, 6));
    let top = if sel.top - h - gap >= 0 { sel.top - h - gap } else { sel.bottom + gap };
    RECT { left: sel.left, top, right: sel.left + w, bottom: top + h }
}

/// Tudo o que a seleção desenha (furo + borda + rótulo), pra invalidar.
fn painted_bounds(sel: RECT, dpi: i32) -> [RECT; 2] {
    [inflate(sel, sx(dpi, 3)), label_rect(sel, dpi)]
}

fn invalidate(hwnd: HWND, r: &RECT) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), Some(r), false);
    }
}

fn set_selection(hwnd: HWND, new_sel: Option<RECT>) {
    let (old, dpi) = with(|s| (std::mem::replace(&mut s.sel, new_sel), s.dpi));
    for r in old.iter().chain(new_sel.iter()) {
        for b in painted_bounds(*r, dpi) {
            invalidate(hwnd, &b);
        }
    }
}

fn paint(hwnd: HWND) {
    let (sel, dpi, font, hint_rect, hint_visible) = with(|s| (s.sel, s.dpi, s.font, s.hint_rect, s.hint_visible));
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let black = CreateSolidBrush(COLORREF(0));
        let key = CreateSolidBrush(KEY);
        let white = CreateSolidBrush(COLORREF(0x00FF_FFFF));
        let border = sx(dpi, 2).max(1);

        // 1. Escuro, SEM passar por cima do furo + borda (senão pisca).
        let saved = SaveDC(dc);
        if let Some(r) = sel {
            let o = inflate(r, border);
            ExcludeClipRect(dc, o.left, o.top, o.right, o.bottom);
        }
        FillRect(dc, &ps.rcPaint, black);
        let _ = RestoreDC(dc, saved);

        if let Some(r) = sel {
            // 2. Furo (cor-chave = transparente) e borda branca por FORA dele.
            FillRect(dc, &r, key);
            let o = inflate(r, border);
            for side in [
                RECT { left: o.left, top: o.top, right: o.right, bottom: r.top },
                RECT { left: o.left, top: r.bottom, right: o.right, bottom: o.bottom },
                RECT { left: o.left, top: r.top, right: r.left, bottom: r.bottom },
                RECT { left: r.right, top: r.top, right: o.right, bottom: r.bottom },
            ] {
                FillRect(dc, &side, white);
            }
            // 3. Tamanho, em px físicos — é o tamanho do vídeo (arredondado
            // pro múltiplo de 16 na gravação).
            let lr = label_rect(r, dpi);
            FillRect(dc, &lr, black);
            let old_font = SelectObject(dc, HGDIOBJ(font.0));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            let mut text: Vec<u16> = format!("{} × {}", r.right - r.left, r.bottom - r.top).encode_utf16().collect();
            let mut lr2 = lr;
            DrawTextW(dc, &mut text, &mut lr2, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            SelectObject(dc, old_font);
        } else if hint_visible {
            // Instrução no meio do monitor do cursor, até o 1º clique.
            let old_font = SelectObject(dc, HGDIOBJ(font.0));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            let mut text: Vec<u16> = lang::t(with(|s| s.hint_key)).encode_utf16().collect();
            let mut hr = hint_rect;
            DrawTextW(dc, &mut text, &mut hr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            SelectObject(dc, old_font);
        }

        let _ = DeleteObject(HGDIOBJ(black.0));
        let _ = DeleteObject(HGDIOBJ(key.0));
        let _ = DeleteObject(HGDIOBJ(white.0));
        let _ = EndPaint(hwnd, &ps);
    }
}

fn point_from(lparam: LPARAM) -> POINT {
    // GET_X/Y_LPARAM: com sinal (monitor à esquerda do principal).
    POINT { x: (lparam.0 & 0xFFFF) as i16 as i32, y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SETCURSOR => {
            unsafe {
                let _ = SetCursor(LoadCursorW(None, IDC_CROSS).ok());
            }
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            // Arrasto já começou: um 2º "aperto" (visto no teste: chega um
            // logo depois do 1º, quando a janela é ativada pelo clique) não
            // pode reiniciar a seleção — o ponto de partida é o primeiro.
            if with(|s| s.anchor.is_some()) {
                return LRESULT(0);
            }
            let p = point_from(lparam);
            let origin = with(|s| s.origin);
            log!("AreaSelect: arrasto comeca em ({}, {}).", p.x + origin.0, p.y + origin.1);
            let hint = with(|s| {
                s.anchor = Some(p);
                std::mem::replace(&mut s.hint_visible, false).then_some(s.hint_rect)
            });
            if let Some(h) = hint {
                invalidate(hwnd, &h);
            }
            unsafe {
                SetCapture(hwnd);
            }
            set_selection(hwnd, Some(RECT { left: p.x, top: p.y, right: p.x, bottom: p.y }));
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(a) = with(|s| s.anchor) {
                set_selection(hwnd, Some(norm(a, point_from(lparam))));
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (anchor, origin) = with(|s| (s.anchor.take(), s.origin));
            if let Some(a) = anchor {
                unsafe {
                    let _ = ReleaseCapture();
                }
                let r = norm(a, point_from(lparam));
                let (w, h) = (r.right - r.left, r.bottom - r.top);
                if w >= MIN_SIZE && h >= MIN_SIZE {
                    finish(Some(AreaRect { x: r.left + origin.0, y: r.top + origin.1, w, h }));
                } else {
                    // Clique sem arrasto: volta a esperar.
                    set_selection(hwnd, None);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            finish(None);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u16 == VK_ESCAPE.0 => {
            finish(None);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn ensure_class() {
    if with(|s| std::mem::replace(&mut s.registered, true)) {
        return;
    }
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance(),
        hCursor: unsafe { LoadCursorW(None, IDC_CROSS).unwrap_or_default() },
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&wc);
    }
}

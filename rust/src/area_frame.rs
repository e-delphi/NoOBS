//! Moldura da gravação de ÁREA ("Capturar... → Área"), com o botão que libera
//! mover/redimensionar a área DURANTE a gravação.
//!
//! Três janelas topmost, todas fora da captura (`WDA_EXCLUDEFROMCAPTURE`):
//! * **moldura** — borda vermelha por FORA da área (nunca entra no vídeo),
//!   sempre "atravessável" pelo mouse;
//! * **botão** — um círculo num canto. O mouse NÃO interage com ele: ele só
//!   deixa de ser atravessável enquanto Ctrl+Shift estão pressionados (lidos
//!   a cada 50 ms). Ctrl+Shift+clique entra/sai do modo de edição. Os dois
//!   juntos, e não só o Ctrl: Ctrl+clique é gesto comum em outros apps
//!   (selecionar, abrir link) e acionaria a moldura por acidente;
//! * **edição** — só existe no modo de edição: um véu INVISÍVEL (alfa 1)
//!   sobre a área que recebe o mouse — escurecer atrapalhava ver o que está
//!   sendo gravado (arrastar o meio move, as bordas/cantos redimensionam;
//!   setas movem 1 px, Shift+setas 10 px; Enter/Esc terminam).
//!
//! Cada mudança vai pro callback na hora (o engine troca recorte/posição das
//! capturas) e de novo no fim do arrasto com `final = true`.
//!
//! Com app em tela cheia na frente as três somem, como o indicador (#49): uma
//! janela por cima do jogo custa FPS.
//!
//! Main thread.

use std::cell::RefCell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::GdiPlus::{
    GdipCreateBitmapFromScan0, GdipCreateFont, GdipCreateFontFamilyFromName, GdipCreatePen1, GdipCreateStringFormat,
    GdipDeleteFont, GdipDeleteFontFamily, GdipDeleteStringFormat, GdipDrawString, GdipSetStringFormatAlign,
    GdipSetStringFormatLineAlign, GdipSetTextRenderingHint, FontStyleBold, GpFont, GpFontFamily, GpStringFormat, RectF,
    StringAlignmentCenter, TextRenderingHintAntiAliasGridFit, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePen,
    GdipDisposeImage, GdipDrawLine, GdipFillEllipse, GdipGetImageGraphicsContext, GdipGraphicsClear, GdipSetPenEndCap,
    GdipSetPenStartCap, GdipSetSmoothingMode, GpBitmap, GpBrush, GpGraphics, GpImage, GpPen, GpSolidFill, LineCapRound,
    SmoothingModeAntiAlias, UnitPixel,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT,
    VK_RETURN, VK_RIGHT, VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::engine::AreaRect;
use crate::{log, rec_indicator};

const FRAME_CLASS: PCWSTR = w!("TNoOBSAreaFrame");
const BUTTON_CLASS: PCWSTR = w!("TNoOBSAreaButton");
const EDIT_CLASS: PCWSTR = w!("TNoOBSAreaEdit");
/// Vermelho da gravação / âmbar da edição (ARGB pré-multiplicado, opaco).
const RED: u32 = 0xFFE5_3935;
const AMBER: u32 = 0xFFFF_B300;
const TIMER_CTRL: usize = 1;
const CTRL_POLL_MS: u32 = 50;
/// A cada N tiques do teclado (= 500 ms) reavalia o "app em tela cheia".
const FULLSCREEN_EVERY: u32 = 10;
/// Menor área durante a edição, em px físicos.
const MIN_EDIT: i32 = 64;
/// `PixelFormat32bppPARGB` (gdipluspixelformats.h) — não exportado pela crate.
const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;

/// `(área, final)`: `final = true` no fim de um arrasto/edição.
pub type OnChanged = fn(AreaRect, bool);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Drag {
    Move,
    /// Bordas: bit 1 esquerda, 2 direita, 4 topo, 8 base.
    Resize(u8),
}

#[derive(Default)]
struct State {
    registered: bool,
    area: AreaRect,
    frame: Option<HWND>,
    button: Option<HWND>,
    edit: Option<HWND>,
    editing: bool,
    ctrl: bool,
    fs_hidden: bool,
    ticks: u32,
    /// Exclusão da captura funcionou? O véu da edição também é excluído
    /// quando dá (ele é invisível de qualquer jeito, alfa 1).
    excluded: bool,
    dpi: i32,
    on_changed: Option<OnChanged>,
    drag: Option<(Drag, POINT, AreaRect)>,
    moved: bool,
    /// Contagem regressiva antes de a gravação começar (atraso da captura de
    /// área): o botão mostra os segundos e a edição fica travada.
    countdown: Option<u32>,
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

/// Espaço por fora da área onde cabem a borda e as alças.
fn pad(dpi: i32) -> i32 {
    sx(dpi, 7)
}

fn border(dpi: i32) -> i32 {
    sx(dpi, 2).max(1)
}

fn button_size(dpi: i32) -> i32 {
    sx(dpi, 30)
}

fn virtual_screen() -> RECT {
    unsafe {
        let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        RECT { left: x, top: y, right: x + GetSystemMetrics(SM_CXVIRTUALSCREEN), bottom: y + GetSystemMetrics(SM_CYVIRTUALSCREEN) }
    }
}

/// Mostra a moldura em volta de `area` (início da gravação de área).
pub fn show(area: AreaRect, on_changed: OnChanged) {
    hide();
    ensure_classes();
    let dpi = unsafe { GetDpiForSystem() }.max(96) as i32;
    with(|s| {
        s.area = area;
        s.dpi = dpi;
        s.on_changed = Some(on_changed);
        s.editing = false;
        s.ctrl = false;
        s.fs_hidden = false;
        s.ticks = 0;
        s.drag = None;
        s.countdown = None;
    });
    unsafe {
        let ex = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
        let Ok(frame) = CreateWindowExW(ex, FRAME_CLASS, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, Some(hinstance()), None) else {
            log::log("AreaFrame: CreateWindowEx (moldura) falhou.");
            return;
        };
        // A borda fica por FORA da área (não entra no vídeo nem sem a
        // exclusão); excluir é pras alças e pro véu da edição.
        let excluded = SetWindowDisplayAffinity(frame, WDA_EXCLUDEFROMCAPTURE).is_ok();
        let Ok(button) = CreateWindowExW(ex, BUTTON_CLASS, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, Some(hinstance()), None) else {
            log::log("AreaFrame: CreateWindowEx (botao) falhou.");
            let _ = DestroyWindow(frame);
            return;
        };
        let _ = SetWindowDisplayAffinity(button, WDA_EXCLUDEFROMCAPTURE);
        with(|s| {
            s.frame = Some(frame);
            s.button = Some(button);
            s.excluded = excluded;
        });
        layout();
        let _ = ShowWindow(frame, SW_SHOWNOACTIVATE);
        let _ = ShowWindow(button, SW_SHOWNOACTIVATE);
        draw_button();
        SetTimer(Some(frame), TIMER_CTRL, CTRL_POLL_MS, None);
    }
    log!("AreaFrame: mostrada em volta de {}x{} ({}, {}).", area.w, area.h, area.x, area.y);
}

/// Some com tudo (fim da gravação). Idempotente.
pub fn hide() {
    let (frame, button, edit) = with(|s| {
        s.editing = false;
        s.drag = None;
        s.on_changed = None;
        s.countdown = None;
        (s.frame.take(), s.button.take(), s.edit.take())
    });
    unsafe {
        for h in [edit, button, frame].into_iter().flatten() {
            let _ = DestroyWindow(h);
        }
    }
    if frame.is_some() {
        log::log("AreaFrame: escondida.");
    }
}

/// Contagem regressiva no botão (`Some(segundos)`) ou fim dela (`None`, a
/// gravação começou). Durante a contagem a moldura fica âmbar e o
/// Ctrl+Shift+clique não abre a edição: ainda não há gravação pra ajustar.
pub fn set_countdown(n: Option<u32>) {
    let (frame, changed) = with(|s| (s.frame, std::mem::replace(&mut s.countdown, n) != n));
    if frame.is_none() || !changed {
        return;
    }
    draw_frame();
    draw_button();
}

/// Reposiciona as janelas pra área atual.
fn layout() {
    let (area, dpi, frame, button, edit) = with(|s| (s.area, s.dpi, s.frame, s.button, s.edit));
    let p = pad(dpi);
    let flags = SWP_NOACTIVATE | SWP_NOZORDER;
    unsafe {
        if frame.is_some() {
            draw_frame();
        }
        if let Some(e) = edit {
            let _ = SetWindowPos(e, None, area.x - p, area.y - p, area.w + 2 * p, area.h + 2 * p, flags);
        }
        if let Some(b) = button {
            let (x, y) = button_pos(area, dpi);
            let n = button_size(dpi);
            let _ = SetWindowPos(b, None, x, y, n, n, flags);
        }
    }
}

/// Canto superior direito, por fora; sem espaço acima, por baixo; sem
/// nenhum dos dois (área da altura da tela), dentro do canto.
fn button_pos(area: AreaRect, dpi: i32) -> (i32, i32) {
    let (n, p, gap) = (button_size(dpi), pad(dpi), sx(dpi, 2));
    let vs = virtual_screen();
    let x = (area.x + area.w - n).clamp(vs.left, vs.right - n);
    let above = area.y - p - gap - n;
    if above >= vs.top {
        return (x, above);
    }
    let below = area.y + area.h + p + gap;
    if below + n <= vs.bottom {
        return (x, below);
    }
    (x - gap, area.y + gap)
}

// ---------------------------------------------------------------------------
// Moldura
// ---------------------------------------------------------------------------

/// Desenha a moldura com `UpdateLayeredWindow`, que troca posição, tamanho
/// e imagem NUMA operação só. Antes era cor-chave + `WM_PAINT`: ao arrastar
/// uma borda a janela crescia primeiro e o pedaço novo aparecia PRETO até a
/// pintura chegar — um piscar escuro sobre a área a cada movimento.
/// Fora da borda e das alças os pixels são alfa 0 (o mouse atravessa).
fn draw_frame() {
    let (frame, area, dpi, editing, counting) = with(|s| (s.frame, s.area, s.dpi, s.editing, s.countdown.is_some()));
    let Some(hwnd) = frame else { return };
    let (p, b) = (pad(dpi), border(dpi));
    let (w, h) = (area.w + 2 * p, area.h + 2 * p);
    if w <= 0 || h <= 0 {
        return;
    }
    let color = if editing || counting { AMBER } else { RED };
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
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
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return;
        };
        // O DIB nasce zerado (transparente): só a borda e as alças são escritas,
        // então o custo é o do contorno, não o da área inteira.
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        let mut fill = |r: RECT| {
            let (l, t) = (r.left.max(0), r.top.max(0));
            let (rr, bb) = (r.right.min(w), r.bottom.min(h));
            for y in t..bb {
                let row = (y * w) as usize;
                px[row + l as usize..row + rr.max(l) as usize].fill(color);
            }
        };
        // Borda por fora da área: [p-b, p) de cada lado.
        let inner = RECT { left: p, top: p, right: p + area.w, bottom: p + area.h };
        let outer = RECT { left: p - b, top: p - b, right: p + area.w + b, bottom: p + area.h + b };
        fill(RECT { left: outer.left, top: outer.top, right: outer.right, bottom: inner.top });
        fill(RECT { left: outer.left, top: inner.bottom, right: outer.right, bottom: outer.bottom });
        fill(RECT { left: outer.left, top: inner.top, right: inner.left, bottom: inner.bottom });
        fill(RECT { left: inner.right, top: inner.top, right: outer.right, bottom: inner.bottom });
        // Editando: alças nos cantos e no meio de cada lado (por fora da área).
        if editing {
            let (cx, cy) = (p + area.w / 2, p + area.h / 2);
            let hs = p;
            for (x, y) in [
                (0, 0),
                (p + area.w, 0),
                (0, p + area.h),
                (p + area.w, p + area.h),
                (cx - hs / 2, 0),
                (cx - hs / 2, p + area.h),
                (0, cy - hs / 2),
                (p + area.w, cy - hs / 2),
            ] {
                fill(RECT { left: x, top: y, right: x + hs, bottom: y + hs });
            }
        }
        let old = SelectObject(mem, HGDIOBJ(dib.0));
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let dst = POINT { x: area.x - p, y: area.y - p };
        let size = SIZE { cx: w, cy: h };
        let src = POINT { x: 0, y: 0 };
        let _ = UpdateLayeredWindow(hwnd, Some(screen), Some(&dst), Some(&size), Some(mem), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
}

extern "system" fn frame_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wparam.0 == TIMER_CTRL => {
            on_tick();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Ctrl+Shift pressionados AGORA (estado físico, não da fila de mensagens).
fn ctrl_shift_down() -> bool {
    let down = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0;
    down(VK_CONTROL.0) && down(VK_SHIFT.0)
}

/// 50 ms: Ctrl+Shift pressionados liberam o botão; a cada 500 ms, tela cheia.
fn on_tick() {
    // Na contagem regressiva o botão não vira clicável (nada pra editar ainda).
    let ctrl = ctrl_shift_down() && with(|s| s.countdown.is_none());
    let changed = with(|s| std::mem::replace(&mut s.ctrl, ctrl) != ctrl);
    if changed {
        set_button_clickable(ctrl);
        draw_button();
    }
    let check = with(|s| {
        s.ticks = s.ticks.wrapping_add(1);
        s.ticks % FULLSCREEN_EVERY == 0
    });
    if check {
        let fs = rec_indicator::fullscreen_app_active();
        let (was, frame, button, edit) = with(|s| (std::mem::replace(&mut s.fs_hidden, fs), s.frame, s.button, s.edit));
        if was != fs {
            let cmd = if fs { SW_HIDE } else { SW_SHOWNOACTIVATE };
            unsafe {
                for h in [frame, button, edit].into_iter().flatten() {
                    let _ = ShowWindow(h, cmd);
                }
            }
            if fs {
                log!("AreaFrame: escondida (app em tela cheia: {}).", rec_indicator::foreground_desc());
            } else {
                log!("AreaFrame: de volta (saiu a tela cheia).");
            }
        }
        // Outra janela "sempre por cima" ativada depois passa na frente da
        // moldura (e ela ficava escondida atrás do programa gravado): volta pro
        // topo. Só fora da tela cheia — por cima de jogo custa quadros (#49).
        if !fs {
            unsafe {
                for h in [frame, button, edit].into_iter().flatten() {
                    let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Botão (Ctrl+Shift+clique)
// ---------------------------------------------------------------------------

/// Sem Ctrl+Shift o botão é atravessável (`WS_EX_TRANSPARENT`): o clique cai no que
/// está embaixo, como se ele não existisse.
fn set_button_clickable(on: bool) {
    let Some(b) = with(|s| s.button) else { return };
    unsafe {
        let ex = GetWindowLongPtrW(b, GWL_EXSTYLE);
        let t = WS_EX_TRANSPARENT.0 as isize;
        let new = if on { ex & !t } else { ex | t };
        if new != ex {
            SetWindowLongPtrW(b, GWL_EXSTYLE, new);
            let _ = SetWindowPos(b, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
    }
}

/// Desenha o botão com alfa por pixel (GDI+ num DIB PARGB + UpdateLayeredWindow):
/// círculo liso, sem a franja que a cor-chave deixaria na borda suavizada.
fn draw_button() {
    let (button, dpi, editing, ctrl, countdown) = with(|s| (s.button, s.dpi, s.editing, s.ctrl, s.countdown));
    let Some(hwnd) = button else { return };
    if !rec_indicator::ensure_gdiplus() {
        return;
    }
    let n = button_size(dpi);
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: n,
                biHeight: -n,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return;
        };
        let old = SelectObject(mem, HGDIOBJ(dib.0));

        let mut bmp: *mut GpBitmap = std::ptr::null_mut();
        let mut g: *mut GpGraphics = std::ptr::null_mut();
        GdipCreateBitmapFromScan0(n, n, n * 4, PIXEL_FORMAT_32BPP_PARGB, Some(bits as *const u8), &mut bmp);
        if !bmp.is_null() {
            GdipGetImageGraphicsContext(bmp as *mut GpImage, &mut g);
        }
        if !g.is_null() {
            GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
            GdipGraphicsClear(g, 0);
            let f = n as f32;
            // Anel: âmbar editando, branco com Ctrl+Shift (clicável), vermelho parado.
            let ring = if editing || countdown.is_some() { 0xFFFF_B300 } else if ctrl { 0xFFFF_FFFF } else { 0xFFE5_3935 };
            fill_circle(g, ring, 0.0, 0.0, f);
            let inset = f * 0.12;
            fill_circle(g, 0xFF1E_1E1E, inset, inset, f - 2.0 * inset);
            if let Some(n) = countdown {
                draw_number(g, n, f);
            }
            // Na contagem o número ocupa o botão: sem o ícone (lápis nulo).
            let mut pen: *mut GpPen = std::ptr::null_mut();
            if countdown.is_none() {
                GdipCreatePen1(0xFFFF_FFFF, (f * 0.08).max(1.5), UnitPixel, &mut pen);
            }
            if !pen.is_null() {
                GdipSetPenStartCap(pen, LineCapRound);
                GdipSetPenEndCap(pen, LineCapRound);
                let c = f / 2.0;
                if editing {
                    // Concluir: ✓.
                    GdipDrawLine(g, pen, c - f * 0.18, c, c - f * 0.04, c + f * 0.14);
                    GdipDrawLine(g, pen, c - f * 0.04, c + f * 0.14, c + f * 0.2, c - f * 0.14);
                } else {
                    // Mover/redimensionar: cruz com setas.
                    let r = f * 0.24;
                    let a = f * 0.08;
                    GdipDrawLine(g, pen, c - r, c, c + r, c);
                    GdipDrawLine(g, pen, c, c - r, c, c + r);
                    for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                        let (tx, ty) = (c + dx * r, c + dy * r);
                        // Duas pernas da seta, perpendiculares ao eixo.
                        let (px, py) = (dy, dx);
                        GdipDrawLine(g, pen, tx, ty, tx - dx * a + px * a, ty - dy * a + py * a);
                        GdipDrawLine(g, pen, tx, ty, tx - dx * a - px * a, ty - dy * a - py * a);
                    }
                }
                GdipDeletePen(pen);
            }
            GdipDeleteGraphics(g);
        }
        if !bmp.is_null() {
            GdipDisposeImage(bmp as *mut GpImage);
        }

        // Sem Ctrl+Shift o botão fica mais apagado: lembra que não é clicável.
        let alpha = if ctrl || editing || countdown.is_some() { 255 } else { 150 };
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: AC_SRC_ALPHA as u8 };
        let size = SIZE { cx: n, cy: n };
        let src = POINT { x: 0, y: 0 };
        let _ = UpdateLayeredWindow(hwnd, Some(screen), None, Some(&size), Some(mem), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);

        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
}

/// Os segundos que faltam, centralizados no botão.
unsafe fn draw_number(g: *mut GpGraphics, n: u32, f: f32) {
    let text: Vec<u16> = n.to_string().encode_utf16().collect();
    let mut family: *mut GpFontFamily = std::ptr::null_mut();
    GdipCreateFontFamilyFromName(w!("Segoe UI"), std::ptr::null_mut(), &mut family);
    if family.is_null() {
        return;
    }
    let size = if text.len() > 1 { f * 0.38 } else { f * 0.5 };
    let mut font: *mut GpFont = std::ptr::null_mut();
    GdipCreateFont(family, size, FontStyleBold.0, UnitPixel, &mut font);
    let mut fmt: *mut GpStringFormat = std::ptr::null_mut();
    GdipCreateStringFormat(0, 0, &mut fmt);
    let mut brush: *mut GpSolidFill = std::ptr::null_mut();
    GdipCreateSolidFill(0xFFFF_FFFF, &mut brush);
    if !font.is_null() && !fmt.is_null() && !brush.is_null() {
        GdipSetStringFormatAlign(fmt, StringAlignmentCenter);
        GdipSetStringFormatLineAlign(fmt, StringAlignmentCenter);
        GdipSetTextRenderingHint(g, TextRenderingHintAntiAliasGridFit);
        let rect = RectF { X: 0.0, Y: 0.0, Width: f, Height: f };
        GdipDrawString(g, PCWSTR(text.as_ptr()), text.len() as i32, font, &rect, fmt, brush as *mut GpBrush);
    }
    if !brush.is_null() {
        GdipDeleteBrush(brush as *mut GpBrush);
    }
    if !fmt.is_null() {
        GdipDeleteStringFormat(fmt);
    }
    if !font.is_null() {
        GdipDeleteFont(font);
    }
    GdipDeleteFontFamily(family);
}

unsafe fn fill_circle(g: *mut GpGraphics, argb: u32, x: f32, y: f32, d: f32) {
    let mut brush: *mut GpSolidFill = std::ptr::null_mut();
    GdipCreateSolidFill(argb, &mut brush);
    if !brush.is_null() {
        GdipFillEllipse(g, brush as *mut GpBrush, x, y, d, d);
        GdipDeleteBrush(brush as *mut GpBrush);
    }
}

extern "system" fn button_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_SETCURSOR => {
            unsafe {
                let _ = SetCursor(LoadCursorW(None, IDC_HAND).ok());
            }
            LRESULT(1)
        }
        WM_LBUTTONUP => {
            // Só chega aqui com Ctrl+Shift pressionados (sem eles o botão é
            // atravessável); confere de novo pra um clique "atrasado" não
            // valer depois de soltar as teclas.
            let held = |vk: u16| unsafe { GetKeyState(vk as i32) } < 0;
            if held(VK_CONTROL.0) && held(VK_SHIFT.0) {
                toggle_edit();
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

// ---------------------------------------------------------------------------
// Modo de edição
// ---------------------------------------------------------------------------

fn toggle_edit() {
    if with(|s| s.editing) {
        end_edit();
    } else {
        begin_edit();
    }
}

fn begin_edit() {
    let (area, dpi, excluded, frame) = with(|s| (s.area, s.dpi, s.excluded, s.frame));
    if frame.is_none() {
        return;
    }
    let p = pad(dpi);
    unsafe {
        let Ok(edit) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            EDIT_CLASS,
            w!("NoOBS - area"),
            WS_POPUP,
            area.x - p,
            area.y - p,
            area.w + 2 * p,
            area.h + 2 * p,
            None,
            None,
            Some(hinstance()),
            None,
        ) else {
            log::log("AreaFrame: CreateWindowEx (edicao) falhou.");
            return;
        };
        // Véu invisível (alfa 1: a janela layered só recebe o mouse com alfa
        // > 0) e fora da captura. A moldura âmbar e as alças já dizem que a
        // área está em edição; escurecer escondia justamente o conteúdo.
        if excluded {
            let _ = SetWindowDisplayAffinity(edit, WDA_EXCLUDEFROMCAPTURE);
        }
        let _ = SetLayeredWindowAttributes(edit, COLORREF(0), 1, LWA_ALPHA);
        with(|s| {
            s.edit = Some(edit);
            s.editing = true;
            s.moved = false;
        });
        let _ = ShowWindow(edit, SW_SHOW);
        // O botão fica por cima do véu.
        if let Some(b) = with(|s| s.button) {
            let _ = SetWindowPos(b, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        let _ = SetForegroundWindow(edit);
        let _ = SetFocus(Some(edit));
    }
    if frame.is_some() {
        draw_frame();
    }
    draw_button();
    log::log("AreaFrame: edicao ligada (Ctrl+Shift+clique no botao).");
}

fn end_edit() {
    let (edit, frame, area, cb) = with(|s| {
        s.editing = false;
        s.drag = None;
        (s.edit.take(), s.frame, s.area, s.on_changed)
    });
    unsafe {
        let _ = ReleaseCapture();
        if let Some(e) = edit {
            let _ = DestroyWindow(e);
        }
    }
    if frame.is_some() {
        draw_frame();
    }
    draw_button();
    log!("AreaFrame: edicao encerrada — area {}x{} em ({}, {}).", area.w, area.h, area.x, area.y);
    if let Some(cb) = cb {
        cb(area, true);
    }
}

/// Aplica uma área nova: janelas + engine (ao vivo).
fn set_area(new: AreaRect) {
    let (changed, cb) = with(|s| {
        let c = s.area != new;
        s.area = new;
        if c {
            s.moved = true;
        }
        (c, s.on_changed)
    });
    if !changed {
        return;
    }
    layout();
    if let Some(cb) = cb {
        cb(new, false);
    }
}

/// Zona do véu sob o cursor: bordas (com folga) redimensionam, o meio move.
fn hit(client: POINT) -> Drag {
    let (area, dpi) = with(|s| (s.area, s.dpi));
    let p = pad(dpi);
    let m = p + sx(dpi, 6);
    let (w, h) = (area.w + 2 * p, area.h + 2 * p);
    let mut bits = 0u8;
    if client.x < m {
        bits |= 1;
    } else if client.x >= w - m {
        bits |= 2;
    }
    if client.y < m {
        bits |= 4;
    } else if client.y >= h - m {
        bits |= 8;
    }
    if bits == 0 {
        Drag::Move
    } else {
        Drag::Resize(bits)
    }
}

fn cursor_for(d: Drag) -> PCWSTR {
    match d {
        Drag::Move => IDC_SIZEALL,
        Drag::Resize(1) | Drag::Resize(2) => IDC_SIZEWE,
        Drag::Resize(4) | Drag::Resize(8) => IDC_SIZENS,
        Drag::Resize(5) | Drag::Resize(10) => IDC_SIZENWSE,
        Drag::Resize(_) => IDC_SIZENESW,
    }
}

/// Área nova a partir do arrasto (`dx`,`dy` desde o início), presa na área
/// de trabalho virtual e com tamanho mínimo.
pub fn drag_area(start: AreaRect, kind_bits: Option<u8>, dx: i32, dy: i32, vs: RECT) -> AreaRect {
    let Some(bits) = kind_bits else {
        let x = (start.x + dx).clamp(vs.left, (vs.right - start.w).max(vs.left));
        let y = (start.y + dy).clamp(vs.top, (vs.bottom - start.h).max(vs.top));
        return AreaRect { x, y, ..start };
    };
    let (mut l, mut t, mut r, mut b) = (start.x, start.y, start.x + start.w, start.y + start.h);
    if bits & 1 != 0 {
        l = (l + dx).clamp(vs.left, r - MIN_EDIT);
    }
    if bits & 2 != 0 {
        r = (r + dx).clamp(l + MIN_EDIT, vs.right);
    }
    if bits & 4 != 0 {
        t = (t + dy).clamp(vs.top, b - MIN_EDIT);
    }
    if bits & 8 != 0 {
        b = (b + dy).clamp(t + MIN_EDIT, vs.bottom);
    }
    AreaRect { x: l, y: t, w: r - l, h: b - t }
}

fn screen_point(lparam: LPARAM, hwnd: HWND) -> POINT {
    let mut p = POINT { x: (lparam.0 & 0xFFFF) as i16 as i32, y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 };
    unsafe {
        let _ = ClientToScreen(hwnd, &mut p);
    }
    p
}

extern "system" fn edit_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SETCURSOR => {
            let mut p = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut p);
                let _ = ScreenToClient(hwnd, &mut p);
            }
            let d = with(|s| s.drag.map(|(k, _, _)| k)).unwrap_or_else(|| hit(p));
            unsafe {
                let _ = SetCursor(LoadCursorW(None, cursor_for(d)).ok());
            }
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            let client = POINT { x: (lparam.0 & 0xFFFF) as i16 as i32, y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 };
            let kind = hit(client);
            let sp = screen_point(lparam, hwnd);
            with(|s| s.drag = Some((kind, sp, s.area)));
            unsafe {
                SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some((kind, start_pt, start_area)) = with(|s| s.drag) {
                let sp = screen_point(lparam, hwnd);
                let bits = match kind {
                    Drag::Move => None,
                    Drag::Resize(b) => Some(b),
                };
                set_area(drag_area(start_area, bits, sp.x - start_pt.x, sp.y - start_pt.y, virtual_screen()));
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let had = with(|s| s.drag.take().is_some());
            unsafe {
                let _ = ReleaseCapture();
            }
            if had {
                let (area, cb) = with(|s| (s.area, s.on_changed));
                if let Some(cb) = cb {
                    cb(area, true);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wparam.0 as u16;
            if vk == VK_RETURN.0 || vk == VK_ESCAPE.0 {
                end_edit();
                return LRESULT(0);
            }
            let step = if unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0 { 10 } else { 1 };
            let (dx, dy) = match vk {
                v if v == VK_LEFT.0 => (-step, 0),
                v if v == VK_RIGHT.0 => (step, 0),
                v if v == VK_UP.0 => (0, -step),
                v if v == VK_DOWN.0 => (0, step),
                _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            };
            let start = with(|s| s.area);
            set_area(drag_area(start, None, dx, dy, virtual_screen()));
            let (area, cb) = with(|s| (s.area, s.on_changed));
            if let Some(cb) = cb {
                cb(area, true);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let br = CreateSolidBrush(COLORREF(0x0020_2020));
                FillRect(dc, &ps.rcPaint, br);
                let _ = DeleteObject(HGDIOBJ(br.0));
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn ensure_classes() {
    if with(|s| std::mem::replace(&mut s.registered, true)) {
        return;
    }
    for (name, proc_) in [
        (FRAME_CLASS, frame_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT),
        (BUTTON_CLASS, button_proc),
        (EDIT_CLASS, edit_proc),
    ] {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc_),
            hInstance: hinstance(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
            lpszClassName: name,
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VS: RECT = RECT { left: -1920, top: 0, right: 1920, bottom: 1080 };

    #[test]
    fn mover_fica_dentro_da_area_de_trabalho() {
        let a = AreaRect { x: 100, y: 100, w: 400, h: 300 };
        assert_eq!(drag_area(a, None, 50, -20, VS), AreaRect { x: 150, y: 80, w: 400, h: 300 });
        // Arrastar pra fora encosta na borda sem mudar o tamanho.
        assert_eq!(drag_area(a, None, 5000, 5000, VS), AreaRect { x: 1520, y: 780, w: 400, h: 300 });
        assert_eq!(drag_area(a, None, -9000, 0, VS), AreaRect { x: -1920, y: 100, w: 400, h: 300 });
    }

    #[test]
    fn redimensionar_respeita_o_minimo_e_a_borda_oposta() {
        let a = AreaRect { x: 100, y: 100, w: 400, h: 300 };
        // Canto inferior direito +50/+40.
        assert_eq!(drag_area(a, Some(2 | 8), 50, 40, VS), AreaRect { x: 100, y: 100, w: 450, h: 340 });
        // Borda esquerda passando da direita: para no mínimo, direita fixa.
        assert_eq!(drag_area(a, Some(1), 1000, 0, VS), AreaRect { x: 500 - MIN_EDIT, y: 100, w: MIN_EDIT, h: 300 });
        // Topo além da tela: preso no topo.
        assert_eq!(drag_area(a, Some(4), 0, -500, VS), AreaRect { x: 100, y: 0, w: 400, h: 400 });
    }
}

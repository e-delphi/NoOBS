//! Ícones de "gravando" gerados em runtime (porte de `OBSRecordIcon.pas`):
//! sem um segundo `.ico` no projeto. Desenha numa DIB 32-bit com alfa
//! pré-multiplicado e cria o `HICON` por `CreateIconIndirect`.
//!
//! * [`create_recording_overlay_icon`] — ícone do app + bolinha vermelha no
//!   canto inferior direito (bandeja, via `Shell_NotifyIcon`);
//! * [`create_recording_dot_icon`] — SÓ a bolinha, centrada, para o
//!   `ITaskbarList3::SetOverlayIcon` (o Windows já a põe no canto do botão).
//!
//! Quem recebe o `HICON` chama `DestroyIcon` (ou o mantém vivo até o fim do
//! processo, como fazem a bandeja e a taskbar).

use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DrawIconEx, DI_NORMAL, HICON, ICONINFO};

/// `#DC2626` — o mesmo `--danger` da UI.
const DOT: (u8, u8, u8) = (220, 38, 38);

/// DIB 32-bit TOP-DOWN (linha 0 = topo) zerada (alfa 0 em tudo).
struct Dib {
    bmp: HBITMAP,
    bits: *mut u8,
    size: usize,
}

impl Dib {
    fn new(size: i32) -> Option<Self> {
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bmp = unsafe { CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
        if bits.is_null() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
            }
            return None;
        }
        // CreateDIBSection devolve memória zerada.
        Some(Self { bmp, bits: bits as *mut u8, size: size as usize })
    }

    fn pixels(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, self.size * self.size * 4) }
    }

    /// Pinta um disco anti-aliased (borda de 1 px com fade linear) em alfa
    /// pré-multiplicado. Pixels fora do círculo ficam como estão.
    fn paint_disc(&mut self, cx: f64, cy: f64, radius: f64, x0: usize, y0: usize) {
        let n = self.size;
        let px = self.pixels();
        for iy in y0..n {
            for ix in x0..n {
                let (dx, dy) = (ix as f64 + 0.5 - cx, iy as f64 + 0.5 - cy);
                let fade = radius - (dx * dx + dy * dy).sqrt();
                if fade <= 0.0 {
                    continue;
                }
                let a = (fade.min(1.0) * 255.0).round() as u32;
                let o = (iy * n + ix) * 4;
                px[o] = (DOT.2 as u32 * a / 255) as u8;
                px[o + 1] = (DOT.1 as u32 * a / 255) as u8;
                px[o + 2] = (DOT.0 as u32 * a / 255) as u8;
                px[o + 3] = a as u8;
            }
        }
    }

    /// `CreateIconIndirect` copia os bitmaps: a DIB pode morrer depois.
    fn into_icon(self) -> Option<HICON> {
        unsafe {
            // Máscara monocromática vazia: o alfa da DIB decide o que aparece,
            // mas o campo tem que vir preenchido.
            let mask = CreateBitmap(self.size as i32, self.size as i32, 1, 1, None);
            let info = ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: self.bmp, ..Default::default() };
            let icon = CreateIconIndirect(&info).ok();
            let _ = DeleteObject(HGDIOBJ(mask.0));
            icon
        }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
        }
    }
}

/// Ícone base + bolinha vermelha (~45% do tamanho) no canto inferior direito.
pub fn create_recording_overlay_icon(base: HICON, size: i32) -> Option<HICON> {
    if base.0.is_null() || size < 16 {
        return None;
    }
    let mut dib = Dib::new(size)?;
    unsafe {
        // DrawIconEx com DI_NORMAL preserva o alfa do ícone.
        let dc = CreateCompatibleDC(None);
        let old = SelectObject(dc, HGDIOBJ(dib.bmp.0));
        let _ = DrawIconEx(dc, 0, 0, base, size, size, 0, None, DI_NORMAL);
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
    }
    let dot = (size * 9 / 20) as usize;
    let start = size as usize - dot;
    let r = dot as f64 / 2.0;
    dib.paint_disc(start as f64 + r, start as f64 + r, r, start, start);
    dib.into_icon()
}

/// Só a bolinha (vermelha sólida, sem borda, ~72% do canvas), CENTRADA: o
/// Windows já desenha o overlay no canto inferior direito do botão.
pub fn create_recording_dot_icon(size: i32) -> Option<HICON> {
    let size = size.max(8);
    let mut dib = Dib::new(size)?;
    let c = size as f64 / 2.0;
    dib.paint_disc(c, c, size as f64 * 0.72 / 2.0, 0, 0);
    dib.into_icon()
}

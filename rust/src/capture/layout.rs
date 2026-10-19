//! Contas de posicionamento, sem GPU: onde cada fonte entra no canvas.
//!
//! Área da tela: o canvas é fixado no tamanho da área ao começar; mover a área
//! muda só o RECORTE, e redimensionar encaixa o novo tamanho dentro do canvas
//! mantendo a proporção (tarja nas sobras) — o `OBS_BOUNDS_SCALE_INNER` que a
//! gravação de área do NoOBS já usa (pegadinha #66). Uma área que cruza
//! monitores vira um pedaço por monitor, cada um no lugar certo do canvas.

use super::gpu::Rect;

/// Encaixa `w`×`h` em `cw`×`ch` mantendo a proporção, centralizado.
pub fn fit_inside(w: u32, h: u32, cw: u32, ch: u32) -> Rect {
    if w == 0 || h == 0 {
        return Rect { x: 0.0, y: 0.0, w: cw as f32, h: ch as f32 };
    }
    let s = (cw as f32 / w as f32).min(ch as f32 / h as f32);
    let (dw, dh) = (w as f32 * s, h as f32 * s);
    Rect { x: (cw as f32 - dw) / 2.0, y: (ch as f32 - dh) / 2.0, w: dw, h: dh }
}

/// Retângulo em coordenadas da área de trabalho (pixels físicos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl DesktopRect {
    fn intersect(&self, o: &DesktopRect) -> Option<DesktopRect> {
        let (l, t) = (self.x.max(o.x), self.y.max(o.y));
        let (r, b) = ((self.x + self.w).min(o.x + o.w), (self.y + self.h).min(o.y + o.h));
        (r > l && b > t).then_some(DesktopRect { x: l, y: t, w: r - l, h: b - t })
    }
}

/// Recorte (pixels do monitor) e destino (pixels do canvas) de um monitor.
/// `None` = a área não passa por ele (a fonte não é desenhada).
pub type Placement = Option<(Rect, Rect)>;

/// Um `Placement` por monitor de `monitors`, pra área `area` num canvas `canvas`.
pub fn area_layout(area: DesktopRect, canvas: (u32, u32), monitors: &[DesktopRect]) -> Vec<Placement> {
    let fit = fit_inside(area.w.max(0) as u32, area.h.max(0) as u32, canvas.0, canvas.1);
    let s = if area.w > 0 { fit.w / area.w as f32 } else { 0.0 };
    monitors
        .iter()
        .map(|m| {
            let i = area.intersect(m)?;
            let crop = Rect { x: (i.x - m.x) as f32, y: (i.y - m.y) as f32, w: i.w as f32, h: i.h as f32 };
            let dest = Rect { x: fit.x + (i.x - area.x) as f32 * s, y: fit.y + (i.y - area.y) as f32 * s, w: i.w as f32 * s, h: i.h as f32 * s };
            Some((crop, dest))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    const MONS: [DesktopRect; 2] = [DesktopRect { x: 0, y: 0, w: 1920, h: 1080 }, DesktopRect { x: 1920, y: 0, w: 2560, h: 1440 }];

    #[test]
    fn area_num_monitor_so() {
        let l = area_layout(DesktopRect { x: 100, y: 100, w: 640, h: 360 }, (640, 360), &MONS);
        assert_eq!(l[0], Some((r(100.0, 100.0, 640.0, 360.0), r(0.0, 0.0, 640.0, 360.0))));
        assert_eq!(l[1], None);
    }

    #[test]
    fn area_cruzando_dois_monitores() {
        // 400 px no 1º monitor, 240 no 2º.
        let l = area_layout(DesktopRect { x: 1520, y: 50, w: 640, h: 360 }, (640, 360), &MONS);
        assert_eq!(l[0], Some((r(1520.0, 50.0, 400.0, 360.0), r(0.0, 0.0, 400.0, 360.0))));
        assert_eq!(l[1], Some((r(0.0, 50.0, 240.0, 360.0), r(400.0, 0.0, 240.0, 360.0))));
    }

    #[test]
    fn area_movida_so_muda_o_recorte() {
        let a = area_layout(DesktopRect { x: 100, y: 100, w: 640, h: 360 }, (640, 360), &MONS);
        let b = area_layout(DesktopRect { x: 300, y: 200, w: 640, h: 360 }, (640, 360), &MONS);
        assert_eq!(a[0].unwrap().1, b[0].unwrap().1, "destino igual");
        assert_eq!(b[0].unwrap().0, r(300.0, 200.0, 640.0, 360.0));
    }

    #[test]
    fn area_redimensionada_encaixa_com_tarja() {
        // Ficou com o dobro da largura: encaixa na largura, tarja em cima e embaixo.
        let l = area_layout(DesktopRect { x: 0, y: 0, w: 1280, h: 360 }, (640, 360), &MONS);
        let (_, d) = l[0].unwrap();
        assert_eq!(d, r(0.0, 90.0, 640.0, 180.0));
    }

    #[test]
    fn area_fora_da_tela_nao_desenha_nada() {
        let l = area_layout(DesktopRect { x: -5000, y: 0, w: 640, h: 360 }, (640, 360), &MONS);
        assert!(l.iter().all(|p| p.is_none()));
    }
}

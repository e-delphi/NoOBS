//! Tipos puros de monitor/áudio e cálculo do canvas (porte de `OBSScene.pas`).

use crate::config;

#[derive(Debug, Clone, Default)]
pub struct ObsMonitor {
    pub index: i32,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub position_x: i32,
    pub position_y: i32,
}

#[derive(Debug, Clone, Default)]
pub struct AudioDevice {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub origin_x: i32,
    pub origin_y: i32,
}

/// Bounding box dos monitores. Sem monitores devolve canvas zerado em vez de
/// falhar: o caso áudio-only (0 monitores) é tratado por quem chama
/// (pegadinha #8).
pub fn compute_canvas(monitors: &[ObsMonitor]) -> Canvas {
    let Some(first) = monitors.first() else {
        return Canvas::default();
    };
    let mut min_x = first.position_x;
    let mut min_y = first.position_y;
    let mut max_x = first.position_x + first.width;
    let mut max_y = first.position_y + first.height;
    for m in &monitors[1..] {
        min_x = min_x.min(m.position_x);
        min_y = min_y.min(m.position_y);
        max_x = max_x.max(m.position_x + m.width);
        max_y = max_y.max(m.position_y + m.height);
    }
    Canvas {
        width: max_x - min_x,
        height: max_y - min_y,
        origin_x: min_x,
        origin_y: min_y,
    }
}

/// Só entram os monitores marcados e não ocultos (pegadinha #11).
/// `auto_profile`: gravação disparada pelo watcher de mic (perfil auto).
pub fn filter_enabled_monitors(mons: &[ObsMonitor], auto_profile: bool) -> Vec<ObsMonitor> {
    mons.iter()
        .filter(|m| {
            let id = m.index.to_string();
            if auto_profile {
                config::get_source_active_for_auto("monitors", &id, true)
            } else {
                config::get_source_active("monitors", &id, true)
            }
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: i32, h: i32) -> ObsMonitor {
        ObsMonitor { position_x: x, position_y: y, width: w, height: h, ..Default::default() }
    }

    #[test]
    fn canvas_vazio() {
        assert_eq!(compute_canvas(&[]), Canvas::default());
    }

    #[test]
    fn canvas_dois_monitores_com_origem_negativa() {
        let c = compute_canvas(&[mon(-1920, 0, 1920, 1080), mon(0, 0, 2560, 1440)]);
        assert_eq!(c, Canvas { width: 4480, height: 1440, origin_x: -1920, origin_y: 0 });
    }
}

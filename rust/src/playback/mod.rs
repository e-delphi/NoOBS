//! Player nativo (docs/ui-iron.md, fase 3): o que o `<video>` do WebView2
//! fazia, sem o Chromium.
//!
//! Duas threads leem o MESMO arquivo, cada uma com o próprio demuxer:
//! * vídeo (`video.rs`): decodifica na placa (D3D11VA, o mesmo caminho da
//!   exportação — pegadinha #51l), converte NA PLACA pro tamanho da tela
//!   (`gpu.rs`) e entrega o quadro no instante do relógio;
//! * áudio (`audio.rs`): decodifica TODAS as faixas, mistura com o volume de
//!   cada uma, mantém o tom em outras velocidades (`TimeStretch`) e toca no
//!   WASAPI. O áudio é o relógio.
//!
//! Zoom, deslocamento e a vista de regiões não são transformação da imagem
//! pronta: viram o recorte da ORIGEM que é convertido (`layout`), então
//! aproximar continua nítido.

mod audio;
mod gpu;
mod video;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;


/// Acima disto não há som (o Chromium também silencia acima de 4×) e o relógio
/// passa a ser o de parede.
pub const AUDIO_MAX_RATE: f64 = 4.0;
/// A partir disto a imagem anda de keyframe em keyframe (pegadinha #61): nenhum
/// decoder entrega 8× uma gravação 4K de 60 fps.
pub const TRICK_MIN_RATE: f64 = 8.0;

// ---------------------------------------------------------------------------
// Geometria: o que da origem vai pra onde na tela
// ---------------------------------------------------------------------------

/// Uma região da gravação (monitor ou webcam), em pixels da origem.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub webcam: bool,
}

/// O palco do player e o que o usuário pediu pra ver nele. Pixels FÍSICOS.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewSpec {
    pub stage_w: i32,
    pub stage_h: i32,
    /// `transform: translate(pan) scale(zoom)` da versão web, com origem no
    /// canto do palco.
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    /// Vazio = tela cheia; 1 = recorte; 2+ = lado a lado (ordenadas por x).
    pub regions: Vec<Region>,
}

impl Default for ViewSpec {
    fn default() -> Self {
        Self { stage_w: 1280, stage_h: 720, zoom: 1.0, pan_x: 0.0, pan_y: 0.0, regions: Vec::new() }
    }
}

/// Um retângulo da origem (`s*`) desenhado num retângulo da saída (`d*`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Piece {
    pub sx: i32,
    pub sy: i32,
    pub sw: i32,
    pub sh: i32,
    pub dx: i32,
    pub dy: i32,
    pub dw: i32,
    pub dh: i32,
}

/// A imagem de saída: onde ela fica no palco e os pedaços dentro dela.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub pieces: Vec<Piece>,
    /// Zoom e deslocamento da vista que gerou o quadro: a tela desloca o
    /// quadro mostrado até chegar o da vista nova (arrasto sem atraso).
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

/// Composição das regiões (`applyRegionView` da web): lado a lado pela
/// ordem de x; altura = a maior; monitor centralizado na vertical, webcam
/// no topo. Devolve (tamanho, [(origem, destino na composição)]).
fn compose(src_w: i32, src_h: i32, regions: &[Region]) -> ((f64, f64), Vec<([f64; 4], [f64; 4])>) {
    let valid: Vec<Region> = regions
        .iter()
        .filter_map(|r| {
            let x = r.x.clamp(0, src_w);
            let y = r.y.clamp(0, src_h);
            let w = (r.x + r.w).clamp(0, src_w) - x;
            let h = (r.y + r.h).clamp(0, src_h) - y;
            (w > 0 && h > 0).then_some(Region { x, y, w, h, webcam: r.webcam })
        })
        .collect();
    if valid.is_empty() {
        let full = [0.0, 0.0, src_w as f64, src_h as f64];
        return ((src_w as f64, src_h as f64), vec![(full, full)]);
    }
    let mut ordered = valid;
    ordered.sort_by_key(|r| r.x);
    let max_h = ordered.iter().map(|r| r.h).max().unwrap_or(0) as f64;
    let mut out = Vec::new();
    let mut dx = 0.0;
    for r in &ordered {
        let dy = if r.webcam { 0.0 } else { ((max_h - r.h as f64) / 2.0).round() };
        out.push(([r.x as f64, r.y as f64, r.w as f64, r.h as f64], [dx, dy, r.w as f64, r.h as f64]));
        dx += r.w as f64;
    }
    ((dx, max_h), out)
}

/// Encaixa (`object-fit: contain`), aplica zoom/deslocamento e recorta no
/// palco. `None` = nada visível.
pub fn layout(src_w: i32, src_h: i32, v: &ViewSpec) -> Option<Layout> {
    if src_w <= 0 || src_h <= 0 || v.stage_w <= 0 || v.stage_h <= 0 {
        return None;
    }
    let ((cw, ch), comp) = compose(src_w, src_h, &v.regions);
    let (sw, sh) = (v.stage_w as f64, v.stage_h as f64);
    let fit = (sw / cw).min(sh / ch);
    let (ox, oy) = ((sw - cw * fit) / 2.0, (sh - ch * fit) / 2.0);
    let z = v.zoom.max(1.0);
    let k = fit * z;
    // Pedaços na tela, já recortados no palco.
    let mut screen: Vec<([f64; 4], [f64; 4])> = Vec::new();
    for (src, dst) in comp {
        let x0 = (ox + dst[0] * fit) * z + v.pan_x;
        let y0 = (oy + dst[1] * fit) * z + v.pan_y;
        let (x1, y1) = (x0 + dst[2] * k, y0 + dst[3] * k);
        let (cx0, cy0, cx1, cy1) = (x0.max(0.0), y0.max(0.0), x1.min(sw), y1.min(sh));
        if cx1 - cx0 < 1.0 || cy1 - cy0 < 1.0 {
            continue;
        }
        // De volta pra origem pela mesma escala.
        let s = [src[0] + (cx0 - x0) / k, src[1] + (cy0 - y0) / k, (cx1 - cx0) / k, (cy1 - cy0) / k];
        screen.push((s, [cx0, cy0, cx1, cy1]));
    }
    if screen.is_empty() {
        return None;
    }
    let bx0 = screen.iter().map(|(_, d)| d[0]).fold(f64::MAX, f64::min).floor();
    let by0 = screen.iter().map(|(_, d)| d[1]).fold(f64::MAX, f64::min).floor();
    let bx1 = screen.iter().map(|(_, d)| d[2]).fold(f64::MIN, f64::max).ceil();
    let by1 = screen.iter().map(|(_, d)| d[3]).fold(f64::MIN, f64::max).ceil();
    let pieces = screen
        .iter()
        .filter_map(|(s, d)| {
            let sx = s[0].round().clamp(0.0, src_w as f64 - 1.0) as i32;
            let sy = s[1].round().clamp(0.0, src_h as f64 - 1.0) as i32;
            let sw_ = (s[2].round() as i32).clamp(1, src_w - sx);
            let sh_ = (s[3].round() as i32).clamp(1, src_h - sy);
            let dx = (d[0] - bx0).round() as i32;
            let dy = (d[1] - by0).round() as i32;
            let dw = ((d[2] - bx0).round() as i32 - dx).max(1);
            let dh = ((d[3] - by0).round() as i32 - dy).max(1);
            Some(Piece { sx, sy, sw: sw_, sh: sh_, dx, dy, dw, dh })
        })
        .collect();
    Some(Layout { x: bx0 as i32, y: by0 as i32, w: (bx1 - bx0) as i32, h: (by1 - by0) as i32, pieces, zoom: z, pan_x: v.pan_x, pan_y: v.pan_y })
}

// ---------------------------------------------------------------------------
// Relógio
// ---------------------------------------------------------------------------

struct ClockState {
    playing: bool,
    rate: f64,
    /// Relógio de parede: instante da mídia em `base_at`.
    base_media: f64,
    base_at: Instant,
    /// O áudio manda quando toca: instante da mídia que o dispositivo acabou de
    /// consumir, e quando.
    audio: Option<(f64, Instant)>,
    use_audio: bool,
}

/// Onde a reprodução "está". Com áudio, é o que o dispositivo de som já
/// tocou; sem ele (sem faixa, ou acima de 4×), o de parede.
pub struct Clock(Mutex<ClockState>);

impl Clock {
    fn new() -> Self {
        Self(Mutex::new(ClockState { playing: false, rate: 1.0, base_media: 0.0, base_at: Instant::now(), audio: None, use_audio: false }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ClockState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn now(&self) -> f64 {
        let s = self.lock();
        if !s.playing {
            return s.base_media;
        }
        if s.use_audio {
            if let Some((t, at)) = s.audio {
                // Entre duas leituras do dispositivo, anda pela parede (no máximo
                // 200 ms: se o áudio parou, o relógio para também).
                return t + at.elapsed().as_secs_f64().min(0.2) * s.rate;
            }
        }
        s.base_media + s.base_at.elapsed().as_secs_f64() * s.rate
    }

    pub fn playing(&self) -> bool {
        self.lock().playing
    }

    fn rebase(&self, media: f64) {
        let mut s = self.lock();
        s.base_media = media;
        s.base_at = Instant::now();
        s.audio = None;
    }

    fn set_playing(&self, on: bool) {
        let t = self.now();
        let mut s = self.lock();
        s.playing = on;
        s.base_media = t;
        s.base_at = Instant::now();
        s.audio = None;
    }

    fn set_rate(&self, rate: f64, has_audio: bool) {
        let t = self.now();
        let mut s = self.lock();
        s.rate = rate;
        s.base_media = t;
        s.base_at = Instant::now();
        s.audio = None;
        s.use_audio = has_audio && rate <= AUDIO_MAX_RATE;
    }

    fn set_use_audio(&self, on: bool) {
        let mut s = self.lock();
        s.use_audio = on && s.rate <= AUDIO_MAX_RATE;
    }

    /// O áudio informa o instante que o dispositivo consumiu.
    fn audio_at(&self, media: f64) {
        let mut s = self.lock();
        if s.playing && s.use_audio {
            s.audio = Some((media, Instant::now()));
            // A parede acompanha, pra a troca de relógio não dar salto.
            s.base_media = media;
            s.base_at = Instant::now();
        }
    }
}

// ---------------------------------------------------------------------------
// Saída e comandos
// ---------------------------------------------------------------------------

/// O que o motor sabe do arquivo (o resto vem do Probe, no painel de informações).
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub duration: f64,
}

/// Um quadro RGBA pronto pra tela (4 bytes por pixel, linha a linha).
pub struct RgbaBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![0; width as usize * height as usize * 4] }
    }

    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        &mut self.pixels
    }
}

pub enum Out {
    Info(Info),
    /// Quadro pronto, já no tamanho e na posição do palco (pixels físicos).
    /// `view` = (zoom, pan_x, pan_y) da vista que gerou o quadro (px físicos).
    Frame { buf: RgbaBuffer, x: i32, y: i32, view: (f64, f64, f64) },
    /// Chegou ao fim (a reprodução parou sozinha).
    Ended,
    Error(String),
}

/// Lido pela interface a cada quadro de tela, sem mensagem.
pub struct Shared {
    pub clock: Clock,
    /// Instante do último quadro mostrado (bits de f64).
    pos: AtomicU64,
    /// Quadro entregue à interface e ainda não aplicado: o próximo é
    /// descartado em vez de empilhar no laço de eventos.
    pub ui_pending: AtomicBool,
    duration: AtomicU64,
}

impl Shared {
    pub fn position(&self) -> f64 {
        f64::from_bits(self.pos.load(Ordering::Relaxed))
    }
    fn set_position(&self, t: f64) {
        self.pos.store(t.to_bits(), Ordering::Relaxed);
    }
    pub fn duration(&self) -> f64 {
        f64::from_bits(self.duration.load(Ordering::Relaxed))
    }
}

enum VCmd {
    Play,
    Pause,
    Seek(f64),
    /// Um quadro pra frente (+1) ou pra trás (-1), pausado.
    Step(i32),
    Rate(f64),
    View(ViewSpec),
}

enum ACmd {
    Play,
    Pause,
    Seek(f64),
    Rate(f64),
    /// Volume geral (0..2) e de cada faixa (0..2).
    Volumes(f32, Vec<f32>),
}

/// Um vídeo aberto. Soltar fecha (as threads saem quando os canais fecham).
pub struct Playback {
    vtx: Sender<VCmd>,
    atx: Sender<ACmd>,
    pub shared: Arc<Shared>,
    has_audio: Arc<AtomicBool>,
    rate: f64,
}

impl Playback {
    /// Abre e começa a tocar. `out` é chamado de threads do motor.
    pub fn open(path: &str, view: ViewSpec, out: Arc<dyn Fn(Out) + Send + Sync>) -> Self {
        Self::open_with(path, view, out, true)
    }

    /// Abre parado no começo (a prévia da exportação só toca a pedido).
    pub fn open_paused(path: &str, view: ViewSpec, out: Arc<dyn Fn(Out) + Send + Sync>) -> Self {
        Self::open_with(path, view, out, false)
    }

    fn open_with(path: &str, view: ViewSpec, out: Arc<dyn Fn(Out) + Send + Sync>, autoplay: bool) -> Self {
        let shared = Arc::new(Shared {
            clock: Clock::new(),
            pos: AtomicU64::new(0f64.to_bits()),
            ui_pending: AtomicBool::new(false),
            duration: AtomicU64::new(0f64.to_bits()),
        });
        let has_audio = Arc::new(AtomicBool::new(false));
        let (vtx, vrx) = channel();
        let (atx, arx) = channel();
        {
            let (path, shared, out) = (path.to_owned(), shared.clone(), out.clone());
            std::thread::Builder::new()
                .name("player-video".into())
                .spawn(move || video::run(&path, view, vrx, shared, out))
                .ok();
        }
        {
            let (path, shared, has_audio) = (path.to_owned(), shared.clone(), has_audio.clone());
            std::thread::Builder::new()
                .name("player-audio".into())
                .spawn(move || audio::run(&path, arx, shared, has_audio))
                .ok();
        }
        let p = Self { vtx, atx, shared, has_audio, rate: 1.0 };
        if autoplay {
            p.play();
        } else {
            p.pause();
        }
        p
    }

    pub fn play(&self) {
        // Do fim, tocar recomeça do zero (o `play()` do HTML).
        if self.shared.duration() > 0.0 && self.shared.position() >= self.shared.duration() - 0.05 {
            self.seek(0.0);
        }
        self.shared.clock.set_playing(true);
        let _ = self.atx.send(ACmd::Play);
        let _ = self.vtx.send(VCmd::Play);
    }

    pub fn pause(&self) {
        self.shared.clock.set_playing(false);
        let _ = self.atx.send(ACmd::Pause);
        let _ = self.vtx.send(VCmd::Pause);
    }

    pub fn playing(&self) -> bool {
        self.shared.clock.playing()
    }

    pub fn seek(&self, t: f64) {
        let d = self.shared.duration();
        let t = if d > 0.0 { t.clamp(0.0, d) } else { t.max(0.0) };
        self.shared.clock.rebase(t);
        self.shared.set_position(t);
        let _ = self.atx.send(ACmd::Seek(t));
        let _ = self.vtx.send(VCmd::Seek(t));
    }

    pub fn step(&self, dir: i32) {
        let _ = self.vtx.send(VCmd::Step(dir));
    }

    pub fn set_rate(&mut self, rate: f64) {
        if rate <= 0.0 {
            return;
        }
        let was_audio = self.rate <= AUDIO_MAX_RATE;
        self.rate = rate;
        let has = self.has_audio.load(Ordering::Relaxed);
        self.shared.clock.set_rate(rate, has);
        let _ = self.vtx.send(VCmd::Rate(rate));
        let _ = self.atx.send(ACmd::Rate(rate));
        // Voltando a uma velocidade com som: o áudio retoma do ponto atual.
        if has && !was_audio && rate <= AUDIO_MAX_RATE {
            let _ = self.atx.send(ACmd::Seek(self.shared.clock.now()));
        }
    }

    pub fn set_volumes(&self, master: f32, tracks: Vec<f32>) {
        let _ = self.atx.send(ACmd::Volumes(master, tracks));
    }

    pub fn set_view(&self, v: ViewSpec) {
        let _ = self.vtx.send(VCmd::View(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(w: i32, h: i32) -> ViewSpec {
        ViewSpec { stage_w: w, stage_h: h, ..Default::default() }
    }

    #[test]
    fn encaixa_com_tarja() {
        // 3840x2160 num palco 1000x1000: 1000x562, centralizado na vertical.
        let l = layout(3840, 2160, &spec(1000, 1000)).unwrap();
        assert_eq!((l.x, l.w), (0, 1000));
        // 218,75..781,25 arredondado pra fora: a imagem cobre o pixel parcial.
        assert_eq!(l.y, 218);
        assert_eq!(l.h, 564);
        assert_eq!(l.pieces.len(), 1);
        assert_eq!((l.pieces[0].sw, l.pieces[0].sh), (3840, 2160));
    }

    #[test]
    fn zoom_recorta_a_origem() {
        // 2x no canto: só o quarto de cima à esquerda da origem, ocupando o palco.
        let v = ViewSpec { zoom: 2.0, ..spec(1920, 1080) };
        let l = layout(3840, 2160, &v).unwrap();
        assert_eq!((l.x, l.y, l.w, l.h), (0, 0, 1920, 1080));
        let p = l.pieces[0];
        assert_eq!((p.sx, p.sy, p.sw, p.sh), (0, 0, 1920, 1080));
    }

    #[test]
    fn regioes_lado_a_lado() {
        // Dois monitores 1920x1080 e um de 1280x720 entre eles; escolhidos o
        // primeiro e o menor: 3200x1080, o menor centralizado na vertical.
        let regs = vec![
            Region { x: 0, y: 0, w: 1920, h: 1080, webcam: false },
            Region { x: 1920, y: 0, w: 1280, h: 720, webcam: false },
        ];
        let v = ViewSpec { regions: regs, ..spec(3200, 1080) };
        let l = layout(5120, 1080, &v).unwrap();
        assert_eq!(l.pieces.len(), 2);
        assert_eq!((l.pieces[1].dx, l.pieces[1].dy, l.pieces[1].dh), (1920, 180, 720));
    }

    #[test]
    fn fora_do_palco_some() {
        let v = ViewSpec { zoom: 2.0, pan_x: -5000.0, ..spec(1920, 1080) };
        assert!(layout(3840, 2160, &v).is_none());
    }
}

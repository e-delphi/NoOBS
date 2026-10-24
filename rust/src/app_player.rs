//! Presenter do player (`player.js` da versão web): abre o motor
//! (`playback`), põe os quadros na tela, mantém barra/tempo/relógio, e cuida
//! de zoom, volume, velocidade, teclado e do esconder dos controles.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, SharedString, VecModel};

use crate::app_view::{AppWindow, InfoRow, InfoSection, Pl, TrackRow, TurnRow, ViewOpt};
use crate::playback::{Out, Playback, Region, ViewSpec, TRICK_MIN_RATE};
use crate::view::{Transcript, VideoInfo};
use crate::{bridge, config, lang, log, mainthread};

/// Os controles somem depois disto sem mexer o mouse, tocando.
const IDLE_MS: u64 = 2000;
/// Em tela cheia, com o mouse parado perto da borda (`EDGE_MARGIN_PX`).
const IDLE_EDGE_MS: u64 = 1000;
const EDGE_MARGIN: f32 = 40.0;
/// Zoom máximo da roda (`onWheel`).
const ZOOM_MAX: f64 = 8.0;
/// Largura de cada barra da forma de onda (barra + vão), px físicos.
const WAVE_PX_PER_BAR: usize = 2;
const WAVE_BUCKETS: i64 = 2000;

#[derive(Default)]
struct State {
    id: String,
    pb: Option<Playback>,
    /// Palco em px lógicos.
    stage: (f32, f32),
    zoom: f64,
    pan: (f64, f64),
    pan_start: (f64, f64),
    master: i32,
    muted: bool,
    tracks: Vec<f32>,
    speed: f64,
    duration: f64,
    start_clock: i32,
    seeking: bool,
    last_activity: Option<Instant>,
    near_edge: bool,
    peaks: Option<(String, Vec<f32>)>,
    wave_width: u32,
    timer: Option<slint::Timer>,
    // ---- painel de informações ----
    info: Option<VideoInfo>,
    info_requested: bool,
    /// Regiões escolhidas na "Visualização" (vazio = tela cheia).
    regions: BTreeSet<usize>,
    /// Volume de cada faixa, 0..200 (%); a 1ª (mistura) começa em 100.
    track_vols: Vec<i32>,
    /// Volume antes de mutar a faixa (o desmutar volta pra ele).
    pre_mute: Vec<i32>,
    // ---- transcrição ----
    transcript: Option<Transcript>,
    tr_requested: bool,
    tr_active: i32,
    tr_pinned: i32,
    tr_query: String,
    editing_turn: i32,
    captions_on: bool,
    /// Turno → blocos de até duas linhas (texto, início, fim).
    cap_cache: Option<(i32, f32, Vec<(String, f64, f64)>)>,
    /// Avisar "sem transcrição" quando ela chegar (a legenda foi ligada antes).
    cap_notify: bool,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State {
        zoom: 1.0,
        master: 100,
        speed: 1.0,
        tr_active: -1,
        tr_pinned: -1,
        editing_turn: -1,
        ..Default::default()
    });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn window() -> Option<AppWindow> {
    crate::app_view::window()
}

fn scale() -> f32 {
    window().map_or(1.0, |w| w.window().scale_factor())
}

/// `fmtTime`: `mm:ss` ou `h:mm:ss`.
fn fmt_time(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    let (h, m, ss) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{ss:02}")
    } else {
        format!("{m:02}:{ss:02}")
    }
}

/// `fmtClock`: hora do relógio, `HH:MM:SS`, volta à meia-noite.
fn fmt_clock(sec: i64) -> String {
    let s = sec.rem_euclid(86400);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Rótulo da velocidade: `1×`, `0,5×`, `1,5×` (vírgula, como a web).
fn speed_label(r: f64) -> String {
    if (r - 1.0).abs() < 1e-9 {
        return "1×".into();
    }
    let s = format!("{r}");
    format!("{}×", s.replace('.', ","))
}

fn view_spec(st: &State) -> ViewSpec {
    let k = scale() as f64;
    ViewSpec {
        stage_w: (st.stage.0 as f64 * k).round() as i32,
        stage_h: (st.stage.1 as f64 * k).round() as i32,
        zoom: st.zoom,
        pan_x: st.pan.0 * k,
        pan_y: st.pan.1 * k,
        regions: selected_regions(st),
    }
}

/// As regiões escolhidas, em pixels do VÍDEO (o layout é do canvas da
/// gravação; um arquivo exportado menor tem o vídeo reduzido).
fn selected_regions(st: &State) -> Vec<Region> {
    let Some(info) = &st.info else { return Vec::new() };
    let Some(lay) = &info.layout else { return Vec::new() };
    let (vw, vh) = info
        .streams
        .iter()
        .find(|s| s.kind == "video")
        .map(|s| (s.width.unwrap_or(0), s.height.unwrap_or(0)))
        .unwrap_or((0, 0));
    let sx = if lay.canvas_w > 0 && vw > 0 { vw as f64 / lay.canvas_w as f64 } else { 1.0 };
    let sy = if lay.canvas_h > 0 && vh > 0 { vh as f64 / lay.canvas_h as f64 } else { 1.0 };
    st.regions
        .iter()
        .filter_map(|&i| lay.regions.get(i))
        .map(|r| Region {
            x: (r.x as f64 * sx).round() as i32,
            y: (r.y as f64 * sy).round() as i32,
            w: (r.w as f64 * sx).round() as i32,
            h: (r.h as f64 * sy).round() as i32,
            webcam: r.kind == "webcam",
        })
        .collect()
}

fn push_view() {
    let v = with(|st| view_spec(st));
    with(|st| {
        if let Some(pb) = &st.pb {
            pb.set_view(v);
        }
    });
}

fn push_volumes() {
    with(|st| {
        let master = if st.muted { 0.0 } else { st.master as f32 / 100.0 };
        let tracks: Vec<f32> = if st.track_vols.is_empty() { st.tracks.clone() } else { st.track_vols.iter().map(|v| *v as f32 / 100.0).collect() };
        if let Some(pb) = &st.pb {
            pb.set_volumes(master, tracks);
        }
    });
    if let Some(w) = window() {
        let g = w.global::<Pl>();
        let (m, muted) = with(|st| (st.master, st.muted));
        g.set_volume(m);
        g.set_muted(muted);
    }
}

// ---------------------------------------------------------------------------
// Abrir / fechar
// ---------------------------------------------------------------------------

/// Abre a gravação no player (clique no card).
pub fn open(id: &str) {
    let Some(w) = window() else { return };
    if !bridge::check_recording_for_player(id) {
        return;
    }
    close_engine();
    let title = std::path::Path::new(id).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let start_clock = bridge::recording_start_clock_sec(id);
    let g = w.global::<Pl>();
    g.set_title(title.into());
    g.set_loading(true);
    g.set_loading_text(lang::t("player.loading").into());
    g.set_frame(slint::Image::default());
    g.set_progress(0.0);
    g.set_time_text("00:00 / 00:00".into());
    g.set_clock_text(SharedString::new());
    g.set_has_waveform(false);
    g.set_idle(false);
    g.set_zoom(1.0);
    g.set_open(true);
    // Sobre o player só valem as dicas dele (as da biblioteca ficam embaixo).
    crate::app_main::set_hint_layer(&w, 2);
    bridge::set_player_open(true);

    let view = with(|st| {
        st.id = id.to_owned();
        st.zoom = 1.0;
        st.pan = (0.0, 0.0);
        // Volume geral volta a 100 % em toda abertura (pedido do usuário na web).
        st.master = 100;
        st.muted = false;
        st.tracks = vec![1.0];
        st.speed = 1.0;
        st.duration = 0.0;
        st.start_clock = start_clock;
        st.seeking = false;
        st.last_activity = Some(Instant::now());
        // Tudo do vídeo anterior sai (a seleção de região, a transcrição — a
        // pegadinha #37 da web).
        st.info = None;
        st.info_requested = false;
        st.regions.clear();
        st.track_vols.clear();
        st.pre_mute.clear();
        st.transcript = None;
        st.tr_requested = false;
        st.tr_active = -1;
        st.tr_pinned = -1;
        st.editing_turn = -1;
        st.cap_cache = None;
        st.captions_on = config::get_config_bool("playerCaptions", false);
        view_spec(st)
    });
    g.set_info_sections(ModelRc::default());
    g.set_view_opts(ModelRc::default());
    g.set_tracks(ModelRc::default());
    g.set_turns(ModelRc::default());
    g.set_caption(SharedString::new());
    g.set_captions_on(with(|st| st.captions_on));
    g.set_editing_turn(-1);
    if with(|st| st.captions_on) {
        request_transcript();
    }
    g.set_speed(1.0);
    g.set_speed_label(speed_label(1.0).into());
    g.set_trick(false);

    let id_out = id.to_owned();
    let out: Arc<dyn Fn(Out) + Send + Sync> = Arc::new(move |o| {
        let id = id_out.clone();
        mainthread::queue(move || on_out(&id, o));
    });
    let pb = Playback::open(id, view, out);
    with(|st| st.pb = Some(pb));
    push_volumes();
    g.set_playing(true);

    // Barra, tempo e relógio a ~30 Hz (o `requestAnimationFrame` da web).
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), tick);
    with(|st| st.timer = Some(timer));

    // Forma de onda: pedida um instante depois (o vídeo tem prioridade).
    let wid = id.to_owned();
    slint::Timer::single_shot(Duration::from_millis(100), move || {
        if with(|st| st.id == wid) {
            let cached = with(|st| st.peaks.as_ref().is_some_and(|(i, _)| i == &wid));
            if cached {
                render_waveform();
            } else {
                bridge::handle_request_waveform(&wid, WAVE_BUCKETS, false);
            }
        }
    });
    log!("Player: abrindo {id}");
}

fn close_engine() {
    let pb = with(|st| {
        st.timer = None;
        st.pb.take()
    });
    // Soltar o handle fecha os canais; as threads saem sozinhas.
    drop(pb);
}

pub fn close() {
    let Some(w) = window() else { return };
    close_engine();
    let g = w.global::<Pl>();
    if g.get_fullscreen() {
        toggle_fullscreen();
    }
    g.set_open(false);
    g.set_frame(slint::Image::default());
    g.set_loading(false);
    g.set_info_open(false);
    g.set_tr_open(false);
    g.set_tr_search(SharedString::new());
    with(|st| st.tr_query.clear());
    with(|st| st.id.clear());
    // Aberto por cima da exportação (prévia gerada): as dicas voltam pra ela.
    crate::app_main::set_hint_layer(&w, if crate::app_export::is_open() { crate::app_export::HINT_LAYER } else { 0 });
    bridge::set_player_open(false);
}

pub fn is_open() -> bool {
    with(|st| !st.id.is_empty())
}

// ---------------------------------------------------------------------------
// Do motor
// ---------------------------------------------------------------------------

fn on_out(id: &str, o: Out) {
    if with(|st| st.id != id) {
        return;
    }
    let Some(w) = window() else { return };
    let g = w.global::<Pl>();
    match o {
        Out::Info(i) => {
            with(|st| st.duration = i.duration);
            g.set_loading(false);
        }
        Out::Frame { buf, x, y } => {
            let k = scale();
            let (bw, bh) = (buf.width(), buf.height());
            g.set_frame(slint::Image::from_rgba8(buf));
            g.set_frame_x(x as f32 / k);
            g.set_frame_y(y as f32 / k);
            g.set_frame_w(bw as f32 / k);
            g.set_frame_h(bh as f32 / k);
            g.set_loading(false);
            with(|st| {
                if let Some(pb) = &st.pb {
                    pb.shared.ui_pending.store(false, std::sync::atomic::Ordering::Release);
                }
            });
        }
        Out::Ended => {
            with(|st| {
                if let Some(pb) = &st.pb {
                    pb.pause();
                }
            });
            set_playing_ui(false);
        }
        Out::Error(e) => {
            g.set_loading(true);
            g.set_loading_text(lang::t("error.loadVideo").into());
            log!("Player: erro {e}");
        }
    }
}

fn set_playing_ui(playing: bool) {
    let Some(w) = window() else { return };
    let g = w.global::<Pl>();
    let was = g.get_playing();
    g.set_playing(playing);
    if was && !playing {
        // Pulso do indicador central na pausa.
        g.set_pause_flash(g.get_pause_flash() + 1);
    }
    with(|st| st.last_activity = Some(Instant::now()));
    g.set_idle(false);
    sync_trick();
}

fn tick() {
    let Some(w) = window() else { return };
    let g = w.global::<Pl>();
    let (pos, dur, clock, seeking) = with(|st| {
        let pos = st.pb.as_ref().map_or(0.0, |p| p.shared.position());
        let dur = st.pb.as_ref().map_or(st.duration, |p| p.shared.duration().max(st.duration));
        (pos, dur, st.start_clock, st.seeking)
    });
    if !seeking {
        g.set_progress(if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) as f32 } else { 0.0 });
    }
    g.set_time_text(format!("{} / {}", fmt_time(pos), fmt_time(dur)).into());
    g.set_clock_text(if clock >= 0 { fmt_clock(clock as i64 + pos.floor() as i64).into() } else { SharedString::new() });
    // Esconde os controles parado o mouse (só tocando).
    let idle_due = with(|st| {
        let ms = if g.get_fullscreen() && st.near_edge { IDLE_EDGE_MS } else { IDLE_MS };
        st.last_activity.is_some_and(|t| t.elapsed() >= Duration::from_millis(ms))
    });
    if g.get_playing() && idle_due && !g.get_idle() {
        g.set_idle(true);
    }
    sync_transcript_active(false);
    sync_caption();
    // A largura da barra mudou: a forma de onda é refeita no tamanho novo.
    let ww = wave_target_width(&w);
    if with(|st| st.wave_width != ww && st.peaks.is_some()) {
        render_waveform();
    }
}

// ---------------------------------------------------------------------------
// Forma de onda (`Waveform` do widgets.js), desenhada como imagem
// ---------------------------------------------------------------------------

/// Largura da barra de progresso em px físicos (o palco menos os controles).
fn wave_target_width(w: &AppWindow) -> u32 {
    let k = w.window().scale_factor();
    let stage_w = with(|st| st.stage.0);
    // Botões e textos à volta da barra (32+90+32+42+32 e os vãos de 12).
    let clock = if with(|st| st.start_clock >= 0) { 90.0 } else { 0.0 };
    ((stage_w - 32.0 - (32.0 + 12.0) - (90.0 + 12.0) - clock - (32.0 + 12.0) * 3.0 - 54.0 - 12.0).max(50.0) * k) as u32
}

pub fn on_waveform(id: &str, peaks: &[f32], hi: bool) {
    if hi {
        return; // a de alta resolução é da exportação (fase 5)
    }
    with(|st| st.peaks = Some((id.to_owned(), peaks.to_vec())));
    if with(|st| st.id == id) {
        render_waveform();
    }
}

fn render_waveform() {
    let Some(w) = window() else { return };
    let (id, peaks) = match with(|st| st.peaks.clone()) {
        Some(p) => p,
        None => return,
    };
    if with(|st| st.id != id) || peaks.is_empty() {
        return;
    }
    let k = w.window().scale_factor();
    let width = wave_target_width(&w).max(1);
    let height = (14.0 * k).round().max(1.0) as u32;
    // Reagrega pro número de barras que cabem (máximo por fatia).
    let bars = (width as usize / WAVE_PX_PER_BAR).max(1).min(peaks.len());
    let agg: Vec<f32> = (0..bars)
        .map(|i| {
            let a = i * peaks.len() / bars;
            let b = ((i + 1) * peaks.len() / bars).max(a + 1);
            peaks[a..b.min(peaks.len())].iter().cloned().fold(0.0, f32::max)
        })
        .collect();
    let maxp = agg.iter().cloned().fold(0.0001f32, f32::max);
    let dark = w.global::<crate::app_view::Theme>().get_dark();
    let (cr, cg, cb, ca) = if dark { (255u8, 255u8, 255u8, 191u8) } else { (0, 0, 0, 153) };
    let mut buf = SharedPixelBuffer::<slint::Rgba8Pixel>::new(width, height);
    let px = buf.make_mut_slice();
    let slot = width as f32 / bars as f32;
    for (i, p) in agg.iter().enumerate() {
        // Raiz quadrada: realça as variações (fala com compressão fica quase
        // plana no linear); mínimo de 4 % pra silêncio ter linha.
        let hpct = ((p / maxp).sqrt() * 100.0).max(4.0) / 100.0;
        let bh = ((height as f32 * hpct).round() as u32).clamp(1, height);
        let x0 = (i as f32 * slot) as u32;
        let x1 = ((i as f32 * slot + slot * 0.88).round() as u32).max(x0 + 1).min(width);
        for y in 0..bh {
            for x in x0..x1 {
                px[(y * width + x) as usize] = slint::Rgba8Pixel { r: cr, g: cg, b: cb, a: ca };
            }
        }
    }
    let g = w.global::<Pl>();
    g.set_waveform(slint::Image::from_rgba8(buf));
    g.set_has_waveform(true);
    with(|st| st.wave_width = width);
}

// ---------------------------------------------------------------------------
// Gestos
// ---------------------------------------------------------------------------

fn toggle_play() {
    let playing = with(|st| {
        let pb = st.pb.as_ref()?;
        if pb.playing() {
            pb.pause();
            Some(false)
        } else {
            pb.play();
            Some(true)
        }
    });
    if let Some(p) = playing {
        set_playing_ui(p);
    }
}

fn seek_frac(f: f32) {
    let Some(w) = window() else { return };
    w.global::<Pl>().set_progress(f);
    with(|st| {
        st.seeking = true;
        if let Some(pb) = &st.pb {
            let d = pb.shared.duration().max(st.duration);
            pb.seek(f as f64 * d);
        }
    });
}

fn seek_by(delta: f64) {
    with(|st| {
        if let Some(pb) = &st.pb {
            let d = pb.shared.duration().max(st.duration);
            let t = (pb.shared.position() + delta).clamp(0.0, d);
            pb.seek(t);
        }
    });
}

fn set_speed(r: f32) {
    let r = r as f64;
    with(|st| {
        st.speed = r;
        if let Some(pb) = st.pb.as_mut() {
            pb.set_rate(r);
        }
    });
    if let Some(w) = window() {
        let g = w.global::<Pl>();
        g.set_speed(r as f32);
        g.set_speed_label(speed_label(r).into());
    }
    sync_trick();
}

/// Andando por saltos: velocidade de salto, tocando e com keyframes.
fn sync_trick() {
    let Some(w) = window() else { return };
    let g = w.global::<Pl>();
    let on = with(|st| st.speed >= TRICK_MIN_RATE) && g.get_playing();
    g.set_trick(on);
}

fn set_volume(v: i32) {
    with(|st| {
        st.master = v.clamp(0, 200);
        st.muted = st.master == 0;
    });
    push_volumes();
}

fn toggle_mute() {
    with(|st| st.muted = !st.muted);
    push_volumes();
}

pub fn toggle_fullscreen() {
    crate::ui::toggle_fullscreen();
    if let Some(w) = window() {
        let g = w.global::<Pl>();
        g.set_fullscreen(!g.get_fullscreen());
    }
}

fn clamp_pan(st: &mut State) {
    let (w, h) = (st.stage.0 as f64, st.stage.1 as f64);
    let min_x = (w - w * st.zoom).min(0.0);
    let min_y = (h - h * st.zoom).min(0.0);
    st.pan.0 = st.pan.0.clamp(min_x, 0.0);
    st.pan.1 = st.pan.1.clamp(min_y, 0.0);
}

fn sync_zoom_ui() {
    if let Some(w) = window() {
        w.global::<Pl>().set_zoom(with(|st| st.zoom) as f32);
    }
    push_view();
}

/// Roda sobre o palco: zoom ancorado no ponto do mouse (`onWheel`).
fn wheel_zoom(cx: f32, cy: f32, dy: f32) {
    with(|st| {
        let (cx, cy) = (cx as f64, cy as f64);
        let vx = (cx - st.pan.0) / st.zoom;
        let vy = (cy - st.pan.1) / st.zoom;
        let factor = if dy > 0.0 { 1.15 } else { 1.0 / 1.15 };
        st.zoom = (st.zoom * factor).clamp(1.0, ZOOM_MAX);
        st.pan = (cx - vx * st.zoom, cy - vy * st.zoom);
        clamp_pan(st);
    });
    sync_zoom_ui();
}

fn reset_zoom() {
    with(|st| {
        st.zoom = 1.0;
        st.pan = (0.0, 0.0);
    });
    sync_zoom_ui();
}

fn activity(x: f32, y: f32) {
    let Some(w) = window() else { return };
    let g = w.global::<Pl>();
    with(|st| {
        st.last_activity = Some(Instant::now());
        let (sw, _) = st.stage;
        st.near_edge = y <= EDGE_MARGIN || x <= EDGE_MARGIN || sw - x <= EDGE_MARGIN;
    });
    if g.get_idle() {
        g.set_idle(false);
    }
}

fn key(text: &str, ctrl: bool, alt: bool, _shift: bool) {
    let Some(w) = window() else { return };
    let g = w.global::<Pl>();
    // Teclas especiais chegam como o caractere de uso privado do Slint.
    const ESC: char = '\u{1b}';
    const LEFT: char = '\u{F702}';
    const RIGHT: char = '\u{F703}';
    let c = text.chars().next().unwrap_or('\0');
    match c {
        ESC => {
            // Em tela cheia, Esc sai dela; senão fecha o player.
            if g.get_fullscreen() {
                toggle_fullscreen();
            } else {
                close();
            }
        }
        ' ' => toggle_play(),
        'm' | 'M' if !ctrl && !alt => toggle_mute(),
        'f' | 'F' if !ctrl && !alt => toggle_fullscreen(),
        '0' if !ctrl && !alt => reset_zoom(),
        'c' | 'C' if !ctrl && !alt => toggle_captions(),
        LEFT | RIGHT => {
            let dir = if c == RIGHT { 1 } else { -1 };
            let playing = with(|st| st.pb.as_ref().is_some_and(|p| p.playing()));
            if playing {
                // Tocando: 5 s (navegar). Pausado: um quadro (revisar).
                seek_by(5.0 * dir as f64);
            } else {
                with(|st| {
                    if let Some(pb) = &st.pb {
                        pb.step(dir);
                    }
                });
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Painel de informações (`toggleInfoPanel` / `renderInfo`)
// ---------------------------------------------------------------------------

fn pl() -> Option<AppWindow> {
    window()
}

/// `formatBitrate`.
fn fmt_bitrate(bps: i64) -> String {
    if bps <= 0 {
        "—".into()
    } else if bps >= 1_000_000 {
        format!("{:.2} Mbps", bps as f64 / 1e6)
    } else if bps >= 1000 {
        format!("{} kbps", (bps as f64 / 1000.0).round())
    } else {
        format!("{bps} bps")
    }
}

/// `formatFps`: inteiro limpo, NTSC com duas casas.
fn fmt_fps(fps: f64) -> String {
    if fps <= 0.0 {
        return "—".into();
    }
    let r = fps.round();
    if (fps - r).abs() < 0.01 {
        format!("{r} fps")
    } else {
        format!("{fps:.2} fps")
    }
}

/// `formatBytes` (com ponto, como o JS).
fn fmt_bytes(b: i64) -> String {
    const KB: f64 = 1024.0;
    let f = b as f64;
    if f >= KB * KB * KB {
        format!("{:.1} GB", f / (KB * KB * KB))
    } else if f >= KB * KB {
        format!("{} MB", (f / (KB * KB)).round())
    } else if f >= KB {
        format!("{} KB", (f / KB).round())
    } else {
        format!("{b} B")
    }
}

/// `formatDuration` dos cards (`mm:ss` / `hh:mm:ss`).
fn fmt_duration(sec: i64) -> String {
    if sec <= 0 {
        return String::new();
    }
    let (h, m, s) = (sec / 3600, (sec % 3600) / 60, sec % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn row(key: &str, val: String) -> InfoRow {
    InfoRow { key: lang::t(key).into(), val: val.into() }
}

fn toggle_info() {
    let Some(w) = pl() else { return };
    let g = w.global::<Pl>();
    if g.get_info_open() {
        g.set_info_open(false);
        return;
    }
    // Um painel por vez: os dois ocupam o mesmo lado do palco.
    g.set_tr_open(false);
    g.set_info_open(true);
    let (need, id) = with(|st| (st.info.is_none() && !st.info_requested, st.id.clone()));
    if need {
        with(|st| st.info_requested = true);
        g.set_info_loading(true);
        bridge::handle_request_video_info(&id);
    }
}

pub fn on_video_info(info: &VideoInfo) {
    if with(|st| st.id != info.id) {
        return;
    }
    with(|st| {
        st.info = Some(info.clone());
        let n_audio = info.streams.iter().filter(|s| s.kind == "audio").count();
        if st.track_vols.len() != n_audio {
            st.track_vols = (0..n_audio).map(|i| if i == 0 { 100 } else { 0 }).collect();
            st.pre_mute = vec![0; n_audio];
        }
        // Índices que não existem mais (regravou com menos monitores).
        let n_regions = info.layout.as_ref().map_or(0, |l| l.regions.len());
        st.regions.retain(|&i| i < n_regions);
    });
    render_info();
}

fn render_info() {
    let Some(w) = pl() else { return };
    let g = w.global::<Pl>();
    let Some(info) = with(|st| st.info.clone()) else { return };
    let mut sections = Vec::new();
    let mut file = vec![
        row("player.info.name", info.file_name.clone()),
        row("player.info.container", info.format.clone()),
        row("player.info.duration", fmt_duration(info.duration.round() as i64)),
        row("player.info.size", fmt_bytes(info.size)),
    ];
    if info.bitrate > 0 {
        file.push(row("player.info.totalBitrate", fmt_bitrate(info.bitrate)));
    }
    sections.push(InfoSection { title: lang::t("player.info.file").into(), rows: ModelRc::from(Rc::new(VecModel::from(file))) });

    let videos: Vec<_> = info.streams.iter().filter(|s| s.kind == "video").collect();
    if !videos.is_empty() {
        let mut rows = Vec::new();
        for (i, s) in videos.iter().enumerate() {
            if videos.len() > 1 {
                rows.push(InfoRow { key: lang::t_with("player.info.stream", &[("n", &(i + 1).to_string())]).into(), val: SharedString::new() });
            }
            rows.push(row("player.info.codec", s.codec.to_uppercase()));
            if let (Some(wd), Some(ht)) = (s.width, s.height) {
                if wd > 0 && ht > 0 {
                    rows.push(row("player.info.resolution", format!("{wd} × {ht}")));
                }
            }
            if s.frame_rate.unwrap_or(0.0) > 0.0 {
                rows.push(row("player.info.frameRate", fmt_fps(s.frame_rate.unwrap_or(0.0))));
            }
            if s.bitrate > 0 {
                rows.push(row("player.info.bitrate", fmt_bitrate(s.bitrate)));
            }
        }
        sections.push(InfoSection { title: lang::t("player.info.video").into(), rows: ModelRc::from(Rc::new(VecModel::from(rows))) });
    }
    g.set_info_sections(ModelRc::from(Rc::new(VecModel::from(sections))));

    // Visualização: tela cheia (rádio) + uma caixa por região.
    let regions = with(|st| st.regions.clone());
    match &info.layout {
        Some(lay) if !lay.regions.is_empty() => {
            let mut opts = vec![ViewOpt { idx: -1, label: lang::t("player.info.fullView").into(), kind: "full".into(), selected: regions.is_empty() }];
            for (i, r) in lay.regions.iter().enumerate() {
                let label = if r.name.is_empty() { lang::t_with("player.info.region", &[("n", &(i + 1).to_string())]) } else { r.name.clone() };
                let kind = if r.kind.is_empty() { "monitor".to_owned() } else { r.kind.clone() };
                opts.push(ViewOpt { idx: i as i32, label: label.into(), kind: kind.into(), selected: regions.contains(&i) });
            }
            g.set_view_opts(ModelRc::from(Rc::new(VecModel::from(opts))));
            g.set_has_view(true);
        }
        _ => g.set_has_view(false),
    }

    // Áudio: uma caixa por faixa com o volume dela.
    let audios: Vec<_> = info.streams.iter().filter(|s| s.kind == "audio").collect();
    let vols = with(|st| st.track_vols.clone());
    let key = if audios.len() == 1 { "player.info.audio_one" } else { "player.info.audio_other" };
    g.set_tracks_title(lang::t_with(key, &[("count", &audios.len().to_string())]).into());
    let tracks: Vec<TrackRow> = audios
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let n = (i + 1).to_string();
            let title = if !s.title.is_empty() {
                s.title.clone()
            } else if i == 0 {
                lang::t_with("player.info.trackMix", &[("n", &n)])
            } else {
                lang::t_with("player.info.track", &[("n", &n)])
            };
            let mut rows = vec![row("player.info.codec", s.codec.to_uppercase())];
            if let Some(ch) = s.channels.filter(|c| *c > 0) {
                let v = match ch {
                    1 => lang::t("player.info.channelsMono"),
                    2 => lang::t("player.info.channelsStereo"),
                    n => lang::t_with("player.info.channelsN", &[("count", &n.to_string())]),
                };
                rows.push(InfoRow { key: lang::t("player.info.channels").into(), val: v.into() });
            }
            if let Some(sr) = s.sample_rate.filter(|r| *r > 0) {
                rows.push(row("player.info.sampleRate", format!("{:.1} kHz", sr as f64 / 1000.0)));
            }
            if s.bitrate > 0 {
                rows.push(row("player.info.bitrate", fmt_bitrate(s.bitrate)));
            }
            TrackRow { idx: i as i32, title: title.into(), rows: ModelRc::from(Rc::new(VecModel::from(rows))), vol: vols.get(i).copied().unwrap_or(0) }
        })
        .collect();
    g.set_tracks(ModelRc::from(Rc::new(VecModel::from(tracks))));
    g.set_info_loading(false);
}

fn view_clicked(idx: i32) {
    with(|st| {
        if idx < 0 {
            st.regions.clear();
        } else if !st.regions.remove(&(idx as usize)) {
            st.regions.insert(idx as usize);
        }
        // Mudou a área visível: o zoom anterior não vale mais.
        st.zoom = 1.0;
        st.pan = (0.0, 0.0);
    });
    if let Some(w) = pl() {
        let g = w.global::<Pl>();
        g.set_zoom(1.0);
        let regions = with(|st| st.regions.clone());
        let m = g.get_view_opts();
        for i in 0..m.row_count() {
            if let Some(mut o) = m.row_data(i) {
                o.selected = if o.idx < 0 { regions.is_empty() } else { regions.contains(&(o.idx as usize)) };
                m.set_row_data(i, o);
            }
        }
    }
    push_view();
}

fn set_track_volume(idx: i32, v: i32) {
    let i = idx.max(0) as usize;
    with(|st| {
        if i < st.track_vols.len() {
            st.track_vols[i] = v.clamp(0, 200);
        }
    });
    update_track_row(i);
    push_volumes();
}

/// Mutar zera guardando o volume; desmutar volta pra ele (ou 100).
fn track_mute(idx: i32) {
    let i = idx.max(0) as usize;
    with(|st| {
        if i >= st.track_vols.len() {
            return;
        }
        if st.track_vols[i] > 0 {
            st.pre_mute[i] = st.track_vols[i];
            st.track_vols[i] = 0;
        } else {
            st.track_vols[i] = if st.pre_mute[i] > 0 { st.pre_mute[i] } else { 100 };
            st.pre_mute[i] = 0;
        }
    });
    update_track_row(i);
    push_volumes();
}

fn update_track_row(i: usize) {
    let Some(w) = pl() else { return };
    let v = with(|st| st.track_vols.get(i).copied().unwrap_or(0));
    let m = w.global::<Pl>().get_tracks();
    if let Some(mut r) = m.row_data(i) {
        r.vol = v;
        m.set_row_data(i, r);
    }
}

// ---------------------------------------------------------------------------
// Transcrição e legenda (`renderTranscript` / `_syncTranscriptActive` /
// `_captionChunks`)
// ---------------------------------------------------------------------------

fn request_transcript() {
    let (need, id) = with(|st| (st.transcript.is_none() && !st.tr_requested, st.id.clone()));
    if need && !id.is_empty() {
        with(|st| st.tr_requested = true);
        bridge::handle_request_transcript(&id);
    }
}

fn toggle_transcript() {
    let Some(w) = pl() else { return };
    let g = w.global::<Pl>();
    let opening = !g.get_tr_open();
    if opening {
        g.set_info_open(false);
    }
    g.set_tr_open(opening);
    if !opening {
        return;
    }
    if with(|st| st.transcript.is_none()) {
        render_turns();
        request_transcript();
    } else {
        scroll_transcript_to_current();
    }
}

fn toggle_captions() {
    let on = with(|st| {
        st.captions_on = !st.captions_on;
        st.captions_on
    });
    // Preferência de VISUALIZAÇÃO desta máquina.
    config::set_config_bool("playerCaptions", on);
    if let Some(w) = pl() {
        let g = w.global::<Pl>();
        g.set_captions_on(on);
        if !on {
            g.set_caption(SharedString::new());
            return;
        }
    }
    if with(|st| st.transcript.is_none()) {
        with(|st| st.cap_notify = true);
        request_transcript();
    } else {
        notify_no_captions();
    }
    sync_caption();
}

/// Legenda ligada numa gravação sem fala: sem aviso parece defeito.
fn notify_no_captions() {
    let (on, empty, transcribed) = with(|st| {
        let t = st.transcript.as_ref();
        (st.captions_on, t.is_some_and(|t| t.turns.is_empty()), t.is_some_and(|t| t.transcribed))
    });
    if on && empty {
        let msg = if transcribed { lang::t("player.transcriptNoSpeech") } else { lang::t("player.transcriptNone") };
        crate::app_view::show_toast(&lang::t("player.captionsOverlay"), &msg, false, 0);
    }
}

pub fn on_transcript(t: &Transcript) {
    if with(|st| st.id != t.id) {
        return;
    }
    let notify = with(|st| {
        st.transcript = Some(t.clone());
        st.tr_active = -1;
        st.tr_pinned = -1;
        st.cap_cache = None;
        std::mem::take(&mut st.cap_notify)
    });
    render_turns();
    if notify {
        notify_no_captions();
    }
    if pl().is_some_and(|w| w.global::<Pl>().get_tr_open()) {
        scroll_transcript_to_current();
    }
    sync_caption();
}

/// "SPEAKER_00" não diz nada pra quem assiste: vira "Falante 1"; o nome dado
/// pelo usuário vence.
fn speaker_label(t: &Transcript, spk: &str) -> String {
    if let Some(n) = t.speakers.get(spk).filter(|n| !n.is_empty()) {
        return n.clone();
    }
    match speaker_number(spk) {
        Some(n) => lang::t_with("player.speakerN", &[("n", &(n + 1).to_string())]),
        None if spk.is_empty() => lang::t("player.speakerUnknown"),
        None => spk.to_owned(),
    }
}

fn speaker_number(spk: &str) -> Option<u32> {
    let s = spk.to_ascii_uppercase();
    let rest = s.strip_prefix("SPEAKER")?;
    rest.trim_start_matches(['_', '-']).parse().ok()
}

/// Cor estável por falante (seis matizes).
fn speaker_color(spk: &str) -> i32 {
    match speaker_number(spk) {
        Some(n) => (n % 6) as i32,
        None => {
            let mut h: i32 = 0;
            for c in spk.chars() {
                h = h.wrapping_mul(31).wrapping_add(c as i32);
            }
            (h.unsigned_abs() % 6) as i32
        }
    }
}

/// Texto pro `StyledText`: escapado, com o termo buscado em verde.
fn styled(text: &str, q: &str) -> slint::StyledText {
    fn esc(s: &str) -> String {
        let mut o = String::with_capacity(s.len());
        for c in s.chars() {
            if "\\`*_{}[]<>()#+-.!|~&".contains(c) {
                o.push('\\');
            }
            o.push(c);
        }
        o
    }
    let md = if q.is_empty() {
        esc(text)
    } else {
        let low = text.to_lowercase();
        let mut out = String::new();
        let mut from = 0;
        while let Some(at) = low[from..].find(q).map(|p| p + from) {
            // Índices do texto em minúsculas valem no original (mesmo comprimento em
            // quase tudo; se não, cai no texto sem destaque).
            if !text.is_char_boundary(at) || !text.is_char_boundary(at + q.len()) {
                break;
            }
            out.push_str(&esc(&text[from..at]));
            out.push_str("<font color=\"#10b981\">");
            out.push_str(&esc(&text[at..at + q.len()]));
            out.push_str("</font>");
            from = at + q.len();
        }
        out.push_str(&esc(&text[from.min(text.len())..]));
        out
    };
    slint::StyledText::from_markdown(&md).unwrap_or_else(|_| slint::StyledText::from_plain_text(text))
}

fn render_turns() {
    let Some(w) = pl() else { return };
    let g = w.global::<Pl>();
    let (t, q, active) = with(|st| (st.transcript.clone(), st.tr_query.clone(), st.tr_active));
    let Some(t) = t else {
        g.set_tr_empty(lang::t("player.transcriptLoading").into());
        g.set_turns(ModelRc::default());
        return;
    };
    if t.turns.is_empty() {
        g.set_tr_empty(if t.transcribed { lang::t("player.transcriptNoSpeech") } else { lang::t("player.transcriptNone") }.into());
        g.set_turns(ModelRc::default());
        return;
    }
    let rows: Vec<TurnRow> = t
        .turns
        .iter()
        .enumerate()
        .filter(|(_, tr)| q.is_empty() || tr.text.to_lowercase().contains(&q))
        .map(|(i, tr)| TurnRow {
            idx: i as i32,
            speaker: speaker_label(&t, &tr.speaker).into(),
            color: speaker_color(&tr.speaker),
            time: {
                let s = fmt_duration(tr.start.floor() as i64);
                if s.is_empty() { "00:00".into() } else { s.into() }
            },
            text: styled(&tr.text, &q),
            active: i as i32 == active,
        })
        .collect();
    g.set_tr_empty(if rows.is_empty() { lang::t("player.transcriptNoMatch").into() } else { SharedString::new() });
    g.set_turns(ModelRc::from(Rc::new(VecModel::from(rows))));
}

fn scroll_transcript_to_current() {
    let Some(w) = pl() else { return };
    sync_transcript_active(true);
    let g = w.global::<Pl>();
    let idx = with(|st| {
        let Some(t) = &st.transcript else { return -1 };
        if st.tr_active >= 0 {
            return st.tr_active;
        }
        // Entre duas falas: o último que já começou.
        let pos = st.pb.as_ref().map_or(0.0, |p| p.shared.position());
        t.turns.iter().rposition(|tr| tr.start <= pos).map_or(0, |i| i as i32)
    });
    // Posição na lista filtrada.
    let m = g.get_turns();
    let pos = (0..m.row_count()).find(|&i| m.row_data(i).is_some_and(|r| r.idx >= idx)).unwrap_or(0);
    g.set_tr_scroll_to(pos as i32);
    g.set_tr_scroll_seq(g.get_tr_scroll_seq() + 1);
}

/// O turno em reprodução. Turnos SOBREPOSTOS são normais (faixas isoladas,
/// gente falando junto): o clicado vence enquanto o tempo está nele; fora
/// disso, o que COMEÇOU por último entre os que contêm o instante (#60p).
fn sync_transcript_active(force: bool) {
    let Some(w) = pl() else { return };
    let changed = with(|st| {
        let Some(t) = &st.transcript else { return None };
        if t.turns.is_empty() {
            return None;
        }
        let pos = st.pb.as_ref().map_or(0.0, |p| p.shared.position());
        let mut idx = -1;
        let pin = st.tr_pinned;
        if pin >= 0 && (pin as usize) < t.turns.len() && pos >= t.turns[pin as usize].start - 0.5 && pos < t.turns[pin as usize].end {
            idx = pin;
        } else {
            st.tr_pinned = -1;
            let mut best = f64::MIN;
            for (i, tr) in t.turns.iter().enumerate() {
                if tr.start > pos {
                    break;
                }
                if pos < tr.end && tr.start >= best {
                    best = tr.start;
                    idx = i as i32;
                }
            }
        }
        if idx == st.tr_active && !force {
            return None;
        }
        st.tr_active = idx;
        Some(idx)
    });
    let Some(idx) = changed else { return };
    let m = w.global::<Pl>().get_turns();
    for i in 0..m.row_count() {
        if let Some(mut r) = m.row_data(i) {
            let a = r.idx == idx;
            if r.active != a {
                r.active = a;
                m.set_row_data(i, r);
            }
        }
    }
}

/// Parte o turno em blocos de até duas linhas pela largura da legenda (o
/// Slint não mede texto daqui: a média de largura do Segoe UI semibold,
/// ~0,55 do tamanho, faz o papel da régua invisível da web) e divide o tempo
/// pela proporção de caracteres.
pub(crate) fn caption_chunks(t: &Transcript, idx: usize, width_px: f32, font_px: f32) -> Vec<(String, f64, f64)> {
    let Some(turn) = t.turns.get(idx) else { return Vec::new() };
    let per_line = ((width_px / (font_px * 0.55)).floor() as usize).max(10);
    let max = per_line * 2;
    let mut texts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for wd in turn.text.split_whitespace() {
        let next = if cur.is_empty() { wd.to_owned() } else { format!("{cur} {wd}") };
        if !cur.is_empty() && next.chars().count() > max {
            texts.push(std::mem::take(&mut cur));
            cur = wd.to_owned();
        } else {
            cur = next;
        }
    }
    if !cur.is_empty() {
        texts.push(cur);
    }
    let dur = (turn.end - turn.start).max(0.0);
    let total: usize = texts.iter().map(|s| s.chars().count()).sum::<usize>().max(1);
    let mut acc = 0usize;
    texts
        .into_iter()
        .map(|s| {
            let a = turn.start + dur * acc as f64 / total as f64;
            acc += s.chars().count();
            let b = turn.start + dur * acc as f64 / total as f64;
            (s, a, b)
        })
        .collect()
}

fn sync_caption() {
    let Some(w) = pl() else { return };
    let g = w.global::<Pl>();
    if !g.get_captions_on() {
        return;
    }
    let (stage_w, win_w) = (with(|st| st.stage.0), w.window().size().width as f32 / w.window().scale_factor());
    let width = (stage_w * 0.86).min(960.0);
    let font = (win_w * 0.022).clamp(16.0, 30.0);
    let text = with(|st| {
        let idx = st.tr_active;
        let t = st.transcript.as_ref()?;
        if idx < 0 {
            return None;
        }
        let valid = st.cap_cache.as_ref().is_some_and(|(i, wd, _)| *i == idx && (*wd - width).abs() < 1.0);
        if !valid {
            st.cap_cache = Some((idx, width, caption_chunks(t, idx as usize, width, font)));
        }
        let pos = st.pb.as_ref().map_or(0.0, |p| p.shared.position());
        let chunks = &st.cap_cache.as_ref()?.2;
        let mut pick = chunks.first()?;
        for c in chunks {
            if pos >= c.1 {
                pick = c;
            } else {
                break;
            }
        }
        Some(pick.0.clone())
    });
    let s: SharedString = text.unwrap_or_default().into();
    if g.get_caption() != s {
        g.set_caption(s);
    }
}

fn turn_clicked(idx: i32) {
    let start = with(|st| st.transcript.as_ref().and_then(|t| t.turns.get(idx.max(0) as usize)).map(|t| t.start));
    let Some(start) = start else { return };
    with(|st| {
        st.tr_pinned = idx;
        if let Some(pb) = &st.pb {
            pb.seek(start.max(0.0));
        }
    });
    sync_transcript_active(true);
    sync_caption();
}

fn speaker_clicked(idx: i32) {
    let Some(w) = pl() else { return };
    let label = with(|st| {
        let t = st.transcript.as_ref()?;
        let tr = t.turns.get(idx.max(0) as usize)?;
        Some(speaker_label(t, &tr.speaker))
    });
    let Some(label) = label else { return };
    with(|st| st.editing_turn = idx);
    let g = w.global::<Pl>();
    g.set_editing_speaker_text(label.into());
    g.set_editing_turn(idx);
}

/// Renomeia o FALANTE (todos os turnos dele mudam de uma vez).
fn speaker_commit(text: &str) {
    let Some(w) = pl() else { return };
    let idx = with(|st| std::mem::replace(&mut st.editing_turn, -1));
    w.global::<Pl>().set_editing_turn(-1);
    if idx < 0 {
        return;
    }
    let nome: String = text.replace(['\r', '\n'], " ").trim().chars().take(60).collect();
    let change = with(|st| {
        let id = st.id.clone();
        let t = st.transcript.as_mut()?;
        let spk = t.turns.get(idx as usize)?.speaker.clone();
        let original = speaker_label(t, &spk);
        if nome == original {
            return None;
        }
        if nome.is_empty() {
            t.speakers.remove(&spk);
        } else {
            t.speakers.insert(spk.clone(), nome.clone());
        }
        Some((id, spk))
    });
    if let Some((id, spk)) = change {
        bridge::handle_set_speaker_name(&id, &spk, &nome);
    }
    render_turns();
}

fn speaker_cancel() {
    with(|st| st.editing_turn = -1);
    if let Some(w) = pl() {
        w.global::<Pl>().set_editing_turn(-1);
    }
}

fn tr_search(text: &str) {
    with(|st| st.tr_query = text.to_lowercase().trim().to_owned());
    render_turns();
    sync_transcript_active(true);
}

pub fn setup(w: &AppWindow) {
    let g = w.global::<Pl>();
    g.on_toggle_info(toggle_info);
    g.on_toggle_transcript(toggle_transcript);
    g.on_toggle_captions(toggle_captions);
    g.on_view_clicked(view_clicked);
    g.on_track_volume(set_track_volume);
    g.on_track_mute(track_mute);
    g.on_turn_clicked(turn_clicked);
    g.on_tr_search_edited(|t| tr_search(t.as_str()));
    g.on_speaker_clicked(speaker_clicked);
    g.on_speaker_commit(|t| speaker_commit(t.as_str()));
    g.on_speaker_cancel(speaker_cancel);
    g.on_stage_resized(|wd, ht| {
        with(|st| {
            st.stage = (wd, ht);
            clamp_pan(st);
        });
        push_view();
    });
    g.on_close(close);
    g.on_toggle_play(toggle_play);
    g.on_seek(seek_frac);
    g.on_seek_end(|| with(|st| st.seeking = false));
    g.on_set_volume(set_volume);
    g.on_toggle_mute(toggle_mute);
    g.on_set_speed(set_speed);
    g.on_toggle_fullscreen(toggle_fullscreen);
    g.on_wheel_zoom(wheel_zoom);
    g.on_pan_start(|| with(|st| st.pan_start = st.pan));
    g.on_pan_move(|dx, dy| {
        with(|st| {
            st.pan = (st.pan_start.0 + dx as f64, st.pan_start.1 + dy as f64);
            clamp_pan(st);
        });
        push_view();
    });
    g.on_reset_zoom(reset_zoom);
    g.on_activity(activity);
    g.on_key(|t, c, a, s| key(t.as_str(), c, a, s));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotulos_como_a_web() {
        assert_eq!(speed_label(1.0), "1×");
        assert_eq!(speed_label(0.25), "0,25×");
        assert_eq!(speed_label(1.5), "1,5×");
        assert_eq!(fmt_time(65.4), "01:05");
        assert_eq!(fmt_time(3725.0), "1:02:05");
        assert_eq!(fmt_clock(86400 + 3661), "01:01:01");
    }
}

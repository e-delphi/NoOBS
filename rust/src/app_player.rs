//! Presenter do player: abre o motor (`playback`), põe os quadros no palco
//! (`FrameView`), mantém barra/tempo/relógio, e cuida de zoom, volume,
//! velocidade, teclado, do esconder dos controles e dos painéis de
//! informações e de transcrição (com a legenda). O desenho é a camada
//! `pnlPlayer` do `forms/main.json`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iron::prelude::*;

use crate::app_view::{main_form, with_ui};
use crate::playback::{Out, Playback, Region, ViewSpec, TRICK_MIN_RATE};
use crate::view::{Transcript, VideoInfo};
use crate::{bridge, config, lang, log, mainthread};

/// Os controles somem depois disto sem mexer o mouse, tocando.
const IDLE_MS: u64 = 2000;
/// Zoom máximo da roda.
const ZOOM_MAX: f64 = 8.0;
const WAVE_BUCKETS: i64 = 2000;
/// As velocidades do menu (de 8× pra cima anda por saltos de keyframe).
const SPEEDS: [f64; 9] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 8.0, 16.0, 32.0];
/// As seis cores dos falantes.
const SPEAKER_COLORS: [u32; 6] = [0x60a5fa, 0x34d399, 0xf472b6, 0xfbbf24, 0xa78bfa, 0xf87171];

#[derive(Default)]
struct State {
    id: String,
    pb: Option<Playback>,
    /// Palco em px lógicos e a escala da janela.
    stage: (f32, f32),
    scale: f32,
    zoom: f64,
    pan: (f64, f64),
    pan_start: Option<((f64, f64), PointF)>,
    /// (zoom, pan_x, pan_y) do quadro na tela, em px lógicos.
    frame_view: (f64, f64, f64),
    master: i32,
    muted: bool,
    speed: f64,
    duration: f64,
    start_clock: i32,
    playing: bool,
    last_activity: Option<Instant>,
    idle: bool,
    fullscreen: bool,
    tick_job: Option<u64>,
    peaks: Option<(String, Vec<f32>)>,
    /// O painel aberto: "info", "transcript" ou vazio.
    side: String,
    info: Option<VideoInfo>,
    info_requested: bool,
    regions: BTreeSet<usize>,
    track_vols: Vec<i32>,
    pre_mute: Vec<i32>,
    transcript: Option<Transcript>,
    tr_requested: bool,
    tr_active: i32,
    tr_pinned: i32,
    tr_query: String,
    /// As linhas da transcrição na tela (índice do turno → painel).
    tr_rows: Vec<(usize, ComponentId)>,
    captions_on: bool,
    cap_cache: Option<(i32, f32, Vec<(String, f64, f64)>)>,
    cap_notify: bool,
    speed_menu: Option<ComponentId>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State {
        zoom: 1.0,
        master: 100,
        speed: 1.0,
        scale: 1.0,
        tr_active: -1,
        tr_pinned: -1,
        ..Default::default()
    });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn c(ui: &Ui, name: &str) -> Option<ComponentId> {
    ui.find_in(main_form()?, name)
}

fn set(ui: &mut Ui, name: &str, prop: &str, value: impl Into<PropValue>) {
    if let Some(id) = c(ui, name) {
        let _ = ui.set_prop(id, prop, value);
    }
}

/// A escala da janela (DPI / 96): o motor trabalha em pixels físicos.
pub(crate) fn window_scale() -> f32 {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    let Some(h) = main_form().and_then(iron::platform::win::hwnd) else { return 1.0 };
    let dpi = unsafe { GetDpiForWindow(HWND(h as *mut _)) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

/// `mm:ss` ou `h:mm:ss`.
fn fmt_time(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    let (h, m, ss) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{ss:02}")
    } else {
        format!("{m:02}:{ss:02}")
    }
}

/// Hora do relógio, `HH:MM:SS`, volta à meia-noite.
fn fmt_clock(sec: i64) -> String {
    let s = sec.rem_euclid(86400);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// `1×`, `0,5×`, `1,5×` (vírgula).
fn speed_label(r: f64) -> String {
    if (r - 1.0).abs() < 1e-9 {
        return "1×".into();
    }
    format!("{}×", format!("{r}").replace('.', ","))
}

fn view_spec(st: &State) -> ViewSpec {
    let k = st.scale as f64;
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
    let (vw, vh) = info.streams.iter().find(|s| s.kind == "video").map(|s| (s.width.unwrap_or(0), s.height.unwrap_or(0))).unwrap_or((0, 0));
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

fn push_view(ui: &mut Ui) {
    let v = with(|st| view_spec(st));
    with(|st| {
        if let Some(pb) = &st.pb {
            pb.set_view(v);
        }
    });
    sync_frame_shift(ui);
}

/// Desloca o quadro mostrado pela diferença entre o deslocamento atual e o da
/// vista que o gerou (só com o mesmo zoom).
fn sync_frame_shift(ui: &mut Ui) {
    let (dx, dy) = with(|st| {
        let (z, px, py) = st.frame_view;
        if (z - st.zoom).abs() < 1e-9 { (st.pan.0 - px, st.pan.1 - py) } else { (0.0, 0.0) }
    });
    if let Some(v) = c(ui, "frmVideo").and_then(|id| ui.get_mut::<FrameView>(id)) {
        v.set_shift(dx as f32, dy as f32);
    }
    ui.invalidate();
}

fn push_volumes(ui: &mut Ui) {
    let (m, muted) = with(|st| {
        let master = if st.muted { 0.0 } else { st.master as f32 / 100.0 };
        let tracks: Vec<f32> = if st.track_vols.is_empty() { vec![1.0] } else { st.track_vols.iter().map(|v| *v as f32 / 100.0).collect() };
        if let Some(pb) = &st.pb {
            pb.set_volumes(master, tracks);
        }
        (st.master, st.muted)
    });
    set(ui, "trkVolume", "value", if muted { 0.0 } else { m as f64 });
    set(ui, "btnMute", "icon", if muted || m == 0 { "volume-off" } else { "volume" });
}

// ---------------------------------------------------------------------------
// Abrir / fechar
// ---------------------------------------------------------------------------

/// Abre a gravação no player (clique no cartão).
pub fn open(ui: &mut Ui, id: &str) {
    if !bridge::check_recording_for_player(id) {
        return;
    }
    close_engine(ui);
    let title = std::path::Path::new(id).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let start_clock = bridge::recording_start_clock_sec(id);
    set(ui, "pnlPlayer", "visible", true);
    set(ui, "lblPlayerTitle", "text", title);
    set(ui, "lblPlayerLoading", "text", lang::t("player.loading"));
    set(ui, "lblPlayerLoading", "visible", true);
    set(ui, "lblPlayerTime", "text", "00:00 / 00:00");
    set(ui, "lblPlayerClock", "text", "");
    set(ui, "lblPlayerPaused", "visible", false);
    set(ui, "btnPlay", "icon", "pause");
    set(ui, "btnSpeed", "text", speed_label(1.0));
    set(ui, "lblCaption", "visible", false);
    set(ui, "pnlPlayerSide", "visible", false);
    set_controls_visible(ui, true);
    if let Some(v) = c(ui, "frmVideo").and_then(|id| ui.get_mut::<FrameView>(id)) {
        v.set_frame(None);
    }
    if let Some(w) = c(ui, "wavPlayer").and_then(|id| ui.get_mut::<Waveform>(id)) {
        w.set_peaks(Vec::new());
        w.position = 0.0;
    }
    ui.layout();
    let stage = c(ui, "frmVideo").and_then(|id| ui.bounds(id)).map_or((1280.0, 720.0), |b| (b.width, b.height));
    let captions = config::get_config_bool("playerCaptions", false);
    let view = with(|st| {
        st.id = id.to_owned();
        st.stage = stage;
        st.scale = window_scale();
        st.zoom = 1.0;
        st.pan = (0.0, 0.0);
        // Volume geral volta a 100 % em toda abertura.
        st.master = 100;
        st.muted = false;
        st.speed = 1.0;
        st.duration = 0.0;
        st.start_clock = start_clock;
        st.playing = true;
        st.last_activity = Some(Instant::now());
        st.idle = false;
        // Tudo do vídeo anterior sai (a região, a transcrição).
        st.side.clear();
        st.info = None;
        st.info_requested = false;
        st.regions.clear();
        st.track_vols.clear();
        st.pre_mute.clear();
        st.transcript = None;
        st.tr_requested = false;
        st.tr_active = -1;
        st.tr_pinned = -1;
        st.tr_rows.clear();
        st.cap_cache = None;
        st.captions_on = captions;
        view_spec(st)
    });
    set(ui, "btnCaptions", "checked", captions);
    if captions {
        request_transcript();
    }
    let id_out = id.to_owned();
    let out: Arc<dyn Fn(Out) + Send + Sync> = Arc::new(move |o| {
        let id = id_out.clone();
        mainthread::queue(move || with_ui(move |ui| on_out(ui, &id, o)));
    });
    let pb = Playback::open(id, view, out);
    with(|st| st.pb = Some(pb));
    push_volumes(ui);
    tick(ui);
    // Forma de onda: pedida um instante depois (o vídeo tem prioridade).
    let wid = id.to_owned();
    ui.set_timeout(100, move |ui| {
        if with(|st| st.id == wid) {
            if with(|st| st.peaks.as_ref().is_some_and(|(i, _)| i == &wid)) {
                render_waveform(ui);
            } else {
                bridge::handle_request_waveform(&wid, WAVE_BUCKETS, false);
            }
        }
    });
    if let Some(stage) = c(ui, "frmVideo") {
        ui.set_focus(stage);
    }
    bridge::set_player_open(true);
    log!("Player: abrindo {id}");
}

fn close_engine(ui: &mut Ui) {
    let (pb, job) = with(|st| (st.pb.take(), st.tick_job.take()));
    if let Some(job) = job {
        ui.clear_timeout(job);
    }
    // Soltar o handle fecha os canais; as threads saem sozinhas.
    drop(pb);
}

pub fn close(ui: &mut Ui) {
    close_engine(ui);
    if with(|st| st.fullscreen) {
        toggle_fullscreen(ui);
    }
    set(ui, "pnlPlayer", "visible", false);
    if let Some(v) = c(ui, "frmVideo").and_then(|id| ui.get_mut::<FrameView>(id)) {
        v.set_frame(None);
    }
    with(|st| {
        st.id.clear();
        st.tr_query.clear();
    });
    bridge::set_player_open(false);
}

pub fn is_open() -> bool {
    with(|st| !st.id.is_empty())
}

// ---------------------------------------------------------------------------
// Do motor
// ---------------------------------------------------------------------------

fn on_out(ui: &mut Ui, id: &str, o: Out) {
    if with(|st| st.id != id) {
        return;
    }
    match o {
        Out::Info(i) => {
            with(|st| st.duration = i.duration);
            set(ui, "lblPlayerLoading", "visible", false);
        }
        Out::Frame { buf, x, y, view } => {
            let k = with(|st| st.scale);
            let (bw, bh) = (buf.width, buf.height);
            let image = Image::rgba(bw, bh, buf.pixels);
            let rect = RectF::new(x as f32 / k, y as f32 / k, bw as f32 / k, bh as f32 / k);
            if let Some(v) = c(ui, "frmVideo").and_then(|id| ui.get_mut::<FrameView>(id)) {
                v.set_frame(Some((image, rect)));
            }
            with(|st| st.frame_view = (view.0, view.1 / k as f64, view.2 / k as f64));
            sync_frame_shift(ui);
            set(ui, "lblPlayerLoading", "visible", false);
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
            set_playing_ui(ui, false);
        }
        Out::Error(e) => {
            set(ui, "lblPlayerLoading", "text", lang::t("error.loadVideo"));
            set(ui, "lblPlayerLoading", "visible", true);
            log!("Player: erro {e}");
        }
    }
}

fn set_playing_ui(ui: &mut Ui, playing: bool) {
    with(|st| {
        st.playing = playing;
        st.last_activity = Some(Instant::now());
    });
    set(ui, "btnPlay", "icon", if playing { "pause" } else { "play" });
    set(ui, "btnPlay", "hint", lang::t("player.playPause"));
    set(ui, "lblPlayerPaused", "visible", !playing);
    set_controls_visible(ui, true);
}

fn set_controls_visible(ui: &mut Ui, on: bool) {
    let was_idle = with(|st| std::mem::replace(&mut st.idle, !on));
    if was_idle == !on {
        return;
    }
    set(ui, "pnlPlayerHead", "visible", on);
    set(ui, "pnlPlayerBar", "visible", on);
    if let Some(cb) = c(ui, "frmVideo").and_then(|id| ui.control_mut(id)) {
        cb.set_cursor(if on { CursorShape::Default } else { CursorShape::None });
    }
}

/// Barra, tempo e relógio a ~30 Hz.
fn tick(ui: &mut Ui) {
    let (pos, dur, clock, playing, idle_due, hold) = with(|st| {
        let pos = st.pb.as_ref().map_or(0.0, |p| p.shared.position());
        let dur = st.pb.as_ref().map_or(st.duration, |p| p.shared.duration().max(st.duration));
        let due = st.last_activity.is_some_and(|t| t.elapsed() >= Duration::from_millis(IDLE_MS));
        (pos, dur, st.start_clock, st.playing, due, !st.side.is_empty())
    });
    if let Some(w) = c(ui, "wavPlayer").and_then(|id| ui.get_mut::<Waveform>(id)) {
        w.duration = dur.max(0.001);
        w.position = pos;
    }
    set(ui, "lblPlayerTime", "text", format!("{} / {}", fmt_time(pos), fmt_time(dur)));
    set(ui, "lblPlayerClock", "text", if clock >= 0 { fmt_clock(clock as i64 + pos.floor() as i64) } else { String::new() });
    // Os controles somem parado o mouse (só tocando, sem painel aberto).
    if playing && idle_due && !hold && !with(|st| st.idle) {
        set_controls_visible(ui, false);
    }
    sync_transcript_active(ui, false);
    sync_caption(ui);
    let job = ui.set_timeout(33, tick);
    with(|st| st.tick_job = Some(job));
}

// ---------------------------------------------------------------------------
// Forma de onda
// ---------------------------------------------------------------------------

pub fn on_waveform(id: &str, peaks: &[f32]) {
    let (id, peaks) = (id.to_owned(), peaks.to_vec());
    with_ui(move |ui| {
        let current = with(|st| {
            st.peaks = Some((id.clone(), peaks));
            st.id == id
        });
        if current {
            render_waveform(ui);
        }
    });
}

/// Reagrega os picos pro número de barras que cabem (raiz quadrada: realça
/// as variações; mínimo de 4 % pra silêncio ter linha).
fn render_waveform(ui: &mut Ui) {
    let Some((id, peaks)) = with(|st| st.peaks.clone()) else { return };
    if with(|st| st.id != id) || peaks.is_empty() {
        return;
    }
    let width = c(ui, "wavPlayer").and_then(|w| ui.bounds(w)).map_or(600.0, |b| b.width);
    let bars = ((width / 3.0) as usize).clamp(1, peaks.len());
    let agg: Vec<f32> = (0..bars)
        .map(|i| {
            let a = i * peaks.len() / bars;
            let b = ((i + 1) * peaks.len() / bars).max(a + 1);
            peaks[a..b.min(peaks.len())].iter().cloned().fold(0.0, f32::max)
        })
        .collect();
    let maxp = agg.iter().cloned().fold(0.0001f32, f32::max);
    let norm: Vec<f32> = agg.iter().map(|p| (p / maxp).sqrt().max(0.04)).collect();
    if let Some(w) = c(ui, "wavPlayer").and_then(|id| ui.get_mut::<Waveform>(id)) {
        w.set_peaks(norm);
    }
    ui.invalidate();
}

// ---------------------------------------------------------------------------
// Gestos
// ---------------------------------------------------------------------------

pub fn toggle_play(ui: &mut Ui) {
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
        set_playing_ui(ui, p);
    }
}

pub fn seek_to(_ui: &mut Ui, t: f64) {
    with(|st| {
        if let Some(pb) = &st.pb {
            let d = pb.shared.duration().max(st.duration);
            pb.seek(t.clamp(0.0, d));
        }
    });
}

fn seek_by(delta: f64) {
    with(|st| {
        if let Some(pb) = &st.pb {
            let d = pb.shared.duration().max(st.duration);
            pb.seek((pb.shared.position() + delta).clamp(0.0, d));
        }
    });
}

fn set_speed(ui: &mut Ui, r: f64) {
    with(|st| {
        st.speed = r;
        if let Some(pb) = st.pb.as_mut() {
            pb.set_rate(r);
        }
    });
    set(ui, "btnSpeed", "text", speed_label(r));
    let hint = if r >= TRICK_MIN_RATE { lang::t("player.trickHint") } else { lang::t("player.speed") };
    set(ui, "btnSpeed", "hint", hint);
}

/// O menu de velocidades, em cima do botão.
pub fn speed_menu(ui: &mut Ui, sender: ComponentId) {
    let Some(form) = main_form() else { return };
    let menu = match with(|st| st.speed_menu).filter(|m| ui.contains(*m)) {
        Some(m) => m,
        None => {
            let Ok(m) = ui.create("PopupMenu", Some(form)) else { return };
            for s in SPEEDS {
                if let Ok(item) = ui.create("MenuItem", Some(m)) {
                    let _ = ui.set_prop(item, "text", speed_label(s));
                    ui.on(item, "on_click", move |ui, _, _| set_speed(ui, s));
                }
            }
            with(|st| st.speed_menu = Some(m));
            m
        }
    };
    let current = with(|st| st.speed);
    for (item, s) in ui.children(menu).to_vec().into_iter().zip(SPEEDS) {
        let _ = ui.set_prop(item, "checked", (s - current).abs() < 1e-9);
    }
    if let Some(cb) = ui.control_mut(sender) {
        cb.set_popup_menu(Some(menu));
    }
    let at = ui.absolute_rect(sender).map_or(PointF::ZERO, |r| PointF::new(r.x, r.y - 8.0 - 30.0 * SPEEDS.len() as f32));
    ui.open_popup_menu(sender, at);
}

pub fn set_volume(ui: &mut Ui, v: f64) {
    with(|st| {
        st.master = (v.round() as i32).clamp(0, 200);
        st.muted = st.master == 0;
    });
    push_volumes(ui);
}

pub fn toggle_mute(ui: &mut Ui) {
    with(|st| st.muted = !st.muted);
    push_volumes(ui);
}

pub fn toggle_fullscreen(ui: &mut Ui) {
    crate::ui::toggle_fullscreen();
    let on = with(|st| {
        st.fullscreen = !st.fullscreen;
        st.fullscreen
    });
    set(ui, "btnFullscreen", "icon", if on { "fullscreen-exit" } else { "fullscreen" });
}

fn clamp_pan(st: &mut State) {
    let (w, h) = (st.stage.0 as f64, st.stage.1 as f64);
    st.pan.0 = st.pan.0.clamp((w - w * st.zoom).min(0.0), 0.0);
    st.pan.1 = st.pan.1.clamp((h - h * st.zoom).min(0.0), 0.0);
}

/// O palco mudou de tamanho (janela, tela cheia, painel).
pub fn stage_resized(ui: &mut Ui) {
    let Some(b) = c(ui, "frmVideo").and_then(|id| ui.bounds(id)) else { return };
    let changed = with(|st| {
        let new = (b.width, b.height);
        let changed = st.stage != new;
        st.stage = new;
        st.scale = window_scale();
        clamp_pan(st);
        changed && !st.id.is_empty()
    });
    if changed {
        push_view(ui);
        render_waveform(ui);
    }
}

/// Roda sobre o palco: zoom ancorado no ponto do mouse.
pub fn wheel(ui: &mut Ui, e: &WheelEvent) {
    activity(ui);
    with(|st| {
        let (cx, cy) = (e.position.x as f64, e.position.y as f64);
        let vx = (cx - st.pan.0) / st.zoom;
        let vy = (cy - st.pan.1) / st.zoom;
        let factor = if e.delta.y > 0.0 { 1.15 } else { 1.0 / 1.15 };
        st.zoom = (st.zoom * factor).clamp(1.0, ZOOM_MAX);
        st.pan = (cx - vx * st.zoom, cy - vy * st.zoom);
        clamp_pan(st);
    });
    push_view(ui);
}

pub fn reset_zoom(ui: &mut Ui) {
    with(|st| {
        st.zoom = 1.0;
        st.pan = (0.0, 0.0);
    });
    push_view(ui);
}

/// Botão no palco: com zoom arrasta; sem, o clique pausa/toca.
pub fn stage_down(ui: &mut Ui, e: &PointerEvent) {
    activity(ui);
    with(|st| st.pan_start = Some((st.pan, e.position)));
}

pub fn stage_move(ui: &mut Ui, e: &PointerEvent) {
    activity(ui);
    let moved = with(|st| {
        let Some((pan, from)) = st.pan_start else { return false };
        if st.zoom <= 1.0 {
            return false;
        }
        st.pan = (pan.0 + (e.position.x - from.x) as f64, pan.1 + (e.position.y - from.y) as f64);
        clamp_pan(st);
        true
    });
    if moved {
        push_view(ui);
    }
}

pub fn stage_up(ui: &mut Ui, e: &PointerEvent) {
    let click = with(|st| {
        let start = st.pan_start.take();
        start.is_some_and(|(_, from)| (from.x - e.position.x).abs() < 4.0 && (from.y - e.position.y).abs() < 4.0)
    });
    if click && e.button == PointerButton::Primary {
        toggle_play(ui);
    }
}

pub fn activity(ui: &mut Ui) {
    with(|st| st.last_activity = Some(Instant::now()));
    if with(|st| st.idle) {
        set_controls_visible(ui, true);
    }
}

/// O teclado do player (Esc, espaço, setas, M, F, 0, C).
pub fn key(ui: &mut Ui, k: &KeyEvent) -> bool {
    if !is_open() {
        return false;
    }
    let plain = !k.modifiers.ctrl && !k.modifiers.alt;
    match k.key {
        Key::Escape => {
            if with(|st| st.fullscreen) {
                toggle_fullscreen(ui);
            } else {
                close(ui);
            }
        }
        Key::Space => toggle_play(ui),
        Key::Char('m') | Key::Char('M') if plain => toggle_mute(ui),
        Key::Char('f') | Key::Char('F') if plain => toggle_fullscreen(ui),
        Key::Char('0') if plain => reset_zoom(ui),
        Key::Char('c') | Key::Char('C') if plain => toggle_captions(ui),
        Key::Left | Key::Right => {
            let dir = if k.key == Key::Right { 1 } else { -1 };
            let playing = with(|st| st.pb.as_ref().is_some_and(|p| p.playing()));
            if playing {
                // Tocando: 5 s. Pausado: um quadro.
                seek_by(5.0 * dir as f64);
            } else {
                with(|st| {
                    if let Some(pb) = &st.pb {
                        pb.step(dir);
                    }
                });
            }
        }
        _ => return false,
    }
    activity(ui);
    true
}

// ---------------------------------------------------------------------------
// Painéis (informações e transcrição)
// ---------------------------------------------------------------------------

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

fn fmt_fps(fps: f64) -> String {
    if fps <= 0.0 {
        return "—".into();
    }
    let r = fps.round();
    if (fps - r).abs() < 0.01 { format!("{r} fps") } else { format!("{fps:.2} fps") }
}

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

fn fmt_duration(sec: i64) -> String {
    if sec <= 0 {
        return String::new();
    }
    let (h, m, s) = (sec / 3600, (sec % 3600) / 60, sec % 60);
    if h > 0 { format!("{h:02}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

fn side_panel(ui: &mut Ui) -> Option<ComponentId> {
    let p = c(ui, "lySide")?;
    for child in ui.children(p).to_vec() {
        ui.free(child);
    }
    Some(p)
}

fn add_label(ui: &mut Ui, parent: ComponentId, text: &str, style: &str, height: f32, top: f32) -> Option<ComponentId> {
    let l = ui.create("Label", Some(parent)).ok()?;
    let _ = ui.set_prop(l, "style", style);
    let _ = ui.set_prop(l, "text", text);
    if let Some(cb) = ui.control_mut(l) {
        cb.set_align(Align::Top).set_height(height).set_margins([0.0, top, 0.0, 0.0]);
    }
    Some(l)
}

/// Uma linha "chave  valor".
fn add_row(ui: &mut Ui, parent: ComponentId, key: &str, val: &str) {
    let Ok(row) = ui.create("Layout", Some(parent)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(22.0);
    }
    if let Ok(k) = ui.create("Label", Some(row)) {
        let _ = ui.set_prop(k, "style", "key");
        let _ = ui.set_prop(k, "text", key);
        if let Some(cb) = ui.control_mut(k) {
            cb.set_align(Align::Left).set_width(130.0);
        }
    }
    if let Ok(v) = ui.create("Label", Some(row)) {
        let _ = ui.set_prop(v, "style", "val");
        let _ = ui.set_prop(v, "text", val);
        if let Some(cb) = ui.control_mut(v) {
            cb.set_align(Align::Client);
        }
    }
}

fn fit_side(ui: &mut Ui) {
    let Some(side) = c(ui, "lySide") else { return };
    let h: f32 = ui
        .children(side)
        .to_vec()
        .iter()
        .filter_map(|&id| ui.control(id).map(|cb| cb.height() + cb.margins().top + cb.margins().bottom))
        .sum();
    if let Some(cb) = ui.control_mut(side) {
        cb.set_height(h + 24.0);
    }
    ui.layout();
}

fn open_side(ui: &mut Ui, which: &str) {
    let opening = with(|st| {
        let opening = st.side != which;
        st.side = if opening { which.to_owned() } else { String::new() };
        opening
    });
    set(ui, "pnlPlayerSide", "visible", opening);
    set(ui, "btnInfo", "checked", opening && which == "info");
    set(ui, "btnTranscript", "checked", opening && which == "transcript");
    set(ui, "edTrSearch", "visible", which == "transcript");
    ui.layout();
    stage_resized(ui);
}

pub fn toggle_info(ui: &mut Ui) {
    open_side(ui, "info");
    if with(|st| st.side != "info") {
        return;
    }
    set(ui, "lblSideTitle", "text", lang::t("player.videoInfo"));
    let (need, id) = with(|st| (st.info.is_none() && !st.info_requested, st.id.clone()));
    if need {
        with(|st| st.info_requested = true);
        if let Some(p) = side_panel(ui) {
            add_label(ui, p, &lang::t("player.loading"), "key", 24.0, 0.0);
        }
        bridge::handle_request_video_info(&id);
    } else {
        render_info(ui);
    }
}

pub fn on_video_info(info: &VideoInfo) {
    let info = info.clone();
    with_ui(move |ui| {
        if with(|st| st.id != info.id) {
            return;
        }
        with(|st| {
            let n_audio = info.streams.iter().filter(|s| s.kind == "audio").count();
            if st.track_vols.len() != n_audio {
                st.track_vols = (0..n_audio).map(|i| if i == 0 { 100 } else { 0 }).collect();
                st.pre_mute = vec![0; n_audio];
            }
            let n_regions = info.layout.as_ref().map_or(0, |l| l.regions.len());
            st.regions.retain(|&i| i < n_regions);
            st.info = Some(info);
        });
        if with(|st| st.side == "info") {
            render_info(ui);
        }
    });
}

fn render_info(ui: &mut Ui) {
    let Some(info) = with(|st| st.info.clone()) else { return };
    let Some(p) = side_panel(ui) else { return };
    add_label(ui, p, &lang::t("player.info.file"), "section", 20.0, 4.0);
    add_row(ui, p, &lang::t("player.info.name"), &info.file_name);
    add_row(ui, p, &lang::t("player.info.container"), &info.format);
    add_row(ui, p, &lang::t("player.info.duration"), &fmt_duration(info.duration.round() as i64));
    add_row(ui, p, &lang::t("player.info.size"), &fmt_bytes(info.size));
    if info.bitrate > 0 {
        add_row(ui, p, &lang::t("player.info.totalBitrate"), &fmt_bitrate(info.bitrate));
    }
    let videos: Vec<_> = info.streams.iter().filter(|s| s.kind == "video").collect();
    if !videos.is_empty() {
        add_label(ui, p, &lang::t("player.info.video"), "section", 20.0, 14.0);
        for s in videos {
            add_row(ui, p, &lang::t("player.info.codec"), &s.codec.to_uppercase());
            if let (Some(wd), Some(ht)) = (s.width, s.height) {
                if wd > 0 && ht > 0 {
                    add_row(ui, p, &lang::t("player.info.resolution"), &format!("{wd} × {ht}"));
                }
            }
            if s.frame_rate.unwrap_or(0.0) > 0.0 {
                add_row(ui, p, &lang::t("player.info.frameRate"), &fmt_fps(s.frame_rate.unwrap_or(0.0)));
            }
            if s.bitrate > 0 {
                add_row(ui, p, &lang::t("player.info.bitrate"), &fmt_bitrate(s.bitrate));
            }
        }
    }
    // Visualização: tela cheia ou as regiões marcadas.
    if let Some(lay) = info.layout.as_ref().filter(|l| !l.regions.is_empty()) {
        add_label(ui, p, &lang::t("player.info.visualization"), "section", 20.0, 14.0);
        let regions = with(|st| st.regions.clone());
        let mut opts = vec![(-1i32, lang::t("player.info.fullView"), regions.is_empty())];
        for (i, r) in lay.regions.iter().enumerate() {
            let label = if r.name.is_empty() { lang::t_with("player.info.region", &[("n", &(i + 1).to_string())]) } else { r.name.clone() };
            opts.push((i as i32, label, regions.contains(&i)));
        }
        for (idx, label, on) in opts {
            if let Ok(b) = ui.create("CheckBox", Some(p)) {
                let _ = ui.set_prop(b, "text", label);
                let _ = ui.set_prop(b, "checked", on);
                if let Some(cb) = ui.control_mut(b) {
                    cb.set_align(Align::Top).set_height(26.0);
                }
                ui.on(b, "on_change", move |ui, _, _| view_clicked(ui, idx));
            }
        }
    }
    // Áudio: uma faixa por vez, com o volume dela.
    let audios: Vec<_> = info.streams.iter().filter(|s| s.kind == "audio").collect();
    if !audios.is_empty() {
        let key = if audios.len() == 1 { "player.info.audio_one" } else { "player.info.audio_other" };
        add_label(ui, p, &lang::t_with(key, &[("count", &audios.len().to_string())]), "section", 20.0, 14.0);
        let vols = with(|st| st.track_vols.clone());
        for (i, s) in audios.iter().enumerate() {
            let n = (i + 1).to_string();
            let title = if !s.title.is_empty() {
                s.title.clone()
            } else if i == 0 {
                lang::t_with("player.info.trackMix", &[("n", &n)])
            } else {
                lang::t_with("player.info.track", &[("n", &n)])
            };
            add_label(ui, p, &title, "row", 20.0, 8.0);
            let mut detail = s.codec.to_uppercase();
            if let Some(ch) = s.channels.filter(|c| *c > 0) {
                detail.push_str(" · ");
                detail.push_str(&match ch {
                    1 => lang::t("player.info.channelsMono"),
                    2 => lang::t("player.info.channelsStereo"),
                    n => lang::t_with("player.info.channelsN", &[("count", &n.to_string())]),
                });
            }
            if let Some(sr) = s.sample_rate.filter(|r| *r > 0) {
                detail.push_str(&format!(" · {:.1} kHz", sr as f64 / 1000.0));
            }
            add_label(ui, p, &detail, "key", 18.0, 0.0);
            let Ok(row) = ui.create("Layout", Some(p)) else { continue };
            if let Some(cb) = ui.control_mut(row) {
                cb.set_align(Align::Top).set_height(30.0);
            }
            let vol = vols.get(i).copied().unwrap_or(0);
            if let Ok(m) = ui.create("Button", Some(row)) {
                let _ = ui.set_prop(m, "style", "tool");
                let _ = ui.set_prop(m, "text", "");
                let _ = ui.set_prop(m, "icon", if vol == 0 { "volume-off" } else { "volume" });
                if let Some(cb) = ui.control_mut(m) {
                    cb.set_align(Align::Left).set_width(28.0).set_margins([0.0, 1.0, 8.0, 1.0]);
                }
                ui.on(m, "on_click", move |ui, _, _| track_mute(ui, i));
            }
            if let Ok(t) = ui.create("TrackBar", Some(row)) {
                let _ = ui.set_prop(t, "min", 0.0);
                let _ = ui.set_prop(t, "max", 200.0);
                let _ = ui.set_prop(t, "value", vol as f64);
                if let Some(cb) = ui.control_mut(t) {
                    cb.set_align(Align::Client);
                }
                ui.on(t, "on_change", move |ui, sender, _| {
                    let v = ui.get_prop(sender, "value").ok().and_then(|v| v.to_f64().ok()).unwrap_or(0.0);
                    set_track_volume(ui, i, v as i32);
                });
            }
        }
    }
    fit_side(ui);
}

fn view_clicked(ui: &mut Ui, idx: i32) {
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
    push_view(ui);
    // As caixas acompanham (tela cheia desmarca as regiões e vice-versa).
    ui.set_timeout(0, render_info);
}

fn set_track_volume(ui: &mut Ui, i: usize, v: i32) {
    with(|st| {
        if i < st.track_vols.len() {
            st.track_vols[i] = v.clamp(0, 200);
        }
    });
    push_volumes(ui);
}

/// Mutar zera guardando o volume; desmutar volta pra ele (ou 100).
fn track_mute(ui: &mut Ui, i: usize) {
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
    push_volumes(ui);
    ui.set_timeout(0, render_info);
}

// ----- transcrição e legenda -----

fn request_transcript() {
    let (need, id) = with(|st| (st.transcript.is_none() && !st.tr_requested, st.id.clone()));
    if need && !id.is_empty() {
        with(|st| st.tr_requested = true);
        bridge::handle_request_transcript(&id);
    }
}

pub fn toggle_transcript(ui: &mut Ui) {
    open_side(ui, "transcript");
    if with(|st| st.side != "transcript") {
        return;
    }
    set(ui, "lblSideTitle", "text", lang::t("player.transcript"));
    set(ui, "edTrSearch", "placeholder", lang::t("player.transcriptSearch"));
    render_turns(ui);
    request_transcript();
}

pub fn toggle_captions(ui: &mut Ui) {
    let on = with(|st| {
        st.captions_on = !st.captions_on;
        st.captions_on
    });
    config::set_config_bool("playerCaptions", on);
    set(ui, "btnCaptions", "checked", on);
    if !on {
        set(ui, "lblCaption", "visible", false);
        return;
    }
    if with(|st| st.transcript.is_none()) {
        with(|st| st.cap_notify = true);
        request_transcript();
    } else {
        notify_no_captions();
    }
    sync_caption(ui);
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
    let t = t.clone();
    with_ui(move |ui| {
        if with(|st| st.id != t.id) {
            return;
        }
        let notify = with(|st| {
            st.transcript = Some(t);
            st.tr_active = -1;
            st.tr_pinned = -1;
            st.cap_cache = None;
            std::mem::take(&mut st.cap_notify)
        });
        if with(|st| st.side == "transcript") {
            render_turns(ui);
        }
        if notify {
            notify_no_captions();
        }
        sync_caption(ui);
    });
}

/// "SPEAKER_00" vira "Falante 1"; o nome dado pelo usuário vence.
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
fn speaker_color(spk: &str) -> Color {
    let i = match speaker_number(spk) {
        Some(n) => (n % 6) as usize,
        None => {
            let mut h: i32 = 0;
            for c in spk.chars() {
                h = h.wrapping_mul(31).wrapping_add(c as i32);
            }
            (h.unsigned_abs() % 6) as usize
        }
    };
    let rgb = SPEAKER_COLORS[i];
    Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn render_turns(ui: &mut Ui) {
    let (t, q, active) = with(|st| (st.transcript.clone(), st.tr_query.clone(), st.tr_active));
    let Some(p) = side_panel(ui) else { return };
    with(|st| st.tr_rows.clear());
    let Some(t) = t else {
        add_label(ui, p, &lang::t("player.transcriptLoading"), "key", 24.0, 0.0);
        fit_side(ui);
        return;
    };
    if t.turns.is_empty() {
        let msg = if t.transcribed { lang::t("player.transcriptNoSpeech") } else { lang::t("player.transcriptNone") };
        if let Some(l) = add_label(ui, p, &msg, "key", 40.0, 0.0) {
            let _ = ui.set_prop(l, "wrap", true);
        }
        fit_side(ui);
        return;
    }
    let width = c(ui, "lySide").and_then(|s| ui.bounds(s)).map_or(344.0, |b| b.width - 36.0).max(120.0);
    let mut rows = Vec::new();
    for (i, tr) in t.turns.iter().enumerate().filter(|(_, tr)| q.is_empty() || tr.text.to_lowercase().contains(&q)) {
        let Ok(row) = ui.create("Panel", Some(p)) else { continue };
        let _ = ui.set_prop(row, "style", if i as i32 == active { "row_active" } else { "row" });
        let text_w = ui.measure_text(&tr.text, &TextStyle::default()).width * 12.0 / 14.0;
        let lines = (text_w / (width - 20.0)).ceil().max(1.0);
        let h = 22.0 + lines * 17.0 + 12.0;
        if let Some(cb) = ui.control_mut(row) {
            cb.set_align(Align::Top).set_height(h).set_padding([10.0, 6.0, 10.0, 6.0]).set_margins([0.0, 2.0, 0.0, 2.0]).set_cursor(CursorShape::Pointer);
        }
        if let Ok(head) = ui.create("Layout", Some(row)) {
            if let Some(cb) = ui.control_mut(head) {
                cb.set_align(Align::Top).set_height(20.0).set_hit_test(false);
            }
            if let Ok(time) = ui.create("Label", Some(head)) {
                let s = fmt_duration(tr.start.floor() as i64);
                let _ = ui.set_prop(time, "style", "key");
                let _ = ui.set_prop(time, "text", if s.is_empty() { "00:00".to_string() } else { s });
                let _ = ui.set_prop(time, "h_align", "trailing");
                if let Some(cb) = ui.control_mut(time) {
                    cb.set_align(Align::Right).set_width(70.0);
                }
            }
            if let Ok(spk) = ui.create("Label", Some(head)) {
                let _ = ui.set_prop(spk, "style", "row");
                let _ = ui.set_prop(spk, "text", speaker_label(&t, &tr.speaker));
                let _ = ui.set_prop(spk, "color", speaker_color(&tr.speaker));
                if let Some(cb) = ui.control_mut(spk) {
                    cb.set_align(Align::Client);
                }
            }
        }
        if let Ok(text) = ui.create("Label", Some(row)) {
            let _ = ui.set_prop(text, "style", "val");
            let _ = ui.set_prop(text, "text", tr.text.as_str());
            let _ = ui.set_prop(text, "wrap", true);
            let _ = ui.set_prop(text, "v_align", "leading");
            let _ = ui.set_prop(text, "highlight", q.as_str());
            if let Some(cb) = ui.control_mut(text) {
                cb.set_align(Align::Client).set_hit_test(false);
            }
        }
        let idx = i;
        ui.on(row, "on_click", move |ui, _, _| turn_clicked(ui, idx));
        rows.push((i, row));
    }
    if rows.is_empty() {
        add_label(ui, p, &lang::t("player.transcriptNoMatch"), "key", 24.0, 0.0);
    }
    with(|st| st.tr_rows = rows);
    fit_side(ui);
}

/// O turno em reprodução. Turnos SOBREPOSTOS são normais: o clicado vence
/// enquanto o tempo está nele; fora disso, o que COMEÇOU por último entre os
/// que contêm o instante.
fn sync_transcript_active(ui: &mut Ui, force: bool) {
    let changed = with(|st| {
        let t = st.transcript.as_ref()?;
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
        Some((idx, st.tr_rows.clone()))
    });
    let Some((idx, rows)) = changed else { return };
    for (i, row) in rows {
        let _ = ui.set_prop(row, "style", if i as i32 == idx { "row_active" } else { "row" });
    }
}

/// Parte o turno em blocos de até duas linhas pela largura da legenda e
/// divide o tempo pela proporção de caracteres.
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

fn sync_caption(ui: &mut Ui) {
    if !with(|st| st.captions_on) {
        return;
    }
    let width = with(|st| (st.stage.0 * 0.86).min(960.0));
    let text = with(|st| {
        let idx = st.tr_active;
        let t = st.transcript.as_ref()?;
        if idx < 0 {
            return None;
        }
        let valid = st.cap_cache.as_ref().is_some_and(|(i, wd, _)| *i == idx && (*wd - width).abs() < 1.0);
        if !valid {
            st.cap_cache = Some((idx, width, caption_chunks(t, idx as usize, width, 22.0)));
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
    let text = text.unwrap_or_default();
    set(ui, "lblCaption", "visible", !text.is_empty());
    set(ui, "lblCaption", "text", text);
}

fn turn_clicked(ui: &mut Ui, idx: usize) {
    let start = with(|st| st.transcript.as_ref().and_then(|t| t.turns.get(idx)).map(|t| t.start));
    let Some(start) = start else { return };
    with(|st| {
        st.tr_pinned = idx as i32;
        if let Some(pb) = &st.pb {
            pb.seek(start.max(0.0));
        }
    });
    sync_transcript_active(ui, true);
    sync_caption(ui);
}

pub fn tr_search(ui: &mut Ui, text: &str) {
    with(|st| st.tr_query = text.to_lowercase().trim().to_owned());
    render_turns(ui);
    sync_transcript_active(ui, true);
}

pub fn close_side(ui: &mut Ui) {
    let which = with(|st| st.side.clone());
    if !which.is_empty() {
        open_side(ui, &which);
    }
}

/// Renomeia o falante (todos os turnos dele mudam de uma vez).
#[allow(dead_code)]
pub fn rename_speaker(ui: &mut Ui, idx: usize, name: &str) {
    let nome: String = name.replace(['\r', '\n'], " ").trim().chars().take(60).collect();
    let change = with(|st| {
        let id = st.id.clone();
        let t = st.transcript.as_mut()?;
        let spk = t.turns.get(idx)?.speaker.clone();
        if nome == speaker_label(t, &spk) {
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
    render_turns(ui);
}

/// Liga os gestos do palco e o teclado do form (uma vez, com o form criado).
pub fn setup(ui: &mut Ui) {
    if let Some(stage) = c(ui, "frmVideo") {
        ui.on(stage, "on_wheel", |ui, _, args| {
            if let EventArgs::Wheel(e) = args {
                wheel(ui, e);
            }
        });
        ui.on(stage, "on_pointer_down", |ui, _, args| {
            if let EventArgs::Pointer(e) = args {
                stage_down(ui, e);
            }
        });
        ui.on(stage, "on_pointer_move", |ui, _, args| {
            if let EventArgs::Pointer(e) = args {
                stage_move(ui, e);
            }
        });
        ui.on(stage, "on_pointer_up", |ui, _, args| {
            if let EventArgs::Pointer(e) = args {
                stage_up(ui, e);
            }
        });
        ui.on(stage, "on_dbl_click", |ui, _, _| toggle_fullscreen(ui));
    }
    for bar in ["pnlPlayerBar", "pnlPlayerHead"] {
        if let Some(id) = c(ui, bar) {
            ui.on(id, "on_pointer_move", |ui, _, _| activity(ui));
        }
    }
    let Some(form) = main_form() else { return };
    ui.on(form, "on_key_down", |ui, _, args| {
        let EventArgs::Key(k) = args else { return };
        if is_open() {
            key(ui, k);
            return;
        }
        if crate::app_export::is_open() {
            let typing = ui.focused(form_of(ui)).is_some_and(|f| ui.get::<Edit>(f).is_some());
            crate::app_export::key(ui, k, typing);
            return;
        }
        if crate::app_settings::is_open() {
            if k.key == Key::Escape {
                crate::app_settings::close(ui);
            }
            return;
        }
        // Na biblioteca, menos com o cursor num campo (o Backspace é do texto).
        let typing = ui.focused(form_of(ui)).is_some_and(|f| ui.get::<Edit>(f).is_some());
        if !typing {
            crate::app_library::key(ui, k);
        }
    });
}

fn form_of(_ui: &Ui) -> ComponentId {
    main_form().unwrap_or(ComponentId::from_u64(0))
}

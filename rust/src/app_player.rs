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
use crate::{bridge, config, lang, log};

/// Os controles somem depois disto sem mexer o mouse, tocando.
const IDLE_MS: u64 = 2000;
/// Zoom máximo da roda.
const ZOOM_MAX: f64 = 8.0;
const WAVE_BUCKETS: i64 = 2000;
/// As velocidades do menu (de 8× pra cima anda por saltos de keyframe).
const SPEEDS: [f64; 9] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 8.0, 16.0, 32.0];
/// O pulso do indicador central na pausa (entra rápido, segura, sai).
const FLASH_MS: f64 = 1200.0;
/// Os ícones cheios do player (grade 24).
const ICONS: [(&str, &str); 10] = [
    ("pl-play", "M8 5v14l11-7z"),
    ("pl-pause", "M6 5h4v14H6zM14 5h4v14h-4z"),
    ("pl-vol-high", "M3 9v6h4l5 5V4L7 9H3zm13.5 3a4.5 4.5 0 0 0-2.5-4.03v8.05A4.5 4.5 0 0 0 16.5 12zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77S18.01 4.14 14 3.23z"),
    ("pl-vol-low", "M7 9v6h4l5 5V4l-5 5H7zm9.5 3a4.5 4.5 0 0 0-2.5-4.03v8.05A4.5 4.5 0 0 0 16.5 12z"),
    ("pl-vol-off", "M16.5 12A4.5 4.5 0 0 0 14 7.97v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51A8.92 8.92 0 0 0 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06a8.99 8.99 0 0 0 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"),
    ("pl-cc", "M19 4H5a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm-8 7H9.5v-.5h-2v3h2V13H11v1c0 .55-.45 1-1 1H7c-.55 0-1-.45-1-1v-4c0-.55.45-1 1-1h3c.55 0 1 .45 1 1v1zm7 0h-1.5v-.5h-2v3h2V13H18v1c0 .55-.45 1-1 1h-3c-.55 0-1-.45-1-1v-4c0-.55.45-1 1-1h3c.55 0 1 .45 1 1v1z"),
    ("pl-info", "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm1 15h-2v-6h2zm0-8h-2V7h2z"),
    ("pl-fs-enter", "M7 14H5v5h5v-2H7v-3zm-2-4h2V7h3V5H5v5zm12 7h-3v2h5v-5h-2v3zM14 5v2h3v3h2V5h-5z"),
    ("pl-fs-exit", "M5 16h3v3h2v-5H5v2zm3-8H5v2h5V5H8v3zm6 11h2v-3h3v-2h-5v5zm2-11V5h-2v5h5V8h-3z"),
    ("pl-clock", "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm.5-13H11v6l5.2 3.1.8-1.3-4.5-2.6z"),
];
/// Os ícones de traço do painel de informações (grade 24).
const LINE_ICONS: [(&str, &str); 5] = [
    ("pl-expand", "M15 3h6v6 M9 21H3v-6 M21 3l-7 7 M3 21l7-7"),
    ("pl-monitor", "M4 4h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M8 22h8 M12 18v4"),
    ("pl-webcam", "M15.5 12A3.5 3.5 0 1 1 8.5 12A3.5 3.5 0 1 1 15.5 12Z M21 12A9 9 0 1 1 3 12A9 9 0 1 1 21 12Z"),
    ("pl-tvol", "M11 5L6 9H2v6h4l5 4V5z M15.54 8.46a5 5 0 0 1 0 7.07 M19.07 4.93a10 10 0 0 1 0 14.14"),
    ("pl-tvol-off", "M11 5L6 9H2v6h4l5 4V5z M23 9l-6 6 M17 9l6 6"),
];
/// As cores dos falantes ficam na paleta do projeto (`speaker_1`…`speaker_6`, uma por modo).
const SPEAKER_COLORS: usize = 6;

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
    /// O turno com o nome do falante em edição (-1 = nenhum).
    tr_editing: i32,
    captions_on: bool,
    cap_cache: Option<(i32, f32, Vec<(String, f64, f64)>)>,
    /// Texto, fonte, largura do palco e folga de baixo da legenda na tela (refaz só se mudar).
    cap_layout: Option<(String, f32, f32, f32)>,
    cap_notify: bool,
    /// O menu de velocidade aberto (em cima do botão).
    speed_open: bool,
    /// Quando começou o pulso de pausa (e o timer que o anima).
    flash: Option<(Instant, u64)>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State {
        zoom: 1.0,
        master: 100,
        speed: 1.0,
        scale: 1.0,
        tr_active: -1,
        tr_pinned: -1,
        tr_editing: -1,
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
    let (zoom, stage_w) = with(|st| (st.zoom, st.stage.0));
    set(ui, "lblPlayerZoom", "visible", zoom > 1.001);
    if zoom > 1.001 {
        let text = format!("{}%", (zoom * 100.0).round());
        let w = text.chars().count() as f32 * 7.0 + 16.0;
        set(ui, "lblPlayerZoom", "text", text);
        if let Some(cb) = c(ui, "lblPlayerZoom").and_then(|id| ui.control_mut(id)) {
            cb.set_width(w);
            cb.set_position(stage_w - 12.0 - w, 56.0);
        }
    }
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
    let v = if muted { 0 } else { m };
    set(ui, "trkVolume", "value", v as f64);
    set(ui, "lblVolPct", "text", format!("{v}%"));
    set(ui, "lblVolPct", "text_color", if v > 100 { "danger_2" } else { "" });
    set(ui, "btnMute", "icon", if v == 0 { "pl-vol-off" } else if v < 50 { "pl-vol-low" } else { "pl-vol-high" });
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
    set(ui, "pnlPlayerLoading", "visible", true);
    set(ui, "lblPlayerTime", "text", "00:00 / 00:00");
    set(ui, "lblPlayerClock", "text", "");
    set(ui, "lyPlayerClock", "visible", start_clock >= 0);
    set(ui, "pnlPlayerPaused", "visible", false);
    set(ui, "btnPlay", "icon", "pl-pause");
    set(ui, "wavPlayer", "paused", false);
    set(ui, "lblCaption", "visible", false);
    set(ui, "lblPlayerZoom", "visible", false);
    set(ui, "pnlPauseFlash", "visible", false);
    close_popups(ui);
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
        st.cap_layout = None;
        st.captions_on = captions;
        view_spec(st)
    });
    set_speed(ui, 1.0);
    sync_cc(ui);
    if captions {
        request_transcript();
    }
    let id_out = id.to_owned();
    let out: Arc<dyn Fn(Out) + Send + Sync> = crate::app_view::frame_sink(id_out, on_out);
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
            set(ui, "pnlPlayerLoading", "visible", false);
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
            set(ui, "pnlPlayerLoading", "visible", false);
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
            set(ui, "pnlPlayerLoading", "visible", true);
            log!("Player: erro {e}");
        }
    }
}

fn set_playing_ui(ui: &mut Ui, playing: bool) {
    let was = with(|st| {
        let was = st.playing;
        st.playing = playing;
        st.last_activity = Some(Instant::now());
        was
    });
    set(ui, "btnPlay", "icon", if playing { "pl-pause" } else { "pl-play" });
    set(ui, "pnlPlayerPaused", "visible", !playing);
    set(ui, "wavPlayer", "paused", !playing);
    if was && !playing {
        start_flash(ui);
    }
    set_controls_visible(ui, true);
}

/// O pulso do indicador central na pausa: sobe em 15 %, segura até 58 %, some.
fn start_flash(ui: &mut Ui) {
    if let Some((_, job)) = with(|st| st.flash.take()) {
        ui.clear_timeout(job);
    }
    set(ui, "pnlPauseFlash", "visible", true);
    let job = ui.set_timeout(0, flash_step);
    with(|st| st.flash = Some((Instant::now(), job)));
}

fn flash_step(ui: &mut Ui) {
    let Some((at, _)) = with(|st| st.flash) else { return };
    let k = (at.elapsed().as_secs_f64() * 1000.0 / FLASH_MS).min(1.0);
    let op = if k < 0.15 { k / 0.15 * 0.95 } else if k < 0.58 { 0.95 } else { (1.0 - k) / 0.42 * 0.95 };
    set(ui, "pnlPauseFlash", "opacity", op);
    if k >= 1.0 {
        set(ui, "pnlPauseFlash", "visible", false);
        with(|st| st.flash = None);
        return;
    }
    let job = ui.set_timeout(16, flash_step);
    with(|st| {
        if let Some(f) = st.flash.as_mut() {
            f.1 = job;
        }
    });
}

fn set_controls_visible(ui: &mut Ui, on: bool) {
    let was_idle = with(|st| std::mem::replace(&mut st.idle, !on));
    if was_idle == !on {
        return;
    }
    set(ui, "lyChrome", "visible", on);
    if !on {
        close_popups(ui);
    }
    // A legenda desce quando os controles somem.
    with(|st| st.cap_layout = None);
    sync_caption(ui);
    ui.layout();
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
        (pos, dur, st.start_clock, st.playing, due, !st.side.is_empty() || st.speed_open)
    });
    let hold = hold || hover_in(ui, &["pnlPlayerBar", "pnlPlayerHead", "lyVolPop", "lyCcPop"]);
    sync_popups(ui);
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
    let bars = ((width * window_scale() / 2.0) as usize).clamp(1, peaks.len());
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
    // De 8× pra cima (saltos entre quadros-chave): âmbar com um ponto.
    let trick = r >= TRICK_MIN_RATE;
    let label = if trick { format!("{} •", speed_label(r)) } else { speed_label(r) };
    let width = (label.chars().count() as f32 * 7.5 + 16.0).max(42.0);
    if let Some(cb) = c(ui, "btnSpeed").and_then(|id| ui.control_mut(id)) {
        cb.set_width(width);
    }
    set(ui, "btnSpeed", "text", label);
    set(ui, "btnSpeed", "style", if trick { "pl_trick" } else { "pl_speed" });
    let hint = if trick { lang::t("player.trickHint") } else { lang::t("player.speed") };
    set(ui, "btnSpeed", "hint", hint);
    close_speed(ui);
}

/// O menu de velocidades, em cima do botão (abre e fecha no clique).
pub fn speed_menu(ui: &mut Ui, _sender: ComponentId) {
    if with(|st| st.speed_open) {
        close_speed(ui);
        return;
    }
    let Some(list) = c(ui, "lySpeedItems") else { return };
    if ui.children(list).is_empty() {
        for s in SPEEDS {
            let Ok(item) = ui.create("Button", Some(list)) else { continue };
            let _ = ui.set_prop(item, "text", speed_label(s));
            let _ = ui.set_prop(item, "tab_stop", false);
            if let Some(cb) = ui.control_mut(item) {
                cb.set_align(Align::Top).set_height(29.5).set_margins([0.0, 0.0, 0.0, 2.0]).set_cursor(CursorShape::Pointer);
            }
            ui.on(item, "on_click", move |ui, _, _| set_speed(ui, s));
        }
    }
    let current = with(|st| st.speed);
    for (item, s) in ui.children(list).to_vec().into_iter().zip(SPEEDS) {
        let _ = ui.set_prop(item, "style", if (s - current).abs() < 1e-9 { "pl_item_on" } else { "pl_item" });
    }
    with(|st| st.speed_open = true);
    set(ui, "btnSpeed", "checked", true);
    place_popup(ui, "pnlSpeedPop", "btnSpeed", 8.0);
}

fn close_speed(ui: &mut Ui) {
    with(|st| st.speed_open = false);
    set(ui, "pnlSpeedPop", "visible", false);
    set(ui, "btnSpeed", "checked", false);
}

/// Põe o popup centrado em cima do botão, `gap` acima dele (coordenadas do palco).
fn place_popup(ui: &mut Ui, popup: &str, button: &str, gap: f32) {
    let (Some(p), Some(b), Some(stage)) = (c(ui, popup), c(ui, button), c(ui, "lyStage")) else { return };
    let (Some(br), Some(sr)) = (ui.absolute_rect(b), ui.absolute_rect(stage)) else { return };
    let Some(cb) = ui.control_mut(p) else { return };
    let (w, h) = (cb.width(), cb.height());
    let x = (br.x - sr.x + (br.width - w) / 2.0).min(sr.width - w - 4.0);
    cb.set_position(x, br.y - sr.y - gap - h);
    cb.set_visible(true);
    ui.layout();
}

fn close_popups(ui: &mut Ui) {
    close_speed(ui);
    set(ui, "lyVolPop", "visible", false);
    set(ui, "lyCcPop", "visible", false);
}

/// O ponteiro está sobre algum destes (ou dentro deles)?
fn hover_in(ui: &Ui, names: &[&str]) -> bool {
    let Some(form) = main_form() else { return false };
    let targets: Vec<ComponentId> = names.iter().filter_map(|n| c(ui, n)).collect();
    let mut cur = ui.hovered(form);
    while let Some(id) = cur {
        if targets.contains(&id) {
            return true;
        }
        cur = ui.parent(id);
    }
    false
}

/// Volume e legendas abrem no hover do botão e ficam abertos com o mouse no popup
/// (o popup cobre o vão até o botão).
fn sync_popups(ui: &mut Ui) {
    for (button, popup) in [("btnMute", "lyVolPop"), ("btnCaptions", "lyCcPop")] {
        let open = c(ui, popup).and_then(|p| ui.control(p)).is_some_and(|cb| cb.visible());
        let hover = hover_in(ui, &[button, popup]);
        if hover && !open && !with(|st| st.idle) {
            if popup == "lyCcPop" {
                sync_cc(ui);
            }
            place_popup(ui, popup, button, 0.0);
        } else if !hover && open {
            set(ui, popup, "visible", false);
        }
    }
}

/// O botão CC aceso (legenda ou painel de transcrição) e as marcas do menu.
fn sync_cc(ui: &mut Ui) {
    let (cap, tr) = with(|st| (st.captions_on, st.side == "transcript"));
    set(ui, "btnCaptions", "checked", cap || tr);
    for (on, check, label, key) in [(cap, "lblCcCheck1", "lblCcCaptions", Some("lblCcKey1")), (tr, "lblCcCheck2", "lblCcTranscript", None)] {
        set(ui, check, "text", if on { "✓" } else { "" });
        let style = if on { "pl_opt_on" } else { "pl_opt" };
        set(ui, check, "style", style);
        set(ui, label, "style", style);
        if let Some(key) = key {
            set(ui, key, "style", if on { "pl_key_on" } else { "pl_key" });
        }
    }
    set(ui, "lblCcCaptions", "text", lang::t("player.captionsOverlay"));
    set(ui, "lblCcTranscript", "text", lang::t("player.transcriptPanel"));
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
    set(ui, "btnFullscreen", "icon", if on { "pl-fs-exit" } else { "pl-fs-enter" });
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
    if with(|st| st.speed_open) {
        close_speed(ui);
    }
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

/// Uma linha "chave ... valor" (o valor à direita): 4 + 16,8 + 4.
fn add_row(ui: &mut Ui, parent: ComponentId, key: &str, val: &str) {
    let Ok(row) = ui.create("Layout", Some(parent)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(24.8);
    }
    if let Ok(k) = ui.create("Label", Some(row)) {
        let _ = ui.set_prop(k, "style", "pl_ikey");
        let _ = ui.set_prop(k, "text", key);
        if let Some(cb) = ui.control_mut(k) {
            cb.set_align(Align::Left).set_width(110.0);
        }
    }
    if let Ok(v) = ui.create("Label", Some(row)) {
        let _ = ui.set_prop(v, "style", "pl_ival");
        let _ = ui.set_prop(v, "text", val);
        let _ = ui.set_prop(v, "h_align", "trailing");
        let _ = ui.set_prop(v, "trimming", "character");
        if let Some(cb) = ui.control_mut(v) {
            cb.set_align(Align::Client).set_margins([10.0, 0.0, 0.0, 0.0]);
        }
    }
}

/// Título de seção: maiúsculo, linha fina 4 px abaixo e 8 de folga.
fn add_section(ui: &mut Ui, parent: ComponentId, title: &str, top: f32) {
    if let Some(l) = add_label(ui, parent, title, "pl_sect", 14.0, top) {
        let _ = ui.set_prop(l, "text_case", "upper");
        let _ = ui.set_prop(l, "letter_spacing", 0.6);
    }
    if let Ok(line) = ui.create("Rectangle", Some(parent)) {
        let _ = ui.set_prop(line, "style", "pl_line");
        if let Some(cb) = ui.control_mut(line) {
            cb.set_align(Align::Top).set_height(1.0).set_margins([0.0, 4.0, 0.0, 8.0]).set_hit_test(false);
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
    if let Some(cb) = c(ui, "pnlPlayerSide").and_then(|id| ui.control_mut(id)) {
        cb.set_width(if which == "info" { 320.0 } else { 360.0 });
    }
    set(ui, "btnInfo", "checked", opening && which == "info");
    sync_cc(ui);
    set(ui, "lyTrSearch", "visible", which == "transcript");
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
    add_section(ui, p, &lang::t("player.info.file"), 0.0);
    add_row(ui, p, &lang::t("player.info.name"), &info.file_name);
    add_row(ui, p, &lang::t("player.info.container"), &info.format);
    add_row(ui, p, &lang::t("player.info.duration"), &fmt_duration(info.duration.round() as i64));
    add_row(ui, p, &lang::t("player.info.size"), &fmt_bytes(info.size));
    if info.bitrate > 0 {
        add_row(ui, p, &lang::t("player.info.totalBitrate"), &fmt_bitrate(info.bitrate));
    }
    let videos: Vec<_> = info.streams.iter().filter(|s| s.kind == "video").collect();
    if !videos.is_empty() {
        add_section(ui, p, &lang::t("player.info.video"), 18.0);
        for (i, s) in videos.iter().enumerate() {
            if videos.len() > 1 {
                add_row(ui, p, &lang::t_with("player.info.stream", &[("n", &(i + 1).to_string())]), "");
            }
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
    // Visualização: tela cheia (rádio) + uma caixa por região.
    let mut gap = 18.0;
    if let Some(lay) = info.layout.as_ref().filter(|l| !l.regions.is_empty()) {
        add_section(ui, p, &lang::t("player.info.visualization"), gap);
        let regions = with(|st| st.regions.clone());
        let mut opts = vec![(-1i32, lang::t("player.info.fullView"), "full".to_owned(), regions.is_empty())];
        for (i, r) in lay.regions.iter().enumerate() {
            let label = if r.name.is_empty() { lang::t_with("player.info.region", &[("n", &(i + 1).to_string())]) } else { r.name.clone() };
            let kind = if r.kind.is_empty() { "monitor".to_owned() } else { r.kind.clone() };
            opts.push((i as i32, label, kind, regions.contains(&i)));
        }
        for (idx, label, kind, on) in opts {
            view_option(ui, p, idx, &label, &kind, on);
        }
        gap = 14.0;
    }
    // Áudio: um cartão por faixa, com o volume dela.
    let audios: Vec<_> = info.streams.iter().filter(|s| s.kind == "audio").collect();
    if !audios.is_empty() {
        let key = if audios.len() == 1 { "player.info.audio_one" } else { "player.info.audio_other" };
        add_section(ui, p, &lang::t_with(key, &[("count", &audios.len().to_string())]), gap);
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
            let mut rows = vec![(lang::t("player.info.codec"), s.codec.to_uppercase())];
            if let Some(ch) = s.channels.filter(|c| *c > 0) {
                let v = match ch {
                    1 => lang::t("player.info.channelsMono"),
                    2 => lang::t("player.info.channelsStereo"),
                    n => lang::t_with("player.info.channelsN", &[("count", &n.to_string())]),
                };
                rows.push((lang::t("player.info.channels"), v));
            }
            if let Some(sr) = s.sample_rate.filter(|r| *r > 0) {
                rows.push((lang::t("player.info.sampleRate"), format!("{:.1} kHz", sr as f64 / 1000.0)));
            }
            if s.bitrate > 0 {
                rows.push((lang::t("player.info.bitrate"), fmt_bitrate(s.bitrate)));
            }
            track_card(ui, p, i, &title, &rows, vols.get(i).copied().unwrap_or(0));
        }
    }
    fit_side(ui);
}

/// Uma opção da "Visualização": rádio (tela cheia) ou caixa (região), ícone e nome.
fn view_option(ui: &mut Ui, parent: ComponentId, idx: i32, label: &str, kind: &str, on: bool) {
    let Ok(opt) = ui.create("Panel", Some(parent)) else { return };
    let _ = ui.set_prop(opt, "style", if on { "pl_view_on" } else { "pl_view" });
    if let Some(cb) = ui.control_mut(opt) {
        cb.set_align(Align::Top).set_height(34.8).set_margins([0.0, 0.0, 0.0, 4.0]).set_padding([11.0, 0.0, 10.0, 0.0]).set_cursor(CursorShape::Pointer);
    }
    let full = kind == "full";
    if let Ok(mark) = ui.create("Panel", Some(opt)) {
        let style = match (full, on) {
            (true, true) => "pl_radio_on",
            (true, false) => "pl_radio",
            (false, true) => "pl_box_on",
            (false, false) => "pl_box",
        };
        let _ = ui.set_prop(mark, "style", style);
        if let Some(cb) = ui.control_mut(mark) {
            cb.set_align(Align::Left).set_width(16.0).set_margins([0.0, 9.4, 10.0, 9.4]).set_hit_test(false);
        }
        if on {
            if full {
                if let Ok(dot) = ui.create("Circle", Some(mark)) {
                    let _ = ui.set_prop(dot, "fill", Color::WHITE);
                    let _ = ui.set_prop(dot, "stroke_width", 0.0);
                    if let Some(cb) = ui.control_mut(dot) {
                        cb.set_align(Align::Center).set_width(7.0).set_height(7.0).set_hit_test(false);
                    }
                }
            } else if let Ok(chk) = ui.create("Button", Some(mark)) {
                let _ = ui.set_prop(chk, "style", "pl_check");
                let _ = ui.set_prop(chk, "text", "");
                let _ = ui.set_prop(chk, "icon", "check");
                let _ = ui.set_prop(chk, "icon_size", 11.0);
                if let Some(cb) = ui.control_mut(chk) {
                    cb.set_align(Align::Client).set_hit_test(false);
                }
            }
        }
    }
    if let Ok(icon) = ui.create("Button", Some(opt)) {
        let _ = ui.set_prop(icon, "style", if on { "pl_vicon_on" } else { "pl_vicon" });
        let _ = ui.set_prop(icon, "text", "");
        let name = match kind {
            "webcam" => "pl-webcam",
            "monitor" => "pl-monitor",
            _ => "pl-expand",
        };
        let _ = ui.set_prop(icon, "icon", name);
        let _ = ui.set_prop(icon, "icon_size", 18.0);
        if let Some(cb) = ui.control_mut(icon) {
            cb.set_align(Align::Left).set_width(18.0).set_margins([0.0, 0.0, 10.0, 0.0]).set_hit_test(false);
        }
    }
    if let Ok(text) = ui.create("Label", Some(opt)) {
        let _ = ui.set_prop(text, "style", "pl_vlabel");
        let _ = ui.set_prop(text, "text", label);
        let _ = ui.set_prop(text, "trimming", "character");
        if let Some(cb) = ui.control_mut(text) {
            cb.set_align(Align::Client).set_hit_test(false);
        }
    }
    ui.on(opt, "on_click", move |ui, _, _| view_clicked(ui, idx));
}

/// O cartão de uma faixa de áudio: título, linhas e o volume (0–200 %, mudo).
fn track_card(ui: &mut Ui, parent: ComponentId, i: usize, title: &str, rows: &[(String, String)], vol: i32) {
    let Ok(card) = ui.create("Panel", Some(parent)) else { return };
    let _ = ui.set_prop(card, "style", "pl_card");
    let muted = vol == 0;
    if let Some(cb) = ui.control_mut(card) {
        let h = 8.0 + 19.4 + rows.len() as f32 * 24.8 + 6.0 + 1.0 + 6.0 + 22.0 + 8.0;
        cb.set_align(Align::Top).set_height(h).set_margins([0.0, 0.0, 0.0, 6.0]).set_padding([10.0, 8.0, 10.0, 8.0]);
    }
    if muted {
        let _ = ui.set_prop(card, "opacity", 0.55);
    }
    if let Some(l) = add_label(ui, card, title, "pl_track", 19.4, 0.0) {
        let _ = ui.set_prop(l, "font_strikeout", muted);
        let _ = ui.set_prop(l, "v_align", "leading");
    }
    for (k, v) in rows {
        add_row(ui, card, k, v);
    }
    if let Ok(line) = ui.create("Rectangle", Some(card)) {
        let _ = ui.set_prop(line, "style", "pl_line2");
        if let Some(cb) = ui.control_mut(line) {
            cb.set_align(Align::Top).set_height(1.0).set_margins([0.0, 6.0, 0.0, 6.0]).set_hit_test(false);
        }
    }
    let Ok(row) = ui.create("Layout", Some(card)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(22.0);
    }
    if let Ok(m) = ui.create("Button", Some(row)) {
        let _ = ui.set_prop(m, "style", if muted { "pl_tmute_off" } else { "pl_tmute" });
        let _ = ui.set_prop(m, "text", "");
        let _ = ui.set_prop(m, "icon", if muted { "pl-tvol-off" } else { "pl-tvol" });
        let _ = ui.set_prop(m, "icon_size", 14.0);
        let _ = ui.set_prop(m, "hint", lang::t(if muted { "player.unmute" } else { "player.muteShort" }));
        if let Some(cb) = ui.control_mut(m) {
            cb.set_align(Align::Left).set_width(22.0).set_margins([0.0, 0.0, 8.0, 0.0]);
        }
        ui.on(m, "on_click", move |ui, _, _| track_mute(ui, i));
    }
    let pct = ui.create("Label", Some(row)).ok();
    if let Some(l) = pct {
        let _ = ui.set_prop(l, "style", "pl_pct");
        let _ = ui.set_prop(l, "text", format!("{vol}%"));
        let _ = ui.set_prop(l, "h_align", "trailing");
        if let Some(cb) = ui.control_mut(l) {
            cb.set_align(Align::Right).set_width(38.0).set_margins([8.0, 0.0, 0.0, 0.0]);
        }
    }
    if let Ok(t) = ui.create("TrackBar", Some(row)) {
        let _ = ui.set_prop(t, "style", "pl_track");
        let _ = ui.set_prop(t, "min", 0.0);
        let _ = ui.set_prop(t, "max", 200.0);
        let _ = ui.set_prop(t, "value", vol as f64);
        let _ = ui.set_prop(t, "thumb_size", 10.0);
        let _ = ui.set_prop(t, "track_size", 3.0);
        let _ = ui.set_prop(t, "solid_thumb", true);
        let _ = ui.set_prop(t, "limit", 100.0);
        let _ = ui.set_prop(t, "danger_color", "danger_2");
        let _ = ui.set_prop(t, "tab_stop", false);
        if let Some(cb) = ui.control_mut(t) {
            cb.set_align(Align::Client);
        }
        ui.on(t, "on_change", move |ui, sender, _| {
            let v = ui.get_prop(sender, "value").ok().and_then(|v| v.to_f64().ok()).unwrap_or(0.0) as i32;
            if let Some(l) = pct {
                let _ = ui.set_prop(l, "text", format!("{v}%"));
            }
            set_track_volume(ui, i, v);
        });
        ui.on(t, "on_dbl_click", move |ui, _, _| {
            set_track_volume(ui, i, 100);
            ui.set_timeout(0, render_info);
        });
    }
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
    with(|st| st.cap_layout = None);
    let on = with(|st| {
        st.captions_on = !st.captions_on;
        st.captions_on
    });
    config::set_config_bool("playerCaptions", on);
    sync_cc(ui);
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
fn speaker_color(spk: &str) -> String {
    let i = match speaker_number(spk) {
        Some(n) => n as usize % SPEAKER_COLORS,
        None => {
            let mut h: i32 = 0;
            for c in spk.chars() {
                h = h.wrapping_mul(31).wrapping_add(c as i32);
            }
            h.unsigned_abs() as usize % SPEAKER_COLORS
        }
    };
    format!("speaker_{}", i + 1)
}

fn render_turns(ui: &mut Ui) {
    let (t, q, active, editing) = with(|st| (st.transcript.clone(), st.tr_query.clone(), st.tr_active, st.tr_editing));
    let Some(p) = side_panel(ui) else { return };
    // A lista encosta nas bordas (o turno tem o próprio respiro).
    if let Some(cb) = ui.control_mut(p) {
        cb.set_padding([0.0, 6.0, 8.0, 6.0]);
    }
    with(|st| st.tr_rows.clear());
    let empty = |ui: &mut Ui, msg: &str| {
        if let Some(l) = add_label(ui, p, msg, "pl_empty", 60.0, 18.0) {
            let _ = ui.set_prop(l, "wrap", true);
            let _ = ui.set_prop(l, "h_align", "center");
            let _ = ui.set_prop(l, "v_align", "leading");
            if let Some(cb) = ui.control_mut(l) {
                cb.set_margins([14.0, 18.0, 14.0, 0.0]);
            }
        }
    };
    let Some(t) = t else {
        empty(ui, &lang::t("player.transcriptLoading"));
        fit_side(ui);
        return;
    };
    if t.turns.is_empty() {
        let msg = if t.transcribed { lang::t("player.transcriptNoSpeech") } else { lang::t("player.transcriptNone") };
        empty(ui, &msg);
        fit_side(ui);
        return;
    }
    let width = c(ui, "pnlPlayerSide").and_then(|s| ui.control(s)).map_or(360.0, |cb| cb.width()) - 8.0;
    let mut body = TextStyle::default();
    body.font.family = "Segoe UI".into();
    body.font.size = 12.0;
    body.wrap = true;
    let line_h = (12.0 * 1.45f32).round();
    let mut rows = Vec::new();
    for (i, tr) in t.turns.iter().enumerate().filter(|(_, tr)| q.is_empty() || tr.text.to_lowercase().contains(&q)) {
        let Ok(row) = ui.create("Panel", Some(p)) else { continue };
        let on = i as i32 == active;
        let _ = ui.set_prop(row, "style", if on { "pl_turn_on" } else { "pl_turn" });
        let natural = ui.measure_text("Ág", &body).height.max(1.0);
        let lines = (ui.measure_text_wrapped(&tr.text, &body, width - 30.0).height / natural).round().max(1.0);
        let h = 8.0 + 15.4 + 3.0 + lines * line_h + 8.0;
        if let Some(cb) = ui.control_mut(row) {
            cb.set_align(Align::Top).set_height(h).set_padding([16.0, 8.0, 14.0, 8.0]).set_cursor(CursorShape::Pointer);
        }
        // A marca do turno ativo, na borda esquerda.
        if let Ok(mark) = ui.create("Rectangle", Some(row)) {
            let _ = ui.set_prop(mark, "style", "pl_turn_mark");
            let _ = ui.set_prop(mark, "visible", on);
            if let Some(cb) = ui.control_mut(mark) {
                cb.set_align(Align::None).set_position(0.0, 0.0).set_width(2.0).set_height(h).set_hit_test(false);
            }
        }
        if let Ok(head) = ui.create("Layout", Some(row)) {
            if let Some(cb) = ui.control_mut(head) {
                cb.set_align(Align::Top).set_height(15.4).set_margins([0.0, 0.0, 0.0, 3.0]);
            }
            if let Ok(time) = ui.create("Label", Some(head)) {
                let s = fmt_duration(tr.start.floor() as i64);
                let _ = ui.set_prop(time, "style", "pl_ttime");
                let _ = ui.set_prop(time, "text", if s.is_empty() { "00:00".to_string() } else { s });
                let _ = ui.set_prop(time, "h_align", "trailing");
                if let Some(cb) = ui.control_mut(time) {
                    cb.set_align(Align::Right).set_width(60.0).set_margins([8.0, 0.0, 0.0, 0.0]).set_hit_test(false);
                }
            }
            let name = speaker_label(&t, &tr.speaker);
            if editing == i as i32 {
                speaker_editor(ui, head, i, &name);
            } else if let Ok(spk) = ui.create("Label", Some(head)) {
                let mut st = body.clone();
                st.font.size = 11.0;
                st.font.weight = FontWeight::SemiBold;
                let w = ui.measure_text(&name, &st).width + 0.2 * name.chars().count() as f32 + 7.0;
                let _ = ui.set_prop(spk, "style", "pl_spk");
                let _ = ui.set_prop(spk, "text", name.as_str());
                let _ = ui.set_prop(spk, "text_color", speaker_color(&tr.speaker));
                let _ = ui.set_prop(spk, "letter_spacing", 0.2);
                let _ = ui.set_prop(spk, "hint", lang::t("player.renameSpeaker"));
                if let Some(cb) = ui.control_mut(spk) {
                    cb.set_align(Align::Left).set_width(w).set_margins([-3.0, 0.0, 0.0, 0.0]).set_cursor(CursorShape::Text).set_hit_test(true);
                }
                ui.on(spk, "on_click", move |ui, _, _| {
                    with(|st| st.tr_editing = i as i32);
                    render_turns(ui);
                });
            }
        }
        if let Ok(text) = ui.create("Label", Some(row)) {
            let _ = ui.set_prop(text, "style", "pl_ttext");
            let _ = ui.set_prop(text, "text", tr.text.as_str());
            let _ = ui.set_prop(text, "wrap", true);
            let _ = ui.set_prop(text, "v_align", "center");
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
        empty(ui, &lang::t("player.transcriptNoMatch"));
    }
    with(|st| st.tr_rows = rows);
    fit_side(ui);
}

/// O campo que troca o nome do falante, no lugar do nome (Enter grava, Esc desiste,
/// sair do campo grava).
fn speaker_editor(ui: &mut Ui, head: ComponentId, idx: usize, name: &str) {
    let Ok(ed) = ui.create("Edit", Some(head)) else { return };
    let _ = ui.set_prop(ed, "style", "pl_spk_edit");
    let _ = ui.set_prop(ed, "text", name);
    if let Some(cb) = ui.control_mut(ed) {
        cb.set_align(Align::Left).set_width(200.0).set_margins([-3.0, -2.0, 0.0, -2.0]);
    }
    ui.on(ed, "on_key_down", move |ui, sender, args| {
        let EventArgs::Key(k) = args else { return };
        match k.key {
            Key::Enter => {
                let text = match ui.get_prop(sender, "text") { Ok(PropValue::String(t)) => t, _ => String::new() };
                finish_rename(ui, idx, Some(text));
            }
            Key::Escape => finish_rename(ui, idx, None),
            _ => {}
        }
    });
    ui.on(ed, "on_exit", move |ui, sender, _| {
        if with(|st| st.tr_editing == idx as i32) {
            let text = match ui.get_prop(sender, "text") { Ok(PropValue::String(t)) => t, _ => String::new() };
            finish_rename(ui, idx, Some(text));
        }
    });
    ui.set_focus(ed);
    if let Some(e) = ui.get_mut::<Edit>(ed) {
        e.select_all();
    }
}

fn finish_rename(ui: &mut Ui, idx: usize, name: Option<String>) {
    with(|st| st.tr_editing = -1);
    match name {
        Some(n) => rename_speaker(ui, idx, &n),
        None => render_turns(ui),
    }
    // O foco volta ao palco (as teclas do player) depois desta tecla.
    ui.set_timeout(0, |ui| {
        if let Some(stage) = c(ui, "frmVideo") {
            ui.set_focus(stage);
        }
    });
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
        let on = i as i32 == idx;
        let _ = ui.set_prop(row, "style", if on { "pl_turn_on" } else { "pl_turn" });
        if let Some(&mark) = ui.children(row).first() {
            let _ = ui.set_prop(mark, "visible", on);
        }
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
    // Como no original: 86 % do palco (até 960), fonte de 16 a 30 px pela largura.
    let (width, fs) = with(|st| ((st.stage.0 * 0.86).min(960.0), (st.stage.0 * 0.022).clamp(16.0, 30.0)));
    let text = with(|st| {
        let idx = st.tr_active;
        let t = st.transcript.as_ref()?;
        if idx < 0 {
            return None;
        }
        let valid = st.cap_cache.as_ref().is_some_and(|(i, wd, _)| *i == idx && (*wd - width).abs() < 1.0);
        if !valid {
            st.cap_cache = Some((idx, width, caption_chunks(t, idx as usize, width, fs)));
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
    let (stage_w, idle) = with(|st| (st.stage.0, st.idle));
    let bottom = if idle { 36.0 } else { 92.0 };
    let key = (text.clone(), fs, stage_w, bottom);
    if with(|st| st.cap_layout.as_ref() == Some(&key)) {
        return;
    }
    with(|st| st.cap_layout = Some(key));
    set(ui, "lblCaption", "visible", !text.is_empty());
    set(ui, "lblCaption", "font_size", fs);
    let mut style = TextStyle::default();
    style.font.family = "Segoe UI".into();
    style.font.size = fs;
    style.font.weight = FontWeight::SemiBold;
    let h = ui.measure_text_wrapped(&text, &style, width).height.ceil() + 8.0;
    if let Some(cb) = c(ui, "lblCaption").and_then(|id| ui.control_mut(id)) {
        let side = ((stage_w - width) / 2.0).max(0.0);
        cb.set_height(h).set_margins([side, 0.0, side, bottom]);
    }
    set(ui, "lblCaption", "text", text);
    ui.layout();
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
    for (name, d) in ICONS {
        let _ = ui.register_icon_svg(name, "", d, 24.0);
    }
    for (name, d) in LINE_ICONS {
        let _ = ui.register_icon_svg(name, d, "", 24.0);
    }
    // A barra da biblioteca usa os desenhos do original (pasta com +, mesclar).
    for (name, d) in [
        ("lib-folder", "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"),
        ("lib-folder-plus", "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z M12 11v6 M9 14h6"),
        ("lib-download", "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M7 10l5 5 5-5 M12 15V3"),
        ("lib-trash", "M3 6h2h16 M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6 M10 11v6 M14 11v6 M9 6V4a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2"),
    ] {
        let _ = ui.register_icon_svg(name, d, "", 24.0);
    }
    let _ = ui.register_icon_svg("lib-merge", "", "M7.586 8.00366L4 8.00366C3.44772 8.00366 3 7.55595 3 7.00366C3 6.45138 3.44772 6.00366 4 6.00366L8 6.00366C8.26509 6.00366 8.51933 6.10892 8.70685 6.2963L13.414 11H18.5845L15.2931 7.71103C14.9025 7.32065 14.9023 6.68748 15.2926 6.29681C15.683 5.90615 16.3162 5.90592 16.7068 6.2963L21.7068 11.2926C21.8945 11.4802 22 11.7346 22 11.9998C22 12.2651 21.8947 12.5195 21.7071 12.7071L16.7071 17.7071C16.3166 18.0976 15.6834 18.0976 15.2929 17.7071C14.9024 17.3166 14.9024 16.6834 15.2929 16.2929L18.5858 13H13.4142L8.70711 17.7071C8.51957 17.8947 8.26522 18 8 18H4C3.44772 18 3 17.5523 3 17C3 16.4477 3.44772 16 4 16H7.58579L11.5855 12.0003L7.586 8.00366Z", 24.0);
    for (name, key) in [
        ("btnPlay", "player.playPause"),
        ("btnMute", "player.mute"),
        ("btnSpeed", "player.speed"),
        ("btnInfo", "player.videoInfo"),
        ("btnFullscreen", "player.fullscreen"),
        ("btnPlayerClose", "player.close"),
        ("lyPlayerClock", "player.clockTitle"),
    ] {
        set(ui, name, "hint", lang::t(key));
    }
    set(ui, "lblPlayerPaused", "text", lang::t("player.paused"));
    for name in ["trkVolume", "pnlCcCaptions", "pnlCcTranscript"] {
        if let Some(id) = c(ui, name) {
            ui.on(id, "on_pointer_move", |ui, _, _| activity(ui));
        }
    }
    if let Some(t) = c(ui, "trkVolume") {
        ui.on(t, "on_dbl_click", |ui, _, _| set_volume(ui, 100.0));
    }
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
        if crate::app_view::confirm_open() {
            match k.key {
                Key::Escape => crate::app_view::confirm_close(ui, false),
                Key::Enter => crate::app_view::confirm_close(ui, true),
                _ => {}
            }
            return;
        }
        if is_open() {
            let typing = ui.focused(form_of(ui)).is_some_and(|f| ui.get::<Edit>(f).is_some());
            if !typing {
                key(ui, k);
            }
            return;
        }
        if crate::app_export::is_open() {
            let typing = ui.focused(form_of(ui)).is_some_and(|f| ui.get::<Edit>(f).is_some());
            crate::app_export::key(ui, k, typing);
            return;
        }
        if crate::app_picker::is_open() {
            if k.key == Key::Escape {
                crate::app_picker::close(ui);
            }
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

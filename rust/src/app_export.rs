//! Presenter da tela de exportação (`export.js` da versão web): telas, prévia
//! com recorte e zoom, linha do tempo com cortes e forma de onda, faixas de
//! áudio, opções de vídeo, nome/formato, a exportação e a prévia gerada, e a
//! barra de progresso no card da gravação de origem.
//!
//! A prévia é o motor do player (`playback`) aberto PARADO na gravação: o
//! cursor da linha do tempo manda nele, e tocando é ele que manda no cursor.
//! A composição das telas escolhidas é a mesma do player (regiões lado a
//! lado), e as faixas marcadas tocam pelo volume de cada faixa do motor.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedPixelBuffer, SharedString, VecModel};

use crate::app_view::{AppWindow, Ex, ExChk, ExCut, ExLabel, ExRow, ExTick, Lib, SelOpt, Theme};
use crate::bridge::export::{self as bexp, ExportRequest};
use crate::playback::{Out, Playback, Region, ViewSpec};
use crate::view::{ExportEncoderInfo, StreamInfo, Transcript, Turn, VideoInfo, VideoLayout};
use crate::{bridge, config, lang, log, mainthread};

/// Camada das dicas desta tela (`ExL.layer` na `.slint`).
pub const HINT_LAYER: i32 = 4;
/// Lado mínimo (px de tela) pra a alça de borda caber (`CROP_EDGE_MIN`).
const CROP_EDGE_MIN: f32 = 46.0;
/// Menor recorte útil, em pixels da origem.
const CROP_MIN: i32 = 32;
const CRF_MAX: i32 = 51;
const CRF_DEFAULT: i32 = 23;
const FPS_MAX: i32 = 1000;
const FPS_MAX_SVT: i32 = 240;
/// A partir deste CRF o SVT-AV1 já está no quantizador máximo (o arquivo sai igual).
const SVT_FLOOR_CRF: i32 = 31;
const SPEED_MAX: f64 = 512.0;
const SPEED_STEPS: [f64; 25] =
    [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0, 192.0, 256.0, 384.0, 512.0];
const PREVIEW_SECS: [f64; 3] = [5.0, 10.0, 30.0];
const TL_H: f32 = 46.0;
const TICK_STEPS: [f64; 13] = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0];

#[derive(Clone, Copy, Debug)]
struct Part {
    start: f64,
    end: f64,
    keep: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

struct CropDrag {
    dir: String,
    kx: f32,
    ky: f32,
    sx: f32,
    sy: f32,
    start: Rect,
    moved: f32,
}

struct PanDrag {
    sx: f32,
    sy: f32,
    px: f32,
    py: f32,
    moved: f32,
}

#[derive(Default)]
struct State {
    id: String,
    loaded: bool,
    duration: f64,
    src_w: i32,
    src_h: i32,
    src_fps: f64,
    layout: Option<VideoLayout>,
    audio: Vec<StreamInfo>,
    sel_regions: BTreeSet<usize>,
    no_video: bool,
    turns: Option<Vec<Turn>>,
    transcribed: bool,
    captions_on: bool,
    cap_cache: Option<(usize, i32, Vec<(String, f64, f64)>)>,
    peaks: Option<Vec<f32>>,
    wave_max: f32,
    crop: Option<Rect>,
    crop_drag: Option<CropDrag>,
    crop_zoom: f32,
    pan: (f32, f32),
    pan_drag: Option<PanDrag>,
    last_pan_moved: f32,
    sel_audio: BTreeSet<i64>,
    original_audio: bool,
    mix_checked: bool,
    parts: Vec<Part>,
    play_sec: f64,
    zoom: f64,
    scroll: f64,
    view_w: f64,
    cursor_part: i32,
    encoders: Vec<ExportEncoderInfo>,
    enc_id: String,
    res_heights: Vec<i32>,
    res_h: i32,
    scale_algo: String,
    speed_text: String,
    fps_val: i32,
    q_pos: i32,
    burn: bool,
    container: String,
    video_container: String,
    prevsec: usize,
    frame: (f32, f32),
    pb: Option<Playback>,
    playing: bool,
    user_muted: bool,
    user_vol: i32,
    timer: Option<slint::Timer>,
    // ---- exportação em curso (vive fora da tela: ela pode ser fechada) ----
    running: bool,
    running_id: String,
    pct: f64,
    canceling: bool,
    previewing: bool,
    preview_pct: f64,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State {
        crop_zoom: 1.0,
        zoom: 1.0,
        user_vol: 100,
        q_pos: CRF_MAX - CRF_DEFAULT,
        scale_algo: "bicubic".into(),
        speed_text: "1".into(),
        container: "mp4".into(),
        video_container: "mp4".into(),
        prevsec: 1,
        cursor_part: -1,
        enc_id: "auto".into(),
        ..Default::default()
    });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn window() -> Option<AppWindow> {
    crate::app_view::window()
}

fn ex(w: &AppWindow) -> Ex<'_> {
    w.global::<Ex>()
}

fn scale() -> f32 {
    window().map_or(1.0, |w| w.window().scale_factor())
}

fn fmt_time(sec: f64) -> String {
    let s = sec.max(0.0).round() as u64;
    let (h, m, ss) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{ss:02}")
    } else {
        format!("{m:02}:{ss:02}")
    }
}

fn parse_time(txt: &str) -> Option<f64> {
    let parts: Vec<&str> = txt.trim().split(':').map(str::trim).collect();
    let mut sec = 0.0;
    for p in parts {
        let v: f64 = p.parse().ok()?;
        sec = sec * 60.0 + v;
    }
    Some(sec.round())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn opt(text: &str) -> SelOpt {
    SelOpt { text: text.into(), header: false, enabled: true }
}

pub fn is_open() -> bool {
    with(|st| !st.id.is_empty())
}

/// A tela está aberta NESTE id (as respostas de probe/transcrição vêm pra cá).
fn waiting_for(id: &str) -> bool {
    with(|st| !st.id.is_empty() && st.id == id)
}

// ---------------------------------------------------------------------------
// Abrir / fechar
// ---------------------------------------------------------------------------

/// Abre a tela pra uma gravação (botão "Exportar" ou menu de contexto).
pub fn open_for(id: &str) {
    let Some(w) = window() else { return };
    if id.is_empty() {
        return;
    }
    // Uma de cada vez (o backend também recusa a segunda).
    if with(|st| st.running) || bexp::export_busy() {
        crate::app_view::show_toast(&lang::t("toast.exportBusy"), &lang::t("toast.exportBusyMsg"), true, 5000);
        return;
    }
    if !bridge::check_recording_for_player(id) {
        return;
    }
    close_engine();
    let captions_on = config::get_config_bool("exportCaptions", false);
    with(|st| {
        let enc = std::mem::take(&mut st.encoders);
        // O tamanho do quadro e da linha do tempo vêm da `.slint` quando a
        // janela muda, não a cada abertura: sobrevivem ao reset.
        let (frame, view_w) = (st.frame, st.view_w);
        *st = State {
            frame,
            view_w,
            id: id.to_owned(),
            crop_zoom: 1.0,
            zoom: 1.0,
            user_vol: 100,
            q_pos: CRF_MAX - CRF_DEFAULT,
            scale_algo: "bicubic".into(),
            speed_text: "1".into(),
            container: "mp4".into(),
            video_container: "mp4".into(),
            prevsec: 1,
            cursor_part: -1,
            enc_id: String::new(),
            original_audio: true,
            captions_on,
            encoders: enc,
            ..Default::default()
        };
    });
    let base = std::path::Path::new(id).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let base = if base.is_empty() { lang::t("export.defaultName") } else { base };
    let g = ex(&w);
    g.set_name(format!("{base} - {}", lang::t("export.nameSuffix")).into());
    g.set_ready(false);
    g.set_msg(lang::t("export.previewLoading").into());
    g.set_frame(slint::Image::default());
    g.set_speed_text("1".into());
    g.set_speed_invalid(false);
    g.set_time_text("00:00".into());
    g.set_prevsec_idx(1);
    g.set_scroll_top_seq(g.get_scroll_top_seq() + 1);
    g.set_open(true);
    crate::app_main::set_hint_layer(&w, HINT_LAYER);

    render_all();

    bridge::handle_request_video_info(id);
    bridge::handle_request_transcript(id);

    // Prévia: o motor do player, parado no começo.
    let view = with(|st| view_spec(st));
    let id_out = id.to_owned();
    let out: Arc<dyn Fn(Out) + Send + Sync> = Arc::new(move |o| {
        let id = id_out.clone();
        mainthread::queue(move || on_out(&id, o));
    });
    let pb = Playback::open_paused(id, view, out);
    with(|st| st.pb = Some(pb));
    push_volumes();

    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), tick);
    with(|st| st.timer = Some(timer));
    log!("Export: tela aberta para {id}");
}

fn close_engine() {
    let pb = with(|st| {
        st.timer = None;
        st.pb.take()
    });
    drop(pb);
}

pub fn close() {
    let Some(w) = window() else { return };
    // A prévia gerada é desta tela: fechar a cancela (a exportação de verdade não).
    if with(|st| st.previewing) {
        bexp::handle_cancel_export();
        with(|st| st.previewing = false);
    }
    close_engine();
    with(|st| {
        st.id.clear();
        st.crop = None;
        st.crop_drag = None;
        st.pan_drag = None;
    });
    let g = ex(&w);
    g.set_open(false);
    g.set_frame(slint::Image::default());
    crate::app_main::set_hint_layer(&w, 0);
    sync_card();
}

// ---------------------------------------------------------------------------
// Respostas do core
// ---------------------------------------------------------------------------

/// `video_info`: devolve true se era desta tela.
pub fn on_video_info(info: &VideoInfo) -> bool {
    if !waiting_for(&info.id) {
        return false;
    }
    with(|st| {
        st.loaded = true;
        st.duration = info.duration.max(0.0);
        st.audio.clear();
        st.src_w = 0;
        st.src_h = 0;
        st.src_fps = 0.0;
        for s in &info.streams {
            if s.kind == "video" && st.src_w == 0 {
                st.src_w = s.width.unwrap_or(0) as i32;
                st.src_h = s.height.unwrap_or(0) as i32;
                st.src_fps = s.frame_rate.unwrap_or(0.0);
            } else if s.kind == "audio" {
                st.audio.push(s.clone());
            }
        }
        st.layout = info.layout.clone().filter(|l| !l.regions.is_empty());
        if let Some(first) = st.audio.first() {
            st.sel_audio.insert(first.index);
        }
        st.parts = vec![Part { start: 0.0, end: st.duration, keep: true }];
        st.play_sec = 0.0;
        st.zoom = 1.0;
        st.scroll = 0.0;
        st.fps_val = max_fps(st);
    });
    request_wave();
    push_view();
    push_volumes();
    render_all();
    true
}

pub fn on_transcript(t: &Transcript) -> bool {
    if !waiting_for(&t.id) {
        return false;
    }
    with(|st| {
        st.transcribed = t.transcribed;
        st.turns = Some(t.turns.clone());
        st.cap_cache = None;
    });
    render_captions();
    sync_caption();
    true
}

/// Forma de onda em alta resolução (`hi`): só a exportação pede.
pub fn on_waveform(id: &str, peaks: &[f32]) {
    if !waiting_for(id) {
        return;
    }
    let max = peaks.iter().cloned().filter(|p| p.is_finite() && *p >= 0.0).fold(0.0f32, f32::max);
    with(|st| {
        st.peaks = Some(peaks.iter().map(|p| if p.is_finite() && *p >= 0.0 { *p } else { 0.0 }).collect());
        st.wave_max = max.max(0.0001);
    });
    render_timeline();
}

fn request_wave() {
    let (id, dur, n) = with(|st| (st.id.clone(), st.duration, st.audio.len()));
    if id.is_empty() || dur <= 0.0 || n == 0 {
        return;
    }
    let buckets = ((dur * 25.0).round() as i64).clamp(2000, 150_000);
    bridge::handle_request_waveform(&id, buckets, true);
}

/// Encoders da exportação (do warmup).
pub fn set_encoders(list: &[ExportEncoderInfo]) {
    with(|st| st.encoders = list.to_vec());
    if is_open() {
        render_encoders();
    }
}

// ---------------------------------------------------------------------------
// Do motor da prévia
// ---------------------------------------------------------------------------

fn on_out(id: &str, o: Out) {
    if !waiting_for(id) {
        return;
    }
    let Some(w) = window() else { return };
    let g = ex(&w);
    match o {
        Out::Info(_) => {}
        Out::Frame { buf, x, y, .. } => {
            let k = scale();
            let (bw, bh) = (buf.width(), buf.height());
            g.set_frame(slint::Image::from_rgba8(buf));
            g.set_frame_x(x as f32 / k);
            g.set_frame_y(y as f32 / k);
            g.set_frame_w(bw as f32 / k);
            g.set_frame_h(bh as f32 / k);
            if !g.get_ready() {
                g.set_ready(true);
                render_playbar();
            }
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
                st.playing = false;
            });
            render_playbar();
        }
        Out::Error(e) => {
            // Sem prévia a exportação continua funcionando.
            g.set_ready(false);
            g.set_msg(lang::t("export.previewUnavailable").into());
            log!("Export: previa indisponivel ({e})");
        }
    }
}

fn tick() {
    let playing = with(|st| st.playing);
    if !playing {
        return;
    }
    let pos = with(|st| st.pb.as_ref().map_or(st.play_sec, |p| p.shared.position()));
    set_play_sec(pos, false);
}

/// Vista do motor: o palco é o QUADRO (proporção da origem ou da composição).
fn view_spec(st: &State) -> ViewSpec {
    let k = scale() as f64;
    let (fw, fh) = (st.frame.0 as f64, st.frame.1 as f64);
    let z = st.crop_zoom as f64;
    // Zoom da web com origem no CENTRO; o do motor tem origem no canto.
    let pan_x = st.pan.0 as f64 + fw / 2.0 * (1.0 - z);
    let pan_y = st.pan.1 as f64 + fh / 2.0 * (1.0 - z);
    ViewSpec {
        stage_w: (fw * k).round().max(2.0) as i32,
        stage_h: (fh * k).round().max(2.0) as i32,
        zoom: z,
        pan_x: pan_x * k,
        pan_y: pan_y * k,
        regions: if compose_on(st) { video_regions(st) } else { Vec::new() },
    }
}

/// As telas escolhidas, em pixels do VÍDEO, sem o "webcam no topo" do player
/// (a composição da exportação centraliza todas na vertical).
fn video_regions(st: &State) -> Vec<Region> {
    let Some(lay) = &st.layout else { return Vec::new() };
    let sx = if lay.canvas_w > 0 && st.src_w > 0 { st.src_w as f64 / lay.canvas_w as f64 } else { 1.0 };
    let sy = if lay.canvas_h > 0 && st.src_h > 0 { st.src_h as f64 / lay.canvas_h as f64 } else { 1.0 };
    st.sel_regions
        .iter()
        .filter_map(|&i| lay.regions.get(i))
        .map(|r| Region {
            x: (r.x as f64 * sx).round() as i32,
            y: (r.y as f64 * sy).round() as i32,
            w: (r.w as f64 * sx).round() as i32,
            h: (r.h as f64 * sy).round() as i32,
            webcam: false,
        })
        .collect()
}

fn push_view() {
    with(|st| {
        if st.frame.0 <= 0.0 {
            return;
        }
        let v = view_spec(st);
        if let Some(pb) = &st.pb {
            pb.set_view(v);
        }
    });
}

/// O que a prévia toca (`_audibleTracks`): a mistura (posição 0) no original
/// ou com ela marcada; senão as isoladas marcadas, somadas.
fn audible(st: &State) -> Vec<usize> {
    let n = st.audio.len();
    if n == 0 {
        return Vec::new();
    }
    if use_original(st) || st.sel_audio.contains(&mix_index(st)) {
        return vec![0];
    }
    st.audio.iter().enumerate().filter(|(_, s)| st.sel_audio.contains(&s.index)).map(|(k, _)| k).collect()
}

fn push_volumes() {
    with(|st| {
        let aud = audible(st);
        let tracks: Vec<f32> = (0..st.audio.len().max(1)).map(|k| if aud.contains(&k) { 1.0 } else { 0.0 }).collect();
        let master = if st.user_muted { 0.0 } else { st.user_vol as f32 / 100.0 };
        if let Some(pb) = &st.pb {
            pb.set_volumes(master, tracks);
        }
    });
}

// ---------------------------------------------------------------------------
// Contas (as mesmas do export.js)
// ---------------------------------------------------------------------------

fn mix_index(st: &State) -> i64 {
    st.audio.first().map_or(-1, |s| s.index)
}

fn use_original(st: &State) -> bool {
    st.original_audio && st.audio.len() > 1
}

fn audio_selection(st: &State) -> Vec<i32> {
    if use_original(st) {
        st.audio.iter().map(|s| s.index as i32).collect()
    } else {
        st.sel_audio.iter().map(|&i| i as i32).collect()
    }
}

fn audio_is_complete(st: &State) -> bool {
    if use_original(st) {
        return true;
    }
    let mix = mix_index(st);
    if st.sel_audio.contains(&mix) {
        return true;
    }
    let isolated = st.audio.iter().filter(|s| s.index != mix).count();
    let picked = st.sel_audio.iter().filter(|&&i| i != mix).count();
    isolated > 0 && picked == isolated
}

fn compose_on(st: &State) -> bool {
    !st.no_video && st.layout.is_some() && !st.sel_regions.is_empty()
}

fn crop_available(st: &State) -> bool {
    !st.id.is_empty() && st.src_w > 0 && st.src_h > 0 && !st.no_video && st.sel_regions.is_empty()
}

fn crop_rect(st: &State) -> Rect {
    st.crop.unwrap_or(Rect { x: 0, y: 0, w: st.src_w, h: st.src_h })
}

fn has_crop(st: &State) -> bool {
    st.crop.is_some_and(|c| c.x > 0 || c.y > 0 || c.w < st.src_w || c.h < st.src_h)
}

fn even_rect(st: &State, x: i32, y: i32, w: i32, h: i32) -> Rect {
    let ev = |v: i32| (v.max(0)) & !1;
    let (x, y) = (ev(x), ev(y));
    Rect { x, y, w: ev(w).min(ev(st.src_w) - x).max(2), h: ev(h).min(ev(st.src_h) - y).max(2) }
}

fn ordered_regions(st: &State) -> Vec<(i64, i64, i64, i64)> {
    let Some(lay) = &st.layout else { return Vec::new() };
    let mut out: Vec<(i64, i64, i64, i64)> = st.sel_regions.iter().filter_map(|&i| lay.regions.get(i)).map(|r| (r.x, r.y, r.w, r.h)).collect();
    out.sort_by_key(|r| r.0);
    out
}

/// Tamanho nativo do que sai (`_composedSize`).
fn composed_size(st: &State) -> (i32, i32) {
    if st.layout.is_none() || st.sel_regions.is_empty() {
        let c = crop_rect(st);
        return (if c.w > 0 { c.w } else { st.src_w }, if c.h > 0 { c.h } else { st.src_h });
    }
    let (mut w, mut h) = (0i64, 0i64);
    for r in ordered_regions(st) {
        w += r.2;
        h = h.max(r.3);
    }
    (if w > 0 { w as i32 } else { st.src_w }, if h > 0 { h as i32 } else { st.src_h })
}

fn stage_dims(st: &State) -> (i32, i32) {
    if compose_on(st) {
        composed_size(st)
    } else {
        (st.src_w, st.src_h)
    }
}

fn output_size(st: &State) -> (i32, i32) {
    let (cw, ch) = composed_size(st);
    if st.res_h <= 0 || st.res_h >= ch {
        return (cw, ch);
    }
    let s = st.res_h as f64 / ch as f64;
    ((((cw as f64 * s).round() as i32) & !1).max(2), (((ch as f64 * s).round() as i32) & !1).max(2))
}

fn is_resizing(st: &State) -> bool {
    output_size(st) != composed_size(st)
}

fn kept_segments(st: &State) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for p in &st.parts {
        if !p.keep {
            continue;
        }
        match out.last_mut() {
            Some(last) if (last.1 - p.start).abs() < 0.001 => last.1 = p.end,
            _ => out.push((p.start, p.end)),
        }
    }
    out
}

fn kept_duration(st: &State) -> f64 {
    kept_segments(st).iter().map(|s| s.1 - s.0).sum()
}

fn part_at_cursor(st: &State) -> i32 {
    if st.parts.is_empty() {
        return -1;
    }
    for (i, p) in st.parts.iter().enumerate() {
        if st.play_sec >= p.start && st.play_sec < p.end {
            return i as i32;
        }
    }
    st.parts.len() as i32 - 1
}

fn nearest_cut(st: &State) -> i32 {
    if st.parts.len() < 2 {
        return -1;
    }
    let mut best = -1;
    let mut dist = f64::MAX;
    for i in 1..st.parts.len() {
        let d = (st.parts[i].start - st.play_sec).abs();
        if d < dist {
            dist = d;
            best = i as i32;
        }
    }
    best
}

fn speed(st: &State) -> f64 {
    let raw = st.speed_text.trim().replace(',', ".");
    match raw.parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 1.0 => v.min(SPEED_MAX),
        _ => 1.0,
    }
}

fn fmt_speed(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
    if lang::current_language().starts_with("en") {
        s
    } else {
        s.replace('.', ",")
    }
}

fn max_fps(st: &State) -> i32 {
    let src = st.src_fps.round() as i32;
    if src <= 0 {
        return 0;
    }
    let s = speed(st);
    let mut max = if s > 1.0 { (src as f64 * s + 1e-6).floor() as i32 } else { src };
    let cap = if st.enc_id == "av1-sw" { FPS_MAX_SVT } else { FPS_MAX };
    if s > 1.0 {
        max = max.min(cap);
    }
    max
}

fn target_fps(st: &State) -> i32 {
    let max = max_fps(st);
    if max <= 20 {
        return 0;
    }
    st.fps_val.clamp(20, max)
}

fn crf(st: &State) -> i32 {
    (CRF_MAX - st.q_pos).clamp(0, CRF_MAX)
}

fn crf_band_key(crf: i32) -> &'static str {
    match crf {
        ..=17 => "export.crfBandLossless",
        18..=23 => "export.crfBandHigh",
        24..=30 => "export.crfBandBalanced",
        31..=40 => "export.crfBandSmall",
        _ => "export.crfBandLow",
    }
}

fn enc_label(id: &str) -> String {
    match id {
        "h264-hw" => lang::t("settings.codec.h264Hw"),
        "h264-sw" => lang::t("settings.codec.h264Sw"),
        "av1-hw" => lang::t("settings.codec.av1Hw"),
        "av1-sw" => lang::t("settings.codec.av1Sw"),
        "hevc-hw" => lang::t("settings.codec.hevcHw"),
        _ => id.to_owned(),
    }
}

/// Largura útil da coluna (a mesma conta da `.slint`).
fn col_width() -> f32 {
    let Some(w) = window() else { return 844.0 };
    let k = w.window().scale_factor();
    let ww = w.window().size().width as f32 / k;
    (ww - 8.0).min(880.0) - 36.0
}

/// Quebra as pílulas em linhas (`flex-wrap`, 6 px entre elas).
fn wrap_rows(items: Vec<ExChk>) -> Vec<ExRow> {
    let max = col_width();
    let mut rows: Vec<Vec<ExChk>> = Vec::new();
    let mut cur: Vec<ExChk> = Vec::new();
    let mut x = 0.0f32;
    for it in items {
        let w = crate::win::text_width(it.text.as_str(), 12, 400) as f32 + 41.0;
        if !cur.is_empty() && x + w > max {
            rows.push(std::mem::take(&mut cur));
            x = 0.0;
        }
        x += w + 6.0;
        cur.push(it);
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    rows.into_iter().map(|r| ExRow { items: model(r) }).collect()
}

// ---------------------------------------------------------------------------
// Desenho
// ---------------------------------------------------------------------------

fn render_all() {
    render_regions();
    render_stage();
    render_playbar();
    render_timeline();
    render_audio();
    render_resolutions();
    render_encoders();
    render_scale();
    render_speed();
    render_fps();
    render_quality();
    render_captions();
    render_container();
    render_actions();
    sync_caption();
}

fn render_regions() {
    let Some(w) = window() else { return };
    let items = with(|st| {
        let regs = st.layout.as_ref().map(|l| l.regions.clone()).unwrap_or_default();
        let mut v = vec![ExChk {
            idx: 0,
            text: if regs.len() > 1 { lang::t("export.allRegions") } else { lang::t("export.fullScreen") }.into(),
            checked: !st.no_video && st.sel_regions.is_empty(),
            disabled: false,
        }];
        if regs.len() > 1 {
            for (i, r) in regs.iter().enumerate() {
                let name = if r.name.is_empty() { lang::t_with("export.regionN", &[("n", &(i + 1).to_string())]) } else { r.name.clone() };
                v.push(ExChk { idx: i as i32 + 1, text: name.into(), checked: !st.no_video && st.sel_regions.contains(&i), disabled: false });
            }
        }
        v.push(ExChk { idx: -1, text: lang::t("export.noScreen").into(), checked: st.no_video, disabled: st.loaded && st.audio.is_empty() });
        v
    });
    ex(&w).set_region_rows(model(wrap_rows(items)));
}

fn region_clicked(idx: i32) {
    with(|st| {
        match idx {
            0 => {
                st.sel_regions.clear();
                st.no_video = false;
            }
            -1 => {
                if st.loaded && st.audio.is_empty() {
                    return;
                }
                st.no_video = !st.no_video;
                if st.no_video {
                    st.sel_regions.clear();
                }
            }
            i => {
                let i = (i - 1) as usize;
                st.no_video = false;
                if !st.sel_regions.remove(&i) {
                    st.sel_regions.insert(i);
                }
            }
        }
        // Recorte e telas são dois jeitos de dizer o que sai: o último vale.
        st.crop = None;
        if compose_on(st) || !crop_available(st) {
            st.crop_zoom = 1.0;
            st.pan = (0.0, 0.0);
        }
        if st.no_video {
            if st.container != "mp3" {
                st.video_container = st.container.clone();
            }
            st.container = "mp3".into();
        } else if st.container == "mp3" {
            st.container = st.video_container.clone();
        }
    });
    push_view();
    render_all();
}

/// Proporção do quadro, recorte, zoom, mensagem de só áudio.
fn render_stage() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (dims, crop_on, r, sw, sh, zoom, pan, no_video, has) = with(|st| {
        (stage_dims(st), crop_available(st), crop_rect(st), st.src_w, st.src_h, st.crop_zoom, st.pan, st.no_video, has_crop(st))
    });
    if dims.0 > 0 && dims.1 > 0 {
        g.set_aspect(dims.0 as f32 / dims.1 as f32);
    }
    g.set_audio_only(no_video);
    g.set_crop_on(crop_on);
    if crop_on && sw > 0 && sh > 0 {
        g.set_crop_l(r.x as f32 / sw as f32);
        g.set_crop_t(r.y as f32 / sh as f32);
        g.set_crop_w(r.w as f32 / sw as f32);
        g.set_crop_h(r.h as f32 / sh as f32);
        g.set_crop_size(format!("{}\u{00D7}{}", r.w, r.h).into());
        // O que cabe desenhar é decisão de PIXEL DE TELA.
        let (fw, fh) = with(|st| st.frame);
        let bw = r.w as f32 / sw as f32 * fw * zoom;
        let bh = r.h as f32 / sh as f32 * fh * zoom;
        g.set_edges_h(fw <= 0.0 || bw >= CROP_EDGE_MIN);
        g.set_edges_v(fh <= 0.0 || bh >= CROP_EDGE_MIN);
    }
    g.set_zoom(zoom);
    g.set_pan_x(pan.0);
    g.set_pan_y(pan.1);
    g.set_zoom_badge(format!("{}%", (zoom * 100.0).round() as i32).into());
    g.set_zoom_reset_enabled(zoom > 1.001);
    g.set_crop_reset_enabled(has);
    g.set_crop_info(if has { lang::t_with("export.cropOf", &[("w", &r.w.to_string()), ("h", &r.h.to_string()), ("sw", &sw.to_string()), ("sh", &sh.to_string())]) } else { String::new() }.into());
    // Legenda proporcional ao quadro (`_layoutCaption`).
    let (fw, fh) = with(|st| st.frame);
    if fh > 0.0 {
        let font = (fh * 0.045).max(9.0);
        g.set_caption_font(font);
        g.set_caption_w((fw * 0.86).min(font * 32.0));
    }
}

fn render_playbar() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (playing, muted, vol, has, no_video, on) = with(|st| {
        (st.playing, st.user_muted || st.user_vol == 0, st.user_vol, st.turns.as_ref().is_some_and(|t| !t.is_empty()), st.no_video, st.captions_on)
    });
    let ready = g.get_ready();
    g.set_play_enabled(ready);
    g.set_playing(playing);
    g.set_muted(muted);
    g.set_volume(vol);
    g.set_cc_enabled(has && !no_video);
    g.set_cc_active(on && has && !no_video);
    g.set_cc_hint(
        if !has {
            lang::t("export.captionsNone")
        } else if on {
            lang::t("export.captionsHide")
        } else {
            lang::t("export.captionsShow")
        }
        .into(),
    );
}

// ---- linha do tempo ----

fn max_zoom(st: &State) -> f64 {
    (st.duration / 2.0).min(2000.0).max(1.0)
}

fn total_w(st: &State) -> f64 {
    st.view_w.max(1.0) * st.zoom.max(1.0)
}

fn clamp_scroll(st: &mut State) {
    let max = (total_w(st) - st.view_w).max(0.0);
    st.scroll = st.scroll.clamp(0.0, max);
}

fn render_timeline() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let dark = w.global::<Theme>().get_dark();
    let k = w.window().scale_factor();
    let data = with(|st| {
        clamp_scroll(st);
        st.cursor_part = part_at_cursor(st);
        let dur = st.duration.max(0.001);
        let total = total_w(st);
        let view = st.view_w;
        let x_of = |sec: f64| (sec / dur * total - st.scroll) as f32;
        // Divisões e a que "Remover divisão" apaga.
        let target = nearest_cut(st);
        let cuts: Vec<ExCut> = (0..st.parts.len().saturating_sub(1)).map(|i| ExCut { x: x_of(st.parts[i].end), target: target == i as i32 + 1 }).collect();
        // Régua com passo de ~90 px, só a janela visível + uma tela de folga.
        let mut ticks = Vec::new();
        if st.duration > 0.0 {
            let step = TICK_STEPS.iter().copied().find(|&s| s / st.duration * total >= 90.0).unwrap_or(3600.0);
            let spp = st.duration / total;
            let from = ((st.scroll - view) * spp).max(0.0);
            let to = ((st.scroll + view * 2.0) * spp).min(st.duration);
            let mut t = (from / step).ceil() * step;
            while t <= to {
                if t / st.duration > 0.985 {
                    break;
                }
                ticks.push(ExTick { x: x_of(t), text: fmt_time(t).into() });
                t += step;
            }
        }
        let labels: Vec<ExLabel> = st
            .parts
            .iter()
            .filter(|p| (p.end - p.start) / dur > 0.12)
            .map(|p| ExLabel { x: x_of(p.start), w: ((p.end - p.start) / dur * total) as f32, text: fmt_time(p.end - p.start).into() })
            .collect();
        let sb_w = if total > 0.0 { (view * view / total).max(20.0).min(view) } else { view };
        let sb_x = if total > view { st.scroll / (total - view) * (view - sb_w) } else { 0.0 };
        let img = timeline_image(st, dark, k);
        let has_wave = st.peaks.as_ref().is_some_and(|p| !p.is_empty()) && st.duration > 0.0;
        (cuts, ticks, labels, sb_x as f32, sb_w as f32, img, has_wave, x_of(st.play_sec))
    });
    let (cuts, ticks, labels, sb_x, sb_w, img, has_wave, ph) = data;
    g.set_cuts(model(cuts));
    g.set_ticks(model(ticks));
    g.set_part_labels(model(labels));
    g.set_sb_x(sb_x);
    g.set_sb_w(sb_w);
    if let Some(img) = img {
        g.set_tl_img(img);
    }
    g.set_has_wave(has_wave);
    g.set_playhead_x(ph);
    render_tlbar();
}

/// Fundo dos trechos (mantido verde, tirado listrado, realce do trecho do
/// cursor) e a forma de onda, só na janela visível.
fn timeline_image(st: &State, dark: bool, k: f32) -> Option<slint::Image> {
    let view = st.view_w as f32;
    if view <= 1.0 {
        return None;
    }
    let (wpx, hpx) = ((view * k).round().max(1.0) as u32, (TL_H * k).round().max(1.0) as u32);
    let mut buf = SharedPixelBuffer::<slint::Rgba8Pixel>::new(wpx, hpx);
    let px = buf.make_mut_slice();
    let rgb = |c: u32| ((c >> 16) as u8, (c >> 8) as u8, c as u8);
    let (p2, p3, b2, succ, t3) = if dark {
        (rgb(0x1a1a1e), rgb(0x202024), rgb(0x36363c), rgb(0x10b981), rgb(0x71717a))
    } else {
        (rgb(0xfafafa), rgb(0xf4f4f5), rgb(0xd4d4d8), rgb(0x059669), rgb(0x71717a))
    };
    let blend = |a: (u8, u8, u8), b: (u8, u8, u8), t: f32| {
        let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
    };
    let keep_bg = blend(p2, (16, 185, 129), 0.16);
    let dur = st.duration.max(0.001);
    let total = total_w(st) as f32;
    let scroll = st.scroll as f32;
    let cur = st.cursor_part;
    let put = |px: &mut [slint::Rgba8Pixel], x: u32, y: u32, c: (u8, u8, u8)| {
        px[(y * wpx + x) as usize] = slint::Rgba8Pixel { r: c.0, g: c.1, b: c.2, a: 255 };
    };
    // Fundo, coluna por coluna (cada coluna cai num trecho só).
    let sec_at = |xp: u32| ((scroll + (xp as f32 + 0.5) / k) / total) as f64 * dur;
    let mut pi = 0usize;
    for xp in 0..wpx {
        let s = sec_at(xp);
        while pi + 1 < st.parts.len() && s >= st.parts[pi].end {
            pi += 1;
        }
        let Some(p) = st.parts.get(pi) else { break };
        let x_css = scroll + xp as f32 / k - (p.start / dur) as f32 * total;
        for yp in 0..hpx {
            let c = if p.keep {
                keep_bg
            } else {
                let d = (x_css + yp as f32 / k) / std::f32::consts::SQRT_2;
                if d.rem_euclid(12.0) < 6.0 { p3 } else { p2 }
            };
            put(px, xp, yp, c);
        }
    }
    // Realce do trecho sob o cursor (`inset 0 0 0 1px var(--border-2)`).
    if cur >= 0 {
        if let Some(p) = st.parts.get(cur as usize) {
            let x0 = ((p.start / dur) as f32 * total - scroll) * k;
            let x1 = ((p.end / dur) as f32 * total - scroll) * k;
            let t = k.round().max(1.0) as i32;
            let (xa, xb) = (x0.round() as i32, x1.round() as i32);
            for xp in xa.max(0)..xb.min(wpx as i32) {
                for d in 0..t {
                    put(px, xp as u32, d as u32, b2);
                    put(px, xp as u32, hpx - 1 - d as u32, b2);
                }
            }
            for d in 0..t {
                for (edge, ok) in [(xa + d, xa + d >= 0 && xa + d < wpx as i32), (xb - 1 - d, xb - 1 - d >= 0 && xb - 1 - d < wpx as i32)] {
                    if ok {
                        for yp in 0..hpx {
                            put(px, edge as u32, yp, b2);
                        }
                    }
                }
            }
        }
    }
    // Forma de onda (`_renderWave`): pico por fatia, raiz quadrada, barras de ~2 px.
    if let Some(peaks) = st.peaks.as_ref().filter(|p| !p.is_empty()) {
        let n = peaks.len();
        let step = (2.0 * k).max(1.0);
        let bar_w = (step * 0.7).max(1.0);
        let mut x = 0.0f32;
        while x < wpx as f32 {
            let f0 = (scroll + x / k) / total;
            let f1 = (scroll + (x + step) / k) / total;
            if f0 >= 1.0 {
                break;
            }
            let i0 = ((f0 * n as f32).floor() as usize).min(n);
            let mut i1 = ((f1 * n as f32).ceil() as usize).min(n);
            if i1 <= i0 {
                i1 = (i0 + 1).min(n);
            }
            let m = peaks[i0.min(n - 1)..i1.max(i0.min(n - 1) + 1).min(n)].iter().cloned().fold(0.0f32, f32::max);
            let bh = ((m / st.wave_max).sqrt() * hpx as f32 * 0.82).max(k);
            let mid = ((f0 + f1) / 2.0) as f64 * dur;
            let kept = st.parts.iter().find(|p| mid >= p.start && mid < p.end).is_none_or(|p| p.keep);
            let (col, a) = if kept { (succ, 0.85) } else { (t3, 0.45) };
            let y0 = ((hpx as f32 - bh) / 2.0).max(0.0);
            let y1 = (y0 + bh).min(hpx as f32);
            let xa = x.floor() as u32;
            let xb = ((x + bar_w).ceil() as u32).min(wpx);
            for xp in xa..xb {
                // Cobertura parcial nas bordas da barra (anti-serrilhado simples).
                let cov_x = ((xp as f32 + 1.0).min(x + bar_w) - (xp as f32).max(x)).clamp(0.0, 1.0);
                for yp in (y0.floor() as u32)..(y1.ceil() as u32).min(hpx) {
                    let cov_y = ((yp as f32 + 1.0).min(y1) - (yp as f32).max(y0)).clamp(0.0, 1.0);
                    let t = a * cov_x * cov_y;
                    let i = (yp * wpx + xp) as usize;
                    let b = px[i];
                    let c = blend((b.r, b.g, b.b), col, t);
                    px[i] = slint::Rgba8Pixel { r: c.0, g: c.1, b: c.2, a: 255 };
                }
            }
            x += step;
        }
    }
    Some(slint::Image::from_rgba8(buf))
}

fn render_tlbar() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (cut_ok, uncut, uncut_at, toggle, toggle_ok, zoom, play, total, kept, spd) = with(|st| {
        let idx = part_at_cursor(st);
        let p = (idx >= 0).then(|| st.parts[idx as usize]);
        let t = st.play_sec;
        let cut_ok = p.is_some_and(|p| t > p.start + 0.05 && t < p.end - 0.05);
        let ci = nearest_cut(st);
        let at = if ci > 0 { st.parts[ci as usize].start } else { 0.0 };
        (cut_ok, ci, at, p.is_none_or(|p| p.keep), p.is_some(), st.zoom, st.play_sec, st.duration, kept_duration(st), speed(st))
    });
    g.set_cut_enabled(cut_ok);
    g.set_uncut_enabled(uncut > 0);
    g.set_uncut_hint(if uncut > 0 { lang::t_with("export.removeCutAt", &[("at", &fmt_time(uncut_at))]) } else { lang::t("export.removeCutNone") }.into());
    g.set_toggle_text(if toggle { lang::t("export.dropPart") } else { lang::t("export.keepPart") }.into());
    g.set_toggle_enabled(toggle_ok);
    g.set_tl_zoom_text(format!("{}×", if zoom < 10.0 { format!("{zoom:.1}") } else { format!("{}", zoom.round()) }).into());
    g.set_time_text(fmt_time(play).into());
    g.set_total_text(fmt_time(total).into());
    g.set_range_text(lang::t_with("export.rangeLength", &[("len", &fmt_time(kept))]).into());
    g.set_speed_value(if spd > 1.0 { lang::t_with("export.speedResult", &[("len", &fmt_time(kept / spd))]) } else { String::new() }.into());
}

/// Move o cursor (barra, tempo, botões); `seek` = também reposiciona o vídeo.
fn set_play_sec(sec: f64, seek: bool) {
    let (changed, ph) = with(|st| {
        let scroll0 = st.scroll;
        st.play_sec = sec.clamp(0.0, st.duration.max(0.0));
        if seek {
            if let Some(pb) = &st.pb {
                pb.seek(st.play_sec);
            }
        }
        // Mantém o cursor na vista.
        let total = total_w(st);
        if st.duration > 0.0 {
            let x = st.play_sec / st.duration * total;
            let margin = (st.view_w * 0.15).min(60.0);
            if x < st.scroll + margin {
                st.scroll = (x - margin).max(0.0);
            } else if x > st.scroll + st.view_w - margin {
                st.scroll = x - st.view_w + margin;
            }
        }
        let changed = part_at_cursor(st) != st.cursor_part || (st.scroll - scroll0).abs() > 0.01;
        let ph = if st.duration > 0.0 { (st.play_sec / st.duration * total - st.scroll) as f32 } else { 0.0 };
        (changed, ph)
    });
    // A imagem só muda quando o trecho do cursor ou a rolagem muda.
    if changed {
        render_timeline();
    } else if let Some(w) = window() {
        ex(&w).set_playhead_x(ph);
        render_tlbar();
    }
    sync_caption();
}

fn tl_seek(x: f32) {
    let sec = with(|st| {
        let total = total_w(st);
        ((st.scroll + x as f64) / total * st.duration).clamp(0.0, st.duration)
    });
    set_play_sec(sec, true);
}

fn set_zoom(z: f64, anchor: Option<f64>) {
    let ok = with(|st| {
        let prev = st.zoom.max(1.0);
        let next = z.clamp(1.0, max_zoom(st));
        if (next - prev).abs() < 0.0001 || st.duration <= 0.0 {
            return false;
        }
        let view = st.view_w.max(1.0);
        let anchor = anchor.unwrap_or_else(|| (st.scroll + view / 2.0) / (view * prev) * st.duration);
        let off = anchor / st.duration * (view * prev) - st.scroll;
        st.zoom = next;
        st.scroll = anchor / st.duration * (view * next) - off;
        clamp_scroll(st);
        true
    });
    if ok {
        render_timeline();
    }
}

fn tl_wheel(x: f32, dy: f32) {
    let anchor = with(|st| (st.scroll + x as f64) / total_w(st) * st.duration);
    let z = with(|st| st.zoom);
    set_zoom(z * if dy > 0.0 { 1.25 } else { 1.0 / 1.25 }, Some(anchor));
}

fn tl_scroll(thumb_x: f32) {
    with(|st| {
        let total = total_w(st);
        let view = st.view_w;
        let sb_w = (view * view / total).max(20.0).min(view);
        if total > view && view > sb_w {
            st.scroll = thumb_x as f64 / (view - sb_w) * (total - view);
        }
        clamp_scroll(st);
    });
    render_timeline();
}

fn cut_here() {
    with(|st| {
        let t = st.play_sec;
        if let Some(i) = st.parts.iter().position(|p| t > p.start + 0.05 && t < p.end - 0.05) {
            let p = st.parts[i];
            st.parts.splice(i..=i, [Part { start: p.start, end: t, keep: p.keep }, Part { start: t, end: p.end, keep: p.keep }]);
        }
    });
    after_parts();
}

fn remove_cut() {
    with(|st| {
        let i = nearest_cut(st);
        if i < 1 {
            return;
        }
        let i = i as usize;
        let (l, r) = (st.parts[i - 1], st.parts[i]);
        st.parts.splice(i - 1..=i, [Part { start: l.start, end: r.end, keep: l.keep || r.keep }]);
    });
    after_parts();
}

fn toggle_part() {
    let refused = with(|st| {
        let i = part_at_cursor(st);
        if i < 0 {
            return false;
        }
        let i = i as usize;
        // Não deixa zerar tudo.
        if st.parts[i].keep && st.parts.iter().filter(|p| p.keep).count() == 1 {
            return true;
        }
        st.parts[i].keep = !st.parts[i].keep;
        false
    });
    if refused {
        crate::app_view::show_toast(&lang::t("toast.errorTitle"), &lang::t("export.needOnePart"), true, 4000);
    }
    after_parts();
}

fn after_parts() {
    render_timeline();
}

// ---- reprodução ----

fn toggle_play() {
    let Some(w) = window() else { return };
    if !ex(&w).get_ready() {
        return;
    }
    with(|st| {
        let Some(pb) = &st.pb else { return };
        if st.playing {
            pb.pause();
            st.playing = false;
            st.play_sec = pb.shared.position();
        } else {
            pb.seek(st.play_sec);
            pb.play();
            st.playing = true;
        }
    });
    render_playbar();
}

/// Clique parado na prévia (arrastar pra deslocar não vira play/pausa).
fn stage_click() {
    let moved = with(|st| std::mem::take(&mut st.last_pan_moved));
    if moved < 4.0 {
        toggle_play();
    }
}

fn toggle_mute() {
    with(|st| {
        st.user_muted = !st.user_muted;
        if !st.user_muted && st.user_vol == 0 {
            st.user_vol = 100;
        }
    });
    push_volumes();
    render_playbar();
}

fn set_volume(v: i32) {
    with(|st| {
        st.user_vol = v.clamp(0, 100);
        if st.user_vol > 0 {
            st.user_muted = false;
        }
    });
    push_volumes();
    render_playbar();
}

fn toggle_cc() {
    with(|st| st.captions_on = !st.captions_on);
    config::set_config_bool("exportCaptions", with(|st| st.captions_on));
    render_playbar();
    sync_caption();
}

fn sync_caption() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let font = g.get_caption_font();
    let width = g.get_caption_w();
    let text = with(|st| {
        if !st.captions_on || st.no_video {
            return None;
        }
        let turns = st.turns.as_ref().filter(|t| !t.is_empty())?;
        // Turnos sobrepostos: vale o que começou por último.
        let t = st.play_sec;
        let mut best: Option<usize> = None;
        for (i, tr) in turns.iter().enumerate() {
            if t >= tr.start && t < tr.end && best.is_none_or(|b| tr.start >= turns[b].start) {
                best = Some(i);
            }
        }
        let idx = best?;
        let key = (width * 10.0) as i32 + (font * 1000.0) as i32;
        let valid = st.cap_cache.as_ref().is_some_and(|(i, k, _)| *i == idx && *k == key);
        if !valid {
            let tr = Transcript { turns: vec![turns[idx].clone()], ..Default::default() };
            st.cap_cache = Some((idx, key, crate::app_player::caption_chunks(&tr, 0, width, font)));
        }
        let chunks = &st.cap_cache.as_ref()?.2;
        let mut pick = chunks.first()?;
        for c in chunks {
            if t >= c.1 {
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

// ---- recorte e zoom da prévia ----

fn crop_down(dir: &str, x: f32, y: f32) {
    with(|st| {
        if !crop_available(st) {
            return;
        }
        let (fw, fh) = st.frame;
        let (zw, zh) = (fw * st.crop_zoom, fh * st.crop_zoom);
        if zw <= 0.0 || zh <= 0.0 {
            return;
        }
        st.crop_drag = Some(CropDrag {
            dir: dir.to_owned(),
            kx: st.src_w as f32 / zw,
            ky: st.src_h as f32 / zh,
            sx: x,
            sy: y,
            start: crop_rect(st),
            moved: 0.0,
        });
    });
    if let Some(w) = window() {
        ex(&w).set_cropping(with(|st| st.crop_drag.is_some()));
    }
}

fn crop_move(x: f32, y: f32) {
    let moved = with(|st| {
        let Some(d) = st.crop_drag.as_mut() else { return false };
        let (dxp, dyp) = (x - d.sx, y - d.sy);
        d.moved = d.moved.max(dxp.abs() + dyp.abs());
        let dx = (dxp * d.kx).round() as i32;
        let dy = (dyp * d.ky).round() as i32;
        let s = d.start;
        let dir = d.dir.clone();
        let (sw, sh) = (st.src_w, st.src_h);
        let (mut nx, mut ny, mut nw, mut nh) = (s.x, s.y, s.w, s.h);
        if dir == "move" {
            nx = (s.x + dx).max(0).min(sw - s.w);
            ny = (s.y + dy).max(0).min(sh - s.h);
        } else {
            if dir.contains('w') {
                let x2 = (s.x + dx).max(0).min(s.x + s.w - CROP_MIN);
                nw = s.x + s.w - x2;
                nx = x2;
            }
            if dir.contains('e') {
                nw = (s.w + dx).max(CROP_MIN).min(sw - s.x);
            }
            if dir.contains('n') {
                let y2 = (s.y + dy).max(0).min(s.y + s.h - CROP_MIN);
                nh = s.y + s.h - y2;
                ny = y2;
            }
            if dir.contains('s') {
                nh = (s.h + dy).max(CROP_MIN).min(sh - s.y);
            }
        }
        st.crop = Some(even_rect(st, nx, ny, nw, nh));
        true
    });
    if moved {
        render_stage();
    }
}

fn crop_up() {
    let toggle = with(|st| {
        let Some(d) = st.crop_drag.take() else { return false };
        let full = |r: Rect| r.x == 0 && r.y == 0 && r.w >= st.src_w - 1 && r.h >= st.src_h - 1;
        // Clique parado no miolo: toca/pausa, como clicar no vídeo sempre fez.
        if d.dir == "move" && d.moved < 4.0 {
            st.crop = if full(d.start) { None } else { Some(d.start) };
            return true;
        }
        if st.crop.is_some_and(full) {
            st.crop = None;
        }
        false
    });
    if let Some(w) = window() {
        ex(&w).set_cropping(false);
    }
    render_stage();
    render_resolutions();
    render_scale();
    if toggle {
        toggle_play();
    }
}

fn clamp_pan(st: &mut State) {
    let (fw, fh) = st.frame;
    let mx = (fw * (st.crop_zoom - 1.0) / 2.0).max(0.0);
    let my = (fh * (st.crop_zoom - 1.0) / 2.0).max(0.0);
    st.pan.0 = st.pan.0.clamp(-mx, mx);
    st.pan.1 = st.pan.1.clamp(-my, my);
}

fn set_crop_zoom(z: f32, anchor: Option<(f32, f32, f32, f32)>) {
    with(|st| {
        let next = z.clamp(1.0, 12.0);
        if (next - st.crop_zoom).abs() < 0.0001 {
            return;
        }
        st.crop_zoom = next;
        if let Some((cx, cy, vx, vy)) = anchor {
            st.pan = (cx - vx * next, cy - vy * next);
        }
        if next <= 1.001 {
            st.pan = (0.0, 0.0);
        }
        clamp_pan(st);
    });
    push_view();
    render_stage();
}

fn crop_zoom_by(f: f32) {
    let (z, pan) = with(|st| (st.crop_zoom, st.pan));
    set_crop_zoom(z * f, Some((0.0, 0.0, -pan.0 / z, -pan.1 / z)));
}

fn crop_zoom_reset() {
    with(|st| {
        st.crop_zoom = 1.0;
        st.pan = (0.0, 0.0);
    });
    push_view();
    render_stage();
}

fn stage_wheel(cx: f32, cy: f32, dy: f32) {
    if !with(|st| crop_available(st)) {
        return;
    }
    let (z, pan) = with(|st| (st.crop_zoom, st.pan));
    let (vx, vy) = ((cx - pan.0) / z, (cy - pan.1) / z);
    set_crop_zoom(z * if dy > 0.0 { 1.2 } else { 1.0 / 1.2 }, Some((cx, cy, vx, vy)));
}

fn pan_down(x: f32, y: f32) {
    with(|st| {
        if st.crop_zoom <= 1.001 {
            return;
        }
        st.pan_drag = Some(PanDrag { sx: x, sy: y, px: st.pan.0, py: st.pan.1, moved: 0.0 });
        st.last_pan_moved = 0.0;
    });
    if let Some(w) = window() {
        ex(&w).set_panning(with(|st| st.pan_drag.is_some()));
    }
}

fn pan_move(x: f32, y: f32) {
    let ok = with(|st| {
        let Some(d) = st.pan_drag.as_mut() else { return false };
        d.moved = d.moved.max((x - d.sx).abs() + (y - d.sy).abs());
        st.pan = (d.px + x - d.sx, d.py + y - d.sy);
        clamp_pan(st);
        true
    });
    if ok {
        push_view();
        render_stage();
    }
}

fn pan_up() {
    with(|st| {
        if let Some(d) = st.pan_drag.take() {
            st.last_pan_moved = d.moved;
        }
    });
    if let Some(w) = window() {
        ex(&w).set_panning(false);
    }
}

fn frame_resized(w: f32, h: f32) {
    let changed = with(|st| {
        if (st.frame.0 - w).abs() < 0.5 && (st.frame.1 - h).abs() < 0.5 {
            return false;
        }
        st.frame = (w, h);
        clamp_pan(st);
        st.cap_cache = None;
        true
    });
    if changed {
        push_view();
        render_stage();
        sync_caption();
    }
}

// ---- áudio ----

fn render_audio() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (vis, orig_vis, orig, items, mix_checked, can_mix) = with(|st| {
        let orig = use_original(st);
        let mix = mix_index(st);
        let has_iso = st.sel_audio.iter().any(|&i| i != mix);
        let has_mix = st.sel_audio.contains(&mix);
        let items: Vec<ExChk> = st
            .audio
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let is_mix = s.index == mix;
                let label = if !s.title.is_empty() {
                    s.title.clone()
                } else if is_mix {
                    lang::t_with("export.trackMix", &[("n", &(i + 1).to_string())])
                } else {
                    lang::t_with("export.trackN", &[("n", &(i + 1).to_string())])
                };
                ExChk {
                    idx: i as i32,
                    text: label.into(),
                    checked: orig || st.sel_audio.contains(&s.index),
                    disabled: orig || if is_mix { has_iso } else { has_mix },
                }
            })
            .collect();
        let can_mix = !orig && st.sel_audio.len() > 1;
        if !can_mix {
            st.mix_checked = false;
        }
        (!st.audio.is_empty(), st.audio.len() > 1, orig, items, st.mix_checked, can_mix)
    });
    g.set_audio_visible(vis);
    g.set_orig_visible(orig_vis);
    g.set_orig_checked(orig);
    g.set_track_rows(model(wrap_rows(items)));
    g.set_mix_checked(mix_checked);
    g.set_mix_enabled(can_mix);
    push_volumes();
}

fn track_clicked(i: i32) {
    with(|st| {
        let Some(s) = st.audio.get(i as usize) else { return };
        let idx = s.index;
        if !st.sel_audio.remove(&idx) {
            st.sel_audio.insert(idx);
        }
    });
    render_audio();
}

// ---- vídeo ----

fn render_resolutions() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (opts, idx) = with(|st| {
        let (cw, ch) = composed_size(st);
        let mut hs = vec![0];
        let mut opts = vec![opt(&lang::t_with("export.resOriginal", &[("w", &cw.to_string()), ("h", &ch.to_string())]))];
        for h in [1440, 1080, 720, 480] {
            if h < ch {
                hs.push(h);
                opts.push(opt(&format!("{h}p")));
            }
        }
        if !hs.contains(&st.res_h) {
            st.res_h = 0;
        }
        let idx = hs.iter().position(|&h| h == st.res_h).unwrap_or(0) as i32;
        st.res_heights = hs;
        (opts, idx)
    });
    g.set_res_opts(model(opts));
    g.set_res_idx(idx);
    g.set_video_off(with(|st| st.no_video));
}

fn render_encoders() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (opts, idx) = with(|st| {
        if st.encoders.is_empty() {
            st.enc_id = "auto".into();
            return (vec![opt(&lang::t("settings.codec.auto"))], 0);
        }
        // Padrão: o codec das Configurações, se estiver na lista.
        if st.enc_id.is_empty() || !st.encoders.iter().any(|e| e.id == st.enc_id) {
            let want = config::get_config_str("codec", "auto");
            st.enc_id = if st.encoders.iter().any(|e| e.id == want) { want } else { st.encoders[0].id.clone() };
        }
        let idx = st.encoders.iter().position(|e| e.id == st.enc_id).unwrap_or(0) as i32;
        (st.encoders.iter().map(|e| opt(&enc_label(&e.id))).collect(), idx)
    });
    g.set_enc_opts(model(opts));
    g.set_enc_idx(idx);
}

fn render_scale() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (vis, algo) = with(|st| (!st.id.is_empty() && st.loaded && is_resizing(st), st.scale_algo.clone()));
    g.set_scale_visible(vis);
    let ids = ["bicubic", "bilinear", "area"];
    g.set_scale_opts(model(ids.iter().map(|i| opt(&lang::t(&format!("export.scaleOpt.{i}")))).collect()));
    g.set_scale_idx(ids.iter().position(|i| *i == algo).unwrap_or(0) as i32);
    g.set_scale_hint(lang::t(&format!("export.scaleHint.{algo}")).into());
}

fn render_speed() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (text, invalid) = with(|st| {
        let raw = st.speed_text.trim().replace(',', ".");
        let bad = !raw.is_empty() && raw.parse::<f64>().map_or(true, |v| !v.is_finite() || !(1.0..=SPEED_MAX).contains(&v));
        (st.speed_text.clone(), bad)
    });
    if g.get_speed_text().as_str() != text {
        g.set_speed_text(text.into());
    }
    g.set_speed_invalid(invalid);
    render_tlbar();
}

fn render_fps() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (max, val, src) = with(|st| {
        let max = max_fps(st);
        st.fps_val = if max > 0 { st.fps_val.clamp(20.min(max), max) } else { 0 };
        (max, st.fps_val, st.src_fps)
    });
    g.set_fps_visible(max > 20);
    g.set_fps_min(20);
    g.set_fps_max(max.max(21));
    g.set_fps_val(val);
    let shown = if max > 20 { val } else { src.round() as i32 };
    g.set_fps_text(lang::t_with("export.fpsValue", &[("fps", &shown.to_string())]).into());
}

fn render_quality() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (pos, c, svt) = with(|st| (st.q_pos, crf(st), st.enc_id == "av1-sw"));
    g.set_q_pos(pos);
    g.set_q_text(lang::t_with("export.qualityValue", &[("crf", &c.to_string())]).into());
    let mut hint = lang::t_with("export.qualityHint", &[("band", &lang::t(crf_band_key(c)))]);
    let warn = svt && c >= SVT_FLOOR_CRF;
    if warn {
        hint = format!("{} {}", lang::t_with("export.qualitySvtFloor", &[("crf", &SVT_FLOOR_CRF.to_string())]), hint);
    }
    g.set_q_hint(hint.into());
    g.set_q_warn(warn);
}

fn render_captions() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (has, loading, transcribed, no_video) = with(|st| {
        let has = st.turns.as_ref().is_some_and(|t| !t.is_empty());
        if !(has && !st.no_video) {
            st.burn = false;
        }
        (has, st.turns.is_none(), st.transcribed, st.no_video)
    });
    let can = has && !no_video;
    g.set_burn_enabled(can);
    g.set_burn_checked(with(|st| st.burn));
    g.set_cap_hint(
        if loading {
            lang::t("export.captionsLoading")
        } else if has {
            lang::t("export.captionsHint")
        } else if transcribed {
            lang::t("player.transcriptNoSpeech")
        } else {
            lang::t("export.captionsNeedTranscript")
        }
        .into(),
    );
    render_playbar();
}

fn render_container() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let c = with(|st| st.container.clone());
    if c == "mp3" {
        g.set_cont_opts(model(vec![opt("MP3")]));
        g.set_cont_idx(0);
        g.set_cont_enabled(false);
    } else {
        g.set_cont_opts(model(vec![opt("MP4"), opt("MKV")]));
        g.set_cont_idx(if c == "mkv" { 1 } else { 0 });
        g.set_cont_enabled(true);
    }
    g.set_cont_hint(
        lang::t(match c.as_str() {
            "mkv" => "export.containerHintMkv",
            "mp3" => "export.containerHintMp3",
            _ => "export.containerHintMp4",
        })
        .into(),
    );
}

fn render_actions() {
    let Some(w) = window() else { return };
    let g = ex(&w);
    let (running, canceling, previewing, ppct, pct, loaded) = with(|st| (st.running, st.canceling, st.previewing, st.preview_pct, st.pct, st.loaded));
    g.set_running(running);
    g.set_run_text(if running { lang::t("common.cancel") } else { lang::t("export.action") }.into());
    g.set_run_enabled(loaded && !canceling && (running || !previewing));
    g.set_prev_text(
        if previewing { lang::t_with("export.preview.generating", &[("pct", &(ppct.round() as i32).to_string())]) } else { lang::t("export.preview.button") }
            .into(),
    );
    g.set_prev_enabled(loaded && !running);
    g.set_prevsec_enabled(!previewing);
    g.set_progress((pct / 100.0) as f32);
    g.set_progress_text(if canceling { lang::t("export.canceling") } else { lang::t_with("export.progress", &[("pct", &(pct.round() as i32).to_string())]) }.into());
}

// ---------------------------------------------------------------------------
// Execução
// ---------------------------------------------------------------------------

/// O pedido, do jeito que a tela está agora (`_buildMsg`). `None` = nada a exportar.
fn build_request(name: &str) -> Option<ExportRequest> {
    let r = with(|st| {
        if st.id.is_empty() || !st.loaded {
            return Err(false);
        }
        let segs = kept_segments(st);
        if segs.is_empty() || kept_duration(st) <= 0.0 {
            return Err(false);
        }
        if st.no_video && audio_selection(st).is_empty() {
            return Err(true);
        }
        let orig = use_original(st);
        Ok(ExportRequest {
            id: st.id.clone(),
            preview: None,
            no_video: st.no_video,
            segments: segs.iter().map(|s| ((s.0 * 1000.0).round() as i32, (s.1 * 1000.0).round() as i32)).collect(),
            target_height: st.res_h,
            fps: target_fps(st),
            speed: speed(st),
            mix_audio: !orig && st.mix_checked,
            crf: crf(st),
            scale_algo: if is_resizing(st) { st.scale_algo.clone() } else { "bicubic".into() },
            encoder: if st.enc_id.is_empty() { "auto".into() } else { st.enc_id.clone() },
            regions: if st.no_video { Vec::new() } else { st.sel_regions.iter().map(|&i| i as i32).collect() },
            crop: if !st.no_video && has_crop(st) { st.crop.map(|c| (c.x, c.y, c.w, c.h)) } else { None },
            audio_streams: audio_selection(st),
            mix_track_index: mix_index(st) as i32,
            original_audio: orig,
            burn_captions: !st.no_video && st.burn && st.turns.as_ref().is_some_and(|t| !t.is_empty()),
            keep_transcript: audio_is_complete(st),
            container: st.container.clone(),
            name: name.to_owned(),
        })
    });
    match r {
        Ok(req) => Some(req),
        Err(true) => {
            crate::app_view::show_toast(&lang::t("toast.errorTitle"), &lang::t("error.exportNothing"), true, 5000);
            None
        }
        Err(false) => None,
    }
}

fn pause_preview() {
    with(|st| {
        if let Some(pb) = &st.pb {
            if st.playing {
                pb.pause();
                st.play_sec = pb.shared.position();
            }
        }
        st.playing = false;
    });
    render_playbar();
}

fn run_or_cancel() {
    if with(|st| st.running) {
        cancel();
        return;
    }
    if with(|st| st.previewing) {
        return;
    }
    let Some(w) = window() else { return };
    let name = ex(&w).get_name().to_string();
    let Some(req) = build_request(&name) else { return };
    pause_preview();
    with(|st| {
        st.running = true;
        st.running_id = st.id.clone();
        st.pct = 0.0;
        st.canceling = false;
    });
    render_actions();
    sync_card();
    bexp::handle_export(req);
}

/// Cancelar (botão da tela ou o × do card).
pub fn cancel() {
    let go = with(|st| {
        if !st.running || st.canceling {
            return false;
        }
        st.canceling = true;
        true
    });
    if go {
        bexp::handle_cancel_export();
        render_actions();
        sync_card();
    }
}

fn preview_action() {
    if with(|st| st.previewing) {
        bexp::handle_cancel_export();
        if let Some(w) = window() {
            ex(&w).set_prev_text(lang::t("export.canceling").into());
            ex(&w).set_prev_enabled(false);
        }
        return;
    }
    if with(|st| st.running) {
        return;
    }
    let Some(w) = window() else { return };
    let name = ex(&w).get_name().to_string();
    let Some(mut req) = build_request(&name) else { return };
    let (from, sec) = with(|st| (st.play_sec, PREVIEW_SECS[st.prevsec.min(2)]));
    req.preview = Some((from, sec));
    pause_preview();
    with(|st| {
        st.previewing = true;
        st.preview_pct = 0.0;
    });
    render_actions();
    bexp::handle_export(req);
}

pub fn on_progress(pct: f64, source: &str) {
    with(|st| {
        // Exportação adotada (a tela pode ter sido reaberta noutro estado).
        if !st.running {
            st.running = true;
        }
        if !source.is_empty() {
            st.running_id = source.to_owned();
        }
        st.pct = pct.clamp(0.0, 100.0);
    });
    render_actions();
    sync_card();
}

pub fn on_done(ok: bool, canceled: bool, _path: &str) {
    with(|st| {
        st.running = false;
        st.running_id.clear();
        st.pct = 0.0;
        st.canceling = false;
    });
    sync_card();
    if ok {
        if is_open() {
            close();
        }
        crate::app_library::clear_selection();
        crate::app_view::show_toast(&lang::t("toast.exportDone"), &lang::t("toast.exportDoneMsg"), false, 6000);
    } else if canceled {
        crate::app_view::show_toast(&lang::t("toast.exportCanceled"), &lang::t("toast.exportCanceledMsg"), false, 4000);
    }
    if is_open() {
        render_actions();
    }
}

pub fn on_preview_progress(pct: f64) {
    with(|st| st.preview_pct = pct);
    if with(|st| st.previewing) {
        render_actions();
    }
}

pub fn on_preview_done(ok: bool, canceled: bool, path: &str) {
    with(|st| st.previewing = false);
    render_actions();
    if ok && !path.is_empty() {
        crate::app_player::open(path);
    } else if canceled {
        crate::app_view::show_toast(&lang::t("export.preview.canceled"), "", false, 3000);
    }
}

/// Exportação desta gravação em curso? (bloqueia excluir/mover/renomear na UI).
pub fn running_id() -> String {
    with(|st| if st.running { st.running_id.clone() } else { String::new() })
}

/// Barra de progresso no card da gravação de origem (`attachCardProgress`).
fn sync_card() {
    let Some(w) = window() else { return };
    let g = w.global::<Lib>();
    let (id, pct, canceling) = with(|st| (if st.running { st.running_id.clone() } else { String::new() }, st.pct, st.canceling));
    g.set_export_id(id.into());
    g.set_export_pct((pct / 100.0) as f32);
    g.set_export_canceling(canceling);
    g.set_export_label(
        if canceling { lang::t("export.canceling") } else { lang::t_with("recordings.exporting", &[("pct", &(pct.round() as i32).to_string())]) }
            .into(),
    );
    crate::app_library::sync_export_button();
}

// ---------------------------------------------------------------------------
// Teclado
// ---------------------------------------------------------------------------

fn key(text: &str, ctrl: bool) -> bool {
    if !is_open() {
        return false;
    }
    if text == "\u{1b}" {
        close();
        return true;
    }
    if ctrl {
        let ps = with(|st| st.play_sec);
        let z = with(|st| st.zoom);
        match text {
            "+" | "=" => set_zoom(z * 1.6, Some(ps)),
            "-" | "_" => set_zoom(z / 1.6, Some(ps)),
            "0" => {
                set_zoom(1.0, Some(0.0));
                with(|st| st.scroll = 0.0);
                render_timeline();
            }
            _ => return false,
        }
        return true;
    }
    if text == " " {
        toggle_play();
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Ligação
// ---------------------------------------------------------------------------

pub fn setup(w: &AppWindow) {
    let g = ex(w);
    g.on_close(close);
    g.on_region(region_clicked);
    g.on_frame_resized(frame_resized);
    g.on_crop_down(|d, x, y| crop_down(d.as_str(), x, y));
    g.on_crop_move(crop_move);
    g.on_crop_up(crop_up);
    g.on_pan_down(pan_down);
    g.on_pan_move(pan_move);
    g.on_pan_up(pan_up);
    g.on_stage_wheel(stage_wheel);
    g.on_stage_click(stage_click);
    g.on_crop_reset(|| {
        with(|st| st.crop = None);
        render_stage();
        render_resolutions();
        render_scale();
    });
    g.on_crop_zoom(crop_zoom_by);
    g.on_crop_zoom_reset(crop_zoom_reset);
    g.on_toggle_play(toggle_play);
    g.on_toggle_mute(toggle_mute);
    g.on_volume_changed(set_volume);
    g.on_toggle_cc(toggle_cc);
    g.on_tl_view(|v| {
        with(|st| {
            st.view_w = v as f64;
            clamp_scroll(st);
        });
        render_timeline();
    });
    g.on_tl_seek(tl_seek);
    g.on_tl_wheel(tl_wheel);
    g.on_tl_scroll(tl_scroll);
    g.on_cut(cut_here);
    g.on_uncut(remove_cut);
    g.on_toggle_part(toggle_part);
    g.on_tl_zoom(|f| {
        let z = with(|st| st.zoom);
        set_zoom(z * f as f64, None);
    });
    g.on_time_commit(|t| match parse_time(t.as_str()) {
        Some(v) => set_play_sec(v, true),
        None => render_tlbar(),
    });
    g.on_orig(|on| {
        with(|st| st.original_audio = on);
        render_audio();
    });
    g.on_track(track_clicked);
    g.on_mix(|on| {
        with(|st| st.mix_checked = on);
        render_audio();
    });
    g.on_res(|i| {
        with(|st| st.res_h = st.res_heights.get(i as usize).copied().unwrap_or(0));
        render_resolutions();
        render_scale();
    });
    g.on_enc(|i| {
        with(|st| {
            if let Some(e) = st.encoders.get(i as usize) {
                st.enc_id = e.id.clone();
            }
        });
        render_encoders();
        render_fps();
        render_quality();
    });
    g.on_scale(|i| {
        with(|st| st.scale_algo = ["bicubic", "bilinear", "area"].get(i as usize).copied().unwrap_or("bicubic").into());
        render_scale();
    });
    g.on_speed_edit(|t| {
        with(|st| {
            st.speed_text = t.to_string();
            // A taxa acompanha a velocidade: o seletor vai pro teto.
            st.fps_val = max_fps(st);
        });
        render_speed();
        render_fps();
    });
    g.on_speed_commit(|_| {
        with(|st| st.speed_text = fmt_speed(speed(st)));
        render_speed();
        render_fps();
    });
    g.on_speed_step(|dir| {
        with(|st| {
            let cur = speed(st);
            let next = if dir > 0 {
                SPEED_STEPS.iter().copied().find(|&s| s > cur + 1e-9).unwrap_or(SPEED_MAX)
            } else {
                SPEED_STEPS.iter().rev().copied().find(|&s| s < cur - 1e-9).unwrap_or(1.0)
            };
            st.speed_text = fmt_speed(next);
            st.fps_val = max_fps(st);
        });
        render_speed();
        render_fps();
    });
    g.on_fps(|v| {
        with(|st| st.fps_val = v);
        render_fps();
    });
    g.on_quality(|v| {
        with(|st| st.q_pos = v.clamp(0, CRF_MAX));
        render_quality();
    });
    g.on_burn(|on| {
        with(|st| {
            st.burn = on;
            // Quem vai gravar a legenda quer ver como ela sai.
            if on && !st.captions_on {
                st.captions_on = true;
            }
        });
        if on {
            config::set_config_bool("exportCaptions", true);
        }
        render_captions();
        sync_caption();
    });
    g.on_cont(|i| {
        with(|st| {
            if st.container != "mp3" {
                st.container = if i == 1 { "mkv".into() } else { "mp4".into() };
                st.video_container = st.container.clone();
            }
        });
        render_container();
    });
    g.on_run(run_or_cancel);
    g.on_preview(preview_action);
    g.on_prevsec(|i| with(|st| st.prevsec = i.clamp(0, 2) as usize));
    g.on_key(|t, ctrl| key(t.as_str(), ctrl));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_como_a_web() {
        assert_eq!(fmt_time(0.0), "00:00");
        assert_eq!(fmt_time(199.5), "03:20");
        assert_eq!(fmt_time(3725.0), "1:02:05");
        assert_eq!(parse_time("1:30"), Some(90.0));
        assert_eq!(parse_time("x"), None);
    }

    #[test]
    fn trechos_vizinhos_se_fundem() {
        let st = State {
            parts: vec![
                Part { start: 0.0, end: 5.0, keep: true },
                Part { start: 5.0, end: 9.0, keep: true },
                Part { start: 9.0, end: 12.0, keep: false },
                Part { start: 12.0, end: 20.0, keep: true },
            ],
            ..Default::default()
        };
        assert_eq!(kept_segments(&st), vec![(0.0, 9.0), (12.0, 20.0)]);
        assert!((kept_duration(&st) - 17.0).abs() < 1e-9);
    }

    #[test]
    fn crf_corre_ao_contrario() {
        let mut st = State::default();
        st.q_pos = 28;
        assert_eq!(crf(&st), 23);
        st.q_pos = 51;
        assert_eq!(crf(&st), 0);
        assert_eq!(crf_band_key(23), "export.crfBandHigh");
    }

    #[test]
    fn velocidade_aceita_virgula_e_clampa() {
        let mut st = State { speed_text: "1,5".into(), ..Default::default() };
        assert_eq!(speed(&st), 1.5);
        st.speed_text = "9999".into();
        assert_eq!(speed(&st), 512.0);
        st.speed_text = "0.5".into();
        assert_eq!(speed(&st), 1.0);
    }
}

//! Presenter da tela de exportação: telas, prévia com recorte e zoom, linha do
//! tempo com cortes e forma de onda, faixas de áudio, opções de vídeo,
//! nome/formato, a exportação e a prévia gerada, e a barra de progresso no
//! cartão da gravação de origem.
//!
//! A casca é a camada `pnlExport` do `forms/main.json` (cabeçalho e a área
//! rolável `lyExport`); o formulário é montado aqui ao abrir, numa coluna de até
//! 880 px. A prévia é o motor do player (`playback`) aberto PARADO na
//! gravação, num `FrameView`, com o `CropBox` do iron por cima; a linha do
//! tempo (trechos + forma de onda) é desenhada aqui numa imagem da janela
//! visível, com as divisões, o cursor e a régua por cima.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use iron::prelude::*;

use crate::app_view::{main_form, with_ui};
use crate::bridge::export::{self as bexp, ExportRequest};
use crate::playback::{Out, Playback, Region, ViewSpec};
use crate::view::{ExportEncoderInfo, StreamInfo, Transcript, Turn, VideoInfo, VideoLayout};
use crate::{bridge, config, lang, log};

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
/// Altura da faixa dos trechos, da régua e da barra de rolagem.
const TL_H: f32 = 46.0;
const RULER_H: f32 = 18.0;
const SB_H: f32 = 8.0;
const TICK_STEPS: [f64; 13] = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0];
/// Largura máxima da coluna do formulário.
const COL_MAX: f32 = 880.0;
const SCALES: [&str; 3] = ["bicubic", "bilinear", "area"];

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
    ready: bool,
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
    crop_zoom: f32,
    pan: (f32, f32),
    pan_drag: Option<PanDrag>,
    sel_audio: BTreeSet<i64>,
    original_audio: bool,
    mix_checked: bool,
    parts: Vec<Part>,
    play_sec: f64,
    zoom: f64,
    scroll: f64,
    view_w: f64,
    cursor_part: i32,
    tl_drag: bool,
    sb_drag: Option<(f32, f64)>,
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
    /// O palco (o quadro sem zoom) na tela: posição dentro do painel e tamanho.
    frame_at: (f32, f32),
    frame: (f32, f32),
    scale: f32,
    pb: Option<Playback>,
    playing: bool,
    user_muted: bool,
    user_vol: i32,
    timer: Option<u64>,
    /// Componentes da tela montada (nome → id).
    ids: HashMap<&'static str, ComponentId>,
    /// Dicas quebradas: refeitas quando a largura muda.
    hints: Vec<ComponentId>,
    /// Divisões, régua e rótulos dos trechos (refeitos a cada desenho).
    tl_marks: Vec<ComponentId>,
    col_w: f32,
    // ---- exportação em curso (vive fora da tela: ela pode ser fechada) ----
    running: bool,
    running_id: String,
    pct: f64,
    canceling: bool,
    previewing: bool,
    preview_pct: f64,
}

fn fresh(id: &str, encoders: Vec<ExportEncoderInfo>, keep: &State) -> State {
    State {
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
        original_audio: true,
        captions_on: config::get_config_bool("exportCaptions", false),
        encoders,
        running: keep.running,
        running_id: keep.running_id.clone(),
        pct: keep.pct,
        canceling: keep.canceling,
        scale: 1.0,
        ..Default::default()
    }
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State { crop_zoom: 1.0, zoom: 1.0, scale: 1.0, ..Default::default() });
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

/// Um componente da tela montada.
fn id(key: &'static str) -> Option<ComponentId> {
    with(|st| st.ids.get(key).copied())
}

fn put(ui: &mut Ui, key: &'static str, prop: &str, value: impl Into<PropValue>) {
    if let Some(i) = id(key).filter(|i| ui.contains(*i)) {
        let _ = ui.set_prop(i, prop, value);
    }
}

fn show(ui: &mut Ui, key: &'static str, on: bool) {
    put(ui, key, "visible", on);
}

fn tw(key: &str, args: &[(&str, &str)]) -> String {
    lang::t_with(key, args)
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
pub fn open_for(ui: &mut Ui, rec: &str) {
    if rec.is_empty() {
        return;
    }
    // Uma de cada vez (o backend também recusa a segunda).
    if with(|st| st.running) || bexp::export_busy() {
        crate::app_view::show_toast(&lang::t("toast.exportBusy"), &lang::t("toast.exportBusyMsg"), true, 5000);
        return;
    }
    if !bridge::check_recording_for_player(rec) {
        return;
    }
    close_engine(ui);
    with(|st| {
        let enc = std::mem::take(&mut st.encoders);
        let next = fresh(rec, enc, st);
        *st = next;
        st.scale = crate::app_player::window_scale();
    });
    set(ui, "pnlExport", "visible", true);
    set(ui, "lblExTitle", "text", lang::t("export.title"));
    build(ui);
    let base = std::path::Path::new(rec).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let base = if base.is_empty() { lang::t("export.defaultName") } else { base };
    put(ui, "name", "text", format!("{base} - {}", lang::t("export.nameSuffix")));
    put(ui, "msg", "text", lang::t("export.previewLoading"));
    show(ui, "msg", true);
    if let Some(sb) = c(ui, "scrExport").and_then(|i| ui.control_mut(i)) {
        sb.set_scroll_offset(PointF::ZERO);
    }
    render_all(ui);

    bridge::handle_request_video_info(rec);
    bridge::handle_request_transcript(rec);

    // Prévia: o motor do player, parado no começo.
    let view = with(|st| view_spec(st));
    let id_out = rec.to_owned();
    let out: Arc<dyn Fn(Out) + Send + Sync> = crate::app_view::frame_sink(id_out, on_out);
    let pb = Playback::open_paused(rec, view, out);
    with(|st| st.pb = Some(pb));
    push_volumes();
    tick_later(ui);
    log!("Export: tela aberta para {rec}");
}

fn tick_later(ui: &mut Ui) {
    let job = ui.set_timeout(33, |ui| {
        if !is_open() {
            return;
        }
        tick(ui);
        tick_later(ui);
    });
    with(|st| st.timer = Some(job));
}

fn close_engine(ui: &mut Ui) {
    let (pb, job) = with(|st| (st.pb.take(), st.timer.take()));
    if let Some(job) = job {
        ui.clear_timeout(job);
    }
    drop(pb);
}

pub fn close(ui: &mut Ui) {
    // A prévia gerada é desta tela: fechar a cancela (a exportação de verdade não).
    if with(|st| st.previewing) {
        bexp::handle_cancel_export();
        with(|st| st.previewing = false);
    }
    close_engine(ui);
    with(|st| {
        st.id.clear();
        st.crop = None;
        st.pan_drag = None;
    });
    set(ui, "pnlExport", "visible", false);
    if let Some(p) = c(ui, "lyExport") {
        for child in ui.children(p).to_vec() {
            ui.free(child);
        }
    }
    with(|st| {
        st.ids.clear();
        st.hints.clear();
        st.tl_marks.clear();
    });
    sync_card();
}

// ---------------------------------------------------------------------------
// Montagem do formulário
// ---------------------------------------------------------------------------

/// A largura da coluna do formulário (até 880 px, centrada).
fn col_width(ui: &Ui) -> f32 {
    let w = c(ui, "lyExport").and_then(|p| ui.bounds(p)).map_or(900.0, |b| b.width);
    (w - 36.0).min(COL_MAX).max(320.0)
}

fn wrapped_height(ui: &Ui, l: ComponentId, text: &str, w: f32) -> f32 {
    let style = TextStyle { font: ui.font_of(l), wrap: true, ..TextStyle::default() };
    ui.measure_text_wrapped(text, &style, w.max(40.0)).height.ceil() + 2.0
}

fn reg(key: &'static str, id: ComponentId) {
    with(|st| st.ids.insert(key, id));
}

fn create(ui: &mut Ui, class: &str, parent: ComponentId) -> Option<ComponentId> {
    ui.create(class, Some(parent)).ok()
}

fn row(ui: &mut Ui, parent: ComponentId, h: f32, top: f32) -> Option<ComponentId> {
    let r = create(ui, "Layout", parent)?;
    if let Some(cb) = ui.control_mut(r) {
        cb.set_align(Align::Top).set_height(h).set_margins([0.0, top, 0.0, 0.0]);
    }
    Some(r)
}

fn label(ui: &mut Ui, parent: ComponentId, text: &str, style: &str, align: Align, w: f32, h: f32) -> Option<ComponentId> {
    let l = create(ui, "Label", parent)?;
    let _ = ui.set_prop(l, "style", style);
    let _ = ui.set_prop(l, "text", text);
    if let Some(cb) = ui.control_mut(l) {
        cb.set_align(align).set_height(h);
        if w > 0.0 {
            cb.set_width(w);
        }
    }
    Some(l)
}

/// O título de uma seção.
fn heading(ui: &mut Ui, page: ComponentId, text: &str) -> Option<ComponentId> {
    let l = label(ui, page, text, "row", Align::Top, 0.0, 20.0)?;
    if let Some(cb) = ui.control_mut(l) {
        cb.set_margins([0.0, 22.0, 0.0, 8.0]);
    }
    Some(l)
}

/// Uma dica quebrada na largura da coluna.
fn hint(ui: &mut Ui, page: ComponentId, key: &'static str, text: &str) -> Option<ComponentId> {
    let l = label(ui, page, text, "sub", Align::Top, 0.0, 16.0)?;
    let _ = ui.set_prop(l, "wrap", true);
    let _ = ui.set_prop(l, "v_align", "leading");
    if let Some(cb) = ui.control_mut(l) {
        cb.set_margins([0.0, 6.0, 0.0, 0.0]);
    }
    reg(key, l);
    with(|st| st.hints.push(l));
    Some(l)
}

fn button(ui: &mut Ui, parent: ComponentId, text: &str, icon: &str, style: &str, f: impl Fn(&mut Ui) + 'static) -> Option<ComponentId> {
    let b = create(ui, "Button", parent)?;
    let _ = ui.set_prop(b, "text", text);
    if !icon.is_empty() {
        let _ = ui.set_prop(b, "icon", icon);
        let _ = ui.set_prop(b, "icon_size", 14.0);
    }
    if !style.is_empty() {
        let _ = ui.set_prop(b, "style", style);
    }
    let w = if text.is_empty() { 32.0 } else { ui.measure_text(text, &TextStyle::default()).width + if icon.is_empty() { 28.0 } else { 48.0 } };
    if let Some(cb) = ui.control_mut(b) {
        cb.set_align(Align::Left).set_width(w).set_margins([0.0, 0.0, 6.0, 0.0]);
    }
    ui.on(b, "on_click", move |ui, _, _| f(ui));
    Some(b)
}

fn combo(ui: &mut Ui, parent: ComponentId, w: f32, f: impl Fn(&mut Ui, usize) + 'static) -> Option<ComponentId> {
    let b = create(ui, "ComboBox", parent)?;
    if let Some(cb) = ui.control_mut(b) {
        cb.set_align(if w > 0.0 { Align::Left } else { Align::Client }).set_margins([0.0, 0.0, 8.0, 0.0]);
        if w > 0.0 {
            cb.set_width(w);
        }
    }
    ui.on(b, "on_change", move |ui, sender, _| {
        let i = ui.get_prop(sender, "item_index").ok().and_then(|v| v.to_i64().ok()).unwrap_or(0).max(0) as usize;
        f(ui, i);
    });
    Some(b)
}

fn set_items(ui: &mut Ui, key: &'static str, items: &[String], index: usize) {
    let Some(b) = id(key) else { return };
    let joined = items.iter().map(|s| s.replace(';', ",")).collect::<Vec<_>>().join(";");
    if !matches!(ui.get_prop(b, "items"), Ok(PropValue::String(s)) if s == joined) {
        let _ = ui.set_prop(b, "items", joined);
    }
    if !matches!(ui.get_prop(b, "item_index").ok().and_then(|v| v.to_i64().ok()), Some(i) if i == index as i64) {
        let _ = ui.set_prop(b, "item_index", index as i64);
    }
}

fn check(ui: &mut Ui, parent: ComponentId, text: &str, f: impl Fn(&mut Ui, bool) + 'static) -> Option<ComponentId> {
    let b = create(ui, "CheckBox", parent)?;
    let _ = ui.set_prop(b, "text", text);
    if let Some(cb) = ui.control_mut(b) {
        cb.set_align(Align::Top).set_height(26.0);
    }
    ui.on(b, "on_change", move |ui, sender, _| {
        let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
        f(ui, on);
    });
    Some(b)
}

fn track(ui: &mut Ui, parent: ComponentId, min: f64, max: f64, f: impl Fn(&mut Ui, f64) + 'static) -> Option<ComponentId> {
    let t = create(ui, "TrackBar", parent)?;
    let _ = ui.set_prop(t, "min", min);
    let _ = ui.set_prop(t, "max", max);
    let _ = ui.set_prop(t, "frequency", 1.0);
    if let Some(cb) = ui.control_mut(t) {
        cb.set_align(Align::Client);
    }
    ui.on(t, "on_change", move |ui, sender, _| {
        let v = ui.get_prop(sender, "value").ok().and_then(|v| v.to_f64().ok()).unwrap_or(0.0);
        f(ui, v.round());
    });
    Some(t)
}

/// Monta o formulário inteiro dentro de `lyExport`.
fn build(ui: &mut Ui) {
    let Some(page) = c(ui, "lyExport") else { return };
    for child in ui.children(page).to_vec() {
        ui.free(child);
    }
    with(|st| {
        st.ids.clear();
        st.hints.clear();
        st.tl_marks.clear();
    });
    let col = col_width(ui);
    with(|st| {
        st.col_w = col;
        st.view_w = col as f64;
    });
    center_column(ui);

    // ---- telas ----
    heading(ui, page, &lang::t("export.regions"));
    if let Some(f) = create(ui, "FlowLayout", page) {
        let _ = ui.set_prop(f, "gap", 6.0);
        let _ = ui.set_prop(f, "line_gap", 6.0);
        if let Some(cb) = ui.control_mut(f) {
            cb.set_align(Align::Top).set_height(30.0);
        }
        reg("regions", f);
    }
    hint(ui, page, "regions_hint", &lang::t("export.regionsHint"));

    // ---- trechos: a prévia ----
    heading(ui, page, &lang::t("export.range"));
    if let Some(stage) = create(ui, "Panel", page) {
        let _ = ui.set_prop(stage, "style", "player");
        if let Some(cb) = ui.control_mut(stage) {
            cb.set_align(Align::Top).set_height(300.0).set_clip_children(true);
        }
        reg("stage", stage);
        if let Some(fv) = create(ui, "FrameView", stage) {
            reg("frame", fv);
            ui.on(fv, "on_pointer_down", |ui, _, args| {
                if let EventArgs::Pointer(e) = args {
                    pan_down(ui, e.position.x, e.position.y);
                }
            });
            ui.on(fv, "on_pointer_move", |ui, _, args| {
                if let EventArgs::Pointer(e) = args {
                    pan_move(ui, e.position.x, e.position.y);
                }
            });
            ui.on(fv, "on_pointer_up", |ui, _, _| pan_up(ui));
            ui.on(fv, "on_wheel", |ui, sender, args| {
                if let EventArgs::Wheel(e) = args {
                    stage_wheel(ui, sender, e.position, e.delta.y);
                }
            });
        }
        if let Some(cr) = create(ui, "CropBox", stage) {
            let _ = ui.set_prop(cr, "min_size", CROP_MIN as f64);
            let _ = ui.set_prop(cr, "snap", 2.0);
            if let Some(cb) = ui.control_mut(cr) {
                cb.set_visible(false);
            }
            reg("crop", cr);
            ui.on(cr, "on_change", |ui, sender, _| crop_changed(ui, sender, false));
            ui.on(cr, "on_end", |ui, sender, _| crop_changed(ui, sender, true));
            ui.on(cr, "on_tap", |ui, _, _| toggle_play(ui));
            ui.on(cr, "on_wheel", |ui, sender, args| {
                if let EventArgs::Wheel(e) = args {
                    stage_wheel(ui, sender, e.position, e.delta.y);
                }
            });
        }
        if let Some(l) = label(ui, stage, "", "pl_time", Align::Center, 420.0, 40.0) {
            let _ = ui.set_prop(l, "h_align", "center");
            let _ = ui.set_prop(l, "hit_test", false);
            reg("msg", l);
        }
        if let Some(l) = label(ui, stage, &lang::t("export.audioOnlyBadge"), "pl_badge", Align::Center, 300.0, 34.0) {
            let _ = ui.set_prop(l, "h_align", "center");
            let _ = ui.set_prop(l, "hit_test", false);
            let _ = ui.set_prop(l, "visible", false);
            reg("audio_only", l);
        }
        if let Some(l) = label(ui, stage, "", "pl_caption", Align::None, 0.0, 40.0) {
            let _ = ui.set_prop(l, "h_align", "center");
            let _ = ui.set_prop(l, "wrap", true);
            let _ = ui.set_prop(l, "hit_test", false);
            let _ = ui.set_prop(l, "visible", false);
            reg("caption", l);
        }
    }
    // Recorte e zoom da prévia.
    if let Some(r) = row(ui, page, 30.0, 8.0) {
        if let Some(b) = button(ui, r, &lang::t("export.cropReset"), "crop", "outline", |ui| {
            with(|st| st.crop = None);
            render_stage(ui);
            render_resolutions(ui);
            render_scale(ui);
        }) {
            reg("crop_reset", b);
        }
        if let Some(b) = button(ui, r, "", "minus", "tool", |ui| crop_zoom_by(ui, 1.0 / 1.25)) {
            let _ = ui.set_prop(b, "hint", lang::t("export.zoomOut"));
            reg("zoom_out", b);
        }
        if let Some(l) = label(ui, r, "100%", "key", Align::Left, 52.0, 30.0) {
            let _ = ui.set_prop(l, "h_align", "center");
            reg("zoom_badge", l);
        }
        if let Some(b) = button(ui, r, "", "plus", "tool", |ui| crop_zoom_by(ui, 1.25)) {
            let _ = ui.set_prop(b, "hint", lang::t("export.zoomIn"));
            reg("zoom_in", b);
        }
        if let Some(b) = button(ui, r, &lang::t("export.zoomReset"), "", "flat", crop_zoom_reset) {
            reg("zoom_reset", b);
        }
        if let Some(l) = label(ui, r, "", "key", Align::Client, 0.0, 30.0) {
            let _ = ui.set_prop(l, "h_align", "trailing");
            reg("crop_info", l);
        }
    }
    hint(ui, page, "crop_hint", &lang::t("export.cropHint"));
    // Tocar, volume, legenda.
    if let Some(r) = row(ui, page, 32.0, 10.0) {
        if let Some(b) = button(ui, r, "", "play", "tool", toggle_play) {
            reg("play", b);
        }
        if let Some(b) = button(ui, r, "", "volume", "tool", toggle_mute) {
            reg("mute", b);
        }
        if let Some(t) = track(ui, r, 0.0, 100.0, |ui, v| set_volume(ui, v as i32)) {
            if let Some(cb) = ui.control_mut(t) {
                cb.set_align(Align::Left).set_width(110.0).set_margins([0.0, 0.0, 10.0, 0.0]);
            }
            let _ = ui.set_prop(t, "value", 100.0);
            reg("volume", t);
        }
        if let Some(b) = button(ui, r, "CC", "", "tool", toggle_cc) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_width(40.0);
            }
            reg("cc", b);
        }
        label(ui, r, &lang::t("export.playHint"), "sub", Align::Client, 0.0, 32.0).map(|l| ui.set_prop(l, "h_align", "trailing"));
    }
    // A barra da linha do tempo: dividir, remover divisão, tirar trecho, tempo, zoom.
    if let Some(r) = row(ui, page, 30.0, 12.0) {
        if let Some(b) = button(ui, r, &lang::t("export.cutHere"), "scissors", "outline", cut_here) {
            reg("cut", b);
        }
        if let Some(b) = button(ui, r, &lang::t("export.removeCut"), "", "outline", remove_cut) {
            reg("uncut", b);
        }
        if let Some(b) = button(ui, r, &lang::t("export.dropPart"), "", "outline", toggle_part) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_width(130.0);
            }
            reg("toggle", b);
        }
        if let Some(b) = button(ui, r, "", "plus", "tool", |ui| {
            let z = with(|st| st.zoom);
            set_zoom(ui, z * 1.6, None);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Right).set_margins([6.0, 0.0, 0.0, 0.0]);
            }
            let _ = ui.set_prop(b, "hint", lang::t("export.zoomIn"));
        }
        if let Some(l) = label(ui, r, "1.0×", "key", Align::Right, 48.0, 30.0) {
            let _ = ui.set_prop(l, "h_align", "center");
            reg("tl_zoom", l);
        }
        if let Some(b) = button(ui, r, "", "minus", "tool", |ui| {
            let z = with(|st| st.zoom);
            set_zoom(ui, z / 1.6, None);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Right).set_margins([6.0, 0.0, 0.0, 0.0]);
            }
            let _ = ui.set_prop(b, "hint", lang::t("export.zoomOut"));
        }
        if let Some(e) = create(ui, "Edit", r) {
            let _ = ui.set_prop(e, "text", "00:00");
            if let Some(cb) = ui.control_mut(e) {
                cb.set_align(Align::Left).set_width(80.0).set_margins([10.0, 0.0, 4.0, 0.0]);
            }
            ui.on(e, "on_exit", |ui, sender, _| time_commit(ui, sender));
            ui.on(e, "on_key_down", |ui, sender, args| {
                if let EventArgs::Key(k) = args {
                    if k.key == Key::Enter {
                        time_commit(ui, sender);
                    }
                }
            });
            reg("time", e);
        }
        if let Some(l) = label(ui, r, "", "key", Align::Client, 0.0, 30.0) {
            reg("range_text", l);
        }
    }
    // A linha do tempo.
    if let Some(tl) = row(ui, page, TL_H + RULER_H + SB_H, 8.0) {
        if let Some(cb) = ui.control_mut(tl) {
            cb.set_clip_children(true);
        }
        reg("tl", tl);
        if let Some(img) = create(ui, "Image", tl) {
            let _ = ui.set_prop(img, "fit", "stretch");
            if let Some(cb) = ui.control_mut(img) {
                cb.set_rect(0.0, 0.0, col, TL_H).set_cursor(CursorShape::Pointer);
            }
            reg("tl_img", img);
            ui.on(img, "on_pointer_down", |ui, _, args| {
                if let EventArgs::Pointer(e) = args {
                    with(|st| st.tl_drag = true);
                    tl_seek(ui, e.position.x);
                }
            });
            ui.on(img, "on_pointer_move", |ui, _, args| {
                if let EventArgs::Pointer(e) = args {
                    if with(|st| st.tl_drag) {
                        tl_seek(ui, e.position.x);
                    }
                }
            });
            ui.on(img, "on_pointer_up", |_, _, _| with(|st| st.tl_drag = false));
            ui.on(img, "on_wheel", |ui, _, args| {
                if let EventArgs::Wheel(e) = args {
                    tl_wheel(ui, e.position.x, e.delta.y);
                }
            });
        }
        if let Some(track) = create(ui, "Panel", tl) {
            let _ = ui.set_prop(track, "style", "flat");
            if let Some(cb) = ui.control_mut(track) {
                cb.set_rect(0.0, TL_H + RULER_H + 2.0, col, SB_H - 2.0);
            }
            reg("sb", track);
            if let Some(th) = create(ui, "Rectangle", track) {
                let _ = ui.set_prop(th, "style", "edge");
                if let Some(cb) = ui.control_mut(th) {
                    cb.set_rect(0.0, 0.0, col, SB_H - 2.0).set_cursor(CursorShape::Pointer);
                }
                reg("sb_thumb", th);
                ui.on(th, "on_pointer_down", |ui, _, args| {
                    if let EventArgs::Pointer(e) = args {
                        let x0 = id("sb_thumb").and_then(|t| ui.control(t)).map_or(0.0, |cb| cb.left());
                        with(|st| st.sb_drag = Some((e.position.x + x0, st.scroll)));
                    }
                });
                ui.on(th, "on_pointer_move", |ui, _, args| {
                    if let EventArgs::Pointer(e) = args {
                        let x0 = id("sb_thumb").and_then(|t| ui.control(t)).map_or(0.0, |cb| cb.left());
                        sb_drag(ui, e.position.x + x0);
                    }
                });
                ui.on(th, "on_pointer_up", |_, _, _| with(|st| st.sb_drag = None));
            }
        }
    }
    hint(ui, page, "range_hint", &lang::t("export.rangeHint"));

    // ---- áudio ----
    if let Some(sec) = create(ui, "Layout", page) {
        if let Some(cb) = ui.control_mut(sec) {
            cb.set_align(Align::Top).set_height(10.0);
        }
        reg("audio_sec", sec);
        heading(ui, sec, &lang::t("export.audio"));
        if let Some(b) = check(ui, sec, &lang::t("export.originalAudio"), |ui, on| {
            with(|st| st.original_audio = on);
            render_audio(ui);
        }) {
            reg("orig", b);
        }
        if let Some(f) = create(ui, "FlowLayout", sec) {
            let _ = ui.set_prop(f, "gap", 6.0);
            let _ = ui.set_prop(f, "line_gap", 6.0);
            if let Some(cb) = ui.control_mut(f) {
                cb.set_align(Align::Top).set_height(30.0).set_margins([0.0, 6.0, 0.0, 0.0]);
            }
            reg("tracks", f);
        }
        if let Some(b) = check(ui, sec, &lang::t("export.mixAudio"), |ui, on| {
            with(|st| st.mix_checked = on);
            render_audio(ui);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_margins([0.0, 6.0, 0.0, 0.0]);
            }
            reg("mix", b);
        }
    }

    // ---- resolução + codificador ----
    if let Some(r) = row(ui, page, 20.0, 22.0) {
        let half = (col - 12.0) / 2.0;
        label(ui, r, &lang::t("export.resolution"), "row", Align::Left, half + 12.0, 20.0);
        label(ui, r, &lang::t("export.encoder"), "row", Align::Client, 0.0, 20.0);
        reg("video_head", r);
    }
    if let Some(r) = row(ui, page, 32.0, 8.0) {
        let half = (col - 12.0) / 2.0;
        if let Some(b) = combo(ui, r, half, |ui, i| {
            with(|st| st.res_h = st.res_heights.get(i).copied().unwrap_or(0));
            render_resolutions(ui);
            render_scale(ui);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_margins([0.0, 0.0, 12.0, 0.0]);
            }
            reg("res", b);
        }
        if let Some(b) = combo(ui, r, 0.0, |ui, i| {
            with(|st| {
                if let Some(e) = st.encoders.get(i) {
                    st.enc_id = e.id.clone();
                }
            });
            render_encoders(ui);
            render_fps(ui);
            render_quality(ui);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_margins([0.0, 0.0, 0.0, 0.0]);
            }
            reg("enc", b);
        }
        reg("video_row", r);
    }
    hint(ui, page, "res_hint", &lang::t("export.resolutionHint"));

    // ---- redimensionamento ----
    if let Some(sec) = create(ui, "Layout", page) {
        if let Some(cb) = ui.control_mut(sec) {
            cb.set_align(Align::Top).set_height(10.0);
        }
        reg("scale_sec", sec);
        heading(ui, sec, &lang::t("export.scale"));
        if let Some(r) = row(ui, sec, 32.0, 0.0) {
            if let Some(b) = combo(ui, r, (col - 12.0) / 2.0, |ui, i| {
                with(|st| st.scale_algo = SCALES.get(i).copied().unwrap_or("bicubic").into());
                render_scale(ui);
            }) {
                reg("scale", b);
            }
        }
        hint(ui, sec, "scale_hint", "");
    }

    // ---- velocidade ----
    heading(ui, page, &lang::t("export.speed"));
    if let Some(r) = row(ui, page, 32.0, 0.0) {
        button(ui, r, "", "minus", "tool", |ui| speed_step(ui, -1));
        if let Some(e) = create(ui, "Edit", r) {
            let _ = ui.set_prop(e, "text", "1");
            if let Some(cb) = ui.control_mut(e) {
                cb.set_align(Align::Left).set_width(80.0).set_margins([0.0, 0.0, 6.0, 0.0]);
            }
            ui.on(e, "on_change", |ui, sender, _| {
                if let Ok(PropValue::String(t)) = ui.get_prop(sender, "text") {
                    speed_edit(ui, &t);
                }
            });
            ui.on(e, "on_exit", |ui, _, _| speed_commit(ui));
            reg("speed", e);
        }
        button(ui, r, "", "plus", "tool", |ui| speed_step(ui, 1));
        if let Some(l) = label(ui, r, "×", "key", Align::Left, 20.0, 32.0) {
            let _ = ui.set_prop(l, "hit_test", false);
        }
        if let Some(l) = label(ui, r, "", "key", Align::Client, 0.0, 32.0) {
            reg("speed_value", l);
        }
    }
    hint(ui, page, "speed_hint", &lang::t("export.speedHint"));

    // ---- quadros por segundo ----
    if let Some(sec) = create(ui, "Layout", page) {
        if let Some(cb) = ui.control_mut(sec) {
            cb.set_align(Align::Top).set_height(10.0);
        }
        reg("fps_sec", sec);
        if let Some(h) = row(ui, sec, 20.0, 22.0) {
            label(ui, h, &lang::t("export.fps"), "row", Align::Left, 220.0, 20.0);
            if let Some(l) = label(ui, h, "", "val", Align::Client, 0.0, 20.0) {
                let _ = ui.set_prop(l, "h_align", "trailing");
                reg("fps_text", l);
            }
        }
        if let Some(r) = row(ui, sec, 30.0, 6.0) {
            if let Some(t) = track(ui, r, 20.0, 60.0, |ui, v| {
                with(|st| st.fps_val = v as i32);
                render_fps(ui);
            }) {
                reg("fps", t);
            }
        }
        hint(ui, sec, "fps_hint", &lang::t("export.fpsHint"));
    }

    // ---- qualidade ----
    if let Some(h) = row(ui, page, 20.0, 22.0) {
        label(ui, h, &lang::t("export.quality"), "row", Align::Left, 220.0, 20.0);
        if let Some(l) = label(ui, h, "", "val", Align::Client, 0.0, 20.0) {
            let _ = ui.set_prop(l, "h_align", "trailing");
            reg("q_text", l);
        }
    }
    if let Some(r) = row(ui, page, 30.0, 6.0) {
        if let Some(t) = track(ui, r, 0.0, CRF_MAX as f64, |ui, v| {
            with(|st| st.q_pos = (v as i32).clamp(0, CRF_MAX));
            render_quality(ui);
        }) {
            reg("quality", t);
        }
    }
    hint(ui, page, "q_hint", "");

    // ---- legenda gravada ----
    heading(ui, page, &lang::t("export.captions"));
    if let Some(b) = check(ui, page, &lang::t("export.burnCaptions"), |ui, on| {
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
        render_captions(ui);
        sync_caption(ui);
    }) {
        reg("burn", b);
    }
    hint(ui, page, "cap_hint", "");

    // ---- nome + formato ----
    heading(ui, page, &lang::t("export.fileName"));
    if let Some(r) = row(ui, page, 32.0, 0.0) {
        if let Some(b) = combo(ui, r, 110.0, |ui, i| {
            with(|st| {
                if st.container != "mp3" {
                    st.container = if i == 1 { "mkv".into() } else { "mp4".into() };
                    st.video_container = st.container.clone();
                }
            });
            render_container(ui);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Right).set_margins([8.0, 0.0, 0.0, 0.0]);
            }
            reg("cont", b);
        }
        if let Some(e) = create(ui, "Edit", r) {
            if let Some(cb) = ui.control_mut(e) {
                cb.set_align(Align::Client);
            }
            reg("name", e);
        }
    }
    hint(ui, page, "cont_hint", "");

    // ---- ação + progresso ----
    if let Some(r) = row(ui, page, 36.0, 24.0) {
        if let Some(b) = button(ui, r, &lang::t("export.action"), "download", "", run_or_cancel) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_width(150.0).set_margins([0.0, 0.0, 12.0, 0.0]);
            }
            reg("run", b);
        }
        if let Some(b) = combo(ui, r, 90.0, |_, i| with(|st| st.prevsec = i.min(2))) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Right).set_margins([6.0, 0.0, 0.0, 0.0]);
            }
            reg("prevsec", b);
        }
        if let Some(b) = button(ui, r, &lang::t("export.preview.button"), "", "outline", preview_action) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Right).set_width(170.0).set_margins([6.0, 0.0, 0.0, 0.0]);
            }
            let _ = ui.set_prop(b, "hint", lang::t("export.preview.hint"));
            reg("preview", b);
        }
        if let Some(pb) = create(ui, "ProgressBar", r) {
            if let Some(cb) = ui.control_mut(pb) {
                cb.set_align(Align::Client).set_margins([0.0, 15.0, 8.0, 15.0]).set_visible(false);
            }
            reg("progress", pb);
        }
    }
    if let Some(l) = label(ui, page, "", "key", Align::Top, 0.0, 18.0) {
        if let Some(cb) = ui.control_mut(l) {
            cb.set_margins([0.0, 4.0, 0.0, 0.0]);
        }
        reg("progress_text", l);
    }
    set_items(ui, "prevsec", &PREVIEW_SECS.iter().map(|s| format!("{s:.0} s")).collect::<Vec<_>>(), 1);
    layout_page(ui);
}

/// Centraliza a coluna (padding lateral) na largura atual.
fn center_column(ui: &mut Ui) {
    let Some(p) = c(ui, "lyExport") else { return };
    let w = ui.bounds(p).map_or(900.0, |b| b.width);
    let col = col_width(ui);
    let side = ((w - col) / 2.0).max(18.0);
    if let Some(cb) = ui.control_mut(p) {
        cb.set_padding([side, 18.0, side, 40.0]);
    }
}

/// Altura de um contêiner de seção = a soma dos filhos visíveis.
fn fit(ui: &mut Ui, p: ComponentId) -> f32 {
    let h: f32 = ui
        .children(p)
        .to_vec()
        .iter()
        .filter_map(|&i| ui.control(i).filter(|cb| cb.visible()).map(|cb| cb.height() + cb.margins().top + cb.margins().bottom))
        .sum();
    if let Some(cb) = ui.control_mut(p) {
        cb.set_height(h);
    }
    h
}

/// Alturas das dicas, das seções e da página (a largura mudou ou algo apareceu).
fn layout_page(ui: &mut Ui) {
    let col = with(|st| st.col_w);
    for h in with(|st| st.hints.clone()) {
        if !ui.contains(h) {
            continue;
        }
        let text = match ui.get_prop(h, "text") {
            Ok(PropValue::String(t)) => t,
            _ => String::new(),
        };
        let height = if text.is_empty() { 0.0 } else { wrapped_height(ui, h, &text, col) };
        if let Some(cb) = ui.control_mut(h) {
            cb.set_height(height).set_visible(height > 0.0);
        }
    }
    for key in ["regions", "tracks"] {
        if let Some(f) = id(key) {
            flow_height(ui, f, col);
        }
    }
    for key in ["audio_sec", "scale_sec", "fps_sec"] {
        if let Some(s) = id(key) {
            fit(ui, s);
        }
    }
    if let Some(p) = c(ui, "lyExport") {
        let h = fit(ui, p);

        if let Some(cb) = ui.control_mut(p) {
            cb.set_height(h + 70.0);
        }
    }
    ui.layout();
}

/// A altura de um `FlowLayout` de pílulas pela quantidade de linhas.
fn flow_height(ui: &mut Ui, f: ComponentId, col: f32) {
    let mut rows = 0;
    let mut x = 0.0f32;
    for ch in ui.children(f).to_vec() {
        let w = ui.control(ch).map_or(0.0, |cb| cb.width());
        if rows == 0 || x + w > col {
            rows += 1;
            x = 0.0;
        }
        x += w + 6.0;
    }
    let h = rows.max(1) as f32 * 30.0 + (rows.max(1) - 1) as f32 * 6.0;
    if let Some(cb) = ui.control_mut(f) {
        cb.set_height(h);
    }
}

pub fn resized(ui: &mut Ui) {
    if !is_open() {
        return;
    }
    let col = col_width(ui);
    let changed = with(|st| {
        let ch = (st.col_w - col).abs() > 0.5;
        st.col_w = col;
        st.view_w = col as f64;
        ch
    });
    if !changed {
        return;
    }
    ui.set_timeout(0, |ui| {
        center_column(ui);
        let col = with(|st| st.col_w);
        if let Some(r) = id("video_row") {
            if let Some(b) = id("res").and_then(|b| ui.control_mut(b)) {
                b.set_width((col - 12.0) / 2.0);
            }
            let _ = r;
        }
        if let Some(r) = id("video_head").and_then(|r| ui.children(r).first().copied()) {
            if let Some(cb) = ui.control_mut(r) {
                cb.set_width((col - 12.0) / 2.0 + 12.0);
            }
        }
        render_stage(ui);
        render_timeline(ui);
        layout_page(ui);
    });
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
    with_ui(|ui| {
        render_all(ui);
        push_view();
        push_volumes();
    });
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
    with_ui(|ui| {
        render_captions(ui);
        sync_caption(ui);
        layout_page(ui);
    });
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
    with_ui(render_timeline);
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
        with_ui(render_encoders);
    }
}

// ---------------------------------------------------------------------------
// Do motor da prévia
// ---------------------------------------------------------------------------

fn on_out(ui: &mut Ui, id: &str, o: Out) {
    if !waiting_for(id) {
        return;
    }
    match o {
        Out::Info(_) => {}
        Out::Frame { buf, x, y, .. } => {
            let k = with(|st| st.scale);
            let (bw, bh) = (buf.width, buf.height);
            let image = Image::rgba(bw, bh, buf.pixels);
            let rect = RectF::new(x as f32 / k, y as f32 / k, bw as f32 / k, bh as f32 / k);
            if let Some(v) = self::id("frame").and_then(|i| ui.get_mut::<FrameView>(i)) {
                v.set_frame(Some((image, rect)));
            }
            ui.invalidate();
            if !with(|st| std::mem::replace(&mut st.ready, true)) {
                show(ui, "msg", false);
                render_playbar(ui);
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
            render_playbar(ui);
        }
        Out::Error(e) => {
            // Sem prévia a exportação continua funcionando.
            with(|st| st.ready = false);
            put(ui, "msg", "text", lang::t("export.previewUnavailable"));
            show(ui, "msg", true);
            log!("Export: previa indisponivel ({e})");
        }
    }
}

fn tick(ui: &mut Ui) {
    let playing = with(|st| st.playing);
    if !playing {
        return;
    }
    let pos = with(|st| st.pb.as_ref().map_or(st.play_sec, |p| p.shared.position()));
    set_play_sec(ui, pos, false);
}

/// Vista do motor: o palco é o QUADRO (proporção da origem ou da composição).
fn view_spec(st: &State) -> ViewSpec {
    let k = st.scale as f64;
    let (fw, fh) = (st.frame.0 as f64, st.frame.1 as f64);
    let z = st.crop_zoom as f64;
    // Zoom com origem no CENTRO; o do motor tem origem no canto.
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

/// As telas escolhidas, em pixels do VÍDEO (a composição centraliza todas na vertical).
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

/// O que a prévia toca: a mistura (posição 0) no original ou com ela marcada;
/// senão as isoladas marcadas, somadas.
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
// Contas
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

fn ordered_regions(st: &State) -> Vec<(i64, i64, i64, i64)> {
    let Some(lay) = &st.layout else { return Vec::new() };
    let mut out: Vec<(i64, i64, i64, i64)> = st.sel_regions.iter().filter_map(|&i| lay.regions.get(i)).map(|r| (r.x, r.y, r.w, r.h)).collect();
    out.sort_by_key(|r| r.0);
    out
}

/// Tamanho nativo do que sai.
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
    if lang::current_language().starts_with("en") { s } else { s.replace('.', ",") }
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

// ---------------------------------------------------------------------------
// Desenho
// ---------------------------------------------------------------------------

fn render_all(ui: &mut Ui) {
    render_regions(ui);
    render_stage(ui);
    render_playbar(ui);
    render_timeline(ui);
    render_audio(ui);
    render_resolutions(ui);
    render_encoders(ui);
    render_scale(ui);
    render_speed(ui);
    render_fps(ui);
    render_quality(ui);
    render_captions(ui);
    render_container(ui);
    render_actions(ui);
    sync_caption(ui);
    layout_page(ui);
}

/// Refaz as pílulas de um `FlowLayout`: (texto, marcada, desligada), com o clique.
fn pills(ui: &mut Ui, key: &'static str, items: Vec<(i32, String, bool, bool)>, f: fn(&mut Ui, i32)) {
    let Some(flow) = id(key) else { return };
    for ch in ui.children(flow).to_vec() {
        ui.free(ch);
    }
    for (idx, text, checked, disabled) in items {
        let Ok(chip) = ui.create("Chip", Some(flow)) else { continue };
        let _ = ui.set_prop(chip, "text", text.as_str());
        let _ = ui.set_prop(chip, "checked", checked);
        let _ = ui.set_prop(chip, "enabled", !disabled);
        let w = ui.measure_text(&text, &TextStyle { font: ui.font_of(chip), ..TextStyle::default() }).width + if checked { 48.0 } else { 30.0 };
        if let Some(cb) = ui.control_mut(chip) {
            cb.set_width(w).set_height(30.0);
        }
        ui.on(chip, "on_click", move |ui, _, _| f(ui, idx));
    }
}

fn render_regions(ui: &mut Ui) {
    let items = with(|st| {
        let regs = st.layout.as_ref().map(|l| l.regions.clone()).unwrap_or_default();
        let mut v = vec![(0, if regs.len() > 1 { lang::t("export.allRegions") } else { lang::t("export.fullScreen") }, !st.no_video && st.sel_regions.is_empty(), false)];
        if regs.len() > 1 {
            for (i, r) in regs.iter().enumerate() {
                let name = if r.name.is_empty() { tw("export.regionN", &[("n", &(i + 1).to_string())]) } else { r.name.clone() };
                v.push((i as i32 + 1, name, !st.no_video && st.sel_regions.contains(&i), false));
            }
        }
        v.push((-1, lang::t("export.noScreen"), st.no_video, st.loaded && st.audio.is_empty()));
        v
    });
    pills(ui, "regions", items, region_clicked);
}

fn region_clicked(ui: &mut Ui, idx: i32) {
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
    render_all(ui);
    push_view();
}

/// Proporção e tamanho do palco, recorte, zoom, mensagem de só áudio.
fn render_stage(ui: &mut Ui) {
    let (dims, crop_on, r, sw, sh, zoom, pan, no_video, has, col) =
        with(|st| (stage_dims(st), crop_available(st), crop_rect(st), st.src_w, st.src_h, st.crop_zoom, st.pan, st.no_video, has_crop(st), st.col_w));
    // O palco: a coluna inteira, no teto de metade da janela (fica mais estreito).
    let aspect = if dims.0 > 0 && dims.1 > 0 { dims.0 as f32 / dims.1 as f32 } else { 16.0 / 9.0 };
    let win_h = main_form().and_then(|f| ui.bounds(f)).map_or(900.0, |b| b.height);
    let max_h = (win_h * 0.5).max(220.0);
    let mut fw = col;
    let mut fh = fw / aspect;
    if fh > max_h {
        fh = max_h;
        fw = fh * aspect;
    }
    let fx = ((col - fw) / 2.0).max(0.0);
    let changed = with(|st| {
        let ch = (st.frame.0 - fw).abs() > 0.5 || (st.frame.1 - fh).abs() > 0.5;
        st.frame = (fw, fh);
        st.frame_at = (fx, 0.0);
        if ch {
            clamp_pan(st);
            st.cap_cache = None;
        }
        ch
    });
    if let Some(cb) = id("stage").and_then(|s| ui.control_mut(s)) {
        cb.set_height(fh);
    }
    if let Some(cb) = id("frame").and_then(|f| ui.control_mut(f)) {
        cb.set_rect(fx, 0.0, fw, fh).set_visible(!no_video);
    }
    show(ui, "audio_only", no_video);
    // A moldura acompanha o zoom (origem no centro) e o deslocamento.
    if let Some(cr) = id("crop") {
        let (zx, zy) = (fx + pan.0 + fw / 2.0 * (1.0 - zoom), pan.1 + fh / 2.0 * (1.0 - zoom));
        if let Some(cb) = ui.control_mut(cr) {
            cb.set_rect(zx, zy, fw * zoom, fh * zoom).set_visible(crop_on && !no_video);
        }
        if let Some(b) = ui.get_mut::<CropBox>(cr) {
            if !b.is_dragging() && sw > 0 && sh > 0 {
                b.source_width = sw as f32;
                b.source_height = sh as f32;
                b.set_selection(r.x as f32, r.y as f32, r.w as f32, r.h as f32);
            }
        }
    }
    put(ui, "zoom_badge", "text", format!("{}%", (zoom * 100.0).round() as i32));
    put(ui, "zoom_reset", "enabled", zoom > 1.001);
    put(ui, "zoom_in", "enabled", crop_on);
    put(ui, "zoom_out", "enabled", crop_on && zoom > 1.001);
    put(ui, "crop_reset", "enabled", has);
    put(
        ui,
        "crop_info",
        "text",
        if has { tw("export.cropOf", &[("w", &r.w.to_string()), ("h", &r.h.to_string()), ("sw", &sw.to_string()), ("sh", &sh.to_string())]) } else { String::new() },
    );
    // Legenda proporcional ao quadro.
    if fh > 0.0 {
        let font = (fh * 0.045).max(9.0);
        let w = (fw * 0.86).min(font * 32.0);
        if let Some(l) = id("caption") {
            let _ = ui.set_prop(l, "font_size", font as f64);
            if let Some(cb) = ui.control_mut(l) {
                cb.set_rect(fx + (fw - w) / 2.0, fh - font * 3.2 - 12.0, w, font * 3.2);
            }
        }
    }
    if changed {
        push_view();
    }
    ui.invalidate();
}

fn render_playbar(ui: &mut Ui) {
    let (playing, muted, vol, has, no_video, on, ready) = with(|st| {
        (st.playing, st.user_muted || st.user_vol == 0, st.user_vol, st.turns.as_ref().is_some_and(|t| !t.is_empty()), st.no_video, st.captions_on, st.ready)
    });
    put(ui, "play", "enabled", ready);
    put(ui, "play", "icon", if playing { "pause" } else { "play" });
    put(ui, "play", "hint", lang::t(if playing { "export.pause" } else { "export.play" }));
    put(ui, "mute", "icon", if muted { "mute" } else { "volume" });
    put(ui, "mute", "hint", lang::t(if muted { "export.unmute" } else { "export.mute" }));
    if let Some(t) = id("volume") {
        if ui.get_prop(t, "value").ok().and_then(|v| v.to_f64().ok()).is_none_or(|v| (v - vol as f64).abs() > 0.5) {
            let _ = ui.set_prop(t, "value", vol as f64);
        }
    }
    put(ui, "cc", "enabled", has && !no_video);
    put(ui, "cc", "style", if on && has && !no_video { "outline" } else { "tool" });
    put(
        ui,
        "cc",
        "hint",
        lang::t(if !has {
            "export.captionsNone"
        } else if on {
            "export.captionsHide"
        } else {
            "export.captionsShow"
        }),
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

fn render_timeline(ui: &mut Ui) {
    let Some(tl) = id("tl") else { return };
    let dark = crate::bridge::resolved_theme() == "dark";
    struct Marks {
        cuts: Vec<(f32, bool)>,
        ticks: Vec<(f32, String)>,
        labels: Vec<(f32, f32, String)>,
        sb: (f32, f32),
        img: Option<Image>,
        ph: f32,
        view: f32,
    }
    let m = with(|st| {
        clamp_scroll(st);
        st.cursor_part = part_at_cursor(st);
        let dur = st.duration.max(0.001);
        let total = total_w(st);
        let view = st.view_w;
        let x_of = |sec: f64| (sec / dur * total - st.scroll) as f32;
        let target = nearest_cut(st);
        let cuts = (0..st.parts.len().saturating_sub(1)).map(|i| (x_of(st.parts[i].end), target == i as i32 + 1)).collect();
        // Régua com passo de ~90 px, só a janela visível.
        let mut ticks = Vec::new();
        if st.duration > 0.0 {
            let step = TICK_STEPS.iter().copied().find(|&s| s / st.duration * total >= 90.0).unwrap_or(3600.0);
            let spp = st.duration / total;
            let from = (st.scroll * spp).max(0.0);
            let to = ((st.scroll + view) * spp).min(st.duration);
            let mut t = (from / step).ceil() * step;
            while t <= to {
                if t / st.duration > 0.985 {
                    break;
                }
                ticks.push((x_of(t), fmt_time(t)));
                t += step;
            }
        }
        let labels = st
            .parts
            .iter()
            .filter(|p| (p.end - p.start) / dur * total > 60.0)
            .map(|p| (x_of(p.start), ((p.end - p.start) / dur * total) as f32, fmt_time(p.end - p.start)))
            .collect();
        let sb_w = if total > 0.0 { (view * view / total).max(20.0).min(view) } else { view };
        let sb_x = if total > view { st.scroll / (total - view) * (view - sb_w) } else { 0.0 };
        let img = timeline_image(st, dark, st.scale);
        Marks { cuts, ticks, labels, sb: (sb_x as f32, sb_w as f32), img, ph: x_of(st.play_sec), view: view as f32 }
    });
    if let Some(img) = id("tl_img") {
        if let Some(cb) = ui.control_mut(img) {
            cb.set_rect(0.0, 0.0, m.view, TL_H);
        }
        if let Some(pic) = m.img {
            if let Some(iv) = ui.get_mut::<ImageView>(img) {
                iv.set_picture(Some(pic));
            }
        }
    }
    for old in with(|st| std::mem::take(&mut st.tl_marks)) {
        if ui.contains(old) {
            ui.free(old);
        }
    }
    let mut marks = Vec::new();
    let accent = ui.theme().color("amber");
    for (x, target) in m.cuts {
        if !(-2.0..=m.view + 2.0).contains(&x) {
            continue;
        }
        if let Ok(r) = ui.create("Rectangle", Some(tl)) {
            let _ = ui.set_prop(r, "fill", if target { accent } else { ui.theme().color("text_3") });
            let _ = ui.set_prop(r, "stroke_width", 0.0);
            if let Some(cb) = ui.control_mut(r) {
                cb.set_rect(x - 1.0, 0.0, 2.0, TL_H).set_hit_test(false);
            }
            marks.push(r);
        }
    }
    for (x, w, text) in m.labels {
        let (lx, lw) = (x.max(0.0), (x + w).min(m.view) - x.max(0.0));
        if lw < 50.0 {
            continue;
        }
        if let Ok(l) = ui.create("Label", Some(tl)) {
            let _ = ui.set_prop(l, "style", "legend");
            let _ = ui.set_prop(l, "text", text);
            let _ = ui.set_prop(l, "h_align", "center");
            if let Some(cb) = ui.control_mut(l) {
                cb.set_rect(lx, 2.0, lw, 14.0).set_hit_test(false);
            }
            marks.push(l);
        }
    }
    for (x, text) in m.ticks {
        if let Ok(l) = ui.create("Label", Some(tl)) {
            let _ = ui.set_prop(l, "style", "legend");
            let _ = ui.set_prop(l, "text", text);
            if let Some(cb) = ui.control_mut(l) {
                cb.set_rect(x + 3.0, TL_H + 2.0, 60.0, RULER_H - 2.0).set_hit_test(false);
            }
            marks.push(l);
        }
        if let Ok(r) = ui.create("Rectangle", Some(tl)) {
            let _ = ui.set_prop(r, "style", "sep");
            if let Some(cb) = ui.control_mut(r) {
                cb.set_rect(x, TL_H, 1.0, 5.0).set_hit_test(false);
            }
            marks.push(r);
        }
    }
    // O cursor por último (fica por cima das divisões).
    if let Ok(ph) = ui.create("Rectangle", Some(tl)) {
        let _ = ui.set_prop(ph, "fill", Color::WHITE);
        let _ = ui.set_prop(ph, "stroke_width", 0.0);
        if let Some(cb) = ui.control_mut(ph) {
            cb.set_rect(m.ph - 1.0, 0.0, 2.0, TL_H).set_hit_test(false).set_visible((-2.0..=m.view + 2.0).contains(&m.ph));
        }
        reg("playhead", ph);
        marks.push(ph);
    }
    with(|st| st.tl_marks = marks);
    if let Some(cb) = id("sb").and_then(|p| ui.control_mut(p)) {
        cb.set_width(m.view).set_visible(m.sb.1 < m.view - 0.5);
    }
    if let Some(cb) = id("sb_thumb").and_then(|p| ui.control_mut(p)) {
        cb.set_left(m.sb.0).set_width(m.sb.1);
    }
    render_tlbar(ui);
    ui.invalidate();
}

/// Fundo dos trechos (mantido verde, tirado listrado, realce do trecho do
/// cursor) e a forma de onda, só na janela visível.
fn timeline_image(st: &State, dark: bool, k: f32) -> Option<Image> {
    let view = st.view_w as f32;
    if view <= 1.0 {
        return None;
    }
    let (wpx, hpx) = ((view * k).round().max(1.0) as u32, (TL_H * k).round().max(1.0) as u32);
    let mut px = vec![0u8; (wpx * hpx * 4) as usize];
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
    let put = |px: &mut [u8], x: u32, y: u32, c: (u8, u8, u8)| {
        let i = ((y * wpx + x) * 4) as usize;
        px[i..i + 4].copy_from_slice(&[c.0, c.1, c.2, 255]);
    };
    let get = |px: &[u8], x: u32, y: u32| {
        let i = ((y * wpx + x) * 4) as usize;
        (px[i], px[i + 1], px[i + 2])
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
            put(&mut px, xp, yp, c);
        }
    }
    // Realce do trecho sob o cursor.
    if cur >= 0 {
        if let Some(p) = st.parts.get(cur as usize) {
            let x0 = ((p.start / dur) as f32 * total - scroll) * k;
            let x1 = ((p.end / dur) as f32 * total - scroll) * k;
            let t = k.round().max(1.0) as i32;
            let (xa, xb) = (x0.round() as i32, x1.round() as i32);
            for xp in xa.max(0)..xb.min(wpx as i32) {
                for d in 0..t {
                    put(&mut px, xp as u32, d as u32, b2);
                    put(&mut px, xp as u32, hpx - 1 - d as u32, b2);
                }
            }
            for d in 0..t {
                for edge in [xa + d, xb - 1 - d] {
                    if edge >= 0 && edge < wpx as i32 {
                        for yp in 0..hpx {
                            put(&mut px, edge as u32, yp, b2);
                        }
                    }
                }
            }
        }
    }
    // Forma de onda: pico por fatia, raiz quadrada, barras de ~2 px.
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
            let i0 = ((f0 * n as f32).floor() as usize).min(n - 1);
            let i1 = ((f1 * n as f32).ceil() as usize).clamp(i0 + 1, n);
            let m = peaks[i0..i1].iter().cloned().fold(0.0f32, f32::max);
            let bh = ((m / st.wave_max).sqrt() * hpx as f32 * 0.82).max(k);
            let mid = ((f0 + f1) / 2.0) as f64 * dur;
            let kept = st.parts.iter().find(|p| mid >= p.start && mid < p.end).is_none_or(|p| p.keep);
            let (col, a) = if kept { (succ, 0.85) } else { (t3, 0.45) };
            let y0 = ((hpx as f32 - bh) / 2.0).max(0.0);
            let y1 = (y0 + bh).min(hpx as f32);
            let xa = x.floor() as u32;
            let xb = ((x + bar_w).ceil() as u32).min(wpx);
            for xp in xa..xb {
                let cov_x = ((xp as f32 + 1.0).min(x + bar_w) - (xp as f32).max(x)).clamp(0.0, 1.0);
                for yp in (y0.floor() as u32)..(y1.ceil() as u32).min(hpx) {
                    let cov_y = ((yp as f32 + 1.0).min(y1) - (yp as f32).max(y0)).clamp(0.0, 1.0);
                    let b = get(&px, xp, yp);
                    put(&mut px, xp, yp, blend(b, col, a * cov_x * cov_y));
                }
            }
            x += step;
        }
    }
    Some(Image::rgba(wpx, hpx, px))
}

fn render_tlbar(ui: &mut Ui) {
    let (cut_ok, uncut, uncut_at, toggle, toggle_ok, zoom, play, total, kept, spd) = with(|st| {
        let idx = part_at_cursor(st);
        let p = (idx >= 0).then(|| st.parts[idx as usize]);
        let t = st.play_sec;
        let cut_ok = p.is_some_and(|p| t > p.start + 0.05 && t < p.end - 0.05);
        let ci = nearest_cut(st);
        let at = if ci > 0 { st.parts[ci as usize].start } else { 0.0 };
        (cut_ok, ci, at, p.is_none_or(|p| p.keep), p.is_some(), st.zoom, st.play_sec, st.duration, kept_duration(st), speed(st))
    });
    put(ui, "cut", "enabled", cut_ok);
    put(ui, "uncut", "enabled", uncut > 0);
    put(ui, "uncut", "hint", if uncut > 0 { tw("export.removeCutAt", &[("at", &fmt_time(uncut_at))]) } else { lang::t("export.removeCutNone") });
    put(ui, "toggle", "text", lang::t(if toggle { "export.dropPart" } else { "export.keepPart" }));
    put(ui, "toggle", "enabled", toggle_ok);
    put(ui, "tl_zoom", "text", format!("{}×", if zoom < 10.0 { format!("{zoom:.1}") } else { format!("{}", zoom.round()) }));
    // O campo do tempo não muda enquanto o usuário digita nele.
    if let Some(e) = id("time") {
        let focused = main_form().and_then(|f| ui.focused(f)) == Some(e);
        if !focused {
            let _ = ui.set_prop(e, "text", fmt_time(play));
        }
    }
    let mut range = format!("/ {}   ·   {}", fmt_time(total), tw("export.rangeLength", &[("len", &fmt_time(kept))]));
    if spd > 1.0 {
        range.push_str(&format!("   ·   {}", tw("export.speedResult", &[("len", &fmt_time(kept / spd))])));
    }
    put(ui, "range_text", "text", range);
}

/// Move o cursor (barra, tempo, botões); `seek` = também reposiciona o vídeo.
fn set_play_sec(ui: &mut Ui, sec: f64, seek: bool) {
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
        render_timeline(ui);
    } else {
        if let Some(cb) = id("playhead").and_then(|p| ui.control_mut(p)) {
            cb.set_left(ph - 1.0);
        }
        render_tlbar(ui);
        ui.invalidate();
    }
    sync_caption(ui);
}

fn tl_seek(ui: &mut Ui, x: f32) {
    let sec = with(|st| {
        let total = total_w(st);
        ((st.scroll + x as f64) / total * st.duration).clamp(0.0, st.duration)
    });
    set_play_sec(ui, sec, true);
}

fn set_zoom(ui: &mut Ui, z: f64, anchor: Option<f64>) {
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
        render_timeline(ui);
    }
}

fn tl_wheel(ui: &mut Ui, x: f32, dy: f32) {
    let (anchor, z) = with(|st| ((st.scroll + x as f64) / total_w(st) * st.duration, st.zoom));
    // Roda pra cima (delta positivo) aproxima.
    set_zoom(ui, z * if dy > 0.0 { 1.25 } else { 1.0 / 1.25 }, Some(anchor));
}

fn sb_drag(ui: &mut Ui, x: f32) {
    let ok = with(|st| {
        let Some((from, scroll0)) = st.sb_drag else { return false };
        let total = total_w(st);
        let view = st.view_w;
        let sb_w = (view * view / total).max(20.0).min(view);
        if total > view && view > sb_w {
            st.scroll = scroll0 + (x - from) as f64 / (view - sb_w) * (total - view);
        }
        clamp_scroll(st);
        true
    });
    if ok {
        render_timeline(ui);
    }
}

fn time_commit(ui: &mut Ui, sender: ComponentId) {
    let text = match ui.get_prop(sender, "text") {
        Ok(PropValue::String(t)) => t,
        _ => String::new(),
    };
    match parse_time(&text) {
        Some(v) => set_play_sec(ui, v, true),
        None => render_tlbar(ui),
    }
}

fn cut_here(ui: &mut Ui) {
    with(|st| {
        let t = st.play_sec;
        if let Some(i) = st.parts.iter().position(|p| t > p.start + 0.05 && t < p.end - 0.05) {
            let p = st.parts[i];
            st.parts.splice(i..=i, [Part { start: p.start, end: t, keep: p.keep }, Part { start: t, end: p.end, keep: p.keep }]);
        }
    });
    render_timeline(ui);
}

fn remove_cut(ui: &mut Ui) {
    with(|st| {
        let i = nearest_cut(st);
        if i < 1 {
            return;
        }
        let i = i as usize;
        let (l, r) = (st.parts[i - 1], st.parts[i]);
        st.parts.splice(i - 1..=i, [Part { start: l.start, end: r.end, keep: l.keep || r.keep }]);
    });
    render_timeline(ui);
}

fn toggle_part(ui: &mut Ui) {
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
    render_timeline(ui);
}

// ---- reprodução ----

fn toggle_play(ui: &mut Ui) {
    if !with(|st| st.ready) {
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
    render_playbar(ui);
}

fn toggle_mute(ui: &mut Ui) {
    with(|st| {
        st.user_muted = !st.user_muted;
        if !st.user_muted && st.user_vol == 0 {
            st.user_vol = 100;
        }
    });
    push_volumes();
    render_playbar(ui);
}

fn set_volume(ui: &mut Ui, v: i32) {
    let same = with(|st| {
        let same = st.user_vol == v.clamp(0, 100);
        st.user_vol = v.clamp(0, 100);
        if st.user_vol > 0 {
            st.user_muted = false;
        }
        same
    });
    if !same {
        push_volumes();
        render_playbar(ui);
    }
}

fn toggle_cc(ui: &mut Ui) {
    with(|st| st.captions_on = !st.captions_on);
    config::set_config_bool("exportCaptions", with(|st| st.captions_on));
    render_playbar(ui);
    sync_caption(ui);
}

fn sync_caption(ui: &mut Ui) {
    let (font, width) = with(|st| {
        let fh = st.frame.1;
        let font = (fh * 0.045).max(9.0);
        (font, (st.frame.0 * 0.86).min(font * 32.0))
    });
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
    let text = text.unwrap_or_default();
    if let Some(l) = id("caption") {
        if !matches!(ui.get_prop(l, "text"), Ok(PropValue::String(s)) if s == text) {
            let _ = ui.set_prop(l, "text", text.as_str());
            let _ = ui.set_prop(l, "visible", !text.is_empty());
        }
    }
}

// ---- recorte e zoom da prévia ----

/// A moldura mudou (arrastando ou ao soltar).
fn crop_changed(ui: &mut Ui, sender: ComponentId, end: bool) {
    let Some((x, y, w, h)) = ui.get::<CropBox>(sender).map(|b| b.selection()) else { return };
    with(|st| {
        let r = Rect { x: x as i32, y: y as i32, w: w as i32, h: h as i32 };
        let full = r.x == 0 && r.y == 0 && r.w >= st.src_w - 1 && r.h >= st.src_h - 1;
        st.crop = if full { None } else { Some(r) };
    });
    if end {
        render_stage(ui);
        render_resolutions(ui);
        render_scale(ui);
        layout_page(ui);
    } else {
        let (r, sw, sh, has) = with(|st| (crop_rect(st), st.src_w, st.src_h, has_crop(st)));
        put(
            ui,
            "crop_info",
            "text",
            if has { tw("export.cropOf", &[("w", &r.w.to_string()), ("h", &r.h.to_string()), ("sw", &sw.to_string()), ("sh", &sh.to_string())]) } else { String::new() },
        );
    }
}

fn clamp_pan(st: &mut State) {
    let (fw, fh) = st.frame;
    let mx = (fw * (st.crop_zoom - 1.0) / 2.0).max(0.0);
    let my = (fh * (st.crop_zoom - 1.0) / 2.0).max(0.0);
    st.pan.0 = st.pan.0.clamp(-mx, mx);
    st.pan.1 = st.pan.1.clamp(-my, my);
}

fn set_crop_zoom(ui: &mut Ui, z: f32, anchor: Option<(f32, f32, f32, f32)>) {
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
    render_stage(ui);
}

fn crop_zoom_by(ui: &mut Ui, f: f32) {
    let (z, pan) = with(|st| (st.crop_zoom, st.pan));
    set_crop_zoom(ui, z * f, Some((0.0, 0.0, -pan.0 / z, -pan.1 / z)));
}

fn crop_zoom_reset(ui: &mut Ui) {
    with(|st| {
        st.crop_zoom = 1.0;
        st.pan = (0.0, 0.0);
    });
    push_view();
    render_stage(ui);
}

/// Roda sobre a prévia (Ctrl): aproxima no ponto do mouse.
fn stage_wheel(ui: &mut Ui, sender: ComponentId, at: PointF, dy: f32) {
    if !with(|st| crop_available(st)) {
        return;
    }
    // O ponto em relação ao centro do quadro.
    let origin = ui.control(sender).map_or((0.0, 0.0), |cb| (cb.left(), cb.top()));
    let (z, pan, (fx, fy), (fw, fh)) = with(|st| (st.crop_zoom, st.pan, st.frame_at, st.frame));
    let (cx, cy) = (origin.0 + at.x - fx - fw / 2.0, origin.1 + at.y - fy - fh / 2.0);
    let (vx, vy) = ((cx - pan.0) / z, (cy - pan.1) / z);
    set_crop_zoom(ui, z * if dy > 0.0 { 1.2 } else { 1.0 / 1.2 }, Some((cx, cy, vx, vy)));
}

fn pan_down(ui: &mut Ui, x: f32, y: f32) {
    let started = with(|st| {
        if st.crop_zoom <= 1.001 {
            return false;
        }
        st.pan_drag = Some(PanDrag { sx: x, sy: y, px: st.pan.0, py: st.pan.1, moved: 0.0 });
        true
    });
    // Sem zoom (ou com as telas compostas), clicar na prévia toca/pausa.
    if !started {
        toggle_play(ui);
    }
}

fn pan_move(ui: &mut Ui, x: f32, y: f32) {
    let ok = with(|st| {
        let Some(d) = st.pan_drag.as_mut() else { return false };
        d.moved = d.moved.max((x - d.sx).abs() + (y - d.sy).abs());
        st.pan = (d.px + x - d.sx, d.py + y - d.sy);
        clamp_pan(st);
        true
    });
    if ok {
        push_view();
        render_stage(ui);
    }
}

fn pan_up(ui: &mut Ui) {
    let still = with(|st| st.pan_drag.take().is_some_and(|d| d.moved < 4.0));
    if still {
        toggle_play(ui);
    }
}

// ---- áudio ----

fn render_audio(ui: &mut Ui) {
    let (vis, orig_vis, orig, items, mix_checked, can_mix) = with(|st| {
        let orig = use_original(st);
        let mix = mix_index(st);
        let has_iso = st.sel_audio.iter().any(|&i| i != mix);
        let has_mix = st.sel_audio.contains(&mix);
        let items: Vec<(i32, String, bool, bool)> = st
            .audio
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let is_mix = s.index == mix;
                let label = if !s.title.is_empty() {
                    s.title.clone()
                } else if is_mix {
                    tw("export.trackMix", &[("n", &(i + 1).to_string())])
                } else {
                    tw("export.trackN", &[("n", &(i + 1).to_string())])
                };
                (i as i32, label, orig || st.sel_audio.contains(&s.index), orig || if is_mix { has_iso } else { has_mix })
            })
            .collect();
        let can_mix = !orig && st.sel_audio.len() > 1;
        if !can_mix {
            st.mix_checked = false;
        }
        (!st.audio.is_empty(), st.audio.len() > 1, orig, items, st.mix_checked, can_mix)
    });
    show(ui, "audio_sec", vis);
    show(ui, "orig", orig_vis);
    put(ui, "orig", "checked", orig);
    pills(ui, "tracks", items, track_clicked);
    put(ui, "mix", "checked", mix_checked);
    put(ui, "mix", "enabled", can_mix);
    push_volumes();
    let col = with(|st| st.col_w);
    if let Some(f) = id("tracks") {
        flow_height(ui, f, col);
    }
    if let Some(s) = id("audio_sec") {
        fit(ui, s);
    }
}

fn track_clicked(ui: &mut Ui, i: i32) {
    with(|st| {
        let Some(s) = st.audio.get(i as usize) else { return };
        let idx = s.index;
        if !st.sel_audio.remove(&idx) {
            st.sel_audio.insert(idx);
        }
    });
    render_audio(ui);
    layout_page(ui);
}

// ---- vídeo ----

fn render_resolutions(ui: &mut Ui) {
    let (opts, idx, no_video) = with(|st| {
        let (cw, ch) = composed_size(st);
        let mut hs = vec![0];
        let mut opts = vec![tw("export.resOriginal", &[("w", &cw.to_string()), ("h", &ch.to_string())])];
        for h in [1440, 1080, 720, 480] {
            if h < ch {
                hs.push(h);
                opts.push(format!("{h}p"));
            }
        }
        if !hs.contains(&st.res_h) {
            st.res_h = 0;
        }
        let idx = hs.iter().position(|&h| h == st.res_h).unwrap_or(0);
        st.res_heights = hs;
        (opts, idx, st.no_video)
    });
    set_items(ui, "res", &opts, idx);
    put(ui, "res", "enabled", !no_video);
    put(ui, "enc", "enabled", !no_video);
}

fn render_encoders(ui: &mut Ui) {
    let (opts, idx) = with(|st| {
        if st.encoders.is_empty() {
            st.enc_id = "auto".into();
            return (vec![lang::t("settings.codec.auto")], 0);
        }
        // Padrão: o codec das Configurações, se estiver na lista.
        if st.enc_id.is_empty() || !st.encoders.iter().any(|e| e.id == st.enc_id) {
            let want = config::get_config_str("codec", "auto");
            st.enc_id = if st.encoders.iter().any(|e| e.id == want) { want } else { st.encoders[0].id.clone() };
        }
        let idx = st.encoders.iter().position(|e| e.id == st.enc_id).unwrap_or(0);
        (st.encoders.iter().map(|e| enc_label(&e.id)).collect(), idx)
    });
    set_items(ui, "enc", &opts, idx);
}

fn render_scale(ui: &mut Ui) {
    let (vis, algo) = with(|st| (!st.id.is_empty() && st.loaded && is_resizing(st), st.scale_algo.clone()));
    let was = id("scale_sec").and_then(|s| ui.control(s)).is_some_and(|cb| cb.visible());
    show(ui, "scale_sec", vis);
    let labels: Vec<String> = SCALES.iter().map(|i| lang::t(&format!("export.scaleOpt.{i}"))).collect();
    set_items(ui, "scale", &labels, SCALES.iter().position(|i| *i == algo).unwrap_or(0));
    put(ui, "scale_hint", "text", lang::t(&format!("export.scaleHint.{algo}")));
    if was != vis {
        layout_page(ui);
    }
}

fn speed_edit(ui: &mut Ui, text: &str) {
    let changed = with(|st| {
        if st.speed_text == text {
            return false;
        }
        st.speed_text = text.to_owned();
        // A taxa acompanha a velocidade: o seletor vai pro teto.
        st.fps_val = max_fps(st);
        true
    });
    if changed {
        render_speed(ui);
        render_fps(ui);
    }
}

fn speed_commit(ui: &mut Ui) {
    with(|st| st.speed_text = fmt_speed(speed(st)));
    render_speed(ui);
    render_fps(ui);
}

fn speed_step(ui: &mut Ui, dir: i32) {
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
    render_speed(ui);
    render_fps(ui);
}

fn render_speed(ui: &mut Ui) {
    let (text, invalid) = with(|st| {
        let raw = st.speed_text.trim().replace(',', ".");
        let bad = !raw.is_empty() && raw.parse::<f64>().map_or(true, |v| !v.is_finite() || !(1.0..=SPEED_MAX).contains(&v));
        (st.speed_text.clone(), bad)
    });
    if let Some(e) = id("speed") {
        if !matches!(ui.get_prop(e, "text"), Ok(PropValue::String(s)) if s == text) {
            let _ = ui.set_prop(e, "text", text.as_str());
        }
        // Sem a cor própria (texto vazio), volta a do tema.
        let _ = ui.set_prop(e, "border_color", PropValue::String(if invalid { "red" } else { "" }.into()));
    }
    render_tlbar(ui);
}

fn render_fps(ui: &mut Ui) {
    let (max, val, src) = with(|st| {
        let max = max_fps(st);
        st.fps_val = if max > 0 { st.fps_val.clamp(20.min(max), max) } else { 0 };
        (max, st.fps_val, st.src_fps)
    });
    let was = id("fps_sec").and_then(|s| ui.control(s)).is_some_and(|cb| cb.visible());
    show(ui, "fps_sec", max > 20);
    if let Some(t) = id("fps") {
        let _ = ui.set_prop(t, "max", max.max(21) as f64);
        let _ = ui.set_prop(t, "min", 20.0);
        if ui.get_prop(t, "value").ok().and_then(|v| v.to_f64().ok()).is_none_or(|v| (v - val as f64).abs() > 0.5) {
            let _ = ui.set_prop(t, "value", val as f64);
        }
    }
    let shown = if max > 20 { val } else { src.round() as i32 };
    put(ui, "fps_text", "text", tw("export.fpsValue", &[("fps", &shown.to_string())]));
    if was != (max > 20) {
        layout_page(ui);
    }
}

fn render_quality(ui: &mut Ui) {
    let (pos, c, svt) = with(|st| (st.q_pos, crf(st), st.enc_id == "av1-sw"));
    if let Some(t) = id("quality") {
        if ui.get_prop(t, "value").ok().and_then(|v| v.to_f64().ok()).is_none_or(|v| (v - pos as f64).abs() > 0.5) {
            let _ = ui.set_prop(t, "value", pos as f64);
        }
    }
    put(ui, "q_text", "text", tw("export.qualityValue", &[("crf", &c.to_string())]));
    let mut hint = tw("export.qualityHint", &[("band", &lang::t(crf_band_key(c)))]);
    let warn = svt && c >= SVT_FLOOR_CRF;
    if warn {
        hint = format!("{} {}", tw("export.qualitySvtFloor", &[("crf", &SVT_FLOOR_CRF.to_string())]), hint);
    }
    let old = id("q_hint").and_then(|l| match ui.get_prop(l, "text") {
        Ok(PropValue::String(s)) => Some(s),
        _ => None,
    });
    put(ui, "q_hint", "text", hint.as_str());
    put(ui, "q_hint", "text_color", PropValue::String(if warn { "amber" } else { "" }.into()));
    if old.as_deref() != Some(hint.as_str()) {
        relayout_hint(ui, "q_hint");
    }
}

/// Uma dica mudou de texto: a altura dela (e a da página) acompanha.
fn relayout_hint(ui: &mut Ui, key: &'static str) {
    let Some(h) = id(key) else { return };
    let col = with(|st| st.col_w);
    let text = match ui.get_prop(h, "text") {
        Ok(PropValue::String(t)) => t,
        _ => String::new(),
    };
    let height = if text.is_empty() { 0.0 } else { wrapped_height(ui, h, &text, col) };
    let old = ui.control(h).map_or(0.0, |cb| cb.height());
    if (old - height).abs() > 0.5 {
        layout_page(ui);
    }
}

fn render_captions(ui: &mut Ui) {
    let (has, loading, transcribed, no_video, burn) = with(|st| {
        let has = st.turns.as_ref().is_some_and(|t| !t.is_empty());
        if !(has && !st.no_video) {
            st.burn = false;
        }
        (has, st.turns.is_none(), st.transcribed, st.no_video, st.burn)
    });
    put(ui, "burn", "enabled", has && !no_video);
    put(ui, "burn", "checked", burn);
    put(
        ui,
        "cap_hint",
        "text",
        lang::t(if loading {
            "export.captionsLoading"
        } else if has {
            "export.captionsHint"
        } else if transcribed {
            "player.transcriptNoSpeech"
        } else {
            "export.captionsNeedTranscript"
        }),
    );
    relayout_hint(ui, "cap_hint");
    render_playbar(ui);
}

fn render_container(ui: &mut Ui) {
    let c = with(|st| st.container.clone());
    if c == "mp3" {
        set_items(ui, "cont", &["MP3".to_owned()], 0);
        put(ui, "cont", "enabled", false);
    } else {
        set_items(ui, "cont", &["MP4".to_owned(), "MKV".to_owned()], if c == "mkv" { 1 } else { 0 });
        put(ui, "cont", "enabled", true);
    }
    put(
        ui,
        "cont_hint",
        "text",
        lang::t(match c.as_str() {
            "mkv" => "export.containerHintMkv",
            "mp3" => "export.containerHintMp3",
            _ => "export.containerHintMp4",
        }),
    );
    relayout_hint(ui, "cont_hint");
}

fn render_actions(ui: &mut Ui) {
    let (running, canceling, previewing, ppct, pct, loaded) = with(|st| (st.running, st.canceling, st.previewing, st.preview_pct, st.pct, st.loaded));
    put(ui, "run", "text", lang::t(if running { "common.cancel" } else { "export.action" }));
    put(ui, "run", "icon", if running { "close" } else { "download" });
    put(ui, "run", "style", if running { "outline" } else { "" });
    put(ui, "run", "enabled", loaded && !canceling && (running || !previewing));
    put(
        ui,
        "preview",
        "text",
        if previewing { tw("export.preview.generating", &[("pct", &(ppct.round() as i32).to_string())]) } else { lang::t("export.preview.button") },
    );
    put(ui, "preview", "enabled", loaded && !running);
    put(ui, "prevsec", "enabled", !previewing);
    show(ui, "progress", running);
    show(ui, "preview", !running);
    show(ui, "prevsec", !running);
    put(ui, "progress", "value", pct);
    put(
        ui,
        "progress_text",
        "text",
        if !running {
            String::new()
        } else if canceling {
            lang::t("export.canceling")
        } else {
            tw("export.progress", &[("pct", &(pct.round() as i32).to_string())])
        },
    );
}

// ---------------------------------------------------------------------------
// Execução
// ---------------------------------------------------------------------------

/// O pedido, do jeito que a tela está agora. `None` = nada a exportar.
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

fn file_name(ui: &Ui) -> String {
    id("name")
        .and_then(|e| match ui.get_prop(e, "text") {
            Ok(PropValue::String(s)) => Some(s),
            _ => None,
        })
        .unwrap_or_default()
}

fn pause_preview(ui: &mut Ui) {
    with(|st| {
        if let Some(pb) = &st.pb {
            if st.playing {
                pb.pause();
                st.play_sec = pb.shared.position();
            }
        }
        st.playing = false;
    });
    render_playbar(ui);
}

fn run_or_cancel(ui: &mut Ui) {
    if with(|st| st.running) {
        cancel(ui);
        return;
    }
    if with(|st| st.previewing) {
        return;
    }
    let Some(req) = build_request(&file_name(ui)) else { return };
    pause_preview(ui);
    with(|st| {
        st.running = true;
        st.running_id = st.id.clone();
        st.pct = 0.0;
        st.canceling = false;
    });
    render_actions(ui);
    sync_card();
    bexp::handle_export(req);
}

/// Cancelar (botão da tela ou o × do cartão).
pub fn cancel(ui: &mut Ui) {
    let go = with(|st| {
        if !st.running || st.canceling {
            return false;
        }
        st.canceling = true;
        true
    });
    if go {
        bexp::handle_cancel_export();
        render_actions(ui);
        sync_card();
    }
}

fn preview_action(ui: &mut Ui) {
    if with(|st| st.previewing) {
        bexp::handle_cancel_export();
        put(ui, "preview", "text", lang::t("export.canceling"));
        put(ui, "preview", "enabled", false);
        return;
    }
    if with(|st| st.running) {
        return;
    }
    let Some(mut req) = build_request(&file_name(ui)) else { return };
    let (from, sec) = with(|st| (st.play_sec, PREVIEW_SECS[st.prevsec.min(2)]));
    req.preview = Some((from, sec));
    pause_preview(ui);
    with(|st| {
        st.previewing = true;
        st.preview_pct = 0.0;
    });
    render_actions(ui);
    bexp::handle_export(req);
}

pub fn on_progress(pct: f64, source: &str) {
    with(|st| {
        // Exportação adotada (a tela pode ter sido reaberta noutro estado).
        st.running = true;
        if !source.is_empty() {
            st.running_id = source.to_owned();
        }
        st.pct = pct.clamp(0.0, 100.0);
    });
    if is_open() {
        with_ui(render_actions);
    }
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
    with_ui(move |ui| {
        if ok {
            if is_open() {
                close(ui);
            }
            crate::app_library::clear_selection();
            crate::app_view::show_toast(&lang::t("toast.exportDone"), &lang::t("toast.exportDoneMsg"), false, 6000);
        } else if canceled {
            crate::app_view::show_toast(&lang::t("toast.exportCanceled"), &lang::t("toast.exportCanceledMsg"), false, 4000);
        }
        if is_open() {
            render_actions(ui);
        }
    });
}

pub fn on_preview_progress(pct: f64) {
    with(|st| st.preview_pct = pct);
    if with(|st| st.previewing) {
        with_ui(render_actions);
    }
}

pub fn on_preview_done(ok: bool, canceled: bool, path: &str) {
    with(|st| st.previewing = false);
    let path = path.to_owned();
    with_ui(move |ui| {
        render_actions(ui);
        if ok && !path.is_empty() {
            crate::app_player::open(ui, &path);
        } else if canceled {
            crate::app_view::show_toast(&lang::t("export.preview.canceled"), "", false, 3000);
        }
    });
}

/// Exportação desta gravação em curso? (bloqueia excluir/mover/renomear na UI).
pub fn running_id() -> String {
    with(|st| if st.running { st.running_id.clone() } else { String::new() })
}

/// Barra de progresso no cartão da gravação de origem.
fn sync_card() {
    thread_local! {
        static LAST: RefCell<String> = const { RefCell::new(String::new()) };
    }
    let (rec, pct, canceling) = with(|st| (if st.running { st.running_id.clone() } else { String::new() }, st.pct, st.canceling));
    // A barra sai do cartão anterior quando a exportação acaba.
    let last = LAST.with(|l| std::mem::replace(&mut *l.borrow_mut(), rec.clone()));
    if !last.is_empty() && last != rec {
        crate::app_library::set_export_progress(&last, None, "");
    }
    if !rec.is_empty() {
        let label = if canceling { lang::t("export.canceling") } else { tw("recordings.exporting", &[("pct", &(pct.round() as i32).to_string())]) };
        crate::app_library::set_export_progress(&rec, Some(pct), &label);
    }
    crate::app_library::sync_export_button();
}

// ---------------------------------------------------------------------------
// Teclado
// ---------------------------------------------------------------------------

/// Teclas com a tela aberta (Esc fecha, espaço toca, Ctrl +/−/0 no zoom da linha do tempo).
pub fn key(ui: &mut Ui, k: &KeyEvent, typing: bool) {
    if k.key == Key::Escape {
        close(ui);
        return;
    }
    if k.modifiers.ctrl {
        let (ps, z) = with(|st| (st.play_sec, st.zoom));
        match k.key {
            Key::Char('+') | Key::Char('=') => set_zoom(ui, z * 1.6, Some(ps)),
            Key::Char('-') | Key::Char('_') => set_zoom(ui, z / 1.6, Some(ps)),
            Key::Char('0') => {
                set_zoom(ui, 1.0, Some(0.0));
                with(|st| st.scroll = 0.0);
                render_timeline(ui);
            }
            _ => {}
        }
        return;
    }
    if k.key == Key::Space && !typing {
        toggle_play(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_like_the_web() {
        assert_eq!(fmt_time(0.0), "00:00");
        assert_eq!(fmt_time(199.5), "03:20");
        assert_eq!(fmt_time(3725.0), "1:02:05");
        assert_eq!(parse_time("1:30"), Some(90.0));
        assert_eq!(parse_time("x"), None);
    }

    #[test]
    fn neighbor_parts_merge() {
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
    fn crf_runs_backwards() {
        let st = State { q_pos: 28, ..Default::default() };
        assert_eq!(crf(&st), 23);
        let st = State { q_pos: 51, ..Default::default() };
        assert_eq!(crf(&st), 0);
        assert_eq!(crf_band_key(23), "export.crfBandHigh");
    }

    #[test]
    fn speed_takes_comma_and_clamps() {
        let mut st = State { speed_text: "1,5".into(), ..Default::default() };
        assert_eq!(speed(&st), 1.5);
        st.speed_text = "9999".into();
        assert_eq!(speed(&st), 512.0);
        st.speed_text = "0.5".into();
        assert_eq!(speed(&st), 1.0);
    }
}

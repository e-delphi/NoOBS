//! Tela principal (coluna de fontes + bloco "Telas"): o cartão de gravar, o
//! buffer em memória, microfones e saídas com medidor, a legenda das faixas e
//! as telas com prévia. O desenho é o form `frmMain` (`forms/main.json`); aqui
//! entra o que o core conta pelo [`crate::view::View`] e os cliques vão direto
//! pros handlers do core.
//!
//! Só a main thread toca aqui, sempre com o `Ui` (pelo `app_view::with_ui`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use iron::prelude::*;

use crate::app_view::{main_form, with_ui};
use crate::view::{AudioMeter, Device, MonitorThumb, PendingKind, RecState, ReplayState};
use crate::{bridge, config, lang};

/// Cor de cada faixa isolada (2, 3, …), em ciclo. Sem verde/vermelho (a bolinha
/// de padrão, gravar/parar).
const TRACK_PALETTE: [u32; 10] = [
    0x3b82f6, 0x06b6d4, 0xf59e0b, 0xa855f7, 0xf472b6, 0x6366f1, 0x14b8a6, 0xc2410c, 0x0ea5e9, 0x8b5cf6,
];

fn rgb(hex: u32) -> Color {
    Color::rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn track_color(t: i32) -> Color {
    rgb(TRACK_PALETTE[((t - 2).max(0) as usize) % TRACK_PALETTE.len()])
}

/// Amplitude linear 0..1 → largura 0..1 em dB (−60 dB = 0, 0 dB = 1).
fn amp_to_bar(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    ((20.0 * v.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

const GREEN: u32 = 0x10b981;
const RED: u32 = 0xef4444;
const AMBER: u32 = 0xf59e0b;
/// Altura de uma linha de dispositivo e de uma da legenda.
const SOURCE_ROW: f32 = 45.0;
const LEGEND_ROW: f32 = 22.0;

#[derive(Default)]
struct State {
    monitors: Vec<Device>,
    webcams: Vec<Device>,
    mics: Vec<Device>,
    speakers: Vec<Device>,
    /// O medidor de cada dispositivo de áudio (id do dispositivo → componente).
    meters: HashMap<String, ComponentId>,
    rec: RecState,
    /// `None` antes do 1º estado (o som não toca no arranque).
    last_active: Option<bool>,
    /// Quando o cronômetro marcaria zero; o número do `set_timeout` dele.
    timer_start: Option<Instant>,
    timer_job: Option<u64>,
    finalizing: bool,
    pause_locked: bool,
    last_stop_sound: Option<Instant>,
    replay: ReplayState,
    replay_received: Option<Instant>,
    replay_job: Option<u64>,
    countdown_sec: u32,
    countdown_end: Option<Instant>,
    countdown_total: f32,
    countdown_job: Option<u64>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

/// Um componente do `frmMain` pelo nome.
fn c(ui: &Ui, name: &str) -> Option<ComponentId> {
    ui.find_in(main_form()?, name)
}

fn set(ui: &mut Ui, name: &str, prop: &str, value: impl Into<PropValue>) {
    if let Some(id) = c(ui, name) {
        let _ = ui.set_prop(id, prop, value);
    }
}

fn set_height(ui: &mut Ui, name: &str, h: f32) {
    if let Some(cb) = c(ui, name).and_then(|id| ui.control_mut(id)) {
        cb.set_height(h);
    }
}

/// Os textos do form vêm do idioma do NoOBS (`lang/*.json`).
fn apply_texts(ui: &mut Ui) {
    for (name, key) in [
        ("lblReplay", "replay.label"),
        ("lblMicsTitle", "sources.mics"),
        ("lblSpeakersTitle", "sources.speakers"),
        ("lblLegendTitle", "tracks.title"),
        ("lblScreens", "sources.monitors"),
    ] {
        set(ui, name, "text", lang::t(key));
    }
    for (name, key) in [("btnSettings", "header.settings"), ("btnCapture", "windowPicker.button"), ("btnRecord", "")] {
        if !key.is_empty() {
            set(ui, name, "hint", lang::t(key));
        }
    }
    if let Some(id) = c(ui, "lblScreens") {
        let w = ui.measure_text(&lang::t("sources.monitors"), &TextStyle::default()).width;
        if let Some(cb) = ui.control_mut(id) {
            cb.set_width(w * 16.0 / 14.0 + 4.0);
        }
    }
}

/// Liga a tela principal: textos, estado inicial. Uma vez, depois do form criado.
pub fn setup(ui: &mut Ui) {
    apply_texts(ui);
    render_recording(ui);
    render_replay(ui);
}

// ---------------------------------------------------------------------------
// Dispositivos
// ---------------------------------------------------------------------------

fn visible(list: &[Device]) -> impl Iterator<Item = &Device> {
    list.iter().filter(|d| !d.hidden)
}

fn count_text(list: &[Device]) -> String {
    let total = visible(list).count();
    let sel = visible(list).filter(|d| d.enabled).count();
    format!("{sel} / {total}")
}

/// Uma linha de dispositivo de áudio: a faixa colorida na borda, a bolinha do
/// padrão, o nome, o medidor e a chave. A linha inteira liga/desliga.
fn add_source_row(ui: &mut Ui, parent: ComponentId, d: &Device, locked: bool, meters: &mut HashMap<String, ComponentId>) {
    let Ok(row) = ui.create("Panel", Some(parent)) else { return };
    let _ = ui.set_prop(row, "style", "row");
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(SOURCE_ROW).set_padding([10.0, 8.0, 10.0, 8.0]).set_clip_children(false);
        if !locked {
            cb.set_cursor(CursorShape::Pointer);
        }
        cb.set_opacity(if locked { 0.5 } else { 1.0 });
    }
    // Faixa de 3 px colada na borda da janela (12 px pra fora da linha).
    if let Ok(stripe) = ui.create("Rectangle", Some(row)) {
        let color = if d.enabled && d.track > 1 {
            Some(track_color(d.track))
        } else if d.enabled {
            None
        } else {
            Some(ui.theme().colors.get("border_2").copied().unwrap_or(ui.theme().color("border")))
        };
        let _ = ui.set_prop(stripe, "style", "stripe");
        match color {
            Some(color) => {
                let _ = ui.set_prop(stripe, "fill", color.with_opacity(if d.enabled { 1.0 } else { 0.7 }));
            }
            None => {
                let _ = ui.set_prop(stripe, "visible", false);
            }
        }
        if let Some(cb) = ui.control_mut(stripe) {
            cb.set_rect(-22.0, 0.0, 3.0, SOURCE_ROW).set_hit_test(false);
        }
    }
    if let Ok(sw) = ui.create("Switch", Some(row)) {
        let _ = ui.set_prop(sw, "checked", d.enabled);
        let _ = ui.set_prop(sw, "enabled", !locked);
        if let Some(cb) = ui.control_mut(sw) {
            cb.set_align(Align::Right).set_width(46.0).set_margins([10.0, 4.0, 0.0, 4.0]);
        }
        let id = d.id.clone();
        ui.on(sw, "on_change", move |ui, sender, _| {
            let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
            toggle_source(ui, &id, on);
        });
    }
    let Ok(body) = ui.create("Layout", Some(row)) else { return };
    if let Some(cb) = ui.control_mut(body) {
        cb.set_align(Align::Client).set_hit_test(false);
    }
    if let Ok(line) = ui.create("Layout", Some(body)) {
        if let Some(cb) = ui.control_mut(line) {
            cb.set_align(Align::Top).set_height(17.0).set_hit_test(false);
        }
        if d.is_default {
            if let Ok(dot) = ui.create("Circle", Some(line)) {
                let _ = ui.set_prop(dot, "fill", rgb(0x22c55e));
                let _ = ui.set_prop(dot, "stroke_width", 0.0);
                let _ = ui.set_prop(dot, "hint", lang::t("sources.defaultDevice"));
                if let Some(cb) = ui.control_mut(dot) {
                    cb.set_align(Align::Left).set_width(7.0).set_margins([0.0, 5.0, 6.0, 5.0]);
                }
            }
        }
        if d.is_bluetooth {
            if let Ok(bt) = ui.create("IconView", Some(line)) {
                let _ = ui.set_prop(bt, "icon", "warning");
                let _ = ui.set_prop(bt, "color", rgb(AMBER));
                let hint = if meters_is_mic(d) { "sources.btMicWarn" } else { "sources.btSpeakerWarn" };
                let _ = ui.set_prop(bt, "hint", lang::t(hint));
                if let Some(cb) = ui.control_mut(bt) {
                    cb.set_align(Align::Left).set_width(13.0).set_margins([0.0, 2.0, 6.0, 2.0]);
                }
            }
        }
        if let Ok(name) = ui.create("Label", Some(line)) {
            let _ = ui.set_prop(name, "style", "row");
            let _ = ui.set_prop(name, "text", d.name.as_str());
            if let Some(cb) = ui.control_mut(name) {
                cb.set_align(Align::Client).set_hit_test(false);
            }
        }
    }
    if let Ok(meter) = ui.create("LevelMeter", Some(body)) {
        for (p, v) in [("stereo", true), ("gradient", true), ("show_peak", false), ("dim", !d.enabled)] {
            let _ = ui.set_prop(meter, p, v);
        }
        let _ = ui.set_prop(meter, "mid", 0.6);
        let _ = ui.set_prop(meter, "high", 0.75);
        if let Some(cb) = ui.control_mut(meter) {
            cb.set_align(Align::Top).set_height(7.0).set_margins([0.0, 4.0, 0.0, 0.0]);
        }
        meters.insert(d.id.clone(), meter);
    }
    let id = d.id.clone();
    let on = !d.enabled;
    if !locked {
        ui.on(row, "on_click", move |ui, _, _| toggle_source(ui, &id, on));
    }
}

/// O aviso do Bluetooth muda pra microfone/saída: o id do WASAPI de captura
/// começa com `{0.0.1`.
fn meters_is_mic(d: &Device) -> bool {
    d.id.starts_with("{0.0.1")
}

fn clear(ui: &mut Ui, parent: ComponentId) {
    for child in ui.children(parent).to_vec() {
        ui.free(child);
    }
}

fn refresh_sources(ui: &mut Ui) {
    let locked = with(|s| s.rec.active);
    let (mics, speakers) = with(|s| (visible(&s.mics).cloned().collect::<Vec<_>>(), visible(&s.speakers).cloned().collect::<Vec<_>>()));
    let mut meters = HashMap::new();
    for (list, name) in [(&mics, "lyMics"), (&speakers, "lySpeakers")] {
        let Some(parent) = c(ui, name) else { continue };
        clear(ui, parent);
        for d in list.iter() {
            add_source_row(ui, parent, d, locked, &mut meters);
        }
        set_height(ui, name, list.len() as f32 * SOURCE_ROW);
    }
    with(|s| s.meters = meters);
    let (mc, sc) = with(|s| (count_text(&s.mics), count_text(&s.speakers)));
    set(ui, "lblMicsCount", "text", mc);
    set(ui, "lblSpeakersCount", "text", sc);
    // A coluna rola: a altura do conteúdo é a soma das seções.
    let content = 2.0 * (22.0 + 15.0 + 12.0) + (mics.len() + speakers.len()) as f32 * SOURCE_ROW + 12.0;
    set_height(ui, "lySources", content);
    render_legend(ui);
    ui.layout();
}

/// Uma linha da legenda das faixas.
struct LegendItem {
    kind: &'static str,
    label: String,
    name: String,
    hint: String,
    color: Color,
}

/// Legenda das faixas: Padrão, Faixa 1 (mix), faixas com o dispositivo
/// padrão, de microfone, de saída, e "sem faixa".
fn legend_items(s: &State) -> Vec<LegendItem> {
    struct Info {
        names: Vec<String>,
        has_default: bool,
        mic: bool,
        spk: bool,
    }
    let mut tracks: std::collections::BTreeMap<i32, Info> = Default::default();
    let mut disabled: Vec<String> = Vec::new();
    let mut any_default = false;
    for (list, is_mic) in [(&s.mics, true), (&s.speakers, false)] {
        for d in visible(list) {
            any_default |= d.is_default;
            if d.track >= 2 {
                let e = tracks.entry(d.track).or_insert(Info { names: vec![], has_default: false, mic: false, spk: false });
                e.names.push(d.name.clone());
                if is_mic {
                    e.mic = true;
                } else {
                    e.spk = true;
                }
                e.has_default |= d.is_default && d.enabled;
            } else if !d.enabled {
                disabled.push(d.name.clone());
            }
        }
    }
    let mut out = Vec::new();
    if tracks.is_empty() && disabled.is_empty() && !any_default {
        return out;
    }
    let item = |kind: &'static str, label: String, name: String, hint: String, color: Color| LegendItem { kind, label, name, hint, color };
    if any_default {
        out.push(item("default", lang::t("tracks.default"), lang::t("tracks.defaultDevice"), lang::t("tracks.defaultHint"), Color::TRANSPARENT));
    }
    if !tracks.is_empty() {
        out.push(item("mix", lang::t("tracks.mix"), lang::t("tracks.mixLabel"), lang::t("tracks.mixHint"), Color::TRANSPARENT));
    }
    let render = |t: i32, info: &Info| {
        let label = match info.names.len() {
            1 => info.names[0].clone(),
            2 => info.names.join(", "),
            n => lang::t_with("tracks.devicesGrouped_other", &[("count", &n.to_string())]),
        };
        item("track", lang::t_with("tracks.track", &[("n", &t.to_string())]), label, info.names.join(", "), track_color(t))
    };
    for (t, info) in tracks.iter().filter(|(_, i)| i.has_default) {
        out.push(render(*t, info));
    }
    for (t, info) in tracks.iter().filter(|(_, i)| !i.has_default && i.mic && !i.spk) {
        out.push(render(*t, info));
    }
    for (t, info) in tracks.iter().filter(|(_, i)| !i.has_default && i.spk) {
        out.push(render(*t, info));
    }
    if !disabled.is_empty() {
        let label = if disabled.len() == 1 {
            disabled[0].clone()
        } else {
            lang::t_with("tracks.devicesDisabled_other", &[("count", &disabled.len().to_string())])
        };
        out.push(item("off", lang::t("tracks.noTrack"), label, disabled.join(", "), Color::TRANSPARENT));
    }
    out
}

fn render_legend(ui: &mut Ui) {
    let items = with(|s| legend_items(s));
    let Some(parent) = c(ui, "lyLegend") else { return };
    clear(ui, parent);
    let muted = ui.theme().color("text_muted");
    let off = ui.theme().colors.get("border_2").copied().unwrap_or(muted);
    for it in &items {
        let Ok(row) = ui.create("Layout", Some(parent)) else { continue };
        let _ = ui.set_prop(row, "hint", it.hint.as_str());
        if let Some(cb) = ui.control_mut(row) {
            cb.set_align(Align::Top).set_height(LEGEND_ROW).set_margins([6.0, 0.0, 6.0, 0.0]);
            cb.set_opacity(if it.kind == "off" { 0.7 } else { 1.0 });
        }
        let marker = if it.kind == "default" { ui.create("Circle", Some(row)) } else { ui.create("Rectangle", Some(row)) };
        if let Ok(m) = marker {
            let (fill, w, margins) = match it.kind {
                "default" => (rgb(0x22c55e), 8.0, (0.0, 7.0, 10.0, 7.0)),
                "mix" => (muted, 3.0, (2.0, 4.0, 13.0, 4.0)),
                "off" => (off, 3.0, (2.0, 4.0, 13.0, 4.0)),
                _ => (it.color, 3.0, (2.0, 4.0, 13.0, 4.0)),
            };
            let _ = ui.set_prop(m, "fill", fill);
            let _ = ui.set_prop(m, "stroke_width", 0.0);
            if let Some(cb) = ui.control_mut(m) {
                cb.set_align(Align::Left).set_width(w).set_margins([margins.0, margins.1, margins.2, margins.3]).set_hit_test(false);
            }
        }
        if let Ok(label) = ui.create("Label", Some(row)) {
            let _ = ui.set_prop(label, "style", "legend_track");
            let _ = ui.set_prop(label, "text", it.label.as_str());
            if let Some(cb) = ui.control_mut(label) {
                cb.set_align(Align::Left).set_width(56.0).set_hit_test(false);
            }
        }
        if let Ok(name) = ui.create("Label", Some(row)) {
            let _ = ui.set_prop(name, "style", "legend");
            let _ = ui.set_prop(name, "text", it.name.as_str());
            if let Some(cb) = ui.control_mut(name) {
                cb.set_align(Align::Client).set_hit_test(false);
            }
        }
    }
    let h = if items.is_empty() { 0.0 } else { 1.0 + 16.0 + 20.0 + 6.0 + items.len() as f32 * LEGEND_ROW + 12.0 };
    set_height(ui, "pnlLegend", h);
    set(ui, "pnlLegend", "visible", !items.is_empty());
}

/// Bloco "Telas": monitores na posição real e webcams à direita.
fn rebuild_displays(ui: &mut Ui) {
    let (mons, cams, locked) = with(|s| (visible(&s.monitors).cloned().collect::<Vec<_>>(), visible(&s.webcams).cloned().collect::<Vec<_>>(), s.rec.active));
    let max_x = mons.iter().map(|d| d.x + d.width).max().unwrap_or(0);
    let mut screens: Vec<ScreenItem> = mons
        .iter()
        .map(|d| ScreenItem {
            id: d.id.clone(),
            name: if d.name.is_empty() { lang::t("display.monitor") } else { d.name.clone() },
            x: d.x,
            y: d.y,
            width: d.width,
            height: d.height,
            enabled: d.enabled,
            camera: false,
            image: None,
        })
        .collect();
    // Webcams à direita do bloco dos monitores, em y = 0.
    let mut cam_x = max_x;
    for d in &cams {
        let (w, h) = (if d.width > 0 { d.width } else { 1280 }, if d.height > 0 { d.height } else { 720 });
        screens.push(ScreenItem {
            id: d.id.clone(),
            name: if d.name.is_empty() { lang::t("display.webcam") } else { d.name.clone() },
            x: cam_x,
            y: 0,
            width: w,
            height: h,
            enabled: d.enabled,
            camera: true,
            image: None,
        });
        cam_x += w;
    }
    if let Some(id) = c(ui, "scrDisplays") {
        if let Some(layout) = ui.get_mut::<ScreenLayout>(id) {
            layout.set_screens(screens);
            layout.locked = locked;
        }
        ui.invalidate();
    }
    // Contagem e "3840×2160" da composição marcada, com o aviso se passar do
    // limite do encoder.
    let total = mons.len() + cams.len();
    let sel = mons.iter().chain(cams.iter()).filter(|d| d.enabled).count();
    set(ui, "lblScreensCount", "text", format!("{sel} / {total}"));
    let (mut cw, mut ch) = (0, 0);
    for d in mons.iter().chain(cams.iter()).filter(|d| d.enabled) {
        cw += d.width;
        ch = ch.max(d.height);
    }
    if cw > 0 && ch > 0 {
        let max = codec_max_dim();
        let over = cw > max || ch > max;
        let text = if over { format!("{cw}×{ch}  ⚠") } else { format!("{cw}×{ch}") };
        set(ui, "lblScreensMeta", "text", text);
        let hint = if over { lang::t_with("warning.canvasOverflow", &[("max", &max.to_string())]) } else { String::new() };
        set(ui, "lblScreensMeta", "hint", hint);
    } else {
        set(ui, "lblScreensMeta", "text", "");
    }
}

/// Maior dimensão do encoder escolhido: H.264 por hardware para em 4096; no
/// automático, vale o H.264 por hardware se a máquina tiver.
fn codec_max_dim() -> i32 {
    match config::get_config_str("codec", "auto").as_str() {
        "h264-hw" => 4096,
        "h264-sw" | "hevc-hw" | "av1-hw" | "av1-sw" => 8192,
        _ => {
            if bridge::last_encoder_caps().is_some_and(|c| c.h264_hw) {
                4096
            } else {
                8192
            }
        }
    }
}

fn update_can_record(ui: &mut Ui) {
    let (any, recording) = with(|s| ([&s.monitors, &s.webcams, &s.mics, &s.speakers].iter().any(|l| visible(l).any(|d| d.enabled)), s.rec.active));
    set(ui, "btnRecord", "enabled", any || recording);
    set(ui, "btnRecord", "hint", if any || recording { String::new() } else { lang::t("record.needDevice") });
}

/// "Microfone adicionado: X · Saída removida: Y · e mais N".
fn format_changes(changes: &[String]) -> String {
    const MAX: usize = 3;
    if changes.len() <= MAX {
        return changes.join(" · ");
    }
    format!("{} · {}", changes[..MAX].join(" · "), lang::t_with("toast.andMore", &[("count", &(changes.len() - MAX).to_string())]))
}

pub fn on_monitors(list: &[Device], changes: &[String]) {
    let list = list.to_vec();
    let changes = changes.to_vec();
    with_ui(move |ui| {
        with(|s| s.monitors = list);
        rebuild_displays(ui);
        update_can_record(ui);
        if !changes.is_empty() {
            crate::app_view::show_toast(&lang::t("toast.monitorsUpdated"), &format_changes(&changes), true, 4500);
        }
    });
}

pub fn on_webcams(list: &[Device], changes: &[String]) {
    let list = list.to_vec();
    let changes = changes.to_vec();
    with_ui(move |ui| {
        with(|s| s.webcams = list);
        rebuild_displays(ui);
        update_can_record(ui);
        // Mesmo título do áudio: um hot-plug USB que mexe nos dois vira um aviso só.
        if !changes.is_empty() {
            crate::app_view::show_toast(&lang::t("toast.devicesUpdated"), &format_changes(&changes), true, 4500);
        }
    });
}

pub fn on_audio_sources(mics: &[Device], speakers: &[Device], silent: bool, changes: &[String]) {
    let (mics, speakers, changes) = (mics.to_vec(), speakers.to_vec(), changes.to_vec());
    with_ui(move |ui| {
        with(|s| {
            s.mics = mics;
            s.speakers = speakers;
        });
        refresh_sources(ui);
        update_can_record(ui);
        if !silent {
            crate::app_view::show_toast(&lang::t("toast.devicesUpdated"), &format_changes(&changes), true, 4500);
        }
    });
}

pub fn on_audio_meters(items: &[AudioMeter]) {
    let items = items.to_vec();
    with_ui(move |ui| {
        let meters = with(|s| s.meters.clone());
        for m in &items {
            let Some(&id) = meters.get(&m.id) else { continue };
            let Some(meter) = ui.get_mut::<LevelMeter>(id) else { continue };
            meter.stereo = m.channels >= 2;
            meter.muted = m.muted;
            meter.value = amp_to_bar(m.left);
            meter.value_right = amp_to_bar(m.right);
        }
        ui.invalidate();
    });
}

pub fn on_monitor_thumbs(items: &[MonitorThumb]) {
    let items: Vec<(String, Option<Image>)> = items
        .iter()
        .map(|t| {
            let image = (t.rgb.len() == (t.width * t.height * 3) as usize && !t.rgb.is_empty()).then(|| {
                let mut rgba = Vec::with_capacity(t.rgb.len() / 3 * 4);
                for px in t.rgb.chunks_exact(3) {
                    rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
                Image::rgba(t.width, t.height, rgba)
            });
            (t.id.clone(), image)
        })
        .collect();
    with_ui(move |ui| {
        let Some(id) = c(ui, "scrDisplays") else { return };
        if let Some(layout) = ui.get_mut::<ScreenLayout>(id) {
            for (screen, image) in items {
                layout.set_image(&screen, image);
            }
        }
        ui.invalidate();
    });
}

pub fn on_device_change_pending(_kind: PendingKind, _pending: bool) {}

pub fn on_refresh_busy(busy: bool, what: &str) {
    // Começar a gravar não cobre a coluna: só desativa o resto.
    if what == "starting" {
        with_ui(move |ui| {
            set(ui, "scrSources", "enabled", !busy);
            set(ui, "scrDisplays", "enabled", !busy);
        });
    }
}

/// Liga/desliga um dispositivo: a tela muda na hora; o core grava a
/// preferência e, no áudio, recalcula as faixas e manda a lista de volta.
fn toggle_source(ui: &mut Ui, id: &str, enabled: bool) {
    with(|s| {
        for list in [&mut s.monitors, &mut s.webcams, &mut s.mics, &mut s.speakers] {
            if let Some(d) = list.iter_mut().find(|d| d.id == id) {
                d.enabled = enabled;
            }
        }
    });
    rebuild_displays(ui);
    let id_owned = id.to_owned();
    // A lista é refeita fora deste evento (a linha clicada sai da árvore).
    ui.set_timeout(0, |ui| refresh_sources(ui));
    update_can_record(ui);
    bridge::handle_toggle_source(&id_owned, enabled);
}

/// O clique numa tela do bloco "Telas" (o `ScreenLayout` já marcou).
pub fn display_change(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let EventArgs::Text(id) = args else { return };
    let on = ui.get::<ScreenLayout>(sender).is_some_and(|l| l.is_enabled(id));
    let id = id.clone();
    toggle_source(ui, &id, on);
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

/// "HH:MM:SS" e ".cc" (centésimos) do cronômetro.
fn timer_text(ms: u64) -> (String, String) {
    let s = ms / 1000;
    (format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60), format!(".{:02}", ms % 1000 / 10))
}

fn render_timer(ui: &mut Ui, ms: u64) {
    let (a, b) = timer_text(ms);
    set(ui, "lblTimer", "text", a);
    set(ui, "lblTimerMs", "text", b);
}

/// Cronômetro a 50 ms, da "partida" pra frente.
fn start_timer(ui: &mut Ui, elapsed_ms: u64) {
    stop_timer(ui);
    with(|s| s.timer_start = Some(Instant::now() - Duration::from_millis(elapsed_ms)));
    render_timer(ui, elapsed_ms);
    tick_timer(ui);
}

fn tick_timer(ui: &mut Ui) {
    let job = ui.set_timeout(50, |ui| {
        let Some(start) = with(|s| s.timer_start) else { return };
        render_timer(ui, start.elapsed().as_millis() as u64);
        tick_timer(ui);
    });
    with(|s| s.timer_job = Some(job));
}

fn stop_timer(ui: &mut Ui) {
    if let Some(job) = with(|s| {
        s.timer_start = None;
        s.timer_job.take()
    }) {
        ui.clear_timeout(job);
    }
}

/// O cartão no estado da gravação: botão, status, cores, texto do botão.
fn render_recording(ui: &mut Ui) {
    let (active, paused, window, finalizing, countdown) =
        with(|s| (s.rec.active, s.rec.paused, s.rec.window.clone(), s.finalizing, s.countdown_sec));
    set(ui, "btnRecord", "recording", active);
    set(ui, "btnRecord", "paused", paused);
    set(ui, "btnRecord", "finalizing", finalizing);
    let status = if countdown > 0 {
        lang::t_with("record.countdown", &[("sec", &countdown.to_string())])
    } else if active && paused {
        lang::t("record.statusPaused")
    } else if active && !window.is_empty() {
        lang::t_with("record.statusRecordingWindow", &[("title", &window)])
    } else if active {
        lang::t("record.statusRecording")
    } else if finalizing {
        lang::t("record.finalizing")
    } else {
        lang::t("record.statusReady")
    };
    set(ui, "lblStatus", "text", status);
    let (dot, status_color) = if active && paused {
        (rgb(AMBER), Some(rgb(AMBER)))
    } else if active {
        (rgb(RED), Some(rgb(RED)))
    } else {
        (rgb(GREEN), None)
    };
    set(ui, "dotStatus", "fill", dot);
    set(ui, "lblStatus", "color", status_color.map_or(PropValue::Null, PropValue::Color));
    set(ui, "lblStatus", "font_weight", if active { "semibold" } else { "default" });
    set(ui, "lblTimer", "color", if active && paused { PropValue::Color(rgb(AMBER)) } else { PropValue::Null });
    set(ui, "lblRecordAction", "text", lang::t(if active { "record.stop" } else { "record.start" }));
    set(ui, "pnlRecordCard", "border_color", if active { PropValue::Color(rgb(RED)) } else { PropValue::Null });
    set(ui, "btnPause", "enabled", active);
    set(ui, "btnPause", "icon", if paused { "play" } else { "pause" });
    set(ui, "btnPause", "hint", lang::t(if paused { "record.resume" } else { "record.pause" }));
    set(ui, "btnCapture", "enabled", !active);
    let size = with(|s| s.rec.size_text.clone()).filter(|_| active).unwrap_or_default();
    set(ui, "lblSize", "text", size);
}

pub fn on_recording_state(st: &RecState) {
    let st = st.clone();
    with_ui(move |ui| {
        // Som de início: só na virada pra gravando (não no 1º estado do arranque).
        let prev = with(|s| s.last_active.replace(st.active));
        if prev == Some(false) && st.active && config::get_config_bool("playSoundOnRecord", true) {
            crate::sounds::play_start();
        }
        let changed = prev != Some(st.active);
        with(|s| {
            if let Some(t) = &st.size_text {
                s.rec.size_text = Some(t.clone());
            }
            if !st.active {
                s.rec.size_text = None;
            }
            s.rec.active = st.active;
            s.rec.paused = st.active && st.paused;
            s.rec.window = if st.active { st.window.clone() } else { String::new() };
            s.pause_locked = false;
        });
        render_recording(ui);
        let (active, paused, running) = with(|s| (s.rec.active, s.rec.paused, s.timer_job.is_some()));
        if active {
            // Pausada: o cronômetro para no tempo gravado (o core manda o tempo já
            // congelado). Gravando: anda a partir do tempo que o core mandou.
            let ms = st.elapsed_ms.unwrap_or(0);
            if paused {
                stop_timer(ui);
                render_timer(ui, ms);
            } else if st.elapsed_ms.is_some() || !running {
                start_timer(ui, ms);
            }
        } else {
            stop_timer(ui);
            render_timer(ui, 0);
        }
        // Gravando, as fontes travam (e destravam no fim).
        if changed {
            refresh_sources(ui);
            rebuild_displays(ui);
            update_can_record(ui);
            render_replay(ui);
        }
    });
}

pub fn on_recording_finalizing(active: bool) {
    with_ui(move |ui| {
        with(|s| s.finalizing = active);
        render_recording(ui);
    });
}

/// Som de parada com debounce de 800 ms: o clique toca na hora e o aviso do
/// core chega logo depois.
pub fn play_stop_sound() {
    let now = Instant::now();
    let recent = with(|s| s.last_stop_sound.replace(now)).is_some_and(|t| now.duration_since(t) < Duration::from_millis(800));
    if !recent {
        crate::sounds::play_stop();
    }
}

pub fn record_click(_ui: &mut Ui) {
    let recording = with(|s| s.rec.active);
    if recording {
        // Som de parada AGORA (o arquivo pode levar centenas de ms pra fechar).
        if config::get_config_bool("playSoundOnRecord", true) {
            play_stop_sound();
        }
        bridge::handle_record_stop();
    } else {
        bridge::handle_record_start_button();
    }
}

/// "Capturar...": o seletor (gravando, não abre).
pub fn capture_click(ui: &mut Ui) {
    let recording = with(|s| s.rec.active);
    crate::app_picker::open(ui, recording);
}

/// Trava até o próximo estado: dois cliques rápidos cruzariam pausa e retomada.
pub fn pause_click(_ui: &mut Ui) {
    let (active, paused, locked) = with(|s| (s.rec.active, s.rec.paused, s.pause_locked));
    if !active || locked {
        return;
    }
    with(|s| s.pause_locked = true);
    bridge::handle_record_pause(!paused);
}

// ---------------------------------------------------------------------------
// Contagem do atraso (atalho com atraso)
// ---------------------------------------------------------------------------

pub fn on_record_countdown(sec: u32, total: u32) {
    with_ui(move |ui| {
        let prev = with(|s| std::mem::replace(&mut s.countdown_sec, sec));
        if sec == 0 {
            if let Some(job) = with(|s| {
                s.countdown_end = None;
                s.countdown_job.take()
            }) {
                ui.clear_timeout(job);
            }
            set(ui, "btnRecord", "countdown", 0i64);
            render_recording(ui);
            return;
        }
        // Contagem nova (ou que voltou a subir): o anel recomeça do ponto certo.
        if prev == 0 || sec > prev || with(|s| s.countdown_end.is_none()) {
            let tot = total.max(sec) as f32;
            with(|s| {
                s.countdown_total = tot;
                s.countdown_end = Some(Instant::now() + Duration::from_secs(sec as u64));
            });
        }
        set(ui, "btnRecord", "countdown", sec as i64);
        render_recording(ui);
        if with(|s| s.countdown_job.is_none()) {
            tick_countdown(ui);
        }
    });
}

/// O anel da contagem esvazia a cada quadro.
fn tick_countdown(ui: &mut Ui) {
    let (end, total) = with(|s| (s.countdown_end, s.countdown_total));
    let Some(end) = end else {
        with(|s| s.countdown_job = None);
        return;
    };
    let left = end.saturating_duration_since(Instant::now()).as_secs_f32();
    set(ui, "btnRecord", "countdown_fraction", if total > 0.0 { left / total } else { 0.0 });
    let job = ui.set_timeout(16, tick_countdown);
    with(|s| s.countdown_job = Some(job));
}

// ---------------------------------------------------------------------------
// Buffer em memória
// ---------------------------------------------------------------------------

/// "m:ss".
fn fmt_min_sec(sec: i64) -> String {
    format!("{}:{:02}", sec / 60, sec % 60)
}

/// Texto e botões da linha do buffer. O "guardado" anda sozinho entre um
/// estado e outro (o core só manda quando algo muda), limitado ao teto.
fn render_replay(ui: &mut Ui) {
    let (st, received, recording) = with(|s| (s.replay.clone(), s.replay_received, s.rec.active));
    let max = if st.max_sec > 0 { st.max_sec } else { 300 };
    let text = if st.active {
        if st.saving {
            lang::t("replay.statusSaving")
        } else {
            let ms = st.since_ms + received.map_or(0, |r| r.elapsed().as_millis() as u64);
            let held = ((ms / 1000) as i64).min(st.max_sec.max(0));
            lang::t_with("replay.statusHolding", &[("held", &fmt_min_sec(held)), ("max", &fmt_min_sec(st.max_sec.max(0)))])
        }
    } else if st.enabled && recording {
        lang::t("replay.statusPaused")
    } else {
        lang::t_with("replay.statusOff", &[("max", &fmt_min_sec(max))])
    };
    set(ui, "lblReplayStatus", "text", text);
    set(ui, "swReplay", "checked", st.enabled);
    set(ui, "btnReplaySave", "visible", st.active);
    set(ui, "btnReplaySave", "enabled", !st.saving);
    let hint = if st.hotkey.is_empty() { lang::t("replay.saveHintNoKey") } else { lang::t_with("replay.saveHint", &[("spec", &st.hotkey)]) };
    set(ui, "btnReplaySave", "hint", hint);
    let border2 = ui.theme().colors.get("border_2").copied().unwrap_or(ui.theme().color("border"));
    set(ui, "dotReplay", "fill", if st.active { rgb(GREEN) } else { border2 });
}

pub fn on_replay_state(st: &ReplayState) {
    let st = st.clone();
    with_ui(move |ui| {
        with(|s| {
            s.replay = st.clone();
            s.replay_received = Some(Instant::now());
        });
        render_replay(ui);
        let ticking = with(|s| s.replay_job.is_some());
        if st.active && !ticking {
            tick_replay(ui);
        } else if !st.active {
            if let Some(job) = with(|s| s.replay_job.take()) {
                ui.clear_timeout(job);
            }
        }
    });
}

fn tick_replay(ui: &mut Ui) {
    let job = ui.set_timeout(1000, |ui| {
        render_replay(ui);
        tick_replay(ui);
    });
    with(|s| s.replay_job = Some(job));
}

pub fn on_replay_saved(name: &str, duration_sec: i32) {
    crate::app_view::show_toast(
        &lang::t("replay.saved"),
        &lang::t_with("replay.savedBody", &[("name", name), ("dur", &fmt_min_sec(duration_sec.max(0) as i64))]),
        false,
        6000,
    );
}

pub fn on_replay_restarted() {
    crate::app_view::show_toast(&lang::t("replay.restarted"), &lang::t("replay.restartedBody"), false, 6000);
}

/// A linha inteira do buffer (ou a chave) liga/desliga.
pub fn replay_toggle(_ui: &mut Ui) {
    let on = with(|s| s.replay.enabled);
    bridge::handle_set_replay_enabled(!on);
}

/// A chave já mudou de estado: segue o que ela diz (sem inverter de novo).
pub fn replay_switch(ui: &mut Ui, sender: ComponentId) {
    let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
    if on != with(|s| s.replay.enabled) {
        bridge::handle_set_replay_enabled(on);
    }
}

pub fn replay_save(_ui: &mut Ui) {
    let (active, saving) = with(|s| (s.replay.active, s.replay.saving));
    if active && !saving {
        bridge::handle_save_replay();
    }
}

/// Troca de idioma: os textos montados aqui são refeitos.
pub fn on_language_changed(ui: &mut Ui) {
    apply_texts(ui);
    render_recording(ui);
    refresh_sources(ui);
    rebuild_displays(ui);
    render_replay(ui);
}

thread_local! {
    /// Página da versão nova (o "Abrir página" do aviso).
    static UPDATE_URL: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Versão nova: o aviso no topo da coluna (dispensar vale só pra sessão).
pub fn show_update_banner(ui: &mut Ui, latest: &str, url: &str) {
    UPDATE_URL.with(|u| *u.borrow_mut() = url.to_owned());
    set(ui, "lblUpdTitle", "text", lang::t("banner.updateTitle"));
    set(ui, "lblUpdBody", "text", lang::t_with("banner.updateBody", &[("version", latest)]));
    set(ui, "btnUpdOpen", "visible", !url.is_empty());
    set(ui, "pnlUpdate", "visible", true);
}

pub fn hide_update_banner(ui: &mut Ui) {
    set(ui, "pnlUpdate", "visible", false);
}

pub fn open_update_page(ui: &mut Ui) {
    let url = UPDATE_URL.with(|u| u.borrow().clone());
    if !url.is_empty() {
        bridge::handle_open_url(&url);
    }
    hide_update_banner(ui);
}

/// Selo da transcrição no cabeçalho do cartão: (mostrar, rodando, número, dica).
pub fn on_transcribe_chip(chip: (bool, bool, i32, String)) {
    with_ui(move |ui| {
        let text = format!("{} {}", if chip.1 { "◌" } else { "≡" }, chip.2);
        set(ui, "lblTrChip", "text", text);
        set(ui, "lblTrChip", "hint", chip.3.as_str());
        set(ui, "lblTrChip", "visible", chip.0);
    });
}

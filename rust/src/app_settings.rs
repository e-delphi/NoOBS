//! Presenter das Configurações: a camada `pnlSettings` do `forms/main.json`
//! (barra lateral com as abas, cabeçalho, área rolável) e as linhas de cada
//! aba, montadas aqui com um construtor de linhas (título, dica e o controle
//! à direita), porque os textos e as listas saem do idioma e do estado do
//! core.
//!
//! Lê o estado direto do core (config, caps, filas) e cada mudança chama o
//! handler do core na hora — sem botão Salvar. Os dados que o core empurra
//! (fila de transcrição, motor local, atalhos, teste de qualidade…) chegam
//! pelo `View` e são guardados aqui pra redesenhar.

use std::cell::RefCell;
use std::collections::HashMap;

use iron::prelude::*;

use crate::app_view::{main_form, with_ui};
use crate::bridge::{self, hotkeys as bhk, recording, replay, settings, shell, transcribe as btr, video_test};
use crate::hotkeys::{Item, Mode};
use crate::types::GpuVendor;
use crate::view::{Device, LocalAsrInfo, QueueItem, ReplayState, RootFolder, TranscribeState, UpdateInfo, VideoTestState};
use crate::{config, encoder, hotkey, lang, log};

const AUDIO_BITRATES: [i32; 6] = [96, 128, 160, 192, 256, 320];
const AUDIO_BITRATE_DEFAULT: i32 = 192;
const REPLAY_SEC_OPTIONS: [i64; 8] = [30, 60, 120, 300, 600, 900, 1800, 3600];
const REPLAY_MB_OPTIONS: [i64; 16] = [512, 1024, 2048, 3072, 4096, 5120, 6144, 7168, 8192, 10240, 12288, 14336, 16384, 20480, 24576, 28672];
const REPLAY_DEFAULT_SEC: i64 = 300;
const REPLAY_DEFAULT_MB: i64 = 2048;
const CORNERS: [&str; 4] = ["top-left", "top-right", "bottom-left", "bottom-right"];
const CODECS: [&str; 6] = ["auto", "h264-hw", "h264-sw", "av1-hw", "av1-sw", "hevc-hw"];
const LIBRARY_MODES: [&str; 3] = ["auto", "always", "off"];
const TABS: [&str; 11] = ["general", "onrecord", "files", "recording", "audio", "replay", "hotkeys", "devices", "transcribe", "updates", "about"];
/// Idiomas oferecidos pra transcrição (o nome no próprio idioma).
const TRANSCRIBE_LANGS: [(&str, &str); 16] = [
    ("pt", "Português"),
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("tr", "Türkçe"),
    ("ar", "العربية"),
    ("hi", "हिन्दी"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("zh", "中文"),
];
const HK_MODES: [Mode; 4] = [Mode::Record, Mode::Window, Mode::Area, Mode::Replay];
const HK_DELAYS: [u32; 6] = [0, 3, 5, 10, 15, 30];
const VT_SVT_FLOOR_LEVEL: i32 = 3;
/// A largura do controle à direita de uma linha.
const CTL_W: f32 = 300.0;

/// Teclas do seletor de atalho: (valor, rótulo, é título de grupo).
fn key_table() -> Vec<(String, String, bool)> {
    let mut v: Vec<(String, String, bool)> = Vec::new();
    let group = |v: &mut Vec<(String, String, bool)>, key: &str| v.push((String::new(), lang::t(key), true));
    let plain = |v: &mut Vec<(String, String, bool)>, k: &str| v.push((k.to_owned(), k.to_owned(), false));
    let tr = |v: &mut Vec<(String, String, bool)>, k: &str, key: &str| v.push((k.to_owned(), lang::t(key), false));
    group(&mut v, "settings.hotkey.groups.function");
    for i in 1..=12 {
        plain(&mut v, &format!("F{i}"));
    }
    group(&mut v, "settings.hotkey.groups.letters");
    for c in 'A'..='Z' {
        plain(&mut v, &c.to_string());
    }
    group(&mut v, "settings.hotkey.groups.numbers");
    for c in '0'..='9' {
        plain(&mut v, &c.to_string());
    }
    group(&mut v, "settings.hotkey.groups.special");
    tr(&mut v, "Space", "settings.hotkey.keys.space");
    for k in ["Enter", "Tab", "Esc", "Insert", "Delete", "Home", "End"] {
        plain(&mut v, k);
    }
    tr(&mut v, "PageUp", "settings.hotkey.keys.pageUp");
    tr(&mut v, "PageDown", "settings.hotkey.keys.pageDown");
    plain(&mut v, "Backspace");
    plain(&mut v, "Pause/Break");
    group(&mut v, "settings.hotkey.groups.arrows");
    tr(&mut v, "Left", "settings.hotkey.keys.left");
    tr(&mut v, "Right", "settings.hotkey.keys.right");
    tr(&mut v, "Up", "settings.hotkey.keys.up");
    tr(&mut v, "Down", "settings.hotkey.keys.down");
    group(&mut v, "settings.hotkey.groups.numpad");
    for i in 0..10 {
        v.push((format!("Numpad{i}"), format!("Num {i}"), false));
    }
    for (k, l) in [("NumpadAdd", "Num +"), ("NumpadSubtract", "Num -"), ("NumpadMultiply", "Num *"), ("NumpadDivide", "Num /"), ("NumpadDecimal", "Num .")] {
        v.push((k.to_owned(), l.to_owned(), false));
    }
    group(&mut v, "settings.hotkey.groups.chars");
    for k in ["-", "=", ",", ".", ";", "/", "`", "[", "\\", "]", "'"] {
        plain(&mut v, k);
    }
    group(&mut v, "settings.hotkey.groups.media");
    tr(&mut v, "MediaPlayPause", "settings.hotkey.media.playPause");
    tr(&mut v, "MediaStop", "settings.hotkey.media.stop");
    tr(&mut v, "MediaPrev", "settings.hotkey.media.prev");
    tr(&mut v, "MediaNext", "settings.hotkey.media.next");
    v
}

#[derive(Default)]
struct State {
    open: bool,
    tab: String,
    fps_presets: Vec<i32>,
    quality: i32,
    fps: i32,
    keyframe: i32,
    monitors: Vec<Device>,
    webcams: Vec<Device>,
    mics: Vec<Device>,
    speakers: Vec<Device>,
    replay: Option<ReplayState>,
    hk_items: Vec<Item>,
    hk_status: Vec<&'static str>,
    hk_delay_max: u32,
    hk_defining: Option<usize>,
    /// Modificadores marcados sem tecla ficam só na tela até a tecla chegar.
    hk_pending: HashMap<usize, (bool, bool, bool, bool)>,
    tr: TranscribeState,
    tr_pending: usize,
    tr_cloud: usize,
    tr_queue: Vec<QueueItem>,
    tr_cancel_clicked: bool,
    tr_eta_shown: i64,
    tr_eta_for: String,
    tr_last_toast_error: String,
    tr_job: Option<u64>,
    asr: Option<LocalAsrInfo>,
    asr_confirm_remove: bool,
    folders: Vec<RootFolder>,
    update: Option<UpdateInfo>,
    checking: bool,
    vt: VtState,
    /// Rótulos que mudam sem refazer a aba (chave → componente).
    slots: HashMap<&'static str, ComponentId>,
    /// Envio adiado de um controle que muda enquanto arrasta (slider, texto).
    debounce: HashMap<&'static str, u64>,
}

#[derive(Default)]
struct VtState {
    running: bool,
    capturing: bool,
    countdown: i32,
    pct: i64,
    level: i32,
    result: Option<VideoTestState>,
    a: i32,
    b: i32,
    job: Option<u64>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { tab: "general".into(), quality: 5, fps: 30, keyframe: 2, ..Default::default() });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn c(ui: &Ui, name: &str) -> Option<ComponentId> {
    ui.find_in(main_form()?, name)
}

fn set(ui: &mut Ui, name: &str, prop: &str, value: impl Into<PropValue>) {
    if let Some(id) = c(ui, name) {
        let _ = ui.set_prop(id, prop, value);
    }
}

fn tw(key: &str, args: &[(&str, &str)]) -> String {
    lang::t_with(key, args)
}

/// O HTML simples dos textos (`<b>`, `<code>`) vira texto puro.
fn plain(html: &str) -> String {
    let mut s = html.to_owned();
    for tag in ["<b>", "</b>", "<code>", "</code>", "<br>", "<br/>"] {
        s = s.replace(tag, "");
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fmt_dur(sec: i64) -> String {
    let sec = sec.max(0);
    let (m, s) = (sec / 60, sec % 60);
    if m >= 60 { format!("{}:{:02}:{:02}", m / 60, m % 60, s) } else { format!("{m}:{s:02}") }
}

/// Número com a vírgula decimal do idioma da interface.
fn fmt_num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if decimals > 0 { s.trim_end_matches('0').trim_end_matches('.').to_owned() } else { s };
    if lang::current_language().starts_with("en") { s } else { s.replace('.', ",") }
}

// ---------------------------------------------------------------------------
// Construtor de linhas
// ---------------------------------------------------------------------------

/// A largura útil da página (o rótulo e a dica ficam com o que sobra do controle).
fn page_width(ui: &Ui) -> f32 {
    c(ui, "lySettings").and_then(|p| ui.bounds(p)).map_or(900.0, |b| (b.width - 72.0).max(360.0))
}

/// Altura do texto do rótulo `l` quebrado na largura `w` (com a fonte do estilo dele).
fn wrapped_height(ui: &Ui, l: ComponentId, text: &str, w: f32) -> f32 {
    let style = TextStyle { font: ui.font_of(l), wrap: true, ..TextStyle::default() };
    ui.measure_text_wrapped(text, &style, w.max(40.0)).height.ceil() + 2.0
}

fn clear_page(ui: &mut Ui) -> Option<ComponentId> {
    let p = c(ui, "lySettings")?;
    for child in ui.children(p).to_vec() {
        ui.free(child);
    }
    with(|s| s.slots.clear());
    Some(p)
}

fn label(ui: &mut Ui, parent: ComponentId, text: &str, style: &str, align: Align, height: f32) -> Option<ComponentId> {
    let l = ui.create("Label", Some(parent)).ok()?;
    let _ = ui.set_prop(l, "style", style);
    let _ = ui.set_prop(l, "text", text);
    if let Some(cb) = ui.control_mut(l) {
        cb.set_align(align).set_height(height);
    }
    Some(l)
}

/// Uma linha: o rótulo e a dica à esquerda, o controle (montado por `ctl`) à
/// direita. Devolve o contêiner do controle.
fn field(ui: &mut Ui, page: ComponentId, title: &str, hint: &str, ctl_h: f32, ctl_w: f32) -> Option<ComponentId> {
    let w = page_width(ui);
    let text_w = if ctl_w > 0.0 { w - ctl_w - 24.0 } else { w };
    let row = ui.create("Layout", Some(page)).ok()?;
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_margins([0.0, 14.0, 0.0, 14.0]);
    }
    let side = ui.create("Layout", Some(row)).ok()?;
    if let Some(cb) = ui.control_mut(side) {
        cb.set_align(Align::Right).set_width(ctl_w.max(0.0)).set_margins([24.0, 0.0, 0.0, 0.0]);
    }
    // O controle fica no alto da linha, na altura dele (a dica pode ser mais alta).
    let ctl = ui.create("Layout", Some(side)).ok()?;
    if let Some(cb) = ui.control_mut(ctl) {
        cb.set_align(Align::Top).set_height(ctl_h);
    }
    let text = ui.create("Layout", Some(row)).ok()?;
    if let Some(cb) = ui.control_mut(text) {
        cb.set_align(Align::Client);
    }
    label(ui, text, title, "row", Align::Top, 20.0);
    let mut left_h = 20.0;
    if !hint.is_empty() {
        if let Some(l) = label(ui, text, hint, "sub", Align::Top, 0.0) {
            let hint_h = wrapped_height(ui, l, hint, text_w);
            left_h += 4.0 + hint_h;
            let _ = ui.set_prop(l, "wrap", true);
            let _ = ui.set_prop(l, "v_align", "leading");
            if let Some(cb) = ui.control_mut(l) {
                cb.set_height(hint_h).set_margins([0.0, 4.0, 0.0, 0.0]);
            }
        }
    }
    if let Some(cb) = ui.control_mut(row) {
        cb.set_height(left_h.max(ctl_h));
    }
    let line = ui.create("Rectangle", Some(page)).ok()?;
    let _ = ui.set_prop(line, "style", "sep");
    if let Some(cb) = ui.control_mut(line) {
        cb.set_align(Align::Top).set_height(1.0);
    }
    Some(ctl)
}

/// Um parágrafo solto (aviso, explicação), quebrado.
fn note(ui: &mut Ui, page: ComponentId, text: &str, style: &str) -> Option<ComponentId> {
    if text.is_empty() {
        return None;
    }
    let w = page_width(ui);
    let l = label(ui, page, text, style, Align::Top, 0.0)?;
    let h = wrapped_height(ui, l, text, w);
    let _ = ui.set_prop(l, "wrap", true);
    let _ = ui.set_prop(l, "v_align", "leading");
    if let Some(cb) = ui.control_mut(l) {
        cb.set_height(h).set_margins([0.0, 8.0, 0.0, 0.0]);
    }
    Some(l)
}

fn section(ui: &mut Ui, page: ComponentId, text: &str) {
    if let Some(l) = label(ui, page, text, "section", Align::Top, 18.0) {
        let _ = ui.set_prop(l, "text_case", "upper");
        let _ = ui.set_prop(l, "letter_spacing", 1.0);
        if let Some(cb) = ui.control_mut(l) {
            cb.set_margins([0.0, 26.0, 0.0, 4.0]);
        }
    }
}

fn switch(ui: &mut Ui, parent: ComponentId, on: bool, f: impl Fn(&mut Ui, bool) + 'static) -> Option<ComponentId> {
    let s = ui.create("Switch", Some(parent)).ok()?;
    let _ = ui.set_prop(s, "checked", on);
    if let Some(cb) = ui.control_mut(s) {
        cb.set_align(Align::Right).set_width(46.0).set_height(24.0).set_margins([0.0, 0.0, 0.0, 0.0]);
    }
    ui.on(s, "on_change", move |ui, sender, _| {
        let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
        f(ui, on);
    });
    Some(s)
}

fn combo(ui: &mut Ui, parent: ComponentId, items: &[String], index: usize, f: impl Fn(&mut Ui, usize) + 'static) -> Option<ComponentId> {
    let b = ui.create("ComboBox", Some(parent)).ok()?;
    let _ = ui.set_prop(b, "items", items.iter().map(|s| s.replace(';', ",")).collect::<Vec<_>>().join(";"));
    let _ = ui.set_prop(b, "item_index", index as i64);
    if let Some(cb) = ui.control_mut(b) {
        cb.set_align(Align::Top).set_height(32.0);
    }
    ui.on(b, "on_change", move |ui, sender, _| {
        let i = ui.get_prop(sender, "item_index").ok().and_then(|v| v.to_i64().ok()).unwrap_or(0).max(0) as usize;
        f(ui, i);
    });
    Some(b)
}

/// Campo de texto: `on_edit` a cada tecla, `on_commit` no Enter ou ao sair.
fn edit(
    ui: &mut Ui,
    parent: ComponentId,
    text: &str,
    placeholder: &str,
    on_edit: impl Fn(&mut Ui, &str) + 'static,
    on_commit: impl Fn(&mut Ui, &str) + 'static,
) -> Option<ComponentId> {
    let e = ui.create("Edit", Some(parent)).ok()?;
    let _ = ui.set_prop(e, "text", text);
    let _ = ui.set_prop(e, "placeholder", placeholder);
    if let Some(cb) = ui.control_mut(e) {
        cb.set_align(Align::Top).set_height(32.0);
    }
    ui.on(e, "on_change", move |ui, sender, _| {
        let t = match ui.get_prop(sender, "text") {
            Ok(PropValue::String(t)) => t,
            _ => String::new(),
        };
        on_edit(ui, &t);
    });
    let commit = std::rc::Rc::new(on_commit);
    let c2 = commit.clone();
    ui.on(e, "on_exit", move |ui, sender, _| {
        if let Ok(PropValue::String(t)) = ui.get_prop(sender, "text") {
            c2(ui, &t);
        }
    });
    ui.on(e, "on_key_down", move |ui, sender, args| {
        if let EventArgs::Key(k) = args {
            if k.key == Key::Enter {
                if let Ok(PropValue::String(t)) = ui.get_prop(sender, "text") {
                    commit(ui, &t);
                }
            }
        }
    });
    Some(e)
}

/// Slider com as legendas das pontas; `f` a cada mudança.
fn slider(ui: &mut Ui, parent: ComponentId, min: f64, max: f64, value: f64, ends: (&str, &str), f: impl Fn(&mut Ui, f64) + 'static) -> Option<ComponentId> {
    let row = ui.create("Layout", Some(parent)).ok()?;
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(30.0);
    }
    label(ui, row, ends.0, "key", Align::Left, 30.0).and_then(|l| ui.control_mut(l).map(|cb| cb.set_width(70.0).set_margins([0.0, 0.0, 6.0, 0.0]).set_height(30.0)).map(|_| ()));
    if let Some(l) = label(ui, row, ends.1, "key", Align::Right, 30.0) {
        let _ = ui.set_prop(l, "h_align", "trailing");
        if let Some(cb) = ui.control_mut(l) {
            cb.set_width(80.0).set_margins([6.0, 0.0, 0.0, 0.0]);
        }
    }
    let t = ui.create("TrackBar", Some(row)).ok()?;
    let _ = ui.set_prop(t, "min", min);
    let _ = ui.set_prop(t, "max", max);
    let _ = ui.set_prop(t, "value", value);
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

fn button(ui: &mut Ui, parent: ComponentId, text: &str, style: &str, align: Align, f: impl Fn(&mut Ui) + 'static) -> Option<ComponentId> {
    let b = ui.create("Button", Some(parent)).ok()?;
    let _ = ui.set_prop(b, "text", text);
    if !style.is_empty() {
        let _ = ui.set_prop(b, "style", style);
    }
    let w = ui.measure_text(text, &TextStyle::default()).width + 36.0;
    if let Some(cb) = ui.control_mut(b) {
        cb.set_align(align).set_height(32.0).set_width(w.max(90.0));
    }
    ui.on(b, "on_click", move |ui, _, _| f(ui));
    Some(b)
}

/// Um rótulo que muda sem refazer a aba.
fn slot(ui: &mut Ui, key: &'static str, id: Option<ComponentId>) {
    if let Some(id) = id {
        with(|s| s.slots.insert(key, id));
    }
    let _ = ui;
}

fn set_slot(ui: &mut Ui, key: &'static str, prop: &str, value: impl Into<PropValue>) {
    if let Some(id) = with(|s| s.slots.get(key).copied()).filter(|id| ui.contains(*id)) {
        let _ = ui.set_prop(id, prop, value);
    }
}

/// Envio adiado (`ms`) — o slider manda a cada passo e o core regrava a config.
fn debounce(ui: &mut Ui, key: &'static str, ms: u64, f: impl FnOnce(&mut Ui) + 'static) {
    if let Some(job) = with(|s| s.debounce.remove(key)) {
        ui.clear_timeout(job);
    }
    let job = ui.set_timeout(ms, move |ui| {
        with(|s| s.debounce.remove(key));
        f(ui);
    });
    with(|s| s.debounce.insert(key, job));
}

fn fit_page(ui: &mut Ui) {
    let Some(p) = c(ui, "lySettings") else { return };
    let h: f32 = ui
        .children(p)
        .to_vec()
        .iter()
        .filter_map(|&id| ui.control(id).filter(|cb| cb.visible()).map(|cb| cb.height() + cb.margins().top + cb.margins().bottom))
        .sum();
    if let Some(cb) = ui.control_mut(p) {
        cb.set_height(h + 60.0);
    }
    ui.layout();
}

// ---------------------------------------------------------------------------
// Abrir / fechar / abas
// ---------------------------------------------------------------------------

/// Abre as Configurações (na aba em que o usuário parou, ou em `tab`).
pub fn open(ui: &mut Ui, tab: Option<&str>) {
    if let Some(t) = tab {
        with(|s| s.tab = t.to_owned());
    }
    with(|s| s.open = true);
    load_values();
    set(ui, "pnlSettings", "visible", true);
    set(ui, "lblSetTitle", "text", lang::t("settings.title"));
    let tab = with(|s| s.tab.clone());
    show_tab(ui, &tab);
}

pub fn is_open() -> bool {
    with(|s| s.open)
}

pub fn close(ui: &mut Ui) {
    with(|s| s.open = false);
    set(ui, "pnlSettings", "visible", false);
}

/// Os valores do core que as abas mostram (lidos ao abrir).
fn load_values() {
    let q = encoder::recording_quality_level().clamp(0, 10);
    let fps = config::get_config_int("recordingFps", 30).max(10);
    let kf = config::get_config_int("recordingKeyframeSec", 2).clamp(1, 10);
    let presets = fps_presets(crate::win_preview::max_monitor_refresh_rate());
    with(|s| {
        s.quality = q;
        s.fps = fps;
        s.keyframe = kf;
        s.fps_presets = presets;
    });
}

fn render_sidebar(ui: &mut Ui) {
    let Some(side) = c(ui, "lySetTabs") else { return };
    for child in ui.children(side).to_vec() {
        ui.free(child);
    }
    let current = with(|s| s.tab.clone());
    for tab in TABS {
        let Ok(row) = ui.create("Panel", Some(side)) else { continue };
        let _ = ui.set_prop(row, "style", if tab == current { "row_active" } else { "flat" });
        if let Some(cb) = ui.control_mut(row) {
            cb.set_align(Align::Top).set_height(34.0).set_padding([12.0, 0.0, 12.0, 0.0]).set_margins([0.0, 0.0, 0.0, 2.0]).set_cursor(CursorShape::Pointer);
        }
        if let Some(l) = label(ui, row, &lang::t(&format!("settings.tabs.{tab}")), if tab == current { "row" } else { "legend" }, Align::Client, 34.0) {
            let _ = ui.set_prop(l, "hit_test", false);
        }
        ui.on(row, "on_click", move |ui, _, _| show_tab(ui, tab));
    }
}

/// Pedidos ao core quando uma aba aparece.
fn enter_tab(tab: &str) {
    match tab {
        "files" => btr::push_root_folders(),
        "replay" => replay::push_replay_state(),
        "hotkeys" => bhk::push_state(),
        "transcribe" => {
            btr::handle_get_transcribe_state();
            btr::push_local_asr_state(false);
        }
        _ => {}
    }
}

pub fn show_tab(ui: &mut Ui, tab: &str) {
    with(|s| s.tab = tab.to_owned());
    set(ui, "lblSetTab", "text", lang::t(&format!("settings.tabs.{tab}")));
    render_sidebar(ui);
    render_tab(ui);
    enter_tab(tab);
    if let Some(sb) = c(ui, "scrSettings").and_then(|id| ui.control_mut(id)) {
        sb.set_scroll_offset(PointF::ZERO);
    }
}

fn render_tab(ui: &mut Ui) {
    let tab = with(|s| s.tab.clone());
    let Some(page) = clear_page(ui) else { return };
    match tab.as_str() {
        "general" => tab_general(ui, page),
        "onrecord" => tab_onrecord(ui, page),
        "files" => tab_files(ui, page),
        "recording" => tab_video(ui, page),
        "audio" => tab_audio(ui, page),
        "replay" => tab_replay(ui, page),
        "hotkeys" => tab_hotkeys(ui, page),
        "devices" => tab_devices(ui, page),
        "transcribe" => tab_transcribe(ui, page),
        "updates" => tab_updates(ui, page),
        _ => tab_about(ui, page),
    }
    fit_page(ui);
}

/// Refaz a aba se for a aberta (dados novos do core).
fn refresh(tab: &'static str) {
    if is_open() && with(|s| s.tab == tab) {
        with_ui(render_tab);
    }
}

pub fn resized(ui: &mut Ui) {
    // As dicas quebram pela largura: a altura das linhas muda.
    let w = page_width(ui);
    thread_local! {
        static LAST: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
    }
    if is_open() && LAST.with(|l| (l.replace(w) - w).abs() > 20.0) {
        ui.set_timeout(0, render_tab);
    }
}

// ---------------------------------------------------------------------------
// Geral
// ---------------------------------------------------------------------------

fn tab_general(ui: &mut Ui, page: ComponentId) {
    // Tema: claro, escuro ou o do sistema.
    if let Some(ctl) = field(ui, page, &lang::t("settings.theme.label"), "", 32.0, CTL_W) {
        let pref = config::get_config_str("theme", "system");
        let opts = ["system", "light", "dark"];
        let labels: Vec<String> = opts.iter().map(|o| lang::t(&format!("settings.theme.{o}"))).collect();
        let i = opts.iter().position(|o| *o == pref).unwrap_or(0);
        combo(ui, ctl, &labels, i, move |_, i| bridge::handle_set_theme(opts[i]));
    }
    // Idioma: "Automático" + os embutidos (nome nativo).
    if let Some(ctl) = field(ui, page, &lang::t("settings.language.label"), &lang::t("settings.language.hint"), 32.0, CTL_W) {
        let pref = config::get_config_str("language", "");
        let mut labels = vec![lang::t("settings.language.auto")];
        let mut codes = vec![String::new()];
        let mut idx = 0;
        if let Some(list) = lang::available_languages().as_array() {
            for l in list {
                let code = l.get("code").and_then(|v| v.as_str()).unwrap_or_default();
                if code.is_empty() {
                    continue;
                }
                let name = l.get("nativeName").or_else(|| l.get("name")).and_then(|v| v.as_str()).unwrap_or(code);
                if code.eq_ignore_ascii_case(&pref) {
                    idx = labels.len();
                }
                labels.push(name.to_owned());
                codes.push(code.to_owned());
            }
        }
        combo(ui, ctl, &labels, idx, move |_, i| bridge::handle_set_language(codes.get(i).map_or("", |s| s.as_str())));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.windowTitle.label"), &lang::t("settings.windowTitle.hint"), 32.0, CTL_W) {
        edit(ui, ctl, &config::get_config_str("windowTitle", "NoOBS"), "NoOBS", |_, _| {}, |_, t| {
            let t = t.trim().to_owned();
            if t != config::get_config_str("windowTitle", "NoOBS") {
                shell::handle_set_window_title(&t);
            }
        });
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.autostart.label"), &lang::t("settings.autostart.hint"), 24.0, 60.0) {
        switch(ui, ctl, crate::autostart::is_autostart_enabled(), |_, on| shell::handle_set_autostart(on));
    }
    let admin_state = lang::t(if crate::elevate::is_process_elevated() { "settings.runAsAdmin.stateOn" } else { "settings.runAsAdmin.stateOff" });
    let admin_hint = format!("{} {}", lang::t("settings.runAsAdmin.hint"), admin_state);
    if let Some(ctl) = field(ui, page, &lang::t("settings.runAsAdmin.label"), &admin_hint, 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("runAsAdmin", false), |_, on| shell::handle_set_run_as_admin(on));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.closeToTray.label"), &lang::t("settings.closeToTray.hint"), 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("closeToTray", true), |_, on| shell::handle_set_close_to_tray(on));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.reset.label"), &lang::t("settings.reset.hint"), 32.0, 200.0) {
        button(ui, ctl, &lang::t("settings.buttons.reset"), "danger", Align::Right, restore_defaults);
    }
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

fn corner_labels() -> Vec<String> {
    ["topLeft", "topRight", "bottomLeft", "bottomRight"].iter().map(|c| lang::t(&format!("settings.recIndicator.corners.{c}"))).collect()
}

/// As opções de um indicador na tela (clicável, canto, opacidade).
fn indicator_options(ui: &mut Ui, page: ComponentId, clickable: (bool, &'static str, &str), corner: &str, opacity: i32, on_corner: fn(&str), on_opacity: fn(i32)) {
    if let Some(ctl) = field(ui, page, &lang::t(clickable.2), &lang::t("settings.recIndicator.clickableHint"), 24.0, 60.0) {
        let key = clickable.1;
        switch(ui, ctl, clickable.0, move |ui, on| check_changed(ui, key, on));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.recIndicator.cornerLabel"), "", 32.0, CTL_W) {
        let i = CORNERS.iter().position(|c| *c == corner).unwrap_or(1);
        combo(ui, ctl, &corner_labels(), i, move |_, i| on_corner(CORNERS.get(i).copied().unwrap_or("top-right")));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.recIndicator.opacityLabel"), "", 30.0, CTL_W) {
        slider(ui, ctl, 20.0, 100.0, opacity as f64, ("20%", "100%"), move |ui, v| debounce(ui, "opacity", 300, move |_| on_opacity(v as i32)));
    }
}

fn tab_onrecord(ui: &mut Ui, page: ComponentId) {
    let rec_on = config::get_config_bool("recIndicator", false);
    if let Some(ctl) = field(ui, page, &lang::t("settings.recIndicator.label"), &plain(&lang::t("settings.recIndicator.hint")), 24.0, 60.0) {
        switch(ui, ctl, rec_on, |ui, on| check_changed(ui, "recIndicator", on));
    }
    if rec_on {
        let corner = config::get_config_str("recIndicatorCorner", "top-right");
        let op = config::get_config_int("recIndicatorOpacity", 90).clamp(20, 100);
        indicator_options(
            ui,
            page,
            (config::get_config_bool("recIndicatorClickable", true), "recIndicatorClickable", "settings.recIndicator.clickable"),
            &corner,
            op,
            recording::handle_set_rec_indicator_corner,
            recording::handle_set_rec_indicator_opacity,
        );
    }
    let replay_on = with(|s| s.replay.as_ref().map(|r| r.indicator)).unwrap_or(true);
    let which = match (rec_on, replay_on) {
        (true, true) => "labelBoth",
        (true, false) => "labelRecording",
        (false, true) => "labelBuffer",
        _ => "labelPreviews",
    };
    let fs_hint = lang::t(if rec_on || replay_on { "settings.fullscreenDetect.hint" } else { "settings.fullscreenDetect.hintPreviews" });
    if let Some(ctl) = field(ui, page, &lang::t(&format!("settings.fullscreenDetect.{which}")), &fs_hint, 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("fullscreenDetect", true), |ui, on| check_changed(ui, "fullscreenDetect", on));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.playSound.label"), &lang::t("settings.playSound.hint"), 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("playSoundOnRecord", true), |ui, on| check_changed(ui, "playSoundOnRecord", on));
    }
    let tray = config::get_config_bool("closeToTray", true);
    let (ml, mh) = if tray { ("settings.minimizeOnRecord.labelTray", "settings.minimizeOnRecord.hintTray") } else { ("settings.minimizeOnRecord.labelTaskbar", "settings.minimizeOnRecord.hintTaskbar") };
    if let Some(ctl) = field(ui, page, &lang::t(ml), &lang::t(mh), 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("minimizeOnRecord", true), |ui, on| check_changed(ui, "minimizeOnRecord", on));
    }
    let auto = config::get_config_bool("autoRecordOnMic", false);
    if let Some(ctl) = field(ui, page, &lang::t("settings.autoRecord.label"), &lang::t("settings.autoRecord.hint"), 24.0, 60.0) {
        switch(ui, ctl, auto, |ui, on| check_changed(ui, "autoRecordOnMic", on));
    }
    if auto {
        if let Some(ctl) = field(ui, page, &lang::t("settings.autoRecord.appsHint"), "", 32.0, CTL_W) {
            edit(ui, ctl, &config::get_config_str("autoRecordMicApps", ""), &lang::t("settings.autoRecord.appsPlaceholder"), |_, _| {}, |_, t| {
                let t = t.trim().to_owned();
                if t != config::get_config_str("autoRecordMicApps", "") {
                    settings::handle_set_auto_record_mic_apps(&t);
                }
            });
        }
        if let Some(ctl) = field(ui, page, &lang::t("settings.autoRecord.exceptHint"), "", 32.0, CTL_W) {
            edit(ui, ctl, &config::get_config_str("autoRecordMicExcept", ""), &lang::t("settings.autoRecord.exceptPlaceholder"), |_, _| {}, |_, t| {
                let t = t.trim().to_owned();
                if t != config::get_config_str("autoRecordMicExcept", "") {
                    settings::handle_set_auto_record_mic_except(&t);
                }
            });
        }
        section(ui, page, &lang::t("settings.autoRecord.devicesLabel"));
        note(ui, page, &lang::t("settings.autoRecord.devicesHint"), "sub");
        device_groups(ui, page, |d| (!d.hidden).then_some(d.auto), |ui, id, on| {
            set_device_flag(id, |d| d.auto = on);
            crate::bridge::devices::handle_set_auto_source(id, on);
            let _ = ui;
        }, &lang::t("settings.autoRecord.devicesEmpty"));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.stopOnLock.label"), &lang::t("settings.stopOnLock.hint"), 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("stopOnLock", true), |ui, on| check_changed(ui, "stopOnLock", on));
    }
}

// ---------------------------------------------------------------------------
// Arquivos
// ---------------------------------------------------------------------------

/// Prévia do nome do arquivo: data/hora de agora e o codec/fps/qualidade.
fn filename_preview(pattern: &str) -> String {
    let codec = config::get_config_str("codec", "auto");
    let fam = if codec.starts_with("av1") {
        "AV1"
    } else if codec.starts_with("hevc") {
        "HEVC"
    } else {
        "H.264"
    };
    let (q, fps, presets) = with(|s| (s.quality, s.fps, s.fps_presets.clone()));
    let fps = presets.get(fps_index(&presets, fps)).copied().unwrap_or(fps);
    let now = chrono::Local::now();
    let rep = |s: &str, code: &str, val: &str| -> String {
        let low = s.to_lowercase();
        let code_l = code.to_lowercase();
        let mut out = String::new();
        let mut from = 0;
        while let Some(p) = low[from..].find(&code_l).map(|p| p + from) {
            out.push_str(&s[from..p]);
            out.push_str(val);
            from = p + code.len();
        }
        out.push_str(&s[from..]);
        out
    };
    use chrono::{Datelike, Timelike};
    let mut s = pattern.to_owned();
    s = rep(&s, "{CODEC}", fam);
    s = rep(&s, "{FPS}", &fps.to_string());
    s = rep(&s, "{QUALIDADE}", &q.to_string());
    s = rep(&s, "{AAAA}", &now.year().to_string());
    s = rep(&s, "{MM}", &format!("{:02}", now.month()));
    s = rep(&s, "{DD}", &format!("{:02}", now.day()));
    s = rep(&s, "{HH}", &format!("{:02}", now.hour()));
    s = rep(&s, "{NN}", &format!("{:02}", now.minute()));
    s = rep(&s, "{SS}", &format!("{:02}", now.second()));
    s = rep(&s, "{ZZZ}", &format!("{:03}", now.timestamp_subsec_millis()));
    let mut s: String = s.chars().filter(|c| *c != '{' && *c != '}').map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    s = s.trim().to_owned();
    if s.is_empty() {
        s = now.format("NoOBS_%Y-%m-%d_%H-%M-%S").to_string();
    }
    tw("settings.filename.previewLabel", &[("name", &format!("{s}.mkv"))])
}

fn library_hint(mode: &str) -> String {
    let mut txt = lang::t(&format!("settings.library.hint.{mode}"));
    if mode != "off" && settings::is_path_under_onedrive(&recording::record_dir().to_string_lossy()) {
        txt.push(' ');
        txt.push_str(&lang::t("settings.library.cloudNote"));
    }
    txt
}

fn tab_files(ui: &mut Ui, page: ComponentId) {
    if let Some(ctl) = field(ui, page, &lang::t("settings.recordDir.label"), "", 32.0, 460.0) {
        button(ui, ctl, &lang::t("settings.recordDir.browse"), "outline", Align::Right, |_| settings::handle_pick_record_dir());
        if let Some(e) = edit(ui, ctl, &recording::record_dir().to_string_lossy(), "", |_, _| {}, |_, p| {
            let p = p.trim().to_owned();
            if p != recording::record_dir().to_string_lossy() {
                settings::handle_set_record_dir(&p);
            }
        }) {
            if let Some(cb) = ui.control_mut(e) {
                cb.set_align(Align::Client).set_margins([0.0, 0.0, 8.0, 0.0]);
            }
            slot(ui, "record_dir", Some(e));
        }
    }
    let pattern = config::get_config_str("filenamePattern", settings::DEFAULT_FILENAME_PATTERN);
    if let Some(ctl) = field(ui, page, &lang::t("settings.filename.label"), &plain(&lang::t("settings.filename.hint")), 32.0, 380.0) {
        edit(
            ui,
            ctl,
            &pattern,
            "",
            |ui, t| set_slot(ui, "filename_preview", "text", filename_preview(t)),
            |_, t| {
                let t = t.trim().to_owned();
                if t != config::get_config_str("filenamePattern", settings::DEFAULT_FILENAME_PATTERN) {
                    settings::handle_set_filename_pattern(&t);
                }
            },
        );
    }
    let preview = note(ui, page, &filename_preview(&pattern), "key");
    slot(ui, "filename_preview", preview);
    let lib = config::get_config_str("libraryThumbs", "auto");
    let li = LIBRARY_MODES.iter().position(|m| *m == lib).unwrap_or(0);
    if let Some(ctl) = field(ui, page, &lang::t("settings.library.label"), &library_hint(LIBRARY_MODES[li]), 32.0, CTL_W) {
        let labels: Vec<String> = LIBRARY_MODES.iter().map(|m| lang::t(&format!("settings.library.{m}"))).collect();
        combo(ui, ctl, &labels, li, |ui, i| {
            settings::handle_set_library_thumbs(LIBRARY_MODES.get(i).copied().unwrap_or("auto"));
            ui.set_timeout(0, render_tab);
        });
    }
    section(ui, page, &lang::t("settings.hiddenFolders.label"));
    note(ui, page, &lang::t("settings.hiddenFolders.hint"), "sub");
    let folders = with(|s| s.folders.clone());
    if folders.is_empty() {
        note(ui, page, &lang::t("settings.hiddenFolders.empty"), "key");
    }
    for f in folders {
        let info = if f.missing {
            lang::t("settings.hiddenFolders.missing")
        } else {
            tw(if f.count == 1 { "settings.hiddenFolders.countOne" } else { "settings.hiddenFolders.countMany" }, &[("count", &f.count.to_string())])
        };
        if let Some(ctl) = field(ui, page, &f.name, &info, 24.0, 60.0) {
            let name = f.name.clone();
            switch(ui, ctl, f.hidden, move |_, on| btr::handle_set_folder_hidden(&name, on));
        }
    }
}

// ---------------------------------------------------------------------------
// Vídeo
// ---------------------------------------------------------------------------

/// 20, 30 e múltiplos de 30 até o monitor mais rápido; se ele não for
/// múltiplo de 30, o último múltiplo dá lugar a ele.
fn fps_presets(max_hz: i32) -> Vec<i32> {
    let max = max_hz.max(20);
    let mut list = vec![20, 30];
    let mut v = 60;
    while v <= max {
        list.push(v);
        v += 30;
    }
    let mut p: Vec<i32> = list.into_iter().filter(|v| *v <= max).collect();
    p.dedup();
    if p.is_empty() {
        return vec![max];
    }
    if *p.last().unwrap_or(&0) != max {
        if p.len() >= 3 {
            p.pop();
        }
        p.push(max);
    }
    p
}

fn fps_index(presets: &[i32], fps: i32) -> usize {
    presets.iter().enumerate().min_by_key(|(_, p)| (*p - fps).abs()).map(|(i, _)| i).unwrap_or(0)
}

fn codec_options() -> Vec<(String, bool)> {
    let caps = bridge::last_encoder_caps();
    let tag = match caps.map(|c| c.vendor) {
        Some(GpuVendor::Nvidia) => lang::t("settings.codec.vendorNvidia"),
        Some(GpuVendor::Amd) => lang::t("settings.codec.vendorAmd"),
        Some(GpuVendor::Intel) => lang::t("settings.codec.vendorIntel"),
        _ => String::new(),
    };
    let unavailable = format!(" {}", lang::t("settings.codec.unavailable"));
    CODECS
        .iter()
        .map(|c| {
            let avail = caps.map_or(true, |k| match *c {
                "av1-hw" => k.av1_hw,
                "hevc-hw" => k.hevc_hw,
                "h264-hw" => k.h264_hw,
                "h264-sw" => k.h264_sw,
                "av1-sw" => k.av1_sw,
                _ => true,
            });
            let text = match *c {
                "auto" => lang::t("settings.codec.auto"),
                "av1-hw" | "hevc-hw" | "h264-hw" if caps.is_some() => {
                    let base = match *c {
                        "av1-hw" => "AV1",
                        "hevc-hw" => "HEVC / H.265",
                        _ => "H.264",
                    };
                    format!("{base}{}{}", lang::t("settings.codec.hwSuffix"), if avail { tag.clone() } else { unavailable.clone() })
                }
                "av1-hw" => lang::t("settings.codec.av1Hw"),
                "hevc-hw" => lang::t("settings.codec.hevcHw"),
                "h264-hw" => lang::t("settings.codec.h264Hw"),
                "h264-sw" => format!("{}{}", lang::t("settings.codec.h264Sw"), if avail { "" } else { &unavailable }),
                _ => format!("{}{}", lang::t("settings.codec.av1Sw"), if avail { "" } else { &unavailable }),
            };
            (text, avail)
        })
        .collect()
}

fn codec_hint() -> String {
    let codec = config::get_config_str("codec", "auto");
    let dim = match codec.as_str() {
        "h264-hw" => 4096,
        "h264-sw" | "hevc-hw" | "av1-hw" | "av1-sw" => 8192,
        _ if bridge::last_encoder_caps().is_some_and(|c| c.h264_hw) => 4096,
        _ => 8192,
    };
    let mut s = format!("{} {}", lang::t("settings.codec.hint"), tw("settings.codec.maxRes", &[("w", &dim.to_string()), ("h", &dim.to_string())]));
    if codec == "av1-sw" {
        s.push(' ');
        s.push_str(&lang::t("settings.codec.swWarn"));
    }
    s
}

fn quality_hint(q: i32) -> String {
    let mut s = lang::t(&format!("settings.quality.hint.{q}"));
    if config::get_config_str("codec", "auto") == "av1-sw" && q <= VT_SVT_FLOOR_LEVEL {
        s.push(' ');
        s.push_str(&lang::t("settings.quality.svtFloor"));
    }
    s
}

fn fps_hint(v: i32) -> String {
    let bucket = match v {
        v if v <= 24 => "low",
        v if v <= 30 => "smooth",
        v if v <= 60 => "good",
        v if v <= 120 => "high",
        _ => "veryHigh",
    };
    tw(&format!("settings.fps.hint.{bucket}"), &[("fps", &v.to_string())])
}

fn keyframe_hint(kf: i32) -> String {
    let kb = match kf {
        k if k <= 1 => "precise",
        k if k <= 2 => "balanced",
        k if k <= 5 => "medium",
        _ => "spaced",
    };
    tw(&format!("settings.keyframe.hint.{kb}"), &[("sec", &kf.to_string())])
}

fn tab_video(ui: &mut Ui, page: ComponentId) {
    let codec = config::get_config_str("codec", "auto");
    let ci = CODECS.iter().position(|c| c.eq_ignore_ascii_case(&codec)).unwrap_or(0);
    if let Some(ctl) = field(ui, page, &lang::t("settings.codec.label"), &codec_hint(), 32.0, 360.0) {
        let opts = codec_options();
        let labels: Vec<String> = opts.iter().map(|(t, _)| t.clone()).collect();
        combo(ui, ctl, &labels, ci, move |ui, i| {
            if !opts.get(i).is_some_and(|(_, ok)| *ok) {
                ui.set_timeout(0, render_tab);
                return;
            }
            settings::handle_set_codec(CODECS.get(i).copied().unwrap_or("auto"));
            ui.set_timeout(0, render_tab);
        });
    }
    let (q, fps, kf, presets) = with(|s| (s.quality, s.fps, s.keyframe, s.fps_presets.clone()));
    if let Some(ctl) = field(ui, page, &tw("settings.quality.label", &[]), &quality_hint(q), 30.0, 380.0) {
        slider(ui, ctl, 0.0, 10.0, q as f64, (&lang::t("settings.quality.lower"), &lang::t("settings.quality.higher")), |ui, v| {
            with(|s| s.quality = v as i32);
            debounce(ui, "quality", 400, move |ui| {
                settings::handle_set_recording_quality(v as i32);
                render_tab(ui);
            });
        });
    }
    let fi = fps_index(&presets, fps);
    let fps_v = presets.get(fi).copied().unwrap_or(fps);
    if let Some(ctl) = field(ui, page, &format!("{} · {fps_v} fps", lang::t("settings.fps.label")), &fps_hint(fps_v), 30.0, 380.0) {
        let n = presets.len().max(1) - 1;
        slider(ui, ctl, 0.0, n as f64, fi as f64, (&lang::t("settings.fps.lessFluid"), &lang::t("settings.fps.moreFluid")), move |ui, v| {
            let f = presets.get(v as usize).copied().unwrap_or(30);
            with(|s| s.fps = f);
            debounce(ui, "fps", 400, move |ui| {
                settings::handle_set_recording_fps(f);
                render_tab(ui);
            });
        });
    }
    if let Some(ctl) = field(ui, page, &format!("{} · {kf} s", lang::t("settings.keyframe.label")), &keyframe_hint(kf), 30.0, 380.0) {
        slider(ui, ctl, 1.0, 10.0, kf as f64, (&lang::t("settings.keyframe.precise"), &lang::t("settings.keyframe.compact")), |ui, v| {
            with(|s| s.keyframe = v as i32);
            debounce(ui, "keyframe", 400, move |ui| {
                settings::handle_set_recording_keyframe(v as i32);
                render_tab(ui);
            });
        });
    }
    tab_video_test(ui, page);
}

// ----- teste de qualidade -----

fn per_hour(bytes: i64, dur: f64) -> f64 {
    if bytes <= 0 || dur <= 0.0 { 0.0 } else { bytes as f64 / dur * 3600.0 }
}

fn fmt_size(b: f64) -> String {
    if b >= 1e9 {
        format!("{} GB", fmt_num(b / 1e9, 1))
    } else if b >= 1e6 {
        format!("{} MB", fmt_num(b / 1e6, 0))
    } else {
        format!("{} KB", ((b / 1e3).round() as i64).max(1))
    }
}

fn tab_video_test(ui: &mut Ui, page: ComponentId) {
    section(ui, page, &lang::t("settings.videoTest.label"));
    note(ui, page, &lang::t("settings.videoTest.hint"), "sub");
    let (running, capturing, countdown, pct, level, result, a) = with(|s| (s.vt.running, s.vt.capturing, s.vt.countdown, s.vt.pct, s.vt.level, s.vt.result.clone(), s.vt.a));
    let Ok(row) = ui.create("Layout", Some(page)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(32.0).set_margins([0.0, 12.0, 0.0, 0.0]);
    }
    let run_label = lang::t(if result.is_some() { "settings.videoTest.again" } else { "settings.videoTest.run" });
    if let Some(b) = button(ui, row, &run_label, "", Align::Left, |ui| {
        if with(|s| s.vt.running) {
            return;
        }
        with(|s| {
            s.vt.running = true;
            s.vt.capturing = true;
            s.vt.countdown = 5;
            s.vt.pct = 0;
            s.vt.level = -1;
        });
        render_tab(ui);
        video_test::handle_test_video_quality();
    }) {
        let _ = ui.set_prop(b, "enabled", !running);
    }
    if running {
        if let Some(b) = button(ui, row, &lang::t("common.cancel"), "flat", Align::Left, |_| video_test::handle_cancel_video_test()) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_margins([8.0, 0.0, 0.0, 0.0]);
            }
        }
        let status = if capturing {
            tw("settings.videoTest.capturing", &[("sec", &countdown.to_string())])
        } else if level >= 0 {
            tw("settings.videoTest.encoding", &[("n", &level.to_string()), ("pct", &pct.to_string())])
        } else {
            lang::t("settings.videoTest.preparing")
        };
        if let Some(l) = label(ui, row, &status, "key", Align::Client, 32.0) {
            if let Some(cb) = ui.control_mut(l) {
                cb.set_margins([12.0, 0.0, 0.0, 0.0]);
            }
            slot(ui, "vt_status", Some(l));
        }
        return;
    }
    let Some(r) = result else { return };
    note(
        ui,
        page,
        &tw(
            "settings.videoTest.meta",
            &[("w", &r.width.to_string()), ("h", &r.height.to_string()), ("fps", &(r.fps.round() as i64).to_string()), ("sec", &fmt_num(r.duration, 1)), ("encoder", &r.encoder)],
        ),
        "key",
    );
    let mut levels = r.levels.clone();
    levels.sort_by_key(|l| -l.level);
    let max = levels.iter().map(|l| per_hour(l.bytes, r.duration)).fold(1.0f64, f64::max);
    let cur_h = levels.iter().find(|l| l.level == r.current_level).map(|l| per_hour(l.bytes, r.duration)).unwrap_or(0.0);
    for l in &levels {
        let h = per_hour(l.bytes, r.duration);
        let rel = if l.level == r.current_level {
            lang::t("settings.videoTest.current")
        } else if h > 0.0 && cur_h > 0.0 {
            let f = h / cur_h;
            format!("×{}", fmt_num(f, if f < 1.0 { 2 } else { 1 }))
        } else {
            String::new()
        };
        let Ok(line) = ui.create("Panel", Some(page)) else { continue };
        let _ = ui.set_prop(line, "style", if l.level == a { "row_active" } else { "flat" });
        if let Some(cb) = ui.control_mut(line) {
            cb.set_align(Align::Top).set_height(30.0).set_padding([10.0, 0.0, 10.0, 0.0]).set_cursor(CursorShape::Pointer);
        }
        let lv = l.level;
        ui.on(line, "on_click", move |ui, _, _| {
            with(|s| s.vt.a = lv);
            render_tab(ui);
        });
        if let Some(n) = label(ui, line, &tw("settings.videoTest.levelN", &[("n", &l.level.to_string())]), "row", Align::Left, 30.0) {
            let _ = ui.set_prop(n, "hit_test", false);
            if let Some(cb) = ui.control_mut(n) {
                cb.set_width(90.0);
            }
        }
        if let Some(t) = label(ui, line, &rel, "key", Align::Right, 30.0) {
            let _ = ui.set_prop(t, "hit_test", false);
            if let Some(cb) = ui.control_mut(t) {
                cb.set_width(90.0);
            }
        }
        let size = if h > 0.0 { tw("settings.videoTest.perHour", &[("size", &fmt_size(h))]) } else { "—".into() };
        if let Some(t) = label(ui, line, &size, "val", Align::Right, 30.0) {
            let _ = ui.set_prop(t, "hit_test", false);
            if let Some(cb) = ui.control_mut(t) {
                cb.set_width(120.0);
            }
        }
        if let Ok(m) = ui.create("LevelMeter", Some(line)) {
            let _ = ui.set_prop(m, "value", if h > 0.0 { (h / max).max(0.015) } else { 0.0 });
            let _ = ui.set_prop(m, "show_peak", false);
            let _ = ui.set_prop(m, "mid", 2.0);
            let _ = ui.set_prop(m, "high", 2.0);
            if let Some(cb) = ui.control_mut(m) {
                cb.set_align(Align::Client).set_margins([8.0, 12.0, 8.0, 12.0]).set_hit_test(false);
            }
        }
    }
    // O quadro do nível escolhido, pra comparar a nitidez.
    if let Some(img) = r.levels.iter().find(|l| l.level == a).and_then(|l| std::fs::read(&l.image).ok()) {
        if let Ok(v) = ui.create("Image", Some(page)) {
            let _ = ui.set_prop(v, "fit", "contain");
            if let Some(iv) = ui.get_mut::<ImageView>(v) {
                iv.set_picture(Some(Image::encoded(img)));
            }
            if let Some(cb) = ui.control_mut(v) {
                cb.set_align(Align::Top).set_height(260.0).set_margins([0.0, 12.0, 0.0, 0.0]);
            }
        }
    }
    let Ok(row) = ui.create("Layout", Some(page)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(32.0).set_margins([0.0, 12.0, 0.0, 0.0]);
    }
    if let Some(b) = button(ui, row, &tw("settings.videoTest.use", &[("n", &a.to_string())]), "", Align::Left, vt_use) {
        let _ = ui.set_prop(b, "enabled", a >= 0 && a != r.current_level);
    }
}

fn vt_use(ui: &mut Ui) {
    let a = with(|s| s.vt.a);
    if a < 0 {
        return;
    }
    with(|s| {
        s.quality = a;
        if let Some(r) = s.vt.result.as_mut() {
            r.current_level = a;
        }
    });
    settings::handle_set_recording_quality(a);
    render_tab(ui);
    crate::app_view::show_toast(&tw("settings.videoTest.applied", &[("n", &a.to_string())]), "", false, 0);
}

pub fn on_video_test(d: &VideoTestState) {
    let d = d.clone();
    with_ui(move |ui| {
        match d.state.as_str() {
            "recording" => {
                with(|s| {
                    s.vt.running = true;
                    s.vt.capturing = true;
                    s.vt.countdown = if d.seconds > 0 { d.seconds as i32 } else { 5 };
                });
                vt_tick(ui);
            }
            "encoding" => with(|s| {
                s.vt.running = true;
                s.vt.capturing = false;
                s.vt.pct = d.pct;
                s.vt.level = d.level;
            }),
            "done" => with(|s| {
                s.vt.running = false;
                s.vt.capturing = false;
                let cur = d.current_level;
                s.vt.a = if cur == 10 { 9 } else { cur };
                s.vt.b = 10;
                s.vt.result = Some(d.clone());
            }),
            st => {
                with(|s| {
                    s.vt.running = false;
                    s.vt.capturing = false;
                });
                if st == "error" {
                    let msg = if d.error.is_empty() { lang::t("settings.videoTest.failed") } else { d.error.clone() };
                    crate::app_view::show_toast(&lang::t("toast.errorTitle"), &msg, true, 6000);
                }
            }
        }
        if is_open() && with(|s| s.tab == "recording") {
            if d.state == "encoding" {
                let status = tw("settings.videoTest.encoding", &[("n", &d.level.to_string()), ("pct", &d.pct.to_string())]);
                set_slot(ui, "vt_status", "text", status);
            } else {
                render_tab(ui);
            }
        }
    });
}

fn vt_tick(ui: &mut Ui) {
    if let Some(job) = with(|s| s.vt.job.take()) {
        ui.clear_timeout(job);
    }
    let job = ui.set_timeout(1000, |ui| {
        let (capturing, sec) = with(|s| {
            s.vt.countdown = (s.vt.countdown - 1).max(0);
            (s.vt.capturing, s.vt.countdown)
        });
        if capturing {
            set_slot(ui, "vt_status", "text", tw("settings.videoTest.capturing", &[("sec", &sec.to_string())]));
            vt_tick(ui);
        }
    });
    with(|s| s.vt.job = Some(job));
}

// ---------------------------------------------------------------------------
// Áudio
// ---------------------------------------------------------------------------

fn audio_hint(k: i32) -> String {
    let band = if k <= 96 {
        "voice"
    } else if k <= 160 {
        "good"
    } else if k <= 192 {
        "default"
    } else {
        "high"
    };
    let mb = ((k as f64) * 3600.0 / 8.0 / 1000.0).round() as i64;
    format!("{} {}", lang::t(&format!("settings.audioBitrate.hint.{band}")), tw("settings.audioBitrate.size", &[("mb", &mb.to_string())]))
}

fn tab_audio(ui: &mut Ui, page: ComponentId) {
    let ab = encoder::audio_bitrate_kbps();
    let ai = AUDIO_BITRATES.iter().position(|k| *k == ab).unwrap_or(3);
    if let Some(ctl) = field(ui, page, &lang::t("settings.audioBitrate.label"), &audio_hint(AUDIO_BITRATES[ai]), 32.0, CTL_W) {
        let labels: Vec<String> = AUDIO_BITRATES
            .iter()
            .map(|k| {
                let key = if *k == AUDIO_BITRATE_DEFAULT { "settings.audioBitrate.optionDefault" } else { "settings.audioBitrate.option" };
                tw(key, &[("kbps", &k.to_string())])
            })
            .collect();
        combo(ui, ctl, &labels, ai, |ui, i| {
            settings::handle_set_audio_bitrate(AUDIO_BITRATES.get(i).copied().unwrap_or(AUDIO_BITRATE_DEFAULT));
            ui.set_timeout(0, render_tab);
        });
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.muteWhenDeviceMuted.label"), &lang::t("settings.muteWhenDeviceMuted.hint"), 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("muteWhenDeviceMuted", true), |ui, on| check_changed(ui, "muteWhenDeviceMuted", on));
    }
}

// ---------------------------------------------------------------------------
// Buffer
// ---------------------------------------------------------------------------

fn sec_label(v: i64) -> String {
    if v < 60 {
        tw("settings.replay.seconds", &[("n", &v.to_string())])
    } else {
        tw("settings.replay.minutes", &[("n", &((v as f64 / 60.0).round() as i64).to_string())])
    }
}

fn mb_label(v: i64) -> String {
    if v < 1024 {
        return format!("{v} MB");
    }
    let gb = v as f64 / 1024.0;
    format!("{} GB", if gb.fract() == 0.0 { fmt_num(gb, 0) } else { fmt_num(gb, 1) })
}

/// Valores da lista com o atual incluído (config editado à mão continua lá).
fn with_current(mut list: Vec<i64>, cur: i64) -> (Vec<i64>, usize) {
    if cur > 0 && !list.contains(&cur) {
        list.push(cur);
        list.sort_unstable();
    }
    let i = list.iter().position(|v| *v == cur).unwrap_or(0);
    (list, i)
}

fn tab_replay(ui: &mut Ui, page: ComponentId) {
    note(ui, page, &plain(&lang::t("settings.replay.hint")), "sub");
    let Some(r) = with(|s| s.replay.clone()) else { return };
    if let Some(ctl) = field(ui, page, &lang::t("settings.replay.autoStart"), &lang::t("settings.replay.autoStartHint"), 24.0, 60.0) {
        switch(ui, ctl, r.auto_start, |ui, on| check_changed(ui, "replayAutoStart", on));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.replay.intoRecording"), &lang::t("settings.replay.intoRecordingHint"), 24.0, 60.0) {
        switch(ui, ctl, r.into_recording, |ui, on| check_changed(ui, "replayIntoRecording", on));
    }
    let limit = if r.max_mb_limit > 0 { r.max_mb_limit } else { REPLAY_DEFAULT_MB };
    let (secs, si) = with_current(REPLAY_SEC_OPTIONS.to_vec(), if r.max_sec > 0 { r.max_sec } else { REPLAY_DEFAULT_SEC });
    let mut mbs: Vec<i64> = REPLAY_MB_OPTIONS.iter().copied().filter(|v| *v < limit).collect();
    mbs.push(limit);
    let (mbs, mi) = with_current(mbs, if r.max_mb > 0 { r.max_mb } else { REPLAY_DEFAULT_MB });
    let sec_now = std::rc::Rc::new(std::cell::Cell::new(secs[si]));
    let mb_now = std::rc::Rc::new(std::cell::Cell::new(mbs[mi]));
    if let Some(ctl) = field(ui, page, &lang::t("settings.replay.duration"), "", 32.0, CTL_W) {
        let labels: Vec<String> = secs.iter().map(|v| sec_label(*v)).collect();
        let (s2, m2) = (sec_now.clone(), mb_now.clone());
        combo(ui, ctl, &labels, si, move |_, i| {
            s2.set(secs.get(i).copied().unwrap_or(REPLAY_DEFAULT_SEC));
            replay::handle_set_replay_limits(s2.get() as i32, m2.get() as i32);
        });
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.replay.memory"), &lang::t("settings.replay.memoryHint"), 32.0, CTL_W) {
        let labels: Vec<String> = mbs.iter().map(|v| if *v >= limit { tw("settings.replay.memoryMax", &[("size", &mb_label(*v))]) } else { mb_label(*v) }).collect();
        let (s2, m2) = (sec_now.clone(), mb_now.clone());
        combo(ui, ctl, &labels, mi, move |_, i| {
            m2.set(mbs.get(i).copied().unwrap_or(REPLAY_DEFAULT_MB));
            replay::handle_set_replay_limits(s2.get() as i32, m2.get() as i32);
        });
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.replay.apps"), &lang::t("settings.replay.appsHint"), 32.0, CTL_W) {
        edit(
            ui,
            ctl,
            &r.apps,
            &lang::t("settings.replay.appsPlaceholder"),
            // Cada envio reinicia o watcher no core: com atraso enquanto digita.
            |ui, t| {
                let t = t.to_owned();
                debounce(ui, "replay_apps", 600, move |_| replay::handle_set_replay_apps(&t));
            },
            |_, t| {
                let cur = with(|s| s.replay.as_ref().map(|r| r.apps.clone())).unwrap_or_default();
                if t != cur {
                    replay::handle_set_replay_apps(t);
                }
            },
        );
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.replay.indicator"), &plain(&lang::t("settings.replay.indicatorHint")), 24.0, 60.0) {
        switch(ui, ctl, r.indicator, |ui, on| check_changed(ui, "replayIndicator", on));
    }
    if r.indicator {
        let op = if r.indicator_opacity > 0 { r.indicator_opacity as i32 } else { 90 };
        indicator_options(
            ui,
            page,
            (r.indicator_clickable, "replayIndicatorClickable", "settings.replay.indicatorClickable"),
            &r.indicator_corner,
            op,
            replay::handle_set_replay_indicator_corner,
            replay::handle_set_replay_indicator_opacity,
        );
    }
}

// ---------------------------------------------------------------------------
// Atalhos
// ---------------------------------------------------------------------------

fn split_spec(spec: &str) -> (bool, bool, bool, bool, String) {
    let mut m = (false, false, false, false);
    let mut key = String::new();
    for p in spec.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match p.to_lowercase().as_str() {
            "ctrl" => m.0 = true,
            "shift" => m.1 = true,
            "alt" => m.2 = true,
            "win" => m.3 = true,
            _ => key = p.to_owned(),
        }
    }
    (m.0, m.1, m.2, m.3, key)
}

fn hk_send(items: Vec<Item>) {
    bhk::handle_set_items(items);
}

fn hk_update(i: usize, f: impl FnOnce(&mut Item)) {
    let mut items = with(|s| s.hk_items.clone());
    if let Some(it) = items.get_mut(i) {
        f(it);
        hk_send(items);
    }
}

/// Modificador ou tecla mudou na linha `i`: monta a combinação e valida (não
/// repetida, não reservada) antes de mandar.
fn hk_combo(ui: &mut Ui, i: usize, mods: (bool, bool, bool, bool), key: String) {
    let mut parts: Vec<&str> = Vec::new();
    if mods.0 {
        parts.push("Ctrl");
    }
    if mods.1 {
        parts.push("Shift");
    }
    if mods.2 {
        parts.push("Alt");
    }
    if mods.3 {
        parts.push("Win");
    }
    if key.is_empty() {
        if !parts.is_empty() {
            // Espera a tecla: o que está marcado fica só na tela.
            with(|s| s.hk_pending.insert(i, mods));
            return;
        }
    } else {
        parts.push(&key);
    }
    with(|s| s.hk_pending.remove(&i));
    let spec = crate::hotkeys::normalize(&parts.join("+"));
    let items = with(|s| s.hk_items.clone());
    if items.get(i).is_some_and(|it| it.hotkey == spec) {
        return;
    }
    if !spec.is_empty() {
        if let Some(other) = items.iter().enumerate().find(|(j, x)| *j != i && !x.hotkey.is_empty() && x.hotkey.eq_ignore_ascii_case(&spec)) {
            crate::app_view::show_toast(
                &lang::t("toast.invalidHotkey"),
                &tw("hotkeys.duplicate", &[("mode", &lang::t(&format!("hotkeys.mode.{}", other.1.mode.as_str())))]),
                true,
                7000,
            );
            render_tab(ui);
            return;
        }
        let reason = match hotkey::parse_hotkey(&spec) {
            None => Some("settings.hotkey.invalidSpec"),
            Some(hk) => hotkey::reserved_reason(hk.modifiers, hk.vk),
        };
        if let Some(r) = reason {
            crate::app_view::show_toast(&lang::t("toast.invalidHotkey"), &format!("{} {}", lang::t(r), lang::t("settings.hotkey.chooseAnother")), true, 7000);
            render_tab(ui);
            return;
        }
    }
    hk_update(i, |it| it.hotkey = spec);
}

fn tab_hotkeys(ui: &mut Ui, page: ComponentId) {
    let keys = key_table();
    let (items, status, delay_max, defining, pending) = with(|s| (s.hk_items.clone(), s.hk_status.clone(), s.hk_delay_max, s.hk_defining, s.hk_pending.clone()));
    let mut delays: Vec<u32> = HK_DELAYS.iter().copied().filter(|v| *v <= delay_max.max(1)).collect();
    for it in &items {
        if !delays.contains(&it.delay_sec) {
            delays.push(it.delay_sec);
        }
    }
    delays.sort_unstable();
    let mode_labels: Vec<String> = HK_MODES.iter().map(|m| lang::t(&format!("hotkeys.mode.{}", m.as_str()))).collect();
    let key_values: Vec<String> = std::iter::once(String::new()).chain(keys.iter().filter(|k| !k.2).map(|k| k.0.clone())).collect();
    let key_labels: Vec<String> = std::iter::once(lang::t("settings.hotkey.notDefined")).chain(keys.iter().filter(|k| !k.2).map(|k| k.1.clone())).collect();
    let delay_labels: Vec<String> = delays.iter().map(|v| if *v == 0 { lang::t("hotkeys.delayOff") } else { tw("hotkeys.delaySec", &[("sec", &v.to_string())]) }).collect();
    for (i, it) in items.iter().enumerate() {
        let (mut ctrl, mut shift, mut alt, mut win, key) = split_spec(&it.hotkey);
        if let Some(p) = pending.get(&i) {
            (ctrl, shift, alt, win) = *p;
        }
        let ki = if key.is_empty() { 0 } else { key_values.iter().position(|k| k.eq_ignore_ascii_case(&key)).unwrap_or(0) };
        let Ok(card) = ui.create("Panel", Some(page)) else { continue };
        let _ = ui.set_prop(card, "style", "box");
        let lines = 2 + usize::from(it.mode == Mode::Window || it.mode == Mode::Area);
        if let Some(cb) = ui.control_mut(card) {
            cb.set_align(Align::Top).set_height(18.0 + lines as f32 * 42.0).set_padding([14.0, 10.0, 14.0, 10.0]).set_margins([0.0, 12.0, 0.0, 0.0]);
        }
        // Linha 1: modo, modificadores, tecla, excluir.
        let Ok(l1) = ui.create("Layout", Some(card)) else { continue };
        if let Some(cb) = ui.control_mut(l1) {
            cb.set_align(Align::Top).set_height(32.0).set_margins([0.0, 0.0, 0.0, 10.0]);
        }
        if let Some(b) = button(ui, l1, "", "tool", Align::Right, move |_| {
            let mut items = with(|s| s.hk_items.clone());
            if i < items.len() {
                items.remove(i);
                hk_send(items);
            }
        }) {
            let _ = ui.set_prop(b, "icon", "trash");
            let _ = ui.set_prop(b, "hint", lang::t("hotkeys.remove"));
            if let Some(cb) = ui.control_mut(b) {
                cb.set_width(32.0);
            }
        }
        if let Some(m) = combo(ui, l1, &mode_labels, HK_MODES.iter().position(|m| *m == it.mode).unwrap_or(0), move |_, mi| {
            let mode = HK_MODES.get(mi).copied().unwrap_or(Mode::Record);
            hk_update(i, |it| {
                let hk = it.hotkey.clone();
                let delay = it.delay_sec;
                *it = Item::new(mode, &hk);
                it.hotkey = hk;
                if mode != Mode::Replay {
                    it.delay_sec = delay;
                }
            });
        }) {
            if let Some(cb) = ui.control_mut(m) {
                cb.set_align(Align::Left).set_width(200.0);
            }
        }
        let mods = std::rc::Rc::new(std::cell::Cell::new((ctrl, shift, alt, win)));
        let key_now = std::rc::Rc::new(RefCell::new(key.clone()));
        for (n, name) in ["Ctrl", "Shift", "Alt", "Win"].iter().enumerate() {
            let Ok(cbx) = ui.create("CheckBox", Some(l1)) else { continue };
            let _ = ui.set_prop(cbx, "text", *name);
            let _ = ui.set_prop(cbx, "checked", [ctrl, shift, alt, win][n]);
            if let Some(cb) = ui.control_mut(cbx) {
                cb.set_align(Align::Left).set_width(70.0).set_margins([12.0, 0.0, 0.0, 0.0]);
            }
            let (mods, key_now) = (mods.clone(), key_now.clone());
            ui.on(cbx, "on_change", move |ui, sender, _| {
                let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
                let mut m = mods.get();
                match n {
                    0 => m.0 = on,
                    1 => m.1 = on,
                    2 => m.2 = on,
                    _ => m.3 = on,
                }
                mods.set(m);
                hk_combo(ui, i, m, key_now.borrow().clone());
            });
        }
        let kv = key_values.clone();
        if let Some(k) = combo(ui, l1, &key_labels, ki, move |ui, ki| {
            let key = kv.get(ki).cloned().unwrap_or_default();
            *key_now.borrow_mut() = key.clone();
            hk_combo(ui, i, mods.get(), key);
        }) {
            if let Some(cb) = ui.control_mut(k) {
                cb.set_align(Align::Client).set_margins([12.0, 0.0, 12.0, 0.0]);
            }
        }
        // Linha do modo: o título da janela (e o som do app) ou a área.
        if it.mode == Mode::Window || it.mode == Mode::Area {
            let Ok(l2) = ui.create("Layout", Some(card)) else { continue };
            if let Some(cb) = ui.control_mut(l2) {
                cb.set_align(Align::Top).set_height(32.0).set_margins([0.0, 0.0, 0.0, 10.0]);
            }
            if it.mode == Mode::Window {
                if let Ok(a) = ui.create("CheckBox", Some(l2)) {
                    let _ = ui.set_prop(a, "text", lang::t("windowPicker.appAudio"));
                    let _ = ui.set_prop(a, "checked", it.app_audio);
                    if let Some(cb) = ui.control_mut(a) {
                        cb.set_align(Align::Right).set_width(200.0).set_margins([12.0, 0.0, 0.0, 0.0]);
                    }
                    ui.on(a, "on_change", move |ui, sender, _| {
                        let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
                        hk_update(i, |it| it.app_audio = on);
                    });
                }
                let cur = it.title.clone();
                if let Some(e) = edit(ui, l2, &it.title, &lang::t("hotkeys.titlePlaceholder"), |_, _| {}, move |_, t| {
                    let t = t.trim().to_owned();
                    if t != cur {
                        hk_update(i, |it| it.title = t);
                    }
                }) {
                    if let Some(cb) = ui.control_mut(e) {
                        cb.set_align(Align::Client);
                    }
                }
            } else {
                let area_text = match it.area {
                    Some(r) => tw("hotkeys.areaValue", &[("w", &r.w.to_string()), ("h", &r.h.to_string()), ("x", &r.x.to_string()), ("y", &r.y.to_string())]),
                    None => lang::t("hotkeys.areaAsk"),
                };
                if it.area.is_some() {
                    button(ui, l2, &lang::t("hotkeys.areaClear"), "flat", Align::Right, move |_| hk_update(i, |it| it.area = None));
                }
                let define = lang::t(if defining == Some(i) {
                    "hotkeys.areaChoosing"
                } else if it.area.is_some() {
                    "hotkeys.areaRedefine"
                } else {
                    "hotkeys.areaDefine"
                });
                if let Some(b) = button(ui, l2, &define, "outline", Align::Right, move |_| bhk::handle_define_area(i)) {
                    if let Some(cb) = ui.control_mut(b) {
                        cb.set_margins([8.0, 0.0, 8.0, 0.0]);
                    }
                }
                label(ui, l2, &area_text, "key", Align::Client, 32.0);
            }
        }
        // Atraso e situação.
        let Ok(l3) = ui.create("Layout", Some(card)) else { continue };
        if let Some(cb) = ui.control_mut(l3) {
            cb.set_align(Align::Top).set_height(32.0);
        }
        if it.mode != Mode::Replay {
            let dl = delays.clone();
            if let Some(d) = combo(ui, l3, &delay_labels, delays.iter().position(|d| *d == it.delay_sec).unwrap_or(0), move |_, di| {
                let v = dl.get(di).copied().unwrap_or(0);
                hk_update(i, |it| it.delay_sec = v);
            }) {
                if let Some(cb) = ui.control_mut(d) {
                    cb.set_align(Align::Left).set_width(160.0);
                }
            }
        }
        let st_key = status.get(i).copied().unwrap_or("");
        if !st_key.is_empty() {
            if let Some(l) = label(ui, l3, &lang::t(&format!("hotkeys.status.{st_key}")), "key", Align::Client, 32.0) {
                if st_key != "inactive" {
                    let _ = ui.set_prop(l, "color", Color::rgb(0xf5, 0x9e, 0x0b));
                }
                if let Some(cb) = ui.control_mut(l) {
                    cb.set_margins([12.0, 0.0, 0.0, 0.0]);
                }
            }
        }
    }
    let Ok(row) = ui.create("Layout", Some(page)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(32.0).set_margins([0.0, 14.0, 0.0, 0.0]);
    }
    button(ui, row, &lang::t("hotkeys.add"), "outline", Align::Left, |_| {
        let mut items = with(|s| s.hk_items.clone());
        items.push(Item::new(Mode::Record, ""));
        hk_send(items);
    });
}

pub fn on_hotkeys_state(items: &[Item], status: &[&'static str], delay_max: u32, defining: Option<usize>) {
    with(|s| {
        s.hk_items = items.to_vec();
        s.hk_status = status.to_vec();
        s.hk_delay_max = delay_max;
        s.hk_defining = defining;
    });
    refresh("hotkeys");
}

// ---------------------------------------------------------------------------
// Dispositivos
// ---------------------------------------------------------------------------

/// Ordem "natural" sem caixa ("Monitor 2" < "Monitor 10").
fn natural_key(s: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut num = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            if !num.is_empty() {
                out.push((std::mem::take(&mut text), num.parse().unwrap_or(0)));
                num.clear();
            }
            text.push(c);
        }
    }
    out.push((text, num.parse().unwrap_or(0)));
    out
}

/// Os grupos de dispositivos (monitores, webcams, microfones, saídas) com uma
/// chave por dispositivo; `pick` diz se entra e o estado da chave.
fn device_groups(ui: &mut Ui, page: ComponentId, pick: impl Fn(&Device) -> Option<bool>, toggle: impl Fn(&mut Ui, &str, bool) + Clone + 'static, empty: &str) {
    let (mons, cams, mics, spks) = with(|s| (s.monitors.clone(), s.webcams.clone(), s.mics.clone(), s.speakers.clone()));
    let mut any = false;
    for (key, mut list) in [("monitors", mons), ("webcams", cams), ("mics", mics), ("speakers", spks)] {
        list.sort_by_key(|d| natural_key(&d.name));
        let rows: Vec<(Device, bool)> = list.into_iter().filter_map(|d| pick(&d).map(|on| (d, on))).collect();
        if rows.is_empty() {
            continue;
        }
        any = true;
        section(ui, page, &lang::t(&format!("devices.groups.{key}")));
        for (d, on) in rows {
            let name = if d.name.is_empty() { d.id.clone() } else { d.name.clone() };
            if let Some(ctl) = field(ui, page, &name, &d.info, 24.0, 60.0) {
                let id = d.id.clone();
                let t = toggle.clone();
                switch(ui, ctl, on, move |ui, on| t(ui, &id, on));
            }
        }
    }
    if !any {
        note(ui, page, empty, "key");
    }
}

fn tab_devices(ui: &mut Ui, page: ComponentId) {
    device_groups(ui, page, |d| Some(!d.hidden), |_, id, on| {
        set_device_flag(id, |d| d.hidden = !on);
        crate::bridge::devices::handle_set_source_hidden(id, !on);
    }, "");
}

fn set_device_flag(id: &str, f: impl Fn(&mut Device)) {
    with(|s| {
        for d in s.monitors.iter_mut().chain(s.webcams.iter_mut()).chain(s.mics.iter_mut()).chain(s.speakers.iter_mut()) {
            if d.id == id {
                f(d);
            }
        }
    });
}

/// Listas de dispositivos da tela principal (hot-plug incluso).
pub fn on_devices(kind: &str, list: &[Device]) {
    with(|s| match kind {
        "monitors" => s.monitors = list.to_vec(),
        "webcams" => s.webcams = list.to_vec(),
        "mics" => s.mics = list.to_vec(),
        _ => s.speakers = list.to_vec(),
    });
    refresh("devices");
}

pub fn on_root_folders(folders: &[RootFolder]) {
    with(|s| s.folders = folders.to_vec());
    refresh("files");
}

// ---------------------------------------------------------------------------
// Transcrição
// ---------------------------------------------------------------------------

fn transcribe_langs() -> (Vec<String>, Vec<String>, usize) {
    let cur = match config::get_config_str("transcribeLanguage", "").trim().to_lowercase() {
        l if l.is_empty() => "app".to_owned(),
        l => l,
    };
    let ui_lang = lang::current_language();
    let base = ui_lang.split('-').next().unwrap_or("pt").to_lowercase();
    let ui_name = TRANSCRIBE_LANGS.iter().find(|(c, _)| *c == base).map(|(_, n)| n.to_string()).unwrap_or(base.clone());
    let mut vals: Vec<String> = vec!["app".into(), "auto".into()];
    let mut labels = vec![tw("settings.transcribe.language.app", &[("name", &ui_name)]), lang::t("settings.transcribe.language.auto")];
    for (c, n) in TRANSCRIBE_LANGS {
        vals.push(c.into());
        labels.push(n.into());
    }
    if !vals.contains(&cur) {
        vals.push(cur.clone());
        labels.push(cur.clone());
    }
    let i = vals.iter().position(|v| *v == cur).unwrap_or(0);
    (vals, labels, i)
}

fn gb(bytes: i64) -> String {
    let g = bytes.max(0) as f64 / 1e9;
    format!("{} GB", fmt_num(g, if g >= 10.0 { 0 } else { 1 }))
}

/// (situação, detalhe, botão, botão perigoso, progresso).
fn asr_texts() -> (String, String, String, bool, f32) {
    let d = with(|s| s.asr.clone()).unwrap_or(LocalAsrInfo { status: "missing".into(), ..Default::default() });
    let confirm = with(|s| s.asr_confirm_remove);
    let l = |k: &str| lang::t(&format!("settings.transcribe.local.{k}"));
    let lw = |k: &str, a: &[(&str, &str)]| tw(&format!("settings.transcribe.local.{k}"), a);
    match d.status.as_str() {
        "installing" => {
            let stage = if d.stage.is_empty() { String::new() } else { l(&format!("stages.{}", d.stage)) };
            (
                lw("installing", &[("stage", &stage)]),
                format!("{} · {}", lw("progress", &[("done", &gb(d.done)), ("total", &gb(d.total))]), l("resumeHint")),
                l("cancel"),
                false,
                if d.total > 0 { (d.done as f64 / d.total as f64).min(1.0) as f32 } else { 0.0 },
            )
        }
        "ready" => (
            if d.device.is_empty() { l("readyCpu") } else { lw("ready", &[("device", &d.device)]) },
            lw("readyHow", &[("size", &gb(d.total))]),
            if confirm { l("removeConfirm") } else { l("remove") },
            confirm,
            -1.0,
        ),
        s => (
            if s == "error" { lw("error", &[("error", &d.error)]) } else { l("missing") },
            lw("missingHow", &[("size", &gb(d.total))]),
            if s == "error" { l("retry") } else { l("install") },
            false,
            -1.0,
        ),
    }
}

fn asr_action(ui: &mut Ui) {
    let status = with(|s| s.asr.as_ref().map(|a| a.status.clone())).unwrap_or_else(|| "missing".into());
    match status.as_str() {
        "installing" => crate::local_asr::cancel_install(),
        "ready" => {
            // Dois cliques: apagar 3,7 GB por engano custaria outro download.
            if !with(|s| std::mem::replace(&mut s.asr_confirm_remove, false)) {
                with(|s| s.asr_confirm_remove = true);
                render_tab(ui);
                return;
            }
            render_tab(ui);
            btr::handle_local_asr_remove();
        }
        _ => crate::local_asr::start_install(),
    }
}

/// A linha de progresso da fila de transcrição.
fn transcribe_line() -> (bool, f32, String) {
    let (t, clicked, eta) = with(|s| (s.tr.clone(), s.tr_cancel_clicked, s.tr_eta_shown));
    let canceling = t.canceling || clicked;
    if !t.running && t.queue == 0 {
        if t.total > 0 {
            let txt = if t.failed > 0 {
                tw("settings.transcribe.doneWithFailures", &[("done", &t.done.to_string()), ("total", &t.total.to_string()), ("failed", &t.failed.to_string())])
            } else {
                tw("settings.transcribe.doneAll", &[("total", &t.total.to_string())])
            };
            return (true, 1.0, txt);
        }
        return (false, 0.0, String::new());
    }
    if canceling && t.running {
        return (true, 0.0, tw("settings.transcribe.cancelingLine", &[("name", if t.current.is_empty() { "—" } else { &t.current })]));
    }
    let pos = (t.done + t.failed + 1).min(t.total.max(1));
    let total = if t.total > 0 { t.total } else { pos };
    let mut txt = tw("settings.transcribe.progress", &[("pos", &pos.to_string()), ("total", &total.to_string()), ("name", if t.current.is_empty() { "—" } else { &t.current })]);
    if !t.stage.is_empty() {
        let stg = lang::t(&format!("settings.transcribe.stage.{}", t.stage));
        if !stg.starts_with('[') {
            txt.push_str(&format!(" · {stg}"));
        }
    }
    if t.track_count > 1 {
        txt.push_str(&format!(" · {}", tw("settings.transcribe.trackN", &[("n", &(t.track + 1).to_string()), ("total", &t.track_count.to_string())])));
    }
    if t.progress >= 0.0 {
        txt.push_str(&format!(" · {}%", (t.progress * 100.0).round() as i64));
    }
    if eta > 0 {
        txt.push_str(&format!(" · {}", tw("settings.transcribe.remaining", &[("eta", &fmt_dur(eta))])));
    } else {
        txt.push_str(&format!(" · {}", tw("settings.transcribe.elapsed", &[("elapsed", &fmt_dur(t.elapsed))])));
    }
    if t.failed > 0 {
        txt.push_str(&format!(" · {}", tw("settings.transcribe.failedN", &[("failed", &t.failed.to_string())])));
    }
    (true, if t.progress >= 0.0 { t.progress as f32 } else { 0.0 }, txt)
}

fn tab_transcribe(ui: &mut Ui, page: ComponentId) {
    let (status, detail, btn, danger, progress) = asr_texts();
    let hint = format!("{}\n{status} · {detail}", lang::t("settings.transcribe.local.hint"));
    if let Some(ctl) = field(ui, page, &lang::t("settings.transcribe.local.label"), &hint, 32.0, 200.0) {
        button(ui, ctl, &btn, if danger { "danger" } else { "outline" }, Align::Right, asr_action);
    }
    if progress >= 0.0 {
        if let Ok(p) = ui.create("ProgressBar", Some(page)) {
            let _ = ui.set_prop(p, "value", (progress * 100.0) as f64);
            if let Some(cb) = ui.control_mut(p) {
                cb.set_align(Align::Top).set_height(6.0).set_margins([0.0, 0.0, 0.0, 8.0]);
            }
        }
    }
    let (vals, labels, li) = transcribe_langs();
    if let Some(ctl) = field(ui, page, &lang::t("settings.transcribe.language.label"), &lang::t("settings.transcribe.language.hint"), 32.0, CTL_W) {
        combo(ui, ctl, &labels, li, move |_, i| btr::handle_set_transcribe_language(vals.get(i).map_or("app", |s| s.as_str())));
    }
    if let Some(ctl) = field(ui, page, &lang::t("settings.transcribe.onStop.label"), &lang::t("settings.transcribe.onStop.hint"), 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("transcribeOnStop", true), |ui, on| check_changed(ui, "transcribeOnStop", on));
    }
    section(ui, page, &lang::t("settings.transcribe.queue"));
    let (t, pending, cloud, queue, clicked) = with(|s| (s.tr.clone(), s.tr_pending, s.tr_cloud, s.tr_queue.clone(), s.tr_cancel_clicked));
    let canceling = t.canceling || clicked;
    let Ok(row) = ui.create("Layout", Some(page)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(32.0).set_margins([0.0, 8.0, 0.0, 8.0]);
    }
    let pending_label = if pending > 0 { tw("settings.transcribe.pendingN", &[("count", &pending.to_string())]) } else { lang::t("settings.transcribe.pendingNone") };
    if let Some(b) = button(ui, row, &pending_label, "outline", Align::Left, |_| btr::handle_transcribe_pending()) {
        let _ = ui.set_prop(b, "enabled", pending > 0);
    }
    let cancel_label = lang::t(if canceling { "settings.transcribe.canceling" } else { "common.cancel" });
    if let Some(b) = button(ui, row, &cancel_label, "flat", Align::Left, |ui| {
        if with(|s| s.tr.canceling || s.tr_cancel_clicked) {
            return;
        }
        with(|s| s.tr_cancel_clicked = true);
        render_tab(ui);
        btr::handle_cancel_transcribe();
    }) {
        let _ = ui.set_prop(b, "enabled", !canceling && (t.running || t.queue > 0));
        if let Some(cb) = ui.control_mut(b) {
            cb.set_margins([8.0, 0.0, 0.0, 0.0]);
        }
    }
    let (visible, p, line) = transcribe_line();
    if visible {
        if let Ok(pb) = ui.create("ProgressBar", Some(page)) {
            let _ = ui.set_prop(pb, "value", (p * 100.0) as f64);
            if let Some(cb) = ui.control_mut(pb) {
                cb.set_align(Align::Top).set_height(6.0).set_margins([0.0, 4.0, 0.0, 4.0]);
            }
            slot(ui, "tr_bar", Some(pb));
        }
        let l = note(ui, page, &line, "key");
        slot(ui, "tr_line", l);
    }
    if !t.last_error.is_empty() {
        let err = if t.last_error_name.is_empty() {
            tw("settings.transcribe.error", &[("error", &t.last_error)])
        } else {
            tw("settings.transcribe.errorNamed", &[("name", &t.last_error_name), ("error", &t.last_error)])
        };
        if let Some(l) = note(ui, page, &err, "key") {
            let _ = ui.set_prop(l, "color", Color::rgb(0xef, 0x44, 0x44));
        }
    }
    for q in &queue {
        let dur = if q.duration > 0 { fmt_dur(q.duration as i64) } else { "—".into() };
        if let Some(ctl) = field(ui, page, &q.name, &dur, 28.0, 80.0) {
            if !q.current {
                let id = q.id.clone();
                if let Some(b) = button(ui, ctl, "", "tool", Align::Right, move |_| btr::handle_remove_transcribe_item(&id)) {
                    let _ = ui.set_prop(b, "icon", "close");
                    if let Some(cb) = ui.control_mut(b) {
                        cb.set_width(28.0).set_height(28.0);
                    }
                }
            }
        }
    }
    if cloud > 0 {
        note(ui, page, &tw("settings.transcribe.pendingCloud", &[("count", &cloud.to_string())]), "key");
    }
    note(ui, page, &lang::t("settings.transcribe.queueHint"), "sub");
}

pub fn on_local_asr_state(st: &LocalAsrInfo) {
    if st.remove_refused {
        crate::app_view::show_toast(&lang::t("settings.transcribe.local.removeBusy"), "", true, 4000);
    }
    with(|s| s.asr = Some(st.clone()));
    refresh("transcribe");
    crate::app_main::on_transcribe_chip(transcribe_chip());
}

pub fn on_transcribe_state(t: &TranscribeState) {
    let t = t.clone();
    with_ui(move |ui| {
        let (failed_before, was_canceling) = with(|s| (s.tr.failed, s.tr.canceling || s.tr_cancel_clicked));
        let structure_changed = with(|s| s.tr.running != t.running || s.tr.queue != t.queue || s.tr.canceling != t.canceling);
        with(|s| {
            s.tr = t.clone();
            if t.canceling || !t.running {
                s.tr_cancel_clicked = false;
            }
            // O "faltam" só desce.
            let key = if t.running { format!("{}#{}", t.current, t.track) } else { String::new() };
            if key != s.tr_eta_for {
                s.tr_eta_for = key;
                s.tr_eta_shown = -1;
            }
            if t.eta > 0 {
                s.tr_eta_shown = if s.tr_eta_shown < 0 { t.eta } else { s.tr_eta_shown.min(t.eta) };
            }
        });
        // Aviso na BORDA do contador e só com motivo novo.
        if t.failed > failed_before && !t.last_error.is_empty() && with(|s| s.tr_last_toast_error != t.last_error) {
            with(|s| s.tr_last_toast_error = t.last_error.clone());
            let name = if t.last_error_name.is_empty() { "—".to_owned() } else { t.last_error_name.clone() };
            crate::app_view::show_toast(&tw("toast.transcribeFailed", &[("name", &name)]), &t.last_error, true, 12000);
        }
        if t.failed == 0 {
            with(|s| s.tr_last_toast_error.clear());
        }
        if was_canceling && !t.canceling && !with(|s| s.tr_cancel_clicked) && !t.running {
            crate::app_view::show_toast(&lang::t("toast.transcribeCanceled"), "", false, 3000);
        }
        sync_tick(ui, t.running);
        if is_open() && with(|s| s.tab == "transcribe") {
            if structure_changed {
                render_tab(ui);
            } else {
                update_transcribe_line(ui);
            }
        }
        crate::app_main::on_transcribe_chip(transcribe_chip());
    });
}

fn update_transcribe_line(ui: &mut Ui) {
    let (_, p, line) = transcribe_line();
    set_slot(ui, "tr_line", "text", line);
    set_slot(ui, "tr_bar", "value", (p * 100.0) as f64);
}

/// O decorrido anda sozinho entre os pushes.
fn sync_tick(ui: &mut Ui, running: bool) {
    let has = with(|s| s.tr_job.is_some());
    if running && !has {
        tr_tick(ui);
    } else if !running && has {
        if let Some(job) = with(|s| s.tr_job.take()) {
            ui.clear_timeout(job);
        }
    }
}

fn tr_tick(ui: &mut Ui) {
    let job = ui.set_timeout(1000, |ui| {
        with(|s| {
            s.tr.elapsed += 1;
            if s.tr_eta_shown > 0 {
                s.tr_eta_shown -= 1;
            }
        });
        if is_open() && with(|s| s.tab == "transcribe") {
            update_transcribe_line(ui);
        }
        tr_tick(ui);
    });
    with(|s| s.tr_job = Some(job));
}

pub fn on_transcribe_pending(count: usize, cloud: usize) {
    with(|s| {
        s.tr_pending = count;
        s.tr_cloud = cloud;
    });
    refresh("transcribe");
}

pub fn on_transcribe_queue(items: &[QueueItem]) {
    with(|s| s.tr_queue = items.to_vec());
    refresh("transcribe");
}

/// Selo da tela inicial: (mostrar, rodando, número, dica).
pub fn transcribe_chip() -> (bool, bool, i32, String) {
    with(|s| {
        let ready = s.asr.as_ref().is_some_and(|a| a.status == "ready");
        let running = s.tr.running;
        let total = s.tr.queue;
        let waiting = (total - if running { 1 } else { 0 }).max(0);
        let show = ready && (running || waiting > 0);
        let canceling = s.tr.canceling || s.tr_cancel_clicked;
        let hint = if canceling {
            lang::t("settings.transcribe.canceling")
        } else if running {
            if waiting > 0 { tw("transcribeChip.runningQueue", &[("count", &waiting.to_string())]) } else { lang::t("transcribeChip.running") }
        } else {
            tw("transcribeChip.queued", &[("count", &waiting.to_string())])
        };
        (show, running, total.max(1) as i32, hint)
    })
}

// ---------------------------------------------------------------------------
// Atualizações e Sobre
// ---------------------------------------------------------------------------

fn update_status() -> (String, bool) {
    let (r, checking) = with(|s| (s.update.clone(), s.checking));
    if checking {
        return (lang::t("settings.updates.checking"), false);
    }
    let Some(r) = r else { return (String::new(), false) };
    if !r.ok {
        (lang::t("settings.updates.failed"), false)
    } else if r.has_update {
        (tw("settings.updates.available", &[("version", &r.latest)]), !r.url.is_empty())
    } else {
        (lang::t("settings.updates.upToDate"), false)
    }
}

fn tab_updates(ui: &mut Ui, page: ComponentId) {
    field(ui, page, &lang::t("settings.updates.versionLabel"), &crate::version::display_version(), 0.0, 0.0);
    if let Some(ctl) = field(ui, page, &lang::t("settings.updates.autoLabel"), &lang::t("settings.updates.autoHint"), 24.0, 60.0) {
        switch(ui, ctl, config::get_config_bool("checkUpdates", true), |ui, on| check_changed(ui, "checkUpdates", on));
    }
    let Ok(row) = ui.create("Layout", Some(page)) else { return };
    if let Some(cb) = ui.control_mut(row) {
        cb.set_align(Align::Top).set_height(32.0).set_margins([0.0, 14.0, 0.0, 0.0]);
    }
    let checking = with(|s| s.checking);
    if let Some(b) = button(ui, row, &lang::t("settings.updates.checkNow"), "outline", Align::Left, |ui| {
        with(|s| s.checking = true);
        render_tab(ui);
        settings::handle_check_updates();
    }) {
        let _ = ui.set_prop(b, "enabled", !checking);
    }
    let (status, has_url) = update_status();
    if has_url {
        if let Some(b) = button(ui, row, &lang::t("settings.updates.openPage"), "", Align::Left, |_| {
            let url = with(|s| s.update.as_ref().map(|u| u.url.clone())).unwrap_or_default();
            if !url.is_empty() {
                bridge::handle_open_url(&url);
            }
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_margins([8.0, 0.0, 0.0, 0.0]);
            }
        }
    }
    if let Some(l) = label(ui, row, &status, "key", Align::Client, 32.0) {
        if let Some(cb) = ui.control_mut(l) {
            cb.set_margins([12.0, 0.0, 0.0, 0.0]);
        }
    }
}

pub fn on_update_result(r: &UpdateInfo) {
    let r = r.clone();
    with_ui(move |ui| {
        with(|s| {
            s.update = Some(r.clone());
            s.checking = false;
        });
        if r.ok && r.has_update {
            crate::app_main::show_update_banner(ui, &r.latest, &r.url);
        }
        if is_open() && with(|s| s.tab == "updates") {
            render_tab(ui);
        }
    });
}

/// As linhas de uma lista do idioma (`about.features.0`, `.1`…).
fn lang_list(key: &str) -> Vec<String> {
    let mut v = Vec::new();
    for i in 0.. {
        let t = lang::t(&format!("{key}.{i}"));
        if t.is_empty() || t.starts_with('[') {
            break;
        }
        v.push(plain(&t));
    }
    v
}

fn tab_about(ui: &mut Ui, page: ComponentId) {
    if let Some((iw, ih, rgba)) = crate::win::main_icon_rgba(256) {
        if let Ok(v) = ui.create("Image", Some(page)) {
            let _ = ui.set_prop(v, "fit", "contain");
            if let Some(iv) = ui.get_mut::<ImageView>(v) {
                iv.set_picture(Some(Image::rgba(iw, ih, rgba)));
            }
            if let Some(cb) = ui.control_mut(v) {
                cb.set_align(Align::Top).set_height(96.0).set_margins([0.0, 16.0, 0.0, 8.0]);
            }
        }
    }
    if let Some(l) = label(ui, page, &format!("NoOBS {}", crate::version::display_version()), "h2", Align::Top, 28.0) {
        let _ = ui.set_prop(l, "h_align", "center");
    }
    if let Some(l) = note(ui, page, &lang::t("about.tagline"), "body") {
        let _ = ui.set_prop(l, "h_align", "center");
    }
    note(ui, page, &plain(&lang::t("about.intro2")), "body");
    note(ui, page, &plain(&lang::t("about.intro3")), "body");
    field(ui, page, &lang::t("about.labels.author"), "Eduardo Rodrigues", 0.0, 0.0);
    if let Some(ctl) = field(ui, page, &lang::t("about.labels.repository"), "github.com/e-delphi/NoOBS", 32.0, 200.0) {
        button(ui, ctl, "GitHub", "outline", Align::Right, |_| bridge::handle_open_url("https://github.com/e-delphi/NoOBS"));
    }
    section(ui, page, &lang::t("about.labels.features"));
    for f in lang_list("about.features") {
        note(ui, page, &format!("•  {f}"), "body");
    }
    section(ui, page, &lang::t("about.labels.builtWith"));
    for f in lang_list("about.builtWith") {
        note(ui, page, &format!("•  {f}"), "body");
    }
    note(ui, page, &lang::t("about.footer"), "sub");
}

// ---------------------------------------------------------------------------
// Restaurar padrões e chaves
// ---------------------------------------------------------------------------

fn restore_defaults(ui: &mut Ui) {
    iron::prelude::confirm(ui, &lang::t("settings.buttons.reset"), &lang::t("toast.restoreConfirmMessage"), &lang::t("toast.restoreConfirmOk"), false, |ui, yes| {
        if yes {
            do_restore(ui);
        }
    });
}

fn do_restore(ui: &mut Ui) {
    bridge::handle_set_language("");
    settings::handle_set_record_dir("");
    settings::handle_set_codec("auto");
    shell::handle_set_window_title("NoOBS");
    settings::handle_set_filename_pattern(settings::DEFAULT_FILENAME_PATTERN);
    shell::handle_set_autostart(true);
    shell::handle_set_run_as_admin(false);
    recording::handle_set_fullscreen_detect(true);
    shell::handle_set_close_to_tray(true);
    shell::handle_set_config_bool("minimizeOnRecord", true);
    recording::handle_set_rec_indicator(false);
    recording::handle_set_rec_indicator_corner("top-right");
    recording::handle_set_rec_indicator_opacity(90);
    recording::handle_set_rec_indicator_clickable(true);
    shell::handle_set_config_bool("playSoundOnRecord", true);
    shell::handle_set_config_bool("stopOnLock", true);
    settings::handle_set_mute_when_device_muted(true);
    btr::handle_set_transcribe_language("app");
    btr::handle_set_transcribe_on_stop(true);
    settings::handle_set_auto_record_on_mic(false);
    settings::handle_set_auto_record_mic_apps("");
    settings::handle_set_auto_record_mic_except("");
    settings::handle_set_check_updates(true);
    settings::handle_set_recording_quality(5);
    settings::handle_set_recording_fps(30);
    settings::handle_set_recording_keyframe(2);
    settings::handle_set_audio_bitrate(AUDIO_BITRATE_DEFAULT);
    settings::handle_set_library_thumbs("auto");
    btr::handle_reset_hidden_lists();
    // Buffer: limites e indicador voltam ao padrão; o liga/desliga é da tela
    // principal e fica como está.
    replay::handle_set_replay_autostart(false);
    replay::handle_set_replay_into_recording(true);
    replay::handle_set_replay_indicator(true);
    replay::handle_set_replay_indicator_corner("top-right");
    replay::handle_set_replay_indicator_opacity(90);
    replay::handle_set_replay_indicator_clickable(true);
    replay::handle_set_replay_apps("");
    replay::handle_set_replay_limits(REPLAY_DEFAULT_SEC as i32, REPLAY_DEFAULT_MB as i32);
    bhk::handle_reset();
    bridge::handle_set_theme("system");
    load_values();
    render_tab(ui);
    crate::app_view::show_toast(&lang::t("toast.fieldsReset"), &lang::t("toast.fieldsResetBody"), false, 4000);
}

/// Uma chave de liga/desliga mudou.
fn check_changed(ui: &mut Ui, key: &str, on: bool) {
    match key {
        "muteWhenDeviceMuted" => settings::handle_set_mute_when_device_muted(on),
        "replayAutoStart" => replay::handle_set_replay_autostart(on),
        "replayIntoRecording" => replay::handle_set_replay_into_recording(on),
        "replayIndicator" => replay::handle_set_replay_indicator(on),
        "replayIndicatorClickable" => replay::handle_set_replay_indicator_clickable(on),
        "recIndicator" => recording::handle_set_rec_indicator(on),
        "recIndicatorClickable" => recording::handle_set_rec_indicator_clickable(on),
        "fullscreenDetect" => recording::handle_set_fullscreen_detect(on),
        "playSoundOnRecord" | "minimizeOnRecord" | "stopOnLock" => shell::handle_set_config_bool(key, on),
        "autoRecordOnMic" => settings::handle_set_auto_record_on_mic(on),
        "transcribeOnStop" => btr::handle_set_transcribe_on_stop(on),
        "checkUpdates" => settings::handle_set_check_updates(on),
        _ => log!("Configuracoes: opcao desconhecida \"{key}\"."),
    }
    // As que mostram ou escondem outras linhas refazem a aba.
    if matches!(key, "recIndicator" | "replayIndicator" | "autoRecordOnMic") {
        ui.set_timeout(0, render_tab);
    }
}

// ---------------------------------------------------------------------------
// Avisos do core
// ---------------------------------------------------------------------------

pub fn on_settings_changed() {
    with_ui(|ui| {
        if is_open() {
            // Com a tela aberta, a pasta é do core (ele resolve o vazio).
            set_slot(ui, "record_dir", "text", recording::record_dir().to_string_lossy().into_owned());
        }
    });
}

pub fn on_record_dir_picked(path: &str) {
    let p = path.to_owned();
    with_ui(move |ui| set_slot(ui, "record_dir", "text", p.as_str()));
    settings::handle_set_record_dir(path);
}

pub fn on_encoder_caps() {
    refresh("recording");
}

pub fn on_replay_state(r: &ReplayState) {
    with(|s| s.replay = Some(r.clone()));
    refresh("replay");
}

pub fn on_language_changed() {
    if is_open() {
        with_ui(|ui| {
            let tab = with(|s| s.tab.clone());
            show_tab(ui, &tab);
        });
    }
}

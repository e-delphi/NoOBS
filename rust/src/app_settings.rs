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
    /// As barras do teste de qualidade (largura só depois do layout).
    vt_bars: Vec<(ComponentId, f32)>,
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
    zoom: i32,
    split: f32,
    pan: (f32, f32),
    drag: Option<(f32, f32, (f32, f32), bool)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { tab: "general".into(), quality: 5, fps: 30, keyframe: 2, vt: VtState { split: 0.5, ..Default::default() }, ..Default::default() });
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

/// Refaz a aba se for a aberta (dados novos do core).
fn refresh(tab: &'static str) {
    if is_open() && with(|s| s.tab == tab) {
        with_ui(render_tab);
    }
}

// ---------------------------------------------------------------------------
// Geral
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

fn corner_labels() -> Vec<String> {
    ["topLeft", "topRight", "bottomLeft", "bottomRight"].iter().map(|c| lang::t(&format!("settings.recIndicator.corners.{c}"))).collect()
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
        v.push(t);
    }
    v
}

// ---------------------------------------------------------------------------
// Restaurar padrões e chaves
// ---------------------------------------------------------------------------

fn restore_defaults(ui: &mut Ui) {
    crate::app_view::confirm(
        ui,
        &lang::t("settings.buttons.reset"),
        &lang::t("toast.restoreConfirmMessage"),
        &lang::t("toast.restoreConfirmOk"),
        false,
        crate::app_view::ConfirmIcon::Merge,
        do_restore,
    );
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

// ---------------------------------------------------------------------------
// Construtor da página (o `VerticalLayout` das telas originais)
// ---------------------------------------------------------------------------
//
// Cada aba é uma coluna: os campos (rótulo, controle, dica) com 6 px entre as
// partes, 10 px entre um campo e a divisória. Os itens entram com `align top` e
// a margem de cima é o espaço pedido (`NEXT_GAP`).

/// Espaço entre as partes de um campo.
const FIELD_GAP: f32 = 6.0;
/// Espaço entre os campos e as divisórias.
const TAB_GAP: f32 = 10.0;
/// Largura máxima da coluna dos campos.
const COL_MAX: f32 = 760.0;

thread_local! {
    /// O espaço antes do próximo item da página.
    static NEXT_GAP: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
    /// (largura da coluna, largura das divisórias).
    static WIDTHS: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((760.0, 760.0)) };
}

fn col_w() -> f32 {
    WIDTHS.with(|w| w.get().0)
}

fn gap(px: f32) {
    NEXT_GAP.with(|g| g.set(px));
}

/// Põe o item no fim da coluna, com a altura dada e o espaço pedido em cima.
fn place(ui: &mut Ui, id: ComponentId, h: f32) {
    let top = NEXT_GAP.with(|g| g.replace(FIELD_GAP));
    if let Some(cb) = ui.control_mut(id) {
        cb.set_align(Align::Top).set_height(h).set_margins([0.0, top, 0.0, 0.0]);
    }
}

fn clear_page(ui: &mut Ui) -> Option<ComponentId> {
    let p = c(ui, "lySettings")?;
    for child in ui.children(p).to_vec() {
        ui.free(child);
    }
    with(|s| s.slots.clear());
    Some(p)
}

/// Altura do texto do rótulo `l` quebrado na largura `w` (com a fonte do estilo dele).
fn wrapped_height(ui: &Ui, l: ComponentId, text: &str, w: f32) -> f32 {
    let style = TextStyle { font: ui.font_of(l), wrap: true, ..TextStyle::default() };
    ui.measure_text_wrapped(text, &style, w.max(40.0)).height.ceil() + 1.0
}

fn text_width(ui: &Ui, l: ComponentId, text: &str) -> f32 {
    let style = TextStyle { font: ui.font_of(l), ..TextStyle::default() };
    ui.measure_text(text, &style).width.ceil()
}

/// Um rótulo solto num pai (dentro de uma linha), sem entrar na coluna.
fn label(ui: &mut Ui, parent: ComponentId, text: &str, style: &str, align: Align, w: f32, h: f32) -> Option<ComponentId> {
    let l = ui.create("Label", Some(parent)).ok()?;
    let _ = ui.set_prop(l, "style", style);
    let _ = ui.set_prop(l, "text", text);
    let _ = ui.set_prop(l, "hit_test", false);
    if let Some(cb) = ui.control_mut(l) {
        cb.set_align(align).set_height(h);
        if w > 0.0 {
            cb.set_width(w);
        }
    }
    Some(l)
}

/// Texto da coluna, quebrado na largura dela (rótulo, dica, aviso).
fn text_in(ui: &mut Ui, page: ComponentId, text: &str, style: &str, w: f32) -> Option<ComponentId> {
    if text.is_empty() {
        return None;
    }
    let l = ui.create("Label", Some(page)).ok()?;
    let _ = ui.set_prop(l, "style", style);
    let _ = ui.set_prop(l, "text", text);
    let _ = ui.set_prop(l, "wrap", true);
    let _ = ui.set_prop(l, "v_align", "leading");
    let h = wrapped_height(ui, l, text, w);
    place(ui, l, h);
    if let Some(cb) = ui.control_mut(l) {
        cb.set_width(w);
        let m = cb.margins();
        cb.set_margins([0.0, m.top, (col_w() - w).max(0.0), 0.0]);
    }
    Some(l)
}

/// Linhas que um texto com `<b>` ocupa na largura `w` (os trechos em negrito medidos
/// em negrito).
fn md_lines(ui: &Ui, l: ComponentId, html: &str, w: f32) -> f32 {
    let font = ui.font_of(l);
    let mut total = 0.0;
    for (run, bold, _) in iron::controls::markup_runs(html) {
        let mut f = font.clone();
        if bold {
            f.weight = FontWeight::Bold;
        }
        total += ui.measure_text(&run, &TextStyle { font: f, ..TextStyle::default() }).width;
    }
    if total <= w {
        return 1.0;
    }
    let plain = iron::controls::markup_plain(html);
    let one = ui.measure_text("Ág", &TextStyle { font: font.clone(), ..TextStyle::default() }).height.max(1.0);
    (wrapped_height(ui, l, &plain, w * 0.97) / one).round().max(1.0)
}

/// Texto com `<b>` e `<code>` (os textos do idioma com HTML simples), quebrado.
fn text_in_md(ui: &mut Ui, page: ComponentId, html: &str, style: &str, w: f32) -> Option<ComponentId> {
    if html.is_empty() {
        return None;
    }
    let l = ui.create("Label", Some(page)).ok()?;
    let _ = ui.set_prop(l, "style", style);
    let _ = ui.set_prop(l, "markup", true);
    let _ = ui.set_prop(l, "v_align", "leading");
    let _ = ui.set_prop(l, "text", html);
    // O negrito é um pouco mais largo: mede com uma folga.
    let h = wrapped_height(ui, l, &iron::controls::markup_plain(html), w * 0.97);
    place(ui, l, h);
    if let Some(cb) = ui.control_mut(l) {
        let m = cb.margins();
        cb.set_margins([0.0, m.top, (col_w() - w).max(0.0), 0.0]);
    }
    Some(l)
}

/// `.settings-field label`: 12 px, seminegrito, `text_2`.
fn s_label(ui: &mut Ui, page: ComponentId, text: &str) -> Option<ComponentId> {
    text_in(ui, page, text, "s_label", col_w())
}

/// `.settings-hint`: 11 px, `text_3`.
fn s_hint(ui: &mut Ui, page: ComponentId, text: &str) -> Option<ComponentId> {
    text_in(ui, page, text, "s_hint", col_w())
}

/// `.settings-warn`: âmbar com a barra à esquerda.
fn s_warn(ui: &mut Ui, page: ComponentId, text: &str) {
    let Ok(box_) = ui.create("Layout", Some(page)) else { return };
    let Some(l) = label(ui, box_, text, "s_warn", Align::Client, 0.0, 0.0) else { return };
    let _ = ui.set_prop(l, "wrap", true);
    let _ = ui.set_prop(l, "v_align", "leading");
    let h = wrapped_height(ui, l, text, col_w() - 10.0);
    if let Some(cb) = ui.control_mut(l) {
        cb.set_margins([10.0, 0.0, 0.0, 0.0]);
    }
    if let Ok(bar) = ui.create("Rectangle", Some(box_)) {
        let _ = ui.set_prop(bar, "fill", "amber");
        let _ = ui.set_prop(bar, "stroke_width", 0.0);
        if let Some(cb) = ui.control_mut(bar) {
            cb.set_rect(0.0, 0.0, 2.0, h).set_hit_test(false);
        }
    }
    place(ui, box_, h);
}

/// A divisória entre campos (`<hr>`), até a borda do corpo.
fn rule(ui: &mut Ui, page: ComponentId) {
    let Ok(r) = ui.create("Rectangle", Some(page)) else { return };
    let _ = ui.set_prop(r, "style", "sep");
    gap(TAB_GAP);
    place(ui, r, 1.0);
    let (col, rule) = WIDTHS.with(|w| w.get());
    if let Some(cb) = ui.control_mut(r) {
        let m = cb.margins();
        cb.set_margins([0.0, m.top, -(rule - col).max(0.0), 0.0]);
    }
    gap(TAB_GAP);
}

/// Uma linha da coluna (para os controles lado a lado).
fn row(ui: &mut Ui, page: ComponentId, h: f32) -> Option<ComponentId> {
    let r = ui.create("Layout", Some(page)).ok()?;
    place(ui, r, h);
    Some(r)
}

/// `.settings-check-row`: a caixa e o texto (12 px, seminegrito), clicável inteiro.
fn s_check(ui: &mut Ui, page: ComponentId, on: bool, text: &str, f: impl Fn(&mut Ui, bool) + 'static) -> Option<ComponentId> {
    let b = ui.create("CheckBox", Some(page)).ok()?;
    let _ = ui.set_prop(b, "text", text);
    let _ = ui.set_prop(b, "box_size", 16.0);
    let _ = ui.set_prop(b, "style", "web");
    let _ = ui.set_prop(b, "checked", on);
    let h = wrapped_height(ui, b, text, col_w() - 24.0).max(16.0);
    place(ui, b, h);
    ui.on(b, "on_change", move |ui, sender, _| {
        let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
        f(ui, on);
    });
    Some(b)
}

/// `.settings-input`: 32 px; `on_edit` a cada tecla, `on_commit` no Enter ou ao sair.
fn s_input(
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

/// O campo de texto ocupando a coluna.
fn s_input_col(
    ui: &mut Ui,
    page: ComponentId,
    text: &str,
    placeholder: &str,
    on_edit: impl Fn(&mut Ui, &str) + 'static,
    on_commit: impl Fn(&mut Ui, &str) + 'static,
) -> Option<ComponentId> {
    let e = s_input(ui, page, text, placeholder, on_edit, on_commit)?;
    place(ui, e, 32.0);
    keep_col(ui, e);
    Some(e)
}

/// O item da coluna não passa da largura dela (a página vai até a borda das divisórias).
fn keep_col(ui: &mut Ui, id: ComponentId) {
    let (col, rule) = WIDTHS.with(|w| w.get());
    let _ = rule;
    let page_w = c(ui, "lySettings").and_then(|p| ui.bounds(p)).map_or(col, |b| b.width);
    let _ = page_w;
    if let Some(cb) = ui.control_mut(id) {
        let m = cb.margins();
        cb.set_margins([m.left, m.top, 0.0, m.bottom]);
    }
}

/// `select.settings-input`: 32 px.
fn s_select(ui: &mut Ui, parent: ComponentId, items: &[String], index: usize, f: impl Fn(&mut Ui, usize) + 'static) -> Option<ComponentId> {
    let b = ui.create("ComboBox", Some(parent)).ok()?;
    let _ = ui.set_prop(b, "arrow", "triangle");
    let _ = ui.set_prop(b, "items", items.iter().map(|s| s.replace(';', ",")).collect::<Vec<_>>().join(";"));
    let _ = ui.set_prop(b, "item_index", index as i64);
    ui.on(b, "on_change", move |ui, sender, _| {
        let i = ui.get_prop(sender, "item_index").ok().and_then(|v| v.to_i64().ok()).unwrap_or(0).max(0) as usize;
        f(ui, i);
    });
    Some(b)
}

fn s_select_col(ui: &mut Ui, page: ComponentId, items: &[String], index: usize, f: impl Fn(&mut Ui, usize) + 'static) -> Option<ComponentId> {
    let b = s_select(ui, page, items, index, f)?;
    place(ui, b, 32.0);
    Some(b)
}

/// `.settings-btn`: 13 px, 8 × 14 de respiro, raio 6. `kind`: "" | "primary" | "danger".
fn s_button(ui: &mut Ui, parent: ComponentId, text: &str, kind: &str, align: Align, f: impl Fn(&mut Ui) + 'static) -> Option<ComponentId> {
    let b = ui.create("Button", Some(parent)).ok()?;
    let _ = ui.set_prop(b, "text", text);
    let _ = ui.set_prop(b, "style", match kind {
        "primary" => "s_primary",
        "danger" => "s_danger",
        _ => "s",
    });
    let w = text_width(ui, b, text) + 30.0;
    if let Some(cb) = ui.control_mut(b) {
        cb.set_align(align).set_width(w).set_height(33.0).set_margins([0.0, 0.0, 8.0, 0.0]);
    }
    ui.on(b, "on_click", move |ui, _, _| f(ui));
    Some(b)
}

/// Uma linha com um botão (ou mais) à esquerda.
fn button_row(ui: &mut Ui, page: ComponentId) -> Option<ComponentId> {
    row(ui, page, 33.0)
}

/// O slider da web: trilho de 4 px, polegar cheio de 14 px, marcas embaixo (10 px)
/// e as legendas das pontas (11 px). `f` recebe o índice 0..=`steps`.
fn s_slider(ui: &mut Ui, page: ComponentId, steps: usize, value: usize, ticks: &[String], ends: (&str, &str), f: impl Fn(&mut Ui, usize) + 'static) -> Option<ComponentId> {
    let h = if ticks.is_empty() { 16.0 } else { 32.0 };
    let r = row(ui, page, h)?;
    let mut middle_w = col_w();
    let mut place_end = |ui: &mut Ui, text: &str, align: Align| {
        if text.is_empty() {
            return;
        }
        if let Some(l) = label(ui, r, text, "s_end", align, 0.0, 32.0) {
            let w = text_width(ui, l, text).max(44.0);
            if align == Align::Right {
                let _ = ui.set_prop(l, "h_align", "trailing");
            }
            if let Some(cb) = ui.control_mut(l) {
                cb.set_width(w).set_margins(if align == Align::Left { [0.0, 0.0, 10.0, 0.0] } else { [10.0, 0.0, 0.0, 0.0] });
            }
            middle_w -= w + 10.0;
        }
    };
    place_end(ui, ends.0, Align::Left);
    place_end(ui, ends.1, Align::Right);
    let mid = ui.create("Layout", Some(r)).ok()?;
    if let Some(cb) = ui.control_mut(mid) {
        cb.set_align(Align::Client);
    }
    let t = ui.create("TrackBar", Some(mid)).ok()?;
    let _ = ui.set_prop(t, "min", 0.0);
    let _ = ui.set_prop(t, "max", steps.max(1) as f64);
    let _ = ui.set_prop(t, "value", value as f64);
    let _ = ui.set_prop(t, "frequency", 1.0);
    let _ = ui.set_prop(t, "solid_thumb", true);
    let _ = ui.set_prop(t, "thumb_size", 14.0);
    let _ = ui.set_prop(t, "track_size", 4.0);
    if let Some(cb) = ui.control_mut(t) {
        cb.set_align(Align::Top).set_height(16.0);
    }
    let n = ticks.len();
    for (i, tick) in ticks.iter().enumerate() {
        if let Some(l) = label(ui, mid, tick, "s_tick", Align::None, 0.0, 14.0) {
            let w = text_width(ui, l, tick) + 2.0;
            let x = 7.0 + if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 } * (middle_w - 14.0) - w / 2.0;
            if let Some(cb) = ui.control_mut(l) {
                cb.set_rect(x, 18.0, w, 14.0);
            }
        }
    }
    ui.on(t, "on_change", move |ui, sender, _| {
        let v = ui.get_prop(sender, "value").ok().and_then(|v| v.to_f64().ok()).unwrap_or(0.0);
        f(ui, v.round().max(0.0) as usize);
    });
    Some(t)
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
    let pad = ui.control(p).map_or(28.0, |cb| cb.padding().top + cb.padding().bottom);
    if let Some(cb) = ui.control_mut(p) {
        cb.set_height(h + pad);
    }
    ui.layout();
}

// ---------------------------------------------------------------------------
// Abas
// ---------------------------------------------------------------------------

fn render_sidebar(ui: &mut Ui) {
    let Some(side) = c(ui, "lySetTabs") else { return };
    for child in ui.children(side).to_vec() {
        ui.free(child);
    }
    let current = with(|s| s.tab.clone());
    for tab in TABS {
        let on = tab == current;
        let Ok(row) = ui.create("Panel", Some(side)) else { continue };
        let _ = ui.set_prop(row, "style", if on { "s_tab_on" } else { "s_tab" });
        if let Some(cb) = ui.control_mut(row) {
            cb.set_align(Align::Top).set_height(35.0).set_padding([12.0, 0.0, 0.0, 0.0]).set_margins([0.0, 0.0, 0.0, 2.0]).set_cursor(CursorShape::Pointer);
        }
        label(ui, row, &lang::t(&format!("settings.tabs.{tab}")), if on { "s_tab_on" } else { "s_tab" }, Align::Client, 0.0, 35.0);
        ui.on(row, "on_click", move |ui, _, _| show_tab(ui, tab));
    }
}

pub fn show_tab(ui: &mut Ui, tab: &str) {
    with(|s| s.tab = tab.to_owned());
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
    // A coluna: até 760 px, encostada à esquerda (18 px); as divisórias vão até 18 px
    // da borda do corpo.
    let body = c(ui, "pnlSettings").and_then(|s| ui.bounds(s)).map_or(1190.0, |b| b.width) - 190.0;
    let col = (body - 36.0).min(COL_MAX).max(200.0);
    WIDTHS.with(|w| w.set((col, (body - 36.0).max(col))));
    let page_w = ui.bounds(page).map_or(body, |b| b.width);
    if let Some(cb) = ui.control_mut(page) {
        // Na web o 1º campo de toda aba menos a Geral herda 10 px do campo escondido.
        cb.set_padding([18.0, if tab == "general" { 14.0 } else { 24.0 }, (page_w - 18.0 - col).max(0.0), 14.0]);
    }
    gap(0.0);
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

pub fn resized(ui: &mut Ui) {
    // As dicas quebram pela largura: a altura das linhas muda.
    let w = c(ui, "scrSettings").and_then(|s| ui.bounds(s)).map_or(0.0, |b| b.width);
    thread_local! {
        static LAST: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
    }
    if is_open() && LAST.with(|l| (l.replace(w) - w).abs() > 4.0) {
        ui.set_timeout(0, render_tab);
    }
}

// ---------------------------------------------------------------------------
// Geral
// ---------------------------------------------------------------------------

/// `.theme-toggle`: Sistema / Claro / Escuro, o escolhido em destaque.
fn theme_toggle(ui: &mut Ui, page: ComponentId) {
    let pref = config::get_config_str("theme", "system");
    let Ok(boxp) = ui.create("Panel", Some(page)) else { return };
    let _ = ui.set_prop(boxp, "style", "s_seg_box");
    let mut x = 4.0;
    for (mode, icon) in [("system", "monitor"), ("light", "sun"), ("dark", "moon")] {
        let Ok(b) = ui.create("Button", Some(boxp)) else { continue };
        let text = lang::t(&format!("settings.theme.{mode}"));
        let _ = ui.set_prop(b, "style", if pref == mode { "s_seg_on" } else { "s_seg" });
        let _ = ui.set_prop(b, "text", text.as_str());
        let _ = ui.set_prop(b, "icon", icon);
        let _ = ui.set_prop(b, "icon_size", 16.0);
        let key = match mode {
            "system" => "settings.theme.toggleSystem",
            "light" => "settings.theme.toggleLight",
            _ => "settings.theme.toggleDark",
        };
        let _ = ui.set_prop(b, "hint", lang::t(key));
        let w = text_width(ui, b, &text) + 16.0 + 6.0 + 28.0;
        if let Some(cb) = ui.control_mut(b) {
            cb.set_rect(x, 4.0, w, 27.0);
        }
        x += w + 3.0;
        ui.on(b, "on_click", move |ui, _, _| {
            bridge::handle_set_theme(mode);
            ui.set_timeout(0, render_tab);
        });
    }
    place(ui, boxp, 35.0);
    if let Some(cb) = ui.control_mut(boxp) {
        let m = cb.margins();
        cb.set_margins([0.0, m.top, (col_w() - x - 1.0).max(0.0), 0.0]);
    }
}

fn tab_general(ui: &mut Ui, page: ComponentId) {
    s_label(ui, page, &lang::t("settings.theme.label"));
    theme_toggle(ui, page);
    rule(ui, page);
    // Idioma: "Automático" + os embutidos (nome nativo).
    s_label(ui, page, &lang::t("settings.language.label"));
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
    s_select_col(ui, page, &labels, idx, move |_, i| bridge::handle_set_language(codes.get(i).map_or("", |s| s.as_str())));
    s_hint(ui, page, &lang::t("settings.language.hint"));
    rule(ui, page);
    s_label(ui, page, &lang::t("settings.windowTitle.label"));
    s_input_col(ui, page, &config::get_config_str("windowTitle", "NoOBS"), "NoOBS", |_, _| {}, |_, t| {
        let t = t.trim().to_owned();
        if t != config::get_config_str("windowTitle", "NoOBS") {
            shell::handle_set_window_title(&t);
        }
    });
    s_hint(ui, page, &lang::t("settings.windowTitle.hint"));
    rule(ui, page);
    s_check(ui, page, crate::autostart::is_autostart_enabled(), &lang::t("settings.autostart.label"), |_, on| shell::handle_set_autostart(on));
    s_hint(ui, page, &lang::t("settings.autostart.hint"));
    rule(ui, page);
    s_check(ui, page, config::get_config_bool("runAsAdmin", false), &lang::t("settings.runAsAdmin.label"), |_, on| shell::handle_set_run_as_admin(on));
    s_hint(ui, page, &lang::t("settings.runAsAdmin.hint"));
    s_hint(ui, page, &lang::t(if crate::elevate::is_process_elevated() { "settings.runAsAdmin.stateOn" } else { "settings.runAsAdmin.stateOff" }));
    rule(ui, page);
    s_check(ui, page, config::get_config_bool("closeToTray", true), &lang::t("settings.closeToTray.label"), |_, on| shell::handle_set_close_to_tray(on));
    s_hint(ui, page, &lang::t("settings.closeToTray.hint"));
    rule(ui, page);
    s_label(ui, page, &lang::t("settings.reset.label"));
    s_hint(ui, page, &lang::t("settings.reset.hint"));
    if let Some(r) = button_row(ui, page) {
        s_button(ui, r, &lang::t("settings.buttons.reset"), "", Align::Left, restore_defaults);
    }
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

/// `.rec-indicator-preview`: a pílula escura com a bolinha e o tempo.
fn indicator_preview(ui: &mut Ui, parent: ComponentId, buffer: bool, opacity: i32) -> Option<ComponentId> {
    let text = if buffer { "2:13 / 5:00" } else { "00:00:00" };
    let p = ui.create("Panel", Some(parent)).ok()?;
    let _ = ui.set_prop(p, "style", "s_pill");
    let _ = ui.set_prop(p, "opacity", opacity as f64 / 100.0);
    let dot = ui.create("Circle", Some(p)).ok()?;
    let _ = ui.set_prop(dot, "fill", if buffer { "green" } else { "alert" });
    let _ = ui.set_prop(dot, "stroke_width", 0.0);
    if let Some(cb) = ui.control_mut(dot) {
        cb.set_rect(8.0, 6.0, 8.0, 8.0).set_hit_test(false);
    }
    let l = label(ui, p, text, "s_pill", Align::None, 0.0, 20.0)?;
    let tw = text_width(ui, l, text) + 2.0;
    if let Some(cb) = ui.control_mut(l) {
        cb.set_rect(22.0, 0.0, tw, 20.0);
    }
    if let Some(cb) = ui.control_mut(p) {
        cb.set_align(Align::Right).set_width(tw + 30.0).set_margins([8.0, 0.0, 8.0, 0.0]);
    }
    Some(p)
}

/// As opções de um indicador na tela (clicável, canto, opacidade com a prévia).
fn indicator_options(ui: &mut Ui, page: ComponentId, buffer: bool, clickable: (bool, &'static str, &str), corner: &str, opacity: i32, on_corner: fn(&str), on_opacity: fn(i32)) {
    gap(FIELD_GAP + 4.0);
    let key = clickable.1;
    s_check(ui, page, clickable.0, &lang::t(clickable.2), move |ui, on| check_changed(ui, key, on));
    s_hint(ui, page, &lang::t("settings.recIndicator.clickableHint"));
    gap(14.0);
    if let Some(r) = row(ui, page, 32.0) {
        if let Some(l) = label(ui, r, &lang::t("settings.recIndicator.cornerLabel"), "s_inline", Align::Left, 0.0, 32.0) {
            let w = text_width(ui, l, &lang::t("settings.recIndicator.cornerLabel"));
            if let Some(cb) = ui.control_mut(l) {
                cb.set_width(w).set_margins([0.0, 0.0, 8.0, 0.0]);
            }
        }
        let i = CORNERS.iter().position(|c| *c == corner).unwrap_or(1);
        if let Some(b) = s_select(ui, r, &corner_labels(), i, move |_, i| on_corner(CORNERS.get(i).copied().unwrap_or("top-right"))) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Client);
            }
        }
    }
    gap(if buffer { 0.0 } else { 8.0 });
    let Some(r) = row(ui, page, 20.0) else { return };
    if let Some(l) = label(ui, r, &lang::t("settings.recIndicator.opacityLabel"), "s_inline", Align::Left, 0.0, 20.0) {
        let w = text_width(ui, l, &lang::t("settings.recIndicator.opacityLabel"));
        if let Some(cb) = ui.control_mut(l) {
            cb.set_width(w).set_margins([0.0, 0.0, 8.0, 0.0]);
        }
    }
    let pct = label(ui, r, &format!("{opacity}%"), "s_inline", Align::Right, 42.0, 20.0);
    if let Some(l) = pct {
        let _ = ui.set_prop(l, "h_align", "trailing");
    }
    let preview = indicator_preview(ui, r, buffer, opacity);
    let mid = ui.create("Layout", Some(r)).ok();
    if let Some(mid) = mid {
        if let Some(cb) = ui.control_mut(mid) {
            cb.set_align(Align::Client);
        }
        if let Ok(t) = ui.create("TrackBar", Some(mid)) {
            let _ = ui.set_prop(t, "min", 0.0);
            let _ = ui.set_prop(t, "max", 16.0);
            let _ = ui.set_prop(t, "value", ((opacity - 20) / 5) as f64);
            let _ = ui.set_prop(t, "frequency", 1.0);
            let _ = ui.set_prop(t, "solid_thumb", true);
            let _ = ui.set_prop(t, "thumb_size", 14.0);
            let _ = ui.set_prop(t, "track_size", 4.0);
            if let Some(cb) = ui.control_mut(t) {
                cb.set_align(Align::VertCenter).set_height(16.0);
            }
            ui.on(t, "on_change", move |ui, sender, _| {
                let v = ui.get_prop(sender, "value").ok().and_then(|v| v.to_f64().ok()).unwrap_or(0.0).round() as i32;
                let pct = 20 + v * 5;
                if let Some(l) = pct_label(ui, sender) {
                    let _ = ui.set_prop(l, "text", format!("{pct}%"));
                }
                if let Some(p) = preview {
                    let _ = ui.set_prop(p, "opacity", pct as f64 / 100.0);
                }
                debounce(ui, "opacity", 300, move |_| on_opacity(pct));
            });
            with(|s| {
                if let Some(l) = pct {
                    s.slots.insert(if buffer { "op_pct_buffer" } else { "op_pct_rec" }, l);
                }
            });
            let _ = t;
        }
    }
    fn pct_label(ui: &Ui, slider: ComponentId) -> Option<ComponentId> {
        let row = ui.parent(ui.parent(slider)?)?;
        ui.children(row).iter().copied().find(|&c| ui.control(c).is_some_and(|cb| cb.align() == Align::Right && cb.width() == 42.0))
    }
}

fn tab_onrecord(ui: &mut Ui, page: ComponentId) {
    let rec_on = config::get_config_bool("recIndicator", false);
    s_check(ui, page, rec_on, &lang::t("settings.recIndicator.label"), |ui, on| check_changed(ui, "recIndicator", on));
    text_in_md(ui, page, &lang::t("settings.recIndicator.hint"), "s_hint", col_w());
    if rec_on {
        let corner = config::get_config_str("recIndicatorCorner", "top-right");
        let op = config::get_config_int("recIndicatorOpacity", 90).clamp(20, 100);
        indicator_options(
            ui,
            page,
            false,
            (config::get_config_bool("recIndicatorClickable", true), "recIndicatorClickable", "settings.recIndicator.clickable"),
            &corner,
            op,
            recording::handle_set_rec_indicator_corner,
            recording::handle_set_rec_indicator_opacity,
        );
    }
    rule(ui, page);
    let replay_on = with(|s| s.replay.as_ref().map(|r| r.indicator)).unwrap_or(true);
    let which = match (rec_on, replay_on) {
        (true, true) => "labelBoth",
        (true, false) => "labelRecording",
        (false, true) => "labelBuffer",
        _ => "labelPreviews",
    };
    s_check(ui, page, config::get_config_bool("fullscreenDetect", true), &lang::t(&format!("settings.fullscreenDetect.{which}")), |ui, on| check_changed(ui, "fullscreenDetect", on));
    s_hint(ui, page, &lang::t(if rec_on || replay_on { "settings.fullscreenDetect.hint" } else { "settings.fullscreenDetect.hintPreviews" }));
    rule(ui, page);
    s_check(ui, page, config::get_config_bool("playSoundOnRecord", true), &lang::t("settings.playSound.label"), |ui, on| check_changed(ui, "playSoundOnRecord", on));
    s_hint(ui, page, &lang::t("settings.playSound.hint"));
    rule(ui, page);
    let tray = config::get_config_bool("closeToTray", true);
    let (ml, mh) = if tray { ("settings.minimizeOnRecord.labelTray", "settings.minimizeOnRecord.hintTray") } else { ("settings.minimizeOnRecord.labelTaskbar", "settings.minimizeOnRecord.hintTaskbar") };
    s_check(ui, page, config::get_config_bool("minimizeOnRecord", true), &lang::t(ml), |ui, on| check_changed(ui, "minimizeOnRecord", on));
    s_hint(ui, page, &lang::t(mh));
    rule(ui, page);
    let auto = config::get_config_bool("autoRecordOnMic", false);
    s_check(ui, page, auto, &lang::t("settings.autoRecord.label"), |ui, on| check_changed(ui, "autoRecordOnMic", on));
    s_hint(ui, page, &lang::t("settings.autoRecord.hint"));
    let apps = config::get_config_str("autoRecordMicApps", "");
    s_input_col(ui, page, &apps, &lang::t("settings.autoRecord.appsPlaceholder"), |_, _| {}, |ui, t| {
        let t = t.trim().to_owned();
        if t != config::get_config_str("autoRecordMicApps", "") {
            settings::handle_set_auto_record_mic_apps(&t);
            ui.set_timeout(0, render_tab);
        }
    });
    s_hint(ui, page, &lang::t("settings.autoRecord.appsHint"));
    if apps.is_empty() {
        s_input_col(ui, page, &config::get_config_str("autoRecordMicExcept", ""), &lang::t("settings.autoRecord.exceptPlaceholder"), |_, _| {}, |_, t| {
            let t = t.trim().to_owned();
            if t != config::get_config_str("autoRecordMicExcept", "") {
                settings::handle_set_auto_record_mic_except(&t);
            }
        });
        s_hint(ui, page, &lang::t("settings.autoRecord.exceptHint"));
    }
    if auto {
        gap(FIELD_GAP + 6.0);
        if let Ok(line) = ui.create("Rectangle", Some(page)) {
            let _ = ui.set_prop(line, "style", "sep");
            place(ui, line, 1.0);
        }
        gap(12.0);
        text_in(ui, page, &lang::t("settings.autoRecord.devicesLabel"), "s_name", col_w());
        gap(2.0);
        s_hint(ui, page, &lang::t("settings.autoRecord.devicesHint"));
        device_groups(ui, page, |d| (!d.hidden).then_some(d.auto), |_, id, on| {
            set_device_flag(id, |d| d.auto = on);
            crate::bridge::devices::handle_set_auto_source(id, on);
        }, &lang::t("settings.autoRecord.devicesEmpty"));
    }
    rule(ui, page);
    s_check(ui, page, config::get_config_bool("stopOnLock", true), &lang::t("settings.stopOnLock.label"), |ui, on| check_changed(ui, "stopOnLock", on));
    s_hint(ui, page, &lang::t("settings.stopOnLock.hint"));
}

// ---------------------------------------------------------------------------
// Arquivos
// ---------------------------------------------------------------------------

fn tab_files(ui: &mut Ui, page: ComponentId) {
    s_label(ui, page, &lang::t("settings.recordDir.label"));
    if let Some(r) = row(ui, page, 33.0) {
        if let Some(b) = s_button(ui, r, &lang::t("settings.recordDir.browse"), "", Align::Right, |_| settings::handle_pick_record_dir()) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_margins([8.0, 0.0, 0.0, 0.0]);
            }
        }
        if let Some(e) = s_input(ui, r, &recording::record_dir().to_string_lossy(), "C:\\Users\\...\\Videos", |_, _| {}, |_, p| {
            let p = p.trim().to_owned();
            if p != recording::record_dir().to_string_lossy() {
                settings::handle_set_record_dir(&p);
            }
        }) {
            if let Some(cb) = ui.control_mut(e) {
                cb.set_align(Align::Client).set_margins([0.0, 0.5, 0.0, 0.5]);
            }
            slot(ui, "record_dir", Some(e));
        }
    }
    rule(ui, page);
    let pattern = config::get_config_str("filenamePattern", settings::DEFAULT_FILENAME_PATTERN);
    s_label(ui, page, &lang::t("settings.filename.label"));
    s_input_col(
        ui,
        page,
        &pattern,
        "{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}",
        |ui, t| set_slot(ui, "filename_preview", "text", filename_preview(t)),
        |_, t| {
            let t = t.trim().to_owned();
            if t != config::get_config_str("filenamePattern", settings::DEFAULT_FILENAME_PATTERN) {
                settings::handle_set_filename_pattern(&t);
            }
        },
    );
    text_in_md(ui, page, &lang::t("settings.filename.hint"), "s_hint", col_w());
    let preview = s_hint(ui, page, &filename_preview(&pattern));
    slot(ui, "filename_preview", preview);
    rule(ui, page);
    let lib = config::get_config_str("libraryThumbs", "auto");
    let li = LIBRARY_MODES.iter().position(|m| *m == lib).unwrap_or(0);
    s_label(ui, page, &lang::t("settings.library.label"));
    let labels: Vec<String> = LIBRARY_MODES.iter().map(|m| lang::t(&format!("settings.library.{m}"))).collect();
    s_select_col(ui, page, &labels, li, |ui, i| {
        settings::handle_set_library_thumbs(LIBRARY_MODES.get(i).copied().unwrap_or("auto"));
        ui.set_timeout(0, render_tab);
    });
    s_hint(ui, page, &library_hint(LIBRARY_MODES[li]));
    rule(ui, page);
    s_label(ui, page, &lang::t("settings.hiddenFolders.label"));
    s_hint(ui, page, &lang::t("settings.hiddenFolders.hint"));
    let folders = with(|s| s.folders.clone());
    if folders.is_empty() {
        s_hint(ui, page, &lang::t("settings.hiddenFolders.empty"));
    }
    for f in folders {
        let info = if f.missing {
            lang::t("settings.hiddenFolders.missing")
        } else {
            tw(if f.count == 1 { "settings.hiddenFolders.countOne" } else { "settings.hiddenFolders.countMany" }, &[("count", &f.count.to_string())])
        };
        let Some(r) = row(ui, page, 18.0) else { continue };
        let Ok(b) = ui.create("CheckBox", Some(r)) else { continue };
        let _ = ui.set_prop(b, "text", f.name.as_str());
        let _ = ui.set_prop(b, "box_size", 16.0);
        let _ = ui.set_prop(b, "style", "web");
        let _ = ui.set_prop(b, "checked", f.hidden);
        let w = text_width(ui, b, &f.name) + 24.0;
        if let Some(cb) = ui.control_mut(b) {
            cb.set_align(Align::Left).set_width(w).set_margins([0.0, 0.0, 8.0, 0.0]);
        }
        let name = f.name.clone();
        ui.on(b, "on_change", move |ui, sender, _| {
            let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
            btr::handle_set_folder_hidden(&name, on);
        });
        label(ui, r, &info, "s_label", Align::Client, 0.0, 18.0);
    }
}

// ---------------------------------------------------------------------------
// Vídeo
// ---------------------------------------------------------------------------

fn tab_video(ui: &mut Ui, page: ComponentId) {
    let codec = config::get_config_str("codec", "auto");
    let ci = CODECS.iter().position(|c| c.eq_ignore_ascii_case(&codec)).unwrap_or(0);
    s_label(ui, page, &lang::t("settings.codec.label"));
    if let Some(r) = row(ui, page, 32.0) {
        // O logo da placa de vídeo ao lado (o encoder de hardware é dela).
        let logo: Option<&'static [u8]> = match bridge::last_encoder_caps().map(|c| c.vendor) {
            Some(GpuVendor::Amd) => Some(include_bytes!("../assets/gpu/amd.png")),
            Some(GpuVendor::Nvidia) => Some(include_bytes!("../assets/gpu/nvidia.png")),
            Some(GpuVendor::Intel) => Some(include_bytes!("../assets/gpu/intel.png")),
            _ => None,
        };
        if let Some(bytes) = logo {
            if let Ok(v) = ui.create("Image", Some(r)) {
                let _ = ui.set_prop(v, "fit", "contain");
                if let Some(iv) = ui.get_mut::<ImageView>(v) {
                    iv.set_picture(Some(Image::encoded(bytes.to_vec())));
                }
                if let Some(cb) = ui.control_mut(v) {
                    cb.set_align(Align::Right).set_width(32.0).set_margins([8.0, 0.0, 0.0, 0.0]);
                }
            }
        }
        let opts = codec_options();
        let labels: Vec<String> = opts.iter().map(|(t, _)| t.clone()).collect();
        if let Some(b) = s_select(ui, r, &labels, ci, move |ui, i| {
            if !opts.get(i).is_some_and(|(_, ok)| *ok) {
                ui.set_timeout(0, render_tab);
                return;
            }
            settings::handle_set_codec(CODECS.get(i).copied().unwrap_or("auto"));
            ui.set_timeout(0, render_tab);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Client);
            }
        }
    }
    let dim = match codec.as_str() {
        "h264-hw" => 4096,
        "h264-sw" | "hevc-hw" | "av1-hw" | "av1-sw" => 8192,
        _ if bridge::last_encoder_caps().is_some_and(|c| c.h264_hw) => 4096,
        _ => 8192,
    };
    s_hint(ui, page, &tw("settings.codec.maxRes", &[("w", &dim.to_string()), ("h", &dim.to_string())]));
    if codec == "av1-sw" {
        s_warn(ui, page, &lang::t("settings.codec.swWarn"));
    }
    s_hint(ui, page, &lang::t("settings.codec.hint"));
    rule(ui, page);
    let (q, fps, kf, presets) = with(|s| (s.quality, s.fps, s.keyframe, s.fps_presets.clone()));
    s_label(ui, page, &lang::t("settings.quality.label"));
    gap(FIELD_GAP + 4.0);
    let ticks: Vec<String> = (0..=10).map(|n| n.to_string()).collect();
    s_slider(ui, page, 10, q.clamp(0, 10) as usize, &ticks, (&lang::t("settings.quality.lower"), &lang::t("settings.quality.higher")), |ui, v| {
        with(|s| s.quality = v as i32);
        set_slot(ui, "quality_hint", "text", lang::t(&format!("settings.quality.hint.{v}")));
        debounce(ui, "quality", 400, move |ui| {
            settings::handle_set_recording_quality(v as i32);
            render_tab(ui);
        });
    });
    let h = s_hint(ui, page, &lang::t(&format!("settings.quality.hint.{q}")));
    slot(ui, "quality_hint", h);
    if codec == "av1-sw" && q <= VT_SVT_FLOOR_LEVEL {
        s_warn(ui, page, &lang::t("settings.quality.svtFloor"));
    }
    rule(ui, page);
    tab_video_test(ui, page);
    rule(ui, page);
    let fi = fps_index(&presets, fps);
    s_label(ui, page, &lang::t("settings.fps.label"));
    gap(FIELD_GAP + 4.0);
    let ticks: Vec<String> = presets.iter().map(|p| p.to_string()).collect();
    let n = presets.len().max(1) - 1;
    s_slider(ui, page, n, fi, &ticks, (&lang::t("settings.fps.lessFluid"), &lang::t("settings.fps.moreFluid")), move |ui, v| {
        let f = presets.get(v).copied().unwrap_or(30);
        with(|s| s.fps = f);
        set_slot(ui, "fps_hint", "text", fps_hint(f));
        debounce(ui, "fps", 400, move |ui| {
            settings::handle_set_recording_fps(f);
            render_tab(ui);
        });
    });
    let fps_v = with(|s| s.fps_presets.get(fi).copied()).unwrap_or(fps);
    let h = s_hint(ui, page, &fps_hint(fps_v));
    slot(ui, "fps_hint", h);
    rule(ui, page);
    s_label(ui, page, &lang::t("settings.keyframe.label"));
    gap(FIELD_GAP + 4.0);
    let ticks: Vec<String> = (1..=10).map(|n| format!("{n}s")).collect();
    s_slider(ui, page, 9, (kf.clamp(1, 10) - 1) as usize, &ticks, (&lang::t("settings.keyframe.precise"), &lang::t("settings.keyframe.compact")), |ui, v| {
        let k = v as i32 + 1;
        with(|s| s.keyframe = k);
        set_slot(ui, "kf_hint", "text", keyframe_hint(k));
        debounce(ui, "keyframe", 400, move |ui| {
            settings::handle_set_recording_keyframe(k);
            render_tab(ui);
        });
    });
    let h = s_hint(ui, page, &keyframe_hint(kf));
    slot(ui, "kf_hint", h);
}

// ----- teste de qualidade -----

fn tab_video_test(ui: &mut Ui, page: ComponentId) {
    s_label(ui, page, &lang::t("settings.videoTest.label"));
    s_hint(ui, page, &lang::t("settings.videoTest.hint"));
    let (running, capturing, countdown, pct, level, result, a, b, zoom) =
        with(|s| (s.vt.running, s.vt.capturing, s.vt.countdown, s.vt.pct, s.vt.level, s.vt.result.clone(), s.vt.a, s.vt.b, s.vt.zoom));
    gap(FIELD_GAP + 2.0);
    if let Some(r) = button_row(ui, page) {
        let run_label = lang::t(if result.is_some() { "settings.videoTest.again" } else { "settings.videoTest.run" });
        if let Some(bt) = s_button(ui, r, &run_label, "", Align::Left, |ui| {
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
            let _ = ui.set_prop(bt, "enabled", !running);
        }
        if running {
            s_button(ui, r, &lang::t("common.cancel"), "", Align::Left, |_| video_test::handle_cancel_video_test());
        }
    }
    if running {
        let status = if capturing {
            tw("settings.videoTest.capturing", &[("sec", &countdown.to_string())])
        } else if level >= 0 {
            tw("settings.videoTest.encoding", &[("n", &level.to_string()), ("pct", &pct.to_string())])
        } else {
            lang::t("settings.videoTest.preparing")
        };
        let l = text_in(ui, page, &status, if capturing { "s_err" } else { "s_hint" }, col_w());
        slot(ui, "vt_status", l);
        if !capturing {
            if let Ok(p) = ui.create("ProgressBar", Some(page)) {
                let _ = ui.set_prop(p, "value", pct.clamp(0, 100) as f64);
                place(ui, p, 4.0);
                slot(ui, "vt_bar", Some(p));
            }
        }
        return;
    }
    let Some(r) = result else { return };
    gap(FIELD_GAP + 4.0);
    s_hint(
        ui,
        page,
        &tw(
            "settings.videoTest.meta",
            &[("w", &r.width.to_string()), ("h", &r.height.to_string()), ("fps", &(r.fps.round() as i64).to_string()), ("sec", &fmt_num(r.duration, 1)), ("encoder", &r.encoder)],
        ),
    );
    let mut levels = r.levels.clone();
    levels.sort_by_key(|l| -l.level);
    let max = levels.iter().map(|l| per_hour(l.bytes, r.duration)).fold(1.0f64, f64::max);
    let cur_h = levels.iter().find(|l| l.level == r.current_level).map(|l| per_hour(l.bytes, r.duration)).unwrap_or(0.0);
    gap(FIELD_GAP);
    for (k, l) in levels.iter().enumerate() {
        let h = per_hour(l.bytes, r.duration);
        let current = l.level == r.current_level;
        let rel = if current {
            lang::t("settings.videoTest.current")
        } else if h > 0.0 && cur_h > 0.0 {
            let f = h / cur_h;
            format!("×{}", fmt_num(f, if f < 1.0 { 2 } else { 1 }))
        } else {
            String::new()
        };
        let Ok(line) = ui.create("Panel", Some(page)) else { continue };
        let _ = ui.set_prop(line, "style", if l.level == a { "s_vt_a" } else if l.level == b { "s_vt_b" } else { "s_vt" });
        if k > 0 {
            gap(2.0);
        }
        place(ui, line, 26.0);
        if let Some(cb) = ui.control_mut(line) {
            cb.set_padding([9.0, 0.0, 9.0, 0.0]).set_cursor(CursorShape::Pointer);
        }
        let lv = l.level;
        ui.on(line, "on_pointer_up", move |ui, _, args| {
            if let EventArgs::Pointer(e) = args {
                with(|s| {
                    if e.button == PointerButton::Secondary {
                        s.vt.b = lv;
                    } else {
                        s.vt.a = lv;
                    }
                });
                render_tab(ui);
            }
        });
        label(ui, line, &tw("settings.videoTest.levelN", &[("n", &l.level.to_string())]), "s_q_on", Align::Left, 96.0, 26.0);
        label(ui, line, &rel, if current { "s_q_now12" } else { "s_q" }, Align::Right, 64.0, 26.0).map(|x| ui.set_prop(x, "h_align", "trailing"));
        let size = if h > 0.0 { tw("settings.videoTest.perHour", &[("size", &fmt_size(h))]) } else { "—".into() };
        if let Some(sz) = label(ui, line, &size, "s_q_on", Align::Right, 120.0, 26.0) {
            let _ = ui.set_prop(sz, "h_align", "trailing");
            if let Some(cb) = ui.control_mut(sz) {
                cb.set_margins([10.0, 0.0, 10.0, 0.0]);
            }
        }
        if let Ok(track) = ui.create("Panel", Some(line)) {
            let _ = ui.set_prop(track, "style", "s_bar");
            if let Some(cb) = ui.control_mut(track) {
                cb.set_align(Align::VertCenter).set_height(8.0).set_margins([10.0, 0.0, 0.0, 0.0]).set_hit_test(false).set_clip_children(true);
            }
            if let Ok(fill) = ui.create("Panel", Some(track)) {
                let _ = ui.set_prop(fill, "style", if current { "s_bar_on" } else { "s_bar_fill" });
                let frac = if h > 0.0 { (h / max).max(0.015) as f32 } else { 0.0 };
                if let Some(cb) = ui.control_mut(fill) {
                    cb.set_align(Align::Left).set_hit_test(false);
                }
                with(|s| s.vt_bars.push((fill, frac)));
            }
        }
    }
    gap(FIELD_GAP + 6.0);
    s_hint(ui, page, &lang::t("settings.videoTest.levelsHint"));
    // Trocar os lados | zoom.
    gap(16.0);
    if let Some(row) = row(ui, page, 31.0) {
        let small = |ui: &mut Ui, text: &str, align: Align, on: bool, f: Box<dyn Fn(&mut Ui)>| {
            if let Some(bt) = s_button(ui, row, text, "", align, move |ui| f(ui)) {
                let w = text_width(ui, bt, text) + 22.0;
                let _ = ui.set_prop(bt, "style", if on { "s_on" } else { "s" });
                let _ = ui.set_prop(bt, "font_size", 12.0);
                if let Some(cb) = ui.control_mut(bt) {
                    cb.set_width(w).set_height(31.0).set_margins(if align == Align::Right { [6.0, 0.0, 0.0, 0.0] } else { [0.0, 0.0, 0.0, 0.0] });
                }
            }
        };
        small(ui, &lang::t("settings.videoTest.swap"), Align::Left, false, Box::new(|ui| {
            with(|s| std::mem::swap(&mut s.vt.a, &mut s.vt.b));
            render_tab(ui);
        }));
        for (z, text) in [(3, "300%".to_owned()), (2, "200%".to_owned()), (1, "100%".to_owned()), (0, lang::t("settings.videoTest.zoomFit"))] {
            small(ui, &text, Align::Right, zoom == z, Box::new(move |ui| {
                with(|s| s.vt.zoom = z);
                render_tab(ui);
            }));
        }
    }
    gap(8.0);
    compare_view(ui, page, &r, a, b, zoom);
    gap(6.0);
    s_hint(ui, page, &lang::t("settings.videoTest.viewHint"));
    gap(FIELD_GAP + 8.0);
    if let Some(row) = button_row(ui, page) {
        if let Some(bt) = s_button(ui, row, &tw("settings.videoTest.use", &[("n", &a.to_string())]), "primary", Align::Left, vt_use) {
            let _ = ui.set_prop(bt, "enabled", a >= 0 && a != r.current_level);
        }
    }
    let bars = with(|s| std::mem::take(&mut s.vt_bars));
    ui.layout();
    for (fill, frac) in bars {
        let w = ui.parent(fill).and_then(|p| ui.bounds(p)).map_or(0.0, |b| b.width);
        if let Some(cb) = ui.control_mut(fill) {
            cb.set_width(w * frac);
        }
    }
}

/// O quadro de dois níveis lado a lado com a divisória (A à esquerda, B à direita).
fn compare_view(ui: &mut Ui, page: ComponentId, r: &VideoTestState, a: i32, b: i32, zoom: i32) {
    let load = |lv: i32| r.levels.iter().find(|l| l.level == lv).and_then(|l| std::fs::read(&l.image).ok());
    let (Some(img_a), img_b) = (load(a), load(b)) else { return };
    let scale = crate::app_player::window_scale().max(0.5);
    let (iw, ih) = (r.width.max(1) as f32 / scale, r.height.max(1) as f32 / scale);
    let w = col_w() - 2.0;
    let fit = (w / iw).min(1.0);
    let k = match zoom {
        1 => 1.0,
        2 => 2.0,
        3 => 3.0,
        _ => fit,
    };
    let h = (ih * fit).min(420.0);
    let Ok(stage) = ui.create("Panel", Some(page)) else { return };
    let _ = ui.set_prop(stage, "style", "s_stage");
    place(ui, stage, h + 2.0);
    if let Some(cb) = ui.control_mut(stage) {
        cb.set_clip_children(true);
    }
    let split = with(|s| s.vt.split);
    let (dw, dh) = (iw * k, ih * k);
    let pan = with(|s| s.vt.pan);
    let (ox, oy) = (1.0 + (w - dw) / 2.0 + pan.0, 1.0 + (h - dh) / 2.0 + pan.1);
    let put = |ui: &mut Ui, parent: ComponentId, bytes: Vec<u8>, x: f32| {
        if let Ok(v) = ui.create("Image", Some(parent)) {
            let _ = ui.set_prop(v, "fit", "stretch");
            if let Some(iv) = ui.get_mut::<ImageView>(v) {
                iv.set_picture(Some(Image::encoded(bytes)));
            }
            if let Some(cb) = ui.control_mut(v) {
                cb.set_rect(x, oy, dw, dh).set_hit_test(false);
            }
        }
    };
    put(ui, stage, img_a, ox);
    let cut = 1.0 + w * split;
    if let Some(bytes) = img_b {
        if let Ok(right) = ui.create("Layout", Some(stage)) {
            if let Some(cb) = ui.control_mut(right) {
                cb.set_rect(cut, 1.0, w - (cut - 1.0), h).set_clip_children(true).set_hit_test(false);
            }
            put(ui, right, bytes, ox - cut);
        }
    }
    // A divisória e a alça.
    if let Ok(line) = ui.create("Rectangle", Some(stage)) {
        let _ = ui.set_prop(line, "fill", Color::WHITE);
        let _ = ui.set_prop(line, "stroke_width", 0.0);
        if let Some(cb) = ui.control_mut(line) {
            cb.set_rect(cut - 1.0, 1.0, 2.0, h).set_hit_test(false);
        }
    }
    if let Ok(knob) = ui.create("Circle", Some(stage)) {
        let _ = ui.set_prop(knob, "fill", Color::WHITE);
        let _ = ui.set_prop(knob, "stroke_width", 0.0);
        if let Some(cb) = ui.control_mut(knob) {
            cb.set_rect(cut - 13.0, 1.0 + h / 2.0 - 13.0, 26.0, 26.0).set_hit_test(false);
        }
    }
    if let Some(l) = label(ui, stage, "‹›", "s_knob", Align::None, 0.0, 26.0) {
        let _ = ui.set_prop(l, "h_align", "center");
        if let Some(cb) = ui.control_mut(l) {
            cb.set_rect(cut - 13.0, 1.0 + h / 2.0 - 13.0, 26.0, 26.0);
        }
    }
    // As etiquetas dos lados.
    for (side, lv) in [(0, a), (1, b)] {
        let text = tw("settings.videoTest.levelN", &[("n", &lv.to_string())]);
        let Ok(tag) = ui.create("Panel", Some(stage)) else { continue };
        let _ = ui.set_prop(tag, "style", "s_tag");
        let Some(l) = label(ui, tag, &text, "s_tag_text", Align::Client, 0.0, 24.0) else { continue };
        let tw_ = text_width(ui, l, &text) + 6.0 + 6.0 + 3.0 + 4.0;
        if let Some(cb) = ui.control_mut(l) {
            cb.set_margins([9.0, 0.0, 0.0, 0.0]);
        }
        if let Ok(bar) = ui.create("Rectangle", Some(tag)) {
            let _ = ui.set_prop(bar, "fill", if side == 1 { "blue" } else { "green" });
            let _ = ui.set_prop(bar, "stroke_width", 0.0);
            if let Some(cb) = ui.control_mut(bar) {
                cb.set_rect(0.0, 0.0, 3.0, 24.0).set_hit_test(false);
            }
        }
        let x = if side == 0 { 9.0 } else { w - tw_ - 8.0 };
        if let Some(cb) = ui.control_mut(tag) {
            cb.set_rect(x, 9.0, tw_, 24.0).set_hit_test(false);
        }
    }
    // Arrastar move a divisória; com zoom, desloca a imagem.
    ui.on(stage, "on_pointer_down", move |ui, _, args| {
        if let EventArgs::Pointer(e) = args {
            with(|s| s.vt.drag = Some((e.position.x, e.position.y, s.vt.pan, (e.position.x - cut).abs() < 16.0 || s.vt.zoom == 0)));
            let _ = ui;
        }
    });
    ui.on(stage, "on_pointer_move", move |ui, _, args| {
        let EventArgs::Pointer(e) = args else { return };
        let Some((x0, y0, pan0, splitting)) = with(|s| s.vt.drag) else { return };
        with(|s| {
            if splitting {
                s.vt.split = ((e.position.x - 1.0) / w).clamp(0.0, 1.0);
            } else {
                s.vt.pan = (pan0.0 + e.position.x - x0, pan0.1 + e.position.y - y0);
            }
        });
        debounce(ui, "vt_drag", 16, render_tab);
    });
    ui.on(stage, "on_pointer_up", |_, _, _| with(|s| s.vt.drag = None));
}

// ---------------------------------------------------------------------------
// Áudio
// ---------------------------------------------------------------------------

fn tab_audio(ui: &mut Ui, page: ComponentId) {
    let ab = encoder::audio_bitrate_kbps();
    let ai = AUDIO_BITRATES.iter().position(|k| *k == ab).unwrap_or(3);
    s_label(ui, page, &lang::t("settings.audioBitrate.label"));
    let labels: Vec<String> = AUDIO_BITRATES
        .iter()
        .map(|k| {
            let key = if *k == AUDIO_BITRATE_DEFAULT { "settings.audioBitrate.optionDefault" } else { "settings.audioBitrate.option" };
            tw(key, &[("kbps", &k.to_string())])
        })
        .collect();
    s_select_col(ui, page, &labels, ai, |ui, i| {
        settings::handle_set_audio_bitrate(AUDIO_BITRATES.get(i).copied().unwrap_or(AUDIO_BITRATE_DEFAULT));
        ui.set_timeout(0, render_tab);
    });
    s_hint(ui, page, &audio_hint(AUDIO_BITRATES[ai]));
    rule(ui, page);
    s_check(ui, page, config::get_config_bool("muteWhenDeviceMuted", true), &lang::t("settings.muteWhenDeviceMuted.label"), |ui, on| check_changed(ui, "muteWhenDeviceMuted", on));
    s_hint(ui, page, &lang::t("settings.muteWhenDeviceMuted.hint"));
}

// ---------------------------------------------------------------------------
// Buffer
// ---------------------------------------------------------------------------

fn tab_replay(ui: &mut Ui, page: ComponentId) {
    s_label(ui, page, &lang::t("settings.replay.label"));
    text_in_md(ui, page, &lang::t("settings.replay.hint"), "s_hint", col_w());
    let Some(r) = with(|s| s.replay.clone()) else { return };
    rule(ui, page);
    s_check(ui, page, r.auto_start, &lang::t("settings.replay.autoStart"), |ui, on| check_changed(ui, "replayAutoStart", on));
    s_hint(ui, page, &lang::t("settings.replay.autoStartHint"));
    rule(ui, page);
    s_check(ui, page, r.into_recording, &lang::t("settings.replay.intoRecording"), |ui, on| check_changed(ui, "replayIntoRecording", on));
    s_hint(ui, page, &lang::t("settings.replay.intoRecordingHint"));
    rule(ui, page);
    let limit = if r.max_mb_limit > 0 { r.max_mb_limit } else { REPLAY_DEFAULT_MB };
    let (secs, si) = with_current(REPLAY_SEC_OPTIONS.to_vec(), if r.max_sec > 0 { r.max_sec } else { REPLAY_DEFAULT_SEC });
    let mut mbs: Vec<i64> = REPLAY_MB_OPTIONS.iter().copied().filter(|v| *v < limit).collect();
    mbs.push(limit);
    let (mbs, mi) = with_current(mbs, if r.max_mb > 0 { r.max_mb } else { REPLAY_DEFAULT_MB });
    let sec_now = std::rc::Rc::new(std::cell::Cell::new(secs[si]));
    let mb_now = std::rc::Rc::new(std::cell::Cell::new(mbs[mi]));
    // Duas colunas: tempo e memória.
    if let Some(heads) = row(ui, page, 17.0) {
        let half = (col_w() - 10.0) / 2.0;
        label(ui, heads, &lang::t("settings.replay.duration"), "s_label", Align::Left, half + 10.0, 17.0);
        label(ui, heads, &lang::t("settings.replay.memory"), "s_label", Align::Client, 0.0, 17.0);
    }
    if let Some(sel) = row(ui, page, 32.0) {
        let half = (col_w() - 10.0) / 2.0;
        let labels: Vec<String> = secs.iter().map(|v| sec_label(*v)).collect();
        let (s2, m2) = (sec_now.clone(), mb_now.clone());
        if let Some(b) = s_select(ui, sel, &labels, si, move |_, i| {
            s2.set(secs.get(i).copied().unwrap_or(REPLAY_DEFAULT_SEC));
            replay::handle_set_replay_limits(s2.get() as i32, m2.get() as i32);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Left).set_width(half).set_margins([0.0, 0.0, 10.0, 0.0]);
            }
        }
        let labels: Vec<String> = mbs.iter().map(|v| if *v >= limit { tw("settings.replay.memoryMax", &[("size", &mb_label(*v))]) } else { mb_label(*v) }).collect();
        let (s2, m2) = (sec_now.clone(), mb_now.clone());
        if let Some(b) = s_select(ui, sel, &labels, mi, move |_, i| {
            m2.set(mbs.get(i).copied().unwrap_or(REPLAY_DEFAULT_MB));
            replay::handle_set_replay_limits(s2.get() as i32, m2.get() as i32);
        }) {
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Client);
            }
        }
    }
    s_hint(ui, page, &lang::t("settings.replay.memoryHint"));
    rule(ui, page);
    s_label(ui, page, &lang::t("settings.replay.apps"));
    s_input_col(
        ui,
        page,
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
    s_hint(ui, page, &lang::t("settings.replay.appsHint"));
    rule(ui, page);
    s_check(ui, page, r.indicator, &lang::t("settings.replay.indicator"), |ui, on| check_changed(ui, "replayIndicator", on));
    text_in_md(ui, page, &lang::t("settings.replay.indicatorHint"), "s_hint", col_w());
    if r.indicator {
        let op = if r.indicator_opacity > 0 { r.indicator_opacity as i32 } else { 90 };
        indicator_options(
            ui,
            page,
            true,
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

/// `.hotkey-mod`: caixa + nome, verde quando marcado.
fn mod_toggle(ui: &mut Ui, parent: ComponentId, text: &str, on: bool, f: impl Fn(&mut Ui, bool) + 'static) {
    let Ok(p) = ui.create("Panel", Some(parent)) else { return };
    let _ = ui.set_prop(p, "style", if on { "s_mod_on" } else { "s_mod" });
    let Ok(b) = ui.create("CheckBox", Some(p)) else { return };
    let _ = ui.set_prop(b, "style", "mod");
    let _ = ui.set_prop(b, "box_size", 13.0);
    let _ = ui.set_prop(b, "text", text);
    let _ = ui.set_prop(b, "checked", on);
    let w = text_width(ui, b, text) + 13.0 + 5.0 + 22.0;
    if let Some(cb) = ui.control_mut(b) {
        cb.set_align(Align::Client).set_margins([11.0, 0.0, 6.0, 0.0]);
    }
    if let Some(cb) = ui.control_mut(p) {
        cb.set_align(Align::Left).set_width(w).set_margins([0.0, 1.5, 6.0, 1.5]);
    }
    ui.on(b, "on_change", move |ui, sender, _| {
        let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
        f(ui, on);
    });
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
    s_hint(ui, page, &lang::t("hotkeys.hint"));
    if items.is_empty() {
        gap(FIELD_GAP + 6.0);
        s_hint(ui, page, &lang::t("hotkeys.empty"));
    }
    for (i, it) in items.iter().enumerate() {
        let (mut ctrl, mut shift, mut alt, mut win, key) = split_spec(&it.hotkey);
        if let Some(p) = pending.get(&i) {
            (ctrl, shift, alt, win) = *p;
        }
        let ki = if key.is_empty() { 0 } else { key_values.iter().position(|k| k.eq_ignore_ascii_case(&key)).unwrap_or(0) };
        let Ok(card) = ui.create("Panel", Some(page)) else { continue };
        let _ = ui.set_prop(card, "style", "s_box");
        if let Some(cb) = ui.control_mut(card) {
            cb.set_padding([12.0, 10.0, 12.0, 10.0]);
        }
        let mut card_h = 20.0;
        let mut line = |ui: &mut Ui, h: f32| -> Option<ComponentId> {
            let l = ui.create("Layout", Some(card)).ok()?;
            if let Some(cb) = ui.control_mut(l) {
                cb.set_align(Align::Top).set_height(h).set_margins([0.0, if card_h > 20.0 { 10.0 } else { 0.0 }, 0.0, 0.0]);
            }
            card_h += h + if card_h > 20.0 { 10.0 } else { 0.0 };
            Some(l)
        };
        // Modo, modificadores, tecla, excluir.
        if let Some(l1) = line(ui, 32.0) {
            if let Some(m) = s_select(ui, l1, &mode_labels, HK_MODES.iter().position(|m| *m == it.mode).unwrap_or(0), move |_, mi| {
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
                    cb.set_align(Align::Left).set_width(170.0).set_margins([0.0, 0.0, 10.0, 0.0]);
                }
            }
            let mods = std::rc::Rc::new(std::cell::Cell::new((ctrl, shift, alt, win)));
            let key_now = std::rc::Rc::new(RefCell::new(key.clone()));
            for (n, name) in ["settings.hotkey.ctrl", "settings.hotkey.shift", "settings.hotkey.alt", "settings.hotkey.win"].iter().enumerate() {
                let (mods, key_now) = (mods.clone(), key_now.clone());
                mod_toggle(ui, l1, &lang::t(name), [ctrl, shift, alt, win][n], move |ui, on| {
                    let mut m = mods.get();
                    match n {
                        0 => m.0 = on,
                        1 => m.1 = on,
                        2 => m.2 = on,
                        _ => m.3 = on,
                    }
                    mods.set(m);
                    hk_combo(ui, i, m, key_now.borrow().clone());
                    ui.set_timeout(0, render_tab);
                });
            }
            if let Some(x) = s_button(ui, l1, "×", "", Align::Right, move |_| {
                let mut items = with(|s| s.hk_items.clone());
                if i < items.len() {
                    items.remove(i);
                    hk_send(items);
                }
            }) {
                let _ = ui.set_prop(x, "style", "s_x");
                let _ = ui.set_prop(x, "font_size", 18.0);
                let _ = ui.set_prop(x, "hint", lang::t("hotkeys.remove"));
                if let Some(cb) = ui.control_mut(x) {
                    cb.set_width(28.0).set_height(28.0).set_margins([10.0, 2.0, 0.0, 2.0]);
                }
            }
            if let Some(plus) = label(ui, l1, "+", "s_plus", Align::Left, 0.0, 32.0) {
                if let Some(cb) = ui.control_mut(plus) {
                    cb.set_width(10.0).set_margins([4.0, 0.0, 10.0, 0.0]);
                }
            }
            let kv = key_values.clone();
            if let Some(k) = s_select(ui, l1, &key_labels, ki, move |ui, ki| {
                let key = kv.get(ki).cloned().unwrap_or_default();
                *key_now.borrow_mut() = key.clone();
                hk_combo(ui, i, mods.get(), key);
            }) {
                if let Some(cb) = ui.control_mut(k) {
                    cb.set_align(Align::Client);
                }
            }
        }
        // Janela: o título (e o som do app); área: a área salva.
        if it.mode == Mode::Window {
            if let Some(l2) = line(ui, 32.0) {
                label(ui, l2, &lang::t("hotkeys.titleLabel"), "s_name2", Align::Left, 150.0, 32.0);
                let cur = it.title.clone();
                if let Some(e) = s_input(ui, l2, &it.title, &lang::t("hotkeys.titlePlaceholder"), |_, _| {}, move |_, t| {
                    let t = t.trim().to_owned();
                    if t != cur {
                        hk_update(i, |it| it.title = t);
                    }
                }) {
                    if let Some(cb) = ui.control_mut(e) {
                        cb.set_align(Align::Left).set_width(360.0);
                    }
                }
            }
            if let Some(l3) = line(ui, 17.0) {
                if let Ok(a) = ui.create("CheckBox", Some(l3)) {
                    let _ = ui.set_prop(a, "box_size", 16.0);
        let _ = ui.set_prop(a, "style", "web");
                    let _ = ui.set_prop(a, "text", lang::t("windowPicker.appAudio"));
                    let _ = ui.set_prop(a, "checked", it.app_audio);
                    if let Some(cb) = ui.control_mut(a) {
                        cb.set_align(Align::Client);
                    }
                    ui.on(a, "on_change", move |ui, sender, _| {
                        let on = matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)));
                        hk_update(i, |it| it.app_audio = on);
                    });
                }
            }
        }
        if it.mode == Mode::Area {
            if let Some(l2) = line(ui, 35.0) {
                if it.area.is_some() {
                    if let Some(b) = s_button(ui, l2, &lang::t("hotkeys.areaClear"), "", Align::Right, move |_| hk_update(i, |it| it.area = None)) {
                        if let Some(cb) = ui.control_mut(b) {
                            cb.set_margins([8.0, 0.0, 0.0, 0.0]);
                        }
                    }
                }
                let define = lang::t(if defining == Some(i) {
                    "hotkeys.areaChoosing"
                } else if it.area.is_some() {
                    "hotkeys.areaRedefine"
                } else {
                    "hotkeys.areaDefine"
                });
                if let Some(b) = s_button(ui, l2, &define, "", Align::Right, move |_| bhk::handle_define_area(i)) {
                    let _ = ui.set_prop(b, "enabled", defining.is_none());
                    if let Some(cb) = ui.control_mut(b) {
                        cb.set_margins([8.0, 0.0, 0.0, 0.0]);
                    }
                }
                let area_text = match it.area {
                    Some(r) => tw("hotkeys.areaValue", &[("w", &r.w.to_string()), ("h", &r.h.to_string()), ("x", &r.x.to_string()), ("y", &r.y.to_string())]),
                    None => lang::t("hotkeys.areaAsk"),
                };
                label(ui, l2, &area_text, if it.area.is_some() { "s_about_v" } else { "s_inline3" }, Align::Client, 0.0, 35.0);
            }
        }
        if it.mode != Mode::Replay {
            if let Some(l4) = line(ui, 32.0) {
                label(ui, l4, &lang::t("hotkeys.delayLabel"), "s_name2", Align::Left, 0.0, 32.0).map(|l| {
                    let w = text_width(ui, l, &lang::t("hotkeys.delayLabel")).max(140.0);
                    if let Some(cb) = ui.control_mut(l) {
                        cb.set_width(w).set_margins([0.0, 0.0, 10.0, 0.0]);
                    }
                });
                let dl = delays.clone();
                if let Some(d) = s_select(ui, l4, &delay_labels, delays.iter().position(|d| *d == it.delay_sec).unwrap_or(0), move |_, di| {
                    let v = dl.get(di).copied().unwrap_or(0);
                    hk_update(i, |it| it.delay_sec = v);
                }) {
                    if let Some(cb) = ui.control_mut(d) {
                        cb.set_align(Align::Left).set_width(160.0);
                    }
                }
            }
        }
        let st_key = status.get(i).copied().unwrap_or("");
        if !st_key.is_empty() {
            if let Some(l5) = line(ui, 16.0) {
                label(ui, l5, &lang::t(&format!("hotkeys.status.{st_key}")), if st_key == "inactive" { "s_hint" } else { "s_err" }, Align::Client, 0.0, 16.0);
            }
        }
        gap(if i == 0 { FIELD_GAP + 6.0 } else { TAB_GAP });
        place(ui, card, card_h);
    }
    gap(FIELD_GAP + 6.0);
    if let Some(r) = button_row(ui, page) {
        s_button(ui, r, &lang::t("hotkeys.add"), "", Align::Left, |_| {
            let mut items = with(|s| s.hk_items.clone());
            items.push(Item::new(Mode::Record, ""));
            hk_send(items);
        });
    }
}

// ---------------------------------------------------------------------------
// Dispositivos
// ---------------------------------------------------------------------------

/// Os grupos de dispositivos (`.devices-group`): título em maiúsculas com a linha e
/// as linhas com a caixa; `pick` diz se entra e se está marcado.
fn device_groups(ui: &mut Ui, page: ComponentId, pick: impl Fn(&Device) -> Option<bool>, toggle: impl Fn(&mut Ui, &str, bool) + Clone + 'static, empty: &str) {
    let (mons, cams, mics, spks) = with(|s| (s.monitors.clone(), s.webcams.clone(), s.mics.clone(), s.speakers.clone()));
    let mut gi = 0;
    for (key, mut list) in [("monitors", mons), ("webcams", cams), ("mics", mics), ("speakers", spks)] {
        list.sort_by_key(|d| natural_key(&d.name));
        let rows: Vec<(Device, bool)> = list.into_iter().filter_map(|d| pick(&d).map(|on| (d, on))).collect();
        if rows.is_empty() {
            continue;
        }
        gap(if gi == 0 { FIELD_GAP + 4.0 } else { 14.0 });
        gi += 1;
        if let Some(t) = text_in(ui, page, &lang::t(&format!("devices.groups.{key}")).to_uppercase(), "s_group", col_w()) {
            let _ = ui.set_prop(t, "wrap", false);
            let _ = ui.set_prop(t, "letter_spacing", 0.4);
        }
        gap(6.0);
        if let Ok(line) = ui.create("Rectangle", Some(page)) {
            let _ = ui.set_prop(line, "style", "sep");
            place(ui, line, 1.0);
        }
        gap(4.0);
        for (d, on) in rows {
            let name = if d.name.is_empty() { d.id.clone() } else { d.name.clone() };
            let Ok(r) = ui.create("Panel", Some(page)) else { continue };
            let _ = ui.set_prop(r, "style", "s_dev");
            place(ui, r, 31.0);
            gap(0.0);
            if let Some(cb) = ui.control_mut(r) {
                cb.set_padding([4.0, 0.0, 4.0, 0.0]).set_cursor(CursorShape::Pointer);
            }
            if let Ok(b) = ui.create("CheckBox", Some(r)) {
                let _ = ui.set_prop(b, "checked", on);
                let _ = ui.set_prop(b, "box_size", 16.0);
        let _ = ui.set_prop(b, "style", "web");
                let _ = ui.set_prop(b, "text", "");
                if let Some(cb) = ui.control_mut(b) {
                    cb.set_align(Align::Left).set_width(16.0).set_margins([0.0, 0.0, 8.0, 0.0]).set_hit_test(false);
                }
            }
            if !d.info.is_empty() {
                if let Some(l) = label(ui, r, &d.info, "s_info", Align::Right, 0.0, 31.0) {
                    let w = text_width(ui, l, &d.info) + 2.0;
                    if let Some(cb) = ui.control_mut(l) {
                        cb.set_width(w).set_margins([8.0, 0.0, 0.0, 0.0]);
                    }
                }
            }
            label(ui, r, &name, "s_name", Align::Client, 0.0, 31.0);
            let id = d.id.clone();
            let t = toggle.clone();
            ui.on(r, "on_click", move |ui, _, _| {
                t(ui, &id, !on);
                ui.set_timeout(0, render_tab);
            });
        }
    }
    if gi == 0 && !empty.is_empty() {
        s_hint(ui, page, empty);
    }
}

fn tab_devices(ui: &mut Ui, page: ComponentId) {
    s_hint(ui, page, &lang::t("devices.hint"));
    device_groups(ui, page, |d| Some(!d.hidden), |_, id, on| {
        set_device_flag(id, |d| d.hidden = !on);
        crate::bridge::devices::handle_set_source_hidden(id, !on);
    }, &lang::t("devices.empty"));
}

// ---------------------------------------------------------------------------
// Transcrição
// ---------------------------------------------------------------------------

/// A barra fina (`.transcribe-bar`): trilho `border_2`, verde.
fn thin_bar(ui: &mut Ui, page: ComponentId, value: f32, h: f32) -> Option<ComponentId> {
    let p = ui.create("ProgressBar", Some(page)).ok()?;
    let _ = ui.set_prop(p, "value", (value.clamp(0.0, 1.0) * 100.0) as f64);
    place(ui, p, h);
    Some(p)
}

fn tab_transcribe(ui: &mut Ui, page: ComponentId) {
    let (status, detail, btn, danger, progress) = asr_texts();
    let kind = with(|s| s.asr.as_ref().map(|a| a.status.clone())).unwrap_or_default();
    s_label(ui, page, &lang::t("settings.transcribe.local.label"));
    s_hint(ui, page, &lang::t("settings.transcribe.local.hint"));
    // O quadro do motor local.
    gap(FIELD_GAP + 4.0);
    if let Ok(frame) = ui.create("Panel", Some(page)) {
        let _ = ui.set_prop(frame, "style", "s_frame");
        if let Some(cb) = ui.control_mut(frame) {
            cb.set_padding([12.0, 10.0, 12.0, 10.0]);
        }
        let inner = col_w() - 24.0;
        let mut h = 20.0;
        let mut add = |ui: &mut Ui, id: ComponentId, hh: f32, top: f32| {
            if let Some(cb) = ui.control_mut(id) {
                cb.set_align(Align::Top).set_height(hh).set_margins([0.0, top, 0.0, 0.0]);
            }
            h += hh + top;
        };
        let style = match kind.as_str() {
            "ready" => "s_status_ok",
            "error" => "s_status_fail",
            _ => "s_status",
        };
        if let Some(l) = label(ui, frame, &status, style, Align::Top, 0.0, 0.0) {
            let _ = ui.set_prop(l, "wrap", true);
            let hh = wrapped_height(ui, l, &status, inner);
            add(ui, l, hh, 0.0);
        }
        if progress >= 0.0 {
            if let Ok(p) = ui.create("ProgressBar", Some(frame)) {
                let _ = ui.set_prop(p, "value", (progress * 100.0) as f64);
                add(ui, p, 6.0, 8.0);
            }
        }
        if let Some(l) = label(ui, frame, &detail, "s_detail", Align::Top, 0.0, 0.0) {
            let _ = ui.set_prop(l, "wrap", true);
            let _ = ui.set_prop(l, "v_align", "leading");
            let hh = wrapped_height(ui, l, &detail, inner) * 1.07;
            add(ui, l, hh, if progress >= 0.0 { 6.0 } else { 4.0 });
        }
        if let Ok(r) = ui.create("Layout", Some(frame)) {
            add(ui, r, 35.0, 6.0);
            s_button(ui, r, &btn, if danger { "danger" } else { "" }, Align::Left, asr_action);
        }
        place(ui, frame, h + 6.0);
    }
    rule(ui, page);
    let (vals, labels, li) = transcribe_langs();
    s_label(ui, page, &lang::t("settings.transcribe.language.label"));
    s_select_col(ui, page, &labels, li, move |_, i| btr::handle_set_transcribe_language(vals.get(i).map_or("app", |s| s.as_str())));
    s_hint(ui, page, &lang::t("settings.transcribe.language.hint"));
    rule(ui, page);
    s_check(ui, page, config::get_config_bool("transcribeOnStop", true), &lang::t("settings.transcribe.onStop.label"), |ui, on| check_changed(ui, "transcribeOnStop", on));
    s_hint(ui, page, &lang::t("settings.transcribe.onStop.hint"));
    rule(ui, page);
    s_label(ui, page, &lang::t("settings.transcribe.queue"));
    let (t, pending, cloud, queue, clicked) = with(|s| (s.tr.clone(), s.tr_pending, s.tr_cloud, s.tr_queue.clone(), s.tr_cancel_clicked));
    let canceling = t.canceling || clicked;
    if let Some(r) = button_row(ui, page) {
        let pending_label = if pending > 0 { tw("settings.transcribe.pendingN", &[("count", &pending.to_string())]) } else { lang::t("settings.transcribe.pendingNone") };
        if let Some(b) = s_button(ui, r, &pending_label, "", Align::Left, |_| btr::handle_transcribe_pending()) {
            let _ = ui.set_prop(b, "enabled", pending > 0);
        }
        let cancel_label = lang::t(if canceling { "settings.transcribe.canceling" } else { "common.cancel" });
        if let Some(b) = s_button(ui, r, &cancel_label, "", Align::Left, |ui| {
            if with(|s| s.tr.canceling || s.tr_cancel_clicked) {
                return;
            }
            with(|s| s.tr_cancel_clicked = true);
            render_tab(ui);
            btr::handle_cancel_transcribe();
        }) {
            let _ = ui.set_prop(b, "enabled", !canceling && (t.running || t.queue > 0));
        }
    }
    let (visible, p, line) = transcribe_line();
    if visible {
        gap(FIELD_GAP + 4.0);
        let bar = thin_bar(ui, page, p, 6.0);
        slot(ui, "tr_bar", bar);
        let l = text_in(ui, page, &line, "s_hint", col_w());
        slot(ui, "tr_line", l);
    }
    if !t.last_error.is_empty() {
        let err = if t.last_error_name.is_empty() {
            tw("settings.transcribe.error", &[("error", &t.last_error)])
        } else {
            tw("settings.transcribe.errorNamed", &[("name", &t.last_error_name), ("error", &t.last_error)])
        };
        text_in(ui, page, &err, "s_err", col_w());
    }
    if !queue.is_empty() {
        gap(FIELD_GAP + 4.0);
        transcribe_queue(ui, page, &queue);
    }
    if cloud > 0 {
        s_hint(ui, page, &tw("settings.transcribe.pendingCloud", &[("count", &cloud.to_string())]));
    }
    s_hint(ui, page, &lang::t("settings.transcribe.queueHint"));
}

/// A fila (`.tq-item`): a em curso no alto; as que esperam se arrastam para reordenar.
fn transcribe_queue(ui: &mut Ui, page: ComponentId, queue: &[QueueItem]) {
    let Ok(frame) = ui.create("Panel", Some(page)) else { return };
    let _ = ui.set_prop(frame, "style", "s_frame");
    if let Some(cb) = ui.control_mut(frame) {
        cb.set_padding([1.0, 1.0, 1.0, 1.0]).set_clip_children(true);
    }
    let mut waiting = 0i32;
    for (i, q) in queue.iter().enumerate() {
        let Ok(r) = ui.create("Panel", Some(frame)) else { continue };
        let _ = ui.set_prop(r, "style", if q.current { "s_q_cur" } else { "s_q_row" });
        if let Some(cb) = ui.control_mut(r) {
            cb.set_align(Align::Top).set_height(30.0).set_padding([8.0, 0.0, 8.0, 0.0]);
        }
        if i > 0 {
            if let Ok(line) = ui.create("Rectangle", Some(r)) {
                let _ = ui.set_prop(line, "style", "edge2");
                if let Some(cb) = ui.control_mut(line) {
                    cb.set_align(Align::MostTop).set_height(1.0).set_margins([-8.0, 0.0, -8.0, 0.0]).set_hit_test(false);
                }
            }
        }
        if let Some(l) = label(ui, r, if q.current { "●" } else { "⠿" }, if q.current { "s_q_dot" } else { "s_q_dur" }, Align::Left, 12.0, 30.0) {
            let _ = ui.set_prop(l, "h_align", "center");
            if let Some(cb) = ui.control_mut(l) {
                cb.set_margins([0.0, 0.0, 8.0, 0.0]);
            }
        }
        if q.current {
            if let Some(l) = label(ui, r, &lang::t("settings.transcribe.queueNow").to_uppercase(), "s_q_now", Align::Right, 0.0, 30.0) {
                let _ = ui.set_prop(l, "letter_spacing", 0.4);
                let w = text_width(ui, l, &lang::t("settings.transcribe.queueNow").to_uppercase()) + 4.0;
                if let Some(cb) = ui.control_mut(l) {
                    cb.set_width(w).set_margins([8.0, 0.0, 0.0, 0.0]);
                }
            }
        } else {
            let id = q.id.clone();
            if let Some(x) = s_button(ui, r, "×", "", Align::Right, move |_| btr::handle_remove_transcribe_item(&id)) {
                let _ = ui.set_prop(x, "style", "s_x");
                let _ = ui.set_prop(x, "font_size", 15.0);
                let _ = ui.set_prop(x, "hint", lang::t("settings.transcribe.queueRemove"));
                if let Some(cb) = ui.control_mut(x) {
                    cb.set_width(20.0).set_height(20.0).set_margins([8.0, 5.0, 0.0, 5.0]);
                }
            }
        }
        let dur = if q.duration > 0 { fmt_dur(q.duration as i64) } else { "—".into() };
        if let Some(l) = label(ui, r, &dur, "s_q_dur", Align::Right, 0.0, 30.0) {
            let w = text_width(ui, l, &dur) + 2.0;
            if let Some(cb) = ui.control_mut(l) {
                cb.set_width(w).set_margins([8.0, 0.0, 0.0, 0.0]);
            }
        }
        label(ui, r, &q.name, if q.current { "s_q_on" } else { "s_q" }, Align::Client, 0.0, 30.0);
        // Arrastar para reordenar a espera (o iron entrega o soltar na linha de destino).
        if !q.current {
            let at = waiting;
            let id = q.id.clone();
            if let Some(cb) = ui.control_mut(r) {
                cb.set_draggable(true).set_accepts_drops(true).set_cursor(CursorShape::Move);
            }
            let name = q.name.clone();
            ui.on(r, "on_drag_start", move |ui, _, _| ui.begin_drag(id.clone(), name.clone()));
            ui.on(r, "on_drag_drop", move |_, _, args| {
                if let EventArgs::Text(id) = args {
                    btr::handle_move_transcribe_item(id, at);
                }
            });
            waiting += 1;
        }
    }
    place(ui, frame, queue.len() as f32 * 30.0 + 2.0);
}

// ---------------------------------------------------------------------------
// Atualizações e Sobre
// ---------------------------------------------------------------------------

fn tab_updates(ui: &mut Ui, page: ComponentId) {
    s_label(ui, page, &lang::t("settings.updates.versionLabel"));
    s_hint(ui, page, &crate::version::display_version());
    rule(ui, page);
    s_check(ui, page, config::get_config_bool("checkUpdates", true), &lang::t("settings.updates.autoLabel"), |ui, on| check_changed(ui, "checkUpdates", on));
    s_hint(ui, page, &lang::t("settings.updates.autoHint"));
    rule(ui, page);
    let checking = with(|s| s.checking);
    if let Some(r) = button_row(ui, page) {
        if let Some(b) = s_button(ui, r, &lang::t("settings.updates.checkNow"), "", Align::Left, |ui| {
            with(|s| s.checking = true);
            render_tab(ui);
            settings::handle_check_updates();
        }) {
            let _ = ui.set_prop(b, "enabled", !checking);
        }
    }
    let (status, has_url) = update_status();
    if !status.is_empty() {
        let available = with(|s| s.update.as_ref().is_some_and(|u| u.ok && u.has_update));
        if let Some(r) = row(ui, page, 16.0) {
            if let Some(l) = label(ui, r, &status, if available { "s_ok" } else { "s_hint" }, Align::Left, 0.0, 16.0) {
                let w = text_width(ui, l, &status) + 4.0;
                if let Some(cb) = ui.control_mut(l) {
                    cb.set_width(w);
                }
            }
            if has_url {
                if let Some(l) = label(ui, r, &lang::t("settings.updates.openPage"), "s_ok", Align::Client, 0.0, 16.0) {
                    let _ = ui.set_prop(l, "hit_test", true);
                    if let Some(cb) = ui.control_mut(l) {
                        cb.set_cursor(CursorShape::Pointer);
                    }
                    ui.on(l, "on_click", |_, _, _| {
                        let url = with(|s| s.update.as_ref().map(|u| u.url.clone())).unwrap_or_default();
                        if !url.is_empty() {
                            bridge::handle_open_url(&url);
                        }
                    });
                }
            }
        }
    }
}

/// Uma linha "rótulo — valor" do Sobre.
fn about_row(ui: &mut Ui, page: ComponentId, label_text: &str, value: &str, extra: Option<&str>, link: Option<&'static str>) {
    let Some(r) = row(ui, page, 18.0) else { return };
    label(ui, r, label_text, "s_about_l", Align::Left, 96.0, 18.0).map(|l| ui.control_mut(l).map(|cb| cb.set_margins([0.0, 0.0, 12.0, 0.0])));
    let style = if link.is_some() { "s_link" } else { "s_about_v" };
    if let Some(l) = label(ui, r, value, style, Align::Left, 0.0, 18.0) {
        let w = text_width(ui, l, value) + 2.0;
        if let Some(cb) = ui.control_mut(l) {
            cb.set_width(w).set_margins([0.0, 0.0, 12.0, 0.0]);
        }
        if let Some(url) = link {
            let _ = ui.set_prop(l, "hit_test", true);
            if let Some(cb) = ui.control_mut(l) {
                cb.set_cursor(CursorShape::Pointer);
            }
            ui.on(l, "on_click", move |_, _, _| bridge::handle_open_url(url));
        }
    }
    if let Some(x) = extra {
        label(ui, r, x, "s_tag", Align::Client, 0.0, 18.0);
    }
}

/// A lista com a bolinha verde (`.about-list`).
fn about_list(ui: &mut Ui, page: ComponentId, items: &[String]) {
    let w = col_w() - 14.0;
    for it in items {
        let Ok(r) = ui.create("Layout", Some(page)) else { continue };
        let Some(l) = label(ui, r, it, "s_list", Align::Client, 0.0, 0.0) else { continue };
        let _ = ui.set_prop(l, "markup", true);
        let h = md_lines(ui, l, it, w) * 12.0 * 1.55 + 2.0;
        if let Some(cb) = ui.control_mut(l) {
            cb.set_margins([14.0, 0.0, 0.0, 0.0]);
        }
        if let Ok(dot) = ui.create("Circle", Some(r)) {
            let _ = ui.set_prop(dot, "style", "s_dot");
            if let Some(cb) = ui.control_mut(dot) {
                cb.set_rect(2.0, 8.0, 6.0, 6.0).set_hit_test(false);
            }
        }
        gap(0.0);
        place(ui, r, h);
    }
    gap(4.0);
}

fn tab_about(ui: &mut Ui, page: ComponentId) {
    if let Some((iw, ih, rgba)) = crate::win::main_icon_rgba(96) {
        if let Ok(v) = ui.create("Image", Some(page)) {
            let _ = ui.set_prop(v, "fit", "contain");
            if let Some(iv) = ui.get_mut::<ImageView>(v) {
                iv.set_picture(Some(Image::rgba(iw, ih, rgba)));
            }
            place(ui, v, 48.0);
        }
    }
    gap(8.0);
    if let Some(l) = text_in(ui, page, "NoOBS", "s_big", col_w()) {
        let _ = ui.set_prop(l, "h_align", "center");
    }
    gap(2.0);
    if let Some(l) = text_in(ui, page, &lang::t("about.tagline"), "s_about_l", col_w()) {
        let _ = ui.set_prop(l, "h_align", "center");
    }
    gap(16.0);
    let desc = |ui: &mut Ui, text: &str| {
        if let Some(l) = text_in_md(ui, page, text, "s_desc", col_w()) {
            let h = ui.control(l).map_or(0.0, |cb| cb.height());
            if let Some(cb) = ui.control_mut(l) {
                cb.set_height(h * 1.165);
            }
        }
        gap(8.0);
    };
    desc(ui, &lang::t("about.intro1"));
    desc(ui, &lang::t("about.intro2"));
    desc(ui, &lang::t("about.intro3"));
    about_row(ui, page, &lang::t("about.labels.author"), "Eduardo Rodrigues", None, None);
    gap(4.0);
    about_row(ui, page, &lang::t("about.labels.coAuthor"), "Claude (Anthropic) - Max 5X · Opus 5.5", Some("1M"), None);
    gap(4.0);
    about_row(ui, page, &lang::t("about.labels.repository"), "github.com/e-delphi/NoOBS", None, Some("https://github.com/e-delphi/NoOBS"));
    gap(12.0);
    for (title, list) in [("about.labels.features", "about.features"), ("about.labels.builtWith", "about.builtWith")] {
        if let Some(t) = text_in(ui, page, &lang::t(title).to_uppercase(), "s_group", col_w()) {
            let _ = ui.set_prop(t, "wrap", false);
            let _ = ui.set_prop(t, "letter_spacing", 0.66);
            let _ = ui.set_prop(t, "v_align", "center");
            if let Some(cb) = ui.control_mut(t) {
                let h = cb.height();
                cb.set_height(h + 8.0);
            }
        }
        gap(0.0);
        about_list(ui, page, &lang_list(list));
    }
    gap(8.0);
    s_hint(ui, page, &lang::t("about.footer"));
}

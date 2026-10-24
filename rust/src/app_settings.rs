//! Presenter das Configurações (`settings.js`, `hotkeys.js`, `transcribe.js`,
//! `videotest.js` e o pedaço do `replay.js`/`displays.js` que mora nelas).
//!
//! Lê o estado direto do core (config, caps, filas) e cada mudança chama o
//! handler do core na hora — sem botão Salvar, como na web. Os dados que o
//! core empurra (fila de transcrição, motor local, atalhos, teste de
//! qualidade…) chegam pelo `View` e são guardados aqui pra redesenhar.

use std::cell::RefCell;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app_view::{AppWindow, DevGroup, DevRow, FolderRow, HkRow, QItem, SelOpt, St, VtLevel};
use crate::bridge::{self, hotkeys as bhk, recording, replay, settings, shell, transcribe as btr, video_test};
use crate::hotkeys::{Item, Mode};
use crate::types::GpuVendor;
use crate::view::{
    Device, LocalAsrInfo, QueueItem, ReplayState, RootFolder, TranscribeState, UpdateInfo, VideoTestState,
};
use crate::{config, encoder, hotkey, lang, log};

/// Camada da dica com as Configurações abertas (`Hints.top-layer`).
const HINT_LAYER: i32 = 3;
const AUDIO_BITRATES: [i32; 6] = [96, 128, 160, 192, 256, 320];
const AUDIO_BITRATE_DEFAULT: i32 = 192;
const REPLAY_SEC_OPTIONS: [i64; 8] = [30, 60, 120, 300, 600, 900, 1800, 3600];
const REPLAY_MB_OPTIONS: [i64; 16] = [512, 1024, 2048, 3072, 4096, 5120, 6144, 7168, 8192, 10240, 12288, 14336, 16384, 20480, 24576, 28672];
const REPLAY_DEFAULT_SEC: i64 = 300;
const REPLAY_DEFAULT_MB: i64 = 2048;
const CORNERS: [&str; 4] = ["top-left", "top-right", "bottom-left", "bottom-right"];
const CODECS: [&str; 6] = ["auto", "h264-hw", "h264-sw", "av1-hw", "av1-sw", "hevc-hw"];
const LIBRARY_MODES: [&str; 3] = ["auto", "always", "off"];
/// Idiomas oferecidos pra transcrição; os nomes saem no próprio idioma (o
/// `Intl.DisplayNames` da web).
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

/// Teclas do seletor de atalho (`HOTKEY_KEY_GROUPS`): (valor, rótulo ou chave
/// de tradução com `@`, é título de grupo).
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
    // Vídeo: o slider de fps guarda o ÍNDICE nas predefinições.
    fps_presets: Vec<i32>,
    quality: i32,
    fps: i32,
    keyframe: i32,
    rec_opacity: i32,
    // Dispositivos (dos pushes da tela principal).
    monitors: Vec<Device>,
    webcams: Vec<Device>,
    mics: Vec<Device>,
    speakers: Vec<Device>,
    replay: Option<ReplayState>,
    // Atalhos: a lista do core e as linhas da tela (modificadores marcados
    // sem tecla ficam só aqui até a tecla chegar).
    hk_items: Vec<Item>,
    hk_status: Vec<&'static str>,
    hk_delay_max: u32,
    hk_defining: Option<usize>,
    hk_rows: Vec<HkRow>,
    hk_keys: Vec<(String, String, bool)>,
    hk_delays: Vec<u32>,
    // Transcrição.
    tr: TranscribeState,
    tr_pending: usize,
    tr_cloud: usize,
    tr_queue: Vec<QueueItem>,
    tr_cancel_clicked: bool,
    tr_eta_shown: i64,
    tr_eta_for: String,
    tr_last_toast_error: String,
    tr_tick: Option<slint::Timer>,
    asr: Option<LocalAsrInfo>,
    asr_confirm_remove: bool,
    folders: Vec<RootFolder>,
    // Atualização.
    update: Option<UpdateInfo>,
    checking: bool,
    // Teste de qualidade.
    vt: VtState,
    // Debounce da lista de programas do buffer.
    apps_timer: Option<slint::Timer>,
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
    zoom: i32,
    timer: Option<slint::Timer>,
    /// Quadros já decodificados por nível (cada PNG 4K custa ~100 ms).
    imgs: std::collections::HashMap<i32, (slint::Image, u32, u32)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { tab: "general".into(), quality: 5, fps: 30, keyframe: 2, rec_opacity: 90, ..Default::default() });
    static WINDOW: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn win() -> Option<AppWindow> {
    WINDOW.with(|w| w.borrow().as_ref().and_then(|w| w.upgrade()))
}

fn sel(opts: &[(String, bool)]) -> ModelRc<SelOpt> {
    let v: Vec<SelOpt> = opts.iter().map(|(t, e)| SelOpt { text: t.into(), header: false, enabled: *e }).collect();
    ModelRc::new(VecModel::from(v))
}

fn sel_plain(opts: &[String]) -> ModelRc<SelOpt> {
    sel(&opts.iter().map(|t| (t.clone(), true)).collect::<Vec<_>>())
}

fn tw(key: &str, args: &[(&str, &str)]) -> String {
    lang::t_with(key, args)
}

/// O HTML simples dos textos (`<b>`, `<code>`) em Markdown pro `StyledText`.
fn md(html: &str) -> slint::StyledText {
    fn esc(s: &str) -> String {
        let mut o = String::with_capacity(s.len());
        for c in s.chars() {
            if "\\*_[]#+-!|~&`".contains(c) {
                o.push('\\');
            }
            o.push(c);
        }
        o
    }
    let mut out = String::new();
    let mut rest = html;
    while !rest.is_empty() {
        let (tag, close, mark) = if let Some(p) = rest.find("<b>") {
            if rest.find("<code>").is_some_and(|c| c < p) {
                ("<code>", "</code>", "`")
            } else {
                ("<b>", "</b>", "**")
            }
        } else if rest.contains("<code>") {
            ("<code>", "</code>", "`")
        } else {
            out.push_str(&esc(rest));
            break;
        };
        let p = rest.find(tag).unwrap_or(0);
        out.push_str(&esc(&rest[..p]));
        let after = &rest[p + tag.len()..];
        let e = after.find(close).unwrap_or(after.len());
        let inner = &after[..e];
        if mark == "`" {
            // O `Code` do StyledText sai minúsculo e com fundo; na web é só a
            // fonte monoespaçada, no mesmo tamanho. Texto normal fica mais perto.
            out.push_str(&esc(inner));
        } else {
            out.push_str("**");
            out.push_str(&esc(inner));
            out.push_str("**");
        }
        rest = after.get(e + close.len()..).unwrap_or("");
    }
    // Quebras de linha do HTML são espaços.
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    slint::StyledText::from_markdown(&out).unwrap_or_else(|_| slint::StyledText::from_plain_text(html))
}

fn fmt_dur(sec: i64) -> String {
    let sec = sec.max(0);
    let (m, s) = (sec / 60, sec % 60);
    if m >= 60 {
        format!("{}:{:02}:{:02}", m / 60, m % 60, s)
    } else {
        format!("{m}:{s:02}")
    }
}

/// Número com a vírgula decimal do idioma da interface.
fn fmt_num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if decimals > 0 {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    };
    if lang::current_language().starts_with("en") {
        s
    } else {
        s.replace('.', ",")
    }
}

// ---------------------------------------------------------------------------
// Abrir / fechar
// ---------------------------------------------------------------------------

/// Abre as Configurações (na aba em que o usuário parou, ou em `tab`).
pub fn open(tab: Option<&str>) {
    let Some(w) = win() else { return };
    if let Some(t) = tab {
        with(|s| s.tab = t.to_owned());
    }
    with(|s| s.open = true);
    fill_all(&w);
    let st = w.global::<St>();
    st.set_tab(with(|s| s.tab.clone()).into());
    st.set_open(true);
    crate::app_main::set_hint_layer(&w, HINT_LAYER);
    enter_tab(&with(|s| s.tab.clone()));
}

pub fn is_open() -> bool {
    with(|s| s.open)
}

fn close() {
    let Some(w) = win() else { return };
    with(|s| s.open = false);
    w.global::<St>().set_open(false);
    crate::app_main::set_hint_layer(&w, 0);
}

/// Pedidos ao core quando uma aba aparece (o `showTab` da web).
fn enter_tab(tab: &str) {
    match tab {
        "files" => {
            render_filename_preview();
            btr::push_root_folders();
        }
        "devices" => render_devices(),
        "replay" => {
            render_replay();
            replay::push_replay_state();
        }
        "hotkeys" => {
            render_hotkeys();
            bhk::push_state();
        }
        "onrecord" => render_auto_devices(),
        "transcribe" => {
            btr::handle_get_transcribe_state();
            btr::push_local_asr_state(false);
        }
        _ => {}
    }
}

fn show_tab(tab: &str) {
    let Some(w) = win() else { return };
    with(|s| s.tab = tab.to_owned());
    w.global::<St>().set_tab(tab.into());
    enter_tab(tab);
}

// ---------------------------------------------------------------------------
// Preenchimento
// ---------------------------------------------------------------------------

fn fill_all(w: &AppWindow) {
    let st = w.global::<St>();
    st.set_theme_pref(config::get_config_str("theme", "system").into());

    // Idioma da interface: "Automático" + os embutidos (nome nativo).
    let pref = config::get_config_str("language", "");
    let mut opts = vec![(lang::t("settings.language.auto"), true)];
    let mut idx = 0;
    if let Some(list) = lang::available_languages().as_array() {
        for l in list {
            let code = l.get("code").and_then(|v| v.as_str()).unwrap_or_default();
            if code.is_empty() {
                continue;
            }
            let name = l.get("nativeName").or_else(|| l.get("name")).and_then(|v| v.as_str()).unwrap_or(code);
            if code.eq_ignore_ascii_case(&pref) {
                idx = opts.len();
            }
            opts.push((name.to_owned(), true));
        }
    }
    st.set_language_options(sel(&opts));
    st.set_language_index(idx as i32);

    st.set_window_title(config::get_config_str("windowTitle", "NoOBS").into());
    st.set_autostart(crate::autostart::is_autostart_enabled());
    st.set_run_as_admin(config::get_config_bool("runAsAdmin", false));
    st.set_run_as_admin_state(
        lang::t(if crate::elevate::is_process_elevated() { "settings.runAsAdmin.stateOn" } else { "settings.runAsAdmin.stateOff" }).into(),
    );
    st.set_close_to_tray(config::get_config_bool("closeToTray", true));

    // Arquivos.
    st.set_record_dir(recording::record_dir().to_string_lossy().into_owned().into());
    st.set_filename_pattern(config::get_config_str("filenamePattern", settings::DEFAULT_FILENAME_PATTERN).into());
    let lib = config::get_config_str("libraryThumbs", "auto");
    st.set_library_index(LIBRARY_MODES.iter().position(|m| *m == lib).unwrap_or(0) as i32);
    render_library_hint();
    st.set_md_filename_hint(md(&lang::t("settings.filename.hint")));

    // Vídeo.
    let codec = config::get_config_str("codec", "auto");
    st.set_codec_index(CODECS.iter().position(|c| c.eq_ignore_ascii_case(&codec)).unwrap_or(0) as i32);
    render_codec_options();
    let q = encoder::recording_quality_level().clamp(0, 10);
    let fps = config::get_config_int("recordingFps", 30).max(10);
    let kf = config::get_config_int("recordingKeyframeSec", 2).clamp(1, 10);
    let presets = fps_presets(crate::win_preview::max_monitor_refresh_rate());
    with(|s| {
        s.quality = q;
        s.fps = fps;
        s.keyframe = kf;
        s.fps_presets = presets.clone();
    });
    st.set_fps_ticks(ModelRc::new(VecModel::from(presets.iter().map(|v| SharedString::from(v.to_string())).collect::<Vec<_>>())));
    render_video_sliders();
    render_vt();

    // Áudio.
    let ab = encoder::audio_bitrate_kbps();
    let labels: Vec<String> = AUDIO_BITRATES
        .iter()
        .map(|k| {
            let key = if *k == AUDIO_BITRATE_DEFAULT { "settings.audioBitrate.optionDefault" } else { "settings.audioBitrate.option" };
            tw(key, &[("kbps", &k.to_string())])
        })
        .collect();
    st.set_audio_bitrate_options(sel_plain(&labels));
    let ai = AUDIO_BITRATES.iter().position(|k| *k == ab).unwrap_or(3);
    st.set_audio_bitrate_index(ai as i32);
    render_audio_hint(AUDIO_BITRATES[ai]);
    st.set_mute_when_muted(config::get_config_bool("muteWhenDeviceMuted", true));

    // Buffer.
    st.set_md_replay_hint(md(&lang::t("settings.replay.hint")));
    st.set_md_replay_ind_hint(md(&lang::t("settings.replay.indicatorHint")));
    render_replay();

    // Gravação.
    st.set_rec_indicator(config::get_config_bool("recIndicator", false));
    st.set_rec_clickable(config::get_config_bool("recIndicatorClickable", true));
    let corner = config::get_config_str("recIndicatorCorner", "top-right");
    st.set_rec_corner(CORNERS.iter().position(|c| *c == corner).unwrap_or(1) as i32);
    let op = config::get_config_int("recIndicatorOpacity", 90).clamp(20, 100);
    with(|s| s.rec_opacity = op);
    st.set_rec_opacity(op);
    st.set_md_rec_ind_hint(md(&lang::t("settings.recIndicator.hint")));
    st.set_fullscreen_detect(config::get_config_bool("fullscreenDetect", true));
    st.set_play_sound(config::get_config_bool("playSoundOnRecord", true));
    st.set_minimize_on_record(config::get_config_bool("minimizeOnRecord", true));
    st.set_auto_record(config::get_config_bool("autoRecordOnMic", false));
    st.set_auto_apps(config::get_config_str("autoRecordMicApps", "").into());
    st.set_auto_except(config::get_config_str("autoRecordMicExcept", "").into());
    st.set_stop_on_lock(config::get_config_bool("stopOnLock", true));
    render_dependent_labels();
    render_auto_devices();

    // Transcrição.
    render_transcribe_langs();
    st.set_tr_on_stop(config::get_config_bool("transcribeOnStop", true));
    render_asr();
    render_transcribe();

    // Atualizações.
    st.set_app_version(crate::version::display_version().into());
    st.set_check_updates(config::get_config_bool("checkUpdates", true));
    render_update();

    // Sobre.
    st.set_md_about_2(md(&lang::t("about.intro2")));
    st.set_md_about_3(md(&lang::t("about.intro3")));
    st.set_md_features(md_list("about.features"));
    st.set_md_built(md_list("about.builtWith"));
    if let Some((iw, ih, rgba)) = crate::win::main_icon_rgba(256) {
        let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, iw, ih);
        st.set_app_icon(slint::Image::from_rgba8(buf));
    }

    render_devices();
    render_hotkeys();
    render_folders();
}

fn md_list(key: &str) -> ModelRc<slint::StyledText> {
    let mut v = Vec::new();
    let mut i = 0;
    loop {
        let k = format!("{key}.{i}");
        let t = lang::t(&k);
        if t.is_empty() || t.starts_with('[') {
            break;
        }
        v.push(md(&t));
        i += 1;
    }
    ModelRc::new(VecModel::from(v))
}

/// Predefinições do slider de fps: 20, 30 e múltiplos de 30 até o monitor
/// mais rápido; se ele não for múltiplo de 30, o último múltiplo dá lugar a
/// ele (`_computeFpsPresets`).
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

fn codec_value() -> String {
    let Some(w) = win() else { return "auto".into() };
    CODECS.get(w.global::<St>().get_codec_index() as usize).copied().unwrap_or("auto").to_owned()
}

fn render_codec_options() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let caps = bridge::last_encoder_caps();
    let tag = match caps.map(|c| c.vendor) {
        Some(GpuVendor::Nvidia) => lang::t("settings.codec.vendorNvidia"),
        Some(GpuVendor::Amd) => lang::t("settings.codec.vendorAmd"),
        Some(GpuVendor::Intel) => lang::t("settings.codec.vendorIntel"),
        _ => String::new(),
    };
    let unavailable = format!(" {}", lang::t("settings.codec.unavailable"));
    let opts: Vec<(String, bool)> = CODECS
        .iter()
        .map(|c| {
            // Sem as caps ainda (antes do warmup) tudo fica escolhível, como na web.
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
        .collect();
    st.set_codec_options(sel(&opts));
    st.set_gpu_vendor(
        match caps.map(|c| c.vendor) {
            Some(GpuVendor::Nvidia) => "nvidia",
            Some(GpuVendor::Amd) => "amd",
            Some(GpuVendor::Intel) => "intel",
            _ => "",
        }
        .into(),
    );
    render_codec_hints();
}

fn render_codec_hints() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let codec = codec_value();
    let dim = match codec.as_str() {
        "h264-hw" => 4096,
        "h264-sw" | "hevc-hw" | "av1-hw" | "av1-sw" => 8192,
        _ if bridge::last_encoder_caps().is_some_and(|c| c.h264_hw) => 4096,
        _ => 8192,
    };
    st.set_codec_maxres(tw("settings.codec.maxRes", &[("w", &dim.to_string()), ("h", &dim.to_string())]).into());
    st.set_codec_cpu_warn(codec == "av1-sw");
    let q = with(|s| s.quality);
    st.set_quality_svt_warn(codec == "av1-sw" && q <= VT_SVT_FLOOR_LEVEL);
    render_filename_preview();
}

fn render_video_sliders() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let (q, fps, kf, presets) = with(|s| (s.quality, s.fps, s.keyframe, s.fps_presets.clone()));
    st.set_quality(q);
    st.set_quality_hint(lang::t(&format!("settings.quality.hint.{q}")).into());
    let fi = fps_index(&presets, fps);
    st.set_fps_index(fi as i32);
    let v = presets.get(fi).copied().unwrap_or(fps);
    let bucket = match v {
        v if v <= 24 => "low",
        v if v <= 30 => "smooth",
        v if v <= 60 => "good",
        v if v <= 120 => "high",
        _ => "veryHigh",
    };
    st.set_fps_hint(tw(&format!("settings.fps.hint.{bucket}"), &[("fps", &v.to_string())]).into());
    st.set_keyframe(kf);
    let kb = match kf {
        k if k <= 1 => "precise",
        k if k <= 2 => "balanced",
        k if k <= 5 => "medium",
        _ => "spaced",
    };
    st.set_keyframe_hint(tw(&format!("settings.keyframe.hint.{kb}"), &[("sec", &kf.to_string())]).into());
    render_codec_hints();
}

fn render_audio_hint(k: i32) {
    let Some(w) = win() else { return };
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
    let txt = format!("{} {}", lang::t(&format!("settings.audioBitrate.hint.{band}")), tw("settings.audioBitrate.size", &[("mb", &mb.to_string())]));
    w.global::<St>().set_audio_bitrate_hint(txt.into());
}

fn render_library_hint() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let mode = LIBRARY_MODES.get(st.get_library_index() as usize).copied().unwrap_or("auto");
    let mut txt = lang::t(&format!("settings.library.hint.{mode}"));
    if mode != "off" && settings::is_path_under_onedrive(&recording::record_dir().to_string_lossy()) {
        txt.push(' ');
        txt.push_str(&lang::t("settings.library.cloudNote"));
    }
    st.set_library_hint(txt.into());
}

/// Prévia do nome do arquivo (`_buildFilenameFromPattern`): data/hora de agora
/// e o codec/fps/qualidade das abas de vídeo.
fn render_filename_preview() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let pattern = st.get_filename_pattern().to_string();
    let codec = codec_value();
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
    let mut s = pattern;
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
    st.set_filename_preview(tw("settings.filename.previewLabel", &[("name", &format!("{s}.mkv"))]).into());
}

/// Textos que dependem de outras opções: "minimizar" (bandeja ou barra de
/// tarefas) e o "tela cheia" (quais indicadores estão em jogo).
fn render_dependent_labels() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let tray = st.get_close_to_tray();
    st.set_minimize_label(lang::t(if tray { "settings.minimizeOnRecord.labelTray" } else { "settings.minimizeOnRecord.labelTaskbar" }).into());
    st.set_minimize_hint(lang::t(if tray { "settings.minimizeOnRecord.hintTray" } else { "settings.minimizeOnRecord.hintTaskbar" }).into());
    let rec_on = st.get_rec_indicator();
    let replay_on = st.get_replay_indicator();
    let label = match (rec_on, replay_on) {
        (true, true) => "labelBoth",
        (true, false) => "labelRecording",
        (false, true) => "labelBuffer",
        _ => "labelPreviews",
    };
    st.set_fullscreen_label(lang::t(&format!("settings.fullscreenDetect.{label}")).into());
    st.set_fullscreen_hint(lang::t(if rec_on || replay_on { "settings.fullscreenDetect.hint" } else { "settings.fullscreenDetect.hintPreviews" }).into());
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

fn replay_lists() -> (Vec<i64>, Vec<i64>) {
    let r = with(|s| s.replay.clone()).unwrap_or_default();
    let limit = if r.max_mb_limit > 0 { r.max_mb_limit } else { REPLAY_DEFAULT_MB };
    let mut mbs: Vec<i64> = REPLAY_MB_OPTIONS.iter().copied().filter(|v| *v < limit).collect();
    mbs.push(limit);
    (REPLAY_SEC_OPTIONS.to_vec(), mbs)
}

fn render_replay() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let Some(r) = with(|s| s.replay.clone()) else { return };
    st.set_replay_autostart(r.auto_start);
    st.set_replay_into_rec(r.into_recording);
    st.set_replay_indicator(r.indicator);
    st.set_replay_clickable(r.indicator_clickable);
    st.set_replay_corner(CORNERS.iter().position(|c| *c == r.indicator_corner).unwrap_or(1) as i32);
    st.set_replay_opacity(if r.indicator_opacity > 0 { r.indicator_opacity as i32 } else { 90 });
    let (secs, mbs) = replay_lists();
    let (secs, si) = with_current(secs, if r.max_sec > 0 { r.max_sec } else { REPLAY_DEFAULT_SEC });
    st.set_replay_sec_options(sel_plain(&secs.iter().map(|v| sec_label(*v)).collect::<Vec<_>>()));
    st.set_replay_sec_index(si as i32);
    let limit = if r.max_mb_limit > 0 { r.max_mb_limit } else { REPLAY_DEFAULT_MB };
    let (mbs, mi) = with_current(mbs, if r.max_mb > 0 { r.max_mb } else { REPLAY_DEFAULT_MB });
    let labels: Vec<String> =
        mbs.iter().map(|v| if *v >= limit { tw("settings.replay.memoryMax", &[("size", &mb_label(*v))]) } else { mb_label(*v) }).collect();
    st.set_replay_mb_options(sel_plain(&labels));
    st.set_replay_mb_index(mi as i32);
    // Não sobrescreve enquanto o usuário digita (pegadinha #56).
    if with(|s| s.apps_timer.is_none()) {
        st.set_replay_apps(r.apps.clone().into());
    }
    render_dependent_labels();
}

fn replay_limits_changed() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let r = with(|s| s.replay.clone()).unwrap_or_default();
    let (secs, mbs) = replay_lists();
    let (secs, _) = with_current(secs, if r.max_sec > 0 { r.max_sec } else { REPLAY_DEFAULT_SEC });
    let (mbs, _) = with_current(mbs, if r.max_mb > 0 { r.max_mb } else { REPLAY_DEFAULT_MB });
    let sec = secs.get(st.get_replay_sec_index() as usize).copied().unwrap_or(REPLAY_DEFAULT_SEC);
    let mb = mbs.get(st.get_replay_mb_index() as usize).copied().unwrap_or(REPLAY_DEFAULT_MB);
    replay::handle_set_replay_limits(sec as i32, mb as i32);
}

// ---------------------------------------------------------------------------
// Dispositivos
// ---------------------------------------------------------------------------

fn sorted(list: &[Device]) -> Vec<Device> {
    let mut v = list.to_vec();
    v.sort_by_key(|d| natural_key(&d.name));
    v
}

/// Ordem "natural" sem acento nem caixa ("Monitor 2" < "Monitor 10").
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

fn groups(pick: impl Fn(&Device) -> Option<bool>) -> ModelRc<DevGroup> {
    let (mons, cams, mics, spks) = with(|s| (s.monitors.clone(), s.webcams.clone(), s.mics.clone(), s.speakers.clone()));
    let mut out = Vec::new();
    for (key, list) in [("monitors", mons), ("webcams", cams), ("mics", mics), ("speakers", spks)] {
        let rows: Vec<DevRow> = sorted(&list)
            .iter()
            .filter_map(|d| {
                pick(d).map(|checked| DevRow {
                    id: d.id.clone().into(),
                    name: if d.name.is_empty() { d.id.clone().into() } else { d.name.clone().into() },
                    info: d.info.clone().into(),
                    checked,
                })
            })
            .collect();
        if !rows.is_empty() {
            out.push(DevGroup { title: lang::t(&format!("devices.groups.{key}")).into(), rows: ModelRc::new(VecModel::from(rows)) });
        }
    }
    ModelRc::new(VecModel::from(out))
}

fn render_devices() {
    let Some(w) = win() else { return };
    w.global::<St>().set_dev_groups(groups(|d| Some(!d.hidden)));
}

fn render_auto_devices() {
    let Some(w) = win() else { return };
    w.global::<St>().set_auto_groups(groups(|d| (!d.hidden).then_some(d.auto)));
}

/// Listas de dispositivos da tela principal (hot-plug incluso).
pub fn on_devices(kind: &str, list: &[Device]) {
    with(|s| match kind {
        "monitors" => s.monitors = list.to_vec(),
        "webcams" => s.webcams = list.to_vec(),
        "mics" => s.mics = list.to_vec(),
        _ => s.speakers = list.to_vec(),
    });
    if is_open() {
        render_devices();
        render_auto_devices();
    }
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

// ---------------------------------------------------------------------------
// Pastas ocultas
// ---------------------------------------------------------------------------

pub fn on_root_folders(folders: &[RootFolder]) {
    with(|s| s.folders = folders.to_vec());
    render_folders();
}

fn render_folders() {
    let Some(w) = win() else { return };
    let rows: Vec<FolderRow> = with(|s| s.folders.clone())
        .iter()
        .map(|f| FolderRow {
            name: f.name.clone().into(),
            info: if f.missing {
                lang::t("settings.hiddenFolders.missing")
            } else {
                tw(
                    if f.count == 1 { "settings.hiddenFolders.countOne" } else { "settings.hiddenFolders.countMany" },
                    &[("count", &f.count.to_string())],
                )
            }
            .into(),
            hidden: f.hidden,
        })
        .collect();
    w.global::<St>().set_hidden_folders(ModelRc::new(VecModel::from(rows)));
}

// ---------------------------------------------------------------------------
// Atalhos
// ---------------------------------------------------------------------------

pub fn on_hotkeys_state(items: &[Item], status: &[&'static str], delay_max: u32, defining: Option<usize>) {
    with(|s| {
        s.hk_items = items.to_vec();
        s.hk_status = status.to_vec();
        s.hk_delay_max = delay_max;
        s.hk_defining = defining;
    });
    render_hotkeys();
}

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

fn render_hotkeys() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let keys = key_table();
    let (items, status, delay_max, defining) = with(|s| (s.hk_items.clone(), s.hk_status.clone(), s.hk_delay_max, s.hk_defining));
    // Atrasos: os da lista até o teto, mais qualquer um fora dela (config editado).
    let mut delays: Vec<u32> = HK_DELAYS.iter().copied().filter(|v| *v <= delay_max.max(1)).collect();
    for it in &items {
        if !delays.contains(&it.delay_sec) {
            delays.push(it.delay_sec);
        }
    }
    delays.sort_unstable();
    let rows: Vec<HkRow> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let (c, sh, a, wn, key) = split_spec(&it.hotkey);
            let ki = if key.is_empty() { 0 } else { keys.iter().position(|(k, _, h)| !h && k.eq_ignore_ascii_case(&key)).map_or(0, |p| p + 1) };
            let st_key = status.get(i).copied().unwrap_or("");
            HkRow {
                mode: HK_MODES.iter().position(|m| *m == it.mode).unwrap_or(0) as i32,
                ctrl: c,
                shift: sh,
                alt: a,
                win: wn,
                key: ki as i32,
                title: it.title.clone().into(),
                app_audio: it.app_audio,
                area_text: match it.area {
                    Some(r) => tw(
                        "hotkeys.areaValue",
                        &[("w", &r.w.to_string()), ("h", &r.h.to_string()), ("x", &r.x.to_string()), ("y", &r.y.to_string())],
                    ),
                    None => lang::t("hotkeys.areaAsk"),
                }
                .into(),
                has_area: it.area.is_some(),
                area_button: lang::t(if defining == Some(i) {
                    "hotkeys.areaChoosing"
                } else if it.area.is_some() {
                    "hotkeys.areaRedefine"
                } else {
                    "hotkeys.areaDefine"
                })
                .into(),
                delay: delays.iter().position(|d| *d == it.delay_sec).unwrap_or(0) as i32,
                status: if st_key.is_empty() { SharedString::new() } else { lang::t(&format!("hotkeys.status.{st_key}")).into() },
                status_warn: !st_key.is_empty() && st_key != "inactive",
            }
        })
        .collect();
    let mut key_opts = vec![SelOpt { text: lang::t("settings.hotkey.notDefined").into(), header: false, enabled: true }];
    key_opts.extend(keys.iter().map(|(_, l, h)| SelOpt { text: l.clone().into(), header: *h, enabled: true }));
    st.set_hk_key_options(ModelRc::new(VecModel::from(key_opts)));
    st.set_hk_mode_options(sel_plain(&HK_MODES.iter().map(|m| lang::t(&format!("hotkeys.mode.{}", m.as_str()))).collect::<Vec<_>>()));
    st.set_hk_delay_options(sel_plain(
        &delays.iter().map(|v| if *v == 0 { lang::t("hotkeys.delayOff") } else { tw("hotkeys.delaySec", &[("sec", &v.to_string())]) }).collect::<Vec<_>>(),
    ));
    st.set_hk_defining(defining.is_some());
    st.set_hk_rows(ModelRc::new(VecModel::from(rows.clone())));
    with(|s| {
        s.hk_rows = rows;
        s.hk_keys = keys;
        s.hk_delays = delays;
    });
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
/// repetida, não reservada) antes de mandar. Só modificadores ainda não é
/// combinação: fica na tela esperando a tecla.
fn hk_combo_changed(i: usize) {
    let Some(w) = win() else { return };
    let rows = with(|s| s.hk_rows.clone());
    let Some(r) = rows.get(i) else { return };
    let keys = with(|s| s.hk_keys.clone());
    let key = if r.key > 0 { keys.get(r.key as usize - 1).map(|k| k.0.clone()).unwrap_or_default() } else { String::new() };
    let mut parts: Vec<&str> = Vec::new();
    if r.ctrl {
        parts.push("Ctrl");
    }
    if r.shift {
        parts.push("Shift");
    }
    if r.alt {
        parts.push("Alt");
    }
    if r.win {
        parts.push("Win");
    }
    if key.is_empty() {
        if !parts.is_empty() {
            // Espera a tecla: o que está marcado fica só na tela.
            w.global::<St>().set_hk_rows(ModelRc::new(VecModel::from(rows)));
            return;
        }
    } else {
        parts.push(&key);
    }
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
            render_hotkeys();
            return;
        }
        let reason = match hotkey::parse_hotkey(&spec) {
            None => Some("settings.hotkey.invalidSpec"),
            Some(hk) => hotkey::reserved_reason(hk.modifiers, hk.vk),
        };
        if let Some(r) = reason {
            crate::app_view::show_toast(
                &lang::t("toast.invalidHotkey"),
                &format!("{} {}", lang::t(r), lang::t("settings.hotkey.chooseAnother")),
                true,
                7000,
            );
            render_hotkeys();
            return;
        }
    }
    hk_update(i, |it| it.hotkey = spec);
}

// ---------------------------------------------------------------------------
// Transcrição
// ---------------------------------------------------------------------------

fn render_transcribe_langs() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let cur = match config::get_config_str("transcribeLanguage", "").trim().to_lowercase() {
        l if l.is_empty() => "app".to_owned(),
        l => l,
    };
    let ui = lang::current_language();
    let base = ui.split('-').next().unwrap_or("pt").to_lowercase();
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
    st.set_tr_lang_options(sel_plain(&labels));
    st.set_tr_lang_index(vals.iter().position(|v| *v == cur).unwrap_or(0) as i32);
}

fn transcribe_lang_value(i: usize) -> String {
    let mut vals: Vec<String> = vec!["app".into(), "auto".into()];
    vals.extend(TRANSCRIBE_LANGS.iter().map(|(c, _)| c.to_string()));
    let cur = config::get_config_str("transcribeLanguage", "").trim().to_lowercase();
    if !cur.is_empty() && !vals.contains(&cur) {
        vals.push(cur);
    }
    vals.get(i).cloned().unwrap_or_else(|| "app".into())
}

fn gb(bytes: i64) -> String {
    let g = bytes.max(0) as f64 / 1e9;
    format!("{} GB", fmt_num(g, if g >= 10.0 { 0 } else { 1 }))
}

pub fn on_local_asr_state(st: &LocalAsrInfo) {
    if st.remove_refused {
        crate::app_view::show_toast(&lang::t("settings.transcribe.local.removeBusy"), "", true, 4000);
    }
    with(|s| s.asr = Some(st.clone()));
    render_asr();
    crate::app_main::on_transcribe_chip(transcribe_chip());
}

fn render_asr() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let d = with(|s| s.asr.clone()).unwrap_or(LocalAsrInfo { status: "missing".into(), ..Default::default() });
    let confirm = with(|s| s.asr_confirm_remove);
    let l = |k: &str| lang::t(&format!("settings.transcribe.local.{k}"));
    let lw = |k: &str, a: &[(&str, &str)]| tw(&format!("settings.transcribe.local.{k}"), a);
    st.set_asr_progress(-1.0);
    st.set_asr_button_danger(false);
    match d.status.as_str() {
        "installing" => {
            let stage = if d.stage.is_empty() { String::new() } else { l(&format!("stages.{}", d.stage)) };
            st.set_asr_kind("".into());
            st.set_asr_status(lw("installing", &[("stage", &stage)]).into());
            st.set_asr_progress(if d.total > 0 { (d.done as f64 / d.total as f64).min(1.0) as f32 } else { 0.0 });
            st.set_asr_detail(format!("{} · {}", lw("progress", &[("done", &gb(d.done)), ("total", &gb(d.total))]), l("resumeHint")).into());
            st.set_asr_button(l("cancel").into());
        }
        "ready" => {
            st.set_asr_kind("ok".into());
            st.set_asr_status(if d.device.is_empty() { l("readyCpu") } else { lw("ready", &[("device", &d.device)]) }.into());
            st.set_asr_detail(lw("readyHow", &[("size", &gb(d.total))]).into());
            st.set_asr_button(if confirm { l("removeConfirm") } else { l("remove") }.into());
            st.set_asr_button_danger(confirm);
        }
        s => {
            if s == "error" {
                st.set_asr_kind("fail".into());
                st.set_asr_status(lw("error", &[("error", &d.error)]).into());
            } else {
                st.set_asr_kind("".into());
                st.set_asr_status(l("missing").into());
            }
            st.set_asr_detail(lw("missingHow", &[("size", &gb(d.total))]).into());
            st.set_asr_button(if s == "error" { l("retry") } else { l("install") }.into());
        }
    }
}

fn asr_action() {
    let status = with(|s| s.asr.as_ref().map(|a| a.status.clone())).unwrap_or_else(|| "missing".into());
    match status.as_str() {
        "installing" => crate::local_asr::cancel_install(),
        "ready" => {
            // Dois cliques: apagar 3,7 GB por engano custaria outro download.
            if !with(|s| std::mem::replace(&mut s.asr_confirm_remove, false)) {
                with(|s| s.asr_confirm_remove = true);
                render_asr();
                return;
            }
            render_asr();
            btr::handle_local_asr_remove();
        }
        _ => crate::local_asr::start_install(),
    }
}

pub fn on_transcribe_state(t: &TranscribeState) {
    let (failed_before, was_canceling) = with(|s| (s.tr.failed, s.tr.canceling || s.tr_cancel_clicked));
    with(|s| {
        s.tr = t.clone();
        if t.canceling || !t.running {
            s.tr_cancel_clicked = false;
        }
        // O "faltam" só desce (`_syncEta`).
        let key = if t.running { format!("{}#{}", t.current, t.track) } else { String::new() };
        if key != s.tr_eta_for {
            s.tr_eta_for = key;
            s.tr_eta_shown = -1;
        }
        if t.eta > 0 {
            s.tr_eta_shown = if s.tr_eta_shown < 0 { t.eta } else { s.tr_eta_shown.min(t.eta) };
        }
    });
    // Toast na BORDA do contador e só com motivo novo.
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
    sync_tick(t.running);
    render_transcribe();
    crate::app_main::on_transcribe_chip(transcribe_chip());
}

/// O decorrido anda sozinho entre os pushes (o core só reempurra quando muda).
fn sync_tick(running: bool) {
    let has = with(|s| s.tr_tick.is_some());
    if running && !has {
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, Duration::from_secs(1), || {
            with(|s| {
                s.tr.elapsed += 1;
                if s.tr_eta_shown > 0 {
                    s.tr_eta_shown -= 1;
                }
            });
            render_transcribe();
        });
        with(|s| s.tr_tick = Some(t));
    } else if !running && has {
        with(|s| s.tr_tick = None);
    }
}

pub fn on_transcribe_pending(count: usize, cloud: usize) {
    with(|s| {
        s.tr_pending = count;
        s.tr_cloud = cloud;
    });
    render_transcribe();
}

pub fn on_transcribe_queue(items: &[QueueItem]) {
    with(|s| s.tr_queue = items.to_vec());
    render_transcribe();
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
            if waiting > 0 {
                tw("transcribeChip.runningQueue", &[("count", &waiting.to_string())])
            } else {
                lang::t("transcribeChip.running")
            }
        } else {
            tw("transcribeChip.queued", &[("count", &waiting.to_string())])
        };
        (show, running, total.max(1) as i32, hint)
    })
}

fn render_transcribe() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let (t, pending, cloud, queue, clicked, eta) = with(|s| (s.tr.clone(), s.tr_pending, s.tr_cloud, s.tr_queue.clone(), s.tr_cancel_clicked, s.tr_eta_shown));
    let canceling = t.canceling || clicked;
    st.set_tr_cancel_enabled(!canceling && (t.running || t.queue > 0));
    st.set_tr_cancel_label(lang::t(if canceling { "settings.transcribe.canceling" } else { "common.cancel" }).into());
    st.set_tr_pending_enabled(pending > 0);
    st.set_tr_pending_label(
        if pending > 0 { tw("settings.transcribe.pendingN", &[("count", &pending.to_string())]) } else { lang::t("settings.transcribe.pendingNone") }.into(),
    );
    st.set_tr_error(
        if t.last_error.is_empty() {
            String::new()
        } else if t.last_error_name.is_empty() {
            tw("settings.transcribe.error", &[("error", &t.last_error)])
        } else {
            tw("settings.transcribe.errorNamed", &[("name", &t.last_error_name), ("error", &t.last_error)])
        }
        .into(),
    );
    st.set_tr_cloud_hint(if cloud > 0 { tw("settings.transcribe.pendingCloud", &[("count", &cloud.to_string())]) } else { String::new() }.into());
    let rows: Vec<QItem> = queue
        .iter()
        .map(|q| QItem { id: q.id.clone().into(), name: q.name.clone().into(), dur: if q.duration > 0 { fmt_dur(q.duration as i64) } else { "—".into() }.into(), current: q.current })
        .collect();
    st.set_tr_queue(ModelRc::new(VecModel::from(rows)));

    if !t.running && t.queue == 0 {
        if t.total > 0 {
            st.set_tr_progress_visible(true);
            st.set_tr_progress(1.0);
            st.set_tr_line(
                if t.failed > 0 {
                    tw("settings.transcribe.doneWithFailures", &[("done", &t.done.to_string()), ("total", &t.total.to_string()), ("failed", &t.failed.to_string())])
                } else {
                    tw("settings.transcribe.doneAll", &[("total", &t.total.to_string())])
                }
                .into(),
            );
        } else {
            st.set_tr_progress_visible(false);
        }
        return;
    }
    st.set_tr_progress_visible(true);
    if canceling && t.running {
        st.set_tr_line(tw("settings.transcribe.cancelingLine", &[("name", if t.current.is_empty() { "—" } else { &t.current })]).into());
        return;
    }
    let pos = (t.done + t.failed + 1).min(t.total.max(1));
    let total = if t.total > 0 { t.total } else { pos };
    let mut txt = tw(
        "settings.transcribe.progress",
        &[("pos", &pos.to_string()), ("total", &total.to_string()), ("name", if t.current.is_empty() { "—" } else { &t.current })],
    );
    if !t.stage.is_empty() {
        let stg = lang::t(&format!("settings.transcribe.stage.{}", t.stage));
        if !stg.starts_with('[') {
            txt.push_str(&format!(" · {stg}"));
        }
    }
    if t.track_count > 1 {
        txt.push_str(&format!(" · {}", tw("settings.transcribe.trackN", &[("n", &(t.track + 1).to_string()), ("total", &t.track_count.to_string())])));
    }
    st.set_tr_progress(if t.progress >= 0.0 { t.progress as f32 } else { 0.0 });
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
    st.set_tr_line(txt.into());
}

// ---------------------------------------------------------------------------
// Atualização
// ---------------------------------------------------------------------------

pub fn on_update_result(r: &UpdateInfo) {
    with(|s| {
        s.update = Some(r.clone());
        s.checking = false;
    });
    if r.ok && r.has_update {
        crate::app_main::show_update_banner(&r.latest, &r.url);
    }
    render_update();
}

fn render_update() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let (r, checking) = with(|s| (s.update.clone(), s.checking));
    st.set_update_checking(checking);
    st.set_update_available(false);
    st.set_update_has_url(false);
    if checking {
        st.set_update_status(lang::t("settings.updates.checking").into());
        return;
    }
    let Some(r) = r else {
        st.set_update_status(SharedString::new());
        return;
    };
    if !r.ok {
        st.set_update_status(lang::t("settings.updates.failed").into());
    } else if r.has_update {
        st.set_update_available(true);
        st.set_update_has_url(!r.url.is_empty());
        st.set_update_status(tw("settings.updates.available", &[("version", &r.latest)]).into());
    } else {
        st.set_update_status(lang::t("settings.updates.upToDate").into());
    }
}

// ---------------------------------------------------------------------------
// Teste de qualidade (pegadinha #64)
// ---------------------------------------------------------------------------

pub fn on_video_test(d: &VideoTestState) {
    match d.state.as_str() {
        "recording" => {
            with(|s| {
                s.vt.running = true;
                s.vt.capturing = true;
                s.vt.countdown = if d.seconds > 0 { d.seconds as i32 } else { 5 };
            });
            let t = slint::Timer::default();
            t.start(slint::TimerMode::Repeated, Duration::from_secs(1), || {
                with(|s| s.vt.countdown = (s.vt.countdown - 1).max(0));
                render_vt();
            });
            with(|s| s.vt.timer = Some(t));
        }
        "encoding" => with(|s| {
            s.vt.running = true;
            s.vt.capturing = false;
            s.vt.timer = None;
            s.vt.pct = d.pct;
            s.vt.level = d.level;
        }),
        "done" => {
            with(|s| {
                s.vt.running = false;
                s.vt.capturing = false;
                s.vt.timer = None;
                let cur = d.current_level;
                s.vt.a = if cur == 10 { 9 } else { cur };
                s.vt.b = 10;
                s.vt.result = Some(d.clone());
                s.vt.imgs.clear();
            });
            if let Some(w) = win() {
                w.global::<St>().set_vt_split(0.5);
            }
        }
        st => {
            with(|s| {
                s.vt.running = false;
                s.vt.capturing = false;
                s.vt.timer = None;
            });
            if st == "error" {
                let msg = if d.error.is_empty() { lang::t("settings.videoTest.failed") } else { d.error.clone() };
                crate::app_view::show_toast(&lang::t("toast.errorTitle"), &msg, true, 6000);
            }
        }
    }
    render_vt();
}

fn per_hour(bytes: i64, dur: f64) -> f64 {
    if bytes <= 0 || dur <= 0.0 {
        0.0
    } else {
        bytes as f64 / dur * 3600.0
    }
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

/// O quadro de um nível (PNG no tamanho original). O Slint daqui não decodifica
/// arquivo em tempo de execução: vai pelo crate `image`.
fn load_png(path: &str) -> Option<(slint::Image, u32, u32)> {
    let img = match image::open(path) {
        Ok(i) => i.into_rgba8(),
        Err(e) => {
            log!("VideoTest: quadro \"{path}\" nao abriu: {e}");
            return None;
        }
    };
    let (w, h) = img.dimensions();
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h);
    Some((slint::Image::from_rgba8(buf), w, h))
}

fn render_vt() {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    let v = with(|s| (s.vt.running, s.vt.capturing, s.vt.countdown, s.vt.pct, s.vt.level, s.vt.result.clone(), s.vt.a, s.vt.b, s.vt.zoom));
    let (running, capturing, countdown, pct, level, result, a, b, zoom) = v;
    st.set_vt_button(lang::t(if result.is_some() { "settings.videoTest.again" } else { "settings.videoTest.run" }).into());
    st.set_vt_busy(running);
    st.set_vt_cancel_visible(running);
    let status = if capturing {
        tw("settings.videoTest.capturing", &[("sec", &countdown.to_string())])
    } else if running {
        if level >= 0 {
            tw("settings.videoTest.encoding", &[("n", &level.to_string()), ("pct", &pct.to_string())])
        } else {
            lang::t("settings.videoTest.preparing")
        }
    } else {
        String::new()
    };
    st.set_vt_status(status.into());
    st.set_vt_status_recording(capturing);
    st.set_vt_progress(if running && !capturing { (pct.clamp(0, 100) as f32) / 100.0 } else { -1.0 });
    let Some(r) = result.filter(|_| !running) else {
        st.set_vt_has_result(false);
        return;
    };
    st.set_vt_has_result(true);
    st.set_vt_meta(
        tw(
            "settings.videoTest.meta",
            &[
                ("w", &r.width.to_string()),
                ("h", &r.height.to_string()),
                ("fps", &(r.fps.round() as i64).to_string()),
                ("sec", &fmt_num(r.duration, 1)),
                ("encoder", &r.encoder),
            ],
        )
        .into(),
    );
    let mut levels = r.levels.clone();
    levels.sort_by_key(|l| -l.level);
    let max = levels.iter().map(|l| per_hour(l.bytes, r.duration)).fold(1.0f64, f64::max);
    let cur_h = levels.iter().find(|l| l.level == r.current_level).map(|l| per_hour(l.bytes, r.duration)).unwrap_or(0.0);
    let rows: Vec<VtLevel> = levels
        .iter()
        .map(|l| {
            let h = per_hour(l.bytes, r.duration);
            let rel = if l.level == r.current_level {
                lang::t("settings.videoTest.current")
            } else if h > 0.0 && cur_h > 0.0 {
                let f = h / cur_h;
                format!("×{}", fmt_num(f, if f < 1.0 { 2 } else { 1 }))
            } else {
                String::new()
            };
            VtLevel {
                level: l.level,
                name: tw("settings.videoTest.levelN", &[("n", &l.level.to_string())]).into(),
                fill: if h > 0.0 { (h / max).max(0.015) as f32 } else { 0.0 },
                size: if h > 0.0 { tw("settings.videoTest.perHour", &[("size", &fmt_size(h))]) } else { "—".into() }.into(),
                rel: rel.into(),
                current: l.level == r.current_level,
                sel_a: l.level == a,
                sel_b: l.level == b,
            }
        })
        .collect();
    st.set_vt_levels(ModelRc::new(VecModel::from(rows)));
    st.set_vt_tag_options(sel_plain(&(0..=10).rev().map(|n| tw("settings.videoTest.levelN", &[("n", &n.to_string())])).collect::<Vec<_>>()));
    st.set_vt_a(a);
    st.set_vt_b(b);
    st.set_vt_zoom(zoom);
    st.set_vt_swap_label(lang::t("settings.videoTest.swap").into());
    st.set_vt_use_label(tw("settings.videoTest.use", &[("n", &a.to_string())]).into());
    st.set_vt_use_enabled(a >= 0 && a != r.current_level);
    let img = |lv: i32| {
        if let Some(c) = with(|s| s.vt.imgs.get(&lv).cloned()) {
            return Some(c);
        }
        let c = r.levels.iter().find(|l| l.level == lv).and_then(|l| load_png(&l.image))?;
        with(|s| s.vt.imgs.insert(lv, c.clone()));
        Some(c)
    };
    let scale = w.window().scale_factor().max(0.5);
    if let Some((ia, iw, ih)) = img(a) {
        st.set_vt_img_a(ia);
        st.set_vt_img_w(iw as f32 / scale);
        st.set_vt_img_h(ih as f32 / scale);
    }
    if let Some((ib, _, _)) = img(b) {
        st.set_vt_img_b(ib);
    }
}

fn vt_use() {
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
    render_video_sliders();
    render_vt();
    crate::app_view::show_toast(&tw("settings.videoTest.applied", &[("n", &a.to_string())]), "", false, 0);
}

// ---------------------------------------------------------------------------
// Restaurar padrões
// ---------------------------------------------------------------------------

fn restore_defaults() {
    let Some(w) = win() else { return };
    w.global::<crate::app_view::Confirm>().invoke_open(
        lang::t("settings.buttons.reset").into(),
        lang::t("toast.restoreConfirmMessage").into(),
        lang::t("toast.restoreConfirmOk").into(),
        false,
        "merge".into(),
        "settings-restore".into(),
        SharedString::new(),
    );
}

/// O "ok" do diálogo de confirmação. `true` = era nosso.
pub fn on_confirm(action: &str) -> bool {
    if action != "settings-restore" {
        return false;
    }
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
    // principal e fica como está. A lista de programas fica VAZIA.
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
    if let Some(w) = win() {
        fill_all(&w);
    }
    crate::app_view::show_toast(&lang::t("toast.fieldsReset"), &lang::t("toast.fieldsResetBody"), false, 4000);
    true
}

// ---------------------------------------------------------------------------
// Avisos do core
// ---------------------------------------------------------------------------

pub fn on_settings_changed() {
    if let Some(w) = win() {
        if is_open() {
            // Com a tela aberta só a pasta é do core (ele resolve o vazio).
            w.global::<St>().set_record_dir(recording::record_dir().to_string_lossy().into_owned().into());
            render_library_hint();
        }
    }
}

pub fn on_record_dir_picked(path: &str) {
    if let Some(w) = win() {
        w.global::<St>().set_record_dir(path.into());
    }
    settings::handle_set_record_dir(path);
}

pub fn on_encoder_caps() {
    render_codec_options();
}

pub fn on_replay_state(r: &ReplayState) {
    with(|s| s.replay = Some(r.clone()));
    if is_open() {
        render_replay();
    }
}

pub fn on_theme_changed(pref: &str) {
    if let Some(w) = win() {
        w.global::<St>().set_theme_pref(pref.into());
    }
}

pub fn on_language_changed() {
    if let Some(w) = win() {
        if is_open() {
            fill_all(&w);
        }
    }
}

// ---------------------------------------------------------------------------
// Ligações
// ---------------------------------------------------------------------------

pub fn setup(w: &AppWindow) {
    WINDOW.with(|cell| *cell.borrow_mut() = Some(w.as_weak()));
    let st = w.global::<St>();
    st.on_close(close);
    st.on_show_tab(|t| show_tab(t.as_str()));
    st.on_set_theme(|m| {
        bridge::handle_set_theme(m.as_str());
        on_theme_changed(m.as_str());
    });
    st.on_language_selected(|i| {
        let code = if i <= 0 {
            String::new()
        } else {
            lang::available_languages().as_array().and_then(|l| l.get(i as usize - 1)).and_then(|l| l.get("code")).and_then(|c| c.as_str()).unwrap_or_default().to_owned()
        };
        if let Some(w) = win() {
            w.global::<St>().set_language_index(i);
        }
        bridge::handle_set_language(&code);
    });
    st.on_window_title_committed(|t| {
        let t = t.trim().to_owned();
        if t != config::get_config_str("windowTitle", "NoOBS") {
            shell::handle_set_window_title(&t);
        }
    });
    st.on_check_changed(|key, on| check_changed(key.as_str(), on));
    st.on_restore_defaults(restore_defaults);

    // Arquivos.
    st.on_pick_folder(settings::handle_pick_record_dir);
    st.on_record_dir_committed(|p| {
        let p = p.trim().to_owned();
        if p != recording::record_dir().to_string_lossy() {
            settings::handle_set_record_dir(&p);
        }
    });
    st.on_filename_edited(|t| {
        if let Some(w) = win() {
            w.global::<St>().set_filename_pattern(t);
        }
        render_filename_preview();
    });
    st.on_filename_committed(|t| {
        let t = t.trim().to_owned();
        if t != config::get_config_str("filenamePattern", settings::DEFAULT_FILENAME_PATTERN) {
            settings::handle_set_filename_pattern(&t);
        }
    });
    st.on_library_selected(|i| {
        if let Some(w) = win() {
            w.global::<St>().set_library_index(i);
        }
        settings::handle_set_library_thumbs(LIBRARY_MODES.get(i as usize).copied().unwrap_or("auto"));
        render_library_hint();
    });
    st.on_folder_hidden(|name, hidden| btr::handle_set_folder_hidden(name.as_str(), hidden));

    // Vídeo.
    st.on_codec_selected(|i| {
        if let Some(w) = win() {
            w.global::<St>().set_codec_index(i);
        }
        settings::handle_set_codec(CODECS.get(i as usize).copied().unwrap_or("auto"));
        render_codec_hints();
    });
    st.on_quality_moved(|v| {
        with(|s| s.quality = v.clamp(0, 10));
        render_video_sliders();
    });
    st.on_quality_released(|| settings::handle_set_recording_quality(with(|s| s.quality)));
    st.on_fps_moved(|i| {
        with(|s| s.fps = s.fps_presets.get(i as usize).copied().unwrap_or(s.fps));
        render_video_sliders();
    });
    st.on_fps_released(|| settings::handle_set_recording_fps(with(|s| s.fps)));
    st.on_keyframe_moved(|v| {
        with(|s| s.keyframe = v.clamp(1, 10));
        render_video_sliders();
    });
    st.on_keyframe_released(|| settings::handle_set_recording_keyframe(with(|s| s.keyframe)));
    st.on_vt_start(|| {
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
        render_vt();
        video_test::handle_test_video_quality();
    });
    st.on_vt_cancel(video_test::handle_cancel_video_test);
    st.on_vt_pick(|level, b| {
        with(|s| if b { s.vt.b = level } else { s.vt.a = level });
        render_vt();
    });
    st.on_vt_tag(|level, b| {
        with(|s| if b { s.vt.b = level } else { s.vt.a = level });
        render_vt();
    });
    st.on_vt_swap(|| {
        with(|s| std::mem::swap(&mut s.vt.a, &mut s.vt.b));
        render_vt();
    });
    st.on_vt_zoom_set(|z| {
        with(|s| s.vt.zoom = z);
        render_vt();
    });
    st.on_vt_use(vt_use);

    // Áudio.
    st.on_audio_bitrate_selected(|i| {
        let k = AUDIO_BITRATES.get(i as usize).copied().unwrap_or(AUDIO_BITRATE_DEFAULT);
        if let Some(w) = win() {
            w.global::<St>().set_audio_bitrate_index(i);
        }
        render_audio_hint(k);
        settings::handle_set_audio_bitrate(k);
    });

    // Buffer.
    st.on_replay_sec_selected(|i| {
        if let Some(w) = win() {
            w.global::<St>().set_replay_sec_index(i);
        }
        replay_limits_changed();
    });
    st.on_replay_mb_selected(|i| {
        if let Some(w) = win() {
            w.global::<St>().set_replay_mb_index(i);
        }
        replay_limits_changed();
    });
    // Cada envio reinicia o watcher no core: com atraso enquanto digita, e já
    // ao sair do campo (Esc fecharia sem salvar).
    st.on_replay_apps_edited(|t| {
        let t = t.to_string();
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::SingleShot, Duration::from_millis(600), move || {
            with(|s| s.apps_timer = None);
            replay::handle_set_replay_apps(&t);
        });
        with(|s| s.apps_timer = Some(timer));
    });
    st.on_replay_apps_committed(|t| {
        with(|s| s.apps_timer = None);
        let cur = with(|s| s.replay.as_ref().map(|r| r.apps.clone())).unwrap_or_default();
        if t.as_str() != cur {
            replay::handle_set_replay_apps(t.as_str());
        }
    });
    st.on_replay_corner_selected(|i| {
        if let Some(w) = win() {
            w.global::<St>().set_replay_corner(i);
        }
        replay::handle_set_replay_indicator_corner(CORNERS.get(i as usize).copied().unwrap_or("top-right"));
    });
    st.on_replay_opacity_moved(|v| {
        if let Some(w) = win() {
            w.global::<St>().set_replay_opacity(v);
        }
        replay::handle_set_replay_indicator_opacity(v);
    });

    // Atalhos.
    st.on_hk_mode(|i, m| {
        let mode = HK_MODES.get(m as usize).copied().unwrap_or(Mode::Record);
        hk_update(i as usize, |it| {
            let hk = it.hotkey.clone();
            let delay = it.delay_sec;
            *it = Item::new(mode, &hk);
            it.hotkey = hk;
            if mode != Mode::Replay {
                it.delay_sec = delay;
            }
        });
    });
    st.on_hk_mod(|i, m, on| {
        with(|s| {
            if let Some(r) = s.hk_rows.get_mut(i as usize) {
                match m.as_str() {
                    "Ctrl" => r.ctrl = on,
                    "Shift" => r.shift = on,
                    "Alt" => r.alt = on,
                    _ => r.win = on,
                }
            }
        });
        hk_combo_changed(i as usize);
    });
    st.on_hk_key(|i, k| {
        with(|s| {
            if let Some(r) = s.hk_rows.get_mut(i as usize) {
                r.key = k;
            }
        });
        hk_combo_changed(i as usize);
    });
    st.on_hk_title(|i, t| {
        let t = t.trim().to_owned();
        if with(|s| s.hk_items.get(i as usize).map(|it| it.title.clone())).unwrap_or_default() != t {
            hk_update(i as usize, |it| it.title = t);
        }
    });
    st.on_hk_app_audio(|i, on| hk_update(i as usize, |it| it.app_audio = on));
    st.on_hk_define_area(|i| bhk::handle_define_area(i.max(0) as usize));
    st.on_hk_clear_area(|i| hk_update(i as usize, |it| it.area = None));
    st.on_hk_delay(|i, d| {
        let v = with(|s| s.hk_delays.get(d as usize).copied().unwrap_or(0));
        hk_update(i as usize, |it| it.delay_sec = v);
    });
    st.on_hk_remove(|i| {
        let mut items = with(|s| s.hk_items.clone());
        if (i as usize) < items.len() {
            items.remove(i as usize);
            hk_send(items);
        }
    });
    st.on_hk_add(|| {
        let mut items = with(|s| s.hk_items.clone());
        items.push(Item::new(Mode::Record, ""));
        hk_send(items);
    });

    // Dispositivos.
    st.on_dev_toggled(|id, on| {
        set_device_flag(id.as_str(), |d| d.hidden = !on);
        render_devices();
        render_auto_devices();
        crate::bridge::devices::handle_set_source_hidden(id.as_str(), !on);
    });
    st.on_auto_dev_toggled(|id, on| {
        set_device_flag(id.as_str(), |d| d.auto = on);
        render_auto_devices();
        crate::bridge::devices::handle_set_auto_source(id.as_str(), on);
    });

    // Gravação.
    st.on_rec_corner_selected(|i| {
        if let Some(w) = win() {
            w.global::<St>().set_rec_corner(i);
        }
        recording::handle_set_rec_indicator_corner(CORNERS.get(i as usize).copied().unwrap_or("top-right"));
    });
    st.on_rec_opacity_moved(|v| {
        with(|s| s.rec_opacity = v);
        if let Some(w) = win() {
            w.global::<St>().set_rec_opacity(v);
        }
    });
    st.on_rec_opacity_released(|| recording::handle_set_rec_indicator_opacity(with(|s| s.rec_opacity)));
    st.on_auto_apps_edited(|t| {
        if let Some(w) = win() {
            w.global::<St>().set_auto_apps(t);
        }
    });
    st.on_auto_apps_committed(|t| {
        let t = t.trim().to_owned();
        if t != config::get_config_str("autoRecordMicApps", "") {
            settings::handle_set_auto_record_mic_apps(&t);
        }
    });
    st.on_auto_except_committed(|t| {
        let t = t.trim().to_owned();
        if t != config::get_config_str("autoRecordMicExcept", "") {
            settings::handle_set_auto_record_mic_except(&t);
        }
    });

    // Transcrição.
    st.on_asr_action(asr_action);
    st.on_tr_lang_selected(|i| {
        if let Some(w) = win() {
            w.global::<St>().set_tr_lang_index(i);
        }
        btr::handle_set_transcribe_language(&transcribe_lang_value(i.max(0) as usize));
    });
    st.on_tr_pending(btr::handle_transcribe_pending);
    st.on_tr_cancel(|| {
        if with(|s| s.tr.canceling || s.tr_cancel_clicked) {
            return;
        }
        with(|s| s.tr_cancel_clicked = true);
        render_transcribe();
        btr::handle_cancel_transcribe();
    });
    st.on_tr_remove(|id| btr::handle_remove_transcribe_item(id.as_str()));
    // A posição é na ESPERA (o item em curso fica fora da conta).
    st.on_tr_move(|id, to_row| {
        let q = with(|s| s.tr_queue.clone());
        let waiting: Vec<&QueueItem> = q.iter().filter(|x| !x.current).collect();
        let Some(target) = q.get(to_row.max(0) as usize) else { return };
        let to = waiting.iter().position(|x| x.id == target.id).unwrap_or(0);
        btr::handle_move_transcribe_item(id.as_str(), to as i32);
    });

    // Atualizações e Sobre.
    st.on_check_now(|| {
        with(|s| s.checking = true);
        render_update();
        settings::handle_check_updates();
    });
    st.on_open_update_page(|| {
        let url = with(|s| s.update.as_ref().map(|u| u.url.clone())).unwrap_or_default();
        if !url.is_empty() {
            bridge::handle_open_url(&url);
        }
    });
    st.on_open_repo(|| bridge::handle_open_url("https://github.com/e-delphi/NoOBS"));
}

fn check_changed(key: &str, on: bool) {
    let Some(w) = win() else { return };
    let st = w.global::<St>();
    match key {
        "autostart" => {
            st.set_autostart(on);
            shell::handle_set_autostart(on);
        }
        "runAsAdmin" => {
            st.set_run_as_admin(on);
            shell::handle_set_run_as_admin(on);
        }
        "closeToTray" => {
            st.set_close_to_tray(on);
            shell::handle_set_close_to_tray(on);
            render_dependent_labels();
        }
        "muteWhenDeviceMuted" => {
            st.set_mute_when_muted(on);
            settings::handle_set_mute_when_device_muted(on);
        }
        "replayAutoStart" => {
            st.set_replay_autostart(on);
            replay::handle_set_replay_autostart(on);
        }
        "replayIntoRecording" => {
            st.set_replay_into_rec(on);
            replay::handle_set_replay_into_recording(on);
        }
        "replayIndicator" => {
            st.set_replay_indicator(on);
            replay::handle_set_replay_indicator(on);
            render_dependent_labels();
        }
        "replayIndicatorClickable" => {
            st.set_replay_clickable(on);
            replay::handle_set_replay_indicator_clickable(on);
        }
        "recIndicator" => {
            st.set_rec_indicator(on);
            recording::handle_set_rec_indicator(on);
            render_dependent_labels();
        }
        "recIndicatorClickable" => {
            st.set_rec_clickable(on);
            recording::handle_set_rec_indicator_clickable(on);
        }
        "fullscreenDetect" => {
            st.set_fullscreen_detect(on);
            recording::handle_set_fullscreen_detect(on);
        }
        "playSoundOnRecord" => {
            st.set_play_sound(on);
            shell::handle_set_config_bool("playSoundOnRecord", on);
        }
        "minimizeOnRecord" => {
            st.set_minimize_on_record(on);
            shell::handle_set_config_bool("minimizeOnRecord", on);
        }
        "autoRecordOnMic" => {
            st.set_auto_record(on);
            settings::handle_set_auto_record_on_mic(on);
            render_auto_devices();
        }
        "stopOnLock" => {
            st.set_stop_on_lock(on);
            shell::handle_set_config_bool("stopOnLock", on);
        }
        "transcribeOnStop" => {
            st.set_tr_on_stop(on);
            btr::handle_set_transcribe_on_stop(on);
        }
        "checkUpdates" => {
            st.set_check_updates(on);
            settings::handle_set_check_updates(on);
        }
        _ => log!("Configuracoes: opcao desconhecida \"{key}\"."),
    }
}

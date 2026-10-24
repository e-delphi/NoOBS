//! Tela principal (coluna de fontes + bloco "Telas"): o que a versão web fazia
//! no `displays.js`, `record.js` e `main.js`. Recebe os eventos do core pelo
//! [`crate::view::View`] (via `app_view::SlintView`), monta os modelos da
//! `.slint` e liga os botões direto nos handlers do core.
//!
//! Só a main thread toca aqui.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, VecModel};

use crate::app_view::{AppWindow, DisplayItem, Hints, LegendItem, Main, SavedArea, SourceItem, WindowCard};
use crate::view::{AudioMeter, Device, MonitorThumb, PendingKind, RecState, ReplayState, WindowItem, WindowMatches};
use crate::{bridge, config, lang, log};

/// Cor de cada faixa isolada (2, 3, …), em ciclo — a `TRACK_PALETTE` da web.
/// Sem verde/vermelho (bolinha de padrão, gravar/parar).
const TRACK_PALETTE: [u32; 10] = [
    0x3b82f6, 0x06b6d4, 0xf59e0b, 0xa855f7, 0xf472b6, 0x6366f1, 0x14b8a6, 0xc2410c, 0x0ea5e9, 0x8b5cf6,
];

fn track_color(t: i32) -> slint::Color {
    let rgb = TRACK_PALETTE[((t - 2).max(0) as usize) % TRACK_PALETTE.len()];
    slint::Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Amplitude linear 0..1 → largura 0..1 em dB (−60 dB = 0, 0 dB = 1): o
/// `ampToBar` da web.
fn amp_to_bar(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    ((20.0 * v.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

#[derive(Default)]
struct State {
    monitors: Vec<Device>,
    webcams: Vec<Device>,
    mics: Vec<Device>,
    speakers: Vec<Device>,
    /// Última prévia de cada monitor (sobrevive a refazer a lista).
    thumbs: HashMap<String, slint::Image>,
    mic_model: Rc<VecModel<SourceItem>>,
    spk_model: Rc<VecModel<SourceItem>>,
    legend_model: Rc<VecModel<LegendItem>>,
    display_model: Rc<VecModel<DisplayItem>>,
    // ---- gravação ----
    /// `None` antes do 1º estado (o som não toca no arranque).
    last_active: Option<bool>,
    rec: RecState,
    /// Quando o cronômetro marcaria zero (`Instant` da "partida").
    timer_start: Option<Instant>,
    timer: Option<slint::Timer>,
    finalizing: bool,
    pause_locked: bool,
    last_stop_sound: Option<Instant>,
    // ---- buffer em memória ----
    replay: ReplayState,
    /// Quando o último estado chegou (base do "guardado" que anda sozinho).
    replay_received: Option<Instant>,
    replay_ticker: Option<slint::Timer>,
    // ---- "Capturar..." ----
    picker: Picker,
    // ---- contagem e onda ----
    countdown_sec: u32,
    /// Quando a contagem marcaria zero e o total dela (o anel esvazia nisso).
    countdown_end: Option<Instant>,
    countdown_total: f32,
    countdown_tick: Option<Instant>,
    countdown_timer: Option<slint::Timer>,
    ripple_timer: Option<slint::Timer>,
}

#[derive(Default)]
struct Picker {
    open: bool,
    area: bool,
    items: Vec<WindowItem>,
    thumbs: HashMap<String, slint::Image>,
    seq: u32,
    search: String,
    loading: bool,
    audio_supported: bool,
    /// Aberto por um atalho: (trecho do título, atraso).
    matches: Option<(String, u32)>,
    model: Rc<VecModel<WindowCard>>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn main_global(w: &AppWindow) -> Main<'_> {
    w.global::<Main>()
}

/// Liga os modelos e os callbacks da tela principal. Uma vez, ao criar a janela.
pub fn setup(w: &AppWindow) {
    let m = main_global(w);
    with(|s| {
        m.set_mics(ModelRc::from(s.mic_model.clone()));
        m.set_speakers(ModelRc::from(s.spk_model.clone()));
        m.set_legend(ModelRc::from(s.legend_model.clone()));
        m.set_displays(ModelRc::from(s.display_model.clone()));
    });
    m.set_status_text(lang::t("record.statusReady").into());

    m.on_toggle_record(on_toggle_record);
    m.on_toggle_pause(on_toggle_pause);
    m.on_toggle_source(|id, enabled| on_toggle_source(id.as_str(), enabled));
    m.on_replay_toggle(|| {
        let on = with(|s| s.replay.enabled);
        bridge::handle_set_replay_enabled(!on);
    });
    m.on_replay_save(|| {
        let (active, saving) = with(|s| (s.replay.active, s.replay.saving));
        if active && !saving {
            bridge::handle_save_replay();
        }
    });
    render_replay(w);
    m.on_open_settings(|| log::log("UI: Configuracoes (tela ainda nao portada)."));
    with(|s| m.set_picker_items(ModelRc::from(s.picker.model.clone())));
    m.on_open_window_picker(open_picker);
    m.on_picker_close(close_picker);
    m.on_picker_refresh(refresh_picker);
    m.on_picker_set_area_mode(|area| set_picker_mode(area, true));
    m.on_picker_search(|t| {
        with(|s| s.picker.search = t.to_string());
        render_picker();
    });
    m.on_picker_pick(|id| pick_window(id.as_str()));
    m.on_picker_audio_changed(|on| {
        config::set_config_bool("windowPickerAppAudio", on);
        if let Some(w) = crate::app_view::window() {
            main_global(&w).set_picker_audio(on);
        }
    });
    m.on_picker_select_area(|| {
        close_picker();
        bridge::handle_select_capture_area();
    });
    m.on_picker_saved_area(|i| {
        close_picker();
        bridge::trigger_hotkey(i.max(0) as usize);
    });
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

fn source_item(d: &Device) -> SourceItem {
    SourceItem {
        id: d.id.as_str().into(),
        name: d.name.as_str().into(),
        enabled: d.enabled,
        is_default: d.is_default,
        bluetooth: d.is_bluetooth,
        track: d.track,
        track_color: track_color(d.track),
        meter_l: 0.0,
        meter_r: 0.0,
        stereo: true,
        muted: false,
    }
}

/// Repõe um modelo de lista mantendo os medidores das linhas que continuam.
fn fill_sources(model: &VecModel<SourceItem>, list: &[Device]) {
    let old: HashMap<String, SourceItem> = model.iter().map(|it| (it.id.to_string(), it)).collect();
    let items: Vec<SourceItem> = visible(list)
        .map(|d| {
            let mut it = source_item(d);
            if let Some(o) = old.get(&d.id) {
                it.meter_l = o.meter_l;
                it.meter_r = o.meter_r;
                it.stereo = o.stereo;
                it.muted = o.muted;
            }
            it
        })
        .collect();
    model.set_vec(items);
}

/// Legenda das faixas (`buildTrackLegend` da web): Padrão, Faixa 1 (mix),
/// faixas com o dispositivo padrão, de microfone, de saída, e "sem faixa".
fn rebuild_legend(s: &State) -> Vec<LegendItem> {
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
    let item = |kind: &str, label: String, name: String, hint: String, color: slint::Color| LegendItem {
        kind: kind.into(),
        track_label: label.into(),
        name: name.into(),
        hint: hint.into(),
        color,
    };
    if any_default {
        out.push(item("default", lang::t("tracks.default"), lang::t("tracks.defaultDevice"), lang::t("tracks.defaultHint"), slint::Color::default()));
    }
    if !tracks.is_empty() {
        out.push(item("mix", lang::t("tracks.mix"), lang::t("tracks.mixLabel"), lang::t("tracks.mixHint"), slint::Color::default()));
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
        out.push(item("off", lang::t("tracks.noTrack"), label, disabled.join(", "), slint::Color::default()));
    }
    out
}

/// Bloco "Telas": monitores na posição real e webcams à direita, na mesma
/// regra do `Displays.render` da web (que é a do canvas da gravação).
fn rebuild_displays(w: &AppWindow) {
    let m = main_global(w);
    with(|s| {
        let mons: Vec<&Device> = visible(&s.monitors).collect();
        let cams: Vec<&Device> = visible(&s.webcams).collect();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for d in &mons {
            min_x = min_x.min(d.x);
            min_y = min_y.min(d.y);
            max_x = max_x.max(d.x + d.width);
            max_y = max_y.max(d.y + d.height);
        }
        if mons.is_empty() {
            (min_x, min_y, max_x, max_y) = (0, 0, 0, 0);
        }
        let mut items: Vec<DisplayItem> = mons
            .iter()
            .map(|d| {
                let thumb = s.thumbs.get(&d.id).cloned();
                DisplayItem {
                    id: d.id.as_str().into(),
                    is_webcam: false,
                    name: d.name.as_str().into(),
                    enabled: d.enabled,
                    x: d.x as f32,
                    y: d.y as f32,
                    w: d.width as f32,
                    h: d.height as f32,
                    has_thumb: thumb.is_some(),
                    thumb: thumb.unwrap_or_default(),
                }
            })
            .collect();
        // Webcams à direita do bloco dos monitores, em y = 0.
        let mut cam_x = max_x;
        for c in &cams {
            let (cw, ch) = (if c.width > 0 { c.width } else { 1280 }, if c.height > 0 { c.height } else { 720 });
            items.push(DisplayItem {
                id: c.id.as_str().into(),
                is_webcam: true,
                name: c.name.as_str().into(),
                enabled: c.enabled,
                x: cam_x as f32,
                y: 0.0,
                w: cw as f32,
                h: ch as f32,
                has_thumb: false,
                thumb: Default::default(),
            });
            cam_x += cw;
            max_y = max_y.max(ch);
        }
        max_x = max_x.max(cam_x);
        m.set_disp_min_x(min_x as f32);
        m.set_disp_min_y(min_y as f32);
        m.set_disp_total_w((max_x - min_x).max(1) as f32);
        m.set_disp_total_h((max_y - min_y).max(1) as f32);
        s.display_model.set_vec(items);

        // Contagem e "3840×2160" da composição marcada (soma das larguras ×
        // maior altura), com o aviso se passar do limite do encoder.
        let total = mons.len() + cams.len();
        let sel = mons.iter().chain(cams.iter()).filter(|d| d.enabled).count();
        m.set_mon_count(format!("{sel} / {total}").into());
        let (mut cw, mut chh) = (0, 0);
        for d in mons.iter().chain(cams.iter()).filter(|d| d.enabled) {
            cw += d.width;
            chh = chh.max(d.height);
        }
        if cw > 0 && chh > 0 {
            let max = codec_max_dim();
            m.set_mon_meta(format!("{cw}×{chh}").into());
            let over = cw > max || chh > max;
            m.set_mon_meta_warn(over);
            if over {
                m.set_mon_meta_warn_hint(lang::t_with("warning.canvasOverflow", &[("max", &max.to_string())]).into());
            }
        } else {
            m.set_mon_meta("".into());
            m.set_mon_meta_warn(false);
        }
    });
}

/// Maior dimensão do encoder escolhido (o `codecMaxDim` da web, pegadinha #7):
/// H.264 por hardware para em 4096; no automático, vale o H.264 por hardware
/// se a máquina tiver.
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

fn refresh_sources(w: &AppWindow) {
    let m = main_global(w);
    let legend = with(|s| {
        fill_sources(&s.mic_model, &s.mics);
        fill_sources(&s.spk_model, &s.speakers);
        m.set_mic_count(count_text(&s.mics).into());
        m.set_spk_count(count_text(&s.speakers).into());
        rebuild_legend(s)
    });
    with(|s| s.legend_model.set_vec(legend));
}

fn update_can_record(w: &AppWindow) {
    let any = with(|s| {
        [&s.monitors, &s.webcams, &s.mics, &s.speakers].iter().any(|l| visible(l).any(|d| d.enabled))
    });
    main_global(w).set_can_record(any);
}

/// "Microfone adicionado: X · Saída removida: Y · e mais N" (`formatDeviceChanges`).
fn format_changes(changes: &[String]) -> String {
    const MAX: usize = 3;
    if changes.len() <= MAX {
        return changes.join(" · ");
    }
    format!("{} · {}", changes[..MAX].join(" · "), lang::t_with("toast.andMore", &[("count", &(changes.len() - MAX).to_string())]))
}

pub fn on_monitors(w: &AppWindow, list: &[Device], changes: &[String]) {
    with(|s| s.monitors = list.to_vec());
    rebuild_displays(w);
    update_can_record(w);
    main_global(w).set_monitor_pending(false);
    if !changes.is_empty() {
        crate::app_view::show_toast(&lang::t("toast.monitorsUpdated"), &format_changes(changes), true, 4500);
    }
}

pub fn on_webcams(w: &AppWindow, list: &[Device], changes: &[String]) {
    with(|s| s.webcams = list.to_vec());
    rebuild_displays(w);
    update_can_record(w);
    // Mesmo título do áudio: um hot-plug USB que mexe nos dois vira um aviso só.
    if !changes.is_empty() {
        crate::app_view::show_toast(&lang::t("toast.devicesUpdated"), &format_changes(changes), true, 4500);
    }
}

pub fn on_audio_sources(w: &AppWindow, mics: &[Device], speakers: &[Device], silent: bool, changes: &[String]) {
    with(|s| {
        s.mics = mics.to_vec();
        s.speakers = speakers.to_vec();
    });
    refresh_sources(w);
    update_can_record(w);
    main_global(w).set_audio_pending(false);
    if !silent {
        crate::app_view::show_toast(&lang::t("toast.devicesUpdated"), &format_changes(changes), true, 4500);
    }
}

pub fn on_audio_meters(items: &[AudioMeter]) {
    with(|s| {
        for model in [&s.mic_model, &s.spk_model] {
            for i in 0..model.row_count() {
                let Some(mut it) = model.row_data(i) else { continue };
                let Some(m) = items.iter().find(|m| m.id == it.id.as_str()) else { continue };
                it.stereo = m.channels >= 2;
                it.muted = m.muted;
                // Mudo: a barra vai CHEIA (vermelha) — num endpoint mudo o nível é
                // zero de verdade e não há como recuperá-lo (pegadinha #59).
                it.meter_l = if m.muted { 1.0 } else { amp_to_bar(m.left) };
                it.meter_r = if m.muted { 1.0 } else { amp_to_bar(m.right) };
                model.set_row_data(i, it);
            }
        }
    });
}

pub fn on_monitor_thumbs(w: &AppWindow, items: &[MonitorThumb]) {
    with(|s| {
        for t in items {
            if t.rgb.is_empty() {
                s.thumbs.remove(&t.id);
            } else if t.rgb.len() == (t.width * t.height * 3) as usize {
                let buf = SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(&t.rgb, t.width, t.height);
                s.thumbs.insert(t.id.clone(), slint::Image::from_rgb8(buf));
            }
        }
        // Só troca a imagem nas linhas existentes (sem refazer a lista).
        for i in 0..s.display_model.row_count() {
            let Some(mut it) = s.display_model.row_data(i) else { continue };
            if it.is_webcam {
                continue;
            }
            let thumb = s.thumbs.get(it.id.as_str()).cloned();
            it.has_thumb = thumb.is_some();
            it.thumb = thumb.unwrap_or_default();
            s.display_model.set_row_data(i, it);
        }
    });
    let _ = w;
}

pub fn on_device_change_pending(w: &AppWindow, kind: PendingKind, pending: bool) {
    let m = main_global(w);
    match kind {
        PendingKind::Monitors => m.set_monitor_pending(pending),
        PendingKind::Audio => m.set_audio_pending(pending),
    }
}

pub fn on_refresh_busy(w: &AppWindow, busy: bool, what: &str) {
    let m = main_global(w);
    m.set_busy(busy);
    if busy {
        let key = match what {
            "monitors" => "toast.updatingMonitors",
            "audio" => "toast.updatingAudio",
            "starting" => "toast.startingRecord",
            _ => "toast.updating",
        };
        m.set_busy_text(lang::t(key).into());
    }
}

/// Liga/desliga um dispositivo: a tela muda na hora (a web também era
/// otimista); o core grava a preferência e, no áudio, recalcula as faixas e
/// manda a lista de volta.
fn on_toggle_source(id: &str, enabled: bool) {
    let Some(w) = crate::app_view::window() else { return };
    with(|s| {
        for list in [&mut s.monitors, &mut s.webcams, &mut s.mics, &mut s.speakers] {
            if let Some(d) = list.iter_mut().find(|d| d.id == id) {
                d.enabled = enabled;
            }
        }
    });
    rebuild_displays(&w);
    refresh_sources(&w);
    update_can_record(&w);
    bridge::handle_toggle_source(id, enabled);
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

/// "HH:MM:SS" e ".cc" (centésimos) do cronômetro.
fn timer_text(ms: u64) -> (String, String) {
    let s = ms / 1000;
    (format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60), format!(".{:02}", ms % 1000 / 10))
}

fn render_timer(w: &AppWindow, ms: u64) {
    let (a, b) = timer_text(ms);
    let m = main_global(w);
    m.set_timer_main(a.into());
    m.set_timer_ms(b.into());
}

/// Cronômetro a 50 ms (o `setInterval` da web), da "partida" pra frente.
fn start_timer(w: &AppWindow, elapsed_ms: u64) {
    let start = Instant::now() - Duration::from_millis(elapsed_ms);
    render_timer(w, elapsed_ms);
    let weak = w.as_weak();
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
        if let Some(w) = weak.upgrade() {
            render_timer(&w, start.elapsed().as_millis() as u64);
        }
    });
    with(|s| {
        s.timer_start = Some(start);
        s.timer = Some(t);
    });
}

fn stop_timer() {
    with(|s| {
        s.timer = None;
        s.timer_start = None;
    });
}

pub fn on_recording_state(w: &AppWindow, st: &RecState) {
    let m = main_global(w);
    // Som de início: só na virada pra gravando (não no 1º estado do arranque).
    let prev = with(|s| s.last_active.replace(st.active));
    if prev == Some(false) && st.active && config::get_config_bool("playSoundOnRecord", true) {
        crate::sounds::play_start();
    }
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
    let (paused, window, size) = with(|s| (s.rec.paused, s.rec.window.clone(), s.rec.size_text.clone().unwrap_or_default()));
    m.set_recording(st.active);
    m.set_paused(paused);
    m.set_size_text(size.into());
    if st.active {
        let status = if paused {
            lang::t("record.statusPaused")
        } else if !window.is_empty() {
            lang::t_with("record.statusRecordingWindow", &[("title", &window)])
        } else {
            lang::t("record.statusRecording")
        };
        m.set_status_text(status.into());
        // Pausada: o cronômetro para no tempo gravado (o core manda o tempo já
        // congelado). Gravando: anda a partir do tempo que o core mandou.
        let ms = st.elapsed_ms.unwrap_or(0);
        if paused {
            stop_timer();
            render_timer(w, ms);
        } else if st.elapsed_ms.is_some() || with(|s| s.timer.is_none()) {
            start_timer(w, ms);
        }
    } else {
        let finalizing = with(|s| s.finalizing);
        m.set_status_text(lang::t(if finalizing { "record.finalizing" } else { "record.statusReady" }).into());
        stop_timer();
        render_timer(w, 0);
    }
}

pub fn on_recording_finalizing(w: &AppWindow, active: bool) {
    with(|s| s.finalizing = active);
    let m = main_global(w);
    m.set_finalizing(active);
    if !m.get_recording() {
        m.set_status_text(lang::t(if active { "record.finalizing" } else { "record.statusReady" }).into());
    }
}

/// Som de parada, com o mesmo debounce de 800 ms da web: o clique toca na
/// hora e o aviso do core chega logo depois.
pub fn play_stop_sound() {
    let now = Instant::now();
    let recent = with(|s| s.last_stop_sound.replace(now)).is_some_and(|t| now.duration_since(t) < Duration::from_millis(800));
    if !recent {
        crate::sounds::play_stop();
    }
}

fn on_toggle_record() {
    let recording = with(|s| s.rec.active);
    if recording {
        // Som de parada AGORA (o arquivo pode levar centenas de ms pra fechar).
        if config::get_config_bool("playSoundOnRecord", true) {
            play_stop_sound();
        }
        bridge::handle_record_stop();
    } else {
        // A onda só no começo: parar já se vê pelo fim do pulso vermelho.
        if with(|s| s.countdown_sec) == 0 {
            start_ripple();
        }
        bridge::handle_record_start_button();
    }
}

/// `cubic-bezier(x1, y1, x2, y2)` em `t` (0..1): acha o parâmetro da curva
/// cujo x é `t` (bissecção) e devolve o y.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let b = |a: f32, c: f32, u: f32| 3.0 * a * u * (1.0 - u).powi(2) + 3.0 * c * u * u * (1.0 - u) + u.powi(3);
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if b(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    b(y1, y2, (lo + hi) / 2.0)
}

/// Onda vermelha que sai do botão de gravar (`triggerRecordRipple` da web):
/// 1,8 s com `cubic-bezier(0.16, 1, 0.3, 1)`.
fn start_ripple() {
    let start = Instant::now();
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(w) = crate::app_view::window() else { return };
        let k = (start.elapsed().as_secs_f32() / 1.8).min(1.0);
        if k >= 1.0 {
            main_global(&w).set_ripple(-1.0);
            with(|s| s.ripple_timer = None);
        } else {
            main_global(&w).set_ripple(cubic_bezier(0.16, 1.0, 0.3, 1.0, k));
        }
    });
    with(|s| s.ripple_timer = Some(t));
}

// ---------------------------------------------------------------------------
// Contagem do atraso (`AreaCapture.onCountdown` da web)
// ---------------------------------------------------------------------------

pub fn on_record_countdown(w: &AppWindow, sec: u32, total: u32) {
    let m = main_global(w);
    let prev = with(|s| std::mem::replace(&mut s.countdown_sec, sec));
    if sec == 0 {
        with(|s| {
            s.countdown_end = None;
            s.countdown_timer = None;
        });
        m.set_countdown_sec(0);
        if !with(|s| s.rec.active) {
            m.set_status_text(lang::t("record.statusReady").into());
        }
        return;
    }
    let now = Instant::now();
    // Contagem nova (ou que voltou a subir): o anel recomeça do ponto certo.
    if prev == 0 || sec > prev || with(|s| s.countdown_end.is_none()) {
        let tot = total.max(sec) as f32;
        with(|s| {
            s.countdown_total = tot;
            s.countdown_end = Some(now + Duration::from_secs(sec as u64));
        });
    }
    with(|s| s.countdown_tick = Some(now));
    m.set_countdown_sec(sec as i32);
    m.set_status_text(lang::t_with("record.countdown", &[("sec", &sec.to_string())]).into());
    if with(|s| s.countdown_timer.is_none()) {
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, Duration::from_millis(16), || {
            let Some(w) = crate::app_view::window() else { return };
            let (end, total, tick) = with(|s| (s.countdown_end, s.countdown_total, s.countdown_tick));
            let m = main_global(&w);
            if let Some(end) = end {
                let left = end.saturating_duration_since(Instant::now()).as_secs_f32();
                m.set_countdown_frac(if total > 0.0 { left / total } else { 0.0 });
            }
            if let Some(t) = tick {
                m.set_countdown_tick_age(t.elapsed().as_secs_f32());
            }
        });
        with(|s| s.countdown_timer = Some(t));
    }
}

// ---------------------------------------------------------------------------
// "Capturar..." (`windowpicker.js`)
// ---------------------------------------------------------------------------

fn picker_subtitle(p: &Picker) -> String {
    if let Some((pattern, _)) = &p.matches {
        lang::t_with("windowPicker.matchesSubtitle", &[("title", pattern)])
    } else if p.area {
        lang::t("windowPicker.areaSubtitle")
    } else {
        lang::t("windowPicker.subtitle")
    }
}

/// Grade filtrada pela busca (título ou exe), o texto de vazio e o resto do
/// estado do seletor.
fn render_picker() {
    let Some(w) = crate::app_view::window() else { return };
    let m = main_global(&w);
    with(|s| {
        let p = &s.picker;
        let q = p.search.trim().to_lowercase();
        let cards: Vec<WindowCard> = p
            .items
            .iter()
            .filter(|it| q.is_empty() || it.title.to_lowercase().contains(&q) || it.exe.to_lowercase().contains(&q))
            .map(|it| {
                let thumb = p.thumbs.get(&it.id).cloned();
                let size = if it.w > 0 && it.h > 0 { format!(" · {}×{}", it.w, it.h) } else { String::new() };
                WindowCard {
                    id: it.id.as_str().into(),
                    title: it.title.as_str().into(),
                    info: format!("{}{size}", it.exe).into(),
                    has_thumb: thumb.is_some(),
                    thumb: thumb.unwrap_or_default(),
                }
            })
            .collect();
        m.set_picker_empty_text(lang::t(if p.items.is_empty() { "windowPicker.empty" } else { "windowPicker.noMatch" }).into());
        p.model.set_vec(cards);
        m.set_picker_loading(p.loading);
        m.set_picker_area_mode(p.area);
        m.set_picker_matches(p.matches.is_some());
        m.set_picker_subtitle(picker_subtitle(p).into());
        m.set_picker_audio_supported(p.audio_supported);
    });
    let saved: Vec<SavedArea> = bridge::saved_areas()
        .into_iter()
        .map(|(i, aw, ah, hk)| {
            let mut label = lang::t_with("windowPicker.savedArea", &[("w", &aw.to_string()), ("h", &ah.to_string())]);
            if !hk.is_empty() {
                label = format!("{label} · {hk}");
            }
            SavedArea { index: i as i32, label: label.into() }
        })
        .collect();
    m.set_picker_saved_areas(ModelRc::from(Rc::new(VecModel::from(saved))));
}

fn open_picker() {
    if with(|s| s.rec.active) {
        return;
    }
    let Some(w) = crate::app_view::window() else { return };
    let area = config::get_config_str("windowPickerMode", "window") == "area";
    with(|s| {
        s.picker = Picker { model: s.picker.model.clone(), seq: s.picker.seq, audio_supported: s.picker.audio_supported, ..Default::default() };
        s.picker.open = true;
    });
    let m = main_global(&w);
    m.set_picker_search_text("".into());
    m.set_picker_audio(config::get_config_bool("windowPickerAppAudio", false));
    m.set_picker_open(true);
    set_hint_layer(&w, 1);
    set_picker_mode(area, false);
}

/// Troca de aba. A lista de janelas só é pedida quando a aba Janela aparece
/// (abrir direto na Área não gasta o PrintWindow de todas as janelas à toa).
fn set_picker_mode(area: bool, remember: bool) {
    let matches = with(|s| {
        s.picker.area = area;
        s.picker.matches.is_some()
    });
    if remember && !matches {
        config::set_config_str("windowPickerMode", if area { "area" } else { "window" });
    }
    let need_list = with(|s| !area && s.picker.items.is_empty() && !s.picker.loading && s.picker.matches.is_none());
    if need_list {
        refresh_picker();
    } else {
        render_picker();
    }
}

fn refresh_picker() {
    with(|s| {
        s.picker.loading = true;
        s.picker.items.clear();
        s.picker.thumbs.clear();
    });
    render_picker();
    bridge::handle_list_capture_windows();
}

/// Camada de dicas em vigor (`Hints.top-layer`): ao abrir um modal, a dica
/// que estava aberta embaixo some e as de baixo param de aparecer.
pub(crate) fn set_hint_layer(w: &AppWindow, layer: i32) {
    let h = w.global::<Hints>();
    h.set_top_layer(layer);
    h.set_visible(false);
}

fn close_picker() {
    with(|s| {
        s.picker.open = false;
        s.picker.matches = None;
    });
    if let Some(w) = crate::app_view::window() {
        main_global(&w).set_picker_open(false);
        set_hint_layer(&w, 0);
    }
}

fn pick_window(id: &str) {
    if id.is_empty() {
        return;
    }
    let (delay, supported) = with(|s| (s.picker.matches.as_ref().map_or(0, |m| m.1), s.picker.audio_supported));
    close_picker();
    start_ripple();
    let app_audio = supported && config::get_config_bool("windowPickerAppAudio", false);
    bridge::handle_record_window(id, app_audio, delay);
}

pub fn on_capture_windows(seq: u32, items: &[WindowItem], app_audio: bool) {
    with(|s| {
        s.picker.seq = seq;
        s.picker.items = items.to_vec();
        s.picker.thumbs.clear();
        s.picker.loading = false;
        s.picker.audio_supported = app_audio;
    });
    render_picker();
}

pub fn on_capture_window_thumb(seq: u32, id: &str, width: u32, height: u32, rgb: &[u8]) {
    if with(|s| s.picker.seq) != seq || rgb.len() != (width * height * 3) as usize {
        return;
    }
    let img = slint::Image::from_rgb8(SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(rgb, width, height));
    // Só troca a imagem do cartão (refazer a grade faria o hover piscar).
    with(|s| {
        s.picker.thumbs.insert(id.to_owned(), img.clone());
        let model = &s.picker.model;
        for i in 0..model.row_count() {
            if let Some(mut c) = model.row_data(i) {
                if c.id.as_str() == id {
                    c.thumb = img.clone();
                    c.has_thumb = true;
                    model.set_row_data(i, c);
                }
            }
        }
    });
}

/// Atalho de janela com várias janelas casando o título: o seletor abre só
/// com elas, sem abas, e o atraso do atalho conta depois da escolha.
pub fn on_window_picker_matches(m: &WindowMatches) {
    let Some(w) = crate::app_view::window() else { return };
    with(|s| {
        s.picker = Picker { model: s.picker.model.clone(), ..Default::default() };
        s.picker.open = true;
        s.picker.seq = m.seq;
        s.picker.items = m.items.clone();
        s.picker.audio_supported = m.app_audio;
        s.picker.matches = Some((m.pattern.clone(), m.delay_sec));
    });
    let g = main_global(&w);
    g.set_picker_search_text("".into());
    g.set_picker_audio(m.want_app_audio);
    g.set_picker_open(true);
    set_hint_layer(&w, 1);
    render_picker();
}

/// Trava até o próximo estado: dois cliques rápidos cruzariam pausa e retomada.
fn on_toggle_pause() {
    let (active, paused, locked) = with(|s| (s.rec.active, s.rec.paused, s.pause_locked));
    if !active || locked {
        return;
    }
    with(|s| s.pause_locked = true);
    bridge::handle_record_pause(!paused);
}

/// Troca de idioma: os textos montados aqui (estado, legenda) são refeitos.
pub fn on_language_changed(w: &AppWindow) {
    let st = with(|s| s.rec.clone());
    on_recording_state(w, &RecState { elapsed_ms: None, ..st });
    refresh_sources(w);
    rebuild_displays(w);
    render_replay(w);
}

// ---------------------------------------------------------------------------
// Buffer em memória (`replay.js`)
// ---------------------------------------------------------------------------

/// "m:ss".
fn fmt_min_sec(sec: i64) -> String {
    format!("{}:{:02}", sec / 60, sec % 60)
}

/// Texto e botões da linha do buffer. O "guardado" anda sozinho entre um
/// estado e outro (o core só manda quando algo muda), limitado ao teto de
/// tempo — o de memória a tela não tem como saber.
fn render_replay(w: &AppWindow) {
    let m = main_global(w);
    let (st, received) = with(|s| (s.replay.clone(), s.replay_received));
    let recording = with(|s| s.rec.active);
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
    m.set_replay_on(st.enabled);
    m.set_replay_active(st.active);
    m.set_replay_saving(st.saving);
    m.set_replay_status(text.into());
    let hint = if st.hotkey.is_empty() {
        lang::t("replay.saveHintNoKey")
    } else {
        lang::t_with("replay.saveHint", &[("spec", &st.hotkey)])
    };
    m.set_replay_save_hint(hint.into());
}

pub fn on_replay_state(w: &AppWindow, st: &ReplayState) {
    with(|s| {
        s.replay = st.clone();
        s.replay_received = Some(Instant::now());
    });
    render_replay(w);
    let ticking = with(|s| s.replay_ticker.is_some());
    if st.active && !ticking {
        let weak = w.as_weak();
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            if let Some(w) = weak.upgrade() {
                render_replay(&w);
            }
        });
        with(|s| s.replay_ticker = Some(t));
    } else if !st.active && ticking {
        with(|s| s.replay_ticker = None);
    }
}

pub fn on_replay_saved(name: &str, duration_sec: i32) {
    crate::app_view::show_toast(
        &lang::t("replay.saved"),
        &lang::t_with("replay.savedBody", &[("name", name), ("dur", &fmt_min_sec(duration_sec.max(0) as i64))]),
        false,
        5000,
    );
}

/// O buffer recomeçou (qualidade/fps/codec/áudio mudou): sem o aviso, o
/// "guardado" voltando a zero pareceria defeito.
pub fn on_replay_restarted() {
    crate::app_view::show_toast(&lang::t("replay.restarted"), &lang::t("replay.restartedBody"), false, 6000);
}

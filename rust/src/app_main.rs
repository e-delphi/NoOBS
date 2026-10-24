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

use crate::app_view::{AppWindow, DisplayItem, LegendItem, Main, SourceItem};
use crate::view::{AudioMeter, Device, MonitorThumb, PendingKind, RecState};
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
    m.on_open_settings(|| log::log("UI: Configuracoes (tela ainda nao portada)."));
    m.on_open_window_picker(|| log::log("UI: seletor de janela (fase 1b)."));
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
        bridge::handle_record_start_button();
    }
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
}

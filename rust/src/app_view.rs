//! A janela no iron: implementa o [`View`] (core → interface) e liga os eventos
//! dos forms direto nas funções do core (interface → core), sem JSON
//! (docs/ui-iron.md).
//!
//! O `Ui` do iron pertence ao laço de eventos: o core roda na main thread, mas
//! fora de um evento do iron, e não o recebe. Cada chamada do `View` vira uma
//! função na fila ([`with_ui`]) que o iron roda logo em seguida, com o `Ui`, na
//! ordem em que chegou.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use iron::prelude::*;

use crate::view::{
    AudioMeter, Device, ExportEncoderInfo, Library, LocalAsrInfo, MonitorThumb, PendingKind, QueueItem, RecItem, RecState, ReplayState,
    RootFolder, TranscribeState, Transcript, UpdateInfo, VideoInfo, VideoTestState, View, WindowItem, WindowMatches,
};
use crate::{app_main, app_settings, lang, log};

/// Tempo de vida padrão de um aviso (o `ttl` do `Toast.show` da web).
const TOAST_TTL_MS: u32 = 6000;

/// Os temas do NoOBS (`themes/` do projeto): o escuro e o claro.
const THEME_DARK: &str = include_str!("../../themes/noobs-dark.json");
const THEME_LIGHT: &str = include_str!("../../themes/noobs-light.json");

type UiJob = Box<dyn FnOnce(&mut Ui)>;

thread_local! {
    static PENDING: RefCell<Vec<UiJob>> = const { RefCell::new(Vec::new()) };
    static POSTER: RefCell<Option<Poster>> = const { RefCell::new(None) };
    static MAIN_FORM: Cell<Option<ComponentId>> = const { Cell::new(None) };
}

/// Roda `f` com o `Ui`, na fila da interface (logo depois do que estiver
/// rodando agora). Só da main thread; as workers chegam pelo `view::call`.
/// Saída do motor de vídeo para a tela: os quadros vão para uma vaga que guarda só o
/// mais recente, e a interface é acordada só quando a vaga estava vazia — o motor
/// entrega todos, nada se acumula no laço e nenhum quadro se perde porque a
/// interface estava pintando. O resto (`Info`, `Ended`, `Error`) vai na ordem.
pub fn frame_sink(id: String, apply: fn(&mut Ui, &str, crate::playback::Out)) -> std::sync::Arc<dyn Fn(crate::playback::Out) + Send + Sync> {
    use crate::playback::Out;
    let slot: std::sync::Arc<std::sync::Mutex<Option<Out>>> = std::sync::Arc::default();
    let poster = poster();
    std::sync::Arc::new(move |o: Out| {
        let Some(p) = poster.as_ref() else { return };
        let id = id.clone();
        if matches!(o, Out::Frame { .. }) {
            let was_empty = slot.lock().map(|mut s| s.replace(o).is_none()).unwrap_or(false);
            if was_empty {
                let slot = slot.clone();
                p.post(move |ui| {
                    let frame = slot.lock().ok().and_then(|mut s| s.take());
                    if let Some(f) = frame {
                        apply(ui, &id, f);
                    }
                });
            }
        } else {
            p.post(move |ui| apply(ui, &id, o));
        }
    })
}

/// A fila da interface do iron, para mandar trabalho direto de outra thread (os
/// quadros do motor de vídeo: um salto só até a tela).
pub fn poster() -> Option<Poster> {
    POSTER.with(|p| p.borrow().clone())
}

pub fn with_ui(f: impl FnOnce(&mut Ui) + 'static) {
    PENDING.with(|p| p.borrow_mut().push(Box::new(f)));
    POSTER.with(|p| {
        if let Some(p) = p.borrow().as_ref() {
            p.post(run_pending);
        }
    });
}

/// Esvazia a fila (o que uma função enfileirar entra na mesma volta).
fn run_pending(ui: &mut Ui) {
    loop {
        let jobs = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
        if jobs.is_empty() {
            break;
        }
        for job in jobs {
            job(ui);
        }
    }
}

/// O form da janela principal (`frmMain`), depois do [`create`].
pub fn main_form() -> Option<ComponentId> {
    MAIN_FORM.with(Cell::get)
}

/// Troca o tema inteiro (o escuro ou o claro do NoOBS).
fn apply_theme(ui: &mut Ui, dark: bool) {
    match iron::json::load_theme(if dark { THEME_DARK } else { THEME_LIGHT }) {
        Ok(theme) => ui.set_theme(theme),
        Err(e) => log!("UI: tema do NoOBS inválido: {e}"),
    }
}

/// Um aviso no canto de cima à direita (âmbar nos avisos). Um aviso com o
/// MESMO título já na tela só renova o prazo (o hot-plug de USB dispara o
/// refresh de áudio e o de webcam juntos).
pub fn show_toast(title: &str, msg: &str, warn: bool, ttl_ms: u32) {
    let (title, msg) = (title.to_owned(), msg.to_owned());
    let ttl = if ttl_ms == 0 { TOAST_TTL_MS } else { ttl_ms };
    with_ui(move |ui| {
        notify(ui, &title, &msg, if warn { ToastKind::Warning } else { ToastKind::Info }, ttl);
    });
}

pub struct IronView;

impl View for IronView {
    fn theme(&self, dark: bool, _pref: &str) {
        with_ui(move |ui| apply_theme(ui, dark));
    }
    fn language_changed(&self, _language: &str, _pref: &str) {
        with_ui(app_main::on_language_changed);
        with_ui(crate::app_library::on_language_changed);
        app_settings::on_language_changed();
    }
    fn toast(&self, title: &str, msg: &str, warn: bool, ttl_ms: u32) {
        show_toast(title, msg, warn, if ttl_ms == 0 { TOAST_TTL_MS } else { ttl_ms });
    }
    fn error(&self, message: &str) {
        let msg = if message.is_empty() { lang::t("toast.unknownError") } else { message.to_owned() };
        show_toast(&lang::t("toast.errorTitle"), &msg, true, 8000);
    }
    fn open_settings(&self) {
        with_ui(|ui| app_settings::open(ui, None));
    }
    fn window_hidden(&self) {
        // Reabrir o app começa limpo, sem um player fantasma.
        with_ui(|ui| {
            if crate::app_player::is_open() {
                crate::app_player::close(ui);
            }
        });
    }

    fn monitors(&self, list: &[Device], changes: &[String]) {
        app_settings::on_devices("monitors", list);
        app_main::on_monitors(list, changes);
    }
    fn webcams(&self, list: &[Device], changes: &[String]) {
        app_settings::on_devices("webcams", list);
        app_main::on_webcams(list, changes);
    }
    fn audio_sources(&self, mics: &[Device], speakers: &[Device], silent: bool, changes: &[String]) {
        app_settings::on_devices("mics", mics);
        app_settings::on_devices("speakers", speakers);
        app_main::on_audio_sources(mics, speakers, silent, changes);
    }
    fn audio_meters(&self, items: &[AudioMeter]) {
        app_main::on_audio_meters(items);
    }
    fn monitor_thumbs(&self, items: &[MonitorThumb]) {
        app_main::on_monitor_thumbs(items);
    }
    fn device_change_pending(&self, kind: PendingKind, pending: bool) {
        app_main::on_device_change_pending(kind, pending);
    }
    fn refresh_busy(&self, busy: bool, what: &str) {
        app_main::on_refresh_busy(busy, what);
    }
    fn recording_state(&self, st: &RecState) {
        app_main::on_recording_state(st);
    }
    fn recording_finalizing(&self, active: bool) {
        app_main::on_recording_finalizing(active);
    }
    fn recording_stopping(&self) {
        // Atalho/bandeja: o clique na tela já tocou (o debounce evita o 2º).
        app_main::play_stop_sound();
    }
    fn replay_state(&self, st: &ReplayState) {
        app_settings::on_replay_state(st);
        app_main::on_replay_state(st);
    }
    fn replay_saved(&self, _id: &str, name: &str, duration_sec: i32) {
        app_main::on_replay_saved(name, duration_sec);
    }
    fn replay_restarted(&self) {
        app_main::on_replay_restarted();
    }

    // As telas entram por fase (docs/ui-iron.md); até lá, o que é delas não
    // aparece.
    fn capture_windows(&self, seq: u32, items: &[WindowItem], app_audio: bool) {
        crate::app_picker::on_capture_windows(seq, items, app_audio);
    }
    fn capture_window_thumb(&self, seq: u32, id: &str, width: u32, height: u32, rgb: &[u8]) {
        crate::app_picker::on_capture_window_thumb(seq, id, width, height, rgb);
    }
    fn window_picker_matches(&self, m: &WindowMatches) {
        crate::app_picker::on_window_picker_matches(m);
    }
    fn record_countdown(&self, sec: u32, total: u32) {
        app_main::on_record_countdown(sec, total);
    }
    fn library(&self, lib: &Library) {
        crate::app_library::on_library(lib);
    }
    fn recording_added(&self, item: &RecItem, free_bytes: i64) {
        crate::app_library::on_recording_added(item, free_bytes);
    }
    fn recording_meta(&self, id: &str, duration: i32, thumb: &str) {
        crate::app_library::on_recording_meta(id, duration, thumb);
    }
    fn recording_renamed(&self, old_id: &str, new_id: &str, new_name: &str) {
        crate::app_library::on_recording_renamed(old_id, new_id, new_name);
    }
    fn recording_removed(&self, id: &str, free_bytes: i64) {
        crate::app_library::on_recording_removed(id, free_bytes);
    }
    fn folder_created(&self, id: &str) {
        crate::app_library::on_folder_created(id);
    }
    fn transcript_search(&self, query: &str, ids: &[String]) {
        crate::app_library::on_transcript_search(query, ids);
    }
    fn merge_pending(&self) {
        crate::app_library::on_merge_pending();
    }
    fn merge_done(&self, ok: bool) {
        crate::app_library::on_merge_done(ok);
    }
    fn waveform(&self, id: &str, peaks: &[f32], hi: bool) {
        // A de alta resolução é da linha do tempo da exportação.
        if hi {
            crate::app_export::on_waveform(id, peaks);
        } else {
            crate::app_player::on_waveform(id, peaks);
        }
    }
    fn video_info(&self, info: &VideoInfo) {
        if !crate::app_export::on_video_info(info) {
            crate::app_player::on_video_info(info);
        }
    }
    fn transcript(&self, t: &Transcript) {
        if !crate::app_export::on_transcript(t) {
            crate::app_player::on_transcript(t);
        }
    }
    fn settings_changed(&self) {
        app_settings::on_settings_changed();
    }
    fn record_dir_picked(&self, path: &str) {
        app_settings::on_record_dir_picked(path);
    }
    fn update_result(&self, r: &UpdateInfo) {
        app_settings::on_update_result(r);
    }
    fn encoder_caps(&self, _caps: &crate::types::EncoderCaps, export: &[ExportEncoderInfo]) {
        app_settings::on_encoder_caps();
        crate::app_export::set_encoders(export);
    }
    fn hotkeys_state(&self, items: &[crate::hotkeys::Item], status: &[&str], delay_max: u32, defining: Option<usize>) {
        let status: Vec<&'static str> = status.iter().map(|s| static_status(s)).collect();
        app_settings::on_hotkeys_state(items, &status, delay_max, defining);
    }
    fn video_test(&self, st: &VideoTestState) {
        app_settings::on_video_test(st);
    }
    fn transcribe_state(&self, st: &TranscribeState) {
        app_settings::on_transcribe_state(st);
    }
    fn transcribe_pending(&self, count: usize, cloud: usize) {
        app_settings::on_transcribe_pending(count, cloud);
    }
    fn transcribe_queue(&self, items: &[QueueItem]) {
        app_settings::on_transcribe_queue(items);
    }
    fn local_asr_state(&self, st: &LocalAsrInfo) {
        app_settings::on_local_asr_state(st);
    }
    fn root_folders(&self, folders: &[RootFolder]) {
        app_settings::on_root_folders(folders);
    }
    fn export_progress(&self, pct: f64, source: &str) {
        crate::app_export::on_progress(pct, source);
    }
    fn export_done(&self, ok: bool, canceled: bool, path: &str, _source: &str) {
        crate::app_export::on_done(ok, canceled, path);
    }
    fn export_preview_progress(&self, pct: f64) {
        crate::app_export::on_preview_progress(pct);
    }
    fn export_preview_done(&self, ok: bool, canceled: bool, path: &str) {
        crate::app_export::on_preview_done(ok, canceled, path);
    }
}

fn static_status(s: &str) -> &'static str {
    match s {
        "empty" => "empty",
        "duplicate" => "duplicate",
        "invalid" => "invalid",
        "reserved" => "reserved",
        "inactive" => "inactive",
        "busy" => "busy",
        _ => "",
    }
}

/// Abre o projeto iron (forms e temas embutidos no exe), registra os
/// handlers e o [`View`]. A janela só nasce quando o laço começa.
pub fn create() -> Result<iron::Application, iron::Error> {
    let mut app = iron::Application::bundled(iron::include_project!())?;
    crate::forms::register(&mut app.ui);
    POSTER.with(|p| *p.borrow_mut() = Some(app.ui.poster()));
    MAIN_FORM.with(|m| m.set(Some(app.main_form)));
    apply_theme(&mut app.ui, crate::bridge::resolved_theme() == "dark");
    crate::view::set(Rc::new(IronView));
    with_ui(app_main::setup);
    with_ui(crate::app_library::apply_texts);
    with_ui(crate::app_player::setup);
    Ok(app)
}

// ---------------------------------------------------------------------------
// Confirmação (o diálogo da versão web): um cartão sobre o fundo escurecido,
// dentro da janela. Esc ou clique no fundo cancelam; Enter confirma.
// ---------------------------------------------------------------------------

/// Ícone do diálogo: a lixeira vermelha ou o "unir" azul.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConfirmIcon {
    Delete,
    Merge,
}

type ConfirmFn = Box<dyn FnOnce(&mut Ui)>;

thread_local! {
    static CONFIRM: RefCell<Option<(ComponentId, ConfirmFn)>> = const { RefCell::new(None) };
}

const TRASH_PATH: &str = "M3 6h18v2H3V6zm3 3h12l-1 12H7L6 9zm5-7h2v2h-2V2zm-3 1h8v1H8V3z";
const MERGE_PATH: &str = "M7.586 8.00366L4 8.00366C3.44772 8.00366 3 7.55595 3 7.00366C3 6.45138 3.44772 6.00366 4 6.00366L8 6.00366C8.26509 6.00366 8.51933 6.10892 8.70685 6.2963L13.414 11H18.5845L15.2931 7.71103C14.9025 7.32065 14.9023 6.68748 15.2926 6.29681C15.683 5.90615 16.3162 5.90592 16.7068 6.2963L21.7068 11.2926C21.8945 11.4802 22 11.7346 22 11.9998C22 12.2651 21.8947 12.5195 21.7071 12.7071L16.7071 17.7071C16.3166 18.0976 15.6834 18.0976 15.2929 17.7071C14.9024 17.3166 14.9024 16.6834 15.2929 16.2929L18.5858 13H13.4142L8.70711 17.7071C8.51957 17.8947 8.26522 18 8 18H4C3.44772 18 3 17.5523 3 17C3 16.4477 3.44772 16 4 16H7.58579L11.5855 12.0003L7.586 8.00366Z";

pub fn confirm_open() -> bool {
    CONFIRM.with(|c| c.borrow().is_some())
}

/// Fecha o diálogo; com `ok`, roda a ação.
pub fn confirm_close(ui: &mut Ui, ok: bool) {
    let Some((scrim, f)) = CONFIRM.with(|c| c.borrow_mut().take()) else { return };
    if ui.contains(scrim) {
        ui.free(scrim);
    }
    ui.invalidate();
    if ok {
        f(ui);
    }
}

/// Pergunta antes de uma ação. `danger` pinta o botão de vermelho.
pub fn confirm(ui: &mut Ui, title: &str, message: &str, ok: &str, danger: bool, icon: ConfirmIcon, on_ok: impl FnOnce(&mut Ui) + 'static) {
    confirm_close(ui, false);
    let Some(form) = main_form() else { return };
    let Some(fb) = ui.bounds(form) else { return };
    let Ok(scrim) = ui.create("Panel", Some(form)) else { return };
    let _ = ui.set_prop(scrim, "style", "scrim");
    if let Some(cb) = ui.control_mut(scrim) {
        cb.set_align(Align::Contents).set_hit_test(true);
    }
    ui.on(scrim, "on_click", |ui, _, _| confirm_close(ui, false));
    let Ok(card) = ui.create("Panel", Some(scrim)) else { return };
    let _ = ui.set_prop(card, "style", "cf_card");
    let w = 420.0f32.min(fb.width - 40.0);
    let text_w = w - 44.0 - 48.0 - 14.0;
    let measure = |ui: &mut Ui, style: &str, text: &str| -> (ComponentId, f32) {
        let l = ui.create("Label", Some(card)).expect("Label");
        let _ = ui.set_prop(l, "style", style);
        let _ = ui.set_prop(l, "text", text);
        let _ = ui.set_prop(l, "wrap", true);
        let _ = ui.set_prop(l, "v_align", "leading");
        let st = TextStyle { font: ui.font_of(l), wrap: true, ..TextStyle::default() };
        let h = ui.measure_text_wrapped(text, &st, text_w).height.ceil() + 1.0;
        (l, h)
    };
    let (lt, ht) = measure(ui, "cf_title", if title.is_empty() { "" } else { title });
    let (lm, hm) = measure(ui, "cf_msg", message);
    let hm = hm * 1.13;
    let text_h = ht + 4.0 + hm;
    let top_h = 22.0 + text_h.max(48.0) + 8.0;
    let h = top_h + 14.0 + 34.0 + 18.0;
    for (l, y, hh) in [(lt, 22.0, ht), (lm, 22.0 + ht + 4.0, hm)] {
        if let Some(cb) = ui.control_mut(l) {
            cb.set_rect(22.0 + 48.0 + 14.0, y, text_w, hh);
        }
    }
    // O ícone num círculo.
    if let Ok(circle) = ui.create("Circle", Some(card)) {
        let tint = if icon == ConfirmIcon::Merge { "blue/10" } else { "danger/12" };
        let _ = ui.set_prop(circle, "fill", tint);
        let _ = ui.set_prop(circle, "stroke_width", 0.0);
        if let Some(cb) = ui.control_mut(circle) {
            cb.set_rect(22.0, 22.0, 48.0, 48.0).set_hit_test(false);
        }
    }
    if let Ok(p) = ui.create("Path", Some(card)) {
        let (data, color, (x, y, pw, ph)) = if icon == ConfirmIcon::Merge {
            (MERGE_PATH, "blue_2", (3.0, 6.0, 19.0, 12.0))
        } else {
            (TRASH_PATH, "red", (3.0, 2.0, 18.0, 19.0))
        };
        let _ = ui.set_prop(p, "data", data);
        let _ = ui.set_prop(p, "fill", color);
        let _ = ui.set_prop(p, "stroke_width", 0.0);
        if let Some(cb) = ui.control_mut(p) {
            cb.set_rect(22.0 + 12.0 + x, 22.0 + 12.0 + y, pw, ph).set_hit_test(false);
        }
    }
    // Os botões, à direita: cancelar e confirmar.
    let mut x = w - 22.0;
    for (text, kind, is_ok) in [(ok.to_owned(), if danger { "cf_danger" } else { "cf_btn" }, true), (lang::t("common.cancel"), "cf_btn", false)] {
        let Ok(b) = ui.create("Button", Some(card)) else { continue };
        let _ = ui.set_prop(b, "style", kind);
        let _ = ui.set_prop(b, "text", text.as_str());
        let tw = ui.measure_text(&text, &TextStyle { font: ui.font_of(b), ..TextStyle::default() }).width.ceil();
        let bw = tw + 32.0;
        x -= bw;
        if let Some(cb) = ui.control_mut(b) {
            cb.set_rect(x, top_h + 14.0, bw, 34.0);
        }
        x -= 8.0;
        ui.on(b, "on_click", move |ui, _, _| confirm_close(ui, is_ok));
    }
    if let Some(cb) = ui.control_mut(card) {
        cb.set_rect((fb.width - w) / 2.0, (fb.height - h) / 2.0, w, h).set_hit_test(true);
    }
    CONFIRM.with(|c| *c.borrow_mut() = Some((scrim, Box::new(on_ok))));
    ui.invalidate();
}

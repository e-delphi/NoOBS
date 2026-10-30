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
use crate::{app_main, lang, log};

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
    }
    fn toast(&self, title: &str, msg: &str, warn: bool, ttl_ms: u32) {
        show_toast(title, msg, warn, if ttl_ms == 0 { TOAST_TTL_MS } else { ttl_ms });
    }
    fn error(&self, message: &str) {
        let msg = if message.is_empty() { lang::t("toast.unknownError") } else { message.to_owned() };
        show_toast(&lang::t("toast.errorTitle"), &msg, true, 8000);
    }
    fn open_settings(&self) {}
    fn window_hidden(&self) {
        // Reabrir o app começa limpo, sem um player fantasma.
        with_ui(|ui| {
            if crate::app_player::is_open() {
                crate::app_player::close(ui);
            }
        });
    }

    fn monitors(&self, list: &[Device], changes: &[String]) {
        app_main::on_monitors(list, changes);
    }
    fn webcams(&self, list: &[Device], changes: &[String]) {
        app_main::on_webcams(list, changes);
    }
    fn audio_sources(&self, mics: &[Device], speakers: &[Device], silent: bool, changes: &[String]) {
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
        if !hi {
            crate::app_player::on_waveform(id, peaks);
        }
    }
    fn video_info(&self, info: &VideoInfo) {
        crate::app_player::on_video_info(info);
    }
    fn transcript(&self, t: &Transcript) {
        crate::app_player::on_transcript(t);
    }
    fn settings_changed(&self) {}
    fn record_dir_picked(&self, _path: &str) {}
    fn update_result(&self, _r: &UpdateInfo) {}
    fn encoder_caps(&self, _caps: &crate::types::EncoderCaps, _export: &[ExportEncoderInfo]) {}
    fn hotkeys_state(&self, _items: &[crate::hotkeys::Item], _status: &[&str], _delay_max: u32, _defining: Option<usize>) {}
    fn video_test(&self, _st: &VideoTestState) {}
    fn transcribe_state(&self, _st: &TranscribeState) {}
    fn transcribe_pending(&self, _count: usize, _cloud: usize) {}
    fn transcribe_queue(&self, _items: &[QueueItem]) {}
    fn local_asr_state(&self, _st: &LocalAsrInfo) {}
    fn root_folders(&self, _folders: &[RootFolder]) {}
    fn export_progress(&self, _pct: f64, _source: &str) {}
    fn export_done(&self, _ok: bool, _canceled: bool, _path: &str, _source: &str) {}
    fn export_preview_progress(&self, _pct: f64) {}
    fn export_preview_done(&self, _ok: bool, _canceled: bool, _path: &str) {}
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

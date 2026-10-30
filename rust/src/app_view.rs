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
use crate::{lang, log};

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

/// Um aviso na tela. Por enquanto o snackbar do iron; o empilhado no canto
/// (título, mensagem, âmbar nos avisos) entra com a tela principal.
pub fn show_toast(title: &str, msg: &str, _warn: bool, _ttl_ms: u32) {
    let text = if msg.is_empty() { title.to_owned() } else { format!("{title} — {msg}") };
    with_ui(move |ui| {
        iron::prelude::snackbar(ui, &text, "", |_| {});
    });
}

pub struct IronView;

impl View for IronView {
    fn theme(&self, dark: bool, _pref: &str) {
        with_ui(move |ui| apply_theme(ui, dark));
    }
    fn language_changed(&self, _language: &str, _pref: &str) {}
    fn toast(&self, title: &str, msg: &str, warn: bool, ttl_ms: u32) {
        show_toast(title, msg, warn, if ttl_ms == 0 { TOAST_TTL_MS } else { ttl_ms });
    }
    fn error(&self, message: &str) {
        let msg = if message.is_empty() { lang::t("toast.unknownError") } else { message.to_owned() };
        show_toast(&lang::t("toast.errorTitle"), &msg, true, 8000);
    }
    fn open_settings(&self) {}
    fn window_hidden(&self) {}

    // As telas entram por fase (docs/ui-iron.md); até lá, o que é delas não
    // aparece.
    fn monitors(&self, _list: &[Device], _changes: &[String]) {}
    fn webcams(&self, _list: &[Device], _changes: &[String]) {}
    fn audio_sources(&self, _mics: &[Device], _speakers: &[Device], _silent: bool, _changes: &[String]) {}
    fn audio_meters(&self, _items: &[AudioMeter]) {}
    fn monitor_thumbs(&self, _items: &[MonitorThumb]) {}
    fn device_change_pending(&self, _kind: PendingKind, _pending: bool) {}
    fn refresh_busy(&self, _busy: bool, _what: &str) {}
    fn recording_state(&self, _st: &RecState) {}
    fn recording_finalizing(&self, _active: bool) {}
    fn recording_stopping(&self) {}
    fn replay_state(&self, _st: &ReplayState) {}
    fn replay_saved(&self, _id: &str, _name: &str, _duration_sec: i32) {}
    fn replay_restarted(&self) {}
    fn capture_windows(&self, _seq: u32, _items: &[WindowItem], _app_audio: bool) {}
    fn capture_window_thumb(&self, _seq: u32, _id: &str, _width: u32, _height: u32, _rgb: &[u8]) {}
    fn window_picker_matches(&self, _m: &WindowMatches) {}
    fn record_countdown(&self, _sec: u32, _total: u32) {}
    fn library(&self, _lib: &Library) {}
    fn recording_added(&self, _item: &RecItem, _free_bytes: i64) {}
    fn recording_meta(&self, _id: &str, _duration: i32, _thumb: &str) {}
    fn recording_renamed(&self, _old_id: &str, _new_id: &str, _new_name: &str) {}
    fn recording_removed(&self, _id: &str, _free_bytes: i64) {}
    fn folder_created(&self, _id: &str) {}
    fn transcript_search(&self, _query: &str, _ids: &[String]) {}
    fn merge_pending(&self) {}
    fn merge_done(&self, _ok: bool) {}
    fn waveform(&self, _id: &str, _peaks: &[f32], _hi: bool) {}
    fn video_info(&self, _info: &VideoInfo) {}
    fn transcript(&self, _t: &Transcript) {}
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
    Ok(app)
}

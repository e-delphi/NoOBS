//! Gravar UMA janela (seletor da tela principal) — parte do `OBSBridge.pas`,
//! pegadinha #66.
//!
//! `list_capture_windows` responde `capture_windows` na hora (EnumWindows é
//! barato) e depois, uma a uma, `capture_window_thumb` — `PrintWindow` custa
//! dezenas de ms por janela. `record_window` só manda o id: a string da janela
//! vem da última lista guardada aqui, nunca da UI.
//!
//! Atalho de janela (aba Atalhos): `record_by_title` procura pelo trecho do
//! título; várias janelas casando mandam `window_picker_matches` (o seletor
//! só com elas) e o `record_window` que volta traz o `delaySec` do atalho.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;

use crate::engine::{self, WindowCapture};
use crate::win_windows::{self, CaptureWindow};
use crate::{lang, log, mainthread, player};

use super::{countdown, is_shutting_down, post_error, post_json, recording};

thread_local! {
    static LAST_LIST: RefCell<Vec<CaptureWindow>> = const { RefCell::new(Vec::new()) };
    /// Seletor pedido por um atalho com a interface descarregada: vai quando a
    /// página nova mandar `ready`.
    static PENDING_PICKER: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };
    static SEQ: Cell<u32> = const { Cell::new(0) };
}

/// Lido pela worker das prévias: um pedido novo interrompe a anterior.
static SEQ_SHARED: AtomicU32 = AtomicU32::new(0);

/// Também é a chave da prévia no mapa de miniaturas do `player` (mesmo
/// servidor das prévias dos monitores).
fn item_id(handle: isize) -> String {
    format!("win{}", handle as usize)
}

/// O som por app (loopback de processo do WASAPI) só existe no Windows 10
/// 2004+ (build 19041). Com o motor de pé a resposta é dele; antes do warmup,
/// a mesma regra de versão.
fn app_audio_supported() -> bool {
    if let Some(Some(v)) = engine::with_engine(|e| e.is_initialized().then(|| e.app_audio_available())) {
        return v;
    }
    crate::win::windows_build() >= 19041
}


fn push_capture_windows(seq: u32, list: Vec<CaptureWindow>) {
    if seq != SEQ.with(Cell::get) {
        return;
    }
    let items: Vec<_> = list
        .iter()
        .map(|w| json!({ "id": item_id(w.handle), "title": w.title, "exe": w.exe, "w": w.width, "h": w.height }))
        .collect();
    LAST_LIST.with(|l| *l.borrow_mut() = list);
    post_json(json!({
        "type": "capture_windows",
        "seq": seq,
        "appAudio": app_audio_supported(),
        "items": items,
    }));
}

fn next_seq() -> u32 {
    let seq = SEQ.with(|s| {
        s.set(s.get() + 1);
        s.get()
    });
    SEQ_SHARED.store(seq, Ordering::SeqCst);
    seq
}

pub fn handle_list_capture_windows() {
    let seq = next_seq();
    std::thread::spawn(move || {
        let list = win_windows::enumerate_capture_windows();
        log!("Janelas: {} gravavel(is).", list.len());
        let handles: Vec<(isize, String)> = list.iter().map(|w| (w.handle, w.title.clone())).collect();
        mainthread::queue(move || push_capture_windows(seq, list));
        thumbs_worker(seq, handles);
    });
}

/// Prévias uma a uma (dezenas de ms cada); um pedido novo interrompe.
fn thumbs_worker(seq: u32, handles: Vec<(isize, String)>) {
    for (handle, title) in handles {
        if is_shutting_down() || SEQ_SHARED.load(Ordering::SeqCst) != seq {
            return;
        }
        let jpeg = win_windows::capture_window_thumb_jpeg(handle, 480, 300);
        if jpeg.is_empty() {
            log!("Janelas: sem previa de \"{title}\".");
            continue;
        }
        let id = item_id(handle);
        player::set_monitor_thumb(&id, jpeg);
        let url = player::get_monitor_thumb_url(&id);
        mainthread::queue(move || {
            if seq != SEQ.with(Cell::get) {
                return;
            }
            post_json(json!({ "type": "capture_window_thumb", "seq": seq, "id": id, "thumb": url }));
        });
    }
}

fn capture_of(w: &CaptureWindow, app_audio: bool) -> WindowCapture {
    // Tamanho lido AGORA, como o Delphi (GetCaptureWindowSize no
    // BuildCaptureGraph): a janela pode ter sido redimensionada ou minimizada
    // depois de a lista ser montada. Sem tamanho legível o engine cai no
    // canvas 1920x1080.
    let (width, height) = win_windows::capture_window_size(w.handle).unwrap_or((0, 0));
    WindowCapture {
        spec: w.spec.clone(),
        title: w.title.clone(),
        width,
        height,
        audio: app_audio,
        app: w.app_name(),
    }
}

/// `record_window`: a janela escolhida no seletor. `delay_sec` > 0 quando o
/// seletor foi aberto por um atalho com atraso (várias janelas casaram).
pub fn handle_record_window(id: &str, app_audio: bool, delay_sec: u32) {
    if recording::recording_active() || countdown::active() {
        return;
    }
    let found = LAST_LIST.with(|l| l.borrow().iter().find(|w| item_id(w.handle) == id).cloned());
    let Some(w) = found else {
        log!("HandleRecordWindow: id {id} fora da ultima lista.");
        post_error(lang::t("windowPicker.gone"));
        return;
    };
    log!("HandleRecordWindow: \"{}\" [{} / {}] somDoApp={app_audio}.", w.title, w.exe, w.class_name);
    countdown::start(countdown::Target::Window(capture_of(&w, app_audio)), delay_sec.min(crate::hotkeys::DELAY_MAX));
}

/// Grava a janela agora (direto ou no fim da contagem).
pub(super) fn start_window(w: WindowCapture) {
    if recording::recording_active() {
        return;
    }
    recording::set_recording_window(Some(w));
    recording::handle_record_start();
    // Não subiu (recusada ou falhou): a próxima gravação volta a ser das telas.
    if !recording::recording_active() {
        recording::set_recording_window(None);
    }
}

/// Atalho de janela: procura pelo trecho do título (sem diferenciar
/// maiúsculas). Uma janela = grava; várias = abre o seletor só com elas (o
/// atraso conta depois da escolha); nenhuma = avisa.
pub(super) fn record_by_title(pattern: String, app_audio: bool, delay_sec: u32) {
    let seq = next_seq();
    std::thread::spawn(move || {
        let list = win_windows::enumerate_capture_windows();
        mainthread::queue(move || on_title_search(seq, list, pattern, app_audio, delay_sec));
    });
}

fn on_title_search(seq: u32, list: Vec<CaptureWindow>, pattern: String, app_audio: bool, delay_sec: u32) {
    if seq != SEQ.with(Cell::get) || recording::recording_active() || countdown::active() {
        return;
    }
    let matches: Vec<CaptureWindow> =
        list.iter().filter(|w| crate::hotkeys::title_matches(&w.title, &pattern)).cloned().collect();
    log!("Atalho de janela \"{pattern}\": {} de {} janela(s) casaram.", matches.len(), list.len());
    match matches.len() {
        0 => {
            let msg = lang::t_with("hotkeys.noWindow", &[("title", &pattern)]);
            if !crate::ui::is_window_visible() {
                crate::tray::show_balloon(&crate::ui::window_title(), &msg);
            }
            post_error(msg);
        }
        1 => {
            let w = &matches[0];
            log!("Atalho de janela: gravando \"{}\" [{}].", w.title, w.exe);
            countdown::start(countdown::Target::Window(capture_of(w, app_audio)), delay_sec);
        }
        _ => {
            // O seletor precisa da janela do app à vista.
            crate::ui::restore_from_tray();
            let items: Vec<_> = matches
                .iter()
                .map(|w| json!({ "id": item_id(w.handle), "title": w.title, "exe": w.exe, "w": w.width, "h": w.height }))
                .collect();
            let handles: Vec<(isize, String)> = matches.iter().map(|w| (w.handle, w.title.clone())).collect();
            LAST_LIST.with(|l| *l.borrow_mut() = list);
            let msg = json!({
                "type": "window_picker_matches",
                "seq": seq,
                "pattern": pattern,
                "appAudio": app_audio_supported(),
                "wantAppAudio": app_audio,
                "delaySec": delay_sec,
                "items": items,
            });
            // Atalho antes de a interface subir: a lista espera o `init_ui`.
            if !super::ui_ready() {
                PENDING_PICKER.with(|p| *p.borrow_mut() = Some(msg));
            } else {
                post_json(msg);
            }
            std::thread::spawn(move || thumbs_worker(seq, handles));
        }
    }
}

/// Interface pronta: o seletor que um atalho pediu enquanto ela subia.
pub fn on_page_ready() {
    if let Some(msg) = PENDING_PICKER.with(|p| p.borrow_mut().take()) {
        post_json(msg);
    }
}

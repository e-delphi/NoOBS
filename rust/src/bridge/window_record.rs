//! Gravar UMA janela (seletor da tela principal) — parte do `OBSBridge.pas`,
//! pegadinha #66.
//!
//! `list_capture_windows` responde `capture_windows` na hora (EnumWindows é
//! barato) e depois, uma a uma, `capture_window_thumb` — `PrintWindow` custa
//! dezenas de ms por janela. `record_window` só manda o id: a string da janela
//! vem da última lista guardada aqui, nunca da UI.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;

use crate::engine::{self, WindowCapture};
use crate::win_windows::{self, CaptureWindow};
use crate::{lang, log, mainthread, player};

use super::{is_shutting_down, post_error, post_json, recording};

thread_local! {
    static LAST_LIST: RefCell<Vec<CaptureWindow>> = const { RefCell::new(Vec::new()) };
    static SEQ: Cell<u32> = const { Cell::new(0) };
}

/// Lido pela worker das prévias: um pedido novo interrompe a anterior.
static SEQ_SHARED: AtomicU32 = AtomicU32::new(0);

/// Também é a chave da prévia no mapa de miniaturas do `player` (mesmo
/// servidor das prévias dos monitores).
fn item_id(handle: isize) -> String {
    format!("win{}", handle as usize)
}

/// O som por app (`wasapi_process_output_capture`) só existe no Windows 10
/// 2004+ (build 19041). Com o libobs de pé a resposta é dele; antes do warmup,
/// a mesma regra de versão do plugin.
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

pub fn handle_list_capture_windows() {
    let seq = SEQ.with(|s| {
        s.set(s.get() + 1);
        s.get()
    });
    SEQ_SHARED.store(seq, Ordering::SeqCst);
    std::thread::spawn(move || {
        let list = win_windows::enumerate_capture_windows();
        log!("Janelas: {} gravavel(is).", list.len());
        let handles: Vec<(isize, String)> = list.iter().map(|w| (w.handle, w.title.clone())).collect();
        mainthread::queue(move || push_capture_windows(seq, list));
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
    });
}

pub fn handle_record_window(id: &str, app_audio: bool) {
    if recording::recording_active() {
        return;
    }
    let found = LAST_LIST.with(|l| l.borrow().iter().find(|w| item_id(w.handle) == id).cloned());
    let Some(w) = found else {
        log!("HandleRecordWindow: id {id} fora da ultima lista.");
        post_error(lang::t("windowPicker.gone"));
        return;
    };
    log!("HandleRecordWindow: \"{}\" [{} / {}] somDoApp={app_audio}.", w.title, w.exe, w.class_name);
    // Tamanho lido AGORA, como o Delphi (GetCaptureWindowSize no
    // BuildCaptureGraph): a janela pode ter sido redimensionada ou minimizada
    // depois de a lista ser montada. Sem tamanho legível o engine cai no
    // canvas 1920x1080.
    let (width, height) = win_windows::capture_window_size(w.handle).unwrap_or((0, 0));
    recording::set_recording_window(Some(WindowCapture {
        spec: w.spec.clone(),
        title: w.title.clone(),
        width,
        height,
        audio: app_audio,
        app: w.app_name(),
    }));
    recording::handle_record_start();
    // Não subiu (recusada ou falhou): a próxima gravação volta a ser das telas.
    if !recording::recording_active() {
        recording::set_recording_window(None);
    }
}

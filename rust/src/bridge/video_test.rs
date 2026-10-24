//! Teste de qualidade de vídeo da aba Vídeo (pegadinha #64) — parte do
//! `OBSBridge.pas`.
//!
//! Grava `VIDEO_TEST_SEC` pelo MESMO caminho da gravação (cena, monitores,
//! canvas, fps e encoder), mas no nível MÁXIMO e fora da biblioteca. Depois,
//! numa worker, reproduz cada nível 0..10 a partir dessa referência pela
//! exportação — mesma família de encoder, CRF calibrado dos dois lados (#51b e
//! #53) —, então o tamanho sai equivalente ao que a gravação daria. De cada
//! nível sai o tamanho, um PNG do meio do clipe e o MP4 que toca no comparador.
//!
//! Por que não gravar os 11 níveis ao vivo: em 4K nenhuma GPU encoda 11 fluxos
//! em tempo real, e quadro perdido muda justamente o tamanho que se mede.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;



use crate::engine::{self, RecordRequest};
use crate::ffmpeg_export::{export_video, resolve_export_encoder, ExportOptions, ExportResult, ExportSegment};
use crate::{encoder, ffmpeg_ops, lang, log, mainthread, probe, ui};

use super::{is_shutting_down, recording, replay};

pub const TIMER_VIDEO_TEST: usize = 7016;
/// Duração da amostra.
const VIDEO_TEST_SEC: u32 = 5;
const VIDEO_TEST_LEVELS: i32 = 11;

thread_local! {
    /// A amostra está ocupando a captura (como uma gravação).
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    /// Captura OU codificações em curso.
    static BUSY: Cell<bool> = const { Cell::new(false) };
    static REF_PATH: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Lido pela worker e pelo `export_video` a cada pacote.
static CANCEL: AtomicI32 = AtomicI32::new(0);

pub fn capturing() -> bool {
    CAPTURING.with(Cell::get)
}

pub fn busy() -> bool {
    BUSY.with(Cell::get)
}

fn canceled() -> bool {
    CANCEL.load(Ordering::SeqCst) != 0
}

fn test_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").or_else(|_| std::env::var("APPDATA")).unwrap_or_default();
    let d = Path::new(&base).join("NoOBS").join("videotest");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Nome do encoder que gravou → vocabulário do app (`av1-hw`, `h264-sw`…),
/// que é o que o `resolve_export_encoder` entende. Pela família do nome.
fn app_codec_from_encoder(id: &str) -> &'static str {
    match encoder::describe_encoder_id(id) {
        ("AV1", true) => "av1-hw",
        ("AV1", false) => "av1-sw",
        ("HEVC", _) => "hevc-hw",
        (_, true) => "h264-hw",
        _ => "h264-sw",
    }
}

fn push_state(state: &str, error: &str, level: i32, pct: f64) {
    let st = crate::view::VideoTestState {
        state: state.to_owned(),
        error: error.to_owned(),
        level,
        pct: pct.round() as i64,
        seconds: VIDEO_TEST_SEC,
        total: VIDEO_TEST_LEVELS,
        ..Default::default()
    };
    crate::view::call(move |v| v.video_test(&st));
}

fn queue_progress(level: i32, pct: f64) {
    mainthread::queue(move || {
        if !is_shutting_down() {
            push_state("encoding", "", level, pct);
        }
    });
}

pub fn handle_test_video_quality() {
    if busy() {
        return;
    }
    // A amostra monta o grafo de captura como uma gravação: não dá pra ter os
    // dois ao mesmo tempo, e derrubar o buffer apagaria o que ele guardou.
    if recording::recording_active() || replay::replay_active() || engine::with_engine(|e| e.is_stopping()).unwrap_or(false) {
        push_state("error", &lang::t("settings.videoTest.busy"), -1, 0.0);
        return;
    }
    // O motor de pé (encoders detectados).
    engine::ensure_created();
    if let Err(e) = recording::with_engine_result(|e| e.ensure_initialized()) {
        log!("VideoTest: motor nao subiu: {e}");
        push_state("error", &lang::t("settings.videoTest.failed"), -1, 0.0);
        return;
    }
    recording::push_encoder_caps_once();
    let dir = test_dir();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let ref_path = dir.join("ref.mkv").to_string_lossy().into_owned();
    REF_PATH.with(|r| *r.borrow_mut() = ref_path.clone());
    CANCEL.store(0, Ordering::SeqCst);

    // Referência no nível MÁXIMO: é dela que saem os outros níveis, e nenhum
    // pode ficar melhor do que ela.
    encoder::set_quality_level_override(10);
    let inputs = recording::gather_capture_inputs();
    let req = RecordRequest { output_path: ref_path.clone(), auto_profile: false, window: None, area: None };
    let r = recording::with_engine_result(|e| e.build_and_start_recording(&req, &inputs));
    encoder::set_quality_level_override(-1);
    if let Err(e) = r {
        log!("VideoTest: captura nao iniciou: {e}");
        push_state("error", &lang::t("settings.videoTest.failed"), -1, 0.0);
        return;
    }
    CAPTURING.with(|c| c.set(true));
    BUSY.with(|b| b.set(true));
    let enc = engine::with_engine(|e| e.video_encoder_id().to_owned()).unwrap_or_default();
    log!("VideoTest: capturando {VIDEO_TEST_SEC}s em \"{ref_path}\" (encoder {enc}).");
    push_state("recording", "", -1, 0.0);
    ui::set_timer(TIMER_VIDEO_TEST, VIDEO_TEST_SEC * 1000);
}

/// Mesmo stop assíncrono da gravação (pegadinha #41): o resto acontece no
/// `on_engine_recording_stopped` → [`on_recorded`].
fn stop_capture() {
    ui::kill_timer(TIMER_VIDEO_TEST);
    if !capturing() || !engine::exists() {
        return;
    }
    engine::with_engine(|e| e.request_stop());
    ui::set_timer(recording::TIMER_STOP_TIMEOUT, recording::STOP_TIMEOUT_MS);
}

pub fn handle_cancel_video_test() {
    if !busy() {
        return;
    }
    CANCEL.store(1, Ordering::SeqCst);
    if capturing() {
        stop_capture();
    }
}

pub fn on_timer(id: usize) -> bool {
    if id != TIMER_VIDEO_TEST {
        return false;
    }
    stop_capture();
    true
}

/// Shutdown: as codificações param no próximo pacote.
pub fn shutdown() {
    ui::kill_timer(TIMER_VIDEO_TEST);
    CANCEL.store(1, Ordering::SeqCst);
}

#[derive(Default, Clone)]
struct LevelOut {
    bytes: i64,
    png: String,
    video: String,
}

/// A amostra fechou (chamado pelo `on_engine_recording_stopped` no lugar de
/// meta/card/transcrição — ela não é gravação do usuário).
pub fn on_recorded(output_path: &str) {
    CAPTURING.with(|c| c.set(false));
    // O buffer pode ter sido pedido durante a captura (programa da lista
    // abriu); o `start_replay_buffer` recusou enquanto ela durava.
    if replay::replay_wanted() && !replay::replay_active() {
        replay::start_replay_buffer();
    }
    let ref_path = REF_PATH.with(|r| r.borrow().clone());
    if canceled() || output_path.is_empty() || !Path::new(&ref_path).exists() {
        BUSY.with(|b| b.set(false));
        if canceled() {
            push_state("canceled", "", -1, 0.0);
        } else {
            push_state("error", &lang::t("settings.videoTest.failed"), -1, 0.0);
        }
        return;
    }
    let enc_id = engine::with_engine(|e| e.video_encoder_id().to_owned()).unwrap_or_default();
    let enc_name = resolve_export_encoder(app_codec_from_encoder(&enc_id), &recording::last_encoder_caps().unwrap_or_default());
    let cur_level = encoder::recording_quality_level();
    let ref_size = std::fs::metadata(&ref_path).map(|m| m.len()).unwrap_or(0);
    log!("VideoTest: amostra pronta ({enc_id}, {ref_size} bytes); niveis via {enc_name}.");
    push_state("encoding", "", 0, 0.0);

    std::thread::spawn(move || {
        let dir = Path::new(&ref_path).parent().map(Path::to_path_buf).unwrap_or_default();
        let mut outs = vec![LevelOut { bytes: -1, ..Default::default() }; VIDEO_TEST_LEVELS as usize];
        let (mut dur, mut fps, mut w, mut h) = (0.0, 0.0, 0, 0);
        let mut err = String::new();
        let mut was_canceled = false;
        if let Some(rep) = probe::probe(Path::new(&ref_path)) {
            dur = rep.duration;
            if let Some(v) = rep.video_stream() {
                (w, h, fps) = (v.width, v.height, v.frame_rate);
            }
        }
        if dur < 1.0 {
            err = "amostra sem duracao".into();
        } else {
            // O meio do clipe: longe do 1º keyframe, onde todo encoder capricha.
            let mid = dur / 2.0;
            for l in 0..VIDEO_TEST_LEVELS {
                if is_shutting_down() || canceled() {
                    was_canceled = true;
                    break;
                }
                queue_progress(l, l as f64 * 100.0 / VIDEO_TEST_LEVELS as f64);
                // MP4 e não MKV: cada nível toca na tela, e o WebView2 não
                // garante MKV.
                let dst = dir.join(format!("q{l}.mp4")).to_string_lossy().into_owned();
                let opts = ExportOptions {
                    src_path: ref_path.clone(),
                    dst_path: dst.clone(),
                    container: "mp4".into(),
                    encoder_name: enc_name.clone(),
                    crf: encoder::quality_level_to_crf(l),
                    segments: vec![ExportSegment { start_sec: 0.0, end_sec: dur }],
                    speed: 1.0,
                    // Só o vídeo conta no tamanho.
                    audio_streams: Vec::new(),
                    ..Default::default()
                };
                let last = Cell::new(Instant::now() - std::time::Duration::from_secs(1));
                let progress = |pct: f64| {
                    if last.get().elapsed().as_millis() < 250 {
                        return;
                    }
                    last.set(Instant::now());
                    queue_progress(l, (l as f64 + pct / 100.0) * 100.0 / VIDEO_TEST_LEVELS as f64);
                };
                match export_video(&opts, Some(&progress), Some(&CANCEL)) {
                    ExportResult::Ok => {}
                    ExportResult::Canceled => {
                        was_canceled = true;
                        break;
                    }
                    r => {
                        err = format!("nivel {l}: exportacao falhou ({r:?})");
                        break;
                    }
                }
                let o = &mut outs[l as usize];
                o.bytes = std::fs::metadata(&dst).map(|m| m.len() as i64).unwrap_or(-1);
                let png = dir.join(format!("q{l}.png")).to_string_lossy().into_owned();
                if ffmpeg_ops::extract_frame_png(&dst, &png, mid) {
                    o.png = png;
                }
                // O arquivo fica: é ele que toca no comparador. A pasta é
                // esvaziada no próximo teste.
                o.video = dst;
                log!(
                    "VideoTest: nivel {l} (crf {}) -> {} bytes em {dur:.1}s ({:.0} kbps).",
                    opts.crf,
                    o.bytes,
                    o.bytes as f64 * 8.0 / dur / 1000.0
                );
            }
        }
        if is_shutting_down() {
            return;
        }
        mainthread::queue(move || {
            BUSY.with(|b| b.set(false));
            if was_canceled {
                push_state("canceled", "", -1, 0.0);
                return;
            }
            if !err.is_empty() {
                log!("VideoTest: falhou: {err}");
                push_state("error", &lang::t("settings.videoTest.failed"), -1, 0.0);
                return;
            }
            let levels: Vec<_> = outs
                .iter()
                .enumerate()
                .map(|(k, o)| crate::view::VideoTestLevel {
                    level: k as i32,
                    crf: encoder::quality_level_to_crf(k as i32),
                    bytes: o.bytes,
                    image: o.png.clone(),
                    video: o.video.clone(),
                })
                .collect();
            let st = crate::view::VideoTestState {
                state: "done".to_owned(),
                level: -1,
                duration: (dur * 100.0).round() / 100.0,
                width: w as i32,
                height: h as i32,
                fps: (fps * 100.0).round() / 100.0,
                encoder: enc_name.to_string(),
                current_level: cur_level,
                levels,
                ..Default::default()
            };
            crate::view::call(move |v| v.video_test(&st));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_do_app_pela_familia() {
        assert_eq!(app_codec_from_encoder("av1_amf"), "av1-hw");
        assert_eq!(app_codec_from_encoder("libsvtav1"), "av1-sw");
        assert_eq!(app_codec_from_encoder("hevc_nvenc"), "hevc-hw");
        assert_eq!(app_codec_from_encoder("h264_amf"), "h264-hw");
        assert_eq!(app_codec_from_encoder("libx264"), "h264-sw");
    }
}

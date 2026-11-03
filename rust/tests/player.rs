//! O player acelerado, medido de verdade: abre o motor (sem a janela do app), toca em
//! 1×, 2×, 4×, 8× e 16× e conta os quadros que chegam à interface e o maior salto entre
//! eles, em segundos do vídeo. Pular de keyframe em keyframe aparece como um salto de
//! segundos; tocando liso, o salto fica perto de velocidade ÷ quadros por segundo.
//!
//!   cargo test --release --features app-tests --test player -- <arquivo> [<arquivo>...]
//!
//! Cada arquivo imprime uma linha por velocidade: `<arquivo> <velocidade>× <qps> qps,
//! maior salto <s> s, saltos > 0,5 s: <n>`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use noobs::playback::{Out, Playback, ViewSpec};

fn main() {
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exe");
    let w: Vec<u16> = runtime.to_string_lossy().encode_utf16().chain([0]).collect();
    unsafe {
        let _ = windows::Win32::System::LibraryLoader::SetDllDirectoryW(windows::core::PCWSTR(w.as_ptr()));
    }
    let files: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect();
    if files.is_empty() {
        println!("PULADO player: passe os arquivos depois de --");
        return;
    }
    for file in files {
        for rate in [1.0, 2.0, 4.0, 8.0, 16.0] {
            measure(&file, rate);
        }
    }
}

fn measure(file: &str, rate: f64) {
    let shared: Arc<OnceLock<Arc<noobs::playback::Shared>>> = Arc::default();
    let frames: Arc<Mutex<Vec<(Instant, f64)>>> = Arc::default();
    let out = {
        let (shared, frames) = (shared.clone(), frames.clone());
        Arc::new(move |o: Out| {
            if let (Out::Frame { .. }, Some(s)) = (o, shared.get()) {
                // A interface pega só o mais recente: aqui ele é "mostrado" na hora.
                s.ui_pending.store(false, std::sync::atomic::Ordering::Relaxed);
                frames.lock().unwrap().push((Instant::now(), s.position()));
            }
        })
    };
    let view = ViewSpec { stage_w: 1920, stage_h: 1080, ..Default::default() };
    let mut p = Playback::open_paused(file, view, out);
    let _ = shared.set(p.shared.clone());
    std::thread::sleep(Duration::from_millis(1500));
    p.set_rate(rate);
    p.play();
    // Um segundo para assentar; mede os seis seguintes (ou até o fim do arquivo).
    std::thread::sleep(Duration::from_secs(1));
    frames.lock().unwrap().clear();
    let start = Instant::now();
    std::thread::sleep(Duration::from_secs(6));
    let elapsed = start.elapsed().as_secs_f64();
    p.pause();
    let list = frames.lock().unwrap().clone();
    let fps = list.len() as f64 / elapsed;
    let gaps: Vec<f64> = list.windows(2).map(|w| w[1].1 - w[0].1).collect();
    let max_gap = gaps.iter().copied().fold(0.0, f64::max);
    let big = gaps.iter().filter(|g| **g > 0.5).count();
    let stalls = list.windows(2).map(|w| w[1].0 - w[0].0).max().unwrap_or_default();
    let name = std::path::Path::new(file).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if rate == 1.0 {
        let info = noobs::probe::probe(std::path::Path::new(file));
        let v = info.as_ref().and_then(|r| r.video_stream().cloned());
        let kf = noobs::ffmpeg_ops::list_video_keyframes(file).map(|k| k.0.len()).unwrap_or(0);
        let dur = info.as_ref().map_or(0.0, |r| r.duration);
        if let Some(v) = v {
            println!("{name}: {}x{} {} {:.1} qps, {kf} keyframes em {dur:.0} s (um a cada {:.1} s)", v.width, v.height, v.codec, v.frame_rate, dur / kf.max(1) as f64);
        }
    }
    println!(
        "{name} {rate}× {fps:.0} qps, maior salto {max_gap:.2} s (esperado ~{:.2}), saltos > 0,5 s: {big}, maior pausa {} ms",
        rate / 60.0,
        stalls.as_millis()
    );
    drop(p);
    std::thread::sleep(Duration::from_millis(300));
}

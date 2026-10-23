//! Prova de conceito: o player de gravações do NoOBS em Slint, sem WebView2.
//! Decode na placa (D3D11VA) com as MESMAS DLLs e bindings do app
//! (`rust\src\ffi`, incluídos por caminho — nada do app é alterado).
//!
//!   cargo run --release -- "C:\caminho\gravacao.mkv"
//!
//! Sem argumento abre a gravação mais recente de %USERPROFILE%\Videos.
//! Espaço = tocar/pausar, ←/→ = 5 s, i = esconde as medidas. Sem áudio.

#![allow(dead_code)]

/// Os bindings do app chamam `crate::log!`.
#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { eprintln!($($t)*) };
}

#[path = "../../src/ffi/mod.rs"]
mod ffi;
mod gpu;
mod player;

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use slint::ComponentHandle;

slint::include_modules!();

/// DLLs do FFmpeg ficam em `rust\exe\` (o runtime de desenvolvimento do app).
fn add_dll_dir() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "\\..\\exe");
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{dir};{old}"));
}

fn newest_recording() -> Option<String> {
    let dir = std::path::PathBuf::from(std::env::var("USERPROFILE").ok()?).join("Videos");
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("mkv") || x.eq_ignore_ascii_case("mp4")))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max()
        .map(|(_, p)| p.to_string_lossy().into_owned())
}

fn main() {
    let t_start = Instant::now();
    add_dll_dir();
    let Some(path) = std::env::args().nth(1).or_else(newest_recording) else {
        eprintln!("uso: poc-slint <arquivo.mkv>");
        return;
    };

    let ui = PlayerWindow::new().expect("janela");
    let shared = Arc::new(player::Shared::default());
    let (tx, rx) = mpsc::channel::<player::Cmd>();

    // Quadros pintados de verdade pelo renderer (não só entregues à UI).
    {
        let shared = shared.clone();
        let mut first = true;
        let r = ui.window().set_rendering_notifier(move |state, _| {
            if let slint::RenderingState::AfterRendering = state {
                if first {
                    first = false;
                    eprintln!("janela pintada {} ms depois do inicio", t_start.elapsed().as_millis());
                }
                shared.painted.fetch_add(1, Ordering::Relaxed);
            }
        });
        if let Err(e) = r {
            eprintln!("rendering notifier indisponivel: {e:?}");
        }
    }

    {
        let tx = tx.clone();
        ui.on_toggle_play(move || {
            let _ = tx.send(player::Cmd::Toggle);
        });
    }
    {
        let tx = tx.clone();
        ui.on_seek(move |v| {
            let _ = tx.send(player::Cmd::Seek(v as f64));
        });
    }
    {
        let tx = tx.clone();
        ui.on_step(move |d| {
            let _ = tx.send(player::Cmd::Step(d as f64));
        });
    }

    // A área do vídeo em px físicos vai pro decoder converter já no tamanho certo.
    let resize_timer = slint::Timer::default();
    {
        let weak = ui.as_weak();
        let tx = tx.clone();
        let mut last = (0, 0);
        resize_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(200), move || {
            let Some(ui) = weak.upgrade() else { return };
            let s = ui.window().scale_factor();
            let cur = ((ui.get_view_w() * s) as i32, (ui.get_view_h() * s) as i32);
            if cur != last && cur.0 > 0 && cur.1 > 0 {
                last = cur;
                let _ = tx.send(player::Cmd::Resize(cur.0, cur.1));
            }
        });
    }

    let weak = ui.as_weak();
    let shared_t = shared.clone();
    let first_frame = std::sync::atomic::AtomicBool::new(true);
    let worker = std::thread::spawn(move || {
        let shared_ui = shared_t.clone();
        player::run(path, rx, shared_t, move |o| {
            let shared = shared_ui.clone();
            match o {
                player::Out::Frame { buf, pts } => {
                    if first_frame.swap(false, Ordering::Relaxed) {
                        eprintln!("1o quadro {} ms depois do inicio", t_start.elapsed().as_millis());
                    }
                    // POC_DUMP=arquivo.png salva o quadro de ~3 s pra conferir a imagem.
                    if pts >= 3.0 {
                        if let Some(d) = std::env::var_os("POC_DUMP") {
                            std::env::remove_var("POC_DUMP");
                            let _ = player::write_png(&d.to_string_lossy(), buf.width(), buf.height(), buf.as_bytes());
                        }
                    }
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_frame(slint::Image::from_rgba8(buf));
                        ui.set_position(pts as f32);
                        ui.set_time_text(player::time_text(pts, ui.get_duration() as f64).into());
                        shared.ui_pending.store(false, Ordering::Release);
                    });
                }
                player::Out::Info(i) => {
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_duration(i.duration.max(0.1) as f32);
                        ui.set_file_name(i.file_name.into());
                    });
                }
                player::Out::State { playing } => {
                    let _ = weak.upgrade_in_event_loop(move |ui| ui.set_playing(playing));
                }
                player::Out::Stats(s) => {
                    if std::env::var_os("POC_SECS").is_some() {
                        eprintln!("{s}\n");
                    }
                    let _ =weak.upgrade_in_event_loop(move |ui| ui.set_stats(s.into()));
                }
                player::Out::Error(e) => {
                    eprintln!("erro: {e}");
                    let _ = weak.upgrade_in_event_loop(move |ui| ui.set_stats(format!("erro: {e}").into()));
                }
            }
        });
    });

    // Medição: POC_SECS=N fecha sozinho depois de N s (as medidas vão pro stderr).
    // POC_SEEKS=1 faz um seek pra um ponto aleatório a cada 2 s.
    let quit_timer = slint::Timer::default();
    let seek_timer = slint::Timer::default();
    if let Some(n) = std::env::var("POC_SECS").ok().and_then(|s| s.parse::<u64>().ok()) {
        quit_timer.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(n), || {
            let _ = slint::quit_event_loop();
        });
    }
    if std::env::var_os("POC_SEEKS").is_some() {
        let weak = ui.as_weak();
        let tx = tx.clone();
        let mut k = 1u32;
        seek_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(2), move || {
            let Some(ui) = weak.upgrade() else { return };
            k = k.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let t = (k >> 8) as f32 / (1u32 << 24) as f32 * ui.get_duration();
            let _ = tx.send(player::Cmd::Seek(t as f64));
        });
    }

    ui.run().expect("laço de eventos");
    // A janela e os timers seguram cópias do remetente: só com tudo solto o
    // canal fecha e a thread de decode sai.
    drop(tx);
    drop(resize_timer);
    drop(seek_timer);
    drop(quit_timer);
    drop(ui);
    let _ = worker.join();
}

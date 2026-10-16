//! NoOBS — gravador de tela com OBS embarcado (libobs) e UI em WebView2.
//! Porte do projeto Delphi para Rust, sem Tauri: janela e WebView2 criados à
//! mão via Win32 (`windows` + `webview2-com`), como o `OBSUI.pas` faz.
//!
//! Camadas (as mesmas do Delphi):
//! 1. bindings raw — [`ffi::obs`], [`ffi::ffmpeg`]
//! 2. wrappers altos — (a portar) `OBSEngine`, `FFmpegOps`, `FFmpegExport`
//! 3. domínio — [`encoder`], [`probe`], [`config`], [`log`], [`lang`], [`scene`], [`types`]
//! 4. orquestração — [`ui`], [`bridge`]

#![windows_subsystem = "windows"]

mod audio_filters;
mod audio_tracks;
mod audio_watch;
mod autostart;
mod bridge;
mod config;
mod elevate;
mod encoder;
mod engine;
mod export_captions;
mod ffi;
mod file_meta;
mod ffmpeg_export;
mod ffmpeg_ops;
mod hibernate;
mod hotkey;
mod lang;
mod numbers_pt;
mod local_asr;
mod log;
mod mainthread;
mod mic_watch;
mod player;
mod probe;
mod proc_watch;
mod record_icon;
mod record_watch;
mod rtwq;
mod scene;
mod single_instance;
mod startup_check;
mod time_stretch;
mod transcribe;
mod tray;
mod types;
mod update;
mod ui;
mod version;
mod win;
mod win_audio_meter;
mod win_http;
mod win_proc;
mod win_preview;
mod win_webcam;

fn main() {
    // Dispatch entre o modo "full" (UI + libobs + watchers) e o "hibernate"
    // (só bandeja + hotkey, ~5 MB), no MESMO exe.
    let cmd = win::cmdline_lower();
    let hibernate = cmd.contains("/hibernate");
    log::init_log(hibernate);
    log!("===== NoOBS startup =====");
    log!("Dispatcher: cmdline=\"{cmd}\"");

    // "Abrir como administrador": reabre elevado ANTES do mutex de instância
    // única e de qualquer janela. Recusado no UAC, segue normal.
    if elevate::relaunch_elevated_if_wanted() {
        return;
    }

    // i18n também na hibernação: o menu da bandeja sai no idioma do usuário.
    lang::init_language();

    if hibernate {
        log::log("Dispatcher: rota -> hibernate::run (modo minimo).");
        hibernate::run();
        log::log("Dispatcher: hibernate::run retornou — processo encerrando.");
    } else {
        log::log("Dispatcher: rota -> ui::run (modo full).");
        bridge::install_hooks();
        ui::run();
        log::log("Dispatcher: ui::run retornou — processo encerrando.");
    }
}

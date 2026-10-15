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

mod bridge;
mod config;
mod encoder;
mod ffi;
mod lang;
mod log;
mod mainthread;
mod probe;
mod rtwq;
mod scene;
mod single_instance;
mod startup_check;
mod types;
mod ui;
mod win;

fn main() {
    // Dispatch entre o modo "full" (UI + libobs + watchers) e o "hibernate"
    // (só bandeja + hotkey, ~5 MB), no MESMO exe.
    let cmd = win::cmdline_lower();
    let hibernate = cmd.contains("/hibernate");
    log::init_log(hibernate);
    log!("===== NoOBS startup =====");
    log!("Dispatcher: cmdline=\"{cmd}\"");

    // TODO(elevate): OBSElevate.RelaunchElevatedIfWanted (verbo `runas`),
    // ANTES do mutex de instância única.

    lang::init_language();

    if hibernate {
        // TODO(hibernate): porte de OBSHibernate.Run.
        log::log("Dispatcher: modo hibernate ainda não portado.");
    } else {
        log::log("Dispatcher: rota -> ui::run (modo full).");
        ui::run();
        log::log("Dispatcher: ui::run retornou — processo encerrando.");
    }
}

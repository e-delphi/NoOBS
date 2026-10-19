//! NoOBS — gravador de tela com OBS embarcado (libobs) e UI em WebView2.
//! Porte do projeto Delphi para Rust, sem Tauri: janela e WebView2 criados à
//! mão via Win32 (`windows` + `webview2-com`), como o `OBSUI.pas` faz.
//!
//! Camadas (as mesmas do Delphi):
//! 1. bindings raw — [`ffi::obs`], [`ffi::ffmpeg`]
//! 2. wrappers altos — (a portar) `OBSEngine`, `FFmpegOps`, `FFmpegExport`
//! 3. domínio — [`encoder`], [`probe`], [`config`], [`log`], [`lang`], [`scene`], [`types`]
//! 4. orquestração — [`ui`], [`bridge`]
//!
//! Biblioteca + binário: o `main.rs` só chama [`run_app`]. Os testes rodam
//! sempre FORA do app: `#[cfg(test)]` (só no `cargo test`) e `tests/obs.rs`
//! (feature `obs-tests`, ver `modulos!`).

/// Os módulos são PRIVADOS no build normal: o exe não expõe nada. Só com a
/// feature `obs-tests` (o autoteste da libobs, `tests/obs.rs`) eles viram
/// públicos, pra aquele executável de teste — que roda FORA do app — chamar o
/// engine direto. Nenhum código de teste entra no exe por aqui: a feature só
/// muda a visibilidade.
macro_rules! modulos {
    ($($m:ident),* $(,)?) => {
        $(
            #[cfg(not(feature = "obs-tests"))]
            mod $m;
            #[cfg(feature = "obs-tests")]
            #[doc(hidden)]
            pub mod $m;
        )*
    };
}

// `log` e `ffi` fora da macro: eles definem macros `#[macro_export]` (`log!`,
// `dll_api!`), que não podem nascer de um módulo declarado por macro — o
// `use crate::log` / `use crate::dll_api` dos outros módulos quebraria.
#[cfg(not(feature = "obs-tests"))]
mod log;
#[cfg(feature = "obs-tests")]
#[doc(hidden)]
pub mod log;
#[cfg(not(feature = "obs-tests"))]
mod ffi;
#[cfg(feature = "obs-tests")]
#[doc(hidden)]
pub mod ffi;

/// Motor de captura próprio (docs/motor-proprio.md). Público enquanto convive
/// com a libobs: o executável de teste `tests/captura.rs` o usa direto.
pub mod capture;

modulos! {
    native_engine,
    audio_filters,
    audio_tracks,
    audio_watch,
    autostart,
    bridge,
    config,
    elevate,
    encoder,
    engine,
    export_captions,
    file_meta,
    ffmpeg_export,
    ffmpeg_ops,
    gpu_usage,
    hibernate,
    hotkey,
    lang,
    numbers_pt,
    local_asr,
    lock_detector,
    mainthread,
    mic_watch,
    player,
    probe,
    proc_watch,
    rec_indicator,
    record_icon,
    record_watch,
    rtwq,
    scene,
    single_instance,
    startup_check,
    text_file,
    time_stretch,
    transcribe,
    tray,
    types,
    update,
    ui,
    version,
    win,
    win_audio_meter,
    win_http,
    win_proc,
    win_preview,
    win_webcam,
    win_windows,
    area_select,
    area_frame,
}

#[cfg(test)]
mod testkit;
#[cfg(test)]
mod media_tests;

/// Ponto de entrada do exe (o `main.rs` só chama isto): o crate é lib + bin
/// pra que os testes em `tests/` enxerguem os módulos.
pub fn run_app() {
    // Dispatch entre o modo "full" (UI + libobs + watchers) e o "hibernate"
    // (só bandeja + hotkey, ~5 MB), no MESMO exe.
    let cmd = win::cmdline_lower();
    let hibernate = cmd.contains("/hibernate");
    log::init_log(hibernate);
    // Toda falha (panic) vai pro log do NoOBS com o local exato, em qualquer
    // thread. Antes nada ficava registrado: o app sumia e o log parava no meio.
    std::panic::set_hook(Box::new(|info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(sem mensagem)".into());
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let th = std::thread::current();
        log!("PANIC em {loc} (thread {}): {msg}", th.name().unwrap_or("?"));
    }));
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

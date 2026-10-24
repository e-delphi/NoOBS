//! NoOBS — gravador de tela com motor de captura próprio (D3D11 + WGC +
//! WASAPI + libavcodec) e UI nativa em Slint (docs/ui-slint.md). Porte do
//! projeto Delphi para Rust. Sem libobs: o runtime é o exe e as
//! DLLs do FFmpeg.
//!
//! Camadas:
//! 1. bindings raw — [`ffi::ffmpeg`]
//! 2. captura e mídia — [`capture`], `ffmpeg_ops`, `ffmpeg_export`
//! 3. domínio — `native_engine`, `encoder`, `probe`, `config`, `lang`, `types`
//! 4. orquestração — `bridge` (handlers), `view` (core → interface),
//!    `app_view` (a janela Slint), `ui` (host: janela oculta, bandeja, laço)
//!
//! Biblioteca + binário: o `main.rs` só chama [`run_app`]. Os testes rodam
//! sempre FORA do app: `#[cfg(test)]` (só no `cargo test`) e os executáveis
//! de `tests/` (feature `app-tests`, ver `modulos!`).

// MIGRAÇÃO PRA SLINT (docs/ui-slint.md): os handlers de telas ainda não
// portadas só são alcançados pelo `bridge::dispatch`, que no app não tem mais
// quem chame — no build do app eles aparecem como código morto. Os testes os
// exercitam (lá o aviso continua ligado). SAI NA FASE 6.
#![cfg_attr(not(any(test, feature = "app-tests")), allow(dead_code))]

/// Os módulos são PRIVADOS no build normal: o exe não expõe nada. Só com a
/// feature `app-tests` eles viram públicos, pros executáveis de teste de
/// `tests/` — que rodam FORA do app — chamarem o motor e o dispatch direto.
/// Nenhum código de teste entra no exe por aqui: a feature só muda a
/// visibilidade.
macro_rules! modulos {
    ($($m:ident),* $(,)?) => {
        $(
            #[cfg(not(feature = "app-tests"))]
            mod $m;
            #[cfg(feature = "app-tests")]
            #[doc(hidden)]
            pub mod $m;
        )*
    };
}

// `log` e `ffi` fora da macro: eles definem macros `#[macro_export]` (`log!`,
// `dll_api!`), que não podem nascer de um módulo declarado por macro — o
// `use crate::log` / `use crate::dll_api` dos outros módulos quebraria.
#[cfg(not(feature = "app-tests"))]
mod log;
#[cfg(feature = "app-tests")]
#[doc(hidden)]
pub mod log;
#[cfg(not(feature = "app-tests"))]
mod ffi;
#[cfg(feature = "app-tests")]
#[doc(hidden)]
pub mod ffi;

/// Motor de captura (docs/motor-proprio.md). Público: o executável de teste
/// `tests/captura.rs` o usa direto.
pub mod capture;

modulos! {
    app_library,
    app_main,
    app_player,
    app_export,
    app_settings,
    app_view,
    sounds,
    view,
    assets,
    native_engine,
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
    hotkey,
    hotkeys,
    lang,
    numbers_pt,
    local_asr,
    lock_detector,
    mainthread,
    mic_watch,
    playback,
    player,
    probe,
    proc_watch,
    rec_indicator,
    record_icon,
    record_watch,
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
    let cmd = win::cmdline_lower();
    log::init_log();
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

    lang::init_language();

    bridge::install_hooks();
    ui::run();
    log::log("Dispatcher: ui::run retornou — processo encerrando.");
}

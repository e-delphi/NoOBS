//! Despachante central UI ↔ Rust (porte do esqueleto de `OBSBridge.pas`).
//!
//! JS → Rust: `window.chrome.webview.postMessage(jsonString)` →
//! `ui::on_web_message` → [`dispatch`]. Rust → JS: [`post_json`] →
//! `PostWebMessageAsJson` → `Bridge.handlers[type]` no `bridge.js`.
//!
//! O `if MsgType = '...' then ... else if ...` de ~150 ramos do Delphi vira um
//! `match` sobre o campo `type`. O protocolo (nomes de mensagem e campos) NÃO
//! muda: a UI em `exe\bin\64bit\ui\` é a mesma.
//!
//! ## Estado do porte
//! Implementados: `ready`/init, tema, idioma, toggles de dispositivo,
//! `open_url`, `ui_log`. Os demais tipos caem em [`not_ported`], que registra
//! no log e devolve um erro à UI — os handlers de gravação/exportação/
//! transcrição dependem do `OBSEngine`/`FFmpegOps`/`FFmpegExport`/
//! `OBSTranscribe`, ainda não portados (ver `rust/README.md`).

use serde_json::{json, Value};

use crate::{config, engine, file_meta, lang, log, mainthread, player, record_watch, ui};

mod devices;
pub mod export;
mod filters;
mod folders;
mod library;
mod recording;
mod replay;
mod settings;
mod shell;
pub mod transcribe;

use std::sync::atomic::{AtomicBool, Ordering};

/// Idle de hibernação: 1 min sem janela visível e sem gravação (porte futuro
/// do `OBSHibernate`); o id já existe porque a gravação o desarma.
pub const TIMER_HIBERNATE_IDLE: usize = 7008;
/// Dados lidos do fim de um arquivo (`file_meta`) mudam o que o card mostra.
pub const TIMER_FILEMETA_REFRESH: usize = 7017;

/// Lido por TODAS as workers: abortam cedo quando o app está fechando. Um
/// `AtomicBool` com `SeqCst` dá a atomicidade e a barreira de memória que o
/// Delphi buscava com `TInterlocked`.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

pub fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

/// Liga a UI a este bridge sem dependência direta de `ui` → `bridge` (o
/// `initialization` do Delphi). Chamado uma vez, antes de `ui::run`.
pub fn install_hooks() {
    recording::install();
    filters::install();
    shell::install();
    ui::set_hooks(ui::Hooks {
        on_timer: Some(on_timer),
        on_hotkey: Some(recording::on_hotkey),
        on_display_change: Some(devices::on_display_change),
        on_device_change: Some(devices::on_device_node_change),
        on_should_hide_on_close: Some(shell::should_hide_on_close),
        on_window_hidden: Some(shell::on_window_hidden),
        on_window_restored: Some(shell::on_window_restored),
        on_shutdown: Some(shutdown),
    });
}

/// `OnTimer`: cada módulo reconhece os próprios ids; `false` = não é dele.
fn on_timer(id: usize) {
    if recording::on_timer(id) || replay::on_timer(id) || devices::on_timer(id) || filters::on_timer(id) || settings::on_timer(id) {
        return;
    }
    on_timer_other(id);
}

/// Encerra o que está vivo; idempotente (o `ShutdownDone` do Delphi).
pub fn shutdown() {
    // Chamado de DOIS lugares no Delphi (o Run e a finalization); aqui basta o
    // hook, mas a guarda fica: repetir o teardown do libobs seria AV.
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    log::log("Shutdown: inicio");
    ui::kill_timer(settings::TIMER_UPDATE_CHECK);
    ui::kill_timer(TIMER_HIBERNATE_IDLE);
    crate::mic_watch::stop();
    crate::proc_watch::stop();
    replay::shutdown();
    filters::shutdown();
    devices::shutdown();
    log::log("Shutdown: dispositivos off");
    record_watch::stop();
    log::log("Shutdown: RecordWatch ok");
    recording::shutdown();
    player::stop_player_server();
    transcribe::shutdown();
    file_meta::shutdown();
    log::log("Shutdown: fim");
}

/// Timers dos módulos ainda não portados caem aqui.
fn on_timer_other(id: usize) {
    if id == TIMER_FILEMETA_REFRESH {
        ui::kill_timer(TIMER_FILEMETA_REFRESH);
        library::push_recordings();
        return;
    }
    if id == TIMER_HIBERNATE_IDLE {
        shell::on_hibernate_idle();
        return;
    }
    log!("OnTimer: id={id} sem tratador (módulo ainda não portado).");
}

/// Envia um objeto JSON à UI (qualquer thread: vai pela fila da main).
pub fn post_json(value: Value) {
    mainthread::queue(move || ui::post_json_now(&value.to_string()));
}

/// Mostra erro na UI (`type: "error"`), já traduzido.
pub fn post_error(message: impl Into<String>) {
    post_json(json!({ "type": "error", "message": message.into() }));
}

fn str_field<'a>(obj: &'a Value, key: &str) -> &'a str {
    obj.get(key).and_then(Value::as_str).unwrap_or("")
}

fn bool_field(obj: &Value, key: &str, default: bool) -> bool {
    obj.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn int_field(obj: &Value, key: &str, default: i32) -> i32 {
    obj.get(key).and_then(Value::as_f64).map_or(default, |f| f as i32)
}

/// Entrada única das mensagens vindas do JS.
pub fn dispatch(raw: &str) {
    let Ok(msg) = serde_json::from_str::<Value>(raw) else {
        log::log("Dispatch: payload não é JSON — descartado.");
        return;
    };
    if !msg.is_object() {
        log::log("Dispatch: payload não é JSON object — descartado.");
        return;
    }
    let ty = str_field(&msg, "type");
    if ty != "ui_log" && ty != "player_state" {
        log!("Dispatch: type=\"{ty}\"");
    }

    match ty {
        "ui_log" => log!("UI: {}", str_field(&msg, "message")),
        "ready" => do_init(),
        "record_start" => recording::handle_record_start(),
        "record_stop" => recording::handle_record_stop(),
        "record_pause" => recording::handle_record_pause(bool_field(&msg, "paused", false)),
        "set_theme" => handle_set_theme(str_field(&msg, "theme")),
        "set_language" => handle_set_language(str_field(&msg, "language")),
        "toggle_source" => devices::handle_toggle_source(str_field(&msg, "id"), bool_field(&msg, "enabled", false)),
        "set_source_hidden" => devices::handle_set_source_hidden(str_field(&msg, "id"), bool_field(&msg, "hidden", false)),
        "set_auto_source" => devices::handle_set_auto_source(str_field(&msg, "id"), bool_field(&msg, "enabled", true)),
        "player_state" => {
            // O modal do player abriu/fechou: suspende medidores e prévias
            // enquanto aberto (a sidebar está atrás dele).
            let open = bool_field(&msg, "open", false);
            devices::set_player_open_all(open);
            log!("PlayerOpen={open}");
        }
        "rename_recording" => library::handle_rename_recording(str_field(&msg, "id"), str_field(&msg, "newName")),
        "open_recording" => library::handle_open_recording(str_field(&msg, "id")),
        "open_record_dir" => library::handle_open_record_dir(),
        "play_recording" => library::handle_play_recording(str_field(&msg, "id")),
        "request_transcode" => library::handle_request_transcode(str_field(&msg, "id")),
        "request_video_info" => library::handle_request_video_info(str_field(&msg, "id")),
        "request_audio_tracks" => library::handle_request_audio_tracks(str_field(&msg, "id")),
        "request_waveform" => library::handle_request_waveform(
            str_field(&msg, "id"),
            msg.get("buckets").and_then(Value::as_i64).unwrap_or(0),
            bool_field(&msg, "hi", false),
        ),
        "request_keyframes" => library::handle_request_keyframes(str_field(&msg, "id")),
        "export_recording" => export::handle_export_recording(&msg),
        "cancel_export" => export::handle_cancel_export(),
        "delete_recording" => library::handle_delete_recording(str_field(&msg, "id")),
        "open_url" => handle_open_url(str_field(&msg, "url")),
        "get_audio_filters" => filters::handle_get_audio_filters(),
        "set_audio_filter" => filters::handle_set_audio_filter(&msg),
        "reset_audio_filters" => filters::handle_reset_audio_filters(),
        "test_audio_filters" => filters::handle_test_audio_filters(str_field(&msg, "device")),
        "set_transcribe_host" => transcribe::handle_set_transcribe_host(str_field(&msg, "host")),
        "test_transcribe_host" => transcribe::handle_test_transcribe_host(str_field(&msg, "host")),
        "set_transcribe_language" => transcribe::handle_set_transcribe_language(str_field(&msg, "language")),
        "set_transcribe_on_stop" => transcribe::handle_set_transcribe_on_stop(bool_field(&msg, "enabled", false)),
        "check_transcribe_setup" => transcribe::handle_check_transcribe_setup(str_field(&msg, "host"), int_field(&msg, "seq", 0)),
        "transcribe_setup_action" => transcribe::handle_transcribe_setup_action(str_field(&msg, "action")),
        "set_transcribe_engine" => transcribe::handle_set_transcribe_engine(str_field(&msg, "engine")),
        "local_asr_install" => crate::local_asr::start_install(),
        "local_asr_cancel" => crate::local_asr::cancel_install(),
        "local_asr_remove" => transcribe::handle_local_asr_remove(),
        "get_local_asr_state" => transcribe::push_local_asr_state(false),
        "transcribe_recording" => transcribe::handle_transcribe_recording(str_field(&msg, "id")),
        "transcribe_pending" => transcribe::handle_transcribe_pending(),
        "cancel_transcribe" => transcribe::handle_cancel_transcribe(),
        "move_transcribe_item" => transcribe::handle_move_transcribe_item(str_field(&msg, "id"), int_field(&msg, "to", 0)),
        "remove_transcribe_item" => transcribe::handle_remove_transcribe_item(str_field(&msg, "id")),
        "get_transcribe_state" => transcribe::handle_get_transcribe_state(),
        "request_transcript" => transcribe::handle_request_transcript(str_field(&msg, "id")),
        "set_speaker_name" => transcribe::handle_set_speaker_name(str_field(&msg, "id"), str_field(&msg, "speaker"), str_field(&msg, "name")),
        "search_transcripts" => transcribe::handle_search_transcripts(str_field(&msg, "query")),
        "get_root_folders" => transcribe::push_root_folders(),
        "set_folder_hidden" => transcribe::handle_set_folder_hidden(str_field(&msg, "name"), bool_field(&msg, "hidden", false)),
        "open_folder" => folders::handle_open_folder(str_field(&msg, "id")),
        "create_folder" => folders::handle_create_folder(str_field(&msg, "name")),
        "rename_folder" => folders::handle_rename_folder(str_field(&msg, "id"), str_field(&msg, "newName")),
        "delete_folder" => folders::handle_delete_folder(str_field(&msg, "id")),
        "move_items" => {
            if let Some(ids) = msg.get("ids").and_then(Value::as_array) {
                folders::handle_move_items(ids, str_field(&msg, "target"));
            }
        }
        "merge_recordings" => {
            if let Some(ids) = msg.get("ids").and_then(Value::as_array) {
                folders::handle_merge_recordings(ids);
            }
        }
        "get_replay_state" => replay::push_replay_state(),
        "set_replay_enabled" => replay::handle_set_replay_enabled(bool_field(&msg, "enabled", false)),
        "save_replay" => replay::handle_save_replay(),
        "set_replay_apps" => replay::handle_set_replay_apps(str_field(&msg, "apps")),
        "set_replay_indicator" => replay::handle_set_replay_indicator(bool_field(&msg, "enabled", false)),
        "set_replay_indicator_corner" => replay::handle_set_replay_indicator_corner(str_field(&msg, "corner")),
        "set_replay_indicator_opacity" => replay::handle_set_replay_indicator_opacity(int_field(&msg, "opacity", 90)),
        "set_replay_indicator_clickable" => replay::handle_set_replay_indicator_clickable(bool_field(&msg, "enabled", false)),
        "set_replay_autostart" => replay::handle_set_replay_autostart(bool_field(&msg, "autoStart", false)),
        "set_replay_into_recording" => replay::handle_set_replay_into_recording(bool_field(&msg, "enabled", false)),
        "set_replay_limits" => replay::handle_set_replay_limits(int_field(&msg, "maxSec", 0), int_field(&msg, "maxMb", 0)),
        "set_replay_hotkey" => replay::handle_set_replay_hotkey(str_field(&msg, "hotkey")),
        "get_settings" => settings::push_settings(),
        "set_codec" => settings::handle_set_codec(str_field(&msg, "codec")),
        "set_recording_quality" => settings::handle_set_recording_quality(int_field(&msg, "level", 0)),
        "set_recording_fps" => settings::handle_set_recording_fps(int_field(&msg, "fps", 0)),
        "set_recording_keyframe" => settings::handle_set_recording_keyframe(int_field(&msg, "sec", 0)),
        "set_audio_bitrate" => settings::handle_set_audio_bitrate(int_field(&msg, "kbps", 0)),
        "set_filename_pattern" => settings::handle_set_filename_pattern(str_field(&msg, "pattern")),
        "set_mute_when_device_muted" => settings::handle_set_mute_when_device_muted(bool_field(&msg, "enabled", false)),
        "pick_record_dir" => settings::handle_pick_record_dir(),
        "set_record_dir" => settings::handle_set_record_dir(str_field(&msg, "path")),
        "set_library_thumbs" => settings::handle_set_library_thumbs(str_field(&msg, "mode")),
        "set_auto_record_on_mic" => settings::handle_set_auto_record_on_mic(bool_field(&msg, "enabled", false)),
        "set_auto_record_mic_apps" => settings::handle_set_auto_record_mic_apps(str_field(&msg, "apps")),
        "set_auto_record_mic_except" => settings::handle_set_auto_record_mic_except(str_field(&msg, "apps")),
        "check_updates" => settings::handle_check_updates(),
        "set_check_updates" => settings::handle_set_check_updates(bool_field(&msg, "enabled", false)),
        "tray_show" => shell::handle_tray_show(),
        "toggle_fullscreen" => ui::toggle_fullscreen(),
        "set_window_title" => shell::handle_set_window_title(str_field(&msg, "title")),
        "set_hotkey" => shell::handle_set_hotkey(str_field(&msg, "hotkey")),
        "validate_hotkey" => shell::handle_validate_hotkey(str_field(&msg, "hotkey")),
        "set_autostart" => shell::handle_set_autostart(bool_field(&msg, "enabled", false)),
        "set_run_as_admin" => shell::handle_set_run_as_admin(bool_field(&msg, "enabled", false)),
        "set_close_to_tray" => shell::handle_set_close_to_tray(bool_field(&msg, "enabled", false)),
        "set_hibernate" => shell::handle_set_hibernate(bool_field(&msg, "enabled", false)),
        "set_minimize_on_record" => shell::handle_set_config_bool("minimizeOnRecord", bool_field(&msg, "enabled", false)),
        "set_notify_on_record" => shell::handle_set_config_bool("notifyOnRecord", bool_field(&msg, "enabled", false)),
        "set_play_sound_on_record" => shell::handle_set_config_bool("playSoundOnRecord", bool_field(&msg, "enabled", false)),
        "set_stop_on_lock" => shell::handle_set_config_bool("stopOnLock", bool_field(&msg, "enabled", false)),
        "set_rec_indicator" => recording::handle_set_rec_indicator(bool_field(&msg, "enabled", false)),
        "set_rec_indicator_corner" => recording::handle_set_rec_indicator_corner(str_field(&msg, "corner")),
        "set_rec_indicator_opacity" => recording::handle_set_rec_indicator_opacity(int_field(&msg, "opacity", 90)),
        "set_rec_indicator_clickable" => recording::handle_set_rec_indicator_clickable(bool_field(&msg, "enabled", true)),
        "set_fullscreen_detect" => recording::handle_set_fullscreen_detect(bool_field(&msg, "enabled", true)),
        other => not_ported(other),
    }
}

fn not_ported(ty: &str) {
    log!("Dispatch: type=\"{ty}\" ainda não portado para Rust.");
}

// ---------------------------------------------------------------------------
// init / tema / idioma
// ---------------------------------------------------------------------------

/// `DoInit`: tema + `init` + dispositivos (áudio em worker, prévias, medidores,
/// hot-plug) + warmup do libobs. O servidor do player, a biblioteca e os
/// watchers de pasta/mic/apps entram com os módulos que os têm.
fn do_init() {
    shell::set_ui_ready();
    push_theme();
    // HTTP local das prévias/vídeos; falhar aqui não derruba o app.
    player::start_player_server();
    log::log("DoInit: PlayerServer ok");
    // Worker que escreve os dados no fim do arquivo (pendentes da sessão anterior
    // voltam). Arquivo importado de outra máquina refaz os cards (pegadinha #65).
    file_meta::set_on_imported(|_| {
        mainthread::queue(|| {
            if !is_shutting_down() {
                ui::set_timer(TIMER_FILEMETA_REFRESH, 800);
            }
        });
    });
    file_meta::start();
    // Fila de transcrição: só registra os callbacks e restaura a salva.
    transcribe::do_init();
    // Lista e metadados (duração/miniatura) da pasta de gravação.
    library::push_recordings();
    library::scan_recordings_meta();
    log::log("DoInit: recordings ok");
    devices::do_init();
    // Refresh automático quando o usuário mexe na pasta pelo Explorer.
    library::start_record_watch();
    // Warmup do libobs com delay: a UI renderiza primeiro e a 1ª gravação fica
    // instantânea. NUNCA inicializar o libobs direto aqui (pode travar o
    // startup). Falha do warmup → a gravação inicializa sob demanda.
    recording::schedule_warmup();
    // Atalho global (falha de registro não é fatal — pegadinha #33).
    shell::apply_hotkey_from_config();
    // Indicador de tela + parar ao bloquear a tela.
    recording::init_indicator_and_lock();
    // Auto-gravação em chamadas (se ligada), buffer ligado por programas
    // monitorados e checagem diária de atualização.
    settings::start_mic_watch_if_enabled();
    replay::do_init();
    settings::start_update_timer();
    log::log("DoInit: pronto (sem OBS — sobe no warmup ou na hora da gravacao).");
}

/// Resolve o tema configurado: `system` (segue o SO), `dark` ou `light`.
pub fn resolved_theme() -> String {
    let mut cfg = config::get_config_str("theme", "");
    if cfg.is_empty() {
        // 1ª execução: marca "estou seguindo o Windows", não foi escolha do user.
        cfg = "system".into();
        config::set_config_str("theme", &cfg);
    }
    match cfg.as_str() {
        "dark" | "light" => cfg,
        _ => crate::win::detect_system_theme().to_owned(),
    }
}

pub fn push_theme() {
    let pref = config::get_config_str("theme", "system");
    let resolved = resolved_theme();
    log!("PushTheme: cfg=\"{pref}\" resolved=\"{resolved}\"");
    ui::set_dark_mode(resolved == "dark");
    // Popup do menu da bandeja no mesmo tema.
    ui::apply_tray_menu_theme(resolved == "dark");
    post_json(json!({ "type": "theme", "theme": resolved, "pref": pref }));
}

fn handle_set_theme(theme: &str) {
    if !matches!(theme, "dark" | "light" | "system") {
        return;
    }
    config::set_config_str("theme", theme);
    push_theme();
}

fn handle_set_language(code: &str) {
    let pref = code.trim();
    config::set_config_str("language", pref);
    log!("Language: pref=\"{pref}\"");
    lang::init_language();
    post_json(json!({
        "type": "language_changed",
        "language": lang::current_language(),
        "languagePref": pref,
        "i18n": lang::current_bundle(),
    }));
    // Rótulos dos filtros de áudio vêm do plugin do OBS: troca o idioma dele também (só se o
    // libobs já subiu; senão o init já pega o novo).
    if engine::with_engine(|e| e.is_initialized()).unwrap_or(false) {
        crate::audio_filters::set_obs_locale_from_app(&lang::current_language());
    }
}

// ---------------------------------------------------------------------------
// misc
// ---------------------------------------------------------------------------

/// Só http(s): a UI poderia mandar `file://` etc.
fn handle_open_url(url: &str) {
    if url.is_empty() {
        return;
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        log!("HandleOpenUrl: URL rejeitada (não http/https): {url}");
        return;
    }
    ui::shell_open(url);
}

//! Tela de Configurações (partes do `OBSBridge.pas`): o push `settings` com
//! tudo o que é configurável e os `set_*` de gravação, pasta, biblioteca,
//! auto-gravação por microfone e atualização.
//!
//! Main thread.

use serde_json::json;
use windows::core::HSTRING;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

use crate::{autostart, config, elevate, encoder, lang, log, mic_watch, record_watch, transcribe, ui, update, version, win_preview};

use super::{devices, library, post_error, post_json, recording};

/// Tique da checagem de atualização (barato: compara dois inteiros e sai).
pub const TIMER_UPDATE_CHECK: usize = 7011;
const UPDATE_CHECK_TICK_MS: u32 = 60_000;
/// Hora local do dia em que a checagem automática acontece.
const UPDATE_CHECK_HOUR: u32 = 6;
const DEFAULT_FILENAME_PATTERN: &str = "{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}";

/// Heurística só INFORMATIVA (dica na UI): a pasta está sob o OneDrive? A
/// decisão real de pular a prévia é por arquivo (`is_file_cloud_only`).
fn is_path_under_onedrive(path: &str) -> bool {
    let with_sep = |p: &str| {
        let p = p.trim_end_matches('\\').to_lowercase();
        format!("{p}\\")
    };
    let target = with_sep(path);
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"].iter().any(|var| {
        std::env::var(var).ok().filter(|v| !v.is_empty()).is_some_and(|od| target.starts_with(&with_sep(&od)))
    })
}

/// Tudo o que a tela de Configurações mostra.
pub fn push_settings() {
    let record_dir = recording::record_dir().to_string_lossy().into_owned();
    // '' no config (nunca mexido) é o mesmo que 'app': a UI recebe sempre um
    // valor que existe no seletor.
    let tr_lang = match config::get_config_str("transcribeLanguage", "").trim().to_lowercase() {
        l if l.is_empty() => "app".to_owned(),
        l => l,
    };
    post_json(json!({
        "type": "settings",
        "recordDir": record_dir,
        "windowTitle": config::get_config_str("windowTitle", "NoOBS"),
        "transcribeHost": transcribe::host_base(),
        "transcribeEngine": if transcribe::use_local_engine() { "local" } else { "server" },
        "transcribeLanguage": tr_lang,
        "transcribeOnStop": config::get_config_bool("transcribeOnStop", true),
        "muteWhenDeviceMuted": config::get_config_bool("muteWhenDeviceMuted", true),
        "filenamePattern": config::get_config_str("filenamePattern", DEFAULT_FILENAME_PATTERN),
        "codec": config::get_config_str("codec", "auto"),
        "hotkey": config::get_config_str("hotkey", "Pause/Break"),
        "autostart": autostart::is_autostart_enabled(),
        "runAsAdmin": config::get_config_bool("runAsAdmin", false),
        "fullscreenDetect": config::get_config_bool("fullscreenDetect", true),
        "elevated": elevate::is_process_elevated(),
        "closeToTray": config::get_config_bool("closeToTray", true),
        "minimizeOnRecord": config::get_config_bool("minimizeOnRecord", true),
        "notifyOnRecord": config::get_config_bool("notifyOnRecord", false),
        "recIndicator": config::get_config_bool("recIndicator", false),
        "recIndicatorCorner": config::get_config_str("recIndicatorCorner", "top-right"),
        "recIndicatorOpacity": config::get_config_int("recIndicatorOpacity", 90),
        "recIndicatorClickable": config::get_config_bool("recIndicatorClickable", true),
        "playSoundOnRecord": config::get_config_bool("playSoundOnRecord", true),
        "stopOnLock": config::get_config_bool("stopOnLock", true),
        "hibernate": config::get_config_bool("hibernate", true),
        "autoRecordOnMic": config::get_config_bool("autoRecordOnMic", false),
        "autoRecordMicApps": config::get_config_str("autoRecordMicApps", ""),
        "autoRecordMicExcept": config::get_config_str("autoRecordMicExcept", ""),
        "checkUpdates": config::get_config_bool("checkUpdates", true),
        "appVersion": version::display_version(),
        "isDevBuild": version::is_dev_build(),
        // Nível 0..10 da fonte única (que migra a escala antiga — #53).
        "recordingQuality": encoder::recording_quality_level(),
        "recordingFps": config::get_config_int("recordingFps", 30),
        "recordingKeyframeSec": config::get_config_int("recordingKeyframeSec", 2),
        "audioBitrate": encoder::audio_bitrate_kbps(),
        "libraryThumbs": config::get_config_str("libraryThumbs", "auto"),
        "recordDirCloud": is_path_under_onedrive(&record_dir),
        // Teto do slider de fps: a maior taxa dos monitores agora.
        "maxMonitorHz": win_preview::max_monitor_refresh_rate(),
        "language": lang::current_language(),
        "languagePref": config::get_config_str("language", ""),
        "availableLanguages": lang::available_languages(),
    }));
    // Buffer: estado + limites + atalho num push próprio, o mesmo da tela principal.
    super::replay::push_replay_state();
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

/// Os valores aceitos acompanham o `<select>` da UI E os ramos do
/// `select_video_encoder`. Valor recusado é sempre bug nosso, então loga.
pub fn handle_set_codec(codec: &str) {
    const VALID: [&str; 6] = ["auto", "av1-hw", "hevc-hw", "h264-hw", "h264-sw", "av1-sw"];
    let Some(codec) = VALID.iter().find(|v| v.eq_ignore_ascii_case(codec)) else {
        log!("HandleSetCodec: valor recusado \"{codec}\" — fora da lista aceita.");
        return;
    };
    if codec.eq_ignore_ascii_case(&config::get_config_str("codec", "auto")) {
        return;
    }
    config::set_config_str("codec", codec);
    log!("Codec preferido alterado para: {codec}");
    super::replay::restart_for_encoder_change("codec");
}

/// Grava em `recordingQualityLevel`, NUNCA na `recordingQuality` antiga: as
/// escalas se sobrepõem em 0/1/2 (pegadinha #53).
pub fn handle_set_recording_quality(level: i32) {
    let level = level.clamp(0, 10);
    if level == encoder::recording_quality_level() {
        return;
    }
    config::set_config_int("recordingQualityLevel", level);
    log!("RecordingQuality: {level}");
    super::replay::restart_for_encoder_change("qualidade");
}

/// Mínimo 10 fps; sem máximo fixo (monitor de 360 Hz existe).
pub fn handle_set_recording_fps(fps: i32) {
    let fps = fps.max(10);
    if fps == config::get_config_int("recordingFps", 30) {
        return;
    }
    config::set_config_int("recordingFps", fps);
    log!("RecordingFps: {fps} fps");
    super::replay::restart_for_encoder_change("taxa de quadros");
}

pub fn handle_set_recording_keyframe(sec: i32) {
    let sec = sec.clamp(1, 10);
    config::set_config_int("recordingKeyframeSec", sec);
    log!("RecordingKeyframe: {sec}s");
}

/// Encaixa no degrau válido. Vale a partir do próximo grafo: encoder de áudio
/// vivo não troca de taxa no meio do arquivo.
pub fn handle_set_audio_bitrate(kbps: i32) {
    let kbps = encoder::normalize_audio_bitrate(kbps);
    if kbps == encoder::audio_bitrate_kbps() {
        return;
    }
    config::set_config_int("audioBitrate", kbps);
    log!("AudioBitrate: {kbps} kbps");
    super::replay::restart_for_encoder_change("qualidade do audio");
}

/// Guarda o modelo cru; vazio volta pro padrão. Os códigos só são trocados na
/// hora de gravar (`apply_filename_pattern`).
pub fn handle_set_filename_pattern(pattern: &str) {
    let p = match pattern.trim() {
        "" => DEFAULT_FILENAME_PATTERN,
        p => p,
    };
    config::set_config_str("filenamePattern", p);
    log!("FilenamePattern: \"{p}\"");
}

/// Aplica já: ligar/desligar no meio da gravação vale agora, não no próximo
/// mudo do microfone (pegadinha #59).
pub fn handle_set_mute_when_device_muted(enable: bool) {
    config::set_config_bool("muteWhenDeviceMuted", enable);
    log!("MuteWhenDeviceMuted: {enable}");
    devices::clear_mic_mute_applied();
    devices::sync_mic_mute_from_endpoints();
}

// ---------------------------------------------------------------------------
// Pasta de gravação e biblioteca
// ---------------------------------------------------------------------------

/// Diálogo nativo de "selecionar pasta" (`IFileOpenDialog`).
fn pick_folder(initial: &str) -> Option<String> {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let opts = dlg.GetOptions().ok()?;
        dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST).ok()?;
        let _ = dlg.SetTitle(&HSTRING::from(lang::t("settings.recordDir.label")));
        if !initial.is_empty() && std::path::Path::new(initial).is_dir() {
            if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(initial), None) {
                let _ = dlg.SetFolder(&item);
            }
        }
        dlg.Show(Some(ui::main_window())).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        path
    }
}

pub fn handle_pick_record_dir() {
    let current = recording::record_dir().to_string_lossy().into_owned();
    if let Some(picked) = pick_folder(&current) {
        post_json(json!({ "type": "record_dir_picked", "path": picked }));
    }
}

/// Vazio = restaurar o padrão (`%USERPROFILE%\Videos`). A navegação volta à
/// raiz nova e o watcher é re-apontado.
pub fn handle_set_record_dir(path: &str) {
    if path.is_empty() {
        config::set_config_str("recordDir", "");
        log!("Pasta de gravacao restaurada pro padrao: {}", recording::record_dir().display());
    } else {
        if !std::path::Path::new(path).is_dir() {
            post_error(lang::t("error.folderNotFound"));
            return;
        }
        config::set_config_str("recordDir", path);
        log!("Pasta de gravacao alterada para: {path}");
    }
    library::set_browse_dir("");
    push_settings();
    library::push_recordings();
    library::scan_recordings_meta();
    record_watch::update_dir(&recording::record_dir().to_string_lossy());
}

/// `auto` | `always` | `off` (fallback `auto`). Re-scan para aplicar já;
/// `off` não apaga caches, só para de gerar.
pub fn handle_set_library_thumbs(mode: &str) {
    let m = match mode.trim().to_lowercase().as_str() {
        "always" => "always",
        "off" => "off",
        _ => "auto",
    };
    config::set_config_str("libraryThumbs", m);
    log!("LibraryThumbs: {m}");
    library::scan_recordings_meta();
}

// ---------------------------------------------------------------------------
// Auto-gravação ao detectar uso do microfone (pegadinha #47)
// ---------------------------------------------------------------------------

/// Callback do watcher — roda NA THREAD dele; a regra roda na main.
fn on_mic_use_changed_thread(in_use: bool) {
    crate::mainthread::queue(move || {
        if super::is_shutting_down() {
            return;
        }
        // Pode ter sido desligado entre o poll e a fila.
        if !config::get_config_bool("autoRecordOnMic", false) {
            return;
        }
        if in_use {
            if !recording::recording_active() {
                log::log("AutoRecord: microfone em uso — iniciando gravacao.");
                recording::set_started_by_mic_watch(true);
                recording::handle_record_start();
            }
        } else {
            // Só para o que NÓS iniciamos — nunca uma gravação manual.
            if recording::recording_active() && recording::started_by_mic_watch() {
                log::log("AutoRecord: microfone liberado — parando gravacao.");
                recording::handle_record_stop();
            }
            recording::set_started_by_mic_watch(false);
        }
    });
}

fn start_mic_watch() {
    mic_watch::start(
        &config::get_config_str("autoRecordMicApps", ""),
        &config::get_config_str("autoRecordMicExcept", ""),
        on_mic_use_changed_thread,
    );
}

/// No `DoInit`: liga o watcher se a opção estiver ligada.
pub fn start_mic_watch_if_enabled() {
    if config::get_config_bool("autoRecordOnMic", false) {
        start_mic_watch();
    }
}

pub fn handle_set_auto_record_on_mic(enable: bool) {
    config::set_config_bool("autoRecordOnMic", enable);
    log!("AutoRecordOnMic: {enable}");
    if enable {
        if !mic_watch::is_running() {
            start_mic_watch();
        }
    } else {
        mic_watch::stop();
    }
}

/// O watcher lê os dois filtros no start: basta parar e subir.
fn restart_mic_watch_if_running() {
    if mic_watch::is_running() {
        mic_watch::stop();
        start_mic_watch();
    }
}

pub fn handle_set_auto_record_mic_apps(apps: &str) {
    config::set_config_str("autoRecordMicApps", apps.trim());
    log!("AutoRecordMicApps: \"{}\"", apps.trim());
    restart_mic_watch_if_running();
}

pub fn handle_set_auto_record_mic_except(apps: &str) {
    config::set_config_str("autoRecordMicExcept", apps.trim());
    log!("AutoRecordMicExcept: \"{}\"", apps.trim());
    restart_mic_watch_if_running();
}

// ---------------------------------------------------------------------------
// Atualização
// ---------------------------------------------------------------------------

/// Resultado → UI. Só marca a checagem como FEITA se o GitHub respondeu algo:
/// sem resposta (offline) não pode queimar as próximas 24 h.
fn on_update_checked(r: update::UpdateResult) {
    if r.status_code != 0 {
        config::set_config_int("lastUpdateCheck", chrono::Utc::now().timestamp() as i32);
    } else {
        log::log("Update: sem resposta do servidor — lastUpdateCheck preservado.");
    }
    post_json(json!({
        "type": "update_result",
        "ok": r.ok,
        "hasUpdate": r.has_update,
        "latest": r.latest_tag,
        "url": r.release_url,
        "error": r.error,
    }));
}

pub fn handle_check_updates() {
    update::check_for_updates(on_update_checked);
}

pub fn handle_set_check_updates(enable: bool) {
    config::set_config_bool("checkUpdates", enable);
    log!("CheckUpdates: {enable}");
}

/// No startup e a cada minuto: uma checagem por dia, ANCORADA às 6 h locais
/// (release raramente sai no meio do dia; com janela deslizante o horário
/// escorregava pra quando o app abriu). Nunca em build de desenvolvimento.
pub fn maybe_auto_check_updates() {
    use chrono::{Duration, Local, TimeZone};
    if !config::get_config_bool("checkUpdates", true) || update::is_checking() || version::is_dev_build() {
        return;
    }
    let last = config::get_config_int("lastUpdateCheck", 0) as i64;
    let now = Local::now();
    // Marco = o último 6 h que JÁ passou (hoje, ou ontem se ainda não deu 6 h).
    let today = now.date_naive().and_hms_opt(UPDATE_CHECK_HOUR, 0, 0).unwrap_or_default();
    let mut marco = Local.from_local_datetime(&today).single().unwrap_or(now);
    if now < marco {
        marco -= Duration::days(1);
    }
    let (agora, marco) = (now.timestamp(), marco.timestamp());
    // last > agora: relógio voltou — trata como nunca checou. last = 0 checa
    // na hora (instalação nova).
    if last > 0 && last <= agora && last >= marco {
        return;
    }
    log!("Update: iniciando checagem automatica (ultima ha {}s).", agora - last);
    update::check_for_updates(on_update_checked);
}

pub fn start_update_timer() {
    maybe_auto_check_updates();
    ui::set_timer(TIMER_UPDATE_CHECK, UPDATE_CHECK_TICK_MS);
}

pub fn on_timer(id: usize) -> bool {
    if id == TIMER_UPDATE_CHECK {
        maybe_auto_check_updates();
        return true;
    }
    false
}

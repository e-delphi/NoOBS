//! Orquestração entre a interface e o core (porte do esqueleto de
//! `OBSBridge.pas`): os handlers de cada área ficam nos submódulos (gravação,
//! dispositivos, biblioteca, pastas, exportação, transcrição, buffer, janela,
//! área, teste de vídeo, shell, settings).
//!
//! ## Migração pra Slint (docs/ui-slint.md)
//! A interface chama os handlers DIRETO e recebe os eventos pelo
//! [`crate::view::View`], sem JSON. Enquanto as telas são portadas, sobram do
//! protocolo antigo:
//! * [`dispatch`] — a entrada por JSON que os testes usam (some quando a última
//!   tela deixar de precisar dela);
//! * [`post_json`] — eventos de telas ainda não portadas: nos testes viram o
//!   registro de sempre; no app não têm destino.

use serde_json::Value;

use crate::{config, file_meta, lang, log, mainthread, player, record_watch, ui, view};

mod area;
mod countdown;
mod devices;
pub mod export;
mod folders;
mod hotkeys;
mod library;
mod recording;
mod replay;
mod settings;
mod shell;
pub mod transcribe;
mod video_test;
mod window_record;

#[cfg(test)]
mod tests;

/// Testes: a navegação da biblioteca volta à raiz (estado por thread).
#[cfg(test)]
pub fn test_reset_browse_dir() {
    library::set_browse_dir("");
}

use std::sync::atomic::{AtomicBool, Ordering};

/// Dados lidos do fim de um arquivo (`file_meta`) mudam o que o card mostra.
pub const TIMER_FILEMETA_REFRESH: usize = 7017;

/// Lido por TODAS as workers: abortam cedo quando o app está fechando. Um
/// `AtomicBool` com `SeqCst` dá a atomicidade e a barreira de memória que o
/// Delphi buscava com `TInterlocked`.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
/// A interface já subiu (o `Initialized` do Delphi).
static INIT_DONE: AtomicBool = AtomicBool::new(false);
/// Backend de pé (`start_backend`).
static BACKEND_STARTED: AtomicBool = AtomicBool::new(false);

pub fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

/// A interface já subiu (`init_ui`)?
pub fn ui_ready() -> bool {
    INIT_DONE.load(Ordering::SeqCst)
}

/// Liga a UI a este bridge sem dependência direta de `ui` → `bridge` (o
/// `initialization` do Delphi). Chamado uma vez, antes de `ui::run`.
pub fn install_hooks() {
    recording::install();
    shell::install();
    ui::set_hooks(ui::Hooks {
        on_timer: Some(on_timer),
        on_hotkey: Some(recording::on_hotkey),
        on_display_change: Some(devices::on_display_change),
        on_device_change: Some(devices::on_device_node_change),
        on_should_hide_on_close: Some(shell::should_hide_on_close),
        on_window_restored: Some(shell::on_window_restored),
        on_start: Some(start),
        on_shutdown: Some(shutdown),
    });
}

/// `OnTimer`: cada módulo reconhece os próprios ids; `false` = não é dele.
fn on_timer(id: usize) {
    // Como o try/except do OnTimer do Delphi: um timer que falha não derruba o app.
    if std::panic::catch_unwind(|| on_timer_inner(id)).is_err() {
        log!("OnTimer: excecao no timer {id} (ver PANIC acima) — seguindo.");
    }
}

fn on_timer_inner(id: usize) {
    if recording::on_timer(id) || replay::on_timer(id) || devices::on_timer(id) || settings::on_timer(id) || video_test::on_timer(id) || countdown::on_timer(id) {
        return;
    }
    on_timer_other(id);
}

/// Encerra o que está vivo; idempotente (o `ShutdownDone` do Delphi).
pub fn shutdown() {
    // Chamado de DOIS lugares no Delphi (o Run e a finalization); aqui basta o
    // hook, mas a guarda fica: parar o motor duas vezes não é seguro.
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    log::log("Shutdown: inicio");
    ui::kill_timer(settings::TIMER_UPDATE_CHECK);
    crate::mic_watch::stop();
    crate::proc_watch::stop();
    // Como o Shutdown do Delphi: a exportação pode estar no meio de um arquivo
    // longo e o laço dela olha o PRÓPRIO flag de cancelamento, não o de
    // shutdown — sem isto ela seguia até o processo morrer, sem limpar o .part.
    export::signal_cancel();
    export::clear_previews();
    video_test::shutdown();
    replay::shutdown();
    countdown::shutdown();
    hotkeys::shutdown();
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
    log!("OnTimer: id={id} sem tratador (módulo ainda não portado).");
}

/// Evento no formato da página antiga. Só os testes recebem; no app, é de uma
/// tela ainda não portada pra Slint e não tem destino (some com a fase dela).
pub fn post_json(value: Value) {
    #[cfg(test)]
    {
        crate::testkit::record_sent(value);
        return;
    }
    #[cfg(all(feature = "app-tests", not(test)))]
    {
        test_sent().lock().unwrap_or_else(|p| p.into_inner()).push(value);
        return;
    }
    #[allow(unreachable_code)]
    let _ = value;
}

#[cfg(feature = "app-tests")]
fn test_sent() -> &'static std::sync::Mutex<Vec<Value>> {
    static SENT: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());
    &SENT
}

/// Só com `app-tests`: tira (e esvazia) o que o Bridge mandou à página.
#[cfg(feature = "app-tests")]
pub fn take_sent_for_tests() -> Vec<Value> {
    std::mem::take(&mut *test_sent().lock().unwrap_or_else(|p| p.into_inner()))
}

/// Mostra erro na interface, já traduzido.
pub fn post_error(message: impl Into<String>) {
    let message = message.into();
    view::call(move |v| v.error(&message));
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
/// Uma falha num handler não derruba o app: registra e segue, como o
/// `try/except` do `Dispatch` do Delphi ("excecao no handler").
pub fn dispatch(raw: &str) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch_inner(raw))).is_err() {
        let ty = serde_json::from_str::<Value>(raw).ok().map(|m| str_field(&m, "type").to_owned()).unwrap_or_default();
        log!("Dispatch: excecao no handler type=\"{ty}\" (ver PANIC acima) — seguindo.");
    }
}

fn dispatch_inner(raw: &str) {
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
        // A página antiga avisava que tinha carregado; os testes ainda mandam.
        "ready" => init_ui(),
        // Na contagem do atraso da área, o botão de gravar CANCELA.
        "record_start" => {
            if !countdown::cancel() {
                recording::handle_record_start();
            }
        }
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
        "set_transcribe_language" => transcribe::handle_set_transcribe_language(str_field(&msg, "language")),
        "set_transcribe_on_stop" => transcribe::handle_set_transcribe_on_stop(bool_field(&msg, "enabled", false)),
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
        "reset_hidden_lists" => transcribe::handle_reset_hidden_lists(),
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
        "validate_hotkey" => shell::handle_validate_hotkey(str_field(&msg, "hotkey")),
        "set_autostart" => shell::handle_set_autostart(bool_field(&msg, "enabled", false)),
        "set_run_as_admin" => shell::handle_set_run_as_admin(bool_field(&msg, "enabled", false)),
        "set_close_to_tray" => shell::handle_set_close_to_tray(bool_field(&msg, "enabled", false)),
        "set_minimize_on_record" => shell::handle_set_config_bool("minimizeOnRecord", bool_field(&msg, "enabled", false)),
        "set_play_sound_on_record" => shell::handle_set_config_bool("playSoundOnRecord", bool_field(&msg, "enabled", false)),
        "set_stop_on_lock" => shell::handle_set_config_bool("stopOnLock", bool_field(&msg, "enabled", false)),
        "set_rec_indicator" => recording::handle_set_rec_indicator(bool_field(&msg, "enabled", false)),
        "set_rec_indicator_corner" => recording::handle_set_rec_indicator_corner(str_field(&msg, "corner")),
        "set_rec_indicator_opacity" => recording::handle_set_rec_indicator_opacity(int_field(&msg, "opacity", 90)),
        "set_rec_indicator_clickable" => recording::handle_set_rec_indicator_clickable(bool_field(&msg, "enabled", true)),
        "set_fullscreen_detect" => recording::handle_set_fullscreen_detect(bool_field(&msg, "enabled", true)),
        "list_capture_windows" => window_record::handle_list_capture_windows(),
        "record_window" => window_record::handle_record_window(
            str_field(&msg, "id"),
            bool_field(&msg, "appAudio", false),
            int_field(&msg, "delaySec", 0).max(0) as u32,
        ),
        "select_capture_area" => area::handle_select_capture_area(),
        "cancel_countdown" => {
            countdown::cancel();
        }
        "get_hotkeys" => hotkeys::push_state(),
        "reset_hotkeys" => hotkeys::handle_reset(),
        "set_hotkeys" => hotkeys::handle_set_hotkeys(msg.get("items").unwrap_or(&Value::Null)),
        "define_hotkey_area" => hotkeys::handle_define_area(int_field(&msg, "index", -1).max(0) as usize),
        "trigger_hotkey" => {
            let i = int_field(&msg, "index", -1);
            if i >= 0 {
                hotkeys::trigger(i as usize);
            }
        }
        "test_video_quality" => video_test::handle_test_video_quality(),
        "cancel_video_test" => video_test::handle_cancel_video_test(),
        other => not_ported(other),
    }
}

fn not_ported(ty: &str) {
    log!("Dispatch: type=\"{ty}\" ainda não portado para Rust.");
}

// ---------------------------------------------------------------------------
// init / tema / idioma
// ---------------------------------------------------------------------------

/// Arranque (hook `on_start` do `ui::run`, com a janela já criada): backend e
/// interface, nessa ordem.
fn start() {
    start_backend();
    init_ui();
}

/// Backend: atalhos, vigias de mic e de programas, fila de transcrição,
/// escrita de metadados, indicador e bloqueio de tela, checagem de
/// atualização. Uma vez por processo.
pub fn start_backend() {
    if BACKEND_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    log::log("Backend: inicio.");
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
    // Lista de atalhos (aba Atalhos; falha de registro não é fatal —
    // pegadinha #33).
    hotkeys::do_init();
    // Indicador de tela + parar ao bloquear a tela.
    recording::init_indicator_and_lock();
    // Auto-gravação em chamadas (se ligada), buffer ligado por programas
    // monitorados e checagem diária de atualização.
    settings::start_mic_watch_if_enabled();
    replay::do_init();
    settings::start_update_timer();
    log::log("Backend: pronto.");
}

/// `DoInit`: o que a interface precisa logo que existe — tema, lista de
/// gravações, dispositivos, configurações, estado da gravação. Uma vez por
/// processo (os testes chamam pelo `ready`; o que já está de pé é idempotente).
fn init_ui() {
    let first = !INIT_DONE.swap(true, Ordering::SeqCst);
    start_backend();
    log!("DoInit: interface pronta ({}).", if first { "primeira" } else { "de novo" });
    push_theme();
    // HTTP local das prévias/vídeos; falhar aqui não derruba o app.
    player::start_player_server();
    // Lista e metadados (duração/miniatura) da pasta de gravação.
    library::push_recordings();
    library::scan_recordings_meta();
    devices::do_init();
    // Configurações JÁ no arranque, como o PushSettings do DoInit do Delphi.
    // Sem isso a tela de Configurações abria com tudo desmarcado: com ela
    // aberta o `applySettings` ignora a resposta do `get_settings` (protege o
    // que o usuário está editando), então nada corrigia as caixas — e salvar
    // gravava as opções ligadas como desligadas. Também deixa certos desde o
    // início os valores que a UI usa fora da tela (som ao gravar etc.).
    settings::push_settings();
    recording::push_recording_state();
    hotkeys::push_state();
    // Refresh automático quando o usuário mexe na pasta pelo Explorer.
    library::start_record_watch();
    // Warmup do motor com delay: a UI renderiza primeiro e a 1ª gravação fica
    // instantânea. NUNCA inicializar o motor direto aqui (detectar encoders
    // bloqueia a main). Falha do warmup → a gravação inicializa sob demanda.
    // Com o motor já de pé (gravando, buffer), as caps vão na hora.
    recording::on_page_ready();
    // Exportação em curso: a barra volta no card.
    export::on_page_ready();
    // Atalho de janela que achou várias janelas antes de a interface subir.
    window_record::on_page_ready();
    if !first {
        return;
    }
    log::log("DoInit: pronto (o motor sobe no warmup ou na hora da gravacao).");
    // Com "minimizar pra bandeja ao fechar" ligado, o ícone fica na bandeja
    // mesmo com a janela aberta (avisa que o [X] vai pra bandeja) — como o
    // DoInit do Delphi. Sem ele, o ícone só nascia quando a 1ª gravação
    // escondia a janela, e nascia sem a bolinha de gravando.
    if config::get_config_bool("closeToTray", true) {
        ui::ensure_tray_icon();
    }
    // 1ª execução depois de instalar: abre as Configurações pro usuário ajustar
    // antes de começar a usar.
    if consume_first_run_flag() {
        log::log("DoInit: primeira execucao detectada — abrindo Configuracoes.");
        view::call(|v| v.open_settings());
    }
}

/// `HKCU\Software\NoOBS\FirstRun` = 1 (escrito pelo instalador na instalação):
/// devolve true UMA vez e apaga o valor.
fn consume_first_run_flag() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut size = 4u32;
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\NoOBS"),
            w!("FirstRun"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    }
    .is_ok();
    if !found || value != 1 {
        return false;
    }
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, w!("Software\\NoOBS"), w!("FirstRun"));
    }
    true
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
    let dark = resolved == "dark";
    view::call(move |v| v.theme(dark, &pref));
}

pub fn handle_set_theme(theme: &str) {
    if !matches!(theme, "dark" | "light" | "system") {
        return;
    }
    config::set_config_str("theme", theme);
    push_theme();
}

pub fn handle_set_language(code: &str) {
    let pref = code.trim().to_owned();
    config::set_config_str("language", &pref);
    log!("Language: pref=\"{pref}\"");
    lang::init_language();
    let language = lang::current_language();
    view::call(move |v| v.language_changed(&language, &pref));
}

// ---------------------------------------------------------------------------
// Entrada direta da interface (sem JSON). Os submódulos são privados do
// bridge; a interface chama por aqui.
// ---------------------------------------------------------------------------

pub use devices::handle_toggle_source;
pub use recording::{handle_record_pause, handle_record_stop, last_encoder_caps};

/// O botão de gravar: na contagem do atraso da área, ele CANCELA (mesma
/// regra do `record_start` do dispatch).
pub fn handle_record_start_button() {
    if !countdown::cancel() {
        recording::handle_record_start();
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

//! Ciclo de gravação do Bridge (porte da parte de gravação do `OBSBridge.pas`):
//! `HandleRecordStart`/`HandleRecordStop`, stop assíncrono pelo sinal "stop"
//! (pegadinha #41), warmup do libobs (~1,5 s após abrir), tique de 1 s com o
//! tamanho já gravado, nome do arquivo por modelo e as caps de encoder.
//!
//! Estado de gravação mora numa `thread_local`: o Bridge inteiro vive na main
//! thread (libobs e UI só nela, pegadinha #3).

use std::cell::RefCell;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, Local};
use serde_json::json;

use crate::engine::{self, CaptureInputs, RecordRequest, WindowCapture};
use crate::types::{EncoderCaps, GpuVendor, RecordingMeta};
use crate::{config, encoder, lang, log, player, ui, win_audio_meter, win_preview, win_webcam};

use super::{devices, library, post_error, post_json};

// ---- timers (ids únicos: dois SetTimer com o mesmo id não coexistem) ----------
pub const TIMER_RECORDING_TICK: usize = 7001;
pub const TIMER_OBS_WARMUP: usize = 7006;
pub const TIMER_STOP_TIMEOUT: usize = 7010;
const RECORDING_TICK_MS: u32 = 1000;
/// Tempo para a UI renderizar antes do init do libobs bloquear a main.
const OBS_WARMUP_DELAY_MS: u32 = 1500;
/// Se o sinal "stop" nunca chegar (muxer travado, crash interno), força a
/// finalização depois deste prazo para não deixar a gravação "presa".
const STOP_TIMEOUT_MS: u32 = 10_000;

/// Hotkeys globais (`HK_*` do Delphi).
pub const HK_RECORD_TOGGLE: i32 = 100;
pub const HK_RECORD_TOGGLE_ALT: i32 = 101;

/// Modelo padrão do nome do arquivo.
const DEFAULT_FILENAME_PATTERN: &str = "{AAAA}-{MM}-{DD} {HH}-{NN}-{SS}";

#[derive(Default)]
struct RecState {
    recording_active: bool,
    /// `RecordingStartTickMs`: quando a gravação (ou o que o buffer já tinha) começou.
    recording_start: Option<Instant>,
    last_recording_path: String,
    last_recording_duration: i32,
    /// Esta gravação nasceu do watcher de mic? (`RecordingStartedByMicWatch`)
    started_by_mic_watch: bool,
    /// Gravação de UMA janela (pegadinha #66); vazio = telas.
    window: Option<WindowCapture>,
    window_was_visible_before_record: bool,
    encoder_caps_pushed: bool,
    last_encoder_caps: Option<EncoderCaps>,
    /// A UI foi avisada de "finalizando" e ainda espera o fim.
    finalizing: bool,
}

thread_local! {
    static STATE: RefCell<RecState> = RefCell::new(RecState::default());
}

fn with_state<R>(f: impl FnOnce(&mut RecState) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

pub fn recording_active() -> bool {
    with_state(|s| s.recording_active)
}

/// Caminho da gravação em curso (ou da última): a listagem o esconde enquanto o
/// muxer ainda escreve.
pub fn last_recording_path() -> String {
    with_state(|s| s.last_recording_path.clone())
}

pub fn last_encoder_caps() -> Option<EncoderCaps> {
    with_state(|s| s.last_encoder_caps)
}

/// Marca (ou desmarca) que a próxima gravação vem do watcher de mic.
pub fn set_started_by_mic_watch(v: bool) {
    with_state(|s| s.started_by_mic_watch = v);
}

/// Janela escolhida no seletor "Gravar uma janela"; `None` volta às telas.
pub fn set_recording_window(w: Option<WindowCapture>) {
    with_state(|s| s.window = w);
}

/// `RecordDir`: pasta de gravação (fallback `%USERPROFILE%\Videos`).
pub fn record_dir() -> PathBuf {
    let dir = config::get_config_str("recordDir", "");
    if !dir.trim().is_empty() {
        return PathBuf::from(dir);
    }
    let profile = std::env::var("USERPROFILE").unwrap_or_default();
    Path::new(&profile).join("Videos")
}

// ---------------------------------------------------------------------------
// Formatação
// ---------------------------------------------------------------------------

/// Tamanho da gravação EM CURSO, atualizado a cada segundo. Diferente do
/// `FormatBytesShort` dos cards: aqui o número precisa SE MEXER, então a
/// resolução é de ~1 KB e vai afrouxando conforme cresce
/// (`812 KB | 45,123 MB | 456,78 MB | 1,234 GB | 12,34 GB`).
pub fn format_bytes_live(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;
    // O Delphi formata com o separador do locale; a UI exibe como vier.
    let comma = |s: String| crate::win::localize_decimal(&s);
    if bytes >= GB {
        let v = bytes as f64 / GB as f64;
        comma(if v < 10.0 { format!("{v:.3} GB") } else { format!("{v:.2} GB") })
    } else if bytes >= MB {
        let v = bytes as f64 / MB as f64;
        comma(if v < 100.0 { format!("{v:.3} MB") } else { format!("{v:.2} MB") })
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Tamanho de um arquivo que OUTRO processo está escrevendo (o MKV em curso).
/// Abre com compartilhamento total e lê o tamanho pelo HANDLE, que é o valor
/// corrente; o fallback (`metadata` do caminho) lê a entrada de diretório, que
/// o NTFS atualiza com atraso — rede de segurança, nunca caminho principal.
pub fn get_growing_file_size(path: &str) -> i64 {
    if path.is_empty() {
        return 0;
    }
    // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    if let Ok(f) = std::fs::OpenOptions::new().read(true).share_mode(7).open(path) {
        if let Ok(m) = f.metadata() {
            return m.len() as i64;
        }
    }
    std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0)
}

/// Aplica o modelo de nome a um instante, devolvendo o nome-base (sem pasta
/// nem extensão). Códigos (case-insensitive, entre chaves): `{AAAA}{MM}{DD}
/// {HH}{NN}{SS}{ZZZ}`, `{CODEC}`, `{FPS}`, `{QUALIDADE}`. Chaves sobrando são
/// removidas e caracteres inválidos de nome de arquivo viram `_`. O espelho da
/// prévia é o `Settings._buildFilenameFromPattern` (settings.js).
pub fn apply_filename_pattern(
    pattern: &str,
    when: DateTime<Local>,
    codec: &str,
    fps: i32,
    quality: i32,
) -> String {
    let mut s = pattern.to_owned();
    let subs: [(&str, String); 10] = [
        ("{CODEC}", codec.to_owned()),
        ("{FPS}", fps.to_string()),
        ("{QUALIDADE}", quality.to_string()),
        ("{AAAA}", when.format("%Y").to_string()),
        ("{MM}", when.format("%m").to_string()),
        ("{DD}", when.format("%d").to_string()),
        ("{HH}", when.format("%H").to_string()),
        ("{NN}", when.format("%M").to_string()),
        ("{SS}", when.format("%S").to_string()),
        ("{ZZZ}", when.format("%3f").to_string()),
    ];
    for (code, value) in subs {
        s = replace_ci(&s, code, &value);
    }
    s = s.replace(['{', '}'], "");
    for ch in ['\\', '/', ':', '*', '?', '"', '<', '>', '|'] {
        s = s.replace(ch, "_");
    }
    let s = s.trim().to_owned();
    if s.is_empty() {
        format!("NoOBS_{}", when.format("%Y-%m-%d_%H-%M-%S"))
    } else {
        s
    }
}

/// `StringReplace` com `rfIgnoreCase` (os códigos são ASCII).
fn replace_ci(haystack: &str, needle: &str, with: &str) -> String {
    let (hl, nl) = (haystack.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(haystack.len());
    let mut i = 0;
    while let Some(pos) = hl[i..].find(&nl) {
        out.push_str(&haystack[i..i + pos]);
        out.push_str(with);
        i += pos + needle.len();
    }
    out.push_str(&haystack[i..]);
    out
}

/// Caminho completo do arquivo de saída: pasta + modelo + `.mkv`, com sufixo
/// ` (N)` se já existir (essencial para modelos só-texto, que colidiriam a cada
/// gravação). `encoder_id`: o encoder que GRAVOU (buffer, que já tem o dele);
/// vazio = o que a gravação vai escolher (o arquivo nasce antes do encoder).
pub fn build_recording_path(when: DateTime<Local>, encoder_id: &str) -> PathBuf {
    let dir = record_dir();
    let enc_id = if encoder_id.is_empty() { encoder::predict_video_encoder_id() } else { encoder_id.to_owned() };
    let (codec, _hw) = encoder::describe_encoder_id(&enc_id);
    let base = apply_filename_pattern(
        &config::get_config_str("filenamePattern", DEFAULT_FILENAME_PATTERN),
        when,
        codec,
        config::get_config_int("recordingFps", 30),
        encoder::recording_quality_level(),
    );
    let mut cand = dir.join(format!("{base}.mkv"));
    let mut n = 2;
    while cand.exists() {
        cand = dir.join(format!("{base} ({n}).mkv"));
        n += 1;
    }
    cand
}

// ---------------------------------------------------------------------------
// Pushes para a UI
// ---------------------------------------------------------------------------

/// Avisa que a gravação anterior ainda está sendo finalizada pelo libobs; a UI
/// desabilita o botão de gravar enquanto isso (em canvas 2x4K passa de 5 s).
pub fn push_finalizing(active: bool) {
    with_state(|s| s.finalizing = active);
    post_json(json!({ "type": "recording_finalizing", "active": active }));
}

/// Overlay de loading na sidebar durante um refresh/start.
pub fn push_refresh_busy(busy: bool, what: &str) {
    post_json(json!({ "type": "refresh_busy", "busy": busy, "what": what }));
}

pub fn push_recording_state() {
    let (active, elapsed, window, path) = with_state(|s| {
        let elapsed = match (s.recording_active, s.recording_start) {
            (true, Some(t)) => t.elapsed().as_secs() as i64,
            _ => 0,
        };
        let window = s.window.as_ref().filter(|_| s.recording_active).map(|w| w.title.clone());
        (s.recording_active, elapsed, window, s.last_recording_path.clone())
    });
    let mut obj = json!({ "type": "recording_state", "active": active, "elapsed": elapsed });
    // Gravando uma janela: a UI diz qual no lugar de "Gravando".
    if let Some(title) = window {
        obj["window"] = json!(title);
    }
    // Tamanho já gravado, junto do tique de 1 s. Zero enquanto o muxer não
    // escreveu nada — a UI simplesmente não mostra nada nesse caso.
    if active && !path.is_empty() {
        let bytes = get_growing_file_size(&path);
        if bytes > 0 {
            obj["sizeBytes"] = json!(bytes);
            obj["sizeText"] = json!(format_bytes_live(bytes));
        }
    }
    post_json(obj);
}

// ---------------------------------------------------------------------------
// Encoder caps
// ---------------------------------------------------------------------------

/// Detecta os encoders do libobs e manda à UI (codecs suportados + logo do
/// vendor da GPU). A lista da EXPORTAÇÃO é outra (libavcodec, não libobs —
/// pegadinha #51) e entra com o `FFmpegExport`.
fn push_encoder_caps() {
    let caps = encoder::detect_encoder_caps();
    let (vendor, logo) = match caps.vendor {
        GpuVendor::Nvidia => ("nvidia", "nvidia.png"),
        GpuVendor::Amd => ("amd", "amd.png"),
        GpuVendor::Intel => ("intel", "intel.png"),
        GpuVendor::Unknown => ("", ""),
    };
    // Guarda para a exportação, que roda muito depois do warmup e não pode
    // depender de o libobs estar respondendo naquele instante.
    with_state(|s| s.last_encoder_caps = Some(caps));
    // Encoders do LIBAVCODEC (não do libobs — pegadinha #51), já filtrados por existência.
    let export_encoders: Vec<serde_json::Value> = crate::ffmpeg_export::list_export_encoders(&caps)
        .into_iter()
        .map(|e| json!({ "id": e.id, "name": e.libav_name, "hardware": e.hardware }))
        .collect();
    post_json(json!({
        "type": "encoder_caps",
        "av1Hw": caps.av1_hw,
        "hevcHw": caps.hevc_hw,
        "h264Hw": caps.h264_hw,
        "h264Sw": caps.h264_sw,
        "av1Sw": caps.av1_sw,
        "vendor": vendor,
        "vendorLogo": logo,
        "exportEncoders": export_encoders,
    }));
    log!(
        "Encoder caps: av1-hw={} hevc-hw={} h264-hw={} h264-sw={} av1-sw={} vendor={vendor}",
        caps.av1_hw,
        caps.hevc_hw,
        caps.h264_hw,
        caps.h264_sw,
        caps.av1_sw
    );
}

/// Empurra as caps UMA vez por sessão, de qualquer caminho que suba o libobs
/// (o warmup NÃO é mais o único — pegadinha #62).
pub fn push_encoder_caps_once() {
    if with_state(|s| s.encoder_caps_pushed) {
        return;
    }
    if !engine::with_engine(|e| e.is_initialized()).unwrap_or(false) {
        return;
    }
    push_encoder_caps();
    with_state(|s| s.encoder_caps_pushed = true);
}

// ---------------------------------------------------------------------------
// Entradas de captura
// ---------------------------------------------------------------------------

/// Coleta o que a máquina oferece: monitores (Win32), webcams (DirectShow) e os
/// endpoints padrão do Windows (WASAPI, do cache já populado pelo worker).
fn gather_capture_inputs() -> CaptureInputs {
    let devs = win_audio_meter::enumerate_audio_devices();
    let default_of = |kind| devs.iter().find(|d| d.kind == kind && d.is_default).map(|d| d.device_id.clone()).unwrap_or_default();
    CaptureInputs {
        monitors: win_preview::monitors_from_win_preview(),
        webcams: win_webcam::enumerate_webcams(),
        default_mic_id: default_of(win_audio_meter::DeviceKind::Input),
        default_spk_id: default_of(win_audio_meter::DeviceKind::Output),
    }
}

pub fn with_engine_result<R>(f: impl FnOnce(&mut engine::Engine) -> Result<R, String>) -> Result<R, String> {
    engine::with_engine(f).unwrap_or_else(|| Err("engine indisponível".into()))
}

// ---------------------------------------------------------------------------
// Start / Stop
// ---------------------------------------------------------------------------

pub fn handle_record_start() {
    if recording_active() {
        return;
    }
    // Gravação em curso = não hibernar.
    ui::kill_timer(super::TIMER_HIBERNATE_IDLE);

    // Finalização anterior em curso: RECUSA o start. (O ForceCompleteStop síncrono
    // travava a MAIN por segundos; a UI desabilita o botão e este guard cobre
    // o atalho global e a auto-gravação.) O TIMER_STOP_TIMEOUT garante que isso
    // nunca fica preso para sempre.
    if let Some((true, elapsed)) = engine::with_engine(|e| (e.is_stopping(), e.stopping_elapsed_ms())) {
        if elapsed < STOP_TIMEOUT_MS as u64 {
            log!("HandleRecordStart: ignorado — finalizando ha {elapsed}ms.");
            return;
        }
        // Passou do prazo: algo travou. Recusar deixaria o app SEM GRAVAR até
        // reiniciar — conclui e segue.
        log!("HandleRecordStart: finalizacao presa ha {elapsed}ms (limite {STOP_TIMEOUT_MS}ms) — forcando conclusao pra nao travar a gravacao.");
        ui::kill_timer(TIMER_STOP_TIMEOUT);
        engine::with_engine(|e| e.force_complete_stop());
    }

    // TODO(buffer em memória, #62): continuar o buffer ligado (`replayIntoRecording`)
    // ou pará-lo antes de gravar.

    let t0 = Instant::now();
    log!("HandleRecordStart: inicio.");
    push_refresh_busy(true, "starting");
    match start_recording_inner() {
        Ok(()) => log!("HandleRecordStart: total {}ms.", t0.elapsed().as_millis()),
        Err(e) => {
            with_state(|s| {
                s.recording_active = false;
                s.window = None;
            });
            super::transcribe::sync_pause(false);
            post_error(lang::t_with("error.recordStartFailed", &[("error", &e)]));
            push_recording_state();
            log!("HandleRecordStart: FALHOU apos {}ms: {e}", t0.elapsed().as_millis());
            // TODO(buffer em memória, #62): a gravação não subiu — o buffer que
            // ela derrubou volta.
        }
    }
    push_refresh_busy(false, "starting");
}

fn start_recording_inner() -> Result<(), String> {
    engine::ensure_created();
    let t_step = Instant::now();
    let cold = !engine::with_engine(|e| e.is_initialized()).unwrap_or(false);
    with_engine_result(|e| e.ensure_initialized())?;
    log!("HandleRecordStart: EnsureInitialized em {}ms.", t_step.elapsed().as_millis());
    // NÃO adianta esperar aqui: a RTWQ é montada pelo win-wasapi na criação da
    // fonte, não no obs_startup (medido). Só registra para correlacionar no log.
    if cold {
        log!("HandleRecordStart: cold start (libobs subiu agora).");
    }

    // Zera antes de montar: o handler de log marca se a captura falhar.
    engine::reset_audio_capture_fault();

    let output_path = build_recording_path(Local::now(), "");
    let (auto_profile, window) = with_state(|s| (s.started_by_mic_watch, s.window.clone()));
    let req = RecordRequest {
        output_path: output_path.to_string_lossy().into_owned(),
        auto_profile,
        window,
    };

    let t_step = Instant::now();
    let inputs = gather_capture_inputs();
    with_engine_result(|e| e.build_and_start_recording(&req, &inputs))?;
    // Captura de áudio falhou em agendar? SÓ log, sem avisar o usuário: o erro
    // do win-wasapi é emitido POR FONTE e endpoints virtuais (Steam Streaming,
    // Saída Digital) falham sozinhos sem afetar os reais — alarme falso ensina
    // o usuário a ignorar o real.
    if engine::had_audio_capture_fault() {
        log!("HandleRecordStart: win-wasapi reportou falha de agendamento (RTWQ). Pode ser so endpoint virtual — nao necessariamente gravacao muda.");
    }
    log!("HandleRecordStart: BuildAndStartRecording em {}ms.", t_step.elapsed().as_millis());

    with_state(|s| {
        s.recording_active = true;
        s.last_recording_path = req.output_path.clone();
        s.last_recording_duration = 0;
        s.recording_start = Some(Instant::now());
    });
    // Transcrever durante a gravação disputaria a máquina com a captura: segura as
    // automáticas até ela terminar.
    super::transcribe::sync_pause(true);
    // TODO(buffer, #62): StartPerfMonitor.
    // TODO(OBSTray/OBSUI): bolinha vermelha na bandeja e na taskbar.
    // Snapshot dos monitores no início: os slots de prévia ficam fixos mesmo se
    // o usuário desplugar um monitor gravando. Sessão nova = fontes novas: sem
    // zerar o mudo aplicado, o 1º tique acharia que nada mudou.
    devices::set_recording_for_thumbs(true);
    devices::clear_mic_mute_applied();
    ui::set_timer(TIMER_RECORDING_TICK, RECORDING_TICK_MS);
    push_recording_state();
    // TODO(OBSTray): minimizar ao gravar (`minimizeOnRecord`), notificação da
    // bandeja e indicador de tela (`recIndicator`, WinRecIndicator).
    Ok(())
}

pub fn handle_record_stop() {
    if !recording_active() {
        log!("HandleRecordStop: chamado mas RecordingActive=False — no-op.");
        return;
    }
    log!("HandleRecordStop: inicio.");
    // Qualquer parada (manual, hotkey ou auto) encerra o vínculo com o monitor
    // de mic e com a janela: a próxima gravação volta a ser das telas.
    let elapsed = with_state(|s| {
        s.started_by_mic_watch = false;
        s.window = None;
        s.recording_start.map(|t| t.elapsed().as_secs() as i32).unwrap_or(0)
    });

    // Som de parada AGORA: o flush do MKV pode levar centenas de ms e o usuário
    // não quer ouvir o "ding" só depois (parece travado).
    if config::get_config_bool("playSoundOnRecord", true) {
        post_json(json!({ "type": "recording_stopping" }));
    }

    ui::kill_timer(TIMER_RECORDING_TICK);
    // TODO(WinRecIndicator): esconder o indicador de tela.

    // O caminho já é conhecido (geramos no start). O arquivo só estará COMPLETO
    // quando o sinal "stop" do output disparar — por isso o card e a meta saem
    // no `on_engine_recording_stopped`, não aqui.
    let output_path = engine::with_engine(|e| e.output_path().to_owned()).unwrap_or_default();

    with_state(|s| {
        s.recording_active = false;
        s.last_recording_path = output_path;
        s.last_recording_duration = elapsed;
    });
    // O buffer em memória (#62) entra aqui com `ReplayActive` quando for portado.
    super::transcribe::sync_pause(false);
    // TODO(OBSTray/OBSUI): tirar a bolinha vermelha dos ícones.
    push_recording_state();

    // Stop ASSÍNCRONO (igual ao OBS): `obs_output_stop` e retorna. Quando o
    // output terminar de verdade o sinal "stop" dispara →
    // `on_engine_recording_stopped`. Timeout de segurança se o sinal nunca vier.
    if engine::with_engine(|e| e.is_recording()).unwrap_or(false) {
        engine::with_engine(|e| e.request_stop());
        ui::set_timer(TIMER_STOP_TIMEOUT, STOP_TIMEOUT_MS);
    }

    // TODO(OBSTray): notificação de fim, restaurar a janela escondida pelo
    // auto-minimizar ou armar o idle de hibernação (OnWindowHiddenForHibernate).

    // O que mudou de dispositivo durante a gravação é aplicado agora.
    devices::set_recording_for_thumbs(false);
    devices::apply_pending_refreshes();

    // A partir daqui o libobs ainda está fechando o arquivo. Trava o botão até
    // o sinal "stop" chegar (on_engine_recording_stopped) ou o timeout.
    push_finalizing(true);
    log!("HandleRecordStop: fim (retornando ao message loop).");
}

/// `Engine.OnStopped`: main thread, quando o output emitiu "stop" — a gravação
/// terminou de verdade, com o arquivo íntegro. É AQUI (não no stop) que se
/// salva a meta e se adiciona o card (mesma lógica do RecordingStop do OBS).
pub fn on_engine_recording_stopped(output_path: &str) {
    ui::kill_timer(TIMER_STOP_TIMEOUT);
    log!("OnEngineRecordingStopped: path=\"{output_path}\"");
    // Arquivo íntegro: libera o botão de gravar.
    push_finalizing(false);
    if output_path.is_empty() {
        // TODO(buffer, #62): religar o buffer se ReplayWanted.
        return;
    }

    // Layout (canvas + monitores/webcams) + duração + como foi gravado, para o
    // <hash>.json. O codec vem do encoder que o engine REALMENTE criou (o
    // fallback pode ter trocado o pedido); fps e qualidade vêm do config, que
    // não muda durante a gravação.
    let meta = engine::with_engine(|e| {
        let (codec, hw) = encoder::describe_encoder_id(e.video_encoder_id());
        RecordingMeta {
            duration_sec: with_state(|s| s.last_recording_duration),
            layout: e.current_layout().clone(),
            codec: codec.to_owned(),
            codec_hw: hw,
            fps: config::get_config_int("recordingFps", 30),
            quality_level: encoder::recording_quality_level(),
        }
    });
    // Persiste ANTES do card: o scan de miniatura (worker) já acha o layout pronto.
    if let Some(m) = &meta {
        player::save_recording_meta(output_path, m);
    }
    crate::file_meta::mark_changed(output_path);
    library::push_recording_added(output_path, with_state(|s| s.last_recording_duration));
    // TRANSCRIÇÃO AUTOMÁTICA. Enfileira já com o arquivo íntegro (o sinal "stop" garante o
    // trailer escrito).
    super::transcribe::enqueue_after_capture(output_path);
    // TODO(buffer, #62): religar o buffer por ÚLTIMO (refaz o CurrentLayout).
}

// ---------------------------------------------------------------------------
// Hotkey global
// ---------------------------------------------------------------------------

/// Registra o atalho global de gravar. Default `Pause/Break`. Falha de registro
/// não é fatal (outro app pode ter o mesmo atalho — pegadinha #33).
/// TODO(OBSHotkey): `ParseHotkey` completo; por ora só o atalho padrão.
pub fn apply_hotkey_from_config() {
    const VK_PAUSE: u32 = 0x13;
    const VK_CANCEL: u32 = 0x03;
    ui::unregister_global_hotkey(HK_RECORD_TOGGLE);
    ui::unregister_global_hotkey(HK_RECORD_TOGGLE_ALT);
    let spec = config::get_config_str("hotkey", "Pause/Break");
    if spec.trim().is_empty() {
        log!("Hotkey: desativada (config vazia).");
        return;
    }
    if !spec.trim().eq_ignore_ascii_case("pause/break") {
        log!("Hotkey: spec \"{spec}\" ainda sem parser em Rust (OBSHotkey) — ignorado.");
        return;
    }
    if ui::register_global_hotkey(HK_RECORD_TOGGLE, 0, VK_PAUSE) {
        log!("Hotkey: registrado \"{spec}\".");
    } else {
        log!("Hotkey: RegisterHotKey falhou pra \"{spec}\" (outro app pode estar usando).");
    }
}

pub fn on_hotkey(id: i32) {
    log!("Hotkey: id={id} disparado.");
    if matches!(id, HK_RECORD_TOGGLE | HK_RECORD_TOGGLE_ALT) {
        if recording_active() {
            handle_record_stop();
        } else {
            handle_record_start();
        }
    }
}

// ---------------------------------------------------------------------------
// Warmup e timers
// ---------------------------------------------------------------------------

/// Agenda o warmup do libobs: com delay para a UI renderizar primeiro; a 1ª
/// gravação fica instantânea (sem ~300 ms de obs.dll + plugins + D3D11).
pub fn schedule_warmup() {
    ui::set_timer(TIMER_OBS_WARMUP, OBS_WARMUP_DELAY_MS);
}

fn on_warmup() {
    // One-shot — desliga antes de chamar (o init bloqueia a main por ~300 ms).
    ui::kill_timer(TIMER_OBS_WARMUP);
    if !engine::exists() && !recording_active() {
        engine::ensure_created();
        match with_engine_result(|e| e.ensure_initialized()) {
            Ok(()) => log!("libobs: warmup pronto — proxima gravacao sera instantanea."),
            Err(e) => {
                log!("libobs: warmup falhou (gravacao vai inicializar sob demanda): {e}");
                engine::destroy();
            }
        }
    }
    // FORA do if acima de propósito: o engine pode ter sido criado ANTES deste
    // timer (watcher de apps ligando o buffer) e as caps têm que sair igual.
    push_encoder_caps_once();
    // TODO(OBSUI /start-record): disparar a gravação pedida pela hibernação.
    // TODO(buffer, #62): `replayAutoStart` / `/start-replay` → ReplayWanted.
}

/// Timers de gravação; `false` = id que não é meu.
pub fn on_timer(id: usize) -> bool {
    match id {
        TIMER_RECORDING_TICK => {
            if recording_active() {
                push_recording_state();
            }
        }
        TIMER_OBS_WARMUP => on_warmup(),
        TIMER_STOP_TIMEOUT => {
            // O sinal "stop" nunca chegou no prazo. One-shot: desarma e força.
            ui::kill_timer(TIMER_STOP_TIMEOUT);
            log!("TIMER_STOP_TIMEOUT: sinal \"stop\" nao chegou — forcando finalizacao.");
            engine::with_engine(|e| e.force_complete_stop());
        }
        _ => return false,
    }
    true
}

/// Liga os callbacks do engine ao Bridge (`Engine.OnStopped := ...`).
pub fn install() {
    engine::set_on_stopped(on_engine_recording_stopped);
}

/// Shutdown do Bridge (idempotente): para o que estiver vivo e encerra o libobs
/// NA MAIN THREAD, com o STA ainda vivo (pegadinha #31).
pub fn shutdown() {
    ui::unregister_global_hotkey(HK_RECORD_TOGGLE);
    ui::unregister_global_hotkey(HK_RECORD_TOGGLE_ALT);
    if engine::exists() {
        // Sync: no shutdown não há message loop para drenar o sinal.
        engine::with_engine(|e| {
            if e.is_recording() || e.is_stopping() {
                e.stop_recording();
            }
        });
        engine::destroy(); // Drop → teardown (obs_shutdown + watchdog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn when() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 3, 7, 9, 5, 2).unwrap()
    }

    #[test]
    fn modelo_padrao() {
        let n = apply_filename_pattern(DEFAULT_FILENAME_PATTERN, when(), "AV1", 30, 5);
        assert_eq!(n, "2026-03-07 09-05-02");
    }

    #[test]
    fn codigos_de_codec_fps_qualidade_sem_caixa() {
        let n = apply_filename_pattern("{codec}_{Fps}_q{QUALIDADE}", when(), "H.264", 60, 7);
        assert_eq!(n, "H.264_60_q7");
    }

    #[test]
    fn invalidos_viram_underscore_e_chave_solta_some() {
        let n = apply_filename_pattern("a:b/{XYZ}", when(), "", 30, 5);
        assert_eq!(n, "a_b_XYZ");
    }

    #[test]
    fn vazio_cai_no_fallback_datado() {
        let n = apply_filename_pattern("  ", when(), "", 30, 5);
        assert_eq!(n, "NoOBS_2026-03-07_09-05-02");
    }

    #[test]
    fn bytes_ao_vivo() {
        assert_eq!(format_bytes_live(512), "512 B");
        assert_eq!(format_bytes_live(812 * 1024), "812 KB");
        // O separador depende do locale do usuário: normaliza antes de comparar.
        let dot = |s: String| s.replace(crate::win::decimal_separator(), ".");
        assert_eq!(dot(format_bytes_live(1024 * 1024 * 45 + 123 * 1024)), "45.120 MB");
        assert_eq!(dot(format_bytes_live(1024 * 1024 * 1024 * 3)), "3.000 GB");
    }
}

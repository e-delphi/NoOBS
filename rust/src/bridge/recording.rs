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

use crate::engine::{self, AreaRect, CaptureInputs, RecordRequest, WindowCapture};
use crate::types::{EncoderCaps, GpuVendor, RecordingMeta};
use crate::{config, encoder, lang, lock_detector, log, player, rec_indicator, tray, ui, win_audio_meter, win_preview, win_webcam};

use super::{devices, library, post_error, post_json, replay, shell};

// ---- timers (ids únicos: dois SetTimer com o mesmo id não coexistem) ----------
pub const TIMER_RECORDING_TICK: usize = 7001;
pub const TIMER_OBS_WARMUP: usize = 7006;
pub const TIMER_STOP_TIMEOUT: usize = 7010;
const RECORDING_TICK_MS: u32 = 1000;
/// Tempo para a UI renderizar antes do init do libobs bloquear a main.
const OBS_WARMUP_DELAY_MS: u32 = 1500;
/// Se o sinal "stop" nunca chegar (muxer travado, crash interno), força a
/// finalização depois deste prazo para não deixar a gravação "presa".
pub(super) const STOP_TIMEOUT_MS: u32 = 10_000;

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
    /// Gravação de uma ÁREA da tela (Capturar... → Área); vence a janela.
    area: Option<AreaRect>,
    window_was_visible_before_record: bool,
    encoder_caps_pushed: bool,
    last_encoder_caps: Option<EncoderCaps>,
    /// A UI foi avisada de "finalizando" e ainda espera o fim.
    finalizing: bool,
    /// Gravação pausada (`obs_output_pause`). O relógio da gravação para em
    /// `pause_at` e, ao retomar, o `recording_start` anda pra frente o tempo
    /// pausado — assim todo lugar que faz "agora - início" (tela, indicador,
    /// duração do card) continua certo sem saber da pausa.
    recording_paused: bool,
    pause_at: Option<Instant>,
}

impl RecState {
    /// "Agora" no relógio da gravação: parado no instante da pausa enquanto ela
    /// durar.
    fn now_tick(&self) -> Instant {
        match (self.recording_paused, self.pause_at) {
            (true, Some(t)) => t,
            _ => Instant::now(),
        }
    }

    /// Tempo gravado, já sem as pausas. Zero fora da gravação.
    fn elapsed(&self) -> std::time::Duration {
        match (self.recording_active, self.recording_start) {
            (true, Some(t)) => self.now_tick().saturating_duration_since(t),
            _ => std::time::Duration::ZERO,
        }
    }
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

/// A gravação em curso foi iniciada pelo watcher de mic (só ela é auto-parada).
pub fn started_by_mic_watch() -> bool {
    with_state(|s| s.started_by_mic_watch)
}

/// Área escolhida no "Capturar... → Área"; `None` volta às telas.
pub fn set_recording_area(a: Option<AreaRect>) {
    with_state(|s| s.area = a);
}

/// A área mudou no meio da gravação (moldura movida/redimensionada).
pub fn update_recording_area(a: AreaRect) {
    with_state(|s| {
        if s.recording_active && s.area.is_some() {
            s.area = Some(a);
        }
    });
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
    let enc_id = if !encoder_id.is_empty() {
        encoder_id.to_owned()
    } else {
        engine::with_engine(|e| e.predict_encoder_id()).unwrap_or_else(|| if engine::native_selected() { String::new() } else { encoder::predict_video_encoder_id() })
    };
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
    let (active, elapsed, paused, window, path) = with_state(|s| {
        let window = if !s.recording_active {
            None
        } else if let Some(a) = s.area {
            Some(lang::t_with("windowPicker.areaRecording", &[("w", &a.w.to_string()), ("h", &a.h.to_string())]))
        } else {
            s.window.as_ref().map(|w| w.title.clone())
        };
        (s.recording_active, s.elapsed(), s.recording_active && s.recording_paused, window, s.last_recording_path.clone())
    });
    let mut obj = json!({ "type": "recording_state", "active": active, "elapsed": elapsed.as_secs() });
    // O mesmo tempo em ms: o cronômetro da tela mostra centésimos, e com o
    // segundo inteiro a pausa congelava sempre em ".00" (e cada tique de 1 s
    // puxava o relógio de volta pro segundo cheio).
    if active {
        obj["elapsedMs"] = json!(elapsed.as_millis() as u64);
    }
    if paused {
        obj["paused"] = json!(true);
    }
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
    // A detecção é do MOTOR: a da libobs enumera os IDs dela; a do motor
    // próprio abre e aquece os encoders do libavcodec.
    let caps = engine::with_engine(|e| e.encoder_caps()).unwrap_or_else(encoder::detect_encoder_caps);
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
pub(super) fn gather_capture_inputs() -> CaptureInputs {
    let devs = win_audio_meter::enumerate_audio_devices();
    let default_of = |kind| devs.iter().find(|d| d.kind == kind && d.is_default).map(|d| d.device_id.clone()).unwrap_or_default();
    CaptureInputs {
        monitors: win_preview::monitors_from_win_preview(),
        webcams: win_webcam::enumerate_webcams(),
        default_mic_id: default_of(win_audio_meter::DeviceKind::Input),
        default_spk_id: default_of(win_audio_meter::DeviceKind::Output),
    }
}

pub fn with_engine_result<R>(f: impl FnOnce(&mut dyn engine::CaptureEngine) -> Result<R, String>) -> Result<R, String> {
    engine::with_engine(f).unwrap_or_else(|| Err("engine indisponível".into()))
}

// ---------------------------------------------------------------------------
// Start / Stop
// ---------------------------------------------------------------------------

pub fn handle_record_start() {
    if recording_active() {
        return;
    }
    // A amostra do teste de qualidade está ocupando a captura (5 s).
    if super::video_test::capturing() {
        log!("HandleRecordStart: ignorado — teste de qualidade capturando.");
        post_error(lang::t("settings.videoTest.busyRecord"));
        with_state(|s| {
            s.window = None;
            s.area = None;
        });
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

    // Buffer ligado: por padrão o que ele guardou vira o COMEÇO desta gravação
    // (`replayIntoRecording`). Fora disso — opção desligada, auto-gravação por
    // mic (outro perfil = outra cena) ou gravação de janela (a cena do buffer
    // são as telas) — é um de cada vez: o buffer é descartado e volta sozinho
    // no fim se `ReplayWanted` seguir ligado.
    let (by_mic, has_window) = with_state(|s| (s.started_by_mic_watch, s.window.is_some() || s.area.is_some()));
    let continue_replay = replay::replay_active()
        && engine::exists()
        && config::get_config_bool("replayIntoRecording", true)
        && !by_mic
        && !has_window;
    if replay::replay_active() && !continue_replay {
        log!("HandleRecordStart: parando o buffer em memoria pra gravar.");
        replay::stop_replay_buffer(false);
    }

    let t0 = Instant::now();
    log!("HandleRecordStart: inicio.");
    push_refresh_busy(true, "starting");
    match start_recording_inner(continue_replay) {
        Ok(()) => log!("HandleRecordStart: total {}ms.", t0.elapsed().as_millis()),
        Err(e) => {
            with_state(|s| {
                s.recording_active = false;
                s.window = None;
                s.area = None;
            });
            replay::sync_transcribe_pause();
            // Reverte os ícones se já tinham virado "gravando" (no-op se não).
            tray::set_recording(false);
            ui::set_window_icon_recording(false);
            post_error(lang::t_with("error.recordStartFailed", &[("error", &e)]));
            push_recording_state();
            log!("HandleRecordStart: FALHOU apos {}ms: {e}", t0.elapsed().as_millis());
            // A gravação não subiu: o buffer que ela derrubou volta.
            if replay::replay_wanted() {
                replay::start_replay_buffer();
            }
        }
    }
    push_refresh_busy(false, "starting");
}

fn start_recording_inner(mut continue_replay: bool) -> Result<(), String> {
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

    // Continuando o buffer, o codec do nome é o dele (os encoders já rodam).
    let enc_id = if continue_replay { engine::with_engine(|e| e.video_encoder_id().to_owned()).unwrap_or_default() } else { String::new() };
    let output_path = build_recording_path(Local::now(), &enc_id);
    let (auto_profile, window, area) = with_state(|s| (s.started_by_mic_watch, s.window.clone(), s.area));
    let req = RecordRequest {
        output_path: output_path.to_string_lossy().into_owned(),
        auto_profile,
        window,
        area,
    };

    let t_step = Instant::now();
    replay::clear_prefix();
    if continue_replay && !replay::continue_into_recording(&req.output_path) {
        continue_replay = false;
        log!("HandleRecordStart: nao deu pra continuar o buffer — descartando-o.");
        replay::stop_replay_buffer(false);
    }
    if !continue_replay {
        let inputs = gather_capture_inputs();
        with_engine_result(|e| e.build_and_start_recording(&req, &inputs))?;
    }
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
        // Continuando o buffer, o relógio já começa no que ele guardou: é o
        // que o arquivo final vai ter (tela, indicador e duração).
        let prefix = std::time::Duration::from_secs(replay::prefix_sec().max(0) as u64);
        s.recording_start = Some(Instant::now().checked_sub(prefix).unwrap_or_else(Instant::now));
        s.recording_paused = false;
        s.pause_at = None;
    });
    replay::start_perf_monitor();
    // Transcrever durante a gravação disputaria a máquina com a captura: segura as
    // automáticas até ela terminar.
    replay::sync_transcribe_pause();
    // Snapshot dos monitores no início: os slots de prévia ficam fixos mesmo se
    // o usuário desplugar um monitor gravando. Sessão nova = fontes novas: sem
    // zerar o mudo aplicado, o 1º tique acharia que nada mudou.
    devices::set_recording_for_thumbs(true);
    devices::clear_mic_mute_applied();
    ui::set_timer(TIMER_RECORDING_TICK, RECORDING_TICK_MS);
    // Bolinha vermelha na bandeja e na taskbar: "gravando" mesmo escondida.
    tray::set_recording(true);
    ui::set_window_icon_recording(true);
    push_recording_state();
    // Minimizar ao gravar (padrão ligado): a bandeja com closeToTray, senão a
    // taskbar. Lembra se estava visível para restaurar no stop — quem já tinha
    // escondido à mão não quer a janela pulando de volta. Minimizada CONTA
    // como visível (IsWindowVisible, como no Delphi): ela vai pra bandeja e
    // volta no stop.
    let visible = ui::is_window_visible();
    with_state(|s| s.window_was_visible_before_record = false);
    if config::get_config_bool("minimizeOnRecord", true) && visible {
        with_state(|s| s.window_was_visible_before_record = true);
        if config::get_config_bool("closeToTray", true) {
            ui::hide_to_tray();
        } else {
            ui::minimize_to_taskbar();
        }
    }
    shell::maybe_notify_record("NoOBS", &lang::t("record.started"));
    // Indicador NA TELA (opcional, padrão desligado), excluído da gravação.
    if config::get_config_bool("recIndicator", false) {
        show_rec_indicator_from_config();
    }
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
        s.area = None;
        // Parando pausada, a pausa não conta (o arquivo também não a tem).
        let e = s.elapsed().as_secs() as i32;
        s.recording_paused = false;
        s.pause_at = None;
        e
    });

    // Som de parada AGORA: o flush do MKV pode levar centenas de ms e o usuário
    // não quer ouvir o "ding" só depois (parece travado).
    if config::get_config_bool("playSoundOnRecord", true) {
        post_json(json!({ "type": "recording_stopping" }));
    }

    ui::kill_timer(TIMER_RECORDING_TICK);
    // Idempotente: no-op se não estava visível.
    rec_indicator::hide();
    // Moldura da gravação de área (se houver).
    crate::area_frame::hide();

    // O caminho já é conhecido (geramos no start). O arquivo só estará COMPLETO
    // quando o sinal "stop" do output disparar — por isso o card e a meta saem
    // no `on_engine_recording_stopped`, não aqui.
    let output_path = engine::with_engine(|e| e.output_path().to_owned()).unwrap_or_default();

    with_state(|s| {
        s.recording_active = false;
        s.last_recording_path = output_path;
        s.last_recording_duration = elapsed;
    });
    replay::sync_transcribe_pause();
    tray::set_recording(false);
    ui::set_window_icon_recording(false);
    push_recording_state();

    // Stop ASSÍNCRONO (igual ao OBS): `obs_output_stop` e retorna. Quando o
    // output terminar de verdade o sinal "stop" dispara →
    // `on_engine_recording_stopped`. Timeout de segurança se o sinal nunca vier.
    if engine::with_engine(|e| e.is_recording()).unwrap_or(false) {
        engine::with_engine(|e| e.request_stop());
        ui::set_timer(TIMER_STOP_TIMEOUT, STOP_TIMEOUT_MS);
    }

    shell::maybe_notify_record(
        "NoOBS",
        &lang::t_with("record.finished", &[("min", &(elapsed / 60).to_string()), ("sec", &(elapsed % 60).to_string())]),
    );
    // O auto-minimizar escondeu a janela no start: ela volta. Senão continua
    // escondida, e o ócio de hibernação é armado.
    if with_state(|s| std::mem::take(&mut s.window_was_visible_before_record)) {
        ui::restore_from_tray();
    } else {
        shell::on_window_hidden();
    }

    // O que mudou de dispositivo durante a gravação é aplicado agora.
    devices::set_recording_for_thumbs(false);
    devices::apply_pending_refreshes();

    // A partir daqui o libobs ainda está fechando o arquivo. Trava o botão até
    // o sinal "stop" chegar (on_engine_recording_stopped) ou o timeout.
    push_finalizing(true);
    log!("HandleRecordStop: fim (retornando ao message loop).");
}

/// Pausa/retoma (botão ao lado do de gravar). O arquivo continua o MESMO: a
/// libobs desconta o intervalo dos timestamps, então o vídeo sai sem o trecho
/// pausado e sem buraco.
pub fn handle_record_pause(pause: bool) {
    if !recording_active() || !engine::exists() {
        return;
    }
    if pause == with_state(|s| s.recording_paused) {
        return;
    }
    if !engine::with_engine(|e| e.set_recording_paused(pause)).unwrap_or(false) {
        if pause {
            post_error(lang::t("record.pauseFailed"));
        }
        push_recording_state();
        return;
    }
    let now = Instant::now();
    with_state(|s| {
        if pause {
            s.pause_at = Some(now);
        } else if let (Some(start), Some(at)) = (s.recording_start, s.pause_at) {
            s.recording_start = Some(start + now.saturating_duration_since(at));
        }
        s.recording_paused = pause;
        if !pause {
            s.pause_at = None;
        }
    });
    if rec_indicator::is_showing() && rec_indicator::current_mode() == rec_indicator::Mode::Recording {
        if let Some(start) = with_state(|s| s.recording_start) {
            rec_indicator::set_paused(pause, start);
        }
    }
    shell::maybe_notify_record("NoOBS", &lang::t(if pause { "record.pausedNotify" } else { "record.resumedNotify" }));
    push_recording_state();
}

/// `Engine.OnStopped`: main thread, quando o output emitiu "stop" — a gravação
/// terminou de verdade, com o arquivo íntegro. É AQUI (não no stop) que se
/// salva a meta e se adiciona o card (mesma lógica do RecordingStop do OBS).
pub fn on_engine_recording_stopped(output_path: &str) {
    ui::kill_timer(TIMER_STOP_TIMEOUT);
    log!("OnEngineRecordingStopped: path=\"{output_path}\"");
    // A amostra do teste de qualidade usa o mesmo caminho de gravação, mas não
    // é gravação do usuário: nada de meta, card nem transcrição.
    if super::video_test::capturing() {
        super::video_test::on_recorded(output_path);
        return;
    }
    // Arquivo íntegro: libera o botão de gravar.
    push_finalizing(false);
    if output_path.is_empty() {
        if replay::replay_wanted() {
            replay::start_replay_buffer();
        }
        return;
    }

    // Gravação que continuou o buffer: o começo dela é outro arquivo. O engine
    // só chega aqui com o trecho resolvido (salvo ou perdido).
    let mut last_duration = with_state(|s| s.last_recording_duration);
    if replay::on_recording_stopped_with_prefix(output_path, &mut last_duration) {
        // O card sai no fim da emenda; o buffer religa já (a emenda só lê
        // arquivos, não depende do libobs).
        if replay::replay_wanted() {
            replay::start_replay_buffer();
        }
        return;
    }
    with_state(|s| s.last_recording_duration = last_duration);

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
    // Buffer ligado: volta a guardar. Por ÚLTIMO — montar o buffer refaz a cena
    // e sobrescreve o layout que a meta acima acabou de ler.
    if replay::replay_wanted() {
        replay::start_replay_buffer();
    }
}

// ---------------------------------------------------------------------------
// Hotkey global
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Indicador de tela (WinRecIndicator) e bloqueio de tela
// ---------------------------------------------------------------------------

/// Mostra o indicador lendo TODAS as preferências (canto, opacidade, clique).
/// Usado pelo start e pelos handlers das Configurações.
pub fn show_rec_indicator_from_config() {
    let (start, paused) = with_state(|s| (s.recording_start.unwrap_or_else(Instant::now), s.recording_paused));
    rec_indicator::show(
        rec_indicator::parse_corner(&config::get_config_str("recIndicatorCorner", "top-right")),
        start,
        config::get_config_int("recIndicatorOpacity", 90),
        rec_indicator::Mode::Recording,
        0,
        config::get_config_bool("recIndicatorClickable", true),
    );
    // Mostrado (de novo) no meio de uma pausa: volta pausado.
    if paused {
        rec_indicator::set_paused(true, start);
    }
}

/// Clique no overlay — chega DENTRO do `WndProc` dele, e o stop destrói essa
/// janela: por isso é diferido para a fila da main.
fn on_indicator_click() {
    crate::mainthread::queue(|| {
        if recording_active() {
            log!("RecIndicator: clique no overlay — parando a gravacao.");
            handle_record_stop();
        } else if replay::replay_active() {
            // No modo buffer o clique SALVA: parar seria destrutivo (o buffer
            // só existe na RAM) e sem desfazer.
            log!("RecIndicator: clique no overlay — salvando o trecho do buffer.");
            replay::handle_save_replay();
        }
    });
}

/// Callback do detector de bloqueio (thread dele). Só o LOCK age, e só com
/// `stopOnLock` (lido aqui, não na criação: liga/desliga sem reiniciar).
fn on_lock_changed(locked: bool) {
    log!("LockDetector: sessao {}.", if locked { "bloqueada" } else { "desbloqueada" });
    if !locked || !config::get_config_bool("stopOnLock", true) {
        return;
    }
    crate::mainthread::queue(|| {
        if super::is_shutting_down() || !recording_active() {
            return;
        }
        log!("LockDetector: parando gravacao por bloqueio de tela.");
        handle_record_stop();
    });
}

/// `DoInit`: indicador e detector de bloqueio (sempre ativo: custo desprezível).
pub fn init_indicator_and_lock() {
    rec_indicator::set_fullscreen_detect(config::get_config_bool("fullscreenDetect", true));
    rec_indicator::set_on_click(on_indicator_click);
    lock_detector::start(on_lock_changed);
}

pub fn handle_set_rec_indicator(enable: bool) {
    config::set_config_bool("recIndicator", enable);
    log!("RecIndicator: {enable}");
    // Gravando: aplica/remove na hora.
    if recording_active() {
        if enable {
            show_rec_indicator_from_config();
        } else {
            rec_indicator::hide();
        }
    }
}

pub fn handle_set_rec_indicator_corner(corner: &str) {
    config::set_config_str("recIndicatorCorner", corner);
    log!("RecIndicatorCorner: {corner}");
    if recording_active() && config::get_config_bool("recIndicator", false) && rec_indicator::is_showing() {
        show_rec_indicator_from_config();
    }
}

/// Slider ao vivo: aplica na janela existente sem recriar. Mesma faixa do
/// slider (e do indicador do buffer): abaixo de 20% o overlay some.
pub fn handle_set_rec_indicator_opacity(pct: i32) {
    let pct = pct.clamp(20, 100);
    config::set_config_int("recIndicatorOpacity", pct);
    log!("RecIndicatorOpacity: {pct}");
    if rec_indicator::is_showing() {
        rec_indicator::set_opacity(pct);
    }
}

/// Desligado, o clique atravessa o indicador (ele pode cobrir um botão).
pub fn handle_set_rec_indicator_clickable(enable: bool) {
    config::set_config_bool("recIndicatorClickable", enable);
    log!("RecIndicatorClickable: {enable}");
    if rec_indicator::is_showing() && rec_indicator::current_mode() == rec_indicator::Mode::Recording {
        rec_indicator::set_clickable(enable);
    }
}

/// Vale na hora: o indicador reavalia no tique de 500 ms; as prévias, no
/// próximo ciclo.
pub fn handle_set_fullscreen_detect(enable: bool) {
    config::set_config_bool("fullscreenDetect", enable);
    rec_indicator::set_fullscreen_detect(enable);
    log!("Tela cheia: detectar = {enable}.");
}

pub fn on_hotkey(id: i32) {
    log!("Hotkey: id={id} disparado.");
    if replay::on_hotkey(id) || super::area::on_hotkey(id) {
        return;
    }
    if matches!(id, HK_RECORD_TOGGLE | HK_RECORD_TOGGLE_ALT) {
        // Na contagem do atraso da área, o atalho de gravar cancela.
        if super::area::cancel_countdown() {
            return;
        }
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
    // /start-record: a hibernação lançou este processo porque o usuário
    // apertou o atalho (ou o mic entrou em uso). Com o libobs aquecido, grava
    // agora; se o warmup falhou, o start inicializa sob demanda.
    if ui::start_record_requested() && !recording_active() {
        log!("TIMER_OBS_WARMUP: /start-record — disparando gravacao.");
        // Veio do watcher de mic: gravação "automática", que para sozinha.
        if ui::start_record_requested_by_mic() {
            set_started_by_mic_watch(true);
        }
        handle_record_start();
    }
    // /start-area: o atalho da área salva foi apertado na hibernação.
    if ui::start_area_requested() && !recording_active() {
        log!("TIMER_OBS_WARMUP: /start-area — gravando a area salva.");
        super::area::record_saved_area();
    }
    // "Ligar o buffer ao abrir" e o /start-replay viram vontade da sessão.
    replay::on_warmup();
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
    replay::install();
}

/// Shutdown do Bridge (idempotente): para o que estiver vivo e encerra o libobs
/// NA MAIN THREAD, com o STA ainda vivo (pegadinha #31).
pub fn shutdown() {
    ui::unregister_global_hotkey(HK_RECORD_TOGGLE);
    ui::unregister_global_hotkey(HK_RECORD_TOGGLE_ALT);
    // O overlay some junto com o app.
    rec_indicator::hide();
    crate::area_frame::hide();
    lock_detector::stop();
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

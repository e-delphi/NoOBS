//! Buffer em memória (replay buffer) no Bridge — porte da seção "Buffer em
//! memória" do `OBSBridge.pas` (pegadinha #62).
//!
//! Guarda continuamente os últimos `replayMaxSec` segundos (ou `replayMaxMb`
//! MB) na RAM, na fila de pacotes do motor. O atalho, o botão ou o indicador
//! salvam o que está guardado como uma gravação da biblioteca e ESVAZIAM o
//! buffer (o corte cai num keyframe forçado: sem buraco nem repetição).
//!
//! Gravar com o buffer ligado CONTINUA o buffer (`replayIntoRecording`): a
//! mesma sessão passa a gravar, começando pela fila — um arquivo só. Fora
//! disso, gravar descarta o buffer, que volta sozinho ao fim (`ReplayWanted`).
//!
//! Main thread.

use std::cell::RefCell;
use std::path::Path;
use std::time::Instant;

use serde_json::json;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, GetProcessTimes};
use windows::Win32::Foundation::FILETIME;

use crate::types::RecordingMeta;
use crate::{config, encoder, engine, file_meta, gpu_usage, hotkey, lang, log, mainthread, player, probe, proc_watch, rec_indicator, ui};

use super::recording;
use super::{library, post_error, post_json, shell, transcribe};

pub const TIMER_REPLAY_SAVE_TIMEOUT: usize = 7012;
pub const TIMER_REPLAY_MEM: usize = 7014;
pub const TIMER_PERF_LOG: usize = 7018;
/// O "saved" pode não vir (erro de escrita não emite nada): desiste depois.
const REPLAY_SAVE_TIMEOUT_MS: u32 = 120_000;
const REPLAY_MEM_LOG_MS: u32 = 30_000;
const PERF_LOG_MS: u32 = 30_000;

const DEFAULT_REPLAY_MAX_SEC: i32 = 300;
const REPLAY_MIN_SEC: i32 = 10;
const REPLAY_MAX_SEC: i32 = 3600;
const DEFAULT_REPLAY_MAX_MB: i32 = 2048;
const REPLAY_MIN_MB: i32 = 128;
const REPLAY_MAX_MB: i32 = 32768;
/// RAM que o buffer nunca toca: acima disso o Windows pagina — exatamente o
/// engasgo que o buffer existe pra não causar.
const REPLAY_RAM_RESERVE_MB: i64 = 4096;
const DEFAULT_REPLAY_HOTKEY: &str = "Ctrl+Shift+F10";

pub const HK_REPLAY_SAVE: i32 = 102;
pub const HK_REPLAY_SAVE_ALT: i32 = 103;

#[derive(Default)]
struct State {
    /// Vontade da SESSÃO (botão da tela principal); não persiste. O arranque é
    /// o `replayAutoStart`, que o warmup converte nisto.
    wanted: bool,
    active: bool,
    /// Desde quando o buffer guarda (ou desde o último "salvar").
    start: Option<Instant>,
    /// Duração do trecho em salvamento, medida no "salvar" (sem abrir o arquivo).
    last_clip_sec: i32,
    /// Só desligamos o que o watcher de apps ligou.
    started_by_app_watch: bool,
    /// Desligado à mão com o app aberto: o watcher não religa até ele fechar.
    app_suppressed: bool,
    /// Segundos que a gravação em curso herdou do buffer (adianta o relógio).
    prefix_sec: i32,
    // Monitor de desempenho.
    perf_active: bool,
    perf_prev: Option<engine::VideoStats>,
    perf_prev_cpu: u64,
    perf_prev_wall: Option<Instant>,
}

thread_local! {
    static S: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    S.with(|s| f(&mut s.borrow_mut()))
}

pub fn replay_active() -> bool {
    with(|s| s.active)
}

pub fn replay_wanted() -> bool {
    with(|s| s.wanted)
}

/// Segundos que a gravação em curso herdou do buffer.
pub fn prefix_sec() -> i32 {
    with(|s| s.prefix_sec)
}

fn held_sec() -> i32 {
    with(|s| s.start.map(|t| t.elapsed().as_secs() as i32).unwrap_or(0)).min(max_sec_cfg())
}

/// Transcrever lê a gravação inteira e sobe dezenas de MB: durante a captura
/// (gravando, ou o buffer guardando — quando tem jogo) a AUTOMÁTICA espera. O
/// que o usuário pediu à mão roda mesmo assim (pegadinha #60c).
pub fn sync_transcribe_pause() {
    transcribe::sync_pause(recording::recording_active() || replay_active());
}

pub fn max_sec_cfg() -> i32 {
    config::get_config_int("replayMaxSec", DEFAULT_REPLAY_MAX_SEC).clamp(REPLAY_MIN_SEC, REPLAY_MAX_SEC)
}

/// Teto de memória NESTA máquina (RAM instalada − reserva). Vale pra UI
/// (último item do seletor) e pro clamp do config — inclusive editado à mão.
pub fn mem_limit_mb() -> i32 {
    let mut ms = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let mut r = REPLAY_MAX_MB as i64;
    if unsafe { GlobalMemoryStatusEx(&mut ms) }.is_ok() {
        r = ms.ullTotalPhys as i64 / (1024 * 1024) - REPLAY_RAM_RESERVE_MB;
    }
    // Pouca RAM: o mínimo ainda vale, senão o buffer nunca ligaria.
    r.clamp(REPLAY_MIN_MB as i64, REPLAY_MAX_MB as i64) as i32
}

pub fn max_mb_cfg() -> i32 {
    config::get_config_int("replayMaxMb", DEFAULT_REPLAY_MAX_MB).clamp(REPLAY_MIN_MB, mem_limit_mb())
}

/// Onde o motor escreve cada trecho. FORA da pasta de gravação: o watcher
/// veria o arquivo nascer vazio (pegadinha #51e).
fn temp_dir() -> String {
    let dir = crate::config::config_dir().join("buffer");
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().into_owned()
}

fn gb(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// Uma linha nas bordas do buffer e a cada 30 s: sem isso não há como separar
/// depois "o jogo travou pelo buffer" de "travou por outra coisa".
pub fn log_mem_usage(when: &str) {
    let mut pm = PROCESS_MEMORY_COUNTERS_EX { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
    // O "privado" do gerenciador de tarefas: o working set encolhe quando o
    // Windows apara o processo — justo o que acontece sob pressão.
    let ok = unsafe {
        GetProcessMemoryInfo(GetCurrentProcess(), &mut pm as *mut _ as *mut PROCESS_MEMORY_COUNTERS, pm.cb).is_ok()
    };
    let (priv_, ws) = if ok { (pm.PrivateUsage as u64, pm.WorkingSetSize as u64) } else { (0, 0) };
    let mut ms = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let _ = unsafe { GlobalMemoryStatusEx(&mut ms) };
    log!(
        "Buffer RAM [{when}]: NoOBS privado={} working set={} | sistema livre={} de {}",
        gb(priv_),
        gb(ws),
        gb(ms.ullAvailPhys),
        gb(ms.ullTotalPhys)
    );
}

// ---------------------------------------------------------------------------
// Indicador (o mesmo overlay da gravação, modo buffer)
// ---------------------------------------------------------------------------

fn show_indicator_from_config() {
    if !config::get_config_bool("replayIndicator", true) {
        return;
    }
    let start = with(|s| s.start).unwrap_or_else(Instant::now);
    rec_indicator::show(
        rec_indicator::parse_corner(&config::get_config_str("replayIndicatorCorner", "top-right")),
        start,
        config::get_config_int("replayIndicatorOpacity", 90),
        rec_indicator::Mode::Buffer,
        max_sec_cfg(),
        config::get_config_bool("replayIndicatorClickable", true),
    );
}

/// Esconde SÓ no modo buffer: no modo gravação o overlay é de outro dono.
fn hide_indicator() {
    if rec_indicator::is_showing() && rec_indicator::current_mode() == rec_indicator::Mode::Buffer {
        rec_indicator::hide();
    }
}

/// Tudo que a aba Buffer e o botão da tela principal mostram, num push só.
pub fn push_replay_state() {
    let (wanted, active, since) = with(|s| {
        (s.wanted, s.active, if s.active { s.start.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0) } else { 0 })
    });
    let saving = engine::with_engine(|e| e.is_replay_saving()).unwrap_or(false);
    post_json(json!({
        "type": "replay_state",
        "enabled": wanted,
        "autoStart": config::get_config_bool("replayAutoStart", false),
        "intoRecording": config::get_config_bool("replayIntoRecording", true),
        "active": active,
        "saving": saving,
        "sinceMs": since,
        "maxSec": max_sec_cfg(),
        "maxMb": max_mb_cfg(),
        "maxMbLimit": mem_limit_mb(),
        "hotkey": hotkey_spec(),
        "apps": config::get_config_str("replayAutoApps", ""),
        "indicator": config::get_config_bool("replayIndicator", true),
        "indicatorCorner": config::get_config_str("replayIndicatorCorner", "top-right"),
        "indicatorOpacity": config::get_config_int("replayIndicatorOpacity", 90),
        "indicatorClickable": config::get_config_bool("replayIndicatorClickable", true),
    }));
}

// ---------------------------------------------------------------------------
// Atalho de salvar
// ---------------------------------------------------------------------------

/// Atalho de salvar configurado (ou o padrão).
pub(super) fn hotkey_spec() -> String {
    config::get_config_str("replayHotkey", DEFAULT_REPLAY_HOTKEY)
}

fn unregister_hotkey() {
    ui::unregister_global_hotkey(HK_REPLAY_SAVE);
    ui::unregister_global_hotkey(HK_REPLAY_SAVE_ALT);
}

/// Só com o buffer LIGADO: desligado, a combinação volta a ser dos outros apps.
fn apply_hotkey() {
    unregister_hotkey();
    if !replay_active() {
        return;
    }
    let spec = hotkey_spec();
    if spec.trim().is_empty() {
        log::log("Buffer: atalho de salvar desativado (config vazia).");
        return;
    }
    let Some(hk) = hotkey::parse_hotkey(&spec) else {
        log!("Buffer: atalho invalido \"{spec}\" — ignorado.");
        return;
    };
    if let Some(reason) = hotkey::reserved_reason(hk.modifiers, hk.vk) {
        log!("Buffer: atalho \"{spec}\" e reservado pelo Windows ({reason}) — ignorado.");
        return;
    }
    if ui::register_global_hotkey(HK_REPLAY_SAVE, hk.modifiers, hk.vk) {
        log!("Buffer: atalho de salvar registrado \"{spec}\".");
    } else {
        log!("Buffer: RegisterHotKey falhou pra \"{spec}\" (outro app, ou o atalho de gravar, ja usa a combinacao).");
    }
    if hk.vk == hotkey::VK_PAUSE && hk.modifiers & hotkey::MOD_CTRL != 0 {
        ui::register_global_hotkey(HK_REPLAY_SAVE_ALT, hk.modifiers, hotkey::VK_CANCEL);
    }
}

// ---------------------------------------------------------------------------
// Trecho salvo
// ---------------------------------------------------------------------------

fn meta_for(duration: i32, layout: crate::types::RecordingLayout, encoder_id: &str) -> RecordingMeta {
    let (codec, hw) = encoder::describe_encoder_id(encoder_id);
    RecordingMeta {
        duration_sec: duration,
        layout,
        codec: codec.to_owned(),
        codec_hw: hw,
        fps: config::get_config_int("recordingFps", 30),
        quality_level: encoder::recording_quality_level(),
    }
}

/// Main, com o trecho já na pasta de gravação: o mesmo que o fim de uma
/// gravação comum faz (meta, card, aviso, transcrição).
fn finish_replay_saved(path: String, result: Result<i32, String>, meta: RecordingMeta) {
    // O pico do salvamento já passou: aqui se vê se a RAM do buffer antigo voltou.
    log_mem_usage("trecho salvo");
    let dur = match result {
        Ok(d) => d,
        Err(e) => {
            log!("Buffer: nao consegui mover o trecho pra pasta de gravacao: {e}");
            post_error(lang::t("error.replaySaveFailed"));
            return;
        }
    };
    let mut meta = meta;
    meta.duration_sec = dur;
    player::save_recording_meta(&path, &meta);
    file_meta::mark_changed(&path);
    library::push_recording_added(&path, dur);
    let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    post_json(json!({ "type": "replay_saved", "id": path, "name": name, "durationSec": dur }));
    // Jogando, a janela está escondida e o toast da UI não aparece.
    shell::maybe_notify_record(
        "NoOBS",
        &lang::t_with("replay.savedNotify", &[("min", &(dur / 60).to_string()), ("sec", &format!("{:02}", dur % 60))]),
    );
    transcribe::enqueue_after_capture(&path);
}

/// Engine: o trecho terminou de ser escrito na pasta temporária. Move em
/// WORKER (outro disco = cópia de até `replayMaxMb`).
fn on_engine_replay_saved(temp_path: &str) {
    ui::kill_timer(TIMER_REPLAY_SAVE_TIMEOUT);
    push_replay_state();
    if temp_path.is_empty() || !Path::new(temp_path).is_file() {
        log!("Buffer: \"saved\" sem arquivo (\"{temp_path}\").");
        post_error(lang::t("error.replaySaveFailed"));
        return;
    }
    // Lidos AGORA, na main: um próximo start sobrescreveria layout e encoder.
    let Some((layout, enc_id)) = engine::with_engine(|e| (e.current_layout().clone(), e.video_encoder_id().to_owned())) else {
        return;
    };
    let dest = recording::build_recording_path(chrono::Local::now(), &enc_id).to_string_lossy().into_owned();
    let est = with(|s| s.last_clip_sec);
    let meta = meta_for(est, layout, &enc_id);
    let temp = temp_path.to_owned();
    std::thread::spawn(move || {
        // Dois trechos no mesmo segundo ainda podem colidir no nome.
        let mut fin = dest.clone();
        let mut n = 2;
        while Path::new(&fin).exists() {
            let p = Path::new(&dest);
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            fin = p.with_file_name(format!("{stem} ({n}).mkv")).to_string_lossy().into_owned();
            n += 1;
        }
        let res = std::fs::rename(&temp, &fin)
            .or_else(|_| std::fs::copy(&temp, &fin).and_then(|_| std::fs::remove_file(&temp)))
            .map_err(|e| e.to_string())
            .map(|()| {
                // Duração SEM ler o arquivo: o tempo guardado no "salvar". O
                // Probe só entra sem medida.
                if est > 0 { est } else { probe::probe(Path::new(&fin)).map(|r| r.duration.round() as i32).unwrap_or(0) }
            });
        if super::is_shutting_down() {
            return;
        }
        mainthread::queue(move || finish_replay_saved(fin, res, meta));
    });
}

// ---------------------------------------------------------------------------
// Ligar / desligar
// ---------------------------------------------------------------------------

pub fn start_replay_buffer() {
    if replay_active() || recording::recording_active() {
        return;
    }
    // O teste de qualidade religa o buffer no fim da captura, se preciso.
    if super::video_test::capturing() {
        return;
    }
    // Gravação ainda finalizando: o fim dela chama de novo.
    if engine::with_engine(|e| e.is_stopping()).unwrap_or(false) {
        return;
    }
    let r = (|| -> Result<(), String> {
        engine::ensure_created();
        recording::with_engine_result(|e| e.ensure_initialized())?;
        recording::push_encoder_caps_once();
        let inputs = recording::gather_capture_inputs();
        let (dir, sec, mb) = (temp_dir(), max_sec_cfg(), max_mb_cfg());
        recording::with_engine_result(|e| e.build_and_start_replay(&dir, sec, mb, &inputs))
    })();
    match r {
        Ok(()) => {
            with(|s| {
                s.active = true;
                s.start = Some(Instant::now());
            });
            show_indicator_from_config();
            sync_transcribe_pause();
            log_mem_usage("buffer ligado");
            start_perf_monitor();
            ui::set_timer(TIMER_REPLAY_MEM, REPLAY_MEM_LOG_MS);
            // Sessão nova = fontes novas (mesma regra do start da gravação).
            super::devices::clear_mic_mute_applied();
            // Buffer ligado = o app tem que ficar acordado.
            ui::kill_timer(super::TIMER_HIBERNATE_IDLE);
            apply_hotkey();
        }
        Err(e) => {
            with(|s| s.active = false);
            log!("Buffer: falhou ao iniciar: {e}");
            post_error(lang::t_with("error.replayStartFailed", &[("error", &e)]));
        }
    }
    push_replay_state();
}

pub fn stop_replay_buffer(arm_hibernate: bool) {
    ui::kill_timer(TIMER_REPLAY_SAVE_TIMEOUT);
    unregister_hotkey();
    hide_indicator();
    let was = replay_active();
    if was {
        log_mem_usage("buffer parando");
    }
    ui::kill_timer(TIMER_REPLAY_MEM);
    if was {
        engine::with_engine(|e| e.stop_replay());
    }
    with(|s| s.active = false);
    sync_transcribe_pause();
    push_replay_state();
    // Desligado com a janela escondida: volta a poder hibernar.
    if arm_hibernate && !ui::is_window_shown() {
        shell::on_window_hidden();
    }
}

/// A gravação que começa agora herda o que o buffer guardou, NUM ARQUIVO SÓ:
/// a mesma sessão de captura passa a gravar, começando pela fila do buffer.
/// Do lado do Bridge o buffer deixa de existir (atalho, indicador, RAM); ele
/// religa quando a gravação parar.
pub fn continue_into_recording(output_path: &str) -> bool {
    if !replay_active() || !engine::exists() {
        return false;
    }
    let held = held_sec();
    if !engine::with_engine(|e| e.start_recording_from_replay(output_path)).unwrap_or(false) {
        return false;
    }
    with(|s| {
        s.prefix_sec = held;
        s.active = false;
    });
    unregister_hotkey();
    hide_indicator();
    log_mem_usage("buffer virando gravacao");
    ui::kill_timer(TIMER_REPLAY_MEM);
    push_replay_state();
    log!("Buffer: {held} s guardados viram o comeco da gravacao.");
    true
}

/// A gravação terminou (ou não continuou o buffer): nada mais a herdar.
pub fn clear_continuation() {
    with(|s| s.prefix_sec = 0);
}

pub fn handle_save_replay() {
    if !replay_active() || !engine::exists() {
        return;
    }
    if engine::with_engine(|e| e.is_replay_saving()).unwrap_or(false) {
        log::log("Buffer: salvar ignorado — o trecho anterior ainda esta sendo gravado.");
        push_replay_state();
        return;
    }
    // Medido ANTES da rotação: depois dela o "guardado" já é o do trecho seguinte.
    let held = held_sec();
    if engine::with_engine(|e| e.save_replay()).unwrap_or(false) {
        log_mem_usage("salvando trecho");
        // O buffer esvaziou: a contagem recomeça na UI e no indicador.
        with(|s| {
            s.last_clip_sec = held;
            s.start = Some(Instant::now());
        });
        show_indicator_from_config();
        ui::set_timer(TIMER_REPLAY_SAVE_TIMEOUT, REPLAY_SAVE_TIMEOUT_MS);
    }
    push_replay_state();
}

/// Monitor/webcam mudou: a cena (e o canvas) eram das fontes antigas. O que
/// estava guardado é descartado — foi o próprio usuário que mudou.
pub fn restart_for_source_change() {
    if !replay_active() {
        return;
    }
    log::log("Buffer: fonte de video mudou — recomecando o buffer.");
    stop_replay_buffer(false);
    start_replay_buffer();
}

/// Qualidade, fps, codec e áudio são fixados quando o encoder do buffer nasce:
/// vale NA HORA recomeçando (o guardado é descartado e a tela avisa). Gravando
/// não: derrubar os encoders cortaria a gravação continuada.
pub fn restart_for_encoder_change(what: &str) {
    if !replay_active() {
        return;
    }
    if recording::recording_active() {
        log!("Buffer: {what} mudou durante a gravacao — vale quando o buffer religar.");
        return;
    }
    log!("Buffer: {what} mudou — recomecando o buffer (o guardado e descartado).");
    stop_replay_buffer(false);
    start_replay_buffer();
    post_json(json!({ "type": "replay_restarted" }));
}

// ---------------------------------------------------------------------------
// Monitor de desempenho
// ---------------------------------------------------------------------------

fn process_cpu_100ns() -> u64 {
    let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) }.is_err() {
        return 0;
    }
    let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    ft(k) + ft(u)
}

fn perf_baseline() {
    let stats = engine::with_engine(|e| e.video_stats()).flatten();
    with(|s| {
        s.perf_prev = stats;
        s.perf_prev_cpu = process_cpu_100ns();
        s.perf_prev_wall = Some(Instant::now());
    });
}

/// Liga com o buffer OU a gravação; se desliga sozinho no tique sem nenhum.
pub fn start_perf_monitor() {
    if with(|s| std::mem::replace(&mut s.perf_active, true)) {
        return;
    }
    gpu_usage::start();
    perf_baseline();
    ui::set_timer(TIMER_PERF_LOG, PERF_LOG_MS);
    log!("Desempenho: monitor ligado (uma linha a cada {} s).", PERF_LOG_MS / 1000);
}

fn stop_perf_monitor() {
    if !with(|s| std::mem::replace(&mut s.perf_active, false)) {
        return;
    }
    ui::kill_timer(TIMER_PERF_LOG);
    gpu_usage::stop();
    log::log("Desempenho: monitor desligado.");
}

/// Delta de contador acumulado; ele ZERA quando o grafo é remontado.
fn delta(cur: u32, prev: u32) -> u32 {
    if cur >= prev { cur - prev } else { cur }
}

/// A cada 30 s: o que o OBS atrasou/pulou, o render médio, a placa por motor e
/// a CPU do NoOBS. Os avisos equivalentes do OBS saem em LOG_INFO, que o
/// nosso handler filtra (#53) — sem isto não sobraria nada no log.
fn perf_tick() {
    let recording = recording::recording_active();
    if !(replay_active() || recording) {
        stop_perf_monitor();
        return;
    }
    let what = if recording { "gravando" } else { "buffer" };
    let mut obs = "OBS sem video".to_owned();
    if let Some(st) = engine::with_engine(|e| e.video_stats()).flatten() {
        let prev = with(|s| s.perf_prev);
        let p = prev.unwrap_or_default();
        obs = format!(
            "render atrasou {} de {} quadros | encoder pulou {} de {} | render medio {:.1} ms (quadro de {:.1} ms)",
            delta(st.lagged, p.lagged),
            delta(st.total, p.total),
            delta(st.skipped, p.skipped),
            delta(st.v_total, p.v_total),
            st.avg_frame_ns as f64 / 1e6,
            st.interval_ns as f64 / 1e6
        );
        with(|s| s.perf_prev = Some(st));
    }
    let gpu = match gpu_usage::sample(unsafe { GetCurrentProcessId() }) {
        s if s.is_empty() => "sem contador".to_owned(),
        s => s,
    };
    let cpu = process_cpu_100ns();
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
    let (prev_cpu, prev_wall) = with(|s| (s.perf_prev_cpu, s.perf_prev_wall));
    let pct = prev_wall
        .map(|w| w.elapsed().as_secs_f64())
        .filter(|&secs| secs > 0.0)
        .map(|secs| (cpu.saturating_sub(prev_cpu)) as f64 / (secs * 1e7 * cpus) * 100.0)
        .unwrap_or(0.0);
    with(|s| {
        s.perf_prev_cpu = cpu;
        s.perf_prev_wall = Some(Instant::now());
    });
    log!("Desempenho [{what}]: {obs} | GPU: {gpu} | CPU NoOBS {pct:.1}%");
}

// ---------------------------------------------------------------------------
// Apps que ligam o buffer sozinho (WinProcWatch)
// ---------------------------------------------------------------------------

/// Watcher (thread dele). Liga quando um app aparece, desliga quando o último
/// fecha — e só desliga o que NÓS ligamos.
fn on_app_running_thread(running: bool) {
    mainthread::queue(move || {
        if super::is_shutting_down() {
            return;
        }
        // A lista pode ter sido esvaziada entre o poll e a fila.
        if config::get_config_str("replayAutoApps", "").trim().is_empty() {
            return;
        }
        if running {
            // Desligado à mão com o app aberto: respeita até o app fechar.
            if with(|s| s.app_suppressed || s.wanted) {
                return;
            }
            log::log("Buffer: app monitorado em execucao — ligando o buffer.");
            with(|s| {
                s.wanted = true;
                s.started_by_app_watch = true;
            });
            if recording::recording_active() {
                push_replay_state();
            } else {
                start_replay_buffer();
            }
        } else {
            let stop = with(|s| {
                s.app_suppressed = false;
                let stop = s.wanted && s.started_by_app_watch;
                if stop {
                    s.wanted = false;
                }
                s.started_by_app_watch = false;
                stop
            });
            if stop {
                log::log("Buffer: nenhum app monitorado rodando — desligando o buffer.");
                stop_replay_buffer(true);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Ciclo de vida e handlers
// ---------------------------------------------------------------------------

/// `initialization` + `DoInit`: callbacks do engine e o watcher de apps (sobe
/// mesmo com a lista vazia, para o callback ficar guardado — #54).
pub fn install() {
    engine::set_on_replay_saved(on_engine_replay_saved);
}

pub fn do_init() {
    proc_watch::start(&config::get_config_str("replayAutoApps", ""), on_app_running_thread);
}

/// Fim do warmup: `/start-replay` (a hibernação promoveu por um jogo) ou
/// `replayAutoStart` viram vontade da sessão. Se um `/start-record` acabou de
/// gravar, o buffer espera ele terminar.
pub fn on_warmup() {
    if ui::start_replay_requested() {
        with(|s| {
            s.wanted = true;
            s.started_by_app_watch = true;
        });
    } else if config::get_config_bool("replayAutoStart", false) {
        with(|s| s.wanted = true);
    }
    if replay_wanted() && !recording::recording_active() {
        start_replay_buffer();
    }
}

pub fn on_hotkey(id: i32) -> bool {
    if matches!(id, HK_REPLAY_SAVE | HK_REPLAY_SAVE_ALT) {
        handle_save_replay();
        return true;
    }
    false
}

pub fn on_timer(id: usize) -> bool {
    match id {
        TIMER_REPLAY_MEM => {
            if replay_active() {
                log_mem_usage("guardando");
            } else {
                ui::kill_timer(TIMER_REPLAY_MEM);
            }
        }
        TIMER_PERF_LOG => perf_tick(),
        TIMER_REPLAY_SAVE_TIMEOUT => {
            ui::kill_timer(TIMER_REPLAY_SAVE_TIMEOUT);
            engine::with_engine(|e| e.abort_replay_save());
            post_error(lang::t("error.replaySaveFailed"));
            push_replay_state();
        }
        _ => return false,
    }
    true
}

pub fn shutdown() {
    for t in [TIMER_REPLAY_SAVE_TIMEOUT, TIMER_REPLAY_MEM, TIMER_PERF_LOG] {
        ui::kill_timer(t);
    }
    unregister_hotkey();
    stop_perf_monitor();
}

pub fn handle_set_replay_enabled(enabled: bool) {
    with(|s| {
        s.wanted = enabled;
        if !enabled {
            // Desligado à mão com o app monitorado aberto: o watcher não religa.
            if s.started_by_app_watch {
                s.app_suppressed = true;
            }
            s.started_by_app_watch = false;
        }
    });
    log!("Buffer: {} pelo usuario.", if enabled { "ligado" } else { "desligado" });
    if enabled {
        // Gravando: entra sozinho quando a gravação terminar.
        if recording::recording_active() {
            push_replay_state();
        } else {
            start_replay_buffer();
        }
    } else {
        stop_replay_buffer(true);
    }
}

/// Vazia = recurso desligado (o watcher nem sobe).
pub fn handle_set_replay_apps(apps: &str) {
    let apps = apps.trim();
    config::set_config_str("replayAutoApps", apps);
    log!("Buffer: apps que ligam o buffer = \"{apps}\".");
    proc_watch::update_filter(apps);
    // Esvaziada com o buffer ligado POR ELA: ninguém mais desligaria.
    if apps.is_empty() && with(|s| s.started_by_app_watch) {
        with(|s| {
            s.started_by_app_watch = false;
            s.wanted = false;
        });
        stop_replay_buffer(true);
    }
    with(|s| s.app_suppressed = false);
    push_replay_state();
}

pub fn handle_set_replay_indicator(enable: bool) {
    config::set_config_bool("replayIndicator", enable);
    log!("Buffer: indicador de tela = {enable}.");
    if enable {
        if replay_active() {
            show_indicator_from_config();
        }
    } else {
        hide_indicator();
    }
    push_replay_state();
}

pub fn handle_set_replay_indicator_corner(corner: &str) {
    config::set_config_str("replayIndicatorCorner", corner);
    if replay_active() {
        show_indicator_from_config();
    }
    push_replay_state();
}

pub fn handle_set_replay_indicator_opacity(pct: i32) {
    let pct = pct.clamp(20, 100);
    config::set_config_int("replayIndicatorOpacity", pct);
    if rec_indicator::is_showing() && rec_indicator::current_mode() == rec_indicator::Mode::Buffer {
        rec_indicator::set_opacity(pct);
    }
}

pub fn handle_set_replay_indicator_clickable(enable: bool) {
    config::set_config_bool("replayIndicatorClickable", enable);
    log!("ReplayIndicatorClickable: {enable}");
    if rec_indicator::is_showing() && rec_indicator::current_mode() == rec_indicator::Mode::Buffer {
        rec_indicator::set_clickable(enable);
    }
    push_replay_state();
}

/// Preferência de ARRANQUE, separada do liga/desliga da tela principal.
pub fn handle_set_replay_autostart(auto_start: bool) {
    config::set_config_bool("replayAutoStart", auto_start);
    log!("Buffer: iniciar junto com o app = {}.", if auto_start { "sim" } else { "nao" });
    push_replay_state();
}

pub fn handle_set_replay_into_recording(enabled: bool) {
    config::set_config_bool("replayIntoRecording", enabled);
    log!("Buffer: gravacao comeca pelo buffer = {}.", if enabled { "sim" } else { "nao" });
    push_replay_state();
}

/// Vale a partir da próxima rotação (não há `update` na saída).
pub fn handle_set_replay_limits(max_sec: i32, max_mb: i32) {
    if max_sec > 0 {
        config::set_config_int("replayMaxSec", max_sec);
    }
    if max_mb > 0 {
        config::set_config_int("replayMaxMb", max_mb);
    }
    if replay_active() {
        let (s, m) = (max_sec_cfg(), max_mb_cfg());
        engine::with_engine(|e| e.set_replay_limits(s, m));
    }
    push_replay_state();
}

/// Mesma regra do atalho de gravar, mais uma: não pode ser a MESMA
/// combinação do de gravar nem do da área (o `RegisterHotKey` recusaria a
/// segunda, e um dos dois deixaria de funcionar sem aviso).
pub fn handle_set_replay_hotkey(spec: &str) {
    let mut normalized = spec.trim().to_owned();
    if !normalized.is_empty() {
        let Some(hk) = hotkey::parse_hotkey(&normalized) else {
            log!("HandleSetReplayHotkey: spec invalido \"{normalized}\" — ignorado.");
            return;
        };
        if let Some(reason) = hotkey::reserved_reason(hk.modifiers, hk.vk) {
            log!("HandleSetReplayHotkey: \"{normalized}\" reservado ({reason}) — ignorado.");
            return;
        }
        normalized = hotkey::format_hotkey(hk.modifiers, hk.vk);
        if normalized.eq_ignore_ascii_case(&config::get_config_str("hotkey", "Pause/Break")) {
            log!("HandleSetReplayHotkey: \"{normalized}\" ja e o atalho de gravar — ignorado.");
            post_error(lang::t("error.replayHotkeyConflict"));
            return;
        }
        if normalized.eq_ignore_ascii_case(&super::area::hotkey_spec()) {
            log!("HandleSetReplayHotkey: \"{normalized}\" ja e o atalho da area — ignorado.");
            post_error(lang::t("error.areaHotkeyConflict"));
            return;
        }
    }
    config::set_config_str("replayHotkey", &normalized);
    apply_hotkey();
    push_replay_state();
}

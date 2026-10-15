//! Motor de gravação via libobs (porte de `OBSEngine.pas`).
//!
//! O core (`obs_startup`) inicializa uma vez e permanece vivo entre gravações;
//! cena, encoders e saída são recriados a cada sessão. Teardown completo só na
//! saída do app. TUDO aqui roda na MAIN thread (pegadinha #3) — inclusive o
//! `obs_shutdown`, par do `obs_startup` (pegadinha #31).
//!
//! ## Como o Delphi virou Rust
//! * `TOBSEngine` + globais `G*` → [`Engine`] numa `thread_local` (main thread).
//!   Os callbacks C do libobs rodam em threads internas e só enfileiram
//!   trabalho para a main ([`mainthread::queue`], pegadinha #3/#41).
//! * Callbacks `OnStopped`/`OnReplaySaved`/`OnReplayPrefixSaved` → [`Event`]s
//!   acumulados no engine e entregues por [`with_engine`] DEPOIS de soltar o
//!   empréstimo — o handler pode chamar o engine de novo sem reentrância.
//! * `raise Exception` → `Result<_, String>`; `try/finally` → `Drop` (`ObsData`).
//! * Entradas de captura (monitores, webcams, dispositivos padrão) chegam por
//!   [`CaptureInputs`]; o engine não toca Win32.

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CString};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};

use crate::audio_tracks::{
    build_track_names, compute_audio_track_assignments, count_true, enumerate_obs_audio_devices,
};
use crate::config;
use crate::encoder;
use crate::ffi::cstr_to_string;
use crate::ffi::obs::*;
use crate::scene::{filter_enabled_monitors, ObsMonitor};
use crate::types::{ObsAudioDev, RecordingLayout, RecordingRegion, Webcam};
use crate::{lang, log, mainthread};

const SCENE_NAME: &str = "NoOBS";
const MANAGED_PREFIX: &str = "NoOBS ";
/// Sinal do output emitido quando a gravação terminou de fato (`obs-output.c`).
const SIG_STOP: &str = "stop";
/// Sinal do `replay_buffer`: o trecho pedido por "save" foi escrito em disco.
const SIG_SAVED: &str = "saved";
/// Keyframe do BUFFER (só dele): teto da sobreposição entre dois trechos (#62).
pub const REPLAY_KEYFRAME_SEC: i32 = 1;
/// Folga sobre o keyframe antes de fechar o trecho (#62).
const REPLAY_SAVE_COMMIT_MARGIN_MS: u64 = 250;
/// Prazo do `obs_shutdown` antes do watchdog encerrar o processo (#31/#48).
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(5000);

// ---------------------------------------------------------------------------
// Estado global tocado pelo handler de log do libobs (thread dele)
// ---------------------------------------------------------------------------

/// Escrito pelo handler de log (thread do libobs), lido pela main depois da
/// montagem. Boolean de uma via — não precisa de lock.
static AUDIO_CAPTURE_FAULT: AtomicBool = AtomicBool::new(false);
/// Ligado só durante o Teardown: as mensagens do `obs_shutdown` são INFO.
static VERBOSE_SHUTDOWN_LOG: AtomicBool = AtomicBool::new(false);

/// O win-wasapi falhou em agendar a captura (RTWQ) na ÚLTIMA montagem: as
/// fontes existem mas entregam silêncio (pegadinha #48).
pub fn had_audio_capture_fault() -> bool {
    AUDIO_CAPTURE_FAULT.load(Ordering::SeqCst)
}

pub fn reset_audio_capture_fault() {
    AUDIO_CAPTURE_FAULT.store(false, Ordering::SeqCst);
}

/// `void (*)(int level, const char *msg, va_list args, void *p)`.
///
/// X64 cdecl varargs: `args` aponta para a primeira variádica na stack; para
/// mensagens com `%s` como primeiro argumento, desreferencia-se para obter a
/// string (só a primeira — o resto fica na mensagem crua).
unsafe extern "C" fn obs_log_handler(level: i32, msg: *const c_char, args: *mut c_void, _p: *mut c_void) {
    if msg.is_null() {
        return;
    }
    // Durante o shutdown deixamos passar INFO, para ver em que subsistema ele
    // para; fora disso o filtro é WARNING.
    let limit = LOG_WARNING + if VERBOSE_SHUTDOWN_LOG.load(Ordering::Relaxed) { 100 } else { 0 };
    if level > limit {
        return;
    }
    let raw = cstr_to_string(msg);

    // RTWQ: se o lock da fila falha, o callback nunca é agendado e a fonte
    // entrega SILÊNCIO sem erro nenhum (pegadinha #48).
    if raw.contains("RTWQ setup failed") || raw.contains("Could not requeue sample receive work") {
        AUDIO_CAPTURE_FAULT.store(true, Ordering::SeqCst);
    }

    // Avisos benignos: cleanup interno de plugins, frontend OBS ausente,
    // destruição diferida de sources.
    if raw.contains("Double destroy just occurred")
        || raw.contains("UI task could not be queued")
        || raw.contains("duplicate name")
    {
        return;
    }

    let prefix = match level {
        LOG_ERROR => "obs[E]",
        LOG_WARNING => "obs[W]",
        _ => "obs[?]",
    };

    let mut arg1 = String::new();
    if !args.is_null() && raw.contains("%s") {
        let p = *(args as *const *const c_char);
        if !p.is_null() {
            arg1 = cstr_to_string(p);
        }
    }
    if arg1.is_empty() {
        log!("{prefix} {raw}");
    } else {
        log!("{prefix} {raw} [arg1=\"{arg1}\"]");
    }
}

// ---------------------------------------------------------------------------
// Entradas e pedidos
// ---------------------------------------------------------------------------

/// O que a máquina oferece para a captura, coletado pelo chamador (Win32).
#[derive(Debug, Clone, Default)]
pub struct CaptureInputs {
    /// Todos os monitores detectados (o engine filtra os habilitados).
    pub monitors: Vec<ObsMonitor>,
    pub webcams: Vec<Webcam>,
    /// Endpoints padrão do Windows (mesmo id do libobs); vazio = desconhecido.
    pub default_mic_id: String,
    pub default_spk_id: String,
}

/// Gravação de UMA janela (`window_capture`) em vez de monitores/webcams.
/// O HWND só dimensiona o canvas; quem acha a janela a cada quadro é o plugin,
/// pela string (pegadinha #66).
#[derive(Debug, Clone, Default)]
pub struct WindowCapture {
    /// `"titulo:classe:exe"` do OBS (WinWindows).
    pub spec: String,
    pub title: String,
    pub handle: isize,
    /// Tamanho da área cliente (0 se a janela não tem tamanho legível).
    pub width: i32,
    pub height: i32,
    /// Som SÓ do app, numa faixa isolada própria.
    pub audio: bool,
    pub app: String,
}

#[derive(Debug, Clone, Default)]
pub struct RecordRequest {
    pub output_path: String,
    /// Perfil de auto-gravação (watcher de mic, pegadinha #47).
    pub auto_profile: bool,
    pub window: Option<WindowCapture>,
}

/// Notificações para o Bridge, sempre na main thread e fora do empréstimo.
#[derive(Debug)]
pub enum Event {
    /// Gravação parou de verdade (arquivo completo). Equivale ao RecordingStop.
    Stopped(String),
    /// Trecho do buffer gravado em disco ('' se não deu pra saber o caminho).
    ReplaySaved(String),
    /// Trecho do buffer que virou o COMEÇO de uma gravação; '' = não saiu.
    /// O número é o `lead_ms` que desempata a emenda (`SpliceContinuation`).
    ReplayPrefixSaved(String, i32),
}

#[derive(Default)]
struct Handlers {
    on_stopped: Option<Rc<dyn Fn(&str)>>,
    on_replay_saved: Option<Rc<dyn Fn(&str)>>,
    on_replay_prefix_saved: Option<Rc<dyn Fn(&str, i32)>>,
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static HANDLERS: RefCell<Handlers> = RefCell::new(Handlers::default());
}

pub fn set_on_stopped(f: impl Fn(&str) + 'static) {
    HANDLERS.with(|h| h.borrow_mut().on_stopped = Some(Rc::new(f)));
}

pub fn set_on_replay_saved(f: impl Fn(&str) + 'static) {
    HANDLERS.with(|h| h.borrow_mut().on_replay_saved = Some(Rc::new(f)));
}

pub fn set_on_replay_prefix_saved(f: impl Fn(&str, i32) + 'static) {
    HANDLERS.with(|h| h.borrow_mut().on_replay_prefix_saved = Some(Rc::new(f)));
}

fn deliver(events: Vec<Event>) {
    for ev in events {
        match ev {
            Event::Stopped(p) => {
                if let Some(cb) = HANDLERS.with(|h| h.borrow().on_stopped.clone()) {
                    cb(&p);
                }
            }
            Event::ReplaySaved(p) => {
                if let Some(cb) = HANDLERS.with(|h| h.borrow().on_replay_saved.clone()) {
                    cb(&p);
                }
            }
            Event::ReplayPrefixSaved(p, lead) => {
                if let Some(cb) = HANDLERS.with(|h| h.borrow().on_replay_prefix_saved.clone()) {
                    cb(&p, lead);
                }
            }
        }
    }
}

/// O engine já foi criado? (Delphi: `Engine <> nil`.)
pub fn exists() -> bool {
    ENGINE.with(|c| c.try_borrow().map(|b| b.is_some()).unwrap_or(true))
}

/// Cria o engine se ainda não existe (sem subir o libobs).
pub fn ensure_created() {
    ENGINE.with(|c| {
        if let Ok(mut b) = c.try_borrow_mut() {
            if b.is_none() {
                *b = Some(Engine::new());
            }
        }
    });
}

/// Roda `f` no engine (se existir) e depois entrega os eventos gerados, já com
/// o empréstimo solto. `None` se não existe ou se chamado de dentro de outro
/// `with_engine` (reentrância).
pub fn with_engine<R>(f: impl FnOnce(&mut Engine) -> R) -> Option<R> {
    let (r, events) = ENGINE.with(|c| {
        let mut b = c.try_borrow_mut().ok()?;
        let e = b.as_mut()?;
        let r = f(e);
        Some((r, std::mem::take(&mut e.events)))
    })?;
    deliver(events);
    Some(r)
}

/// Descarta o engine (Delphi: `FreeAndNil(Engine)` → `Teardown`).
pub fn destroy() {
    let taken = ENGINE.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.take()));
    drop(taken); // Drop faz o teardown
}

// ---------------------------------------------------------------------------
// Callbacks C → main thread
// ---------------------------------------------------------------------------

/// Sinal "stop": roda numa thread INTERNA do libobs — proibido tocar libobs/UI
/// aqui (pegadinha #3). Só marshala para a main.
unsafe extern "C" fn stop_signal_thunk(_data: *mut c_void, _cd: CalldataPtr) {
    mainthread::queue(|| {
        with_engine(|e| e.on_stop_signal());
    });
}

/// Sinal "saved" do replay_buffer — emitido pela thread de mux dele.
unsafe extern "C" fn replay_saved_thunk(_data: *mut c_void, _cd: CalldataPtr) {
    mainthread::queue(|| {
        with_engine(|e| e.on_replay_saved_signal());
    });
}

unsafe extern "C" fn remove_source_cb(_param: *mut c_void, source: ObsSource) -> bool {
    if !source.is_null() {
        if let Ok(a) = api() {
            (a.obs_source_remove)(source);
        }
    }
    true // continua a enumeração
}

/// Desmonta TUDO que o libobs ainda tem registrado, na ordem do
/// `ClearSceneData` do OBS. Sem isto o `obs_shutdown` precisa desmontar as
/// fontes sozinho — e trava, porque fontes WASAPI ainda têm thread viva.
/// Soltar as NOSSAS referências não basta: a source segue na lista do core.
fn clear_all_obs_data() {
    // libobs/obs-defs.h: MAX_CHANNELS 64 — limpa TODOS, como o frontend.
    const MAX_CHANNELS: u32 = 64;
    let Ok(a) = api() else { return };
    unsafe {
        for i in 0..MAX_CHANNELS {
            (a.obs_set_output_source)(i, std::ptr::null_mut());
        }
        (a.obs_enum_scenes)(remove_source_cb, std::ptr::null_mut());
        (a.obs_enum_sources)(remove_source_cb, std::ptr::null_mut());
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Hook dos filtros de áudio dos microfones (`OBSAudioFilters.ApplyAudioFilters`),
/// registrado pelo módulo de filtros. Sem ele a cadeia simplesmente não existe.
static MIC_FILTER_HOOK: OnceLock<fn(ObsSource)> = OnceLock::new();

pub fn set_mic_filter_hook(f: fn(ObsSource)) {
    let _ = MIC_FILTER_HOOK.set(f);
}

/// Locale do OBS a partir do idioma do app (rótulos dos filtros, pegadinha #63).
pub fn obs_locale_for_app(lang_code: &str) -> String {
    let l = lang_code.trim().to_lowercase();
    if lang_code.contains('-') {
        return lang_code.to_owned();
    }
    match l.as_str() {
        "pt" => "pt-BR",
        "es" => "es-ES",
        "fr" => "fr-FR",
        "de" => "de-DE",
        "it" => "it-IT",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "zh" => "zh-CN",
        "ru" => "ru-RU",
        _ => "en-US",
    }
    .to_owned()
}

/// `Round` do Delphi arredonda para o par mais próximo (banker's).
fn round(v: f64) -> i32 {
    v.round_ties_even() as i32
}

fn same_text(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Resolve o `monitor_id` interno do OBS (varia entre reinstalls/drivers): cria
/// a lista de propriedades de `monitor_capture` e casa pelo sufixo `@ X,Y` da
/// descrição (pegadinha #20).
fn resolve_monitor_id(m: &ObsMonitor) -> String {
    let Ok(a) = api() else { return String::new() };
    let pos_tag = format!("@ {},{}", m.position_x, m.position_y);
    let id = cstring("monitor_capture");
    let key = cstring("monitor_id");
    let mut result = String::new();
    unsafe {
        let props = (a.obs_get_source_properties)(id.as_ptr());
        if props.is_null() {
            return result;
        }
        let prop = (a.obs_properties_get)(props, key.as_ptr());
        if !prop.is_null() {
            let count = (a.obs_property_list_item_count)(prop);
            for i in 0..count {
                let name = cstr_to_string((a.obs_property_list_item_name)(prop, i));
                let value = cstr_to_string((a.obs_property_list_item_string)(prop, i));
                if name.contains(&pos_tag) && value != "DUMMY" {
                    result = value;
                    break;
                }
            }
        }
        (a.obs_properties_destroy)(props);
    }
    result
}

struct SourceEntry {
    source: ObsSource,
    name: String,
}

// ---------------------------------------------------------------------------
// Watchdog do obs_shutdown
// ---------------------------------------------------------------------------

/// Rede de segurança do `obs_shutdown`, que roda na MAIN thread: com o
/// `RtwqStartup` no arranque (#48) conclui em ~80 ms e este watchdog nunca
/// dispara. Se uma regressão voltar a travar o teardown, a main bloqueada não
/// vigia a si mesma — esta thread espera o prazo e encerra o processo (melhor
/// que virar zumbi segurando o mutex de instância única). Não dumpa pilha: a
/// técnica está na pegadinha #31 do CLAUDE.md.
struct ShutdownWatchdog {
    cancel: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ShutdownWatchdog {
    fn start(timeout: Duration) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            // Disconnect (Cancel + Drop) também conta como "terminou".
            if !matches!(rx.recv_timeout(timeout), Err(mpsc::RecvTimeoutError::Timeout)) {
                return;
            }
            // Segunda chance curta: fecha a corrida em que o obs_shutdown
            // terminou exatamente no estouro do prazo.
            if !matches!(rx.recv_timeout(Duration::from_millis(250)), Err(mpsc::RecvTimeoutError::Timeout)) {
                return;
            }
            log!(
                "libobs: obs_shutdown NAO retornou em {}ms — encerrando o processo (OS libera o resto).",
                timeout.as_millis()
            );
            use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
            unsafe {
                let _ = TerminateProcess(GetCurrentProcess(), 0);
            }
        });
        Self { cancel: tx, thread: Some(thread) }
    }

    fn cancel(mut self) {
        let _ = self.cancel.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

pub struct Engine {
    initialized: bool,
    recording: bool,
    /// Stop assíncrono em andamento: `obs_output_stop` foi chamado mas o sinal
    /// "stop" (release seguro) ainda não chegou (#41).
    stopping: bool,
    /// Deixa o Bridge distinguir "finalização normal" de "travou".
    stopping_since: Option<Instant>,
    shutting_down: bool,
    stop_signal_handler: SignalHandler,
    output_path: String,
    exe_dir: std::path::PathBuf,
    plugin_bin_dir: std::path::PathBuf,
    plugin_data_dir: std::path::PathBuf,
    /// Layout do canvas atual: o Bridge persiste em `<hash>.json`.
    current_layout: RecordingLayout,
    /// ID do encoder que a gravação REALMENTE usou (o fallback pode ter
    /// escolhido outro que o pedido). Vira meta do arquivo.
    video_encoder_id: String,
    window: Option<WindowCapture>,

    // objetos libobs da sessão (os `G*` do Delphi)
    scene: ObsScene,
    output: ObsOutput,
    /// Buffer: a saída ANTIGA, gravando um trecho em disco enquanto `output`
    /// (a nova) já guarda o que vem depois.
    replay_saving: ObsOutput,
    video_encoder: ObsEncoder,
    audio_encoders: Vec<ObsEncoder>,
    sources: Vec<SourceEntry>,

    // buffer em memória (replay_buffer, pegadinha #62)
    replay_active: bool,
    replay_dir: String,
    replay_max_sec: i32,
    replay_max_mb: i32,
    replay_seq: i32,
    replay_saved_handler: SignalHandler,
    replay_save_since: Option<Instant>,
    /// Rotação feita, "save" ainda não pedido: é essa espera que faz os dois
    /// trechos se SOBREPOREM em vez de ter buraco.
    replay_save_commit_pending: bool,
    /// O "save" em curso é o começo de uma gravação, não um trecho avulso.
    prefix_mode: bool,
    /// A gravação já parou mas o trecho do buffer ainda é escrito.
    prefix_await_stop: bool,
    prefix_stop_invoke_cb: bool,
    rec_start: Option<Instant>,
    prefix_lead_ms: i32,

    events: Vec<Event>,
}

impl Engine {
    fn new() -> Self {
        Self {
            initialized: false,
            recording: false,
            stopping: false,
            stopping_since: None,
            shutting_down: false,
            stop_signal_handler: std::ptr::null_mut(),
            output_path: String::new(),
            exe_dir: Default::default(),
            plugin_bin_dir: Default::default(),
            plugin_data_dir: Default::default(),
            current_layout: RecordingLayout::default(),
            video_encoder_id: String::new(),
            window: None,
            scene: std::ptr::null_mut(),
            output: std::ptr::null_mut(),
            replay_saving: std::ptr::null_mut(),
            video_encoder: std::ptr::null_mut(),
            audio_encoders: Vec::new(),
            sources: Vec::new(),
            replay_active: false,
            replay_dir: String::new(),
            replay_max_sec: 0,
            replay_max_mb: 0,
            replay_seq: 0,
            replay_saved_handler: std::ptr::null_mut(),
            replay_save_since: None,
            replay_save_commit_pending: false,
            prefix_mode: false,
            prefix_await_stop: false,
            prefix_stop_invoke_cb: false,
            rec_start: None,
            prefix_lead_ms: 0,
            events: Vec::new(),
        }
    }

    // ---- consultas --------------------------------------------------------

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    pub fn is_recording(&self) -> bool {
        self.recording
    }
    pub fn is_stopping(&self) -> bool {
        self.stopping
    }
    pub fn output_path(&self) -> &str {
        &self.output_path
    }
    pub fn current_layout(&self) -> &RecordingLayout {
        &self.current_layout
    }
    pub fn video_encoder_id(&self) -> &str {
        &self.video_encoder_id
    }
    /// Há quanto tempo está finalizando (ms). 0 se não está.
    pub fn stopping_elapsed_ms(&self) -> u64 {
        match (self.stopping, self.stopping_since) {
            (true, Some(t)) => t.elapsed().as_millis() as u64,
            _ => 0,
        }
    }
    pub fn is_replay_active(&self) -> bool {
        self.replay_active
    }
    pub fn is_replay_saving(&self) -> bool {
        self.replay_save_since.is_some()
    }
    pub fn replay_save_elapsed_ms(&self) -> u64 {
        self.replay_save_since.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0)
    }
    pub fn is_replay_save_commit_pending(&self) -> bool {
        self.replay_save_commit_pending
    }
    /// Trecho do buffer que vai virar começo da gravação ainda sendo escrito.
    pub fn is_prefix_saving(&self) -> bool {
        self.prefix_mode && self.replay_save_since.is_some()
    }
    /// Quanto esperar entre a rotação e o "save": um intervalo de keyframe do
    /// buffer mais folga. É o teto da sobreposição entre dois trechos.
    pub fn replay_save_commit_delay_ms(&self) -> u64 {
        REPLAY_KEYFRAME_SEC as u64 * 1000 + REPLAY_SAVE_COMMIT_MARGIN_MS
    }

    /// True se o win-wasapi registrou a captura de áudio por processo
    /// (Windows 10 2004+). False também com o libobs ainda não iniciado.
    pub fn app_audio_available(&self) -> bool {
        if !self.initialized {
            return false;
        }
        let Ok(a) = api() else { return false };
        let id = cstring("wasapi_process_output_capture");
        unsafe { !(a.obs_get_latest_input_type_id)(id.as_ptr()).is_null() }
    }

    /// Contadores ACUMULADOS do pipeline de vídeo (o monitor de desempenho faz
    /// o delta). `None` sem libobs/vídeo.
    pub fn video_stats(&self) -> Option<VideoStats> {
        if !self.initialized {
            return None;
        }
        let a = api().ok()?;
        unsafe {
            let v = (a.obs_get_video)();
            if v.is_null() {
                return None;
            }
            Some(VideoStats {
                total: (a.obs_get_total_frames)(),
                lagged: (a.obs_get_lagged_frames)(),
                v_total: (a.video_output_get_total_frames)(v),
                skipped: (a.video_output_get_skipped_frames)(v),
                avg_frame_ns: (a.obs_get_average_frame_time_ns)(),
                interval_ns: (a.obs_get_frame_interval_ns)(),
            })
        }
    }

    pub fn set_source_muted(&self, source_name: &str, muted: bool) {
        let Ok(a) = api() else { return };
        if let Some(s) = self.sources.iter().find(|s| s.name == source_name) {
            unsafe { (a.obs_source_set_muted)(s.source, muted) }
        }
    }

    // ---- init -------------------------------------------------------------

    fn resolve_paths(&mut self) {
        // NoOBS.exe roda em obs\bin\64bit\ (ao lado da obs.dll); plugins e data
        // ficam em ..\..\obs-plugins\64bit e ..\..\data\obs-plugins.
        self.exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_default();
        let root = self.exe_dir.ancestors().nth(2).map(|p| p.to_path_buf()).unwrap_or_default();
        self.plugin_bin_dir = root.join("obs-plugins").join("64bit");
        self.plugin_data_dir = root.join("data").join("obs-plugins");
    }

    /// Carrega APENAS os plugins necessários. `obs_load_all_modules` não
    /// filtra e o `obs-websocket` crasha sem frontend (pegadinha #13).
    fn load_modules(&self) -> Result<(), String> {
        const WANTED: [&str; 7] = [
            "obs-ffmpeg",  // ffmpeg_muxer (output) + ffmpeg_aac (encoder de áudio)
            "obs-x264",    // encoder CPU fallback
            "obs-nvenc",   // encoder HEVC/H264 NVIDIA (opcional)
            "win-capture", // monitor_capture / window_capture
            "win-dshow",   // dshow_input (webcam)
            "win-wasapi",  // wasapi_input/output_capture
            "obs-filters", // filtros de áudio dos mics (opcional)
        ];
        let a = api()?;
        let (mut loaded, mut failed) = (0, 0);
        for name in WANTED {
            let bin = self.plugin_bin_dir.join(format!("{name}.dll"));
            let data = self.plugin_data_dir.join(name);
            let bin_c = cstring(&bin.to_string_lossy());
            let data_c = cstring(&data.to_string_lossy());
            let mut module: *mut c_void = std::ptr::null_mut();
            let rc = unsafe { (a.obs_open_module)(&mut module, bin_c.as_ptr(), data_c.as_ptr()) };
            if rc == 0 && !module.is_null() && unsafe { (a.obs_init_module)(module) } {
                loaded += 1;
                continue;
            }
            failed += 1;
            log!("libobs: plugin {name} falhou (rc={rc}).");
        }
        unsafe { (a.obs_post_load_modules)() };
        log!("libobs: {loaded} plugins carregados, {failed} falharam.");
        Ok(())
    }

    /// `obs_startup` + vídeo placeholder + áudio + plugins. Idempotente.
    pub fn ensure_initialized(&mut self) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }
        self.resolve_paths();
        if !self.exe_dir.join("obs.dll").exists() {
            return Err(format!(
                "obs.dll nao encontrado em {}. NoOBS.exe precisa rodar da pasta bin\\64bit do OBS.",
                self.exe_dir.display()
            ));
        }
        let a = api()?;
        let locale = cstring("en-US");
        if !unsafe { (a.obs_startup)(locale.as_ptr(), std::ptr::null(), std::ptr::null_mut()) } {
            return Err("obs_startup falhou.".into());
        }
        log!("libobs: startup ok.");
        unsafe { (a.base_set_log_handler)(obs_log_handler, std::ptr::null_mut()) };

        let init = || -> Result<(), String> {
            // Vídeo: canvas placeholder 1920x1080 (reconfigurado depois). O
            // graphics_module é um nome simples: o LoadLibrary resolve pela
            // pasta do exe.
            let graphics = cstring("libobs-d3d11");
            let mut ovi = video_info(&graphics, 30, 1920, 1080);
            let rc = unsafe { (a.obs_reset_video)(&mut ovi) };
            if rc != OBS_VIDEO_SUCCESS {
                return Err(format!("obs_reset_video falhou (code={rc})."));
            }
            log!("libobs: video ok (1920x1080 placeholder).");

            let oai = ObsAudioInfo { samples_per_sec: 48000, speakers: SPEAKERS_STEREO };
            if !unsafe { (a.obs_reset_audio)(&oai) } {
                return Err("obs_reset_audio falhou.".into());
            }
            log!("libobs: audio ok (48kHz stereo).");

            self.load_modules()?;
            // Rótulos dos filtros (vêm do plugin) no idioma do app.
            let loc = cstring(&obs_locale_for_app(&lang::current_language()));
            unsafe { (a.obs_set_locale)(loc.as_ptr()) };
            Ok(())
        };
        if let Err(e) = init() {
            log!("libobs: init parcial — chamando obs_shutdown pra reset.");
            unsafe { (a.obs_shutdown)() };
            return Err(e);
        }
        self.initialized = true;
        Ok(())
    }

    // ---- liberação --------------------------------------------------------

    /// Ordem: saídas → encoders → sources → scene. As DUAS saídas do buffer
    /// (a corrente e a que ainda grava um trecho) caem antes dos encoders que
    /// elas compartilham.
    fn release_recording_objects(&mut self) {
        // Sem libobs iniciado só zeramos os ponteiros: chamar
        // obs_set_output_source antes do obs_startup dá AV (pegadinha #32).
        if !self.initialized {
            self.output = std::ptr::null_mut();
            self.replay_saving = std::ptr::null_mut();
            self.video_encoder = std::ptr::null_mut();
            self.audio_encoders.clear();
            self.sources.clear();
            self.scene = std::ptr::null_mut();
            return;
        }
        let Ok(a) = api() else { return };
        unsafe {
            // Sem limpar os canais, a próxima gravação herda referências velhas.
            for i in 0..64u32 {
                (a.obs_set_output_source)(i, std::ptr::null_mut());
            }
        }
        self.drop_saving_replay();
        unsafe {
            if !self.output.is_null() {
                (a.obs_output_release)(self.output);
                self.output = std::ptr::null_mut();
            }
            if !self.video_encoder.is_null() {
                (a.obs_encoder_release)(self.video_encoder);
                self.video_encoder = std::ptr::null_mut();
            }
            for e in self.audio_encoders.drain(..) {
                if !e.is_null() {
                    (a.obs_encoder_release)(e);
                }
            }
            for s in self.sources.drain(..) {
                if !s.source.is_null() {
                    (a.obs_source_release)(s.source);
                }
            }
            if !self.scene.is_null() {
                (a.obs_scene_release)(self.scene);
                self.scene = std::ptr::null_mut();
            }
        }
    }

    // ---- montagem do grafo de captura --------------------------------------

    fn create_source(&mut self, id: &str, name: &str, settings: ObsData) -> Result<ObsSource, String> {
        let a = api()?;
        let (id_c, name_c) = (cstring(id), cstring(name));
        let src = unsafe { (a.obs_source_create)(id_c.as_ptr(), name_c.as_ptr(), settings.raw(), std::ptr::null_mut()) };
        if src.is_null() {
            return Err(format!("obs_source_create falhou para \"{name}\" ({id})."));
        }
        self.sources.push(SourceEntry { source: src, name: name.to_owned() });
        Ok(src) // `settings` é liberado no Drop
    }

    /// Passos 1 a 7: canvas, cena, fontes de monitor/webcam/áudio e encoders.
    /// Comum à gravação em arquivo e ao buffer — só a SAÍDA muda. Não limpa em
    /// caso de erro: quem chama chama `release_recording_objects`.
    fn build_capture_graph(&mut self, auto_profile: bool, inputs: &CaptureInputs) -> Result<(), String> {
        let a = api()?;
        // Um dispositivo entra na gravação? Na auto-gravação também exige o
        // perfil (aba Comportamento). Centraliza o if para webcam/mic/speaker.
        let src_active = |cat: &str, id: &str, default: bool| -> bool {
            if auto_profile {
                config::get_source_active_for_auto(cat, id, default)
            } else {
                config::get_source_active(cat, id, default)
            }
        };

        // 1. Inventário de monitores (mesmo indexador da UI).
        log!("-- Inventario --");
        let window = self.window.clone();
        let (mut win_w, mut win_h) = (0, 0);
        let mut monitors: Vec<ObsMonitor>;
        if let Some(w) = &window {
            // Gravação de janela: nada de monitor/webcam, o canvas é a janela.
            monitors = Vec::new();
            win_w = w.width;
            win_h = w.height;
            if win_w < 16 || win_h < 16 {
                // A janela sumiu entre a escolha e o clique (ou está minimizada).
                // O plugin a reencontra pela string se voltar; o canvas só
                // precisa de um tamanho razoável.
                win_w = 1920;
                win_h = 1080;
                log!("   Janela sem tamanho legivel — canvas {win_w}x{win_h}.");
            }
            log!("   Janela \"{}\" ({win_w}x{win_h}) spec={}", w.title, w.spec);
        } else {
            monitors = inputs.monitors.clone();
            log!("   {} monitor(es) detectado(s).", monitors.len());
            monitors = filter_enabled_monitors(&monitors, auto_profile);
            log!("   {} monitor(es) habilitado(s).", monitors.len());
        }

        monitors.sort_by_key(|m| m.position_x);

        // Bounding compacto (monitores + webcams habilitadas).
        let mut bounding_w: i32 = monitors.iter().map(|m| m.width).sum();
        let mut bounding_h: i32 = monitors.iter().map(|m| m.height).max().unwrap_or(0);
        let cams: Vec<Webcam> = if window.is_some() {
            bounding_w = win_w;
            bounding_h = win_h;
            Vec::new()
        } else {
            inputs.webcams.clone()
        };
        for c in &cams {
            if src_active("webcams", &c.name, false) {
                bounding_w += c.width;
                bounding_h = bounding_h.max(c.height);
            }
        }

        // Fallback áudio-only: canvas preto 800x600 (pegadinha #8).
        if bounding_w == 0 || bounding_h == 0 {
            bounding_w = 800;
            bounding_h = 600;
            log!("   Sem monitor/webcam — canvas preto 800x600 (gravacao so-audio).");
        } else {
            log!("   Bounding compacto: {bounding_w}x{bounding_h}");
        }
        let raw_bounding_w = bounding_w;

        // Clamp pelo encoder que SERÁ usado (pegadinha #7).
        let max_dim = encoder::encoder_max_dimension();
        if bounding_w > max_dim || bounding_h > max_dim {
            let scale = (max_dim as f64 / bounding_w as f64).min(max_dim as f64 / bounding_h as f64);
            bounding_w = round(bounding_w as f64 * scale);
            bounding_h = round(bounding_h as f64 * scale);
            if bounding_w % 2 != 0 {
                bounding_w -= 1;
            }
            if bounding_h % 2 != 0 {
                bounding_h -= 1;
            }
            log!("   Bounding clamped: {bounding_w}x{bounding_h} (limite {max_dim}, scale={scale:.3}).");
        }
        let mut canvas_w = bounding_w;
        let mut canvas_h = bounding_h;

        // NV12 (4:2:0) exige largura E altura PARES — também no caminho sem clamp.
        if canvas_w % 2 != 0 {
            canvas_w -= 1;
        }
        if canvas_h % 2 != 0 {
            canvas_h -= 1;
        }
        canvas_w = canvas_w.max(2);
        canvas_h = canvas_h.max(2);
        // Janela tem tamanho arbitrário: encoder de hardware codifica em blocos
        // de 16 e a AMD deixava o excesso como coluna VERDE. Múltiplo de 16 pra
        // baixo; o SCALE_INNER da fonte põe no máximo 15 px de tarja (#66).
        if window.is_some() && canvas_w >= 32 && canvas_h >= 32 {
            canvas_w &= !15;
            canvas_h &= !15;
        }

        // 2. Vídeo (canvas). `obs_reset_video` pode rodar entre gravações, só
        // não durante uma saída ativa.
        let mut fps = config::get_config_int("recordingFps", 30);
        // 0 = não configurado → 30. Clamp nos dois extremos (fonte única).
        if fps < 10 {
            fps = 30;
        } else if fps > 1000 {
            fps = 1000;
        }
        log!("-- Configurando video {canvas_w}x{canvas_h} @ {fps} fps --");
        let graphics = cstring("libobs-d3d11");
        let mut ovi = video_info(&graphics, fps as u32, canvas_w as u32, canvas_h as u32);
        let rc = unsafe { (a.obs_reset_video)(&mut ovi) };
        if rc != OBS_VIDEO_SUCCESS {
            return Err(format!("obs_reset_video {canvas_w}x{canvas_h} falhou (code={rc})."));
        }

        self.current_layout = RecordingLayout { canvas_w, canvas_h, regions: Vec::new() };
        let scale = if raw_bounding_w > 0 { canvas_w as f64 / raw_bounding_w as f64 } else { 1.0 };

        // 3. Cena.
        log!("-- Cena \"{SCENE_NAME}\" --");
        let scene_name = cstring(SCENE_NAME);
        self.scene = unsafe { (a.obs_scene_create)(scene_name.as_ptr()) };
        if self.scene.is_null() {
            return Err("obs_scene_create falhou.".into());
        }
        unsafe { (a.obs_set_output_source)(0, (a.obs_scene_get_source)(self.scene)) };

        // 4. Monitores.
        log!("-- Capturas de monitor --");
        let mut pos_x = 0.0f64;
        for m in &monitors {
            let source_name = format!("NoOBS Monitor {}", m.index);
            let mon_id = resolve_monitor_id(m);

            let settings = ObsData::create()?;
            settings.set_int("monitor", m.index.into());
            if !mon_id.is_empty() {
                settings.set_string("monitor_id", &mon_id);
            }
            let src = self.create_source("monitor_capture", &source_name, settings)?;
            unsafe {
                let item = (a.obs_scene_add)(self.scene, src);
                (a.obs_sceneitem_set_pos)(item, &Vec2 { x: pos_x as f32, y: 0.0 });
                (a.obs_sceneitem_set_scale)(item, &Vec2 { x: scale as f32, y: scale as f32 });
            }
            log!("   {source_name} -> canvas ({pos_x:.0}, 0) scale={scale:.3} monitor_id={mon_id}");

            // Região no layout (o player usa pro seletor de monitor).
            let name = if m.name.is_empty() { format!("Monitor {}", m.index) } else { m.name.clone() };
            self.current_layout.regions.push(RecordingRegion {
                name,
                kind: "monitor".into(),
                x: round(pos_x),
                y: 0,
                w: round(m.width as f64 * scale),
                h: round(m.height as f64 * scale),
            });
            pos_x += m.width as f64 * scale;
        }

        // 4b. Janela. Bounds = canvas inteiro com SCALE_INNER: se o usuário
        // redimensionar no meio, a imagem encolhe mantendo a proporção.
        let mut win_src: ObsSource = std::ptr::null_mut();
        let mut app_audio = false;
        let mut app_track = 0;
        if let Some(w) = &window {
            log!("-- Captura de janela --");
            let settings = ObsData::create()?;
            settings.set_string("window", &w.spec);
            // PRIORITY_EXE: casa pelo executável. O default (CLASS) vira "título
            // exato" em classes genéricas (Chrome, Electron).
            settings.set_int("priority", 2);
            // WGC (2) FORÇADO: o "auto" cai em BitBlt fora de uma lista de
            // classes, e o BitBlt só vê GDI (cabeçalho do Explorer saía preto).
            settings.set_int("method", 2);
            settings.set_bool("cursor", true);
            settings.set_bool("client_area", true);
            // Som só do app: o plugin cria uma wasapi_process_output_capture
            // filha e REROTEIA o áudio pra esta fonte — a faixa é decidida
            // pelos mixers DESTA fonte, mais abaixo.
            app_audio = w.audio && self.app_audio_available();
            if w.audio && !app_audio {
                log!("   Som do app pedido, mas o tipo wasapi_process_output_capture nao existe (Windows anterior ao 10 2004?) — so a imagem.");
            }
            settings.set_bool("capture_audio", app_audio);
            let src = self.create_source("window_capture", "NoOBS Window", settings)?;
            win_src = src;
            unsafe {
                let item = (a.obs_scene_add)(self.scene, src);
                (a.obs_sceneitem_set_pos)(item, &Vec2 { x: 0.0, y: 0.0 });
                (a.obs_sceneitem_set_bounds_type)(item, OBS_BOUNDS_SCALE_INNER);
                (a.obs_sceneitem_set_bounds)(item, &Vec2 { x: canvas_w as f32, y: canvas_h as f32 });
            }
            self.current_layout.regions = vec![RecordingRegion {
                name: w.title.clone(),
                kind: "window".into(),
                x: 0,
                y: 0,
                w: canvas_w,
                h: canvas_h,
            }];
        }

        // 5. Webcams habilitadas.
        log!("-- Webcams --");
        for c in &cams {
            if !src_active("webcams", &c.name, false) {
                continue;
            }
            let source_name = format!("NoOBS Webcam - {}", c.name);
            let res = format!("{}x{}", c.width, c.height);
            let settings = ObsData::create()?;
            settings.set_string("video_device_id", &c.device_id);
            settings.set_string("last_video_device_id", &c.device_id);
            settings.set_int("res_type", 1);
            settings.set_string("resolution", &res);
            settings.set_string("last_resolution", &res);
            settings.set_int("video_format", 400); // MJPEG
            settings.set_int("frame_interval", 333333); // 30 fps
            settings.set_bool("active", true);
            settings.set_int("audio_output_mode", 2); // none

            let src = self.create_source("dshow_input", &source_name, settings)?;
            let (bw, bh) = (c.width as f64 * scale, c.height as f64 * scale);
            unsafe {
                let item = (a.obs_scene_add)(self.scene, src);
                (a.obs_sceneitem_set_pos)(item, &Vec2 { x: pos_x as f32, y: 0.0 });
                // Bounds stretch: preenche o espaço reservado.
                (a.obs_sceneitem_set_bounds_type)(item, OBS_BOUNDS_STRETCH);
                (a.obs_sceneitem_set_bounds)(item, &Vec2 { x: bw as f32, y: bh as f32 });
            }
            log!("   {source_name} -> canvas ({pos_x:.0}, 0) bounds={}x{}", round(bw), round(bh));

            self.current_layout.regions.push(RecordingRegion {
                name: format!("Webcam — {}", c.name),
                kind: "webcam".into(),
                x: round(pos_x),
                y: 0,
                w: round(bw),
                h: round(bh),
            });
            pos_x += c.width as f64 * scale;
        }

        // 6. Áudio: enumera dispositivos via obs_properties. Se WASAPI/libobs
        // falhar, a gravação continua só com o mix vazio (silêncio).
        log!("-- Audio --");
        let mics = enumerate_obs_audio_devices("wasapi_input_capture");
        let outputs = enumerate_obs_audio_devices("wasapi_output_capture");
        log!("   {} mic(s), {} output(s)", mics.len(), outputs.len());

        // Reordena: default primeiro, depois os outros (ordem original). Mesma
        // lógica do BuildAudioJsonWithTracks do Bridge — engine e UI sincronizados.
        let reorder = |list: Vec<ObsAudioDev>, default_id: &str| -> Vec<ObsAudioDev> {
            let is_def = |d: &ObsAudioDev| !default_id.is_empty() && same_text(&d.device_id, default_id);
            let (mut first, rest): (Vec<_>, Vec<_>) = list.into_iter().partition(|d| is_def(d));
            first.extend(rest);
            first
        };
        let mics = reorder(mics, &inputs.default_mic_id);
        let outputs = reorder(outputs, &inputs.default_spk_id);

        let mic_enabled: Vec<bool> = mics.iter().map(|m| src_active("mics", &m.name, true)).collect();
        let mic_default: Vec<bool> = mics
            .iter()
            .map(|m| !inputs.default_mic_id.is_empty() && same_text(&m.device_id, &inputs.default_mic_id))
            .collect();
        let out_enabled: Vec<bool> = outputs.iter().map(|m| src_active("speakers", &m.name, true)).collect();
        let out_default: Vec<bool> = outputs
            .iter()
            .map(|m| !inputs.default_spk_id.is_empty() && same_text(&m.device_id, &inputs.default_spk_id))
            .collect();

        // Som do app: reserva UMA faixa isolada e ela fica logo depois da
        // última dos devices.
        let mut asg = compute_audio_track_assignments(&mic_enabled, &mic_default, &out_enabled, &out_default, app_audio as i32);
        if app_audio {
            app_track = asg.total_tracks + 1;
            asg.total_tracks = app_track;
        }
        let total_tracks = asg.total_tracks;
        log!(
            "   habilitados: {} mic(s), {} output(s)",
            count_true(&mic_enabled),
            count_true(&out_enabled)
        );

        // Canal 0 já é a cena (vídeo). Canais 1+ recebem as fontes de áudio:
        // o jeito canônico do OBS (fontes soltas atribuídas a canais são
        // mixadas na saída mesmo fora da cena). MAX_CHANNELS = 64.
        let mut channel: u32 = 1;

        log!("-- Microfones --");
        for (j, mic) in mics.iter().enumerate() {
            let audio_name = format!("{MANAGED_PREFIX}Mic - {}", mic.name);
            let settings = ObsData::create()?;
            settings.set_string("device_id", &mic.device_id);
            let src = self.create_source("wasapi_input_capture", &audio_name, settings)?;
            // Filtros da aba Áudio, só em microfone (no som do sistema
            // estragariam a música). Ordem da cadeia = ordem de inserção (#63).
            if let Some(hook) = MIC_FILTER_HOOK.get() {
                hook(src);
            }

            // Bitmask: bit 0 = Mix (faixa 1) + o bit da faixa isolada, se houver.
            let track = asg.mic_tracks[j];
            let bitmask: u32 = if track > 0 { 1 | (1u32 << (track - 1)) } else { 1 };
            let enabled = src_active("mics", &mic.name, true);
            unsafe {
                (a.obs_source_set_audio_mixers)(src, bitmask);
                (a.obs_source_set_muted)(src, !enabled);
                (a.obs_set_output_source)(channel, src);
            }
            channel += 1;
            log!("   {audio_name} -> tracks 1,{track} muted={}", !enabled);
        }
        if mics.is_empty() {
            log!("   (nenhum mic detectado)");
        }

        log!("-- Saidas de audio --");
        for (j, out) in outputs.iter().enumerate() {
            let audio_name = format!("{MANAGED_PREFIX}Out - {}", out.name);
            let settings = ObsData::create()?;
            settings.set_string("device_id", &out.device_id);
            let src = self.create_source("wasapi_output_capture", &audio_name, settings)?;

            let track = asg.out_tracks[j];
            let bitmask: u32 = if track > 0 { 1 | (1u32 << (track - 1)) } else { 1 };
            let enabled = src_active("speakers", &out.name, true);
            unsafe {
                (a.obs_source_set_audio_mixers)(src, bitmask);
                (a.obs_source_set_muted)(src, !enabled);
                (a.obs_set_output_source)(channel, src);
            }
            channel += 1;
            log!("   {audio_name} -> tracks 1,{track} muted={} ch={}", !enabled, channel - 1);
        }
        if outputs.is_empty() {
            log!("   (nenhuma saida detectada)");
        }

        // Fonte da janela: com o som do app, faixa própria; sem ele, nada. O Mix
        // só recebe o som do app quando NENHUMA saída está sendo gravada — o
        // loopback do alto-falante já o contém e somar dobraria o app (#66).
        if !win_src.is_null() {
            unsafe {
                if app_audio {
                    let mut bitmask = 1u32 << (app_track - 1);
                    if count_true(&out_enabled) == 0 {
                        bitmask |= 1;
                    }
                    (a.obs_source_set_audio_mixers)(win_src, bitmask);
                    log!("-- Som do app --");
                    let app = window.as_ref().map(|w| w.app.clone()).unwrap_or_default();
                    log!("   {app} -> track {app_track}, mix={}", bitmask & 1 != 0);
                } else {
                    (a.obs_source_set_audio_mixers)(win_src, 0);
                }
            }
        }

        // 7. Encoder de vídeo.
        log!("-- Encoder --");
        self.video_encoder = encoder::select_video_encoder()?;
        unsafe { (a.obs_encoder_set_video)(self.video_encoder, (a.obs_get_video)()) };
        // COMO a gravação foi feita (vira meta do arquivo): lê do encoder criado,
        // não do config — o fallback pode ter escolhido outro codec.
        self.video_encoder_id = unsafe { cstr_to_string((a.obs_encoder_get_id)(self.video_encoder)) };

        // Encoders de áudio: um por faixa. O "name" vira o `title` da stream no
        // MKV (visível no painel de info e em editores externos).
        let audio_kbps = encoder::audio_bitrate_kbps();
        log!("-- Audio encoders ({total_tracks} tracks, {audio_kbps} kbps) --");
        let mut names = build_track_names(total_tracks, &mics, &outputs, &asg.mic_tracks, &asg.out_tracks);
        if app_audio {
            let app = window.as_ref().map(|w| w.app.clone()).unwrap_or_default();
            names[(app_track - 1) as usize] = lang::t_with("tracks.appAudio", &[("app", &app)]);
        }
        for (i, name) in names.iter().enumerate() {
            let enc_settings = ObsData::create()?;
            enc_settings.set_int("bitrate", audio_kbps.into());
            let (id_c, name_c) = (cstring("ffmpeg_aac"), cstring(name));
            let enc = unsafe {
                (a.obs_audio_encoder_create)(id_c.as_ptr(), name_c.as_ptr(), enc_settings.raw(), i, std::ptr::null_mut())
            };
            if enc.is_null() {
                return Err(format!("obs_audio_encoder_create falhou (track {}).", i + 1));
            }
            self.audio_encoders.push(enc);
            unsafe { (a.obs_encoder_set_audio)(enc, (a.obs_get_audio)()) };
            log!("   Track {}: {name}", i + 1);
        }
        Ok(())
    }

    // ---- gravação em arquivo ----------------------------------------------

    pub fn build_and_start_recording(&mut self, req: &RecordRequest, inputs: &CaptureInputs) -> Result<(), String> {
        if self.recording || self.replay_active {
            return Err("Ja esta gravando.".into());
        }
        self.release_recording_objects();
        self.window = req.window.clone().filter(|w| !w.spec.is_empty()).map(|mut w| {
            if w.app.is_empty() {
                w.app = w.title.clone();
            }
            w
        });
        let r = self.start_recording_inner(req, inputs);
        if r.is_err() {
            // Qualquer falha no meio deixaria cena/fontes/encoders meio-criados
            // e canais ligados a sources que serão destruídas.
            self.release_recording_objects();
        }
        r
    }

    fn start_recording_inner(&mut self, req: &RecordRequest, inputs: &CaptureInputs) -> Result<(), String> {
        let a = api()?;
        self.build_capture_graph(req.auto_profile, inputs)?;

        // 8. Saída (ffmpeg_muxer = gravação em arquivo).
        log!("-- Output --");
        let out = self.create_muxer_output(&req.output_path)?;
        self.output = out;
        unsafe {
            (a.obs_output_set_video_encoder)(self.output, self.video_encoder);
            for (i, e) in self.audio_encoders.iter().enumerate() {
                (a.obs_output_set_audio_encoder)(self.output, *e, i);
            }
        }

        // 9. Iniciar.
        log!("-- StartRecording -> {} --", req.output_path);
        if !unsafe { (a.obs_output_start)(self.output) } {
            let err = unsafe { cstr_to_string((a.obs_output_get_last_error)(self.output)) };
            return Err(format!("obs_output_start falhou: {err}"));
        }

        self.output_path = req.output_path.clone();
        self.recording = true;
        self.stopping = false;
        // Conecta ao sinal "stop": é por ele que sabemos, sem poll nem Sleep,
        // que a gravação terminou de verdade (pegadinha #41).
        self.connect_stop_signal();
        log!("Gravacao iniciada.");
        Ok(())
    }

    fn create_muxer_output(&self, path: &str) -> Result<ObsOutput, String> {
        let a = api()?;
        let s = ObsData::create()?;
        s.set_string("path", path);
        s.set_string("muxer_settings", "");
        let (id, name) = (cstring("ffmpeg_muxer"), cstring("NoOBS Recording"));
        let out = unsafe { (a.obs_output_create)(id.as_ptr(), name.as_ptr(), s.raw(), std::ptr::null_mut()) };
        if out.is_null() {
            return Err("obs_output_create falhou.".into());
        }
        Ok(out)
    }

    // ---- buffer em memória ---------------------------------------------------

    /// Cria E inicia uma saída `replay_buffer` sobre os encoders já montados.
    /// Cada saída recebe número próprio no nome do arquivo: duas podem
    /// coexistir (a que grava o trecho e a nova).
    fn create_replay_output(&mut self) -> Result<ObsOutput, String> {
        let a = api()?;
        self.replay_seq += 1;
        let s = ObsData::create()?;
        s.set_string("directory", &self.replay_dir);
        // Códigos de data do os_generate_formatted_filename; o Bridge renomeia
        // pelo modelo do usuário ao mover para a pasta de gravação.
        s.set_string("format", &format!("NoOBS-buffer-{} %CCYY-%MM-%DD %hh-%mm-%ss", self.replay_seq));
        s.set_string("extension", "mkv");
        s.set_bool("allow_spaces", true);
        s.set_int("max_time_sec", self.replay_max_sec.into());
        s.set_int("max_size_mb", self.replay_max_mb.into());
        s.set_string("muxer_settings", "");
        let (id, name) = (cstring("replay_buffer"), cstring(&format!("NoOBS Buffer {}", self.replay_seq)));
        let out = unsafe { (a.obs_output_create)(id.as_ptr(), name.as_ptr(), s.raw(), std::ptr::null_mut()) };
        if out.is_null() {
            return Err("obs_output_create(replay_buffer) falhou.".into());
        }
        unsafe {
            (a.obs_output_set_video_encoder)(out, self.video_encoder);
            for (i, e) in self.audio_encoders.iter().enumerate() {
                (a.obs_output_set_audio_encoder)(out, *e, i);
            }
            if !(a.obs_output_start)(out) {
                let err = cstr_to_string((a.obs_output_get_last_error)(out));
                (a.obs_output_release)(out);
                return Err(format!("obs_output_start(replay_buffer) falhou: {err}"));
            }
        }
        log!(
            "Buffer: saida {} iniciada ({}s / {} MB em {}).",
            self.replay_seq,
            self.replay_max_sec,
            self.replay_max_mb,
            self.replay_dir
        );
        Ok(out)
    }

    /// Monta a mesma cena da gravação manual com uma saída `replay_buffer` que
    /// guarda os últimos `max_sec` segundos (ou `max_mb` MB, o que estourar
    /// primeiro) na RAM. Exclusivo com a gravação manual.
    pub fn build_and_start_replay(
        &mut self,
        dir: &str,
        max_sec: i32,
        max_mb: i32,
        inputs: &CaptureInputs,
    ) -> Result<(), String> {
        if self.recording || self.replay_active {
            return Err("Ja esta gravando.".into());
        }
        self.release_recording_objects();
        // O buffer grava sempre as telas, nunca a janela da última gravação.
        self.window = None;
        self.replay_dir = dir.to_owned();
        self.replay_max_sec = max_sec;
        self.replay_max_mb = max_mb;

        // Keyframe curto SÓ no buffer: decide quanto os dois trechos salvos se
        // sobrepõem na emenda.
        encoder::set_keyframe_sec_override(REPLAY_KEYFRAME_SEC);
        let graph = self.build_capture_graph(false, inputs);
        encoder::set_keyframe_sec_override(0);
        let r = graph.and_then(|()| {
            self.output = self.create_replay_output()?;
            self.replay_active = true;
            log!("Buffer em memoria iniciado.");
            Ok(())
        });
        if r.is_err() {
            self.release_recording_objects();
        }
        r
    }

    /// Começa uma gravação em arquivo SEM remontar a captura: pendura uma saída
    /// `ffmpeg_muxer` nos MESMOS encoders do buffer e manda o buffer salvar o
    /// que tem. O "save" é adiado (igual ao `save_replay`) para o trecho do
    /// buffer terminar DEPOIS do 1º keyframe da gravação — sobreposição, nunca
    /// buraco. `false` (sem mexer em nada) se não der: o Bridge cai no caminho
    /// normal, que descarta o buffer.
    pub fn start_recording_from_replay(&mut self, output_path: &str) -> bool {
        if self.recording || !self.replay_active || self.output.is_null() {
            return false;
        }
        let Ok(a) = api() else { return false };
        // Um trecho avulso ainda sendo gravado ocupa o `replay_saving` e o sinal
        // "saved" — não cabem os dois ao mesmo tempo.
        if self.replay_save_since.is_some() {
            log!("Buffer: trecho anterior ainda sendo gravado — gravacao nao continua o buffer.");
            return false;
        }
        let rec_out = match self.create_muxer_output(output_path) {
            Ok(o) => o,
            Err(_) => {
                log!("Buffer: obs_output_create(ffmpeg_muxer) falhou — gravacao nao continua o buffer.");
                return false;
            }
        };
        unsafe {
            (a.obs_output_set_video_encoder)(rec_out, self.video_encoder);
            for (i, e) in self.audio_encoders.iter().enumerate() {
                (a.obs_output_set_audio_encoder)(rec_out, *e, i);
            }
        }
        log!("-- StartRecording (continuando o buffer) -> {output_path} --");
        if !unsafe { (a.obs_output_start)(rec_out) } {
            let err = unsafe { cstr_to_string((a.obs_output_get_last_error)(rec_out)) };
            unsafe { (a.obs_output_release)(rec_out) };
            log!("Buffer: obs_output_start da gravacao falhou ({err}) — caminho normal.");
            return false;
        }

        // A saída do buffer vira a "que está salvando": mesmo trilho do
        // `save_replay`, só que o "saved" dela é o começo desta gravação.
        self.replay_saving = self.output;
        self.output = rec_out;
        self.connect_saved_signal();
        let now = Instant::now();
        self.replay_save_since = Some(now);
        self.replay_save_commit_pending = true;
        self.prefix_mode = true;
        self.prefix_await_stop = false;
        self.rec_start = Some(now);
        self.prefix_lead_ms = 0;
        self.replay_active = false;

        self.output_path = output_path.to_owned();
        self.recording = true;
        self.stopping = false;
        self.connect_stop_signal();
        log!(
            "Gravacao iniciada continuando o buffer (o trecho guardado fecha em {} ms).",
            self.replay_save_commit_delay_ms()
        );
        true
    }

    fn connect_saved_signal(&mut self) {
        let Ok(a) = api() else { return };
        self.replay_saved_handler = unsafe { (a.obs_output_get_signal_handler)(self.replay_saving) };
        if !self.replay_saved_handler.is_null() {
            let sig = cstring(SIG_SAVED);
            unsafe { (a.signal_handler_connect)(self.replay_saved_handler, sig.as_ptr(), replay_saved_thunk, std::ptr::null_mut()) };
        }
    }

    /// O trecho do começo da gravação se resolveu (salvo ou perdido). Avisa o
    /// Bridge e, se a gravação já tinha parado esperando por ele, conclui o stop.
    fn end_prefix(&mut self, path: &str) {
        if !self.prefix_mode {
            return;
        }
        self.prefix_mode = false;
        self.events.push(Event::ReplayPrefixSaved(path.to_owned(), self.prefix_lead_ms));
        if self.prefix_await_stop {
            self.prefix_await_stop = false;
            self.finalize_stop(self.prefix_stop_invoke_cb, false);
        }
    }

    /// "Salvou, esvazia": o replay_buffer do OBS NÃO limpa o buffer ao salvar e
    /// não há procedimento pra esvaziar. Então troca de saída: uma NOVA começa a
    /// guardar agora e a ANTIGA grava o que tinha e é descartada no "saved".
    ///
    /// A emenda SOBREPÕE, nunca perde: a saída nova só abre no 1º keyframe K
    /// (`obs-output.c:2237`) e o "save" corta no instante do pedido
    /// (`obs-ffmpeg-mux.c:1236`). Pedindo os dois juntos, K cairia DEPOIS do
    /// corte e o miolo sumiria. Por isso a rotação e o save são separados: o
    /// `commit_replay_save` fecha o trecho `replay_save_commit_delay_ms` depois.
    pub fn save_replay(&mut self) -> bool {
        if !self.replay_active || self.output.is_null() {
            return false;
        }
        // Pelo relógio do save, não por `replay_saving`: no plano B (sem saída
        // nova) `replay_saving` fica nulo com um trecho ainda em gravação.
        if self.replay_save_since.is_some() {
            log!("Buffer: save ignorado — o trecho anterior ainda esta sendo gravado.");
            return false;
        }
        let new_out = match self.create_replay_output() {
            Ok(o) => Some(o),
            Err(e) => {
                log!("Buffer: nao consegui abrir a saida nova ({e}) — salvando sem esvaziar.");
                None
            }
        };
        // Sem saída nova o buffer não esvazia, mas o trecho ainda sai.
        self.replay_saving = self.output;
        if let Some(n) = new_out {
            self.output = n;
        }
        self.connect_saved_signal();
        self.replay_save_since = Some(Instant::now());
        // Sem saída nova, `replay_saving` é a própria `output`: o
        // `drop_saving_replay` do "saved" não pode derrubá-la. O sinal continua
        // conectado nela e o `on_replay_saved_signal` lê o caminho da `output`.
        if new_out.is_none() {
            self.replay_saving = std::ptr::null_mut();
        }
        self.replay_save_commit_pending = true;
        if new_out.is_none() {
            // Sem rotação não há emenda pra proteger — fecha o trecho agora.
            self.commit_replay_save();
        } else {
            log!(
                "Buffer: saida nova no ar; o trecho fecha em {} ms (sobreposicao).",
                self.replay_save_commit_delay_ms()
            );
        }
        true
    }

    /// Fecha o trecho: pede o "save" à saída ANTIGA, que grava tudo que guardou
    /// até agora. Chamado pelo `TIMER_REPLAY_SAVE_COMMIT` (ou na hora, no plano
    /// B). A partir daqui é o sinal "saved" que manda.
    pub fn commit_replay_save(&mut self) {
        if !self.replay_save_commit_pending {
            return;
        }
        self.replay_save_commit_pending = false;
        // Quanto a gravação já andou quando o trecho fechou: a emenda usa isso
        // para saber onde, no fim do trecho, a gravação começou.
        if self.prefix_mode {
            self.prefix_lead_ms = self.rec_start.map(|t| t.elapsed().as_millis() as i32).unwrap_or(0);
        }
        // No plano B quem guarda o trecho é a própria saída corrente.
        let target = if self.replay_saving.is_null() { self.output } else { self.replay_saving };
        if target.is_null() {
            return;
        }
        let Ok(a) = api() else { return };
        let mut cd = CallData::zeroed();
        let name = cstring("save");
        unsafe {
            (a.proc_handler_call)((a.obs_output_get_proc_handler)(target), name.as_ptr(), &mut cd as *mut _ as CalldataPtr);
            if !cd.stack.is_null() && !cd.fixed {
                (a.bfree)(cd.stack);
            }
        }
        log!("Buffer: gravando trecho em disco.");
    }

    /// Desliga e libera a saída que acabou de gravar um trecho (ou que
    /// desistimos de esperar). Idempotente. `obs_output_release` se
    /// auto-sincroniza (espera a saída parar): liberar logo após o stop é
    /// seguro (pegadinha #41).
    fn drop_saving_replay(&mut self) {
        let Ok(a) = api() else { return };
        if !self.replay_saved_handler.is_null() {
            let sig = cstring(SIG_SAVED);
            unsafe { (a.signal_handler_disconnect)(self.replay_saved_handler, sig.as_ptr(), replay_saved_thunk, std::ptr::null_mut()) };
            self.replay_saved_handler = std::ptr::null_mut();
        }
        if self.replay_saving.is_null() {
            return;
        }
        unsafe {
            (a.obs_output_stop)(self.replay_saving);
            (a.obs_output_release)(self.replay_saving);
        }
        self.replay_saving = std::ptr::null_mut();
    }

    /// Main thread. O trecho está completo no disco: pega o caminho pelo
    /// `get_last_replay`, derruba a saída antiga e entrega o arquivo.
    fn on_replay_saved_signal(&mut self) {
        if self.shutting_down {
            return;
        }
        // Sem saída nova (plano B), quem salvou foi a própria `output`.
        let src = if self.replay_saving.is_null() { self.output } else { self.replay_saving };
        if src.is_null() {
            return;
        }
        let Ok(a) = api() else { return };
        let mut path = String::new();
        let mut cd = CallData::zeroed();
        let mut p: *const c_char = std::ptr::null();
        let (proc_name, key) = (cstring("get_last_replay"), cstring("path"));
        unsafe {
            let cdp = &mut cd as *mut _ as CalldataPtr;
            if (a.proc_handler_call)((a.obs_output_get_proc_handler)(src), proc_name.as_ptr(), cdp)
                && (a.calldata_get_string)(cdp, key.as_ptr(), &mut p)
                && !p.is_null()
            {
                path = cstr_to_string(p);
            }
            if !cd.stack.is_null() && !cd.fixed {
                (a.bfree)(cd.stack);
            }
        }
        // A libobs devolve com '/' (dstr_replace no generate_filename).
        let path = path.replace('/', "\\");

        // Desconecta o "saved" e derruba a saída antiga (no plano B só
        // desconecta, a `output` segue guardando).
        self.drop_saving_replay();
        self.replay_save_commit_pending = false;
        log!("Buffer: trecho gravado em {} ms: {path}", self.replay_save_elapsed_ms());
        self.replay_save_since = None;
        if self.prefix_mode {
            self.end_prefix(&path);
            return;
        }
        self.events.push(Event::ReplaySaved(path));
    }

    /// Desiste de um trecho cujo "saved" nunca chegou (timeout do Bridge).
    pub fn abort_replay_save(&mut self) {
        if self.replay_save_since.is_none() {
            return;
        }
        // Rotação sem "save": o trecho nunca foi pedido, só a saída velha some.
        self.replay_save_commit_pending = false;
        log!(
            "Buffer: \"saved\" nao chegou em {} ms — desistindo do trecho.",
            self.replay_save_elapsed_ms()
        );
        self.drop_saving_replay();
        self.replay_save_since = None;
        // Era o começo de uma gravação: ela segue, só sem o que o buffer tinha.
        self.end_prefix("");
    }

    pub fn stop_replay(&mut self) {
        if !self.replay_active {
            return;
        }
        self.replay_active = false;
        self.replay_save_since = None;
        if self.replay_save_commit_pending {
            // Parou dentro da janela de sobreposição: o trecho pedido nunca foi
            // fechado. Não há o que salvar — mas fica dito.
            log!("Buffer: parado antes de fechar o trecho pedido — trecho perdido.");
        }
        self.replay_save_commit_pending = false;
        self.release_recording_objects();
        log!("Buffer em memoria parado.");
    }

    /// Limites novos: a `replay_buffer` só lê max_time/max_size no start, então
    /// valem a partir da PRÓXIMA saída (próximo "salvar trecho" ou religar).
    pub fn set_replay_limits(&mut self, max_sec: i32, max_mb: i32) {
        self.replay_max_sec = max_sec;
        self.replay_max_mb = max_mb;
    }

    // ---- stop assíncrono (pegadinha #41) --------------------------------------

    fn connect_stop_signal(&mut self) {
        if self.output.is_null() {
            return;
        }
        let Ok(a) = api() else { return };
        self.stop_signal_handler = unsafe { (a.obs_output_get_signal_handler)(self.output) };
        if !self.stop_signal_handler.is_null() {
            let sig = cstring(SIG_STOP);
            unsafe { (a.signal_handler_connect)(self.stop_signal_handler, sig.as_ptr(), stop_signal_thunk, std::ptr::null_mut()) };
        }
    }

    /// Desconecta ANTES do release da saída (o handler vive dentro dela).
    fn disconnect_stop_signal(&mut self) {
        if self.stop_signal_handler.is_null() {
            return;
        }
        if let Ok(a) = api() {
            let sig = cstring(SIG_STOP);
            unsafe { (a.signal_handler_disconnect)(self.stop_signal_handler, sig.as_ptr(), stop_signal_thunk, std::ptr::null_mut()) };
        }
        self.stop_signal_handler = std::ptr::null_mut();
    }

    /// Main thread (via `mainthread::queue` do thunk). O sinal "stop" disparou =
    /// arquivo completo + threads encerradas: seguro liberar e notificar.
    fn on_stop_signal(&mut self) {
        if self.shutting_down {
            return;
        }
        self.finalize_stop(true, true);
    }

    /// Libera os objetos da gravação e (opcional) notifica `Stopped`.
    /// Idempotente via `stopping`: o sinal e o timeout do Bridge podem ambos
    /// chamar; só o primeiro age.
    fn finalize_stop(&mut self, invoke_callback: bool, wait_prefix: bool) {
        if !self.stopping {
            return;
        }
        if self.is_prefix_saving() {
            if wait_prefix {
                // A gravação acabou, mas o trecho do buffer que é o COMEÇO dela
                // ainda é escrito. Liberar tudo derrubaria a saída do buffer no
                // meio. Solta só a saída da gravação (o arquivo dela já está
                // completo) e deixa o resto pro "saved" (`end_prefix`).
                // `stopping` continua true: pro Bridge ainda está finalizando.
                if self.prefix_await_stop {
                    return;
                }
                self.prefix_await_stop = true;
                self.prefix_stop_invoke_cb = invoke_callback;
                self.disconnect_stop_signal();
                if !self.output.is_null() {
                    if let Ok(a) = api() {
                        unsafe { (a.obs_output_release)(self.output) };
                    }
                    self.output = std::ptr::null_mut();
                }
                log!("Gravacao parada; esperando o trecho do buffer terminar de ser gravado.");
                return;
            }
            // Forçado (timeout ou shutdown): desiste do trecho.
            log!("Gravacao finalizada a forca com o trecho do buffer ainda em gravacao — trecho perdido.");
            self.prefix_await_stop = false;
            self.replay_save_commit_pending = false;
            self.drop_saving_replay();
            self.replay_save_since = None;
            // `prefix_await_stop` já zerado: o end_prefix só avisa, não conclui de novo.
            self.end_prefix("");
        }
        self.prefix_await_stop = false;
        self.stopping = false;
        self.recording = false;
        let path = self.output_path.clone();
        self.disconnect_stop_signal();
        self.release_recording_objects();
        log!("Gravacao finalizada: {path}");
        if invoke_callback {
            self.events.push(Event::Stopped(path));
        }
    }

    /// Pede o stop e retorna NA HORA (não bloqueia a UI). A conclusão chega
    /// depois por `Event::Stopped`. Modelo idêntico ao `SimpleOutput::StopRecording`.
    pub fn request_stop(&mut self) {
        if !self.recording || self.stopping {
            return;
        }
        self.stopping = true;
        self.stopping_since = Some(Instant::now());
        // Parou antes de o trecho do buffer fechar (primeiro ~1 s): fecha agora,
        // senão só fecharia depois de a saída da gravação já ter parado.
        if self.prefix_mode && self.replay_save_commit_pending {
            self.commit_replay_save();
        }
        log!("Parando gravacao (assincrono)...");
        if let Ok(a) = api() {
            unsafe { (a.obs_output_stop)(self.output) };
        }
    }

    /// Chamado pelo timeout do Bridge se o sinal "stop" nunca chegou. O
    /// `obs_output_release` ainda espera/junta as threads, então é seguro.
    pub fn force_complete_stop(&mut self) {
        if !self.stopping {
            return;
        }
        log!("ForceCompleteStop: sinal \"stop\" nao chegou — finalizando a forca.");
        self.finalize_stop(true, false);
    }

    /// Caminho SÍNCRONO — só no shutdown, onde bloquear é aceitável e não há
    /// message loop para drenar a fila do sinal. Devolve o caminho do arquivo.
    pub fn stop_recording(&mut self) -> String {
        let result = self.output_path.clone();
        if !(self.recording || self.stopping) {
            return result;
        }
        let Ok(a) = api() else { return result };
        if !self.stopping {
            self.stopping = true;
            self.stopping_since = Some(Instant::now());
            log!("Parando gravacao (sincrono, shutdown)...");
            unsafe { (a.obs_output_stop)(self.output) };
        }
        // O release do finalize_stop também espera/junta; o poll evita o force-stop.
        let deadline = Instant::now() + Duration::from_secs(10);
        while unsafe { (a.obs_output_active)(self.output) } {
            if Instant::now() > deadline {
                log!("Timeout esperando output parar.");
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.finalize_stop(false, false); // sem callback: shutdown não precisa de UI
        result
    }

    // ---- teardown --------------------------------------------------------------

    /// Encerra o libobs. `obs_shutdown` NA MAIN THREAD, a mesma do
    /// `obs_startup` (contrato da libobs, pegadinha #31), com watchdog.
    pub fn teardown(&mut self) {
        // Marca shutdown: qualquer evento enfileirado que ainda venha a rodar
        // vira no-op, evitando mexer em objetos meio-liberados.
        self.shutting_down = true;
        self.disconnect_stop_signal();
        let Ok(a) = api() else { return };
        if (self.recording || self.stopping) && !self.output.is_null() {
            unsafe { (a.obs_output_stop)(self.output) };
            // obs_output_stop é ASSÍNCRONO (#41): a finalização custa segundos
            // (medido 5,3 s num canvas 2x4K + webcam). O release logo abaixo
            // se auto-sincroniza e só volta quando ela terminou de verdade.
            self.recording = false;
            self.stopping = false;
        }
        // Buffer: nada a salvar ao fechar. As duas saídas caem no release.
        self.replay_active = false;
        self.replay_save_since = None;
        self.replay_save_commit_pending = false;

        // LIBERA OS OBJETOS ANTES DO obs_shutdown — a ordem do frontend do OBS
        // ("any obs data must be released before calling obs_shutdown").
        if self.initialized {
            let t0 = Instant::now();
            self.release_recording_objects();
            // Solta as NOSSAS referências; o core segue com as sources
            // registradas — desmontá-las é o que trava o obs_shutdown.
            clear_all_obs_data();
            log!("libobs: objetos liberados em {}ms (antes do shutdown).", t0.elapsed().as_millis());
            // A partir daqui queremos TODAS as mensagens do libobs.
            VERBOSE_SHUTDOWN_LOG.store(true, Ordering::Relaxed);

            let watchdog = ShutdownWatchdog::start(SHUTDOWN_TIMEOUT);
            let t0 = Instant::now();
            unsafe { (a.obs_shutdown)() };
            log!("libobs: shutdown ok em {}ms.", t0.elapsed().as_millis());
            watchdog.cancel();

            self.output = std::ptr::null_mut();
            self.replay_saving = std::ptr::null_mut();
            self.video_encoder = std::ptr::null_mut();
            self.audio_encoders.clear();
            self.sources.clear();
            self.scene = std::ptr::null_mut();
            self.initialized = false;
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.teardown();
    }
}

/// Contadores do pipeline de vídeo (acumulados desde o último reset).
#[derive(Debug, Clone, Copy, Default)]
pub struct VideoStats {
    pub total: u32,
    /// Render atrasado.
    pub lagged: u32,
    pub v_total: u32,
    /// Encoder atrasado.
    pub skipped: u32,
    pub avg_frame_ns: u64,
    pub interval_ns: u64,
}

/// `obs_video_info` NV12/709/parcial com canvas = saída (pegadinha #18: o
/// padding de `gpu_conversion` já está no `repr(C)`).
fn video_info(graphics: &CString, fps: u32, w: u32, h: u32) -> ObsVideoInfo {
    ObsVideoInfo {
        graphics_module: graphics.as_ptr(),
        fps_num: fps,
        fps_den: 1,
        base_width: w,
        base_height: h,
        output_width: w,
        output_height: h,
        output_format: VIDEO_FORMAT_NV12,
        adapter: 0,
        gpu_conversion: true,
        _pad0: [0; 3],
        colorspace: VIDEO_CS_709,
        range: VIDEO_RANGE_PARTIAL,
        scale_type: OBS_SCALE_BICUBIC,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_do_obs() {
        assert_eq!(obs_locale_for_app("pt-BR"), "pt-BR");
        assert_eq!(obs_locale_for_app("pt"), "pt-BR");
        assert_eq!(obs_locale_for_app("xx"), "en-US");
    }

    #[test]
    fn round_do_delphi_e_bancario() {
        assert_eq!(round(2.5), 2);
        assert_eq!(round(3.5), 4);
    }
}

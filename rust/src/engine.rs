//! O motor de captura do ponto de vista do Bridge: a interface
//! ([`CaptureEngine`]), os pedidos que ela recebe e o motor vivo da sessão.
//!
//! Quem implementa é o motor próprio (`native_engine::NativeEngine`, sobre o
//! `capture::*`: D3D11 + WGC + WASAPI + libavcodec — docs/motor-proprio.md).
//! TUDO aqui roda na MAIN thread: o motor fica numa `thread_local`, e o que
//! termina em outra thread (parar, salvar trecho) volta como [`Event`],
//! entregue por [`with_engine`] DEPOIS de soltar o empréstimo — o handler pode
//! chamar o motor de novo sem reentrância.

use std::cell::RefCell;
use std::rc::Rc;

use crate::scene::CaptureMonitor;
use crate::types::{RecordingLayout, Webcam};

/// O que a máquina oferece para a captura, coletado pelo chamador (Win32).
#[derive(Debug, Clone, Default)]
pub struct CaptureInputs {
    /// Todos os monitores detectados (o engine filtra os habilitados).
    pub monitors: Vec<CaptureMonitor>,
    pub webcams: Vec<Webcam>,
    /// Endpoints padrão do Windows (id do WASAPI); vazio = desconhecido.
    pub default_mic_id: String,
    pub default_spk_id: String,
}

/// Gravação de UMA janela em vez de monitores/webcams. O tamanho só
/// dimensiona o canvas (lido na hora de gravar); a janela é reencontrada pela
/// identidade `"titulo:classe:exe"` (pegadinha #66).
#[derive(Debug, Clone, Default)]
pub struct WindowCapture {
    /// `"titulo:classe:exe"` (WinWindows).
    pub spec: String,
    pub title: String,
    /// Tamanho da área cliente (0 se a janela não tem tamanho legível).
    pub width: i32,
    pub height: i32,
    /// Som SÓ do app, numa faixa isolada própria.
    pub audio: bool,
    pub app: String,
}

/// Gravação de uma ÁREA da área de trabalho (pixels físicos, coordenadas da
/// área de trabalho virtual — origem no monitor principal, pode ser negativa).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AreaRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Default)]
pub struct RecordRequest {
    pub output_path: String,
    /// Perfil de auto-gravação (watcher de mic, pegadinha #47).
    pub auto_profile: bool,
    pub window: Option<WindowCapture>,
    /// Gravação de uma área (vence a janela; os dois nunca vêm juntos).
    pub area: Option<AreaRect>,
}

/// A parte demorada de começar uma gravação (abrir encoder e dispositivos,
/// ~200 ms), separada pra rodar fora da main: o `Box` devolvido volta pro
/// motor no `complete_recording`, já na main.
pub struct StartJob(pub Box<dyn FnOnce() -> Result<Box<dyn std::any::Any + Send>, String> + Send>);

/// Notificações para o Bridge, sempre na main thread e fora do empréstimo.
#[derive(Debug)]
pub enum Event {
    /// Gravação parou de verdade (arquivo completo).
    Stopped(String),
    /// Trecho do buffer gravado em disco ('' se falhou).
    ReplaySaved(String),
}

/// O motor de captura, do ponto de vista do Bridge (inversão de dependência:
/// o Bridge não conhece o pipeline, e os testes podem trocar o motor).
pub trait CaptureEngine {
    /// Eventos pro Bridge (fim de gravação, trecho salvo), entregues pelo `with_engine`.
    fn take_events(&mut self) -> Vec<Event>;
    /// Encoders que a máquina tem (a tela de Configurações e a exportação usam).
    fn encoder_caps(&mut self) -> crate::types::EncoderCaps;
    /// O encoder que a PRÓXIMA gravação vai usar (pro `{CODEC}` do nome do arquivo).
    fn predict_encoder_id(&mut self) -> String;

    fn is_initialized(&self) -> bool;
    fn ensure_initialized(&mut self) -> Result<(), String>;
    fn is_recording(&self) -> bool;
    fn is_stopping(&self) -> bool;
    fn stopping_elapsed_ms(&self) -> u64;
    fn output_path(&self) -> &str;
    fn current_layout(&self) -> &RecordingLayout;
    fn video_encoder_id(&self) -> &str;
    fn app_audio_available(&self) -> bool;
    fn video_stats(&self) -> Option<VideoStats>;
    fn set_source_muted(&self, source_name: &str, muted: bool);
    fn update_area(&mut self, ar: AreaRect) -> bool;
    fn build_and_start_recording(&mut self, req: &RecordRequest, inputs: &CaptureInputs) -> Result<(), String>;
    /// O mesmo, em duas metades: a montagem (`StartJob`, qualquer thread) e o
    /// registro do que ela montou (`complete_recording`, na main).
    fn begin_recording(&mut self, req: &RecordRequest, inputs: &CaptureInputs) -> Result<StartJob, String>;
    fn complete_recording(&mut self, req: &RecordRequest, started: Box<dyn std::any::Any + Send>) -> Result<(), String>;
    fn set_recording_paused(&mut self, pause: bool) -> bool;
    fn request_stop(&mut self);
    fn force_complete_stop(&mut self);
    fn stop_recording(&mut self) -> String;

    fn is_replay_active(&self) -> bool;
    fn is_replay_saving(&self) -> bool;
    fn build_and_start_replay(&mut self, dir: &str, max_sec: i32, max_mb: i32, inputs: &CaptureInputs) -> Result<(), String>;
    /// A gravação começa pelo que o buffer guardou, num arquivo só.
    fn start_recording_from_replay(&mut self, output_path: &str) -> bool;
    fn save_replay(&mut self) -> bool;
    fn abort_replay_save(&mut self);
    fn stop_replay(&mut self);
    fn set_replay_limits(&mut self, max_sec: i32, max_mb: i32);
}

#[derive(Default)]
struct Handlers {
    on_stopped: Option<Rc<dyn Fn(&str)>>,
    on_replay_saved: Option<Rc<dyn Fn(&str)>>,
}

thread_local! {
    static ENGINE: RefCell<Option<Box<dyn CaptureEngine>>> = const { RefCell::new(None) };
    static HANDLERS: RefCell<Handlers> = RefCell::new(Handlers::default());
}

pub fn set_on_stopped(f: impl Fn(&str) + 'static) {
    HANDLERS.with(|h| h.borrow_mut().on_stopped = Some(Rc::new(f)));
}

pub fn set_on_replay_saved(f: impl Fn(&str) + 'static) {
    HANDLERS.with(|h| h.borrow_mut().on_replay_saved = Some(Rc::new(f)));
}

fn deliver(events: Vec<Event>) {
    for ev in events {
        let (cb, p) = match ev {
            Event::Stopped(p) => (HANDLERS.with(|h| h.borrow().on_stopped.clone()), p),
            Event::ReplaySaved(p) => (HANDLERS.with(|h| h.borrow().on_replay_saved.clone()), p),
        };
        if let Some(cb) = cb {
            cb(&p);
        }
    }
}

/// O motor já foi criado?
pub fn exists() -> bool {
    ENGINE.with(|c| c.try_borrow().map(|b| b.is_some()).unwrap_or(true))
}

/// Cria o motor se ainda não existe (sem detectar encoders nem abrir a GPU).
pub fn ensure_created() {
    ENGINE.with(|c| {
        if let Ok(mut b) = c.try_borrow_mut() {
            if b.is_none() {
                *b = Some(Box::new(crate::native_engine::NativeEngine::new()));
            }
        }
    });
}

/// Roda `f` no motor (se existir) e depois entrega os eventos gerados, já com
/// o empréstimo solto. `None` se não existe ou se chamado de dentro de outro
/// `with_engine` (reentrância).
pub fn with_engine<R>(f: impl FnOnce(&mut dyn CaptureEngine) -> R) -> Option<R> {
    let (r, events) = ENGINE.with(|c| {
        let mut b = c.try_borrow_mut().ok()?;
        let e = b.as_mut()?;
        let r = f(e.as_mut());
        Some((r, e.take_events()))
    })?;
    deliver(events);
    Some(r)
}

/// Descarta o motor (o `Drop` dele para o que estiver rodando).
pub fn destroy() {
    let taken = ENGINE.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.take()));
    drop(taken);
}

/// Contadores do pipeline de vídeo (acumulados desde o último reset).
#[derive(Debug, Clone, Copy, Default)]
pub struct VideoStats {
    pub total: u32,
    /// Quadros que saíram atrasados da composição.
    pub lagged: u32,
    pub v_total: u32,
    /// Quadros que o encoder pulou.
    pub skipped: u32,
    pub avg_frame_ns: u64,
    pub interval_ns: u64,
}

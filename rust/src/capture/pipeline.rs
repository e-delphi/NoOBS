//! Sessão de captura: liga fontes → compositor → conversor → encoders → saída.
//! A `Session` só ORQUESTRA: não sabe capturar, compor, misturar nem
//! codificar — cada peça vem de fora por trait (`VideoSource`, `AudioSource`,
//! `VideoEncoder`, `AudioEncoder`, `PacketSink`).
//!
//! Threads:
//! - **vídeo** (a "graphics thread" do OBS, `obs-video.c`): grade de fps,
//!   compõe, converte, codifica;
//! - **áudio**: a cada bloco de 1024 amostras (com `MIX_DELAY_NS` de atraso),
//!   mistura as faixas e codifica;
//! - **gravador**: o único dono do `PacketSink`; recebe os pacotes das duas
//!   por canal (disco lento não segura a captura).
//!
//! Vídeo e áudio partem do MESMO `start_ns`: o quadro N está em
//! `start + N/fps` e o bloco K em `start + K×1024/48000`, os dois no relógio
//! QPC — é o que os mantém juntos.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

use super::audio::{AudioDevice, AudioSource, WasapiSource, CHANNELS};
use super::clock::{now_ns, FrameGrid, Sleeper};
use super::encode::{AudioEncoder, AvAacEncoder, AvVideoEncoder, StreamInfo, VideoEncoder, VideoEncoderConfig};
use super::gpu::{Compositor, Device, Nv12Converter, Rect};
use super::mixer::{block_start_ns, AudioMixer, MixInput, BLOCK_FRAMES, MIX_DELAY_NS};
use super::motion::{Mask, MotionDetector};
use super::router::{Cmd, FileDone, Msg, Router, RouterStats};
use super::layout::fit_inside;
use super::video::{VideoSource, WgcSource, WgcTarget};
use super::webcam::WebcamSource;
use crate::log;

/// O que uma fonte de vídeo captura.
#[derive(Debug, Clone)]
pub enum VideoTarget {
    /// Monitor pelo `HMONITOR` (como número).
    Monitor(isize),
    /// Janela pelo `HWND`; `client_area` corta a moldura.
    Window { hwnd: isize, client_area: bool },
    /// Webcam pelo nome amigável (vazio = a primeira).
    Webcam(String),
}

/// Uma fonte no canvas: o que capturar, que pedaço dela e onde desenhar.
#[derive(Debug, Clone)]
pub struct SourceSpec {
    pub name: String,
    pub target: VideoTarget,
    /// Recorte em pixels da ORIGEM; `None` = imagem inteira.
    pub crop: Option<Rect>,
    /// Retângulo de destino no canvas; `None` = canvas inteiro, mantendo a
    /// proporção (janela que muda de tamanho encolhe com tarja — o
    /// `OBS_BOUNDS_SCALE_INNER` do OBS).
    pub dest: Option<Rect>,
}

/// Uma fonte de áudio e as faixas em que ela entra (índice 0 = a mistura).
#[derive(Debug, Clone)]
pub struct AudioInputSpec {
    pub name: String,
    pub device: AudioDevice,
    pub tracks: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub inputs: Vec<AudioInputSpec>,
    /// Uma por faixa (o nome vai no MKV); a 1ª é a mistura.
    pub track_titles: Vec<String>,
    /// Taxa do AAC por faixa.
    pub kbps: u32,
}

/// Configuração de uma sessão de gravação.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub sources: Vec<SourceSpec>,
    pub encoder: VideoEncoderConfig,
    /// `None` = só vídeo.
    pub audio: Option<AudioConfig>,
    /// Buffer em memória: `(segundos, MB)` guardados. `None` = sem buffer.
    pub replay: Option<(u32, u32)>,
    /// Gravação que começa junto com a sessão; vazio = nenhuma (só buffer,
    /// ou gravação pedida depois com `start_recording`).
    pub output_path: String,
    /// Timelapse (gravação de área): só grava quando a imagem muda.
    pub motion: Option<MotionConfig>,
}

/// Timelapse: a área na área de trabalho (pra achar o cursor no canvas) e se
/// mexer o cursor conta como mudança.
#[derive(Debug, Clone, Copy)]
pub struct MotionConfig {
    /// `(x, y, largura, altura)` em pixels da área de trabalho.
    pub area: (i32, i32, i32, i32),
    pub count_cursor: bool,
}

/// Margem do Timelapse: quanto entra antes e depois de cada mudança.
pub const TIMELAPSE_MARGIN_NS: i64 = 1_000_000_000;

/// Números da sessão (lidos de qualquer thread).
#[derive(Default)]
pub struct Stats {
    pub frames: AtomicU64,
    pub lagged: AtomicU64,
    pub packets: AtomicU64,
    pub audio_blocks: AtomicU64,
    /// Soma do tempo de render+encode por quadro, em µs.
    pub busy_us: AtomicU64,
    /// Pior quadro de cada etapa, em µs: ler as fontes, compor+converter, codificar.
    pub worst_poll_us: AtomicU64,
    pub worst_draw_us: AtomicU64,
    pub worst_encode_us: AtomicU64,
}

/// Resultado do fim da sessão.
#[derive(Debug)]
pub struct SessionEnd {
    pub path: String,
    pub frames: u64,
    pub lagged: u64,
    pub audio_blocks: u64,
    pub error: Option<String>,
}

type PacketTx = Sender<Msg>;

/// Recorte/destino de cada fonte, trocáveis no meio (mover a área). Destino
/// com largura 0 = a fonte não é desenhada.
type Placements = Arc<Mutex<Vec<(Option<Rect>, Option<Rect>)>>>;

/// Área do Timelapse na área de trabalho; muda junto com a moldura.
type MotionArea = Arc<Mutex<(i32, i32, i32, i32)>>;

struct Control {
    stop: AtomicBool,
    /// O próximo quadro TEM que ser keyframe (corte do buffer, retomada, início de gravação).
    force_key: Arc<AtomicBool>,
    /// Instante do pedido de parada: o áudio cobre até ele.
    stop_ns: AtomicU64,
    error: Mutex<Option<String>>,
}

impl Control {
    fn fail(&self, e: String) {
        log!("Captura: {e}");
        self.error.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert(e);
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub struct Session {
    ctl: Arc<Control>,
    pub stats: Arc<Stats>,
    pub router: Arc<RouterStats>,
    tx: Option<Sender<Msg>>,
    start_ns: u64,
    placements: Placements,
    motion_area: MotionArea,
    audio_sources: Vec<Arc<dyn AudioSource>>,
    video: Option<JoinHandle<()>>,
    audio: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    path: String,
}

/// Mensagem de largada pras threads de captura.
struct Go {
    start_ns: u64,
    tx: PacketTx,
}

impl Session {
    /// Monta tudo e começa. Volta quando a captura começou; erro de montagem
    /// (fonte, encoder, arquivo) volta aqui e nada fica rodando.
    pub fn start(cfg: SessionConfig) -> Result<Self, String> {
        let ctl = Arc::new(Control { stop: AtomicBool::new(false), force_key: Arc::new(AtomicBool::new(false)), stop_ns: AtomicU64::new(u64::MAX), error: Mutex::new(None) });
        let stats = Arc::new(Stats::default());
        let placements: Placements = Arc::new(Mutex::new(cfg.sources.iter().map(|s| (s.crop, s.dest)).collect()));
        let motion_area: MotionArea = Arc::new(Mutex::new(cfg.motion.map(|m| m.area).unwrap_or_default()));

        // ---- vídeo: montado na própria thread (D3D11 + WinRT ficam nela) ----
        let (vready_tx, vready_rx) = channel::<Result<StreamInfo, String>>();
        let (vgo_tx, vgo_rx) = channel::<Option<Go>>();
        let (c2, s2, vcfg, pl2, ma2) = (ctl.clone(), stats.clone(), cfg.clone(), placements.clone(), motion_area.clone());
        let video = std::thread::Builder::new()
            .name("noobs-captura-video".into())
            .spawn(move || video_thread(vcfg, pl2, ma2, c2, s2, vready_tx, vgo_rx))
            .map_err(|e| format!("thread de vídeo: {e}"))?;
        let vinfo = match vready_rx.recv() {
            Ok(Ok(i)) => i,
            Ok(Err(e)) => {
                let _ = video.join();
                return Err(e);
            }
            Err(_) => {
                let _ = video.join();
                return Err("a thread de vídeo caiu ao montar".into());
            }
        };
        let abort_video = |video: JoinHandle<()>| {
            let _ = vgo_tx.send(None);
            let _ = video.join();
        };

        // ---- áudio: fontes e encoders montados aqui, rodados na thread de áudio ----
        let mut infos = vec![vinfo];
        let mut audio_sources: Vec<Arc<dyn AudioSource>> = Vec::new();
        let mut audio_parts = None;
        if let Some(a) = &cfg.audio {
            let mut inputs = Vec::new();
            for i in &a.inputs {
                // Dispositivo que não abre (desconectado, em uso exclusivo) não
                // derruba a gravação: ela sai sem ele, como no OBS.
                match WasapiSource::start(&i.name, i.device.clone()) {
                    Ok(s) => {
                        let s: Arc<dyn AudioSource> = Arc::new(s);
                        audio_sources.push(s.clone());
                        inputs.push(MixInput { source: s, tracks: i.tracks.clone() });
                    }
                    Err(e) => log!("Captura: fonte de audio \"{}\" fora da gravacao: {e}", i.name),
                }
            }
            let mut encoders: Vec<Box<dyn AudioEncoder>> = Vec::new();
            for title in &a.track_titles {
                let enc = match AvAacEncoder::open(a.kbps, title) {
                    Ok(e) => e,
                    Err(e) => {
                        abort_video(video);
                        return Err(e);
                    }
                };
                match enc.stream_info() {
                    Ok(i) => infos.push(i),
                    Err(e) => {
                        abort_video(video);
                        return Err(e);
                    }
                }
                encoders.push(Box::new(enc));
            }
            audio_parts = Some((AudioMixer::new(inputs, a.track_titles.len()), encoders));
        }

        // ---- gravador ----
        let router_stats = Arc::new(RouterStats::default());
        let router = Router::new(infos, cfg.replay, cfg.motion.is_some(), ctl.force_key.clone(), router_stats.clone());
        let (tx, rx) = channel::<Msg>();
        let c3 = ctl.clone();
        let writer = std::thread::Builder::new()
            .name("noobs-captura-gravador".into())
            .spawn(move || {
                if let Some(e) = router.run(rx) {
                    c3.fail(e);
                }
            })
            .map_err(|e| format!("thread do gravador: {e}"))?;
        if !cfg.output_path.is_empty() {
            let (rtx, rrx) = channel();
            let _ = tx.send(Msg::Cmd(Cmd::StartRecording { path: cfg.output_path.clone(), from_replay: false, reply: rtx }));
            if let Err(e) = rrx.recv().unwrap_or_else(|_| Err("o gravador caiu".into())) {
                drop(tx);
                abort_video(video);
                let _ = writer.join();
                return Err(e);
            }
        }

        // ---- largada: o mesmo instante pras duas ----
        let start_ns = now_ns();
        let audio = audio_parts.map(|(mixer, encoders)| {
            let (c4, s4, atx) = (ctl.clone(), stats.clone(), tx.clone());
            std::thread::Builder::new()
                .name("noobs-captura-audio".into())
                .spawn(move || audio_thread(mixer, encoders, start_ns, atx, c4, s4))
                .ok()
        });
        let _ = vgo_tx.send(Some(Go { start_ns, tx: tx.clone() }));
        log!("Captura: sessao {}x{} @{} fps, {} fonte(s) de video, {} de audio → {}", cfg.width, cfg.height, cfg.fps, cfg.sources.len(), audio_sources.len(), cfg.output_path);
        Ok(Self { ctl, stats, router: router_stats, tx: Some(tx), start_ns, placements, motion_area, audio_sources, video: Some(video), audio: audio.flatten(), writer: Some(writer), path: cfg.output_path })
    }

    fn cmd<T>(&self, make: impl FnOnce(Sender<Result<T, String>>) -> Cmd) -> Result<T, String> {
        let (rtx, rrx) = channel();
        self.tx.as_ref().ok_or("sessão encerrada")?.send(Msg::Cmd(make(rtx))).map_err(|_| "o gravador caiu".to_string())?;
        rrx.recv().unwrap_or_else(|_| Err("o gravador caiu".into()))
    }

    /// Começa a gravar em `path`. `from_replay` = o que o buffer guardou vira o
    /// começo do arquivo (um arquivo só, sem emenda).
    pub fn start_recording(&self, path: &str, from_replay: bool) -> Result<(), String> {
        self.cmd(|reply| Cmd::StartRecording { path: path.to_owned(), from_replay, reply })
    }

    /// Fecha a gravação; ao voltar, o arquivo está completo. A captura (e o buffer) seguem.
    pub fn stop_recording(&self) -> Result<FileDone, String> {
        self.cmd(|reply| Cmd::StopRecording { reply })
    }

    /// Pausa/retoma a gravação: o trecho pausado não entra nem como buraco.
    pub fn set_paused(&self, paused: bool) {
        let at = now_ns().saturating_sub(self.start_ns) as i64;
        let c = if paused { Cmd::Pause { at_ns: at } } else { Cmd::Resume };
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Cmd(c));
        }
    }

    /// Salva o que o buffer guardou em `path` e esvazia o buffer (sem buraco:
    /// o corte é num keyframe forçado agora). Volta na hora; o resultado chega
    /// pelo canal quando o arquivo estiver completo.
    pub fn save_replay(&self, path: &str) -> std::sync::mpsc::Receiver<Result<FileDone, String>> {
        let (rtx, rrx) = channel();
        match &self.tx {
            Some(tx) => {
                let _ = tx.send(Msg::Cmd(Cmd::SaveReplay { path: path.to_owned(), reply: rtx }));
            }
            None => {
                let _ = rtx.send(Err("sessão encerrada".into()));
            }
        }
        rrx
    }

    pub fn set_replay_limits(&self, max_sec: u32, max_mb: u32) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Cmd(Cmd::SetReplayLimits { max_sec, max_mb }));
        }
    }

    /// Troca recorte/destino da fonte `index` (na ordem de `sources`) — vale
    /// a partir do próximo quadro. É o "mover/redimensionar a área no meio".
    pub fn set_placement(&self, index: usize, crop: Option<Rect>, dest: Option<Rect>) -> bool {
        let mut p = self.placements.lock().unwrap_or_else(|e| e.into_inner());
        match p.get_mut(index) {
            Some(slot) => {
                *slot = (crop, dest);
                true
            }
            None => false,
        }
    }

    /// A área do Timelapse mudou (moldura movida): o cursor é achado nela.
    pub fn set_motion_area(&self, area: (i32, i32, i32, i32)) {
        *self.motion_area.lock().unwrap_or_else(|e| e.into_inner()) = area;
    }

    /// Muda o mudo de uma fonte de áudio (pelo nome do `AudioInputSpec`).
    pub fn set_audio_muted(&self, name: &str, muted: bool) -> bool {
        let mut found = false;
        for s in self.audio_sources.iter().filter(|s| s.name() == name) {
            s.set_muted(muted);
            found = true;
        }
        found
    }

    /// Para, esvazia os encoders e fecha o arquivo. Ao voltar, o arquivo está completo.
    pub fn stop(mut self) -> SessionEnd {
        self.shutdown();
        let error = self.ctl.error.lock().unwrap_or_else(|p| p.into_inner()).clone();
        SessionEnd {
            path: self.path.clone(),
            frames: self.stats.frames.load(Ordering::Relaxed),
            lagged: self.stats.lagged.load(Ordering::Relaxed),
            audio_blocks: self.stats.audio_blocks.load(Ordering::Relaxed),
            error,
        }
    }

    fn shutdown(&mut self) {
        let _ = self.ctl.stop_ns.compare_exchange(u64::MAX, now_ns(), Ordering::SeqCst, Ordering::SeqCst);
        self.ctl.stop.store(true, Ordering::SeqCst);
        // Vídeo e áudio terminam e soltam os canais; o gravador fecha o arquivo
        // quando o último canal some (o da sessão sai aqui).
        self.tx = None;
        for t in [self.video.take(), self.audio.take(), self.writer.take()].into_iter().flatten() {
            let _ = t.join();
        }
        self.audio_sources.clear();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------------------
// Vídeo
// ---------------------------------------------------------------------------

struct VideoParts {
    dev: Device,
    sources: Vec<Box<dyn VideoSource>>,
    comp: Compositor,
    conv: Nv12Converter,
    enc: Box<dyn VideoEncoder>,
    motion: Option<MotionDetector>,
}

fn build_video(cfg: &SessionConfig) -> Result<VideoParts, String> {
    let dev = Device::create()?;
    let mut sources: Vec<Box<dyn VideoSource>> = Vec::new();
    for s in &cfg.sources {
        let src: Result<Box<dyn VideoSource>, String> = match &s.target {
            VideoTarget::Monitor(h) => WgcSource::new(&dev, WgcTarget::Monitor(*h), &s.name).map(|v| Box::new(v) as _),
            VideoTarget::Window { hwnd, client_area } => WgcSource::new(&dev, WgcTarget::Window { hwnd: *hwnd, client_area: *client_area }, &s.name).map(|v| Box::new(v) as _),
            VideoTarget::Webcam(cam) => WebcamSource::new(cam, &s.name).map(|v| Box::new(v) as _),
        };
        sources.push(src.map_err(|e| format!("fonte \"{}\": {e}", s.name))?);
    }
    let comp = Compositor::new(&dev, cfg.width, cfg.height)?;
    let conv = Nv12Converter::new(&dev, cfg.width, cfg.height)?;
    let enc = AvVideoEncoder::open(&dev, &cfg.encoder)?;
    let motion = match cfg.motion {
        Some(_) => Some(MotionDetector::new(&dev, cfg.width, cfg.height)?),
        None => None,
    };
    Ok(VideoParts { dev, sources, comp, conv, enc: Box::new(enc), motion })
}

fn video_thread(cfg: SessionConfig, placements: Placements, motion_area: MotionArea, ctl: Arc<Control>, stats: Arc<Stats>, ready: Sender<Result<StreamInfo, String>>, go: Receiver<Option<Go>>) {
    // WGC precisa de apartamento WinRT; o pool é free-threaded, então MTA serve.
    let ro = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
    let built = build_video(&cfg).and_then(|p| p.enc.stream_info().map(|i| (p, i)));
    match built {
        Err(e) => {
            log!("Captura: video nao montou: {e}");
            let _ = ready.send(Err(e));
        }
        Ok((mut p, info)) => {
            let _ = ready.send(Ok(info));
            if let Ok(Some(g)) = go.recv() {
                run_video(&cfg, &mut p, g, &placements, &motion_area, &ctl, &stats);
            }
        }
    }
    if ro {
        unsafe { RoUninitialize() };
    }
}

/// Cursor na área de trabalho, se visível.
fn cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorInfo, CURSORINFO, CURSOR_SHOWING};
    let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
    unsafe { GetCursorInfo(&mut ci) }.ok()?;
    (ci.flags.0 & CURSOR_SHOWING.0 != 0).then_some((ci.ptScreenPos.x, ci.ptScreenPos.y))
}

/// Caixa do cursor no canvas: a seta e o I-beam cabem em 64 px (escala 200%)
/// a partir do ponto quente, que fica perto do canto de cima à esquerda.
fn cursor_mask(pos: (i32, i32), area: (i32, i32, i32, i32), cw: u32, ch: u32) -> Option<Mask> {
    let (ax, ay, aw, ah) = area;
    if aw <= 0 || ah <= 0 {
        return None;
    }
    let (kx, ky) = (cw as f32 / aw as f32, ch as f32 / ah as f32);
    Some(Mask { x: (pos.0 - 16 - ax) as f32 * kx, y: (pos.1 - 16 - ay) as f32 * ky, w: 80.0 * kx, h: 80.0 * ky })
}

fn run_video(cfg: &SessionConfig, p: &mut VideoParts, go: Go, placements: &Placements, motion_area: &MotionArea, ctl: &Control, stats: &Stats) {
    let mut grid = FrameGrid::starting_at(cfg.fps, go.start_ns);
    let mut packets = Vec::new();
    let mut last_cursor: Option<(i32, i32)> = None;
    let frame_ns = |index: u64| (index as i128 * 1_000_000_000 / cfg.fps.max(1) as i128) as i64;
    while !ctl.stop.load(Ordering::SeqCst) {
        let tick = grid.wait();
        let t0 = now_ns();
        let key = ctl.force_key.swap(false, Ordering::SeqCst);
        p.comp.clear(&p.dev);
        let place = placements.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut poll_ns = 0;
        for (i, src) in p.sources.iter_mut().enumerate() {
            let tp = now_ns();
            src.poll(&p.dev);
            poll_ns += now_ns() - tp;
            if let Some((view, w, h)) = src.image() {
                let (crop, dest) = place.get(i).copied().unwrap_or((None, None));
                let crop = crop.unwrap_or(Rect { x: 0.0, y: 0.0, w: w as f32, h: h as f32 });
                let dest = dest.unwrap_or_else(|| fit_inside(crop.w as u32, crop.h as u32, cfg.width, cfg.height));
                p.comp.draw(&p.dev, view, w, h, crop, dest);
            }
        }
        p.conv.convert(&p.dev, p.comp.canvas_srv());
        if let (Some(det), Some(mc)) = (p.motion.as_mut(), cfg.motion) {
            let mut masks = Vec::new();
            if !mc.count_cursor {
                // O cursor de antes e o de agora: os dois pedaços mudaram.
                let area = *motion_area.lock().unwrap_or_else(|e| e.into_inner());
                let now = cursor_pos();
                for pos in [last_cursor, now].into_iter().flatten() {
                    masks.extend(cursor_mask(pos, area, cfg.width, cfg.height));
                }
                last_cursor = now;
            }
            if let Some((index, moving)) = det.feed(&p.dev, &p.comp.canvas, tick.index, masks) {
                let _ = go.tx.send(Msg::Activity { ns: frame_ns(index), moving });
            }
        }
        let te = now_ns();
        stats.worst_poll_us.fetch_max(poll_ns / 1000, Ordering::Relaxed);
        stats.worst_draw_us.fetch_max((te - t0 - poll_ns) / 1000, Ordering::Relaxed);
        if let Err(e) = p.enc.encode(&p.dev, &p.conv.nv12, tick.index as i64, key, &mut packets) {
            ctl.fail(e);
            break;
        }
        stats.worst_encode_us.fetch_max((now_ns() - te) / 1000, Ordering::Relaxed);
        for pk in packets.drain(..) {
            let _ = go.tx.send(Msg::Pkt(0, pk));
        }
        stats.frames.fetch_add(1, Ordering::Relaxed);
        stats.lagged.store(grid.lagged, Ordering::Relaxed);
        stats.busy_us.fetch_add((now_ns() - t0) / 1000, Ordering::Relaxed);
    }
    p.enc.flush(&mut packets);
    for pk in packets.drain(..) {
        let _ = go.tx.send(Msg::Pkt(0, pk));
    }
    log!(
        "Captura: video encerrado — {} quadros, {} atrasados; pior quadro: fontes {} ms, composicao {} ms, encoder {} ms.",
        stats.frames.load(Ordering::Relaxed),
        grid.lagged,
        stats.worst_poll_us.load(Ordering::Relaxed) / 1000,
        stats.worst_draw_us.load(Ordering::Relaxed) / 1000,
        stats.worst_encode_us.load(Ordering::Relaxed) / 1000
    );
}

// ---------------------------------------------------------------------------
// Áudio
// ---------------------------------------------------------------------------

fn audio_thread(mut mixer: AudioMixer, mut encoders: Vec<Box<dyn AudioEncoder>>, start_ns: u64, tx: PacketTx, ctl: Arc<Control>, stats: Arc<Stats>) {
    let sleeper = Sleeper::new();
    let mut out: Vec<Vec<f32>> = vec![Vec::new(); mixer.track_count()];
    let mut packets = Vec::new();
    let mut k: u64 = 0;
    loop {
        let t0 = block_start_ns(start_ns, k);
        // Parou: o áudio vai até o instante do pedido de parada (o vídeo para ali).
        if ctl.stop.load(Ordering::SeqCst) && t0 >= ctl.stop_ns.load(Ordering::SeqCst) {
            break;
        }
        let t1 = block_start_ns(start_ns, k + 1);
        sleeper.sleep_until(t1 + MIX_DELAY_NS);
        mixer.mix(t0, &mut out);
        let pts = (k * BLOCK_FRAMES as u64) as i64;
        for (t, enc) in encoders.iter_mut().enumerate() {
            if let Err(e) = enc.encode(&out[t], pts, &mut packets) {
                ctl.fail(e);
                return;
            }
            for pk in packets.drain(..) {
                let _ = tx.send(Msg::Pkt(1 + t, pk));
            }
        }
        stats.audio_blocks.fetch_add(1, Ordering::Relaxed);
        k += 1;
        debug_assert_eq!(out.first().map_or(BLOCK_FRAMES * CHANNELS, Vec::len), BLOCK_FRAMES * CHANNELS);
    }
    for (t, enc) in encoders.iter_mut().enumerate() {
        enc.flush(&mut packets);
        for pk in packets.drain(..) {
            let _ = tx.send(Msg::Pkt(1 + t, pk));
        }
    }
    log!("Captura: audio encerrado — {k} blocos ({:.2} s).", (k * BLOCK_FRAMES as u64) as f64 / super::audio::SAMPLE_RATE as f64);
}

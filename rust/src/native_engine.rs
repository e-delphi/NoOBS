//! O motor de captura atrás da interface que o Bridge enxerga
//! (`engine::CaptureEngine`). Traduz os pedidos do Bridge pro
//! `capture::pipeline::Session`:
//!
//! - montagem: monitores habilitados lado a lado, área = uma captura por
//!   monitor, janela = canvas da janela em múltiplo de 16, webcams no fim,
//!   TODAS as fontes de áudio — as desmarcadas mudas, pra religar no meio —,
//!   faixas pelo `compute_audio_track_assignments` (sem teto), e os nomes
//!   `NoOBS Mic - …` que o Bridge usa pra mutar;
//! - o que é demorado (parar, salvar trecho) roda numa thread, e o resultado
//!   volta pela fila de eventos do `with_engine`, na main thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use crate::audio_tracks::{build_track_names, compute_audio_track_assignments};
use crate::capture::audio::AudioDevice;
use crate::capture::encode::VideoEncoderConfig;
use crate::capture::gpu::{Device, Rect};
use crate::capture::layout::{area_layout, DesktopRect};
use crate::capture::pipeline::{AudioConfig, AudioInputSpec, Session, SessionConfig, SourceSpec, VideoTarget};
use crate::engine::{AreaRect, CaptureEngine, CaptureInputs, Event, RecordRequest, VideoStats};
use crate::scene::{filter_enabled_monitors, CaptureMonitor};
use crate::types::{EncoderCaps, AudioDev, RecordingLayout, RecordingRegion};
use crate::{config, encoder, lang, log, mainthread, win_audio_meter as wam};

/// Prefixo dos nomes das fontes de áudio: o Bridge muta pelo nome
/// (`set_source_muted`).
const PREFIX: &str = "NoOBS ";
/// Keyframe do buffer: o corte de um trecho cai num keyframe forçado, mas a
/// FILA é aparada por GOP inteiro — 1 s de GOP = 1 s de granularidade (#62).
const REPLAY_KEYFRAME_SEC: u32 = 1;

/// Área em gravação: o que o `update_area` precisa pra recalcular.
struct AreaState {
    canvas: (u32, u32),
    monitors: Vec<DesktopRect>,
}

pub struct NativeEngine {
    initialized: bool,
    caps: Option<EncoderCaps>,
    session: Option<Arc<Session>>,
    recording: bool,
    replay_active: bool,
    stopping: Option<Instant>,
    output_path: String,
    layout: RecordingLayout,
    encoder_id: String,
    fps: u32,
    area: Option<AreaState>,
    replay_dir: String,
    replay_saving: Arc<AtomicBool>,
    replay_save_start: Option<Instant>,
    /// Eventos produzidos nas threads de trabalho; o `with_engine` entrega.
    events: Arc<Mutex<Vec<Event>>>,
}

impl Default for NativeEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Avisa a main que há evento: o `with_engine` vazio recolhe e entrega.
fn wake_main() {
    mainthread::queue(|| {
        crate::engine::with_engine(|_| ());
    });
}

fn hmonitor_of(m: &CaptureMonitor) -> isize {
    let p = POINT { x: m.position_x + m.width / 2, y: m.position_y + m.height / 2 };
    unsafe { MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST) }.0 as isize
}

fn same_text(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

impl NativeEngine {
    pub fn new() -> Self {
        Self {
            initialized: false,
            caps: None,
            session: None,
            recording: false,
            replay_active: false,
            stopping: None,
            output_path: String::new(),
            layout: RecordingLayout::default(),
            encoder_id: String::new(),
            fps: 30,
            area: None,
            replay_dir: String::new(),
            replay_saving: Arc::new(AtomicBool::new(false)),
            replay_save_start: None,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn caps(&mut self) -> EncoderCaps {
        if self.caps.is_none() {
            // Detectar é também AQUECER: o AMF só se inicializa no 1º quadro (#fase 5).
            self.caps = Some(match Device::create() {
                Ok(d) => crate::capture::caps::detect(&d),
                Err(e) => {
                    log!("Motor proprio: sem D3D11 pra detectar encoders: {e}");
                    EncoderCaps { h264_sw: true, ..Default::default() }
                }
            });
        }
        self.caps.unwrap_or_default()
    }

    /// O pedido do Bridge em forma de `SessionConfig`.
    fn build_config(&mut self, req: &RecordRequest, inputs: &CaptureInputs, replay: Option<(u32, u32)>) -> Result<(SessionConfig, Vec<(String, bool)>), String> {
        let auto_profile = req.auto_profile;
        let src_active = |cat: &str, id: &str, default: bool| -> bool {
            if auto_profile {
                config::get_source_active_for_auto(cat, id, default)
            } else {
                config::get_source_active(cat, id, default)
            }
        };
        let area = req.area.filter(|a| a.w >= 16 && a.h >= 16);
        let window = req.window.clone().filter(|w| !w.spec.is_empty() && area.is_none());

        // ---- vídeo: inventário e canvas ----
        let mut monitors: Vec<CaptureMonitor> = if area.is_some() {
            inputs.monitors.clone()
        } else if window.is_some() {
            Vec::new()
        } else {
            filter_enabled_monitors(&inputs.monitors, auto_profile)
        };
        monitors.sort_by_key(|m| m.position_x);
        let cams: Vec<_> = if window.is_some() || area.is_some() {
            Vec::new()
        } else {
            inputs.webcams.iter().filter(|c| src_active("webcams", &c.name, false)).cloned().collect()
        };
        let (mut bw, mut bh): (i32, i32) = if let Some(a) = area {
            (a.w, a.h)
        } else if let Some(w) = &window {
            if w.width < 16 || w.height < 16 {
                (1920, 1080)
            } else {
                (w.width, w.height)
            }
        } else {
            (monitors.iter().map(|m| m.width).sum(), monitors.iter().map(|m| m.height).max().unwrap_or(0))
        };
        for c in &cams {
            bw += c.width;
            bh = bh.max(c.height);
        }
        if bw <= 0 || bh <= 0 {
            // Só áudio: canvas preto (#8).
            bw = 800;
            bh = 600;
        }
        let caps = self.caps();
        let codec = crate::capture::caps::resolve(&config::get_config_str("codec", "auto"), &caps);
        let (mut cw, mut ch) = crate::capture::caps::clamp_canvas(bw as u32, bh as u32, &codec);
        // Janela/área têm tamanho qualquer: múltiplo de 16 (coluna verde no AMF, #66).
        if (window.is_some() || area.is_some()) && cw >= 32 && ch >= 32 {
            cw &= !15;
            ch &= !15;
        }
        let (cw, ch) = (cw.max(2), ch.max(2));
        let scale = cw as f32 / bw as f32;
        let fps = match config::get_config_int("recordingFps", 30) {
            f if f < 10 => 30,
            f => f.min(1000),
        } as u32;

        let mut sources = Vec::new();
        let mut regions = Vec::new();
        let mut x = 0.0f32;
        self.area = None;
        if let Some(a) = area {
            let desk: Vec<DesktopRect> = monitors.iter().map(|m| DesktopRect { x: m.position_x, y: m.position_y, w: m.width, h: m.height }).collect();
            let place = area_layout(DesktopRect { x: a.x, y: a.y, w: a.w, h: a.h }, (cw, ch), &desk);
            for (m, p) in monitors.iter().zip(place) {
                let (crop, dest) = match p {
                    Some((c, d)) => (Some(c), Some(d)),
                    None => (None, Some(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 })),
                };
                sources.push(SourceSpec { name: format!("{PREFIX}Area Monitor {}", m.index), target: VideoTarget::Monitor(hmonitor_of(m)), crop, dest });
            }
            regions.push(RecordingRegion { name: format!("{}x{}", a.w, a.h), kind: "area".into(), x: 0, y: 0, w: cw as i32, h: ch as i32 });
            self.area = Some(AreaState { canvas: (cw, ch), monitors: desk });
        } else if let Some(w) = &window {
            sources.push(SourceSpec { name: format!("{PREFIX}Window"), target: VideoTarget::Window { hwnd: window_hwnd(&w.spec), client_area: true }, crop: None, dest: None });
            regions.push(RecordingRegion { name: w.title.clone(), kind: "window".into(), x: 0, y: 0, w: cw as i32, h: ch as i32 });
        } else {
            for m in &monitors {
                let (dw, dh) = (m.width as f32 * scale, m.height as f32 * scale);
                sources.push(SourceSpec { name: format!("{PREFIX}Monitor {}", m.index), target: VideoTarget::Monitor(hmonitor_of(m)), crop: None, dest: Some(Rect { x, y: 0.0, w: dw, h: dh }) });
                let name = if m.name.is_empty() { format!("Monitor {}", m.index) } else { m.name.clone() };
                regions.push(RecordingRegion { name, kind: "monitor".into(), x: x.round() as i32, y: 0, w: dw.round() as i32, h: dh.round() as i32 });
                x += dw;
            }
        }
        for c in &cams {
            let (dw, dh) = (c.width as f32 * scale, c.height as f32 * scale);
            sources.push(SourceSpec { name: format!("{PREFIX}Webcam - {}", c.name), target: VideoTarget::Webcam(c.name.clone()), crop: None, dest: Some(Rect { x, y: 0.0, w: dw, h: dh }) });
            regions.push(RecordingRegion { name: format!("Webcam — {}", c.name), kind: "webcam".into(), x: x.round() as i32, y: 0, w: dw.round() as i32, h: dh.round() as i32 });
            x += dw;
        }

        // ---- áudio: TODOS os dispositivos, desmarcados mudos (religáveis no meio) ----
        let devs = wam::enumerate_audio_devices();
        let pick = |kind: wam::DeviceKind, default_id: &str| -> Vec<AudioDev> {
            let mut v: Vec<AudioDev> = devs.iter().filter(|d| d.kind == kind).map(|d| AudioDev { name: d.name.clone(), device_id: d.device_id.clone() }).collect();
            // Padrão primeiro (mesma ordem da lista do Bridge).
            v.sort_by_key(|d| !(!default_id.is_empty() && same_text(&d.device_id, default_id)));
            v
        };
        let mics = pick(wam::DeviceKind::Input, &inputs.default_mic_id);
        let outs = pick(wam::DeviceKind::Output, &inputs.default_spk_id);
        let mic_en: Vec<bool> = mics.iter().map(|m| src_active("mics", &m.name, true)).collect();
        let out_en: Vec<bool> = outs.iter().map(|m| src_active("speakers", &m.name, true)).collect();
        let mic_def: Vec<bool> = mics.iter().map(|m| !inputs.default_mic_id.is_empty() && same_text(&m.device_id, &inputs.default_mic_id)).collect();
        let out_def: Vec<bool> = outs.iter().map(|m| !inputs.default_spk_id.is_empty() && same_text(&m.device_id, &inputs.default_spk_id)).collect();
        let app_pid = window.as_ref().filter(|w| w.audio && self.app_audio_available()).map(|w| window_pid(&w.spec));
        let mut asg = compute_audio_track_assignments(&mic_en, &mic_def, &out_en, &out_def);
        let mut app_track = 0;
        if app_pid.is_some() {
            app_track = asg.total_tracks + 1;
            asg.total_tracks = app_track;
        }
        let mut titles = build_track_names(asg.total_tracks, &mics, &outs, &asg.mic_tracks, &asg.out_tracks);
        // Faixa 1 do MKV = índice 0 (a mistura); desmarcado (0) entra só nela,
        // mudo — pra poder religar no meio sem remontar.
        let tracks_of = |track: i32| if track > 1 { vec![0, track as usize - 1] } else { vec![0] };
        let mut inputs_a = Vec::new();
        let mut mutes = Vec::new();
        for (j, m) in mics.iter().enumerate() {
            let name = format!("{PREFIX}Mic - {}", m.name);
            inputs_a.push(AudioInputSpec { name: name.clone(), device: AudioDevice::Input(m.device_id.clone()), tracks: tracks_of(asg.mic_tracks[j]) });
            mutes.push((name, !mic_en[j]));
        }
        for (j, o) in outs.iter().enumerate() {
            let name = format!("{PREFIX}Out - {}", o.name);
            inputs_a.push(AudioInputSpec { name: name.clone(), device: AudioDevice::Output(o.device_id.clone()), tracks: tracks_of(asg.out_tracks[j]) });
            mutes.push((name, !out_en[j]));
        }
        if let (Some(pid), Some(w)) = (app_pid, &window) {
            // Som do app: faixa própria; na mistura só se nenhuma saída grava (#66).
            let mut tracks = vec![app_track as usize - 1];
            if !out_en.iter().any(|&e| e) {
                tracks.push(0);
            }
            inputs_a.push(AudioInputSpec { name: format!("{PREFIX}App"), device: AudioDevice::App(pid), tracks });
            let app = if w.app.is_empty() { w.title.clone() } else { w.app.clone() };
            titles[(app_track - 1) as usize] = lang::t_with("tracks.appAudio", &[("app", &app)]);
        }
        let audio = (asg.total_tracks > 0).then(|| AudioConfig { inputs: inputs_a, track_titles: titles, kbps: encoder::audio_bitrate_kbps() as u32 });

        let keyframe = if replay.is_some() { REPLAY_KEYFRAME_SEC } else { config::get_config_int("recordingKeyframeSec", 2).clamp(1, 10) as u32 };
        let crf = encoder::quality_level_to_crf(encoder::capture_quality_level());
        self.encoder_id = codec.clone();
        self.fps = fps;
        self.layout = RecordingLayout { canvas_w: cw as i32, canvas_h: ch as i32, regions };
        log!("Motor proprio: {}x{} @{fps} fps, {} fonte(s) de video, {} de audio, encoder {codec} crf {crf}", cw, ch, sources.len(), mutes.len());
        let cfg = SessionConfig {
            width: cw,
            height: ch,
            fps,
            sources,
            encoder: VideoEncoderConfig { codec, width: cw, height: ch, fps, crf, keyframe_sec: keyframe },
            audio,
            replay,
            output_path: req.output_path.clone(),
        };
        Ok((cfg, mutes))
    }

    fn start_session(&mut self, cfg: SessionConfig, mutes: Vec<(String, bool)>) -> Result<(), String> {
        let s = Session::start(cfg)?;
        for (name, muted) in mutes {
            s.set_audio_muted(&name, muted);
        }
        self.session = Some(Arc::new(s));
        Ok(())
    }

    /// Para tudo numa thread (o `Drop` da sessão espera vídeo, áudio e gravador).
    fn drop_session_async(&mut self) {
        if let Some(s) = self.session.take() {
            std::thread::spawn(move || drop(s));
        }
    }
}

/// `HWND` da janela gravada: o Bridge passa o `"título:classe:exe"`; a
/// janela é reencontrada pelo `win_windows` (a lista guarda o handle).
fn window_hwnd(spec: &str) -> isize {
    crate::win_windows::enumerate_capture_windows().into_iter().find(|w| w.spec == spec).map(|w| w.handle).unwrap_or(0)
}

fn window_pid(spec: &str) -> u32 {
    let h = window_hwnd(spec);
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(HWND(h as *mut _), Some(&mut pid)) };
    pid
}

impl CaptureEngine for NativeEngine {
    fn take_events(&mut self) -> Vec<Event> {
        let ev = std::mem::take(&mut *self.events.lock().unwrap_or_else(|p| p.into_inner()));
        for e in &ev {
            match e {
                Event::Stopped(_) => {
                    self.stopping = None;
                }
                Event::ReplaySaved(_) => {
                    self.replay_save_start = None;
                }
            }
        }
        ev
    }
    fn encoder_caps(&mut self) -> EncoderCaps {
        self.caps()
    }
    fn predict_encoder_id(&mut self) -> String {
        let caps = self.caps();
        crate::capture::caps::resolve(&config::get_config_str("codec", "auto"), &caps)
    }

    fn is_initialized(&self) -> bool {
        self.initialized
    }
    fn ensure_initialized(&mut self) -> Result<(), String> {
        if !self.initialized {
            let t = Instant::now();
            let _ = self.caps();
            self.initialized = true;
            log!("Motor proprio: pronto em {} ms (encoders detectados e aquecidos).", t.elapsed().as_millis());
        }
        Ok(())
    }
    fn is_recording(&self) -> bool {
        self.recording
    }
    fn is_stopping(&self) -> bool {
        self.stopping.is_some()
    }
    fn stopping_elapsed_ms(&self) -> u64 {
        self.stopping.map_or(0, |t| t.elapsed().as_millis() as u64)
    }
    fn output_path(&self) -> &str {
        &self.output_path
    }
    fn current_layout(&self) -> &RecordingLayout {
        &self.layout
    }
    fn video_encoder_id(&self) -> &str {
        &self.encoder_id
    }
    fn app_audio_available(&self) -> bool {
        crate::win::windows_build() >= 19041
    }
    fn video_stats(&self) -> Option<VideoStats> {
        let s = self.session.as_ref()?;
        let frames = s.stats.frames.load(Ordering::Relaxed);
        let busy = s.stats.busy_us.load(Ordering::Relaxed);
        Some(VideoStats {
            total: frames as u32,
            lagged: s.stats.lagged.load(Ordering::Relaxed) as u32,
            v_total: frames as u32,
            skipped: 0,
            avg_frame_ns: if frames > 0 { busy * 1000 / frames } else { 0 },
            interval_ns: 1_000_000_000 / self.fps.max(1) as u64,
        })
    }
    fn set_source_muted(&self, source_name: &str, muted: bool) {
        if let Some(s) = &self.session {
            s.set_audio_muted(source_name, muted);
        }
    }
    fn update_area(&mut self, ar: AreaRect) -> bool {
        let (Some(st), Some(s)) = (&self.area, &self.session) else { return false };
        let place = area_layout(DesktopRect { x: ar.x, y: ar.y, w: ar.w, h: ar.h }, st.canvas, &st.monitors);
        for (i, p) in place.into_iter().enumerate() {
            let (c, d) = match p {
                Some((c, d)) => (Some(c), Some(d)),
                None => (None, Some(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 })),
            };
            s.set_placement(i, c, d);
        }
        true
    }
    fn build_and_start_recording(&mut self, req: &RecordRequest, inputs: &CaptureInputs) -> Result<(), String> {
        if self.recording || self.replay_active {
            return Err("Ja esta gravando.".into());
        }
        self.ensure_initialized()?;
        let (cfg, mutes) = self.build_config(req, inputs, None)?;
        self.start_session(cfg, mutes)?;
        self.output_path = req.output_path.clone();
        self.recording = true;
        Ok(())
    }
    fn set_recording_paused(&mut self, pause: bool) -> bool {
        match &self.session {
            Some(s) if self.recording => {
                s.set_paused(pause);
                true
            }
            _ => false,
        }
    }
    fn request_stop(&mut self) {
        if !self.recording || self.stopping.is_some() {
            return;
        }
        self.recording = false;
        self.stopping = Some(Instant::now());
        let path = self.output_path.clone();
        let events = self.events.clone();
        // Com o buffer ligado a sessão continua (só a gravação fecha); sem ele, a
        // sessão inteira para — fora da main, que o Drop espera as threads.
        let keep = self.replay_active;
        let Some(s) = (if keep { self.session.clone() } else { self.session.take() }) else { return };
        std::thread::spawn(move || {
            if let Err(e) = s.stop_recording() {
                log!("Motor proprio: parar a gravacao: {e}");
            }
            // O arquivo já está completo: avisa ANTES de desmontar a sessão. A
            // desmontagem espera as threads das fontes (a webcam pode demorar,
            // ver `WebcamSource::drop`) e não pode segurar o botão de gravar.
            events.lock().unwrap_or_else(|p| p.into_inner()).push(Event::Stopped(path));
            wake_main();
            drop(s);
        });
    }
    fn force_complete_stop(&mut self) {
        // O stop roda numa thread; o evento chega pela fila. Sem nada a forçar.
        if self.stopping.is_some() {
            log!("Motor proprio: finalizacao ainda em curso ({} ms) — o arquivo ainda nao fechou.", self.stopping_elapsed_ms());
        }
    }
    fn stop_recording(&mut self) -> String {
        // Síncrono (desligamento): fecha o arquivo antes de voltar.
        let path = self.output_path.clone();
        if self.recording {
            if let Some(s) = &self.session {
                let _ = s.stop_recording();
            }
            self.recording = false;
        }
        if !self.replay_active {
            self.session = None;
        }
        path
    }
    fn is_replay_active(&self) -> bool {
        self.replay_active
    }
    fn is_replay_saving(&self) -> bool {
        self.replay_saving.load(Ordering::SeqCst)
    }
    fn build_and_start_replay(&mut self, dir: &str, max_sec: i32, max_mb: i32, inputs: &CaptureInputs) -> Result<(), String> {
        if self.recording || self.replay_active {
            return Err("Ja esta gravando.".into());
        }
        self.ensure_initialized()?;
        let req = RecordRequest { output_path: String::new(), auto_profile: false, window: None, area: None };
        let (cfg, mutes) = self.build_config(&req, inputs, Some((max_sec.max(1) as u32, max_mb.max(1) as u32)))?;
        self.start_session(cfg, mutes)?;
        self.replay_dir = dir.to_owned();
        let _ = std::fs::create_dir_all(dir);
        self.replay_active = true;
        Ok(())
    }
    fn start_recording_from_replay(&mut self, output_path: &str) -> bool {
        let Some(s) = &self.session else { return false };
        if !self.replay_active || self.recording {
            return false;
        }
        match s.start_recording(output_path, true) {
            Ok(()) => {
                self.output_path = output_path.to_owned();
                self.recording = true;
                // O buffer virou o começo da gravação e, pro Bridge, acabou — ele o religa quando a gravação parar. A
                // fila da sessão fica mínima até lá, e o fim da gravação fecha a
                // sessão inteira.
                self.replay_active = false;
                s.set_replay_limits(1, 1);
                true
            }
            Err(e) => {
                log!("Motor proprio: gravar a partir do buffer: {e}");
                false
            }
        }
    }
    fn save_replay(&mut self) -> bool {
        let Some(s) = &self.session else { return false };
        if !self.replay_active || self.replay_saving.load(Ordering::SeqCst) {
            return false;
        }
        let name = format!("Trecho {}.mkv", chrono::Local::now().format("%Y-%m-%d %H-%M-%S"));
        let path = std::path::Path::new(&self.replay_dir).join(name).to_string_lossy().into_owned();
        let rx = s.save_replay(&path);
        self.replay_saving.store(true, Ordering::SeqCst);
        self.replay_save_start = Some(Instant::now());
        let (saving, events) = (self.replay_saving.clone(), self.events.clone());
        std::thread::spawn(move || {
            let done = match rx.recv() {
                Ok(Ok(d)) => d.path,
                Ok(Err(e)) => {
                    log!("Motor proprio: trecho do buffer falhou: {e}");
                    String::new()
                }
                Err(_) => String::new(),
            };
            saving.store(false, Ordering::SeqCst);
            events.lock().unwrap_or_else(|p| p.into_inner()).push(Event::ReplaySaved(done));
            wake_main();
        });
        true
    }
    fn abort_replay_save(&mut self) {
        // O corte não tem fase adiada; um trecho em escrita termina sozinho.
        self.replay_saving.store(false, Ordering::SeqCst);
    }
    fn stop_replay(&mut self) {
        if !self.replay_active {
            return;
        }
        self.replay_active = false;
        if self.recording {
            // A gravação segue na mesma sessão: o buffer só para de crescer.
            if let Some(s) = &self.session {
                s.set_replay_limits(1, 1);
            }
        } else {
            self.drop_session_async();
        }
    }
    fn set_replay_limits(&mut self, max_sec: i32, max_mb: i32) {
        if let Some(s) = &self.session {
            s.set_replay_limits(max_sec.max(1) as u32, max_mb.max(1) as u32);
        }
    }
}

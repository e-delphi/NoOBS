//! Gravador: a única thread que escreve arquivo. Recebe numa fila só os
//! pacotes (das threads de vídeo e áudio) e os pedidos da sessão — então a
//! ordem entre "este pacote" e "comece a gravar" é a ordem em que chegaram.
//!
//! Duas saídas, cada uma opcional e independente:
//! - **gravação**: um MKV; começa no 1º keyframe (forçado no pedido), passa
//!   pelo portão de pausa (`output::PauseGate`) e é rebaseada pra começar em 0;
//! - **buffer**: a fila em memória (`output::ReplayRing`); um corte vira um
//!   trecho escrito numa thread à parte, sem segurar a captura.
//!
//! A gravação que CONTINUA o buffer recebe a fila inteira como começo e segue
//! com os pacotes ao vivo: um arquivo só, sem emenda (a #62 inteira some).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use super::encode::StreamInfo;
use super::output::{from_first_keyframe, Item, Meta, PauseGate, ReplayRing, Timelapse};
use super::sink::{MkvSink, PacketSink};
use crate::ffi::ffmpeg::*;
use crate::log;

/// Resultado de uma gravação ou de um trecho do buffer.
#[derive(Debug, Clone)]
pub struct FileDone {
    pub path: String,
    /// Duração do arquivo (do 1º keyframe ao último quadro), em segundos.
    pub duration_sec: f64,
}

pub enum Cmd {
    StartRecording { path: String, from_replay: bool, reply: Sender<Result<(), String>> },
    StopRecording { reply: Sender<Result<FileDone, String>> },
    /// `at_ns` em tempo de sessão.
    Pause { at_ns: i64 },
    Resume,
    SaveReplay { path: String, reply: Sender<Result<FileDone, String>> },
    SetReplayLimits { max_sec: u32, max_mb: u32 },
}

pub enum Msg {
    Pkt(usize, Packet),
    Cmd(Cmd),
    /// Timelapse: o quadro do instante `ns` mudou (ou não) em relação ao anterior.
    Activity { ns: i64, moving: bool },
}

/// O que o roteador conta pra fora.
#[derive(Default)]
pub struct RouterStats {
    pub replay_held_ms: AtomicU64,
    pub replay_bytes: AtomicU64,
    pub recording_ms: AtomicU64,
}

struct Recording {
    sink: MkvSink,
    gate: PauseGate,
    /// Instante do 1º keyframe (o 0 do arquivo).
    base: Option<i64>,
    /// Pacotes que chegaram ANTES do 1º keyframe: o encoder de vídeo segura os
    /// primeiros quadros (lookahead do x264, latência do AMF) por centenas de
    /// ms, e o áudio do mesmo instante chega antes deles. Descartar dava um
    /// arquivo com o áudio começando ~300 ms depois do vídeo.
    early: Vec<(Meta, Packet)>,
    last_ns: i64,
    /// Timelapse: os pacotes passam por aqui antes (corta o que ficou parado).
    timelapse: Option<Timelapse<Packet>>,
}

pub struct Router {
    infos: Vec<StreamInfo>,
    recording: Option<Recording>,
    replay: Option<ReplayRing<Packet>>,
    /// As gravações desta sessão são Timelapse.
    timelapse: bool,
    pending: HashMap<u64, (String, Sender<Result<FileDone, String>>)>,
    next_cut: u64,
    /// Força um keyframe no próximo quadro (a sessão e o roteador pedem).
    force_key: Arc<AtomicBool>,
    stats: Arc<RouterStats>,
    clip_threads: Vec<std::thread::JoinHandle<()>>,
}

fn ns_of(pkt: &Packet, tb: AVRational) -> i64 {
    if pkt.pts == AV_NOPTS_VALUE || tb.den == 0 {
        return 0;
    }
    (pkt.pts as i128 * tb.num as i128 * 1_000_000_000 / tb.den as i128) as i64
}

/// Desloca pts/dts de `shift_ns` pra trás, na time_base do stream.
fn shift(pkt: &mut Packet, shift_ns: i64, tb: AVRational) {
    if shift_ns == 0 || tb.num == 0 {
        return;
    }
    let d = (shift_ns as i128 * tb.den as i128 / (tb.num as i128 * 1_000_000_000)) as i64;
    if pkt.pts != AV_NOPTS_VALUE {
        pkt.pts -= d;
    }
    if pkt.dts != AV_NOPTS_VALUE {
        pkt.dts -= d;
    }
}

impl Router {
    pub fn new(infos: Vec<StreamInfo>, replay: Option<(u32, u32)>, timelapse: bool, force_key: Arc<AtomicBool>, stats: Arc<RouterStats>) -> Self {
        let audio: Vec<usize> = (1..infos.len()).collect();
        Self {
            replay: replay.map(|(s, m)| ReplayRing::new(s, m, audio)),
            infos,
            recording: None,
            timelapse,
            pending: HashMap::new(),
            next_cut: 1,
            force_key,
            stats,
            clip_threads: Vec::new(),
        }
    }

    fn meta(&self, stream: usize, pkt: &Packet) -> Meta {
        let tb = self.infos.get(stream).map(|i| i.time_base).unwrap_or(AVRational { num: 1, den: 1000 });
        Meta { stream, ns: ns_of(pkt, tb), video: stream == 0, key: pkt.flags & AV_PKT_FLAG_KEY != 0, size: pkt.size.max(0) as usize }
    }

    /// Laço: até vídeo, áudio e sessão soltarem a fila.
    pub fn run(mut self, rx: Receiver<Msg>) -> Option<String> {
        let mut error = None;
        for msg in rx {
            let r = match msg {
                Msg::Pkt(s, p) => self.packet(s, p),
                Msg::Cmd(c) => {
                    self.command(c);
                    Ok(())
                }
                Msg::Activity { ns, moving } => self.activity(ns, moving),
            };
            if let Err(e) = r {
                log!("Captura: gravador: {e}");
                error.get_or_insert(e);
            }
        }
        // Fim da sessão: fecha a gravação e o corte pendente.
        if let Err(e) = self.timelapse_release(true) {
            error.get_or_insert(e);
        }
        if let Some(rec) = self.recording.take() {
            if let Err(e) = Self::finish_recording(rec) {
                error.get_or_insert(e);
            }
        }
        if let Some((id, clip)) = self.replay.as_mut().and_then(|r| r.force_close_cut()) {
            self.spawn_clip(id, clip);
        }
        for t in self.clip_threads.drain(..) {
            let _ = t.join();
        }
        error
    }

    fn packet(&mut self, stream: usize, pkt: Packet) -> Result<(), String> {
        let meta = self.meta(stream, &pkt);
        let mut done = None;
        if let Some(ring) = &mut self.replay {
            if let Some(copy) = pkt.clone_pkt() {
                done = ring.push(Item { meta, data: copy });
            }
            self.stats.replay_held_ms.store((ring.held_ns() / 1_000_000).max(0) as u64, Ordering::Relaxed);
            self.stats.replay_bytes.store(ring.bytes() as u64, Ordering::Relaxed);
        }
        if let Some((id, clip)) = done {
            self.spawn_clip(id, clip);
        }
        if let Some(tl) = self.recording.as_mut().and_then(|r| r.timelapse.as_mut()) {
            tl.push(Item { meta, data: pkt });
            return self.timelapse_release(false);
        }
        if self.recording.is_some() {
            self.record(meta, pkt)?;
        }
        Ok(())
    }

    fn activity(&mut self, ns: i64, moving: bool) -> Result<(), String> {
        let Some(tl) = self.recording.as_mut().and_then(|r| r.timelapse.as_mut()) else { return Ok(()) };
        tl.activity(ns, moving);
        self.timelapse_release(false)
    }

    /// Passa pra gravação o que o Timelapse já decidiu, já no tempo emendado.
    fn timelapse_release(&mut self, end: bool) -> Result<(), String> {
        let Some(tl) = self.recording.as_mut().and_then(|r| r.timelapse.as_mut()) else { return Ok(()) };
        let out = tl.release(end);
        for (mut it, off) in out {
            if off != 0 {
                shift(&mut it.data, off, self.infos[it.meta.stream].time_base);
                it.meta.ns -= off;
            }
            self.record(it.meta, it.data)?;
        }
        Ok(())
    }

    /// Um pacote na gravação: base no 1º keyframe, portão de pausa, rebase.
    fn record(&mut self, meta: Meta, pkt: Packet) -> Result<(), String> {
        let rec = self.recording.as_mut().unwrap();
        if rec.base.is_none() {
            if !(meta.video && meta.key) {
                // Ainda sem o 1º keyframe: segura (com teto, pra um encoder mudo
                // não acumular memória sem fim).
                if rec.early.len() < 4096 {
                    rec.early.push((meta, pkt));
                }
                return Ok(());
            }
            rec.base = Some(meta.ns);
            // O que chegou antes e é de DEPOIS da base entra agora, na ordem.
            let early = std::mem::take(&mut rec.early);
            self.record_one(meta, pkt)?;
            for (m, p) in early {
                if !m.video {
                    self.record_one(m, p)?;
                }
            }
            return Ok(());
        }
        self.record_one(meta, pkt)
    }

    fn record_one(&mut self, meta: Meta, mut pkt: Packet) -> Result<(), String> {
        let tb = self.infos[meta.stream].time_base;
        let rec = self.recording.as_mut().unwrap();
        let Some(base) = rec.base else { return Ok(()) };
        if meta.ns < base {
            return Ok(()); // áudio de antes da imagem
        }
        let Some(offset) = rec.gate.admit(&meta) else { return Ok(()) };
        shift(&mut pkt, base + offset, tb);
        rec.last_ns = rec.last_ns.max(meta.ns - base - offset);
        self.stats.recording_ms.store((rec.last_ns / 1_000_000).max(0) as u64, Ordering::Relaxed);
        rec.sink.write(meta.stream, &mut pkt)
    }

    fn command(&mut self, c: Cmd) {
        match c {
            Cmd::StartRecording { path, from_replay, reply } => {
                let r = self.start_recording(&path, from_replay);
                let _ = reply.send(r);
            }
            Cmd::StopRecording { reply } => {
                let flushed = self.timelapse_release(true);
                let r = match self.recording.take() {
                    Some(rec) => Self::finish_recording(rec),
                    None => Err("não está gravando".into()),
                };
                let r = flushed.and(r);
                let _ = reply.send(r);
            }
            Cmd::Pause { at_ns } => {
                if let Some(r) = &mut self.recording {
                    r.gate.pause(at_ns);
                }
            }
            Cmd::Resume => {
                if let Some(r) = &mut self.recording {
                    r.gate.resume();
                    self.force_key.store(true, Ordering::SeqCst);
                }
            }
            Cmd::SaveReplay { path, reply } => match &mut self.replay {
                None => {
                    let _ = reply.send(Err("buffer desligado".into()));
                }
                Some(ring) => {
                    let id = self.next_cut;
                    if ring.request_cut(id) {
                        self.next_cut += 1;
                        self.pending.insert(id, (path, reply));
                        // O corte é no próximo keyframe: força ele AGORA.
                        self.force_key.store(true, Ordering::SeqCst);
                    } else {
                        let _ = reply.send(Err("o trecho anterior ainda está sendo salvo".into()));
                    }
                }
            },
            Cmd::SetReplayLimits { max_sec, max_mb } => {
                if let Some(ring) = &mut self.replay {
                    ring.set_limits(max_sec, max_mb);
                }
            }
        }
    }

    fn clone_infos(&self) -> Result<Vec<StreamInfo>, String> {
        self.infos.iter().map(|i| i.try_clone()).collect()
    }

    fn start_recording(&mut self, path: &str, from_replay: bool) -> Result<(), String> {
        if self.recording.is_some() {
            return Err("já está gravando".into());
        }
        if from_replay && self.replay.as_ref().is_some_and(|r| r.cut_pending()) {
            return Err("um trecho do buffer ainda está sendo salvo".into());
        }
        let sink = MkvSink::create(path, &self.clone_infos()?)?;
        let timelapse = self.timelapse.then(|| Timelapse::new(super::pipeline::TIMELAPSE_MARGIN_NS));
        self.recording = Some(Recording { sink, gate: PauseGate::default(), base: None, early: Vec::new(), last_ns: 0, timelapse });
        if from_replay {
            // O que o buffer guardou vira o COMEÇO desta gravação; a fila recomeça.
            let held = self.replay.as_mut().map(|r| r.take_all()).unwrap_or_default();
            let (_, items) = from_first_keyframe(held);
            log!("Captura: gravacao continua o buffer ({} pacotes guardados).", items.len());
            for it in items {
                self.record(it.meta, it.data)?;
            }
        }
        // Sem buffer pra continuar, o arquivo começa num keyframe forçado agora.
        self.force_key.store(true, Ordering::SeqCst);
        log!("Captura: gravando em \"{path}\".");
        Ok(())
    }

    fn finish_recording(mut rec: Recording) -> Result<FileDone, String> {
        rec.sink.finish()?;
        Ok(FileDone { path: rec.sink.path.clone(), duration_sec: rec.last_ns as f64 / 1e9 })
    }

    /// Escreve um trecho do buffer numa thread à parte.
    fn spawn_clip(&mut self, id: u64, clip: Vec<Item<Packet>>) {
        let Some((path, reply)) = self.pending.remove(&id) else { return };
        let infos = match self.clone_infos() {
            Ok(i) => i,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let t = std::thread::Builder::new().name("noobs-trecho".into()).spawn(move || {
            let r = write_clip(&path, &infos, clip);
            match &r {
                Ok(d) => log!("Captura: trecho salvo \"{}\" ({:.2} s).", d.path, d.duration_sec),
                Err(e) => log!("Captura: trecho falhou: {e}"),
            }
            let _ = reply.send(r);
        });
        if let Ok(t) = t {
            self.clip_threads.push(t);
        }
    }
}

/// Um arquivo a partir de pacotes guardados: começa no 1º keyframe, em 0.
fn write_clip(path: &str, infos: &[StreamInfo], clip: Vec<Item<Packet>>) -> Result<FileDone, String> {
    let (base, items) = from_first_keyframe(clip);
    let base = base.ok_or("o buffer não tinha nenhum keyframe")?;
    let mut sink = MkvSink::create(path, infos)?;
    let mut last = 0i64;
    for mut it in items {
        let tb = infos[it.meta.stream].time_base;
        shift(&mut it.data, base, tb);
        last = last.max(it.meta.ns - base);
        sink.write(it.meta.stream, &mut it.data)?;
    }
    sink.finish()?;
    Ok(FileDone { path: path.to_owned(), duration_sec: last as f64 / 1e9 })
}

//! Thread de áudio do player: decodifica TODAS as faixas do arquivo (a mistura
//! e as isoladas), soma cada uma com o volume dela, mantém o tom em outras
//! velocidades (`TimeStretch`, o WSOLA da exportação — #51m) e toca no
//! dispositivo padrão pelo WASAPI. É o relógio do player: o instante que o
//! dispositivo já consumiu é o "agora" da reprodução.
//!
//! A versão web precisava de um `<audio>` por faixa isolada sincronizado ao
//! vídeo (com correção de deriva e o limite de 6 conexões do Chromium); aqui as
//! faixas saem do MESMO demuxer, então andam juntas por construção. Decodificar
//! todas sempre custa pouco (AAC) e deixa o volume de cada uma valer na hora.
//!
//! O volume geral é aplicado na escrita pro dispositivo (responde já); o de cada
//! faixa, na mistura (vale ~150 ms depois, o que está no anel).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, WAVEFORMATEX,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

use super::{ACmd, Shared, AUDIO_MAX_RATE};
use crate::ffi::ffmpeg::*;
use crate::log;
use crate::time_stretch::TimeStretch;

/// `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY`:
/// o Windows converte da taxa do arquivo pra do dispositivo.
const AUTOCONVERT: u32 = 0x8000_0000 | 0x0800_0000;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const AV_SAMPLE_FMT_S16: i32 = 1;
const AV_SAMPLE_FMT_FLT: i32 = 3;
const AV_SAMPLE_FMT_S16P: i32 = 6;
/// Bloco da mistura (o quadro do AAC).
const BLOCK: usize = 1024;
/// Quanto fica pronto à frente do dispositivo.
const RING_TARGET_SEC: f64 = 0.15;
/// Buffer do dispositivo (100 ms em unidades de 100 ns).
const DEVICE_BUFFER_HNS: i64 = 1_000_000;

struct Track {
    dec: CodecCtx,
    idx: i32,
    tb: AVRational,
    channels: usize,
    /// Amostras estéreo intercaladas a partir de `head`.
    fifo: VecDeque<f32>,
    head: f64,
    eof: bool,
}

/// Um pedaço pronto pro dispositivo: estéreo intercalado, o instante da mídia
/// da 1ª amostra e quanto da mídia cada amostra anda (velocidade).
struct Chunk {
    data: Vec<f32>,
    pos: usize,
    media: f64,
    rate: f64,
}

/// O que foi escrito no dispositivo: a partir da amostra `at` (contagem total),
/// `n` amostras que começam no instante `media`.
struct Mark {
    at: u64,
    n: u64,
    media: f64,
    rate: f64,
}

struct Device {
    client: IAudioClient,
    render: IAudioRenderClient,
    buf_frames: u32,
    started: bool,
}

impl Device {
    fn open(rate: u32) -> Result<Self, String> {
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("MMDeviceEnumerator: {e}"))?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|e| format!("sem dispositivo de saida: {e}"))?;
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None).map_err(|e| format!("Activate: {e}"))?;
            let wfx = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
                nChannels: 2,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * 8,
                nBlockAlign: 8,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUTOCONVERT, DEVICE_BUFFER_HNS, 0, &wfx, None).map_err(|e| format!("Initialize: {e}"))?;
            let buf_frames = client.GetBufferSize().map_err(|e| format!("GetBufferSize: {e}"))?;
            let render: IAudioRenderClient = client.GetService().map_err(|e| format!("GetService: {e}"))?;
            Ok(Self { client, render, buf_frames, started: false })
        }
    }

    fn start(&mut self) {
        if !self.started {
            unsafe {
                let _ = self.client.Start();
            }
            self.started = true;
        }
    }

    fn stop(&mut self) {
        if self.started {
            unsafe {
                let _ = self.client.Stop();
            }
            self.started = false;
        }
    }

    /// Descarta o que estava no buffer do dispositivo (seek).
    fn reset(&mut self) {
        let was = self.started;
        self.stop();
        unsafe {
            let _ = self.client.Reset();
        }
        if was {
            self.start();
        }
    }

    fn padding(&self) -> Result<u32, String> {
        unsafe { self.client.GetCurrentPadding() }.map_err(|e| format!("GetCurrentPadding: {e}"))
    }
}

/// Amostra `s` do canal `ch` como float (FLTP/FLT/S16P/S16).
unsafe fn sample(f: &AVFrame, s: usize, ch: usize, nch: usize) -> f32 {
    match f.format {
        AV_SAMPLE_FMT_FLTP => f.data.get(ch).filter(|p| !p.is_null()).map_or(0.0, |p| (*p as *const f32).add(s).read_unaligned()),
        AV_SAMPLE_FMT_FLT => (f.data[0] as *const f32).add(s * nch + ch).read_unaligned(),
        AV_SAMPLE_FMT_S16P => f.data.get(ch).filter(|p| !p.is_null()).map_or(0.0, |p| (*p as *const i16).add(s).read_unaligned() as f32 / 32768.0),
        AV_SAMPLE_FMT_S16 => (f.data[0] as *const i16).add(s * nch + ch).read_unaligned() as f32 / 32768.0,
        _ => 0.0,
    }
}

struct Engine {
    a: &'static FfmpegApi,
    input: Input,
    tracks: Vec<Track>,
    pkt: Packet,
    frame: Frame,
    rate_hz: u32,
    /// Próximo instante a misturar.
    mix_t: f64,
    gains: Vec<f32>,
    stretch: Option<TimeStretch>,
    speed: f64,
    ring: VecDeque<Chunk>,
    ring_frames: usize,
    /// Instante da mídia que o próximo pedaço esticado representa.
    stretch_media: f64,
}

impl Engine {
    fn open(path: &str) -> Result<Self, String> {
        let a = api()?;
        let input = Input::open(path).map_err(|rc| format!("abrir: {}", av_err_str(rc)))?;
        if !input.find_stream_info() {
            return Err("find_stream_info falhou".into());
        }
        let mut tracks = Vec::new();
        let mut rate_hz = 0u32;
        for i in 0..input.nb_streams() {
            let Some(st) = input.stream(i) else { continue };
            let Some(par) = (unsafe { st.codecpar.as_ref() }) else { continue };
            if par.codec_type != AVMEDIA_TYPE_AUDIO {
                continue;
            }
            let Some(dec) = CodecCtx::open_decoder_for(par, false) else { continue };
            if rate_hz == 0 && par.sample_rate > 0 {
                rate_hz = par.sample_rate as u32;
            }
            // Faixa com outra taxa não entra na mistura (o NoOBS grava todas iguais).
            if par.sample_rate > 0 && par.sample_rate as u32 != rate_hz {
                continue;
            }
            tracks.push(Track {
                dec,
                idx: i as i32,
                tb: st.time_base,
                channels: par.ch_layout.nb_channels.max(1) as usize,
                fifo: VecDeque::new(),
                head: 0.0,
                eof: false,
            });
        }
        if tracks.is_empty() {
            return Err("sem audio".into());
        }
        let mut gains = vec![0.0; tracks.len()];
        gains[0] = 1.0;
        Ok(Self {
            a,
            input,
            tracks,
            pkt: Packet::alloc().ok_or("pkt")?,
            frame: Frame::alloc().ok_or("frame")?,
            rate_hz: if rate_hz == 0 { 48000 } else { rate_hz },
            mix_t: 0.0,
            gains,
            stretch: None,
            speed: 1.0,
            ring: VecDeque::new(),
            ring_frames: 0,
            stretch_media: 0.0,
        })
    }

    fn seek(&mut self, t: f64) {
        let a = self.a;
        unsafe {
            (a.format.av_seek_frame)(self.input.ptr(), -1, (t.max(0.0) * 1e6) as i64, AVSEEK_FLAG_BACKWARD);
        }
        for tr in &mut self.tracks {
            unsafe { (a.codec.avcodec_flush_buffers)(tr.dec.ptr()) };
            tr.fifo.clear();
            tr.eof = false;
        }
        self.mix_t = t.max(0.0);
        self.reset_output();
    }

    fn reset_output(&mut self) {
        self.ring.clear();
        self.ring_frames = 0;
        self.stretch_media = self.mix_t;
        self.stretch = (self.speed != 1.0).then(|| TimeStretch::new(2, self.rate_hz as i32, self.speed));
    }

    fn set_speed(&mut self, s: f64) {
        if (s - self.speed).abs() < 1e-9 {
            return;
        }
        self.speed = s;
        // O que já está no anel foi esticado na velocidade velha e continua valendo.
        self.stretch_media = self.mix_t;
        self.stretch = (s != 1.0).then(|| TimeStretch::new(2, self.rate_hz as i32, s));
    }

    fn all_eof(&self) -> bool {
        self.tracks.iter().all(|t| t.eof && t.fifo.is_empty())
    }

    /// A faixa tem amostras até `t`?
    fn covers(&self, i: usize, t: f64) -> bool {
        let tr = &self.tracks[i];
        tr.eof || tr.head + (tr.fifo.len() / 2) as f64 / self.rate_hz as f64 >= t
    }

    /// Lê pacotes até toda faixa cobrir `until` (ou o arquivo acabar).
    fn fill(&mut self, until: f64) {
        let a = self.a;
        let mut guard = 0;
        while (0..self.tracks.len()).any(|i| !self.covers(i, until)) {
            guard += 1;
            // Uma faixa que nunca entrega (vazia no trecho) não pode segurar as
            // outras: depois de 2 s de folga, a mistura segue com silêncio nela.
            let ahead = self.tracks.iter().any(|t| t.head + (t.fifo.len() / 2) as f64 / self.rate_hz as f64 > until + 2.0);
            if ahead || guard > 20_000 {
                break;
            }
            if unsafe { (a.format.av_read_frame)(self.input.ptr(), self.pkt.ptr()) } < 0 {
                for i in 0..self.tracks.len() {
                    unsafe { (a.codec.avcodec_send_packet)(self.tracks[i].dec.ptr(), std::ptr::null()) };
                    self.drain(i);
                    self.tracks[i].eof = true;
                }
                break;
            }
            let si = self.pkt.stream_index;
            if let Some(i) = self.tracks.iter().position(|t| t.idx == si) {
                if unsafe { (a.codec.avcodec_send_packet)(self.tracks[i].dec.ptr(), self.pkt.ptr()) } == 0 {
                    self.drain(i);
                }
            }
            self.pkt.unref();
        }
    }

    fn drain(&mut self, i: usize) {
        let a = self.a;
        while unsafe { (a.codec.avcodec_receive_frame)(self.tracks[i].dec.ptr(), self.frame.ptr()) } == 0 {
            let f = unsafe { &*self.frame.ptr() };
            let n = f.nb_samples.max(0) as usize;
            let tr = &mut self.tracks[i];
            let ts = if f.pts != AV_NOPTS_VALUE { f.pts } else { f.pkt_dts };
            if tr.fifo.is_empty() && ts != AV_NOPTS_VALUE {
                tr.head = ts as f64 * tr.tb.num as f64 / tr.tb.den as f64;
            }
            let nch = tr.channels;
            for s in 0..n {
                let (l, r) = unsafe {
                    if nch == 1 {
                        let v = sample(f, s, 0, 1);
                        (v, v)
                    } else {
                        (sample(f, s, 0, nch), sample(f, s, 1, nch))
                    }
                };
                tr.fifo.push_back(l);
                tr.fifo.push_back(r);
            }
            self.frame.unref();
        }
    }

    /// Mistura `BLOCK` amostras a partir de `mix_t`. `None` = acabou.
    fn mix_block(&mut self) -> Option<Vec<f32>> {
        let n = BLOCK;
        let sr = self.rate_hz as f64;
        let end = self.mix_t + n as f64 / sr;
        self.fill(end);
        if self.all_eof() {
            return None;
        }
        let mut out = vec![0.0f32; n * 2];
        let t0 = self.mix_t;
        for (i, tr) in self.tracks.iter_mut().enumerate() {
            let g = self.gains.get(i).copied().unwrap_or(0.0);
            // Antes do bloco (o seek cai no keyframe/quadro anterior): descarta.
            let mut start = ((tr.head - t0) * sr).round() as i64;
            if start < 0 {
                let drop = ((-start) as usize * 2).min(tr.fifo.len());
                tr.fifo.drain(..drop);
                tr.head += (drop / 2) as f64 / sr;
                start = ((tr.head - t0) * sr).round().max(0.0) as i64;
            }
            let start = start as usize;
            if start >= n {
                continue; // a faixa só começa depois deste bloco
            }
            let take = ((n - start) * 2).min(tr.fifo.len());
            if g > 0.0 {
                for (k, v) in tr.fifo.iter().take(take).enumerate() {
                    out[start * 2 + k] += v * g;
                }
            }
            tr.fifo.drain(..take);
            tr.head += (take / 2) as f64 / sr;
        }
        self.mix_t = end;
        Some(out)
    }

    /// Completa o anel até a meta.
    fn top_up(&mut self) {
        let target = (RING_TARGET_SEC * self.rate_hz as f64) as usize;
        while self.ring_frames < target {
            let t_block = self.mix_t;
            let Some(block) = self.mix_block() else { break };
            match self.stretch.as_mut() {
                None => {
                    self.ring_frames += BLOCK;
                    self.ring.push_back(Chunk { data: block, pos: 0, media: t_block, rate: 1.0 });
                }
                Some(ts) => {
                    // Planar pro WSOLA.
                    let l: Vec<f32> = block.iter().step_by(2).copied().collect();
                    let r: Vec<f32> = block.iter().skip(1).step_by(2).copied().collect();
                    let mut planes = [std::ptr::null_mut::<u8>(); 8];
                    planes[0] = l.as_ptr() as *mut u8;
                    planes[1] = r.as_ptr() as *mut u8;
                    unsafe { ts.put(Some(&planes), BLOCK) };
                    ts.process();
                    if ts.out_n > 0 {
                        let m = ts.out_n;
                        let mut data = Vec::with_capacity(m * 2);
                        for k in 0..m {
                            data.push(ts.outp[0][k]);
                            data.push(ts.outp[1][k]);
                        }
                        ts.out_n = 0;
                        let media = self.stretch_media;
                        self.stretch_media += m as f64 * self.speed / self.rate_hz as f64;
                        self.ring_frames += m;
                        self.ring.push_back(Chunk { data, pos: 0, media, rate: self.speed });
                    }
                }
            }
        }
    }
}

pub(super) fn run(path: &str, rx: Receiver<ACmd>, shared: Arc<Shared>, has_audio: Arc<AtomicBool>) {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    run_inner(path, rx, &shared, &has_audio);
    if com {
        unsafe { CoUninitialize() };
    }
}

fn run_inner(path: &str, rx: Receiver<ACmd>, shared: &Shared, has_audio: &AtomicBool) {
    let mut eng = match Engine::open(path) {
        Ok(e) => e,
        Err(e) => {
            log!("Player: sem audio ({e})");
            // Sem áudio o relógio é o de parede; a thread só espera fechar.
            while rx.recv().is_ok() {}
            return;
        }
    };
    let mut dev = match Device::open(eng.rate_hz) {
        Ok(d) => d,
        Err(e) => {
            log!("Player: saida de audio nao abriu ({e})");
            while rx.recv().is_ok() {}
            return;
        }
    };
    log!("Player: audio {} faixa(s) a {} Hz", eng.tracks.len(), eng.rate_hz);
    has_audio.store(true, Ordering::Relaxed);
    shared.clock.set_use_audio(true);

    let mut playing = false;
    let mut master = 1.0f32;
    let mut written: u64 = 0;
    let mut marks: VecDeque<Mark> = VecDeque::new();
    let silent = |speed: f64| speed > AUDIO_MAX_RATE;

    loop {
        // ---- comandos ----
        let wait = if playing { Duration::from_millis(5) } else { Duration::from_millis(200) };
        let mut cmds = Vec::new();
        match rx.recv_timeout(wait) {
            Ok(c) => cmds.push(c),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        while let Ok(c) = rx.try_recv() {
            cmds.push(c);
        }
        let last_seek = cmds.iter().rposition(|c| matches!(c, ACmd::Seek(_)));
        for (i, c) in cmds.into_iter().enumerate() {
            match c {
                ACmd::Play => {
                    playing = true;
                    if !silent(eng.speed) {
                        dev.start();
                    }
                }
                ACmd::Pause => {
                    playing = false;
                    dev.stop();
                }
                ACmd::Seek(t) if Some(i) == last_seek => {
                    eng.seek(t);
                    dev.reset();
                    marks.clear();
                }
                ACmd::Rate(r) => {
                    eng.set_speed(r);
                    if silent(r) {
                        dev.stop();
                        dev.reset();
                        eng.reset_output();
                        marks.clear();
                    } else if playing {
                        dev.start();
                    }
                }
                ACmd::Volumes(m, g) => {
                    master = m;
                    for (k, v) in g.into_iter().enumerate() {
                        if k < eng.gains.len() {
                            eng.gains[k] = v;
                        }
                    }
                }
                _ => {}
            }
        }
        if !playing || silent(eng.speed) {
            continue;
        }

        // ---- produz e escreve ----
        eng.top_up();
        let pad = match dev.padding() {
            Ok(p) => p,
            Err(e) => {
                // Dispositivo trocado/sumiu (fone desplugado): reabre o padrão e
                // retoma do ponto atual.
                log!("Player: saida de audio caiu ({e}); reabrindo");
                std::thread::sleep(Duration::from_millis(200));
                match Device::open(eng.rate_hz) {
                    Ok(d) => {
                        dev = d;
                        let now = shared.clock.now();
                        eng.seek(now);
                        marks.clear();
                        dev.start();
                    }
                    Err(e) => log!("Player: reabrir a saida falhou ({e})"),
                }
                continue;
            }
        };
        let space = dev.buf_frames.saturating_sub(pad) as usize;
        let n = space.min(eng.ring_frames);
        if n > 0 {
            let mut data: *mut u8 = std::ptr::null_mut();
            if let Ok(p) = unsafe { dev.render.GetBuffer(n as u32) } {
                data = p;
            }
            if !data.is_null() {
                let out = unsafe { std::slice::from_raw_parts_mut(data as *mut f32, n * 2) };
                let mut k = 0usize;
                while k < n {
                    let Some(ch) = eng.ring.front_mut() else { break };
                    let avail = ch.data.len() / 2 - ch.pos;
                    let take = avail.min(n - k);
                    let media = ch.media + ch.pos as f64 * ch.rate / eng.rate_hz as f64;
                    marks.push_back(Mark { at: written + k as u64, n: take as u64, media, rate: ch.rate });
                    for j in 0..take * 2 {
                        out[k * 2 + j] = ch.data[ch.pos * 2 + j] * master;
                    }
                    ch.pos += take;
                    k += take;
                    if ch.pos * 2 >= ch.data.len() {
                        eng.ring.pop_front();
                    }
                }
                eng.ring_frames -= k;
                unsafe {
                    let _ = dev.render.ReleaseBuffer(k as u32, 0);
                }
                written += k as u64;
            }
        }
        // ---- relógio: o que o dispositivo já consumiu ----
        let played = written.saturating_sub(pad as u64);
        while marks.len() > 1 && marks[0].at + marks[0].n <= played {
            marks.pop_front();
        }
        if let Some(m) = marks.front() {
            let into = played.saturating_sub(m.at).min(m.n) as f64;
            shared.clock.audio_at(m.media + into * m.rate / eng.rate_hz as f64);
        }
    }
    dev.stop();
}

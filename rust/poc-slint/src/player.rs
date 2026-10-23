//! Thread de decode: lê o arquivo, decodifica na placa (D3D11VA, o mesmo
//! caminho da exportação — pegadinha #51l), traz o quadro pra memória, converte
//! pra RGBA no tamanho da tela e entrega pra UI no instante do pts. O relógio é
//! de parede (não há áudio nesta prova): quadro atrasado é descartado ANTES da
//! cópia da placa, que é a etapa cara.

use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{Rgba8Pixel, SharedPixelBuffer};

use crate::ffi::ffmpeg::*;

/// `AV_PIX_FMT_RGBA` no avutil-59 (pixfmt.h: ... nv12=23, nv21, argb, rgba=26).
const AV_PIX_FMT_RGBA: c_int = 26;

pub enum Cmd {
    Toggle,
    Seek(f64),
    Step(f64),
    Resize(i32, i32),
}

/// Contadores lidos pela UI e pela thread.
#[derive(Default)]
pub struct Shared {
    /// Quadro entregue à UI e ainda não aplicado: o próximo é descartado em vez
    /// de empilhar no laço de eventos.
    pub ui_pending: AtomicBool,
    /// Quadros que o renderer do Slint de fato pintou (rendering notifier).
    pub painted: AtomicU64,
}

pub struct Info {
    pub duration: f64,
    pub file_name: String,
}

pub enum Out {
    Info(Info),
    Frame { buf: SharedPixelBuffer<Rgba8Pixel>, pts: f64 },
    State { playing: bool },
    Stats(String),
    Error(String),
}

struct Player {
    a: &'static FfmpegApi,
    input: Input,
    v_idx: i32,
    tb: AVRational,
    dec: Option<CodecCtx>,
    dec_name: String,
    hw_dev: *mut c_void,
    pkt: Packet,
    frame: Frame,
    sw: Frame,
    sws: Option<(Sws, [c_int; 5])>,
    src_w: i32,
    src_h: i32,
    fps: f64,
    duration: f64,
    view: (i32, i32),
    eof_sent: bool,
    // medidas
    t_xfer: Duration,
    t_sws: Duration,
    n_conv: u32,
    hw_frames: bool,
    /// Conversão na placa; `None` = caminho de CPU (sem placa, falhou, ou
    /// POC_CPU=1 pra comparar).
    gpu: Option<crate::gpu::GpuConv>,
}

impl Drop for Player {
    fn drop(&mut self) {
        self.dec = None;
        if !self.hw_dev.is_null() {
            unsafe { (self.a.util.av_buffer_unref)(&mut self.hw_dev) };
        }
    }
}

impl Player {
    fn open(path: &str) -> Result<Self, String> {
        let a = api()?;
        let input = Input::open(path).map_err(|rc| format!("abrir: {}", av_err_str(rc)))?;
        if !input.find_stream_info() {
            return Err("find_stream_info falhou".into());
        }
        let v_idx = unsafe { first_stream_of_type(input.ptr(), AVMEDIA_TYPE_VIDEO) }.ok_or("sem vídeo")? as i32;
        let st = input.stream(v_idx as u32).ok_or("stream")?;
        let par = unsafe { st.codecpar.as_ref() }.ok_or("codecpar")?;
        let fr = st.avg_frame_rate;
        let mut fps = if fr.den > 0 { fr.num as f64 / fr.den as f64 } else { 0.0 };
        if fps <= 0.0 {
            fps = unsafe { estimate_fps_from_packets(input.ptr(), v_idx) };
        }
        if fps <= 0.0 {
            fps = 30.0;
        }
        let (tb, src_w, src_h) = (st.time_base, par.width, par.height);
        let duration = unsafe { container_duration_us(input.ptr()) } as f64 / 1e6;
        let mut p = Player {
            a,
            input,
            v_idx,
            tb,
            dec: None,
            dec_name: String::new(),
            hw_dev: std::ptr::null_mut(),
            pkt: Packet::alloc().ok_or("pkt")?,
            frame: Frame::alloc().ok_or("frame")?,
            sw: Frame::alloc().ok_or("frame")?,
            sws: None,
            src_w,
            src_h,
            fps,
            duration,
            view: (1280, 720),
            eof_sent: false,
            t_xfer: Duration::ZERO,
            t_sws: Duration::ZERO,
            n_conv: 0,
            hw_frames: false,
            gpu: None,
        };
        // Placa primeiro; só vale se saírem quadros D3D11 de verdade (o nativo de
        // H.264/HEVC cai calado pro software, e o de AV1 nem decodifica — #51l).
        if p.open_hw_decoder() && p.probe() == AV_PIX_FMT_D3D11 {
            p.hw_frames = true;
            if std::env::var_os("POC_CPU").is_none() {
                p.gpu = crate::gpu::GpuConv::new(p.hw_dev);
            }
        } else {
            p.dec = None;
            let par = unsafe { p.input.stream(v_idx as u32).and_then(|s| s.codecpar.as_ref()) }.ok_or("codecpar")?;
            let dec = CodecCtx::open_decoder_for(par, true).ok_or("nenhum decoder abriu")?;
            p.dec_name = unsafe { crate::ffi::cstr_to_string((*(a.codec.avcodec_find_decoder)(par.codec_id)).name) };
            p.dec = Some(dec);
            if p.probe() == AV_PIX_FMT_NONE {
                return Err(format!("decoder {} não entregou quadro", p.dec_name));
            }
        }
        p.input.rewind();
        Ok(p)
    }

    fn dec_ptr(&self) -> *mut AVCodecContext {
        self.dec.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr)
    }

    fn open_hw_decoder(&mut self) -> bool {
        let a = self.a;
        let Some(par) = (unsafe { self.input.stream(self.v_idx as u32).and_then(|s| s.codecpar.as_ref()) }) else {
            return false;
        };
        let name = match par.codec_id {
            AV_CODEC_ID_AV1 => "av1",
            AV_CODEC_ID_HEVC => "hevc",
            AV_CODEC_ID_H264 => "h264",
            _ => return false,
        };
        let d = unsafe { (a.codec.avcodec_find_decoder_by_name)(to_utf8(name).as_ptr()) };
        if d.is_null() {
            return false;
        }
        if unsafe { (a.util.av_hwdevice_ctx_create)(&mut self.hw_dev, AV_HWDEVICE_TYPE_D3D11VA, std::ptr::null(), std::ptr::null_mut(), 0) } < 0 {
            return false;
        }
        let Some(ctx) = CodecCtx::alloc(d) else { return false };
        if unsafe { (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par) } < 0 {
            return false;
        }
        unsafe {
            ((ctx.ptr() as *mut u8).add(OFFS_CODECCTX_HW_DEVICE_CTX) as *mut *mut c_void).write_unaligned((a.util.av_buffer_ref)(self.hw_dev));
        }
        if ctx.open(d) < 0 {
            return false;
        }
        self.dec = Some(ctx);
        self.dec_name = name.to_string();
        true
    }

    /// Decodifica até sair um quadro; devolve o formato (ou NONE).
    fn probe(&mut self) -> c_int {
        self.eof_sent = false;
        let mut fmt = AV_PIX_FMT_NONE;
        for _ in 0..240 {
            if !self.next_frame() {
                break;
            }
            fmt = self.frame.format;
            self.frame.unref();
            break;
        }
        unsafe { (self.a.codec.avcodec_flush_buffers)(self.dec_ptr()) };
        self.eof_sent = false;
        fmt
    }

    /// Próximo quadro decodificado em `self.frame`. `false` = fim (ou erro).
    fn next_frame(&mut self) -> bool {
        let a = self.a;
        let dec = self.dec_ptr();
        loop {
            let r = unsafe { (a.codec.avcodec_receive_frame)(dec, self.frame.ptr()) };
            if r == 0 {
                return true;
            }
            if r != AVERROR_EAGAIN || self.eof_sent {
                return false;
            }
            if unsafe { (a.format.av_read_frame)(self.input.ptr(), self.pkt.ptr()) } < 0 {
                unsafe { (a.codec.avcodec_send_packet)(dec, std::ptr::null()) };
                self.eof_sent = true;
                continue;
            }
            if self.pkt.stream_index == self.v_idx {
                unsafe { (a.codec.avcodec_send_packet)(dec, self.pkt.ptr()) };
            }
            self.pkt.unref();
        }
    }

    fn frame_pts(&self) -> f64 {
        let ts = if self.frame.pts != AV_NOPTS_VALUE { self.frame.pts } else { self.frame.pkt_dts };
        if ts == AV_NOPTS_VALUE {
            return 0.0;
        }
        ts as f64 * self.tb.num as f64 / self.tb.den as f64
    }

    /// Tamanho de saída: cabe na área do vídeo, nunca maior que a origem, par.
    fn out_size(&self) -> (i32, i32) {
        let (vw, vh) = (self.view.0.max(16) as f64, self.view.1.max(16) as f64);
        let s = (vw / self.src_w as f64).min(vh / self.src_h as f64).min(1.0);
        let w = ((self.src_w as f64 * s) as i32) & !1;
        let h = ((self.src_h as f64 * s) as i32) & !1;
        (w.max(2), h.max(2))
    }

    /// Placa → memória (NV12) → RGBA no tamanho da tela.
    fn convert(&mut self) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
        let a = self.a;
        let t0 = Instant::now();
        if self.frame.format == AV_PIX_FMT_D3D11 {
            if let Some(g) = self.gpu.as_mut() {
                let (dw, dh) = {
                    let (vw, vh) = (self.view.0.max(16) as f64, self.view.1.max(16) as f64);
                    let s = (vw / self.src_w as f64).min(vh / self.src_h as f64).min(1.0);
                    (((self.src_w as f64 * s) as i32) & !1, ((self.src_h as f64 * s) as i32) & !1)
                };
                let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(dw as u32, dh as u32);
                match g.convert(unsafe { &*self.frame.ptr() }, dw, dh, buf.make_mut_bytes()) {
                    Ok(()) => {
                        self.t_xfer += t0.elapsed();
                        self.n_conv += 1;
                        return Some(buf);
                    }
                    Err(e) => {
                        eprintln!("conversao na placa falhou ({e}); caindo pra CPU");
                        self.gpu = None;
                    }
                }
            }
        }
        let src: *const AVFrame = if self.frame.format == AV_PIX_FMT_D3D11 {
            self.sw.unref();
            if unsafe { (a.util.av_hwframe_transfer_data)(self.sw.ptr(), self.frame.ptr(), 0) } < 0 {
                return None;
            }
            self.sw.ptr()
        } else {
            self.frame.ptr()
        };
        let t1 = Instant::now();
        let s = unsafe { &*src };
        let (dw, dh) = self.out_size();
        let key = [s.width, s.height, s.format, dw, dh];
        if self.sws.as_ref().is_none_or(|(_, k)| *k != key) {
            self.sws = Sws::new(s.width, s.height, s.format, dw, dh, AV_PIX_FMT_RGBA, SWS_BILINEAR).map(|c| (c, key));
        }
        let (sws, _) = self.sws.as_ref()?;
        let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(dw as u32, dh as u32);
        let mut dst = [std::ptr::null_mut::<u8>(); 8];
        let mut dls = [0 as c_int; 8];
        dst[0] = buf.make_mut_bytes().as_mut_ptr();
        dls[0] = dw * 4;
        let mut sd = [std::ptr::null::<u8>(); 8];
        for i in 0..8 {
            sd[i] = s.data[i];
        }
        sws.scale_raw(&sd, &s.linesize, s.height, &dst, &dls);
        self.t_xfer += t1 - t0;
        self.t_sws += t1.elapsed();
        self.n_conv += 1;
        Some(buf)
    }

    /// Seek EXATO: keyframe anterior + decodifica descartando até o instante.
    fn seek(&mut self, t: f64) -> bool {
        let a = self.a;
        let t = t.clamp(0.0, (self.duration - 0.05).max(0.0));
        let ts = us_to_ts((t * 1e6) as i64, self.tb);
        unsafe {
            (a.format.av_seek_frame)(self.input.ptr(), self.v_idx, ts, AVSEEK_FLAG_BACKWARD);
            (a.codec.avcodec_flush_buffers)(self.dec_ptr());
        }
        self.eof_sent = false;
        let half = 0.5 / self.fps;
        while self.next_frame() {
            if self.frame_pts() + half >= t {
                return true;
            }
            self.frame.unref();
        }
        false
    }
}

/// PNG sem compressão (blocos "stored" do deflate): só pra conferir a imagem a olho.
pub fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity((w * 4 + 1) as usize * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * w as usize * 4..(y + 1) * w as usize * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        z.push(if chunks.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    std::fs::write(path, out)
}

fn fmt_time(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

pub fn time_text(pos: f64, dur: f64) -> String {
    format!("{} / {}", fmt_time(pos), fmt_time(dur))
}

/// Memória do processo (working set e privada), MB.
fn mem_mb() -> (u64, u64) {
    #[repr(C)]
    #[derive(Default)]
    struct Pmc {
        cb: u32,
        page_fault_count: u32,
        peak_ws: usize,
        ws: usize,
        qpp: usize,
        qp: usize,
        qpnp: usize,
        qnp: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(h: isize, p: *mut Pmc, cb: u32) -> i32;
    }
    let mut p = Pmc { cb: std::mem::size_of::<Pmc>() as u32, ..Default::default() };
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut p, p.cb) };
    ((p.ws / 1048576) as u64, (p.pagefile / 1048576) as u64)
}

pub fn run(path: String, rx: Receiver<Cmd>, shared: Arc<Shared>, out: impl Fn(Out)) {
    let mut p = match Player::open(&path) {
        Ok(p) => p,
        Err(e) => {
            out(Out::Error(e));
            return;
        }
    };
    let file_name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    out(Out::Info(Info { duration: p.duration, file_name }));

    let frame_dur = 1.0 / p.fps;
    let mut playing = true;
    let mut clock_start = Instant::now();
    let mut clock_base = 0.0f64; // instante do vídeo em `clock_start`
    let mut cur_pos = 0.0f64;
    let mut inbox: VecDeque<Cmd> = VecDeque::new();
    let (mut decoded, mut shown, mut late, mut ui_busy) = (0u64, 0u64, 0u64, 0u64);
    let mut last_seek_ms = 0.0f64;
    let mut stats_at = Instant::now();
    let mut painted_at = shared.painted.load(Ordering::Relaxed);
    let mut ended = false;
    out(Out::State { playing });

    let present = |buf: SharedPixelBuffer<Rgba8Pixel>, pts: f64, shown: &mut u64, ui_busy: &mut u64| {
        if shared.ui_pending.swap(true, Ordering::AcqRel) {
            *ui_busy += 1;
            return;
        }
        *shown += 1;
        out(Out::Frame { buf, pts });
    };

    'main: loop {
        // ---- comandos ----
        loop {
            match if playing || !inbox.is_empty() { rx.try_recv() } else { rx.recv().map_err(|_| TryRecvError::Disconnected) } {
                Ok(c) => inbox.push_back(c),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'main,
            }
            if !playing {
                // Parado: junta o que mais chegou junto e processa.
                while let Ok(c) = rx.try_recv() {
                    inbox.push_back(c);
                }
                break;
            }
        }
        while let Some(c) = inbox.pop_front() {
            match c {
                Cmd::Toggle => {
                    playing = !playing;
                    if playing && ended {
                        ended = false;
                        if p.seek(0.0) {
                            p.frame.unref();
                        }
                        cur_pos = 0.0;
                    }
                    clock_base = cur_pos;
                    clock_start = Instant::now();
                    out(Out::State { playing });
                }
                Cmd::Seek(_) | Cmd::Step(_) => {
                    // Só o último seek da fila importa (arrastar a barra manda dezenas).
                    if inbox.iter().any(|c| matches!(c, Cmd::Seek(_) | Cmd::Step(_))) {
                        continue;
                    }
                    let t = match c {
                        Cmd::Seek(t) => t,
                        Cmd::Step(d) => cur_pos + d,
                        _ => unreachable!(),
                    };
                    let t0 = Instant::now();
                    ended = false;
                    if p.seek(t) {
                        let pts = p.frame_pts();
                        if let Some(buf) = p.convert() {
                            shared.ui_pending.store(false, Ordering::Release);
                            present(buf, pts, &mut shown, &mut ui_busy);
                        }
                        p.frame.unref();
                        cur_pos = pts;
                    }
                    last_seek_ms = t0.elapsed().as_secs_f64() * 1000.0;
                    clock_base = cur_pos;
                    clock_start = Instant::now();
                }
                Cmd::Resize(w, h) => p.view = (w, h),
            }
        }

        // ---- medidas (1/s) ----
        if stats_at.elapsed() >= Duration::from_secs(1) {
            let secs = stats_at.elapsed().as_secs_f64();
            let painted = shared.painted.load(Ordering::Relaxed);
            let (ws, private) = mem_mb();
            let n = p.n_conv.max(1) as f64;
            let (ow, oh) = p.out_size();
            out(Out::Stats(format!(
                "decoder {} ({})   origem {}x{} @ {:.2} fps -> {}x{}\n\
                 decodificados {:5.1}/s   entregues {:5.1}/s   pintados {:5.1}/s\n\
                 descartados: atrasados {}   UI ocupada {}\n\
                 {} {:5.2} ms   RGBA na CPU {:5.2} ms   ultimo seek {:.0} ms\n\
                 memoria: {} MB (privada {} MB)   [i] esconde",
                p.dec_name,
                if p.hw_frames { "placa, D3D11VA" } else { "software" },
                p.src_w,
                p.src_h,
                p.fps,
                ow,
                oh,
                decoded as f64 / secs,
                shown as f64 / secs,
                (painted - painted_at) as f64 / secs,
                late,
                ui_busy,
                if p.gpu.is_some() { "conversao NA PLACA" } else { "copia da placa" },
                p.t_xfer.as_secs_f64() * 1000.0 / n,
                p.t_sws.as_secs_f64() * 1000.0 / n,
                last_seek_ms,
                ws,
                private
            )));
            decoded = 0;
            shown = 0;
            p.t_xfer = Duration::ZERO;
            p.t_sws = Duration::ZERO;
            p.n_conv = 0;
            painted_at = painted;
            stats_at = Instant::now();
        }

        if !playing {
            continue;
        }

        // ---- reprodução ----
        if !p.next_frame() {
            playing = false;
            ended = true;
            out(Out::State { playing });
            continue;
        }
        decoded += 1;
        let pts = p.frame_pts();
        let now_media = clock_base + clock_start.elapsed().as_secs_f64();
        if now_media - pts > frame_dur * 1.5 {
            // Atrasado: descarta antes da cópia da placa, que é a etapa cara.
            late += 1;
            p.frame.unref();
            cur_pos = pts;
            continue;
        }
        let buf = p.convert();
        p.frame.unref();
        // Espera o instante do quadro, acordando pra comandos.
        let due = clock_start + Duration::from_secs_f64((pts - clock_base).max(0.0));
        loop {
            let now = Instant::now();
            if now >= due {
                break;
            }
            match rx.recv_timeout((due - now).min(Duration::from_millis(5))) {
                Ok(c) => {
                    inbox.push_back(c);
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break 'main,
            }
        }
        if !inbox.is_empty() {
            continue; // seek/pausa no meio da espera: o quadro já não vale
        }
        cur_pos = pts;
        if let Some(buf) = buf {
            present(buf, pts, &mut shown, &mut ui_busy);
        }
    }
}

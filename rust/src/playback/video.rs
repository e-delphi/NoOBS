//! Thread de vídeo do player: lê o arquivo, decodifica na placa (D3D11VA —
//! pegadinha #51l), converte NA PLACA pros pedaços do `layout` e entrega no
//! instante do relógio. Quadro atrasado é descartado ANTES da conversão, que é a
//! etapa cara.
//!
//! Sem placa (decoder sem D3D11VA, ou o de AV1 que não decodifica — #51l) o
//! quadro vem em memória e o swscale converte cada pedaço.

use std::ffi::{c_int, c_void};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{Rgba8Pixel, SharedPixelBuffer};

use super::{gpu::GpuConv, layout, Info, Layout, Out, Shared, VCmd, ViewSpec, TRICK_MIN_RATE};
use crate::ffi::ffmpeg::*;
use crate::log;

/// `AV_PIX_FMT_RGBA` no avutil-59 (pixfmt.h: … nv12=23, nv21, argb, rgba=26).
const AV_PIX_FMT_RGBA: c_int = 26;
/// Imagem de saída nunca maior que isto em pixels (proteção contra palco
/// absurdo; um 4K cheio é 8,3 M).
const MAX_OUT_PIXELS: i64 = 16_000_000;

struct Dec {
    a: &'static FfmpegApi,
    input: Input,
    v_idx: i32,
    tb: AVRational,
    dec: Option<CodecCtx>,
    hw_dev: *mut c_void,
    pkt: Packet,
    frame: Frame,
    /// Último quadro mostrado (refeito quando o zoom/a vista muda, pausado).
    held: Frame,
    has_held: bool,
    sw: Frame,
    sws: Vec<(Sws, [c_int; 5])>,
    src_w: i32,
    src_h: i32,
    fps: f64,
    duration: f64,
    eof_sent: bool,
    hw_frames: bool,
    gpu: Option<GpuConv>,
    keyframes: Vec<f64>,
}

impl Drop for Dec {
    fn drop(&mut self) {
        self.held.unref();
        self.frame.unref();
        self.dec = None;
        if !self.hw_dev.is_null() {
            unsafe { (self.a.util.av_buffer_unref)(&mut self.hw_dev) };
        }
    }
}

impl Dec {
    fn open(path: &str) -> Result<Self, String> {
        let a = api()?;
        let input = Input::open(path).map_err(|rc| format!("abrir: {}", av_err_str(rc)))?;
        if !input.find_stream_info() {
            return Err("find_stream_info falhou".into());
        }
        let v_idx = unsafe { first_moving_video(input.ptr()) }.ok_or(NO_VIDEO)? as i32;
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
        let mut duration = unsafe { container_duration_us(input.ptr()) } as f64 / 1e6;
        if duration <= 0.0 {
            duration = unsafe { scan_duration_by_packets(input.ptr()) } as f64 / 1e6;
        }
        let mut d = Dec {
            a,
            input,
            v_idx,
            tb,
            dec: None,
            hw_dev: std::ptr::null_mut(),
            pkt: Packet::alloc().ok_or("pkt")?,
            frame: Frame::alloc().ok_or("frame")?,
            held: Frame::alloc().ok_or("frame")?,
            has_held: false,
            sw: Frame::alloc().ok_or("frame")?,
            sws: Vec::new(),
            src_w,
            src_h,
            fps,
            duration: duration.max(0.0),
            eof_sent: false,
            hw_frames: false,
            gpu: None,
            keyframes: Vec::new(),
        };
        // Placa primeiro; só vale se saírem quadros D3D11 de verdade (o nativo de
        // H.264/HEVC cai calado pro software, e o de AV1 nem decodifica — #51l).
        if d.open_hw_decoder() && d.probe() == AV_PIX_FMT_D3D11 {
            d.hw_frames = true;
            d.gpu = GpuConv::new(d.hw_dev);
        } else {
            d.dec = None;
            let par = unsafe { d.input.stream(v_idx as u32).and_then(|s| s.codecpar.as_ref()) }.ok_or("codecpar")?;
            d.dec = Some(CodecCtx::open_decoder_for(par, true).ok_or("nenhum decoder abriu")?);
            if d.probe() == AV_PIX_FMT_NONE {
                return Err("o decoder nao entregou quadro".into());
            }
        }
        d.input.rewind();
        Ok(d)
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
        true
    }

    /// Decodifica até sair um quadro; devolve o formato (ou NONE).
    fn probe(&mut self) -> c_int {
        self.eof_sent = false;
        let mut fmt = AV_PIX_FMT_NONE;
        if self.next_frame() {
            fmt = self.frame.format;
            self.frame.unref();
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

    fn pts_of(&self, f: &Frame) -> f64 {
        let ts = if f.pts != AV_NOPTS_VALUE { f.pts } else { f.pkt_dts };
        if ts == AV_NOPTS_VALUE {
            return 0.0;
        }
        ts as f64 * self.tb.num as f64 / self.tb.den as f64
    }

    /// Guarda o quadro decodificado como o "mostrado".
    fn hold(&mut self) {
        self.held.unref();
        unsafe { (self.a.util.av_frame_move_ref)(self.held.ptr(), self.frame.ptr()) };
        self.has_held = true;
    }

    /// Seek EXATO: keyframe anterior + decodifica descartando até o instante.
    fn seek(&mut self, t: f64) -> bool {
        let a = self.a;
        let t = t.clamp(0.0, (self.duration - 0.02).max(0.0));
        let ts = us_to_ts((t * 1e6) as i64, self.tb);
        unsafe {
            (a.format.av_seek_frame)(self.input.ptr(), self.v_idx, ts, AVSEEK_FLAG_BACKWARD);
            (a.codec.avcodec_flush_buffers)(self.dec_ptr());
        }
        self.eof_sent = false;
        let half = 0.5 / self.fps;
        let mut got = false;
        while self.next_frame() {
            got = true;
            if self.pts_of(&self.frame) + half >= t {
                return true;
            }
            // O último antes do fim vale, se o instante pedido não existir.
            self.hold();
        }
        if got && self.has_held {
            unsafe { (a.util.av_frame_move_ref)(self.frame.ptr(), self.held.ptr()) };
            self.has_held = false;
            return true;
        }
        false
    }

    /// Converte o quadro `f` pros pedaços do layout.
    fn render(&mut self, f_held: bool, lay: &Layout) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
        if lay.w <= 0 || lay.h <= 0 || lay.w as i64 * lay.h as i64 > MAX_OUT_PIXELS {
            return None;
        }
        let fptr = if f_held { self.held.ptr() } else { self.frame.ptr() };
        let f = unsafe { &*fptr };
        let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(lay.w as u32, lay.h as u32);
        if f.format == AV_PIX_FMT_D3D11 {
            if let Some(g) = self.gpu.as_mut() {
                match g.render(f, lay.w, lay.h, &lay.pieces, buf.make_mut_bytes()) {
                    Ok(()) => return Some(buf),
                    Err(e) => {
                        log!("Player: conversao na placa falhou ({e}); caindo pra CPU");
                        self.gpu = None;
                    }
                }
            }
        }
        let a = self.a;
        let src: *const AVFrame = if f.format == AV_PIX_FMT_D3D11 {
            self.sw.unref();
            if unsafe { (a.util.av_hwframe_transfer_data)(self.sw.ptr(), fptr, 0) } < 0 {
                return None;
            }
            self.sw.ptr()
        } else {
            fptr
        };
        let s = unsafe { &*src };
        // Preto onde nenhum pedaço cobre (tarja entre monitores de alturas diferentes).
        for px in buf.make_mut_slice() {
            *px = Rgba8Pixel { r: 0, g: 0, b: 0, a: 255 };
        }
        let stride = lay.w as usize * 4;
        for (i, p) in lay.pieces.iter().enumerate() {
            // Recorte por deslocamento de ponteiro: em 4:2:0 o croma tem metade da
            // resolução, então o canto fica par (#51c).
            let (sx, sy) = (p.sx & !1, p.sy & !1);
            let (sw, sh) = ((p.sw + (p.sx - sx)).max(2) & !1, (p.sh + (p.sy - sy)).max(2) & !1);
            let mut sd = [std::ptr::null::<u8>(); 8];
            let ok = unsafe {
                match s.format {
                    AV_PIX_FMT_YUV420P | AV_PIX_FMT_YUVJ420P => {
                        sd[0] = s.data[0].add(sy as usize * s.linesize[0] as usize + sx as usize);
                        sd[1] = s.data[1].add((sy / 2) as usize * s.linesize[1] as usize + (sx / 2) as usize);
                        sd[2] = s.data[2].add((sy / 2) as usize * s.linesize[2] as usize + (sx / 2) as usize);
                        true
                    }
                    AV_PIX_FMT_NV12 => {
                        sd[0] = s.data[0].add(sy as usize * s.linesize[0] as usize + sx as usize);
                        sd[1] = s.data[1].add((sy / 2) as usize * s.linesize[1] as usize + sx as usize);
                        true
                    }
                    _ => false,
                }
            };
            // Formato sem recorte conhecido: o quadro inteiro no pedaço.
            let (cw, ch) = if ok { (sw, sh) } else { (s.width, s.height) };
            if !ok {
                for k in 0..8 {
                    sd[k] = s.data[k];
                }
            }
            let key = [cw, ch, s.format, p.dw, p.dh];
            if self.sws.get(i).is_none_or(|(_, k)| *k != key) {
                let Some(c) = Sws::new(cw, ch, s.format, p.dw, p.dh, AV_PIX_FMT_RGBA, SWS_BILINEAR) else { continue };
                if i < self.sws.len() {
                    self.sws[i] = (c, key);
                } else {
                    self.sws.push((c, key));
                }
            }
            let mut dst = [std::ptr::null_mut::<u8>(); 8];
            let mut dls = [0 as c_int; 8];
            let bytes = buf.make_mut_bytes();
            dst[0] = unsafe { bytes.as_mut_ptr().add(p.dy as usize * stride + p.dx as usize * 4) };
            dls[0] = stride as c_int;
            self.sws[i].0.scale_raw(&sd, &s.linesize, ch, &dst, &dls);
        }
        Some(buf)
    }
}

/// `AV_DISPOSITION_ATTACHED_PIC`: a "faixa de vídeo" é a capa embutida (MP3,
/// M4A) — um quadro só. Como vídeo, a reprodução acabava no mesmo instante.
const DISPOSITION_ATTACHED_PIC: i32 = 0x0400;

/// Primeira faixa de vídeo que não é capa.
unsafe fn first_moving_video(ic: *const AVFormatContext) -> Option<u32> {
    (0..nb_streams(ic)).find(|&i| {
        stream(ic, i).is_some_and(|s| {
            s.disposition & DISPOSITION_ATTACHED_PIC == 0
                && s.codecpar.as_ref().is_some_and(|cp| cp.codec_type == AVMEDIA_TYPE_VIDEO)
        })
    })
}

/// Erro do `Dec::open` pra arquivo sem faixa de vídeo (o `.mp3` da exportação
/// "só áudio", ou um com só a capa): o player toca só o som.
const NO_VIDEO: &str = "sem video";

/// Arquivo só de áudio: o som sai da thread de áudio como sempre; esta só faz
/// o papel que a de vídeo tem no relógio — duração, posição e fim.
fn run_audio_only(path: &str, rx: Receiver<VCmd>, shared: Arc<Shared>, out: Arc<dyn Fn(Out) + Send + Sync>) {
    let duration = Input::open(path)
        .ok()
        .map(|input| {
            let d = unsafe { container_duration_us(input.ptr()) } as f64 / 1e6;
            if d > 0.0 { d } else { (unsafe { scan_duration_by_packets(input.ptr()) }) as f64 / 1e6 }
        })
        .unwrap_or(0.0)
        .max(0.0);
    shared.duration.store(duration.to_bits(), Ordering::Relaxed);
    log!("Player: so audio, {duration:.1} s");
    out(Out::Info(Info { duration }));
    let mut playing = false;
    loop {
        let c = if playing {
            match rx.recv_timeout(Duration::from_millis(30)) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(_) => break,
            }
        } else {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        };
        match c {
            Some(VCmd::Play) => playing = true,
            Some(VCmd::Pause) => playing = false,
            Some(VCmd::Seek(t)) => shared.set_position(t),
            _ => {}
        }
        if playing {
            let t = shared.clock.now().max(0.0);
            if duration > 0.0 && t >= duration {
                playing = false;
                shared.set_position(duration);
                out(Out::Ended);
            } else {
                shared.set_position(t);
            }
        }
    }
}

/// Maior keyframe <= t (a lista vem ordenada do índice).
fn kf_at_or_before(kf: &[f64], t: f64) -> Option<f64> {
    let i = kf.partition_point(|&k| k <= t);
    (i > 0).then(|| kf[i - 1])
}

pub(super) fn run(path: &str, view: ViewSpec, rx: Receiver<VCmd>, shared: Arc<Shared>, out: Arc<dyn Fn(Out) + Send + Sync>) {
    let mut d = match Dec::open(path) {
        Ok(d) => d,
        Err(e) if e == NO_VIDEO => return run_audio_only(path, rx, shared, out),
        Err(e) => {
            log!("Player: video nao abriu ({e}): {path}");
            out(Out::Error(e));
            return;
        }
    };
    d.keyframes = crate::ffmpeg_ops::list_video_keyframes(path).map(|(t, _)| t).unwrap_or_default();
    shared.duration.store(d.duration.to_bits(), Ordering::Relaxed);
    log!(
        "Player: {}x{} @ {:.2} fps, {:.1} s, {} ({} keyframes)",
        d.src_w,
        d.src_h,
        d.fps,
        d.duration,
        if d.hw_frames { if d.gpu.is_some() { "placa + conversao na placa" } else { "placa" } } else { "software" },
        d.keyframes.len()
    );
    out(Out::Info(Info { duration: d.duration }));

    let mut view = view;
    let mut lay = layout(d.src_w, d.src_h, &view);
    let mut playing = false;
    let mut rate = 1.0f64;
    let mut ended = false;
    let frame_dur = 1.0 / d.fps;
    let mut cur = 0.0f64;
    let mut pending: Vec<VCmd> = Vec::new();

    let present = |d: &mut Dec, held: bool, lay: &Option<Layout>, pts: f64| {
        shared.set_position(pts);
        let Some(l) = lay else { return };
        if shared.ui_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        match d.render(held, l) {
            Some(buf) => out(Out::Frame { buf, x: l.x, y: l.y, view: (l.zoom, l.pan_x, l.pan_y) }),
            None => shared.ui_pending.store(false, Ordering::Release),
        }
    };

    // Primeiro quadro na tela já (antes do play).
    if d.seek(0.0) {
        cur = d.pts_of(&d.frame);
        d.hold();
        present(&mut d, true, &lay, cur);
    }

    'main: loop {
        // ---- comandos ----
        loop {
            let c = if playing || !pending.is_empty() {
                match rx.try_recv() {
                    Ok(c) => c,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break 'main,
                }
            } else {
                match rx.recv() {
                    Ok(c) => c,
                    Err(_) => break 'main,
                }
            };
            pending.push(c);
            if !playing {
                while let Ok(c) = rx.try_recv() {
                    pending.push(c);
                }
                break;
            }
        }
        let cmds: Vec<VCmd> = std::mem::take(&mut pending);
        // Só o último seek importa (arrastar a barra manda dezenas).
        let last_seek = cmds.iter().rposition(|c| matches!(c, VCmd::Seek(_) | VCmd::Step(_)));
        // Idem a vista: um arrasto rápido manda várias por lote, e refazer o
        // quadro a cada uma acumulava atraso.
        let last_view = cmds.iter().rposition(|c| matches!(c, VCmd::View(_)));
        for (i, c) in cmds.into_iter().enumerate() {
            match c {
                VCmd::Play => {
                    playing = true;
                    if ended {
                        ended = false;
                    }
                }
                VCmd::Pause => playing = false,
                VCmd::Rate(r) => rate = r,
                VCmd::View(v) => {
                    view = v;
                    lay = layout(d.src_w, d.src_h, &view);
                    if d.has_held && Some(i) == last_view {
                        // Pausado (ou entre quadros): o mostrado é refeito na vista nova.
                        shared.ui_pending.store(false, Ordering::Release);
                        present(&mut d, true, &lay, cur);
                    }
                }
                VCmd::Seek(t) if Some(i) == last_seek => {
                    ended = false;
                    if d.seek(t) {
                        cur = d.pts_of(&d.frame);
                        d.hold();
                        shared.ui_pending.store(false, Ordering::Release);
                        present(&mut d, true, &lay, cur);
                    }
                }
                VCmd::Step(dir) if Some(i) == last_seek => {
                    ended = false;
                    let ok = if dir > 0 {
                        // Pra frente: só o próximo quadro, sem seek.
                        d.next_frame()
                    } else {
                        d.seek((cur - frame_dur).max(0.0))
                    };
                    if ok {
                        cur = d.pts_of(&d.frame);
                        d.hold();
                        shared.ui_pending.store(false, Ordering::Release);
                        present(&mut d, true, &lay, cur);
                        shared.clock.rebase(cur);
                    }
                }
                _ => {}
            }
        }

        if !playing {
            continue;
        }

        // ---- por saltos: keyframe em keyframe, pelo relógio ----
        if rate >= TRICK_MIN_RATE && d.keyframes.len() >= 2 {
            let target = shared.clock.now();
            if target >= d.duration {
                playing = false;
                ended = true;
                shared.set_position(d.duration);
                out(Out::Ended);
                continue;
            }
            match kf_at_or_before(&d.keyframes, target) {
                Some(k) if k > cur + 1e-3 => {
                    // Um terço de quadro depois do keyframe: o instante exato pode
                    // cair um tique antes por arredondamento (#61).
                    if d.seek(k + frame_dur / 3.0) {
                        cur = d.pts_of(&d.frame);
                        d.hold();
                        present(&mut d, true, &lay, cur);
                    }
                }
                _ => {
                    // Nada novo ainda: espera um pouco ouvindo comandos.
                    match rx.recv_timeout(Duration::from_millis(5)) {
                        Ok(c) => pending.push(c),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(_) => break 'main,
                    }
                }
            }
            continue;
        }

        // ---- reprodução ----
        if !d.next_frame() {
            playing = false;
            ended = true;
            shared.set_position(d.duration.max(cur));
            out(Out::Ended);
            continue;
        }
        let pts = d.pts_of(&d.frame);
        let now = shared.clock.now();
        let late = now - pts;
        if late > frame_dur * 1.5 {
            // Muito atrás (o decoder não acompanha esta velocidade): pula pro
            // próximo keyframe em vez de ficar parado mostrando o passado.
            if rate > 1.0 && late > (rate * 0.25).max(1.0) {
                if let Some(k) = d.keyframes.iter().copied().find(|&k| k > now) {
                    d.frame.unref();
                    if d.seek(k) {
                        cur = d.pts_of(&d.frame);
                        d.hold();
                        present(&mut d, true, &lay, cur);
                    }
                    continue;
                }
            }
            // Atrasado: descarta antes da conversão, que é a etapa cara.
            d.frame.unref();
            cur = pts;
            continue;
        }
        // Espera o instante do quadro, acordando pra comandos.
        let started = Instant::now();
        loop {
            let wait = pts - shared.clock.now();
            if wait <= 0.0 || started.elapsed() > Duration::from_secs(2) {
                break;
            }
            match rx.recv_timeout(Duration::from_secs_f64(wait.min(0.005))) {
                Ok(c) => {
                    pending.push(c);
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(_) => break 'main,
            }
        }
        if pending.iter().any(|c| matches!(c, VCmd::Seek(_) | VCmd::Step(_) | VCmd::Pause)) {
            // Seek/pausa no meio da espera: este quadro já não vale (mas fica
            // como o mostrado se for pausa).
            if pending.iter().any(|c| matches!(c, VCmd::Pause)) {
                cur = pts;
                d.hold();
                present(&mut d, true, &lay, cur);
            } else {
                d.frame.unref();
            }
            continue;
        }
        cur = pts;
        d.hold();
        present(&mut d, true, &lay, cur);
    }
}

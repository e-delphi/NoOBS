//! Gerador das amostras de mídia (`tests/fixtures/media/`), com as próprias
//! DLLs do FFmpeg empacotadas: não há `ffmpeg.exe` na máquina, e as amostras
//! precisam ser parecidas com o que o OBS grava (MKV, AAC 48 kHz em várias
//! faixas com título, keyframe a cada 1 s).
//!
//! Duas etapas:
//! 1. [`encode`] codifica tudo NA MEMÓRIA — vídeo sintético (degradê + um
//!    quadrado que anda 1 px por quadro, então cada quadro é diferente e dá pra
//!    saber qual é) e um seno por faixa de áudio (0 Hz = silêncio);
//! 2. [`mux`] escreve uma JANELA desses pacotes num arquivo, com o relógio
//!    zerado no primeiro pacote (`rebase`) ou não.
//!
//! Regerar: `cargo test --release gerar_amostras -- --ignored`.

use std::ffi::CString;
use std::os::raw::c_void;

use crate::ffi::ffmpeg::*;

pub struct VideoSpec {
    pub encoder: &'static str,
    pub w: i32,
    pub h: i32,
    pub fps: i32,
    /// B-frames (só onde o encoder aceita `bf`).
    pub bframes: i32,
    /// Opções extras do encoder (qualidade, preset).
    pub opts: &'static [(&'static str, &'static str)],
    /// Muda o desenho (cor e altura do quadrado): duas amostras com sementes
    /// diferentes não têm nenhum quadro igual.
    pub seed: u8,
}

pub struct AudioSpec {
    pub title: &'static str,
    /// 0 = silêncio.
    pub freq: f32,
}

pub struct Spec {
    pub seconds: f64,
    pub video: Option<VideoSpec>,
    pub audio: Vec<AudioSpec>,
}

/// Um pacote codificado, no relógio do encoder.
#[derive(Clone)]
pub struct Pkt {
    pub stream: usize,
    pub pts: i64,
    pub dts: i64,
    pub duration: i64,
    pub key: bool,
    pub data: Vec<u8>,
}

pub struct StreamInfo {
    pub par: *mut AVCodecParameters,
    pub tb: AVRational,
    pub title: String,
    /// Vídeo: a taxa declarada no container (o `DefaultDuration` do MKV, como o OBS grava).
    pub fps: Option<AVRational>,
}

pub struct Encoded {
    pub streams: Vec<StreamInfo>,
    pub pkts: Vec<Pkt>,
}

impl Drop for Encoded {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            for s in &mut self.streams {
                unsafe { (a.codec.avcodec_parameters_free)(&mut s.par) };
            }
        }
    }
}

impl Encoded {
    /// Segundos de um pacote (pelo dts).
    pub fn sec(&self, p: &Pkt) -> f64 {
        let tb = self.streams[p.stream].tb;
        p.dts as f64 * tb.num as f64 / tb.den as f64
    }
}

fn check(rc: i32, what: &str) -> Result<(), String> {
    if rc < 0 {
        Err(format!("{what}: {}", av_err_str(rc)))
    } else {
        Ok(())
    }
}

/// Recebe tudo o que o encoder tem pronto.
unsafe fn drain(a: &FfmpegApi, ctx: *mut AVCodecContext, stream: usize, out: &mut Vec<Pkt>) -> Result<(), String> {
    let pkt = Packet::alloc().ok_or("av_packet_alloc")?;
    loop {
        let rc = (a.codec.avcodec_receive_packet)(ctx, pkt.ptr());
        if rc == AVERROR_EAGAIN || rc == AVERROR_EOF {
            return Ok(());
        }
        check(rc, "avcodec_receive_packet")?;
        let p = &*pkt.ptr();
        out.push(Pkt {
            stream,
            pts: p.pts,
            dts: p.dts,
            duration: p.duration,
            key: p.flags & AV_PKT_FLAG_KEY != 0,
            data: std::slice::from_raw_parts(p.data, p.size as usize).to_vec(),
        });
        (a.codec.av_packet_unref)(pkt.ptr());
    }
}

unsafe fn params_of(a: &FfmpegApi, ctx: *mut AVCodecContext) -> Result<*mut AVCodecParameters, String> {
    let par = (a.codec.avcodec_parameters_alloc)();
    if par.is_null() {
        return Err("avcodec_parameters_alloc".into());
    }
    check((a.codec.avcodec_parameters_from_context)(par, ctx), "parameters_from_context")?;
    Ok(par)
}

unsafe fn encode_video(a: &FfmpegApi, v: &VideoSpec, seconds: f64, stream: usize, out: &mut Vec<Pkt>) -> Result<StreamInfo, String> {
    let name = CString::new(v.encoder).unwrap();
    let codec = (a.codec.avcodec_find_encoder_by_name)(name.as_ptr());
    if codec.is_null() {
        return Err(format!("encoder {} indisponivel", v.encoder));
    }
    let ctx = CodecCtx::alloc(codec).ok_or("alloc_context")?;
    let mut par = (a.codec.avcodec_parameters_alloc)();
    (*par).codec_type = AVMEDIA_TYPE_VIDEO;
    (*par).codec_id = (*codec).id;
    (*par).width = v.w;
    (*par).height = v.h;
    (*par).format = AV_PIX_FMT_YUV420P;
    let rc = (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par);
    (a.codec.avcodec_parameters_free)(&mut par);
    check(rc, "parameters_to_context")?;
    let tb = AVRational { num: 1, den: v.fps };
    (a.util.av_opt_set_q)(ctx.ptr(), c"time_base".as_ptr(), tb, 0);
    (a.util.av_opt_set_q)(ctx.ptr(), c"framerate".as_ptr(), AVRational { num: v.fps, den: 1 }, 0);
    (a.util.av_opt_set)(ctx.ptr(), c"flags".as_ptr(), c"+global_header".as_ptr(), 0);
    let mut d = Dict::new();
    // Keyframe a cada 1 s, como o buffer do NoOBS grava (#62).
    d.set("g", &v.fps.to_string());
    if v.encoder.starts_with("libx264") {
        d.set("bf", &v.bframes.to_string());
    }
    for (k, val) in v.opts {
        d.set(k, val);
    }
    check((a.codec.avcodec_open2)(ctx.ptr(), codec, d.as_mut_ptr()), "avcodec_open2 (video)")?;

    let mut f = Frame::alloc().ok_or("frame")?;
    f.width = v.w;
    f.height = v.h;
    f.format = AV_PIX_FMT_YUV420P;
    check((a.util.av_frame_get_buffer)(f.ptr(), 0), "frame_get_buffer (video)")?;
    let frames = (seconds * v.fps as f64).round() as i64;
    for i in 0..frames {
        check((a.util.av_frame_make_writable)(f.ptr()), "make_writable")?;
        let (w, h) = (v.w as usize, v.h as usize);
        let ls = [f.linesize[0] as usize, f.linesize[1] as usize, f.linesize[2] as usize];
        // Degradê + quadrado que anda: cada quadro diferente do anterior.
        let sq = (i as usize * 2) % (w.saturating_sub(32).max(1));
        for y in 0..h {
            let row = std::slice::from_raw_parts_mut(f.data[0].add(y * ls[0]), w);
            for (x, px) in row.iter_mut().enumerate() {
                let top = h / 3 + v.seed as usize * 8;
                let inside = x >= sq && x < sq + 32 && y >= top && y < top + 32;
                *px = if inside { 235 } else { (16 + (x * 200 / w) as u32 + (y * 20 / h) as u32) as u8 };
            }
        }
        for p in 1..3 {
            for y in 0..h / 2 {
                let row = std::slice::from_raw_parts_mut(f.data[p].add(y * ls[p]), w / 2);
                row.fill(if p == 1 { 96 + (i % 64) as u8 } else { 160u8.wrapping_sub(v.seed.wrapping_mul(40)) });
            }
        }
        f.pts = i;
        check((a.codec.avcodec_send_frame)(ctx.ptr(), f.ptr()), "send_frame (video)")?;
        drain(a, ctx.ptr(), stream, out)?;
    }
    check((a.codec.avcodec_send_frame)(ctx.ptr(), std::ptr::null()), "flush (video)")?;
    drain(a, ctx.ptr(), stream, out)?;
    Ok(StreamInfo { par: params_of(a, ctx.ptr())?, tb, title: String::new(), fps: Some(AVRational { num: v.fps, den: 1 }) })
}

unsafe fn encode_audio(a: &FfmpegApi, au: &AudioSpec, seconds: f64, stream: usize, out: &mut Vec<Pkt>) -> Result<StreamInfo, String> {
    const RATE: i32 = 48_000;
    let codec = (a.codec.avcodec_find_encoder_by_name)(c"aac".as_ptr());
    if codec.is_null() {
        return Err("encoder aac indisponivel".into());
    }
    let ctx = CodecCtx::alloc(codec).ok_or("alloc_context")?;
    // Estéreo, ordem nativa (FL|FR), como o OBS grava.
    let layout = AVChannelLayout { order: 1, nb_channels: 2, u: 3, opaque: std::ptr::null_mut() };
    let mut par = (a.codec.avcodec_parameters_alloc)();
    (*par).codec_type = AVMEDIA_TYPE_AUDIO;
    (*par).codec_id = (*codec).id;
    (*par).format = AV_SAMPLE_FMT_FLTP;
    (*par).sample_rate = RATE;
    (*par).bit_rate = 32_000;
    std::ptr::copy_nonoverlapping(&layout, &mut (*par).ch_layout, 1);
    let rc = (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par);
    std::ptr::write_bytes(&mut (*par).ch_layout as *mut AVChannelLayout, 0, 1);
    (a.codec.avcodec_parameters_free)(&mut par);
    check(rc, "parameters_to_context (audio)")?;
    let tb = AVRational { num: 1, den: RATE };
    (a.util.av_opt_set_q)(ctx.ptr(), c"time_base".as_ptr(), tb, 0);
    (a.util.av_opt_set)(ctx.ptr(), c"flags".as_ptr(), c"+global_header".as_ptr(), 0);
    check(ctx.open(codec), "avcodec_open2 (audio)")?;
    let mut fs = 0i64;
    (a.util.av_opt_get_int)(ctx.ptr(), c"frame_size".as_ptr(), 0, &mut fs);
    let fs = if fs > 0 { fs } else { 1024 };

    let total = (seconds * RATE as f64).round() as i64;
    let mut n = 0i64;
    while n < total {
        let count = fs.min(total - n);
        let mut f = Frame::alloc().ok_or("frame")?;
        f.nb_samples = count as i32;
        f.format = AV_SAMPLE_FMT_FLTP;
        ((f.ptr() as *mut u8).add(OFFS_FRAME_SAMPLE_RATE) as *mut i32).write_unaligned(RATE);
        check(
            (a.util.av_channel_layout_copy)((f.ptr() as *mut u8).add(OFFS_FRAME_CH_LAYOUT) as *mut c_void, &layout as *const _ as *const c_void),
            "ch_layout",
        )?;
        check((a.util.av_frame_get_buffer)(f.ptr(), 0), "frame_get_buffer (audio)")?;
        for ch in 0..2 {
            let buf = std::slice::from_raw_parts_mut(f.data[ch] as *mut f32, count as usize);
            for (k, s) in buf.iter_mut().enumerate() {
                let t = (n + k as i64) as f32 / RATE as f32;
                *s = if au.freq > 0.0 { 0.3 * (2.0 * std::f32::consts::PI * au.freq * t).sin() } else { 0.0 };
            }
        }
        f.pts = n;
        check((a.codec.avcodec_send_frame)(ctx.ptr(), f.ptr()), "send_frame (audio)")?;
        drain(a, ctx.ptr(), stream, out)?;
        n += count;
    }
    check((a.codec.avcodec_send_frame)(ctx.ptr(), std::ptr::null()), "flush (audio)")?;
    drain(a, ctx.ptr(), stream, out)?;
    Ok(StreamInfo { par: params_of(a, ctx.ptr())?, tb, title: au.title.to_owned(), fps: None })
}

/// Codifica a amostra inteira na memória.
pub fn encode(spec: &Spec) -> Result<Encoded, String> {
    let a = api()?;
    let mut streams = Vec::new();
    let mut pkts = Vec::new();
    unsafe {
        if let Some(v) = &spec.video {
            streams.push(encode_video(a, v, spec.seconds, streams.len(), &mut pkts)?);
        }
        for au in &spec.audio {
            streams.push(encode_audio(a, au, spec.seconds, streams.len(), &mut pkts)?);
        }
    }
    let mut enc = Encoded { streams, pkts };
    // Ordem de escrita: pelo instante (dts), vídeo antes do áudio no empate.
    let mut keyed: Vec<(f64, usize, Pkt)> = enc.pkts.drain(..).map(|p| (0.0, p.stream, p)).collect();
    for k in &mut keyed {
        let tb = enc.streams[k.2.stream].tb;
        k.0 = k.2.dts as f64 * tb.num as f64 / tb.den as f64;
    }
    keyed.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap().then(x.1.cmp(&y.1)));
    enc.pkts = keyed.into_iter().map(|k| k.2).collect();
    Ok(enc)
}

/// Escreve os pacotes `keep` (em ordem) num arquivo. `rebase`: zera o relógio de
/// CADA faixa no primeiro pacote dela. `trailer:
/// false` fecha sem o `av_write_trailer` — gravação interrompida (queda de
/// energia): sem a duração no cabeçalho, sem índice, e o último cluster perdido.
pub fn mux(enc: &Encoded, path: &str, format: &str, keep: &[&Pkt], rebase: bool, trailer: bool) -> Result<(), String> {
    let a = api()?;
    let _ = std::fs::remove_file(path);
    unsafe {
        let fmt = CString::new(format).unwrap();
        let cpath = CString::new(path).unwrap();
        let mut oc: *mut AVFormatContext = std::ptr::null_mut();
        check((a.format.avformat_alloc_output_context2)(&mut oc, std::ptr::null(), fmt.as_ptr(), cpath.as_ptr()), "alloc_output")?;
        let result = (|| -> Result<(), String> {
            let mut outs = Vec::new();
            for s in &enc.streams {
                let st = (a.format.avformat_new_stream)(oc, std::ptr::null());
                if st.is_null() {
                    return Err("new_stream".into());
                }
                check((a.codec.avcodec_parameters_copy)((*st).codecpar, s.par), "parameters_copy")?;
                (*(*st).codecpar).codec_tag = 0;
                (*st).time_base = s.tb;
                if let Some(r) = s.fps {
                    (*st).avg_frame_rate = r;
                }
                if !s.title.is_empty() {
                    let t = CString::new(s.title.as_str()).unwrap();
                    (a.util.av_dict_set)(&mut (*st).metadata, c"title".as_ptr(), t.as_ptr(), 0);
                }
                outs.push(st);
            }
            let mut pb: *mut c_void = std::ptr::null_mut();
            check((a.format.avio_open2)(&mut pb, cpath.as_ptr(), AVIO_FLAG_WRITE, std::ptr::null(), std::ptr::null_mut()), "avio_open2")?;
            set_pb(oc, pb);
            check((a.format.avformat_write_header)(oc, std::ptr::null_mut()), "write_header")?;
            let mut base: Vec<Option<i64>> = vec![None; enc.streams.len()];
            let pkt = Packet::alloc().ok_or("packet")?;
            for p in keep {
                let b = if rebase { *base[p.stream].get_or_insert(p.dts) } else { 0 };
                let raw = pkt.ptr();
                (*raw).data = p.data.as_ptr() as *mut u8;
                (*raw).size = p.data.len() as i32;
                (*raw).buf = std::ptr::null_mut();
                (*raw).stream_index = p.stream as i32;
                (*raw).pts = p.pts - b;
                (*raw).dts = p.dts - b;
                (*raw).duration = p.duration;
                (*raw).flags = if p.key { AV_PKT_FLAG_KEY } else { 0 };
                let st = outs[p.stream];
                (a.codec.av_packet_rescale_ts)(raw, enc.streams[p.stream].tb, (*st).time_base);
                check((a.format.av_interleaved_write_frame)(oc, raw), "write_frame")?;
            }
            if trailer {
                check((a.format.av_write_trailer)(oc), "write_trailer")?;
            }
            Ok(())
        })();
        let mut pb = field_pb(oc);
        if !pb.is_null() {
            (a.format.avio_closep)(&mut pb);
        }
        (a.format.avformat_free_context)(oc);
        result
    }
}

unsafe fn field_pb(oc: *mut AVFormatContext) -> *mut c_void {
    ((oc as *const u8).add(OFFS_PB) as *const *mut c_void).read_unaligned()
}

/// Todos os pacotes.
pub fn all(enc: &Encoded) -> Vec<&Pkt> {
    enc.pkts.iter().collect()
}

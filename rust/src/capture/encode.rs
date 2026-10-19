//! Codificação. O motor fala com o trait `VideoEncoder`; a implementação é o
//! libavcodec, o mesmo da exportação (nomes `h264_amf`, `hevc_nvenc`,
//! `libx264`…, e as MESMAS tabelas de qualidade calibradas,
//! `ffmpeg_export::apply_quality_options`, #51b).
//!
//! Dois caminhos dentro da mesma implementação (o chamador não sabe qual):
//! - **hardware** (`*_amf`, `*_nvenc`): o quadro NV12 é copiado pra um quadro
//!   do pool D3D11 do FFmpeg, montado sobre o NOSSO dispositivo — não sai da
//!   placa (como os `*_texture_*` do OBS);
//! - **software** (`lib*`): o NV12 desce pra memória por uma textura de
//!   staging e vai num quadro comum.

use std::ffi::c_void;

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Texture2D, D3D11_BIND_FLAG, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};

use super::gpu::Device;
use crate::ffi::ffmpeg::*;
use crate::log;

/// Parâmetros de um stream (o que o muxer precisa pra criar a faixa).
pub struct StreamInfo {
    par: *mut AVCodecParameters,
    pub time_base: AVRational,
    pub frame_rate: AVRational,
    pub title: String,
}

impl StreamInfo {
    pub fn params(&self) -> *const AVCodecParameters {
        self.par
    }

    /// Cópia independente (cada trecho salvo do buffer abre o próprio arquivo).
    pub fn try_clone(&self) -> Result<StreamInfo, String> {
        let a = api()?;
        let par = unsafe { (a.codec.avcodec_parameters_alloc)() };
        if par.is_null() {
            return Err("avcodec_parameters_alloc".into());
        }
        let rc = unsafe { (a.codec.avcodec_parameters_copy)(par, self.par) };
        let c = StreamInfo { par, time_base: self.time_base, frame_rate: self.frame_rate, title: self.title.clone() };
        if rc < 0 {
            return Err(format!("avcodec_parameters_copy: {}", av_err_str(rc)));
        }
        Ok(c)
    }
}

impl Drop for StreamInfo {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.codec.avcodec_parameters_free)(&mut self.par) };
        }
    }
}

// SAFETY: o AVCodecParameters é dono só deste valor; só é lido pelo muxer.
unsafe impl Send for StreamInfo {}

/// Um encoder de vídeo que recebe quadros NV12 na GPU.
pub trait VideoEncoder: Send {
    fn name(&self) -> &str;
    fn stream_info(&self) -> Result<StreamInfo, String>;
    /// Codifica o quadro `pts` (em 1/fps). Os pacotes prontos vão pra `out`.
    /// `key` = este quadro TEM que ser keyframe (corte do buffer, retomada da pausa).
    fn encode(&mut self, dev: &Device, nv12: &ID3D11Texture2D, pts: i64, key: bool, out: &mut Vec<Packet>) -> Result<(), String>;
    /// Fim: esvazia o que o encoder segurou.
    fn flush(&mut self, out: &mut Vec<Packet>);
}

/// Opções de abertura.
#[derive(Debug, Clone)]
pub struct VideoEncoderConfig {
    /// Nome do libavcodec (`h264_amf`, `libx264`…).
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Qualidade constante, escala 0..51 (calibrada por encoder).
    pub crf: i32,
    /// Intervalo de keyframe em segundos.
    pub keyframe_sec: u32,
}

/// Formato do quadro em MEMÓRIA (caminho de software): NV12, menos o SVT-AV1,
/// que só aceita YUV420P (planos U e V separados).
fn sw_pix_fmt(codec: &str) -> i32 {
    if codec.eq_ignore_ascii_case("libsvtav1") {
        AV_PIX_FMT_YUV420P
    } else {
        AV_PIX_FMT_NV12
    }
}

fn is_hw(codec: &str) -> bool {
    let c = codec.to_ascii_lowercase();
    c.ends_with("_amf") || c.ends_with("_nvenc")
}

unsafe fn rd<T: Copy>(base: *const c_void, off: usize) -> T {
    ((base as *const u8).add(off) as *const T).read_unaligned()
}
unsafe fn wr<T>(base: *mut c_void, off: usize, v: T) {
    ((base as *mut u8).add(off) as *mut T).write_unaligned(v)
}

/// `AVBufferRef*` com unref no Drop.
struct BufRef(*mut c_void);
impl BufRef {
    fn data(&self) -> *mut c_void {
        unsafe { rd(self.0, OFFS_BUFREF_DATA) }
    }
}
impl Drop for BufRef {
    fn drop(&mut self) {
        if !self.0.is_null() {
            if let Ok(a) = api() {
                unsafe { (a.util.av_buffer_unref)(&mut self.0) };
            }
        }
    }
}

/// Device D3D11 do FFmpeg montado sobre o NOSSO `ID3D11Device` + pool de
/// quadros NV12 na GPU. Os offsets (`OFFS_HW*`) são do avutil-59 e cada um é
/// conferido aqui contra um valor conhecido antes de escrever.
struct HwFrames {
    _device: BufRef,
    frames: BufRef,
}

impl HwFrames {
    fn new(device: &ID3D11Device, w: u32, h: u32) -> Result<Self, String> {
        let a = api()?;
        unsafe {
            let dev_ref = BufRef((a.util.av_hwdevice_ctx_alloc)(AV_HWDEVICE_TYPE_D3D11VA));
            if dev_ref.0.is_null() {
                return Err("av_hwdevice_ctx_alloc(d3d11va) falhou".into());
            }
            let devctx = dev_ref.data();
            let ty: i32 = rd(devctx, OFFS_HWDEV_TYPE);
            if ty != AV_HWDEVICE_TYPE_D3D11VA {
                return Err(format!("AVHWDeviceContext.type em {OFFS_HWDEV_TYPE} = {ty} (esperado {AV_HWDEVICE_TYPE_D3D11VA}): offsets do avutil mudaram"));
            }
            let hwctx: *mut c_void = rd(devctx, OFFS_HWDEV_HWCTX);
            if hwctx.is_null() {
                return Err("AVHWDeviceContext.hwctx nulo".into());
            }
            // AVD3D11VADeviceContext.device: o FFmpeg dá Release no fim — a
            // referência entregue é nossa (clone = AddRef).
            wr(hwctx, 0, device.clone().into_raw());
            let rc = (a.util.av_hwdevice_ctx_init)(dev_ref.0);
            if rc < 0 {
                return Err(format!("av_hwdevice_ctx_init: {}", av_err_str(rc)));
            }
            let frames = BufRef((a.util.av_hwframe_ctx_alloc)(dev_ref.0));
            if frames.0.is_null() {
                return Err("av_hwframe_ctx_alloc falhou".into());
            }
            let fr = frames.data();
            let fr_dev: *mut c_void = rd(fr, OFFS_HWFR_DEVICE_REF);
            if fr_dev.is_null() || rd::<*mut c_void>(fr_dev, OFFS_BUFREF_DATA) != devctx {
                return Err("AVHWFramesContext.device_ref não confere: offsets do avutil mudaram".into());
            }
            wr(fr, OFFS_HWFR_FORMAT, AV_PIX_FMT_D3D11);
            wr(fr, OFFS_HWFR_SW_FORMAT, AV_PIX_FMT_NV12);
            wr(fr, OFFS_HWFR_WIDTH, w as i32);
            wr(fr, OFFS_HWFR_HEIGHT, h as i32);
            // Poucos quadros em voo: o encoder segura alguns enquanto codifica.
            wr(fr, OFFS_HWFR_POOL_SIZE, 8i32);
            let d3dfr: *mut c_void = rd(fr, OFFS_HWFR_HWCTX);
            if !d3dfr.is_null() {
                // Sem bind flag o D3D11 recusa a textura NV12 em array (E_INVALIDARG);
                // RENDER_TARGET é o que o FFmpeg recomenda pra entrada de encoder.
                wr(d3dfr, OFFS_D3D11FR_BIND_FLAGS, windows::Win32::Graphics::Direct3D11::D3D11_BIND_RENDER_TARGET.0 as u32);
            }
            let rc = (a.util.av_hwframe_ctx_init)(frames.0);
            if rc < 0 {
                return Err(format!("av_hwframe_ctx_init {w}x{h}: {}", av_err_str(rc)));
            }
            Ok(Self { _device: dev_ref, frames })
        }
    }
}

/// Textura de staging pra descer o NV12 (caminho de software).
struct Readback {
    staging: ID3D11Texture2D,
}

/// Encoder de vídeo do libavcodec.
pub struct AvVideoEncoder {
    name: String,
    ctx: CodecCtx,
    cfg: VideoEncoderConfig,
    hw: Option<HwFrames>,
    readback: Option<Readback>,
    frame: Frame,
    pkt: Packet,
    /// O encoder obedece a um quadro marcado como I. O AMF do FFmpeg 7.1 NÃO
    /// (o `amfenc.c` só lê o tipo do pacote de SAÍDA; não há `forced_idr`):
    /// pra ele o keyframe forçado é esvaziar e reabrir o codificador — que
    /// sempre começa por IDR.
    can_force_key: bool,
    /// SPS/PPS do 1º contexto: a reabertura tem que produzir o mesmo (é o que
    /// está no cabeçalho do MKV).
    extradata: Vec<u8>,
    pub restarts: u32,
    /// Quadros já entregues ao encoder (o 1º sempre sai keyframe: não precisa forçar).
    sent: u64,
}

// SAFETY: o contexto do codec e os quadros são usados por UMA thread (a de vídeo).
unsafe impl Send for AvVideoEncoder {}

impl AvVideoEncoder {
    /// Contexto aberto do codec (o pool de quadros da GPU, se houver, é de quem chama).
    fn open_ctx(cfg: &VideoEncoderConfig, hw_frames: Option<&HwFrames>) -> Result<CodecCtx, String> {
        let a = api()?;
        let codec = unsafe { (a.codec.avcodec_find_encoder_by_name)(to_utf8(&cfg.codec).as_ptr()) };
        if codec.is_null() {
            return Err(format!("encoder \"{}\" não existe no avcodec", cfg.codec));
        }
        let hw = hw_frames.is_some();
        let ctx = CodecCtx::alloc(codec).ok_or("avcodec_alloc_context3 falhou")?;
        unsafe {
            // Dimensões pelo AVCodecParameters, nunca av_opt "width" (#28).
            let mut par = (a.codec.avcodec_parameters_alloc)();
            if par.is_null() {
                return Err("avcodec_parameters_alloc".into());
            }
            (*par).codec_type = AVMEDIA_TYPE_VIDEO;
            (*par).codec_id = (*codec).id;
            (*par).width = cfg.width as i32;
            (*par).height = cfg.height as i32;
            (*par).format = if hw { AV_PIX_FMT_D3D11 } else { sw_pix_fmt(&cfg.codec) };
            let r = (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par);
            (a.codec.avcodec_parameters_free)(&mut par);
            if r < 0 {
                return Err(format!("avcodec_parameters_to_context: {}", av_err_str(r)));
            }
            (a.util.av_opt_set_q)(ctx.ptr(), c"time_base".as_ptr(), AVRational { num: 1, den: cfg.fps as i32 }, 0);
            (a.util.av_opt_set_q)(ctx.ptr(), c"framerate".as_ptr(), AVRational { num: cfg.fps as i32, den: 1 }, 0);
            // O MKV quer o SPS/PPS no cabeçalho (CodecPrivate).
            (a.util.av_opt_set)(ctx.ptr(), c"flags".as_ptr(), c"+global_header".as_ptr(), 0);
            // BT.709 faixa limitada — o que o conversor da GPU produz.
            (a.util.av_opt_set)(ctx.ptr(), c"color_primaries".as_ptr(), c"bt709".as_ptr(), 0);
            (a.util.av_opt_set)(ctx.ptr(), c"color_trc".as_ptr(), c"bt709".as_ptr(), 0);
            (a.util.av_opt_set)(ctx.ptr(), c"colorspace".as_ptr(), c"bt709".as_ptr(), 0);
            (a.util.av_opt_set)(ctx.ptr(), c"color_range".as_ptr(), c"tv".as_ptr(), 0);
            match hw_frames {
                Some(h) => set_hw_frames_ctx(ctx.ptr(), h.frames.0)?,
                None => ctx.set_threads_auto(),
            }
        }
        let mut opts = Dict::new();
        if !crate::ffmpeg_export::apply_quality_options(&mut opts, &cfg.codec, cfg.crf, 0) {
            return Err(format!("sem controle de qualidade constante pra \"{}\"", cfg.codec));
        }
        opts.set("g", &(cfg.fps * cfg.keyframe_sec.max(1)).to_string());
        // Quadro marcado como I vira IDR de verdade (x264/NVENC). O AMF não tem
        // a opção: ver `can_force_key`.
        if !cfg.codec.ends_with("_amf") {
            opts.set("forced-idr", "1");
        }
        let rc = unsafe { (a.codec.avcodec_open2)(ctx.ptr(), codec, opts.as_mut_ptr()) };
        let left = opts.leftover_keys();
        if !left.is_empty() {
            log!("Captura: \"{}\" ignorou as opções {left:?}", cfg.codec);
        }
        if rc < 0 {
            return Err(format!("avcodec_open2(\"{}\"): {}", cfg.codec, av_err_str(rc)));
        }
        Ok(ctx)
    }

    pub fn open(dev: &Device, cfg: &VideoEncoderConfig) -> Result<Self, String> {
        let hw = is_hw(&cfg.codec);
        let hw_frames = if hw { Some(HwFrames::new(&dev.device, cfg.width, cfg.height)?) } else { None };
        let ctx = Self::open_ctx(cfg, hw_frames.as_ref())?;
        let readback = if hw {
            None
        } else {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: cfg.width,
                Height: cfg.height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: D3D11_BIND_FLAG(0).0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut t = None;
            unsafe { dev.device.CreateTexture2D(&desc, None, Some(&mut t)) }.map_err(|e| format!("staging NV12: {e}"))?;
            Some(Readback { staging: t.ok_or("sem staging")? })
        };
        log!("Captura: encoder {} {}x{} @{} fps crf={} ({})", cfg.codec, cfg.width, cfg.height, cfg.fps, cfg.crf, if hw { "quadro na GPU" } else { "copia pra memoria" });
        Ok(Self {
            name: cfg.codec.clone(),
            cfg: cfg.clone(),
            hw: hw_frames,
            readback,
            frame: Frame::alloc().ok_or("av_frame_alloc")?,
            pkt: Packet::alloc().ok_or("av_packet_alloc")?,
            can_force_key: !cfg.codec.ends_with("_amf"),
            extradata: extradata_of(&ctx),
            restarts: 0,
            sent: 0,
            ctx,
        })
    }

    /// Keyframe sem suporte do encoder: esvazia o que ele segurava e reabre.
    fn restart(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        let a = api()?;
        unsafe { (a.codec.avcodec_send_frame)(self.ctx.ptr(), std::ptr::null()) };
        self.drain(out)?;
        self.ctx = Self::open_ctx(&self.cfg, self.hw.as_ref())?;
        self.restarts += 1;
        let ed = extradata_of(&self.ctx);
        if ed != self.extradata {
            log!("Captura: \"{}\" reabriu com SPS/PPS DIFERENTE do cabeçalho ({} → {} bytes).", self.name, self.extradata.len(), ed.len());
        }
        Ok(())
    }

    fn drain(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        let a = api()?;
        loop {
            let rc = unsafe { (a.codec.avcodec_receive_packet)(self.ctx.ptr(), self.pkt.ptr()) };
            if rc == AVERROR_EAGAIN || rc == AVERROR_EOF {
                return Ok(());
            }
            if rc < 0 {
                return Err(format!("avcodec_receive_packet: {}", av_err_str(rc)));
            }
            // Pacote sem duração (encoders de hardware): um quadro.
            if self.pkt.duration <= 0 {
                self.pkt.duration = 1;
            }
            if let Some(p) = self.pkt.clone_pkt() {
                out.push(p);
            }
            self.pkt.unref();
        }
    }

    fn fill_hw(&mut self, dev: &Device, nv12: &ID3D11Texture2D) -> Result<(), String> {
        let a = api()?;
        let h = self.hw.as_ref().ok_or("sem pool de quadros")?;
        self.frame.unref();
        let rc = unsafe { (a.util.av_hwframe_get_buffer)(h.frames.0, self.frame.ptr(), 0) };
        if rc < 0 {
            return Err(format!("av_hwframe_get_buffer: {}", av_err_str(rc)));
        }
        let raw = self.frame.data[0] as *mut c_void;
        let index = self.frame.data[1] as usize as u32;
        let tex = unsafe { ID3D11Texture2D::from_raw_borrowed(&raw) }.ok_or("quadro D3D11 sem textura")?;
        unsafe { dev.ctx.CopySubresourceRegion(tex, index, 0, 0, 0, nv12, 0, None) };
        Ok(())
    }

    fn fill_sw(&mut self, dev: &Device, nv12: &ID3D11Texture2D) -> Result<(), String> {
        let a = api()?;
        let rb = self.readback.as_ref().ok_or("sem staging")?;
        let (w, h) = (self.cfg.width as usize, self.cfg.height as usize);
        self.frame.unref();
        self.frame.width = w as i32;
        self.frame.height = h as i32;
        let fmt = sw_pix_fmt(&self.cfg.codec);
        self.frame.format = fmt;
        let rc = unsafe { (a.util.av_frame_get_buffer)(self.frame.ptr(), 0) };
        if rc < 0 {
            return Err(format!("av_frame_get_buffer: {}", av_err_str(rc)));
        }
        unsafe {
            dev.ctx.CopyResource(&rb.staging, nv12);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            dev.ctx.Map(&rb.staging, 0, D3D11_MAP_READ, 0, Some(&mut m)).map_err(|e| format!("Map staging: {e}"))?;
            let src = m.pData as *const u8;
            let pitch = m.RowPitch as usize;
            // NV12 mapeado: o plano UV vem logo depois das `h` linhas do Y.
            let uv = src.add(pitch * h);
            let (ly, dy) = (self.frame.linesize[0] as usize, self.frame.data[0]);
            for y in 0..h {
                std::ptr::copy_nonoverlapping(src.add(y * pitch), dy.add(y * ly), w);
            }
            if fmt == AV_PIX_FMT_NV12 {
                let (luv, duv) = (self.frame.linesize[1] as usize, self.frame.data[1]);
                for y in 0..h / 2 {
                    std::ptr::copy_nonoverlapping(uv.add(y * pitch), duv.add(y * luv), w);
                }
            } else {
                // YUV420P: separa o UV intercalado em U e V.
                let (lu, du, lv, dv) = (self.frame.linesize[1] as usize, self.frame.data[1], self.frame.linesize[2] as usize, self.frame.data[2]);
                for y in 0..h / 2 {
                    let row = uv.add(y * pitch);
                    for x in 0..w / 2 {
                        *du.add(y * lu + x) = *row.add(x * 2);
                        *dv.add(y * lv + x) = *row.add(x * 2 + 1);
                    }
                }
            }
            dev.ctx.Unmap(&rb.staging, 0);
        }
        Ok(())
    }
}

/// O `extradata` (SPS/PPS) que o encoder publicou.
fn extradata_of(ctx: &CodecCtx) -> Vec<u8> {
    let Ok(a) = api() else { return Vec::new() };
    unsafe {
        let mut par = (a.codec.avcodec_parameters_alloc)();
        if par.is_null() {
            return Vec::new();
        }
        let v = if (a.codec.avcodec_parameters_from_context)(par, ctx.ptr()) >= 0 && !(*par).extradata.is_null() {
            std::slice::from_raw_parts((*par).extradata, (*par).extradata_size.max(0) as usize).to_vec()
        } else {
            Vec::new()
        };
        (a.codec.avcodec_parameters_free)(&mut par);
        v
    }
}

/// `AVCodecContext.hw_frames_ctx` por offset, conferido pela âncora
/// `extra_hw_frames` (AVOption) 20 bytes depois, e exigindo o campo vazio.
unsafe fn set_hw_frames_ctx(ctx: *mut AVCodecContext, frames: *mut c_void) -> Result<(), String> {
    let a = api()?;
    const MARK: i32 = 0x2468;
    (a.util.av_opt_set_int)(ctx, c"extra_hw_frames".as_ptr(), MARK as i64, 0);
    let got: i32 = rd(ctx, OFFS_CODECCTX_EXTRA_HW_FRAMES);
    (a.util.av_opt_set_int)(ctx, c"extra_hw_frames".as_ptr(), -1, 0);
    let cur: *mut c_void = rd(ctx, OFFS_CODECCTX_HW_FRAMES_CTX);
    if got != MARK || !cur.is_null() {
        return Err(format!("AVCodecContext: extra_hw_frames fora de {OFFS_CODECCTX_EXTRA_HW_FRAMES} ou hw_frames_ctx ocupado — offsets do avcodec mudaram"));
    }
    let r = (a.util.av_buffer_ref)(frames);
    if r.is_null() {
        return Err("av_buffer_ref(frames)".into());
    }
    wr(ctx, OFFS_CODECCTX_HW_FRAMES_CTX, r);
    Ok(())
}

impl VideoEncoder for AvVideoEncoder {
    fn name(&self) -> &str {
        &self.name
    }

    fn stream_info(&self) -> Result<StreamInfo, String> {
        let a = api()?;
        let par = unsafe { (a.codec.avcodec_parameters_alloc)() };
        if par.is_null() {
            return Err("avcodec_parameters_alloc".into());
        }
        let rc = unsafe { (a.codec.avcodec_parameters_from_context)(par, self.ctx.ptr()) };
        let info = StreamInfo {
            par,
            time_base: AVRational { num: 1, den: self.cfg.fps as i32 },
            frame_rate: AVRational { num: self.cfg.fps as i32, den: 1 },
            title: String::new(),
        };
        if rc < 0 {
            return Err(format!("avcodec_parameters_from_context: {}", av_err_str(rc)));
        }
        // O stream leva o formato de SOFTWARE (o MKV não sabe o que é "d3d11").
        unsafe { (*par).format = if self.hw.is_some() { AV_PIX_FMT_NV12 } else { sw_pix_fmt(&self.cfg.codec) } };
        Ok(info)
    }

    fn encode(&mut self, dev: &Device, nv12: &ID3D11Texture2D, pts: i64, key: bool, out: &mut Vec<Packet>) -> Result<(), String> {
        // O 1º quadro de um encoder recém-aberto já é keyframe: reabrir ali seria
        // só custo (e no 1º AMF do processo custa quadros atrasados).
        if key && !self.can_force_key && self.sent > 0 {
            self.restart(out)?;
            self.sent = 0;
        }
        self.sent += 1;
        if self.hw.is_some() {
            self.fill_hw(dev, nv12)?;
        } else {
            self.fill_sw(dev, nv12)?;
        }
        self.frame.pts = pts;
        // AV_PICTURE_TYPE_I (1) força keyframe; NONE (0) deixa o encoder decidir.
        self.frame.pict_type = if key { 1 } else { 0 };
        let a = api()?;
        let rc = unsafe { (a.codec.avcodec_send_frame)(self.ctx.ptr(), self.frame.ptr()) };
        self.frame.unref();
        if rc < 0 && rc != AVERROR_EAGAIN {
            return Err(format!("avcodec_send_frame: {}", av_err_str(rc)));
        }
        self.drain(out)
    }

    fn flush(&mut self, out: &mut Vec<Packet>) {
        if let Ok(a) = api() {
            unsafe { (a.codec.avcodec_send_frame)(self.ctx.ptr(), std::ptr::null()) };
            if let Err(e) = self.drain(out) {
                log!("Captura: flush do encoder: {e}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Áudio
// ---------------------------------------------------------------------------

/// Um encoder de áudio que recebe blocos estéreo intercalados de 1024 amostras.
pub trait AudioEncoder: Send {
    fn stream_info(&self) -> Result<StreamInfo, String>;
    /// `pts` em amostras (time_base 1/48000).
    fn encode(&mut self, interleaved: &[f32], pts: i64, out: &mut Vec<Packet>) -> Result<(), String>;
    fn flush(&mut self, out: &mut Vec<Packet>);
}

/// AAC do libavcodec (o `ffmpeg_aac` do OBS).
pub struct AvAacEncoder {
    ctx: CodecCtx,
    frame: Frame,
    pkt: Packet,
    title: String,
}

// SAFETY: usado por uma thread (a de áudio).
unsafe impl Send for AvAacEncoder {}

/// `AVChannelLayout` estéreo nativo (`AV_CHANNEL_ORDER_NATIVE`, FL|FR).
fn stereo() -> AVChannelLayout {
    AVChannelLayout { order: 1, nb_channels: 2, u: 3, opaque: std::ptr::null_mut() }
}

impl AvAacEncoder {
    /// `kbps` por faixa (a aba Áudio: 96..320).
    pub fn open(kbps: u32, title: &str) -> Result<Self, String> {
        let a = api()?;
        let codec = unsafe { (a.codec.avcodec_find_encoder_by_name)(c"aac".as_ptr()) };
        if codec.is_null() {
            return Err("encoder \"aac\" não existe no avcodec".into());
        }
        let ctx = CodecCtx::alloc(codec).ok_or("avcodec_alloc_context3")?;
        unsafe {
            let mut par = (a.codec.avcodec_parameters_alloc)();
            if par.is_null() {
                return Err("avcodec_parameters_alloc".into());
            }
            (*par).codec_type = AVMEDIA_TYPE_AUDIO;
            (*par).codec_id = (*codec).id;
            (*par).format = AV_SAMPLE_FMT_FLTP;
            (*par).sample_rate = super::audio::SAMPLE_RATE as i32;
            (*par).bit_rate = kbps as i64 * 1000;
            (*par).ch_layout = stereo();
            let rc = (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par);
            (*par).ch_layout = AVChannelLayout { order: 0, nb_channels: 0, u: 0, opaque: std::ptr::null_mut() };
            (a.codec.avcodec_parameters_free)(&mut par);
            if rc < 0 {
                return Err(format!("avcodec_parameters_to_context(aac): {}", av_err_str(rc)));
            }
            (a.util.av_opt_set_q)(ctx.ptr(), c"time_base".as_ptr(), AVRational { num: 1, den: super::audio::SAMPLE_RATE as i32 }, 0);
            (a.util.av_opt_set)(ctx.ptr(), c"flags".as_ptr(), c"+global_header".as_ptr(), 0);
        }
        let rc = ctx.open(codec);
        if rc < 0 {
            return Err(format!("avcodec_open2(aac): {}", av_err_str(rc)));
        }
        Ok(Self { ctx, frame: Frame::alloc().ok_or("av_frame_alloc")?, pkt: Packet::alloc().ok_or("av_packet_alloc")?, title: title.to_owned() })
    }

    fn drain(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        let a = api()?;
        loop {
            let rc = unsafe { (a.codec.avcodec_receive_packet)(self.ctx.ptr(), self.pkt.ptr()) };
            if rc == AVERROR_EAGAIN || rc == AVERROR_EOF {
                return Ok(());
            }
            if rc < 0 {
                return Err(format!("avcodec_receive_packet(aac): {}", av_err_str(rc)));
            }
            if let Some(p) = self.pkt.clone_pkt() {
                out.push(p);
            }
            self.pkt.unref();
        }
    }
}

impl AudioEncoder for AvAacEncoder {
    fn stream_info(&self) -> Result<StreamInfo, String> {
        let a = api()?;
        let par = unsafe { (a.codec.avcodec_parameters_alloc)() };
        if par.is_null() {
            return Err("avcodec_parameters_alloc".into());
        }
        let rc = unsafe { (a.codec.avcodec_parameters_from_context)(par, self.ctx.ptr()) };
        let info = StreamInfo {
            par,
            time_base: AVRational { num: 1, den: super::audio::SAMPLE_RATE as i32 },
            frame_rate: AVRational { num: 0, den: 1 },
            title: self.title.clone(),
        };
        if rc < 0 {
            return Err(format!("avcodec_parameters_from_context(aac): {}", av_err_str(rc)));
        }
        Ok(info)
    }

    fn encode(&mut self, interleaved: &[f32], pts: i64, out: &mut Vec<Packet>) -> Result<(), String> {
        let a = api()?;
        let n = interleaved.len() / super::audio::CHANNELS;
        self.frame.unref();
        self.frame.nb_samples = n as i32;
        self.frame.format = AV_SAMPLE_FMT_FLTP;
        let f = self.frame.ptr();
        // `ch_layout`/`sample_rate` ficam fora da parte declarada do AVFrame:
        // posições medidas no avutil-59 (#51j).
        unsafe {
            ((f as *mut u8).add(OFFS_FRAME_CH_LAYOUT) as *mut AVChannelLayout).write_unaligned(stereo());
            ((f as *mut u8).add(OFFS_FRAME_SAMPLE_RATE) as *mut i32).write_unaligned(super::audio::SAMPLE_RATE as i32);
        }
        let rc = unsafe { (a.util.av_frame_get_buffer)(f, 0) };
        if rc < 0 {
            return Err(format!("av_frame_get_buffer(aac): {}", av_err_str(rc)));
        }
        unsafe {
            let (l, r) = (self.frame.data[0] as *mut f32, self.frame.data[1] as *mut f32);
            for i in 0..n {
                *l.add(i) = interleaved[i * 2];
                *r.add(i) = interleaved[i * 2 + 1];
            }
        }
        self.frame.pts = pts;
        let rc = unsafe { (a.codec.avcodec_send_frame)(self.ctx.ptr(), f) };
        self.frame.unref();
        if rc < 0 && rc != AVERROR_EAGAIN {
            return Err(format!("avcodec_send_frame(aac): {}", av_err_str(rc)));
        }
        self.drain(out)
    }

    fn flush(&mut self, out: &mut Vec<Packet>) {
        if let Ok(a) = api() {
            unsafe { (a.codec.avcodec_send_frame)(self.ctx.ptr(), std::ptr::null()) };
            if let Err(e) = self.drain(out) {
                log!("Captura: flush do AAC: {e}");
            }
        }
    }
}

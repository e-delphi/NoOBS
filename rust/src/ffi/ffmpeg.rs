//! Bindings do FFmpeg 7.x (porte de `FFmpegLib.pas`). Cabeçalhos de referência:
//! <https://github.com/FFmpeg/FFmpeg> (`libavformat/avformat.h`,
//! `libavcodec/{avcodec,packet,codec_par}.h`, `libavutil/frame.h`).
//!
//! As DLLs vêm do OBS portátil e a major tem que casar (pegadinha #15). O
//! mapeamento de famílias é pegadinha #25: `av_frame_*`, `av_dict_*`,
//! `av_opt_*`, `av_rescale_q` e `av_image_*` são do **avutil**, não do avcodec.
//!
//! `AVFormatContext` e `AVStream` NÃO são ABI-estáveis: os campos são lidos
//! por offset, validados contra as DLLs empacotadas (pegadinhas #26 e #40).
//! Trocar a major (61→62) exige revalidar TODOS os `OFFS_*`.

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

use crate::dll_api;

pub const LIB_AVFORMAT: &str = "avformat-61.dll";
pub const LIB_AVCODEC: &str = "avcodec-61.dll";
pub const LIB_AVUTIL: &str = "avutil-59.dll";
pub const LIB_SWSCALE: &str = "swscale-8.dll";

pub const AV_TIME_BASE: i64 = 1_000_000;
pub const AV_NOPTS_VALUE: i64 = i64::MIN;
pub const AVSEEK_FLAG_BACKWARD: c_int = 1;

pub const AVMEDIA_TYPE_VIDEO: i32 = 0;
pub const AVMEDIA_TYPE_AUDIO: i32 = 1;
pub const AVMEDIA_TYPE_DATA: i32 = 2;
pub const AVMEDIA_TYPE_SUBTITLE: i32 = 3;

pub const AV_CODEC_ID_MJPEG: i32 = 7;
pub const AV_CODEC_ID_H264: i32 = 27;
pub const AV_CODEC_ID_HEVC: i32 = 173;
pub const AV_CODEC_ID_AV1: i32 = 225;

// pixel formats (avutil/pixfmt.h)
pub const AV_PIX_FMT_NONE: c_int = -1;
pub const AV_PIX_FMT_YUV420P: c_int = 0;
pub const AV_PIX_FMT_RGB24: c_int = 2;
pub const AV_PIX_FMT_YUVJ420P: c_int = 12;
pub const AV_PIX_FMT_NV12: c_int = 23;
/// Quadro numa textura D3D11 (decode por hardware): conferido com
/// `av_get_pix_fmt('d3d11')` na avutil-59 empacotada.
pub const AV_PIX_FMT_D3D11: c_int = 171;
pub const AV_HWDEVICE_TYPE_D3D11VA: c_int = 7;
/// FLTP: o decoder de AAC entrega isso e o encoder `aac` também o recebe — a
/// mixagem de faixas não precisa de swresample.
pub const AV_SAMPLE_FMT_FLTP: c_int = 8;

pub const AVIO_FLAG_WRITE: c_int = 2;
pub const AVERROR_EAGAIN: c_int = -11;
/// `FFERRTAG('E','O','F',' ')` como int32.
pub const AVERROR_EOF: c_int = -541_478_725;
pub const AVINDEX_KEYFRAME: c_int = 1;
pub const AV_PKT_FLAG_KEY: c_int = 0x0001;

/// Algoritmos de reamostragem do swscale (tabela de opções do `SwsContext`,
/// conferidos contra a swscale-8 empacotada).
pub const SWS_BILINEAR: c_int = 2;
pub const SWS_BICUBIC: c_int = 4;
pub const SWS_AREA: c_int = 32;

// --- offsets de AVFormatContext (avformat-61, Win64) — pegadinha #26 --------
pub const OFFS_IFORMAT: usize = 8;
pub const OFFS_PB: usize = 32;
pub const OFFS_NB_STREAMS: usize = 44;
pub const OFFS_STREAMS: usize = 48;
pub const OFFS_DURATION: usize = 104;
pub const OFFS_BIT_RATE: usize = 112;
/// `AVCodecContext.hw_device_ctx` (avcodec-61): achado MEDINDO (#51l).
pub const OFFS_CODECCTX_HW_DEVICE_CTX: usize = 560;
/// `AVCodecContext.hw_frames_ctx` (avcodec-61): o campo IMEDIATAMENTE antes do
/// `hw_device_ctx` (avcodec.h do 7.1: `hw_frames_ctx`, `hw_device_ctx`,
/// `hwaccel_flags`, `extra_hw_frames`). Conferido pela âncora: o
/// `extra_hw_frames` (AVOption) cai em 572. Uma conta anterior pela vizinhança
/// do `max_pixels` estava errada — ele fica em 792 (medido).
pub const OFFS_CODECCTX_HW_FRAMES_CTX: usize = 552;
pub const OFFS_CODECCTX_EXTRA_HW_FRAMES: usize = 572;
/// `AVBufferRef.data` (estável: `AVBuffer *buffer; uint8_t *data; size_t size`).
pub const OFFS_BUFREF_DATA: usize = 8;
/// `AVHWDeviceContext` (avutil-59, sem o `internal` do 6.x): `type` em 8, `hwctx` em 16.
pub const OFFS_HWDEV_TYPE: usize = 8;
pub const OFFS_HWDEV_HWCTX: usize = 16;
/// `AVHWFramesContext` (avutil-59): `device_ref` 8, `hwctx` 24, `initial_pool_size`
/// 56, `format` 60, `sw_format` 64, `width` 68, `height` 72.
pub const OFFS_HWFR_DEVICE_REF: usize = 8;
pub const OFFS_HWFR_HWCTX: usize = 24;
pub const OFFS_HWFR_POOL_SIZE: usize = 56;
pub const OFFS_HWFR_FORMAT: usize = 60;
pub const OFFS_HWFR_SW_FORMAT: usize = 64;
pub const OFFS_HWFR_WIDTH: usize = 68;
pub const OFFS_HWFR_HEIGHT: usize = 72;
/// `AVD3D11VAFramesContext`: `texture` 0, `BindFlags` 8, `MiscFlags` 12.
pub const OFFS_D3D11FR_BIND_FLAGS: usize = 8;
/// `AVFrame` (avutil-59), medidos em #51j.
pub const OFFS_FRAME_SAMPLE_RATE: usize = 192;
pub const OFFS_FRAME_CH_LAYOUT: usize = 408;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct AVRational {
    pub num: c_int,
    pub den: c_int,
}

#[repr(C)]
pub struct AVChannelLayout {
    pub order: c_int,
    pub nb_channels: c_int,
    pub u: u64,
    pub opaque: *mut c_void,
}

/// `AVCodecParameters` — ABI estável, layout completo.
#[repr(C)]
pub struct AVCodecParameters {
    pub codec_type: c_int,
    pub codec_id: c_int,
    pub codec_tag: u32,
    pub extradata: *mut u8,
    pub extradata_size: c_int,
    pub coded_side_data: *mut c_void,
    pub nb_coded_side_data: c_int,
    pub format: c_int,
    pub bit_rate: i64,
    pub bits_per_coded_sample: c_int,
    pub bits_per_raw_sample: c_int,
    pub profile: c_int,
    pub level: c_int,
    pub width: c_int,
    pub height: c_int,
    pub sample_aspect_ratio: AVRational,
    pub framerate: AVRational,
    pub field_order: c_int,
    pub color_range: c_int,
    pub color_primaries: c_int,
    pub color_trc: c_int,
    pub color_space: c_int,
    pub chroma_location: c_int,
    pub video_delay: c_int,
    pub ch_layout: AVChannelLayout,
    pub sample_rate: c_int,
    pub block_align: c_int,
    pub frame_size: c_int,
    pub initial_padding: c_int,
    pub trailing_padding: c_int,
    pub seek_preroll: c_int,
}

/// Prefixo público do `AVStream` (FFmpeg 7.x): resto truncado de propósito.
#[repr(C)]
pub struct AVStream {
    pub av_class: *const c_void,
    pub index: c_int,
    pub id: c_int,
    pub codecpar: *mut AVCodecParameters,
    pub priv_data: *mut c_void,
    pub time_base: AVRational,
    pub start_time: i64,
    pub duration: i64,
    pub nb_frames: i64,
    pub disposition: c_int,
    pub discard: c_int,
    pub sample_aspect_ratio: AVRational,
    pub metadata: *mut c_void,
    pub avg_frame_rate: AVRational,
}

#[repr(C)]
pub struct AVDictionaryEntry {
    pub key: *const c_char,
    pub value: *const c_char,
}

/// `AVPacket` — ABI estável desde o FFmpeg 5.0.
#[repr(C)]
pub struct AVPacket {
    pub buf: *mut c_void,
    pub pts: i64,
    pub dts: i64,
    pub data: *mut u8,
    pub size: c_int,
    pub stream_index: c_int,
    pub flags: c_int,
    pub side_data: *mut c_void,
    pub side_data_elems: c_int,
    pub duration: i64,
    pub pos: i64,
    pub opaque: *mut c_void,
    pub opaque_ref: *mut c_void,
    pub time_base: AVRational,
}

pub type AVFormatContext = c_void;
/// `AVCodecContext` NÃO é ABI-estável: só opaco; campos por `av_opt_*`.
pub type AVCodecContext = c_void;
pub type SwsContext = c_void;

/// `AVFrame` — campos públicos do início; o resto é truncado de propósito (o
/// alocador resolve o tamanho real). Os campos de áudio fora daqui (`sample_rate`,
/// `ch_layout`) vão por offset (`OFFS_FRAME_*`).
#[repr(C)]
pub struct AVFrame {
    pub data: [*mut u8; 8],
    pub linesize: [c_int; 8],
    pub extended_data: *mut *mut u8,
    pub width: c_int,
    pub height: c_int,
    pub nb_samples: c_int,
    pub format: c_int,
    pub key_frame: c_int,
    pub pict_type: c_int,
    pub sample_aspect_ratio: AVRational,
    pub pts: i64,
    pub pkt_dts: i64,
    pub time_base: AVRational,
}

/// `AVCodec` — prefixo público.
#[repr(C)]
pub struct AVCodec {
    pub name: *const c_char,
    pub long_name: *const c_char,
    pub codec_type: c_int,
    pub id: c_int,
}

/// `AVIndexEntry`. Em C `flags` e `size` são bitfields de um mesmo int
/// (`flags:2, size:30`); no MSVC o primeiro ocupa os bits BAIXOS.
#[repr(C)]
pub struct AVIndexEntry {
    pub pos: i64,
    pub timestamp: i64,
    pub flags_size: c_int,
    pub min_distance: c_int,
}

dll_api! {
    /// avformat + avcodec + avutil + swscale. Cada DLL tem a sua tabela; ver
    /// `FfmpegApi`. Falta de QUALQUER símbolo derruba o carregamento inteiro.
    pub struct AvFormatApi;
    fn avformat_open_input(ps: *mut *mut AVFormatContext, url: *const c_char, fmt: *const c_void, options: *mut *mut c_void) -> c_int;
    fn avformat_find_stream_info(ic: *mut AVFormatContext, options: *mut *mut c_void) -> c_int;
    fn avformat_close_input(ps: *mut *mut AVFormatContext);
    fn av_read_frame(ic: *mut AVFormatContext, pkt: *mut AVPacket) -> c_int;
    fn av_seek_frame(ic: *mut AVFormatContext, stream_index: c_int, ts: i64, flags: c_int) -> c_int;
    // Índice de busca de um stream (lavf 59+). No MKV vem dos Cues, que o demuxer
    // só lê no PRIMEIRO seek — abrir o arquivo não basta.
    fn avformat_index_get_entries_count(st: *const AVStream) -> c_int;
    fn avformat_index_get_entry(st: *mut AVStream, idx: c_int) -> *const AVIndexEntry;
    fn avformat_alloc_output_context2(ctx: *mut *mut AVFormatContext, oformat: *const c_void, format_name: *const c_char, filename: *const c_char) -> c_int;
    fn avformat_new_stream(s: *mut AVFormatContext, c: *const AVCodec) -> *mut AVStream;
    fn avio_open2(s: *mut *mut c_void, url: *const c_char, flags: c_int, int_cb: *const c_void, options: *mut *mut c_void) -> c_int;
    fn avio_closep(s: *mut *mut c_void) -> c_int;
    fn avformat_write_header(s: *mut AVFormatContext, options: *mut *mut c_void) -> c_int;
    fn av_write_trailer(s: *mut AVFormatContext) -> c_int;
    fn av_interleaved_write_frame(s: *mut AVFormatContext, pkt: *mut AVPacket) -> c_int;
    fn avformat_free_context(s: *mut AVFormatContext);
}

dll_api! {
    pub struct AvCodecApi;
    fn avcodec_get_name(id: c_int) -> *const c_char;
    fn av_packet_alloc() -> *mut AVPacket;
    fn av_packet_free(pkt: *mut *mut AVPacket);
    fn av_packet_unref(pkt: *mut AVPacket);
    fn av_packet_rescale_ts(pkt: *mut AVPacket, tb_src: AVRational, tb_dst: AVRational);
    /// Cópia nova do pacote (referencia os mesmos dados; libera com `av_packet_free`).
    fn av_packet_clone(src: *const AVPacket) -> *mut AVPacket;
    fn avcodec_find_decoder(id: c_int) -> *const AVCodec;
    fn avcodec_find_encoder(id: c_int) -> *const AVCodec;
    fn avcodec_find_encoder_by_name(name: *const c_char) -> *const AVCodec;
    fn avcodec_find_decoder_by_name(name: *const c_char) -> *const AVCodec;
    fn avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
    fn avcodec_free_context(avctx: *mut *mut AVCodecContext);
    fn avcodec_parameters_to_context(codec: *mut AVCodecContext, par: *const AVCodecParameters) -> c_int;
    fn avcodec_parameters_copy(dst: *mut AVCodecParameters, src: *const AVCodecParameters) -> c_int;
    fn avcodec_parameters_alloc() -> *mut AVCodecParameters;
    fn avcodec_parameters_free(par: *mut *mut AVCodecParameters);
    fn avcodec_parameters_from_context(par: *mut AVCodecParameters, codec: *const AVCodecContext) -> c_int;
    fn avcodec_open2(avctx: *mut AVCodecContext, codec: *const AVCodec, options: *mut *mut c_void) -> c_int;
    fn avcodec_send_packet(avctx: *mut AVCodecContext, avpkt: *const AVPacket) -> c_int;
    fn avcodec_receive_frame(avctx: *mut AVCodecContext, frame: *mut AVFrame) -> c_int;
    fn avcodec_send_frame(avctx: *mut AVCodecContext, frame: *const AVFrame) -> c_int;
    fn avcodec_receive_packet(avctx: *mut AVCodecContext, avpkt: *mut AVPacket) -> c_int;
    /// Descarta o estado interno do codec. OBRIGATÓRIO depois de um seek.
    fn avcodec_flush_buffers(avctx: *mut AVCodecContext);
}

dll_api! {
    pub struct AvUtilApi;
    fn av_dict_get(m: *const c_void, key: *const c_char, prev: *const AVDictionaryEntry, flags: c_int) -> *mut AVDictionaryEntry;
    fn av_dict_set(pm: *mut *mut c_void, key: *const c_char, value: *const c_char, flags: c_int) -> c_int;
    fn av_dict_free(pm: *mut *mut c_void);
    fn av_rescale_q(a: i64, bq: AVRational, cq: AVRational) -> i64;
    fn av_strerror(errnum: c_int, buf: *mut c_char, size: usize) -> c_int;
    fn av_image_get_buffer_size(pix_fmt: c_int, width: c_int, height: c_int, align: c_int) -> c_int;
    fn av_image_fill_arrays(dst_data: *mut *mut u8, dst_linesize: *mut c_int, src: *const u8, pix_fmt: c_int, width: c_int, height: c_int, align: c_int) -> c_int;
    // av_frame_* são do AVUTIL, não do avcodec (pegadinha #25).
    fn av_frame_alloc() -> *mut AVFrame;
    fn av_frame_free(frame: *mut *mut AVFrame);
    fn av_frame_unref(frame: *mut AVFrame);
    /// Outra referência ao mesmo quadro (o buffer não é copiado).
    fn av_frame_ref(dst: *mut AVFrame, src: *const AVFrame) -> c_int;
    /// Aloca os buffers a partir de width/height/format (vídeo) ou
    /// nb_samples/format/ch_layout (áudio), que o chamador preenche ANTES.
    fn av_frame_get_buffer(frame: *mut AVFrame, align: c_int) -> c_int;
    /// Garante frame gravável (buffer não compartilhado).
    fn av_frame_make_writable(frame: *mut AVFrame) -> c_int;
    /// Move a posse do conteúdo de `src` para `dst` (`src` fica em branco).
    fn av_frame_move_ref(dst: *mut AVFrame, src: *mut AVFrame);
    /// Copia pts/duração/metadados (não os dados). O `av_hwframe_transfer_data`
    /// só copia a IMAGEM — sem isto o quadro vindo da GPU chega sem pts.
    fn av_frame_copy_props(dst: *mut AVFrame, src: *const AVFrame) -> c_int;
    fn av_opt_set_int(obj: *mut c_void, name: *const c_char, val: i64, flags: c_int) -> c_int;
    fn av_opt_set_q(obj: *mut c_void, name: *const c_char, val: AVRational, flags: c_int) -> c_int;
    fn av_opt_set(obj: *mut c_void, name: *const c_char, val: *const c_char, flags: c_int) -> c_int;
    fn av_opt_get_int(obj: *mut c_void, name: *const c_char, flags: c_int, out_val: *mut i64) -> c_int;
    fn av_channel_layout_copy(dst: *mut c_void, src: *const c_void) -> c_int;
    // Decode por hardware: o "device" é um AVBufferRef*.
    fn av_hwdevice_ctx_create(device_ctx: *mut *mut c_void, device_type: c_int, device: *const c_char, opts: *mut c_void, flags: c_int) -> c_int;
    fn av_hwframe_transfer_data(dst: *mut AVFrame, src: *const AVFrame, flags: c_int) -> c_int;
    fn av_buffer_ref(buf: *const c_void) -> *mut c_void;
    fn av_buffer_unref(buf: *mut *mut c_void);
    // Codificação por hardware com quadros D3D11 (motor de captura próprio):
    // device da NOSSA placa entregue ao FFmpeg + pool de quadros na GPU.
    fn av_hwdevice_ctx_alloc(device_type: c_int) -> *mut c_void;
    fn av_hwdevice_ctx_init(device_ref: *mut c_void) -> c_int;
    fn av_hwframe_ctx_alloc(device_ref: *mut c_void) -> *mut c_void;
    fn av_hwframe_ctx_init(frames_ref: *mut c_void) -> c_int;
    fn av_hwframe_get_buffer(frames_ref: *mut c_void, frame: *mut AVFrame, flags: c_int) -> c_int;
}

dll_api! {
    pub struct SwScaleApi;
    // Nome = símbolo exportado pela swscale-8.dll (o macro faz stringify!).
    #[allow(non_snake_case)]
    fn sws_getContext(src_w: c_int, src_h: c_int, src_format: c_int, dst_w: c_int, dst_h: c_int, dst_format: c_int, flags: c_int, src_filter: *mut c_void, dst_filter: *mut c_void, param: *const f64) -> *mut SwsContext;
    #[allow(non_snake_case)]
    fn sws_freeContext(ctx: *mut SwsContext);
    fn sws_scale(c: *mut SwsContext, src_slice: *const *const u8, src_stride: *const c_int, src_slice_y: c_int, src_slice_h: c_int, dst: *const *mut u8, dst_stride: *const c_int) -> c_int;
}

pub struct FfmpegApi {
    pub format: AvFormatApi,
    pub codec: AvCodecApi,
    pub util: AvUtilApi,
    pub scale: SwScaleApi,
}

fn open(name: &str) -> Result<libloading::Library, String> {
    unsafe { libloading::Library::new(name) }.map_err(|e| format!("{name}: {e}"))
}

/// As quatro DLLs, carregadas sob demanda. `Err` = alguma faltou.
pub fn api() -> Result<&'static FfmpegApi, String> {
    static API: OnceLock<Result<FfmpegApi, String>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let err = |n: &str, e: libloading::Error| format!("{n}: {e}");
        Ok(FfmpegApi {
            format: AvFormatApi::load(open(LIB_AVFORMAT)?).map_err(|e| err(LIB_AVFORMAT, e))?,
            codec: AvCodecApi::load(open(LIB_AVCODEC)?).map_err(|e| err(LIB_AVCODEC, e))?,
            util: AvUtilApi::load(open(LIB_AVUTIL)?).map_err(|e| err(LIB_AVUTIL, e))?,
            scale: SwScaleApi::load(open(LIB_SWSCALE)?).map_err(|e| err(LIB_SWSCALE, e))?,
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub fn ffmpeg_lib_available() -> bool {
    api().is_ok()
}

// --- acessores de AVFormatContext por offset --------------------------------

unsafe fn field<T: Copy>(ic: *const AVFormatContext, off: usize) -> T {
    (ic as *const u8).add(off).cast::<T>().read_unaligned()
}

/// # Safety
/// `ic` válido (aberto por `avformat_open_input`).
pub unsafe fn nb_streams(ic: *const AVFormatContext) -> u32 {
    field(ic, OFFS_NB_STREAMS)
}

/// # Safety
/// `ic` válido.
pub unsafe fn duration(ic: *const AVFormatContext) -> i64 {
    field(ic, OFFS_DURATION)
}

/// # Safety
/// `ic` válido.
pub unsafe fn bit_rate(ic: *const AVFormatContext) -> i64 {
    field(ic, OFFS_BIT_RATE)
}

/// i-ésimo stream, ou `None` fora do intervalo. Guarda de contagem ANTES do
/// laço (pegadinha #24: `count - 1` em unsigned dá underflow).
///
/// # Safety
/// `ic` válido.
pub unsafe fn stream<'a>(ic: *const AVFormatContext, idx: u32) -> Option<&'a AVStream> {
    if idx >= nb_streams(ic) {
        return None;
    }
    let streams: *const *const AVStream = field(ic, OFFS_STREAMS);
    if streams.is_null() {
        return None;
    }
    (*streams.add(idx as usize)).as_ref()
}

/// Nome do formato (`matroska,webm`, `mov,mp4,m4a,...`): `AVInputFormat.name`
/// é o primeiro campo da struct apontada por `iformat`.
///
/// # Safety
/// `ic` válido.
pub unsafe fn iformat_name(ic: *const AVFormatContext) -> String {
    let iformat: *const *const c_char = field(ic, OFFS_IFORMAT);
    if iformat.is_null() {
        return String::new();
    }
    super::cstr_to_string(*iformat)
}

/// Texto → UTF-8 para o FFmpeg (pegadinha #29: nunca ANSI/locale). Corta no
/// primeiro NUL em vez de falhar.
pub fn to_utf8(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s.split(' ').next().unwrap_or("")).unwrap_or_default()
}

/// `av_strerror` legível, no formato `-22 (Invalid argument)`. Só para log — nunca
/// mostre ao usuário, que vê as mensagens do `lang`. Código desconhecido: o número cru.
pub fn av_err_str(code: c_int) -> String {
    let Ok(a) = api() else { return code.to_string() };
    let mut buf = [0 as c_char; 256];
    unsafe {
        if (a.util.av_strerror)(code, buf.as_mut_ptr(), buf.len()) == 0 {
            return format!("{code} ({})", super::cstr_to_string(buf.as_ptr()));
        }
    }
    code.to_string()
}

/// Valor de uma tag de metadata (ex.: `title`).
///
/// # Safety
/// `dict` é um `AVDictionary*` válido ou nulo.
pub unsafe fn metadata_string(dict: *const c_void, key: &str) -> String {
    let Ok(a) = api() else { return String::new() };
    if dict.is_null() {
        return String::new();
    }
    let e = (a.util.av_dict_get)(dict, to_utf8(key).as_ptr(), std::ptr::null(), 0);
    if e.is_null() { String::new() } else { super::cstr_to_string((*e).value) }
}

/// Duração varrendo os pacotes (último recurso; O(N) de pacotes) — vale para
/// MKV sem `Duration` (gravação interrompida, pegadinha #27). µs.
///
/// # Safety
/// `ic` válido e aberto.
pub unsafe fn scan_duration_by_packets(ic: *mut AVFormatContext) -> i64 {
    let Ok(a) = api() else { return 0 };
    let mut pkt = (a.codec.av_packet_alloc)();
    if pkt.is_null() {
        return 0;
    }
    let tb_us = AVRational { num: 1, den: AV_TIME_BASE as c_int };
    let mut best = 0i64;
    while (a.format.av_read_frame)(ic, pkt) == 0 {
        if let Some(s) = stream(ic, (*pkt).stream_index as u32) {
            if s.time_base.den > 0 && (*pkt).pts != AV_NOPTS_VALUE {
                // av_rescale_q usa 128 bits; multiplicar à mão estoura em
                // time_bases finos de arquivos longos.
                let end_us = (a.util.av_rescale_q)((*pkt).pts + (*pkt).duration, s.time_base, tb_us);
                best = best.max(end_us);
            }
        }
        (a.codec.av_packet_unref)(pkt);
    }
    (a.codec.av_packet_free)(&mut pkt);
    best
}

/// Taxa medida nos primeiros pacotes de um stream (quadros ÷ intervalo), para
/// arquivos que não declaram `avg_frame_rate` (o MKV de uma união — #51m).
///
/// # Safety
/// `ic` válido e aberto.
pub unsafe fn estimate_fps_from_packets(ic: *mut AVFormatContext, stream_idx: i32) -> f64 {
    const MAX_PKTS: u32 = 240; // ~4 s a 60 fps
    const MAX_READS: u32 = 4000; // teto contando pacotes de qualquer stream
    let Ok(a) = api() else { return 0.0 };
    let Some(s) = (if stream_idx >= 0 { stream(ic, stream_idx as u32) } else { None }) else {
        return 0.0;
    };
    if s.time_base.den <= 0 || s.time_base.num <= 0 {
        return 0.0;
    }
    let tb = s.time_base;
    let mut pkt = (a.codec.av_packet_alloc)();
    if pkt.is_null() {
        return 0.0;
    }
    let (mut n, mut reads) = (0u32, 0u32);
    let (mut min_pts, mut max_pts) = (i64::MAX, i64::MIN);
    while n < MAX_PKTS && reads < MAX_READS && (a.format.av_read_frame)(ic, pkt) == 0 {
        reads += 1;
        if (*pkt).stream_index == stream_idx && (*pkt).pts != AV_NOPTS_VALUE {
            n += 1;
            min_pts = min_pts.min((*pkt).pts);
            max_pts = max_pts.max((*pkt).pts);
        }
        (a.codec.av_packet_unref)(pkt);
    }
    (a.codec.av_packet_free)(&mut pkt);
    (a.format.av_seek_frame)(ic, -1, 0, AVSEEK_FLAG_BACKWARD);
    if n < 10 || max_pts <= min_pts {
        return 0.0;
    }
    // Quadros pelo intervalo que eles cobrem — NÃO a mediana dos intervalos: com
    // tempo em ms, 60 fps alterna 16 e 17 ms e a mediana daria 58,8.
    let span = (max_pts - min_pts) as f64 * tb.num as f64 / tb.den as f64;
    if span <= 0.0 {
        return 0.0;
    }
    let fps = (n - 1) as f64 / span;
    // Taxas cheias (30, 60, 144) são o caso comum: arredonda o que estiver a
    // menos de 0,05% de um inteiro (o erro de medir com tempo em ms em ~4 s é
    // ~0,03%) e deixa 29,97 (0,1% abaixo de 30) como 29,97.
    if (fps - fps.round()).abs() < fps * 0.0005 {
        fps.round()
    } else {
        (fps * 100.0).round() / 100.0
    }
}

/// Atribui o `AVIOContext*` ao campo `pb` (encapsula o offset ABI).
///
/// # Safety
/// `ic` válido; `pb` aberto por `avio_open2`.
pub unsafe fn set_pb(ic: *mut AVFormatContext, pb: *mut c_void) {
    if !ic.is_null() {
        ((ic as *mut u8).add(OFFS_PB) as *mut *mut c_void).write_unaligned(pb);
    }
}

fn tb_us() -> AVRational {
    AVRational { num: 1, den: AV_TIME_BASE as c_int }
}

/// Duração do container em µs. Campo direto; se zero (comum em MKV), o máximo
/// das durações dos streams (cada um na sua time_base, via `av_rescale_q` —
/// multiplicar à mão estoura Int64 em time_bases finos).
///
/// # Safety
/// `ic` válido.
pub unsafe fn container_duration_us(ic: *const AVFormatContext) -> i64 {
    if ic.is_null() {
        return 0;
    }
    let direct = duration(ic);
    if direct > 0 {
        return direct;
    }
    let Ok(a) = api() else { return 0 };
    let mut best = 0i64;
    for i in 0..nb_streams(ic) {
        let Some(s) = stream(ic, i) else { continue };
        if s.duration > 0 && s.time_base.den > 0 {
            best = best.max((a.util.av_rescale_q)(s.duration, s.time_base, tb_us()));
        }
    }
    best
}

/// Bitrate em bps: campo direto; se zero, a soma dos streams.
///
/// # Safety
/// `ic` válido.
pub unsafe fn container_bit_rate(ic: *const AVFormatContext) -> i64 {
    if ic.is_null() {
        return 0;
    }
    let direct = bit_rate(ic);
    if direct > 0 {
        return direct;
    }
    let mut total = 0i64;
    for i in 0..nb_streams(ic) {
        if let Some(cp) = stream(ic, i).and_then(|s| s.codecpar.as_ref()) {
            if cp.bit_rate > 0 {
                total += cp.bit_rate;
            }
        }
    }
    total
}

/// µs → timestamp na `tb` (ou `AV_NOPTS_VALUE` se a base é inválida).
pub fn us_to_ts(us: i64, tb: AVRational) -> i64 {
    match api() {
        Ok(a) if tb.den > 0 => unsafe { (a.util.av_rescale_q)(us, tb_us(), tb) },
        _ => AV_NOPTS_VALUE,
    }
}

/// timestamp na `tb` → µs (`AV_NOPTS_VALUE` propaga).
pub fn ts_to_us(ts: i64, tb: AVRational) -> i64 {
    match api() {
        Ok(a) if ts != AV_NOPTS_VALUE && tb.den > 0 => unsafe { (a.util.av_rescale_q)(ts, tb, tb_us()) },
        _ => AV_NOPTS_VALUE,
    }
}

/// Contexto de entrada aberto: fecha no `Drop` (o `try/finally` do Delphi).
pub struct Input {
    ic: *mut AVFormatContext,
}

impl Input {
    /// Abre `path` (UTF-8, pegadinha #29). `Err(rc)` do `avformat_open_input`.
    pub fn open(path: &str) -> Result<Self, c_int> {
        let a = api().map_err(|_| -1)?;
        let c_path = to_utf8(path);
        let mut ic: *mut AVFormatContext = std::ptr::null_mut();
        let rc = unsafe { (a.format.avformat_open_input)(&mut ic, c_path.as_ptr(), std::ptr::null(), std::ptr::null_mut()) };
        if rc < 0 || ic.is_null() {
            if !ic.is_null() {
                unsafe { (a.format.avformat_close_input)(&mut ic) };
            }
            return Err(if rc < 0 { rc } else { -1 });
        }
        Ok(Self { ic })
    }

    /// `avformat_open_input` com retry curto: cobre o arquivo estar
    /// MOMENTANEAMENTE aberto por outra thread (geração de prévia/duração da
    /// biblioteca, ~200–500 ms). Sem isto, unir logo após adicionar falhava com
    /// "arquivo em uso". Só chamar de worker (dorme).
    pub fn open_with_retry(path: &str) -> Option<Self> {
        const MAX_RETRIES: u32 = 10;
        const RETRY_MS: u64 = 200;
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        for attempt in 0..MAX_RETRIES {
            match Self::open(path) {
                Ok(i) => return Some(i),
                Err(rc) => {
                    if attempt == 0 {
                        crate::log!("OpenInput: \"{name}\" ocupado (rc={rc}) — tentando de novo.");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(RETRY_MS));
                }
            }
        }
        crate::log!("OpenInput: desistiu de \"{name}\" apos {MAX_RETRIES} tentativas.");
        None
    }

    /// Assume a posse de um contexto já aberto (ex.: com opções no `open_input`).
    pub fn from_raw(ic: *mut AVFormatContext) -> Self {
        Self { ic }
    }

    pub fn find_stream_info(&self) -> bool {
        match api() {
            Ok(a) => unsafe { (a.format.avformat_find_stream_info)(self.ic, std::ptr::null_mut()) >= 0 },
            Err(_) => false,
        }
    }

    pub fn ptr(&self) -> *mut AVFormatContext {
        self.ic
    }

    pub fn nb_streams(&self) -> u32 {
        unsafe { nb_streams(self.ic) }
    }

    pub fn stream(&self, idx: u32) -> Option<&AVStream> {
        unsafe { stream(self.ic, idx) }
    }

    /// `av_seek_frame(-1, 0, BACKWARD)`: volta ao começo.
    pub fn rewind(&self) {
        if let Ok(a) = api() {
            unsafe { (a.format.av_seek_frame)(self.ic, -1, 0, AVSEEK_FLAG_BACKWARD) };
        }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.format.avformat_close_input)(&mut self.ic) }
        }
    }
}

/// `AVPacket` alocado; libera no `Drop`.
pub struct Packet(*mut AVPacket);

// SAFETY: o pacote é dono dos próprios dados (referência contada do libav) e só
// é usado por uma thread por vez — o motor de captura passa pacotes da thread
// do encoder pra do gravador por canal.
unsafe impl Send for Packet {}

impl Packet {
    pub fn alloc() -> Option<Self> {
        let a = api().ok()?;
        let p = unsafe { (a.codec.av_packet_alloc)() };
        (!p.is_null()).then_some(Self(p))
    }
    pub fn ptr(&self) -> *mut AVPacket {
        self.0
    }
    pub fn unref(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.codec.av_packet_unref)(self.0) }
        }
    }
    /// Cópia que referencia os mesmos dados.
    pub fn clone_pkt(&self) -> Option<Self> {
        let a = api().ok()?;
        let p = unsafe { (a.codec.av_packet_clone)(self.0) };
        (!p.is_null()).then_some(Self(p))
    }
}

impl std::ops::Deref for Packet {
    type Target = AVPacket;
    fn deref(&self) -> &AVPacket {
        unsafe { &*self.0 }
    }
}
impl std::ops::DerefMut for Packet {
    fn deref_mut(&mut self) -> &mut AVPacket {
        unsafe { &mut *self.0 }
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.codec.av_packet_free)(&mut self.0) }
        }
    }
}

/// `AVFrame` alocado; libera no `Drop`.
pub struct Frame(*mut AVFrame);

impl Frame {
    pub fn alloc() -> Option<Self> {
        let a = api().ok()?;
        let f = unsafe { (a.util.av_frame_alloc)() };
        (!f.is_null()).then_some(Self(f))
    }
    pub fn ptr(&self) -> *mut AVFrame {
        self.0
    }
    pub fn unref(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.util.av_frame_unref)(self.0) }
        }
    }
}

impl std::ops::Deref for Frame {
    type Target = AVFrame;
    fn deref(&self) -> &AVFrame {
        unsafe { &*self.0 }
    }
}
impl std::ops::DerefMut for Frame {
    fn deref_mut(&mut self) -> &mut AVFrame {
        unsafe { &mut *self.0 }
    }
}

// O quadro é contado por referência pelo FFmpeg: pode mudar de thread.
unsafe impl Send for Frame {}

impl Drop for Frame {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.util.av_frame_free)(&mut self.0) }
        }
    }
}

/// Lista de opções para o `AVDictionary` do `avformat_write_header` etc.
pub struct Dict(*mut c_void);

impl Dict {
    pub fn new() -> Self {
        Self(std::ptr::null_mut())
    }
    pub fn set(&mut self, key: &str, value: &str) {
        if let Ok(a) = api() {
            unsafe { (a.util.av_dict_set)(&mut self.0, to_utf8(key).as_ptr(), to_utf8(value).as_ptr(), 0) };
        }
    }
    pub fn as_mut_ptr(&mut self) -> *mut *mut c_void {
        &mut self.0
    }
    /// Chaves que sobraram (o `avcodec_open2` deixa no dicionário o que o encoder
    /// NÃO consumiu). Itera com a chave vazia + `AV_DICT_IGNORE_SUFFIX` (2).
    pub fn leftover_keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.0.is_null() {
            return out;
        }
        let Ok(a) = api() else { return out };
        let empty = c"";
        let mut prev: *const AVDictionaryEntry = std::ptr::null();
        loop {
            let e = unsafe { (a.util.av_dict_get)(self.0, empty.as_ptr(), prev, 2) };
            if e.is_null() {
                break;
            }
            out.push(unsafe { super::cstr_to_string((*e).key) });
            prev = e;
        }
        out
    }
}

impl Drop for Dict {
    fn drop(&mut self) {
        if !self.0.is_null() {
            if let Ok(a) = api() {
                unsafe { (a.util.av_dict_free)(&mut self.0) }
            }
        }
    }
}

/// Copia UMA tag de metadata (ex.: `title`, `language`) de um stream pro outro,
/// se existir — no lugar de `av_dict_copy`, para NÃO arrastar tags que ficam
/// inválidas após corte/remux (DURATION, `_STATISTICS_*`, `NUMBER_OF_*`: o
/// demuxer lê DURATION e reportaria a duração do arquivo original).
///
/// # Safety
/// Streams válidos.
pub unsafe fn copy_stream_tag(src: &AVStream, dst: *mut AVStream, key: &str) {
    let Ok(a) = api() else { return };
    if key == "title" {
        // O título vai TAMBÉM no `handler_name`: o MP4 não guarda o `title` de
        // faixa (medido: some no caminho de volta), só o nome do handler. O MKV
        // guarda os dois como tags, sem efeito.
        let t = stream_title(src);
        if !t.is_empty() {
            let v = to_utf8(&t);
            (a.util.av_dict_set)(&mut (*dst).metadata, c"title".as_ptr(), v.as_ptr(), 0);
            (a.util.av_dict_set)(&mut (*dst).metadata, c"handler_name".as_ptr(), v.as_ptr(), 0);
        }
        return;
    }
    let k = to_utf8(key);
    let e = (a.util.av_dict_get)(src.metadata, k.as_ptr(), std::ptr::null(), 0);
    if !e.is_null() && !(*e).value.is_null() {
        (a.util.av_dict_set)(&mut (*dst).metadata, k.as_ptr(), (*e).value, 0);
    }
}

/// Nome da faixa: o `title` (MKV) ou, sem ele, o `handler_name` (MP4) — menos os
/// nomes genéricos que o próprio libav e os editores põem quando não há nome.
///
/// # Safety
/// `st` de um contexto aberto.
pub unsafe fn stream_title(st: &AVStream) -> String {
    let t = metadata_string(st.metadata, "title");
    if !t.trim().is_empty() {
        return t;
    }
    let h = metadata_string(st.metadata, "handler_name");
    const GENERIC: [&str; 6] = ["SoundHandler", "VideoHandler", "Core Media Audio", "Core Media Video", "ISO Media file produced by Google Inc.", "Mainconcept Video Media Handler"];
    if GENERIC.iter().any(|g| h.trim().eq_ignore_ascii_case(g)) {
        String::new()
    } else {
        h
    }
}

/// `AVCodecContext` alocado; libera no `Drop`.
pub struct CodecCtx(*mut AVCodecContext);

impl CodecCtx {
    pub fn alloc(codec: *const AVCodec) -> Option<Self> {
        let a = api().ok()?;
        let c = unsafe { (a.codec.avcodec_alloc_context3)(codec) };
        (!c.is_null()).then_some(Self(c))
    }
    pub fn ptr(&self) -> *mut AVCodecContext {
        self.0
    }
    /// `avcodec_open2` sem opções; devolve o `rc`.
    pub fn open(&self, codec: *const AVCodec) -> c_int {
        match api() {
            Ok(a) => unsafe { (a.codec.avcodec_open2)(self.0, codec, std::ptr::null_mut()) },
            Err(_) => -1,
        }
    }
    /// `threads` do libavcodec tem default **1**, não "auto" (pegadinha #52):
    /// 0 = automático.
    pub fn set_threads_auto(&self) {
        if let Ok(a) = api() {
            unsafe { (a.util.av_opt_set_int)(self.0, c"threads".as_ptr(), 0, 0) };
        }
    }
    /// Abre um decoder para o stream (`find_decoder` + `parameters_to_context` + `open2`).
    pub fn open_decoder_for(par: &AVCodecParameters, threads_auto: bool) -> Option<Self> {
        let a = api().ok()?;
        let codec = unsafe { (a.codec.avcodec_find_decoder)(par.codec_id) };
        if codec.is_null() {
            return None;
        }
        let ctx = Self::alloc(codec)?;
        if unsafe { (a.codec.avcodec_parameters_to_context)(ctx.0, par) } < 0 {
            return None;
        }
        if threads_auto {
            ctx.set_threads_auto();
        }
        (ctx.open(codec) >= 0).then_some(ctx)
    }
}

impl Drop for CodecCtx {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.codec.avcodec_free_context)(&mut self.0) }
        }
    }
}

/// `SwsContext` (swscale) com liberação automática.
pub struct Sws(*mut SwsContext);

impl Sws {
    pub fn new(src_w: c_int, src_h: c_int, src_fmt: c_int, dst_w: c_int, dst_h: c_int, dst_fmt: c_int, flags: c_int) -> Option<Self> {
        let a = api().ok()?;
        let p = unsafe {
            (a.scale.sws_getContext)(src_w, src_h, src_fmt, dst_w, dst_h, dst_fmt, flags, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null())
        };
        (!p.is_null()).then_some(Self(p))
    }
    /// `sws_scale` com ponteiros de plano/passos montados à mão (recorte por
    /// deslocamento de ponteiro — o `linesize` continua sendo o do quadro inteiro).
    pub fn scale_raw(&self, src_data: &[*const u8; 8], src_ls: &[c_int; 8], src_h: c_int, dst_data: &[*mut u8; 8], dst_ls: &[c_int; 8]) {
        if let Ok(a) = api() {
            unsafe { (a.scale.sws_scale)(self.0, src_data.as_ptr(), src_ls.as_ptr(), 0, src_h, dst_data.as_ptr(), dst_ls.as_ptr()) };
        }
    }

    /// Escala o quadro inteiro `src` (altura `src_h`) para `dst`.
    pub fn scale(&self, src: &AVFrame, src_h: c_int, dst: &AVFrame) {
        if let Ok(a) = api() {
            unsafe {
                (a.scale.sws_scale)(
                    self.0,
                    src.data.as_ptr() as *const *const u8,
                    src.linesize.as_ptr(),
                    0,
                    src_h,
                    dst.data.as_ptr() as *const *mut u8,
                    dst.linesize.as_ptr(),
                );
            }
        }
    }
}

impl Drop for Sws {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.scale.sws_freeContext)(self.0) }
        }
    }
}

/// Índice do primeiro stream do tipo pedido.
///
/// # Safety
/// `ic` válido.
pub unsafe fn first_stream_of_type(ic: *const AVFormatContext, media_type: i32) -> Option<u32> {
    (0..nb_streams(ic)).find(|&i| {
        stream(ic, i)
            .and_then(|s| s.codecpar.as_ref())
            .is_some_and(|cp| cp.codec_type == media_type)
    })
}

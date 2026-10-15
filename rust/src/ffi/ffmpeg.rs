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

pub const AV_CODEC_ID_H264: i32 = 27;
pub const AV_CODEC_ID_HEVC: i32 = 173;
pub const AV_CODEC_ID_AV1: i32 = 225;

pub const AV_LOG_QUIET: c_int = -8;

// --- offsets de AVFormatContext (avformat-61, Win64) — pegadinha #26 --------
pub const OFFS_IFORMAT: usize = 8;
pub const OFFS_PB: usize = 32;
pub const OFFS_NB_STREAMS: usize = 44;
pub const OFFS_STREAMS: usize = 48;
pub const OFFS_DURATION: usize = 104;
pub const OFFS_BIT_RATE: usize = 112;
/// `AVCodecContext.hw_device_ctx` (avcodec-61): achado MEDINDO (#51l).
pub const OFFS_CODECCTX_HW_DEVICE_CTX: usize = 560;
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

dll_api! {
    /// avformat + avcodec + avutil (funções usadas pelo probe/remux/thumb).
    /// Cada DLL tem a sua tabela; ver `FfmpegApi`.
    pub struct AvFormatApi;
    fn avformat_version() -> u32;
    fn avformat_open_input(ps: *mut *mut AVFormatContext, url: *const c_char, fmt: *const c_void, options: *mut *mut c_void) -> c_int;
    fn avformat_find_stream_info(ic: *mut AVFormatContext, options: *mut *mut c_void) -> c_int;
    fn avformat_close_input(ps: *mut *mut AVFormatContext);
    fn av_read_frame(ic: *mut AVFormatContext, pkt: *mut AVPacket) -> c_int;
    fn av_seek_frame(ic: *mut AVFormatContext, stream_index: c_int, ts: i64, flags: c_int) -> c_int;
}

dll_api! {
    pub struct AvCodecApi;
    fn avcodec_version() -> u32;
    fn avcodec_get_name(id: c_int) -> *const c_char;
    fn av_packet_alloc() -> *mut AVPacket;
    fn av_packet_free(pkt: *mut *mut AVPacket);
    fn av_packet_unref(pkt: *mut AVPacket);
}

dll_api! {
    pub struct AvUtilApi;
    fn avutil_version() -> u32;
    fn av_log_set_level(level: c_int);
    fn av_dict_get(m: *const c_void, key: *const c_char, prev: *const AVDictionaryEntry, flags: c_int) -> *mut AVDictionaryEntry;
    fn av_dict_set(pm: *mut *mut c_void, key: *const c_char, value: *const c_char, flags: c_int) -> c_int;
    fn av_dict_free(pm: *mut *mut c_void);
    fn av_rescale_q(a: i64, bq: AVRational, cq: AVRational) -> i64;
    fn av_strerror(errnum: c_int, buf: *mut c_char, size: usize) -> c_int;
    fn av_frame_alloc() -> *mut c_void;
    fn av_frame_free(frame: *mut *mut c_void);
    fn av_opt_set_int(obj: *mut c_void, name: *const c_char, val: i64, flags: c_int) -> c_int;
    fn av_opt_set(obj: *mut c_void, name: *const c_char, val: *const c_char, flags: c_int) -> c_int;
}

dll_api! {
    pub struct SwScaleApi;
    fn swscale_version() -> u32;
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

/// Texto → UTF-8 para o FFmpeg (pegadinha #29: nunca ANSI/locale).
pub fn to_utf8(s: &str) -> std::ffi::CString {
    super::obs::cstring(s)
}

/// `av_strerror` legível.
pub fn av_err_str(code: c_int) -> String {
    let Ok(a) = api() else { return format!("erro {code}") };
    let mut buf = [0 as c_char; 128];
    unsafe {
        (a.util.av_strerror)(code, buf.as_mut_ptr(), buf.len());
        super::cstr_to_string(buf.as_ptr())
    }
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
    if n < 2 || max_pts <= min_pts {
        return 0.0;
    }
    let span = (max_pts - min_pts) as f64 * tb.num as f64 / tb.den as f64;
    if span > 0.0 { (n - 1) as f64 / span } else { 0.0 }
}

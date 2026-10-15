//! Inspeção de mídia via libavformat (porte de `OBSProbe.pas`): codec, faixas,
//! duração (com varredura de pacotes como fallback), bitrate, fps. Tudo
//! in-process, sem ffprobe.exe.

use std::path::Path;

use crate::ffi::ffmpeg::{self as av, AVMEDIA_TYPE_AUDIO, AVMEDIA_TYPE_DATA, AVMEDIA_TYPE_SUBTITLE, AVMEDIA_TYPE_VIDEO};

#[derive(Debug, Clone, Default)]
pub struct StreamInfo {
    pub index: i32,
    /// `video` | `audio` | `subtitle` | `data`
    pub kind: &'static str,
    pub codec: String,
    /// `tags.title` (nome da faixa).
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub channels: i32,
    pub sample_rate: i32,
    pub bit_rate: i64,
    /// segundos
    pub duration: f64,
    /// `avg_frame_rate` (0 = desconhecido)
    pub frame_rate: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ProbeReport {
    pub file_path: String,
    pub format: String,
    pub duration: f64,
    pub bit_rate: i64,
    pub size: u64,
    pub streams: Vec<StreamInfo>,
}

impl ProbeReport {
    pub fn video_stream(&self) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| s.kind == "video")
    }

    pub fn audio_streams(&self) -> impl Iterator<Item = &StreamInfo> {
        self.streams.iter().filter(|s| s.kind == "audio")
    }

    pub fn has_video(&self) -> bool {
        self.video_stream().is_some()
    }

    pub fn audio_track_count(&self) -> usize {
        self.audio_streams().count()
    }
}

fn media_type_to_kind(t: i32) -> &'static str {
    match t {
        AVMEDIA_TYPE_VIDEO => "video",
        AVMEDIA_TYPE_AUDIO => "audio",
        AVMEDIA_TYPE_SUBTITLE => "subtitle",
        AVMEDIA_TYPE_DATA => "data",
        _ => "",
    }
}

/// Fecha o contexto ao sair do escopo (o `try/finally` do Delphi).
struct InputGuard {
    ic: *mut av::AVFormatContext,
}

impl Drop for InputGuard {
    fn drop(&mut self) {
        if let Ok(a) = av::api() {
            unsafe { (a.format.avformat_close_input)(&mut self.ic) }
        }
    }
}

/// Inspeciona `path`. `None` se o arquivo não existe, as DLLs faltam ou o
/// libavformat não consegue abri-lo.
pub fn probe(path: &Path) -> Option<ProbeReport> {
    let size = std::fs::metadata(path).ok()?.len();
    let a = av::api().ok()?;
    let c_path = av::to_utf8(&path.to_string_lossy());

    let mut ic: *mut av::AVFormatContext = std::ptr::null_mut();
    let rc = unsafe {
        (a.format.avformat_open_input)(&mut ic, c_path.as_ptr(), std::ptr::null(), std::ptr::null_mut())
    };
    if rc < 0 || ic.is_null() {
        return None;
    }
    let guard = InputGuard { ic };

    unsafe {
        if (a.format.avformat_find_stream_info)(guard.ic, std::ptr::null_mut()) < 0 {
            return None;
        }

        let mut report = ProbeReport {
            file_path: path.to_string_lossy().into_owned(),
            format: av::iformat_name(guard.ic),
            duration: av::duration(guard.ic) as f64 / av::AV_TIME_BASE as f64,
            bit_rate: av::bit_rate(guard.ic),
            size,
            streams: Vec::new(),
        };

        // Último recurso de duração: varre os pacotes (MKV sem Duration EBML,
        // comum quando o trailer não foi escrito — pegadinha #27).
        if report.duration == 0.0 {
            report.duration = av::scan_duration_by_packets(guard.ic) as f64 / av::AV_TIME_BASE as f64;
            (a.format.av_seek_frame)(guard.ic, -1, 0, av::AVSEEK_FLAG_BACKWARD);
        }
        // Último fallback de bit_rate: tamanho ÷ duração.
        if report.bit_rate == 0 && report.size > 0 && report.duration > 0.0 {
            report.bit_rate = ((report.size * 8) as f64 / report.duration).round() as i64;
        }

        // `for i in 0..n` com n unsigned é seguro em Rust (range vazio com 0),
        // ao contrário do `0 to N-1` do Delphi (pegadinha #24).
        for i in 0..av::nb_streams(guard.ic) {
            let Some(s) = av::stream(guard.ic, i) else { continue };
            let mut info = StreamInfo {
                index: s.index,
                title: av::metadata_string(s.metadata, "title"),
                ..Default::default()
            };
            if let Some(cp) = s.codecpar.as_ref() {
                info.kind = media_type_to_kind(cp.codec_type);
                info.codec = crate::ffi::cstr_to_string((a.codec.avcodec_get_name)(cp.codec_id));
                info.width = cp.width;
                info.height = cp.height;
                info.channels = cp.ch_layout.nb_channels;
                info.sample_rate = cp.sample_rate;
                info.bit_rate = cp.bit_rate;
            }
            // Taxa como fração (suporta NTSC 30000/1001 = 29,97).
            if info.kind == "video" && s.avg_frame_rate.den > 0 {
                info.frame_rate = s.avg_frame_rate.num as f64 / s.avg_frame_rate.den as f64;
            }
            // Arquivo sem taxa declarada (MKV de uma união): mede nos pacotes.
            if info.kind == "video" && info.frame_rate <= 0.0 {
                info.frame_rate = av::estimate_fps_from_packets(guard.ic, i as i32);
            }
            info.duration = if s.duration > 0 && s.time_base.den > 0 {
                (s.duration * s.time_base.num as i64) as f64 / s.time_base.den as f64
            } else {
                report.duration
            };
            report.streams.push(info);
        }
        Some(report)
    }
}

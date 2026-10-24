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

    #[cfg(any(test, feature = "app-tests"))]
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

/// Inspeciona `path`. `None` se o arquivo não existe, as DLLs faltam ou o
/// libavformat não consegue abri-lo.
pub fn probe(path: &Path) -> Option<ProbeReport> {
    // Tamanho pelo sistema de arquivos (o libavformat não expõe direto); 0 se falhar.
    let size = std::fs::metadata(path).map(|m| m.len()).ok()?;
    let a = av::api().ok()?;
    let input = av::Input::open(&path.to_string_lossy()).ok()?;
    let ic = input.ptr();

    unsafe {
        if !input.find_stream_info() {
            return None;
        }

        let mut report = ProbeReport {
            format: av::iformat_name(ic),
            duration: av::container_duration_us(ic) as f64 / av::AV_TIME_BASE as f64,
            bit_rate: av::container_bit_rate(ic),
            size,
            streams: Vec::new(),
        };

        // Último recurso de duração: varre os pacotes (MKV sem Duration EBML,
        // comum quando o trailer não foi escrito — pegadinha #27).
        if report.duration == 0.0 {
            report.duration = av::scan_duration_by_packets(ic) as f64 / av::AV_TIME_BASE as f64;
            // A varredura consome o cursor até o EOF; voltar ao início deixa o
            // contexto limpo para qualquer leitura futura.
            input.rewind();
        }
        // Último fallback de bit_rate: tamanho ÷ duração.
        if report.bit_rate == 0 && report.size > 0 && report.duration > 0.0 {
            report.bit_rate = ((report.size * 8) as f64 / report.duration).round() as i64;
        }

        // `for i in 0..n` com n unsigned é seguro em Rust (range vazio com 0),
        // ao contrário do `0 to N-1` do Delphi (pegadinha #24).
        for i in 0..av::nb_streams(ic) {
            let Some(s) = av::stream(ic, i) else { continue };
            let mut info = StreamInfo {
                index: s.index,
                title: av::stream_title(s),
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
                info.frame_rate = av::estimate_fps_from_packets(ic, i as i32);
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

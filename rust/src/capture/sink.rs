//! Saídas: quem recebe os pacotes codificados. A gravação em arquivo e (na
//! fase 4) o buffer em memória são dois `PacketSink` pendurados nos MESMOS
//! encoders — trocar/somar uma saída não mexe na captura.
//!
//! Referência: `plugins/obs-ffmpeg/obs-ffmpeg-mux.c` (o OBS usa um processo
//! à parte; aqui o libavformat roda numa thread nossa).

use std::ffi::c_void;

use super::encode::StreamInfo;
use crate::ffi::ffmpeg::*;
use crate::log;

/// Recebe pacotes de vários streams (índice = ordem do `create`).
pub trait PacketSink: Send {
    /// `pkt` vem na time_base do encoder (`StreamInfo::time_base`).
    fn write(&mut self, stream: usize, pkt: &mut Packet) -> Result<(), String>;
    /// Fecha o arquivo (trailer). Depois disso o arquivo está COMPLETO.
    fn finish(&mut self) -> Result<(), String>;
}

/// Arquivo Matroska — o único formato seguro contra queda de energia (#10).
pub struct MkvSink {
    oc: *mut AVFormatContext,
    in_tb: Vec<AVRational>,
    finished: bool,
    pub path: String,
    pub packets: u64,
}

// SAFETY: o contexto é usado por uma thread por vez (a de saída).
unsafe impl Send for MkvSink {}

impl MkvSink {
    pub fn create(path: &str, streams: &[StreamInfo]) -> Result<Self, String> {
        let a = api()?;
        let mut oc: *mut AVFormatContext = std::ptr::null_mut();
        // Muxer explícito, nunca pela extensão (#51e).
        let rc = unsafe { (a.format.avformat_alloc_output_context2)(&mut oc, std::ptr::null(), c"matroska".as_ptr(), to_utf8(path).as_ptr()) };
        if rc < 0 || oc.is_null() {
            return Err(format!("avformat_alloc_output_context2: {}", av_err_str(rc)));
        }
        let mut sink = Self { oc, in_tb: Vec::new(), finished: false, path: path.to_owned(), packets: 0 };
        for s in streams {
            let st = unsafe { (a.format.avformat_new_stream)(oc, std::ptr::null()) };
            if st.is_null() {
                return Err("avformat_new_stream".into());
            }
            unsafe {
                let rc = (a.codec.avcodec_parameters_copy)((*st).codecpar, s.params());
                if rc < 0 {
                    return Err(format!("avcodec_parameters_copy: {}", av_err_str(rc)));
                }
                (*st).time_base = s.time_base;
                (*st).avg_frame_rate = s.frame_rate;
                if !s.title.is_empty() {
                    let v = to_utf8(&s.title);
                    (a.util.av_dict_set)(&mut (*st).metadata, c"title".as_ptr(), v.as_ptr(), 0);
                }
            }
            sink.in_tb.push(s.time_base);
        }
        unsafe {
            let mut pb: *mut c_void = std::ptr::null_mut();
            let rc = (a.format.avio_open2)(&mut pb, to_utf8(path).as_ptr(), AVIO_FLAG_WRITE, std::ptr::null(), std::ptr::null_mut());
            if rc < 0 {
                return Err(format!("avio_open2 \"{path}\": {}", av_err_str(rc)));
            }
            set_pb(oc, pb);
            let rc = (a.format.avformat_write_header)(oc, std::ptr::null_mut());
            if rc < 0 {
                return Err(format!("avformat_write_header: {}", av_err_str(rc)));
            }
        }
        Ok(sink)
    }

    fn close_io(&mut self) {
        if let Ok(a) = api() {
            unsafe {
                let pb = (self.oc as *mut u8).add(OFFS_PB) as *mut *mut c_void;
                if !(*pb).is_null() {
                    (a.format.avio_closep)(pb);
                }
            }
        }
    }
}

impl PacketSink for MkvSink {
    fn write(&mut self, stream: usize, pkt: &mut Packet) -> Result<(), String> {
        let a = api()?;
        let Some(&in_tb) = self.in_tb.get(stream) else { return Err(format!("stream {stream} inexistente")) };
        let st = unsafe { stream_at(self.oc, stream) }.ok_or("stream sumiu")?;
        pkt.stream_index = stream as i32;
        unsafe { (a.codec.av_packet_rescale_ts)(pkt.ptr(), in_tb, (*st).time_base) };
        let rc = unsafe { (a.format.av_interleaved_write_frame)(self.oc, pkt.ptr()) };
        if rc < 0 {
            return Err(format!("av_interleaved_write_frame: {}", av_err_str(rc)));
        }
        self.packets += 1;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let a = api()?;
        let rc = unsafe { (a.format.av_write_trailer)(self.oc) };
        self.close_io();
        if rc < 0 {
            return Err(format!("av_write_trailer: {}", av_err_str(rc)));
        }
        log!("Captura: \"{}\" fechado ({} pacotes).", self.path, self.packets);
        Ok(())
    }
}

impl Drop for MkvSink {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.finish();
        }
        if let Ok(a) = api() {
            unsafe { (a.format.avformat_free_context)(self.oc) };
        }
    }
}

/// `AVStream*` de saída pelo índice.
unsafe fn stream_at(oc: *mut AVFormatContext, idx: usize) -> Option<*mut AVStream> {
    stream(oc, idx as u32).map(|s| s as *const AVStream as *mut AVStream)
}

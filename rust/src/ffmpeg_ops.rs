//! Operações de alto nível sobre mídia com libavformat/libavcodec/libavutil/
//! libswscale (porte de `FFmpegOps.pas`). Camada de wrapper limpa sobre
//! `ffi::ffmpeg`: quem consome não vê structs C, ponteiros nem ABI.
//!
//! * [`remux_file`]          — troca o container sem reencodar (MKV → MP4)
//! * [`merge_files`]         — une N arquivos (stream copy)
//! * [`splice_continuation`] — buffer em memória + gravação que continuou dele (#62)
//! * [`extract_audio_tracks`]— separa faixas de áudio em M4A
//! * thumbnails, picos de áudio e keyframes (mais abaixo)
//!
//! Tudo in-process, sem fork de ffmpeg.exe, e seguro de worker thread (libav
//! não tem requisito de main thread — ao contrário do libobs, pegadinha #3).

use std::ffi::c_void;

use crate::ffi::ffmpeg::*;
use crate::log;

/// "Ainda vale continuar?" — `true` = parar. Lida a cada pacote nas operações
/// longas que a transcrição usa, para o Cancelar valer NO MEIO delas.
pub type CancelFn<'a> = &'a dyn Fn() -> bool;

fn detect_container_from_ext(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    match ext.as_str() {
        ".mp4" | ".m4a" | ".m4v" => "mp4",
        ".mkv" => "matroska",
        ".mov" => "mov",
        ".aac" => "adts",
        _ => "mp4",
    }
}

// ---------------------------------------------------------------------------
// Saída (base de RemuxFile, MergeFiles e ExtractAudioTracks)
// ---------------------------------------------------------------------------

/// Um arquivo de saída com os streams espelhados do source. `Drop` escreve o
/// trailer (se o header saiu), fecha o IO e libera o contexto: o `CloseOutput`
/// do Delphi, sem como esquecer.
pub struct OutputFile {
    ctx: *mut AVFormatContext,
    pb: *mut c_void,
    header_written: bool,
    /// índice de stream do source → índice de stream da saída (-1 = ignorar)
    pub stream_map: Vec<i32>,
}

impl OutputFile {
    /// Aloca o contexto de saída, cria streams espelhando `keep`, abre o IO e
    /// escreve o header. `container` vazio = deduz pela extensão; quem escreve
    /// num `.part` passa o muxer explícito (pegadinha #51e).
    pub fn open_for_streams(src: &Input, dst: &str, keep: &[u32], container: Option<&str>) -> Option<Self> {
        let a = api().ok()?;
        let fmt_name = container.unwrap_or_else(|| detect_container_from_ext(dst)).to_owned();
        let dst_c = to_utf8(dst);
        let fmt_c = to_utf8(&fmt_name);

        let mut out = Self { ctx: std::ptr::null_mut(), pb: std::ptr::null_mut(), header_written: false, stream_map: Vec::new() };
        unsafe {
            let rc = (a.format.avformat_alloc_output_context2)(&mut out.ctx, std::ptr::null(), fmt_c.as_ptr(), dst_c.as_ptr());
            if rc < 0 || out.ctx.is_null() {
                return None;
            }

            // Mapeia índices do source para a saída. Default = -1 (ignorar).
            let n = src.nb_streams();
            if n == 0 {
                return None;
            }
            out.stream_map = vec![-1; n as usize];

            for &idx in keep {
                if idx >= n {
                    continue;
                }
                let Some(src_stream) = src.stream(idx) else { continue };
                let dst_stream = (a.format.avformat_new_stream)(out.ctx, std::ptr::null());
                if dst_stream.is_null() {
                    return None;
                }
                if (a.codec.avcodec_parameters_copy)((*dst_stream).codecpar, src_stream.codecpar) < 0 {
                    return None;
                }
                // codec_tag = 0 deixa o muxer escolher conforme o container.
                (*(*dst_stream).codecpar).codec_tag = 0;
                // A taxa de quadros NÃO está no codecpar: sem copiar, o MKV sai sem
                // DefaultDuration e quem lê depois vê avg_frame_rate 0/0 — a união
                // de duas gravações de 60 fps virava "fps desconhecido" (#51m).
                (*dst_stream).avg_frame_rate = src_stream.avg_frame_rate;
                // Preserva SÓ title (nome da faixa) e language; ver copy_stream_tag.
                copy_stream_tag(src_stream, dst_stream, "title");
                copy_stream_tag(src_stream, dst_stream, "language");
                out.stream_map[idx as usize] = (*dst_stream).index;
            }

            let rc = (a.format.avio_open2)(&mut out.pb, dst_c.as_ptr(), AVIO_FLAG_WRITE, std::ptr::null(), std::ptr::null_mut());
            if rc < 0 {
                out.pb = std::ptr::null_mut();
                return None;
            }
            set_pb(out.ctx, out.pb);

            // +faststart para MP4 (move o moov para o início).
            let mut opts = Dict::new();
            if fmt_name == "mp4" || fmt_name == "mov" {
                opts.set("+movflags", "+faststart");
            }
            if (a.format.avformat_write_header)(out.ctx, opts.as_mut_ptr()) < 0 {
                return None;
            }
            out.header_written = true;
        }
        Some(out)
    }

    pub fn stream(&self, idx: u32) -> Option<&AVStream> {
        unsafe { stream(self.ctx, idx) }
    }

    /// `av_interleaved_write_frame`; `false` = falha do muxer (fatal).
    pub fn write(&self, pkt: *mut AVPacket) -> bool {
        match api() {
            Ok(a) => unsafe { (a.format.av_interleaved_write_frame)(self.ctx, pkt) >= 0 },
            Err(_) => false,
        }
    }
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        let Ok(a) = api() else { return };
        unsafe {
            if !self.ctx.is_null() {
                if self.header_written {
                    (a.format.av_write_trailer)(self.ctx);
                }
                if !self.pb.is_null() {
                    (a.format.avio_closep)(&mut self.pb);
                }
                (a.format.avformat_free_context)(self.ctx);
                self.ctx = std::ptr::null_mut();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RemuxFile / ExtractAudioTracks
// ---------------------------------------------------------------------------

/// Loop genérico: abre o source e N saídas, lê pacotes e despacha para cada
/// saída que mapeia o stream. `targets[i]` = streams que vão para `outputs[i]`.
fn remux_dispatch(src: &str, targets: &[Vec<u32>], outputs: &[String], cancel: Option<CancelFn>) -> bool {
    if targets.len() != outputs.len() || targets.is_empty() {
        return false;
    }
    let Ok(a) = api() else { return false };
    let Ok(input) = Input::open(src) else { return false };
    if !input.find_stream_info() {
        return false;
    }

    let outs: Vec<Option<OutputFile>> = targets
        .iter()
        .zip(outputs)
        .map(|(t, o)| OutputFile::open_for_streams(&input, o, t, None))
        .collect();
    if outs.iter().all(Option::is_none) {
        return false;
    }
    let Some(mut pkt) = Packet::alloc() else { return false };

    // av_packet_rescale_ts converte entre a time_base do source e a da saída.
    let write_one = |p: &mut Packet| -> bool {
        // Sem dts depois da retenção (raro): vale o pts, nunca o vazio.
        if p.dts == AV_NOPTS_VALUE {
            p.dts = p.pts;
        }
        for out in outs.iter().flatten() {
            let Some(&dst_idx) = out.stream_map.get(p.stream_index as usize) else { continue };
            if dst_idx < 0 {
                continue;
            }
            let Some(src_stream) = input.stream(p.stream_index as u32) else { continue };
            let Some(dst_stream) = out.stream(dst_idx as u32) else { continue };
            let src_index = src_stream.index;
            // Cópia por saída: o rescale altera o pacote, e a próxima saída quer
            // os timestamps da ORIGEM.
            let (pts, dts, dur) = (p.pts, p.dts, p.duration);
            p.stream_index = dst_idx;
            unsafe { (a.codec.av_packet_rescale_ts)(p.ptr(), src_stream.time_base, dst_stream.time_base) };
            p.pos = -1;
            // Ignorar erro de write produzia MP4 truncado servido ao player como
            // sucesso (disco cheio, codec incompatível): falha de muxer é fatal.
            let ok = out.write(p.ptr());
            if !ok {
                log!("Remux: av_interleaved_write_frame falhou — abortando.");
                return false;
            }
            // Restaura índice e relógio para a próxima saída que queira o pacote.
            p.stream_index = src_index;
            p.pts = pts;
            p.dts = dts;
            p.duration = dur;
        }
        true
    };
    // Começo RETIDO e dts preenchido, como na união: com B-frames o Matroska
    // entrega os primeiros quadros sem dts, e repassá-los vazios dependia do
    // preenchimento obsoleto do libavformat ("Timestamps are unset ... will
    // stop working in the future").
    let mut held: Vec<Packet> = Vec::new();
    let mut eof = false;
    while held.len() < MERGE_HOLD_PKTS {
        if unsafe { (a.format.av_read_frame)(input.ptr(), pkt.ptr()) } != 0 {
            eof = true;
            break;
        }
        let Some(c) = pkt.clone_pkt() else { return false };
        held.push(c);
        pkt.unref();
    }
    fill_missing_dts(&mut held);
    let mut write_failed = false;
    for mut h in held {
        // Cancelado: sai como falha (o arquivo parcial não serve para nada).
        if cancel.is_some_and(|c| c()) || !write_one(&mut h) {
            write_failed = true;
            break;
        }
    }
    while !write_failed && !eof && unsafe { (a.format.av_read_frame)(input.ptr(), pkt.ptr()) } == 0 {
        if cancel.is_some_and(|c| c()) || !write_one(&mut pkt) {
            write_failed = true;
        }
        pkt.unref();
    }
    !write_failed
}

/// Copia TODOS os streams do source para um único output (`ffmpeg -i src -c copy dst`).
pub fn remux_file(src: &str, dst: &str) -> bool {
    if !ffmpeg_lib_available() {
        return false;
    }
    // Mini-probe só para contar streams.
    let keep: Vec<u32> = {
        let Ok(input) = Input::open(src) else { return false };
        if !input.find_stream_info() {
            return false;
        }
        let n = input.nb_streams();
        if n == 0 {
            return false;
        }
        (0..n).collect()
    };
    remux_dispatch(src, &[keep], &[dst.to_owned()], None)
}

/// Cada stream de áudio do source vai para um arquivo M4A separado (AAC stream
/// copy), numa passada. `outputs[j]` recebe o `(j + audio_start_index)`-ésimo
/// stream de áudio: `audio_start_index = 1` pula o mix (faixa 1).
pub fn extract_audio_tracks(src: &str, outputs: &[String], audio_start_index: usize, cancel: Option<CancelFn>) -> bool {
    if !ffmpeg_lib_available() || outputs.is_empty() {
        return false;
    }
    let audio_idxs: Vec<u32> = {
        let Ok(input) = Input::open(src) else { return false };
        if !input.find_stream_info() {
            return false;
        }
        (0..input.nb_streams())
            .filter(|&i| input.stream(i).and_then(|s| unsafe { s.codecpar.as_ref() }).is_some_and(|cp| cp.codec_type == AVMEDIA_TYPE_AUDIO))
            .collect()
    };
    if audio_idxs.is_empty() || outputs.len() + audio_start_index > audio_idxs.len() {
        return false;
    }
    let targets: Vec<Vec<u32>> = (0..outputs.len()).map(|j| vec![audio_idxs[j + audio_start_index]]).collect();
    remux_dispatch(src, &targets, outputs, cancel)
}

// ---------------------------------------------------------------------------
// MergeFiles
// ---------------------------------------------------------------------------

fn stream_params_compatible(r: &AVStream, s: &AVStream) -> bool {
    let (Some(rp), Some(sp)) = (unsafe { r.codecpar.as_ref() }, unsafe { s.codecpar.as_ref() }) else { return false };
    if rp.codec_type != sp.codec_type || rp.codec_id != sp.codec_id {
        return false;
    }
    match rp.codec_type {
        AVMEDIA_TYPE_VIDEO => rp.width == sp.width && rp.height == sp.height,
        AVMEDIA_TYPE_AUDIO => rp.sample_rate == sp.sample_rate && rp.ch_layout.nb_channels == sp.ch_layout.nb_channels,
        _ => true,
    }
}

fn inputs_compatible(r: &Input, s: &Input) -> bool {
    let (n, m) = (r.nb_streams(), s.nb_streams());
    if n == 0 || n != m {
        return false;
    }
    (0..n).all(|i| match (r.stream(i), s.stream(i)) {
        (Some(a), Some(b)) => stream_params_compatible(a, b),
        _ => false,
    })
}

/// Une N arquivos num novo output (stream copy), na ordem recebida. Os inputs
/// precisam ter a mesma estrutura de streams/codec. `Err(true)` = incompatíveis;
/// `Err(false)` = qualquer outra falha.
/// Pacotes do começo de cada arquivo retidos (união, remux e emenda) até os dts
/// que faltam serem conhecidos (com B-frames são os 2–3 primeiros do vídeo).
const MERGE_HOLD_PKTS: usize = 48;

/// Preenche, por faixa, o dts dos pacotes que vieram sem ele, de trás pra
/// frente a partir do próximo conhecido (um tique a menos por pacote). Faixa sem
/// nenhum dts conhecido na retenção: vale o pts.
fn fill_missing_dts(pkts: &mut [Packet]) {
    let mut next_known: std::collections::HashMap<i32, i64> = std::collections::HashMap::new();
    for p in pkts.iter_mut().rev() {
        if p.dts != AV_NOPTS_VALUE {
            next_known.insert(p.stream_index, p.dts);
        } else if let Some(d) = next_known.get_mut(&p.stream_index) {
            // Um tique antes do seguinte, e nunca depois do próprio pts.
            *d = if p.pts != AV_NOPTS_VALUE { (*d - 1).min(p.pts) } else { *d - 1 };
            p.dts = *d;
        } else {
            p.dts = p.pts;
        }
    }
}

pub fn merge_files(inputs: &[String], dst: &str) -> Result<(), bool> {
    if !ffmpeg_lib_available() || inputs.len() < 2 || dst.is_empty() {
        return Err(false);
    }
    let a = api().map_err(|_| false)?;
    let ref_in = Input::open_with_retry(&inputs[0]).ok_or(false)?;
    if !ref_in.find_stream_info() {
        return Err(false);
    }
    let n = ref_in.nb_streams();
    if n == 0 {
        return Err(false);
    }
    let all: Vec<u32> = (0..n).collect();
    let out = OutputFile::open_for_streams(&ref_in, dst, &all, None).ok_or(false)?;
    let mut pkt = Packet::alloc().ok_or(false)?;

    let mut global_offset_us = 0i64;
    let mut write_failed = false;
    for path in inputs {
        if write_failed {
            break;
        }
        let src = Input::open_with_retry(path).ok_or(false)?;
        if !src.find_stream_info() {
            return Err(false);
        }
        if !inputs_compatible(&ref_in, &src) {
            return Err(true);
        }

        let declared_dur_us = unsafe { container_duration_us(src.ptr()) }.max(0);

        // Começo do arquivo RETIDO: com B-frames o demuxer do Matroska entrega os
        // primeiros pacotes de vídeo SEM dts (o MKV só guarda o pts) e só passa a
        // conhecê-lo uns quadros depois. Repassado vazio, o muxer o inventa a
        // partir do arquivo ANTERIOR e a união morre em "non monotonically
        // increasing dts" — toda gravação em x264 ou NVENC (#50). O que falta é
        // preenchido de trás pra frente a partir do 1º dts conhecido da faixa,
        // um tique por pacote: só a ordem de decodificação importa ao muxer.
        let mut held: Vec<Packet> = Vec::new();
        let mut eof = false;
        while held.len() < MERGE_HOLD_PKTS {
            if unsafe { (a.format.av_read_frame)(src.ptr(), pkt.ptr()) } != 0 {
                eof = true;
                break;
            }
            match pkt.clone_pkt() {
                Some(c) => held.push(c),
                None => return Err(false),
            }
            pkt.unref();
        }
        fill_missing_dts(&mut held);

        // Base ÚNICA por arquivo, não uma base POR STREAM: a diferença de início
        // entre os streams é informação de sync — nas gravações do OBS o áudio
        // começa ~20 ms antes do vídeo, e zerar cada stream pelo próprio 1º
        // pacote fazia o áudio escorregar alguns ms a cada trecho unido. É o MENOR
        // instante do começo do arquivo (com os dts preenchidos, o vídeo de
        // B-frames começa ANTES do pts 0), pra nada cair antes do trecho anterior.
        let mut base_us = AV_NOPTS_VALUE;
        for h in &held {
            if let Some(st) = src.stream(h.stream_index.max(0) as u32) {
                let t = ts_to_us(if h.dts != AV_NOPTS_VALUE { h.dts } else { h.pts }, st.time_base);
                if t != AV_NOPTS_VALUE && (base_us == AV_NOPTS_VALUE || t < base_us) {
                    base_us = t;
                }
            }
        }
        if base_us == AV_NOPTS_VALUE {
            base_us = 0;
        }

        let mut local_best_us = 0i64;
        let mut write_one = |p: &mut Packet| -> bool {
            let sidx = p.stream_index;
            if sidx < 0 || sidx as u32 >= n {
                return true;
            }
            let Some(src_stream) = src.stream(sidx as u32) else { return true };
            let dst_idx = out.stream_map[sidx as usize];
            if dst_idx < 0 {
                return true;
            }
            let Some(dst_stream) = out.stream(dst_idx as u32) else { return true };
            let (src_tb, dst_tb) = (src_stream.time_base, dst_stream.time_base);

            // Pacote sem dts DEPOIS da retenção (raro): vale o pts.
            if p.dts == AV_NOPTS_VALUE {
                p.dts = p.pts;
            }
            let mut end_us = 0i64;
            if p.pts != AV_NOPTS_VALUE {
                let pts_us = ts_to_us(p.pts, src_tb);
                if pts_us != AV_NOPTS_VALUE {
                    end_us = pts_us - base_us;
                    p.pts = us_to_ts(end_us.max(0) + global_offset_us, dst_tb);
                } else {
                    p.pts = AV_NOPTS_VALUE;
                }
            }
            if p.dts != AV_NOPTS_VALUE {
                let dts_us = ts_to_us(p.dts, src_tb);
                if dts_us != AV_NOPTS_VALUE {
                    let rel = dts_us - base_us;
                    p.dts = us_to_ts(rel.max(0) + global_offset_us, dst_tb);
                    // O trecho seguinte tem que começar DEPOIS do último dts
                    // escrito (com B-frames ele pode passar do fim pelo pts).
                    end_us = end_us.max(rel + 1);
                } else {
                    p.dts = AV_NOPTS_VALUE;
                }
            }
            if p.duration > 0 {
                let dur_us = ts_to_us(p.duration, src_tb);
                p.duration = if dur_us > 0 { us_to_ts(dur_us, dst_tb) } else { 0 };
                if dur_us > 0 {
                    end_us += dur_us;
                }
            }
            p.stream_index = dst_idx;
            p.pos = -1;
            local_best_us = local_best_us.max(end_us);

            if !out.write(p.ptr()) {
                log!("Merge: av_interleaved_write_frame falhou.");
                return false;
            }
            true
        };

        for mut h in held {
            if !write_one(&mut h) {
                write_failed = true;
                break;
            }
        }
        while !write_failed && !eof && unsafe { (a.format.av_read_frame)(src.ptr(), pkt.ptr()) } == 0 {
            if !write_one(&mut pkt) {
                write_failed = true;
            }
            pkt.unref();
        }

        local_best_us = local_best_us.max(declared_dur_us);
        if local_best_us > 0 {
            global_offset_us += local_best_us;
        }
    }

    let ok = !write_failed;
    log!("Merge: inputs={} durationUs={global_offset_us} result={ok}", inputs.len());
    if ok { Ok(()) } else { Err(false) }
}

// ---------------------------------------------------------------------------
// SpliceContinuation — trecho do buffer + gravação que continuou dele
// ---------------------------------------------------------------------------
//
// Os dois arquivos saíram dos MESMOS encoders (a gravação foi pendurada nos
// encoders do buffer, sem remontar a captura), então os pacotes são idênticos
// byte a byte. O que NÃO bate são os timestamps: cada saída da libobs zera o
// relógio no próprio começo (obs-output.c:2042 na gravação, obs-ffmpeg-mux.c:
// 1163 no "save" do buffer), faixa por faixa. A emenda se ancora pelo CONTEÚDO:
//   1. copia o trecho do buffer inteiro, guardando uma assinatura de cada
//      pacote (tamanho + hash das pontas + flag de keyframe);
//   2. acha no fim dele o 1º pacote de vídeo da gravação (um keyframe) e
//      confirma que os seguintes também batem (corrida de VERIFY_PKTS);
//   3. faz o mesmo em cada faixa de áudio. Faixa MUDA casa em dezenas de
//      lugares (todo pacote de silêncio é igual): usa o deslocamento de uma
//      faixa com som que casou num lugar só — saem do mesmo mixer, nos mesmos
//      instantes, e os dois arquivos cortam todas juntas;
//   4. da gravação, pula exatamente os pacotes que o buffer já tinha e escreve
//      o resto deslocado para o relógio do buffer.
// A contagem é por PACOTE, não por tempo: o MKV guarda ms e arredonda diferente
// nos dois arquivos, então comparar timestamp erraria por 1 tique na borda.

#[derive(Clone, Copy)]
struct PktSig {
    /// pts em µs (dts quando não há pts)
    ts_us: i64,
    size: i32,
    fp: u64,
    key: bool,
}

impl PktSig {
    fn same(&self, o: &PktSig) -> bool {
        self.size == o.size && self.fp == o.fp && self.key == o.key
    }
}

/// FNV-1a 64 só das PONTAS do pacote (4 KB de cada lado): barato mesmo num
/// keyframe 4K de megabytes. Tamanho + pontas distinguem pacotes diferentes na
/// janela curta da busca, e a corrida confirma o resto.
fn packet_fingerprint(p: &AVPacket) -> u64 {
    const EDGE: usize = 4096;
    const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    if p.data.is_null() || p.size <= 0 {
        return 0;
    }
    let d = unsafe { std::slice::from_raw_parts(p.data, p.size as usize) };
    let mix = |mut h: u64, bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(FNV_PRIME); // estourar é o ponto do FNV
        }
        h
    };
    let h = if d.len() <= 2 * EDGE {
        mix(FNV_OFFSET, d)
    } else {
        mix(mix(FNV_OFFSET, &d[..EDGE]), &d[d.len() - EDGE..])
    };
    h ^ p.size as u64
}

/// Os pacotes do começo da gravação batem com os do buffer a partir de `j`?
/// Confere até onde os dois têm pacote — o buffer acaba no meio da corrida.
fn run_matches(head: &[PktSig], j: usize, tail_start: &[PktSig]) -> bool {
    tail_start.iter().enumerate().all(|(k, t)| head.get(j + k).map_or(true, |h| h.same(t)))
}

fn sig_of(ctx: &Input, p: &AVPacket) -> PktSig {
    let mut ts = p.pts;
    if ts == AV_NOPTS_VALUE {
        ts = p.dts;
    }
    let mut ts_us = ctx.stream(p.stream_index as u32).map_or(AV_NOPTS_VALUE, |s| ts_to_us(ts, s.time_base));
    if ts_us == AV_NOPTS_VALUE {
        ts_us = 0;
    }
    PktSig { ts_us, size: p.size, fp: packet_fingerprint(p), key: p.flags & AV_PKT_FLAG_KEY != 0 }
}

/// Escreve `p` (lido de `ctx`) na saída, deslocado de `off_us`. `last_dts` por
/// stream de saída.
fn write_pkt(out: &OutputFile, ctx: &Input, p: &mut Packet, off_us: i64, last_dts: &mut [i64]) -> bool {
    let Some(&dst_idx) = out.stream_map.get(p.stream_index as usize) else { return true };
    if dst_idx < 0 {
        return true;
    }
    let (Some(src_st), Some(dst_st)) = (ctx.stream(p.stream_index as u32), out.stream(dst_idx as u32)) else { return true };
    // Quem chama preenche o começo de cada arquivo (`fill_missing_dts`); sem dts
    // depois disso (raro): vale o pts, nunca o vazio.
    if p.dts == AV_NOPTS_VALUE {
        p.dts = p.pts;
    }
    if p.pts != AV_NOPTS_VALUE {
        p.pts = us_to_ts(ts_to_us(p.pts, src_st.time_base) + off_us, dst_st.time_base);
    }
    if p.dts != AV_NOPTS_VALUE {
        p.dts = us_to_ts(ts_to_us(p.dts, src_st.time_base) + off_us, dst_st.time_base);
        // O ms do MKV arredonda diferente nos dois arquivos: o 1º pacote da
        // gravação pode cair 1 tique ANTES do último do buffer. Segura no último
        // escrito (o Matroska aceita dts igual, AVFMT_TS_NONSTRICT).
        let ld = last_dts[dst_idx as usize];
        if ld != AV_NOPTS_VALUE && p.dts < ld {
            p.dts = ld;
        }
        last_dts[dst_idx as usize] = p.dts;
        if p.pts != AV_NOPTS_VALUE && p.pts < p.dts {
            p.pts = p.dts;
        }
    }
    if p.duration > 0 {
        if let Ok(a) = api() {
            p.duration = unsafe { (a.util.av_rescale_q)(p.duration, src_st.time_base, dst_st.time_base) };
        }
    }
    p.stream_index = dst_idx;
    p.pos = -1;
    let ok = out.write(p.ptr());
    if !ok {
        log!("Splice: av_interleaved_write_frame falhou.");
    }
    ok
}

/// Emenda o trecho do buffer (`head`) com a gravação que continuou dele
/// (`tail`) num MKV único em `dst` (stream copy). `lead_ms` = quanto tempo
/// antes do fim de `head` a gravação começou (desempata keyframes idênticos de
/// tela parada; <= 0 = desconhecido). `Err(motivo)` se não achar o ponto de
/// encontro: quem chama mantém os dois arquivos em vez de emendar às cegas.
/// `Ok(info)` traz o resumo (pacotes repetidos descartados).
pub fn splice_continuation(head: &str, tail: &str, dst: &str, lead_ms: i32) -> Result<String, String> {
    // Pacotes do começo da gravação conferidos em sequência, por faixa.
    const VERIFY_PKTS: usize = 12;
    // Só procura o ponto de encontro nos últimos 15 s do buffer: a gravação
    // começou ~1 s antes do fim dele (ReplaySaveCommitDelayMs).
    const SEARCH_WINDOW_US: i64 = 15 * 1_000_000;
    // Folga do lead_ms: latência do encoder + atraso do timer da UI.
    const LEAD_TOLERANCE_US: i64 = 300 * 1000;
    // Teto do que se lê da gravação antes de decidir (uma faixa sem pacote
    // nenhum não pode segurar o resto).
    const MAX_PREREAD: usize = 20_000;
    // Áudio: candidatos a mais de 2 s da estimativa nem entram.
    const AUDIO_SEARCH_US: i64 = 2 * 1_000_000;

    if !ffmpeg_lib_available() {
        return Err("libav indisponivel".into());
    }
    let a = api().map_err(|_| "libav indisponivel".to_owned())?;

    let h_ctx = Input::open_with_retry(head).filter(Input::find_stream_info).ok_or("nao abriu o trecho do buffer")?;
    let t_ctx = Input::open_with_retry(tail).filter(Input::find_stream_info).ok_or("nao abriu a gravacao")?;
    if !inputs_compatible(&h_ctx, &t_ctx) {
        return Err("faixas diferentes nos dois arquivos".into());
    }

    // inputs_compatible já recusou N = 0.
    let n = h_ctx.nb_streams() as usize;
    let v_idx = (0..n as u32)
        .find(|&i| h_ctx.stream(i).and_then(|s| unsafe { s.codecpar.as_ref() }).is_some_and(|cp| cp.codec_type == AVMEDIA_TYPE_VIDEO))
        .ok_or("sem faixa de video")? as usize;

    let mut h_sigs: Vec<Vec<PktSig>> = vec![Vec::new(); n];
    let mut t_start: Vec<Vec<PktSig>> = vec![Vec::new(); n];
    let mut h_last_us = vec![i64::MIN; n];
    let mut off_us = vec![0i64; n];
    let mut last_dts = vec![AV_NOPTS_VALUE; n];
    let mut skip = vec![0i32; n];
    let mut skipped = vec![0i32; n];
    let all_idx: Vec<u32> = (0..n as u32).collect();

    let out = OutputFile::open_for_streams(&h_ctx, dst, &all_idx, Some("matroska")).ok_or("nao abriu o arquivo de saida")?;
    let mut pkt = Packet::alloc().ok_or("sem memoria")?;

    // 1. Trecho do buffer, inteiro e sem mexer no relógio dele. O começo é
    // RETIDO até o dts que falta ser preenchido (B-frames, ver a união); as
    // assinaturas saem dos pacotes ORIGINAIS, antes disso — são elas que acham o
    // ponto de encontro, e o preenchimento não pode mudá-las.
    let mut write_failed = false;
    let mut held: Vec<Packet> = Vec::new();
    let mut head_eof = false;
    while held.len() < MERGE_HOLD_PKTS {
        if unsafe { (a.format.av_read_frame)(h_ctx.ptr(), pkt.ptr()) } != 0 {
            head_eof = true;
            break;
        }
        let s = pkt.stream_index;
        if s >= 0 && (s as usize) < n {
            let sig = sig_of(&h_ctx, &pkt);
            h_sigs[s as usize].push(sig);
            h_last_us[s as usize] = h_last_us[s as usize].max(sig.ts_us);
            held.push(pkt.clone_pkt().ok_or("sem memoria")?);
        }
        pkt.unref();
    }
    fill_missing_dts(&mut held);
    for mut h in held {
        if !write_pkt(&out, &h_ctx, &mut h, 0, &mut last_dts) {
            write_failed = true;
            break;
        }
    }
    while !write_failed && !head_eof && unsafe { (a.format.av_read_frame)(h_ctx.ptr(), pkt.ptr()) } == 0 {
        let s = pkt.stream_index;
        if s >= 0 && (s as usize) < n {
            let sig = sig_of(&h_ctx, &pkt);
            h_sigs[s as usize].push(sig);
            h_last_us[s as usize] = h_last_us[s as usize].max(sig.ts_us);
            if !write_pkt(&out, &h_ctx, &mut pkt, 0, &mut last_dts) {
                write_failed = true;
            }
        }
        pkt.unref();
    }
    if write_failed {
        return Err("falha de escrita".into());
    }
    if h_sigs[v_idx].is_empty() {
        return Err("trecho do buffer sem video".into());
    }

    // 2. Começo da gravação, guardado em memória até decidir a emenda.
    let mut pending: Vec<Packet> = Vec::new();
    let mut pre_read = 0usize;
    let mut eof = false;
    loop {
        let ready = t_start.iter().all(|v| v.len() >= VERIFY_PKTS);
        if ready || pre_read >= MAX_PREREAD {
            break;
        }
        if unsafe { (a.format.av_read_frame)(t_ctx.ptr(), pkt.ptr()) } != 0 {
            eof = true;
            break;
        }
        pre_read += 1;
        let s = pkt.stream_index;
        if s >= 0 && (s as usize) < n {
            if t_start[s as usize].len() < VERIFY_PKTS {
                t_start[s as usize].push(sig_of(&t_ctx, &pkt));
            }
            match pkt.clone_pkt() {
                Some(c) => pending.push(c),
                None => {
                    pkt.unref();
                    return Err("sem memoria".into());
                }
            }
        }
        pkt.unref();
    }
    if t_start[v_idx].is_empty() {
        return Err("gravacao sem video".into());
    }

    // 3a. Vídeo: o 1º pacote da gravação (keyframe) dentro do buffer.
    let h_end_us = h_last_us[v_idx];
    let mut cands: Vec<usize> = Vec::new(); // do fim para o começo
    for j in (0..h_sigs[v_idx].len()).rev() {
        if h_sigs[v_idx][j].ts_us < h_end_us - SEARCH_WINDOW_US {
            break;
        }
        if h_sigs[v_idx][j].same(&t_start[v_idx][0]) && run_matches(&h_sigs[v_idx], j, &t_start[v_idx]) {
            cands.push(j);
        }
    }
    if cands.is_empty() {
        return Err("ponto de encontro do video nao encontrado".into());
    }
    // Mais de um candidato só com tela PARADA (keyframes idênticos). O certo é o
    // 1º keyframe DEPOIS do instante em que a gravação começou — o fim do buffer
    // menos lead_ms. `cands` vem do fim pro começo, então a última atribuição é
    // o candidato mais cedo que ainda passa do limite.
    let mut best: Option<usize> = None;
    if lead_ms > 0 {
        let thr_us = h_end_us - lead_ms as i64 * 1000 - LEAD_TOLERANCE_US;
        for &c in &cands {
            if h_sigs[v_idx][c].ts_us >= thr_us {
                best = Some(c);
            }
        }
    }
    let best = best.unwrap_or(cands[0]);
    skip[v_idx] = (h_sigs[v_idx].len() - best) as i32;
    let v_anchor_us = h_sigs[v_idx][best].ts_us;
    let tv0_us = t_start[v_idx][0].ts_us;
    off_us[v_idx] = v_anchor_us - tv0_us;
    log!(
        "Splice: video casou em {} candidato(s); {} pacote(s) em comum, emenda em {:.3} s do buffer.",
        cands.len(),
        skip[v_idx],
        v_anchor_us as f64 / 1_000_000.0
    );

    // 3b. Demais faixas. Primeiro os candidatos de cada uma, perto do instante
    // que o vídeo indica.
    let mut a_cands: Vec<Vec<usize>> = vec![Vec::new(); n];
    for s in 0..n {
        if s == v_idx || t_start[s].is_empty() {
            continue;
        }
        let est_us = v_anchor_us + (t_start[s][0].ts_us - tv0_us);
        for j in (0..h_sigs[s].len()).rev() {
            if h_sigs[s][j].ts_us < est_us - AUDIO_SEARCH_US {
                break;
            }
            if h_sigs[s][j].same(&t_start[s][0]) && run_matches(&h_sigs[s], j, &t_start[s]) {
                a_cands[s].push(j);
            }
        }
    }
    // Faixa que casou num lugar SÓ tem o deslocamento exato — e ele vale para as
    // outras: saem do mesmo mixer, pacote a pacote nos mesmos instantes, e os
    // dois arquivos zeram todas no mesmo ponto. Resolve a faixa muda (todo
    // pacote de silêncio é igual) onde estimar pelo vídeo erraria por um quadro
    // de AAC (medido: um pacote sobrando ou faltando em 5 de 8 emendas).
    let ref_off_us = (0..n).find(|&s| s != v_idx && a_cands[s].len() == 1).map(|s| h_sigs[s][a_cands[s][0]].ts_us - t_start[s][0].ts_us);
    for s in 0..n {
        if s == v_idx || t_start[s].is_empty() {
            continue;
        }
        let est_us = match ref_off_us {
            Some(r) => t_start[s][0].ts_us + r,
            None => v_anchor_us + (t_start[s][0].ts_us - tv0_us),
        };
        let best = a_cands[s].iter().copied().min_by_key(|&k| (h_sigs[s][k].ts_us - est_us).abs());
        match best {
            Some(b) => {
                skip[s] = (h_sigs[s].len() - b) as i32;
                off_us[s] = h_sigs[s][b].ts_us - t_start[s][0].ts_us;
            }
            None => {
                // Nada casou (só se o buffer acabou antes do áudio da gravação
                // começar). Vai pela estimativa e corta pelo tempo.
                skip[s] = -1;
                off_us[s] = est_us - t_start[s][0].ts_us;
                log!("Splice: faixa {s} sem pacote casado — emenda pelo tempo.");
            }
        }
    }

    // 4. Resto da gravação, sem o que o buffer já tinha.
    let mut dropped = 0u32;
    let mut write_tail = |p: &mut Packet, skipped: &mut [i32], last_dts: &mut [i64]| -> bool {
        let si = p.stream_index;
        if si < 0 || si as usize >= n {
            return true;
        }
        let si = si as usize;
        if skip[si] < 0 {
            // Faixa sem pacote casado: corta pelo tempo.
            if sig_of(&t_ctx, p).ts_us + off_us[si] <= h_last_us[si] {
                dropped += 1;
                return true;
            }
        } else if skipped[si] < skip[si] {
            skipped[si] += 1;
            dropped += 1;
            return true;
        }
        write_pkt(&out, &t_ctx, p, off_us[si], last_dts)
    };
    // O começo da gravação já está guardado: preenche o dts que falta (as
    // assinaturas dele já foram tiradas, no passo 2).
    fill_missing_dts(&mut pending);
    for mut c in pending.drain(..) {
        if !write_failed && !write_tail(&mut c, &mut skipped, &mut last_dts) {
            write_failed = true;
        }
    }
    while !write_failed && !eof && unsafe { (a.format.av_read_frame)(t_ctx.ptr(), pkt.ptr()) } == 0 {
        if !write_tail(&mut pkt, &mut skipped, &mut last_dts) {
            write_failed = true;
        }
        pkt.unref();
    }
    if write_failed {
        return Err("falha de escrita".into());
    }
    Ok(format!("{dropped} pacote(s) repetido(s) descartado(s)"))
}

// ---------------------------------------------------------------------------
// Quadros (miniatura JPEG, PNG exato)
// ---------------------------------------------------------------------------

/// Codifica UM quadro com `codec` (MJPEG/PNG) e devolve os bytes. A configuração
/// vai por `AVCodecParameters` (ABI estável) + `avcodec_parameters_to_context`,
/// evitando campos do `AVCodecContext` (que não é ABI-estável, pegadinha #28).
fn encode_still(codec: *const AVCodec, w: i32, h: i32, pix_fmt: i32, frame: *mut AVFrame) -> Result<Vec<u8>, String> {
    let a = api().map_err(|_| "libav indisponivel".to_owned())?;
    unsafe {
        let enc = CodecCtx::alloc(codec).ok_or("avcodec_alloc_context3 (encoder) falhou")?;
        let mut par = (a.codec.avcodec_parameters_alloc)();
        if par.is_null() {
            return Err("avcodec_parameters_alloc falhou".into());
        }
        (*par).codec_type = AVMEDIA_TYPE_VIDEO;
        (*par).codec_id = (*codec).id;
        (*par).width = w;
        (*par).height = h;
        (*par).format = pix_fmt;
        let rc = (a.codec.avcodec_parameters_to_context)(enc.ptr(), par);
        (a.codec.avcodec_parameters_free)(&mut par);
        if rc < 0 {
            return Err("avcodec_parameters_to_context (encoder) falhou".into());
        }
        // time_base não está no AVCodecParameters, mas é AVOption documentada.
        (a.util.av_opt_set_q)(enc.ptr(), c"time_base".as_ptr(), AVRational { num: 1, den: 25 }, 0);
        let rc = enc.open(codec);
        if rc < 0 {
            return Err(format!("avcodec_open2 (encoder) falhou (rc={rc})"));
        }
        let mut pkt = Packet::alloc().ok_or("av_packet_alloc falhou")?;
        (*frame).pts = 0;
        let rc = (a.codec.avcodec_send_frame)(enc.ptr(), frame);
        if rc < 0 {
            return Err(format!("avcodec_send_frame falhou (rc={rc})"));
        }
        // Flush — sinaliza o fim do stream para o encoder produzir o pacote.
        (a.codec.avcodec_send_frame)(enc.ptr(), std::ptr::null());
        let rc = (a.codec.avcodec_receive_packet)(enc.ptr(), pkt.ptr());
        if rc != 0 {
            return Err(format!("avcodec_receive_packet falhou (rc={rc})"));
        }
        let bytes = std::slice::from_raw_parts(pkt.data, pkt.size as usize).to_vec();
        pkt.unref();
        Ok(bytes)
    }
}

/// Buffer de imagem + `AVFrame` apontando para ele (o frame não é dono dos dados).
struct ImageFrame {
    frame: Frame,
    _buf: Vec<u8>,
}

fn image_frame(pix_fmt: i32, w: i32, h: i32) -> Option<ImageFrame> {
    let a = api().ok()?;
    let size = unsafe { (a.util.av_image_get_buffer_size)(pix_fmt, w, h, 32) };
    if size <= 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let mut frame = Frame::alloc()?;
    frame.format = pix_fmt;
    frame.width = w;
    frame.height = h;
    unsafe {
        (a.util.av_image_fill_arrays)(frame.data.as_mut_ptr(), frame.linesize.as_mut_ptr(), buf.as_mut_ptr(), pix_fmt, w, h, 32);
    }
    Some(ImageFrame { frame, _buf: buf })
}

/// Extrai um frame em `timestamp_sec` e grava como JPEG (altura `target_height`,
/// proporção preservada). Faz seek para o keyframe anterior e aceita o PRIMEIRO
/// frame decodificado — NÃO decodifica até o timestamp exato (pegadinha #42): em
/// canvas multi-monitor (~4–5K) com AV1/HEVC por software cada frame custa
/// centenas de ms e a miniatura levava ~8 s. O keyframe próximo serve.
pub fn extract_frame_jpeg(src: &str, dst_jpeg: &str, timestamp_sec: i32, target_height: i32) -> bool {
    if !ffmpeg_lib_available() {
        log!("Thumb: libavformat indisponivel.");
        return false;
    }
    let target_height = if target_height <= 0 { 240 } else { target_height };
    match extract_frame_jpeg_inner(src, dst_jpeg, timestamp_sec, target_height) {
        Ok(()) => true,
        Err(e) => {
            log!("Thumb: {e}");
            false
        }
    }
}

fn extract_frame_jpeg_inner(src: &str, dst_jpeg: &str, timestamp_sec: i32, target_height: i32) -> Result<(), String> {
    let a = api().map_err(|e| e)?;
    let file = std::path::Path::new(src).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

    // Limita o find_stream_info: para a miniatura só precisamos de codecpar
    // (w/h/codec_id) + time_base, que já vêm do header do MKV. Sem isso, em
    // AV1/HEVC por software ele decodifica vários frames (8 s+).
    let input = {
        let mut opts = Dict::new();
        opts.set("analyzeduration", "500000"); // 0,5 s de stream
        opts.set("probesize", "2000000"); // 2 MB
        let c_src = to_utf8(src);
        let mut ic: *mut AVFormatContext = std::ptr::null_mut();
        let rc = unsafe { (a.format.avformat_open_input)(&mut ic, c_src.as_ptr(), std::ptr::null(), opts.as_mut_ptr()) };
        if rc < 0 {
            return Err(format!("avformat_open_input falhou para {file}"));
        }
        Input::from_raw(ic)
    };
    if !input.find_stream_info() {
        return Err("avformat_find_stream_info falhou.".into());
    }
    let v_idx = unsafe { first_stream_of_type(input.ptr(), AVMEDIA_TYPE_VIDEO) }.ok_or("nenhum stream de video encontrado.")?;
    let vstream = input.stream(v_idx).ok_or("sem stream")?;
    let par = unsafe { vstream.codecpar.as_ref() }.ok_or("sem codecpar")?;

    let dec = CodecCtx::open_decoder_for(par, false)
        .ok_or_else(|| format!("decoder nao abriu para codec_id={}.", par.codec_id))?;

    let (src_w, src_h) = (par.width, par.height);
    if src_w <= 0 || src_h <= 0 {
        return Err(format!("video sem dimensoes ({src_w}x{src_h})."));
    }
    // Tamanho: altura fixa, largura proporcional, par.
    let target_h = target_height;
    let mut target_w = src_w * target_h / src_h;
    if target_w % 2 != 0 {
        target_w -= 1;
    }
    target_w = target_w.max(16);

    // Seek para o keyframe anterior. time_base inválida (den=0): decodifica do início.
    let mut seek_ts = 0i64;
    if vstream.time_base.den > 0 {
        seek_ts = timestamp_sec as i64 * vstream.time_base.den as i64 / vstream.time_base.num.max(1) as i64;
        unsafe { (a.format.av_seek_frame)(input.ptr(), v_idx as i32, seek_ts, AVSEEK_FLAG_BACKWARD) };
    }

    let mut pkt = Packet::alloc().ok_or("av_packet_alloc falhou")?;
    let frame = Frame::alloc().ok_or("av_frame_alloc falhou")?;
    let mut got = false;
    while !got {
        if unsafe { (a.format.av_read_frame)(input.ptr(), pkt.ptr()) } < 0 {
            break;
        }
        if pkt.stream_index == v_idx as i32
            && unsafe { (a.codec.avcodec_send_packet)(dec.ptr(), pkt.ptr()) } == 0
            && unsafe { (a.codec.avcodec_receive_frame)(dec.ptr(), frame.ptr()) } == 0
        {
            got = true;
        }
        pkt.unref();
    }
    if !got {
        return Err(format!("nao conseguiu decodar nenhum frame >= ts={seek_ts}."));
    }

    // Scale: source → YUVJ420P @ target.
    let pix = frame.format;
    let sws = Sws::new(src_w, src_h, pix, target_w, target_h, AV_PIX_FMT_YUVJ420P, SWS_BICUBIC)
        .ok_or_else(|| format!("sws_getContext falhou (src {src_w}x{src_h} fmt={pix} -> {target_w}x{target_h})."))?;
    let scaled = image_frame(AV_PIX_FMT_YUVJ420P, target_w, target_h).ok_or("av_image_get_buffer_size falhou.")?;
    sws.scale(&frame, src_h, &scaled.frame);

    let encoder = unsafe { (a.codec.avcodec_find_encoder)(AV_CODEC_ID_MJPEG) };
    if encoder.is_null() {
        return Err("encoder MJPEG nao encontrado.".into());
    }
    let bytes = encode_still(encoder, target_w, target_h, AV_PIX_FMT_YUVJ420P, scaled.frame.ptr())?;
    // `std::fs::write` usa CreateFileW (paths com acentos, pegadinha #29).
    std::fs::write(dst_jpeg, &bytes).map_err(|e| format!("gravar {dst_jpeg} falhou: {e}"))?;
    log!("Thumb: {} -> {} bytes ({target_w}x{target_h}).", std::path::Path::new(dst_jpeg).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), bytes.len());
    Ok(())
}

/// Quadro EXATO no instante `sec` (segundos contados do 1º quadro do arquivo),
/// em PNG no tamanho original — sem a perda do JPEG, que misturaria os artefatos
/// dele com os do encoder. Decodifica desde o início: é para clipes curtos (o
/// teste de qualidade, ~5 s), onde isso custa pouco e o seek para o keyframe
/// daria o quadro errado. Passando do fim, fica o último quadro decodificado.
pub fn extract_frame_png(src: &str, dst_png: &str, sec: f64) -> bool {
    if !ffmpeg_lib_available() {
        return false;
    }
    match extract_frame_png_inner(src, dst_png, sec) {
        Ok(()) => true,
        Err(e) => {
            log!("FramePng: {e}");
            false
        }
    }
}

fn extract_frame_png_inner(src: &str, dst_png: &str, sec: f64) -> Result<(), String> {
    let a = api()?;
    let file = std::path::Path::new(src).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let input = Input::open(src).map_err(|_| format!("nao abriu {file}"))?;
    if !input.find_stream_info() {
        return Err("find_stream_info falhou".into());
    }
    let v_idx = unsafe { first_stream_of_type(input.ptr(), AVMEDIA_TYPE_VIDEO) }.ok_or("sem video")?;
    let vstream = input.stream(v_idx).ok_or("sem stream")?;
    let tb = vstream.time_base;
    let par = unsafe { vstream.codecpar.as_ref() }.ok_or("sem codecpar")?;
    let (w, h) = (par.width, par.height);
    if w <= 0 || h <= 0 {
        return Err("sem dimensoes".into());
    }
    // Aqui SIM vale threading (ao contrário da miniatura, pegadinha #52): são
    // dezenas de quadros até o instante pedido.
    let dec = CodecCtx::open_decoder_for(par, true).ok_or("decoder nao abriu")?;

    let mut pkt = Packet::alloc().ok_or("sem memoria")?;
    let mut frame = Frame::alloc().ok_or("sem memoria")?;
    let mut grab = Grab { kept: Frame::alloc().ok_or("sem memoria")?, first_ts: None, have_kept: false, got: false };

    // Um quadro saído do decoder: chegou no instante pedido? Senão guarda como
    // "último visto" (vale se o arquivo acabar antes).
    let pump = |frame: &mut Frame, grab: &mut Grab| {
        while !grab.got {
            if unsafe { (a.codec.avcodec_receive_frame)(dec.ptr(), frame.ptr()) } != 0 {
                break;
            }
            let mut ts = frame.pts;
            if ts == AV_NOPTS_VALUE {
                ts = frame.pkt_dts;
            }
            let first = *grab.first_ts.get_or_insert(ts);
            grab.kept.unref();
            unsafe { (a.util.av_frame_move_ref)(grab.kept.ptr(), frame.ptr()) };
            grab.have_kept = true;
            if ts != AV_NOPTS_VALUE && tb.den > 0 && (ts - first) as f64 * tb.num as f64 / tb.den as f64 >= sec {
                grab.got = true;
            }
        }
    };

    while !grab.got {
        if unsafe { (a.format.av_read_frame)(input.ptr(), pkt.ptr()) } < 0 {
            // Drena o que o decoder segura.
            unsafe { (a.codec.avcodec_send_packet)(dec.ptr(), std::ptr::null()) };
            pump(&mut frame, &mut grab);
            break;
        }
        if pkt.stream_index == v_idx as i32 && unsafe { (a.codec.avcodec_send_packet)(dec.ptr(), pkt.ptr()) } == 0 {
            pump(&mut frame, &mut grab);
        }
        pkt.unref();
    }
    if !grab.have_kept {
        return Err(format!("nenhum quadro decodificado em {file}"));
    }

    // Para RGB no tamanho original: PNG é sem perdas, então o que se vê é
    // exatamente o que o encoder de vídeo entregou.
    let sws = Sws::new(w, h, grab.kept.format, w, h, AV_PIX_FMT_RGB24, SWS_BICUBIC).ok_or("sws_getContext falhou")?;
    let rgb = image_frame(AV_PIX_FMT_RGB24, w, h).ok_or("av_image_get_buffer_size falhou")?;
    sws.scale(&grab.kept, h, &rgb.frame);

    let encoder = unsafe { (a.codec.avcodec_find_encoder_by_name)(c"png".as_ptr()) };
    if encoder.is_null() {
        return Err("encoder png ausente.".into());
    }
    let bytes = encode_still(encoder, w, h, AV_PIX_FMT_RGB24, rgb.frame.ptr())?;
    std::fs::write(dst_png, bytes).map_err(|e| format!("gravar {dst_png} falhou: {e}"))
}

struct Grab {
    kept: Frame,
    first_ts: Option<i64>,
    have_kept: bool,
    got: bool,
}

// ---------------------------------------------------------------------------
// Áudio: picos (waveform) e PCM mono em blocos
// ---------------------------------------------------------------------------

// AVSampleFormat (libavutil/samplefmt.h).
const AV_SAMPLE_FMT_S16: i32 = 1;
const AV_SAMPLE_FMT_FLT: i32 = 3;
const AV_SAMPLE_FMT_S16P: i32 = 6;

/// Decoder da 1ª faixa de áudio de `src`.
struct AudioDecoder {
    input: Input,
    idx: u32,
    dec: CodecCtx,
    sample_rate: i32,
    channels: i32,
}

fn open_audio_decoder(src: &str, need_rate: bool) -> Option<AudioDecoder> {
    let input = Input::open(src).ok()?;
    if !input.find_stream_info() {
        return None;
    }
    let idx = unsafe { first_stream_of_type(input.ptr(), AVMEDIA_TYPE_AUDIO) }?;
    let par = unsafe { input.stream(idx)?.codecpar.as_ref()? };
    let dec = CodecCtx::open_decoder_for(par, false)?;
    let mut sample_rate = par.sample_rate;
    if sample_rate <= 0 {
        if need_rate {
            return None;
        }
        sample_rate = 48000;
    }
    let channels = par.ch_layout.nb_channels.max(1);
    Some(AudioDecoder { input, idx, dec, sample_rate, channels })
}

/// Lê a amostra `s` do canal `ch` de um quadro (FLTP/FLT/S16P/S16), como float.
/// Planar usa um ponteiro por canal em `data[ch]` (limitado a 8, o
/// `AV_NUM_DATA_POINTERS`); interleaved entrelaça tudo em `data[0]`.
unsafe fn frame_sample(f: &AVFrame, fmt: i32, s: usize, ch: usize, nch: usize) -> f32 {
    match fmt {
        AV_SAMPLE_FMT_FLTP => f.data.get(ch).filter(|p| !p.is_null()).map_or(0.0, |p| (*p as *const f32).add(s).read_unaligned()),
        AV_SAMPLE_FMT_FLT => (f.data[0] as *const f32).add(s * nch + ch).read_unaligned(),
        AV_SAMPLE_FMT_S16P => f.data.get(ch).filter(|p| !p.is_null()).map_or(0.0, |p| (*p as *const i16).add(s).read_unaligned() as f32 / 32768.0),
        AV_SAMPLE_FMT_S16 => (f.data[0] as *const i16).add(s * nch + ch).read_unaligned() as f32 / 32768.0,
        _ => 0.0,
    }
}

fn is_planar(fmt: i32) -> bool {
    fmt == AV_SAMPLE_FMT_FLTP || fmt == AV_SAMPLE_FMT_S16P
}

/// Decodifica a 1ª faixa de áudio e calcula picos por bucket para a waveform do
/// player. Cada valor em [0..1] é o pico absoluto daquela fatia. Uma passada
/// linear (~0,5–2 s para 10 min). `cancel` interrompe no meio (→ `None`).
///
/// Acumula num array de ALTA resolução proporcional ao sample count REAL e
/// compacta no fim: independe da duração declarada na metadata (que pode estar
/// errada), então a waveform sempre ocupa 100% da largura. 4000 buckets hi-res.
pub fn compute_audio_peaks(src: &str, buckets: usize, cancel: Option<CancelFn>) -> Option<Vec<f32>> {
    if buckets == 0 || !ffmpeg_lib_available() {
        return None;
    }
    let a = api().ok()?;
    let ad = open_audio_decoder(src, false)?;
    let mut pkt = Packet::alloc()?;
    let mut frame = Frame::alloc()?;

    // Duração só para o log (stream.duration; senão a do container).
    let st = ad.input.stream(ad.idx)?;
    let mut dur_sec = if st.duration > 0 && st.time_base.den > 0 { st.duration as f64 * (st.time_base.num as f64 / st.time_base.den as f64) } else { 0.0 };
    if dur_sec <= 0.0 {
        let us = unsafe { container_duration_us(ad.input.ptr()) };
        if us > 0 {
            dur_sec = us as f64 / AV_TIME_BASE as f64;
        }
    }
    if dur_sec <= 0.0 {
        dur_sec = 60.0;
    }
    log!("Waveform: audio stream={} rate={} ch={} declared dur={dur_sec:.1}s", ad.idx, ad.sample_rate, ad.channels);

    let samples_per_hi = (ad.sample_rate / 20).max(1) as i64; // 50 ms por bucket hi-res
    let mut hi: Vec<f32> = vec![0.0; 4000];
    let mut hi_count = 0usize;
    let mut sample_idx = 0i64;

    // Pico entre TODOS os canais por amostra: áudio só no canal direito (ou com
    // o canal 0 mudo) gerava waveform achatada.
    let consume = |frame: &Frame, hi: &mut Vec<f32>, hi_count: &mut usize, sample_idx: &mut i64| {
        if frame.data[0].is_null() {
            return;
        }
        let fmt = frame.format;
        let mut ch_count = ad.channels as usize;
        if is_planar(fmt) {
            ch_count = ch_count.min(8);
        }
        for s in 0..frame.nb_samples.max(0) as usize {
            let mut v = 0f32;
            for ch in 0..ch_count {
                v = v.max(unsafe { frame_sample(frame, fmt, s, ch, ad.channels as usize) }.abs());
            }
            // Bucket hi-res pelo sample count real; cresce se passar do inicial.
            let hi_idx = (*sample_idx / samples_per_hi) as usize;
            if hi_idx >= hi.len() {
                hi.resize(hi.len() * 2, 0.0);
            }
            if v > hi[hi_idx] {
                hi[hi_idx] = v;
            }
            *hi_count = (*hi_count).max(hi_idx + 1);
            *sample_idx += 1;
        }
    };

    while unsafe { (a.format.av_read_frame)(ad.input.ptr(), pkt.ptr()) } == 0 {
        if cancel.is_some_and(|c| c()) {
            pkt.unref();
            return None;
        }
        if pkt.stream_index == ad.idx as i32 && unsafe { (a.codec.avcodec_send_packet)(ad.dec.ptr(), pkt.ptr()) } == 0 {
            while unsafe { (a.codec.avcodec_receive_frame)(ad.dec.ptr(), frame.ptr()) } == 0 {
                consume(&frame, &mut hi, &mut hi_count, &mut sample_idx);
                frame.unref();
            }
        }
        pkt.unref();
    }
    // Flush: captura qualquer frame final em buffer.
    unsafe { (a.codec.avcodec_send_packet)(ad.dec.ptr(), std::ptr::null()) };
    while unsafe { (a.codec.avcodec_receive_frame)(ad.dec.ptr(), frame.ptr()) } == 0 {
        frame.unref();
    }

    if hi_count == 0 {
        log!("Waveform: 0 samples processed — empty audio?");
        return None;
    }
    // Compacta hi-res → buckets finais, máximo por fatia.
    let mut peaks = vec![0f32; buckets];
    for (i, p) in peaks.iter_mut().enumerate() {
        let start = i * hi_count / buckets;
        let mut end = ((i + 1) * hi_count / buckets).min(hi_count);
        if end <= start {
            end = start + 1;
        }
        for v in &hi[start..end.min(hi.len())] {
            if *v > *p {
                *p = *v;
            }
        }
    }
    let max_p = peaks.iter().cloned().fold(0.0, f32::max);
    log!("Waveform: {sample_idx} samples, {hi_count} hi-res buckets used, peak max={max_p:.4}");
    Some(peaks)
}

/// Decodifica a 1ª faixa de áudio inteira, misturando os canais em mono, e
/// entrega em blocos (`on_block(amostras, taxa)`; `false` interrompe) — sem
/// guardar o arquivo todo em memória (1 h a 48 kHz em float = ~700 MB). Usado
/// pela transcrição local, que reamostra para 16 kHz no caminho. Formatos
/// FLTP/FLT/S16P/S16. `false` = não abriu, sem áudio, formato não suportado ou
/// o callback interrompeu.
pub fn decode_audio_mono(src: &str, on_block: &mut dyn FnMut(&[f32], i32) -> bool) -> bool {
    if !ffmpeg_lib_available() {
        return false;
    }
    let Ok(a) = api() else { return false };
    let Some(ad) = open_audio_decoder(src, true) else { return false };
    let (Some(mut pkt), Some(mut frame)) = (Packet::alloc(), Frame::alloc()) else { return false };
    let mut mono: Vec<f32> = Vec::new();
    let mut aborted = false;

    // Mistura um frame decodificado em `mono` e entrega.
    let mut deliver = |frame: &Frame, mono: &mut Vec<f32>, aborted: &mut bool| {
        let fmt = frame.format;
        if frame.nb_samples <= 0 || frame.data[0].is_null() {
            return;
        }
        if !matches!(fmt, AV_SAMPLE_FMT_FLTP | AV_SAMPLE_FMT_FLT | AV_SAMPLE_FMT_S16P | AV_SAMPLE_FMT_S16) {
            log!("DecodeAudioMono: formato de amostra {fmt} nao suportado.");
            *aborted = true;
            return;
        }
        let mut ch_count = ad.channels as usize;
        if is_planar(fmt) {
            ch_count = ch_count.min(8);
        }
        let n = frame.nb_samples as usize;
        mono.clear();
        mono.reserve(n);
        for s in 0..n {
            let acc: f32 = (0..ch_count).map(|ch| unsafe { frame_sample(frame, fmt, s, ch, ad.channels as usize) }).sum();
            // Média, não soma: dois canais iguais não podem dobrar o volume.
            mono.push(acc / ch_count as f32);
        }
        if !on_block(mono, ad.sample_rate) {
            *aborted = true;
        }
    };

    while !aborted && unsafe { (a.format.av_read_frame)(ad.input.ptr(), pkt.ptr()) } == 0 {
        if pkt.stream_index == ad.idx as i32 && unsafe { (a.codec.avcodec_send_packet)(ad.dec.ptr(), pkt.ptr()) } == 0 {
            while unsafe { (a.codec.avcodec_receive_frame)(ad.dec.ptr(), frame.ptr()) } == 0 {
                deliver(&frame, &mut mono, &mut aborted);
                frame.unref();
                if aborted {
                    break;
                }
            }
        }
        pkt.unref();
    }
    if aborted {
        return false;
    }
    // Flush: o AAC segura o último frame até receber o pacote nulo.
    unsafe { (a.codec.avcodec_send_packet)(ad.dec.ptr(), std::ptr::null()) };
    while unsafe { (a.codec.avcodec_receive_frame)(ad.dec.ptr(), frame.ptr()) } == 0 {
        deliver(&frame, &mut mono, &mut aborted);
        frame.unref();
        if aborted {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Keyframes (grade do avanço por saltos do player — pegadinha #61)
// ---------------------------------------------------------------------------

/// Tempos (s, pts) dos keyframes do 1º stream de vídeo, lidos do ÍNDICE do
/// container (Cues no MKV) — não lê pacote nenhum, poucos ms mesmo em horas de
/// gravação. Devolve também o `avg_frame_rate` (0 se desconhecido). Gravação
/// interrompida (sem os Cues, que vão no trailer): o demuxer do Matroska varre
/// os clusters no seek e monta um índice PARCIAL — vêm os keyframes que
/// sobraram no arquivo (medido). `None` = sem vídeo ou arquivo ilegível.
///
/// Sem `avformat_find_stream_info` de propósito: codecpar/time_base/
/// avg_frame_rate já vêm do cabeçalho do MKV, e ele decodificaria quadros só
/// para confirmar o que o índice já diz.
pub fn list_video_keyframes(src: &str) -> Option<(Vec<f64>, f64)> {
    if !ffmpeg_lib_available() {
        return None;
    }
    let a = api().ok()?;
    let input = Input::open(src).ok()?;
    let v_idx = unsafe { first_stream_of_type(input.ptr(), AVMEDIA_TYPE_VIDEO) }?;
    let vst = input.stream(v_idx)?;
    if vst.time_base.den == 0 {
        return None;
    }
    // O MKV adia a leitura dos Cues até o primeiro seek — sem isto o índice volta
    // vazio mesmo num arquivo íntegro.
    input.rewind();
    let st_ptr = vst as *const AVStream;
    let n = unsafe { (a.format.avformat_index_get_entries_count)(st_ptr) };
    if n <= 0 {
        return None;
    }
    let tb = vst.time_base.num as f64 / vst.time_base.den as f64;
    let mut times = Vec::with_capacity(n as usize);
    for i in 0..n {
        let e = unsafe { (a.format.avformat_index_get_entry)(st_ptr as *mut AVStream, i).as_ref() };
        let Some(e) = e else { continue };
        if e.flags_size & AVINDEX_KEYFRAME == 0 {
            continue;
        }
        times.push(e.timestamp as f64 * tb);
    }
    let fps = if vst.avg_frame_rate.den > 0 { vst.avg_frame_rate.num as f64 / vst.avg_frame_rate.den as f64 } else { 0.0 };
    (!times.is_empty()).then_some((times, fps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_pela_extensao() {
        assert_eq!(detect_container_from_ext("a.MKV"), "matroska");
        assert_eq!(detect_container_from_ext("a.m4a"), "mp4");
        assert_eq!(detect_container_from_ext("a.aac"), "adts");
        assert_eq!(detect_container_from_ext("a.xyz"), "mp4");
    }

    fn sig(size: i32, fp: u64, key: bool) -> PktSig {
        PktSig { ts_us: 0, size, fp, key }
    }

    #[test]
    fn corrida_para_onde_o_buffer_acaba() {
        let head = [sig(1, 1, true), sig(2, 2, false), sig(3, 3, false)];
        // O começo da gravação tem 4 pacotes, mas o buffer só 2 a partir de 1.
        let tail = [sig(2, 2, false), sig(3, 3, false), sig(9, 9, false)];
        assert!(run_matches(&head, 1, &tail));
        assert!(!run_matches(&head, 0, &tail));
    }
}

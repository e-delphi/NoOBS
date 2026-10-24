//! Exportação de gravações com RE-ENCODE (porte de `FFmpegExport.pas`).
//!
//! Camada 2 (wrapper alto), irmã de `ffmpeg_ops`. Enquanto o `ffmpeg_ops` só COPIA
//! pacotes, esta é a única operação do projeto que decodifica e reencoda vídeo,
//! para produzir um arquivo menor e compartilhável.
//!
//! [`export_video`] sabe:
//! * manter N trechos do original e emendá-los (o usuário corta e escolhe quais entram);
//! * escolher quais REGIÕES do canvas entram (monitores/webcams) e recompor as
//!   escolhidas lado a lado, sem o buraco preto de uma região pulada;
//! * reduzir resolução e taxa de quadros (nunca aumenta nenhuma das duas);
//! * escolher o encoder entre os que existem no avcodec empacotado;
//! * controlar a qualidade por CRF (0..51, escala do x264), sempre em VBR de
//!   qualidade constante;
//! * manter as faixas de áudio escolhidas (stream copy) ou mixá-las numa só;
//! * exportar SÓ o áudio, sem decodificar nem encodar vídeo;
//! * acelerar o vídeo (1×..512×) com o áudio esticado mantendo o tom;
//! * queimar a transcrição como legenda nos quadros (`export_captions`).
//!
//! Roda SEMPRE em worker thread (pegadinha #3). O progresso volta pelo callback e
//! o cancelamento é um inteiro lido a cada pacote — quem chama marca de outra thread.

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicI32, Ordering};

use crate::export_captions::{CaptionBurner, CaptionTurn};
use crate::ffi::ffmpeg::*;
use crate::time_stretch::TimeStretch;
use crate::types::{EncoderCaps, GpuVendor, RecordingLayout, RecordingRegion};
use crate::{log};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportResult {
    Ok,
    Canceled,
    NoEncoder,
    Error,
}

/// Um pedaço do original que ENTRA no resultado: em ordem crescente e sem
/// sobreposição (o chamador garante), emendados numa linha do tempo contínua.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExportSegment {
    pub start_sec: f64,
    pub end_sec: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    pub src_path: String,
    pub dst_path: String,
    /// Muxer do libavformat: `mp4` ou `matroska`. Vazio = `mp4`. A extensão do
    /// destino é do chamador — aqui o formato é sempre explícito, nunca deduzido
    /// do nome (o `.part` temporário quebraria a dedução).
    pub container: String,
    /// Regiões do canvas a manter, em qualquer ordem (reordena pelo X original).
    /// VAZIO = canvas inteiro.
    pub regions: Vec<RecordingRegion>,
    /// Altura final desejada. 0 = mantém a composta. Nunca faz upscale.
    pub target_height: i32,
    /// Taxa de quadros final. 0 = a maior possível. Nunca passa da taxa da origem
    /// VEZES A VELOCIDADE (acelerado cabem mais quadros da origem em cada segundo
    /// da saída), nem de `EXPORT_FPS_MAX`. Quadros fora da cadência são
    /// descartados ANTES de compor/escalar.
    pub target_fps: i32,
    /// 1 = normal, 2 = o dobro… até `EXPORT_SPEED_MAX`; fração vale (1,5). Todo
    /// quadro da origem é decodificado — nada de pular por keyframe — e a cadência
    /// da saída escolhe os que entram. O áudio é acelerado junto, mantendo o tom,
    /// e por isso deixa de ser copiado.
    pub speed: f64,
    /// Nome do encoder no libavcodec (`libx264`, `h264_amf`…).
    pub encoder_name: String,
    /// `bicubic` (padrão), `bilinear` ou `area`. Só muda algo com REDUÇÃO de resolução.
    pub scale_algo: String,
    /// Qualidade CONSTANTE na escala do x264 (0 = sem perdas, 51 = pior); fora da
    /// faixa é clampado. Não existe alvo de bitrate: recortar uma região ou reduzir
    /// a resolução já economiza sozinho.
    pub crf: i32,
    pub segments: Vec<ExportSegment>,
    /// Índices de stream DO SOURCE.
    pub audio_streams: Vec<i32>,
    /// Junta as escolhidas numa faixa só.
    pub mix_audio: bool,
    pub mix_title: String,
    /// Encoder de áudio da saída (`libmp3lame`). Vazio = copia as faixas, ou AAC
    /// quando mistura. Preenchido, o áudio é SEMPRE recodificado.
    pub audio_codec: String,
    /// bits/s do áudio recodificado; 0 = `MIX_BITRATE`.
    pub audio_bitrate: i32,
    /// Só o áudio: nenhum stream de vídeo na saída e o vídeo da origem nem é decodificado.
    pub no_video: bool,
    /// Legenda queimada: turnos no relógio da ORIGEM. Vazio = sem legenda.
    pub captions: Vec<CaptionTurn>,
}

/// Um encoder oferecido no dropdown da UI.
#[derive(Debug, Clone)]
pub struct ExportEncoder {
    /// `h264-hw` | `h264-sw` | `av1-hw` | `av1-sw` | `hevc-hw`
    pub id: String,
    /// `h264_amf`, `libx264`…
    pub libav_name: String,
    pub hardware: bool,
}

pub const EXPORT_CRF_MIN: i32 = 0;
pub const EXPORT_CRF_MAX: i32 = 51;
/// Padrão do `export_recording` dos testes (a tela parte do mesmo 23).
#[cfg(any(test, feature = "app-tests"))]
pub const EXPORT_CRF_DEFAULT: i32 = 23;
/// Velocidade máxima da exportação acelerada.
pub const EXPORT_SPEED_MAX: f64 = 512.0;
/// Teto da taxa de quadros: o MKV guarda o tempo em MILISSEGUNDOS (acima de 1000
/// fps dois quadros cairiam no mesmo instante). O SVT-AV1 para em 240 (#51g).
pub const EXPORT_FPS_MAX: i32 = 1000;
pub const EXPORT_FPS_MAX_SVT: i32 = 240;

/// Bitrate da faixa mixada: 192k estéreo cobre voz + áudio de sistema com folga.
const MIX_BITRATE: i32 = 192_000;
/// Keyframe da saída (s): casa com o padrão da gravação.
const OUT_KEYINT_SEC: i32 = 2;

// ---- calibração de qualidade entre encoders (pegadinhas #51b/#53) ------------
// `CRF_ANCHOR` são pontos da escala de REFERÊNCIA (o CRF do x264, que a tela
// mostra). Cada `Q_EXP_*` diz, naquele ponto, qual parâmetro nativo do encoder
// entrega a MESMA qualidade — medido por PSNR sobre quadros reais de tela.
// Âncoras densas no meio (17..28), onde o olho discrimina.
const CRF_ANCHOR: [i32; 16] = [0, 4, 8, 12, 15, 17, 19, 21, 23, 26, 28, 31, 34, 38, 44, 51];
/// SVT-AV1 (crf 1..63). Piso 1: o `crf 0` do wrapper significa "não setado".
/// Satura em 63 — o quantizador MÁXIMO dele — a partir do CRF 31: abaixo
/// disso não existe qualidade menor para pedir (medido: prévia de 10 s 1080p
/// com o mesmo 1015 KB no CRF 34 e no 45). A tela avisa (`SVT_FLOOR_CRF`).
const Q_EXP_SVTAV1: [i32; 16] = [1, 5, 17, 29, 37, 42, 46, 51, 55, 60, 61, 63, 63, 63, 63, 63];
/// AMF AV1 (qp 0..255 pelo libavcodec).
const Q_EXP_AV1AMF: [i32; 16] = [0, 0, 12, 30, 46, 57, 73, 93, 109, 131, 145, 165, 186, 212, 244, 255];
/// AMF H.264 e HEVC (qp 0..51). O HEVC pede qp maior para o mesmo PSNR.
const Q_EXP_H264AMF: [i32; 16] = [0, 6, 11, 16, 19, 21, 23, 25, 27, 30, 32, 35, 38, 42, 48, 51];
const Q_EXP_HEVCAMF: [i32; 16] = [0, 9, 14, 18, 21, 23, 25, 27, 29, 32, 34, 37, 40, 44, 50, 51];

/// Preto em YUV limited range (o que o OBS grava): 0 no Y daria "super preto".
const YUV_BLACK_Y: u8 = 16;
const YUV_BLACK_C: u8 = 128;

/// `Round` do Delphi (banker's).
fn round_i(v: f64) -> i32 {
    v.round_ties_even() as i32
}
fn round_i64(v: f64) -> i64 {
    v.round_ties_even() as i64
}

fn even_down(v: i32) -> i32 {
    v & !1
}

// ---------------------------------------------------------------------------
// Seleção de encoder
// ---------------------------------------------------------------------------

fn encoder_exists(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    match api() {
        Ok(a) => unsafe { !(a.codec.avcodec_find_encoder_by_name)(to_utf8(name).as_ptr()).is_null() },
        Err(_) => false,
    }
}

/// `family`: `h264` | `hevc` | `av1`. Vazio = sem caminho de hardware para esse
/// vendor/família. O FFmpeg do OBS NÃO traz nenhum `*_qsv`, então Intel vai por
/// Media Foundation (`h264_mf`/`hevc_mf`), que não tem AV1.
pub(crate) fn hw_encoder_name(family: &str, vendor: GpuVendor) -> String {
    match vendor {
        GpuVendor::Nvidia => format!("{family}_nvenc"),
        GpuVendor::Amd => format!("{family}_amf"),
        GpuVendor::Intel if family != "av1" => format!("{family}_mf"),
        _ => String::new(),
    }
}

/// Encoders que EXISTEM de fato no avcodec empacotado e servem para esta GPU,
/// a partir das caps detectadas pelo motor (pegadinha #51).
///
/// REGRA DIFERENTE DA GRAVAÇÃO, de propósito: lá o software é último recurso
/// (concorre com o que o usuário faz); aqui a exportação roda em worker, o
/// usuário escolheu esperar, e o formato vale mais que o tempo. Então AV1 é
/// oferecido MESMO SEM hardware AV1, via `libsvtav1` (72 q/s a 1080p contra 8 do
/// `libaom-av1`, que em 4K viraria uma barra de progresso que não anda). Os ramos
/// de HARDWARE seguem condicionados às caps. Não há HEVC por software: o build não
/// tem libx265 e o `hevc_mf` pega o MFT de hardware.
pub fn list_export_encoders(caps: &EncoderCaps) -> Vec<ExportEncoder> {
    let mut out = Vec::new();
    if !ffmpeg_lib_available() {
        return out;
    }
    let mut add = |id: &str, name: String, hw: bool| {
        if encoder_exists(&name) {
            out.push(ExportEncoder { id: id.to_owned(), libav_name: name, hardware: hw });
        }
    };
    if caps.h264_hw {
        add("h264-hw", hw_encoder_name("h264", caps.vendor), true);
    }
    add("h264-sw", "libx264".into(), false);
    if caps.av1_hw {
        add("av1-hw", hw_encoder_name("av1", caps.vendor), true);
    }
    add("av1-sw", "libsvtav1".into(), false);
    if caps.hevc_hw {
        add("hevc-hw", hw_encoder_name("hevc", caps.vendor), true);
    }
    out
}

/// Preferência do usuário (vocabulário do config `codec`, incluindo `auto`) →
/// nome do encoder no libavcodec. Sempre devolve algo utilizável (cai no `libx264`).
pub fn resolve_export_encoder(pref: &str, caps: &EncoderCaps) -> String {
    let list = list_export_encoders(caps);
    if list.is_empty() {
        return "libx264".into();
    }
    let find = |id: &str| list.iter().find(|e| e.id == id).map(|e| e.libav_name.clone());
    let pref = pref.trim().to_lowercase();
    let name = if pref.is_empty() || pref == "auto" {
        // Mesma ordem do select_video_encoder: H.264 hw → x264 → AV1 hw → HEVC hw.
        find("h264-hw").or_else(|| find("h264-sw")).or_else(|| find("av1-hw")).or_else(|| find("hevc-hw"))
    } else {
        find(&pref).or_else(|| find("h264-sw"))
    };
    name.unwrap_or_else(|| "libx264".into())
}

// ---------------------------------------------------------------------------
// Composição das regiões
// ---------------------------------------------------------------------------

/// Uma região do canvas original mapeada para uma faixa do canvas final. Todas as
/// coordenadas já arredondadas para PAR (YUV420 não aceita offset nem dimensão ímpar).
#[derive(Default)]
struct CompRegion {
    src_x: i32,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    dst_x: i32,
    dst_y: i32,
    dst_w: i32,
    dst_h: i32,
    sws: Option<Sws>,
}

/// Monta o mapeamento origem → destino. As regiões escolhidas ficam LADO A LADO
/// na ordem do X original, encostadas: pular o monitor do meio de três não deixa
/// faixa preta. Mesma regra de canvas do `compute_canvas` da gravação: soma das
/// larguras × maior altura. Devolve `(regiões, out_w, out_h)`.
fn build_comp_regions(
    regions: &[RecordingRegion],
    canvas_w: i32,
    canvas_h: i32,
    target_height: i32,
) -> Option<(Vec<CompRegion>, i32, i32)> {
    if canvas_w <= 0 || canvas_h <= 0 {
        return None;
    }
    let mut regs: Vec<CompRegion> = Vec::new();
    // Sem regiões (ou gravação antiga sem layout): canvas inteiro.
    if regions.is_empty() {
        regs.push(CompRegion { src_w: even_down(canvas_w), src_h: even_down(canvas_h), ..Default::default() });
    } else {
        for (i, r) in regions.iter().enumerate() {
            let src_x = even_down(r.x.max(0));
            let src_y = even_down(r.y.max(0));
            let src_w = even_down(r.w.min(canvas_w - src_x));
            let src_h = even_down(r.h.min(canvas_h - src_y));
            if src_w < 2 || src_h < 2 {
                log!("Export: regiao {i} degenerada ({src_w}x{src_h}) — ignorada.");
                continue;
            }
            regs.push(CompRegion { src_x, src_y, src_w, src_h, ..Default::default() });
        }
        if regs.is_empty() {
            return None;
        }
        // Ordena pelo X original (insertion sort — são 1–4 itens; estável).
        regs.sort_by_key(|r| r.src_x);
    }

    // Tamanho nativo da composição.
    let native_w: i32 = regs.iter().map(|r| r.src_w).sum();
    let native_h: i32 = regs.iter().map(|r| r.src_h).max().unwrap_or(0);
    if native_w < 2 || native_h < 2 {
        return None;
    }
    // Fator único de escala, nunca acima de 1 (não inventa pixel).
    let scale = if target_height > 0 && target_height < native_h { target_height as f64 / native_h as f64 } else { 1.0 };

    let mut cursor = 0;
    for r in &mut regs {
        r.dst_w = 2.max(even_down(round_i(r.src_w as f64 * scale)));
        r.dst_h = 2.max(even_down(round_i(r.src_h as f64 * scale)));
        r.dst_x = cursor;
        cursor += r.dst_w;
    }
    let out_w = even_down(cursor);
    let out_h = even_down(regs.iter().map(|r| r.dst_h).max().unwrap_or(2).max(2));
    // Região mais baixa que a maior fica centrada na vertical sobre preto.
    for r in &mut regs {
        r.dst_y = even_down((out_h - r.dst_h) / 2);
    }
    (out_w >= 2 && out_h >= 2).then_some((regs, out_w, out_h))
}

/// Layout de monitores do ARQUIVO EXPORTADO. Cada região do layout original que
/// aparece na saída vira uma região nova nas coordenadas da saída: recortada pelo
/// que entrou (monitor escolhido ou recorte livre), deslocada para onde a
/// composição a pôs e escalada pela redução de resolução. É o que deixa o player
/// do arquivo exportado continuar oferecendo "ver só o monitor X". Mesma
/// composição do `export_video` (`build_comp_regions`): as duas contas nunca divergem.
pub fn compute_export_layout(src: &RecordingLayout, regions: &[RecordingRegion], target_height: i32) -> Option<RecordingLayout> {
    if src.canvas_w <= 0 || src.canvas_h <= 0 {
        return None;
    }
    let (regs, out_w, out_h) = build_comp_regions(regions, src.canvas_w, src.canvas_h, target_height)?;
    let mut layout = RecordingLayout { canvas_w: out_w, canvas_h: out_h, regions: Vec::new() };
    for r in &regs {
        if r.src_w <= 0 || r.src_h <= 0 {
            continue;
        }
        let sx = r.dst_w as f64 / r.src_w as f64;
        let sy = r.dst_h as f64 / r.src_h as f64;
        for k in &src.regions {
            // Interseção do monitor original com o pedaço que entrou.
            let ix = k.x.max(r.src_x);
            let iy = k.y.max(r.src_y);
            let ix2 = (k.x + k.w).min(r.src_x + r.src_w);
            let iy2 = (k.y + k.h).min(r.src_y + r.src_h);
            // Sobra de poucos pixels (arredondamento par nas bordas) não é monitor.
            if ix2 - ix < 8 || iy2 - iy < 8 {
                continue;
            }
            let mut n = k.clone();
            n.x = r.dst_x + round_i((ix - r.src_x) as f64 * sx);
            n.y = r.dst_y + round_i((iy - r.src_y) as f64 * sy);
            n.w = round_i((ix2 - ix) as f64 * sx);
            n.h = round_i((iy2 - iy) as f64 * sy);
            layout.regions.push(n);
        }
    }
    Some(layout)
}

/// Pinta o frame de destino de preto. Só chamado quando as regiões não cobrem o
/// canvas inteiro (alturas diferentes).
fn fill_frame_black(f: &AVFrame, w: i32, h: i32) {
    unsafe {
        for y in 0..h as usize {
            std::ptr::write_bytes(f.data[0].add(y * f.linesize[0] as usize), YUV_BLACK_Y, w as usize);
        }
        for y in 0..(h / 2) as usize {
            std::ptr::write_bytes(f.data[1].add(y * f.linesize[1] as usize), YUV_BLACK_C, (w / 2) as usize);
            std::ptr::write_bytes(f.data[2].add(y * f.linesize[2] as usize), YUV_BLACK_C, (w / 2) as usize);
        }
    }
}

/// Recorta `reg.src_*` de `src` e escala para `reg.dst_*` dentro de `dst`. O
/// recorte sai de graça deslocando os ponteiros de plano — o `linesize` continua
/// sendo o do frame inteiro (pegadinha #51c). Destino planar YUV 4:2:0 8 bits;
/// origem nele ou em NV12 (decode na placa, #51l), com o `Sws` da região criado
/// no mesmo formato. Croma tem metade da resolução nos dois eixos (`/2`).
fn blit_region(reg: &CompRegion, src: &AVFrame, dst: &AVFrame) {
    let Some(sws) = &reg.sws else { return };
    let mut src_data = [std::ptr::null::<u8>(); 8];
    let mut dst_data = [std::ptr::null_mut::<u8>(); 8];
    let mut src_ls = [0 as c_int; 8];
    let mut dst_ls = [0 as c_int; 8];
    for p in 0..3 {
        src_ls[p] = src.linesize[p];
        dst_ls[p] = dst.linesize[p];
    }
    unsafe {
        src_data[0] = src.data[0].add(reg.src_y as usize * src_ls[0] as usize + reg.src_x as usize);
        if src.format == AV_PIX_FMT_NV12 {
            // NV12: um plano só de croma, U e V intercalados. Cada amostra ocupa 2
            // bytes e cobre 2 pixels, então o deslocamento em bytes é o próprio
            // src_x (par).
            src_data[1] = src.data[1].add((reg.src_y / 2) as usize * src_ls[1] as usize + reg.src_x as usize);
        } else {
            src_data[1] = src.data[1].add((reg.src_y / 2) as usize * src_ls[1] as usize + (reg.src_x / 2) as usize);
            src_data[2] = src.data[2].add((reg.src_y / 2) as usize * src_ls[2] as usize + (reg.src_x / 2) as usize);
        }
        dst_data[0] = dst.data[0].add(reg.dst_y as usize * dst_ls[0] as usize + reg.dst_x as usize);
        dst_data[1] = dst.data[1].add((reg.dst_y / 2) as usize * dst_ls[1] as usize + (reg.dst_x / 2) as usize);
        dst_data[2] = dst.data[2].add((reg.dst_y / 2) as usize * dst_ls[2] as usize + (reg.dst_x / 2) as usize);
    }
    sws.scale_raw(&src_data, &src_ls, reg.src_h, &dst_data, &dst_ls);
}

/// Soma as amostras de `src` em `dst`, plano a plano, com clamp. Os dois em FLTP
/// (o decoder de AAC sempre entrega assim). Faixa mono entrando em acumulador
/// estéreo vai para os dois canais.
fn mix_into(dst: *mut AVFrame, src: *mut AVFrame) {
    if dst.is_null() || src.is_null() {
        return;
    }
    unsafe {
        let cnt = (*dst).nb_samples.min((*src).nb_samples);
        if cnt <= 0 {
            return;
        }
        // Planos até onde os dois têm ponteiro. Áudio com mais de 8 canais usa
        // extended_data, que não cobrimos — o OBS grava 1 ou 2.
        for ch in 0..8 {
            if (*dst).data[ch].is_null() {
                break;
            }
            let mut src_ch = ch;
            if (*src).data[src_ch].is_null() {
                src_ch = 0; // mono → os dois lados
            }
            if (*src).data[src_ch].is_null() {
                break;
            }
            let d = std::slice::from_raw_parts_mut((*dst).data[ch] as *mut f32, cnt as usize);
            let s = std::slice::from_raw_parts((*src).data[src_ch] as *const f32, cnt as usize);
            for n in 0..cnt as usize {
                d[n] = (d[n] + s[n]).clamp(-1.0, 1.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Configuração do encoder de vídeo
// ---------------------------------------------------------------------------

/// Vocabulário do app → flag do swscale. Desconhecido/vazio = bicubic (padrão
/// histórico). Medido reduzindo 3840×2160 → 1920×1080: bicubic 4,77 ms/quadro,
/// bilinear 2,69, área 2,54 (o `fast_bilinear` mediu 6,71 e NÃO entra).
fn resolve_scale_flags(algo: &str) -> c_int {
    match algo.trim().to_lowercase().as_str() {
        "bilinear" => SWS_BILINEAR,
        "area" => SWS_AREA,
        _ => SWS_BICUBIC,
    }
}

/// Traduz o CRF de referência (0..51, escala do x264) para o parâmetro nativo do
/// encoder, interpolando entre as âncoras MEDIDAS. Reescalar linearmente não
/// alinha qualidade (medido, CRF 23: x264 46,5 dB · SVT-AV1 crf 28 → 56,6 dB e
/// crf 55 equivale · AMF H.264 qp 23 → 49,9 dB e qp 27 equivale).
fn calibrated_quality(table: &[i32; 16], crf: i32, min: i32, max: i32) -> i32 {
    let last = CRF_ANCHOR.len() - 1;
    if crf <= CRF_ANCHOR[0] {
        return table[0];
    }
    if crf >= CRF_ANCHOR[last] {
        return table[last];
    }
    for i in 0..last {
        if crf >= CRF_ANCHOR[i] && crf <= CRF_ANCHOR[i + 1] {
            let (t0, t1) = (CRF_ANCHOR[i], CRF_ANCHOR[i + 1]);
            let (v0, v1) = (table[i], table[i + 1]);
            if t1 == t0 {
                return v0;
            }
            let r = v0 + round_i((v1 - v0) as f64 * (crf - t0) as f64 / (t1 - t0) as f64);
            return r.clamp(min, max);
        }
    }
    table[last]
}

/// Reescalonamento LINEAR — sobrou só para os encoders ainda NÃO calibrados
/// (NVENC e Media Foundation, sem hardware aqui para medir).
fn scale_crf(crf: i32, max: i32) -> i32 {
    round_i(crf as f64 * (max as f64 / EXPORT_CRF_MAX as f64)).clamp(0, max)
}

/// Traduz o CRF 0..51 para o controle de QUALIDADE CONSTANTE de cada encoder,
/// sempre em VBR: o encoder gasta os bits que o conteúdo pedir.
///
/// Nada de `b`/`maxrate`/`bufsize` — com alvo de bitrate junto, todos voltam ao
/// modo de alvo e o parâmetro de qualidade vira enfeite (no NVENC é literalmente
/// ignorado, daí o `b=0` explícito). `attempt` 0 é a forma preferida; HOJE NENHUM
/// encoder tem plano B (o único era o `qvbr` do AMF, INVERTIDO — maior nível =
/// melhor —, que fazia a tela funcionar ao contrário). `false` = não há tentativa
/// `attempt`: pare de tentar.
pub(crate) fn apply_quality_options(dict: &mut Dict, encoder: &str, crf: i32, attempt: i32) -> bool {
    let e = encoder.to_lowercase();
    let crf = crf.clamp(EXPORT_CRF_MIN, EXPORT_CRF_MAX);
    let has = |s: &str| e.contains(s);

    // x264: CRF nativo, exatamente a mesma escala do controle.
    if e == "libx264" {
        if attempt > 0 {
            return false;
        }
        dict.set("crf", &crf.to_string());
        // 'preset' só existe no x264; num encoder de hardware seria ignorado.
        dict.set("preset", "veryfast");
        return true;
    }
    // SVT-AV1 (AV1 por SOFTWARE da exportação): 'crf' em 0..63. Sem 'preset'
    // explícito: o default (−2) rendeu os 72 quadros/s medidos a 1080p.
    if e == "libsvtav1" {
        if attempt > 0 {
            return false;
        }
        dict.set("crf", &calibrated_quality(&Q_EXP_SVTAV1, crf, 1, 63).to_string());
        return true;
    }
    // NVENC: 'rc=vbr' + 'cq' = VBR guiado por qualidade. Piso 1: 'cq=0' significa
    // "sem alvo de qualidade" (um CRF 0 cairia no controle de bitrate padrão).
    if has("_nvenc") {
        if attempt > 0 {
            return false;
        }
        dict.set("rc", "vbr");
        let cq = if has("av1") { scale_crf(crf, 63).max(1) } else { crf.max(1) };
        dict.set("cq", &cq.to_string());
        dict.set("b", "0");
        return true;
    }
    // AMF: CQP, tentativa única. NÃO usar 'qvbr' (invertido — medido no av1_amf:
    // nível 51 (mínima pedida) → 6414 KB contra nível 1 (máxima) → 2536 KB; o CQP é
    // monotônico e cobre uma faixa 13× maior). Mesma conclusão da gravação (#53).
    if has("_amf") {
        if attempt != 0 {
            return false;
        }
        dict.set("rc", "cqp");
        if has("av1") {
            // No AV1 do AMF o qp vai a 255, não a 51.
            let q = calibrated_quality(&Q_EXP_AV1AMF, crf, 0, 255).to_string();
            dict.set("qp_i", &q);
            dict.set("qp_p", &q);
        } else if has("hevc") || has("h265") {
            let q = calibrated_quality(&Q_EXP_HEVCAMF, crf, 0, 51).to_string();
            dict.set("qp_i", &q);
            dict.set("qp_p", &q);
            // qp_b só existe no AVC — o HEVC do AMF não expõe.
        } else {
            let q = calibrated_quality(&Q_EXP_H264AMF, crf, 0, 51).to_string();
            dict.set("qp_i", &q);
            dict.set("qp_p", &q);
            dict.set("qp_b", &q);
        }
        return true;
    }
    // Media Foundation (hardware da Intel neste build): 'rate_control=quality' é
    // VBR guiado por qualidade, mas a escala é 0..100 e INVERTIDA (100 = melhor).
    if has("_mf") {
        if attempt > 0 {
            return false;
        }
        dict.set("rate_control", "quality");
        dict.set("quality", &(100 - scale_crf(crf, 100)).to_string());
        return true;
    }
    // Encoder que não conhecemos: 'crf' é o nome mais comum. Se ele não consumir, o
    // log de opções ignoradas avisa.
    if attempt > 0 {
        return false;
    }
    dict.set("crf", &crf.to_string());
    true
}

/// Opções que o `avcodec_open2` não consumiu ficam no dicionário. Não é fatal (um
/// 'crf' sobra em encoder de hardware), mas saber disso economiza meia hora quando
/// a qualidade não muda.
fn log_leftover_options(opts: &Dict, what: &str) {
    for k in opts.leftover_keys() {
        log!("Export: opcao \"{k}\" ignorada pelo encoder {what}.");
    }
}

// ---------------------------------------------------------------------------
// Helpers de tempo e de saída
// ---------------------------------------------------------------------------

fn pts_to_sec(pts: i64, tb: AVRational) -> f64 {
    if pts == AV_NOPTS_VALUE || tb.den <= 0 {
        return -1.0;
    }
    pts as f64 * (tb.num as f64 / tb.den as f64)
}

fn sec_to_ts(sec: f64, tb: AVRational) -> i64 {
    if tb.num <= 0 {
        return 0;
    }
    round_i64(sec / (tb.num as f64 / tb.den as f64))
}

/// Contexto de saída com o IO e o trailer sob `Drop` (o `finally` do Delphi). Se
/// saímos no meio (erro/cancelamento) o trailer é escrito mesmo assim e o arquivo
/// parcial é apagado por quem chamou.
struct RawOutput {
    ctx: *mut AVFormatContext,
    pb: *mut c_void,
    header_written: bool,
}

impl RawOutput {
    fn empty() -> Self {
        Self { ctx: std::ptr::null_mut(), pb: std::ptr::null_mut(), header_written: false }
    }

    fn alloc(&mut self, container: &str, dst: &str) -> c_int {
        let Ok(a) = api() else { return -1 };
        unsafe { (a.format.avformat_alloc_output_context2)(&mut self.ctx, std::ptr::null(), to_utf8(container).as_ptr(), to_utf8(dst).as_ptr()) }
    }

    fn new_stream(&self) -> *mut AVStream {
        match api() {
            Ok(a) => unsafe { (a.format.avformat_new_stream)(self.ctx, std::ptr::null()) },
            Err(_) => std::ptr::null_mut(),
        }
    }

    fn open_io(&mut self, dst: &str) -> c_int {
        let Ok(a) = api() else { return -1 };
        let rc = unsafe { (a.format.avio_open2)(&mut self.pb, to_utf8(dst).as_ptr(), AVIO_FLAG_WRITE, std::ptr::null(), std::ptr::null_mut()) };
        if rc >= 0 {
            unsafe { set_pb(self.ctx, self.pb) };
        } else {
            self.pb = std::ptr::null_mut();
        }
        rc
    }

    fn write_header(&mut self, opts: &mut Dict) -> c_int {
        let Ok(a) = api() else { return -1 };
        let rc = unsafe { (a.format.avformat_write_header)(self.ctx, opts.as_mut_ptr()) };
        if rc >= 0 {
            self.header_written = true;
        }
        rc
    }

    fn write_trailer(&mut self) -> c_int {
        let Ok(a) = api() else { return -1 };
        let rc = unsafe { (a.format.av_write_trailer)(self.ctx) };
        self.header_written = false;
        rc
    }

    fn write(&self, pkt: *mut AVPacket) -> c_int {
        match api() {
            Ok(a) => unsafe { (a.format.av_interleaved_write_frame)(self.ctx, pkt) },
            Err(_) => -1,
        }
    }
}

impl Drop for RawOutput {
    fn drop(&mut self) {
        let Ok(a) = api() else { return };
        unsafe {
            if self.ctx.is_null() {
                return;
            }
            if self.header_written {
                (a.format.av_write_trailer)(self.ctx);
            }
            if !self.pb.is_null() {
                (a.format.avio_closep)(&mut self.pb);
            }
            (a.format.avformat_free_context)(self.ctx);
        }
    }
}

/// Device D3D11VA (`AVBufferRef*`); solta depois do decoder, que segura um ref próprio.
struct HwDevice(*mut c_void);

impl Drop for HwDevice {
    fn drop(&mut self) {
        if !self.0.is_null() {
            if let Ok(a) = api() {
                unsafe { (a.util.av_buffer_unref)(&mut self.0) }
            }
        }
    }
}

fn copy_ch_layout(dst: *mut AVFrame, src: *const AVFrame) -> bool {
    let Ok(a) = api() else { return false };
    unsafe { (a.util.av_channel_layout_copy)((dst as *mut u8).add(OFFS_FRAME_CH_LAYOUT) as *mut c_void, (src as *const u8).add(OFFS_FRAME_CH_LAYOUT) as *const c_void) >= 0 }
}

fn set_frame_sample_rate(f: *mut AVFrame, rate: i32) {
    unsafe { ((f as *mut u8).add(OFFS_FRAME_SAMPLE_RATE) as *mut i32).write_unaligned(rate) };
}

/// Uma faixa de áudio selecionada.
struct AudioTrack {
    src_idx: i32,
    /// stream na saída (-1 = entra no mix)
    out_idx: i32,
    src_tb: AVRational,
    /// no modo mix e no acelerado
    dec: Option<CodecCtx>,
    /// já passou do fim do trecho
    done: bool,
    /// acelerado sem mix: índice em `aouts`
    aout: usize,
}

/// Uma saída de áudio RECODIFICADA na exportação acelerada: a mistura, ou uma por
/// faixa escolhida. A fila refaz os quadros no tamanho que o encoder exige (o
/// esticador entrega pedaços de qualquer tamanho).
struct AudioOut {
    enc: *mut AVCodecContext,
    /// `Some` = esta saída é dona do encoder (`false` é o `mix_ctx`, liberado à parte)
    owned_enc: Option<CodecCtx>,
    stream: *mut AVStream,
    /// 1/taxa de amostragem
    tb: AVRational,
    frame_size: usize,
    ch: usize,
    /// modelo do ch_layout (1º quadro)
    tpl: Option<Frame>,
    buf: Vec<Vec<f32>>,
    n: usize,
    /// em `tb` — contagem contínua
    next_pts: i64,
    stretch: Option<TimeStretch>,
}

impl AudioOut {
    fn new(enc: *mut AVCodecContext, tb: AVRational) -> Self {
        Self {
            enc,
            owned_enc: None,
            stream: std::ptr::null_mut(),
            tb,
            frame_size: 1024,
            ch: 0,
            tpl: None,
            buf: Vec::new(),
            n: 0,
            next_pts: 0,
            stretch: None,
        }
    }
}

/// Abre um encoder de áudio FLTP. `ch_layout` copiado do source: as faixas do OBS
/// são sempre ordem nativa (mono/estéreo), então copiar o record é seguro — em
/// ordem CUSTOM o campo `u` seria ponteiro e viraria alias.
fn open_audio_encoder(codec: *const AVCodec, rate: i32, bitrate: i32, layout: &AVChannelLayout) -> Result<CodecCtx, c_int> {
    let a = api().map_err(|_| -1)?;
    let ctx = CodecCtx::alloc(codec).ok_or(-1)?;
    unsafe {
        let mut par = (a.codec.avcodec_parameters_alloc)();
        if par.is_null() {
            return Err(-1);
        }
        (*par).codec_type = AVMEDIA_TYPE_AUDIO;
        (*par).codec_id = (*codec).id;
        (*par).format = AV_SAMPLE_FMT_FLTP;
        (*par).sample_rate = rate;
        (*par).bit_rate = if bitrate > 0 { bitrate as i64 } else { MIX_BITRATE as i64 };
        std::ptr::copy_nonoverlapping(layout as *const AVChannelLayout, &mut (*par).ch_layout, 1);
        let rc = (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par);
        // O layout copiado é só alias (ordem nativa): zera antes de liberar o par.
        std::ptr::write_bytes(&mut (*par).ch_layout as *mut AVChannelLayout, 0, 1);
        (a.codec.avcodec_parameters_free)(&mut par);
        if rc < 0 {
            return Err(rc);
        }
        (a.util.av_opt_set_q)(ctx.ptr(), c"time_base".as_ptr(), AVRational { num: 1, den: rate }, 0);
        (a.util.av_opt_set)(ctx.ptr(), c"flags".as_ptr(), c"+global_header".as_ptr(), 0);
        let rc = ctx.open(codec);
        if rc < 0 {
            return Err(rc);
        }
    }
    Ok(ctx)
}

fn frame_size_of(ctx: *mut AVCodecContext) -> i64 {
    let mut v = 0i64;
    if let Ok(a) = api() {
        unsafe { (a.util.av_opt_get_int)(ctx, c"frame_size".as_ptr(), 0, &mut v) };
    }
    v
}

// ---------------------------------------------------------------------------
// export_video
// ---------------------------------------------------------------------------

/// Chamado com o percentual 0..100 conforme a exportação anda — na thread da
/// exportação; quem consome marshala para a main.
pub type ProgressFn<'a> = &'a dyn Fn(f64);

/// Exporta. `cancel` pode ser `None`; marcar `!= 0` de outra thread cancela (a
/// única transição possível é 0 → 1).
pub fn export_video(opts: &ExportOptions, progress: Option<ProgressFn>, cancel: Option<&AtomicI32>) -> ExportResult {
    if !ffmpeg_lib_available() || opts.src_path.is_empty() || opts.dst_path.is_empty() {
        return ExportResult::Error;
    }
    let Ok(a) = api() else { return ExportResult::Error };
    let Some(src) = Input::open_with_retry(&opts.src_path) else {
        log!("Export: nao conseguiu abrir \"{}\".", std::path::Path::new(&opts.src_path).file_name().unwrap_or_default().to_string_lossy());
        return ExportResult::Error;
    };
    let (Some(pkt), Some(enc_pkt), Some(frame), Some(out_frame), Some(acc_frame), Some(hw_frame)) =
        (Packet::alloc(), Packet::alloc(), Frame::alloc(), Frame::alloc(), Frame::alloc(), Frame::alloc())
    else {
        return ExportResult::Error;
    };

    let mut speed = opts.speed;
    if speed < 1.0 || speed.is_nan() {
        speed = 1.0;
    }
    if speed > EXPORT_SPEED_MAX {
        speed = EXPORT_SPEED_MAX;
    }
    let speed_mode = (speed - 1.0).abs() > 1e-6;
    if !speed_mode {
        speed = 1.0;
    }

    // A ordem dos campos é a ordem em que soltam (o `finally` do Delphi): legenda,
    // saídas de áudio, regiões, quadros, pacotes, faixas, encoders/decoder, device
    // de hardware (DEPOIS do decoder, que segura um ref próprio), saída, entrada.
    let mut ex = Export {
        burner: None,
        aouts: Vec::new(),
        regs: Vec::new(),
        norm_sws: None,
        acc_frame,
        tpl_frame: None,
        out_frame,
        norm_frame: None,
        hw_frame,
        frame,
        enc_pkt,
        pkt,
        tracks: Vec::new(),
        mix_ctx: None,
        enc_ctx: None,
        dec_ctx: None,
        hw_dev: HwDevice(std::ptr::null_mut()),
        out: RawOutput::empty(),
        src,
        opts,
        progress,
        cancel,
        a,
        v_idx: -1,
        vstream: std::ptr::null_mut(),
        vdec_name: String::new(),
        out_vstream: std::ptr::null_mut(),
        out_mix_stream: std::ptr::null_mut(),
        out_w: 0,
        out_h: 0,
        src_fmt: AV_PIX_FMT_NONE,
        out_fps: 30,
        drop_frames: false,
        frame_interval: 0.0,
        next_keep_sec: 0.0,
        video_tb: AVRational { num: 1, den: 1000 },
        enc_tb: AVRational { num: 1, den: 1000 },
        mix_tb: AVRational { num: 1, den: 48000 },
        mix_src_tb: AVRational { num: 1, den: 48000 },
        seg_start_ts: 0,
        seg_end_ts: 0,
        last_enc_pts: i64::MIN,
        pts_collision_logged: false,
        vid_frames_encoded: 0,
        vid_send_err_logged: false,
        out_offset_sec: 0.0,
        total_sec: 0.0,
        seg_start_sec: 0.0,
        seg_end_sec: 0.0,
        needs_fill: false,
        needs_normalize: false,
        do_mix: false,
        video_done: false,
        scale_flags: resolve_scale_flags(&opts.scale_algo),
        last_pct_step: -1,
        failed: false,
        setup_done: false,
        mix_frame_size: 0,
        rechunk: false,
        fifo_buf: Vec::new(),
        fifo_n: 0,
        fifo_ch: 0,
        fifo_pts: 0,
        speed,
        speed_mode,
        next_slot: 0,
        cur_slot: 0,
        probe_fmt: AV_PIX_FMT_NONE,
        hw_decoding: false,
    };
    match ex.run() {
        Ok(()) => ExportResult::Ok,
        Err(r) => r,
    }
}

struct Export<'a> {
    // ---- recursos, na ordem de Drop ----
    burner: Option<CaptionBurner>,
    aouts: Vec<AudioOut>,
    regs: Vec<CompRegion>,
    norm_sws: Option<Sws>,
    acc_frame: Frame,
    tpl_frame: Option<Frame>,
    out_frame: Frame,
    norm_frame: Option<Frame>,
    hw_frame: Frame,
    frame: Frame,
    enc_pkt: Packet,
    pkt: Packet,
    tracks: Vec<AudioTrack>,
    mix_ctx: Option<CodecCtx>,
    enc_ctx: Option<CodecCtx>,
    dec_ctx: Option<CodecCtx>,
    hw_dev: HwDevice,
    out: RawOutput,
    src: Input,

    // ---- entradas ----
    opts: &'a ExportOptions,
    progress: Option<ProgressFn<'a>>,
    cancel: Option<&'a AtomicI32>,
    a: &'static FfmpegApi,

    // ---- estado ----
    v_idx: i32,
    vstream: *mut AVStream,
    /// Nome do decoder de VÍDEO para o log (o laço das faixas de áudio reaproveita a
    /// variável do decoder no Delphi).
    vdec_name: String,
    out_vstream: *mut AVStream,
    out_mix_stream: *mut AVStream,
    out_w: i32,
    out_h: i32,
    src_fmt: i32,
    out_fps: i32,
    drop_frames: bool,
    frame_interval: f64,
    next_keep_sec: f64,
    video_tb: AVRational,
    enc_tb: AVRational,
    mix_tb: AVRational,
    mix_src_tb: AVRational,
    seg_start_ts: i64,
    seg_end_ts: i64,
    /// Último pts entregue ao encoder de vídeo, na time_base DELE (guarda de
    /// monotonicidade em `encode_video_frame`).
    last_enc_pts: i64,
    pts_collision_logged: bool,
    /// Quadros entregues ao encoder: zero num arquivo com vídeo é FALHA, não
    /// "concluído" (sairia só o cabeçalho, 261 bytes).
    vid_frames_encoded: i32,
    vid_send_err_logged: bool,
    /// Linha do tempo da SAÍDA: quanto já foi escrito, em segundos. É o que emenda
    /// um trecho no outro sem buraco.
    out_offset_sec: f64,
    total_sec: f64,
    seg_start_sec: f64,
    seg_end_sec: f64,
    needs_fill: bool,
    needs_normalize: bool,
    do_mix: bool,
    video_done: bool,
    scale_flags: c_int,
    last_pct_step: i32,
    failed: bool,
    setup_done: bool,
    // Fila que refaz os quadros de áudio no tamanho que o encoder exige (só com
    // `rechunk`: hoje o MP3, 1152 amostras).
    mix_frame_size: i64,
    rechunk: bool,
    fifo_buf: Vec<Vec<f32>>,
    fifo_n: usize,
    fifo_ch: usize,
    fifo_pts: i64,
    // Exportação acelerada: o relógio da saída anda `speed` vezes mais devagar que
    // o da origem, o vídeo sai em CFR pela grade de quadros da saída (`cur_slot` =
    // índice do quadro, que é o pts com `enc_tb = 1/out_fps`) e o áudio vai pelas `aouts`.
    speed: f64,
    speed_mode: bool,
    next_slot: i64,
    cur_slot: i64,
    // Decode na placa (#51l)
    probe_fmt: i32,
    hw_decoding: bool,
}

impl<'a> Export<'a> {
    fn is_canceled(&self) -> bool {
        self.cancel.is_some_and(|f| f.load(Ordering::Relaxed) != 0)
    }

    /// `asec` = posição dentro do trecho corrente, no tempo do ORIGINAL. O que conta
    /// para o progresso é o tempo de SAÍDA já produzido.
    fn report_progress(&mut self, asec: f64) {
        let Some(progress) = self.progress else { return };
        if self.total_sec <= 0.0 {
            return;
        }
        let pct = ((self.out_offset_sec + (asec - self.seg_start_sec) / self.speed) / self.total_sec * 100.0).clamp(0.0, 100.0);
        // Só notifica a cada 0,5% — quem consome ainda limita por tempo.
        let step = (pct * 2.0) as i32;
        if step == self.last_pct_step {
            return;
        }
        self.last_pct_step = step;
        progress(pct);
    }

    /// Tira os pacotes prontos do encoder e escreve no muxer. `true` = ok.
    fn drain_encoder(&mut self, ctx: *mut AVCodecContext, stream: *mut AVStream, ctx_tb: AVRational) -> bool {
        if ctx.is_null() || stream.is_null() {
            return true;
        }
        let a = self.a;
        loop {
            let r = unsafe { (a.codec.avcodec_receive_packet)(ctx, self.enc_pkt.ptr()) };
            if r == AVERROR_EAGAIN || r == AVERROR_EOF {
                return true;
            }
            if r < 0 {
                log!("Export: avcodec_receive_packet falhou ({}).", av_err_str(r));
                return false;
            }
            // Vídeo sem duração no pacote (o encoder não a sabe com time_base 1/1000):
            // um quadro na taxa de saída. Sem isso o MP4 fecha a faixa no INÍCIO do
            // último quadro e a taxa lida de volta sai acima da real (30,25 em vez
            // de 30, medido).
            if stream == self.out_vstream && self.enc_pkt.duration <= 0 && self.out_fps > 0 {
                let d = unsafe { (a.util.av_rescale_q)(1, AVRational { num: 1, den: self.out_fps }, ctx_tb) };
                self.enc_pkt.duration = d.max(1);
            }
            let ok = unsafe {
                self.enc_pkt.stream_index = (*stream).index;
                (a.codec.av_packet_rescale_ts)(self.enc_pkt.ptr(), ctx_tb, (*stream).time_base);
                self.enc_pkt.pos = -1;
                let w = self.out.write(self.enc_pkt.ptr());
                if w < 0 {
                    log!("Export: av_interleaved_write_frame falhou ({}).", av_err_str(w));
                }
                w >= 0
            };
            self.enc_pkt.unref();
            if !ok {
                return false;
            }
        }
    }

    // ---- fila do MP3 (quadros de 1152) ------------------------------------------

    /// Tira da fila quadros de EXATAMENTE `mix_frame_size` amostras e manda para o
    /// encoder. `final_`: fim da exportação — o que sobrar sai num quadro menor (o
    /// libmp3lame aceita o último curto). O quadro é criado aqui — o que a mistura
    /// normal evita (#51d) porque exige o ch_layout, que fica fora da parte
    /// declarada do AVFrame: vem copiado do quadro-modelo pelas posições medidas.
    fn fifo_emit(&mut self, final_: bool) -> bool {
        let a = self.a;
        while self.fifo_n as i64 >= self.mix_frame_size || (final_ && self.fifo_n > 0) {
            let n = self.fifo_n.min(self.mix_frame_size.max(1) as usize);
            let Some(mut f) = Frame::alloc() else { return false };
            f.nb_samples = n as i32;
            f.format = AV_SAMPLE_FMT_FLTP;
            set_frame_sample_rate(f.ptr(), self.mix_tb.den);
            let Some(tpl) = &self.tpl_frame else { return false };
            if !copy_ch_layout(f.ptr(), tpl.ptr()) {
                return false;
            }
            let r = unsafe { (a.util.av_frame_get_buffer)(f.ptr(), 0) };
            if r < 0 {
                log!("Export: av_frame_get_buffer (audio) falhou ({}).", av_err_str(r));
                return false;
            }
            for ch in 0..self.fifo_ch {
                if !f.data[ch].is_null() {
                    unsafe { std::ptr::copy_nonoverlapping(self.fifo_buf[ch].as_ptr(), f.data[ch] as *mut f32, n) };
                }
            }
            f.pts = self.fifo_pts;
            let mix = self.mix_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
            let r = unsafe { (a.codec.avcodec_send_frame)(mix, f.ptr()) };
            if r < 0 {
                log!("Export: avcodec_send_frame (audio) falhou ({}).", av_err_str(r));
                return false;
            }
            drop(f);
            // Tira as N amostras da frente da fila.
            for ch in 0..self.fifo_ch {
                if self.fifo_n > n {
                    self.fifo_buf[ch].copy_within(n..self.fifo_n, 0);
                }
            }
            self.fifo_n -= n;
            self.fifo_pts += n as i64;
            let (stream, tb) = (self.out_mix_stream, self.mix_tb);
            if !self.drain_encoder(mix, stream, tb) {
                return false;
            }
        }
        true
    }

    /// Põe o acumulador (já com pts no `mix_tb`) no fim da fila e manda o que
    /// formar quadro inteiro.
    fn fifo_push(&mut self) -> bool {
        let n = self.acc_frame.nb_samples.max(0) as usize;
        if n == 0 {
            return true;
        }
        if self.tpl_frame.is_none() {
            // Modelo do layout de canais: o do primeiro quadro que chegou.
            let Some(t) = Frame::alloc() else { return false };
            if !copy_ch_layout(t.ptr(), self.acc_frame.ptr()) {
                return false;
            }
            self.tpl_frame = Some(t);
            self.fifo_ch = 1;
            while self.fifo_ch < 8 && !self.acc_frame.data[self.fifo_ch].is_null() {
                self.fifo_ch += 1;
            }
            self.fifo_buf = vec![Vec::new(); self.fifo_ch];
        }
        if self.fifo_n == 0 && self.acc_frame.pts != AV_NOPTS_VALUE {
            self.fifo_pts = self.acc_frame.pts;
        }
        for ch in 0..self.fifo_ch {
            if self.fifo_buf[ch].len() < self.fifo_n + n {
                self.fifo_buf[ch].resize((self.fifo_n + n) * 2, 0.0);
            }
            let dst = &mut self.fifo_buf[ch][self.fifo_n..self.fifo_n + n];
            if self.acc_frame.data[ch].is_null() {
                dst.iter_mut().for_each(|v| *v = 0.0);
            } else {
                dst.copy_from_slice(unsafe { std::slice::from_raw_parts(self.acc_frame.data[ch] as *const f32, n) });
            }
        }
        self.fifo_n += n;
        self.fifo_emit(false)
    }

    // ---- saídas de áudio da exportação acelerada (aouts) --------------------------
    // Mesma ideia da fila do MP3, generalizada: N saídas, cada uma com o seu encoder
    // e o esticador de tempo na frente.

    /// Manda para o encoder os quadros completos da fila (`final_`: o resto num
    /// quadro curto — AAC e MP3 aceitam o último menor).
    fn ao_emit(&mut self, idx: usize, final_: bool) -> bool {
        let a = self.a;
        loop {
            let (n, frame_size) = (self.aouts[idx].n, self.aouts[idx].frame_size);
            if !(n >= frame_size || (final_ && n > 0)) {
                return true;
            }
            if self.aouts[idx].tpl.is_none() {
                return false;
            }
            let cnt = n.min(frame_size);
            let Some(mut f) = Frame::alloc() else { return false };
            f.nb_samples = cnt as i32;
            f.format = AV_SAMPLE_FMT_FLTP;
            set_frame_sample_rate(f.ptr(), self.aouts[idx].tb.den);
            if !copy_ch_layout(f.ptr(), self.aouts[idx].tpl.as_ref().map_or(std::ptr::null(), |t| t.ptr())) {
                return false;
            }
            let r = unsafe { (a.util.av_frame_get_buffer)(f.ptr(), 0) };
            if r < 0 {
                log!("Export: av_frame_get_buffer (audio acelerado) falhou ({}).", av_err_str(r));
                return false;
            }
            let ch = self.aouts[idx].ch;
            for c in 0..ch {
                if !f.data[c].is_null() {
                    unsafe { std::ptr::copy_nonoverlapping(self.aouts[idx].buf[c].as_ptr(), f.data[c] as *mut f32, cnt) };
                }
            }
            f.pts = self.aouts[idx].next_pts;
            let r = unsafe { (a.codec.avcodec_send_frame)(self.aouts[idx].enc, f.ptr()) };
            if r < 0 {
                log!("Export: avcodec_send_frame (audio acelerado) falhou ({}).", av_err_str(r));
                return false;
            }
            drop(f);
            self.aouts[idx].next_pts += cnt as i64;
            for c in 0..ch {
                if self.aouts[idx].n > cnt {
                    let end = self.aouts[idx].n;
                    self.aouts[idx].buf[c].copy_within(cnt..end, 0);
                }
            }
            self.aouts[idx].n -= cnt;
            let (enc, stream, tb) = (self.aouts[idx].enc, self.aouts[idx].stream, self.aouts[idx].tb);
            if !self.drain_encoder(enc, stream, tb) {
                return false;
            }
        }
    }

    /// Move a saída pronta do esticador para o fim da fila e emite.
    fn ao_take_stretched(&mut self, idx: usize) -> bool {
        {
            let ao = &mut self.aouts[idx];
            let ch = ao.ch;
            if let Some(st) = ao.stretch.as_mut() {
                let cnt = st.out_n;
                if cnt > 0 {
                    for c in 0..ch {
                        if ao.buf[c].len() < ao.n + cnt {
                            ao.buf[c].resize(8192.max((ao.n + cnt) * 2), 0.0);
                        }
                        ao.buf[c][ao.n..ao.n + cnt].copy_from_slice(&st.outp[c][..cnt]);
                    }
                    ao.n += cnt;
                    st.out_n = 0;
                }
            }
        }
        self.ao_emit(idx, false)
    }

    /// Um quadro decodificado (FLTP) do trecho corrente entra na saída.
    fn ao_push(&mut self, idx: usize, frame: *mut AVFrame) -> bool {
        let (nb, speed) = (unsafe { (*frame).nb_samples }, self.speed);
        if nb <= 0 {
            return true;
        }
        if self.aouts[idx].tpl.is_none() {
            // Layout de canais da saída: o do primeiro quadro (as faixas do OBS são
            // sempre mono ou estéreo, ordem nativa).
            let Some(t) = Frame::alloc() else { return false };
            if !copy_ch_layout(t.ptr(), frame) {
                return false;
            }
            let mut ch = 1usize;
            while ch < 8 && unsafe { !(*frame).data[ch].is_null() } {
                ch += 1;
            }
            let ao = &mut self.aouts[idx];
            ao.tpl = Some(t);
            ao.ch = ch;
            ao.buf = vec![Vec::new(); ch];
            ao.stretch = Some(TimeStretch::new(ch, ao.tb.den, speed));
        }
        if let Some(st) = self.aouts[idx].stretch.as_mut() {
            unsafe { st.put(Some(&(*frame).data), nb as usize) };
            st.process();
        }
        self.ao_take_stretched(idx)
    }

    /// Fecha o trecho: a saída passa a ter EXATAMENTE o tamanho da linha do tempo
    /// até aqui (o esticador completa com silêncio ou corta o excesso). É o que
    /// impede o áudio de escorregar do vídeo emenda após emenda.
    fn ao_end_segment(&mut self, idx: usize) -> bool {
        let target = round_i64((self.out_offset_sec + (self.seg_end_sec - self.seg_start_sec) / self.speed) * self.aouts[idx].tb.den as f64);
        let ao = &mut self.aouts[idx];
        if ao.stretch.is_none() {
            // Faixa que ainda não entregou nenhum quadro: não há layout para montar
            // silêncio. O primeiro áudio dela começa no ponto certo da linha do tempo
            // (o muxer deixa o buraco antes).
            ao.next_pts = target;
            return true;
        }
        // Em velocidade alta a 1ª janela do trecho sai "de graça", então o trecho
        // pode ter produzido até uma janela a mais que a linha do tempo. O excesso
        // ainda na fila é cortado aqui; o que já foi para o encoder o trecho seguinte
        // desconta (o alvo é absoluto).
        let excess = (ao.next_pts + ao.n as i64) - target;
        if excess > 0 {
            ao.n -= (ao.n as i64).min(excess) as usize;
        }
        let more = (target - (ao.next_pts + ao.n as i64)).max(0);
        if let Some(st) = ao.stretch.as_mut() {
            st.finish(more);
        }
        self.ao_take_stretched(idx)
    }

    fn ao_finish(&mut self, idx: usize) -> bool {
        if !self.ao_emit(idx, true) {
            return false;
        }
        let (enc, stream, tb) = (self.aouts[idx].enc, self.aouts[idx].stream, self.aouts[idx].tb);
        if enc.is_null() {
            return true;
        }
        unsafe { (self.a.codec.avcodec_send_frame)(enc, std::ptr::null()) };
        self.drain_encoder(enc, stream, tb)
    }

    // ---- mistura ---------------------------------------------------------------

    /// Manda o acumulador de áudio para o encoder e limpa. O pts vem na time_base da
    /// FAIXA de origem (não na do vídeo): tira o início do trecho e soma o offset da
    /// linha do tempo de saída.
    fn flush_mix_frame(&mut self) -> bool {
        if self.acc_frame.nb_samples <= 0 {
            return true;
        }
        // Acelerado: a mistura é a saída 0 das aouts (esticador + fila). O pts lá é
        // uma contagem contínua, então o deste quadro não interessa.
        if self.speed_mode {
            let ok = self.ao_push(0, self.acc_frame.ptr());
            self.acc_frame.unref();
            return ok;
        }
        if self.acc_frame.pts != AV_NOPTS_VALUE {
            let a = self.a;
            let moved = self.acc_frame.pts - sec_to_ts(self.seg_start_sec, self.mix_src_tb);
            self.acc_frame.pts = unsafe { (a.util.av_rescale_q)(moved, self.mix_src_tb, self.mix_tb) } + sec_to_ts(self.out_offset_sec, self.mix_tb);
        }
        if self.rechunk {
            let ok = self.fifo_push();
            self.acc_frame.unref();
            return ok;
        }
        let mix = self.mix_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
        let r = unsafe { (self.a.codec.avcodec_send_frame)(mix, self.acc_frame.ptr()) };
        self.acc_frame.unref();
        if r < 0 {
            log!("Export: avcodec_send_frame (mix) falhou ({}).", av_err_str(r));
            return false;
        }
        let (stream, tb) = (self.out_mix_stream, self.mix_tb);
        self.drain_encoder(mix, stream, tb)
    }

    /// Adota o frame recém-decodificado como acumulador: evita CRIAR um frame de
    /// áudio do zero, o que exigiria preencher o `ch_layout` (fora da parte
    /// declarada do AVFrame).
    fn adopt_accumulator(&mut self) -> bool {
        unsafe { (self.a.util.av_frame_move_ref)(self.acc_frame.ptr(), self.frame.ptr()) };
        let ok = unsafe { (self.a.util.av_frame_make_writable)(self.acc_frame.ptr()) } >= 0;
        if !ok {
            log!("Export: av_frame_make_writable (mix) falhou.");
            self.acc_frame.unref();
        }
        ok
    }

    /// Decodifica o pacote corrente de uma faixa selecionada e acumula. As faixas do
    /// NoOBS saem do mesmo encoder de áudio do OBS, com os mesmos timestamps e 1024
    /// amostras por quadro — acumular por pts idêntico basta, sem buffer de
    /// realinhamento.
    fn handle_mix_packet(&mut self, track: usize) -> bool {
        let a = self.a;
        let dec = self.tracks[track].dec.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
        let r = unsafe { (a.codec.avcodec_send_packet)(dec, self.pkt.ptr()) };
        if r < 0 {
            return true; // pacote solto logo após o seek: ignora
        }
        loop {
            let r = unsafe { (a.codec.avcodec_receive_frame)(dec, self.frame.ptr()) };
            if r == AVERROR_EAGAIN || r == AVERROR_EOF {
                return true;
            }
            if r < 0 {
                return false;
            }
            let ok = 'f: {
                if self.frame.format != AV_SAMPLE_FMT_FLTP {
                    break 'f true;
                }
                let sec = pts_to_sec(self.frame.pts, self.tracks[track].src_tb);
                if sec < self.seg_start_sec || sec >= self.seg_end_sec {
                    break 'f true;
                }
                if self.acc_frame.nb_samples <= 0 {
                    break 'f self.adopt_accumulator();
                }
                if self.acc_frame.pts == self.frame.pts {
                    mix_into(self.acc_frame.ptr(), self.frame.ptr());
                    break 'f true;
                }
                // Quadro de outro instante: fecha o atual e recomeça.
                self.flush_mix_frame() && self.adopt_accumulator()
            };
            self.frame.unref();
            if !ok {
                return false;
            }
        }
    }

    /// Acelerado sem mistura: cada faixa é decodificada e vai, esticada, para a
    /// própria saída. (Copiar o pacote, como no 1×, não dá: o tempo muda.)
    fn handle_speed_track_packet(&mut self, track: usize) -> bool {
        let a = self.a;
        let dec = self.tracks[track].dec.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
        let r = unsafe { (a.codec.avcodec_send_packet)(dec, self.pkt.ptr()) };
        if r < 0 {
            return true;
        }
        loop {
            let r = unsafe { (a.codec.avcodec_receive_frame)(dec, self.frame.ptr()) };
            if r == AVERROR_EAGAIN || r == AVERROR_EOF {
                return true;
            }
            if r < 0 {
                return false;
            }
            let ok = 'f: {
                if self.frame.format != AV_SAMPLE_FMT_FLTP {
                    break 'f true;
                }
                let sec = pts_to_sec(self.frame.pts, self.tracks[track].src_tb);
                if sec < self.seg_start_sec || sec >= self.seg_end_sec {
                    break 'f true;
                }
                self.ao_push(self.tracks[track].aout, self.frame.ptr())
            };
            self.frame.unref();
            if !ok {
                return false;
            }
        }
    }

    // ---- vídeo -------------------------------------------------------------------

    fn encode_video_frame(&mut self, mut pts: i64) -> bool {
        // Guarda de monotonicidade. Só morde no SVT-AV1, cuja time_base é a do FPS de
        // saída: se a origem tiver cadência irregular, dois quadros podem cair no
        // MESMO tique e o encoder recusa pts repetido — falha no meio de uma
        // exportação longa. Nos outros a time_base é a da origem e isto nunca dispara.
        if self.last_enc_pts != i64::MIN && pts <= self.last_enc_pts {
            if !self.pts_collision_logged {
                self.pts_collision_logged = true;
                log!("Export: pts repetido apos quantizar pro time_base do encoder; empurrando 1 tique (so este aviso).");
            }
            pts = self.last_enc_pts + 1;
        }
        self.last_enc_pts = pts;
        self.out_frame.pts = pts;
        let enc = self.enc_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
        let r = unsafe { (self.a.codec.avcodec_send_frame)(enc, self.out_frame.ptr()) };
        if r < 0 {
            log!("Export: avcodec_send_frame (video) falhou ({}).", av_err_str(r));
            return false;
        }
        self.vid_frames_encoded += 1;
        let (stream, tb) = (self.out_vstream, self.enc_tb);
        self.drain_encoder(enc, stream, tb)
    }

    /// Roda uma vez, no primeiro quadro útil — só aí sabemos o pixel format real
    /// que o decoder entrega.
    fn setup_scalers(&mut self) -> bool {
        self.src_fmt = self.frame.format;
        // NV12 é o que o decode na placa entrega. Tem caminho rápido próprio no
        // blit_region: normalizar para YUV420P seria uma passada extra de swscale
        // sobre o quadro 4K inteiro.
        self.needs_normalize = !matches!(self.src_fmt, AV_PIX_FMT_YUV420P | AV_PIX_FMT_YUVJ420P | AV_PIX_FMT_NV12);
        if self.needs_normalize {
            // Origem exótica (10 bits, RGB…): converte o quadro inteiro para YUV420P
            // uma vez e recorta dali. As gravações do app caem sempre num caminho
            // rápido e nunca passam por aqui.
            log!("Export: pix_fmt {} nao e YUV420P — normalizando cada quadro.", self.src_fmt);
            // Bicubic fixo de propósito: origem e destino têm o MESMO tamanho, então
            // não há reamostragem e o algoritmo escolhido não mudaria nada.
            let (w, h) = (self.frame.width, self.frame.height);
            let Some(sws) = Sws::new(w, h, self.src_fmt, w, h, AV_PIX_FMT_YUV420P, SWS_BICUBIC) else { return false };
            self.norm_sws = Some(sws);
            let Some(mut nf) = Frame::alloc() else { return false };
            nf.format = AV_PIX_FMT_YUV420P;
            nf.width = w;
            nf.height = h;
            if unsafe { (self.a.util.av_frame_get_buffer)(nf.ptr(), 0) } < 0 {
                return false;
            }
            self.norm_frame = Some(nf);
        }
        // Um SwsContext por região, criado uma vez e reusado em todos os quadros. A
        // escala do target_height já está embutida aqui — uma passada só, sem canvas
        // intermediário. O algoritmo é o do usuário; só pesa quando há redução.
        let reg_fmt = if self.src_fmt == AV_PIX_FMT_NV12 { AV_PIX_FMT_NV12 } else { AV_PIX_FMT_YUV420P };
        let flags = self.scale_flags;
        for r in &mut self.regs {
            r.sws = Sws::new(r.src_w, r.src_h, reg_fmt, r.dst_w, r.dst_h, AV_PIX_FMT_YUV420P, flags);
            if r.sws.is_none() {
                log!("Export: sws_getContext falhou ({}x{} -> {}x{}).", r.src_w, r.src_h, r.dst_w, r.dst_h);
                return false;
            }
        }
        true
    }

    /// Monta o quadro de saída a partir do decodificado e manda para o encoder.
    fn compose_and_encode(&mut self) -> bool {
        let a = self.a;
        // O encoder pode reter referência do frame que recebeu: NUNCA reescreva
        // out_frame sem passar por aqui (pegadinha #51c).
        if unsafe { (a.util.av_frame_make_writable)(self.out_frame.ptr()) } < 0 {
            log!("Export: av_frame_make_writable (video) falhou.");
            return false;
        }
        // Só precisa pintar quando alguma região é mais baixa que o canvas — as
        // áreas cobertas por blit são sempre reescritas.
        if self.needs_fill {
            fill_frame_black(&self.out_frame, self.out_w, self.out_h);
        }
        let src_frame: &AVFrame = if self.needs_normalize {
            if let (Some(sws), Some(nf)) = (&self.norm_sws, &self.norm_frame) {
                sws.scale(&self.frame, self.frame.height, nf);
            }
            self.norm_frame.as_deref().unwrap_or(&self.frame)
        } else {
            &self.frame
        };
        for r in &self.regs {
            blit_region(r, src_frame, &self.out_frame);
        }
        // Legenda por CIMA da composição, no relógio da ORIGEM — os turnos da
        // transcrição estão nele, e os cortes não importam aqui.
        let frame_sec = pts_to_sec(self.frame.pts, self.video_tb);
        if let Some(b) = self.burner.as_mut() {
            b.blend_at(frame_sec, &mut self.out_frame);
        }
        // Emenda na linha do tempo de saída: posição dentro do trecho mais o que já
        // foi escrito pelos trechos anteriores. O pts do quadro está na time_base da
        // ORIGEM; o encoder espera na dele (no-op quando coincidem — todo encoder
        // menos o SVT-AV1). Acelerado: o pts é o lugar na grade (saída em taxa
        // constante, sem quantizar o relógio da origem).
        if self.speed_mode {
            self.encode_video_frame(self.cur_slot)
        } else {
            let pts = unsafe { (a.util.av_rescale_q)(self.frame.pts - self.seg_start_ts, self.video_tb, self.enc_tb) }
                + sec_to_ts(self.out_offset_sec, self.enc_tb);
            self.encode_video_frame(pts)
        }
    }

    /// Tira do decoder tudo o que já está pronto e manda para composição/encode.
    /// Roda em DOIS lugares: depois de cada pacote de vídeo e no DRENO do fim do
    /// trecho (com threading em quadros o decoder segura vários quadros dentro dele,
    /// e sem o segundo uso o fim de cada trecho sairia cortado — pegadinha #52).
    fn pump_decoder(&mut self) {
        let a = self.a;
        let dec = self.dec_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
        loop {
            let r = unsafe { (a.codec.avcodec_receive_frame)(dec, self.frame.ptr()) };
            if r == AVERROR_EAGAIN || r == AVERROR_EOF {
                break;
            }
            if r < 0 {
                log!("Export: avcodec_receive_frame falhou ({}).", av_err_str(r));
                self.failed = true;
                break;
            }
            let mut brk = false;
            'q: {
                // Quadros antes do início existem só como referência do GOP —
                // decodifica e descarta.
                if self.frame.pts == AV_NOPTS_VALUE || self.frame.pts < self.seg_start_ts {
                    break 'q;
                }
                if self.frame.pts >= self.seg_end_ts {
                    // O decoder entrega em ordem de APRESENTAÇÃO, então dali pra
                    // frente todo pts é maior: não há quadro do trecho preso lá dentro
                    // e o dreno nem é preciso neste caminho.
                    self.video_done = true;
                    brk = true;
                    break 'q;
                }
                // Redução de taxa: só passam os quadros que caem na cadência pedida. A
                // decisão vem ANTES de compor/escalar para o descartado custar zero. O
                // relógio é o da SAÍDA, então a cadência atravessa a emenda dos trechos.
                if self.speed_mode {
                    // Acelerado: o quadro ocupa o último lugar da grade da saída que já
                    // passou (relógio da saída = origem / velocidade). Lugar já
                    // preenchido = descarta. Lugar pulado (origem mais rala que a grade)
                    // fica com o quadro anterior na tela — o pts só salta.
                    let out_sec = self.out_offset_sec + (pts_to_sec(self.frame.pts, self.video_tb) - self.seg_start_sec) / self.speed;
                    self.cur_slot = (out_sec * self.out_fps as f64 + 1e-6).floor() as i64;
                    if self.cur_slot < self.next_slot {
                        break 'q;
                    }
                    self.next_slot = self.cur_slot + 1;
                } else if self.drop_frames {
                    let out_sec = self.out_offset_sec + (pts_to_sec(self.frame.pts, self.video_tb) - self.seg_start_sec);
                    if out_sec + 1e-9 < self.next_keep_sec {
                        break 'q;
                    }
                    loop {
                        self.next_keep_sec += self.frame_interval;
                        if self.next_keep_sec > out_sec {
                            break;
                        }
                    }
                }
                // Decode na placa: o quadro é uma textura. Traz para a memória (NV12)
                // SÓ AGORA, depois das decisões de descarte: a cópia custa ~8 ms num
                // quadro 4K — mais que o próprio decode —, e os quadros antes do trecho
                // e os da redução de fps não precisam dela.
                if self.frame.format == AV_PIX_FMT_D3D11 {
                    let mut r = unsafe { (a.util.av_hwframe_transfer_data)(self.hw_frame.ptr(), self.frame.ptr(), 0) };
                    // A transferência só copia a imagem; pts e cia vêm à parte.
                    if r >= 0 {
                        r = unsafe { (a.util.av_frame_copy_props)(self.hw_frame.ptr(), self.frame.ptr()) };
                    }
                    if r < 0 {
                        log!("Export: copia do quadro da placa falhou ({}).", av_err_str(r));
                        self.hw_frame.unref();
                        self.failed = true;
                        brk = true;
                        break 'q;
                    }
                    self.frame.unref();
                    unsafe { (a.util.av_frame_move_ref)(self.frame.ptr(), self.hw_frame.ptr()) };
                }
                if !self.setup_done {
                    if !self.setup_scalers() {
                        self.failed = true;
                        brk = true;
                        break 'q;
                    }
                    self.setup_done = true;
                }
                if !self.compose_and_encode() {
                    self.failed = true;
                    brk = true;
                    break 'q;
                }
                let sec = pts_to_sec(self.frame.pts, self.video_tb);
                self.report_progress(sec);
            }
            self.frame.unref();
            if brk {
                break;
            }
        }
    }

    /// Decodifica do COMEÇO até sair UM quadro (no máximo 120 pacotes de vídeo).
    /// Existe porque um decoder pode abrir sem erro e recusar TODO pacote: o libaom
    /// (decoder de AV1 do build) com o AV1 que a NVENC grava — o laço de exportação
    /// pulava os recusados em silêncio e o arquivo saía com 261 bytes, "concluído"
    /// (pegadinha #51k). O seek do trecho logo depois devolve a leitura ao lugar certo.
    fn probe_video_decode(&mut self) -> (bool, c_int) {
        const PROBE_PACKETS: u32 = 120;
        let a = self.a;
        let dec = self.dec_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
        let mut err = 0;
        let mut n = 0u32;
        let mut ok = false;
        self.probe_fmt = AV_PIX_FMT_NONE;
        unsafe {
            (a.format.av_seek_frame)(self.src.ptr(), -1, 0, AVSEEK_FLAG_BACKWARD);
            (a.codec.avcodec_flush_buffers)(dec);
        }
        while n < PROBE_PACKETS && unsafe { (a.format.av_read_frame)(self.src.ptr(), self.pkt.ptr()) } == 0 {
            let mut stop = false;
            'p: {
                if self.pkt.stream_index != self.v_idx {
                    break 'p;
                }
                n += 1;
                let r = unsafe { (a.codec.avcodec_send_packet)(dec, self.pkt.ptr()) };
                if r < 0 {
                    err = r;
                    break 'p;
                }
                let r = unsafe { (a.codec.avcodec_receive_frame)(dec, self.frame.ptr()) };
                if r == 0 {
                    self.probe_fmt = self.frame.format;
                    self.frame.unref();
                    ok = true;
                    stop = true;
                    break 'p;
                }
                if r != AVERROR_EAGAIN && r != AVERROR_EOF {
                    err = r;
                }
            }
            self.pkt.unref();
            if stop {
                break;
            }
        }
        // Com threading em quadros o 1º quadro pode estar preso no decoder.
        if !ok && unsafe { (a.codec.avcodec_send_packet)(dec, std::ptr::null()) } >= 0 && unsafe { (a.codec.avcodec_receive_frame)(dec, self.frame.ptr()) } == 0 {
            self.probe_fmt = self.frame.format;
            self.frame.unref();
            ok = true;
        }
        // Sai do modo dreno e esquece o que decodificou: o trecho recomeça.
        unsafe { (a.codec.avcodec_flush_buffers)(dec) };
        (ok, err)
    }

    fn vstream_par(&self) -> Option<&AVCodecParameters> {
        unsafe { self.vstream.as_ref().and_then(|s| s.codecpar.as_ref()) }
    }

    /// Troca o decoder de vídeo por outro do build, pelo NOME. `false` = não existe
    /// ou não abriu (o atual continua).
    fn switch_video_decoder(&mut self, name: &str) -> bool {
        let a = self.a;
        let d = unsafe { (a.codec.avcodec_find_decoder_by_name)(to_utf8(name).as_ptr()) };
        if d.is_null() {
            return false;
        }
        let Some(c2) = CodecCtx::alloc(d) else { return false };
        let par = self.vstream_par().map_or(std::ptr::null(), |p| p as *const AVCodecParameters);
        if unsafe { (a.codec.avcodec_parameters_to_context)(c2.ptr(), par) } < 0 || c2.open(d) < 0 {
            return false;
        }
        self.dec_ctx = Some(c2);
        self.vdec_name = unsafe { crate::ffi::cstr_to_string((*d).name) };
        true
    }

    /// Decoder de SOFTWARE padrão do build para o codec da origem (para o AV1, o
    /// libaom). Substitui o atual, se houver.
    fn open_sw_video_decoder(&mut self) -> bool {
        self.dec_ctx = None;
        let Some(par) = self.vstream_par() else { return false };
        let codec_id = par.codec_id;
        let a = self.a;
        let d = unsafe { (a.codec.avcodec_find_decoder)(codec_id) };
        if d.is_null() {
            log!("Export: decoder nao encontrado (codec_id={codec_id}).");
            return false;
        }
        let Some(ctx) = CodecCtx::alloc(d) else { return false };
        let par = self.vstream_par().map_or(std::ptr::null(), |p| p as *const AVCodecParameters);
        if unsafe { (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par) } < 0 {
            return false;
        }
        // O default do libavcodec para 'threads' é 1 — NÃO "automático". Sem isto o
        // decode de um canvas 4K roda num núcleo só (a máquina parece ociosa, ~6% no
        // gerenciador, e a exportação arrasta): 173 → 435 quadros/s medidos. Ligar
        // isto EXIGE o dreno do decoder no fim do trecho (pump_decoder).
        ctx.set_threads_auto();
        self.vdec_name = unsafe { crate::ffi::cstr_to_string((*d).name) };
        let rc = ctx.open(d);
        if rc < 0 {
            log!("Export: avcodec_open2 (decoder) falhou ({}).", av_err_str(rc));
            return false;
        }
        self.dec_ctx = Some(ctx);
        true
    }

    /// Decoder NATIVO do libavcodec com aceleração D3D11VA: o decode vai para a
    /// placa (qualquer fabricante). Medido numa gravação AV1 4K: 538 quadros/s na
    /// placa contra ~45–60 do libaom — que era o gargalo da exportação inteira. O
    /// escolhido é o NATIVO (`av1`, não o libaom que `avcodec_find_decoder` devolve):
    /// só o nativo tem hwaccel, e sem a placa ele não decodifica AV1 nenhum, então o
    /// `probe_video_decode` confere que saíram quadros D3D11 de verdade. O get_format
    /// padrão escolhe sozinho o formato de hardware com o `hw_device_ctx`
    /// preenchido; `threads` fica no default (1): o trabalho é da placa (#51l).
    fn open_hw_video_decoder(&mut self) -> bool {
        let a = self.a;
        let Some(par) = self.vstream_par() else { return false };
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
        if self.hw_dev.0.is_null() {
            let mut dev: *mut c_void = std::ptr::null_mut();
            let r = unsafe { (a.util.av_hwdevice_ctx_create)(&mut dev, AV_HWDEVICE_TYPE_D3D11VA, std::ptr::null(), std::ptr::null_mut(), 0) };
            if r < 0 {
                log!("Export: sem D3D11VA pra decodificar na placa ({}).", av_err_str(r));
                return false;
            }
            self.hw_dev = HwDevice(dev);
        }
        let Some(c2) = CodecCtx::alloc(d) else { return false };
        let par = self.vstream_par().map_or(std::ptr::null(), |p| p as *const AVCodecParameters);
        if unsafe { (a.codec.avcodec_parameters_to_context)(c2.ptr(), par) } < 0 {
            return false;
        }
        // Ref próprio do contexto: o avcodec_free_context o solta. Sem setter nem
        // AVOption para este campo: vai por offset (achado MEDINDO — #51l).
        unsafe {
            ((c2.ptr() as *mut u8).add(OFFS_CODECCTX_HW_DEVICE_CTX) as *mut *mut c_void).write_unaligned((a.util.av_buffer_ref)(self.hw_dev.0));
        }
        let r = c2.open(d);
        if r < 0 {
            log!("Export: decoder \"{name}\" com D3D11VA nao abriu ({}).", av_err_str(r));
            return false;
        }
        self.dec_ctx = Some(c2);
        self.vdec_name = unsafe { crate::ffi::cstr_to_string((*d).name) };
        true
    }
}

fn has_stream_of(src: &Input, nb: u32, kind: i32) -> Option<u32> {
    (0..nb).find(|&i| src.stream(i).and_then(|s| unsafe { s.codecpar.as_ref() }).is_some_and(|cp| cp.codec_type == kind))
}

impl<'a> Export<'a> {
    /// Corpo do `ExportVideo`. `Err(código)` carrega o resultado de qualquer saída
    /// antecipada (o `Exit(erX)` do Delphi); o padrão de uma saída sem código é `Error`.
    fn run(&mut self) -> Result<(), ExportResult> {
        use ExportResult::{Canceled, Error, NoEncoder};
        let a = self.a;
        let opts = self.opts;

        if !self.src.find_stream_info() {
            return Err(Error);
        }
        let nb_streams = self.src.nb_streams();
        if nb_streams == 0 {
            return Err(Error);
        }

        // ---- stream de vídeo ----
        let mut audio_only = opts.no_video;
        self.v_idx = has_stream_of(&self.src, nb_streams, AVMEDIA_TYPE_VIDEO).map_or(-1, |i| i as i32);
        if self.v_idx < 0 {
            // Origem só de áudio (uma exportação só de áudio reexportada): não há o
            // que compor, e o caminho de só áudio serve inteiro.
            if !audio_only {
                log!("Export: origem sem stream de video — exportando so o audio.");
            }
            audio_only = true;
        }
        if self.v_idx >= 0 {
            self.vstream = self.src.stream(self.v_idx as u32).map_or(std::ptr::null_mut(), |s| s as *const AVStream as *mut AVStream);
            self.video_tb = unsafe { (*self.vstream).time_base };
            if self.video_tb.den <= 0 {
                return Err(Error);
            }
        }
        self.enc_tb = self.video_tb;
        if audio_only && opts.audio_streams.is_empty() {
            log!("Export: so audio pedido sem nenhuma faixa de audio.");
            return Err(Error);
        }

        let mut fps = 30;
        if !self.vstream.is_null() {
            let avg = unsafe { (*self.vstream).avg_frame_rate };
            if avg.den > 0 && avg.num > 0 {
                fps = 1.max(round_i(avg.num as f64 / avg.den as f64));
            } else {
                // Sem taxa declarada (o MKV de uma união sai assim): mede. O chute de 30
                // numa gravação de 60 dava o teto errado no acelerado e um "origem 30fps"
                // falso no log.
                let measured = unsafe { estimate_fps_from_packets(self.src.ptr(), self.v_idx) };
                if measured > 0.0 {
                    fps = 1.max(round_i(measured));
                }
                log!("Export: origem sem taxa declarada — medida {measured:.2} fps.");
            }
        }

        // Taxa de saída: nunca acima da origem (não dá para inventar quadro). Acelerado,
        // cada segundo da saída contém `speed` segundos da origem — fps × speed quadros
        // é o teto (limitado ao que o container e o encoder aguentam).
        let is_svt = opts.encoder_name.to_lowercase() == "libsvtav1";
        let mut out_fps = opts.target_fps;
        if self.speed_mode {
            let cap = 1.max((fps as f64 * self.speed + 1e-6).floor() as i32);
            let cap = if is_svt { cap.min(EXPORT_FPS_MAX_SVT) } else { cap.min(EXPORT_FPS_MAX) };
            if out_fps <= 0 || out_fps > cap {
                out_fps = cap;
            }
        } else if out_fps <= 0 || out_fps > fps {
            out_fps = fps;
        }
        out_fps = out_fps.max(1);
        self.out_fps = out_fps;
        self.drop_frames = out_fps < fps;
        self.frame_interval = 1.0 / out_fps as f64;
        self.next_keep_sec = 0.0;

        // ---- trechos ----
        let total: f64 = opts.segments.iter().filter(|s| s.end_sec > s.start_sec).map(|s| s.end_sec - s.start_sec).sum();
        if total <= 0.0 {
            log!("Export: nenhum trecho valido pra exportar.");
            return Err(Error);
        }
        // Duração da SAÍDA (é o que o progresso mede).
        self.total_sec = total / self.speed;
        log!("Export: {} trecho(s), {:.1}s de saida.", opts.segments.len(), self.total_sec);
        if self.speed_mode {
            log!(
                "Export: acelerado {:.2}x — {out_fps} fps na saida (origem {fps} fps, teto {}).",
                self.speed,
                1.max((fps as f64 * self.speed + 1e-6).floor() as i32)
            );
        }

        if !audio_only {
            // ---- composição ----
            let (cw, ch) = match self.vstream_par() {
                Some(p) => (p.width, p.height),
                None => return Err(Error),
            };
            let Some((regs, out_w, out_h)) = build_comp_regions(&opts.regions, cw, ch, opts.target_height) else {
                log!("Export: nao consegui montar o canvas de saida.");
                return Err(Error);
            };
            self.regs = regs;
            self.out_w = out_w;
            self.out_h = out_h;
            self.needs_fill = self.regs.iter().any(|r| r.dst_h < out_h);
            log!("Export: {} regiao(oes) -> canvas {out_w}x{out_h} (origem {cw}x{ch}), escala={}.", self.regs.len(), opts.scale_algo);

            // ---- decoder de vídeo ----
            // Placa primeiro (confirmado no probe_video_decode, mais abaixo); sem ela, o
            // software de sempre.
            self.hw_decoding = self.open_hw_video_decoder();
            if !self.hw_decoding && !self.open_sw_video_decoder() {
                return Err(Error);
            }

            // ---- encoder de vídeo ----
            let encoder: *const AVCodec = unsafe { (a.codec.avcodec_find_encoder_by_name)(to_utf8(&opts.encoder_name).as_ptr()) };
            if encoder.is_null() {
                log!("Export: encoder \"{}\" nao existe no avcodec.", opts.encoder_name);
                return Err(NoEncoder);
            }
            // time_base do encoder = a do stream de vídeo da origem: os pts passam sem
            // reescala e o corte fica exato. EXCETO no SVT-AV1: o wrapper deriva a taxa
            // de quadros do TIME_BASE (não do campo 'framerate', que ele ignora) e valida
            // contra um teto de 240 fps — como o MKV do OBS tem 1/1000, o SVT lia "1000
            // fps" e o avcodec_open2 devolvia −22. Exportar ACIMA de 240 fps por SVT-AV1
            // falha na abertura, e falha alto de propósito (clampar daria câmera lenta
            // silenciosa). Acelerado vale o mesmo para todos: o pts é o lugar na grade
            // da saída, então a time_base É a grade (#51g).
            if self.speed_mode || is_svt {
                self.enc_tb = AVRational { num: 1, den: out_fps };
            }
            // Dica de framerate = a taxa de SAÍDA: passar a da origem depois de descartar
            // quadros faria o encoder achar que tem mais quadros do que terá.
            let fps_rat = AVRational { num: out_fps, den: 1 };
            let crf = opts.crf.clamp(EXPORT_CRF_MIN, EXPORT_CRF_MAX);

            // ---- abre o encoder em QUALIDADE CONSTANTE (VBR) ----
            // Cada tentativa começa de um contexto NOVO: um avcodec_open2 que falha deixa
            // o contexto meio-desmontado.
            let mut attempt = 0;
            let mut rc = -1;
            loop {
                self.enc_ctx = None;
                let ctx = CodecCtx::alloc(encoder).ok_or(Error)?;
                unsafe {
                    let mut par = (a.codec.avcodec_parameters_alloc)();
                    if par.is_null() {
                        return Err(Error);
                    }
                    (*par).codec_type = AVMEDIA_TYPE_VIDEO;
                    (*par).codec_id = (*encoder).id;
                    (*par).width = out_w;
                    (*par).height = out_h;
                    (*par).format = AV_PIX_FMT_YUV420P;
                    // Dimensões NUNCA via av_opt_set_int('width') — a tabela de opções do
                    // AVCodecContext não expõe width/height (pegadinha #28).
                    let r = (a.codec.avcodec_parameters_to_context)(ctx.ptr(), par);
                    (a.codec.avcodec_parameters_free)(&mut par);
                    if r < 0 {
                        return Err(Error);
                    }
                    (a.util.av_opt_set_q)(ctx.ptr(), c"time_base".as_ptr(), self.enc_tb, 0);
                    (a.util.av_opt_set_q)(ctx.ptr(), c"framerate".as_ptr(), fps_rat, 0);
                }
                // Encoder de SOFTWARE ('lib*' é a convenção do libavcodec): o default é 1
                // thread (35,5 → 10,3 ms por quadro em 4K no x264). Os de hardware não usam.
                if opts.encoder_name.to_lowercase().starts_with("lib") {
                    ctx.set_threads_auto();
                }
                // MP4 exige o extradata (SPS/PPS) no cabeçalho do container; sem esta
                // flag o arquivo sai sem eles e não toca em lugar nenhum (#51a).
                unsafe { (a.util.av_opt_set)(ctx.ptr(), c"flags".as_ptr(), c"+global_header".as_ptr(), 0) };

                let mut enc_opts = Dict::new();
                if !apply_quality_options(&mut enc_opts, &opts.encoder_name, crf, attempt) {
                    self.enc_ctx = Some(ctx);
                    break; // acabaram as formas de pedir qualidade constante
                }
                enc_opts.set("g", &(out_fps * OUT_KEYINT_SEC).to_string());
                rc = unsafe { (a.codec.avcodec_open2)(ctx.ptr(), encoder, enc_opts.as_mut_ptr()) };
                log_leftover_options(&enc_opts, &opts.encoder_name);
                self.enc_ctx = Some(ctx);
                if rc >= 0 {
                    break;
                }
                log!("Export: avcodec_open2 (\"{}\") falhou na tentativa {attempt} ({}).", opts.encoder_name, av_err_str(rc));
                attempt += 1;
            }
            if rc < 0 {
                return Err(NoEncoder);
            }
            // "qualidade constante" sem prometer VBR: o AMF vai por CQP (QP fixo).
            log!(
                "Export: encoder={} {out_w}x{out_h} @{out_fps}fps (origem {fps}fps) crf={crf} (qualidade constante, tentativa {attempt})",
                opts.encoder_name
            );

            // Legenda queimada: só agora se sabe o tamanho de saída. Falhar aqui (GDI
            // indisponível, algo estranho na fonte) não derruba a exportação: o vídeo sai
            // sem a legenda e o log diz por quê.
            if !opts.captions.is_empty() {
                match CaptionBurner::new(out_w, out_h, &opts.captions) {
                    Ok(b) if b.chunk_count() > 0 => self.burner = Some(b),
                    Ok(_) => {}
                    Err(e) => log!("Export: legenda desligada — {e}"),
                }
            }
        } else {
            log!("Export: so audio ({} faixa(s)).", opts.audio_streams.len());
        }

        // ---- faixas de áudio selecionadas ----
        for &j in &opts.audio_streams {
            if j < 0 || j as u32 >= nb_streams {
                continue;
            }
            let Some(s) = self.src.stream(j as u32) else { continue };
            if unsafe { s.codecpar.as_ref() }.is_none_or(|cp| cp.codec_type != AVMEDIA_TYPE_AUDIO) {
                continue;
            }
            self.tracks.push(AudioTrack { src_idx: j, out_idx: -1, src_tb: s.time_base, dec: None, done: false, aout: 0 });
        }
        // Mixar uma faixa só seria reencode à toa — degrada para cópia. Exceto com
        // `audio_codec`: aí o container não aceita o AAC da origem (mp3), e o caminho da
        // mistura é o que decodifica e recodifica.
        self.do_mix = (opts.mix_audio && self.tracks.len() > 1) || (!opts.audio_codec.is_empty() && !self.tracks.is_empty());

        // ---- saída ----
        // Muxer SEMPRE explícito: deduzir do nome do arquivo quebraria com o ".part"
        // temporário que o chamador usa enquanto escreve (#51e).
        let container = if opts.container.is_empty() { "mp4".to_owned() } else { opts.container.clone() };
        let rc = self.out.alloc(&container, &opts.dst_path);
        if rc < 0 || self.out.ctx.is_null() {
            log!("Export: avformat_alloc_output_context2 falhou ({}).", av_err_str(rc));
            return Err(Error);
        }

        if !audio_only {
            self.out_vstream = self.out.new_stream();
            if self.out_vstream.is_null() {
                return Err(Error);
            }
            let enc = self.enc_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
            unsafe {
                if (a.codec.avcodec_parameters_from_context)((*self.out_vstream).codecpar, enc) < 0 {
                    return Err(Error);
                }
                (*self.out_vstream).time_base = self.enc_tb;
            }
        }

        let mut mix_encoder: *const AVCodec = std::ptr::null();
        if self.do_mix {
            let name = if opts.audio_codec.is_empty() { "aac" } else { opts.audio_codec.as_str() };
            mix_encoder = unsafe { (a.codec.avcodec_find_encoder_by_name)(to_utf8(name).as_ptr()) };
            if mix_encoder.is_null() {
                log!("Export: encoder de audio {} ausente.", opts.audio_codec);
                // Sem o encoder pedido o container não tem o que receber: copiar o AAC num
                // .mp3 geraria um arquivo que nada toca.
                if !opts.audio_codec.is_empty() {
                    return Err(NoEncoder);
                }
                self.do_mix = false;
            }
        }

        if self.do_mix {
            let s0 = self.src.stream(self.tracks[0].src_idx as u32).ok_or(Error)?;
            let (sample_rate, layout) = unsafe {
                let cp = s0.codecpar.as_ref().ok_or(Error)?;
                (cp.sample_rate, &cp.ch_layout as *const AVChannelLayout)
            };
            self.mix_src_tb = self.tracks[0].src_tb;
            self.mix_tb = AVRational { num: 1, den: if sample_rate <= 0 { 48000 } else { sample_rate } };
            match open_audio_encoder(mix_encoder, self.mix_tb.den, opts.audio_bitrate, unsafe { &*layout }) {
                Err(-1) => return Err(Error),
                Err(rc) => {
                    log!("Export: avcodec_open2 ({}) falhou ({}) — mixagem desligada.", unsafe { crate::ffi::cstr_to_string((*mix_encoder).name) }, av_err_str(rc));
                    if !opts.audio_codec.is_empty() {
                        return Err(NoEncoder);
                    }
                    self.do_mix = false;
                }
                Ok(ctx) => {
                    // Tamanho de quadro que o encoder exige (definido no open2). O AAC do OBS
                    // decodifica em 1024; o MP3 (libmp3lame) quer 1152 e recusa outro tamanho
                    // a não ser no último quadro. Diferente de 1024 ⇒ as amostras passam pela
                    // fila que refaz os quadros (fifo_push).
                    self.mix_frame_size = frame_size_of(ctx.ptr());
                    self.rechunk = self.mix_frame_size > 0 && self.mix_frame_size != 1024;
                    if self.rechunk {
                        log!("Export: audio em quadros de {} amostras ({}).", self.mix_frame_size, unsafe { crate::ffi::cstr_to_string((*mix_encoder).name) });
                    }
                    self.out_mix_stream = self.out.new_stream();
                    if self.out_mix_stream.is_null() {
                        return Err(Error);
                    }
                    unsafe {
                        if (a.codec.avcodec_parameters_from_context)((*self.out_mix_stream).codecpar, ctx.ptr()) < 0 {
                            return Err(Error);
                        }
                        (*self.out_mix_stream).time_base = self.mix_tb;
                        if !opts.mix_title.is_empty() {
                            // `handler_name` também: o MP4 não guarda o `title` de faixa.
                            let t = to_utf8(&opts.mix_title);
                            (a.util.av_dict_set)(&mut (*self.out_mix_stream).metadata, c"title".as_ptr(), t.as_ptr(), 0);
                            (a.util.av_dict_set)(&mut (*self.out_mix_stream).metadata, c"handler_name".as_ptr(), t.as_ptr(), 0);
                        }
                    }
                    self.mix_ctx = Some(ctx);
                    // Um decoder por faixa selecionada.
                    for t in &mut self.tracks {
                        let Some(s) = self.src.stream(t.src_idx as u32) else { continue };
                        if let Some(cp) = unsafe { s.codecpar.as_ref() } {
                            t.dec = CodecCtx::open_decoder_for(cp, false);
                        }
                    }
                }
            }
        }

        // Acelerado + mistura: a mistura sai pela aouts[0], com o encoder que acabou de
        // ser aberto (o mix_ctx continua sendo liberado à parte).
        if self.do_mix && self.speed_mode {
            let mut ao = AudioOut::new(self.mix_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr), self.mix_tb);
            ao.stream = self.out_mix_stream;
            ao.frame_size = if self.mix_frame_size <= 0 { 1024 } else { self.mix_frame_size as usize };
            self.aouts.push(ao);
        }

        // Acelerado SEM mistura: copiar pacote não serve (o tempo muda), então cada
        // faixa escolhida é decodificada, esticada e recodificada em AAC na sua própria
        // stream — as faixas continuam separadas como no 1×.
        if !self.do_mix && self.speed_mode {
            mix_encoder = unsafe { (a.codec.avcodec_find_encoder_by_name)(c"aac".as_ptr()) };
            if mix_encoder.is_null() && !self.tracks.is_empty() {
                log!("Export: encoder AAC ausente — sem audio acelerado.");
                return Err(NoEncoder);
            }
            for i in 0..self.tracks.len() {
                let s = self.src.stream(self.tracks[i].src_idx as u32).ok_or(Error)?;
                let cp = unsafe { s.codecpar.as_ref() }.ok_or(Error)?;
                let rate = if cp.sample_rate <= 0 { 48000 } else { cp.sample_rate };
                let tb = AVRational { num: 1, den: rate };
                let ctx = match open_audio_encoder(mix_encoder, rate, opts.audio_bitrate, &cp.ch_layout) {
                    Ok(c) => c,
                    Err(-1) => return Err(Error),
                    Err(rc) => {
                        log!("Export: avcodec_open2 (aac, faixa {}) falhou ({}).", self.tracks[i].src_idx, av_err_str(rc));
                        return Err(NoEncoder);
                    }
                };
                let mut ao = AudioOut::new(ctx.ptr(), tb);
                let fs = frame_size_of(ctx.ptr());
                ao.frame_size = if fs <= 0 { 1024 } else { fs as usize };
                let out_a = self.out.new_stream();
                if out_a.is_null() {
                    return Err(Error);
                }
                unsafe {
                    if (a.codec.avcodec_parameters_from_context)((*out_a).codecpar, ctx.ptr()) < 0 {
                        return Err(Error);
                    }
                    (*out_a).time_base = tb;
                    copy_stream_tag(s, out_a, "title");
                    copy_stream_tag(s, out_a, "language");
                    self.tracks[i].out_idx = (*out_a).index;
                }
                ao.stream = out_a;
                ao.owned_enc = Some(ctx);
                self.tracks[i].aout = i;
                self.aouts.push(ao);
                // Decoder da faixa (o mesmo do caminho da mistura).
                self.tracks[i].dec = CodecCtx::open_decoder_for(cp, false);
            }
        } else if !self.do_mix {
            // Stream copy: um stream de saída por faixa escolhida.
            for t in &mut self.tracks {
                let s = self.src.stream(t.src_idx as u32).ok_or(Error)?;
                let out_a = self.out.new_stream();
                if out_a.is_null() {
                    return Err(Error);
                }
                unsafe {
                    if (a.codec.avcodec_parameters_copy)((*out_a).codecpar, s.codecpar) < 0 {
                        return Err(Error);
                    }
                    (*(*out_a).codecpar).codec_tag = 0;
                    (*out_a).time_base = s.time_base;
                    copy_stream_tag(s, out_a, "title");
                    copy_stream_tag(s, out_a, "language");
                    t.out_idx = (*out_a).index;
                }
            }
        }

        let rc = self.out.open_io(&opts.dst_path);
        if rc < 0 {
            log!("Export: avio_open2 falhou ({}).", av_err_str(rc));
            return Err(Error);
        }
        // faststart move o moov para o início — arquivo pronto para compartilhar. Só
        // existe no muxer MP4/MOV; no Matroska o índice já vai no lugar.
        let mut hdr = Dict::new();
        if container == "mp4" {
            hdr.set("movflags", "+faststart");
        }
        let rc = self.out.write_header(&mut hdr);
        if rc < 0 {
            log!("Export: avformat_write_header falhou ({}).", av_err_str(rc));
            return Err(Error);
        }

        // ---- objetos de trabalho ----
        if !audio_only {
            self.out_frame.format = AV_PIX_FMT_YUV420P;
            self.out_frame.width = self.out_w;
            self.out_frame.height = self.out_h;
            let rc = unsafe { (a.util.av_frame_get_buffer)(self.out_frame.ptr(), 0) };
            if rc < 0 {
                log!("Export: av_frame_get_buffer falhou ({}).", av_err_str(rc));
                return Err(Error);
            }

            // Decode na placa: só vale se saíram quadros D3D11 de verdade. O decoder
            // nativo de H.264/HEVC cai calado para o software quando a placa recusa o
            // perfil (e com 1 thread só, mais lento que o caminho normal); o de AV1 nem
            // isso. Nos dois casos, volta ao decoder de software de sempre.
            if self.hw_decoding {
                let (ok, rc) = self.probe_video_decode();
                if ok && self.probe_fmt == AV_PIX_FMT_D3D11 {
                    log!("Export: decodificando na placa (D3D11VA, decoder \"{}\").", self.vdec_name);
                } else {
                    log!("Export: decode na placa nao entregou quadro ({}, formato {}) — usando software.", av_err_str(rc), self.probe_fmt);
                    self.hw_decoding = false;
                    if !self.open_sw_video_decoder() {
                        return Err(Error);
                    }
                }
            }

            // O decoder escolhido decodifica ESTE arquivo? Abrir sem erro não garante (o
            // libaom recusa o AV1 da NVENC pacote a pacote). Senão, tenta os decoders de
            // hardware do build para o mesmo codec — o *_cuvid é o NVDEC, e entrega NV12
            // em memória, que o needs_normalize converte (#51k).
            if !self.hw_decoding {
                let (ok, rc) = self.probe_video_decode();
                if !ok {
                    log!("Export: decoder \"{}\" nao entregou nenhum quadro ({}); tentando outro.", self.vdec_name, av_err_str(rc));
                    let want_id = self.vstream_par().map_or(0, |p| p.codec_id);
                    let mut found = false;
                    for alt in ["av1_cuvid", "hevc_cuvid", "h264_cuvid"] {
                        let d = unsafe { (a.codec.avcodec_find_decoder_by_name)(to_utf8(alt).as_ptr()) };
                        if d.is_null() || unsafe { (*d).id } != want_id {
                            continue;
                        }
                        if !self.switch_video_decoder(alt) {
                            log!("Export: decoder \"{alt}\" nao abriu.");
                            continue;
                        }
                        let (ok, rc) = self.probe_video_decode();
                        if ok {
                            log!("Export: usando o decoder \"{alt}\".");
                            found = true;
                            break;
                        }
                        log!("Export: decoder \"{alt}\" tambem nao entregou quadro ({}).", av_err_str(rc));
                    }
                    if !found {
                        log!("Export: nenhum decoder conseguiu ler o video da origem.");
                        return Err(Error);
                    }
                }
            }
        }

        // ---- um passe por trecho, emendando na saída ----
        let mut canceled = false;
        self.out_offset_sec = 0.0;
        for seg in &opts.segments {
            self.seg_start_sec = seg.start_sec;
            self.seg_end_sec = seg.end_sec;
            if self.seg_end_sec <= self.seg_start_sec {
                continue;
            }
            self.seg_start_ts = sec_to_ts(self.seg_start_sec, self.video_tb);
            self.seg_end_ts = sec_to_ts(self.seg_end_sec, self.video_tb);

            // Posiciona no keyframe anterior ao início do trecho. O avcodec_flush_buffers é
            // obrigatório: sem ele o decoder tentaria continuar de referências que não
            // valem mais aqui.
            unsafe {
                if self.seg_start_ts > 0 && self.v_idx >= 0 {
                    (a.format.av_seek_frame)(self.src.ptr(), self.v_idx, self.seg_start_ts, AVSEEK_FLAG_BACKWARD);
                } else if self.seg_start_sec > 0.0 {
                    // Sem vídeo: stream −1 = tempo em AV_TIME_BASE (µs).
                    (a.format.av_seek_frame)(self.src.ptr(), -1, round_i64(self.seg_start_sec * 1_000_000.0), AVSEEK_FLAG_BACKWARD);
                } else {
                    (a.format.av_seek_frame)(self.src.ptr(), -1, 0, AVSEEK_FLAG_BACKWARD);
                }
                if let Some(d) = &self.dec_ctx {
                    (a.codec.avcodec_flush_buffers)(d.ptr());
                }
            }
            for t in &mut self.tracks {
                if let Some(d) = &t.dec {
                    unsafe { (a.codec.avcodec_flush_buffers)(d.ptr()) };
                }
                t.done = false;
            }

            // Só áudio: o "vídeo" já nasce terminado, e o fim do trecho passa a ser
            // decidido só pelas faixas.
            self.video_done = audio_only;
            while unsafe { (a.format.av_read_frame)(self.src.ptr(), self.pkt.ptr()) } == 0 {
                if self.is_canceled() {
                    self.pkt.unref();
                    canceled = true;
                    break;
                }
                // Termina assim que o vídeo E todas as faixas passaram do fim do trecho.
                // Checado no TOPO de propósito: os ramos abaixo usam `continue`, que pularia
                // uma checagem no rodapé — e num arquivo só de vídeo isso faria a leitura ir
                // até o EOF à toa.
                if self.video_done && self.tracks.iter().all(|t| t.done) {
                    self.pkt.unref();
                    break;
                }

                let mut stop = false;
                'pk: {
                    if self.pkt.stream_index == self.v_idx {
                        if self.video_done {
                            break 'pk;
                        }
                        let dec = self.dec_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
                        let rc = unsafe { (a.codec.avcodec_send_packet)(dec, self.pkt.ptr()) };
                        if rc < 0 {
                            // Um pacote ruim isolado não derruba a exportação, mas tem que
                            // aparecer no log: recusa em TODO pacote passava calada.
                            if !self.vid_send_err_logged {
                                self.vid_send_err_logged = true;
                                log!("Export: decoder recusou um pacote de video ({}) — so este aviso.", av_err_str(rc));
                            }
                            break 'pk;
                        }
                        self.pump_decoder();
                        if self.failed {
                            stop = true;
                        }
                    } else {
                        // Áudio.
                        for i in 0..self.tracks.len() {
                            if self.tracks[i].src_idx != self.pkt.stream_index {
                                continue;
                            }
                            if self.tracks[i].done {
                                break;
                            }
                            // Posição ANTES do rebase abaixo: é o relógio da origem, o mesmo
                            // do report_progress.
                            let src_tb = self.tracks[i].src_tb;
                            let pkt_sec = pts_to_sec(self.pkt.pts, src_tb);
                            if self.pkt.pts != AV_NOPTS_VALUE && pkt_sec >= self.seg_end_sec {
                                self.tracks[i].done = true;
                                break;
                            }
                            if self.pkt.pts != AV_NOPTS_VALUE && pkt_sec < self.seg_start_sec {
                                break;
                            }

                            if self.do_mix {
                                if self.tracks[i].dec.is_some() && !self.handle_mix_packet(i) {
                                    self.failed = true;
                                }
                            } else if self.speed_mode {
                                if self.tracks[i].dec.is_some() && !self.handle_speed_track_packet(i) {
                                    self.failed = true;
                                }
                            } else {
                                let out_idx = self.tracks[i].out_idx;
                                let Some(out_stream) = (unsafe { stream(self.out.ctx, out_idx as u32) }) else { break };
                                let out_tb = out_stream.time_base;
                                let out_si = out_stream.index;
                                // Mesmo deslocamento temporal do vídeo — preserva o sync e emenda na
                                // linha do tempo de saída.
                                let shift = sec_to_ts(self.seg_start_sec, src_tb);
                                let add = sec_to_ts(self.out_offset_sec, src_tb);
                                if self.pkt.pts != AV_NOPTS_VALUE {
                                    self.pkt.pts = (self.pkt.pts - shift + add).max(0);
                                }
                                if self.pkt.dts != AV_NOPTS_VALUE {
                                    self.pkt.dts = (self.pkt.dts - shift + add).max(0);
                                }
                                self.pkt.stream_index = out_si;
                                unsafe { (a.codec.av_packet_rescale_ts)(self.pkt.ptr(), src_tb, out_tb) };
                                self.pkt.pos = -1;
                                let rc = self.out.write(self.pkt.ptr());
                                if rc < 0 {
                                    log!("Export: write (audio copy) falhou ({}).", av_err_str(rc));
                                    self.failed = true;
                                }
                            }
                            // Sem vídeo quem anda o progresso é a 1ª faixa.
                            if audio_only && i == 0 && pkt_sec >= 0.0 {
                                self.report_progress(pkt_sec);
                            }
                            break;
                        }
                        if self.failed {
                            stop = true;
                        }
                    }
                }
                self.pkt.unref();
                if stop {
                    break;
                }
            }

            if canceled || self.failed {
                break;
            }

            // Dreno do decoder. Só faz falta quando o trecho terminou por EOF do arquivo
            // (não por termos VISTO um quadro além do fim): aí os últimos quadros ainda
            // estão dentro do decoder, e com threading em quadros são vários — meio
            // segundo de vídeo sumindo do fim, calado. Depois de um send(nil) o decoder
            // fica em modo dreno; quem o devolve à vida é o flush no topo do próximo
            // trecho.
            if !self.video_done {
                if let Some(d) = &self.dec_ctx {
                    unsafe { (a.codec.avcodec_send_packet)(d.ptr(), std::ptr::null()) };
                    self.pump_decoder();
                    if self.failed {
                        break;
                    }
                }
            }

            // Fecha o acumulador de áudio na borda do trecho: o próximo começa noutro
            // ponto do original e não pode somar por cima deste quadro.
            if self.do_mix && !self.flush_mix_frame() {
                self.failed = true;
                break;
            }
            // Acelerado: cada saída fecha o trecho com o tamanho exato da linha do tempo
            // (usa o out_offset_sec ANTES de somar este trecho).
            for i in 0..self.aouts.len() {
                if !self.ao_end_segment(i) {
                    self.failed = true;
                    break;
                }
            }
            if self.failed {
                break;
            }

            self.out_offset_sec += (self.seg_end_sec - self.seg_start_sec) / self.speed;
        }

        if canceled {
            return Err(Canceled);
        }
        if self.failed {
            return Err(Error);
        }

        // ---- flush dos encoders ----
        // O acumulador de áudio já foi fechado na borda de cada trecho.
        if let Some(e) = &self.enc_ctx {
            let enc = e.ptr();
            unsafe { (a.codec.avcodec_send_frame)(enc, std::ptr::null()) };
            let (stream, tb) = (self.out_vstream, self.enc_tb);
            self.drain_encoder(enc, stream, tb);
        }
        if self.speed_mode {
            // Inclui a mistura (aouts[0] usa o mix_ctx): o que sobrou na fila sai num
            // último quadro curto e o encoder é drenado.
            for i in 0..self.aouts.len() {
                if !self.ao_finish(i) {
                    return Err(Error);
                }
            }
        } else if self.do_mix {
            // O que sobrou na fila sai num último quadro curto.
            if self.rechunk && !self.fifo_emit(true) {
                return Err(Error);
            }
            let mix = self.mix_ctx.as_ref().map_or(std::ptr::null_mut(), CodecCtx::ptr);
            unsafe { (a.codec.avcodec_send_frame)(mix, std::ptr::null()) };
            let (stream, tb) = (self.out_mix_stream, self.mix_tb);
            self.drain_encoder(mix, stream, tb);
        }

        let rc = self.out.write_trailer();
        if rc < 0 {
            log!("Export: av_write_trailer falhou ({}).", av_err_str(rc));
            return Err(Error);
        }

        if !audio_only && self.vid_frames_encoded == 0 {
            log!("Export: nenhum quadro de video chegou ao encoder — arquivo descartado.");
            return Err(Error);
        }

        let name = std::path::Path::new(&opts.dst_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        log!("Export: concluido — {name} ({:.1}s, {}x{}).", self.total_sec, self.out_w, self.out_h);
        if let Some(p) = self.progress {
            p(100.0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(x: i32, y: i32, w: i32, h: i32) -> RecordingRegion {
        RecordingRegion { name: "m".into(), kind: "monitor".into(), x, y, w, h }
    }

    #[test]
    fn canvas_inteiro_sem_regioes() {
        let (regs, w, h) = build_comp_regions(&[], 3841, 2161, 0).unwrap();
        assert_eq!((regs.len(), w, h), (1, 3840, 2160));
    }

    #[test]
    fn pula_o_monitor_do_meio_sem_faixa_preta() {
        // 3 monitores 1920x1080 lado a lado; mantém o 1º e o 3º.
        let (regs, w, h) = build_comp_regions(&[region(3840, 0, 1920, 1080), region(0, 0, 1920, 1080)], 5760, 1080, 0).unwrap();
        assert_eq!((w, h), (3840, 1080));
        assert_eq!(regs[0].src_x, 0); // reordenado pelo X original
        assert_eq!(regs[1].dst_x, 1920);
    }

    #[test]
    fn nunca_faz_upscale_e_centraliza_a_regiao_baixa() {
        let (regs, _w, h) = build_comp_regions(&[region(0, 0, 1920, 1080), region(1920, 0, 1280, 720)], 3200, 1080, 4000).unwrap();
        assert_eq!(h, 1080);
        assert_eq!(regs[1].dst_y, 180); // (1080-720)/2
    }

    #[test]
    fn reduz_pela_altura_alvo() {
        let (regs, w, h) = build_comp_regions(&[], 3840, 2160, 1080).unwrap();
        assert_eq!((w, h, regs[0].dst_w), (1920, 1080, 1920));
    }

    #[test]
    fn calibracao_interpola_e_respeita_extremos() {
        assert_eq!(calibrated_quality(&Q_EXP_SVTAV1, 0, 1, 63), 1);
        assert_eq!(calibrated_quality(&Q_EXP_SVTAV1, 23, 1, 63), 55); // CRF 23 → crf 55 (medido)
        // Piso do SVT-AV1: o primeiro CRF que já cai no quantizador máximo. A tela
        // de exportação avisa a partir dele (`SVT_FLOOR_CRF` no export.js = 31).
        assert_eq!(calibrated_quality(&Q_EXP_SVTAV1, 30, 1, 63), 62);
        assert_eq!(calibrated_quality(&Q_EXP_SVTAV1, 31, 1, 63), 63);
        assert_eq!(calibrated_quality(&Q_EXP_SVTAV1, 51, 1, 63), 63);
        assert_eq!(calibrated_quality(&Q_EXP_H264AMF, 23, 0, 51), 27);
    }

    #[test]
    fn amf_usa_cqp_nunca_qvbr() {
        let mut d = Dict::new();
        assert!(apply_quality_options(&mut d, "h264_amf", 23, 0));
        assert!(!apply_quality_options(&mut Dict::new(), "h264_amf", 23, 1));
    }

    #[test]
    fn nvenc_tem_piso_1_e_sem_bitrate() {
        // O piso evita 'cq=0' (= "sem alvo"); só dá para checar que não há tentativa extra.
        assert!(apply_quality_options(&mut Dict::new(), "h264_nvenc", 0, 0));
        assert!(!apply_quality_options(&mut Dict::new(), "h264_nvenc", 0, 1));
    }

    #[test]
    fn flags_do_swscale() {
        assert_eq!(resolve_scale_flags("Area"), SWS_AREA);
        assert_eq!(resolve_scale_flags(" bilinear "), SWS_BILINEAR);
        assert_eq!(resolve_scale_flags(""), SWS_BICUBIC);
        assert_eq!(resolve_scale_flags("lanczos"), SWS_BICUBIC);
    }

    #[test]
    fn layout_exportado_acompanha_o_recorte() {
        let src = RecordingLayout {
            canvas_w: 3840,
            canvas_h: 1080,
            regions: vec![region(0, 0, 1920, 1080), region(1920, 0, 1920, 1080)],
        };
        // Só o 2º monitor entra, na metade da altura.
        let out = compute_export_layout(&src, &[region(1920, 0, 1920, 1080)], 540).unwrap();
        assert_eq!((out.canvas_w, out.canvas_h), (960, 540));
        assert_eq!(out.regions.len(), 1);
        assert_eq!((out.regions[0].x, out.regions[0].w), (0, 960));
    }
}

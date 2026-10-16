//! Seleção de encoder e qualidade da gravação (porte de `OBSEncoder.pas`).
//!
//! A tradução nível→parâmetro é uma função PURA (`plan_constant_quality`) que
//! devolve a lista de settings; quem fala com o libobs só aplica a lista. Em
//! Delphi isso era `obs_data_set_*` direto no meio da lógica — aqui dá pra
//! testar sem a obs.dll.
//!
//! Pegadinha #53: o default de TODO encoder do libobs é CBR (e CBR enche o
//! arquivo de bits de lixo). A gravação usa SEMPRE qualidade constante
//! adaptativa, nunca alvo de bitrate, e `bitrate` nunca entra junto.

use std::cell::Cell;
use std::ffi::{c_char, CString};

use crate::config;
use crate::ffi::cstr_to_string;
use crate::ffi::obs::{api, cstring, ObsData, ObsEncoder};
use crate::log;
use crate::types::{EncoderCaps, GpuVendor};

type QualityTable = [i32; 11];

/// Escala de referência: a do x264 (0..51), a mesma que a exportação usa.
const CRF_SCALE_MAX: i32 = 51;

// Tabelas CALIBRADAS por PSNR (pegadinha #53), indexadas pelo NÍVEL 0..10.
// Reescalar linearmente entre escalas nativas NÃO alinha qualidade.
const Q_X264: QualityTable = [50, 44, 38, 33, 28, 23, 21, 19, 17, 15, 12];
const Q_SVT_AV1: QualityTable = [63, 63, 63, 63, 61, 54, 50, 46, 41, 37, 29];
/// A chave `cqp` do AMF AV1 vai de 0..63 e o plugin multiplica por 4.
const Q_AMF_AV1: QualityTable = [63, 61, 53, 45, 36, 27, 23, 18, 14, 12, 7];
const Q_AMF_H264: QualityTable = [51, 48, 42, 37, 32, 27, 25, 23, 21, 19, 16];
/// HEVC precisa de qp mais alto que H.264 para o mesmo PSNR.
const Q_AMF_HEVC: QualityTable = [51, 50, 44, 39, 34, 29, 27, 25, 23, 21, 18];

/// AV1 por CPU (`obs-ffmpeg-av1.c`). Escolha EXPLÍCITA; nunca automático.
pub const AV1_SW_ID: &str = "ffmpeg_svt_av1";
/// Preset fixo: joelho da curva velocidade × tamanho (medido).
const AV1_SW_PRESET: i64 = 10;

/// Teto de pico (kbps) do `max_bitrate` do NVENC em CQVBR. O default do
/// plugin (10000) estrangularia a qualidade constante.
const DEFAULT_MAX_BITRATE: i32 = 100_000;

pub const AUDIO_BITRATES: [i32; 6] = [96, 128, 160, 192, 256, 320];
pub const AUDIO_BITRATE_DEFAULT: i32 = 192;

const DEFAULT_LEVEL: usize = 5;

/// IDs do libobs por codec, em ordem de prioridade.
pub const AV1_IDS: [&str; 4] =
    ["obs_nvenc_av1_tex", "obs_nvenc_av1", "av1_texture_amf", "obs_qsv11_av1"];

/// Valor de uma propriedade do `obs_data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setting {
    Str(&'static str),
    Int(i64),
}

pub type SettingList = Vec<(&'static str, Setting)>;

fn level_index(level: i32) -> usize {
    if (0..=10).contains(&level) { level as usize } else { DEFAULT_LEVEL }
}

/// CRF equivalente de um nível na escala do x264 (a que a exportação recebe).
pub fn quality_level_to_crf(level: i32) -> i32 {
    Q_X264[level_index(level)]
}

pub fn normalize_audio_bitrate(kbps: i32) -> i32 {
    if kbps <= 0 {
        return AUDIO_BITRATE_DEFAULT;
    }
    *AUDIO_BITRATES
        .iter()
        .min_by_key(|b| (**b - kbps).abs())
        .unwrap_or(&AUDIO_BITRATE_DEFAULT)
}

pub fn audio_bitrate_kbps() -> i32 {
    normalize_audio_bitrate(config::get_config_int("audioBitrate", AUDIO_BITRATE_DEFAULT))
}

/// Nível do slider (0..10, padrão 5), já migrado da escala antiga `-4..+2`.
///
/// As duas faixas se sobrepõem em 0/1/2, então não dá pra distinguir pelo
/// valor: a chave nova `recordingQualityLevel` existe por isso, e a legada
/// `recordingQuality` só é lida quando a nova está ausente.
pub fn recording_quality_level() -> i32 {
    let level = config::get_config_int("recordingQualityLevel", -1);
    let level = if level < 0 {
        match config::get_config_int("recordingQuality", 0) {
            -4 => 1,
            -3 => 2,
            -2 => 3,
            -1 => 4,
            1 => 7,
            2 => 9,
            _ => 5,
        }
    } else {
        level
    };
    level.clamp(0, 10)
}

/// Reescala linear — SÓ para encoders ainda NÃO calibrados (NVENC e QSV).
fn scale_quality(crf: i32, max: i32) -> i32 {
    if max == CRF_SCALE_MAX {
        crf
    } else {
        (crf as f64 * (max as f64 / CRF_SCALE_MAX as f64)).round() as i32
    }
}

/// Família (`AV1`/`HEVC`/`H.264`) e se é hardware, a partir do ID do libobs.
/// Sai do NOME, não de tabela: os IDs mudam entre versões do OBS.
pub fn describe_encoder_id(id: &str) -> (&'static str, bool) {
    let id = id.to_lowercase();
    let family = if id.contains("av1") {
        "AV1"
    } else if id.contains("hevc") || id.contains("h265") {
        "HEVC"
    } else if ["h264", "x264", "avc", "jim_nvenc"].iter().any(|k| id.contains(k)) {
        "H.264"
    } else {
        ""
    };
    (family, !id.contains("x264") && id != AV1_SW_ID)
}

/// Nível 0..10 → settings de qualidade constante do encoder `id`.
pub fn plan_constant_quality(id: &str, level: i32, max_bitrate_kbps: i32) -> SettingList {
    let lv = level_index(level);
    let crf = Q_X264[lv];
    let id = id.to_lowercase();
    let is_av1 = id.contains("av1");

    // x264: CRF nativo, é a própria referência da calibração.
    if id.contains("x264") {
        return vec![("rate_control", Setting::Str("CRF")), ("crf", Setting::Int(Q_X264[lv].into()))];
    }
    // SVT-AV1: o plugin compara `astrcmpi(rc, "cqp")` e qualquer outra string
    // cai no ramo CBR (`obs-ffmpeg-av1.c:112`). Tem que ser "cqp" minúsculo.
    if id == AV1_SW_ID {
        return vec![
            ("rate_control", Setting::Str("cqp")),
            ("cqp", Setting::Int(Q_SVT_AV1[lv].into())),
            ("preset", Setting::Int(AV1_SW_PRESET)),
        ];
    }
    // NVENC: CQVBR lê `target_quality` (NÃO `cqp`). `max_bitrate` explícito.
    // NÃO calibrado (sem máquina NVIDIA para medir) → reescala linear.
    if id.contains("nvenc") {
        let target = if is_av1 { scale_quality(crf, 63) } else { crf }.max(1);
        return vec![
            ("rate_control", Setting::Str("CQVBR")),
            ("target_quality", Setting::Int(target.into())),
            ("max_bitrate", Setting::Int(max_bitrate_kbps.max(0).into())), // 0 = sem teto
        ];
    }
    // AMF: CQP, nunca QVBR (invertido: maior = melhor).
    if id.contains("amf") {
        let v = if is_av1 {
            Q_AMF_AV1[lv]
        } else if id.contains("h265") || id.contains("hevc") {
            Q_AMF_HEVC[lv]
        } else {
            Q_AMF_H264[lv]
        };
        return vec![("rate_control", Setting::Str("CQP")), ("cqp", Setting::Int(v.into()))];
    }
    // QSV: ICQ. Inalcançável hoje (obs-qsv11 fora da whitelist), mas mapeado
    // para que ligar o plugin não reintroduza o CBR por omissão.
    if id.contains("qsv") {
        let q = if is_av1 { scale_quality(crf, 63) } else { crf }.max(1);
        return vec![("rate_control", Setting::Str("ICQ")), ("icq_quality", Setting::Int(q.into()))];
    }
    // Desconhecido: "CRF" é o nome mais comum; se não aceitar, cai no default.
    vec![("rate_control", Setting::Str("CRF")), ("crf", Setting::Int(crf.into()))]
}

/// Aplica o plano num `obs_data_t` (libobs, main thread).
pub fn apply_constant_quality(settings: &ObsData, id: &str, level: i32) {
    let max = config::get_config_int("recordingMaxBitrate", DEFAULT_MAX_BITRATE);
    let mut applied = Vec::new();
    for (key, value) in plan_constant_quality(id, level, max) {
        match value {
            Setting::Str(s) => {
                settings.set_string(key, s);
                applied.push(format!("{key}={s}"));
            }
            Setting::Int(n) => {
                settings.set_int(key, n);
                applied.push(format!("{key}={n}"));
            }
        }
    }
    // O que FOI aplicado no encoder — é a linha que se procura no log quando a
    // qualidade/tamanho sai estranho (pegadinha #53). NVENC e QSV ainda não
    // têm tabela medida: reescala linear.
    let lower = id.to_lowercase();
    let calibrated = !(lower.contains("nvenc") || lower.contains("qsv"));
    log!(
        "Encoder rc: {} (nivel {level}, {})",
        applied.join(" "),
        if calibrated { "calibrado" } else { "NAO calibrado" }
    );
}

/// Maior dimensão aceita pelo encoder que SERÁ criado (pegadinha #7).
/// H.264 hardware: 4096 em todas as GPUs; HEVC/AV1 hw e x264: 8192.
pub fn max_dimension_for(codec_pref: &str, caps: &EncoderCaps) -> i32 {
    const MAX_H264_HW: i32 = 4096;
    const MAX_OTHER: i32 = 8192;
    // A cadeia automática é H.264 hw → x264 → …; como o x264 sempre existe,
    // o fallback aterrissa em H.264 hw (se houver) ou x264.
    let fallback = if caps.h264_hw { MAX_H264_HW } else { MAX_OTHER };
    match codec_pref.to_lowercase().as_str() {
        "h264-hw" => if caps.h264_hw { MAX_H264_HW } else { fallback },
        "hevc-hw" => if caps.hevc_hw { MAX_OTHER } else { fallback },
        "av1-hw" => if caps.av1_hw { MAX_OTHER } else { fallback },
        "h264-sw" => MAX_OTHER,
        "av1-sw" => if caps.av1_sw { MAX_OTHER } else { fallback },
        _ => fallback,
    }
}

pub fn max_dimension(caps: &EncoderCaps) -> i32 {
    max_dimension_for(&config::get_config_str("codec", "auto"), caps)
}

pub const HEVC_IDS: [&str; 6] = [
    "obs_nvenc_hevc_tex",
    "jim_hevc_nvenc",
    "obs_qsv11_hevc",
    "h265_texture_amf",
    "obs_nvenc_hevc",
    "amd_amf_hevc",
];
pub const H264_IDS: [&str; 6] = [
    "obs_nvenc_h264_tex",
    "jim_nvenc",
    "obs_qsv11_h264",
    "h264_texture_amf",
    "obs_nvenc_h264",
    "obs_x264",
];

thread_local! {
    // Main thread only (como todo o libobs). Ver `set_keyframe_sec_override`.
    static KEYFRAME_SEC_OVERRIDE: Cell<i32> = const { Cell::new(0) };
    static QUALITY_LEVEL_OVERRIDE: Cell<i32> = const { Cell::new(-1) };
}

/// Keyframe curto SÓ no buffer em memória (0 = sem override).
pub fn set_keyframe_sec_override(sec: i32) {
    KEYFRAME_SEC_OVERRIDE.with(|c| c.set(sec));
}

/// Nível fixo (0..10) para o teste de qualidade; -1 = sem override.
pub fn set_quality_level_override(level: i32) {
    QUALITY_LEVEL_OVERRIDE.with(|c| c.set(level));
}

/// Lista os IDs de encoder registrados (`obs_enum_encoder_types`).
fn registered_encoder_ids() -> Vec<String> {
    let Ok(a) = api() else { return Vec::new() };
    let mut out = Vec::new();
    let mut i = 0usize;
    unsafe {
        let mut p: *const c_char = std::ptr::null();
        while (a.obs_enum_encoder_types)(i, &mut p) {
            if !p.is_null() {
                out.push(cstr_to_string(p));
            }
            i += 1;
            p = std::ptr::null();
        }
    }
    out
}

/// OBS recente devolve um encoder "phantom" para ID não registrado — por isso
/// a existência é validada pela enumeração antes de criar.
pub fn encoder_type_exists(id: &str) -> bool {
    registered_encoder_ids().iter().any(|e| e == id)
}

/// Classifica os encoders registrados por vendor. libobs já iniciado.
pub fn detect_encoder_caps() -> EncoderCaps {
    let mut caps = EncoderCaps::default();
    for raw in registered_encoder_ids() {
        let id = raw.to_lowercase();
        if id == "obs_x264" || id == "ffmpeg_x264" {
            caps.h264_sw = true;
        } else if id == AV1_SW_ID {
            // AV1 por CPU não mexe no vendor: vem no obs-ffmpeg, em qualquer máquina.
            caps.av1_sw = true;
        } else if id.contains("nvenc") {
            if caps.vendor == GpuVendor::Unknown {
                caps.vendor = GpuVendor::Nvidia;
            }
            if id.contains("av1") {
                caps.av1_hw = true;
            } else if id.contains("hevc") {
                caps.hevc_hw = true;
            } else {
                caps.h264_hw = true;
            }
        } else if id.contains("amf") {
            if caps.vendor == GpuVendor::Unknown {
                caps.vendor = GpuVendor::Amd;
            }
            if id.contains("av1") {
                caps.av1_hw = true;
            } else if id.contains("h265") {
                caps.hevc_hw = true;
            } else if id.contains("h264") {
                caps.h264_hw = true;
            }
        } else if id.contains("qsv") {
            if caps.vendor == GpuVendor::Unknown {
                caps.vendor = GpuVendor::Intel;
            }
            if id.contains("av1") {
                caps.av1_hw = true;
            } else if id.contains("hevc") {
                caps.hevc_hw = true;
            } else if id.contains("h264") {
                caps.h264_hw = true;
            }
        }
    }
    caps
}

fn try_create_video_encoder(id: &str) -> Option<ObsEncoder> {
    if !encoder_type_exists(id) {
        return None;
    }
    let a = api().ok()?;
    let settings = ObsData::create().ok()?;

    let mut level = recording_quality_level();
    let ov = QUALITY_LEVEL_OVERRIDE.with(Cell::get);
    if (0..=10).contains(&ov) {
        level = ov;
    }
    log!(
        "Encoder quality: nivel={level} (equivale a crf {} na referencia x264)",
        quality_level_to_crf(level)
    );
    // TODO nível aplica qualidade constante, inclusive o padrão (pegadinha #53).
    apply_constant_quality(&settings, id, level);

    // Sem keyint o default é "auto" (~8 s). Configurável 1..10 s (default 2).
    let mut keyframe = config::get_config_int("recordingKeyframeSec", 2);
    let ko = KEYFRAME_SEC_OVERRIDE.with(Cell::get);
    if ko > 0 {
        keyframe = ko;
    }
    let keyframe = keyframe.clamp(1, 10);
    settings.set_int("keyint_sec", keyframe.into());
    log!("Encoder keyint: {keyframe}s");

    let id_c = cstring(id);
    let name = CString::new("NoOBS Video Encoder").unwrap_or_default();
    let enc = unsafe {
        (a.obs_video_encoder_create)(id_c.as_ptr(), name.as_ptr(), settings.raw(), std::ptr::null_mut())
    };
    (!enc.is_null()).then_some(enc)
}

fn try_first(ids: &[&str], skip_x264: bool) -> Option<ObsEncoder> {
    for id in ids {
        if skip_x264 && *id == "obs_x264" {
            continue;
        }
        if let Some(e) = try_create_video_encoder(id) {
            log!("Encoder: {id}");
            return Some(e);
        }
    }
    None
}

fn try_h264_sw() -> Option<ObsEncoder> {
    let e = try_create_video_encoder("obs_x264");
    if e.is_some() {
        log!("Encoder: obs_x264");
    }
    e
}

fn try_av1_sw() -> Option<ObsEncoder> {
    let e = try_create_video_encoder(AV1_SW_ID);
    if e.is_some() {
        log!("Encoder: {AV1_SW_ID} (CPU)");
    }
    e
}

/// Cria o encoder de vídeo conforme a preferência `codec` (auto | av1-hw |
/// hevc-hw | h264-hw | h264-sw | av1-sw). A cadeia automática prioriza
/// compatibilidade: H.264 hw → x264 → AV1 hw → HEVC hw. O `av1-sw` NUNCA entra
/// nela: satura todos os núcleos e a gravação concorre com o que o usuário faz.
pub fn select_video_encoder() -> Result<ObsEncoder, String> {
    let pref = config::get_config_str("codec", "auto").to_lowercase();
    log!("Codec preferido: {pref}");
    let (preferred, why) = match pref.as_str() {
        "av1-hw" => (try_first(&AV1_IDS, false), "av1-hw indisponivel"),
        "hevc-hw" => (try_first(&HEVC_IDS, false), "hevc-hw indisponivel"),
        "h264-hw" => (try_first(&H264_IDS, true), "h264-hw indisponivel"),
        "h264-sw" => (try_h264_sw(), "h264-sw indisponivel (estranho)"),
        "av1-sw" => (try_av1_sw(), "av1-sw indisponivel"),
        _ => (None, ""),
    };
    if let Some(e) = preferred {
        return Ok(e);
    }
    if !why.is_empty() {
        log!("Codec {why}, caindo pro fallback.");
    }
    try_first(&H264_IDS, true)
        .or_else(try_h264_sw)
        .or_else(|| try_first(&AV1_IDS, false))
        .or_else(|| try_first(&HEVC_IDS, false))
        .ok_or_else(|| "Nenhum encoder de video disponivel.".to_owned())
}

/// Espelho do `select_video_encoder`: qual ID de encoder SERÁ usado (o arquivo
/// nasce antes do encoder, e o `{CODEC}` do nome precisa saber). Vazio se nenhum.
pub fn predict_video_encoder_id() -> String {
    let registered = registered_encoder_ids();
    let first_of = |ids: &[&str], skip_x264: bool| -> String {
        ids.iter()
            .find(|id| !(skip_x264 && **id == "obs_x264") && registered.iter().any(|r| r == **id))
            .map(|s| (*s).to_owned())
            .unwrap_or_default()
    };
    let pref = config::get_config_str("codec", "auto").to_lowercase();
    let chosen = match pref.as_str() {
        "av1-hw" => first_of(&AV1_IDS, false),
        "hevc-hw" => first_of(&HEVC_IDS, false),
        "h264-hw" => first_of(&H264_IDS, true),
        "h264-sw" if registered.iter().any(|r| r == "obs_x264") => "obs_x264".to_owned(),
        "av1-sw" if registered.iter().any(|r| r == AV1_SW_ID) => AV1_SW_ID.to_owned(),
        _ => String::new(),
    };
    if !chosen.is_empty() {
        return chosen;
    }
    let mut r = first_of(&H264_IDS, true);
    if r.is_empty() {
        r = first_of(&["obs_x264"], false);
    }
    if r.is_empty() {
        r = first_of(&AV1_IDS, false);
    }
    if r.is_empty() {
        r = first_of(&HEVC_IDS, false);
    }
    r
}

/// `GetEncoderMaxDimension`: consulta as caps reais do libobs (pegadinha #7).
pub fn encoder_max_dimension() -> i32 {
    max_dimension(&detect_encoder_caps())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amf_usa_cqp_nunca_qvbr() {
        let p = plan_constant_quality("h264_texture_amf", 5, 0);
        assert!(p.contains(&("rate_control", Setting::Str("CQP"))));
        assert!(p.contains(&("cqp", Setting::Int(27))));
    }

    #[test]
    fn svt_av1_usa_cqp_minusculo() {
        let p = plan_constant_quality(AV1_SW_ID, 5, 0);
        assert!(p.contains(&("rate_control", Setting::Str("cqp"))));
        assert!(p.contains(&("cqp", Setting::Int(54))));
    }

    #[test]
    fn nvenc_usa_target_quality_e_max_bitrate() {
        let p = plan_constant_quality("obs_nvenc_h264_tex", 5, 100_000);
        assert!(p.contains(&("target_quality", Setting::Int(23))));
        assert!(p.contains(&("max_bitrate", Setting::Int(100_000))));
        assert!(!p.iter().any(|(k, _)| *k == "bitrate"));
    }

    #[test]
    fn nivel_invalido_cai_no_padrao() {
        assert_eq!(quality_level_to_crf(99), 23);
    }

    #[test]
    fn audio_bitrate_degrau_mais_proximo() {
        assert_eq!(normalize_audio_bitrate(0), 192);
        assert_eq!(normalize_audio_bitrate(200), 192);
        assert_eq!(normalize_audio_bitrate(1000), 320);
    }

    #[test]
    fn limite_h264_hw() {
        let caps = EncoderCaps { h264_hw: true, ..Default::default() };
        assert_eq!(max_dimension_for("auto", &caps), 4096);
        assert_eq!(max_dimension_for("h264-sw", &caps), 8192);
    }
}

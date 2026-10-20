//! Qualidade e áudio da gravação (o que sobrou de `OBSEncoder.pas` sem a
//! libobs): o nível 0..10 do slider vira o CRF da escala do x264, e quem
//! traduz o CRF pro parâmetro de cada encoder do libavcodec é o MESMO código
//! da exportação (`ffmpeg_export::apply_quality_options`, calibrado por PSNR —
//! pegadinhas #51b e #53). Qualidade constante sempre, nunca alvo de bitrate.

use std::cell::Cell;

use crate::config;

type QualityTable = [i32; 11];

/// CRF do x264 por NÍVEL 0..10 — a referência de toda a calibração (#53).
const Q_X264: QualityTable = [50, 44, 38, 33, 28, 23, 21, 19, 17, 15, 12];

pub const AUDIO_BITRATES: [i32; 6] = [96, 128, 160, 192, 256, 320];
pub const AUDIO_BITRATE_DEFAULT: i32 = 192;

const DEFAULT_LEVEL: usize = 5;

fn level_index(level: i32) -> usize {
    if (0..=10).contains(&level) { level as usize } else { DEFAULT_LEVEL }
}

/// CRF equivalente de um nível na escala do x264 (a que o encoder recebe).
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

thread_local! {
    // Main thread only (é ela que monta a captura).
    static QUALITY_LEVEL_OVERRIDE: Cell<i32> = const { Cell::new(-1) };
}

/// Nível fixo (0..10) pro teste de qualidade; -1 = sem override.
pub fn set_quality_level_override(level: i32) {
    QUALITY_LEVEL_OVERRIDE.with(|c| c.set(level));
}

/// O nível que a PRÓXIMA captura usa: o do slider, ou o do teste de qualidade.
pub fn capture_quality_level() -> i32 {
    let ov = QUALITY_LEVEL_OVERRIDE.with(Cell::get);
    if (0..=10).contains(&ov) { ov } else { recording_quality_level() }
}

/// Família (`AV1`/`HEVC`/`H.264`) e se é hardware, pelo nome do encoder do
/// libavcodec. Software é o prefixo `lib` (`libx264`, `libsvtav1`); o resto
/// (`*_amf`, `*_nvenc`, `*_mf`) é da placa.
pub fn describe_encoder_id(id: &str) -> (&'static str, bool) {
    let id = id.to_lowercase();
    let family = if id.contains("av1") {
        "AV1"
    } else if id.contains("hevc") || id.contains("h265") {
        "HEVC"
    } else if ["h264", "x264", "avc"].iter().any(|k| id.contains(k)) {
        "H.264"
    } else {
        ""
    };
    (family, !id.is_empty() && !id.starts_with("lib"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn familia_e_hardware_pelo_nome() {
        assert_eq!(describe_encoder_id("av1_amf"), ("AV1", true));
        assert_eq!(describe_encoder_id("hevc_nvenc"), ("HEVC", true));
        assert_eq!(describe_encoder_id("libx264"), ("H.264", false));
        assert_eq!(describe_encoder_id("libsvtav1"), ("AV1", false));
    }

    #[test]
    fn override_do_teste_vence_o_slider() {
        set_quality_level_override(10);
        assert_eq!(capture_quality_level(), 10);
        set_quality_level_override(-1);
    }
}

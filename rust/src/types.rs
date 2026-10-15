//! Tipos compartilhados entre camadas (porte de `NoOBSTypes.pas`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuVendor {
    #[default]
    Unknown,
    Nvidia,
    Amd,
    Intel,
}

/// Classes de encoder disponíveis, detectadas via `obs_enum_encoder_types`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EncoderCaps {
    pub av1_hw: bool,
    pub hevc_hw: bool,
    pub h264_hw: bool,
    pub h264_sw: bool,
    pub av1_sw: bool,
    pub vendor: GpuVendor,
}

/// Dispositivo de áudio enumerado pelo libobs.
#[derive(Debug, Clone, Default)]
pub struct ObsAudioDev {
    pub name: String,
    pub device_id: String,
}

/// Região do canvas ocupada por um source (monitor ou webcam), em pixels.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordingRegion {
    pub name: String,
    /// `"monitor"` | `"webcam"`
    pub kind: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordingLayout {
    pub canvas_w: i32,
    pub canvas_h: i32,
    pub regions: Vec<RecordingRegion>,
}

/// Metadados de uma gravação (o `<hash>.json` do cache).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordingMeta {
    pub duration_sec: i32,
    pub layout: RecordingLayout,
    pub codec: String,
    pub codec_hw: bool,
    pub fps: i32,
    /// Nível do slider 0..10; -1 = desconhecido.
    pub quality_level: i32,
}

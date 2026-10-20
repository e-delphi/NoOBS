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

/// Encoders que a máquina tem (detectados abrindo e codificando um quadro).
#[derive(Debug, Clone, Copy, Default)]
pub struct EncoderCaps {
    pub av1_hw: bool,
    pub hevc_hw: bool,
    pub h264_hw: bool,
    pub h264_sw: bool,
    pub av1_sw: bool,
    pub vendor: GpuVendor,
}

/// Dispositivo de áudio (endpoint WASAPI).
#[derive(Debug, Clone, Default)]
pub struct AudioDev {
    pub name: String,
    pub device_id: String,
}

/// Webcam DirectShow (porte de `TWebcamInfo` do `WinWebcam.pas`).
#[derive(Debug, Clone, Default)]
pub struct Webcam {
    /// Moniker display name.
    pub device_id: String,
    /// Friendly name (mostrado na UI).
    pub name: String,
    pub width: i32,
    pub height: i32,
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

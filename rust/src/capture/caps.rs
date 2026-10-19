//! O que esta máquina sabe codificar, sem libobs: o fabricante da placa (pelo
//! DXGI do nosso dispositivo) e uma abertura de teste de cada encoder do
//! libavcodec. Devolve o MESMO `EncoderCaps` que a tela já consome, e a escolha
//! do encoder reusa a da exportação (`ffmpeg_export::resolve_export_encoder`:
//! mesma ordem do "auto" da gravação, e AV1 por software nunca no automático —
//! pegadinha #53).

use windows::core::Interface;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;

use super::encode::{AvVideoEncoder, VideoEncoder, VideoEncoderConfig};
use super::gpu::Device;
use crate::types::{EncoderCaps, GpuVendor};

/// Fabricante pelo `VendorId` do adaptador do dispositivo.
pub fn vendor_of(dev: &Device) -> GpuVendor {
    let id = (|| -> Option<u32> {
        let dxgi: IDXGIDevice = dev.device.cast().ok()?;
        let ad = unsafe { dxgi.GetAdapter() }.ok()?;
        Some(unsafe { ad.GetDesc() }.ok()?.VendorId)
    })();
    match id {
        Some(0x10DE) => GpuVendor::Nvidia,
        Some(0x1002) | Some(0x1022) => GpuVendor::Amd,
        Some(0x8086) => GpuVendor::Intel,
        _ => GpuVendor::Unknown,
    }
}

/// Abre o encoder e codifica um quadro preto de verdade: existir no build não
/// basta (o NVENC existe em toda máquina e só abre com placa NVIDIA), e abrir
/// também não — o AMF só se inicializa de fato no 1º quadro, e isso custava
/// ~500 ms de quadros atrasados no começo da 1ª gravação do processo. Rodando
/// aqui (no arranque do app), a detecção é também o AQUECIMENTO.
fn opens(dev: &Device, codec: &str) -> bool {
    if codec.is_empty() {
        return false;
    }
    // 1280x720: o AMF de HEVC/AV1 recusa quadro pequeno ("Init() failed with
    // error 5" em 256x144) e grava normal em tamanho de tela — o teste
    // pequeno dava "sem hardware" falso.
    let (w, h) = (1280, 720);
    let cfg = VideoEncoderConfig { codec: codec.into(), width: w, height: h, fps: 30, crf: 23, keyframe_sec: 2 };
    let Ok(mut enc) = AvVideoEncoder::open(dev, &cfg) else { return false };
    let Ok(conv) = super::gpu::Nv12Converter::new(dev, w, h) else { return true };
    if let Ok(comp) = super::gpu::Compositor::new(dev, w, h) {
        comp.clear(dev);
        conv.convert(dev, comp.canvas_srv());
    }
    let mut out = Vec::new();
    let ok = enc.encode(dev, &conv.nv12, 0, false, &mut out).is_ok();
    enc.flush(&mut out);
    ok
}

pub fn detect(dev: &Device) -> EncoderCaps {
    let vendor = vendor_of(dev);
    let hw = |family: &str| opens(dev, &crate::ffmpeg_export::hw_encoder_name(family, vendor));
    let caps = EncoderCaps {
        h264_hw: hw("h264"),
        hevc_hw: hw("hevc"),
        av1_hw: hw("av1"),
        h264_sw: opens(dev, "libx264"),
        av1_sw: opens(dev, "libsvtav1"),
        vendor,
    };
    crate::log!("Captura: encoders desta maquina: {caps:?}");
    caps
}

/// Preferência do app (`auto`, `h264-hw`, `av1-sw`…) → nome no libavcodec.
pub fn resolve(pref: &str, caps: &EncoderCaps) -> String {
    crate::ffmpeg_export::resolve_export_encoder(pref, caps)
}

/// Maior dimensão que o encoder aceita (#7): H.264 por hardware para em 4096
/// em todo fabricante; HEVC/AV1 por hardware em 8192; software sem limite
/// prático (8192 pra não explodir memória).
pub fn max_dimension(codec: &str) -> u32 {
    let c = codec.to_ascii_lowercase();
    if c.starts_with("h264_") {
        4096
    } else {
        8192
    }
}

/// Encolhe `w`×`h` proporcionalmente pra caber no limite do encoder, em pares.
pub fn clamp_canvas(w: u32, h: u32, codec: &str) -> (u32, u32) {
    let max = max_dimension(codec) as f64;
    let s = (max / w.max(1) as f64).min(max / h.max(1) as f64).min(1.0);
    (((w as f64 * s) as u32) & !1, ((h as f64 * s) as u32) & !1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limite_de_canvas_por_encoder() {
        assert_eq!(clamp_canvas(7680, 2160, "h264_amf"), (4096, 1152));
        assert_eq!(clamp_canvas(7680, 2160, "hevc_amf"), (7680, 2160));
        assert_eq!(clamp_canvas(1921, 1081, "libx264"), (1920, 1080), "ímpar vira par");
        assert_eq!(clamp_canvas(9000, 3000, "av1_nvenc"), (8192, 2730));
    }
}

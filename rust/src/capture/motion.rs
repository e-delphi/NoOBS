//! Detector de mudança na imagem gravada (Timelapse da gravação de área).
//!
//! Por quadro, na placa: o canvas é copiado pro nível 0 de uma textura com
//! mipmaps e o `GenerateMips` faz a média 2×2 sucessiva até um nível de
//! ~320 px de largura. É MÉDIA de verdade (uma letra digitada em 4K ainda
//! move o bloco dela); reduzir por amostragem bilinear leria 4 texels a cada
//! 12 e perderia o que cai entre eles. Só esse nível pequeno volta pra
//! memória (~170 KB), por um anel de texturas de leitura: o resultado de um
//! quadro sai DOIS quadros depois, sem esperar a placa.
//!
//! "Mudou" = algum pixel do nível pequeno com a luminância diferente do
//! quadro anterior além de `THRESHOLD`. O cursor, quando não conta, é
//! mascarado pela posição real dele (a de antes e a de agora).

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

use super::gpu::Device;

/// Largura máxima do nível lido.
const TARGET_W: u32 = 320;
/// Diferença de luminância (0..255) que conta como mudança num pixel do nível
/// pequeno. Ruído de compressão de vídeo e o sub-pixel do texto ficam abaixo.
const THRESHOLD: i32 = 18;
/// Texturas de leitura em rodízio: lê a de 2 quadros atrás.
const RING: usize = 3;

/// Retângulo a ignorar, em pixels do canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mask {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub struct MotionDetector {
    mips: ID3D11Texture2D,
    mips_srv: ID3D11ShaderResourceView,
    level: u32,
    lw: u32,
    lh: u32,
    canvas_w: u32,
    staging: Vec<ID3D11Texture2D>,
    /// Por texture de leitura: o quadro (índice) e as máscaras daquele quadro.
    pending: Vec<Option<(u64, Vec<Mask>)>>,
    next: usize,
    prev: Vec<u8>,
}

impl MotionDetector {
    pub fn new(dev: &Device, canvas_w: u32, canvas_h: u32) -> Result<Self, String> {
        let mut level = 0;
        while (canvas_w >> level) > TARGET_W && (canvas_h >> level) > 8 {
            level += 1;
        }
        let levels = level + 1;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: canvas_w,
            Height: canvas_h,
            MipLevels: levels,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        };
        let mut t = None;
        unsafe { dev.device.CreateTexture2D(&desc, None, Some(&mut t)) }.map_err(|e| format!("textura de mipmaps: {e}"))?;
        let mips = t.ok_or("sem textura de mipmaps")?;
        let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            ViewDimension: windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_SRV { MostDetailedMip: 0, MipLevels: levels } },
        };
        let mut v = None;
        unsafe { dev.device.CreateShaderResourceView(&mips, Some(&srv_desc), Some(&mut v)) }.map_err(|e| format!("SRV dos mipmaps: {e}"))?;
        let (lw, lh) = ((canvas_w >> level).max(1), (canvas_h >> level).max(1));
        let sdesc = D3D11_TEXTURE2D_DESC {
            Width: lw,
            Height: lh,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging = Vec::new();
        for _ in 0..RING {
            let mut s = None;
            unsafe { dev.device.CreateTexture2D(&sdesc, None, Some(&mut s)) }.map_err(|e| format!("textura de leitura: {e}"))?;
            staging.push(s.ok_or("sem textura de leitura")?);
        }
        crate::log!("Captura: Timelapse — detector de mudanca em {lw}x{lh} (nivel {level} de {canvas_w}x{canvas_h}).");
        Ok(Self {
            mips,
            mips_srv: v.ok_or("sem SRV dos mipmaps")?,
            level,
            lw,
            lh,
            canvas_w,
            staging,
            pending: vec![None; RING],
            next: 0,
            prev: Vec::new(),
        })
    }

    /// Entrega o canvas do quadro `index` (com as áreas a ignorar) e devolve,
    /// quando pronto, o veredito de um quadro anterior: `(índice, mudou)`.
    pub fn feed(&mut self, dev: &Device, canvas: &ID3D11Texture2D, index: u64, masks: Vec<Mask>) -> Option<(u64, bool)> {
        let slot = self.next;
        self.next = (self.next + 1) % RING;
        // O que está neste slot é de RING quadros atrás: lê antes de reusar.
        let verdict = self.pending[slot].take().and_then(|(i, m)| self.read(dev, slot, &m).map(|c| (i, c)));
        unsafe {
            dev.ctx.CopySubresourceRegion(&self.mips, 0, 0, 0, 0, canvas, 0, None);
            dev.ctx.GenerateMips(&self.mips_srv);
            dev.ctx.CopySubresourceRegion(&self.staging[slot], 0, 0, 0, 0, &self.mips, self.level, None);
        }
        self.pending[slot] = Some((index, masks));
        verdict
    }

    fn read(&mut self, dev: &Device, slot: usize, masks: &[Mask]) -> Option<bool> {
        let (w, h) = (self.lw as usize, self.lh as usize);
        let mut cur = vec![0u8; w * h];
        unsafe {
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            dev.ctx.Map(&self.staging[slot], 0, D3D11_MAP_READ, 0, Some(&mut m)).ok()?;
            let src = m.pData as *const u8;
            let pitch = m.RowPitch as usize;
            for y in 0..h {
                let row = src.add(y * pitch);
                for x in 0..w {
                    let p = row.add(x * 4);
                    // BGRA → luminância BT.709 (inteira).
                    let (b, g, r) = (*p as u32, *p.add(1) as u32, *p.add(2) as u32);
                    cur[y * w + x] = ((r * 54 + g * 183 + b * 19) >> 8) as u8;
                }
            }
            dev.ctx.Unmap(&self.staging[slot], 0);
        }
        let k = w as f32 / self.canvas_w as f32;
        let small: Vec<Mask> = masks.iter().map(|m| Mask { x: m.x * k, y: m.y * k, w: m.w * k, h: m.h * k }).collect();
        let changed = !self.prev.is_empty() && changed(&self.prev, &cur, w, h, &small);
        self.prev = cur;
        Some(changed)
    }
}

/// Algum pixel fora das máscaras mudou além do limiar?
fn changed(prev: &[u8], cur: &[u8], w: usize, h: usize, masks: &[Mask]) -> bool {
    let masked = |x: usize, y: usize| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        masks.iter().any(|m| fx >= m.x - 1.0 && fx < m.x + m.w + 1.0 && fy >= m.y - 1.0 && fy < m.y + m.h + 1.0)
    };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if (cur[i] as i32 - prev[i] as i32).abs() > THRESHOLD && !masked(x, y) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mudanca_fora_da_mascara_conta() {
        let prev = vec![10u8; 16];
        let mut cur = prev.clone();
        cur[5] = 200;
        assert!(changed(&prev, &cur, 4, 4, &[]));
        // Pixel (1,1) mascarado.
        assert!(!changed(&prev, &cur, 4, 4, &[Mask { x: 1.0, y: 1.0, w: 1.0, h: 1.0 }]));
    }

    #[test]
    fn ruido_abaixo_do_limiar_nao_conta() {
        let prev = vec![100u8; 16];
        let cur: Vec<u8> = prev.iter().map(|v| v + THRESHOLD as u8).collect();
        assert!(!changed(&prev, &cur, 4, 4, &[]));
    }
}

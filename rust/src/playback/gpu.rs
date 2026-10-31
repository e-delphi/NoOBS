//! Conversão NA PLACA do quadro decodificado (D3D11, NV12) pra RGBA no tamanho
//! da tela, pelo D3D11 Video Processor: só a imagem pequena volta pra memória.
//! No caminho de CPU (cópia do 4K inteiro + swscale) um quadro 4K custava
//! ~20 ms, teto de ~48 fps (medido na prova de conceito do player).
//!
//! Um quadro de saída pode ter VÁRIOS pedaços: cada um é um retângulo da
//! origem desenhado num retângulo da saída. É assim que saem o zoom (o pedaço
//! visível da origem, ampliado) e a vista de regiões (monitores lado a lado)
//! sem perder nitidez — o redimensionamento é sempre da origem.
//!
//! O device é o do próprio FFmpeg (`AVD3D11VADeviceContext`), então a textura
//! do decoder é usada sem cópia entre devices. O contexto imediato é
//! compartilhado com o decoder: todo uso passa pelo lock dele.

use std::ffi::c_void;
use std::mem::ManuallyDrop;

use windows::core::Interface;
use windows::Win32::Foundation::{RECT, TRUE};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::Piece;
use crate::ffi::ffmpeg::*;

/// `AVD3D11VADeviceContext` (hwcontext_d3d11va.h, ABI estável): device,
/// device_context, video_device, video_context, lock, unlock, lock_ctx.
#[repr(C)]
struct AvD3d11Dev {
    device: *mut c_void,
    device_context: *mut c_void,
    video_device: *mut c_void,
    video_context: *mut c_void,
    lock: unsafe extern "C" fn(*mut c_void),
    unlock: unsafe extern "C" fn(*mut c_void),
    lock_ctx: *mut c_void,
}

struct Target {
    key: [i32; 4],
    en: ID3D11VideoProcessorEnumerator,
    vp: ID3D11VideoProcessor,
    ov: ID3D11VideoProcessorOutputView,
    rtv: ID3D11RenderTargetView,
    out_tex: ID3D11Texture2D,
    staging: ID3D11Texture2D,
}

pub struct GpuConv {
    hw: *const AvD3d11Dev,
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    target: Option<Target>,
    /// Saída em BGRA (placa sem RGBA de saída): troca R e B na cópia.
    swap_rb: bool,
}

// O ponteiro do device do FFmpeg só é usado na thread de vídeo dona dele.
unsafe impl Send for GpuConv {}

impl GpuConv {
    /// `hw_dev` = o `AVBufferRef*` do device D3D11VA do FFmpeg.
    pub fn new(hw_dev: *mut c_void) -> Option<Self> {
        unsafe {
            let devctx = *((hw_dev as *const u8).add(OFFS_BUFREF_DATA) as *const *const u8);
            let hw = *(devctx.add(OFFS_HWDEV_HWCTX) as *const *const AvD3d11Dev);
            let h = &*hw;
            let device = ID3D11Device::from_raw_borrowed(&h.device)?.clone();
            let ctx = ID3D11DeviceContext::from_raw_borrowed(&h.device_context)?.clone();
            let vdev = ID3D11VideoDevice::from_raw_borrowed(&h.video_device)?.clone();
            let vctx = ID3D11VideoContext::from_raw_borrowed(&h.video_context)?.clone();
            Some(Self { hw, device, ctx, vdev, vctx, target: None, swap_rb: false })
        }
    }

    fn ensure(&mut self, iw: i32, ih: i32, ow: i32, oh: i32) -> windows::core::Result<()> {
        let key = [iw, ih, ow, oh];
        if self.target.as_ref().is_some_and(|t| t.key == key) {
            return Ok(());
        }
        self.target = None;
        unsafe {
            let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: DXGI_RATIONAL { Numerator: 60, Denominator: 1 },
                InputWidth: iw as u32,
                InputHeight: ih as u32,
                OutputFrameRate: DXGI_RATIONAL { Numerator: 60, Denominator: 1 },
                OutputWidth: ow as u32,
                OutputHeight: oh as u32,
                Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
            };
            let en = self.vdev.CreateVideoProcessorEnumerator(&desc)?;
            // A saída é BGRA (a ordem da tela); sem suporte, RGBA e a troca na cópia.
            let bgra_ok = en
                .CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM)
                .map(|f| f & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 != 0)
                .unwrap_or(false);
            self.swap_rb = !bgra_ok;
            let fmt = if bgra_ok { DXGI_FORMAT_B8G8R8A8_UNORM } else { DXGI_FORMAT_R8G8B8A8_UNORM };
            let vp = self.vdev.CreateVideoProcessor(&en, 0)?;
            let mut td = D3D11_TEXTURE2D_DESC {
                Width: ow as u32,
                Height: oh as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: fmt,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut out_tex = None;
            self.device.CreateTexture2D(&td, None, Some(&mut out_tex))?;
            let out_tex = out_tex.ok_or_else(windows::core::Error::empty)?;
            td.Usage = D3D11_USAGE_STAGING;
            td.BindFlags = 0;
            td.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            self.device.CreateTexture2D(&td, None, Some(&mut staging))?;
            let ovd = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let mut ov = None;
            self.vdev.CreateVideoProcessorOutputView(&out_tex, &en, &ovd, Some(&mut ov))?;
            let mut rtv = None;
            self.device.CreateRenderTargetView(&out_tex, None, Some(&mut rtv))?;
            // Entrada BT.709 de faixa limitada (16-235); saída RGB 0-255.
            // Bitfield: Usage:1 RGB_Range:1 YCbCr_Matrix:1 xvYCC:1 Nominal_Range:2.
            self.vctx.VideoProcessorSetStreamColorSpace(&vp, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: (1 << 2) | (1 << 4) });
            self.vctx.VideoProcessorSetOutputColorSpace(&vp, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
            self.vctx.VideoProcessorSetStreamFrameFormat(&vp, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            self.vctx.VideoProcessorSetStreamAutoProcessingMode(&vp, 0, false);
            self.target = Some(Target {
                key,
                en,
                vp,
                ov: ov.ok_or_else(windows::core::Error::empty)?,
                rtv: rtv.ok_or_else(windows::core::Error::empty)?,
                out_tex,
                staging: staging.ok_or_else(windows::core::Error::empty)?,
            });
        }
        Ok(())
    }

    /// Desenha os `pieces` do quadro D3D11 (`frame.data[0]` = textura,
    /// `data[1]` = índice no array) numa imagem RGBA `ow`x`oh` preta, dentro de
    /// `dst` (passo `ow*4`).
    pub fn render(&mut self, frame: &AVFrame, ow: i32, oh: i32, pieces: &[Piece], dst: &mut [u8]) -> windows::core::Result<()> {
        self.ensure(frame.width, frame.height, ow, oh)?;
        let h = unsafe { &*self.hw };
        unsafe { (h.lock)(h.lock_ctx) };
        let r = self.render_locked(frame, ow, oh, pieces, dst);
        unsafe { (h.unlock)(h.lock_ctx) };
        r
    }

    fn render_locked(&mut self, frame: &AVFrame, ow: i32, oh: i32, pieces: &[Piece], dst: &mut [u8]) -> windows::core::Result<()> {
        let t = self.target.as_ref().ok_or_else(windows::core::Error::empty)?;
        unsafe {
            self.ctx.ClearRenderTargetView(&t.rtv, &[0.0, 0.0, 0.0, 1.0]);
            let tex_ptr = frame.data[0] as *mut c_void;
            let tex = ID3D11Texture2D::from_raw_borrowed(&tex_ptr).ok_or_else(windows::core::Error::empty)?;
            let ivd = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: frame.data[1] as usize as u32 },
                },
            };
            let mut iv = None;
            self.vdev.CreateVideoProcessorInputView(tex, &t.en, &ivd, Some(&mut iv))?;
            for p in pieces {
                let src = RECT { left: p.sx, top: p.sy, right: p.sx + p.sw, bottom: p.sy + p.sh };
                let dr = RECT { left: p.dx, top: p.dy, right: p.dx + p.dw, bottom: p.dy + p.dh };
                self.vctx.VideoProcessorSetStreamSourceRect(&t.vp, 0, true, Some(&src));
                self.vctx.VideoProcessorSetStreamDestRect(&t.vp, 0, true, Some(&dr));
                // Alvo = o retângulo do pedaço: o fundo que o Blt pinta fica
                // dentro dele, e os pedaços anteriores sobrevivem.
                self.vctx.VideoProcessorSetOutputTargetRect(&t.vp, true, Some(&dr));
                let stream = D3D11_VIDEO_PROCESSOR_STREAM {
                    Enable: TRUE,
                    OutputIndex: 0,
                    InputFrameOrField: 0,
                    PastFrames: 0,
                    FutureFrames: 0,
                    ppPastSurfaces: std::ptr::null_mut(),
                    pInputSurface: ManuallyDrop::new(iv.clone()),
                    ppFutureSurfaces: std::ptr::null_mut(),
                    ppPastSurfacesRight: std::ptr::null_mut(),
                    pInputSurfaceRight: ManuallyDrop::new(None),
                    ppFutureSurfacesRight: std::ptr::null_mut(),
                };
                let mut streams = [stream];
                let r = self.vctx.VideoProcessorBlt(&t.vp, &t.ov, 0, &streams);
                ManuallyDrop::drop(&mut streams[0].pInputSurface);
                r?;
            }
            self.ctx.CopyResource(&t.staging, &t.out_tex);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(&t.staging, 0, D3D11_MAP_READ, 0, Some(&mut m))?;
            let row = (ow * 4) as usize;
            if m.RowPitch as usize == row {
                // Sem folga no fim das linhas: uma cópia só.
                dst[..row * oh as usize].copy_from_slice(std::slice::from_raw_parts(m.pData as *const u8, row * oh as usize));
            } else {
                for y in 0..oh as usize {
                    let s = std::slice::from_raw_parts((m.pData as *const u8).add(y * m.RowPitch as usize), row);
                    dst[y * row..(y + 1) * row].copy_from_slice(s);
                }
            }
            self.ctx.Unmap(&t.staging, 0);
        }
        if self.swap_rb {
            for px in dst.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        Ok(())
    }
}

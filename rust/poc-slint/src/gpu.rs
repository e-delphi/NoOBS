//! ConversÃ£o NA PLACA: o quadro D3D11 do decoder (NV12, 4K) vira RGBA no
//! tamanho da tela pelo D3D11 Video Processor, e sÃ³ essa imagem pequena volta
//! pra memÃ³ria. No caminho de CPU (cÃ³pia do 4K inteiro + swscale) um quadro 4K
//! custava ~20 ms, teto de ~48 fps.
//!
//! O device Ã© o do prÃ³prio FFmpeg (`AVD3D11VADeviceContext`), entÃ£o a textura do
//! decoder Ã© usada sem cÃ³pia entre devices. O contexto imediato Ã© compartilhado
//! com o decoder: todo uso passa pelo lock dele.

use std::ffi::c_void;
use std::mem::ManuallyDrop;

use windows::core::Interface;
use windows::Win32::Foundation::{RECT, TRUE};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::ffi::ffmpeg::*;

/// `AVD3D11VADeviceContext` (hwcontext_d3d11va.h, ABI estÃ¡vel): device,
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

pub struct GpuConv {
    hw: *const AvD3d11Dev,
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    key: [i32; 4],
    vp: Option<(ID3D11VideoProcessorEnumerator, ID3D11VideoProcessor, ID3D11VideoProcessorOutputView, ID3D11Texture2D, ID3D11Texture2D)>,
    /// SaÃ­da em BGRA (placa sem RGBA de saÃ­da): troca R e B na cÃ³pia.
    swap_rb: bool,
}

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
            Some(Self { hw, device, ctx, vdev, vctx, key: [0; 4], vp: None, swap_rb: false })
        }
    }

    fn ensure(&mut self, iw: i32, ih: i32, ow: i32, oh: i32) -> windows::core::Result<()> {
        let key = [iw, ih, ow, oh];
        if self.vp.is_some() && self.key == key {
            return Ok(());
        }
        self.vp = None;
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
            let rgba_ok = en.CheckVideoProcessorFormat(DXGI_FORMAT_R8G8B8A8_UNORM).map(|f| f & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 != 0).unwrap_or(false);
            self.swap_rb = !rgba_ok;
            let fmt = if rgba_ok { DXGI_FORMAT_R8G8B8A8_UNORM } else { DXGI_FORMAT_B8G8R8A8_UNORM };
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
            let out_tex = out_tex.unwrap();
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
            // Entrada BT.709 de faixa limitada (16-235); saÃ­da RGB 0-255.
            // Bitfield: Usage:1 RGB_Range:1 YCbCr_Matrix:1 xvYCC:1 Nominal_Range:2.
            self.vctx.VideoProcessorSetStreamColorSpace(&vp, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: (1 << 2) | (1 << 4) });
            self.vctx.VideoProcessorSetOutputColorSpace(&vp, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
            self.vctx.VideoProcessorSetStreamFrameFormat(&vp, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            self.vctx.VideoProcessorSetStreamAutoProcessingMode(&vp, 0, false);
            self.vctx.VideoProcessorSetStreamSourceRect(&vp, 0, true, Some(&RECT { left: 0, top: 0, right: iw, bottom: ih }));
            self.vctx.VideoProcessorSetStreamDestRect(&vp, 0, true, Some(&RECT { left: 0, top: 0, right: ow, bottom: oh }));
            self.vp = Some((en, vp, ov.unwrap(), out_tex, staging.unwrap()));
        }
        self.key = key;
        Ok(())
    }

    /// Converte o quadro D3D11 (`frame.data[0]` = textura, `data[1]` = Ã­ndice no
    /// array) pra RGBA `ow`x`oh` dentro de `dst` (passo `ow*4`).
    pub fn convert(&mut self, frame: &AVFrame, ow: i32, oh: i32, dst: &mut [u8]) -> windows::core::Result<()> {
        self.ensure(frame.width, frame.height, ow, oh)?;
        let h = unsafe { &*self.hw };
        unsafe { (h.lock)(h.lock_ctx) };
        let r = self.convert_locked(frame, ow, oh, dst);
        unsafe { (h.unlock)(h.lock_ctx) };
        r
    }

    fn convert_locked(&mut self, frame: &AVFrame, ow: i32, oh: i32, dst: &mut [u8]) -> windows::core::Result<()> {
        let (en, vp, ov, out_tex, staging) = self.vp.as_ref().unwrap();
        unsafe {
            let tex_ptr = frame.data[0] as *mut c_void;
            let tex = ID3D11Texture2D::from_raw_borrowed(&tex_ptr).ok_or_else(windows::core::Error::empty)?;
            let ivd = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: frame.data[1] as usize as u32 } },
            };
            let mut iv = None;
            self.vdev.CreateVideoProcessorInputView(tex, en, &ivd, Some(&mut iv))?;
            let stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: TRUE,
                OutputIndex: 0,
                InputFrameOrField: 0,
                PastFrames: 0,
                FutureFrames: 0,
                ppPastSurfaces: std::ptr::null_mut(),
                pInputSurface: ManuallyDrop::new(iv),
                ppFutureSurfaces: std::ptr::null_mut(),
                ppPastSurfacesRight: std::ptr::null_mut(),
                pInputSurfaceRight: ManuallyDrop::new(None),
                ppFutureSurfacesRight: std::ptr::null_mut(),
            };
            let mut streams = [stream];
            let r = self.vctx.VideoProcessorBlt(vp, ov, 0, &streams);
            ManuallyDrop::drop(&mut streams[0].pInputSurface);
            r?;
            self.ctx.CopyResource(staging, out_tex);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut m))?;
            let row = (ow * 4) as usize;
            for y in 0..oh as usize {
                let s = std::slice::from_raw_parts((m.pData as *const u8).add(y * m.RowPitch as usize), row);
                dst[y * row..(y + 1) * row].copy_from_slice(s);
            }
            self.ctx.Unmap(staging, 0);
        }
        if self.swap_rb {
            for px in dst.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        Ok(())
    }
}


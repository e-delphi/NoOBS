//! GPU do motor: o dispositivo D3D11 e o que roda nele — o compositor (cada
//! fonte desenhada no canvas com recorte e escala) e o conversor de cor
//! (canvas BGRA → NV12 BT.709 de faixa limitada, o que todo encoder quer).
//!
//! O contexto imediato é usado SÓ pela thread de vídeo (`pipeline`). O
//! dispositivo é marcado multithread-protegido porque o encoder (AMF/NVENC,
//! pelo libavcodec) também fala com ele.
//!
//! Referências no OBS: `libobs-d3d11/d3d11-subsystem.cpp` (dispositivo),
//! `libobs/data/default.effect` (desenho com recorte), `format_conversion.effect`
//! (os coeficientes do BT.709).

use windows::core::{s, Interface, PCSTR};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3};
use windows::Win32::Graphics::Direct3D::{ID3DBlob, D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D11_SRV_DIMENSION_TEXTURE2D};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_R8_UNORM, DXGI_SAMPLE_DESC};

/// Dispositivo + contexto imediato.
pub struct Device {
    pub device: ID3D11Device,
    pub ctx: ID3D11DeviceContext,
}

impl Device {
    pub fn create() -> Result<Self, String> {
        let mut device = None;
        let mut ctx = None;
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                flags,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )
        }
        .map_err(|e| format!("D3D11CreateDevice: {e}"))?;
        let device: ID3D11Device = device.ok_or("D3D11CreateDevice sem dispositivo")?;
        let ctx: ID3D11DeviceContext = ctx.ok_or("D3D11CreateDevice sem contexto")?;
        // O encoder (thread do AMF/NVENC) também usa o dispositivo.
        if let Ok(mt) = ctx.cast::<ID3D11Multithread>() {
            unsafe {
                let _ = mt.SetMultithreadProtected(true);
            }
        }
        Ok(Self { device, ctx })
    }

    /// Textura 2D com as views pedidas.
    pub fn texture(&self, w: u32, h: u32, format: DXGI_FORMAT, bind: D3D11_BIND_FLAG) -> Result<ID3D11Texture2D, String> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut tex = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut tex)) }.map_err(|e| format!("CreateTexture2D {w}x{h} {format:?}: {e}"))?;
        tex.ok_or_else(|| "CreateTexture2D sem textura".into())
    }

    pub fn srv(&self, tex: &ID3D11Texture2D, format: DXGI_FORMAT) -> Result<ID3D11ShaderResourceView, String> {
        let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_SRV { MostDetailedMip: 0, MipLevels: 1 } },
        };
        let mut v = None;
        unsafe { self.device.CreateShaderResourceView(tex, Some(&desc), Some(&mut v)) }.map_err(|e| format!("CreateShaderResourceView: {e}"))?;
        v.ok_or_else(|| "sem SRV".into())
    }

    pub fn rtv(&self, tex: &ID3D11Texture2D, format: DXGI_FORMAT) -> Result<ID3D11RenderTargetView, String> {
        let desc = D3D11_RENDER_TARGET_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_RTV { MipSlice: 0 } },
        };
        let mut v = None;
        unsafe { self.device.CreateRenderTargetView(tex, Some(&desc), Some(&mut v)) }.map_err(|e| format!("CreateRenderTargetView {format:?}: {e}"))?;
        v.ok_or_else(|| "sem RTV".into())
    }
}

// ---------------------------------------------------------------------------
// Shaders
// ---------------------------------------------------------------------------

/// Um triângulo que cobre a viewport inteira (sem vertex buffer); `uv` 0..1
/// dentro da viewport. Cada passada escolhe a viewport = o retângulo de destino.
const HLSL: &str = r#"
cbuffer Params : register(b0) { float2 uv_off; float2 uv_scale; };
Texture2D tex : register(t0);
SamplerState smp : register(s0);
struct V { float4 pos : SV_Position; float2 uv : TEXCOORD0; };
V vs_main(uint id : SV_VertexID) {
    V o;
    float2 t = float2((id << 1) & 2, id & 2);
    o.pos = float4(t * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = t;
    return o;
}
float4 ps_copy(V i) : SV_Target { return float4(tex.Sample(smp, uv_off + i.uv * uv_scale).rgb, 1); }
// BT.709, faixa limitada (16-235 / 16-240) — os coeficientes do OBS.
float ps_y(V i) : SV_Target {
    float3 c = tex.Sample(smp, i.uv).rgb;
    return dot(c, float3(0.182586, 0.614231, 0.062007)) + 0.0627451;
}
// Viewport com METADE do tamanho: o centro de cada pixel cai entre 4 texels do
// canvas, e a amostragem bilinear tira a média deles (croma 4:2:0).
float2 ps_uv(V i) : SV_Target {
    float3 c = tex.Sample(smp, i.uv).rgb;
    return float2(dot(c, float3(-0.100644, -0.338572, 0.439216)), dot(c, float3(0.439216, -0.398942, -0.040274))) + 0.501961;
}
"#;

fn compile(entry: PCSTR, target: PCSTR) -> Result<ID3DBlob, String> {
    let mut code = None;
    let mut errors = None;
    let r = unsafe {
        D3DCompile(
            HLSL.as_ptr() as *const _,
            HLSL.len(),
            s!("noobs-capture"),
            None,
            None,
            entry,
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(e) = r {
        let msg = errors
            .map(|b: ID3DBlob| unsafe { String::from_utf8_lossy(std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize())).into_owned() })
            .unwrap_or_default();
        return Err(format!("D3DCompile: {e} {msg}"));
    }
    code.ok_or_else(|| "D3DCompile sem código".into())
}

fn blob_bytes(b: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize()) }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Params {
    uv_off: [f32; 2],
    uv_scale: [f32; 2],
}

/// Shaders + estado comuns ao compositor e ao conversor.
struct Pipeline {
    vs: ID3D11VertexShader,
    ps_copy: ID3D11PixelShader,
    ps_y: ID3D11PixelShader,
    ps_uv: ID3D11PixelShader,
    cb: ID3D11Buffer,
    sampler: ID3D11SamplerState,
}

impl Pipeline {
    fn new(dev: &Device) -> Result<Self, String> {
        let vs_code = compile(s!("vs_main"), s!("vs_5_0"))?;
        let ps = |entry| -> Result<ID3D11PixelShader, String> {
            let code = compile(entry, s!("ps_5_0"))?;
            let mut p = None;
            unsafe { dev.device.CreatePixelShader(blob_bytes(&code), None, Some(&mut p)) }.map_err(|e| format!("CreatePixelShader: {e}"))?;
            p.ok_or_else(|| "sem pixel shader".into())
        };
        let mut vs = None;
        unsafe { dev.device.CreateVertexShader(blob_bytes(&vs_code), None, Some(&mut vs)) }.map_err(|e| format!("CreateVertexShader: {e}"))?;
        let cb_desc = D3D11_BUFFER_DESC {
            ByteWidth: std::mem::size_of::<Params>().next_multiple_of(16) as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let mut cb = None;
        unsafe { dev.device.CreateBuffer(&cb_desc, None, Some(&mut cb)) }.map_err(|e| format!("CreateBuffer: {e}"))?;
        let sdesc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let mut sampler = None;
        unsafe { dev.device.CreateSamplerState(&sdesc, Some(&mut sampler)) }.map_err(|e| format!("CreateSamplerState: {e}"))?;
        Ok(Self {
            vs: vs.ok_or("sem vertex shader")?,
            ps_copy: ps(s!("ps_copy"))?,
            ps_y: ps(s!("ps_y"))?,
            ps_uv: ps(s!("ps_uv"))?,
            cb: cb.ok_or("sem constant buffer")?,
            sampler: sampler.ok_or("sem sampler")?,
        })
    }

    /// Uma passada: `src` → `rtv`, na viewport `(x, y, w, h)`, lendo a janela
    /// `uv_off..uv_off+uv_scale` da textura de origem.
    #[allow(clippy::too_many_arguments)]
    fn draw(&self, dev: &Device, ps: &ID3D11PixelShader, rtv: &ID3D11RenderTargetView, src: &ID3D11ShaderResourceView, vp: (f32, f32, f32, f32), uv_off: [f32; 2], uv_scale: [f32; 2]) {
        let c = &dev.ctx;
        let p = Params { uv_off, uv_scale };
        unsafe {
            c.UpdateSubresource(&self.cb, 0, None, &p as *const Params as *const _, 0, 0);
            c.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            c.RSSetViewports(Some(&[D3D11_VIEWPORT { TopLeftX: vp.0, TopLeftY: vp.1, Width: vp.2, Height: vp.3, MinDepth: 0.0, MaxDepth: 1.0 }]));
            c.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            c.IASetInputLayout(None);
            c.VSSetShader(&self.vs, None);
            c.PSSetShader(ps, None);
            c.PSSetConstantBuffers(0, Some(&[Some(self.cb.clone())]));
            c.PSSetShaderResources(0, Some(&[Some(src.clone())]));
            c.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            c.Draw(3, 0);
            // Solta a textura de origem: a próxima passada pode escrever nela.
            c.PSSetShaderResources(0, Some(&[None]));
            c.OMSetRenderTargets(None, None);
        }
    }
}

// ---------------------------------------------------------------------------
// Compositor
// ---------------------------------------------------------------------------

/// Retângulo em pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Canvas BGRA: cada fonte é desenhada num retângulo, com recorte.
pub struct Compositor {
    pipe: Pipeline,
    pub width: u32,
    pub height: u32,
    pub canvas: ID3D11Texture2D,
    canvas_rtv: ID3D11RenderTargetView,
    canvas_srv: ID3D11ShaderResourceView,
}

impl Compositor {
    pub fn new(dev: &Device, width: u32, height: u32) -> Result<Self, String> {
        let canvas = dev.texture(width, height, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_BIND_FLAG(D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0))?;
        Ok(Self {
            pipe: Pipeline::new(dev)?,
            width,
            height,
            canvas_rtv: dev.rtv(&canvas, DXGI_FORMAT_B8G8R8A8_UNORM)?,
            canvas_srv: dev.srv(&canvas, DXGI_FORMAT_B8G8R8A8_UNORM)?,
            canvas,
        })
    }

    /// Preto (o que não tem fonte por cima — monitor desligado, janela menor).
    pub fn clear(&self, dev: &Device) {
        unsafe { dev.ctx.ClearRenderTargetView(&self.canvas_rtv, &[0.0, 0.0, 0.0, 1.0]) };
    }

    /// Desenha `src` (de tamanho `src_w`×`src_h`), recortado em `crop` (pixels
    /// da origem), no retângulo `dst` do canvas.
    pub fn draw(&self, dev: &Device, src: &ID3D11ShaderResourceView, src_w: u32, src_h: u32, crop: Rect, dst: Rect) {
        if src_w == 0 || src_h == 0 || dst.w <= 0.0 || dst.h <= 0.0 {
            return;
        }
        let (sw, sh) = (src_w as f32, src_h as f32);
        self.pipe.draw(dev, &self.pipe.ps_copy, &self.canvas_rtv, src, (dst.x, dst.y, dst.w, dst.h), [crop.x / sw, crop.y / sh], [crop.w / sw, crop.h / sh]);
    }

    pub fn canvas_srv(&self) -> &ID3D11ShaderResourceView {
        &self.canvas_srv
    }
}

// ---------------------------------------------------------------------------
// Conversor de cor
// ---------------------------------------------------------------------------

/// Canvas BGRA → textura NV12 (Y em R8 na largura cheia, UV em R8G8 na metade).
/// As duas views sobre a MESMA textura NV12 são o que o D3D11 oferece pra
/// escrever nos planos (Windows 8+), como o `gs_texture_create_nv12` do OBS.
pub struct Nv12Converter {
    pipe: Pipeline,
    pub width: u32,
    pub height: u32,
    pub nv12: ID3D11Texture2D,
    rtv_y: ID3D11RenderTargetView,
    rtv_uv: ID3D11RenderTargetView,
}

impl Nv12Converter {
    pub fn new(dev: &Device, width: u32, height: u32) -> Result<Self, String> {
        if width % 2 != 0 || height % 2 != 0 {
            return Err(format!("NV12 exige dimensões pares ({width}x{height})"));
        }
        let nv12 = dev.texture(width, height, DXGI_FORMAT_NV12, D3D11_BIND_RENDER_TARGET)?;
        Ok(Self {
            pipe: Pipeline::new(dev)?,
            width,
            height,
            rtv_y: dev.rtv(&nv12, DXGI_FORMAT_R8_UNORM)?,
            rtv_uv: dev.rtv(&nv12, DXGI_FORMAT_R8G8_UNORM)?,
            nv12,
        })
    }

    pub fn convert(&self, dev: &Device, canvas: &ID3D11ShaderResourceView) {
        let (w, h) = (self.width as f32, self.height as f32);
        self.pipe.draw(dev, &self.pipe.ps_y, &self.rtv_y, canvas, (0.0, 0.0, w, h), [0.0, 0.0], [1.0, 1.0]);
        self.pipe.draw(dev, &self.pipe.ps_uv, &self.rtv_uv, canvas, (0.0, 0.0, w / 2.0, h / 2.0), [0.0, 0.0], [1.0, 1.0]);
    }
}

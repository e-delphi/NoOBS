//! Fontes de vídeo. Toda fonte entrega a MESMA coisa: a última imagem que
//! recebeu, numa textura BGRA da nossa placa (o compositor não sabe de onde
//! ela veio).
//!
//! `WgcSource` — Windows Graphics Capture, de monitor ou de janela. Referência:
//! `libobs-winrt/winrt-capture.cpp`. Diferença deliberada: o OBS recebe os
//! quadros por EVENTO (pool normal + dispatcher na thread de gráficos) e
//! destrói a captura noutra thread; aqui o pool é *free-threaded* e a thread de
//! vídeo só CONSULTA (`TryGetNextFrame`) a cada tique. Sem callback, não há
//! quadro chegando enquanto a captura é desmontada.

use windows::core::Interface;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Direct3D11::{ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{ClientToScreen, HMONITOR};
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, IsWindow};

use super::gpu::Device;

/// Uma fonte de vídeo vista pelo compositor.
pub trait VideoSource {
    fn name(&self) -> &str;
    /// Chamado a cada tique, na thread de vídeo: pega a imagem mais nova, se houver.
    fn poll(&mut self, dev: &Device);
    /// A última imagem: view + tamanho. `None` = ainda nada (ou a fonte morreu).
    fn image(&self) -> Option<(&ID3D11ShaderResourceView, u32, u32)>;
    /// A fonte acabou (janela fechada): o compositor deixa o espaço preto.
    fn ended(&self) -> bool {
        false
    }
}

/// O que capturar. Os handles vão como número: `HWND`/`HMONITOR` não são
/// `Send`, e a configuração atravessa pra thread de vídeo.
#[derive(Debug, Clone, Copy)]
pub enum WgcTarget {
    Monitor(isize),
    /// Janela; `client_area` recorta a moldura (barra de título, bordas).
    Window { hwnd: isize, client_area: bool },
}

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut _)
}

/// Captura WGC consultada pela thread de vídeo.
pub struct WgcSource {
    name: String,
    target: WgcTarget,
    _item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    d3d: IDirect3DDevice,
    pool_size: SizeInt32,
    /// Cópia NOSSA da última imagem: o quadro do WGC volta pro pool no `Close`.
    image: Option<(ID3D11Texture2D, ID3D11ShaderResourceView, u32, u32)>,
    ended: bool,
}

static BORDERLESS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static BORDER_FAIL_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn log_once(flag: &std::sync::atomic::AtomicBool, msg: &str) {
    if !flag.swap(true, std::sync::atomic::Ordering::SeqCst) {
        crate::log!("{msg}");
    }
}

/// Pede uma vez por processo a captura sem borda. Num app desktop comum o
/// Windows concede na hora, sem perguntar ao usuário.
fn borderless_allowed() -> bool {
    *BORDERLESS.get_or_init(|| {
        let r = GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless).and_then(|op| op.get());
        match r {
            Ok(s) => {
                crate::log!("Captura: acesso sem borda = {}.", s.0);
                s == AppCapabilityAccessStatus::Allowed
            }
            Err(e) => {
                crate::log!("Captura: acesso sem borda indisponivel ({e}).");
                // Windows antigo sem o pedido: tenta a chamada direta mesmo assim.
                true
            }
        }
    })
}

impl WgcSource {
    /// Precisa de apartamento WinRT na thread (o `pipeline` faz `RoInitialize`).
    pub fn new(dev: &Device, target: WgcTarget, name: &str) -> Result<Self, String> {
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(|e| format!("WGC indisponível: {e}"))?;
        let item: GraphicsCaptureItem = unsafe {
            match target {
                WgcTarget::Monitor(h) => interop.CreateForMonitor(HMONITOR(h as *mut _)),
                WgcTarget::Window { hwnd: h, .. } => interop.CreateForWindow(hwnd(h)),
            }
        }
        .map_err(|e| format!("CreateForMonitor/Window: {e}"))?;
        let dxgi: IDXGIDevice = dev.device.cast().map_err(|e| format!("IDXGIDevice: {e}"))?;
        let d3d: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
            .and_then(|i| i.cast())
            .map_err(|e| format!("CreateDirect3D11DeviceFromDXGIDevice: {e}"))?;
        let size = item.Size().map_err(|e| format!("item.Size: {e}"))?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)
            .map_err(|e| format!("CreateFreeThreaded: {e}"))?;
        let session = pool.CreateCaptureSession(&item).map_err(|e| format!("CreateCaptureSession: {e}"))?;
        let _ = session.SetIsCursorCaptureEnabled(true);
        // Sem a borda amarela (Windows 11; no 10 a chamada falha e a borda fica).
        // O "sem borda" só vale com o acesso pedido antes — sem ele o Windows
        // aceita a chamada e desenha a borda assim mesmo (o OBS pede igual,
        // `winrt-capture.cpp`).
        if borderless_allowed() {
            if let Err(e) = session.SetIsBorderRequired(false) {
                log_once(&BORDER_FAIL_LOGGED, &format!("Captura: sem borda recusado ({e}) — a borda amarela aparece."));
            }
        }
        session.StartCapture().map_err(|e| format!("StartCapture: {e}"))?;
        Ok(Self { name: name.to_owned(), target, _item: item, pool, session, d3d, pool_size: size, image: None, ended: false })
    }

    /// Recorte da área cliente dentro do quadro da janela inteira (o WGC
    /// entrega a janela com a moldura). Mesma conta do `get_client_box` do OBS.
    fn client_box(&self, frame_w: u32, frame_h: u32) -> Option<D3D11_BOX> {
        let WgcTarget::Window { hwnd: h, client_area: true } = self.target else { return None };
        let hwnd = hwnd(h);
        unsafe {
            let mut wr = RECT::default();
            let mut cr = RECT::default();
            GetWindowRect(hwnd, &mut wr).ok()?;
            GetClientRect(hwnd, &mut cr).ok()?;
            let mut origin = POINT { x: 0, y: 0 };
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return None;
            }
            let left = (origin.x - wr.left).clamp(0, frame_w as i32) as u32;
            let top = (origin.y - wr.top).clamp(0, frame_h as i32) as u32;
            let right = (left + cr.right.max(0) as u32).min(frame_w);
            let bottom = (top + cr.bottom.max(0) as u32).min(frame_h);
            (right > left && bottom > top).then_some(D3D11_BOX { left, top, front: 0, right, bottom, back: 1 })
        }
    }

    fn take_frame(&mut self, dev: &Device) -> Result<bool, String> {
        let Ok(frame) = self.pool.TryGetNextFrame() else { return Ok(false) };
        let content = frame.ContentSize().map_err(|e| e.to_string())?;
        let surface = frame.Surface().map_err(|e| e.to_string())?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(|e| e.to_string())?;
        let tex: ID3D11Texture2D = unsafe { access.GetInterface() }.map_err(|e| e.to_string())?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { tex.GetDesc(&mut desc) };
        // O conteúdo útil é o `ContentSize` (o pool pode ser maior que a janela).
        let full = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: (content.Width.max(0) as u32).min(desc.Width),
            bottom: (content.Height.max(0) as u32).min(desc.Height),
            back: 1,
        };
        let bx = self.client_box(full.right, full.bottom).unwrap_or(full);
        let (w, h) = (bx.right - bx.left, bx.bottom - bx.top);
        if w > 0 && h > 0 {
            let reuse = matches!(&self.image, Some((_, _, iw, ih)) if *iw == w && *ih == h);
            if !reuse {
                let t = dev.texture(w, h, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_BIND_SHADER_RESOURCE)?;
                let v = dev.srv(&t, DXGI_FORMAT_B8G8R8A8_UNORM)?;
                self.image = Some((t, v, w, h));
            }
            if let Some((dst, _, _, _)) = &self.image {
                unsafe { dev.ctx.CopySubresourceRegion(dst, 0, 0, 0, 0, &tex, 0, Some(&bx)) };
            }
        }
        let _ = frame.Close();
        // Janela/monitor mudou de tamanho: o pool acompanha (como o OBS no on_frame_arrived).
        if content.Width > 0 && content.Height > 0 && (content.Width != self.pool_size.Width || content.Height != self.pool_size.Height) {
            let _ = self.pool.Recreate(&self.d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, content);
            self.pool_size = content;
        }
        Ok(true)
    }
}

impl VideoSource for WgcSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn poll(&mut self, dev: &Device) {
        if self.ended {
            return;
        }
        if let WgcTarget::Window { hwnd: h, .. } = self.target {
            if !unsafe { IsWindow(Some(hwnd(h))) }.as_bool() {
                self.ended = true;
                return;
            }
        }
        // Mais de um quadro na fila (tique atrasado): fica o mais novo.
        for _ in 0..4 {
            match self.take_frame(dev) {
                Ok(true) => continue,
                Ok(false) => break,
                Err(e) => {
                    crate::log!("Captura: \"{}\" falhou ao ler o quadro: {e}", self.name);
                    break;
                }
            }
        }
    }

    fn image(&self) -> Option<(&ID3D11ShaderResourceView, u32, u32)> {
        self.image.as_ref().map(|(_, v, w, h)| (v, *w, *h))
    }

    fn ended(&self) -> bool {
        self.ended
    }
}

impl Drop for WgcSource {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

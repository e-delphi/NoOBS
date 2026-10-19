//! Webcam por Media Foundation (o OBS usa DirectShow, `plugins/win-dshow`).
//!
//! Uma thread lê os quadros pelo `IMFSourceReader`, já convertidos pra RGB32
//! pelo próprio Windows (`MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING` — MJPG,
//! YUY2, NV12, o que a câmera der), e guarda o mais novo. A fonte só sobe esse
//! quadro pra uma textura quando ele muda: a câmera anda no ritmo dela (30 fps
//! típico) e o laço de vídeo no da gravação, sem um esperar o outro.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::core::PWSTR;
use windows::Win32::Graphics::Direct3D11::{ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED};

use super::gpu::Device;
use super::video::VideoSource;
use crate::log;

/// O quadro mais novo, BGRA de cima pra baixo.
#[derive(Default)]
struct Latest {
    data: Vec<u8>,
    width: u32,
    height: u32,
    seq: u64,
}

struct Shared {
    latest: Mutex<Latest>,
    stop: AtomicBool,
    ended: AtomicBool,
}

pub struct WebcamSource {
    name: String,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    shown_seq: u64,
    image: Option<(ID3D11Texture2D, ID3D11ShaderResourceView, u32, u32)>,
}

/// Câmeras que o Media Foundation enxerga (nome amigável).
pub fn list_webcams() -> Vec<String> {
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        let names = enum_devices().map(|v| v.into_iter().map(|(n, _)| n).collect()).unwrap_or_default();
        let _ = MFShutdown();
        if com {
            CoUninitialize();
        }
        names
    }
}

unsafe fn enum_devices() -> Result<Vec<(String, IMFActivate)>, String> {
    let mut attrs = None;
    MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    let attrs = attrs.ok_or("sem atributos")?;
    attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID).map_err(|e| e.to_string())?;
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    MFEnumDeviceSources(&attrs, &mut list, &mut count).map_err(|e| format!("MFEnumDeviceSources: {e}"))?;
    let mut out = Vec::new();
    for i in 0..count as usize {
        if let Some(act) = (*list.add(i)).take() {
            let mut name = PWSTR::null();
            let mut len = 0u32;
            let n = if act.GetAllocatedString(&MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &mut name, &mut len).is_ok() {
                let s = name.to_string().unwrap_or_default();
                CoTaskMemFree(Some(name.0 as *const _));
                s
            } else {
                String::new()
            };
            out.push((n, act));
        }
    }
    if !list.is_null() {
        CoTaskMemFree(Some(list as *const _));
    }
    Ok(out)
}

impl WebcamSource {
    /// Abre a câmera pelo nome amigável (vazio = a primeira).
    pub fn new(camera: &str, name: &str) -> Result<Self, String> {
        let shared = Arc::new(Shared { latest: Mutex::new(Latest::default()), stop: AtomicBool::new(false), ended: AtomicBool::new(false) });
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let (sh, cam) = (shared.clone(), camera.to_owned());
        let thread = std::thread::Builder::new()
            .name("noobs-webcam".into())
            .spawn(move || reader_thread(&cam, sh, tx))
            .map_err(|e| format!("thread da webcam: {e}"))?;
        match rx.recv() {
            Ok(Ok(())) => Ok(Self { name: name.to_owned(), shared, thread: Some(thread), shown_seq: 0, image: None }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err("a thread da webcam caiu ao abrir".into())
            }
        }
    }
}

unsafe fn open_reader(camera: &str) -> Result<(IMFSourceReader, u32, u32), String> {
    let devs = enum_devices()?;
    let (_, act) = devs
        .into_iter()
        .find(|(n, _)| camera.is_empty() || n.eq_ignore_ascii_case(camera))
        .ok_or_else(|| format!("câmera \"{camera}\" não encontrada"))?;
    let source: IMFMediaSource = act.ActivateObject().map_err(|e| format!("ActivateObject (câmera em uso ou bloqueada pela privacidade?): {e}"))?;
    let mut attrs = None;
    MFCreateAttributes(&mut attrs, 1).map_err(|e| e.to_string())?;
    let attrs = attrs.ok_or("sem atributos")?;
    // O Windows converte o que a câmera der pra RGB32.
    attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).map_err(|e| e.to_string())?;
    let reader = MFCreateSourceReaderFromMediaSource(&source, &attrs).map_err(|e| format!("MFCreateSourceReaderFromMediaSource: {e}"))?;
    let mt = MFCreateMediaType().map_err(|e| e.to_string())?;
    mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
    mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(|e| e.to_string())?;
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    reader.SetCurrentMediaType(stream, None, &mt).map_err(|e| format!("SetCurrentMediaType(RGB32): {e}"))?;
    let cur = reader.GetCurrentMediaType(stream).map_err(|e| e.to_string())?;
    let size = cur.GetUINT64(&MF_MT_FRAME_SIZE).map_err(|e| e.to_string())?;
    Ok((reader, (size >> 32) as u32, (size & 0xFFFF_FFFF) as u32))
}

fn reader_thread(camera: &str, sh: Arc<Shared>, ready: std::sync::mpsc::Sender<Result<(), String>>) {
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        match open_reader(camera) {
            Err(e) => {
                log!("Captura: webcam \"{camera}\" nao abriu: {e}");
                let _ = ready.send(Err(e));
            }
            Ok((reader, w, h)) => {
                log!("Captura: webcam \"{camera}\" aberta ({w}x{h}).");
                let _ = ready.send(Ok(()));
                let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
                while !sh.stop.load(Ordering::SeqCst) {
                    let mut flags = 0u32;
                    let mut sample = None;
                    if let Err(e) = reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample)) {
                        log!("Captura: webcam parou: {e}");
                        break;
                    }
                    // Fim do stream / câmera removida.
                    if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 | MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
                        break;
                    }
                    let Some(sample) = sample else { continue };
                    if let Err(e) = store(&sample, w, h, &sh) {
                        log!("Captura: webcam: quadro ilegivel: {e}");
                    }
                }
                sh.ended.store(true, Ordering::SeqCst);
            }
        }
        let _ = MFShutdown();
        if com {
            CoUninitialize();
        }
    }
}

/// Copia o quadro RGB32 pro `Latest`, de cima pra baixo (o `Lock2D` dá a 1ª
/// linha e o passo — negativo quando a imagem vem de baixo pra cima).
unsafe fn store(sample: &IMFSample, w: u32, h: u32, sh: &Shared) -> Result<(), String> {
    let buf = sample.ConvertToContiguousBuffer().map_err(|e| e.to_string())?;
    let buf2d: IMF2DBuffer = windows::core::Interface::cast(&buf).map_err(|e| e.to_string())?;
    let mut scan0: *mut u8 = std::ptr::null_mut();
    let mut pitch = 0i32;
    buf2d.Lock2D(&mut scan0, &mut pitch).map_err(|e| e.to_string())?;
    let row = w as usize * 4;
    {
        let mut l = sh.latest.lock().unwrap_or_else(|p| p.into_inner());
        l.data.resize(row * h as usize, 0);
        for y in 0..h as usize {
            let src = scan0.offset(y as isize * pitch as isize);
            std::ptr::copy_nonoverlapping(src, l.data.as_mut_ptr().add(y * row), row);
        }
        l.width = w;
        l.height = h;
        l.seq += 1;
    }
    let _ = buf2d.Unlock2D();
    Ok(())
}

impl VideoSource for WebcamSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn poll(&mut self, dev: &Device) {
        let l = self.shared.latest.lock().unwrap_or_else(|p| p.into_inner());
        if l.seq == self.shown_seq || l.width == 0 {
            return;
        }
        let (w, h) = (l.width, l.height);
        let reuse = matches!(&self.image, Some((_, _, iw, ih)) if *iw == w && *ih == h);
        if !reuse {
            let Ok(t) = dev.texture(w, h, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_BIND_SHADER_RESOURCE) else { return };
            let Ok(v) = dev.srv(&t, DXGI_FORMAT_B8G8R8A8_UNORM) else { return };
            self.image = Some((t, v, w, h));
        }
        if let Some((t, _, _, _)) = &self.image {
            unsafe { dev.ctx.UpdateSubresource(t, 0, None, l.data.as_ptr() as *const _, w * 4, 0) };
        }
        self.shown_seq = l.seq;
    }

    fn image(&self) -> Option<(&ID3D11ShaderResourceView, u32, u32)> {
        self.image.as_ref().map(|(_, v, w, h)| (v, *w, *h))
    }

    fn ended(&self) -> bool {
        self.shared.ended.load(Ordering::SeqCst)
    }
}

impl Drop for WebcamSource {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // O ReadSample é bloqueante: volta no próximo quadro da câmera (~33 ms).
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

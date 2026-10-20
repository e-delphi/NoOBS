//! Fontes de áudio. Toda fonte entrega a MESMA coisa: amostras estéreo float a
//! 48 kHz, cada pacote marcado no relógio do `clock` (QPC). O mixer pede
//! "o trecho [t, t+n)" e recebe as amostras daquele instante — com silêncio
//! onde a fonte não tinha nada (loopback sem som tocando não entrega pacote
//! nenhum, e isso tem que virar silêncio, não um buraco que encurta a faixa).
//!
//! `WasapiSource` — microfone (captura) ou som do sistema (loopback do
//! dispositivo de saída). Referência: `plugins/win-wasapi/win-wasapi.cpp`.
//! Diferenças deliberadas: o formato é pedido já em 48 kHz float estéreo
//! (`AUTOCONVERTPCM` — o Windows reamostra, sem resampler nosso), e a leitura
//! é por consulta a cada 5 ms numa thread própria — sem RTWQ (pegadinha #48:
//! foi a RTWQ do plugin que trouxe o áudio mudo e a corrupção de memória).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::core::{implement, Interface, Ref, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, BLOB, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

use crate::log;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY`.
const AUTOCONVERT: u32 = 0x8000_0000 | 0x0800_0000;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
/// Quanto o anel guarda (o mixer anda ~150 ms atrás do relógio).
const RING_SEC: u64 = 3;
/// Desvio entre o instante do pacote e o esperado pela contagem de amostras
/// acima do qual o anel se realinha (deriva do relógio do dispositivo,
/// pacote perdido).
const RESYNC_NS: u64 = 20_000_000;

/// ns de `frames` amostras a 48 kHz.
pub fn frames_to_ns(frames: u64) -> u64 {
    frames * 1_000_000_000 / SAMPLE_RATE as u64
}

/// Amostras em `ns`, arredondado pro mais próximo (`frames_to_ns` trunca: sem
/// arredondar, 100 amostras → 2.083.333 ns → 99 amostras na volta).
pub fn ns_to_frames(ns: u64) -> u64 {
    ((ns as u128 * SAMPLE_RATE as u128 + 500_000_000) / 1_000_000_000) as u64
}

/// Anel de amostras intercaladas (L,R) com o instante da 1ª.
#[derive(Default)]
pub struct SampleRing {
    start_ns: u64,
    samples: VecDeque<f32>,
}

impl SampleRing {
    fn frames(&self) -> u64 {
        (self.samples.len() / CHANNELS) as u64
    }

    fn end_ns(&self) -> u64 {
        self.start_ns + frames_to_ns(self.frames())
    }

    /// Acrescenta `data` (intercalado) que começa no instante `ts_ns`.
    pub fn push(&mut self, ts_ns: u64, data: &[f32]) {
        if self.samples.is_empty() {
            self.start_ns = ts_ns;
        } else {
            let expected = self.end_ns();
            if ts_ns > expected + RESYNC_NS {
                // Buraco (pacote perdido, loopback que voltou a tocar): silêncio até ele.
                let gap = ns_to_frames(ts_ns - expected) as usize * CHANNELS;
                self.samples.extend(std::iter::repeat_n(0.0, gap));
            } else if ts_ns + RESYNC_NS < expected {
                // O dispositivo andou mais rápido que o relógio: corta a sobra do fim.
                let over = (ns_to_frames(expected - ts_ns) as usize * CHANNELS).min(self.samples.len());
                let keep = self.samples.len() - over;
                self.samples.truncate(keep);
                if self.samples.is_empty() {
                    self.start_ns = ts_ns;
                }
            }
        }
        self.samples.extend(data.iter().copied());
        // Limite: descarta o mais antigo.
        let max = ns_to_frames(RING_SEC * 1_000_000_000) as usize * CHANNELS;
        if self.samples.len() > max {
            let drop = self.samples.len() - max;
            self.samples.drain(..drop);
            self.start_ns += frames_to_ns((drop / CHANNELS) as u64);
        }
    }

    /// Soma o trecho `[t0, t0 + out.len()/2 amostras)` em `out` (intercalado),
    /// com ganho `gain`, e descarta o que ficou antes dele. Onde o anel não
    /// tem amostra, nada é somado (= silêncio).
    pub fn mix_into(&mut self, t0_ns: u64, out: &mut [f32], gain: f32) {
        // Descarta o que já passou.
        if self.start_ns < t0_ns && !self.samples.is_empty() {
            let old = (ns_to_frames(t0_ns - self.start_ns) as usize * CHANNELS).min(self.samples.len());
            self.samples.drain(..old);
            self.start_ns += frames_to_ns((old / CHANNELS) as u64);
            if self.samples.is_empty() {
                self.start_ns = t0_ns;
            }
        }
        let n = out.len() / CHANNELS;
        // Onde, dentro do bloco, o anel começa (0 = colado no início).
        let offset = if self.start_ns > t0_ns { ns_to_frames(self.start_ns - t0_ns) as usize } else { 0 };
        if offset >= n || gain == 0.0 {
            return;
        }
        let avail = self.frames() as usize;
        let take = (n - offset).min(avail);
        for i in 0..take * CHANNELS {
            out[offset * CHANNELS + i] += self.samples[i] * gain;
        }
    }
}

/// O que capturar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDevice {
    /// Microfone pelo id do MMDevice (vazio = o padrão do Windows).
    Input(String),
    /// Som do sistema: loopback do dispositivo de saída (vazio = o padrão).
    Output(String),
    /// Som de UM programa (e dos filhos dele), pelo PID: loopback de processo
    /// (`VAD\Process_Loopback`, Windows 10 2004+). É o que o
    /// `wasapi_process_output_capture` do OBS faz pro "som do app" da janela.
    App(u32),
}

/// Uma fonte vista pelo mixer.
pub trait AudioSource: Send + Sync {
    fn name(&self) -> &str;
    /// Soma o trecho `[t0, t0+n)` em `out` (intercalado), já com mudo aplicado.
    fn mix_into(&self, t0_ns: u64, out: &mut [f32]);
    fn set_muted(&self, muted: bool);
}

struct Shared {
    ring: Mutex<SampleRing>,
    muted: AtomicBool,
    stop: AtomicBool,
}

/// Captura WASAPI em thread própria, por consulta.
pub struct WasapiSource {
    name: String,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl WasapiSource {
    /// Abre o dispositivo e começa a capturar. Erro de abertura volta aqui.
    pub fn start(name: &str, device: AudioDevice) -> Result<Self, String> {
        let shared = Arc::new(Shared { ring: Mutex::new(SampleRing::default()), muted: AtomicBool::new(false), stop: AtomicBool::new(false) });
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let sh = shared.clone();
        let label = name.to_owned();
        let thread = std::thread::Builder::new()
            .name(format!("noobs-audio-{name}"))
            .spawn(move || capture_thread(&label, device, sh, tx))
            .map_err(|e| format!("thread de áudio: {e}"))?;
        match rx.recv() {
            Ok(Ok(())) => Ok(Self { name: name.to_owned(), shared, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err("a thread de áudio caiu ao abrir".into())
            }
        }
    }
}

impl AudioSource for WasapiSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn mix_into(&self, t0_ns: u64, out: &mut [f32]) {
        let gain = if self.shared.muted.load(Ordering::Relaxed) { 0.0 } else { 1.0 };
        // Mudo também consome o anel (senão ele voltaria atrasado).
        self.shared.ring.lock().unwrap_or_else(|p| p.into_inner()).mix_into(t0_ns, out, gain);
    }

    fn set_muted(&self, muted: bool) {
        self.shared.muted.store(muted, Ordering::Relaxed);
    }
}

impl Drop for WasapiSource {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn open_device(en: &IMMDeviceEnumerator, device: &AudioDevice) -> Result<IMMDevice, String> {
    let (id, flow) = match device {
        AudioDevice::Input(id) => (id, eCapture),
        AudioDevice::Output(id) => (id, eRender),
        AudioDevice::App(_) => return Err("loopback de processo não tem MMDevice".into()),
    };
    unsafe {
        if id.is_empty() || id.eq_ignore_ascii_case("default") {
            en.GetDefaultAudioEndpoint(flow, eConsole).map_err(|e| format!("dispositivo padrão: {e}"))
        } else {
            let w: Vec<u16> = id.encode_utf16().chain([0]).collect();
            en.GetDevice(PCWSTR(w.as_ptr())).map_err(|e| format!("GetDevice: {e}"))
        }
    }
}

struct Opened {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    /// Só no loopback de processo, que exige modo por evento.
    event: Option<HANDLE>,
}

impl Drop for Opened {
    fn drop(&mut self) {
        if let Some(e) = self.event.take() {
            unsafe {
                let _ = CloseHandle(e);
            }
        }
    }
}

/// Avisa a thread que pediu a ativação assíncrona.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationDone {
    tx: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationDone_Impl {
    fn ActivateCompleted(&self, _op: Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        if let Some(tx) = self.tx.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// `IAudioClient` do loopback de um processo (e da árvore de filhos dele).
fn activate_process_loopback(pid: u32) -> Result<IAudioClient, String> {
    // Na heap: a ativação é assíncrona. Se ela estourar o prazo, o Windows
    // ainda pode ler isto depois de a função voltar — aí fica vazado.
    let params = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId: pid, ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE },
        },
    });
    let pv = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB { cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, pBlobData: &*params as *const _ as *mut u8 },
                },
            }),
        },
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationDone { tx: Mutex::new(Some(tx)) }.into();
    let op = unsafe { ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&pv), &handler) }
        .map_err(|e| format!("ActivateAudioInterfaceAsync (Windows anterior ao 10 2004?): {e}"))?;
    // O PROPVARIANT aponta pra `params` (nosso): não pode ser liberado pelo Drop.
    std::mem::forget(pv);
    if rx.recv_timeout(Duration::from_secs(5)).is_err() {
        std::mem::forget(params);
        return Err("a ativação do loopback de processo não respondeu".into());
    }
    let mut hr = windows::core::HRESULT(0);
    let mut unk = None;
    unsafe { op.GetActivateResult(&mut hr, &mut unk) }.map_err(|e| format!("GetActivateResult: {e}"))?;
    hr.ok().map_err(|e| format!("loopback do processo {pid}: {e}"))?;
    unk.ok_or("sem IAudioClient")?.cast().map_err(|e| format!("IAudioClient: {e}"))
}

fn open(device: &AudioDevice) -> Result<Opened, String> {
    unsafe {
        let client: IAudioClient = match device {
            AudioDevice::App(pid) => activate_process_loopback(*pid)?,
            _ => {
                let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("MMDeviceEnumerator: {e}"))?;
                let dev = open_device(&en, device)?;
                dev.Activate(CLSCTX_ALL, None).map_err(|e| format!("Activate(IAudioClient): {e}"))?
            }
        };
        let wfx = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
            nChannels: CHANNELS as u16,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * 4 * CHANNELS as u32,
            nBlockAlign: 4 * CHANNELS as u16,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        let app = matches!(device, AudioDevice::App(_));
        let flags = match device {
            AudioDevice::Input(_) => AUTOCONVERT,
            AudioDevice::Output(_) => AUTOCONVERT | AUDCLNT_STREAMFLAGS_LOOPBACK,
            // O dispositivo virtual não tem formato de mistura: aceita o que se
            // pedir, sem conversão — e só funciona por evento.
            AudioDevice::App(_) => AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        };
        // 200 ms de buffer no Windows; lemos a cada 5 ms (ou a cada evento).
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 2_000_000, 0, &wfx, None).map_err(|e| format!("IAudioClient::Initialize: {e}"))?;
        let event = if app {
            let ev = CreateEventW(None, false, false, None).map_err(|e| format!("CreateEventW: {e}"))?;
            client.SetEventHandle(ev).map_err(|e| format!("SetEventHandle: {e}"))?;
            Some(ev)
        } else {
            None
        };
        let capture: IAudioCaptureClient = client.GetService().map_err(|e| format!("GetService(IAudioCaptureClient): {e}"))?;
        client.Start().map_err(|e| format!("IAudioClient::Start: {e}"))?;
        Ok(Opened { client, capture, event })
    }
}

fn capture_thread(name: &str, device: AudioDevice, sh: Arc<Shared>, ready: std::sync::mpsc::Sender<Result<(), String>>) {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    match open(&device) {
        Err(e) => {
            log!("Captura: audio \"{name}\" nao abriu: {e}");
            let _ = ready.send(Err(e));
        }
        Ok(o) => {
            log!("Captura: audio \"{name}\" ({device:?}) aberto.");
            let _ = ready.send(Ok(()));
            let mut buf: Vec<f32> = Vec::new();
            while !sh.stop.load(Ordering::SeqCst) {
                if let Err(e) = drain(&o, &sh, &mut buf) {
                    log!("Captura: audio \"{name}\" parou: {e}");
                    break;
                }
                match o.event {
                    Some(ev) => unsafe {
                        WaitForSingleObject(ev, 10);
                    },
                    None => std::thread::sleep(Duration::from_millis(5)),
                }
            }
            unsafe {
                let _ = o.client.Stop();
            }
        }
    }
    if com {
        unsafe { CoUninitialize() };
    }
}

/// Lê todos os pacotes disponíveis pro anel.
fn drain(o: &Opened, sh: &Shared, buf: &mut Vec<f32>) -> Result<(), String> {
    loop {
        let size = unsafe { o.capture.GetNextPacketSize() }.map_err(|e| format!("GetNextPacketSize: {e}"))?;
        if size == 0 {
            return Ok(());
        }
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut frames = 0u32;
        let mut flags = 0u32;
        let mut qpc_100ns = 0u64;
        unsafe { o.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc_100ns)) }.map_err(|e| format!("GetBuffer: {e}"))?;
        let n = frames as usize * CHANNELS;
        buf.clear();
        if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
            buf.resize(n, 0.0);
        } else {
            buf.extend_from_slice(unsafe { std::slice::from_raw_parts(data as *const f32, n) });
        }
        unsafe {
            let _ = o.capture.ReleaseBuffer(frames);
        }
        // O WASAPI dá o instante da 1ª amostra em 100 ns na escala do QPC —
        // a mesma régua do `clock::now_ns`.
        // Sem instante (o loopback de processo pode não dar): agora menos a
        // duração do pacote.
        let ts = if qpc_100ns != 0 { qpc_100ns * 100 } else { super::clock::now_ns().saturating_sub(frames_to_ns(frames as u64)) };
        sh.ring.lock().unwrap_or_else(|p| p.into_inner()).push(ts, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(n: usize, v: f32) -> Vec<f32> {
        vec![v; n * CHANNELS]
    }

    #[test]
    fn trecho_alinhado_pelo_instante() {
        let mut r = SampleRing::default();
        let t = 1_000_000_000;
        r.push(t, &block(480, 1.0)); // 10 ms de 1.0
        let mut out = vec![0.0; 960 * CHANNELS];
        r.mix_into(t, &mut out, 1.0);
        assert!(out[..480 * CHANNELS].iter().all(|&s| s == 1.0));
        assert!(out[480 * CHANNELS..].iter().all(|&s| s == 0.0), "depois do fim é silêncio");
    }

    #[test]
    fn fonte_atrasada_cai_no_meio_do_bloco() {
        let mut r = SampleRing::default();
        let t = 1_000_000_000;
        r.push(t + frames_to_ns(100), &block(50, 0.5));
        let mut out = vec![0.0; 256 * CHANNELS];
        r.mix_into(t, &mut out, 1.0);
        assert_eq!(out[99 * CHANNELS], 0.0);
        assert_eq!(out[100 * CHANNELS], 0.5);
        assert_eq!(out[149 * CHANNELS], 0.5);
        assert_eq!(out[150 * CHANNELS], 0.0);
    }

    #[test]
    fn descarta_o_que_ja_passou() {
        let mut r = SampleRing::default();
        let t = 1_000_000_000;
        let mut data = block(200, 1.0);
        data[100 * CHANNELS..].fill(2.0);
        r.push(t, &data);
        let mut out = vec![0.0; 100 * CHANNELS];
        r.mix_into(t + frames_to_ns(100), &mut out, 1.0);
        assert!(out.iter().all(|&s| s == 2.0), "o 1º bloco devia ter sido pulado");
    }

    #[test]
    fn buraco_vira_silencio_e_deriva_e_cortada() {
        let mut r = SampleRing::default();
        let t = 1_000_000_000;
        r.push(t, &block(480, 1.0)); // 0..10 ms
        // Próximo pacote só 50 ms depois (loopback ficou sem som): silêncio no meio.
        r.push(t + 50_000_000, &block(480, 3.0));
        let mut out = vec![0.0; 2880 * CHANNELS]; // 60 ms
        r.mix_into(t, &mut out, 1.0);
        assert_eq!(out[0], 1.0);
        assert_eq!(out[1000 * CHANNELS], 0.0);
        assert_eq!(out[2400 * CHANNELS], 3.0, "pacote depois do buraco no instante certo");
        // Dispositivo adiantado: pacote marcado 30 ms ANTES do fim esperado.
        let mut r = SampleRing::default();
        r.push(t, &block(4800, 1.0)); // 0..100 ms
        r.push(t + 70_000_000, &block(480, 5.0));
        let mut out = vec![0.0; 4800 * CHANNELS];
        r.mix_into(t, &mut out, 1.0);
        assert_eq!(out[(70 * 48) * CHANNELS], 5.0, "realinhado no instante do pacote");
    }

    #[test]
    fn mudo_consome_sem_somar() {
        let mut r = SampleRing::default();
        r.push(0, &block(100, 1.0));
        let mut out = vec![0.0; 100 * CHANNELS];
        r.mix_into(0, &mut out, 0.0);
        assert!(out.iter().all(|&s| s == 0.0));
    }
}

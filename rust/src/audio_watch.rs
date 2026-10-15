//! Detecta hot-plug de dispositivos de áudio (mic ou saída) com o
//! `IMMNotificationClient` da MMDevice API (porte de `OBSAudioWatch.pas`).
//! A notificação é nativa: chega em ~ms, sem polling.
//!
//! O callback roda na thread do WASAPI — nada de UI/libobs ali; o Bridge
//! marshala para a main. Propriedade alterada é ignorada de propósito: dispara
//! para tudo (volume, formato…) e não serve para detectar plug/unplug.
//!
//! No Delphi era refcount manual e um `FStopped` para o callback não rodar com
//! o Bridge desmontado, com uma ref local no `Stop` contra use-after-free.
//! Aqui o `#[implement]` cuida do refcount e o `Arc<AtomicBool>` faz o papel do
//! `FStopped`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    EDataFlow, ERole, IMMNotificationClient, IMMNotificationClient_Impl, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

use crate::log;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceChangeKind {
    Added,
    Removed,
    StateChanged,
    DefaultChanged,
}

impl DeviceChangeKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::StateChanged => "stateChanged",
            Self::DefaultChanged => "defaultChanged",
        }
    }
}

pub type Callback = Arc<dyn Fn(DeviceChangeKind, &str) + Send + Sync>;

#[implement(IMMNotificationClient)]
struct NotifClient {
    callback: Callback,
    /// Setado por `stop`: ignora callbacks que ainda cheguem durante o teardown.
    stopped: Arc<AtomicBool>,
}

impl NotifClient {
    fn notify(&self, kind: DeviceChangeKind, id: &PCWSTR) {
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }
        let id = unsafe { id.to_string().unwrap_or_default() };
        let cb = self.callback.clone();
        // Um panic do handler não pode atravessar a fronteira COM.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(kind, &id)));
    }
}

#[allow(non_snake_case)]
impl IMMNotificationClient_Impl for NotifClient_Impl {
    fn OnDeviceStateChanged(&self, pwstrdeviceid: &PCWSTR, _dwnewstate: DEVICE_STATE) -> windows::core::Result<()> {
        self.notify(DeviceChangeKind::StateChanged, pwstrdeviceid);
        Ok(())
    }
    fn OnDeviceAdded(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        self.notify(DeviceChangeKind::Added, pwstrdeviceid);
        Ok(())
    }
    fn OnDeviceRemoved(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        self.notify(DeviceChangeKind::Removed, pwstrdeviceid);
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        _flow: EDataFlow,
        _role: ERole,
        pwstrdefaultdeviceid: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.notify(DeviceChangeKind::DefaultChanged, pwstrdefaultdeviceid);
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _pwstrdeviceid: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(()) // ignorado: ver o cabeçalho do módulo
    }
}

struct Watch {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
    stopped: Arc<AtomicBool>,
}
// O objeto vive entre register/unregister e só é tocado em start/stop.
unsafe impl Send for Watch {}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);

pub fn start(callback: Callback) {
    let mut guard = WATCH.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_some() {
        return;
    }
    let enumerator =
        match unsafe { CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) } {
            Ok(e) => e,
            Err(e) => {
                log!("AudioWatch: CoCreateInstance MMDeviceEnumerator falhou ({:x})", e.code().0);
                return;
            }
        };
    let stopped = Arc::new(AtomicBool::new(false));
    let client: IMMNotificationClient = NotifClient { callback, stopped: stopped.clone() }.into();
    if let Err(e) = unsafe { enumerator.RegisterEndpointNotificationCallback(&client) } {
        log!("AudioWatch: RegisterEndpointNotificationCallback falhou ({:x})", e.code().0);
        return;
    }
    *guard = Some(Watch { enumerator, client, stopped });
    log!("AudioWatch: monitorando hot-plug de dispositivos de audio.");
}

pub fn stop() {
    let Some(w) = WATCH.lock().unwrap_or_else(|p| p.into_inner()).take() else { return };
    // 1. Sinaliza para o callback parar de disparar (antes do unregister).
    w.stopped.store(true, Ordering::SeqCst);
    // 2. `w.client` mantém o objeto vivo até o fim do unregister, mesmo com um
    //    callback WASAPI em andamento: sem use-after-free.
    unsafe {
        let _ = w.enumerator.UnregisterEndpointNotificationCallback(&w.client);
    }
}

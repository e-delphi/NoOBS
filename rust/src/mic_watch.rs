//! Detecta o microfone em uso por OUTRO app (chamada de Teams/WhatsApp/Zoom…)
//! pelas sessões WASAPI de captura, para auto-iniciar e auto-parar a gravação
//! — porte de `WinMicWatch.pas` (pegadinha #47).
//!
//! Sessão `Active`, de um processo que casa com o filtro e que NÃO é o NoOBS =
//! "microfone em uso". Thread própria (COM MTA), poll de 1 s; o callback roda
//! NA THREAD do watcher e só na TRANSIÇÃO.
//!
//! * A PRIMEIRA leitura é linha de base e não dispara: um mic já em uso quando
//!   o watcher sobe é uma chamada EM CURSO, não uma nova (sem isso, abrir o
//!   NoOBS no meio de uma chamada a gravaria sem ninguém pedir).
//! * Exclui o próprio PID (o NoOBS abre o mic pros medidores — pegadinha #12)
//!   E o próprio NOME de exe: outra instância do NoOBS (um processo que acabou
//!   de sair deixa a sessão `Active` por ~1-2 s) não é chamada.
//! * Filtro de monitorados vazio = qualquer app; as EXCEÇÕES só valem com ele
//!   vazio ("tudo, menos estes"). Exceções vazias = nada ignorado.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eCapture, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::log;
use crate::proc_watch::{app_in_list, own_exe_name, parse_apps};

const POLL: Duration = Duration::from_millis(1000);
const SLICE: Duration = Duration::from_millis(50);

pub type Callback = fn(bool);

struct Watcher {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

static WATCHER: Mutex<Option<Watcher>> = Mutex::new(None);

/// Nome do executável (basename minúsculo) de um PID; vazio se não der.
fn process_name(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return String::new();
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        full.rsplit('\\').next().unwrap_or("").to_lowercase()
    }
}

/// Uma varredura. O enumerador de sessões é um SNAPSHOT: recriado a cada poll.
fn poll_mic_in_use(wanted: &[String], except: &[String], own_pid: u32, own_name: &str) -> windows::core::Result<bool> {
    unsafe {
        let devs: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)?;
        let coll = devs.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        let count = coll.GetCount()?;
        for d in 0..count {
            let Ok(dev) = coll.Item(d) else { continue };
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_INPROC_SERVER, None) else { continue };
            let Ok(sessions) = mgr.GetSessionEnumerator() else { continue };
            let n = sessions.GetCount().unwrap_or(0);
            for i in 0..n {
                let Ok(ctrl) = sessions.GetSession(i) else { continue };
                if ctrl.GetState().ok() != Some(AudioSessionStateActive) {
                    continue;
                }
                let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else { continue };
                // S_OK = sessão de sons do sistema.
                if ctrl2.IsSystemSoundsSession().0 == 0 {
                    continue;
                }
                let pid = ctrl2.GetProcessId().unwrap_or(0);
                if pid == 0 || pid == own_pid {
                    continue;
                }
                let name = process_name(pid);
                // Outra instância do NoOBS (ou uma que acabou de sair).
                if name.is_empty() || name == own_name {
                    continue;
                }
                if wanted.is_empty() {
                    if !app_in_list(&name, except) {
                        return Ok(true);
                    }
                } else if app_in_list(&name, wanted) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

/// Inicia (no-op se já rodando).
pub fn start(apps_filter: &str, except_filter: &str, callback: Callback) {
    let mut w = WATCHER.lock().unwrap_or_else(|p| p.into_inner());
    if w.is_some() {
        return;
    }
    let (apps, except) = (parse_apps(apps_filter), parse_apps(except_filter));
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let handle = std::thread::spawn(move || {
        let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        let (own_pid, own_name) = (unsafe { GetCurrentProcessId() }, own_exe_name());
        let mut last: Option<bool> = None;
        while !stop2.load(Ordering::SeqCst) {
            // Erro transitório: preserva o estado anterior.
            let in_use = poll_mic_in_use(&apps, &except, own_pid, &own_name).unwrap_or(last.unwrap_or(false));
            match last {
                // Linha de base: não dispara.
                None => last = Some(in_use),
                Some(prev) if prev != in_use => {
                    last = Some(in_use);
                    callback(in_use);
                }
                _ => {}
            }
            let mut slept = Duration::ZERO;
            while slept < POLL && !stop2.load(Ordering::SeqCst) {
                std::thread::sleep(SLICE);
                slept += SLICE;
            }
        }
        if com_ok {
            unsafe { CoUninitialize() };
        }
    });
    *w = Some(Watcher { stop, handle });
    log!("WinMicWatch: iniciado (monitorar=\"{apps_filter}\" excecoes=\"{except_filter}\").");
}

pub fn stop() {
    let w = WATCHER.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(w) = w {
        w.stop.store(true, Ordering::SeqCst);
        let _ = w.handle.join();
        log::log("WinMicWatch: parado.");
    }
}

pub fn is_running() -> bool {
    WATCHER.lock().unwrap_or_else(|p| p.into_inner()).is_some()
}

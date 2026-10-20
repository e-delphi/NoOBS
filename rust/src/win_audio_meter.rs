//! Dispositivos de áudio (mics e saídas) via WASAPI/MMDevice e pico por
//! dispositivo via `IAudioMeterInformation` (porte de `WinAudioMeter.pas`).
//! Substitui o evento `InputVolumeMeters` do OBS: a UI funciona inteira sem o
//! motor de captura de pé.
//!
//! Pegadinhas que seguem valendo:
//! * #12 — o pico de um endpoint de CAPTURA é 0 sem uma sessão aberta: o
//!   medidor segura um `IAudioClient` shared por mic (nunca consome o buffer);
//! * #30 — `RebuildCache` faz chamadas WASAPI bloqueantes e roda só em worker;
//!   `read_peak_levels`/`read_input_mutes` (main, a cada 100 ms) só LEEM de
//!   interfaces já ativadas;
//! * #59 — o mudo que se enxerga é o do ENDPOINT, e `Activate` do
//!   `IAudioEndpointVolume` mora no rebuild pelo mesmo motivo.

use std::sync::Mutex;

use windows::core::PWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGM_READ,
};

use crate::log;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Input,
    Output,
}

#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    /// Id único do MMDevice (o mesmo que a captura WASAPI abre).
    pub device_id: String,
    /// Nome amigável.
    pub name: String,
    pub kind: DeviceKind,
    pub is_default: bool,
    /// Atrás de um adapter Bluetooth — a UI avisa da limitação do perfil HFP.
    pub is_bluetooth: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AudioLevel {
    pub device_id: String,
    /// 0..1 — pico total (máximo dos canais).
    pub peak: f32,
    pub left: f32,
    pub right: f32,
    /// 1 = mono, 2+ = estéreo (só L/R são expostos).
    pub channels: i32,
}

/// Mudo do ENDPOINT de captura, no nível do Windows — o mesmo que o botão de
/// mudo do headset e o mudo do sistema mexem. Não é o mudo interno de um app.
#[derive(Debug, Clone)]
pub struct AudioMute {
    pub device_id: String,
    pub muted: bool,
}

/// Consultada antes de abrir a sessão de captura de um mic. Existe porque a
/// sessão deixa o dispositivo "em uso" no Windows (indicador de privacidade,
/// LED da webcam). `false` = não segura o mic (pico fica 0). Sem política,
/// abre todos.
pub type MicOpenPolicy = fn(&str) -> bool;

/// Interfaces COM guardadas num `static`. Os objetos nascem num worker MTA e
/// são lidos pela main: ficam atrás do `Mutex`, e o MTA permite o acesso.
struct Com<T>(T);
unsafe impl<T> Send for Com<T> {}

struct MeterEntry {
    info: AudioDeviceInfo,
    /// Guardado para `apply_mic_open_policy` REABRIR a sessão sem re-enumerar.
    dev: Com<IMMDevice>,
    meter: Option<Com<IAudioMeterInformation>>,
    /// Só mics: sem sessão de captura viva o `GetPeakValue` devolve 0.
    client: Option<Com<IAudioClient>>,
    /// Controle de mudo do endpoint (só entradas). `None` = sem controle.
    endpoint_vol: Option<Com<IAudioEndpointVolume>>,
}

#[derive(Default)]
struct State {
    enumerator: Option<Com<IMMDeviceEnumerator>>,
    cache: Vec<MeterEntry>,
    mic_open_policy: Option<MicOpenPolicy>,
}

/// Serializa o cache e o rebuild: sem isso, o worker (`DoRefreshAudio`) e a
/// main (`TIMER_AUDIO_METER`) entrariam no rebuild juntos e duplicariam devices.
static STATE: Mutex<State> = Mutex::new(State { enumerator: None, cache: Vec::new(), mic_open_policy: None });

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------------------
// Propriedades do dispositivo
// ---------------------------------------------------------------------------

fn pwstr_to_string_free(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string().unwrap_or_default() };
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

/// Lê uma propriedade string do property store. Só libera se for realmente uma
/// string COM-alocada (`PropVariantToStringAlloc` já garante isso).
fn property_string(dev: &IMMDevice, key: &PROPERTYKEY) -> String {
    unsafe {
        let Ok(store) = dev.OpenPropertyStore(STGM_READ) else { return String::new() };
        let Ok(mut pv) = store.GetValue(key) else { return String::new() };
        let s = PropVariantToStringAlloc(&pv).map(pwstr_to_string_free).unwrap_or_default();
        let _ = PropVariantClear(&mut pv);
        s
    }
}

fn friendly_name(dev: &IMMDevice) -> String {
    property_string(dev, &PKEY_Device_FriendlyName)
}

/// Atrás do bus Bluetooth (`PKEY_Device_EnumeratorName` = `BTHENUM`). Fallback
/// heurístico: vários drivers não reportam BTHENUM no endpoint mas põem
/// "Hands-Free"/"Bluetooth" no nome.
fn is_bluetooth_device(dev: &IMMDevice, friendly: &str) -> bool {
    // PKEY_Device_EnumeratorName: mesmo fmtid do FriendlyName, pid 24.
    let key = PROPERTYKEY { fmtid: PKEY_Device_FriendlyName.fmtid, pid: 24 };
    if property_string(dev, &key).eq_ignore_ascii_case("BTHENUM") {
        return true;
    }
    let lower = friendly.to_lowercase();
    lower.contains("hands-free") || lower.contains("hands free") || lower.contains("bluetooth")
}

fn device_id(dev: &IMMDevice) -> String {
    unsafe { dev.GetId().map(pwstr_to_string_free).unwrap_or_default() }
}

// ---------------------------------------------------------------------------
// Ciclo de vida
// ---------------------------------------------------------------------------

pub fn init_audio() {
    unsafe {
        // MTA (0). Na main (já STA) devolve RPC_E_CHANGED_MODE — ignorado.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut st = lock();
    if st.enumerator.is_none() {
        if let Ok(e) = unsafe { CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) } {
            st.enumerator = Some(Com(e));
        }
    }
}

/// Para as sessões e solta as refs COM uma a uma. Device desconectado
/// (Bluetooth/USB) durante a gravação deixa o proxy COM instável: soltar com
/// cuidado evita AV nativo.
fn release_cache(st: &mut State) {
    for e in st.cache.drain(..) {
        if let Some(c) = &e.client {
            let _ = unsafe { c.0.Stop() };
        }
        drop(e);
    }
}

pub fn done_audio() {
    let mut st = lock();
    release_cache(&mut st);
    st.enumerator = None;
}

pub fn set_mic_open_policy(policy: MicOpenPolicy) {
    lock().mic_open_policy = Some(policy);
}

/// Sem política = abre todos. (Falha na política em Delphi caía em "abre"; em
/// Rust a política é uma `fn` simples, sem exceção.)
fn mic_should_open(st: &State, name: &str) -> bool {
    st.mic_open_policy.map(|p| p(name)).unwrap_or(true)
}

/// Ativa um `IAudioClient` de captura em shared mode com o mix format do device
/// e dá `Start`. Não consumimos o buffer — o sistema faz overflow, mas o
/// `IAudioMeterInformation` continua atualizando.
fn start_capture_for_meter(dev: &IMMDevice) -> Option<IAudioClient> {
    unsafe {
        let client = dev.Activate::<IAudioClient>(CLSCTX_INPROC_SERVER, None).ok()?;
        let fmt = client.GetMixFormat().ok()?;
        if fmt.is_null() {
            return None;
        }
        // Buffer de 200 ms (em unidades de 100 ns).
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 2_000_000, 0, fmt, None);
        CoTaskMemFree(Some(fmt as *const _));
        init.ok()?;
        client.Start().ok()?;
        Some(client)
    }
}

fn has_device(st: &State, id: &str, kind: DeviceKind) -> bool {
    st.cache.iter().any(|e| e.info.kind == kind && e.info.device_id.eq_ignore_ascii_case(id))
}

/// O chamador segura o lock. Chamadas WASAPI bloqueantes: só em worker.
fn rebuild_cache(st: &mut State) {
    st.cache.clear();
    let Some(enumerator) = st.enumerator.as_ref().map(|e| e.0.clone()) else { return };

    let kinds: [(DeviceKind, EDataFlow, &str); 2] =
        [(DeviceKind::Input, eCapture, "IN "), (DeviceKind::Output, eRender, "OUT")];
    for (kind, flow, tag) in kinds {
        let word = if kind == DeviceKind::Input { "input" } else { "output" };
        let def_id = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }
            .map(|d| device_id(&d))
            .unwrap_or_default();
        log!("WinAudioMeter: {tag} default id=\"{}\"", if def_id.is_empty() { "<none>" } else { &def_id });

        let Ok(coll) = (unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }) else {
            log!("WinAudioMeter: EnumAudioEndpoints falhou pra {word}");
            continue;
        };
        let count = unsafe { coll.GetCount() }.unwrap_or(0);
        log!("WinAudioMeter: {tag} collection.GetCount={count}");
        if count == 0 {
            log!("WinAudioMeter: 0 devices ativos de {word}");
            continue;
        }
        for i in 0..count {
            let Ok(dev) = (unsafe { coll.Item(i) }) else {
                log!("WinAudioMeter: Coll.Item({i}) falhou em {word}");
                continue;
            };
            let id = device_id(&dev);
            // Dedup: após hot-plug rápido o WASAPI pode devolver o mesmo endpoint
            // duas vezes em estado transitório.
            if !id.is_empty() && has_device(st, &id, kind) {
                log!("WinAudioMeter: dedup {tag} id=\"{id}\"");
                continue;
            }
            let name = friendly_name(&dev);
            // IsDefault só se def_id existe — evita falso-positivo com os dois vazios.
            let is_default = !def_id.is_empty() && id.eq_ignore_ascii_case(&def_id);
            let is_bluetooth = is_bluetooth_device(&dev, &name);
            log!(
                "WinAudioMeter: + {tag} \"{name}\" id=\"{id}\"{}{}",
                if is_default { " [default]" } else { "" },
                if is_bluetooth { " [bluetooth]" } else { "" }
            );

            let meter = unsafe { dev.Activate::<IAudioMeterInformation>(CLSCTX_INPROC_SERVER, None) }.ok();

            // Mudo do endpoint — só ENTRADA ("o microfone está mudo no
            // Windows?"). Falhar aqui não é fatal: o device só não entra no
            // read_input_mutes e quem consulta trata como "não sei".
            let mut endpoint_vol = None;
            if kind == DeviceKind::Input {
                match unsafe { dev.Activate::<IAudioEndpointVolume>(CLSCTX_INPROC_SERVER, None) } {
                    Ok(v) => endpoint_vol = Some(Com(v)),
                    Err(_) => log!("WinAudioMeter: sem controle de mudo em \"{name}\""),
                }
            }

            // Inputs: IAudioClient shared só para manter a sessão viva. A
            // política evita segurar mic que o usuário desmarcou ou ocultou.
            let mut client = None;
            if kind == DeviceKind::Input && meter.is_some() {
                if mic_should_open(st, &name) {
                    client = start_capture_for_meter(&dev).map(Com);
                } else {
                    log!("WinAudioMeter: mic \"{name}\" inativo — nao abre sessao de captura.");
                }
            }

            st.cache.push(MeterEntry {
                info: AudioDeviceInfo { device_id: id, name, kind, is_default, is_bluetooth },
                dev: Com(dev),
                meter: meter.map(Com),
                client,
                endpoint_vol,
            });
        }
    }
}

/// Lista (e popula, se vazio) o cache. Pode bloquear — worker.
pub fn enumerate_audio_devices() -> Vec<AudioDeviceInfo> {
    if lock().enumerator.is_none() {
        init_audio();
    }
    let mut st = lock();
    // Double-check: outra thread pode ter populado o cache enquanto esperávamos o lock.
    if st.cache.is_empty() {
        rebuild_cache(&mut st);
    }
    st.cache.iter().map(|e| e.info.clone()).collect()
}

/// Nome amigável de um device id (cache primeiro; senão pergunta direto ao
/// WASAPI — devices que acabaram de aparecer ou já saíram do cache).
pub fn resolve_device_name(device_id_str: &str) -> String {
    if device_id_str.is_empty() {
        return String::new();
    }
    let enumerator = {
        let st = lock();
        if let Some(e) = st.cache.iter().find(|e| e.info.device_id.eq_ignore_ascii_case(device_id_str)) {
            return e.info.name.clone();
        }
        st.enumerator.as_ref().map(|e| e.0.clone())
    };
    // Fora do lock: não bloqueia o timer de medidores durante a chamada WASAPI.
    let Some(enumerator) = enumerator else { return String::new() };
    let wide: Vec<u16> = device_id_str.encode_utf16().chain(std::iter::once(0)).collect();
    match unsafe { enumerator.GetDevice(windows::core::PCWSTR(wide.as_ptr())) } {
        Ok(dev) => friendly_name(&dev),
        Err(_) => String::new(),
    }
}

/// Re-enumera do zero (após hot-plug); o rebuild é lazy na próxima
/// `enumerate_audio_devices`.
pub fn refresh_audio_devices() {
    if lock().enumerator.is_none() {
        init_audio();
    }
    let mut st = lock();
    release_cache(&mut st);
}

/// Reaplica a política aos mics JÁ em cache: fecha a sessão dos que deixaram de
/// ser ativos e abre a dos que voltaram. Barato e sem piscar os medidores.
pub fn apply_mic_open_policy() {
    let mut st = lock();
    let policy = st.mic_open_policy;
    for e in st.cache.iter_mut() {
        if e.info.kind != DeviceKind::Input {
            continue;
        }
        let want = policy.map(|p| p(&e.info.name)).unwrap_or(true);
        if want && e.client.is_none() && e.meter.is_some() {
            // Voltou a ser ativo: reabre a sessão para o medidor sair do zero.
            e.client = start_capture_for_meter(&e.dev.0).map(Com);
            if e.client.is_none() {
                log!("WinAudioMeter: reabrir \"{}\" falhou.", e.info.name);
            }
        } else if !want {
            if let Some(c) = e.client.take() {
                // Deixou de ser ativo: SOLTA o microfone (some o "em uso" do Windows).
                let _ = unsafe { c.0.Stop() };
                log!("WinAudioMeter: mic \"{}\" liberado (inativo).", e.info.name);
            }
        }
    }
}

/// Picos por device. Estéreo: canais L/R individuais via `GetChannelsPeakValues`;
/// mono (ou falha): `GetPeakValue` duplicado em L/R. NÃO reconstrói o cache —
/// roda na main a cada 100 ms e o rebuild bloqueia (pegadinha #30); sem cache
/// devolve vazio e tenta no próximo tique.
pub fn read_peak_levels() -> Vec<AudioLevel> {
    const MAX_CH: usize = 8; // 7.1 é o limite do OBS
    let st = lock();
    let mut out = Vec::new();
    for e in &st.cache {
        let Some(meter) = &e.meter else { continue };
        let meter = &meter.0;
        let ch = unsafe { meter.GetMeteringChannelCount() }.unwrap_or(1).clamp(1, MAX_CH as u32) as usize;
        let mut buf = [0f32; MAX_CH];
        let mut lvl = AudioLevel { device_id: e.info.device_id.clone(), channels: ch as i32, ..Default::default() };
        if ch >= 2 && unsafe { meter.GetChannelsPeakValues(&mut buf[..ch]) }.is_ok() {
            lvl.left = buf[0];
            lvl.right = buf[1];
            lvl.peak = buf[..ch].iter().cloned().fold(0.0, f32::max);
        } else {
            let Ok(p) = (unsafe { meter.GetPeakValue() }) else { continue };
            lvl.peak = p;
            lvl.left = p;
            lvl.right = p;
        }
        out.push(lvl);
    }
    out
}

/// Mudo do endpoint de cada ENTRADA em cache. Só entram os que responderam:
/// "ausente" é "não sei", nunca "não está mudo". Só lê de interfaces já
/// ativadas (main thread, 100 ms).
pub fn read_input_mutes() -> Vec<AudioMute> {
    let st = lock();
    let mut out = Vec::new();
    for e in &st.cache {
        if e.info.kind != DeviceKind::Input {
            continue;
        }
        let Some(vol) = &e.endpoint_vol else { continue };
        // Device em transição (BT desconectando, USB removido): sai da lista em
        // vez de derrubar o tique inteiro. BOOL → bool por TESTE (um driver pode
        // devolver 0xFFFFFFFF).
        let Ok(m) = (unsafe { vol.0.GetMute() }) else { continue };
        out.push(AudioMute { device_id: e.info.device_id.clone(), muted: m.as_bool() });
    }
    out
}

//! Dispositivos da tela inicial no Bridge (porte do trecho de monitores,
//! webcams e áudio do `OBSBridge.pas`): listas para a UI, `init`, refresh por
//! hot-plug (áudio/monitor/webcam) com diff do que mudou, medidores de nível,
//! mudo do microfone espelhado do endpoint e prévias dos monitores.
//!
//! Tudo na main thread, exceto o que é dito em contrário: enumeração WASAPI e
//! captura de prévia rodam em workers e só o `post` volta pela fila da main
//! (pegadinhas #22 e #30).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::audio_tracks::compute_audio_track_assignments;
use crate::audio_watch::{self, DeviceChangeKind};
use crate::engine;
use crate::types::Webcam;
use crate::win_audio_meter::{self as wam, AudioDeviceInfo, DeviceKind};
use crate::win_preview::{self, MonitorInfo};
use crate::{config, lang, log, mainthread, player, ui, win_webcam};

use super::recording::{push_refresh_busy, record_dir, recording_active};
use super::{is_shutting_down, post_json};

pub const PFX_MONITOR: &str = "NoOBS Monitor ";
pub const PFX_MIC: &str = "NoOBS Mic - ";
pub const PFX_OUT: &str = "NoOBS Out - ";
pub const PFX_WEBCAM: &str = "NoOBS Webcam - ";

pub const TIMER_AUDIO_REFRESH: usize = 7003;
pub const TIMER_MONITOR_REFRESH: usize = 7004;
pub const TIMER_AUDIO_METER: usize = 7005;
pub const TIMER_WEBCAM_REFRESH: usize = 7007;
const AUDIO_REFRESH_DEBOUNCE_MS: u32 = 800;
const MONITOR_REFRESH_DEBOUNCE_MS: u32 = 2500;
const WEBCAM_REFRESH_DEBOUNCE_MS: u32 = 800;
const AUDIO_METER_MS: u32 = 100;
/// 2 FPS: boa sensação de "ao vivo" sem custo proibitivo de BitBlt.
const THUMB_TICK_MS: u64 = 500;

#[derive(Default)]
struct DevState {
    /// O JS já mandou `ready` e o init terminou.
    initialized: bool,
    /// Setado no topo do `do_init`: um segundo `ready` (reload) não re-arma timers.
    init_started: bool,
    player_open: bool,
    pending_audio_refresh: bool,
    pending_monitor_refresh: bool,
    pending_webcam_refresh: bool,
    /// Re-entrância POR TIPO: uma flag global fazia o refresh de monitor ser
    /// ignorado enquanto o de áudio ainda estava no worker.
    audio_refresh_in_progress: bool,
    monitor_refresh_in_progress: bool,
    last_monitor_count: i32,
    monitor_retry_attempts: i32,
    /// Assinatura determinista do áudio que a UI conhece: deduplica refreshes
    /// (os eventos do WASAPI vêm em rajada, e não queremos o banner se a lista
    /// visível segue idêntica).
    last_applied_audio_sig: String,
    last_applied_audio_devs: Vec<AudioDeviceInfo>,
    last_applied_monitors: Vec<MonitorInfo>,
    last_applied_webcams: Vec<Webcam>,
    /// Snapshot dos monitores no início da gravação: estabiliza os slots de
    /// prévia mesmo se o usuário desplugar um monitor gravando.
    recording_monitors_snapshot: Vec<MonitorInfo>,
    /// Último mudo APLICADO por fonte de mic — o tique de 100 ms não pode
    /// chamar o libobs sem mudança nenhuma.
    mic_mute_applied: HashMap<String, bool>,
    thumb_thread_started: bool,
}

thread_local! {
    static DEV: RefCell<DevState> = RefCell::new(DevState { last_monitor_count: -1, ..Default::default() });
}

fn with_dev<R>(f: impl FnOnce(&mut DevState) -> R) -> R {
    DEV.with(|d| f(&mut d.borrow_mut()))
}

/// Pausa da captura de prévias enquanto o player está aberto (a sidebar fica
/// atrás do modal): a UI manda `player_state`.
pub fn set_player_open(open: bool) {
    with_dev(|d| d.player_open = open);
}

pub fn player_open() -> bool {
    with_dev(|d| d.player_open)
}

// ---------------------------------------------------------------------------
// Ids de fonte
// ---------------------------------------------------------------------------

/// `"NoOBS Monitor 1"` → `("monitors", "1")`; `"NoOBS Mic - X"` → `("mics", "X")`;
/// `"NoOBS Out - Y"` → `("speakers", "Y")`; `"NoOBS Webcam - Z"` → `("webcams", "Z")`.
/// Comparação sem diferenciar caixa, como o `StartsText` do Delphi.
pub fn split_source_id(id: &str) -> Option<(&'static str, String)> {
    const PREFIXES: [(&str, &str); 4] = [
        (PFX_MONITOR, "monitors"),
        (PFX_MIC, "mics"),
        (PFX_OUT, "speakers"),
        (PFX_WEBCAM, "webcams"),
    ];
    let lower = id.to_lowercase();
    PREFIXES.iter().find_map(|(p, cat)| {
        lower.starts_with(&p.to_lowercase()).then(|| (*cat, id.chars().skip(p.chars().count()).collect()))
    })
}

pub fn monitor_id_from_index(i: i32) -> String {
    format!("{PFX_MONITOR}{i}")
}
pub fn mic_id_from_name(name: &str) -> String {
    format!("{PFX_MIC}{name}")
}
pub fn out_id_from_name(name: &str) -> String {
    format!("{PFX_OUT}{name}")
}
pub fn webcam_id_from_name(name: &str) -> String {
    format!("{PFX_WEBCAM}{name}")
}

fn get_source_enabled(id: &str, default: bool) -> bool {
    match split_source_id(id) {
        Some((cat, raw)) => config::get_source_bool(cat, &raw, default),
        None => default,
    }
}

fn get_source_hidden_by_id(id: &str) -> bool {
    split_source_id(id).map(|(cat, raw)| config::get_source_hidden(cat, &raw)).unwrap_or(false)
}

fn get_source_auto_record_by_id(id: &str) -> bool {
    split_source_id(id).map(|(cat, raw)| config::get_source_auto_record(cat, &raw)).unwrap_or(true)
}

/// Política do medidor de nível: só segura o microfone se ele estiver marcado
/// E visível. Sem isso o NoOBS mantinha uma sessão de captura em TODO mic do
/// sistema (pegadinha #12) e o Windows mostrava o dispositivo "em uso".
fn mic_meter_open_policy(name: &str) -> bool {
    config::get_source_active("mics", name, true)
}

// ---------------------------------------------------------------------------
// Listas para a UI
// ---------------------------------------------------------------------------

fn build_monitors_json() -> Value {
    let mons = win_preview::enumerate_monitors();
    Value::Array(
        mons.iter()
            .map(|m| {
                let id = monitor_id_from_index(m.index);
                let thumb = win_preview::capture_monitor_as_data_url(m, 480, 270);
                let mut item = json!({
                    "id": id,
                    "name": format!("Monitor {}", m.index + 1),
                    "info": format!("{}x{} @ {},{}", m.width, m.height, m.x, m.y),
                    "enabled": get_source_enabled(&id, true),
                    "hidden": get_source_hidden_by_id(&id),
                    "auto": get_source_auto_record_by_id(&id),
                    // x/y/width/height numéricos pro layout proporcional da UI.
                    "x": m.x, "y": m.y, "width": m.width, "height": m.height,
                });
                if !thumb.is_empty() {
                    item["thumb"] = json!(thumb);
                }
                item
            })
            .collect(),
    )
}

fn build_webcams_json_from(cams: &[Webcam]) -> Value {
    Value::Array(
        cams.iter()
            .map(|c| {
                let id = webcam_id_from_name(&c.name);
                json!({
                    "id": id,
                    "name": c.name,
                    "info": format!("{}x{}", c.width, c.height),
                    // Default DESMARCADO para webcams (gravação normalmente não precisa).
                    "enabled": get_source_enabled(&id, false),
                    "hidden": get_source_hidden_by_id(&id),
                    "auto": get_source_auto_record_by_id(&id),
                    "width": c.width, "height": c.height,
                })
            })
            .collect(),
    )
}

/// Ordena os 4 vetores paralelos pela faixa ascendente (insertion sort estável;
/// faixa 0 = desabilitado = +∞, vai pro fim). A lista da UI segue a numeração
/// das faixas — coerente com a legenda, mesmo com a faixa agrupada indo pra 6.
fn sort_audio_parallel_by_track(idxs: &mut [usize], tracks: &mut [i32], enabled: &mut [bool], default: &mut [bool]) {
    let key = |t: i32| if t == 0 { i32::MAX } else { t };
    for i in 1..tracks.len() {
        let (t_idx, t_track, t_en, t_def) = (idxs[i], tracks[i], enabled[i], default[i]);
        let mut j = i;
        while j > 0 && key(tracks[j - 1]) > key(t_track) {
            idxs[j] = idxs[j - 1];
            tracks[j] = tracks[j - 1];
            enabled[j] = enabled[j - 1];
            default[j] = default[j - 1];
            j -= 1;
        }
        idxs[j] = t_idx;
        tracks[j] = t_track;
        enabled[j] = t_en;
        default[j] = t_def;
    }
}

/// `micsJson` + `speakersJson` com a `track` já calculada pela função central de
/// atribuição — a UI não reimplementa o agrupamento. Pode bloquear na primeira
/// vez (enumeração WASAPI): chamar com o cache já populado ou em worker.
fn build_audio_json_with_tracks() -> (Value, Value) {
    wam::init_audio();
    let devs = wam::enumerate_audio_devices();

    // Default primeiro (topo da UI), depois o resto na ordem original.
    let partition = |kind: DeviceKind| -> Vec<usize> {
        let of_kind = |i: &usize| devs[*i].kind == kind;
        let mut v: Vec<usize> = (0..devs.len()).filter(of_kind).filter(|i| devs[*i].is_default).collect();
        v.extend((0..devs.len()).filter(of_kind).filter(|i| !devs[*i].is_default));
        v
    };
    let mut mic_idxs = partition(DeviceKind::Input);
    let mut spk_idxs = partition(DeviceKind::Output);

    // "and not hidden": o engine grava via get_source_active, então o cálculo
    // de faixas aqui tem que enxergar o mesmo.
    let flags = |idxs: &[usize], id_of: fn(&str) -> String| -> (Vec<bool>, Vec<bool>) {
        let en = idxs
            .iter()
            .map(|i| {
                let id = id_of(&devs[*i].name);
                get_source_enabled(&id, true) && !get_source_hidden_by_id(&id)
            })
            .collect();
        let df = idxs.iter().map(|i| devs[*i].is_default).collect();
        (en, df)
    };
    let (mut mic_en, mut mic_def) = flags(&mic_idxs, mic_id_from_name);
    let (mut spk_en, mut spk_def) = flags(&spk_idxs, out_id_from_name);

    let asg = compute_audio_track_assignments(&mic_en, &mic_def, &spk_en, &spk_def, 0);
    let (mut mic_tracks, mut spk_tracks) = (asg.mic_tracks, asg.out_tracks);
    sort_audio_parallel_by_track(&mut mic_idxs, &mut mic_tracks, &mut mic_en, &mut mic_def);
    sort_audio_parallel_by_track(&mut spk_idxs, &mut spk_tracks, &mut spk_en, &mut spk_def);

    let item = |d: &AudioDeviceInfo, id: String, enabled: bool, is_default: bool, track: i32| {
        json!({
            "id": id, "name": d.name, "info": "",
            "enabled": enabled,
            "hidden": get_source_hidden_by_id(&id),
            "auto": get_source_auto_record_by_id(&id),
            "isDefault": is_default,
            "isBluetooth": d.is_bluetooth,
            "track": track,
        })
    };
    let mics = mic_idxs
        .iter()
        .enumerate()
        .map(|(k, i)| item(&devs[*i], mic_id_from_name(&devs[*i].name), mic_en[k], mic_def[k], mic_tracks[k]))
        .collect();
    let spks = spk_idxs
        .iter()
        .enumerate()
        .map(|(k, i)| item(&devs[*i], out_id_from_name(&devs[*i].name), spk_en[k], spk_def[k], spk_tracks[k]))
        .collect();
    (Value::Array(mics), Value::Array(spks))
}

/// Espaço livre no volume da pasta de gravação (-1 se não conseguiu ler).
/// `GetDiskFreeSpaceExW` devolve o disponível pro USUÁRIO (respeita cotas).
pub fn record_dir_free_bytes() -> i64 {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let dir = record_dir();
    if dir.as_os_str().is_empty() {
        return -1;
    }
    let mut probe = dir.to_string_lossy().into_owned();
    if !probe.ends_with('\\') {
        probe.push('\\');
    }
    let wide: Vec<u16> = probe.encode_utf16().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    let ok = unsafe { GetDiskFreeSpaceExW(windows::core::PCWSTR(wide.as_ptr()), Some(&mut free), None, None) };
    if ok.is_ok() {
        free as i64
    } else {
        -1
    }
}

/// `init`: bundle i18n, monitores, (opcional) áudio, webcams, gravações e a
/// navegação. `include_audio=false` pula a enumeração WASAPI (pode demorar 30 s+
/// com audio service doente): quem chama dispara a enumeração em worker e manda
/// `audio_sources_refreshed` depois.
pub fn push_init(include_audio: bool) {
    let (mics, spks) = if include_audio {
        build_audio_json_with_tracks()
    } else {
        (json!([]), json!([]))
    };
    let dir = record_dir().to_string_lossy().into_owned();
    let mut init = json!({
        "type": "init",
        "i18n": lang::current_bundle(),
        "language": lang::current_language(),
        "monitors": build_monitors_json(),
        "mics": mics,
        "speakers": spks,
        "webcams": build_webcams_json_from(&win_webcam::enumerate_webcams()),
        "recordDir": dir,
        "freeBytes": record_dir_free_bytes(),
        "languagePref": config::get_config_str("language", ""),
        "languages": lang::available_languages(),
    });
    // A biblioteca: gravações da pasta navegada + pastas + caminho.
    for (k, v) in super::library::init_listing() {
        init[k] = v;
    }
    post_json(init);
}

// ---------------------------------------------------------------------------
// Diff do que mudou (toast "Dispositivos atualizados")
// ---------------------------------------------------------------------------

fn audio_signature(devs: &[AudioDeviceInfo]) -> String {
    if devs.is_empty() {
        return String::new();
    }
    // Ordena por (Kind, DeviceId) para a ordem de enumeração não afetar a comparação.
    let mut tmp: Vec<String> = devs
        .iter()
        .map(|d| format!("{}|{}|{}|{}", d.kind as i32, d.device_id, d.is_default as i32, d.name))
        .collect();
    tmp.sort();
    tmp.join(";")
}

fn audio_changes(old: &[AudioDeviceInfo], new: &[AudioDeviceInfo]) -> Value {
    let kind_word = |k: DeviceKind| if k == DeviceKind::Input { "Microfone" } else { "Alto-falante" };
    let find = |arr: &[AudioDeviceInfo], id: &str, k: DeviceKind| {
        arr.iter().position(|d| d.kind == k && d.device_id.eq_ignore_ascii_case(id))
    };
    let default_of = |arr: &[AudioDeviceInfo], k: DeviceKind| {
        arr.iter().find(|d| d.kind == k && d.is_default).map(|d| d.device_id.clone())
    };
    let mut out: Vec<String> = Vec::new();
    for d in old {
        if find(new, &d.device_id, d.kind).is_none() {
            out.push(format!("{} removido: {}", kind_word(d.kind), d.name));
        }
    }
    for d in new {
        if find(old, &d.device_id, d.kind).is_none() {
            out.push(format!("{} adicionado: {}", kind_word(d.kind), d.name));
        }
    }
    // Default trocado — só se o novo default já existia (os recém-adicionados já
    // foram reportados acima).
    for k in [DeviceKind::Input, DeviceKind::Output] {
        let (Some(o), Some(n)) = (default_of(old, k), default_of(new, k)) else { continue };
        if o.eq_ignore_ascii_case(&n) || find(old, &n, k).is_none() {
            continue;
        }
        let name = find(new, &n, k).map(|i| new[i].name.clone()).unwrap_or_default();
        out.push(format!("Padrão de {}: {}", kind_word(k).to_lowercase(), name));
    }
    json!(out)
}

fn monitor_changes(old: &[MonitorInfo], new: &[MonitorInfo]) -> Value {
    let name = |m: &MonitorInfo| if m.friendly_name.is_empty() { m.device_name.clone() } else { m.friendly_name.clone() };
    let has = |arr: &[MonitorInfo], m: &MonitorInfo| arr.iter().any(|x| x.device_name.eq_ignore_ascii_case(&m.device_name));
    let mut out: Vec<String> = Vec::new();
    out.extend(old.iter().filter(|m| !has(new, m)).map(|m| format!("Monitor removido: {}", name(m))));
    out.extend(new.iter().filter(|m| !has(old, m)).map(|m| format!("Monitor adicionado: {}", name(m))));
    json!(out)
}

fn webcam_changes(old: &[Webcam], new: &[Webcam]) -> Value {
    let has = |arr: &[Webcam], c: &Webcam| arr.iter().any(|x| x.device_id.eq_ignore_ascii_case(&c.device_id));
    let mut out: Vec<String> = Vec::new();
    out.extend(old.iter().filter(|c| !has(new, c)).map(|c| format!("Webcam removida: {}", c.name)));
    out.extend(new.iter().filter(|c| !has(old, c)).map(|c| format!("Webcam adicionada: {}", c.name)));
    json!(out)
}

// ---------------------------------------------------------------------------
// Refresh (hot-plug)
// ---------------------------------------------------------------------------

fn push_monitor_changed(pending: bool) {
    post_json(json!({ "type": "monitor_changed", "pending": pending }));
}

fn push_audio_device_changed(pending: bool) {
    post_json(json!({ "type": "audio_device_changed", "pending": pending }));
}

pub fn do_refresh_monitors() {
    if with_dev(|d| std::mem::replace(&mut d.monitor_refresh_in_progress, true)) {
        log!("DoRefreshMonitors: outro refresh de monitor em andamento, ignorando.");
        return;
    }
    push_refresh_busy(true, "monitors");
    let mons = win_preview::enumerate_monitors();
    let new_count = mons.len() as i32;
    let changes = with_dev(|d| monitor_changes(&d.last_applied_monitors, &mons));
    post_json(json!({ "type": "monitors_refreshed", "monitors": build_monitors_json(), "changes": changes }));
    with_dev(|d| d.last_applied_monitors = mons);

    // Contagem inalterada logo após o evento: o Windows às vezes ainda não
    // atualizou o inventário — tenta de novo até 2x.
    let retry = with_dev(|d| {
        if new_count == d.last_monitor_count && d.monitor_retry_attempts < 2 {
            d.monitor_retry_attempts += 1;
            log!("MonitorRefresh: contagem inalterada ({new_count}), retry {} em 2s", d.monitor_retry_attempts);
            true
        } else {
            d.last_monitor_count = new_count;
            d.monitor_retry_attempts = 0;
            false
        }
    });
    if retry {
        ui::set_timer(TIMER_MONITOR_REFRESH, 2000);
    }
    with_dev(|d| d.monitor_refresh_in_progress = false);
    push_refresh_busy(false, "monitors");
}

pub fn do_refresh_webcams() {
    let cams = win_webcam::enumerate_webcams();
    let changes = with_dev(|d| webcam_changes(&d.last_applied_webcams, &cams));
    post_json(json!({ "type": "webcams_refreshed", "webcams": build_webcams_json_from(&cams), "changes": changes }));
    with_dev(|d| d.last_applied_webcams = cams);
    log!("DoRefreshWebcams: pushed.");
}

fn log_device_snapshot(tag: &str, devs: &[AudioDeviceInfo]) {
    let n_mics = devs.iter().filter(|d| d.kind == DeviceKind::Input).count();
    log!("{tag} snapshot: {n_mics} mic(s), {} spk(s)", devs.len() - n_mics);
    let mut seen: Vec<String> = Vec::new();
    for d in devs {
        let dir = if d.kind == DeviceKind::Input { "IN " } else { "OUT" };
        log!("   {dir} \"{}\"{}", d.name, if d.is_default { " [default]" } else { "" });
        let key = format!("{}:{}", d.kind as i32, d.name.to_lowercase());
        if seen.contains(&key) {
            log!("   ^ AVISO: nome duplicado neste kind — UI vai mostrar 2 cards iguais.");
        } else {
            seen.push(key);
        }
    }
}

/// Re-enumera o áudio via WASAPI e empurra a lista. A enumeração roda em worker
/// (pode bloquear 60 s+ com o audio service doente — pegadinha #30); só o
/// `build_audio_json_with_tracks`, que lê do cache já populado, volta pra main.
pub fn do_refresh_audio() {
    if with_dev(|d| std::mem::replace(&mut d.audio_refresh_in_progress, true)) {
        log!("DoRefreshAudio: outro refresh de audio em andamento, ignorando.");
        return;
    }
    push_refresh_busy(true, "audio");
    log!("DoRefreshAudio: disparando worker.");
    std::thread::spawn(|| {
        let t0 = std::time::Instant::now();
        if !is_shutting_down() {
            wam::init_audio(); // CoInitializeEx no worker: o IMMDeviceEnumerator precisa
            // Invalida o cache: a próxima enumeração pega devices novos e descarta os removidos.
            wam::refresh_audio_devices();
            let t = std::time::Instant::now();
            // O ponto que costuma travar quando o audio service está doente.
            let devs = wam::enumerate_audio_devices();
            log!("DoRefreshAudio: EnumerateAudioDevices em {}ms ({} device(s)).", t.elapsed().as_millis(), devs.len());
            log_device_snapshot("DoRefreshAudio", &devs);
            let sig = audio_signature(&devs);
            if !is_shutting_down() {
                mainthread::queue(move || apply_audio_refresh(devs, sig));
            }
        }
        log!("DoRefreshAudio: worker terminou em {}ms.", t0.elapsed().as_millis());
        mainthread::queue(|| {
            with_dev(|d| d.audio_refresh_in_progress = false);
            push_refresh_busy(false, "audio");
        });
    });
}

fn apply_audio_refresh(devs: Vec<AudioDeviceInfo>, sig: String) {
    if is_shutting_down() {
        return;
    }
    // Dedup: lista (incluindo defaults) idêntica ao que a UI já conhece — o
    // evento que disparou o refresh não trouxe nada. FECHA o banner e limpa o
    // flag (incondicional: o stop zera `pending` ANTES de chamar o refresh, então
    // um `if pending` nunca fecharia o banner no pós-stop).
    if sig == with_dev(|d| d.last_applied_audio_sig.clone()) {
        log!("DoRefreshAudio: signature inalterada, sem push pra UI.");
        with_dev(|d| d.pending_audio_refresh = false);
        push_audio_device_changed(false);
        return;
    }
    // Gravando, a lista não muda na UI (confunde): só o banner "dispositivos
    // alterados"; a aplicação real fica pro pós-stop.
    if recording_active() {
        if !with_dev(|d| std::mem::replace(&mut d.pending_audio_refresh, true)) {
            push_audio_device_changed(true);
        }
        return;
    }
    let (mics, spks) = build_audio_json_with_tracks();
    let changes = with_dev(|d| audio_changes(&d.last_applied_audio_devs, &devs));
    post_json(json!({ "type": "audio_sources_refreshed", "mics": mics, "speakers": spks, "changes": changes }));
    with_dev(|d| {
        d.last_applied_audio_sig = sig;
        d.last_applied_audio_devs = devs;
        d.pending_audio_refresh = false;
    });
}

/// `WM_DISPLAYCHANGE`: debounce — cada chamada reagenda o timer.
pub fn on_display_change() {
    if recording_active() {
        if !with_dev(|d| std::mem::replace(&mut d.pending_monitor_refresh, true)) {
            push_monitor_changed(true);
        }
        return;
    }
    ui::kill_timer(TIMER_MONITOR_REFRESH);
    ui::set_timer(TIMER_MONITOR_REFRESH, MONITOR_REFRESH_DEBOUNCE_MS);
}

/// `WM_DEVICECHANGE` (DBT_DEVNODES_CHANGED): qualquer mudança de hardware. O
/// DirectShow não tem callback nativo, então é por aqui que a webcam é achada.
pub fn on_device_node_change() {
    log!("WM_DEVICECHANGE recebido (DBT_DEVNODES_CHANGED).");
    if recording_active() {
        // Marca pendente e aplica após o stop: monitor/webcam ficam bloqueados
        // durante a gravação, não há como refazer no meio.
        with_dev(|d| d.pending_webcam_refresh = true);
        return;
    }
    ui::kill_timer(TIMER_WEBCAM_REFRESH);
    ui::set_timer(TIMER_WEBCAM_REFRESH, WEBCAM_REFRESH_DEBOUNCE_MS);
}

/// Callback do `audio_watch` (thread do WASAPI): só reagenda o debounce, na main.
/// A decisão "banner × refresh × nada" é do `apply_audio_refresh`, depois que o
/// worker enumera e compara a assinatura — assim nada é notificado à toa.
fn on_audio_device_change(kind: DeviceChangeKind, device_id: &str) {
    let short: String = if device_id.chars().count() > 40 {
        device_id.chars().take(37).collect::<String>() + "..."
    } else {
        device_id.to_owned()
    };
    let friendly = wam::resolve_device_name(device_id);
    if friendly.is_empty() {
        log!("AudioWatch: {} id=\"{short}\"", kind.name());
    } else {
        log!("AudioWatch: {} \"{friendly}\" (id=\"{short}\")", kind.name());
    }
    mainthread::queue(|| {
        ui::kill_timer(TIMER_AUDIO_REFRESH);
        ui::set_timer(TIMER_AUDIO_REFRESH, AUDIO_REFRESH_DEBOUNCE_MS);
    });
}

/// Aplica o que ficou pendente durante a gravação (chamado no stop).
pub fn apply_pending_refreshes() {
    if with_dev(|d| std::mem::take(&mut d.pending_audio_refresh)) {
        do_refresh_audio();
    }
    if with_dev(|d| std::mem::take(&mut d.pending_monitor_refresh)) {
        do_refresh_monitors();
    }
    if with_dev(|d| std::mem::take(&mut d.pending_webcam_refresh)) {
        do_refresh_webcams();
    }
}

// ---------------------------------------------------------------------------
// Medidores e mudo do microfone
// ---------------------------------------------------------------------------

/// Pico por device via `IAudioMeterInformation`: funciona sem o libobs.
fn push_audio_meters_from_win() {
    wam::init_audio();
    let levels = wam::read_peak_levels();
    if levels.is_empty() {
        return;
    }
    let devs = wam::enumerate_audio_devices();
    // Mudo do ENDPOINT, pra UI pintar a barra do mic de vermelho. Num endpoint
    // mudo o pico vem ZERO e não há como recuperar o nível (pegadinha #59): a
    // UI desenha a barra CHEIA de vermelho — não há intensidade pra mostrar.
    let mutes = wam::read_input_mutes();

    let items: Vec<Value> = levels
        .iter()
        .filter_map(|l| {
            let d = devs.iter().find(|d| d.device_id.eq_ignore_ascii_case(&l.device_id))?;
            let is_input = d.kind == DeviceKind::Input;
            let id = if is_input { mic_id_from_name(&d.name) } else { out_id_from_name(&d.name) };
            let mut item = json!({
                "id": id,
                // level = pico total (compatibilidade); L/R individuais → duas barras se canais >= 2.
                "level": l.peak, "left": l.left, "right": l.right, "channels": l.channels,
            });
            // Só pra entrada: "está mudo?" não faz sentido pra saída.
            if is_input {
                let muted = mutes.iter().find(|m| m.device_id.eq_ignore_ascii_case(&l.device_id)).map(|m| m.muted);
                item["muted"] = json!(muted.unwrap_or(false));
            }
            Some(item)
        })
        .collect();
    if !items.is_empty() {
        post_json(json!({ "type": "audio_meters", "items": items }));
    }
}

/// Espelha na gravação o mudo do ENDPOINT do microfone no Windows (botão do
/// headset, mudo do sistema, apps de chamada que propagam). A regra COMBINA as
/// duas razões de uma faixa estar muda: `mudo = desmarcado na tela OU endpoint
/// mudo`. O termo do endpoint sai da conta com a opção desligada, para desligá-la
/// no meio DESFAZER o mudo que ela aplicou (pegadinha #59).
pub fn sync_mic_mute_from_endpoints() {
    // O buffer em memória grava as mesmas fontes: o mudo vale pra ele também.
    if !(recording_active() || engine::with_engine(|e| e.is_replay_active()).unwrap_or(false)) {
        return;
    }
    if !engine::exists() {
        return;
    }
    let mutes = wam::read_input_mutes();
    if mutes.is_empty() {
        return;
    }
    let devs = wam::enumerate_audio_devices();
    let use_endpoint = config::get_config_bool("muteWhenDeviceMuted", true);
    for m in &mutes {
        let Some(dev) = devs.iter().find(|d| d.kind == DeviceKind::Input && d.device_id.eq_ignore_ascii_case(&m.device_id))
        else {
            continue;
        };
        let src_id = mic_id_from_name(&dev.name);
        let eff = !config::get_source_active("mics", &dev.name, true) || (use_endpoint && m.muted);
        // Só chama o libobs quando MUDA: este tique roda 10x por segundo.
        let changed = with_dev(|d| d.mic_mute_applied.insert(src_id.clone(), eff) != Some(eff));
        if !changed {
            continue;
        }
        engine::with_engine(|e| e.set_source_muted(&src_id, eff));
        log!("MicMute: \"{}\" -> {eff} (endpoint={})", dev.name, m.muted);
    }
}

/// Sessão nova = fontes novas: sem zerar, o 1º tique acharia que nada mudou e a
/// faixa começaria com o mudo errado.
pub fn clear_mic_mute_applied() {
    with_dev(|d| d.mic_mute_applied.clear());
}

/// Guarda os monitores no início da gravação (slots de prévia fixos).
pub fn snapshot_monitors_for_recording() {
    let mons = win_preview::enumerate_monitors();
    with_dev(|d| d.recording_monitors_snapshot = mons);
}

// ---------------------------------------------------------------------------
// Prévias dos monitores
// ---------------------------------------------------------------------------

static THUMB_BUSY: AtomicBool = AtomicBool::new(false);

/// Por que a captura deve parar (para o log de transição); vazio = capturar.
/// A prévia lê a tela INTEIRA por GDI, o que obriga o DWM a copiar a superfície
/// da GPU pra memória — com um jogo rodando vira engasgo a cada tique. Só
/// capturamos quando alguém pode estar olhando (pegadinha #22).
/// Inclui app em tela cheia na frente: o jogo cobre o NoOBS sem que ele esteja
/// minimizado, e a visibilidade sozinha não pegaria (mesma regra e mesma chave
/// `fullscreenDetect` do indicador).
fn thumb_pause_reason() -> &'static str {
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};
    if player_open_atomic() {
        return "player aberto";
    }
    let hwnd = ui::main_window();
    if !hwnd.0.is_null() {
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return "janela escondida";
            }
            if IsIconic(hwnd).as_bool() {
                return "janela minimizada";
            }
        }
    }
    if crate::rec_indicator::fullscreen_app_active() {
        return "app em tela cheia";
    }
    ""
}

/// Espelho atômico do `player_open` para a thread de prévias (o estado completo
/// vive na `thread_local` da main).
static PLAYER_OPEN: AtomicBool = AtomicBool::new(false);
fn player_open_atomic() -> bool {
    PLAYER_OPEN.load(Ordering::Relaxed)
}

pub fn set_player_open_all(open: bool) {
    PLAYER_OPEN.store(open, Ordering::Relaxed);
    set_player_open(open);
}

/// Thread própria: o `WM_TIMER` é suprimido pelo loop modal de arrastar/
/// redimensionar a janela (pegadinha #21), e a captura precisa continuar.
fn start_thumb_thread() {
    if with_dev(|d| std::mem::replace(&mut d.thumb_thread_started, true)) {
        return;
    }
    std::thread::spawn(|| {
        let mut last_reason = String::new();
        while !is_shutting_down() {
            // Captura PRIMEIRO, dorme DEPOIS: a 1ª prévia aparece quase na hora.
            let reason = thumb_pause_reason();
            if reason != last_reason {
                if reason.is_empty() {
                    log!("ThumbThread: RETOMADO (estava: {last_reason})");
                } else {
                    log!("ThumbThread: SUSPENSO ({reason})");
                }
                last_reason = reason.to_owned();
            }
            if reason.is_empty() {
                push_monitor_thumbs();
            }
            // Dorme em pedaços de 100 ms para terminar rápido no shutdown.
            let mut slept = 0;
            while slept < THUMB_TICK_MS && !is_shutting_down() {
                let step = (THUMB_TICK_MS - slept).min(100);
                std::thread::sleep(Duration::from_millis(step));
                slept += step;
            }
        }
    });
}

/// Captura cada monitor e empurra a prévia. Em gravação itera o SNAPSHOT do
/// início (slots fixos) e casa cada slot ao monitor atual por `device_name`; se o
/// monitor sumiu, manda prévia vazia (a UI mostra preto, não a última imagem).
/// O JPEG fica no servidor HTTP local e só a URL vai pelo IPC (sem base64).
fn push_monitor_thumbs() {
    if is_shutting_down() || THUMB_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let current = win_preview::enumerate_monitors();
    let snapshot = DEV_SNAPSHOT.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let recording = RECORDING_FLAG.load(Ordering::Relaxed) && !snapshot.is_empty();
    let work: Vec<MonitorInfo> = if recording { snapshot } else { current.clone() };

    let mut items = Vec::new();
    for slot in &work {
        if is_shutting_down() {
            THUMB_BUSY.store(false, Ordering::SeqCst);
            return;
        }
        let id = monitor_id_from_index(slot.index);
        let thumb = match current.iter().find(|c| c.device_name.eq_ignore_ascii_case(&slot.device_name)) {
            Some(m) => {
                // Monitor ainda existe: captura com as coordenadas atuais.
                let jpeg = win_preview::capture_monitor_as_jpeg(m, 480, 270);
                if !jpeg.is_empty() {
                    player::set_monitor_thumb(&id, jpeg);
                }
                player::get_monitor_thumb_url(&id)
            }
            None => String::new(), // monitor sumiu: a UI limpa a imagem
        };
        items.push(json!({ "id": id, "thumb": thumb }));
    }
    mainthread::queue(move || {
        if !items.is_empty() {
            post_json(json!({ "type": "monitor_thumbs", "items": items }));
        }
        THUMB_BUSY.store(false, Ordering::SeqCst);
    });
}

/// A thread de prévias não enxerga a `thread_local` da main: o snapshot e a flag
/// de gravação têm espelhos compartilhados.
static DEV_SNAPSHOT: std::sync::Mutex<Vec<MonitorInfo>> = std::sync::Mutex::new(Vec::new());
static RECORDING_FLAG: AtomicBool = AtomicBool::new(false);

/// Chamado no start/stop da gravação para a thread de prévias usar o snapshot.
pub fn set_recording_for_thumbs(recording: bool) {
    if recording {
        snapshot_monitors_for_recording();
        let snap = with_dev(|d| d.recording_monitors_snapshot.clone());
        *DEV_SNAPSHOT.lock().unwrap_or_else(|p| p.into_inner()) = snap;
    }
    RECORDING_FLAG.store(recording, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Mensagens da UI: ligar/ocultar/perfil automático de um dispositivo
// ---------------------------------------------------------------------------

fn is_monitor_or_webcam(id: &str) -> bool {
    let l = id.to_lowercase();
    l.starts_with(&PFX_MONITOR.to_lowercase()) || l.starts_with(&PFX_WEBCAM.to_lowercase())
}

fn is_audio_source(id: &str) -> bool {
    let l = id.to_lowercase();
    l.starts_with(&PFX_MIC.to_lowercase()) || l.starts_with(&PFX_OUT.to_lowercase())
}

fn is_mic(id: &str) -> bool {
    id.to_lowercase().starts_with(&PFX_MIC.to_lowercase())
}

/// Re-empurra as listas de áudio (silencioso, sem toast): ligar/ocultar mexe no
/// agrupamento de faixas e a UI recalcula cores e legenda. A conta só existe aqui.
fn push_audio_sources_silent() {
    let (mics, spks) = build_audio_json_with_tracks();
    post_json(json!({ "type": "audio_sources_refreshed", "silent": true, "mics": mics, "speakers": spks }));
}

pub fn handle_toggle_source(id: &str, enabled: bool) {
    let monitor = is_monitor_or_webcam(id);
    let audio = is_audio_source(id);

    // Monitor/webcam mudam o canvas: bloqueado durante a gravação.
    if recording_active() && monitor {
        super::post_error(lang::t("error.cantChangeSourcesWhileRecording"));
        return;
    }
    if let Some((cat, raw)) = split_source_id(id) {
        config::set_source_bool(cat, &raw, enabled);
    }
    // A cena do buffer era das fontes antigas: recomeça.
    if monitor {
        super::replay::restart_for_source_change();
    }

    // Mic desmarcado não pode continuar segurado pelo medidor — senão o Windows
    // segue mostrando o dispositivo "em uso".
    if is_mic(id) {
        wam::apply_mic_open_policy();
    }
    let replay = engine::with_engine(|e| e.is_replay_active()).unwrap_or(false);
    if (recording_active() || replay) && audio {
        engine::with_engine(|e| e.set_source_muted(id, !enabled));
    }
    // Reaplica a regra combinada na hora: sem isto, reabilitar um mic cujo
    // ENDPOINT está mudo o deixaria audível até o próximo tique de 100 ms.
    with_dev(|d| d.mic_mute_applied.remove(id));
    sync_mic_mute_from_endpoints();

    if audio {
        push_audio_sources_silent();
    }
}

/// Oculta/mostra um dispositivo (modal "Dispositivos"). Oculto some da tela
/// inicial E não entra na gravação (o engine lê via `get_source_active`). NÃO
/// mexe no `enabled`: desocultar devolve o device com a preferência de antes.
pub fn handle_set_source_hidden(id: &str, hidden: bool) {
    let monitor = is_monitor_or_webcam(id);
    let audio = is_audio_source(id);
    if recording_active() && monitor {
        super::post_error(lang::t("error.cantChangeSourcesWhileRecording"));
        return;
    }
    if let Some((cat, raw)) = split_source_id(id) {
        config::set_source_hidden(cat, &raw, hidden);
    }
    log!("SetSourceHidden: \"{id}\" -> {hidden}");
    // A cena do buffer era das fontes antigas: recomeça.
    if monitor {
        super::replay::restart_for_source_change();
    }
    if is_mic(id) {
        wam::apply_mic_open_policy();
    }
    // Gravando + áudio: ocultar equivale a desativar, então muta na hora.
    let replay = engine::with_engine(|e| e.is_replay_active()).unwrap_or(false);
    if (recording_active() || replay) && audio {
        engine::with_engine(|e| e.set_source_muted(id, hidden));
    }
    if audio {
        push_audio_sources_silent();
    }
}

/// Perfil da gravação automática (aba Comportamento). Só escreve o config: o
/// perfil é lido ao montar a gravação, então não afeta uma já em andamento.
pub fn handle_set_auto_source(id: &str, enabled: bool) {
    if let Some((cat, raw)) = split_source_id(id) {
        config::set_source_auto_record(cat, &raw, enabled);
    }
    log!("SetAutoSource: \"{id}\" -> {enabled}");
}

// ---------------------------------------------------------------------------
// Init / timers
// ---------------------------------------------------------------------------

/// `DoInit` da parte de dispositivos. O libobs só sobe no warmup/ao gravar: a UI
/// funciona inteira por Win32 (monitores) e WASAPI (medidores). A enumeração
/// WASAPI roda em worker (pode levar 30 s+ em máquinas sem mic).
pub fn do_init() {
    // ANTES de qualquer enumeração WASAPI: senão o 1º rebuild já abriria sessão
    // de captura em todo mic, inclusive nos desmarcados.
    wam::set_mic_open_policy(mic_meter_open_policy);
    if with_dev(|d| d.initialized) {
        push_init(true);
        return;
    }
    if with_dev(|d| std::mem::replace(&mut d.init_started, true)) {
        // Segundo `ready` com o init ainda em andamento (reload da página): só
        // re-empurra o básico, sem timers/watchers duplicados.
        log!("DoInit: ja em andamento — re-push leve, sem re-init.");
        push_init(false);
        return;
    }
    log!("DoInit: inicio");

    // Push imediato com áudio vazio; a enumeração WASAPI vem em worker.
    push_init(false);
    // Snapshots iniciais: sem isso o 1º hot-plug compararia contra lista vazia e
    // reportaria "todos adicionados".
    with_dev(|d| {
        d.last_applied_monitors = win_preview::enumerate_monitors();
        d.last_applied_webcams = win_webcam::enumerate_webcams();
    });
    log!("DoInit: UI pushed");

    std::thread::spawn(|| {
        if is_shutting_down() {
            return;
        }
        wam::init_audio();
        let devs = wam::enumerate_audio_devices();
        log_device_snapshot("DoInit", &devs);
        let sig = audio_signature(&devs);
        if is_shutting_down() {
            return;
        }
        mainthread::queue(move || {
            if is_shutting_down() {
                return;
            }
            let (mics, spks) = build_audio_json_with_tracks();
            // silent=true: carga inicial, não um hot-plug do usuário (sem toast).
            post_json(json!({ "type": "audio_sources_refreshed", "silent": true, "mics": mics, "speakers": spks }));
            // Assinatura + lista iniciais para dedup e diff dos eventos futuros.
            with_dev(|d| {
                d.last_applied_audio_sig = sig;
                d.last_applied_audio_devs = devs;
            });
            log!("DoInit: audio enumeration completa");
        });
    });

    start_thumb_thread();
    // Medidores continuam via WM_TIMER (ok pausar durante o arrasto da janela).
    ui::set_timer(TIMER_AUDIO_METER, AUDIO_METER_MS);
    // Hot-plug de áudio (continua funcionando sem o libobs).
    audio_watch::start(Arc::new(on_audio_device_change));
    with_dev(|d| d.initialized = true);
}

/// Timers de dispositivos; `false` = não é meu.
pub fn on_timer(id: usize) -> bool {
    match id {
        TIMER_AUDIO_METER => {
            // Suspende os medidores com o player aberto: a sidebar está atrás do
            // modal e atualizá-la só gera reflow inútil.
            if !player_open() {
                push_audio_meters_from_win();
            }
            // Independe do player: é a GRAVAÇÃO que acompanha o mudo do mic.
            sync_mic_mute_from_endpoints();
        }
        TIMER_AUDIO_REFRESH => {
            if with_dev(|d| d.audio_refresh_in_progress) {
                return true; // continua agendado, tenta no próximo tique
            }
            ui::kill_timer(TIMER_AUDIO_REFRESH);
            log!("TIMER_AUDIO_REFRESH: debounce fired.");
            // SEM guard de gravação: o refresh já trata isso (banner + adiado).
            do_refresh_audio();
        }
        TIMER_MONITOR_REFRESH => {
            if with_dev(|d| d.monitor_refresh_in_progress) {
                return true;
            }
            ui::kill_timer(TIMER_MONITOR_REFRESH);
            if !recording_active() {
                do_refresh_monitors();
            }
        }
        TIMER_WEBCAM_REFRESH => {
            ui::kill_timer(TIMER_WEBCAM_REFRESH);
            log!("TIMER_WEBCAM_REFRESH: debounce fired.");
            if !recording_active() {
                do_refresh_webcams();
            }
        }
        _ => return false,
    }
    true
}

/// Encerra o que este módulo mantém vivo.
pub fn shutdown() {
    for id in [TIMER_AUDIO_REFRESH, TIMER_MONITOR_REFRESH, TIMER_AUDIO_METER, TIMER_WEBCAM_REFRESH] {
        ui::kill_timer(id);
    }
    audio_watch::stop();
    wam::done_audio();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(name: &str, kind: DeviceKind, def: bool) -> AudioDeviceInfo {
        AudioDeviceInfo { device_id: name.to_owned(), name: name.to_owned(), kind, is_default: def, is_bluetooth: false }
    }

    #[test]
    fn ids_de_fonte() {
        assert_eq!(split_source_id("NoOBS Monitor 1"), Some(("monitors", "1".to_owned())));
        assert_eq!(split_source_id("noobs mic - Fone USB"), Some(("mics", "Fone USB".to_owned())));
        assert_eq!(split_source_id("NoOBS Out - Caixa"), Some(("speakers", "Caixa".to_owned())));
        assert_eq!(split_source_id("NoOBS Webcam - C920"), Some(("webcams", "C920".to_owned())));
        assert_eq!(split_source_id("outra coisa"), None);
    }

    #[test]
    fn ordena_por_faixa_zero_no_fim() {
        let mut idxs = vec![0, 1, 2, 3];
        let mut tracks = vec![0, 4, 2, 3];
        let mut en = vec![false, true, true, true];
        let mut df = vec![false; 4];
        sort_audio_parallel_by_track(&mut idxs, &mut tracks, &mut en, &mut df);
        assert_eq!(tracks, vec![2, 3, 4, 0]);
        assert_eq!(idxs, vec![2, 3, 1, 0]);
    }

    #[test]
    fn mudanca_de_audio() {
        let old = vec![dev("a", DeviceKind::Input, true)];
        let new = vec![dev("a", DeviceKind::Input, true), dev("b", DeviceKind::Output, false)];
        assert_eq!(audio_changes(&old, &new), json!(["Alto-falante adicionado: b"]));
        assert_eq!(audio_changes(&new, &old), json!(["Alto-falante removido: b"]));
    }

    #[test]
    fn assinatura_independe_da_ordem() {
        let a = vec![dev("x", DeviceKind::Input, true), dev("y", DeviceKind::Output, false)];
        let mut b = a.clone();
        b.reverse();
        assert_eq!(audio_signature(&a), audio_signature(&b));
        assert_eq!(audio_signature(&[]), "");
    }
}

//! Core → interface, por chamada tipada (docs/ui-slint.md). Substitui o
//! `post_json` + `Bridge.handlers[type]` da versão web: cada evento que a
//! página recebia vira um método do [`View`].
//!
//! Quem implementa:
//! * na janela, `app_view::SlintView` (atualiza propriedades e modelos);
//! * nos testes (`cargo test` e o executável `tests/mensagens.rs`), o
//!   [`Recorder`], que guarda cada chamada no MESMO formato das mensagens
//!   antigas — assim as conferências por `type` dos testes continuam valendo
//!   enquanto as telas são portadas.
//!
//! Regra do projeto: interface só na main thread. [`call`] pode ser chamado
//! de qualquer thread: na janela, a chamada vai pela fila da main.

use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

/// Um dispositivo de captura (monitor, webcam, microfone, saída). Os campos e
/// nomes são os do JSON da versão web — o `Recorder` dos testes serializa
/// direto.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    /// Linha de apoio (`1920x1080 @ 0,0`); vazia no áudio.
    pub info: String,
    /// Marcado pra gravar.
    pub enabled: bool,
    /// Oculto na aba Dispositivos (some da tela inicial e não grava).
    pub hidden: bool,
    /// Entra na gravação automática por microfone.
    pub auto: bool,
    /// Áudio: dispositivo padrão do Windows.
    pub is_default: bool,
    /// Áudio: Bluetooth (aviso de qualidade HFP).
    pub is_bluetooth: bool,
    /// Áudio: faixa do arquivo (0 = sem faixa, 2+ = isolada). A conta é do
    /// `audio_tracks` (fonte única).
    pub track: i32,
    /// Monitor: posição na área de trabalho; webcam: só o tamanho.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Nível de um dispositivo de áudio (pico 0..1, linear).
#[derive(Clone, Debug, Default, Serialize)]
pub struct AudioMeter {
    pub id: String,
    pub left: f32,
    pub right: f32,
    pub channels: u32,
    /// Microfone mudo no Windows (pegadinha #59). Sempre `false` na saída.
    pub muted: bool,
}

/// Prévia de um monitor: RGB 8 bits, de cima pra baixo. `rgb` vazio = o
/// monitor sumiu (a interface limpa a imagem).
#[derive(Clone, Debug, Default)]
pub struct MonitorThumb {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// Estado da gravação (o `recording_state` da web).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecState {
    pub active: bool,
    /// Tempo gravado. `None` = não mudou (só reaplicar o estado).
    pub elapsed_ms: Option<u64>,
    /// Tamanho já gravado, formatado. `None` = não mudou.
    pub size_text: Option<String>,
    /// Título da janela gravada ('' = telas).
    pub window: String,
    pub paused: bool,
}

/// Buffer em memória: tudo que a linha da tela principal e a aba Buffer
/// mostram (o `replay_state` da web).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayState {
    /// Ligado nesta sessão (o botão da tela principal).
    pub enabled: bool,
    /// Ligar sozinho ao abrir o app (Configurações).
    pub auto_start: bool,
    pub into_recording: bool,
    /// Guardando agora (gravar manual pausa o buffer).
    pub active: bool,
    pub saving: bool,
    /// Há quanto tempo está guardando, no instante do envio.
    pub since_ms: u64,
    pub max_sec: i64,
    pub max_mb: i64,
    pub max_mb_limit: i64,
    pub hotkey: String,
    pub apps: String,
    pub indicator: bool,
    pub indicator_corner: String,
    pub indicator_opacity: i64,
    pub indicator_clickable: bool,
}

/// Uma janela gravável no seletor "Capturar...". O `id` só aponta pra última
/// lista guardada no core (pegadinha #66).
#[derive(Clone, Debug, Default, Serialize)]
pub struct WindowItem {
    pub id: String,
    pub title: String,
    pub exe: String,
    pub w: i32,
    pub h: i32,
}

/// Seletor aberto por um atalho de janela que achou várias janelas.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowMatches {
    pub seq: u32,
    /// O trecho de título procurado.
    pub pattern: String,
    /// O Windows tem captura de som por processo.
    pub app_audio: bool,
    /// O atalho pede o som do app.
    pub want_app_audio: bool,
    /// O atraso do atalho conta depois da escolha.
    pub delay_sec: u32,
    pub items: Vec<WindowItem>,
}

/// Uma gravação da biblioteca (um card).
#[derive(Clone, Debug, Default)]
pub struct RecItem {
    /// Caminho completo (é o id em toda a biblioteca).
    pub id: String,
    /// Nome do arquivo sem extensão.
    pub name: String,
    /// Data de modificação, local (`2026-10-23T18:53:52`): agrupa por dia.
    pub date: String,
    pub size: i64,
    pub size_text: String,
    /// Segundos (0 = ainda não medida).
    pub duration: i32,
    /// Miniatura no cache (`<hash>.jpg`); vazia = ainda não gerada.
    pub thumb: String,
    /// Selos de como foi gravada: vazios/negativos = desconhecido.
    pub codec: String,
    pub codec_hw: bool,
    pub fps: i32,
    pub quality: i32,
    /// `ok` (transcrita), `empty` (transcrita, sem fala) ou vazio.
    pub transcript: String,
}

/// Uma subpasta da pasta navegada (card de pasta).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderItem {
    pub id: String,
    pub name: String,
    /// Gravações dentro (recursivo) e quanto ocupam.
    pub count: i64,
    pub size: i64,
    pub size_text: String,
}

/// Um nível do caminho até a pasta navegada. O primeiro é a raiz.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Crumb {
    pub id: String,
    pub name: String,
    pub root: bool,
}

/// A biblioteca inteira da pasta navegada (o `recordings_loaded` da web): a
/// lista e a navegação chegam juntas, pra nunca divergirem.
#[derive(Clone, Debug, Default)]
pub struct Library {
    pub recordings: Vec<RecItem>,
    pub folders: Vec<FolderItem>,
    pub current_dir: String,
    pub parent_dir: String,
    pub at_root: bool,
    pub breadcrumb: Vec<Crumb>,
    pub record_dir: String,
    /// Espaço livre no disco da pasta de gravação (-1 = desconhecido).
    pub free_bytes: i64,
}

/// Uma faixa do arquivo (o Probe), no painel de informações do player.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamInfo {
    pub index: i64,
    /// `video`, `audio`, …
    pub kind: String,
    pub codec: String,
    pub title: String,
    pub bitrate: i64,
    pub duration: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    /// 0 = desconhecido (a linha some).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i64>,
}

/// Um monitor/webcam no canvas da gravação (a "Visualização" do painel).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutRegion {
    pub name: String,
    /// `monitor` ou `webcam`.
    pub kind: String,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoLayout {
    pub canvas_w: i64,
    pub canvas_h: i64,
    pub regions: Vec<LayoutRegion>,
}

/// O que o painel de informações mostra (é também o formato do cache
/// `videoInfo` do `<hash>.json`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoInfo {
    pub id: String,
    pub file_name: String,
    pub format: String,
    pub duration: f64,
    pub bitrate: i64,
    pub size: i64,
    pub streams: Vec<StreamInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<VideoLayout>,
}

/// Um turno da transcrição (quem falou, quando, o quê).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Turn {
    pub speaker: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A transcrição de uma gravação pro painel e a legenda. `transcribed` = o
/// arquivo existe; uma gravação sem ninguém falando vem transcrita e sem turnos.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Transcript {
    pub id: String,
    pub transcribed: bool,
    pub turns: Vec<Turn>,
    /// Nomes dados pelo usuário (`SPEAKER_00` → "Eduardo").
    pub speakers: std::collections::BTreeMap<String, String>,
}

/// Qual lista mudou durante a gravação (o banner de "atualiza ao parar").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingKind {
    Monitors,
    Audio,
}

pub trait View {
    /// Tema resolvido (`dark`) e a preferência salva (`system`/`dark`/`light`).
    fn theme(&self, dark: bool, pref: &str);
    /// Idioma trocado: os textos são lidos de novo do `lang`.
    fn language_changed(&self, language: &str, pref: &str);
    /// Aviso não-bloqueante. `warn` = amarelo (erro, pendência). Os avisos
    /// das telas entram com elas (o `Toast.show` da web era chamado do JS).
    #[allow(dead_code)]
    fn toast(&self, title: &str, msg: &str, warn: bool, ttl_ms: u32);
    /// Erro de uma operação, já traduzido.
    fn error(&self, message: &str);
    /// Abrir as Configurações (1ª execução depois de instalar).
    fn open_settings(&self);
    /// A janela foi pra bandeja/taskbar.
    fn window_hidden(&self);

    // ---- tela principal: dispositivos ----
    /// Lista de monitores. `changes` = o que mudou num hot-plug (vazio no
    /// arranque): a interface avisa.
    fn monitors(&self, list: &[Device], changes: &[String]);
    fn webcams(&self, list: &[Device], changes: &[String]);
    /// Microfones e saídas já com as faixas. `silent` = carga inicial (sem aviso).
    fn audio_sources(&self, mics: &[Device], speakers: &[Device], silent: bool, changes: &[String]);
    /// Níveis, 10×/s.
    fn audio_meters(&self, items: &[AudioMeter]);
    /// Prévias dos monitores, 2×/s (só com a janela à vista).
    fn monitor_thumbs(&self, items: &[MonitorThumb]);
    /// Uma lista mudou durante a gravação e só atualiza ao parar.
    fn device_change_pending(&self, kind: PendingKind, pending: bool);
    /// Trabalho bloqueante em curso na coluna de fontes (`what`: `monitors`,
    /// `audio`, `starting`). `busy=false` some com o aviso.
    fn refresh_busy(&self, busy: bool, what: &str);

    // ---- tela principal: gravação ----
    fn recording_state(&self, st: &RecState);
    /// Entre parar e o arquivo ficar íntegro: o botão de gravar trava.
    fn recording_finalizing(&self, active: bool);
    /// A gravação começou a parar (som de parada sem esperar o arquivo).
    fn recording_stopping(&self);

    // ---- buffer em memória ----
    fn replay_state(&self, st: &ReplayState);
    /// Trecho salvo na biblioteca (`id` = caminho).
    fn replay_saved(&self, id: &str, name: &str, duration_sec: i32);
    /// Qualidade/fps/codec/áudio mudou com o buffer ligado: ele recomeçou e o
    /// guardado foi descartado.
    fn replay_restarted(&self);

    // ---- "Capturar..." (janela / área) e contagem ----
    /// A lista de janelas da rodada `seq` (as prévias vêm depois, uma a uma).
    fn capture_windows(&self, seq: u32, items: &[WindowItem], app_audio: bool);
    /// Prévia de uma janela (RGB). Rodada velha a interface ignora.
    fn capture_window_thumb(&self, seq: u32, id: &str, width: u32, height: u32, rgb: &[u8]);
    fn window_picker_matches(&self, m: &WindowMatches);
    /// Contagem do atraso de um atalho: `sec` restantes de `total` (0 = acabou).
    fn record_countdown(&self, sec: u32, total: u32);

    // ---- biblioteca ----
    /// A lista da pasta navegada, inteira (navegar, mover, renomear pasta, o
    /// watcher da pasta).
    fn library(&self, lib: &Library);
    /// Um card novo na pasta navegada (fim de gravação, união, trecho salvo).
    fn recording_added(&self, item: &RecItem, free_bytes: i64);
    /// Miniatura e/ou duração ficaram prontas (`thumb` vazio = sem miniatura).
    fn recording_meta(&self, id: &str, duration: i32, thumb: &str);
    fn recording_renamed(&self, old_id: &str, new_id: &str, new_name: &str);
    /// Saiu da biblioteca. `free_bytes` < 0 = não mudou.
    fn recording_removed(&self, id: &str, free_bytes: i64);
    /// Pasta criada pela interface: já está na lista, entra em edição do nome.
    fn folder_created(&self, id: &str);
    /// Gravações cuja TRANSCRIÇÃO casa com a busca `query`.
    fn transcript_search(&self, query: &str, ids: &[String]);
    /// União em curso / terminou.
    fn merge_pending(&self);
    fn merge_done(&self, ok: bool);

    // ---- player ----
    /// Picos da faixa de áudio (0..1) pra forma de onda. `hi` = a de alta
    /// resolução da exportação.
    fn waveform(&self, id: &str, peaks: &[f32], hi: bool);
    /// Resposta do Probe (painel de informações).
    fn video_info(&self, info: &VideoInfo);
    fn transcript(&self, t: &Transcript);
}

/// O formato antigo de um card (`recordings_loaded`/`recording_added`).
#[cfg(any(test, feature = "app-tests"))]
fn rec_item_json(it: &RecItem) -> serde_json::Value {
    let mut v = serde_json::json!({
        "id": it.id, "name": it.name, "date": it.date, "size": it.size, "sizeText": it.size_text,
        "duration": if it.duration > 0 { serde_json::json!(it.duration) } else { serde_json::json!("") },
    });
    if !it.thumb.is_empty() {
        v["thumb"] = it.thumb.clone().into();
    }
    if !it.codec.is_empty() {
        v["codec"] = it.codec.clone().into();
        v["codecHw"] = it.codec_hw.into();
    }
    if it.fps > 0 {
        v["fps"] = it.fps.into();
    }
    if it.quality >= 0 {
        v["quality"] = it.quality.into();
    }
    if !it.transcript.is_empty() {
        v["transcript"] = it.transcript.clone().into();
    }
    v
}

thread_local! {
    static VIEW: RefCell<Option<Rc<dyn View>>> = const { RefCell::new(None) };
}

/// Registra a interface (main thread, uma vez, ao criar a janela).
pub fn set(v: Rc<dyn View>) {
    VIEW.with(|cell| *cell.borrow_mut() = Some(v));
}

/// Chama a interface. De qualquer thread; sem interface (nada registrado),
/// não faz nada.
pub fn call(f: impl FnOnce(&dyn View) + Send + 'static) {
    #[cfg(any(test, feature = "app-tests"))]
    {
        f(&Recorder);
        return;
    }
    #[allow(unreachable_code)]
    crate::mainthread::queue(move || {
        let v = VIEW.with(|cell| cell.borrow().clone());
        if let Some(v) = v {
            f(&*v);
        }
    });
}

/// Testes: cada chamada vira o JSON que a página recebia, no mesmo canal do
/// `bridge::post_json` dos testes.
#[cfg(any(test, feature = "app-tests"))]
pub struct Recorder;

#[cfg(any(test, feature = "app-tests"))]
impl View for Recorder {
    fn theme(&self, dark: bool, pref: &str) {
        let theme = if dark { "dark" } else { "light" };
        crate::bridge::post_json(serde_json::json!({ "type": "theme", "theme": theme, "pref": pref }));
    }
    fn language_changed(&self, language: &str, pref: &str) {
        crate::bridge::post_json(serde_json::json!({ "type": "language_changed", "language": language, "languagePref": pref }));
    }
    fn toast(&self, title: &str, msg: &str, warn: bool, ttl_ms: u32) {
        crate::bridge::post_json(serde_json::json!({ "type": "toast", "title": title, "message": msg, "warn": warn, "ttl": ttl_ms }));
    }
    fn error(&self, message: &str) {
        crate::bridge::post_json(serde_json::json!({ "type": "error", "message": message }));
    }
    fn open_settings(&self) {
        crate::bridge::post_json(serde_json::json!({ "type": "open_settings" }));
    }
    fn window_hidden(&self) {
        crate::bridge::post_json(serde_json::json!({ "type": "window_hidden" }));
    }
    fn monitors(&self, list: &[Device], changes: &[String]) {
        crate::bridge::post_json(serde_json::json!({ "type": "monitors_refreshed", "monitors": list, "changes": changes }));
    }
    fn webcams(&self, list: &[Device], changes: &[String]) {
        crate::bridge::post_json(serde_json::json!({ "type": "webcams_refreshed", "webcams": list, "changes": changes }));
    }
    fn audio_sources(&self, mics: &[Device], speakers: &[Device], silent: bool, changes: &[String]) {
        crate::bridge::post_json(serde_json::json!({
            "type": "audio_sources_refreshed", "mics": mics, "speakers": speakers, "silent": silent, "changes": changes,
        }));
    }
    fn audio_meters(&self, items: &[AudioMeter]) {
        crate::bridge::post_json(serde_json::json!({ "type": "audio_meters", "items": items }));
    }
    fn monitor_thumbs(&self, items: &[MonitorThumb]) {
        let items: Vec<_> = items.iter().map(|t| serde_json::json!({ "id": t.id, "thumb": !t.rgb.is_empty() })).collect();
        crate::bridge::post_json(serde_json::json!({ "type": "monitor_thumbs", "items": items }));
    }
    fn device_change_pending(&self, kind: PendingKind, pending: bool) {
        let ty = if kind == PendingKind::Monitors { "monitor_changed" } else { "audio_device_changed" };
        crate::bridge::post_json(serde_json::json!({ "type": ty, "pending": pending }));
    }
    fn refresh_busy(&self, busy: bool, what: &str) {
        crate::bridge::post_json(serde_json::json!({ "type": "refresh_busy", "busy": busy, "what": what }));
    }
    fn recording_state(&self, st: &RecState) {
        let mut v = serde_json::to_value(st).unwrap_or_default();
        v["type"] = "recording_state".into();
        // O formato antigo: `elapsed` em segundos, sem campos ausentes.
        if let Some(ms) = st.elapsed_ms {
            v["elapsed"] = (ms / 1000).into();
        }
        if !st.paused {
            if let Some(o) = v.as_object_mut() {
                o.remove("paused");
            }
        }
        crate::bridge::post_json(v);
    }
    fn recording_finalizing(&self, active: bool) {
        crate::bridge::post_json(serde_json::json!({ "type": "recording_finalizing", "active": active }));
    }
    fn recording_stopping(&self) {
        crate::bridge::post_json(serde_json::json!({ "type": "recording_stopping" }));
    }
    fn replay_state(&self, st: &ReplayState) {
        let mut v = serde_json::to_value(st).unwrap_or_default();
        v["type"] = "replay_state".into();
        crate::bridge::post_json(v);
    }
    fn replay_saved(&self, id: &str, name: &str, duration_sec: i32) {
        crate::bridge::post_json(serde_json::json!({ "type": "replay_saved", "id": id, "name": name, "durationSec": duration_sec }));
    }
    fn replay_restarted(&self) {
        crate::bridge::post_json(serde_json::json!({ "type": "replay_restarted" }));
    }
    fn capture_windows(&self, seq: u32, items: &[WindowItem], app_audio: bool) {
        crate::bridge::post_json(serde_json::json!({ "type": "capture_windows", "seq": seq, "appAudio": app_audio, "items": items }));
    }
    fn capture_window_thumb(&self, seq: u32, id: &str, width: u32, height: u32, rgb: &[u8]) {
        // Os pixels não viram JSON: vai só o tamanho, pros testes conferirem.
        crate::bridge::post_json(serde_json::json!({ "type": "capture_window_thumb", "seq": seq, "id": id, "w": width, "h": height, "bytes": rgb.len() }));
    }
    fn window_picker_matches(&self, m: &WindowMatches) {
        let mut v = serde_json::to_value(m).unwrap_or_default();
        v["type"] = "window_picker_matches".into();
        crate::bridge::post_json(v);
    }
    fn record_countdown(&self, sec: u32, total: u32) {
        crate::bridge::post_json(serde_json::json!({ "type": "record_countdown", "sec": sec, "total": total }));
    }
    fn library(&self, lib: &Library) {
        let recs: Vec<_> = lib.recordings.iter().map(rec_item_json).collect();
        crate::bridge::post_json(serde_json::json!({
            "type": "recordings_loaded", "recordings": recs, "recordDir": lib.record_dir, "freeBytes": lib.free_bytes,
            "folders": lib.folders, "currentDir": lib.current_dir, "parentDir": lib.parent_dir,
            "atRoot": lib.at_root, "breadcrumb": lib.breadcrumb,
        }));
    }
    fn recording_added(&self, item: &RecItem, free_bytes: i64) {
        crate::bridge::post_json(serde_json::json!({ "type": "recording_added", "item": rec_item_json(item), "freeBytes": free_bytes }));
    }
    fn recording_meta(&self, id: &str, duration: i32, thumb: &str) {
        let mut v = serde_json::json!({ "type": "recording_meta", "id": id, "duration": duration });
        if !thumb.is_empty() {
            v["thumb"] = thumb.into();
        }
        crate::bridge::post_json(v);
    }
    fn recording_renamed(&self, old_id: &str, new_id: &str, new_name: &str) {
        crate::bridge::post_json(serde_json::json!({ "type": "recording_renamed", "oldId": old_id, "newId": new_id, "newName": new_name }));
    }
    fn recording_removed(&self, id: &str, free_bytes: i64) {
        let mut v = serde_json::json!({ "type": "recording_removed", "id": id });
        if free_bytes >= 0 {
            v["freeBytes"] = free_bytes.into();
        }
        crate::bridge::post_json(v);
    }
    fn folder_created(&self, id: &str) {
        crate::bridge::post_json(serde_json::json!({ "type": "folder_created", "id": id }));
    }
    fn transcript_search(&self, query: &str, ids: &[String]) {
        crate::bridge::post_json(serde_json::json!({ "type": "transcript_search", "query": query, "ids": ids }));
    }
    fn merge_pending(&self) {
        crate::bridge::post_json(serde_json::json!({ "type": "merge_pending" }));
    }
    fn merge_done(&self, ok: bool) {
        crate::bridge::post_json(serde_json::json!({ "type": "merge_done", "ok": ok }));
    }
    fn waveform(&self, id: &str, peaks: &[f32], hi: bool) {
        let p: Vec<f64> = peaks.iter().map(|v| (*v as f64 * 1000.0).round() / 1000.0).collect();
        crate::bridge::post_json(serde_json::json!({ "type": "waveform_ready", "id": id, "buckets": p.len(), "hi": hi, "peaks": p }));
    }
    fn video_info(&self, info: &VideoInfo) {
        let mut v = serde_json::to_value(info).unwrap_or_default();
        v["type"] = "video_info".into();
        crate::bridge::post_json(v);
    }
    fn transcript(&self, t: &Transcript) {
        crate::bridge::post_json(serde_json::json!({
            "type": "transcript", "id": t.id, "transcribed": t.transcribed, "has": !t.turns.is_empty(),
            "turns": t.turns, "speakers": t.speakers,
        }));
    }
}

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

use serde::Serialize;

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
}

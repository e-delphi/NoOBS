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
}

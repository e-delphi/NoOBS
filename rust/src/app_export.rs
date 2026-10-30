//! A exportação (fase 5, docs/ui-iron.md): por enquanto só o que a biblioteca
//! pergunta.

use iron::prelude::*;

/// A gravação sendo exportada agora (vazio = nenhuma).
pub fn running_id() -> String {
    String::new()
}

/// Abre a tela de exportação para a gravação `id`.
pub fn open_for(_ui: &mut Ui, _id: &str) {}

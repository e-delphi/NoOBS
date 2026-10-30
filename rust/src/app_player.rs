//! O player (fase 3, docs/ui-iron.md): por enquanto só a entrada que a
//! biblioteca chama.

use iron::prelude::*;

/// Abre a gravação `id` no player.
pub fn open(_ui: &mut Ui, id: &str) {
    crate::app_view::show_toast(id, "", false, 2000);
}

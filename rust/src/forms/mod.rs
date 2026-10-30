//! Forms do projeto. Gerado pela IDE do iron a cada "Gerar código": não edite.

use iron::prelude::*;

pub mod components;
pub mod main;

/// Registra os handlers de todos os forms.
pub fn register(ui: &mut Ui) {
    main::register(ui);
}

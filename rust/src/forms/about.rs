//! Eventos do form fraAbout. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraAbout;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraAbout.about_repository", about_repository);
    // iron-ide:register-end
}

pub fn about_repository(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    { let _ = ui; crate::app_settings::about::repository() }
}

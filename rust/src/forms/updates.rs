//! Eventos do form fraUpdates. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraUpdates;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraUpdates.updates_auto", updates_auto);
    ui.register_handler("fraUpdates.updates_check_now", updates_check_now);
    ui.register_handler("fraUpdates.updates_open_page", updates_open_page);
    // iron-ide:register-end
}

pub fn updates_auto(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::updates::auto(ui, sender);
}

pub fn updates_check_now(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::updates::check_now(ui);
}

pub fn updates_open_page(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    { let _ = ui; crate::app_settings::updates::open_page() }
}

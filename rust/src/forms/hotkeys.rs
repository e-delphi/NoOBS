//! Eventos do form fraHotkeys. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraHotkeys;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraHotkeys.hotkeys_add", hotkeys_add);
    // iron-ide:register-end
}

pub fn hotkeys_add(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    { let _ = ui; crate::app_settings::hotkeys_tab::add() }
}

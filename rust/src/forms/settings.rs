//! Eventos do form frmSettings. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::frmSettings;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("frmSettings.settings_close", settings_close);
    ui.register_handler("frmSettings.settings_tab", settings_tab);
    ui.register_handler("frmSettings.settings_resize", settings_resize);
    // iron-ide:register-end
}

pub fn settings_close(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::close(ui);
}

/// Uma aba da barra lateral: o `tag` da linha é a posição dela.
pub fn settings_tab(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    let index = ui.get_prop(sender, "tag").ok().and_then(|v| v.to_i64().ok()).unwrap_or(0);
    crate::app_settings::show_tab_at(ui, index.max(0) as usize);
}

pub fn settings_resize(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::resized(ui);
}

//! Eventos do form frmExport. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::frmExport;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("frmExport.export_close", export_close);
    ui.register_handler("frmExport.export_resize", export_resize);
    // iron-ide:register-end
}

pub fn export_close(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_export::close(ui);
}

pub fn export_resize(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_export::resized(ui);
}

//! Eventos do form frmMain. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::frmMain;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("frmMain.close_query", close_query);
    // iron-ide:register-end
}

/// [X] / Alt+F4: o host decide entre ir pra bandeja e fechar.
pub fn close_query(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::ui::on_close_query(ui);
}

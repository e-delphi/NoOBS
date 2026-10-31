//! Eventos do form fraGeneral. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraGeneral;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraGeneral.general_theme", general_theme);
    ui.register_handler("fraGeneral.general_language", general_language);
    ui.register_handler("fraGeneral.general_window_title", general_window_title);
    ui.register_handler("fraGeneral.general_autostart", general_autostart);
    ui.register_handler("fraGeneral.general_run_as_admin", general_run_as_admin);
    ui.register_handler("fraGeneral.general_close_to_tray", general_close_to_tray);
    ui.register_handler("fraGeneral.general_reset", general_reset);
    // iron-ide:register-end
}

/// Um dos botões do tema (Sistema, Claro, Escuro).
pub fn general_theme(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::general::theme(ui, sender);
}

pub fn general_language(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::general::language(ui, sender);
}

/// O título: ao sair do campo ou no Enter.
pub fn general_window_title(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_settings::general::window_title(ui, sender, args);
}

pub fn general_autostart(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::general::autostart(ui, sender);
}

pub fn general_run_as_admin(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::general::run_as_admin(ui, sender);
}

pub fn general_close_to_tray(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::general::close_to_tray(ui, sender);
}

pub fn general_reset(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::general::reset(ui);
}

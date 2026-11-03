//! Eventos do form fraGeneral: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraGeneral;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraGeneral {
    /// Um dos botões do tema (Sistema, Claro, Escuro).
    pub fn general_theme(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::general::theme(ui, sender);
    }

    pub fn general_language(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::general::language(ui, sender);
    }

    /// O título: ao sair do campo ou no Enter.
    pub fn general_window_title(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        crate::app_settings::general::window_title(ui, sender, args);
    }

    pub fn general_autostart(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::general::autostart(ui, sender);
    }

    pub fn general_run_as_admin(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::general::run_as_admin(ui, sender);
    }

    pub fn general_close_to_tray(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::general::close_to_tray(ui, sender);
    }

    pub fn general_reset(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::general::reset(ui);
    }

    pub fn general_hibernate(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::general::hibernate(ui, sender);
    }
}


//! Eventos do form fraReplay: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraReplay;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraReplay {
    pub fn replay_auto_start(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::check(ui, sender);
    }

    pub fn replay_into_recording(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::check(ui, sender);
    }

    pub fn replay_seconds(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::limits(ui);
    }

    pub fn replay_memory(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::limits(ui);
    }

    /// Os programas: com atraso enquanto digita; ao sair do campo ou no Enter, na hora.
    pub fn replay_apps_edit(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::apps_edit(ui, sender);
    }

    pub fn replay_apps(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        crate::app_settings::replay_tab::apps(ui, sender, args);
    }

    pub fn replay_indicator(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::check(ui, sender);
    }

    pub fn replay_clickable(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::check(ui, sender);
    }

    pub fn replay_corner(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::corner(ui, sender);
    }

    pub fn replay_opacity(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::replay_tab::opacity(ui, sender);
    }
}


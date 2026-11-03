//! Eventos do form fraOnRecord: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraOnRecord;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraOnRecord {
    pub fn onrecord_rec_indicator(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::check(ui, sender);
    }

    pub fn onrecord_rec_clickable(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::check(ui, sender);
    }

    pub fn onrecord_rec_corner(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::rec_corner(ui, sender);
    }

    pub fn onrecord_rec_opacity(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::rec_opacity(ui, sender);
    }

    pub fn onrecord_fullscreen(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::check(ui, sender);
    }

    pub fn onrecord_play_sound(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::check(ui, sender);
    }

    pub fn onrecord_minimize(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::check(ui, sender);
    }

    pub fn onrecord_auto_record(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::check(ui, sender);
    }

    /// A lista de apps: ao sair do campo ou no Enter.
    pub fn onrecord_auto_apps(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        crate::app_settings::onrecord::auto_apps(ui, sender, args);
    }

    pub fn onrecord_auto_except(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        crate::app_settings::onrecord::auto_except(ui, sender, args);
    }

    pub fn onrecord_stop_on_lock(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::onrecord::check(ui, sender);
    }
}


//! Eventos do form fraOnRecord. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraOnRecord;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraOnRecord.onrecord_rec_indicator", onrecord_rec_indicator);
    ui.register_handler("fraOnRecord.onrecord_rec_clickable", onrecord_rec_clickable);
    ui.register_handler("fraOnRecord.onrecord_rec_corner", onrecord_rec_corner);
    ui.register_handler("fraOnRecord.onrecord_rec_opacity", onrecord_rec_opacity);
    ui.register_handler("fraOnRecord.onrecord_fullscreen", onrecord_fullscreen);
    ui.register_handler("fraOnRecord.onrecord_play_sound", onrecord_play_sound);
    ui.register_handler("fraOnRecord.onrecord_minimize", onrecord_minimize);
    ui.register_handler("fraOnRecord.onrecord_auto_record", onrecord_auto_record);
    ui.register_handler("fraOnRecord.onrecord_auto_apps", onrecord_auto_apps);
    ui.register_handler("fraOnRecord.onrecord_auto_except", onrecord_auto_except);
    ui.register_handler("fraOnRecord.onrecord_stop_on_lock", onrecord_stop_on_lock);
    // iron-ide:register-end
}

pub fn onrecord_rec_indicator(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::check(ui, sender);
}

pub fn onrecord_rec_clickable(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::check(ui, sender);
}

pub fn onrecord_rec_corner(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::rec_corner(ui, sender);
}

pub fn onrecord_rec_opacity(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::rec_opacity(ui, sender);
}

pub fn onrecord_fullscreen(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::check(ui, sender);
}

pub fn onrecord_play_sound(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::check(ui, sender);
}

pub fn onrecord_minimize(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::check(ui, sender);
}

pub fn onrecord_auto_record(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::check(ui, sender);
}

/// A lista de apps: ao sair do campo ou no Enter.
pub fn onrecord_auto_apps(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_settings::onrecord::auto_apps(ui, sender, args);
}

pub fn onrecord_auto_except(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_settings::onrecord::auto_except(ui, sender, args);
}

pub fn onrecord_stop_on_lock(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::onrecord::check(ui, sender);
}

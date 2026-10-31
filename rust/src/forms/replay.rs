//! Eventos do form fraReplay. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraReplay;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraReplay.replay_auto_start", replay_auto_start);
    ui.register_handler("fraReplay.replay_into_recording", replay_into_recording);
    ui.register_handler("fraReplay.replay_seconds", replay_seconds);
    ui.register_handler("fraReplay.replay_memory", replay_memory);
    ui.register_handler("fraReplay.replay_apps_edit", replay_apps_edit);
    ui.register_handler("fraReplay.replay_apps", replay_apps);
    ui.register_handler("fraReplay.replay_indicator", replay_indicator);
    ui.register_handler("fraReplay.replay_clickable", replay_clickable);
    ui.register_handler("fraReplay.replay_corner", replay_corner);
    ui.register_handler("fraReplay.replay_opacity", replay_opacity);
    // iron-ide:register-end
}

pub fn replay_auto_start(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::check(ui, sender);
}

pub fn replay_into_recording(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::check(ui, sender);
}

pub fn replay_seconds(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::limits(ui);
}

pub fn replay_memory(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::limits(ui);
}

/// Os programas: com atraso enquanto digita; ao sair do campo ou no Enter, na hora.
pub fn replay_apps_edit(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::apps_edit(ui, sender);
}

pub fn replay_apps(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_settings::replay_tab::apps(ui, sender, args);
}

pub fn replay_indicator(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::check(ui, sender);
}

pub fn replay_clickable(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::check(ui, sender);
}

pub fn replay_corner(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::corner(ui, sender);
}

pub fn replay_opacity(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::replay_tab::opacity(ui, sender);
}

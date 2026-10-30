//! Eventos do form frmMain. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::frmMain;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("frmMain.close_query", close_query);
    ui.register_handler("frmMain.settings_click", settings_click);
    ui.register_handler("frmMain.record_click", record_click);
    ui.register_handler("frmMain.replay_click", replay_click);
    ui.register_handler("frmMain.pause_click", pause_click);
    ui.register_handler("frmMain.capture_click", capture_click);
    ui.register_handler("frmMain.replay_switch", replay_switch);
    ui.register_handler("frmMain.replay_save_click", replay_save_click);
    ui.register_handler("frmMain.display_change", display_change);
    ui.register_handler("frmMain.picker_close", picker_close);
    ui.register_handler("frmMain.picker_refresh", picker_refresh);
    ui.register_handler("frmMain.picker_mode", picker_mode);
    ui.register_handler("frmMain.picker_audio", picker_audio);
    ui.register_handler("frmMain.picker_search", picker_search);
    ui.register_handler("frmMain.picker_timelapse", picker_timelapse);
    ui.register_handler("frmMain.picker_timelapse_cursor", picker_timelapse_cursor);
    ui.register_handler("frmMain.picker_select_area", picker_select_area);
    // iron-ide:register-end
}

/// [X] / Alt+F4: o host decide entre ir pra bandeja e fechar.
pub fn close_query(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::ui::on_close_query(ui);
}

/// A engrenagem do cartão: as Configurações (entram na fase delas).
pub fn settings_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    let _ = ui;
}

pub fn record_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::record_click(ui);
}

/// A linha do buffer em memória inteira liga/desliga.
pub fn replay_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::replay_toggle(ui);
}

pub fn pause_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::pause_click(ui);
}

/// "Capturar..." (janela ou área).
pub fn capture_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::capture_click(ui);
}

pub fn replay_switch(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_main::replay_switch(ui, sender);
}

pub fn replay_save_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::replay_save(ui);
}

pub fn display_change(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_main::display_change(ui, sender, args);
}

/// O X, ou o clique fora do diálogo.
pub fn picker_close(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::close(ui);
}

pub fn picker_refresh(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::refresh(ui);
}

pub fn picker_mode(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_mode(ui, sender);
}

pub fn picker_audio(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_audio(ui, sender);
}

pub fn picker_search(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_search(ui, sender);
}

pub fn picker_timelapse(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_timelapse(ui, sender);
}

pub fn picker_timelapse_cursor(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_timelapse_cursor(ui, sender);
}

pub fn picker_select_area(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_select_area(ui);
}

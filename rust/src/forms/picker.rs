//! Eventos do form frmPicker. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::frmPicker;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("frmPicker.picker_close", picker_close);
    ui.register_handler("frmPicker.picker_refresh", picker_refresh);
    ui.register_handler("frmPicker.picker_mode", picker_mode);
    ui.register_handler("frmPicker.picker_audio", picker_audio);
    ui.register_handler("frmPicker.picker_search", picker_search);
    ui.register_handler("frmPicker.picker_timelapse", picker_timelapse);
    ui.register_handler("frmPicker.picker_timelapse_cursor", picker_timelapse_cursor);
    ui.register_handler("frmPicker.picker_select_area", picker_select_area);
    // iron-ide:register-end
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

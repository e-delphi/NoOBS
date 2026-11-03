//! Eventos do form frmPicker: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FrmPicker;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FrmPicker {
    /// O X, ou o clique fora do diálogo.
    pub fn picker_close(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::close(ui);
    }

    pub fn picker_refresh(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::refresh(ui);
    }

    pub fn picker_mode(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::on_mode(ui, sender);
    }

    pub fn picker_audio(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::on_audio(ui, sender);
    }

    pub fn picker_search(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::on_search(ui, sender);
    }

    pub fn picker_timelapse(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::on_timelapse(ui, sender);
    }

    pub fn picker_timelapse_cursor(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::on_timelapse_cursor(ui, sender);
    }

    pub fn picker_select_area(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_picker::on_select_area(ui);
    }
}


//! Eventos do form frmExport: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FrmExport;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FrmExport {
    pub fn export_close(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_export::close(ui);
    }

    pub fn export_resize(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_export::resized(ui);
    }
}


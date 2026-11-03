//! Eventos do form fraFiles: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraFiles;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraFiles {
    pub fn files_browse(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        { let _ = ui; crate::app_settings::files::browse() }
    }

    pub fn files_record_dir(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        crate::app_settings::files::record_dir(ui, sender, args);
    }

    /// A prévia do nome a cada tecla.
    pub fn files_filename_edit(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::files::filename_edit(ui, sender);
    }

    pub fn files_filename(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        crate::app_settings::files::filename(ui, sender, args);
    }

    pub fn files_library(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::files::library(ui, sender);
    }
}


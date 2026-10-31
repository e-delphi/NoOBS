//! Eventos do form fraFiles. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraFiles;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraFiles.files_browse", files_browse);
    ui.register_handler("fraFiles.files_record_dir", files_record_dir);
    ui.register_handler("fraFiles.files_filename_edit", files_filename_edit);
    ui.register_handler("fraFiles.files_filename", files_filename);
    ui.register_handler("fraFiles.files_library", files_library);
    // iron-ide:register-end
}

pub fn files_browse(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    { let _ = ui; crate::app_settings::files::browse() }
}

pub fn files_record_dir(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_settings::files::record_dir(ui, sender, args);
}

/// A prévia do nome a cada tecla.
pub fn files_filename_edit(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::files::filename_edit(ui, sender);
}

pub fn files_filename(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_settings::files::filename(ui, sender, args);
}

pub fn files_library(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::files::library(ui, sender);
}

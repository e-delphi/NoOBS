//! Eventos do form fraTranscribe. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraTranscribe;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraTranscribe.transcribe_engine", transcribe_engine);
    ui.register_handler("fraTranscribe.transcribe_language", transcribe_language);
    ui.register_handler("fraTranscribe.transcribe_on_stop", transcribe_on_stop);
    ui.register_handler("fraTranscribe.transcribe_pending", transcribe_pending);
    ui.register_handler("fraTranscribe.transcribe_cancel", transcribe_cancel);
    // iron-ide:register-end
}

pub fn transcribe_engine(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::transcribe_tab::engine(ui);
}

pub fn transcribe_language(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::transcribe_tab::language(ui, sender);
}

pub fn transcribe_on_stop(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::transcribe_tab::on_stop(ui, sender);
}

pub fn transcribe_pending(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    { let _ = ui; crate::app_settings::transcribe_tab::pending() }
}

pub fn transcribe_cancel(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::transcribe_tab::cancel(ui);
}

//! Eventos do form fraUpdates: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraUpdates;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraUpdates {
    pub fn updates_auto(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::updates::auto(ui, sender);
    }

    pub fn updates_check_now(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::updates::check_now(ui);
    }

    pub fn updates_open_page(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        { let _ = ui; crate::app_settings::updates::open_page() }
    }
}


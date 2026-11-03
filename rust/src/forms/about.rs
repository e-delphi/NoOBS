//! Eventos do form fraAbout: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraAbout;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraAbout {
    pub fn about_repository(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        { let _ = ui; crate::app_settings::about::repository() }
    }
}

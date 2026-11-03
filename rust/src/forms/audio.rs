//! Eventos do form fraAudio: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraAudio;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraAudio {
    pub fn audio_bitrate(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::audio::bitrate(ui, sender);
    }

    pub fn audio_mute_when_muted(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::audio::mute_when_muted(ui, sender);
    }
}


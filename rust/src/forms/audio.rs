//! Eventos do form fraAudio. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraAudio;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraAudio.audio_bitrate", audio_bitrate);
    ui.register_handler("fraAudio.audio_mute_when_muted", audio_mute_when_muted);
    // iron-ide:register-end
}

pub fn audio_bitrate(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::audio::bitrate(ui, sender);
}

pub fn audio_mute_when_muted(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::audio::mute_when_muted(ui, sender);
}

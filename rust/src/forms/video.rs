//! Eventos do form fraVideo. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::fraVideo;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("fraVideo.video_codec", video_codec);
    ui.register_handler("fraVideo.video_quality", video_quality);
    ui.register_handler("fraVideo.video_fps", video_fps);
    ui.register_handler("fraVideo.video_keyframe", video_keyframe);
    // iron-ide:register-end
}

pub fn video_codec(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::video::codec(ui, sender);
}

pub fn video_quality(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::video::quality(ui, sender);
}

pub fn video_fps(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::video::fps(ui, sender);
}

pub fn video_keyframe(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::video::keyframe(ui, sender);
}

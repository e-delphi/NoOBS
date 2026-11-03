//! Eventos do form fraVideo: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FraVideo;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FraVideo {
    pub fn video_codec(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::video::codec(ui, sender);
    }

    pub fn video_quality(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::video::quality(ui, sender);
    }

    pub fn video_fps(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::video::fps(ui, sender);
    }

    pub fn video_keyframe(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::video::keyframe(ui, sender);
    }
}


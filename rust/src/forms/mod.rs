//! Forms do projeto. Gerado pela IDE do iron a cada "Gerar código": não edite.

use iron::prelude::*;

pub mod components;
pub mod main;
pub mod settings;
pub mod player;
pub mod export;
pub mod picker;
pub mod general;
pub mod onrecord;
pub mod files;
pub mod video;
pub mod audio;
pub mod replay;
pub mod hotkeys;
pub mod devices;
pub mod transcribe;
pub mod updates;
pub mod about;

/// Registra os handlers de todos os forms.
pub fn register(ui: &mut Ui) {
    main::register(ui);
    settings::register(ui);
    player::register(ui);
    export::register(ui);
    picker::register(ui);
    general::register(ui);
    onrecord::register(ui);
    files::register(ui);
    video::register(ui);
    audio::register(ui);
    replay::register(ui);
    hotkeys::register(ui);
    devices::register(ui);
    transcribe::register(ui);
    updates::register(ui);
    about::register(ui);
}

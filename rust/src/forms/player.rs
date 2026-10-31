//! Eventos do form frmPlayer. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::frmPlayer;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("frmPlayer.player_side_close", player_side_close);
    ui.register_handler("frmPlayer.player_tr_search", player_tr_search);
    ui.register_handler("frmPlayer.player_stage_resize", player_stage_resize);
    ui.register_handler("frmPlayer.player_close", player_close);
    ui.register_handler("frmPlayer.player_play", player_play);
    ui.register_handler("frmPlayer.player_fullscreen", player_fullscreen);
    ui.register_handler("frmPlayer.player_info", player_info);
    ui.register_handler("frmPlayer.player_captions", player_captions);
    ui.register_handler("frmPlayer.player_speed", player_speed);
    ui.register_handler("frmPlayer.player_mute", player_mute);
    ui.register_handler("frmPlayer.player_seek", player_seek);
    ui.register_handler("frmPlayer.player_volume", player_volume);
    ui.register_handler("frmPlayer.player_transcript", player_transcript);
    // iron-ide:register-end
}

pub fn player_side_close(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::close_side(ui);
}

pub fn player_tr_search(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    let text = match ui.get_prop(sender, "text") { Ok(PropValue::String(t)) => t, _ => String::new() };
    crate::app_player::tr_search(ui, &text);
}

pub fn player_stage_resize(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::stage_resized(ui);
}

pub fn player_close(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::close(ui);
}

pub fn player_play(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::toggle_play(ui);
}

pub fn player_fullscreen(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::toggle_fullscreen(ui);
}

pub fn player_info(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::toggle_info(ui);
}

pub fn player_captions(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::toggle_captions(ui);
}

pub fn player_speed(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::speed_menu(ui, sender);
}

pub fn player_mute(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::toggle_mute(ui);
}

pub fn player_seek(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    if let EventArgs::Value(v) = args {
        crate::app_player::seek_to(ui, v.to_f64().unwrap_or(0.0));
    }
}

pub fn player_volume(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    let v = ui.get_prop(sender, "value").ok().and_then(|v| v.to_f64().ok()).unwrap_or(100.0);
    crate::app_player::set_volume(ui, v);
}

pub fn player_transcript(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::toggle_transcript(ui);
}

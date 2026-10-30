//! Eventos do form frmMain. Gerado pela IDE do iron: o corpo das funções é seu;
//! a IDE só refaz o bloco de registro e acrescenta funções novas.

use iron::prelude::*;
#[allow(unused_imports)]
use super::components::frmMain;

/// Liga os handlers aos nomes usados no layout.
pub fn register(ui: &mut Ui) {
    // iron-ide:register-begin (gerado pela IDE; não edite este bloco)
    ui.register_handler("frmMain.close_query", close_query);
    ui.register_handler("frmMain.update_close", update_close);
    ui.register_handler("frmMain.update_open", update_open);
    ui.register_handler("frmMain.settings_click", settings_click);
    ui.register_handler("frmMain.record_click", record_click);
    ui.register_handler("frmMain.replay_click", replay_click);
    ui.register_handler("frmMain.pause_click", pause_click);
    ui.register_handler("frmMain.capture_click", capture_click);
    ui.register_handler("frmMain.replay_switch", replay_switch);
    ui.register_handler("frmMain.replay_save_click", replay_save_click);
    ui.register_handler("frmMain.display_change", display_change);
    ui.register_handler("frmMain.library_resize", library_resize);
    ui.register_handler("frmMain.library_search", library_search);
    ui.register_handler("frmMain.library_open_dir", library_open_dir);
    ui.register_handler("frmMain.library_new_folder", library_new_folder);
    ui.register_handler("frmMain.library_merge", library_merge);
    ui.register_handler("frmMain.library_export", library_export);
    ui.register_handler("frmMain.library_delete", library_delete);
    ui.register_handler("frmMain.library_crumb", library_crumb);
    ui.register_handler("frmMain.library_background", library_background);
    ui.register_handler("frmMain.picker_close", picker_close);
    ui.register_handler("frmMain.picker_refresh", picker_refresh);
    ui.register_handler("frmMain.picker_mode", picker_mode);
    ui.register_handler("frmMain.picker_audio", picker_audio);
    ui.register_handler("frmMain.picker_search", picker_search);
    ui.register_handler("frmMain.picker_timelapse", picker_timelapse);
    ui.register_handler("frmMain.picker_timelapse_cursor", picker_timelapse_cursor);
    ui.register_handler("frmMain.picker_select_area", picker_select_area);
    ui.register_handler("frmMain.settings_close", settings_close);
    ui.register_handler("frmMain.settings_resize", settings_resize);
    ui.register_handler("frmMain.export_close", export_close);
    ui.register_handler("frmMain.export_resize", export_resize);
    ui.register_handler("frmMain.player_side_close", player_side_close);
    ui.register_handler("frmMain.player_tr_search", player_tr_search);
    ui.register_handler("frmMain.player_close", player_close);
    ui.register_handler("frmMain.player_play", player_play);
    ui.register_handler("frmMain.player_fullscreen", player_fullscreen);
    ui.register_handler("frmMain.player_transcript", player_transcript);
    ui.register_handler("frmMain.player_info", player_info);
    ui.register_handler("frmMain.player_captions", player_captions);
    ui.register_handler("frmMain.player_speed", player_speed);
    ui.register_handler("frmMain.player_volume", player_volume);
    ui.register_handler("frmMain.player_mute", player_mute);
    ui.register_handler("frmMain.player_seek", player_seek);
    ui.register_handler("frmMain.player_stage_resize", player_stage_resize);
    // iron-ide:register-end
}

/// [X] / Alt+F4: o host decide entre ir pra bandeja e fechar.
pub fn close_query(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::ui::on_close_query(ui);
}

/// A engrenagem do cartão: as Configurações (entram na fase delas).
pub fn settings_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::open(ui, None);
}

pub fn record_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::record_click(ui);
}

/// A linha do buffer em memória inteira liga/desliga.
pub fn replay_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::replay_toggle(ui);
}

pub fn pause_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::pause_click(ui);
}

/// "Capturar..." (janela ou área).
pub fn capture_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::capture_click(ui);
}

pub fn replay_switch(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_main::replay_switch(ui, sender);
}

pub fn replay_save_click(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::replay_save(ui);
}

pub fn display_change(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    crate::app_main::display_change(ui, sender, args);
}

/// O X, ou o clique fora do diálogo.
pub fn picker_close(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::close(ui);
}

pub fn picker_refresh(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::refresh(ui);
}

pub fn picker_mode(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_mode(ui, sender);
}

pub fn picker_audio(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_audio(ui, sender);
}

pub fn picker_search(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_search(ui, sender);
}

pub fn picker_timelapse(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_timelapse(ui, sender);
}

pub fn picker_timelapse_cursor(ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_timelapse_cursor(ui, sender);
}

pub fn picker_select_area(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_picker::on_select_area(ui);
}

pub fn library_resize(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_library::resized(ui);
}

pub fn library_search(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    let text = match ui.get_prop(sender, "text") { Ok(PropValue::String(t)) => t, _ => String::new() };
    crate::app_library::search(ui, &text);
}

pub fn library_open_dir(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    let _ = ui;
    crate::bridge::handle_open_record_dir();
}

pub fn library_new_folder(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_library::new_folder(ui);
}

pub fn library_merge(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_library::merge_selected(ui);
}

pub fn library_export(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_library::export_selected(ui);
}

pub fn library_delete(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_library::delete_selected(ui);
}

pub fn library_crumb(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_library::crumb(ui, args);
}

pub fn library_background(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_library::background_context(ui, sender, args);
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

pub fn player_transcript(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::toggle_transcript(ui);
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

pub fn player_volume(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    let v = ui.get_prop(sender, "value").ok().and_then(|v| v.to_f64().ok()).unwrap_or(100.0);
    crate::app_player::set_volume(ui, v);
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

pub fn player_stage_resize(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    let _ = (&sender, &args);
    crate::app_player::stage_resized(ui);
}

pub fn settings_close(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::close(ui);
}

pub fn settings_resize(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_settings::resized(ui);
}

pub fn update_close(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::hide_update_banner(ui);
}

pub fn update_open(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_main::open_update_page(ui);
}

pub fn export_close(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_export::close(ui);
}

pub fn export_resize(ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
    crate::app_export::resized(ui);
}

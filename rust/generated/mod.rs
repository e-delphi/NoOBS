//! O que a IDE do Iron refaz a cada "Gerar código" (não edite): o acesso tipado aos
//! componentes, a struct de cada form e o registro. O código dos forms é seu, em
//! `src/forms/`.

use iron::prelude::Ui;
use iron::prelude::instances::form_handler;

pub mod components;
pub use components::*;

#[path = "../src/forms/main.rs"]
pub mod main;

#[path = "../src/forms/settings.rs"]
pub mod settings;

#[path = "../src/forms/player.rs"]
pub mod player;

#[path = "../src/forms/export.rs"]
pub mod export;

#[path = "../src/forms/picker.rs"]
pub mod picker;

#[path = "../src/forms/general.rs"]
pub mod general;

#[path = "../src/forms/onrecord.rs"]
pub mod onrecord;

#[path = "../src/forms/files.rs"]
pub mod files;

#[path = "../src/forms/video.rs"]
pub mod video;

#[path = "../src/forms/audio.rs"]
pub mod audio;

#[path = "../src/forms/replay.rs"]
pub mod replay;

#[path = "../src/forms/hotkeys.rs"]
pub mod hotkeys;

#[path = "../src/forms/devices.rs"]
pub mod devices;

#[path = "../src/forms/transcribe.rs"]
pub mod transcribe;

#[path = "../src/forms/updates.rs"]
pub mod updates;

#[path = "../src/forms/about.rs"]
pub mod about;

/// Registra as classes dos forms (cada form aberto ganha a instância dele), os handlers e
/// as classes dos pacotes de código.
pub fn register(ui: &mut Ui) {
    ui.register_form_class("frmMain", FrmMain::bind);
    ui.register_handler("frmMain.close_query", form_handler(FrmMain::close_query));
    ui.register_handler("frmMain.update_close", form_handler(FrmMain::update_close));
    ui.register_handler("frmMain.update_open", form_handler(FrmMain::update_open));
    ui.register_handler("frmMain.settings_click", form_handler(FrmMain::settings_click));
    ui.register_handler("frmMain.record_click", form_handler(FrmMain::record_click));
    ui.register_handler("frmMain.replay_click", form_handler(FrmMain::replay_click));
    ui.register_handler("frmMain.pause_click", form_handler(FrmMain::pause_click));
    ui.register_handler("frmMain.capture_click", form_handler(FrmMain::capture_click));
    ui.register_handler("frmMain.replay_switch", form_handler(FrmMain::replay_switch));
    ui.register_handler("frmMain.replay_save_click", form_handler(FrmMain::replay_save_click));
    ui.register_handler("frmMain.display_change", form_handler(FrmMain::display_change));
    ui.register_handler("frmMain.library_resize", form_handler(FrmMain::library_resize));
    ui.register_handler("frmMain.library_background", form_handler(FrmMain::library_background));
    ui.register_handler("frmMain.library_search", form_handler(FrmMain::library_search));
    ui.register_handler("frmMain.library_open_dir", form_handler(FrmMain::library_open_dir));
    ui.register_handler("frmMain.library_new_folder", form_handler(FrmMain::library_new_folder));
    ui.register_handler("frmMain.library_merge", form_handler(FrmMain::library_merge));
    ui.register_handler("frmMain.library_export", form_handler(FrmMain::library_export));
    ui.register_handler("frmMain.library_delete", form_handler(FrmMain::library_delete));
    ui.register_handler("frmMain.library_trash_drop", form_handler(FrmMain::library_trash_drop));
    ui.register_handler("frmMain.library_crumb", form_handler(FrmMain::library_crumb));
    ui.register_form_class("frmSettings", FrmSettings::bind);
    ui.register_handler("frmSettings.settings_close", form_handler(FrmSettings::settings_close));
    ui.register_handler("frmSettings.settings_tab", form_handler(FrmSettings::settings_tab));
    ui.register_handler("frmSettings.settings_resize", form_handler(FrmSettings::settings_resize));
    ui.register_form_class("frmPlayer", FrmPlayer::bind);
    ui.register_handler("frmPlayer.player_side_close", form_handler(FrmPlayer::player_side_close));
    ui.register_handler("frmPlayer.player_tr_search", form_handler(FrmPlayer::player_tr_search));
    ui.register_handler("frmPlayer.player_stage_resize", form_handler(FrmPlayer::player_stage_resize));
    ui.register_handler("frmPlayer.player_close", form_handler(FrmPlayer::player_close));
    ui.register_handler("frmPlayer.player_play", form_handler(FrmPlayer::player_play));
    ui.register_handler("frmPlayer.player_fullscreen", form_handler(FrmPlayer::player_fullscreen));
    ui.register_handler("frmPlayer.player_info", form_handler(FrmPlayer::player_info));
    ui.register_handler("frmPlayer.player_captions", form_handler(FrmPlayer::player_captions));
    ui.register_handler("frmPlayer.player_speed", form_handler(FrmPlayer::player_speed));
    ui.register_handler("frmPlayer.player_mute", form_handler(FrmPlayer::player_mute));
    ui.register_handler("frmPlayer.player_seek", form_handler(FrmPlayer::player_seek));
    ui.register_handler("frmPlayer.player_volume", form_handler(FrmPlayer::player_volume));
    ui.register_handler("frmPlayer.player_transcript", form_handler(FrmPlayer::player_transcript));
    ui.register_form_class("frmExport", FrmExport::bind);
    ui.register_handler("frmExport.export_close", form_handler(FrmExport::export_close));
    ui.register_handler("frmExport.export_resize", form_handler(FrmExport::export_resize));
    ui.register_form_class("frmPicker", FrmPicker::bind);
    ui.register_handler("frmPicker.picker_close", form_handler(FrmPicker::picker_close));
    ui.register_handler("frmPicker.picker_refresh", form_handler(FrmPicker::picker_refresh));
    ui.register_handler("frmPicker.picker_mode", form_handler(FrmPicker::picker_mode));
    ui.register_handler("frmPicker.picker_audio", form_handler(FrmPicker::picker_audio));
    ui.register_handler("frmPicker.picker_search", form_handler(FrmPicker::picker_search));
    ui.register_handler("frmPicker.picker_timelapse", form_handler(FrmPicker::picker_timelapse));
    ui.register_handler("frmPicker.picker_timelapse_cursor", form_handler(FrmPicker::picker_timelapse_cursor));
    ui.register_handler("frmPicker.picker_select_area", form_handler(FrmPicker::picker_select_area));
    ui.register_form_class("fraGeneral", FraGeneral::bind);
    ui.register_handler("fraGeneral.general_theme", form_handler(FraGeneral::general_theme));
    ui.register_handler("fraGeneral.general_language", form_handler(FraGeneral::general_language));
    ui.register_handler("fraGeneral.general_window_title", form_handler(FraGeneral::general_window_title));
    ui.register_handler("fraGeneral.general_autostart", form_handler(FraGeneral::general_autostart));
    ui.register_handler("fraGeneral.general_run_as_admin", form_handler(FraGeneral::general_run_as_admin));
    ui.register_handler("fraGeneral.general_close_to_tray", form_handler(FraGeneral::general_close_to_tray));
    ui.register_handler("fraGeneral.general_hibernate", form_handler(FraGeneral::general_hibernate));
    ui.register_handler("fraGeneral.general_reset", form_handler(FraGeneral::general_reset));
    ui.register_form_class("fraOnRecord", FraOnRecord::bind);
    ui.register_handler("fraOnRecord.onrecord_rec_indicator", form_handler(FraOnRecord::onrecord_rec_indicator));
    ui.register_handler("fraOnRecord.onrecord_rec_clickable", form_handler(FraOnRecord::onrecord_rec_clickable));
    ui.register_handler("fraOnRecord.onrecord_rec_corner", form_handler(FraOnRecord::onrecord_rec_corner));
    ui.register_handler("fraOnRecord.onrecord_rec_opacity", form_handler(FraOnRecord::onrecord_rec_opacity));
    ui.register_handler("fraOnRecord.onrecord_fullscreen", form_handler(FraOnRecord::onrecord_fullscreen));
    ui.register_handler("fraOnRecord.onrecord_play_sound", form_handler(FraOnRecord::onrecord_play_sound));
    ui.register_handler("fraOnRecord.onrecord_minimize", form_handler(FraOnRecord::onrecord_minimize));
    ui.register_handler("fraOnRecord.onrecord_auto_record", form_handler(FraOnRecord::onrecord_auto_record));
    ui.register_handler("fraOnRecord.onrecord_auto_apps", form_handler(FraOnRecord::onrecord_auto_apps));
    ui.register_handler("fraOnRecord.onrecord_auto_except", form_handler(FraOnRecord::onrecord_auto_except));
    ui.register_handler("fraOnRecord.onrecord_stop_on_lock", form_handler(FraOnRecord::onrecord_stop_on_lock));
    ui.register_form_class("fraFiles", FraFiles::bind);
    ui.register_handler("fraFiles.files_browse", form_handler(FraFiles::files_browse));
    ui.register_handler("fraFiles.files_record_dir", form_handler(FraFiles::files_record_dir));
    ui.register_handler("fraFiles.files_filename_edit", form_handler(FraFiles::files_filename_edit));
    ui.register_handler("fraFiles.files_filename", form_handler(FraFiles::files_filename));
    ui.register_handler("fraFiles.files_library", form_handler(FraFiles::files_library));
    ui.register_form_class("fraVideo", FraVideo::bind);
    ui.register_handler("fraVideo.video_codec", form_handler(FraVideo::video_codec));
    ui.register_handler("fraVideo.video_quality", form_handler(FraVideo::video_quality));
    ui.register_handler("fraVideo.video_fps", form_handler(FraVideo::video_fps));
    ui.register_handler("fraVideo.video_keyframe", form_handler(FraVideo::video_keyframe));
    ui.register_form_class("fraAudio", FraAudio::bind);
    ui.register_handler("fraAudio.audio_bitrate", form_handler(FraAudio::audio_bitrate));
    ui.register_handler("fraAudio.audio_mute_when_muted", form_handler(FraAudio::audio_mute_when_muted));
    ui.register_form_class("fraReplay", FraReplay::bind);
    ui.register_handler("fraReplay.replay_auto_start", form_handler(FraReplay::replay_auto_start));
    ui.register_handler("fraReplay.replay_into_recording", form_handler(FraReplay::replay_into_recording));
    ui.register_handler("fraReplay.replay_seconds", form_handler(FraReplay::replay_seconds));
    ui.register_handler("fraReplay.replay_memory", form_handler(FraReplay::replay_memory));
    ui.register_handler("fraReplay.replay_apps_edit", form_handler(FraReplay::replay_apps_edit));
    ui.register_handler("fraReplay.replay_apps", form_handler(FraReplay::replay_apps));
    ui.register_handler("fraReplay.replay_indicator", form_handler(FraReplay::replay_indicator));
    ui.register_handler("fraReplay.replay_clickable", form_handler(FraReplay::replay_clickable));
    ui.register_handler("fraReplay.replay_corner", form_handler(FraReplay::replay_corner));
    ui.register_handler("fraReplay.replay_opacity", form_handler(FraReplay::replay_opacity));
    ui.register_form_class("fraHotkeys", FraHotkeys::bind);
    ui.register_handler("fraHotkeys.hotkeys_add", form_handler(FraHotkeys::hotkeys_add));
    ui.register_form_class("fraDevices", FraDevices::bind);
    ui.register_form_class("fraTranscribe", FraTranscribe::bind);
    ui.register_handler("fraTranscribe.transcribe_engine", form_handler(FraTranscribe::transcribe_engine));
    ui.register_handler("fraTranscribe.transcribe_language", form_handler(FraTranscribe::transcribe_language));
    ui.register_handler("fraTranscribe.transcribe_on_stop", form_handler(FraTranscribe::transcribe_on_stop));
    ui.register_handler("fraTranscribe.transcribe_pending", form_handler(FraTranscribe::transcribe_pending));
    ui.register_handler("fraTranscribe.transcribe_cancel", form_handler(FraTranscribe::transcribe_cancel));
    ui.register_form_class("fraUpdates", FraUpdates::bind);
    ui.register_handler("fraUpdates.updates_auto", form_handler(FraUpdates::updates_auto));
    ui.register_handler("fraUpdates.updates_check_now", form_handler(FraUpdates::updates_check_now));
    ui.register_handler("fraUpdates.updates_open_page", form_handler(FraUpdates::updates_open_page));
    ui.register_form_class("fraAbout", FraAbout::bind);
    ui.register_handler("fraAbout.about_repository", form_handler(FraAbout::about_repository));
}

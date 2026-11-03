//! Eventos do form frmMain: o código é seu. Os handlers são métodos da
//! struct do form (gerada em `generated/`); a IDE só acrescenta os novos.

use iron::prelude::*;
use crate::generated::FrmMain;

/// O estado do form: cada janela aberta dele tem o seu (`self.data`).
#[derive(Default)]
pub struct Data {}

impl FrmMain {
    /// [X] / Alt+F4: o host decide entre ir pra bandeja e fechar.
    pub fn close_query(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::ui::on_close_query(ui);
    }

    /// A engrenagem do cartão: as Configurações (entram na fase delas).
    pub fn settings_click(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_settings::open(ui, None);
    }

    pub fn record_click(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_main::record_click(ui);
    }

    /// A linha do buffer em memória inteira liga/desliga.
    pub fn replay_click(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_main::replay_toggle(ui);
    }

    pub fn pause_click(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_main::pause_click(ui);
    }

    /// "Capturar..." (janela ou área).
    pub fn capture_click(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_main::capture_click(ui);
    }

    pub fn replay_switch(&mut self, ui: &mut Ui, sender: ComponentId, _args: &EventArgs) {
        crate::app_main::replay_switch(ui, sender);
    }

    pub fn replay_save_click(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_main::replay_save(ui);
    }

    pub fn display_change(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        crate::app_main::display_change(ui, sender, args);
    }

    pub fn library_resize(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        crate::app_library::resized(ui);
    }

    pub fn library_search(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        let text = match ui.get_prop(sender, "text") { Ok(PropValue::String(t)) => t, _ => String::new() };
        crate::app_library::search(ui, &text);
    }

    pub fn library_open_dir(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        let _ = ui;
        crate::bridge::handle_open_record_dir();
    }

    pub fn library_new_folder(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        crate::app_library::new_folder(ui);
    }

    pub fn library_merge(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        crate::app_library::merge_selected(ui);
    }

    pub fn library_export(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        crate::app_library::export_selected(ui);
    }

    pub fn library_delete(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        crate::app_library::delete_selected(ui);
    }

    pub fn library_crumb(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        crate::app_library::crumb(ui, args);
    }

    pub fn library_background(&mut self, ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
        let _ = (&sender, &args);
        crate::app_library::background_context(ui, sender, args);
    }

    pub fn update_close(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_main::hide_update_banner(ui);
    }

    pub fn update_open(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_main::open_update_page(ui);
    }

    pub fn library_trash_drop(&mut self, ui: &mut Ui, _sender: ComponentId, _args: &EventArgs) {
        crate::app_library::drop_on_trash(ui);
    }
}


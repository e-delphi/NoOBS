//! Aba Atalhos: registro e execução da lista (`crate::hotkeys` é o modelo).
//!
//! * Cada item ganha dois ids de `RegisterHotKey` ([`hotkeys::ids_of`]); o
//!   segundo só é usado no Ctrl+Pause, que o Windows entrega como Ctrl+Break.
//! * Os de `replay` só são registrados com o buffer LIGADO: desligado, a
//!   combinação volta a ser dos outros programas.
//! * Combinação repetida: vale a PRIMEIRA da lista (o Windows aceita um
//!   registro por combinação); as outras ficam marcadas pra a tela avisar.
//! * Qualquer atalho de gravação (record/window/area) PARA a gravação em
//!   curso e CANCELA a contagem regressiva.
//!
//! Mensagens: `get_hotkeys` / `set_hotkeys {items}` → `hotkeys_state
//! {items, status, delayMax}`; `define_hotkey_area {index}` escolhe o
//! retângulo de um item de área; `trigger_hotkey {index}` executa um item
//! (o seletor "Capturar... → Área" usa pros atalhos com área salva).
//! Os atalhos ficam na janela oculta (`ui`): valem com a janela do app na
//! bandeja, e o de gravar grava sem mostrá-la.

use std::cell::{Cell, RefCell};

use serde_json::{json, Value};

use crate::engine::AreaRect;
use crate::hotkeys::{self, Item, Mode};
use crate::{area_select, hotkey, lang, log, ui};

use super::{area, countdown, post_error, post_json, recording, replay, window_record};

thread_local! {
    static LIST: RefCell<Vec<Item>> = const { RefCell::new(Vec::new()) };
    static LOADED: Cell<bool> = const { Cell::new(false) };
    /// Situação de cada item, pra a tela: "" ok, ou o motivo.
    static STATUS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static REGISTERED: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    /// Item cuja área está sendo escolhida (`define_hotkey_area`).
    static DEFINING: Cell<Option<usize>> = const { Cell::new(None) };
    /// Atraso do atalho de área sem retângulo, enquanto o usuário seleciona.
    static SELECT_DELAY: Cell<u32> = const { Cell::new(0) };
}

/// A lista, lida do config (com a migração das chaves antigas) no 1º uso.
fn list() -> Vec<Item> {
    if !LOADED.with(|l| l.replace(true)) {
        LIST.with(|l| *l.borrow_mut() = hotkeys::load());
    }
    LIST.with(|l| l.borrow().clone())
}

fn set_list(list: Vec<Item>) {
    hotkeys::save(&list);
    LOADED.with(|l| l.set(true));
    LIST.with(|l| *l.borrow_mut() = list);
}

/// Primeira combinação do modo (dica dos botões da tela principal).
/// Atalhos de área com retângulo salvo: (índice, largura, altura, atalho). O
/// seletor "Capturar..." mostra um botão pra cada.
pub fn saved_areas() -> Vec<(usize, i32, i32, String)> {
    list()
        .iter()
        .enumerate()
        .filter(|(_, it)| it.mode == Mode::Area)
        .filter_map(|(i, it)| it.area.map(|a| (i, a.w, a.h, it.hotkey.clone())))
        .collect()
}

pub fn spec_of(mode: Mode) -> String {
    hotkeys::first_spec(&list(), mode)
}

fn unregister_all() {
    for id in REGISTERED.with(|r| std::mem::take(&mut *r.borrow_mut())) {
        ui::unregister_global_hotkey(id);
    }
}

/// (Re)registra a lista inteira. Chamado no init, a cada mudança da lista e
/// quando o buffer liga/desliga.
pub fn apply() {
    unregister_all();
    let list = list();
    let dups = hotkeys::duplicates(&list);
    let replay_on = replay::replay_active();
    let mut status = Vec::with_capacity(list.len());
    let mut registered = Vec::new();
    for (i, it) in list.iter().enumerate() {
        let st: &'static str = (|| {
            if it.hotkey.is_empty() {
                return "empty";
            }
            if dups.contains(&i) {
                return "duplicate";
            }
            let Some(hk) = hotkey::parse_hotkey(&it.hotkey) else { return "invalid" };
            if hotkey::reserved_reason(hk.modifiers, hk.vk).is_some() {
                return "reserved";
            }
            if it.mode == Mode::Replay && !replay_on {
                return "inactive";
            }
            let (id, alt) = hotkeys::ids_of(i);
            if !ui::register_global_hotkey(id, hk.modifiers, hk.vk) {
                log!("Atalhos: \"{}\" ({}) em uso por outro programa.", it.hotkey, it.mode.as_str());
                return "busy";
            }
            registered.push(id);
            // Ctrl+Pause chega como VK_CANCEL (Ctrl+Break).
            if hk.vk == hotkey::VK_PAUSE && hk.modifiers & hotkey::MOD_CTRL != 0
                && ui::register_global_hotkey(alt, hk.modifiers, hotkey::VK_CANCEL)
            {
                registered.push(alt);
            }
            ""
        })();
        status.push(st);
    }
    log!("Atalhos: {} de {} registrado(s).", status.iter().filter(|s| s.is_empty()).count(), list.len());
    REGISTERED.with(|r| *r.borrow_mut() = registered);
    STATUS.with(|s| *s.borrow_mut() = status);
}

pub fn push_state() {
    // A situação sai do registro: sem um `apply` ainda (tela pediu antes do
    // init), faz agora.
    if STATUS.with(|s| s.borrow().len()) != list().len() {
        apply();
    }
    let items = hotkeys::list_to_json(&list());
    let status: Vec<&str> = STATUS.with(|s| s.borrow().clone());
    post_json(json!({
        "type": "hotkeys_state",
        "items": items,
        "status": status,
        "delayMax": hotkeys::DELAY_MAX,
        "defining": DEFINING.with(Cell::get),
    }));
}

/// `set_hotkeys`: a tela manda a lista inteira a cada mudança.
pub fn handle_set_hotkeys(items: &Value) {
    set_list(hotkeys::list_from_json(items));
    apply();
    push_state();
    // A dica do botão "Salvar trecho" mostra o atalho.
    replay::push_replay_state();
}

/// `reset_hotkeys` ("Restaurar padrões"): gravar e salvar trecho.
pub fn handle_reset() {
    handle_set_hotkeys(&hotkeys::list_to_json(&hotkeys::defaults()));
}

/// `define_hotkey_area`: a seleção de sempre, mas só GUARDA o retângulo no item.
pub fn handle_define_area(index: usize) {
    if area_select::is_active() || countdown::active() || list().len() <= index {
        return;
    }
    DEFINING.with(|d| d.set(Some(index)));
    if !area_select::start_with_hint(on_area_defined, "areaSelect.hintSave") {
        DEFINING.with(|d| d.set(None));
    }
    push_state();
}

fn on_area_defined(area: Option<AreaRect>) {
    let index = DEFINING.with(|d| d.take());
    if let (Some(i), Some(a)) = (index, area) {
        let mut l = list();
        if let Some(it) = l.get_mut(i).filter(|it| it.mode == Mode::Area) {
            it.area = Some(a);
            set_list(l);
        }
        log!("Atalhos: area do item {i} = {}x{} em ({}, {}).", a.w, a.h, a.x, a.y);
    }
    // Cancelada também responde: a tela sai do "escolhendo".
    push_state();
}

/// `WM_HOTKEY`. `true` = o id era da lista.
pub fn on_hotkey(id: i32) -> bool {
    if !REGISTERED.with(|r| r.borrow().contains(&id)) {
        return false;
    }
    if let Some(i) = hotkeys::index_of_id(id) {
        trigger(i);
    }
    true
}

/// Executa o item `index` (atalho ou o seletor).
pub fn trigger(index: usize) {
    let Some(it) = list().get(index).cloned() else {
        log!("Atalhos: item {index} nao existe.");
        return;
    };
    log!("Atalhos: \"{}\" ({}).", it.hotkey, it.mode.as_str());
    if it.mode == Mode::Replay {
        replay::handle_save_replay();
        return;
    }
    // Atalho de gravação: na contagem cancela, gravando para.
    if countdown::cancel() {
        return;
    }
    if recording::recording_active() {
        recording::handle_record_stop();
        return;
    }
    if area_select::is_active() {
        return;
    }
    match it.mode {
        Mode::Record => countdown::start(countdown::Target::Screens, it.delay_sec),
        Mode::Window => {
            if it.title.trim().is_empty() {
                post_error(lang::t("hotkeys.noTitle"));
                return;
            }
            window_record::record_by_title(it.title.clone(), it.app_audio, it.delay_sec);
        }
        Mode::Area => match it.area {
            Some(saved) => match area::clamp_to_screen(saved) {
                Some(a) => countdown::start(countdown::Target::Area(a), it.delay_sec),
                None => {
                    log!("Atalhos: area {}x{} em ({}, {}) fora da tela atual.", saved.w, saved.h, saved.x, saved.y);
                    post_error(lang::t("error.areaOffScreen"));
                }
            },
            None => {
                SELECT_DELAY.with(|d| d.set(it.delay_sec));
                if !area_select::start(on_area_selected) {
                    post_json(json!({ "type": "capture_area_canceled" }));
                }
            }
        },
        Mode::Replay => {}
    }
}

fn on_area_selected(area: Option<AreaRect>) {
    let delay = SELECT_DELAY.with(|d| d.take());
    match area {
        Some(a) => countdown::start(countdown::Target::Area(a), delay),
        None => post_json(json!({ "type": "capture_area_canceled" })),
    }
}

pub fn do_init() {
    LOADED.with(|l| l.set(false));
    apply();
    push_state();
}

pub fn shutdown() {
    unregister_all();
}

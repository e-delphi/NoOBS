//! O seletor "Capturar..." (janela ou área), uma camada por cima da tela
//! principal (`pnlPicker` no `forms/main.json`): abas Janela/Área, busca, a
//! grade das janelas abertas com miniatura, o som do app, as áreas salvas e o
//! timelapse. Aberto por um atalho de janela que achou várias, mostra só elas
//! (sem abas) e o atraso do atalho conta depois da escolha.

use std::cell::RefCell;
use std::collections::HashMap;

use iron::prelude::*;

use crate::app_view::{main_form, with_ui};
use crate::view::{WindowItem, WindowMatches};
use crate::{bridge, config, lang};

#[derive(Default)]
struct Picker {
    open: bool,
    area: bool,
    items: Vec<WindowItem>,
    thumbs: HashMap<String, Image>,
    /// A miniatura de cada cartão na grade (id da janela → `ImageView`).
    cards: HashMap<String, ComponentId>,
    seq: u32,
    search: String,
    loading: bool,
    audio_supported: bool,
    /// Aberto por um atalho: (trecho do título, atraso).
    matches: Option<(String, u32)>,
}

thread_local! {
    static P: RefCell<Picker> = RefCell::new(Picker::default());
}

fn with<R>(f: impl FnOnce(&mut Picker) -> R) -> R {
    P.with(|p| f(&mut p.borrow_mut()))
}

/// Um componente do `frmPicker` pelo nome.
fn c(ui: &Ui, name: &str) -> Option<ComponentId> {
    ui.find_in(crate::app_view::form(ui, "frmPicker")?, name)
}

fn set(ui: &mut Ui, name: &str, prop: &str, value: impl Into<PropValue>) {
    if let Some(id) = c(ui, name) {
        let _ = ui.set_prop(id, prop, value);
    }
}

/// Largura de um cartão: três por linha.
const CARD_W: f32 = 205.0;
const CARD_THUMB_H: f32 = CARD_W * 10.0 / 16.0;
const CARD_H: f32 = CARD_THUMB_H + 6.0 + 16.0 + 6.0 + 14.0 + 16.0;

fn subtitle(p: &Picker) -> String {
    if let Some((pattern, _)) = &p.matches {
        lang::t_with("windowPicker.matchesSubtitle", &[("title", pattern)])
    } else if p.area {
        lang::t("windowPicker.areaSubtitle")
    } else {
        lang::t("windowPicker.subtitle")
    }
}

fn apply_texts(ui: &mut Ui) {
    for (name, key) in [
        ("lblPickerTitle", "windowPicker.title"),
        ("chipAppAudio", "windowPicker.appAudio"),
        ("chipTimelapse", "windowPicker.timelapse"),
        ("chipTimelapseCursor", "windowPicker.timelapseCursor"),
        ("btnSelectArea", "windowPicker.areaStart"),
        ("lblAreaStep1", "windowPicker.areaSteps.0"),
        ("lblAreaStep2", "windowPicker.areaSteps.1"),
    ] {
        set(ui, name, "text", lang::t(key));
    }
    set(ui, "edPickerSearch", "placeholder", lang::t("windowPicker.search"));
    set(ui, "segPickerMode", "items", format!("{};{}", lang::t("windowPicker.modeWindow"), lang::t("windowPicker.modeArea")));
    for (name, key) in [
        ("btnPickerRefresh", "windowPicker.refresh"),
        ("btnPickerClose", "windowPicker.close"),
        ("chipAppAudio", "windowPicker.appAudioHint"),
        ("chipTimelapse", "windowPicker.timelapseHint"),
        ("chipTimelapseCursor", "windowPicker.timelapseCursorHint"),
    ] {
        set(ui, name, "hint", lang::t(key));
    }
}

/// Grade filtrada pela busca (título ou exe), o texto de vazio, as abas e as
/// áreas salvas.
fn render(ui: &mut Ui) {
    let (area, matches, loading, supported, sub) = with(|p| (p.area, p.matches.is_some(), p.loading, p.audio_supported, subtitle(p)));
    set(ui, "lblPickerSubtitle", "text", sub);
    set(ui, "lyPickerMode", "visible", !matches);
    set(ui, "btnPickerRefresh", "visible", !area && !matches);
    set(ui, "segPickerMode", "item_index", if area { 1i64 } else { 0 });
    set(ui, "lyPickerWindow", "visible", !area);
    set(ui, "lyPickerArea", "visible", area);
    set(ui, "chipAppAudio", "visible", supported);
    fit_dialog(ui);
    if area {
        render_saved_areas(ui);
        ui.layout();
        return;
    }
    let cards: Vec<WindowItem> = with(|p| {
        let q = p.search.trim().to_lowercase();
        p.items.iter().filter(|it| q.is_empty() || it.title.to_lowercase().contains(&q) || it.exe.to_lowercase().contains(&q)).cloned().collect()
    });
    let empty = if loading {
        Some(lang::t("windowPicker.loading") + "...")
    } else if cards.is_empty() {
        Some(lang::t(if with(|p| p.items.is_empty()) { "windowPicker.empty" } else { "windowPicker.noMatch" }))
    } else {
        None
    };
    set(ui, "lblPickerEmpty", "visible", empty.is_some());
    set(ui, "lblPickerEmpty", "text", empty.unwrap_or_default());
    let Some(grid) = c(ui, "flowPickerGrid") else { return };
    for child in ui.children(grid).to_vec() {
        ui.free(child);
    }
    let mut views = HashMap::new();
    for it in &cards {
        if let Some(view) = add_card(ui, grid, it) {
            views.insert(it.id.clone(), view);
        }
    }
    let rows = cards.len().div_ceil(3).max(1);
    if let Some(cb) = ui.control_mut(grid) {
        cb.set_height(rows as f32 * (CARD_H + 12.0));
    }
    with(|p| p.cards = views);
    ui.layout();
}

/// A altura do diálogo: a do conteúdo (a aba Área é curta; a Janela cresce com
/// a grade), no máximo a da janela menos 48 px.
fn fit_dialog(ui: &mut Ui) {
    let (area, matches, count) = with(|p| (p.area, p.matches.is_some(), p.items.len()));
    let header = 52.0 + 23.0 + if matches { 0.0 } else { 48.0 };
    let content = if area {
        36.0 + 42.0 + 46.0 + 52.0 + if bridge::saved_areas().len() > 2 { 44.0 } else { 0.0 }
    } else {
        let rows = count.div_ceil(3).max(1) as f32;
        44.0 + rows * (CARD_H + 12.0)
    };
    let max = main_form().and_then(|f| ui.bounds(f)).map_or(800.0, |b| b.height - 48.0);
    let h = (16.0 + header + content + 20.0).min(max).max(260.0);
    if let Some(cb) = c(ui, "pnlPickerDialog").and_then(|d| ui.control_mut(d)) {
        cb.set_height(h);
    }
}

/// Um cartão de janela: a miniatura, o título e "exe · 1920×1080". O clique grava.
fn add_card(ui: &mut Ui, grid: ComponentId, it: &WindowItem) -> Option<ComponentId> {
    let card = ui.create("Panel", Some(grid)).ok()?;
    let _ = ui.set_prop(card, "style", "wcard");
    if let Some(cb) = ui.control_mut(card) {
        cb.set_rect(0.0, 0.0, CARD_W, CARD_H).set_padding([8.0, 8.0, 8.0, 8.0]).set_cursor(CursorShape::Pointer);
    }
    let _ = ui.set_prop(card, "hint", lang::t("windowPicker.pick"));
    let id = it.id.clone();
    ui.on(card, "on_click", move |ui, _, _| pick(ui, &id));
    let view = ui.create("Image", Some(card)).ok()?;
    let _ = ui.set_prop(view, "fit", "contain");
    let _ = ui.set_prop(view, "radius", 6.0);
    let _ = ui.set_prop(view, "fill", "thumb_bg");
    if let Some(cb) = ui.control_mut(view) {
        cb.set_align(Align::Top).set_height(CARD_THUMB_H - 8.0).set_hit_test(false);
    }
    if let Some(image) = with(|p| p.thumbs.get(&it.id).cloned()) {
        if let Some(v) = ui.get_mut::<ImageView>(view) {
            v.set_picture(Some(image));
        }
    }
    let size = if it.w > 0 && it.h > 0 { format!(" · {}×{}", it.w, it.h) } else { String::new() };
    for (text, style, h, top) in [(it.title.clone(), "row", 16.0, 6.0), (format!("{}{size}", it.exe), "sub", 14.0, 2.0)] {
        if let Ok(label) = ui.create("Label", Some(card)) {
            let _ = ui.set_prop(label, "style", style);
            let _ = ui.set_prop(label, "text", text);
            if let Some(cb) = ui.control_mut(label) {
                cb.set_align(Align::Top).set_height(h).set_margins([0.0, top, 0.0, 0.0]).set_hit_test(false);
            }
        }
    }
    Some(view)
}

/// Os botões das áreas salvas (atalhos de área com retângulo), depois do
/// "Selecionar área".
fn render_saved_areas(ui: &mut Ui) {
    let Some(row) = c(ui, "lyAreaButtons") else { return };
    let first = c(ui, "btnSelectArea");
    for child in ui.children(row).to_vec() {
        if Some(child) != first {
            ui.free(child);
        }
    }
    let mut width = 170.0;
    for (i, aw, ah, hk) in bridge::saved_areas() {
        let mut label = lang::t_with("windowPicker.savedArea", &[("w", &aw.to_string()), ("h", &ah.to_string())]);
        if !hk.is_empty() {
            label = format!("{label} · {hk}");
        }
        let Ok(b) = ui.create("Button", Some(row)) else { continue };
        let w = ui.measure_text(&label, &TextStyle::default()).width + 40.0;
        let _ = ui.set_prop(b, "style", "outline");
        let _ = ui.set_prop(b, "text", label);
        if let Some(cb) = ui.control_mut(b) {
            cb.set_rect(0.0, 0.0, w, 34.0);
        }
        width += w + 8.0;
        ui.on(b, "on_click", move |ui, _, _| {
            close(ui);
            bridge::trigger_hotkey(i);
        });
    }
    if let Some(cb) = ui.control_mut(row) {
        cb.set_height(if width > 620.0 { 80.0 } else { 36.0 });
    }
}

fn show(ui: &mut Ui) {
    crate::app_view::show_screen(ui, "frmPicker", true);
    crate::app_view::set_main(ui, "btnRecord", "glow", false);
    set(ui, "edPickerSearch", "text", "");
    set(ui, "chipTimelapse", "checked", config::get_config_bool("areaTimelapse", false));
    set(ui, "chipTimelapseCursor", "checked", config::get_config_bool("areaTimelapseCursor", true));
    set(ui, "chipTimelapseCursor", "enabled", config::get_config_bool("areaTimelapse", false));
    apply_texts(ui);
    if let Some(edit) = c(ui, "edPickerSearch") {
        ui.set_focus(edit);
    }
}

/// O botão "Capturar..." do cartão.
pub fn open(ui: &mut Ui, recording: bool) {
    if recording {
        return;
    }
    let area = config::get_config_str("windowPickerMode", "window") == "area";
    with(|p| {
        let (seq, supported) = (p.seq, p.audio_supported);
        *p = Picker { seq, audio_supported: supported, open: true, ..Default::default() };
    });
    set(ui, "chipAppAudio", "checked", config::get_config_bool("windowPickerAppAudio", false));
    show(ui);
    set_mode(ui, area, false);
}

/// Troca de aba. A lista de janelas só é pedida quando a aba Janela aparece
/// (abrir direto na Área não gasta o PrintWindow de todas as janelas à toa).
fn set_mode(ui: &mut Ui, area: bool, remember: bool) {
    let matches = with(|p| {
        p.area = area;
        p.matches.is_some()
    });
    if remember && !matches {
        config::set_config_str("windowPickerMode", if area { "area" } else { "window" });
    }
    let need_list = with(|p| !area && p.items.is_empty() && !p.loading && p.matches.is_none());
    if need_list {
        refresh(ui);
    } else {
        render(ui);
    }
}

pub fn refresh(ui: &mut Ui) {
    with(|p| {
        p.loading = true;
        p.items.clear();
        p.thumbs.clear();
    });
    render(ui);
    bridge::handle_list_capture_windows();
}

pub fn is_open() -> bool {
    with(|p| p.open)
}

pub fn close(ui: &mut Ui) {
    with(|p| {
        p.open = false;
        p.matches = None;
    });
    crate::app_view::show_screen(ui, "frmPicker", false);
    crate::app_view::set_main(ui, "btnRecord", "glow", true);
}

fn pick(ui: &mut Ui, id: &str) {
    if id.is_empty() {
        return;
    }
    let (delay, supported) = with(|p| (p.matches.as_ref().map_or(0, |m| m.1), p.audio_supported));
    close(ui);
    crate::app_main::start_ripple(ui);
    let app_audio = supported && config::get_config_bool("windowPickerAppAudio", false);
    bridge::handle_record_window(id, app_audio, delay);
}

pub fn on_capture_windows(seq: u32, items: &[WindowItem], app_audio: bool) {
    let items = items.to_vec();
    with_ui(move |ui| {
        with(|p| {
            p.seq = seq;
            p.items = items;
            p.thumbs.clear();
            p.loading = false;
            p.audio_supported = app_audio;
        });
        if with(|p| p.open) {
            render(ui);
        }
    });
}

pub fn on_capture_window_thumb(seq: u32, id: &str, width: u32, height: u32, rgb: &[u8]) {
    if rgb.len() != (width * height * 3) as usize {
        return;
    }
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    let image = Image::rgba(width, height, rgba);
    let id = id.to_owned();
    with_ui(move |ui| {
        if with(|p| p.seq) != seq {
            return;
        }
        // Só troca a imagem do cartão (refazer a grade faria o hover piscar).
        let view = with(|p| {
            p.thumbs.insert(id.clone(), image.clone());
            p.cards.get(&id).copied()
        });
        if let Some(view) = view.and_then(|v| ui.get_mut::<ImageView>(v)) {
            view.set_picture(Some(image));
        }
        ui.invalidate();
    });
}

/// Atalho de janela com várias janelas casando o título.
pub fn on_window_picker_matches(m: &WindowMatches) {
    let m = m.clone();
    with_ui(move |ui| {
        with(|p| {
            *p = Picker {
                open: true,
                seq: m.seq,
                items: m.items.clone(),
                audio_supported: m.app_audio,
                matches: Some((m.pattern.clone(), m.delay_sec)),
                ..Default::default()
            };
        });
        set(ui, "chipAppAudio", "checked", m.want_app_audio);
        show(ui);
        render(ui);
    });
}

// ----- eventos do form -----

pub fn on_mode(ui: &mut Ui, sender: ComponentId) {
    let area = matches!(ui.get_prop(sender, "item_index"), Ok(PropValue::Int(1)));
    set_mode(ui, area, true);
}

pub fn on_search(ui: &mut Ui, sender: ComponentId) {
    let text = match ui.get_prop(sender, "text") {
        Ok(PropValue::String(t)) => t,
        _ => String::new(),
    };
    with(|p| p.search = text);
    render(ui);
}

fn checked(ui: &Ui, sender: ComponentId) -> bool {
    matches!(ui.get_prop(sender, "checked"), Ok(PropValue::Bool(true)))
}

pub fn on_audio(ui: &mut Ui, sender: ComponentId) {
    config::set_config_bool("windowPickerAppAudio", checked(ui, sender));
}

pub fn on_timelapse(ui: &mut Ui, sender: ComponentId) {
    let on = checked(ui, sender);
    config::set_config_bool("areaTimelapse", on);
    set(ui, "chipTimelapseCursor", "enabled", on);
}

pub fn on_timelapse_cursor(ui: &mut Ui, sender: ComponentId) {
    config::set_config_bool("areaTimelapseCursor", checked(ui, sender));
}

pub fn on_select_area(ui: &mut Ui) {
    close(ui);
    bridge::handle_select_capture_area();
}

//! Os componentes dos forms com as propriedades tipadas (frmX.componente.Propriedade).
//! Gerado pela IDE do iron a cada "Gerar código": não edite.

#![allow(non_snake_case, non_upper_case_globals, dead_code)]

use iron::prelude::{ComponentId, PropValue, Ui};

fn get(ui: &Ui, target: &str, prop: &str) -> Option<PropValue> {
    ui.get_prop(ui.find_path(target)?, prop).ok()
}

fn invoke(ui: &mut Ui, target: &str, method: &str, args: Vec<PropValue>) -> Option<PropValue> {
    let id = ui.find_path(target)?;
    ui.invoke(id, method, &args).ok()
}

fn set(ui: &mut Ui, target: &str, prop: &str, value: impl Into<PropValue>) {
    if let Some(id) = ui.find_path(target) {
        let _ = ui.set_prop(id, prop, value);
    }
}

fn text(ui: &Ui, value: Option<PropValue>) -> String {
    match value {
        Some(PropValue::Color(c)) => c.to_hex(),
        Some(PropValue::Component(Some(id))) => ui.path_of(id),
        Some(v) => v.to_str().map(str::to_string).unwrap_or_default(),
        None => String::new(),
    }
}

/// Um Breadcrumb de um form.
pub struct Breadcrumb {
    pub target: &'static str,
}

impl Breadcrumb {
    pub fn items(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "items"))
    }
    pub fn set_items(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "items", Into::<String>::into(value))
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um Button de um form.
pub struct Button {
    pub target: &'static str,
}

impl Button {
    pub fn text(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text"))
    }
    pub fn set_text(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text", Into::<String>::into(value))
    }
    pub fn icon(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "icon"))
    }
    pub fn set_icon(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "icon", Into::<String>::into(value))
    }
    pub fn checked_icon(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "checked_icon"))
    }
    pub fn set_checked_icon(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "checked_icon", Into::<String>::into(value))
    }
    pub fn icon_position(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "icon_position"))
    }
    pub fn set_icon_position(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "icon_position", Into::<String>::into(value))
    }
    pub fn icon_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "icon_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_icon_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "icon_size", value)
    }
    pub fn checkable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "checkable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_checkable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "checkable", value)
    }
    pub fn checked(&self, ui: &Ui) -> bool {
        get(ui, self.target, "checked").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_checked(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "checked", value)
    }
    pub fn modal_result(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "modal_result"))
    }
    pub fn set_modal_result(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "modal_result", Into::<String>::into(value))
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
    pub fn set_focus(&self, ui: &mut Ui) -> bool {
        ui.find_path(self.target).is_some_and(|id| ui.set_focus(id))
    }
}

/// Um Chip de um form.
pub struct Chip {
    pub target: &'static str,
}

impl Chip {
    pub fn text(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text"))
    }
    pub fn set_text(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text", Into::<String>::into(value))
    }
    pub fn icon(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "icon"))
    }
    pub fn set_icon(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "icon", Into::<String>::into(value))
    }
    pub fn closable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "closable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_closable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "closable", value)
    }
    pub fn checkable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "checkable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_checkable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "checkable", value)
    }
    pub fn checked(&self, ui: &Ui) -> bool {
        get(ui, self.target, "checked").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_checked(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "checked", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
    pub fn set_focus(&self, ui: &mut Ui) -> bool {
        ui.find_path(self.target).is_some_and(|id| ui.set_focus(id))
    }
}

/// Um Circle de um form.
pub struct Circle {
    pub target: &'static str,
}

impl Circle {
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn stroke(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "stroke"))
    }
    pub fn set_stroke(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "stroke", Into::<String>::into(value))
    }
    pub fn stroke_width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "stroke_width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_stroke_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "stroke_width", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um Edit de um form.
pub struct Edit {
    pub target: &'static str,
}

impl Edit {
    pub fn text(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text"))
    }
    pub fn set_text(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text", Into::<String>::into(value))
    }
    pub fn placeholder(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "placeholder"))
    }
    pub fn set_placeholder(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "placeholder", Into::<String>::into(value))
    }
    pub fn border(&self, ui: &Ui) -> bool {
        get(ui, self.target, "border").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_border(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "border", value)
    }
    pub fn max_length(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "max_length").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_max_length(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "max_length", value)
    }
    pub fn allowed_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "allowed_chars"))
    }
    pub fn set_allowed_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "allowed_chars", Into::<String>::into(value))
    }
    pub fn blocked_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "blocked_chars"))
    }
    pub fn set_blocked_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "blocked_chars", Into::<String>::into(value))
    }
    pub fn only_allowed(&self, ui: &Ui) -> bool {
        get(ui, self.target, "only_allowed").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_only_allowed(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "only_allowed", value)
    }
    pub fn read_only(&self, ui: &Ui) -> bool {
        get(ui, self.target, "read_only").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_read_only(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "read_only", value)
    }
    pub fn password(&self, ui: &Ui) -> bool {
        get(ui, self.target, "password").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_password(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "password", value)
    }
    pub fn h_align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "h_align"))
    }
    pub fn set_h_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "h_align", Into::<String>::into(value))
    }
    pub fn v_align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "v_align"))
    }
    pub fn set_v_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "v_align", Into::<String>::into(value))
    }
    pub fn wrap(&self, ui: &Ui) -> bool {
        get(ui, self.target, "wrap").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_wrap(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "wrap", value)
    }
    pub fn trimming(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "trimming"))
    }
    pub fn set_trimming(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "trimming", Into::<String>::into(value))
    }
    pub fn icon(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "icon"))
    }
    pub fn set_icon(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "icon", Into::<String>::into(value))
    }
    pub fn clear_button(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clear_button").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clear_button(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clear_button", value)
    }
    pub fn label(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "label"))
    }
    pub fn set_label(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "label", Into::<String>::into(value))
    }
    pub fn helper(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "helper"))
    }
    pub fn set_helper(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "helper", Into::<String>::into(value))
    }
    pub fn error(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "error"))
    }
    pub fn set_error(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "error", Into::<String>::into(value))
    }
    pub fn counter(&self, ui: &Ui) -> bool {
        get(ui, self.target, "counter").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_counter(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "counter", value)
    }
    pub fn prefix(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "prefix"))
    }
    pub fn set_prefix(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "prefix", Into::<String>::into(value))
    }
    pub fn suffix(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "suffix"))
    }
    pub fn set_suffix(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "suffix", Into::<String>::into(value))
    }
    pub fn mask(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "mask"))
    }
    pub fn set_mask(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "mask", Into::<String>::into(value))
    }
    pub fn rules(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "rules"))
    }
    pub fn set_rules(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "rules", Into::<String>::into(value))
    }
    pub fn spellcheck(&self, ui: &Ui) -> bool {
        get(ui, self.target, "spellcheck").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_spellcheck(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "spellcheck", value)
    }
    pub fn autocomplete(&self, ui: &Ui) -> bool {
        get(ui, self.target, "autocomplete").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_autocomplete(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "autocomplete", value)
    }
    pub fn language(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "language"))
    }
    pub fn set_language(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "language", Into::<String>::into(value))
    }
    pub fn keyboard_suggestions(&self, ui: &Ui) -> bool {
        get(ui, self.target, "keyboard_suggestions").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_keyboard_suggestions(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "keyboard_suggestions", value)
    }
    pub fn caret_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "caret_color"))
    }
    pub fn set_caret_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "caret_color", Into::<String>::into(value))
    }
    pub fn caret_width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "caret_width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_caret_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "caret_width", value)
    }
    pub fn caret_style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "caret_style"))
    }
    pub fn set_caret_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "caret_style", Into::<String>::into(value))
    }
    pub fn caret_blink(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "caret_blink").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_caret_blink(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "caret_blink", value)
    }
    pub fn sel_start(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "sel_start").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_sel_start(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "sel_start", value)
    }
    pub fn sel_length(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "sel_length").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_sel_length(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "sel_length", value)
    }
    pub fn sel_text(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "sel_text"))
    }
    pub fn set_sel_text(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "sel_text", Into::<String>::into(value))
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
    pub fn set_focus(&self, ui: &mut Ui) -> bool {
        ui.find_path(self.target).is_some_and(|id| ui.set_focus(id))
    }
}

/// Um FlowLayout de um form.
pub struct FlowLayout {
    pub target: &'static str,
}

impl FlowLayout {
    pub fn gap(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "gap").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_gap(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "gap", value)
    }
    pub fn line_gap(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "line_gap").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_line_gap(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "line_gap", value)
    }
    pub fn h_align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "h_align"))
    }
    pub fn set_h_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "h_align", Into::<String>::into(value))
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um Form de um form.
pub struct Form {
    pub target: &'static str,
}

impl Form {
    pub fn caption(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "caption"))
    }
    pub fn set_caption(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "caption", Into::<String>::into(value))
    }
    pub fn position(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "position"))
    }
    pub fn set_position(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "position", Into::<String>::into(value))
    }
    pub fn border_style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_style"))
    }
    pub fn set_border_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_style", Into::<String>::into(value))
    }
    pub fn transition(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "transition"))
    }
    pub fn set_transition(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "transition", Into::<String>::into(value))
    }
    pub fn topmost(&self, ui: &Ui) -> bool {
        get(ui, self.target, "topmost").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_topmost(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "topmost", value)
    }
    pub fn show_in_taskbar(&self, ui: &Ui) -> bool {
        get(ui, self.target, "show_in_taskbar").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_show_in_taskbar(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "show_in_taskbar", value)
    }
    pub fn active_control(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "active_control"))
    }
    pub fn set_active_control(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "active_control", Into::<String>::into(value))
    }
    pub fn tab_order(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "tab_order"))
    }
    pub fn set_tab_order(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "tab_order", Into::<String>::into(value))
    }
    pub fn tab_keys(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "tab_keys"))
    }
    pub fn set_tab_keys(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "tab_keys", Into::<String>::into(value))
    }
    pub fn back_tab_keys(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "back_tab_keys"))
    }
    pub fn set_back_tab_keys(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "back_tab_keys", Into::<String>::into(value))
    }
    pub fn allowed_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "allowed_chars"))
    }
    pub fn set_allowed_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "allowed_chars", Into::<String>::into(value))
    }
    pub fn blocked_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "blocked_chars"))
    }
    pub fn set_blocked_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "blocked_chars", Into::<String>::into(value))
    }
    pub fn only_allowed(&self, ui: &Ui) -> bool {
        get(ui, self.target, "only_allowed").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_only_allowed(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "only_allowed", value)
    }
    pub fn chars_override(&self, ui: &Ui) -> bool {
        get(ui, self.target, "chars_override").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_chars_override(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "chars_override", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
    pub fn show(&self, ui: &mut Ui) {
        if let Some(id) = ui.find_path(self.target) {
            let _ = ui.show(id);
        }
    }
    pub fn close(&self, ui: &mut Ui) -> bool {
        ui.find_path(self.target).is_some_and(|id| ui.close(id))
    }
    /// O nome do controle com foco (vazio sem foco), como o `ActiveControl`.
    pub fn focused(&self, ui: &Ui) -> String {
        let Some(form) = ui.find_path(self.target) else { return String::new() };
        ui.focused(ui.root_of(form)).and_then(|f| ui.name(f)).unwrap_or_default().to_string()
    }
    pub fn focus_next(&self, ui: &mut Ui) -> bool {
        let Some(form) = ui.find_path(self.target) else { return false };
        let root = ui.root_of(form);
        ui.focus_next(root, true)
    }
    pub fn focus_prev(&self, ui: &mut Ui) -> bool {
        let Some(form) = ui.find_path(self.target) else { return false };
        let root = ui.root_of(form);
        ui.focus_next(root, false)
    }
}

/// Um FrameView de um form.
pub struct FrameView {
    pub target: &'static str,
}

impl FrameView {
    pub fn background(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "background"))
    }
    pub fn set_background(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "background", Into::<String>::into(value))
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um Label de um form.
pub struct Label {
    pub target: &'static str,
}

impl Label {
    pub fn text(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text"))
    }
    pub fn set_text(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text", Into::<String>::into(value))
    }
    pub fn h_align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "h_align"))
    }
    pub fn set_h_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "h_align", Into::<String>::into(value))
    }
    pub fn v_align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "v_align"))
    }
    pub fn set_v_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "v_align", Into::<String>::into(value))
    }
    pub fn wrap(&self, ui: &Ui) -> bool {
        get(ui, self.target, "wrap").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_wrap(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "wrap", value)
    }
    pub fn trimming(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "trimming"))
    }
    pub fn set_trimming(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "trimming", Into::<String>::into(value))
    }
    pub fn bold(&self, ui: &Ui) -> bool {
        get(ui, self.target, "bold").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_bold(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "bold", value)
    }
    pub fn color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "color"))
    }
    pub fn set_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "color", Into::<String>::into(value))
    }
    pub fn highlight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "highlight"))
    }
    pub fn set_highlight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "highlight", Into::<String>::into(value))
    }
    pub fn letter_spacing(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "letter_spacing").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_letter_spacing(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "letter_spacing", value)
    }
    pub fn text_case(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_case"))
    }
    pub fn set_text_case(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_case", Into::<String>::into(value))
    }
    pub fn focus_control(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "focus_control"))
    }
    pub fn set_focus_control(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "focus_control", Into::<String>::into(value))
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um Layout de um form.
pub struct Layout {
    pub target: &'static str,
}

impl Layout {
    pub fn allowed_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "allowed_chars"))
    }
    pub fn set_allowed_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "allowed_chars", Into::<String>::into(value))
    }
    pub fn blocked_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "blocked_chars"))
    }
    pub fn set_blocked_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "blocked_chars", Into::<String>::into(value))
    }
    pub fn only_allowed(&self, ui: &Ui) -> bool {
        get(ui, self.target, "only_allowed").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_only_allowed(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "only_allowed", value)
    }
    pub fn chars_override(&self, ui: &Ui) -> bool {
        get(ui, self.target, "chars_override").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_chars_override(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "chars_override", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um Panel de um form.
pub struct Panel {
    pub target: &'static str,
}

impl Panel {
    pub fn allowed_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "allowed_chars"))
    }
    pub fn set_allowed_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "allowed_chars", Into::<String>::into(value))
    }
    pub fn blocked_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "blocked_chars"))
    }
    pub fn set_blocked_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "blocked_chars", Into::<String>::into(value))
    }
    pub fn only_allowed(&self, ui: &Ui) -> bool {
        get(ui, self.target, "only_allowed").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_only_allowed(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "only_allowed", value)
    }
    pub fn chars_override(&self, ui: &Ui) -> bool {
        get(ui, self.target, "chars_override").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_chars_override(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "chars_override", value)
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn fill2(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill2"))
    }
    pub fn set_fill2(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill2", Into::<String>::into(value))
    }
    pub fn gradient_angle(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "gradient_angle").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_gradient_angle(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "gradient_angle", value)
    }
    pub fn stroke(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "stroke"))
    }
    pub fn set_stroke(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "stroke", Into::<String>::into(value))
    }
    pub fn stroke_width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "stroke_width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_stroke_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "stroke_width", value)
    }
    pub fn radius(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "radius").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_radius(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "radius", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um RecordButton de um form.
pub struct RecordButton {
    pub target: &'static str,
}

impl RecordButton {
    pub fn recording(&self, ui: &Ui) -> bool {
        get(ui, self.target, "recording").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_recording(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "recording", value)
    }
    pub fn paused(&self, ui: &Ui) -> bool {
        get(ui, self.target, "paused").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_paused(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "paused", value)
    }
    pub fn finalizing(&self, ui: &Ui) -> bool {
        get(ui, self.target, "finalizing").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_finalizing(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "finalizing", value)
    }
    pub fn countdown(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "countdown").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_countdown(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "countdown", value)
    }
    pub fn countdown_fraction(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "countdown_fraction").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_countdown_fraction(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "countdown_fraction", value)
    }
    pub fn glow(&self, ui: &Ui) -> bool {
        get(ui, self.target, "glow").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_glow(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "glow", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um Rectangle de um form.
pub struct Rectangle {
    pub target: &'static str,
}

impl Rectangle {
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn fill2(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill2"))
    }
    pub fn set_fill2(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill2", Into::<String>::into(value))
    }
    pub fn gradient_angle(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "gradient_angle").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_gradient_angle(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "gradient_angle", value)
    }
    pub fn stroke(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "stroke"))
    }
    pub fn set_stroke(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "stroke", Into::<String>::into(value))
    }
    pub fn stroke_width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "stroke_width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_stroke_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "stroke_width", value)
    }
    pub fn radius(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "radius").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_radius(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "radius", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um ScreenLayout de um form.
pub struct ScreenLayout {
    pub target: &'static str,
}

impl ScreenLayout {
    pub fn locked(&self, ui: &Ui) -> bool {
        get(ui, self.target, "locked").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_locked(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "locked", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um ScrollBox de um form.
pub struct ScrollBox {
    pub target: &'static str,
}

impl ScrollBox {
    pub fn horizontal(&self, ui: &Ui) -> bool {
        get(ui, self.target, "horizontal").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_horizontal(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "horizontal", value)
    }
    pub fn pull_to_refresh(&self, ui: &Ui) -> bool {
        get(ui, self.target, "pull_to_refresh").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_pull_to_refresh(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "pull_to_refresh", value)
    }
    pub fn refreshing(&self, ui: &Ui) -> bool {
        get(ui, self.target, "refreshing").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_refreshing(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "refreshing", value)
    }
    pub fn allowed_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "allowed_chars"))
    }
    pub fn set_allowed_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "allowed_chars", Into::<String>::into(value))
    }
    pub fn blocked_chars(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "blocked_chars"))
    }
    pub fn set_blocked_chars(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "blocked_chars", Into::<String>::into(value))
    }
    pub fn only_allowed(&self, ui: &Ui) -> bool {
        get(ui, self.target, "only_allowed").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_only_allowed(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "only_allowed", value)
    }
    pub fn chars_override(&self, ui: &Ui) -> bool {
        get(ui, self.target, "chars_override").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_chars_override(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "chars_override", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

/// Um SegmentedControl de um form.
pub struct SegmentedControl {
    pub target: &'static str,
}

impl SegmentedControl {
    pub fn items(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "items"))
    }
    pub fn set_items(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "items", Into::<String>::into(value))
    }
    pub fn icons(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "icons"))
    }
    pub fn set_icons(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "icons", Into::<String>::into(value))
    }
    pub fn item_index(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "item_index").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_item_index(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "item_index", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
    pub fn set_focus(&self, ui: &mut Ui) -> bool {
        ui.find_path(self.target).is_some_and(|id| ui.set_focus(id))
    }
}

/// Um Switch de um form.
pub struct Switch {
    pub target: &'static str,
}

impl Switch {
    pub fn text(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text"))
    }
    pub fn set_text(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text", Into::<String>::into(value))
    }
    pub fn checked(&self, ui: &Ui) -> bool {
        get(ui, self.target, "checked").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_checked(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "checked", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
    pub fn set_focus(&self, ui: &mut Ui) -> bool {
        ui.find_path(self.target).is_some_and(|id| ui.set_focus(id))
    }
}

/// Um TrackBar de um form.
pub struct TrackBar {
    pub target: &'static str,
}

impl TrackBar {
    pub fn value(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "value").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_value(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "value", value)
    }
    pub fn min(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "min").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_min(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "min", value)
    }
    pub fn max(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "max").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_max(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "max", value)
    }
    pub fn frequency(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "frequency").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_frequency(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "frequency", value)
    }
    pub fn vertical(&self, ui: &Ui) -> bool {
        get(ui, self.target, "vertical").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_vertical(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "vertical", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
    pub fn set_focus(&self, ui: &mut Ui) -> bool {
        ui.find_path(self.target).is_some_and(|id| ui.set_focus(id))
    }
}

/// Um Waveform de um form.
pub struct Waveform {
    pub target: &'static str,
}

impl Waveform {
    pub fn position(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "position").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_position(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "position", value)
    }
    pub fn duration(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "duration").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_duration(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "duration", value)
    }
    pub fn left(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "left").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_left(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "left", value)
    }
    pub fn top(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "top").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_top(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "top", value)
    }
    pub fn width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "width", value)
    }
    pub fn height(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "height").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_height(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "height", value)
    }
    pub fn align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "align"))
    }
    pub fn set_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "align", Into::<String>::into(value))
    }
    pub fn visible(&self, ui: &Ui) -> bool {
        get(ui, self.target, "visible").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_visible(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "visible", value)
    }
    pub fn enabled(&self, ui: &Ui) -> bool {
        get(ui, self.target, "enabled").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_enabled(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "enabled", value)
    }
    pub fn hit_test(&self, ui: &Ui) -> bool {
        get(ui, self.target, "hit_test").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_hit_test(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "hit_test", value)
    }
    pub fn clip_children(&self, ui: &Ui) -> bool {
        get(ui, self.target, "clip_children").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_clip_children(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "clip_children", value)
    }
    pub fn can_focus(&self, ui: &Ui) -> bool {
        get(ui, self.target, "can_focus").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_can_focus(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "can_focus", value)
    }
    pub fn tab_stop(&self, ui: &Ui) -> bool {
        get(ui, self.target, "tab_stop").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_tab_stop(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "tab_stop", value)
    }
    pub fn cursor(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "cursor"))
    }
    pub fn set_cursor(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "cursor", Into::<String>::into(value))
    }
    pub fn opacity(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "opacity").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_opacity(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "opacity", value)
    }
    pub fn rotation(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "rotation").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_rotation(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "rotation", value)
    }
    pub fn translate_x(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_x").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_x(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_x", value)
    }
    pub fn translate_y(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "translate_y").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_translate_y(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "translate_y", value)
    }
    pub fn aspect_ratio(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "aspect_ratio").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_aspect_ratio(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "aspect_ratio", value)
    }
    pub fn shadow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "shadow"))
    }
    pub fn set_shadow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "shadow", Into::<String>::into(value))
    }
    pub fn style(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "style"))
    }
    pub fn set_style(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "style", Into::<String>::into(value))
    }
    pub fn theme(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "theme"))
    }
    pub fn set_theme(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "theme", Into::<String>::into(value))
    }
    pub fn fill(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill"))
    }
    pub fn set_fill(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill", Into::<String>::into(value))
    }
    pub fn border_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "border_color"))
    }
    pub fn set_border_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "border_color", Into::<String>::into(value))
    }
    pub fn text_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "text_color"))
    }
    pub fn set_text_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "text_color", Into::<String>::into(value))
    }
    pub fn accent_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "accent_color"))
    }
    pub fn set_accent_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "accent_color", Into::<String>::into(value))
    }
    pub fn success_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "success_color"))
    }
    pub fn set_success_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "success_color", Into::<String>::into(value))
    }
    pub fn warning_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "warning_color"))
    }
    pub fn set_warning_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "warning_color", Into::<String>::into(value))
    }
    pub fn danger_color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "danger_color"))
    }
    pub fn set_danger_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "danger_color", Into::<String>::into(value))
    }
    pub fn font_family(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_family"))
    }
    pub fn set_font_family(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_family", Into::<String>::into(value))
    }
    pub fn font_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "font_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_font_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "font_size", value)
    }
    pub fn font_weight(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "font_weight"))
    }
    pub fn set_font_weight(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "font_weight", Into::<String>::into(value))
    }
    pub fn font_italic(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_italic").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_italic(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_italic", value)
    }
    pub fn font_underline(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_underline").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_underline(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_underline", value)
    }
    pub fn font_strikeout(&self, ui: &Ui) -> bool {
        get(ui, self.target, "font_strikeout").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_font_strikeout(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "font_strikeout", value)
    }
    pub fn hint(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "hint"))
    }
    pub fn set_hint(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "hint", Into::<String>::into(value))
    }
    pub fn tag(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tag").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tag(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tag", value)
    }
    pub fn popup_menu(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "popup_menu"))
    }
    pub fn set_popup_menu(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "popup_menu", Into::<String>::into(value))
    }
}

impl FrmMainComponents {
    /// O Breadcrumb deste form com o nome (`None` se não há um Breadcrumb com ele).
    pub fn find_breadcrumb(&'static self, name: &str) -> Option<&'static Breadcrumb> {
        match name {
            "brdLib" => Some(&self.brdLib),
            _ => None,
        }
    }
    /// O Breadcrumb que disparou o evento (`None` se o `sender` não é um Breadcrumb deste form).
    pub fn sender_as_breadcrumb(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Breadcrumb> {
        self.find_breadcrumb(ui.name(sender)?)
    }
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnUpdClose" => Some(&self.btnUpdClose),
            "btnUpdOpen" => Some(&self.btnUpdOpen),
            "btnSettings" => Some(&self.btnSettings),
            "btnPause" => Some(&self.btnPause),
            "btnCapture" => Some(&self.btnCapture),
            "btnReplaySave" => Some(&self.btnReplaySave),
            "btnLibOpenDir" => Some(&self.btnLibOpenDir),
            "btnLibNewFolder" => Some(&self.btnLibNewFolder),
            "btnLibMerge" => Some(&self.btnLibMerge),
            "btnLibExport" => Some(&self.btnLibExport),
            "btnLibDelete" => Some(&self.btnLibDelete),
            "btnPickerClose" => Some(&self.btnPickerClose),
            "btnPickerRefresh" => Some(&self.btnPickerRefresh),
            "btnSelectArea" => Some(&self.btnSelectArea),
            "btnSetClose" => Some(&self.btnSetClose),
            "btnSideClose" => Some(&self.btnSideClose),
            "btnPlayerClose" => Some(&self.btnPlayerClose),
            "btnPlay" => Some(&self.btnPlay),
            "btnFullscreen" => Some(&self.btnFullscreen),
            "btnTranscript" => Some(&self.btnTranscript),
            "btnInfo" => Some(&self.btnInfo),
            "btnCaptions" => Some(&self.btnCaptions),
            "btnSpeed" => Some(&self.btnSpeed),
            "btnMute" => Some(&self.btnMute),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O Chip deste form com o nome (`None` se não há um Chip com ele).
    pub fn find_chip(&'static self, name: &str) -> Option<&'static Chip> {
        match name {
            "chipAppAudio" => Some(&self.chipAppAudio),
            "chipTimelapse" => Some(&self.chipTimelapse),
            "chipTimelapseCursor" => Some(&self.chipTimelapseCursor),
            _ => None,
        }
    }
    /// O Chip que disparou o evento (`None` se o `sender` não é um Chip deste form).
    pub fn sender_as_chip(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Chip> {
        self.find_chip(ui.name(sender)?)
    }
    /// O Circle deste form com o nome (`None` se não há um Circle com ele).
    pub fn find_circle(&'static self, name: &str) -> Option<&'static Circle> {
        match name {
            "dotStatus" => Some(&self.dotStatus),
            "dotReplay" => Some(&self.dotReplay),
            _ => None,
        }
    }
    /// O Circle que disparou o evento (`None` se o `sender` não é um Circle deste form).
    pub fn sender_as_circle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Circle> {
        self.find_circle(ui.name(sender)?)
    }
    /// O Edit deste form com o nome (`None` se não há um Edit com ele).
    pub fn find_edit(&'static self, name: &str) -> Option<&'static Edit> {
        match name {
            "edLibSearch" => Some(&self.edLibSearch),
            "edPickerSearch" => Some(&self.edPickerSearch),
            "edTrSearch" => Some(&self.edTrSearch),
            _ => None,
        }
    }
    /// O Edit que disparou o evento (`None` se o `sender` não é um Edit deste form).
    pub fn sender_as_edit(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Edit> {
        self.find_edit(ui.name(sender)?)
    }
    /// O FlowLayout deste form com o nome (`None` se não há um FlowLayout com ele).
    pub fn find_flow_layout(&'static self, name: &str) -> Option<&'static FlowLayout> {
        match name {
            "lyTimer" => Some(&self.lyTimer),
            "lyRecord" => Some(&self.lyRecord),
            "flowPickerGrid" => Some(&self.flowPickerGrid),
            "lyAreaOptions" => Some(&self.lyAreaOptions),
            "lyAreaButtons" => Some(&self.lyAreaButtons),
            _ => None,
        }
    }
    /// O FlowLayout que disparou o evento (`None` se o `sender` não é um FlowLayout deste form).
    pub fn sender_as_flow_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static FlowLayout> {
        self.find_flow_layout(ui.name(sender)?)
    }
    /// O FrameView deste form com o nome (`None` se não há um FrameView com ele).
    pub fn find_frame_view(&'static self, name: &str) -> Option<&'static FrameView> {
        match name {
            "frmVideo" => Some(&self.frmVideo),
            _ => None,
        }
    }
    /// O FrameView que disparou o evento (`None` se o `sender` não é um FrameView deste form).
    pub fn sender_as_frame_view(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static FrameView> {
        self.find_frame_view(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblUpdTitle" => Some(&self.lblUpdTitle),
            "lblUpdBody" => Some(&self.lblUpdBody),
            "lblTrChip" => Some(&self.lblTrChip),
            "lblStatus" => Some(&self.lblStatus),
            "lblTimer" => Some(&self.lblTimer),
            "lblTimerMs" => Some(&self.lblTimerMs),
            "lblSize" => Some(&self.lblSize),
            "lblRecordAction" => Some(&self.lblRecordAction),
            "lblReplay" => Some(&self.lblReplay),
            "lblReplayStatus" => Some(&self.lblReplayStatus),
            "lblLegendTitle" => Some(&self.lblLegendTitle),
            "lblMicsCount" => Some(&self.lblMicsCount),
            "lblMicsTitle" => Some(&self.lblMicsTitle),
            "lblSpeakersCount" => Some(&self.lblSpeakersCount),
            "lblSpeakersTitle" => Some(&self.lblSpeakersTitle),
            "lblScreens" => Some(&self.lblScreens),
            "lblScreensCount" => Some(&self.lblScreensCount),
            "lblScreensMeta" => Some(&self.lblScreensMeta),
            "lblRecordings" => Some(&self.lblRecordings),
            "lblLibMeta" => Some(&self.lblLibMeta),
            "lblLibEmpty" => Some(&self.lblLibEmpty),
            "lblPickerTitle" => Some(&self.lblPickerTitle),
            "lblPickerSubtitle" => Some(&self.lblPickerSubtitle),
            "lblPickerEmpty" => Some(&self.lblPickerEmpty),
            "lblAreaStep1" => Some(&self.lblAreaStep1),
            "lblAreaStep2" => Some(&self.lblAreaStep2),
            "lblSetTitle" => Some(&self.lblSetTitle),
            "lblSetTab" => Some(&self.lblSetTab),
            "lblSideTitle" => Some(&self.lblSideTitle),
            "lblPlayerPaused" => Some(&self.lblPlayerPaused),
            "lblPlayerTitle" => Some(&self.lblPlayerTitle),
            "lblPlayerTime" => Some(&self.lblPlayerTime),
            "lblPlayerClock" => Some(&self.lblPlayerClock),
            "lblPlayerLoading" => Some(&self.lblPlayerLoading),
            "lblCaption" => Some(&self.lblCaption),
            _ => None,
        }
    }
    /// O Label que disparou o evento (`None` se o `sender` não é um Label deste form).
    pub fn sender_as_label(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Label> {
        self.find_label(ui.name(sender)?)
    }
    /// O Layout deste form com o nome (`None` se não há um Layout com ele).
    pub fn find_layout(&'static self, name: &str) -> Option<&'static Layout> {
        match name {
            "lyCardHeader" => Some(&self.lyCardHeader),
            "lyReplayText" => Some(&self.lyReplayText),
            "lyLegendTitle" => Some(&self.lyLegendTitle),
            "lyLegend" => Some(&self.lyLegend),
            "lySources" => Some(&self.lySources),
            "lyMicsTitle" => Some(&self.lyMicsTitle),
            "lyMics" => Some(&self.lyMics),
            "lySpeakersTitle" => Some(&self.lySpeakersTitle),
            "lySpeakers" => Some(&self.lySpeakers),
            "lyDisplays" => Some(&self.lyDisplays),
            "lyDisplaysTitle" => Some(&self.lyDisplaysTitle),
            "lyLibrary" => Some(&self.lyLibrary),
            "lyLibHeader" => Some(&self.lyLibHeader),
            "lyLibTools" => Some(&self.lyLibTools),
            "lyLibContent" => Some(&self.lyLibContent),
            "lyPickerHeader" => Some(&self.lyPickerHeader),
            "lyPickerTitles" => Some(&self.lyPickerTitles),
            "lyPickerMode" => Some(&self.lyPickerMode),
            "lyPickerWindow" => Some(&self.lyPickerWindow),
            "lyPickerTools" => Some(&self.lyPickerTools),
            "lyPickerArea" => Some(&self.lyPickerArea),
            "lySetTabs" => Some(&self.lySetTabs),
            "lySetHead" => Some(&self.lySetHead),
            "lySettings" => Some(&self.lySettings),
            "lyPlayerSideHead" => Some(&self.lyPlayerSideHead),
            "lySide" => Some(&self.lySide),
            _ => None,
        }
    }
    /// O Layout que disparou o evento (`None` se o `sender` não é um Layout deste form).
    pub fn sender_as_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Layout> {
        self.find_layout(ui.name(sender)?)
    }
    /// O Panel deste form com o nome (`None` se não há um Panel com ele).
    pub fn find_panel(&'static self, name: &str) -> Option<&'static Panel> {
        match name {
            "pnlSources" => Some(&self.pnlSources),
            "pnlUpdate" => Some(&self.pnlUpdate),
            "pnlRecordCard" => Some(&self.pnlRecordCard),
            "pnlReplay" => Some(&self.pnlReplay),
            "pnlLegend" => Some(&self.pnlLegend),
            "pnlRecordings" => Some(&self.pnlRecordings),
            "pnlPicker" => Some(&self.pnlPicker),
            "pnlPickerDialog" => Some(&self.pnlPickerDialog),
            "pnlSettings" => Some(&self.pnlSettings),
            "pnlSetSide" => Some(&self.pnlSetSide),
            "pnlPlayer" => Some(&self.pnlPlayer),
            "pnlPlayerSide" => Some(&self.pnlPlayerSide),
            "pnlPlayerHead" => Some(&self.pnlPlayerHead),
            "pnlPlayerBar" => Some(&self.pnlPlayerBar),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O RecordButton deste form com o nome (`None` se não há um RecordButton com ele).
    pub fn find_record_button(&'static self, name: &str) -> Option<&'static RecordButton> {
        match name {
            "btnRecord" => Some(&self.btnRecord),
            _ => None,
        }
    }
    /// O RecordButton que disparou o evento (`None` se o `sender` não é um RecordButton deste form).
    pub fn sender_as_record_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static RecordButton> {
        self.find_record_button(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnSourcesEdge" => Some(&self.lnSourcesEdge),
            "lnCard" => Some(&self.lnCard),
            "lnSources" => Some(&self.lnSources),
            "lnLegend" => Some(&self.lnLegend),
            "lnPicker" => Some(&self.lnPicker),
            "lnSetSide" => Some(&self.lnSetSide),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O ScreenLayout deste form com o nome (`None` se não há um ScreenLayout com ele).
    pub fn find_screen_layout(&'static self, name: &str) -> Option<&'static ScreenLayout> {
        match name {
            "scrDisplays" => Some(&self.scrDisplays),
            _ => None,
        }
    }
    /// O ScreenLayout que disparou o evento (`None` se o `sender` não é um ScreenLayout deste form).
    pub fn sender_as_screen_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ScreenLayout> {
        self.find_screen_layout(ui.name(sender)?)
    }
    /// O ScrollBox deste form com o nome (`None` se não há um ScrollBox com ele).
    pub fn find_scroll_box(&'static self, name: &str) -> Option<&'static ScrollBox> {
        match name {
            "scrSources" => Some(&self.scrSources),
            "scrRecordings" => Some(&self.scrRecordings),
            "scrPickerGrid" => Some(&self.scrPickerGrid),
            "scrSettings" => Some(&self.scrSettings),
            "scrPlayerSide" => Some(&self.scrPlayerSide),
            _ => None,
        }
    }
    /// O ScrollBox que disparou o evento (`None` se o `sender` não é um ScrollBox deste form).
    pub fn sender_as_scroll_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ScrollBox> {
        self.find_scroll_box(ui.name(sender)?)
    }
    /// O SegmentedControl deste form com o nome (`None` se não há um SegmentedControl com ele).
    pub fn find_segmented_control(&'static self, name: &str) -> Option<&'static SegmentedControl> {
        match name {
            "segPickerMode" => Some(&self.segPickerMode),
            _ => None,
        }
    }
    /// O SegmentedControl que disparou o evento (`None` se o `sender` não é um SegmentedControl deste form).
    pub fn sender_as_segmented_control(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static SegmentedControl> {
        self.find_segmented_control(ui.name(sender)?)
    }
    /// O Switch deste form com o nome (`None` se não há um Switch com ele).
    pub fn find_switch(&'static self, name: &str) -> Option<&'static Switch> {
        match name {
            "swReplay" => Some(&self.swReplay),
            _ => None,
        }
    }
    /// O Switch que disparou o evento (`None` se o `sender` não é um Switch deste form).
    pub fn sender_as_switch(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Switch> {
        self.find_switch(ui.name(sender)?)
    }
    /// O TrackBar deste form com o nome (`None` se não há um TrackBar com ele).
    pub fn find_track_bar(&'static self, name: &str) -> Option<&'static TrackBar> {
        match name {
            "trkVolume" => Some(&self.trkVolume),
            _ => None,
        }
    }
    /// O TrackBar que disparou o evento (`None` se o `sender` não é um TrackBar deste form).
    pub fn sender_as_track_bar(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static TrackBar> {
        self.find_track_bar(ui.name(sender)?)
    }
    /// O Waveform deste form com o nome (`None` se não há um Waveform com ele).
    pub fn find_waveform(&'static self, name: &str) -> Option<&'static Waveform> {
        match name {
            "wavPlayer" => Some(&self.wavPlayer),
            _ => None,
        }
    }
    /// O Waveform que disparou o evento (`None` se o `sender` não é um Waveform deste form).
    pub fn sender_as_waveform(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Waveform> {
        self.find_waveform(ui.name(sender)?)
    }
}

/// O form frmMain e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FrmMainComponents {
    pub form: Form,
    pub pnlSources: Panel,
    pub lnSourcesEdge: Rectangle,
    pub pnlUpdate: Panel,
    pub btnUpdClose: Button,
    pub btnUpdOpen: Button,
    pub lblUpdTitle: Label,
    pub lblUpdBody: Label,
    pub pnlRecordCard: Panel,
    pub lyCardHeader: Layout,
    pub dotStatus: Circle,
    pub btnSettings: Button,
    pub lblTrChip: Label,
    pub lblStatus: Label,
    pub lyTimer: FlowLayout,
    pub lblTimer: Label,
    pub lblTimerMs: Label,
    pub lblSize: Label,
    pub lyRecord: FlowLayout,
    pub btnRecord: RecordButton,
    pub lblRecordAction: Label,
    pub lnCard: Rectangle,
    pub pnlReplay: Panel,
    pub dotReplay: Circle,
    pub btnPause: Button,
    pub btnCapture: Button,
    pub swReplay: Switch,
    pub btnReplaySave: Button,
    pub lyReplayText: Layout,
    pub lblReplay: Label,
    pub lblReplayStatus: Label,
    pub lnSources: Rectangle,
    pub pnlLegend: Panel,
    pub lnLegend: Rectangle,
    pub lyLegendTitle: Layout,
    pub lblLegendTitle: Label,
    pub lyLegend: Layout,
    pub scrSources: ScrollBox,
    pub lySources: Layout,
    pub lyMicsTitle: Layout,
    pub lblMicsCount: Label,
    pub lblMicsTitle: Label,
    pub lyMics: Layout,
    pub lySpeakersTitle: Layout,
    pub lblSpeakersCount: Label,
    pub lblSpeakersTitle: Label,
    pub lySpeakers: Layout,
    pub pnlRecordings: Panel,
    pub scrRecordings: ScrollBox,
    pub lyDisplays: Layout,
    pub lyDisplaysTitle: Layout,
    pub lblScreens: Label,
    pub lblScreensCount: Label,
    pub lblScreensMeta: Label,
    pub scrDisplays: ScreenLayout,
    pub lyLibrary: Layout,
    pub lyLibHeader: Layout,
    pub lblRecordings: Label,
    pub lblLibMeta: Label,
    pub lyLibTools: Layout,
    pub edLibSearch: Edit,
    pub btnLibOpenDir: Button,
    pub btnLibNewFolder: Button,
    pub btnLibMerge: Button,
    pub btnLibExport: Button,
    pub btnLibDelete: Button,
    pub brdLib: Breadcrumb,
    pub lblLibEmpty: Label,
    pub lyLibContent: Layout,
    pub pnlPicker: Panel,
    pub pnlPickerDialog: Panel,
    pub lyPickerHeader: Layout,
    pub btnPickerClose: Button,
    pub btnPickerRefresh: Button,
    pub lyPickerTitles: Layout,
    pub lblPickerTitle: Label,
    pub lblPickerSubtitle: Label,
    pub lnPicker: Rectangle,
    pub lyPickerMode: Layout,
    pub segPickerMode: SegmentedControl,
    pub lyPickerWindow: Layout,
    pub lyPickerTools: Layout,
    pub chipAppAudio: Chip,
    pub edPickerSearch: Edit,
    pub lblPickerEmpty: Label,
    pub scrPickerGrid: ScrollBox,
    pub flowPickerGrid: FlowLayout,
    pub lyPickerArea: Layout,
    pub lblAreaStep1: Label,
    pub lblAreaStep2: Label,
    pub lyAreaOptions: FlowLayout,
    pub chipTimelapse: Chip,
    pub chipTimelapseCursor: Chip,
    pub lyAreaButtons: FlowLayout,
    pub btnSelectArea: Button,
    pub pnlSettings: Panel,
    pub pnlSetSide: Panel,
    pub lnSetSide: Rectangle,
    pub lblSetTitle: Label,
    pub lySetTabs: Layout,
    pub lySetHead: Layout,
    pub btnSetClose: Button,
    pub lblSetTab: Label,
    pub scrSettings: ScrollBox,
    pub lySettings: Layout,
    pub pnlPlayer: Panel,
    pub pnlPlayerSide: Panel,
    pub lyPlayerSideHead: Layout,
    pub btnSideClose: Button,
    pub lblSideTitle: Label,
    pub edTrSearch: Edit,
    pub scrPlayerSide: ScrollBox,
    pub lySide: Layout,
    pub pnlPlayerHead: Panel,
    pub btnPlayerClose: Button,
    pub lblPlayerPaused: Label,
    pub lblPlayerTitle: Label,
    pub pnlPlayerBar: Panel,
    pub btnPlay: Button,
    pub lblPlayerTime: Label,
    pub btnFullscreen: Button,
    pub btnTranscript: Button,
    pub btnInfo: Button,
    pub btnCaptions: Button,
    pub btnSpeed: Button,
    pub trkVolume: TrackBar,
    pub btnMute: Button,
    pub lblPlayerClock: Label,
    pub wavPlayer: Waveform,
    pub frmVideo: FrameView,
    pub lblPlayerLoading: Label,
    pub lblCaption: Label,
}

impl std::ops::Deref for FrmMainComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static frmMain: FrmMainComponents = FrmMainComponents {
    form: Form { target: "frmMain" },
    pnlSources: Panel { target: "frmMain.pnlSources" },
    lnSourcesEdge: Rectangle { target: "frmMain.lnSourcesEdge" },
    pnlUpdate: Panel { target: "frmMain.pnlUpdate" },
    btnUpdClose: Button { target: "frmMain.btnUpdClose" },
    btnUpdOpen: Button { target: "frmMain.btnUpdOpen" },
    lblUpdTitle: Label { target: "frmMain.lblUpdTitle" },
    lblUpdBody: Label { target: "frmMain.lblUpdBody" },
    pnlRecordCard: Panel { target: "frmMain.pnlRecordCard" },
    lyCardHeader: Layout { target: "frmMain.lyCardHeader" },
    dotStatus: Circle { target: "frmMain.dotStatus" },
    btnSettings: Button { target: "frmMain.btnSettings" },
    lblTrChip: Label { target: "frmMain.lblTrChip" },
    lblStatus: Label { target: "frmMain.lblStatus" },
    lyTimer: FlowLayout { target: "frmMain.lyTimer" },
    lblTimer: Label { target: "frmMain.lblTimer" },
    lblTimerMs: Label { target: "frmMain.lblTimerMs" },
    lblSize: Label { target: "frmMain.lblSize" },
    lyRecord: FlowLayout { target: "frmMain.lyRecord" },
    btnRecord: RecordButton { target: "frmMain.btnRecord" },
    lblRecordAction: Label { target: "frmMain.lblRecordAction" },
    lnCard: Rectangle { target: "frmMain.lnCard" },
    pnlReplay: Panel { target: "frmMain.pnlReplay" },
    dotReplay: Circle { target: "frmMain.dotReplay" },
    btnPause: Button { target: "frmMain.btnPause" },
    btnCapture: Button { target: "frmMain.btnCapture" },
    swReplay: Switch { target: "frmMain.swReplay" },
    btnReplaySave: Button { target: "frmMain.btnReplaySave" },
    lyReplayText: Layout { target: "frmMain.lyReplayText" },
    lblReplay: Label { target: "frmMain.lblReplay" },
    lblReplayStatus: Label { target: "frmMain.lblReplayStatus" },
    lnSources: Rectangle { target: "frmMain.lnSources" },
    pnlLegend: Panel { target: "frmMain.pnlLegend" },
    lnLegend: Rectangle { target: "frmMain.lnLegend" },
    lyLegendTitle: Layout { target: "frmMain.lyLegendTitle" },
    lblLegendTitle: Label { target: "frmMain.lblLegendTitle" },
    lyLegend: Layout { target: "frmMain.lyLegend" },
    scrSources: ScrollBox { target: "frmMain.scrSources" },
    lySources: Layout { target: "frmMain.lySources" },
    lyMicsTitle: Layout { target: "frmMain.lyMicsTitle" },
    lblMicsCount: Label { target: "frmMain.lblMicsCount" },
    lblMicsTitle: Label { target: "frmMain.lblMicsTitle" },
    lyMics: Layout { target: "frmMain.lyMics" },
    lySpeakersTitle: Layout { target: "frmMain.lySpeakersTitle" },
    lblSpeakersCount: Label { target: "frmMain.lblSpeakersCount" },
    lblSpeakersTitle: Label { target: "frmMain.lblSpeakersTitle" },
    lySpeakers: Layout { target: "frmMain.lySpeakers" },
    pnlRecordings: Panel { target: "frmMain.pnlRecordings" },
    scrRecordings: ScrollBox { target: "frmMain.scrRecordings" },
    lyDisplays: Layout { target: "frmMain.lyDisplays" },
    lyDisplaysTitle: Layout { target: "frmMain.lyDisplaysTitle" },
    lblScreens: Label { target: "frmMain.lblScreens" },
    lblScreensCount: Label { target: "frmMain.lblScreensCount" },
    lblScreensMeta: Label { target: "frmMain.lblScreensMeta" },
    scrDisplays: ScreenLayout { target: "frmMain.scrDisplays" },
    lyLibrary: Layout { target: "frmMain.lyLibrary" },
    lyLibHeader: Layout { target: "frmMain.lyLibHeader" },
    lblRecordings: Label { target: "frmMain.lblRecordings" },
    lblLibMeta: Label { target: "frmMain.lblLibMeta" },
    lyLibTools: Layout { target: "frmMain.lyLibTools" },
    edLibSearch: Edit { target: "frmMain.edLibSearch" },
    btnLibOpenDir: Button { target: "frmMain.btnLibOpenDir" },
    btnLibNewFolder: Button { target: "frmMain.btnLibNewFolder" },
    btnLibMerge: Button { target: "frmMain.btnLibMerge" },
    btnLibExport: Button { target: "frmMain.btnLibExport" },
    btnLibDelete: Button { target: "frmMain.btnLibDelete" },
    brdLib: Breadcrumb { target: "frmMain.brdLib" },
    lblLibEmpty: Label { target: "frmMain.lblLibEmpty" },
    lyLibContent: Layout { target: "frmMain.lyLibContent" },
    pnlPicker: Panel { target: "frmMain.pnlPicker" },
    pnlPickerDialog: Panel { target: "frmMain.pnlPickerDialog" },
    lyPickerHeader: Layout { target: "frmMain.lyPickerHeader" },
    btnPickerClose: Button { target: "frmMain.btnPickerClose" },
    btnPickerRefresh: Button { target: "frmMain.btnPickerRefresh" },
    lyPickerTitles: Layout { target: "frmMain.lyPickerTitles" },
    lblPickerTitle: Label { target: "frmMain.lblPickerTitle" },
    lblPickerSubtitle: Label { target: "frmMain.lblPickerSubtitle" },
    lnPicker: Rectangle { target: "frmMain.lnPicker" },
    lyPickerMode: Layout { target: "frmMain.lyPickerMode" },
    segPickerMode: SegmentedControl { target: "frmMain.segPickerMode" },
    lyPickerWindow: Layout { target: "frmMain.lyPickerWindow" },
    lyPickerTools: Layout { target: "frmMain.lyPickerTools" },
    chipAppAudio: Chip { target: "frmMain.chipAppAudio" },
    edPickerSearch: Edit { target: "frmMain.edPickerSearch" },
    lblPickerEmpty: Label { target: "frmMain.lblPickerEmpty" },
    scrPickerGrid: ScrollBox { target: "frmMain.scrPickerGrid" },
    flowPickerGrid: FlowLayout { target: "frmMain.flowPickerGrid" },
    lyPickerArea: Layout { target: "frmMain.lyPickerArea" },
    lblAreaStep1: Label { target: "frmMain.lblAreaStep1" },
    lblAreaStep2: Label { target: "frmMain.lblAreaStep2" },
    lyAreaOptions: FlowLayout { target: "frmMain.lyAreaOptions" },
    chipTimelapse: Chip { target: "frmMain.chipTimelapse" },
    chipTimelapseCursor: Chip { target: "frmMain.chipTimelapseCursor" },
    lyAreaButtons: FlowLayout { target: "frmMain.lyAreaButtons" },
    btnSelectArea: Button { target: "frmMain.btnSelectArea" },
    pnlSettings: Panel { target: "frmMain.pnlSettings" },
    pnlSetSide: Panel { target: "frmMain.pnlSetSide" },
    lnSetSide: Rectangle { target: "frmMain.lnSetSide" },
    lblSetTitle: Label { target: "frmMain.lblSetTitle" },
    lySetTabs: Layout { target: "frmMain.lySetTabs" },
    lySetHead: Layout { target: "frmMain.lySetHead" },
    btnSetClose: Button { target: "frmMain.btnSetClose" },
    lblSetTab: Label { target: "frmMain.lblSetTab" },
    scrSettings: ScrollBox { target: "frmMain.scrSettings" },
    lySettings: Layout { target: "frmMain.lySettings" },
    pnlPlayer: Panel { target: "frmMain.pnlPlayer" },
    pnlPlayerSide: Panel { target: "frmMain.pnlPlayerSide" },
    lyPlayerSideHead: Layout { target: "frmMain.lyPlayerSideHead" },
    btnSideClose: Button { target: "frmMain.btnSideClose" },
    lblSideTitle: Label { target: "frmMain.lblSideTitle" },
    edTrSearch: Edit { target: "frmMain.edTrSearch" },
    scrPlayerSide: ScrollBox { target: "frmMain.scrPlayerSide" },
    lySide: Layout { target: "frmMain.lySide" },
    pnlPlayerHead: Panel { target: "frmMain.pnlPlayerHead" },
    btnPlayerClose: Button { target: "frmMain.btnPlayerClose" },
    lblPlayerPaused: Label { target: "frmMain.lblPlayerPaused" },
    lblPlayerTitle: Label { target: "frmMain.lblPlayerTitle" },
    pnlPlayerBar: Panel { target: "frmMain.pnlPlayerBar" },
    btnPlay: Button { target: "frmMain.btnPlay" },
    lblPlayerTime: Label { target: "frmMain.lblPlayerTime" },
    btnFullscreen: Button { target: "frmMain.btnFullscreen" },
    btnTranscript: Button { target: "frmMain.btnTranscript" },
    btnInfo: Button { target: "frmMain.btnInfo" },
    btnCaptions: Button { target: "frmMain.btnCaptions" },
    btnSpeed: Button { target: "frmMain.btnSpeed" },
    trkVolume: TrackBar { target: "frmMain.trkVolume" },
    btnMute: Button { target: "frmMain.btnMute" },
    lblPlayerClock: Label { target: "frmMain.lblPlayerClock" },
    wavPlayer: Waveform { target: "frmMain.wavPlayer" },
    frmVideo: FrameView { target: "frmMain.frmVideo" },
    lblPlayerLoading: Label { target: "frmMain.lblPlayerLoading" },
    lblCaption: Label { target: "frmMain.lblCaption" },
};

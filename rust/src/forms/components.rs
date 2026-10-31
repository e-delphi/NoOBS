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

/// Um ActivityIndicator de um form.
pub struct ActivityIndicator {
    pub target: &'static str,
}

impl ActivityIndicator {
    pub fn active(&self, ui: &Ui) -> bool {
        get(ui, self.target, "active").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_active(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "active", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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

/// Um CheckBox de um form.
pub struct CheckBox {
    pub target: &'static str,
}

impl CheckBox {
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
    pub fn box_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "box_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_box_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "box_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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

/// Um ComboBox de um form.
pub struct ComboBox {
    pub target: &'static str,
}

impl ComboBox {
    pub fn items(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "items"))
    }
    pub fn set_items(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "items", Into::<String>::into(value))
    }
    pub fn item_index(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "item_index").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_item_index(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "item_index", value)
    }
    pub fn h_align(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "h_align"))
    }
    pub fn set_h_align(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "h_align", Into::<String>::into(value))
    }
    pub fn trimming(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "trimming"))
    }
    pub fn set_trimming(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "trimming", Into::<String>::into(value))
    }
    pub fn arrow(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "arrow"))
    }
    pub fn set_arrow(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "arrow", Into::<String>::into(value))
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn zoom(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "zoom").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_zoom(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "zoom", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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

/// Um Frame de um form.
pub struct Frame {
    pub target: &'static str,
}

impl Frame {
    pub fn form(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "form"))
    }
    pub fn set_form(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "form", Into::<String>::into(value))
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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

/// Um Image de um form.
pub struct Image {
    pub target: &'static str,
}

impl Image {
    pub fn image(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "image"))
    }
    pub fn set_image(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "image", Into::<String>::into(value))
    }
    pub fn fit(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fit"))
    }
    pub fn set_fit(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fit", Into::<String>::into(value))
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn markup(&self, ui: &Ui) -> bool {
        get(ui, self.target, "markup").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_markup(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "markup", value)
    }
    pub fn outline(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "outline"))
    }
    pub fn set_outline(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "outline", Into::<String>::into(value))
    }
    pub fn outline_width(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "outline_width").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_outline_width(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "outline_width", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn fill_mid(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill_mid"))
    }
    pub fn set_fill_mid(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill_mid", Into::<String>::into(value))
    }
    pub fn fill_mid_at(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "fill_mid_at").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_fill_mid_at(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "fill_mid_at", value)
    }
    pub fn gradient_angle(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "gradient_angle").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_gradient_angle(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "gradient_angle", value)
    }
    pub fn gradient_radial(&self, ui: &Ui) -> bool {
        get(ui, self.target, "gradient_radial").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_gradient_radial(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "gradient_radial", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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

/// Um ProgressBar de um form.
pub struct ProgressBar {
    pub target: &'static str,
}

impl ProgressBar {
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
    pub fn color(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "color"))
    }
    pub fn set_color(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "color", Into::<String>::into(value))
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn fill_mid(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "fill_mid"))
    }
    pub fn set_fill_mid(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "fill_mid", Into::<String>::into(value))
    }
    pub fn fill_mid_at(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "fill_mid_at").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_fill_mid_at(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "fill_mid_at", value)
    }
    pub fn gradient_angle(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "gradient_angle").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_gradient_angle(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "gradient_angle", value)
    }
    pub fn gradient_radial(&self, ui: &Ui) -> bool {
        get(ui, self.target, "gradient_radial").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_gradient_radial(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "gradient_radial", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn wheel_step(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "wheel_step").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_wheel_step(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "wheel_step", value)
    }
    pub fn smooth(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "smooth"))
    }
    pub fn set_smooth(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "smooth", Into::<String>::into(value))
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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

/// Um TabControl de um form.
pub struct TabControl {
    pub target: &'static str,
}

impl TabControl {
    pub fn tab_index(&self, ui: &Ui) -> i64 {
        get(ui, self.target, "tab_index").and_then(|v| v.to_i64().ok()).unwrap_or_default()
    }
    pub fn set_tab_index(&self, ui: &mut Ui, value: i64) {
        set(ui, self.target, "tab_index", value)
    }
    pub fn show_tabs(&self, ui: &Ui) -> bool {
        get(ui, self.target, "show_tabs").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_show_tabs(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "show_tabs", value)
    }
    pub fn transition(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "transition"))
    }
    pub fn set_transition(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "transition", Into::<String>::into(value))
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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

/// Um TabItem de um form.
pub struct TabItem {
    pub target: &'static str,
}

impl TabItem {
    pub fn caption(&self, ui: &Ui) -> String {
        text(ui, get(ui, self.target, "caption"))
    }
    pub fn set_caption(&self, ui: &mut Ui, value: impl Into<String>) {
        set(ui, self.target, "caption", Into::<String>::into(value))
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn thumb_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "thumb_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_thumb_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "thumb_size", value)
    }
    pub fn track_size(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "track_size").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_track_size(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "track_size", value)
    }
    pub fn solid_thumb(&self, ui: &Ui) -> bool {
        get(ui, self.target, "solid_thumb").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_solid_thumb(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "solid_thumb", value)
    }
    pub fn limit(&self, ui: &Ui) -> f64 {
        get(ui, self.target, "limit").and_then(|v| v.to_f64().ok()).unwrap_or_default()
    }
    pub fn set_limit(&self, ui: &mut Ui, value: f64) {
        set(ui, self.target, "limit", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
    pub fn seek_bar(&self, ui: &Ui) -> bool {
        get(ui, self.target, "seek_bar").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_seek_bar(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "seek_bar", value)
    }
    pub fn paused(&self, ui: &Ui) -> bool {
        get(ui, self.target, "paused").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_paused(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "paused", value)
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
    pub fn draggable(&self, ui: &Ui) -> bool {
        get(ui, self.target, "draggable").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_draggable(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "draggable", value)
    }
    pub fn accepts_drops(&self, ui: &Ui) -> bool {
        get(ui, self.target, "accepts_drops").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_accepts_drops(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "accepts_drops", value)
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
    pub fn auto_size(&self, ui: &Ui) -> bool {
        get(ui, self.target, "auto_size").and_then(|v| v.to_bool().ok()).unwrap_or_default()
    }
    pub fn set_auto_size(&self, ui: &mut Ui, value: bool) {
        set(ui, self.target, "auto_size", value)
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
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
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
            _ => None,
        }
    }
    /// O FlowLayout que disparou o evento (`None` se o `sender` não é um FlowLayout deste form).
    pub fn sender_as_flow_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static FlowLayout> {
        self.find_flow_layout(ui.name(sender)?)
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
            _ => None,
        }
    }
    /// O ScrollBox que disparou o evento (`None` se o `sender` não é um ScrollBox deste form).
    pub fn sender_as_scroll_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ScrollBox> {
        self.find_scroll_box(ui.name(sender)?)
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
};

impl FrmSettingsComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnSetBack" => Some(&self.btnSetBack),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O Frame deste form com o nome (`None` se não há um Frame com ele).
    pub fn find_frame(&'static self, name: &str) -> Option<&'static Frame> {
        match name {
            "frGeneral" => Some(&self.frGeneral),
            "frOnRecord" => Some(&self.frOnRecord),
            "frFiles" => Some(&self.frFiles),
            "frVideo" => Some(&self.frVideo),
            "frAudio" => Some(&self.frAudio),
            "frReplay" => Some(&self.frReplay),
            "frHotkeys" => Some(&self.frHotkeys),
            "frDevices" => Some(&self.frDevices),
            "frTranscribe" => Some(&self.frTranscribe),
            "frUpdates" => Some(&self.frUpdates),
            "frAbout" => Some(&self.frAbout),
            _ => None,
        }
    }
    /// O Frame que disparou o evento (`None` se o `sender` não é um Frame deste form).
    pub fn sender_as_frame(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Frame> {
        self.find_frame(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblSetTitle" => Some(&self.lblSetTitle),
            "lblTabGeneral" => Some(&self.lblTabGeneral),
            "lblTabOnRecord" => Some(&self.lblTabOnRecord),
            "lblTabFiles" => Some(&self.lblTabFiles),
            "lblTabVideo" => Some(&self.lblTabVideo),
            "lblTabAudio" => Some(&self.lblTabAudio),
            "lblTabReplay" => Some(&self.lblTabReplay),
            "lblTabHotkeys" => Some(&self.lblTabHotkeys),
            "lblTabDevices" => Some(&self.lblTabDevices),
            "lblTabTranscribe" => Some(&self.lblTabTranscribe),
            "lblTabUpdates" => Some(&self.lblTabUpdates),
            "lblTabAbout" => Some(&self.lblTabAbout),
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
            "lySetHead" => Some(&self.lySetHead),
            "lySetTabs" => Some(&self.lySetTabs),
            "lySettings" => Some(&self.lySettings),
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
            "pnlSettings" => Some(&self.pnlSettings),
            "pnlSetSide" => Some(&self.pnlSetSide),
            "pnlTabGeneral" => Some(&self.pnlTabGeneral),
            "pnlTabOnRecord" => Some(&self.pnlTabOnRecord),
            "pnlTabFiles" => Some(&self.pnlTabFiles),
            "pnlTabVideo" => Some(&self.pnlTabVideo),
            "pnlTabAudio" => Some(&self.pnlTabAudio),
            "pnlTabReplay" => Some(&self.pnlTabReplay),
            "pnlTabHotkeys" => Some(&self.pnlTabHotkeys),
            "pnlTabDevices" => Some(&self.pnlTabDevices),
            "pnlTabTranscribe" => Some(&self.pnlTabTranscribe),
            "pnlTabUpdates" => Some(&self.pnlTabUpdates),
            "pnlTabAbout" => Some(&self.pnlTabAbout),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnSetHead" => Some(&self.lnSetHead),
            "lnSetSide" => Some(&self.lnSetSide),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O ScrollBox deste form com o nome (`None` se não há um ScrollBox com ele).
    pub fn find_scroll_box(&'static self, name: &str) -> Option<&'static ScrollBox> {
        match name {
            "scrSettings" => Some(&self.scrSettings),
            _ => None,
        }
    }
    /// O ScrollBox que disparou o evento (`None` se o `sender` não é um ScrollBox deste form).
    pub fn sender_as_scroll_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ScrollBox> {
        self.find_scroll_box(ui.name(sender)?)
    }
    /// O TabControl deste form com o nome (`None` se não há um TabControl com ele).
    pub fn find_tab_control(&'static self, name: &str) -> Option<&'static TabControl> {
        match name {
            "tbcSettings" => Some(&self.tbcSettings),
            _ => None,
        }
    }
    /// O TabControl que disparou o evento (`None` se o `sender` não é um TabControl deste form).
    pub fn sender_as_tab_control(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static TabControl> {
        self.find_tab_control(ui.name(sender)?)
    }
    /// O TabItem deste form com o nome (`None` se não há um TabItem com ele).
    pub fn find_tab_item(&'static self, name: &str) -> Option<&'static TabItem> {
        match name {
            "tabGeneral" => Some(&self.tabGeneral),
            "tabOnRecord" => Some(&self.tabOnRecord),
            "tabFiles" => Some(&self.tabFiles),
            "tabVideo" => Some(&self.tabVideo),
            "tabAudio" => Some(&self.tabAudio),
            "tabReplay" => Some(&self.tabReplay),
            "tabHotkeys" => Some(&self.tabHotkeys),
            "tabDevices" => Some(&self.tabDevices),
            "tabTranscribe" => Some(&self.tabTranscribe),
            "tabUpdates" => Some(&self.tabUpdates),
            "tabAbout" => Some(&self.tabAbout),
            _ => None,
        }
    }
    /// O TabItem que disparou o evento (`None` se o `sender` não é um TabItem deste form).
    pub fn sender_as_tab_item(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static TabItem> {
        self.find_tab_item(ui.name(sender)?)
    }
}

/// O form frmSettings e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FrmSettingsComponents {
    pub form: Form,
    pub pnlSettings: Panel,
    pub lySetHead: Layout,
    pub lnSetHead: Rectangle,
    pub btnSetBack: Button,
    pub lblSetTitle: Label,
    pub pnlSetSide: Panel,
    pub lnSetSide: Rectangle,
    pub lySetTabs: Layout,
    pub pnlTabGeneral: Panel,
    pub lblTabGeneral: Label,
    pub pnlTabOnRecord: Panel,
    pub lblTabOnRecord: Label,
    pub pnlTabFiles: Panel,
    pub lblTabFiles: Label,
    pub pnlTabVideo: Panel,
    pub lblTabVideo: Label,
    pub pnlTabAudio: Panel,
    pub lblTabAudio: Label,
    pub pnlTabReplay: Panel,
    pub lblTabReplay: Label,
    pub pnlTabHotkeys: Panel,
    pub lblTabHotkeys: Label,
    pub pnlTabDevices: Panel,
    pub lblTabDevices: Label,
    pub pnlTabTranscribe: Panel,
    pub lblTabTranscribe: Label,
    pub pnlTabUpdates: Panel,
    pub lblTabUpdates: Label,
    pub pnlTabAbout: Panel,
    pub lblTabAbout: Label,
    pub scrSettings: ScrollBox,
    pub lySettings: Layout,
    pub tbcSettings: TabControl,
    pub tabGeneral: TabItem,
    pub frGeneral: Frame,
    pub tabOnRecord: TabItem,
    pub frOnRecord: Frame,
    pub tabFiles: TabItem,
    pub frFiles: Frame,
    pub tabVideo: TabItem,
    pub frVideo: Frame,
    pub tabAudio: TabItem,
    pub frAudio: Frame,
    pub tabReplay: TabItem,
    pub frReplay: Frame,
    pub tabHotkeys: TabItem,
    pub frHotkeys: Frame,
    pub tabDevices: TabItem,
    pub frDevices: Frame,
    pub tabTranscribe: TabItem,
    pub frTranscribe: Frame,
    pub tabUpdates: TabItem,
    pub frUpdates: Frame,
    pub tabAbout: TabItem,
    pub frAbout: Frame,
}

impl std::ops::Deref for FrmSettingsComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static frmSettings: FrmSettingsComponents = FrmSettingsComponents {
    form: Form { target: "frmSettings" },
    pnlSettings: Panel { target: "frmSettings.pnlSettings" },
    lySetHead: Layout { target: "frmSettings.lySetHead" },
    lnSetHead: Rectangle { target: "frmSettings.lnSetHead" },
    btnSetBack: Button { target: "frmSettings.btnSetBack" },
    lblSetTitle: Label { target: "frmSettings.lblSetTitle" },
    pnlSetSide: Panel { target: "frmSettings.pnlSetSide" },
    lnSetSide: Rectangle { target: "frmSettings.lnSetSide" },
    lySetTabs: Layout { target: "frmSettings.lySetTabs" },
    pnlTabGeneral: Panel { target: "frmSettings.pnlTabGeneral" },
    lblTabGeneral: Label { target: "frmSettings.lblTabGeneral" },
    pnlTabOnRecord: Panel { target: "frmSettings.pnlTabOnRecord" },
    lblTabOnRecord: Label { target: "frmSettings.lblTabOnRecord" },
    pnlTabFiles: Panel { target: "frmSettings.pnlTabFiles" },
    lblTabFiles: Label { target: "frmSettings.lblTabFiles" },
    pnlTabVideo: Panel { target: "frmSettings.pnlTabVideo" },
    lblTabVideo: Label { target: "frmSettings.lblTabVideo" },
    pnlTabAudio: Panel { target: "frmSettings.pnlTabAudio" },
    lblTabAudio: Label { target: "frmSettings.lblTabAudio" },
    pnlTabReplay: Panel { target: "frmSettings.pnlTabReplay" },
    lblTabReplay: Label { target: "frmSettings.lblTabReplay" },
    pnlTabHotkeys: Panel { target: "frmSettings.pnlTabHotkeys" },
    lblTabHotkeys: Label { target: "frmSettings.lblTabHotkeys" },
    pnlTabDevices: Panel { target: "frmSettings.pnlTabDevices" },
    lblTabDevices: Label { target: "frmSettings.lblTabDevices" },
    pnlTabTranscribe: Panel { target: "frmSettings.pnlTabTranscribe" },
    lblTabTranscribe: Label { target: "frmSettings.lblTabTranscribe" },
    pnlTabUpdates: Panel { target: "frmSettings.pnlTabUpdates" },
    lblTabUpdates: Label { target: "frmSettings.lblTabUpdates" },
    pnlTabAbout: Panel { target: "frmSettings.pnlTabAbout" },
    lblTabAbout: Label { target: "frmSettings.lblTabAbout" },
    scrSettings: ScrollBox { target: "frmSettings.scrSettings" },
    lySettings: Layout { target: "frmSettings.lySettings" },
    tbcSettings: TabControl { target: "frmSettings.tbcSettings" },
    tabGeneral: TabItem { target: "frmSettings.tabGeneral" },
    frGeneral: Frame { target: "frmSettings.frGeneral" },
    tabOnRecord: TabItem { target: "frmSettings.tabOnRecord" },
    frOnRecord: Frame { target: "frmSettings.frOnRecord" },
    tabFiles: TabItem { target: "frmSettings.tabFiles" },
    frFiles: Frame { target: "frmSettings.frFiles" },
    tabVideo: TabItem { target: "frmSettings.tabVideo" },
    frVideo: Frame { target: "frmSettings.frVideo" },
    tabAudio: TabItem { target: "frmSettings.tabAudio" },
    frAudio: Frame { target: "frmSettings.frAudio" },
    tabReplay: TabItem { target: "frmSettings.tabReplay" },
    frReplay: Frame { target: "frmSettings.frReplay" },
    tabHotkeys: TabItem { target: "frmSettings.tabHotkeys" },
    frHotkeys: Frame { target: "frmSettings.frHotkeys" },
    tabDevices: TabItem { target: "frmSettings.tabDevices" },
    frDevices: Frame { target: "frmSettings.frDevices" },
    tabTranscribe: TabItem { target: "frmSettings.tabTranscribe" },
    frTranscribe: Frame { target: "frmSettings.frTranscribe" },
    tabUpdates: TabItem { target: "frmSettings.tabUpdates" },
    frUpdates: Frame { target: "frmSettings.frUpdates" },
    tabAbout: TabItem { target: "frmSettings.tabAbout" },
    frAbout: Frame { target: "frmSettings.frAbout" },
};

impl FrmPlayerComponents {
    /// O ActivityIndicator deste form com o nome (`None` se não há um ActivityIndicator com ele).
    pub fn find_activity_indicator(&'static self, name: &str) -> Option<&'static ActivityIndicator> {
        match name {
            "aniPlayerLoading" => Some(&self.aniPlayerLoading),
            _ => None,
        }
    }
    /// O ActivityIndicator que disparou o evento (`None` se o `sender` não é um ActivityIndicator deste form).
    pub fn sender_as_activity_indicator(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ActivityIndicator> {
        self.find_activity_indicator(ui.name(sender)?)
    }
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnSideClose" => Some(&self.btnSideClose),
            "btnPlayerClose" => Some(&self.btnPlayerClose),
            "btnPlay" => Some(&self.btnPlay),
            "btnClockIcon" => Some(&self.btnClockIcon),
            "btnFullscreen" => Some(&self.btnFullscreen),
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
    /// O Circle deste form com o nome (`None` se não há um Circle com ele).
    pub fn find_circle(&'static self, name: &str) -> Option<&'static Circle> {
        match name {
            "cirPauseFlash" => Some(&self.cirPauseFlash),
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
            "edTrSearch" => Some(&self.edTrSearch),
            _ => None,
        }
    }
    /// O Edit que disparou o evento (`None` se o `sender` não é um Edit deste form).
    pub fn sender_as_edit(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Edit> {
        self.find_edit(ui.name(sender)?)
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
            "lblSideTitle" => Some(&self.lblSideTitle),
            "lblCaption" => Some(&self.lblCaption),
            "lblPlayerZoom" => Some(&self.lblPlayerZoom),
            "lblPlayerLoading" => Some(&self.lblPlayerLoading),
            "lblPlayerPaused" => Some(&self.lblPlayerPaused),
            "lblPlayerTitle" => Some(&self.lblPlayerTitle),
            "lblPlayerTime" => Some(&self.lblPlayerTime),
            "lblPlayerClock" => Some(&self.lblPlayerClock),
            "lblVolPct" => Some(&self.lblVolPct),
            "lblCcCheck1" => Some(&self.lblCcCheck1),
            "lblCcKey1" => Some(&self.lblCcKey1),
            "lblCcCaptions" => Some(&self.lblCcCaptions),
            "lblCcCheck2" => Some(&self.lblCcCheck2),
            "lblCcTranscript" => Some(&self.lblCcTranscript),
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
            "lySideBody" => Some(&self.lySideBody),
            "lyPlayerSideHead" => Some(&self.lyPlayerSideHead),
            "lyTrSearch" => Some(&self.lyTrSearch),
            "lySide" => Some(&self.lySide),
            "lyStage" => Some(&self.lyStage),
            "lyPlayerLoading" => Some(&self.lyPlayerLoading),
            "lyChrome" => Some(&self.lyChrome),
            "lyPlayerClock" => Some(&self.lyPlayerClock),
            "lyVolPop" => Some(&self.lyVolPop),
            "lyCcPop" => Some(&self.lyCcPop),
            "lySpeedItems" => Some(&self.lySpeedItems),
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
            "pnlPlayer" => Some(&self.pnlPlayer),
            "pnlPlayerSide" => Some(&self.pnlPlayerSide),
            "pnlPauseFlash" => Some(&self.pnlPauseFlash),
            "pnlPlayerLoading" => Some(&self.pnlPlayerLoading),
            "pnlPlayerHead" => Some(&self.pnlPlayerHead),
            "pnlPlayerPaused" => Some(&self.pnlPlayerPaused),
            "pnlPlayerBar" => Some(&self.pnlPlayerBar),
            "pnlVolPop" => Some(&self.pnlVolPop),
            "pnlCcPop" => Some(&self.pnlCcPop),
            "pnlCcCaptions" => Some(&self.pnlCcCaptions),
            "pnlCcTranscript" => Some(&self.pnlCcTranscript),
            "pnlSpeedPop" => Some(&self.pnlSpeedPop),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "recSideEdge" => Some(&self.recSideEdge),
            "recSideHeadLine" => Some(&self.recSideHeadLine),
            "recTrSearchLine" => Some(&self.recTrSearchLine),
            "recPauseFlash1" => Some(&self.recPauseFlash1),
            "recPauseFlash2" => Some(&self.recPauseFlash2),
            "recPaused1" => Some(&self.recPaused1),
            "recPaused2" => Some(&self.recPaused2),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O ScrollBox deste form com o nome (`None` se não há um ScrollBox com ele).
    pub fn find_scroll_box(&'static self, name: &str) -> Option<&'static ScrollBox> {
        match name {
            "scrPlayerSide" => Some(&self.scrPlayerSide),
            _ => None,
        }
    }
    /// O ScrollBox que disparou o evento (`None` se o `sender` não é um ScrollBox deste form).
    pub fn sender_as_scroll_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ScrollBox> {
        self.find_scroll_box(ui.name(sender)?)
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

/// O form frmPlayer e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FrmPlayerComponents {
    pub form: Form,
    pub pnlPlayer: Panel,
    pub pnlPlayerSide: Panel,
    pub recSideEdge: Rectangle,
    pub lySideBody: Layout,
    pub lyPlayerSideHead: Layout,
    pub recSideHeadLine: Rectangle,
    pub btnSideClose: Button,
    pub lblSideTitle: Label,
    pub lyTrSearch: Layout,
    pub recTrSearchLine: Rectangle,
    pub edTrSearch: Edit,
    pub scrPlayerSide: ScrollBox,
    pub lySide: Layout,
    pub lyStage: Layout,
    pub frmVideo: FrameView,
    pub lblCaption: Label,
    pub lblPlayerZoom: Label,
    pub pnlPauseFlash: Panel,
    pub cirPauseFlash: Circle,
    pub recPauseFlash1: Rectangle,
    pub recPauseFlash2: Rectangle,
    pub pnlPlayerLoading: Panel,
    pub lyPlayerLoading: Layout,
    pub aniPlayerLoading: ActivityIndicator,
    pub lblPlayerLoading: Label,
    pub lyChrome: Layout,
    pub pnlPlayerHead: Panel,
    pub btnPlayerClose: Button,
    pub pnlPlayerPaused: Panel,
    pub recPaused1: Rectangle,
    pub recPaused2: Rectangle,
    pub lblPlayerPaused: Label,
    pub lblPlayerTitle: Label,
    pub pnlPlayerBar: Panel,
    pub btnPlay: Button,
    pub lblPlayerTime: Label,
    pub lyPlayerClock: Layout,
    pub btnClockIcon: Button,
    pub lblPlayerClock: Label,
    pub btnFullscreen: Button,
    pub btnInfo: Button,
    pub btnCaptions: Button,
    pub btnSpeed: Button,
    pub btnMute: Button,
    pub wavPlayer: Waveform,
    pub lyVolPop: Layout,
    pub pnlVolPop: Panel,
    pub lblVolPct: Label,
    pub trkVolume: TrackBar,
    pub lyCcPop: Layout,
    pub pnlCcPop: Panel,
    pub pnlCcCaptions: Panel,
    pub lblCcCheck1: Label,
    pub lblCcKey1: Label,
    pub lblCcCaptions: Label,
    pub pnlCcTranscript: Panel,
    pub lblCcCheck2: Label,
    pub lblCcTranscript: Label,
    pub pnlSpeedPop: Panel,
    pub lySpeedItems: Layout,
}

impl std::ops::Deref for FrmPlayerComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static frmPlayer: FrmPlayerComponents = FrmPlayerComponents {
    form: Form { target: "frmPlayer" },
    pnlPlayer: Panel { target: "frmPlayer.pnlPlayer" },
    pnlPlayerSide: Panel { target: "frmPlayer.pnlPlayerSide" },
    recSideEdge: Rectangle { target: "frmPlayer.recSideEdge" },
    lySideBody: Layout { target: "frmPlayer.lySideBody" },
    lyPlayerSideHead: Layout { target: "frmPlayer.lyPlayerSideHead" },
    recSideHeadLine: Rectangle { target: "frmPlayer.recSideHeadLine" },
    btnSideClose: Button { target: "frmPlayer.btnSideClose" },
    lblSideTitle: Label { target: "frmPlayer.lblSideTitle" },
    lyTrSearch: Layout { target: "frmPlayer.lyTrSearch" },
    recTrSearchLine: Rectangle { target: "frmPlayer.recTrSearchLine" },
    edTrSearch: Edit { target: "frmPlayer.edTrSearch" },
    scrPlayerSide: ScrollBox { target: "frmPlayer.scrPlayerSide" },
    lySide: Layout { target: "frmPlayer.lySide" },
    lyStage: Layout { target: "frmPlayer.lyStage" },
    frmVideo: FrameView { target: "frmPlayer.frmVideo" },
    lblCaption: Label { target: "frmPlayer.lblCaption" },
    lblPlayerZoom: Label { target: "frmPlayer.lblPlayerZoom" },
    pnlPauseFlash: Panel { target: "frmPlayer.pnlPauseFlash" },
    cirPauseFlash: Circle { target: "frmPlayer.cirPauseFlash" },
    recPauseFlash1: Rectangle { target: "frmPlayer.recPauseFlash1" },
    recPauseFlash2: Rectangle { target: "frmPlayer.recPauseFlash2" },
    pnlPlayerLoading: Panel { target: "frmPlayer.pnlPlayerLoading" },
    lyPlayerLoading: Layout { target: "frmPlayer.lyPlayerLoading" },
    aniPlayerLoading: ActivityIndicator { target: "frmPlayer.aniPlayerLoading" },
    lblPlayerLoading: Label { target: "frmPlayer.lblPlayerLoading" },
    lyChrome: Layout { target: "frmPlayer.lyChrome" },
    pnlPlayerHead: Panel { target: "frmPlayer.pnlPlayerHead" },
    btnPlayerClose: Button { target: "frmPlayer.btnPlayerClose" },
    pnlPlayerPaused: Panel { target: "frmPlayer.pnlPlayerPaused" },
    recPaused1: Rectangle { target: "frmPlayer.recPaused1" },
    recPaused2: Rectangle { target: "frmPlayer.recPaused2" },
    lblPlayerPaused: Label { target: "frmPlayer.lblPlayerPaused" },
    lblPlayerTitle: Label { target: "frmPlayer.lblPlayerTitle" },
    pnlPlayerBar: Panel { target: "frmPlayer.pnlPlayerBar" },
    btnPlay: Button { target: "frmPlayer.btnPlay" },
    lblPlayerTime: Label { target: "frmPlayer.lblPlayerTime" },
    lyPlayerClock: Layout { target: "frmPlayer.lyPlayerClock" },
    btnClockIcon: Button { target: "frmPlayer.btnClockIcon" },
    lblPlayerClock: Label { target: "frmPlayer.lblPlayerClock" },
    btnFullscreen: Button { target: "frmPlayer.btnFullscreen" },
    btnInfo: Button { target: "frmPlayer.btnInfo" },
    btnCaptions: Button { target: "frmPlayer.btnCaptions" },
    btnSpeed: Button { target: "frmPlayer.btnSpeed" },
    btnMute: Button { target: "frmPlayer.btnMute" },
    wavPlayer: Waveform { target: "frmPlayer.wavPlayer" },
    lyVolPop: Layout { target: "frmPlayer.lyVolPop" },
    pnlVolPop: Panel { target: "frmPlayer.pnlVolPop" },
    lblVolPct: Label { target: "frmPlayer.lblVolPct" },
    trkVolume: TrackBar { target: "frmPlayer.trkVolume" },
    lyCcPop: Layout { target: "frmPlayer.lyCcPop" },
    pnlCcPop: Panel { target: "frmPlayer.pnlCcPop" },
    pnlCcCaptions: Panel { target: "frmPlayer.pnlCcCaptions" },
    lblCcCheck1: Label { target: "frmPlayer.lblCcCheck1" },
    lblCcKey1: Label { target: "frmPlayer.lblCcKey1" },
    lblCcCaptions: Label { target: "frmPlayer.lblCcCaptions" },
    pnlCcTranscript: Panel { target: "frmPlayer.pnlCcTranscript" },
    lblCcCheck2: Label { target: "frmPlayer.lblCcCheck2" },
    lblCcTranscript: Label { target: "frmPlayer.lblCcTranscript" },
    pnlSpeedPop: Panel { target: "frmPlayer.pnlSpeedPop" },
    lySpeedItems: Layout { target: "frmPlayer.lySpeedItems" },
};

impl FrmExportComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnExClose" => Some(&self.btnExClose),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblExTitle" => Some(&self.lblExTitle),
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
            "lyExport" => Some(&self.lyExport),
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
            "pnlExport" => Some(&self.pnlExport),
            "pnlExHead" => Some(&self.pnlExHead),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnExHead" => Some(&self.lnExHead),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O ScrollBox deste form com o nome (`None` se não há um ScrollBox com ele).
    pub fn find_scroll_box(&'static self, name: &str) -> Option<&'static ScrollBox> {
        match name {
            "scrExport" => Some(&self.scrExport),
            _ => None,
        }
    }
    /// O ScrollBox que disparou o evento (`None` se o `sender` não é um ScrollBox deste form).
    pub fn sender_as_scroll_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ScrollBox> {
        self.find_scroll_box(ui.name(sender)?)
    }
}

/// O form frmExport e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FrmExportComponents {
    pub form: Form,
    pub pnlExport: Panel,
    pub pnlExHead: Panel,
    pub lnExHead: Rectangle,
    pub btnExClose: Button,
    pub lblExTitle: Label,
    pub scrExport: ScrollBox,
    pub lyExport: Layout,
}

impl std::ops::Deref for FrmExportComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static frmExport: FrmExportComponents = FrmExportComponents {
    form: Form { target: "frmExport" },
    pnlExport: Panel { target: "frmExport.pnlExport" },
    pnlExHead: Panel { target: "frmExport.pnlExHead" },
    lnExHead: Rectangle { target: "frmExport.lnExHead" },
    btnExClose: Button { target: "frmExport.btnExClose" },
    lblExTitle: Label { target: "frmExport.lblExTitle" },
    scrExport: ScrollBox { target: "frmExport.scrExport" },
    lyExport: Layout { target: "frmExport.lyExport" },
};

impl FrmPickerComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnPickerClose" => Some(&self.btnPickerClose),
            "btnPickerRefresh" => Some(&self.btnPickerRefresh),
            "btnSelectArea" => Some(&self.btnSelectArea),
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
    /// O Edit deste form com o nome (`None` se não há um Edit com ele).
    pub fn find_edit(&'static self, name: &str) -> Option<&'static Edit> {
        match name {
            "edPickerSearch" => Some(&self.edPickerSearch),
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
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblPickerTitle" => Some(&self.lblPickerTitle),
            "lblPickerSubtitle" => Some(&self.lblPickerSubtitle),
            "lblPickerEmpty" => Some(&self.lblPickerEmpty),
            "lblAreaStep1" => Some(&self.lblAreaStep1),
            "lblAreaStep2" => Some(&self.lblAreaStep2),
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
            "lyPickerHeader" => Some(&self.lyPickerHeader),
            "lyPickerTitles" => Some(&self.lyPickerTitles),
            "lyPickerMode" => Some(&self.lyPickerMode),
            "lyPickerWindow" => Some(&self.lyPickerWindow),
            "lyPickerTools" => Some(&self.lyPickerTools),
            "lyPickerArea" => Some(&self.lyPickerArea),
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
            "pnlPicker" => Some(&self.pnlPicker),
            "pnlPickerDialog" => Some(&self.pnlPickerDialog),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnPicker" => Some(&self.lnPicker),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O ScrollBox deste form com o nome (`None` se não há um ScrollBox com ele).
    pub fn find_scroll_box(&'static self, name: &str) -> Option<&'static ScrollBox> {
        match name {
            "scrPickerGrid" => Some(&self.scrPickerGrid),
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
}

/// O form frmPicker e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FrmPickerComponents {
    pub form: Form,
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
}

impl std::ops::Deref for FrmPickerComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static frmPicker: FrmPickerComponents = FrmPickerComponents {
    form: Form { target: "frmPicker" },
    pnlPicker: Panel { target: "frmPicker.pnlPicker" },
    pnlPickerDialog: Panel { target: "frmPicker.pnlPickerDialog" },
    lyPickerHeader: Layout { target: "frmPicker.lyPickerHeader" },
    btnPickerClose: Button { target: "frmPicker.btnPickerClose" },
    btnPickerRefresh: Button { target: "frmPicker.btnPickerRefresh" },
    lyPickerTitles: Layout { target: "frmPicker.lyPickerTitles" },
    lblPickerTitle: Label { target: "frmPicker.lblPickerTitle" },
    lblPickerSubtitle: Label { target: "frmPicker.lblPickerSubtitle" },
    lnPicker: Rectangle { target: "frmPicker.lnPicker" },
    lyPickerMode: Layout { target: "frmPicker.lyPickerMode" },
    segPickerMode: SegmentedControl { target: "frmPicker.segPickerMode" },
    lyPickerWindow: Layout { target: "frmPicker.lyPickerWindow" },
    lyPickerTools: Layout { target: "frmPicker.lyPickerTools" },
    chipAppAudio: Chip { target: "frmPicker.chipAppAudio" },
    edPickerSearch: Edit { target: "frmPicker.edPickerSearch" },
    lblPickerEmpty: Label { target: "frmPicker.lblPickerEmpty" },
    scrPickerGrid: ScrollBox { target: "frmPicker.scrPickerGrid" },
    flowPickerGrid: FlowLayout { target: "frmPicker.flowPickerGrid" },
    lyPickerArea: Layout { target: "frmPicker.lyPickerArea" },
    lblAreaStep1: Label { target: "frmPicker.lblAreaStep1" },
    lblAreaStep2: Label { target: "frmPicker.lblAreaStep2" },
    lyAreaOptions: FlowLayout { target: "frmPicker.lyAreaOptions" },
    chipTimelapse: Chip { target: "frmPicker.chipTimelapse" },
    chipTimelapseCursor: Chip { target: "frmPicker.chipTimelapseCursor" },
    lyAreaButtons: FlowLayout { target: "frmPicker.lyAreaButtons" },
    btnSelectArea: Button { target: "frmPicker.btnSelectArea" },
};

impl FraGeneralComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnThemeSystem" => Some(&self.btnThemeSystem),
            "btnThemeLight" => Some(&self.btnThemeLight),
            "btnThemeDark" => Some(&self.btnThemeDark),
            "btnReset" => Some(&self.btnReset),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O CheckBox deste form com o nome (`None` se não há um CheckBox com ele).
    pub fn find_check_box(&'static self, name: &str) -> Option<&'static CheckBox> {
        match name {
            "chkAutostart" => Some(&self.chkAutostart),
            "chkRunAsAdmin" => Some(&self.chkRunAsAdmin),
            "chkCloseToTray" => Some(&self.chkCloseToTray),
            _ => None,
        }
    }
    /// O CheckBox que disparou o evento (`None` se o `sender` não é um CheckBox deste form).
    pub fn sender_as_check_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static CheckBox> {
        self.find_check_box(ui.name(sender)?)
    }
    /// O ComboBox deste form com o nome (`None` se não há um ComboBox com ele).
    pub fn find_combo_box(&'static self, name: &str) -> Option<&'static ComboBox> {
        match name {
            "cmbLanguage" => Some(&self.cmbLanguage),
            _ => None,
        }
    }
    /// O ComboBox que disparou o evento (`None` se o `sender` não é um ComboBox deste form).
    pub fn sender_as_combo_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ComboBox> {
        self.find_combo_box(ui.name(sender)?)
    }
    /// O Edit deste form com o nome (`None` se não há um Edit com ele).
    pub fn find_edit(&'static self, name: &str) -> Option<&'static Edit> {
        match name {
            "edtWindowTitle" => Some(&self.edtWindowTitle),
            _ => None,
        }
    }
    /// O Edit que disparou o evento (`None` se o `sender` não é um Edit deste form).
    pub fn sender_as_edit(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Edit> {
        self.find_edit(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblTheme" => Some(&self.lblTheme),
            "lblLanguage" => Some(&self.lblLanguage),
            "lblLanguageHint" => Some(&self.lblLanguageHint),
            "lblWindowTitle" => Some(&self.lblWindowTitle),
            "lblWindowTitleHint" => Some(&self.lblWindowTitleHint),
            "lblAutostartHint" => Some(&self.lblAutostartHint),
            "lblRunAsAdminHint" => Some(&self.lblRunAsAdminHint),
            "lblRunAsAdminState" => Some(&self.lblRunAsAdminState),
            "lblCloseToTrayHint" => Some(&self.lblCloseToTrayHint),
            "lblReset" => Some(&self.lblReset),
            "lblResetHint" => Some(&self.lblResetHint),
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
            "lyReset" => Some(&self.lyReset),
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
            "pnlTheme" => Some(&self.pnlTheme),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnRule1" => Some(&self.lnRule1),
            "lnRule2" => Some(&self.lnRule2),
            "lnRule3" => Some(&self.lnRule3),
            "lnRule4" => Some(&self.lnRule4),
            "lnRule5" => Some(&self.lnRule5),
            "lnRule6" => Some(&self.lnRule6),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
}

/// O form fraGeneral e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraGeneralComponents {
    pub form: Form,
    pub lblTheme: Label,
    pub pnlTheme: Panel,
    pub btnThemeSystem: Button,
    pub btnThemeLight: Button,
    pub btnThemeDark: Button,
    pub lnRule1: Rectangle,
    pub lblLanguage: Label,
    pub cmbLanguage: ComboBox,
    pub lblLanguageHint: Label,
    pub lnRule2: Rectangle,
    pub lblWindowTitle: Label,
    pub edtWindowTitle: Edit,
    pub lblWindowTitleHint: Label,
    pub lnRule3: Rectangle,
    pub chkAutostart: CheckBox,
    pub lblAutostartHint: Label,
    pub lnRule4: Rectangle,
    pub chkRunAsAdmin: CheckBox,
    pub lblRunAsAdminHint: Label,
    pub lblRunAsAdminState: Label,
    pub lnRule5: Rectangle,
    pub chkCloseToTray: CheckBox,
    pub lblCloseToTrayHint: Label,
    pub lnRule6: Rectangle,
    pub lblReset: Label,
    pub lblResetHint: Label,
    pub lyReset: Layout,
    pub btnReset: Button,
}

impl std::ops::Deref for FraGeneralComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraGeneral: FraGeneralComponents = FraGeneralComponents {
    form: Form { target: "fraGeneral" },
    lblTheme: Label { target: "fraGeneral.lblTheme" },
    pnlTheme: Panel { target: "fraGeneral.pnlTheme" },
    btnThemeSystem: Button { target: "fraGeneral.btnThemeSystem" },
    btnThemeLight: Button { target: "fraGeneral.btnThemeLight" },
    btnThemeDark: Button { target: "fraGeneral.btnThemeDark" },
    lnRule1: Rectangle { target: "fraGeneral.lnRule1" },
    lblLanguage: Label { target: "fraGeneral.lblLanguage" },
    cmbLanguage: ComboBox { target: "fraGeneral.cmbLanguage" },
    lblLanguageHint: Label { target: "fraGeneral.lblLanguageHint" },
    lnRule2: Rectangle { target: "fraGeneral.lnRule2" },
    lblWindowTitle: Label { target: "fraGeneral.lblWindowTitle" },
    edtWindowTitle: Edit { target: "fraGeneral.edtWindowTitle" },
    lblWindowTitleHint: Label { target: "fraGeneral.lblWindowTitleHint" },
    lnRule3: Rectangle { target: "fraGeneral.lnRule3" },
    chkAutostart: CheckBox { target: "fraGeneral.chkAutostart" },
    lblAutostartHint: Label { target: "fraGeneral.lblAutostartHint" },
    lnRule4: Rectangle { target: "fraGeneral.lnRule4" },
    chkRunAsAdmin: CheckBox { target: "fraGeneral.chkRunAsAdmin" },
    lblRunAsAdminHint: Label { target: "fraGeneral.lblRunAsAdminHint" },
    lblRunAsAdminState: Label { target: "fraGeneral.lblRunAsAdminState" },
    lnRule5: Rectangle { target: "fraGeneral.lnRule5" },
    chkCloseToTray: CheckBox { target: "fraGeneral.chkCloseToTray" },
    lblCloseToTrayHint: Label { target: "fraGeneral.lblCloseToTrayHint" },
    lnRule6: Rectangle { target: "fraGeneral.lnRule6" },
    lblReset: Label { target: "fraGeneral.lblReset" },
    lblResetHint: Label { target: "fraGeneral.lblResetHint" },
    lyReset: Layout { target: "fraGeneral.lyReset" },
    btnReset: Button { target: "fraGeneral.btnReset" },
};

impl FraOnRecordComponents {
    /// O CheckBox deste form com o nome (`None` se não há um CheckBox com ele).
    pub fn find_check_box(&'static self, name: &str) -> Option<&'static CheckBox> {
        match name {
            "chkRecIndicator" => Some(&self.chkRecIndicator),
            "chkRecClickable" => Some(&self.chkRecClickable),
            "chkFullscreen" => Some(&self.chkFullscreen),
            "chkPlaySound" => Some(&self.chkPlaySound),
            "chkMinimize" => Some(&self.chkMinimize),
            "chkAutoRecord" => Some(&self.chkAutoRecord),
            "chkStopOnLock" => Some(&self.chkStopOnLock),
            _ => None,
        }
    }
    /// O CheckBox que disparou o evento (`None` se o `sender` não é um CheckBox deste form).
    pub fn sender_as_check_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static CheckBox> {
        self.find_check_box(ui.name(sender)?)
    }
    /// O Circle deste form com o nome (`None` se não há um Circle com ele).
    pub fn find_circle(&'static self, name: &str) -> Option<&'static Circle> {
        match name {
            "dotRecPreview" => Some(&self.dotRecPreview),
            _ => None,
        }
    }
    /// O Circle que disparou o evento (`None` se o `sender` não é um Circle deste form).
    pub fn sender_as_circle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Circle> {
        self.find_circle(ui.name(sender)?)
    }
    /// O ComboBox deste form com o nome (`None` se não há um ComboBox com ele).
    pub fn find_combo_box(&'static self, name: &str) -> Option<&'static ComboBox> {
        match name {
            "cmbRecCorner" => Some(&self.cmbRecCorner),
            _ => None,
        }
    }
    /// O ComboBox que disparou o evento (`None` se o `sender` não é um ComboBox deste form).
    pub fn sender_as_combo_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ComboBox> {
        self.find_combo_box(ui.name(sender)?)
    }
    /// O Edit deste form com o nome (`None` se não há um Edit com ele).
    pub fn find_edit(&'static self, name: &str) -> Option<&'static Edit> {
        match name {
            "edtAutoApps" => Some(&self.edtAutoApps),
            "edtAutoExcept" => Some(&self.edtAutoExcept),
            _ => None,
        }
    }
    /// O Edit que disparou o evento (`None` se o `sender` não é um Edit deste form).
    pub fn sender_as_edit(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Edit> {
        self.find_edit(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblRecIndicatorHint" => Some(&self.lblRecIndicatorHint),
            "lblRecClickableHint" => Some(&self.lblRecClickableHint),
            "lblRecCorner" => Some(&self.lblRecCorner),
            "lblRecOpacity" => Some(&self.lblRecOpacity),
            "lblRecOpacityPct" => Some(&self.lblRecOpacityPct),
            "lblRecPreview" => Some(&self.lblRecPreview),
            "lblFullscreenHint" => Some(&self.lblFullscreenHint),
            "lblPlaySoundHint" => Some(&self.lblPlaySoundHint),
            "lblMinimizeHint" => Some(&self.lblMinimizeHint),
            "lblAutoRecordHint" => Some(&self.lblAutoRecordHint),
            "lblAutoAppsHint" => Some(&self.lblAutoAppsHint),
            "lblAutoExceptHint" => Some(&self.lblAutoExceptHint),
            "lblAutoDevices" => Some(&self.lblAutoDevices),
            "lblAutoDevicesHint" => Some(&self.lblAutoDevicesHint),
            "lblStopOnLockHint" => Some(&self.lblStopOnLockHint),
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
            "lyRecOptions" => Some(&self.lyRecOptions),
            "lyRecCorner" => Some(&self.lyRecCorner),
            "lyRecOpacity" => Some(&self.lyRecOpacity),
            "lyRecOpacitySlider" => Some(&self.lyRecOpacitySlider),
            "lyAutoExcept" => Some(&self.lyAutoExcept),
            "lyAutoDevices" => Some(&self.lyAutoDevices),
            "lyAutoDeviceList" => Some(&self.lyAutoDeviceList),
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
            "pnlRecPreview" => Some(&self.pnlRecPreview),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnRule1" => Some(&self.lnRule1),
            "lnRule2" => Some(&self.lnRule2),
            "lnRule3" => Some(&self.lnRule3),
            "lnRule4" => Some(&self.lnRule4),
            "lnAutoDevices" => Some(&self.lnAutoDevices),
            "lnRule5" => Some(&self.lnRule5),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O TrackBar deste form com o nome (`None` se não há um TrackBar com ele).
    pub fn find_track_bar(&'static self, name: &str) -> Option<&'static TrackBar> {
        match name {
            "trkRecOpacity" => Some(&self.trkRecOpacity),
            _ => None,
        }
    }
    /// O TrackBar que disparou o evento (`None` se o `sender` não é um TrackBar deste form).
    pub fn sender_as_track_bar(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static TrackBar> {
        self.find_track_bar(ui.name(sender)?)
    }
}

/// O form fraOnRecord e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraOnRecordComponents {
    pub form: Form,
    pub chkRecIndicator: CheckBox,
    pub lblRecIndicatorHint: Label,
    pub lyRecOptions: Layout,
    pub chkRecClickable: CheckBox,
    pub lblRecClickableHint: Label,
    pub lyRecCorner: Layout,
    pub lblRecCorner: Label,
    pub cmbRecCorner: ComboBox,
    pub lyRecOpacity: Layout,
    pub lblRecOpacity: Label,
    pub lblRecOpacityPct: Label,
    pub pnlRecPreview: Panel,
    pub dotRecPreview: Circle,
    pub lblRecPreview: Label,
    pub lyRecOpacitySlider: Layout,
    pub trkRecOpacity: TrackBar,
    pub lnRule1: Rectangle,
    pub chkFullscreen: CheckBox,
    pub lblFullscreenHint: Label,
    pub lnRule2: Rectangle,
    pub chkPlaySound: CheckBox,
    pub lblPlaySoundHint: Label,
    pub lnRule3: Rectangle,
    pub chkMinimize: CheckBox,
    pub lblMinimizeHint: Label,
    pub lnRule4: Rectangle,
    pub chkAutoRecord: CheckBox,
    pub lblAutoRecordHint: Label,
    pub edtAutoApps: Edit,
    pub lblAutoAppsHint: Label,
    pub lyAutoExcept: Layout,
    pub edtAutoExcept: Edit,
    pub lblAutoExceptHint: Label,
    pub lyAutoDevices: Layout,
    pub lnAutoDevices: Rectangle,
    pub lblAutoDevices: Label,
    pub lblAutoDevicesHint: Label,
    pub lyAutoDeviceList: Layout,
    pub lnRule5: Rectangle,
    pub chkStopOnLock: CheckBox,
    pub lblStopOnLockHint: Label,
}

impl std::ops::Deref for FraOnRecordComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraOnRecord: FraOnRecordComponents = FraOnRecordComponents {
    form: Form { target: "fraOnRecord" },
    chkRecIndicator: CheckBox { target: "fraOnRecord.chkRecIndicator" },
    lblRecIndicatorHint: Label { target: "fraOnRecord.lblRecIndicatorHint" },
    lyRecOptions: Layout { target: "fraOnRecord.lyRecOptions" },
    chkRecClickable: CheckBox { target: "fraOnRecord.chkRecClickable" },
    lblRecClickableHint: Label { target: "fraOnRecord.lblRecClickableHint" },
    lyRecCorner: Layout { target: "fraOnRecord.lyRecCorner" },
    lblRecCorner: Label { target: "fraOnRecord.lblRecCorner" },
    cmbRecCorner: ComboBox { target: "fraOnRecord.cmbRecCorner" },
    lyRecOpacity: Layout { target: "fraOnRecord.lyRecOpacity" },
    lblRecOpacity: Label { target: "fraOnRecord.lblRecOpacity" },
    lblRecOpacityPct: Label { target: "fraOnRecord.lblRecOpacityPct" },
    pnlRecPreview: Panel { target: "fraOnRecord.pnlRecPreview" },
    dotRecPreview: Circle { target: "fraOnRecord.dotRecPreview" },
    lblRecPreview: Label { target: "fraOnRecord.lblRecPreview" },
    lyRecOpacitySlider: Layout { target: "fraOnRecord.lyRecOpacitySlider" },
    trkRecOpacity: TrackBar { target: "fraOnRecord.trkRecOpacity" },
    lnRule1: Rectangle { target: "fraOnRecord.lnRule1" },
    chkFullscreen: CheckBox { target: "fraOnRecord.chkFullscreen" },
    lblFullscreenHint: Label { target: "fraOnRecord.lblFullscreenHint" },
    lnRule2: Rectangle { target: "fraOnRecord.lnRule2" },
    chkPlaySound: CheckBox { target: "fraOnRecord.chkPlaySound" },
    lblPlaySoundHint: Label { target: "fraOnRecord.lblPlaySoundHint" },
    lnRule3: Rectangle { target: "fraOnRecord.lnRule3" },
    chkMinimize: CheckBox { target: "fraOnRecord.chkMinimize" },
    lblMinimizeHint: Label { target: "fraOnRecord.lblMinimizeHint" },
    lnRule4: Rectangle { target: "fraOnRecord.lnRule4" },
    chkAutoRecord: CheckBox { target: "fraOnRecord.chkAutoRecord" },
    lblAutoRecordHint: Label { target: "fraOnRecord.lblAutoRecordHint" },
    edtAutoApps: Edit { target: "fraOnRecord.edtAutoApps" },
    lblAutoAppsHint: Label { target: "fraOnRecord.lblAutoAppsHint" },
    lyAutoExcept: Layout { target: "fraOnRecord.lyAutoExcept" },
    edtAutoExcept: Edit { target: "fraOnRecord.edtAutoExcept" },
    lblAutoExceptHint: Label { target: "fraOnRecord.lblAutoExceptHint" },
    lyAutoDevices: Layout { target: "fraOnRecord.lyAutoDevices" },
    lnAutoDevices: Rectangle { target: "fraOnRecord.lnAutoDevices" },
    lblAutoDevices: Label { target: "fraOnRecord.lblAutoDevices" },
    lblAutoDevicesHint: Label { target: "fraOnRecord.lblAutoDevicesHint" },
    lyAutoDeviceList: Layout { target: "fraOnRecord.lyAutoDeviceList" },
    lnRule5: Rectangle { target: "fraOnRecord.lnRule5" },
    chkStopOnLock: CheckBox { target: "fraOnRecord.chkStopOnLock" },
    lblStopOnLockHint: Label { target: "fraOnRecord.lblStopOnLockHint" },
};

impl FraFilesComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnRecordDir" => Some(&self.btnRecordDir),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O ComboBox deste form com o nome (`None` se não há um ComboBox com ele).
    pub fn find_combo_box(&'static self, name: &str) -> Option<&'static ComboBox> {
        match name {
            "cmbLibrary" => Some(&self.cmbLibrary),
            _ => None,
        }
    }
    /// O ComboBox que disparou o evento (`None` se o `sender` não é um ComboBox deste form).
    pub fn sender_as_combo_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ComboBox> {
        self.find_combo_box(ui.name(sender)?)
    }
    /// O Edit deste form com o nome (`None` se não há um Edit com ele).
    pub fn find_edit(&'static self, name: &str) -> Option<&'static Edit> {
        match name {
            "edtRecordDir" => Some(&self.edtRecordDir),
            "edtFilename" => Some(&self.edtFilename),
            _ => None,
        }
    }
    /// O Edit que disparou o evento (`None` se o `sender` não é um Edit deste form).
    pub fn sender_as_edit(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Edit> {
        self.find_edit(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblRecordDir" => Some(&self.lblRecordDir),
            "lblFilename" => Some(&self.lblFilename),
            "lblFilenameHint" => Some(&self.lblFilenameHint),
            "lblFilenamePreview" => Some(&self.lblFilenamePreview),
            "lblLibrary" => Some(&self.lblLibrary),
            "lblLibraryHint" => Some(&self.lblLibraryHint),
            "lblHiddenFolders" => Some(&self.lblHiddenFolders),
            "lblHiddenFoldersHint" => Some(&self.lblHiddenFoldersHint),
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
            "lyRecordDir" => Some(&self.lyRecordDir),
            "lyHiddenFolders" => Some(&self.lyHiddenFolders),
            _ => None,
        }
    }
    /// O Layout que disparou o evento (`None` se o `sender` não é um Layout deste form).
    pub fn sender_as_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Layout> {
        self.find_layout(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnRule1" => Some(&self.lnRule1),
            "lnRule2" => Some(&self.lnRule2),
            "lnRule3" => Some(&self.lnRule3),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
}

/// O form fraFiles e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraFilesComponents {
    pub form: Form,
    pub lblRecordDir: Label,
    pub lyRecordDir: Layout,
    pub btnRecordDir: Button,
    pub edtRecordDir: Edit,
    pub lnRule1: Rectangle,
    pub lblFilename: Label,
    pub edtFilename: Edit,
    pub lblFilenameHint: Label,
    pub lblFilenamePreview: Label,
    pub lnRule2: Rectangle,
    pub lblLibrary: Label,
    pub cmbLibrary: ComboBox,
    pub lblLibraryHint: Label,
    pub lnRule3: Rectangle,
    pub lblHiddenFolders: Label,
    pub lblHiddenFoldersHint: Label,
    pub lyHiddenFolders: Layout,
}

impl std::ops::Deref for FraFilesComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraFiles: FraFilesComponents = FraFilesComponents {
    form: Form { target: "fraFiles" },
    lblRecordDir: Label { target: "fraFiles.lblRecordDir" },
    lyRecordDir: Layout { target: "fraFiles.lyRecordDir" },
    btnRecordDir: Button { target: "fraFiles.btnRecordDir" },
    edtRecordDir: Edit { target: "fraFiles.edtRecordDir" },
    lnRule1: Rectangle { target: "fraFiles.lnRule1" },
    lblFilename: Label { target: "fraFiles.lblFilename" },
    edtFilename: Edit { target: "fraFiles.edtFilename" },
    lblFilenameHint: Label { target: "fraFiles.lblFilenameHint" },
    lblFilenamePreview: Label { target: "fraFiles.lblFilenamePreview" },
    lnRule2: Rectangle { target: "fraFiles.lnRule2" },
    lblLibrary: Label { target: "fraFiles.lblLibrary" },
    cmbLibrary: ComboBox { target: "fraFiles.cmbLibrary" },
    lblLibraryHint: Label { target: "fraFiles.lblLibraryHint" },
    lnRule3: Rectangle { target: "fraFiles.lnRule3" },
    lblHiddenFolders: Label { target: "fraFiles.lblHiddenFolders" },
    lblHiddenFoldersHint: Label { target: "fraFiles.lblHiddenFoldersHint" },
    lyHiddenFolders: Layout { target: "fraFiles.lyHiddenFolders" },
};

impl FraVideoComponents {
    /// O ComboBox deste form com o nome (`None` se não há um ComboBox com ele).
    pub fn find_combo_box(&'static self, name: &str) -> Option<&'static ComboBox> {
        match name {
            "cmbCodec" => Some(&self.cmbCodec),
            _ => None,
        }
    }
    /// O ComboBox que disparou o evento (`None` se o `sender` não é um ComboBox deste form).
    pub fn sender_as_combo_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ComboBox> {
        self.find_combo_box(ui.name(sender)?)
    }
    /// O Image deste form com o nome (`None` se não há um Image com ele).
    pub fn find_image(&'static self, name: &str) -> Option<&'static Image> {
        match name {
            "imgGpu" => Some(&self.imgGpu),
            _ => None,
        }
    }
    /// O Image que disparou o evento (`None` se o `sender` não é um Image deste form).
    pub fn sender_as_image(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Image> {
        self.find_image(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblCodec" => Some(&self.lblCodec),
            "lblCodecMaxRes" => Some(&self.lblCodecMaxRes),
            "lblCodecSwWarn" => Some(&self.lblCodecSwWarn),
            "lblCodecHint" => Some(&self.lblCodecHint),
            "lblQuality" => Some(&self.lblQuality),
            "lblQualityLow" => Some(&self.lblQualityLow),
            "lblQualityHigh" => Some(&self.lblQualityHigh),
            "lblQualityHint" => Some(&self.lblQualityHint),
            "lblQualitySvtFloor" => Some(&self.lblQualitySvtFloor),
            "lblVideoTest" => Some(&self.lblVideoTest),
            "lblVideoTestHint" => Some(&self.lblVideoTestHint),
            "lblFps" => Some(&self.lblFps),
            "lblFpsLow" => Some(&self.lblFpsLow),
            "lblFpsHigh" => Some(&self.lblFpsHigh),
            "lblFpsHint" => Some(&self.lblFpsHint),
            "lblKeyframe" => Some(&self.lblKeyframe),
            "lblKeyframeLow" => Some(&self.lblKeyframeLow),
            "lblKeyframeHigh" => Some(&self.lblKeyframeHigh),
            "lblKeyframeHint" => Some(&self.lblKeyframeHint),
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
            "lyCodec" => Some(&self.lyCodec),
            "lylblCodecSwWarn" => Some(&self.lylblCodecSwWarn),
            "lyQuality" => Some(&self.lyQuality),
            "lyQualityTicks" => Some(&self.lyQualityTicks),
            "lylblQualitySvtFloor" => Some(&self.lylblQualitySvtFloor),
            "lyVideoTest" => Some(&self.lyVideoTest),
            "lyFps" => Some(&self.lyFps),
            "lyFpsTicks" => Some(&self.lyFpsTicks),
            "lyKeyframe" => Some(&self.lyKeyframe),
            "lyKeyframeTicks" => Some(&self.lyKeyframeTicks),
            _ => None,
        }
    }
    /// O Layout que disparou o evento (`None` se o `sender` não é um Layout deste form).
    pub fn sender_as_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Layout> {
        self.find_layout(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "barlblCodecSwWarn" => Some(&self.barlblCodecSwWarn),
            "lnRule1" => Some(&self.lnRule1),
            "barlblQualitySvtFloor" => Some(&self.barlblQualitySvtFloor),
            "lnRule2" => Some(&self.lnRule2),
            "lnRule3" => Some(&self.lnRule3),
            "lnRule4" => Some(&self.lnRule4),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O TrackBar deste form com o nome (`None` se não há um TrackBar com ele).
    pub fn find_track_bar(&'static self, name: &str) -> Option<&'static TrackBar> {
        match name {
            "trkQuality" => Some(&self.trkQuality),
            "trkFps" => Some(&self.trkFps),
            "trkKeyframe" => Some(&self.trkKeyframe),
            _ => None,
        }
    }
    /// O TrackBar que disparou o evento (`None` se o `sender` não é um TrackBar deste form).
    pub fn sender_as_track_bar(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static TrackBar> {
        self.find_track_bar(ui.name(sender)?)
    }
}

/// O form fraVideo e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraVideoComponents {
    pub form: Form,
    pub lblCodec: Label,
    pub lyCodec: Layout,
    pub imgGpu: Image,
    pub cmbCodec: ComboBox,
    pub lblCodecMaxRes: Label,
    pub lylblCodecSwWarn: Layout,
    pub barlblCodecSwWarn: Rectangle,
    pub lblCodecSwWarn: Label,
    pub lblCodecHint: Label,
    pub lnRule1: Rectangle,
    pub lblQuality: Label,
    pub lyQuality: Layout,
    pub lblQualityLow: Label,
    pub lblQualityHigh: Label,
    pub lyQualityTicks: Layout,
    pub trkQuality: TrackBar,
    pub lblQualityHint: Label,
    pub lylblQualitySvtFloor: Layout,
    pub barlblQualitySvtFloor: Rectangle,
    pub lblQualitySvtFloor: Label,
    pub lnRule2: Rectangle,
    pub lblVideoTest: Label,
    pub lblVideoTestHint: Label,
    pub lyVideoTest: Layout,
    pub lnRule3: Rectangle,
    pub lblFps: Label,
    pub lyFps: Layout,
    pub lblFpsLow: Label,
    pub lblFpsHigh: Label,
    pub lyFpsTicks: Layout,
    pub trkFps: TrackBar,
    pub lblFpsHint: Label,
    pub lnRule4: Rectangle,
    pub lblKeyframe: Label,
    pub lyKeyframe: Layout,
    pub lblKeyframeLow: Label,
    pub lblKeyframeHigh: Label,
    pub lyKeyframeTicks: Layout,
    pub trkKeyframe: TrackBar,
    pub lblKeyframeHint: Label,
}

impl std::ops::Deref for FraVideoComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraVideo: FraVideoComponents = FraVideoComponents {
    form: Form { target: "fraVideo" },
    lblCodec: Label { target: "fraVideo.lblCodec" },
    lyCodec: Layout { target: "fraVideo.lyCodec" },
    imgGpu: Image { target: "fraVideo.imgGpu" },
    cmbCodec: ComboBox { target: "fraVideo.cmbCodec" },
    lblCodecMaxRes: Label { target: "fraVideo.lblCodecMaxRes" },
    lylblCodecSwWarn: Layout { target: "fraVideo.lylblCodecSwWarn" },
    barlblCodecSwWarn: Rectangle { target: "fraVideo.barlblCodecSwWarn" },
    lblCodecSwWarn: Label { target: "fraVideo.lblCodecSwWarn" },
    lblCodecHint: Label { target: "fraVideo.lblCodecHint" },
    lnRule1: Rectangle { target: "fraVideo.lnRule1" },
    lblQuality: Label { target: "fraVideo.lblQuality" },
    lyQuality: Layout { target: "fraVideo.lyQuality" },
    lblQualityLow: Label { target: "fraVideo.lblQualityLow" },
    lblQualityHigh: Label { target: "fraVideo.lblQualityHigh" },
    lyQualityTicks: Layout { target: "fraVideo.lyQualityTicks" },
    trkQuality: TrackBar { target: "fraVideo.trkQuality" },
    lblQualityHint: Label { target: "fraVideo.lblQualityHint" },
    lylblQualitySvtFloor: Layout { target: "fraVideo.lylblQualitySvtFloor" },
    barlblQualitySvtFloor: Rectangle { target: "fraVideo.barlblQualitySvtFloor" },
    lblQualitySvtFloor: Label { target: "fraVideo.lblQualitySvtFloor" },
    lnRule2: Rectangle { target: "fraVideo.lnRule2" },
    lblVideoTest: Label { target: "fraVideo.lblVideoTest" },
    lblVideoTestHint: Label { target: "fraVideo.lblVideoTestHint" },
    lyVideoTest: Layout { target: "fraVideo.lyVideoTest" },
    lnRule3: Rectangle { target: "fraVideo.lnRule3" },
    lblFps: Label { target: "fraVideo.lblFps" },
    lyFps: Layout { target: "fraVideo.lyFps" },
    lblFpsLow: Label { target: "fraVideo.lblFpsLow" },
    lblFpsHigh: Label { target: "fraVideo.lblFpsHigh" },
    lyFpsTicks: Layout { target: "fraVideo.lyFpsTicks" },
    trkFps: TrackBar { target: "fraVideo.trkFps" },
    lblFpsHint: Label { target: "fraVideo.lblFpsHint" },
    lnRule4: Rectangle { target: "fraVideo.lnRule4" },
    lblKeyframe: Label { target: "fraVideo.lblKeyframe" },
    lyKeyframe: Layout { target: "fraVideo.lyKeyframe" },
    lblKeyframeLow: Label { target: "fraVideo.lblKeyframeLow" },
    lblKeyframeHigh: Label { target: "fraVideo.lblKeyframeHigh" },
    lyKeyframeTicks: Layout { target: "fraVideo.lyKeyframeTicks" },
    trkKeyframe: TrackBar { target: "fraVideo.trkKeyframe" },
    lblKeyframeHint: Label { target: "fraVideo.lblKeyframeHint" },
};

impl FraAudioComponents {
    /// O CheckBox deste form com o nome (`None` se não há um CheckBox com ele).
    pub fn find_check_box(&'static self, name: &str) -> Option<&'static CheckBox> {
        match name {
            "chkMuteWhenMuted" => Some(&self.chkMuteWhenMuted),
            _ => None,
        }
    }
    /// O CheckBox que disparou o evento (`None` se o `sender` não é um CheckBox deste form).
    pub fn sender_as_check_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static CheckBox> {
        self.find_check_box(ui.name(sender)?)
    }
    /// O ComboBox deste form com o nome (`None` se não há um ComboBox com ele).
    pub fn find_combo_box(&'static self, name: &str) -> Option<&'static ComboBox> {
        match name {
            "cmbAudioBitrate" => Some(&self.cmbAudioBitrate),
            _ => None,
        }
    }
    /// O ComboBox que disparou o evento (`None` se o `sender` não é um ComboBox deste form).
    pub fn sender_as_combo_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ComboBox> {
        self.find_combo_box(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblAudioBitrate" => Some(&self.lblAudioBitrate),
            "lblAudioBitrateHint" => Some(&self.lblAudioBitrateHint),
            "lblMuteWhenMutedHint" => Some(&self.lblMuteWhenMutedHint),
            _ => None,
        }
    }
    /// O Label que disparou o evento (`None` se o `sender` não é um Label deste form).
    pub fn sender_as_label(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Label> {
        self.find_label(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnRule1" => Some(&self.lnRule1),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
}

/// O form fraAudio e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraAudioComponents {
    pub form: Form,
    pub lblAudioBitrate: Label,
    pub cmbAudioBitrate: ComboBox,
    pub lblAudioBitrateHint: Label,
    pub lnRule1: Rectangle,
    pub chkMuteWhenMuted: CheckBox,
    pub lblMuteWhenMutedHint: Label,
}

impl std::ops::Deref for FraAudioComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraAudio: FraAudioComponents = FraAudioComponents {
    form: Form { target: "fraAudio" },
    lblAudioBitrate: Label { target: "fraAudio.lblAudioBitrate" },
    cmbAudioBitrate: ComboBox { target: "fraAudio.cmbAudioBitrate" },
    lblAudioBitrateHint: Label { target: "fraAudio.lblAudioBitrateHint" },
    lnRule1: Rectangle { target: "fraAudio.lnRule1" },
    chkMuteWhenMuted: CheckBox { target: "fraAudio.chkMuteWhenMuted" },
    lblMuteWhenMutedHint: Label { target: "fraAudio.lblMuteWhenMutedHint" },
};

impl FraReplayComponents {
    /// O CheckBox deste form com o nome (`None` se não há um CheckBox com ele).
    pub fn find_check_box(&'static self, name: &str) -> Option<&'static CheckBox> {
        match name {
            "chkReplayAutoStart" => Some(&self.chkReplayAutoStart),
            "chkReplayIntoRecording" => Some(&self.chkReplayIntoRecording),
            "chkReplayIndicator" => Some(&self.chkReplayIndicator),
            "chkRepClickable" => Some(&self.chkRepClickable),
            _ => None,
        }
    }
    /// O CheckBox que disparou o evento (`None` se o `sender` não é um CheckBox deste form).
    pub fn sender_as_check_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static CheckBox> {
        self.find_check_box(ui.name(sender)?)
    }
    /// O Circle deste form com o nome (`None` se não há um Circle com ele).
    pub fn find_circle(&'static self, name: &str) -> Option<&'static Circle> {
        match name {
            "dotRepPreview" => Some(&self.dotRepPreview),
            _ => None,
        }
    }
    /// O Circle que disparou o evento (`None` se o `sender` não é um Circle deste form).
    pub fn sender_as_circle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Circle> {
        self.find_circle(ui.name(sender)?)
    }
    /// O ComboBox deste form com o nome (`None` se não há um ComboBox com ele).
    pub fn find_combo_box(&'static self, name: &str) -> Option<&'static ComboBox> {
        match name {
            "cmbReplaySeconds" => Some(&self.cmbReplaySeconds),
            "cmbReplayMemory" => Some(&self.cmbReplayMemory),
            "cmbRepCorner" => Some(&self.cmbRepCorner),
            _ => None,
        }
    }
    /// O ComboBox que disparou o evento (`None` se o `sender` não é um ComboBox deste form).
    pub fn sender_as_combo_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ComboBox> {
        self.find_combo_box(ui.name(sender)?)
    }
    /// O Edit deste form com o nome (`None` se não há um Edit com ele).
    pub fn find_edit(&'static self, name: &str) -> Option<&'static Edit> {
        match name {
            "edtReplayApps" => Some(&self.edtReplayApps),
            _ => None,
        }
    }
    /// O Edit que disparou o evento (`None` se o `sender` não é um Edit deste form).
    pub fn sender_as_edit(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Edit> {
        self.find_edit(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblReplay" => Some(&self.lblReplay),
            "lblReplayHint" => Some(&self.lblReplayHint),
            "lblReplayAutoStartHint" => Some(&self.lblReplayAutoStartHint),
            "lblReplayIntoRecordingHint" => Some(&self.lblReplayIntoRecordingHint),
            "lblReplayDuration" => Some(&self.lblReplayDuration),
            "lblReplayMemory" => Some(&self.lblReplayMemory),
            "lblReplayMemoryHint" => Some(&self.lblReplayMemoryHint),
            "lblReplayApps" => Some(&self.lblReplayApps),
            "lblReplayAppsHint" => Some(&self.lblReplayAppsHint),
            "lblReplayIndicatorHint" => Some(&self.lblReplayIndicatorHint),
            "lblRepClickableHint" => Some(&self.lblRepClickableHint),
            "lblRepCorner" => Some(&self.lblRepCorner),
            "lblRepOpacity" => Some(&self.lblRepOpacity),
            "lblRepOpacityPct" => Some(&self.lblRepOpacityPct),
            "lblRepPreview" => Some(&self.lblRepPreview),
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
            "lyReplayOptions" => Some(&self.lyReplayOptions),
            "lyReplayHeads" => Some(&self.lyReplayHeads),
            "lyReplayLimits" => Some(&self.lyReplayLimits),
            "lyRepOptions" => Some(&self.lyRepOptions),
            "lyRepCorner" => Some(&self.lyRepCorner),
            "lyRepOpacity" => Some(&self.lyRepOpacity),
            "lyRepOpacitySlider" => Some(&self.lyRepOpacitySlider),
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
            "pnlRepPreview" => Some(&self.pnlRepPreview),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnRule1" => Some(&self.lnRule1),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
    /// O TrackBar deste form com o nome (`None` se não há um TrackBar com ele).
    pub fn find_track_bar(&'static self, name: &str) -> Option<&'static TrackBar> {
        match name {
            "trkRepOpacity" => Some(&self.trkRepOpacity),
            _ => None,
        }
    }
    /// O TrackBar que disparou o evento (`None` se o `sender` não é um TrackBar deste form).
    pub fn sender_as_track_bar(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static TrackBar> {
        self.find_track_bar(ui.name(sender)?)
    }
}

/// O form fraReplay e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraReplayComponents {
    pub form: Form,
    pub lblReplay: Label,
    pub lblReplayHint: Label,
    pub lyReplayOptions: Layout,
    pub lnRule1: Rectangle,
    pub chkReplayAutoStart: CheckBox,
    pub lblReplayAutoStartHint: Label,
    pub chkReplayIntoRecording: CheckBox,
    pub lblReplayIntoRecordingHint: Label,
    pub lyReplayHeads: Layout,
    pub lblReplayDuration: Label,
    pub lblReplayMemory: Label,
    pub lyReplayLimits: Layout,
    pub cmbReplaySeconds: ComboBox,
    pub cmbReplayMemory: ComboBox,
    pub lblReplayMemoryHint: Label,
    pub lblReplayApps: Label,
    pub edtReplayApps: Edit,
    pub lblReplayAppsHint: Label,
    pub chkReplayIndicator: CheckBox,
    pub lblReplayIndicatorHint: Label,
    pub lyRepOptions: Layout,
    pub chkRepClickable: CheckBox,
    pub lblRepClickableHint: Label,
    pub lyRepCorner: Layout,
    pub lblRepCorner: Label,
    pub cmbRepCorner: ComboBox,
    pub lyRepOpacity: Layout,
    pub lblRepOpacity: Label,
    pub lblRepOpacityPct: Label,
    pub pnlRepPreview: Panel,
    pub dotRepPreview: Circle,
    pub lblRepPreview: Label,
    pub lyRepOpacitySlider: Layout,
    pub trkRepOpacity: TrackBar,
}

impl std::ops::Deref for FraReplayComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraReplay: FraReplayComponents = FraReplayComponents {
    form: Form { target: "fraReplay" },
    lblReplay: Label { target: "fraReplay.lblReplay" },
    lblReplayHint: Label { target: "fraReplay.lblReplayHint" },
    lyReplayOptions: Layout { target: "fraReplay.lyReplayOptions" },
    lnRule1: Rectangle { target: "fraReplay.lnRule1" },
    chkReplayAutoStart: CheckBox { target: "fraReplay.chkReplayAutoStart" },
    lblReplayAutoStartHint: Label { target: "fraReplay.lblReplayAutoStartHint" },
    chkReplayIntoRecording: CheckBox { target: "fraReplay.chkReplayIntoRecording" },
    lblReplayIntoRecordingHint: Label { target: "fraReplay.lblReplayIntoRecordingHint" },
    lyReplayHeads: Layout { target: "fraReplay.lyReplayHeads" },
    lblReplayDuration: Label { target: "fraReplay.lblReplayDuration" },
    lblReplayMemory: Label { target: "fraReplay.lblReplayMemory" },
    lyReplayLimits: Layout { target: "fraReplay.lyReplayLimits" },
    cmbReplaySeconds: ComboBox { target: "fraReplay.cmbReplaySeconds" },
    cmbReplayMemory: ComboBox { target: "fraReplay.cmbReplayMemory" },
    lblReplayMemoryHint: Label { target: "fraReplay.lblReplayMemoryHint" },
    lblReplayApps: Label { target: "fraReplay.lblReplayApps" },
    edtReplayApps: Edit { target: "fraReplay.edtReplayApps" },
    lblReplayAppsHint: Label { target: "fraReplay.lblReplayAppsHint" },
    chkReplayIndicator: CheckBox { target: "fraReplay.chkReplayIndicator" },
    lblReplayIndicatorHint: Label { target: "fraReplay.lblReplayIndicatorHint" },
    lyRepOptions: Layout { target: "fraReplay.lyRepOptions" },
    chkRepClickable: CheckBox { target: "fraReplay.chkRepClickable" },
    lblRepClickableHint: Label { target: "fraReplay.lblRepClickableHint" },
    lyRepCorner: Layout { target: "fraReplay.lyRepCorner" },
    lblRepCorner: Label { target: "fraReplay.lblRepCorner" },
    cmbRepCorner: ComboBox { target: "fraReplay.cmbRepCorner" },
    lyRepOpacity: Layout { target: "fraReplay.lyRepOpacity" },
    lblRepOpacity: Label { target: "fraReplay.lblRepOpacity" },
    lblRepOpacityPct: Label { target: "fraReplay.lblRepOpacityPct" },
    pnlRepPreview: Panel { target: "fraReplay.pnlRepPreview" },
    dotRepPreview: Circle { target: "fraReplay.dotRepPreview" },
    lblRepPreview: Label { target: "fraReplay.lblRepPreview" },
    lyRepOpacitySlider: Layout { target: "fraReplay.lyRepOpacitySlider" },
    trkRepOpacity: TrackBar { target: "fraReplay.trkRepOpacity" },
};

impl FraHotkeysComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnHotkeyAdd" => Some(&self.btnHotkeyAdd),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblHotkeysHint" => Some(&self.lblHotkeysHint),
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
            "lyHotkeys" => Some(&self.lyHotkeys),
            "lyHotkeysAdd" => Some(&self.lyHotkeysAdd),
            _ => None,
        }
    }
    /// O Layout que disparou o evento (`None` se o `sender` não é um Layout deste form).
    pub fn sender_as_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Layout> {
        self.find_layout(ui.name(sender)?)
    }
}

/// O form fraHotkeys e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraHotkeysComponents {
    pub form: Form,
    pub lblHotkeysHint: Label,
    pub lyHotkeys: Layout,
    pub lyHotkeysAdd: Layout,
    pub btnHotkeyAdd: Button,
}

impl std::ops::Deref for FraHotkeysComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraHotkeys: FraHotkeysComponents = FraHotkeysComponents {
    form: Form { target: "fraHotkeys" },
    lblHotkeysHint: Label { target: "fraHotkeys.lblHotkeysHint" },
    lyHotkeys: Layout { target: "fraHotkeys.lyHotkeys" },
    lyHotkeysAdd: Layout { target: "fraHotkeys.lyHotkeysAdd" },
    btnHotkeyAdd: Button { target: "fraHotkeys.btnHotkeyAdd" },
};

impl FraDevicesComponents {
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblDevicesHint" => Some(&self.lblDevicesHint),
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
            "lyDevices" => Some(&self.lyDevices),
            _ => None,
        }
    }
    /// O Layout que disparou o evento (`None` se o `sender` não é um Layout deste form).
    pub fn sender_as_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Layout> {
        self.find_layout(ui.name(sender)?)
    }
}

/// O form fraDevices e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraDevicesComponents {
    pub form: Form,
    pub lblDevicesHint: Label,
    pub lyDevices: Layout,
}

impl std::ops::Deref for FraDevicesComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraDevices: FraDevicesComponents = FraDevicesComponents {
    form: Form { target: "fraDevices" },
    lblDevicesHint: Label { target: "fraDevices.lblDevicesHint" },
    lyDevices: Layout { target: "fraDevices.lyDevices" },
};

impl FraTranscribeComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnTrEngine" => Some(&self.btnTrEngine),
            "btnTrPending" => Some(&self.btnTrPending),
            "btnTrCancel" => Some(&self.btnTrCancel),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O CheckBox deste form com o nome (`None` se não há um CheckBox com ele).
    pub fn find_check_box(&'static self, name: &str) -> Option<&'static CheckBox> {
        match name {
            "chkTrOnStop" => Some(&self.chkTrOnStop),
            _ => None,
        }
    }
    /// O CheckBox que disparou o evento (`None` se o `sender` não é um CheckBox deste form).
    pub fn sender_as_check_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static CheckBox> {
        self.find_check_box(ui.name(sender)?)
    }
    /// O ComboBox deste form com o nome (`None` se não há um ComboBox com ele).
    pub fn find_combo_box(&'static self, name: &str) -> Option<&'static ComboBox> {
        match name {
            "cmbTrLanguage" => Some(&self.cmbTrLanguage),
            _ => None,
        }
    }
    /// O ComboBox que disparou o evento (`None` se o `sender` não é um ComboBox deste form).
    pub fn sender_as_combo_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ComboBox> {
        self.find_combo_box(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblTrLocal" => Some(&self.lblTrLocal),
            "lblTrLocalHint" => Some(&self.lblTrLocalHint),
            "lblTrStatus" => Some(&self.lblTrStatus),
            "lblTrDetail" => Some(&self.lblTrDetail),
            "lblTrLanguage" => Some(&self.lblTrLanguage),
            "lblTrLanguageHint" => Some(&self.lblTrLanguageHint),
            "lblTrOnStopHint" => Some(&self.lblTrOnStopHint),
            "lblTrQueue" => Some(&self.lblTrQueue),
            "lblTrLine" => Some(&self.lblTrLine),
            "lblTrError" => Some(&self.lblTrError),
            "lblTrCloud" => Some(&self.lblTrCloud),
            "lblTrQueueHint" => Some(&self.lblTrQueueHint),
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
            "lyTrEngineButtons" => Some(&self.lyTrEngineButtons),
            "lyTrQueueButtons" => Some(&self.lyTrQueueButtons),
            "lyTrProgress" => Some(&self.lyTrProgress),
            "lyTrQueue" => Some(&self.lyTrQueue),
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
            "pnlTrEngine" => Some(&self.pnlTrEngine),
            _ => None,
        }
    }
    /// O Panel que disparou o evento (`None` se o `sender` não é um Panel deste form).
    pub fn sender_as_panel(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Panel> {
        self.find_panel(ui.name(sender)?)
    }
    /// O ProgressBar deste form com o nome (`None` se não há um ProgressBar com ele).
    pub fn find_progress_bar(&'static self, name: &str) -> Option<&'static ProgressBar> {
        match name {
            "prgTrInstall" => Some(&self.prgTrInstall),
            "prgTrQueue" => Some(&self.prgTrQueue),
            _ => None,
        }
    }
    /// O ProgressBar que disparou o evento (`None` se o `sender` não é um ProgressBar deste form).
    pub fn sender_as_progress_bar(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static ProgressBar> {
        self.find_progress_bar(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnRule1" => Some(&self.lnRule1),
            "lnRule2" => Some(&self.lnRule2),
            "lnRule3" => Some(&self.lnRule3),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
}

/// O form fraTranscribe e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraTranscribeComponents {
    pub form: Form,
    pub lblTrLocal: Label,
    pub lblTrLocalHint: Label,
    pub pnlTrEngine: Panel,
    pub lblTrStatus: Label,
    pub prgTrInstall: ProgressBar,
    pub lblTrDetail: Label,
    pub lyTrEngineButtons: Layout,
    pub btnTrEngine: Button,
    pub lnRule1: Rectangle,
    pub lblTrLanguage: Label,
    pub cmbTrLanguage: ComboBox,
    pub lblTrLanguageHint: Label,
    pub lnRule2: Rectangle,
    pub chkTrOnStop: CheckBox,
    pub lblTrOnStopHint: Label,
    pub lnRule3: Rectangle,
    pub lblTrQueue: Label,
    pub lyTrQueueButtons: Layout,
    pub btnTrPending: Button,
    pub btnTrCancel: Button,
    pub lyTrProgress: Layout,
    pub prgTrQueue: ProgressBar,
    pub lblTrLine: Label,
    pub lblTrError: Label,
    pub lyTrQueue: Layout,
    pub lblTrCloud: Label,
    pub lblTrQueueHint: Label,
}

impl std::ops::Deref for FraTranscribeComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraTranscribe: FraTranscribeComponents = FraTranscribeComponents {
    form: Form { target: "fraTranscribe" },
    lblTrLocal: Label { target: "fraTranscribe.lblTrLocal" },
    lblTrLocalHint: Label { target: "fraTranscribe.lblTrLocalHint" },
    pnlTrEngine: Panel { target: "fraTranscribe.pnlTrEngine" },
    lblTrStatus: Label { target: "fraTranscribe.lblTrStatus" },
    prgTrInstall: ProgressBar { target: "fraTranscribe.prgTrInstall" },
    lblTrDetail: Label { target: "fraTranscribe.lblTrDetail" },
    lyTrEngineButtons: Layout { target: "fraTranscribe.lyTrEngineButtons" },
    btnTrEngine: Button { target: "fraTranscribe.btnTrEngine" },
    lnRule1: Rectangle { target: "fraTranscribe.lnRule1" },
    lblTrLanguage: Label { target: "fraTranscribe.lblTrLanguage" },
    cmbTrLanguage: ComboBox { target: "fraTranscribe.cmbTrLanguage" },
    lblTrLanguageHint: Label { target: "fraTranscribe.lblTrLanguageHint" },
    lnRule2: Rectangle { target: "fraTranscribe.lnRule2" },
    chkTrOnStop: CheckBox { target: "fraTranscribe.chkTrOnStop" },
    lblTrOnStopHint: Label { target: "fraTranscribe.lblTrOnStopHint" },
    lnRule3: Rectangle { target: "fraTranscribe.lnRule3" },
    lblTrQueue: Label { target: "fraTranscribe.lblTrQueue" },
    lyTrQueueButtons: Layout { target: "fraTranscribe.lyTrQueueButtons" },
    btnTrPending: Button { target: "fraTranscribe.btnTrPending" },
    btnTrCancel: Button { target: "fraTranscribe.btnTrCancel" },
    lyTrProgress: Layout { target: "fraTranscribe.lyTrProgress" },
    prgTrQueue: ProgressBar { target: "fraTranscribe.prgTrQueue" },
    lblTrLine: Label { target: "fraTranscribe.lblTrLine" },
    lblTrError: Label { target: "fraTranscribe.lblTrError" },
    lyTrQueue: Layout { target: "fraTranscribe.lyTrQueue" },
    lblTrCloud: Label { target: "fraTranscribe.lblTrCloud" },
    lblTrQueueHint: Label { target: "fraTranscribe.lblTrQueueHint" },
};

impl FraUpdatesComponents {
    /// O Button deste form com o nome (`None` se não há um Button com ele).
    pub fn find_button(&'static self, name: &str) -> Option<&'static Button> {
        match name {
            "btnCheckNow" => Some(&self.btnCheckNow),
            _ => None,
        }
    }
    /// O Button que disparou o evento (`None` se o `sender` não é um Button deste form).
    pub fn sender_as_button(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Button> {
        self.find_button(ui.name(sender)?)
    }
    /// O CheckBox deste form com o nome (`None` se não há um CheckBox com ele).
    pub fn find_check_box(&'static self, name: &str) -> Option<&'static CheckBox> {
        match name {
            "chkCheckUpdates" => Some(&self.chkCheckUpdates),
            _ => None,
        }
    }
    /// O CheckBox que disparou o evento (`None` se o `sender` não é um CheckBox deste form).
    pub fn sender_as_check_box(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static CheckBox> {
        self.find_check_box(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblVersion" => Some(&self.lblVersion),
            "lblVersionValue" => Some(&self.lblVersionValue),
            "lblCheckUpdatesHint" => Some(&self.lblCheckUpdatesHint),
            "lblUpdateStatus" => Some(&self.lblUpdateStatus),
            "lblUpdateOpen" => Some(&self.lblUpdateOpen),
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
            "lyCheckNow" => Some(&self.lyCheckNow),
            "lyUpdateStatus" => Some(&self.lyUpdateStatus),
            _ => None,
        }
    }
    /// O Layout que disparou o evento (`None` se o `sender` não é um Layout deste form).
    pub fn sender_as_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Layout> {
        self.find_layout(ui.name(sender)?)
    }
    /// O Rectangle deste form com o nome (`None` se não há um Rectangle com ele).
    pub fn find_rectangle(&'static self, name: &str) -> Option<&'static Rectangle> {
        match name {
            "lnRule1" => Some(&self.lnRule1),
            "lnRule2" => Some(&self.lnRule2),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
}

/// O form fraUpdates e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraUpdatesComponents {
    pub form: Form,
    pub lblVersion: Label,
    pub lblVersionValue: Label,
    pub lnRule1: Rectangle,
    pub chkCheckUpdates: CheckBox,
    pub lblCheckUpdatesHint: Label,
    pub lnRule2: Rectangle,
    pub lyCheckNow: Layout,
    pub btnCheckNow: Button,
    pub lyUpdateStatus: Layout,
    pub lblUpdateStatus: Label,
    pub lblUpdateOpen: Label,
}

impl std::ops::Deref for FraUpdatesComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraUpdates: FraUpdatesComponents = FraUpdatesComponents {
    form: Form { target: "fraUpdates" },
    lblVersion: Label { target: "fraUpdates.lblVersion" },
    lblVersionValue: Label { target: "fraUpdates.lblVersionValue" },
    lnRule1: Rectangle { target: "fraUpdates.lnRule1" },
    chkCheckUpdates: CheckBox { target: "fraUpdates.chkCheckUpdates" },
    lblCheckUpdatesHint: Label { target: "fraUpdates.lblCheckUpdatesHint" },
    lnRule2: Rectangle { target: "fraUpdates.lnRule2" },
    lyCheckNow: Layout { target: "fraUpdates.lyCheckNow" },
    btnCheckNow: Button { target: "fraUpdates.btnCheckNow" },
    lyUpdateStatus: Layout { target: "fraUpdates.lyUpdateStatus" },
    lblUpdateStatus: Label { target: "fraUpdates.lblUpdateStatus" },
    lblUpdateOpen: Label { target: "fraUpdates.lblUpdateOpen" },
};

impl FraAboutComponents {
    /// O Image deste form com o nome (`None` se não há um Image com ele).
    pub fn find_image(&'static self, name: &str) -> Option<&'static Image> {
        match name {
            "imgAbout" => Some(&self.imgAbout),
            _ => None,
        }
    }
    /// O Image que disparou o evento (`None` se o `sender` não é um Image deste form).
    pub fn sender_as_image(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Image> {
        self.find_image(ui.name(sender)?)
    }
    /// O Label deste form com o nome (`None` se não há um Label com ele).
    pub fn find_label(&'static self, name: &str) -> Option<&'static Label> {
        match name {
            "lblAboutName" => Some(&self.lblAboutName),
            "lblAboutTagline" => Some(&self.lblAboutTagline),
            "lblAboutIntro1" => Some(&self.lblAboutIntro1),
            "lblAboutIntro2" => Some(&self.lblAboutIntro2),
            "lblAboutIntro3" => Some(&self.lblAboutIntro3),
            "lblAboutAuthor" => Some(&self.lblAboutAuthor),
            "lblAboutAuthorValue" => Some(&self.lblAboutAuthorValue),
            "lblAboutCoAuthor" => Some(&self.lblAboutCoAuthor),
            "lblAboutCoAuthorValue" => Some(&self.lblAboutCoAuthorValue),
            "lblAboutCoAuthorTag" => Some(&self.lblAboutCoAuthorTag),
            "lblAboutRepository" => Some(&self.lblAboutRepository),
            "lblAboutRepositoryValue" => Some(&self.lblAboutRepositoryValue),
            "lblAboutFooter" => Some(&self.lblAboutFooter),
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
            "lyAboutAuthor" => Some(&self.lyAboutAuthor),
            "lyAboutCoAuthor" => Some(&self.lyAboutCoAuthor),
            "lyAboutRepository" => Some(&self.lyAboutRepository),
            "lyAboutLists" => Some(&self.lyAboutLists),
            _ => None,
        }
    }
    /// O Layout que disparou o evento (`None` se o `sender` não é um Layout deste form).
    pub fn sender_as_layout(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Layout> {
        self.find_layout(ui.name(sender)?)
    }
}

/// O form fraAbout e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FraAboutComponents {
    pub form: Form,
    pub imgAbout: Image,
    pub lblAboutName: Label,
    pub lblAboutTagline: Label,
    pub lblAboutIntro1: Label,
    pub lblAboutIntro2: Label,
    pub lblAboutIntro3: Label,
    pub lyAboutAuthor: Layout,
    pub lblAboutAuthor: Label,
    pub lblAboutAuthorValue: Label,
    pub lyAboutCoAuthor: Layout,
    pub lblAboutCoAuthor: Label,
    pub lblAboutCoAuthorValue: Label,
    pub lblAboutCoAuthorTag: Label,
    pub lyAboutRepository: Layout,
    pub lblAboutRepository: Label,
    pub lblAboutRepositoryValue: Label,
    pub lyAboutLists: Layout,
    pub lblAboutFooter: Label,
}

impl std::ops::Deref for FraAboutComponents {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

pub static fraAbout: FraAboutComponents = FraAboutComponents {
    form: Form { target: "fraAbout" },
    imgAbout: Image { target: "fraAbout.imgAbout" },
    lblAboutName: Label { target: "fraAbout.lblAboutName" },
    lblAboutTagline: Label { target: "fraAbout.lblAboutTagline" },
    lblAboutIntro1: Label { target: "fraAbout.lblAboutIntro1" },
    lblAboutIntro2: Label { target: "fraAbout.lblAboutIntro2" },
    lblAboutIntro3: Label { target: "fraAbout.lblAboutIntro3" },
    lyAboutAuthor: Layout { target: "fraAbout.lyAboutAuthor" },
    lblAboutAuthor: Label { target: "fraAbout.lblAboutAuthor" },
    lblAboutAuthorValue: Label { target: "fraAbout.lblAboutAuthorValue" },
    lyAboutCoAuthor: Layout { target: "fraAbout.lyAboutCoAuthor" },
    lblAboutCoAuthor: Label { target: "fraAbout.lblAboutCoAuthor" },
    lblAboutCoAuthorValue: Label { target: "fraAbout.lblAboutCoAuthorValue" },
    lblAboutCoAuthorTag: Label { target: "fraAbout.lblAboutCoAuthorTag" },
    lyAboutRepository: Layout { target: "fraAbout.lyAboutRepository" },
    lblAboutRepository: Label { target: "fraAbout.lblAboutRepository" },
    lblAboutRepositoryValue: Label { target: "fraAbout.lblAboutRepositoryValue" },
    lyAboutLists: Layout { target: "fraAbout.lyAboutLists" },
    lblAboutFooter: Label { target: "fraAbout.lblAboutFooter" },
};

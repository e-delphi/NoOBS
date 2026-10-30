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

impl FrmMainComponents {
    /// O Panel deste form com o nome (`None` se não há um Panel com ele).
    pub fn find_panel(&'static self, name: &str) -> Option<&'static Panel> {
        match name {
            "pnlSources" => Some(&self.pnlSources),
            "pnlRecordings" => Some(&self.pnlRecordings),
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
            "lnSourcesEdge" => Some(&self.lnSourcesEdge),
            _ => None,
        }
    }
    /// O Rectangle que disparou o evento (`None` se o `sender` não é um Rectangle deste form).
    pub fn sender_as_rectangle(&'static self, ui: &Ui, sender: ComponentId) -> Option<&'static Rectangle> {
        self.find_rectangle(ui.name(sender)?)
    }
}

/// O form frmMain e os componentes dele (as propriedades do form vêm do `Form`).
pub struct FrmMainComponents {
    pub form: Form,
    pub pnlSources: Panel,
    pub lnSourcesEdge: Rectangle,
    pub pnlRecordings: Panel,
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
    pnlRecordings: Panel { target: "frmMain.pnlRecordings" },
};

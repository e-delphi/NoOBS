//! Parsing e formatação de combinações de tecla para o `RegisterHotKey`
//! (porte de `OBSHotkey.pas`).
//!
//! Formato textual: tokens separados por `+`, sem diferenciar caixa.
//! Modificadores: `Ctrl`, `Shift`, `Alt`, `Win`. Tecla: `F1..F24`, `A..Z`,
//! `0..9`, `Space`, `Enter`, `Tab`, `Esc`, `Insert`, `Delete`, `Home`, `End`,
//! `PageUp`, `PageDown`, setas, `Numpad0..9`, `NumpadAdd`…, pontuação OEM e
//! as teclas de mídia. Exemplo: `Ctrl+Shift+F9`. Vazio = nenhum atalho.

use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};

pub const MOD_CTRL: u32 = MOD_CONTROL.0;
pub const MOD_SHIFT_: u32 = MOD_SHIFT.0;
pub const MOD_ALT_: u32 = MOD_ALT.0;
pub const MOD_WIN_: u32 = MOD_WIN.0;

// VK codes usados aqui (Win32). Declarados locais pra a tabela ficar legível.
const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
pub const VK_PAUSE: u32 = 0x13;
/// `Ctrl+Pause` chega como `VK_CANCEL` (a função "Break" da tecla).
pub const VK_CANCEL: u32 = 0x03;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_INSERT: u32 = 0x2D;
const VK_DELETE: u32 = 0x2E;
const VK_NUMPAD0: u32 = 0x60;
const VK_MULTIPLY: u32 = 0x6A;
const VK_ADD: u32 = 0x6B;
const VK_SUBTRACT: u32 = 0x6D;
const VK_DECIMAL: u32 = 0x6E;
const VK_DIVIDE: u32 = 0x6F;
const VK_F1: u32 = 0x70;
const VK_F24: u32 = 0x87;
const VK_MEDIA_NEXT_TRACK: u32 = 0xB0;
const VK_MEDIA_PREV_TRACK: u32 = 0xB1;
const VK_MEDIA_STOP: u32 = 0xB2;
const VK_MEDIA_PLAY_PAUSE: u32 = 0xB3;
const VK_OEM_1: u32 = 0xBA;
const VK_OEM_PLUS: u32 = 0xBB;
const VK_OEM_COMMA: u32 = 0xBC;
const VK_OEM_MINUS: u32 = 0xBD;
const VK_OEM_PERIOD: u32 = 0xBE;
const VK_OEM_2: u32 = 0xBF;
const VK_OEM_3: u32 = 0xC0;
const VK_OEM_4: u32 = 0xDB;
const VK_OEM_5: u32 = 0xDC;
const VK_OEM_6: u32 = 0xDD;
const VK_OEM_7: u32 = 0xDE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeySpec {
    /// Máscara `MOD_CONTROL | MOD_SHIFT | MOD_ALT | MOD_WIN`.
    pub modifiers: u32,
    pub vk: u32,
}

fn vk_to_name(vk: u32) -> Option<String> {
    let s = match vk {
        VK_F1..=VK_F24 => return Some(format!("F{}", vk - VK_F1 + 1)),
        0x30..=0x39 | 0x41..=0x5A => return Some(char::from_u32(vk)?.to_string()),
        VK_NUMPAD0..=0x69 => return Some(format!("Numpad{}", vk - VK_NUMPAD0)),
        VK_SPACE => "Space",
        VK_RETURN => "Enter",
        VK_TAB => "Tab",
        VK_ESCAPE => "Esc",
        VK_INSERT => "Insert",
        VK_DELETE => "Delete",
        VK_HOME => "Home",
        VK_END => "End",
        VK_PRIOR => "PageUp",
        VK_NEXT => "PageDown",
        VK_LEFT => "Left",
        VK_RIGHT => "Right",
        VK_UP => "Up",
        VK_DOWN => "Down",
        VK_BACK => "Backspace",
        // O rótulo reflete o que está impresso no teclado; o parse aceita
        // "Pause" e "Pause/Break" (config antigo).
        VK_PAUSE => "Pause/Break",
        VK_ADD => "NumpadAdd",
        VK_SUBTRACT => "NumpadSubtract",
        VK_MULTIPLY => "NumpadMultiply",
        VK_DIVIDE => "NumpadDivide",
        VK_DECIMAL => "NumpadDecimal",
        VK_OEM_MINUS => "-",
        VK_OEM_PLUS => "=",
        VK_OEM_COMMA => ",",
        VK_OEM_PERIOD => ".",
        VK_OEM_1 => ";",
        VK_OEM_2 => "/",
        VK_OEM_3 => "`",
        VK_OEM_4 => "[",
        VK_OEM_5 => "\\",
        VK_OEM_6 => "]",
        VK_OEM_7 => "'",
        VK_MEDIA_PLAY_PAUSE => "MediaPlayPause",
        VK_MEDIA_STOP => "MediaStop",
        VK_MEDIA_PREV_TRACK => "MediaPrev",
        VK_MEDIA_NEXT_TRACK => "MediaNext",
        _ => return None,
    };
    Some(s.to_owned())
}

fn name_to_vk(name: &str) -> u32 {
    let s = name.trim();
    let mut chars = s.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        // Pegadinha #43: para letra/dígito o código do caractere É o VK, mas
        // para pontuação NÃO (`'-'` = 45 = VK_INSERT). Espelho do vk_to_name.
        return match c {
            'A'..='Z' | '0'..='9' => c as u32,
            'a'..='z' => c.to_ascii_uppercase() as u32,
            '-' => VK_OEM_MINUS,
            '=' => VK_OEM_PLUS,
            ',' => VK_OEM_COMMA,
            '.' => VK_OEM_PERIOD,
            ';' => VK_OEM_1,
            '/' => VK_OEM_2,
            '`' => VK_OEM_3,
            '[' => VK_OEM_4,
            '\\' => VK_OEM_5,
            ']' => VK_OEM_6,
            '\'' => VK_OEM_7,
            _ => 0,
        };
    }
    let up = s.to_ascii_uppercase();
    if let Some(n) = up.strip_prefix('F').and_then(|r| r.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return VK_F1 + n - 1;
        }
    }
    if let Some(n) = up.strip_prefix("NUMPAD").filter(|r| r.len() == 1).and_then(|r| r.parse::<u32>().ok()) {
        return VK_NUMPAD0 + n;
    }
    match up.as_str() {
        "SPACE" => VK_SPACE,
        "ENTER" => VK_RETURN,
        "TAB" => VK_TAB,
        "ESC" | "ESCAPE" => VK_ESCAPE,
        "INSERT" => VK_INSERT,
        "DELETE" => VK_DELETE,
        "HOME" => VK_HOME,
        "END" => VK_END,
        "PAGEUP" => VK_PRIOR,
        "PAGEDOWN" => VK_NEXT,
        "LEFT" => VK_LEFT,
        "RIGHT" => VK_RIGHT,
        "UP" => VK_UP,
        "DOWN" => VK_DOWN,
        "BACKSPACE" => VK_BACK,
        "PAUSE" | "BREAK" | "PAUSE/BREAK" => VK_PAUSE,
        "NUMPADADD" => VK_ADD,
        "NUMPADSUBTRACT" => VK_SUBTRACT,
        "NUMPADMULTIPLY" => VK_MULTIPLY,
        "NUMPADDIVIDE" => VK_DIVIDE,
        "NUMPADDECIMAL" => VK_DECIMAL,
        "MEDIAPLAYPAUSE" => VK_MEDIA_PLAY_PAUSE,
        "MEDIASTOP" => VK_MEDIA_STOP,
        "MEDIAPREV" => VK_MEDIA_PREV_TRACK,
        "MEDIANEXT" => VK_MEDIA_NEXT_TRACK,
        _ => 0,
    }
}

/// `"Ctrl+Shift+F9"` → spec. `None` se vazio, sem tecla ou tecla desconhecida.
pub fn parse_hotkey(spec: &str) -> Option<HotkeySpec> {
    let mut modifiers = 0;
    let mut key = "";
    for tok in spec.split('+') {
        let t = tok.trim();
        match t.to_ascii_uppercase().as_str() {
            "" => {}
            "CTRL" | "CONTROL" => modifiers |= MOD_CTRL,
            "SHIFT" => modifiers |= MOD_SHIFT_,
            "ALT" => modifiers |= MOD_ALT_,
            "WIN" => modifiers |= MOD_WIN_,
            _ => key = t,
        }
    }
    // `Ctrl++`: o split dá dois tokens vazios no fim e a tecla seria `+`, que o
    // Delphi também não aceita (não há VK pra ela no mapa).
    if key.is_empty() {
        return None;
    }
    let vk = name_to_vk(key);
    (vk != 0).then_some(HotkeySpec { modifiers, vk })
}

/// Inverso do [`parse_hotkey`], na ordem canônica Ctrl, Shift, Alt, Win.
pub fn format_hotkey(modifiers: u32, vk: u32) -> String {
    let Some(key) = vk_to_name(vk) else { return String::new() };
    let mut parts: Vec<String> = Vec::new();
    for (bit, name) in [(MOD_CTRL, "Ctrl"), (MOD_SHIFT_, "Shift"), (MOD_ALT_, "Alt"), (MOD_WIN_, "Win")] {
        if modifiers & bit != 0 {
            parts.push(name.to_owned());
        }
    }
    parts.push(key);
    parts.join("+")
}

/// Combinação reservada pelo Windows (o `RegisterHotKey` nunca dispararia)?
/// Devolve a CHAVE i18n do motivo. Match exato de modificadores. Espelho do
/// `RESERVED_HOTKEYS` da UI, que bloqueia antes; isto pega config editado a mão.
pub fn reserved_reason(modifiers: u32, vk: u32) -> Option<&'static str> {
    let (c, s, a, w) = (MOD_CTRL, MOD_SHIFT_, MOD_ALT_, MOD_WIN_);
    let key = match (modifiers, vk) {
        (m, VK_DELETE) if m == c | a => "ctrlAltDel",
        (m, VK_TAB) if m == a => "altTab",
        (m, 0x73) if m == a => "altF4",
        (m, VK_ESCAPE) if m == a => "altEsc",
        (m, VK_SPACE) if m == a => "altSpace",
        (m, VK_ESCAPE) if m == c => "ctrlEsc",
        (m, VK_TAB) if m == w => "winTab",
        (m, VK_UP | VK_DOWN | VK_LEFT | VK_RIGHT) if m == w => "winArrows",
        (m, VK_OEM_PERIOD) if m == w => "winPeriod",
        (m, VK_OEM_1) if m == w => "winSemicolon",
        (m, 0x53) if m == w | s => "winShiftS",
        (m, k) if m == w => match char::from_u32(k).unwrap_or('\0') {
            'L' => "winL",
            'D' => "winD",
            'E' => "winE",
            'R' => "winR",
            'I' => "winI",
            'X' => "winX",
            'A' => "winA",
            'S' => "winS",
            'Q' => "winQ",
            'M' => "winM",
            'P' => "winP",
            'V' => "winV",
            'G' => "winG",
            'K' => "winK",
            'H' => "winH",
            'U' => "winU",
            'B' => "winB",
            'T' => "winT",
            'W' => "winW",
            _ => return None,
        },
        _ => return None,
    };
    Some(match key {
        "ctrlAltDel" => "settings.hotkey.reserved.ctrlAltDel",
        "altTab" => "settings.hotkey.reserved.altTab",
        "altF4" => "settings.hotkey.reserved.altF4",
        "altEsc" => "settings.hotkey.reserved.altEsc",
        "altSpace" => "settings.hotkey.reserved.altSpace",
        "ctrlEsc" => "settings.hotkey.reserved.ctrlEsc",
        "winTab" => "settings.hotkey.reserved.winTab",
        "winArrows" => "settings.hotkey.reserved.winArrows",
        "winPeriod" => "settings.hotkey.reserved.winPeriod",
        "winSemicolon" => "settings.hotkey.reserved.winSemicolon",
        "winShiftS" => "settings.hotkey.reserved.winShiftS",
        "winL" => "settings.hotkey.reserved.winL",
        "winD" => "settings.hotkey.reserved.winD",
        "winE" => "settings.hotkey.reserved.winE",
        "winR" => "settings.hotkey.reserved.winR",
        "winI" => "settings.hotkey.reserved.winI",
        "winX" => "settings.hotkey.reserved.winX",
        "winA" => "settings.hotkey.reserved.winA",
        "winS" => "settings.hotkey.reserved.winS",
        "winQ" => "settings.hotkey.reserved.winQ",
        "winM" => "settings.hotkey.reserved.winM",
        "winP" => "settings.hotkey.reserved.winP",
        "winV" => "settings.hotkey.reserved.winV",
        "winG" => "settings.hotkey.reserved.winG",
        "winK" => "settings.hotkey.reserved.winK",
        "winH" => "settings.hotkey.reserved.winH",
        "winU" => "settings.hotkey.reserved.winU",
        "winB" => "settings.hotkey.reserved.winB",
        "winT" => "settings.hotkey.reserved.winT",
        _ => "settings.hotkey.reserved.winW",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta_canonica() {
        let hk = parse_hotkey("shift+ctrl+f9").unwrap();
        assert_eq!(hk, HotkeySpec { modifiers: MOD_CTRL | MOD_SHIFT_, vk: VK_F1 + 8 });
        assert_eq!(format_hotkey(hk.modifiers, hk.vk), "Ctrl+Shift+F9");
    }

    #[test]
    fn pontuacao_oem_nao_vira_outra_tecla() {
        // Pegadinha #43: '-' não pode virar Insert, ';' não pode sumir.
        for s in ["Ctrl+-", "Ctrl+.", "Ctrl+;", "Ctrl+/", "Ctrl+[", "Ctrl+\\", "Ctrl+'", "Ctrl+`", "Ctrl+=", "Ctrl+,", "Ctrl+]"] {
            let hk = parse_hotkey(s).unwrap();
            assert_eq!(format_hotkey(hk.modifiers, hk.vk), s, "{s}");
        }
    }

    #[test]
    fn pause_aceita_os_dois_nomes() {
        assert_eq!(parse_hotkey("Pause").unwrap().vk, VK_PAUSE);
        assert_eq!(format_hotkey(0, parse_hotkey("pause/break").unwrap().vk), "Pause/Break");
    }

    #[test]
    fn invalidos() {
        assert!(parse_hotkey("").is_none());
        assert!(parse_hotkey("Ctrl+Shift").is_none());
        assert!(parse_hotkey("Ctrl+Banana").is_none());
        assert!(parse_hotkey("F25").is_none());
    }

    #[test]
    fn reservados_so_com_os_modificadores_exatos() {
        let wl = parse_hotkey("Win+L").unwrap();
        assert_eq!(reserved_reason(wl.modifiers, wl.vk), Some("settings.hotkey.reserved.winL"));
        let cwl = parse_hotkey("Ctrl+Win+L").unwrap();
        assert_eq!(reserved_reason(cwl.modifiers, cwl.vk), None);
        let a4 = parse_hotkey("Alt+F4").unwrap();
        assert_eq!(reserved_reason(a4.modifiers, a4.vk), Some("settings.hotkey.reserved.altF4"));
        let wss = parse_hotkey("Win+Shift+S").unwrap();
        assert_eq!(reserved_reason(wss.modifiers, wss.vk), Some("settings.hotkey.reserved.winShiftS"));
    }
}

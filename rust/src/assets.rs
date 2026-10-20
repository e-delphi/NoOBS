//! UI e traduções EMBUTIDAS no exe (geradas pelo `build.rs` a partir de
//! `rust\ui\` e `rust\lang\`): o runtime é só o NoOBS.exe e as DLLs do FFmpeg.
//! Nada é extraído pro disco: as traduções são lidas da memória, e a UI é
//! servida da memória pro WebView2 (`ui::start_navigate`, `WebResourceRequested`).

use std::path::Path;

use crate::log;

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Conteúdo de `lang\<code>.json`.
pub fn lang_file(code: &str) -> Option<&'static [u8]> {
    let name = format!("{code}.json");
    LANG_FILES.iter().find(|(n, _)| n.eq_ignore_ascii_case(&name)).map(|(_, b)| *b)
}

/// Códigos de idioma embutidos (nome do arquivo sem `.json`), em ordem.
pub fn lang_codes() -> Vec<String> {
    let mut v: Vec<String> = LANG_FILES.iter().filter_map(|(n, _)| n.strip_suffix(".json").map(str::to_owned)).collect();
    v.sort();
    v
}

/// Um arquivo da UI pelo caminho relativo (`index.html`, `js/main.js`).
pub fn ui_file(path: &str) -> Option<&'static [u8]> {
    UI_FILES.iter().find(|(n, _)| *n == path).map(|(_, b)| *b)
}

pub fn ui_file_count() -> usize {
    UI_FILES.len()
}

/// Versões anteriores extraíam a UI em `%LOCALAPPDATA%\NoOBS\ui\`; a pasta
/// não é mais usada. Apagada no arranque (o instalador também a apaga).
pub fn remove_old_ui_folder() {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_default();
    if base.is_empty() {
        return;
    }
    let dir = Path::new(&base).join("NoOBS").join("ui");
    if dir.is_dir() {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => log!("UI: pasta antiga da interface extraida apagada ({}).", dir.display()),
            Err(e) => log!("UI: nao consegui apagar a pasta antiga \"{}\": {e}", dir.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_e_idiomas_embutidos() {
        assert!(UI_FILES.iter().any(|(n, _)| *n == "index.html"));
        assert!(UI_FILES.iter().any(|(n, _)| n.starts_with("js/")));
        assert_eq!(lang_codes(), ["en", "es", "pt-BR"]);
        assert!(lang_file("pt-br").is_some());
        assert!(ui_file("index.html").is_some_and(|b| b.starts_with(b"<!")));
        assert!(ui_file("nada.js").is_none());
    }
}

//! Traduções EMBUTIDAS no exe (geradas pelo `build.rs` a partir de
//! `rust\lang\`): o runtime é só o NoOBS.exe e as DLLs do FFmpeg. Nada é
//! extraído pro disco. A interface (Slint) é compilada no exe pelo próprio
//! `build.rs`.

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

/// Resto de versões anteriores no perfil: a interface extraída em
/// `%LOCALAPPDATA%\NoOBS\ui\`. Apagado no arranque.
///
/// Os dados do WebView2 (`%LOCALAPPDATA%\TNoOBS\`) NÃO saem daqui: enquanto a
/// versão web estiver instalada na mesma máquina ela depende deles, e apagar
/// a pasta deixou o WebView2 dela sem subir (docs/ui-slint.md). Quem remove é
/// o instalador da versão Slint (fase 6).
pub fn remove_old_ui_folders() {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_default();
    if base.is_empty() {
        return;
    }
    for dir in [Path::new(&base).join("NoOBS").join("ui")] {
        if dir.is_dir() {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => log!("UI: pasta antiga apagada ({}).", dir.display()),
                Err(e) => log!("UI: nao consegui apagar a pasta antiga \"{}\": {e}", dir.display()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idiomas_embutidos() {
        assert_eq!(lang_codes(), ["en", "es", "pt-BR"]);
        assert!(lang_file("pt-br").is_some());
    }
}

//! UI e traduções EMBUTIDAS no exe (geradas pelo `build.rs` a partir de
//! `rust\ui\` e `rust\lang\`): o runtime é só o NoOBS.exe e as DLLs do FFmpeg.
//!
//! As traduções são lidas direto da memória. A UI não: o WebView2 só serve um
//! origin `https://` real a partir de uma PASTA (`SetVirtualHostNameToFolderMapping`,
//! pegadinha #16), então ela é extraída uma vez em
//! `%LOCALAPPDATA%\NoOBS\ui\<hash do conteúdo>\` — UI nova = pasta nova, nunca
//! um arquivo velho servido no meio de uma versão nova. As pastas de versões
//! anteriores são apagadas na extração.

use std::path::{Path, PathBuf};

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

fn ui_root() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").or_else(|_| std::env::var("APPDATA")).unwrap_or_else(|_| ".".into());
    Path::new(&base).join("NoOBS").join("ui")
}

/// Escreve a UI inteira numa pasta temporária e a renomeia pro nome final:
/// duas instâncias (full + hibernação promovendo) nunca veem uma pasta pela
/// metade.
fn extract_to(dir: &Path) -> std::io::Result<()> {
    let tmp = dir.with_extension(format!("tmp{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    for (rel, bytes) in UI_FILES {
        let p = tmp.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, bytes)?;
    }
    match std::fs::rename(&tmp, dir) {
        Ok(()) => Ok(()),
        // Outra instância extraiu antes: vale a dela.
        Err(_) if dir.join("index.html").is_file() => {
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Pasta da UI desta versão, extraída se ainda não existe. `None` se não
/// deu pra escrever (disco cheio, permissão) — a janela abre em branco e o
/// log diz por quê.
pub fn ui_dir() -> Option<PathBuf> {
    let root = ui_root();
    let dir = root.join(ASSETS_HASH);
    if !dir.join("index.html").is_file() {
        if let Err(e) = std::fs::create_dir_all(&root).and_then(|()| extract_to(&dir)) {
            log!("UI: nao consegui extrair a interface em \"{}\": {e}", dir.display());
            return None;
        }
        log!("UI: interface extraida em \"{}\" ({} arquivos).", dir.display(), UI_FILES.len());
        // Versões anteriores: ninguém mais serve delas. (A extração em curso de
        // outra instância começa com o mesmo hash e fica.)
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                if !e.file_name().to_string_lossy().starts_with(ASSETS_HASH) {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
    }
    Some(dir)
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
    }
}

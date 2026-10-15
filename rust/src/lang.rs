//! i18n (porte de `OBSLang.pas`): `lang\<code>.json` aninhado, chaves com
//! ponto, interpolação `{{var}}`, fallback por `meta.fallback` e, no fim, `en`.
//! Chave inexistente devolve `[chave]` (sinalizador visual para o tradutor).

use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::{config, log};

/// Idioma-base: fallback final da detecção e bundle de reserva. Nenhum
/// `lang\*.json` é opcional, nem o `pt-BR` (pegadinha do CLAUDE.md).
const DEFAULT_LANGUAGE: &str = "en";

#[derive(Default)]
struct State {
    code: String,
    bundle: Value,
    fallback: Value,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default()))
}

/// `<ExeDir>\lang\` — única, em dev e em produção.
pub fn lang_folder() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("lang")))
        .unwrap_or_else(|| PathBuf::from("lang"))
}

fn read_bundle(code: &str) -> Option<Value> {
    let path = lang_folder().join(format!("{code}.json"));
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
        .ok()
        .filter(Value::is_object)
}

/// Carrega um idioma. `false` (sem trocar o ativo) se faltar ou for inválido.
pub fn load_language(code: &str) -> bool {
    let Some(bundle) = read_bundle(code) else {
        log!("OBSLang: idioma \"{code}\" não carregou");
        return false;
    };
    let fb_code = bundle
        .pointer("/meta/fallback")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && *s != code)
        .unwrap_or(DEFAULT_LANGUAGE)
        .to_owned();
    let fallback = if fb_code == code {
        bundle.clone()
    } else {
        read_bundle(&fb_code).unwrap_or(Value::Null)
    };
    let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
    *s = State { code: code.to_owned(), bundle, fallback };
    true
}

/// Códigos disponíveis (nome do arquivo sem `.json`).
fn available_codes() -> Vec<String> {
    let Ok(rd) = fs::read_dir(lang_folder()) else { return Vec::new() };
    let mut v: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".json").map(str::to_owned))
        .collect();
    v.sort();
    v
}

/// Match exato (`pt-BR`), depois por prefixo de 2 letras (`pt` → `pt-BR`),
/// por fim `en`.
fn match_locale(locale: &str, available: &[String]) -> String {
    if let Some(c) = available.iter().find(|c| c.eq_ignore_ascii_case(locale)) {
        return c.clone();
    }
    let prefix = locale.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    if let Some(c) = available
        .iter()
        .find(|c| c.split('-').next().unwrap_or("").eq_ignore_ascii_case(&prefix))
    {
        return c.clone();
    }
    DEFAULT_LANGUAGE.to_owned()
}

/// Locale do Windows (`GetUserDefaultLocaleName`), ex.: `pt-BR`.
fn system_locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..(n as usize - 1)])
    } else {
        DEFAULT_LANGUAGE.to_owned()
    }
}

/// `language` do config (vazio = automático → locale do Windows).
pub fn init_language() {
    let available = available_codes();
    let configured = config::get_config_str("language", "");
    let code = if !configured.is_empty() && available.contains(&configured) {
        configured
    } else {
        match_locale(&system_locale(), &available)
    };
    if !load_language(&code) && code != DEFAULT_LANGUAGE {
        load_language(DEFAULT_LANGUAGE);
    }
}

pub fn current_language() -> String {
    state().lock().unwrap_or_else(|p| p.into_inner()).code.clone()
}

fn lookup<'a>(bundle: &'a Value, key: &str) -> Option<&'a str> {
    key.split('.')
        .try_fold(bundle, |v, part| v.get(part))
        .and_then(Value::as_str)
}

/// `t("record.finished", &[("min", "5")])`.
pub fn t_with(key: &str, args: &[(&str, &str)]) -> String {
    let s = state().lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut out) = lookup(&s.bundle, key)
        .or_else(|| lookup(&s.fallback, key))
        .map(str::to_owned)
    else {
        return format!("[{key}]");
    };
    for (name, value) in args {
        out = out.replace(&format!("{{{{{name}}}}}"), value);
    }
    out
}

pub fn t(key: &str) -> String {
    t_with(key, &[])
}

/// Bundle inteiro do idioma atual, enviado à UI no `init`/`language_changed`.
pub fn current_bundle() -> Value {
    state().lock().unwrap_or_else(|p| p.into_inner()).bundle.clone()
}

/// `[{code, name, nativeName}]` para o dropdown de idiomas.
pub fn available_languages() -> Value {
    let list: Vec<Value> = available_codes()
        .into_iter()
        .map(|code| {
            let meta = read_bundle(&code)
                .and_then(|b| b.get("meta").cloned())
                .unwrap_or(Value::Null);
            let name = meta.get("name").and_then(Value::as_str).unwrap_or(&code).to_owned();
            let native = meta.get("nativeName").and_then(Value::as_str).unwrap_or(&name).to_owned();
            json!({ "code": code, "name": name, "nativeName": native })
        })
        .collect();
    Value::Array(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_por_prefixo() {
        let av = vec!["en".to_owned(), "es".to_owned(), "pt-BR".to_owned()];
        assert_eq!(match_locale("pt-PT", &av), "pt-BR");
        assert_eq!(match_locale("pt-BR", &av), "pt-BR");
        assert_eq!(match_locale("ja-JP", &av), "en");
    }

    #[test]
    fn lookup_aninhado() {
        let b = json!({ "settings": { "title": "Config" } });
        assert_eq!(lookup(&b, "settings.title"), Some("Config"));
        assert_eq!(lookup(&b, "settings.nope"), None);
    }
}

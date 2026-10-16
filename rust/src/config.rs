//! Preferências em JSON (porte de `OBSConfig.pas`).
//!
//! `%LOCALAPPDATA%\NoOBS\config.json`, com discriminador `version`. Arquivo
//! com versão diferente (ou corrompido) é descartado e reescrito do zero.
//! Escrita atômica: `.tmp` + rename por cima (um config truncado por queda no
//! meio da escrita, ou duas instâncias gravando juntas, não pode acontecer —
//! pegadinha #36).

use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::log::log;

/// Incrementar quando o schema mudar de forma incompatível.
const CURRENT_VERSION: i64 = 1;

fn store() -> &'static Mutex<Map<String, Value>> {
    static STORE: OnceLock<Mutex<Map<String, Value>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(load_from_disk()))
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_default();
    base.join("NoOBS")
}

pub fn config_file_path() -> PathBuf {
    config_dir().join("config.json")
}

fn load_from_disk() -> Map<String, Value> {
    let parsed = fs::read_to_string(config_file_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(s.trim_start_matches('\u{feff}')).ok());
    if let Some(Value::Object(obj)) = parsed {
        if obj.get("version").and_then(Value::as_i64) == Some(CURRENT_VERSION) {
            return obj;
        }
    }
    // Schema incompatível ou arquivo ilegível: recomeça e já sobrescreve,
    // senão todo arranque reencontraria o mesmo arquivo inválido.
    let mut fresh = Map::new();
    fresh.insert("version".into(), json!(CURRENT_VERSION));
    write_to_disk(&fresh);
    fresh
}

fn write_to_disk(map: &Map<String, Value>) {
    let dir = config_dir();
    let final_path = config_file_path();
    let tmp_path = final_path.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        fs::create_dir_all(&dir)?;
        let text = serde_json::to_string_pretty(map).map_err(std::io::Error::other)?;
        fs::write(&tmp_path, text)?;
        // No Windows `rename` substitui o destino existente.
        fs::rename(&tmp_path, &final_path)
    })();
    if let Err(e) = result {
        log(format!("OBSConfig: falha ao gravar \"{}\": {e}", final_path.display()));
        let _ = fs::remove_file(&tmp_path);
    }
}

fn with_store<R>(f: impl FnOnce(&mut Map<String, Value>) -> R) -> R {
    let mut guard = store().lock().unwrap_or_else(|p| p.into_inner());
    f(&mut guard)
}

fn set_value(key: &str, value: Value) {
    with_store(|m| {
        m.insert(key.to_owned(), value);
        write_to_disk(m);
    });
}

/// Lê como `i32` com clamp: `as` truncaria/daria a volta em valores fora de
/// Int32 (ex.: `recordingFps: 9999999999`).
fn clamp_i32(v: &Value, default: i32) -> i32 {
    match v.as_i64() {
        Some(n) => n.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        None => default,
    }
}

pub fn get_config_str(key: &str, default: &str) -> String {
    with_store(|m| match m.get(key) {
        Some(Value::String(s)) => s.clone(),
        _ => default.to_owned(),
    })
}

pub fn set_config_str(key: &str, value: &str) {
    set_value(key, Value::String(value.to_owned()));
}

pub fn get_config_bool(key: &str, default: bool) -> bool {
    with_store(|m| m.get(key).and_then(Value::as_bool).unwrap_or(default))
}

pub fn set_config_bool(key: &str, value: bool) {
    set_value(key, Value::Bool(value));
}

pub fn get_config_int(key: &str, default: i32) -> i32 {
    with_store(|m| m.get(key).map_or(default, |v| clamp_i32(v, default)))
}

pub fn set_config_int(key: &str, value: i32) {
    set_value(key, json!(value));
}

/// Valor estruturado (objeto/array). `None` se a chave não existe ou é escalar.
pub fn get_config_json(key: &str) -> Option<Value> {
    with_store(|m| match m.get(key) {
        Some(v @ (Value::Object(_) | Value::Array(_))) => Some(v.clone()),
        _ => None,
    })
}

pub fn set_config_json(key: &str, value: Option<Value>) {
    with_store(|m| {
        m.remove(key);
        if let Some(v) = value {
            m.insert(key.to_owned(), v);
        }
        write_to_disk(m);
    });
}

/// Toggle de source. `category` = `monitors`/`mics`/`speakers`/`webcams`;
/// `id` = índice (monitor) ou nome do dispositivo.
pub fn get_source_bool(category: &str, id: &str, default: bool) -> bool {
    with_store(|m| {
        m.get(category)
            .and_then(Value::as_object)
            .and_then(|cat| cat.get(id))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    })
}

pub fn set_source_bool(category: &str, id: &str, value: bool) {
    with_store(|m| {
        let entry = m
            .entry(category.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new()); // valor inválido (string solta etc.)
        }
        if let Some(cat) = entry.as_object_mut() {
            cat.insert(id.to_owned(), Value::Bool(value));
        }
        write_to_disk(m);
    });
}

// "Oculto" fica num namespace paralelo (`hidden_<cat>`) de propósito: ocultar
// não mexe no "enabled", então desocultar devolve a preferência de antes.
pub fn get_source_hidden(category: &str, id: &str) -> bool {
    get_source_bool(&format!("hidden_{category}"), id, false)
}

pub fn set_source_hidden(category: &str, id: &str, value: bool) {
    set_source_bool(&format!("hidden_{category}"), id, value);
}

/// Marcado E não oculto. Os caminhos de gravação devem usar ESTA função, nunca
/// `get_source_bool` direto, senão um dispositivo oculto seguiria gravando.
pub fn get_source_active(category: &str, id: &str, default: bool) -> bool {
    get_source_bool(category, id, default) && !get_source_hidden(category, id)
}

// Perfil da gravação AUTOMÁTICA (watcher de mic, pegadinha #47):
// namespace `auto_<cat>`, default true.
pub fn get_source_auto_record(category: &str, id: &str) -> bool {
    get_source_bool(&format!("auto_{category}"), id, true)
}

pub fn set_source_auto_record(category: &str, id: &str, value: bool) {
    set_source_bool(&format!("auto_{category}"), id, value);
}

/// Não oculto E marcado no perfil auto. Não exige o `enabled` manual: o perfil
/// auto é uma seleção própria.
pub fn get_source_active_for_auto(category: &str, id: &str, _default: bool) -> bool {
    !get_source_hidden(category, id) && get_source_auto_record(category, id)
}

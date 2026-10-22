//! Lista de atalhos (aba "Atalhos" das Configurações) — o modelo, sem Win32.
//!
//! Cada item é uma combinação e o que ela faz:
//!
//! ```json
//! "hotkeys": [
//!   { "mode": "record", "hotkey": "Pause/Break" },
//!   { "mode": "replay", "hotkey": "Ctrl+Shift+Pause/Break" },
//!   { "mode": "window", "hotkey": "Ctrl+Shift+F7", "title": "nova guia", "appAudio": true, "delaySec": 0 },
//!   { "mode": "area",   "hotkey": "Ctrl+Pause/Break", "area": { "x": -1683, "y": 1114, "w": 642, "h": 465 }, "delaySec": 3 },
//!   { "mode": "area",   "hotkey": "Ctrl+Alt+Pause/Break", "area": null }
//! ]
//! ```
//!
//! * `record` grava as telas; `window` grava a janela cujo título CONTÉM
//!   `title` (sem diferenciar maiúsculas); `area` grava o retângulo salvo, ou
//!   abre a seleção quando não há um; `replay` salva o trecho do buffer.
//! * `delaySec` (0..[`DELAY_MAX`]) vale pros três modos de gravação.
//! * Sem tempo máximo: a gravação para pelo mesmo atalho (ou qualquer outro de
//!   gravação), como sempre.
//!
//! Antes desta lista eram chaves soltas (`hotkey`, `replayHotkey`,
//! `areaHotkey`, `areaSaved`, `areaDelaySec`); [`migrate_legacy`] as converte
//! UMA vez e elas deixam de ser lidas. Quem registra e executa é o
//! `bridge::hotkeys`, com ou sem a interface carregada.

use serde_json::{json, Map, Value};

use crate::engine::AreaRect;
use crate::{config, hotkey};

/// Teto do atraso, em segundos.
pub const DELAY_MAX: u32 = 60;
/// Menor lado de uma área salva (o mesmo da seleção).
pub const AREA_MIN: i32 = 32;

pub const DEFAULT_RECORD: &str = "Pause/Break";
pub const DEFAULT_REPLAY: &str = "Ctrl+Shift+Pause/Break";
/// Padrões das chaves ANTIGAS (antes da lista): o salvar trecho era
/// Ctrl+Shift+F10, e o Ctrl+Shift+Pause/Break era da área salva.
const LEGACY_REPLAY: &str = "Ctrl+Shift+F10";
const DEFAULT_LEGACY_AREA: &str = "Ctrl+Shift+Pause/Break";

/// Chaves antigas, apagadas depois da migração.
const LEGACY_KEYS: [&str; 5] = ["hotkey", "replayHotkey", "areaHotkey", "areaSaved", "areaDelaySec"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Record,
    Replay,
    Window,
    Area,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "record" => Some(Mode::Record),
            "replay" => Some(Mode::Replay),
            "window" => Some(Mode::Window),
            "area" => Some(Mode::Area),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Record => "record",
            Mode::Replay => "replay",
            Mode::Window => "window",
            Mode::Area => "area",
        }
    }

    /// Começa (ou para) uma gravação — o `replay` não.
    pub fn records(self) -> bool {
        self != Mode::Replay
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub mode: Mode,
    /// Forma canônica (`hotkey::format_hotkey`); vazio = sem combinação.
    pub hotkey: String,
    /// `window`: trecho do título procurado.
    pub title: String,
    /// `window`: som só do app numa faixa própria.
    pub app_audio: bool,
    /// `area`: retângulo salvo; `None` = abrir a seleção na hora.
    pub area: Option<AreaRect>,
    pub delay_sec: u32,
}

impl Item {
    pub fn new(mode: Mode, hotkey: &str) -> Item {
        Item { mode, hotkey: normalize(hotkey), title: String::new(), app_audio: false, area: None, delay_sec: 0 }
    }
}

/// Forma canônica de uma combinação, ou vazio se não dá pra ler.
pub fn normalize(spec: &str) -> String {
    hotkey::parse_hotkey(spec.trim()).map(|h| hotkey::format_hotkey(h.modifiers, h.vk)).unwrap_or_default()
}

fn area_from(v: Option<&Value>) -> Option<AreaRect> {
    let v = v?;
    let get = |k: &str| v.get(k).and_then(Value::as_i64).map(|n| n.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    let a = AreaRect { x: get("x")?, y: get("y")?, w: get("w")?, h: get("h")? };
    (a.w >= AREA_MIN && a.h >= AREA_MIN).then_some(a)
}

pub fn area_json(a: Option<AreaRect>) -> Value {
    a.map_or(Value::Null, |a| json!({ "x": a.x, "y": a.y, "w": a.w, "h": a.h }))
}

/// Um item do JSON. Modo desconhecido = descartado (config de versão futura
/// ou editado a mão); o resto tem default.
pub fn item_from_json(v: &Value) -> Option<Item> {
    let mode = Mode::parse(v.get("mode")?.as_str()?)?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    let delay = v.get("delaySec").and_then(Value::as_i64).unwrap_or(0).clamp(0, DELAY_MAX as i64) as u32;
    Some(Item {
        mode,
        hotkey: normalize(&s("hotkey")),
        title: if mode == Mode::Window { s("title").trim().to_owned() } else { String::new() },
        app_audio: mode == Mode::Window && v.get("appAudio").and_then(Value::as_bool).unwrap_or(false),
        area: if mode == Mode::Area { area_from(v.get("area")) } else { None },
        delay_sec: if mode.records() { delay } else { 0 },
    })
}

/// Só os campos que valem pro modo (o arquivo fica legível).
pub fn item_to_json(it: &Item) -> Value {
    let mut m = Map::new();
    m.insert("mode".into(), json!(it.mode.as_str()));
    m.insert("hotkey".into(), json!(it.hotkey));
    match it.mode {
        Mode::Window => {
            m.insert("title".into(), json!(it.title));
            m.insert("appAudio".into(), json!(it.app_audio));
        }
        Mode::Area => {
            m.insert("area".into(), area_json(it.area));
        }
        _ => {}
    }
    if it.mode.records() {
        m.insert("delaySec".into(), json!(it.delay_sec));
    }
    Value::Object(m)
}

pub fn list_from_json(v: &Value) -> Vec<Item> {
    v.as_array().map(|a| a.iter().filter_map(item_from_json).collect()).unwrap_or_default()
}

pub fn list_to_json(list: &[Item]) -> Value {
    Value::Array(list.iter().map(item_to_json).collect())
}

/// O que vem numa instalação nova (e no "Restaurar padrões").
pub fn defaults() -> Vec<Item> {
    vec![Item::new(Mode::Record, DEFAULT_RECORD), Item::new(Mode::Replay, DEFAULT_REPLAY)]
}

/// Converte as chaves antigas. `get` lê do config (chave ausente = `None`).
/// Chave de atalho AUSENTE vale o padrão; presente e vazia = o usuário
/// tinha desligado, e o item não nasce. O atalho da área só existia com uma
/// área salva. O salvar trecho nunca configurado ganha o padrão novo, salvo
/// se ele já for o da área: aí fica o Ctrl+Shift+F10 que essa pessoa usava,
/// em vez de nascer repetido.
pub fn migrate_legacy(get: impl Fn(&str) -> Option<Value>) -> Vec<Item> {
    let spec = |key: &str, default: &str| match get(key) {
        Some(Value::String(s)) => s,
        _ => default.to_owned(),
    };
    let mut out = Vec::new();
    let rec = spec("hotkey", DEFAULT_RECORD);
    if !normalize(&rec).is_empty() {
        out.push(Item::new(Mode::Record, &rec));
    }
    let rep = spec("replayHotkey", DEFAULT_REPLAY);
    if !normalize(&rep).is_empty() {
        out.push(Item::new(Mode::Replay, &rep));
    }
    if let Some(area) = area_from(get("areaSaved").as_ref()) {
        let mut it = Item::new(Mode::Area, &spec("areaHotkey", DEFAULT_LEGACY_AREA));
        it.area = Some(area);
        it.delay_sec = get("areaDelaySec").and_then(|v| v.as_i64()).unwrap_or(0).clamp(0, DELAY_MAX as i64) as u32;
        if get("replayHotkey").is_none() {
            if let Some(r) = out.iter_mut().find(|i| i.mode == Mode::Replay && i.hotkey.eq_ignore_ascii_case(&it.hotkey)) {
                r.hotkey = normalize(LEGACY_REPLAY);
            }
        }
        out.push(it);
    }
    out
}

/// Índices cuja combinação já apareceu ANTES na lista (o Windows aceita um
/// registro por combinação: o segundo nunca dispararia). Vazio não conta.
pub fn duplicates(list: &[Item]) -> Vec<usize> {
    let mut seen: Vec<&str> = Vec::new();
    let mut dup = Vec::new();
    for (i, it) in list.iter().enumerate() {
        if it.hotkey.is_empty() {
            continue;
        }
        if seen.iter().any(|s| s.eq_ignore_ascii_case(&it.hotkey)) {
            dup.push(i);
        } else {
            seen.push(&it.hotkey);
        }
    }
    dup
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

fn raw_config(key: &str) -> Option<Value> {
    config::get_config_json(key).or_else(|| {
        // `get_config_json` só devolve objeto/array; os escalares vêm pelos
        // getters tipados. Sentinela pra separar "ausente" de "vazio".
        const MISSING: &str = "\u{0}";
        let s = config::get_config_str(key, MISSING);
        if s != MISSING {
            return Some(Value::String(s));
        }
        let n = config::get_config_int(key, i32::MIN);
        (n != i32::MIN).then(|| json!(n))
    })
}

/// A lista do config. Sem a chave `hotkeys`, migra as chaves antigas, grava e
/// apaga as antigas.
pub fn load() -> Vec<Item> {
    if let Some(v) = config::get_config_json("hotkeys") {
        return list_from_json(&v);
    }
    let list = migrate_legacy(raw_config);
    crate::log!("Atalhos: migrando as chaves antigas ({} item(ns)).", list.len());
    save(&list);
    for k in LEGACY_KEYS {
        config::set_config_json(k, None);
    }
    list
}

pub fn save(list: &[Item]) {
    config::set_config_json("hotkeys", Some(list_to_json(list)));
}

/// Primeira combinação do modo (texto de dica: "Salvar trecho (Ctrl+Shift+Pause/Break)").
pub fn first_spec(list: &[Item], mode: Mode) -> String {
    list.iter().find(|i| i.mode == mode && !i.hotkey.is_empty()).map(|i| i.hotkey.clone()).unwrap_or_default()
}

/// Ids do `RegisterHotKey`: dois por item (o segundo é o Ctrl+Break do
/// Ctrl+Pause).
pub const ID_BASE: i32 = 200;

pub fn ids_of(index: usize) -> (i32, i32) {
    let id = ID_BASE + 2 * index as i32;
    (id, id + 1)
}

pub fn index_of_id(id: i32) -> Option<usize> {
    (id >= ID_BASE).then(|| ((id - ID_BASE) / 2) as usize)
}

/// Janelas cujo título contém o trecho (sem diferenciar maiúsculas). Trecho
/// vazio não casa nada — senão o atalho gravaria "qualquer janela".
pub fn title_matches(title: &str, pattern: &str) -> bool {
    let p = pattern.trim().to_lowercase();
    !p.is_empty() && title.to_lowercase().contains(&p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_ida_e_volta() {
        let v = json!([
            { "mode": "record", "hotkey": "pause" },
            { "mode": "replay", "hotkey": "ctrl+shift+f10" },
            { "mode": "window", "hotkey": "Ctrl+Shift+F7", "title": " nova guia ", "appAudio": true, "delaySec": 2 },
            { "mode": "area", "hotkey": "Ctrl+Pause/Break", "area": { "x": -1683, "y": 1114, "w": 642, "h": 465 }, "delaySec": 3 },
            { "mode": "area", "hotkey": "Ctrl+Alt+Pause/Break", "area": null },
            { "mode": "outra coisa", "hotkey": "F1" }
        ]);
        let l = list_from_json(&v);
        assert_eq!(l.len(), 5, "modo desconhecido sai");
        assert_eq!(l[0].hotkey, "Pause/Break");
        assert_eq!(l[1].hotkey, "Ctrl+Shift+F10");
        assert_eq!(l[2].title, "nova guia");
        assert!(l[2].app_audio);
        assert_eq!(l[3].area, Some(AreaRect { x: -1683, y: 1114, w: 642, h: 465 }));
        assert_eq!(l[3].delay_sec, 3);
        assert_eq!(l[4].area, None);
        assert_eq!(list_from_json(&list_to_json(&l)), l);
        // Campos de outro modo não vão pro arquivo.
        let r = item_to_json(&l[1]);
        assert!(r.get("delaySec").is_none() && r.get("title").is_none() && r.get("area").is_none());
    }

    #[test]
    fn atraso_tem_teto_e_replay_nao_tem_atraso() {
        let it = item_from_json(&json!({ "mode": "record", "hotkey": "F9", "delaySec": 999 })).unwrap();
        assert_eq!(it.delay_sec, DELAY_MAX);
        let it = item_from_json(&json!({ "mode": "replay", "hotkey": "F9", "delaySec": 5 })).unwrap();
        assert_eq!(it.delay_sec, 0);
    }

    #[test]
    fn area_pequena_demais_vira_selecao() {
        let it = item_from_json(&json!({ "mode": "area", "hotkey": "F9", "area": { "x": 0, "y": 0, "w": 10, "h": 500 } })).unwrap();
        assert_eq!(it.area, None);
    }

    fn cfg<'a>(pairs: &'a [(&'a str, Value)]) -> impl Fn(&str) -> Option<Value> + 'a {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
    }

    #[test]
    fn migracao_instalacao_nova_da_os_padroes() {
        assert_eq!(migrate_legacy(cfg(&[])), defaults());
    }

    #[test]
    fn migracao_completa() {
        let l = migrate_legacy(cfg(&[
            ("hotkey", json!("Ctrl+F9")),
            ("replayHotkey", json!("")),
            ("areaHotkey", json!("Ctrl+Pause/Break")),
            ("areaSaved", json!({ "x": 10, "y": 20, "w": 640, "h": 480 })),
            ("areaDelaySec", json!(3)),
        ]));
        assert_eq!(l.len(), 2, "replay desligado (vazio) nao nasce");
        assert_eq!(l[0], Item::new(Mode::Record, "Ctrl+F9"));
        assert_eq!(l[1].mode, Mode::Area);
        assert_eq!(l[1].hotkey, "Ctrl+Pause/Break");
        assert_eq!(l[1].area, Some(AreaRect { x: 10, y: 20, w: 640, h: 480 }));
        assert_eq!(l[1].delay_sec, 3);
    }

    #[test]
    fn migracao_nao_repete_o_atalho_da_area_no_salvar_trecho() {
        // Área salva com o atalho padrão de antes e o salvar trecho nunca
        // configurado: cada um fica com o que essa pessoa já usava.
        let l = migrate_legacy(cfg(&[("areaSaved", json!({ "x": 0, "y": 0, "w": 640, "h": 480 }))]));
        assert_eq!(l[1].hotkey, LEGACY_REPLAY);
        assert_eq!(l[2].hotkey, DEFAULT_LEGACY_AREA);
        assert!(duplicates(&l).is_empty());
        // Salvar trecho configurado à mão não é tocado (a tela avisa a repetição).
        let l = migrate_legacy(cfg(&[
            ("replayHotkey", json!("Ctrl+Shift+Pause/Break")),
            ("areaSaved", json!({ "x": 0, "y": 0, "w": 640, "h": 480 })),
        ]));
        assert_eq!(l[1].hotkey, "Ctrl+Shift+Pause/Break");
    }

    #[test]
    fn migracao_area_sem_retangulo_nao_vira_item() {
        let l = migrate_legacy(cfg(&[("areaHotkey", json!("Ctrl+Pause/Break"))]));
        assert!(l.iter().all(|i| i.mode != Mode::Area));
    }

    #[test]
    fn duplicadas_sao_as_que_repetem_depois() {
        let l = vec![
            Item::new(Mode::Record, "Pause/Break"),
            Item::new(Mode::Area, "Ctrl+Pause/Break"),
            Item::new(Mode::Window, "pause"),
            Item::new(Mode::Window, ""),
            Item::new(Mode::Replay, ""),
        ];
        assert_eq!(duplicates(&l), vec![2]);
    }

    #[test]
    fn ids_por_item() {
        assert_eq!(ids_of(0), (200, 201));
        assert_eq!(ids_of(3), (206, 207));
        assert_eq!(index_of_id(207), Some(3));
        assert_eq!(index_of_id(206), Some(3));
        assert_eq!(index_of_id(105), None);
    }

    #[test]
    fn titulo_casa_por_trecho() {
        assert!(title_matches("Nova guia - Google Chrome", "nova GUIA"));
        assert!(!title_matches("Bloco de notas", "nova guia"));
        assert!(!title_matches("qualquer", "  "));
    }

    #[test]
    fn load_migra_e_apaga_chaves_antigas() {
        let _s = crate::testkit::begin();
        config::set_config_str("hotkey", "Ctrl+F9");
        config::set_config_str("replayHotkey", "");
        config::set_config_json("areaSaved", Some(json!({ "x": 0, "y": 0, "w": 800, "h": 600 })));
        config::set_config_int("areaDelaySec", 5);
        let l = load();
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].delay_sec, 5);
        assert_eq!(l[1].hotkey, DEFAULT_LEGACY_AREA);
        for k in LEGACY_KEYS {
            assert_eq!(config::get_config_str(k, "x"), "x", "{k} ficou no config");
            assert!(config::get_config_json(k).is_none());
        }
        // Segunda leitura vem da lista, não migra de novo.
        config::set_config_str("hotkey", "F1");
        assert_eq!(load(), l);
    }
}

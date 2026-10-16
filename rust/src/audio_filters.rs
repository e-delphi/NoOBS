//! Filtros de áudio do OBS nos microfones da gravação (porte de `OBSAudioFilters.pas`,
//! pegadinha #63).
//!
//! Os filtros são os do plugin `obs-filters` (supressão de ruído, porta de ruído,
//! compressor, limitador, ganho, equalizador...). Este módulo:
//!
//! * LISTA os filtros de áudio registrados (`obs_enum_filter_types`, só os com
//!   `OBS_SOURCE_AUDIO` e sem marca de obsoleto) numa ordem de cadeia que faz sentido
//!   (limpar o ruído antes de subir o ganho, limitar por último);
//! * descreve as PROPRIEDADES de cada um num JSON genérico para a UI montar o editor
//!   (slider, número, lista, caixa) — os rótulos vêm do próprio plugin, no idioma do app
//!   (`obs_set_locale`);
//! * guarda a escolha no config (`audioFilters`: id → enabled + settings);
//! * PENDURA a cadeia habilitada em cada fonte de microfone ([`apply_audio_filters`],
//!   chamado pelo engine ao montar o grafo);
//! * faz o TESTE A/B: grava o microfone CRU uma vez e refaz a versão com filtros a partir
//!   desse mesmo trecho a cada mudança (sem gravar de novo), empurrando as amostras numa
//!   fonte nossa sem som próprio (`noobs_audio_feed`) com a cadeia pendurada.
//!
//! Tudo aqui é libobs: MAIN THREAD, com o core já inicializado (pegadinha #3). A única
//! exceção é o callback de captura do microfone cru, que roda na thread do WASAPI e só copia
//! amostras sob lock.
//!
//! Propriedades SEMPRE de uma INSTÂNCIA do filtro, nunca do tipo
//! (`obs_get_source_properties`): o `expander_properties` desreferencia o próprio estado
//! (`cd->is_upwcomp`) e com `data = NULL` derrubaria o app.

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::ffi::obs::*;
use crate::{config, engine, log};

const CONFIG_KEY: &str = "audioFilters";

/// Ordem da cadeia. O que não está aqui (filtro de plugin que um dia entre) vai no fim, na
/// ordem em que a libobs o registrou.
/// * supressão/porta/expansor: tiram ruído ANTES de qualquer ganho, senão o ganho sobe o
///   chiado junto;
/// * equalizador e ganho: moldam e nivelam o que sobrou;
/// * compressor e limitador: por último, controlam os picos do resultado.
const CHAIN_ORDER: [&str; 9] = [
    "noise_suppress_filter_v2",
    "noise_gate_filter",
    "expander_filter",
    "upward_compressor_filter",
    "basic_eq_filter",
    "gain_filter",
    "compressor_filter",
    "limiter_filter",
    "invert_polarity_filter",
];

/// Campos que não fazem sentido aqui: o sidechain do compressor aponta para OUTRA fonte da
/// cena do OBS, que no NoOBS não existe.
const SKIPPED_PROPS: [&str; 1] = ["sidechain_source"];

/// `obs_reset_audio` do engine.
const SAMPLE_RATE: usize = 48_000;
const MAX_TEST_SEC: usize = 15;

const OBS_NUMBER_SLIDER: i32 = 1;
const OBS_TEXT_INFO: i32 = 3;
const OBS_COMBO_FORMAT_INT: i32 = 1;
const OBS_COMBO_FORMAT_FLOAT: i32 = 2;
const OBS_COMBO_FORMAT_STRING: i32 = 3;

fn from_c(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// Fonte do libobs liberada no `Drop` (o `try/finally` + `obs_source_release`).
struct Source(ObsSource);

impl Source {
    fn private(id: &str, name: &str, settings: ObsDataRaw) -> Option<Self> {
        let a = api().ok()?;
        let s = unsafe { (a.obs_source_create_private)(cstring(id).as_ptr(), cstring(name).as_ptr(), settings) };
        (!s.is_null()).then_some(Self(s))
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.obs_source_release)(self.0) }
        }
    }
}

/// `obs_properties_t` destruído no `Drop`.
struct Props(ObsProperties);

impl Drop for Props {
    fn drop(&mut self) {
        if !self.0.is_null() {
            if let Ok(a) = api() {
                unsafe { (a.obs_properties_destroy)(self.0) }
            }
        }
    }
}

/// `obs_data_t` OBTIDO (a referência é nossa) e liberado no `Drop`.
struct Data(ObsDataRaw);

impl Drop for Data {
    fn drop(&mut self) {
        if !self.0.is_null() {
            if let Ok(a) = api() {
                unsafe { (a.obs_data_release)(self.0) }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tipos registrados
// ---------------------------------------------------------------------------

fn registered_audio_filter_ids() -> Vec<String> {
    let Ok(a) = api() else { return Vec::new() };
    let mut all: Vec<String> = Vec::new();
    let mut idx = 0usize;
    let mut p: *const c_char = std::ptr::null();
    unsafe {
        while (a.obs_enum_filter_types)(idx, &mut p) {
            idx += 1;
            if p.is_null() {
                continue;
            }
            let flags = (a.obs_get_source_output_flags)(p);
            if flags & OBS_SOURCE_AUDIO == 0 || flags & (OBS_SOURCE_DEPRECATED | OBS_SOURCE_CAP_DISABLED) != 0 {
                continue;
            }
            all.push(from_c(p));
        }
    }
    let mut result: Vec<String> = CHAIN_ORDER.iter().filter(|id| all.iter().any(|a| a == **id)).map(|s| (*s).to_owned()).collect();
    result.extend(all.into_iter().filter(|id| !CHAIN_ORDER.contains(&id.as_str())));
    result
}

// ---------------------------------------------------------------------------
// Config: { "<id>": { "enabled": bool, "settings": { ... } } }
// ---------------------------------------------------------------------------

fn load_cfg() -> Map<String, Value> {
    match config::get_config_json(CONFIG_KEY) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

fn is_enabled(cfg: &Map<String, Value>, id: &str) -> bool {
    cfg.get(id).and_then(|e| e.get("enabled")).and_then(Value::as_bool).unwrap_or(false)
}

fn settings_json_of(cfg: &Map<String, Value>, id: &str) -> String {
    cfg.get(id).and_then(|e| e.get("settings")).filter(|s| s.is_object()).map_or_else(|| "{}".to_owned(), Value::to_string)
}

/// A entrada do filtro, criada vazia se faltar (ou se não for objeto).
fn entry_of<'a>(cfg: &'a mut Map<String, Value>, id: &str) -> &'a mut Map<String, Value> {
    if !cfg.get(id).is_some_and(Value::is_object) {
        cfg.insert(id.to_owned(), json!({ "enabled": false, "settings": {} }));
    }
    cfg.get_mut(id).and_then(Value::as_object_mut).unwrap()
}

/// Instância privada do filtro com as configurações salvas.
fn create_filter_instance(id: &str, settings_json: &str) -> Option<Source> {
    let a = api().ok()?;
    let raw = unsafe { (a.obs_data_create_from_json)(cstring(settings_json).as_ptr()) };
    let settings = Data(if raw.is_null() { unsafe { (a.obs_data_create)() } } else { raw });
    Source::private(id, &format!("NoOBS filtro {id}"), settings.0)
}

// ---------------------------------------------------------------------------
// Propriedades → JSON
// ---------------------------------------------------------------------------

fn add_props(props: ObsProperties, settings: ObsDataRaw, out: &mut Vec<Value>) {
    if props.is_null() {
        return;
    }
    let Ok(a) = api() else { return };
    unsafe {
        let mut p = (a.obs_properties_first)(props);
        while !p.is_null() {
            let name = from_c((a.obs_property_name)(p));
            let name_c = cstring(&name);
            let kind = (a.obs_property_get_type)(p);
            if (a.obs_property_visible)(p) && !SKIPPED_PROPS.contains(&name.as_str()) {
                let mut o: Option<Map<String, Value>> = None;
                match kind {
                    OBS_PROPERTY_BOOL => {
                        o = Some(
                            json!({ "type": "bool", "value": (a.obs_data_get_bool)(settings, name_c.as_ptr()) }).as_object().unwrap().clone(),
                        );
                    }
                    OBS_PROPERTY_INT => {
                        o = Some(
                            json!({
                                "type": "int",
                                "min": (a.obs_property_int_min)(p),
                                "max": (a.obs_property_int_max)(p),
                                "step": (a.obs_property_int_step)(p).max(1),
                                "slider": (a.obs_property_int_type)(p) == OBS_NUMBER_SLIDER,
                                "suffix": from_c((a.obs_property_int_suffix)(p)),
                                "value": (a.obs_data_get_int)(settings, name_c.as_ptr()),
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        );
                    }
                    OBS_PROPERTY_FLOAT => {
                        o = Some(
                            json!({
                                "type": "float",
                                "min": (a.obs_property_float_min)(p),
                                "max": (a.obs_property_float_max)(p),
                                "step": (a.obs_property_float_step)(p),
                                "slider": (a.obs_property_float_type)(p) == OBS_NUMBER_SLIDER,
                                "suffix": from_c((a.obs_property_float_suffix)(p)),
                                "value": (a.obs_data_get_double)(settings, name_c.as_ptr()),
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        );
                    }
                    OBS_PROPERTY_LIST => {
                        let fmt = (a.obs_property_list_format)(p);
                        if [OBS_COMBO_FORMAT_INT, OBS_COMBO_FORMAT_FLOAT, OBS_COMBO_FORMAT_STRING].contains(&fmt) {
                            let mut items = Vec::new();
                            let count = (a.obs_property_list_item_count)(p);
                            for i in 0..count {
                                if (a.obs_property_list_item_disabled)(p, i) {
                                    continue;
                                }
                                let item_name = from_c((a.obs_property_list_item_name)(p, i));
                                let value = match fmt {
                                    OBS_COMBO_FORMAT_INT => json!((a.obs_property_list_item_int)(p, i)),
                                    OBS_COMBO_FORMAT_FLOAT => json!((a.obs_property_list_item_float)(p, i)),
                                    _ => json!(from_c((a.obs_property_list_item_string)(p, i))),
                                };
                                items.push(json!({ "name": item_name, "value": value }));
                            }
                            let (format, value) = match fmt {
                                OBS_COMBO_FORMAT_INT => ("int", json!((a.obs_data_get_int)(settings, name_c.as_ptr()))),
                                OBS_COMBO_FORMAT_FLOAT => ("float", json!((a.obs_data_get_double)(settings, name_c.as_ptr()))),
                                _ => ("string", json!(from_c((a.obs_data_get_string)(settings, name_c.as_ptr())))),
                            };
                            o = Some(json!({ "type": "list", "items": items, "format": format, "value": value }).as_object().unwrap().clone());
                        }
                    }
                    // Só o texto informativo (aviso do plugin); campo de digitar não existe nos
                    // filtros de áudio.
                    OBS_PROPERTY_TEXT => {
                        if (a.obs_property_text_type)(p) == OBS_TEXT_INFO {
                            o = Some(json!({ "type": "info" }).as_object().unwrap().clone());
                        }
                    }
                    OBS_PROPERTY_GROUP => {
                        let mut sub = Vec::new();
                        add_props((a.obs_property_group_content)(p), settings, &mut sub);
                        o = Some(json!({ "type": "group", "props": sub }).as_object().unwrap().clone());
                    }
                    _ => {}
                }
                if let Some(mut o) = o {
                    o.insert("name".into(), json!(name));
                    o.insert("label".into(), json!(from_c((a.obs_property_description)(p))));
                    o.insert("hint".into(), json!(from_c((a.obs_property_long_description)(p))));
                    o.insert("enabled".into(), json!((a.obs_property_enabled)(p)));
                    out.push(Value::Object(o));
                }
            }
            if !(a.obs_property_next)(&mut p) {
                break;
            }
        }
    }
}

fn filter_json(cfg: &Map<String, Value>, id: &str) -> Value {
    let mut arr = Vec::new();
    let name = api().map(|a| from_c(unsafe { (a.obs_source_get_display_name)(cstring(id).as_ptr()) })).unwrap_or_default();
    if let (Ok(a), Some(inst)) = (api(), create_filter_instance(id, &settings_json_of(cfg, id))) {
        unsafe {
            let settings = Data((a.obs_source_get_settings)(inst.0));
            let props = Props((a.obs_source_properties)(inst.0));
            if !props.0.is_null() && !settings.0.is_null() {
                // Roda os callbacks de "modificado" com os valores atuais: é o que decide quais
                // campos aparecem (o OBS faz o mesmo ao abrir a tela).
                (a.obs_properties_apply_settings)(props.0, settings.0);
                add_props(props.0, settings.0, &mut arr);
            }
        }
    }
    json!({ "id": id, "name": name, "enabled": is_enabled(cfg, id), "props": arr })
}

/// Lista completa para a UI: `[{id, name, enabled, props:[...]}]`.
pub fn build_audio_filters_json() -> Vec<Value> {
    let cfg = load_cfg();
    registered_audio_filter_ids().iter().map(|id| filter_json(&cfg, id)).collect()
}

/// Um filtro só — depois de mudar um valor a visibilidade dos campos pode mudar (ex.: método
/// da supressão de ruído).
pub fn build_audio_filter_json(id: &str) -> Value {
    filter_json(&load_cfg(), id)
}

// ---------------------------------------------------------------------------
// Alterações
// ---------------------------------------------------------------------------

pub fn set_audio_filter_enabled(id: &str, enabled: bool) {
    let mut cfg = load_cfg();
    entry_of(&mut cfg, id).insert("enabled".into(), json!(enabled));
    config::set_config_json(CONFIG_KEY, Some(Value::Object(cfg)));
    log!("AudioFilters: {id} {}", if enabled { "ligado" } else { "desligado" });
}

/// Grava pelo `obs_data` (e não direto no JSON) para o valor ter o TIPO que o plugin espera:
/// um 3 que chega da UI num campo float tem que virar double, senão o `obs_data_get_double` do
/// filtro lê 0. `value`: bool / número / string, conforme o tipo do campo.
pub fn set_audio_filter_value(id: &str, key: &str, value: &Value) {
    if key.is_empty() {
        return;
    }
    let Ok(a) = api() else { return };
    let mut cfg = load_cfg();
    let Some(inst) = create_filter_instance(id, &settings_json_of(&cfg, id)) else { return };
    let key_c = cstring(key);
    unsafe {
        let (kind, fmt) = {
            let props = Props((a.obs_source_properties)(inst.0));
            let p = if props.0.is_null() { std::ptr::null_mut() } else { (a.obs_properties_get)(props.0, key_c.as_ptr()) };
            if p.is_null() {
                log!("AudioFilters: {id}.{key} nao existe.");
                return;
            }
            let kind = (a.obs_property_get_type)(p);
            (kind, if kind == OBS_PROPERTY_LIST { (a.obs_property_list_format)(p) } else { 0 })
        };

        // Parte do que JÁ estava salvo (só os valores do usuário, sem os defaults) — assim o
        // config não congela os defaults do plugin.
        let raw = (a.obs_data_create_from_json)(cstring(&settings_json_of(&cfg, id)).as_ptr());
        let cur = Data(if raw.is_null() { (a.obs_data_create)() } else { raw });
        let num = value.as_f64();
        if kind == OBS_PROPERTY_BOOL && value.is_boolean() {
            (a.obs_data_set_bool)(cur.0, key_c.as_ptr(), value.as_bool().unwrap_or(false));
        } else if kind == OBS_PROPERTY_INT || (kind == OBS_PROPERTY_LIST && fmt == OBS_COMBO_FORMAT_INT) {
            if let Some(n) = num {
                (a.obs_data_set_int)(cur.0, key_c.as_ptr(), n.round_ties_even() as i64);
            }
        } else if kind == OBS_PROPERTY_FLOAT || (kind == OBS_PROPERTY_LIST && fmt == OBS_COMBO_FORMAT_FLOAT) {
            if let Some(n) = num {
                (a.obs_data_set_double)(cur.0, key_c.as_ptr(), n);
            }
        } else if kind == OBS_PROPERTY_LIST && fmt == OBS_COMBO_FORMAT_STRING {
            let s = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
            (a.obs_data_set_string)(cur.0, key_c.as_ptr(), cstring(&s).as_ptr());
        } else {
            log!("AudioFilters: {id}.{key} tipo {kind} nao editavel.");
            return;
        }

        // Alguns campos mexem em outros (preset do expansor preenche razão, limiar...): passa
        // pela instância para os callbacks rodarem e guarda o que ficou.
        (a.obs_source_update)(inst.0, cur.0);
        {
            let settings = Data((a.obs_source_get_settings)(inst.0));
            let props = Props((a.obs_source_properties)(inst.0));
            if !props.0.is_null() && !settings.0.is_null() {
                let p = (a.obs_properties_get)(props.0, key_c.as_ptr());
                if !p.is_null() {
                    (a.obs_property_modified)(p, settings.0);
                }
                (a.obs_data_apply)(cur.0, settings.0);
            }
        }
        let json = from_c((a.obs_data_get_json)(cur.0));
        drop(cur);
        let Ok(new_settings @ Value::Object(_)) = serde_json::from_str::<Value>(&json) else { return };
        entry_of(&mut cfg, id).insert("settings".into(), new_settings);
    }
    config::set_config_json(CONFIG_KEY, Some(Value::Object(cfg)));
}

/// "Restaurar padrões" das Configurações: nenhum filtro ligado e os ajustes de todos de volta
/// aos do plugin (apaga a chave do config).
pub fn reset_all_audio_filters() {
    config::set_config_json(CONFIG_KEY, Some(json!({})));
}

pub fn reset_audio_filter(id: &str) {
    let mut cfg = load_cfg();
    entry_of(&mut cfg, id).insert("settings".into(), json!({}));
    config::set_config_json(CONFIG_KEY, Some(Value::Object(cfg)));
}

pub fn enabled_audio_filter_count() -> usize {
    let cfg = load_cfg();
    registered_audio_filter_ids().iter().filter(|id| is_enabled(&cfg, id)).count()
}

/// Pendura os filtros habilitados, na ordem da cadeia, numa fonte de microfone recém-criada.
pub fn apply_audio_filters(source: ObsSource) {
    if source.is_null() {
        return;
    }
    let Ok(a) = api() else { return };
    let cfg = load_cfg();
    let mut names: Vec<String> = Vec::new();
    // `obs_source_filter_add` insere na FRENTE da lista e o áudio percorre a lista de trás para
    // frente — ou seja, o primeiro adicionado é o primeiro a processar. Adicionar na ordem da
    // cadeia = cadeia certa.
    for id in registered_audio_filter_ids() {
        if !is_enabled(&cfg, &id) {
            continue;
        }
        let Some(f) = create_filter_instance(&id, &settings_json_of(&cfg, &id)) else {
            log!("AudioFilters: nao consegui criar {id}.");
            continue;
        };
        unsafe { (a.obs_source_filter_add)(source, f.0) };
        // `f` cai aqui e libera a NOSSA referência: o filter_add pegou a própria.
        names.push(id);
    }
    if !names.is_empty() {
        log!("   filtros: {}", names.join(", "));
    }
}

/// Idioma dos rótulos que vêm do plugin. `lang` no vocabulário do app (`pt-BR`, `en`, `es`...).
pub fn set_obs_locale_from_app(lang: &str) {
    let Ok(a) = api() else { return };
    let loc = engine::obs_locale_for_app(lang);
    unsafe { (a.obs_set_locale)(cstring(&loc).as_ptr()) };
}

// ---------------------------------------------------------------------------
// Teste A/B: grava o ORIGINAL uma vez e refaz a versão filtrada a cada mudança, sem gravar de
// novo.
//
// O trecho cru fica guardado (`clip_l`/`clip_r`, float estéreo no formato do áudio do OBS).
// Para gerar uma versão, ele é EMPURRADO bloco a bloco (`obs_source_output_audio`) numa fonte
// nossa sem som próprio (`noobs_audio_feed`) com a cadeia de filtros pendurada; o callback de
// captura devolve o resultado. Tudo síncrono, na main thread: o `obs_source_output_audio` roda
// os filtros e o callback antes de voltar. Medido: 4 s de áudio refeitos em 40 ms (ganho) a
// ~110 ms (RNNoise).
//
// A versão ORIGINAL passa pelo mesmo caminho, só que sem filtros — sai idêntica ao capturado
// (medido), e as duas ficam com o mesmo formato.
//
// Filtro com estado (porta, compressor, supressão) começa do zero a cada versão: fonte nova,
// filtros novos.
// ---------------------------------------------------------------------------

const FEED_SOURCE_ID: &CStr = c"noobs_audio_feed";
/// `AUDIO_OUTPUT_FRAMES` do OBS.
const FEED_CHUNK: usize = 1024;
/// Silêncio empurrado depois do trecho: filtro com atraso (a supressão de ruído guarda uns
/// milissegundos) só devolve o fim do trecho se receber mais áudio depois dele.
const FEED_TAIL_SEC: f64 = 0.25;

/// `struct obs_source_info` (obs-source.h), SÓ os campos iniciais. O `obs_register_source_s`
/// recebe o TAMANHO e zera o resto — o layout que importa é só este prefixo (40 bytes).
#[repr(C)]
struct ObsSourceInfoHead {
    id: *const c_char,
    /// `OBS_SOURCE_TYPE_INPUT` = 0
    ty: i32,
    output_flags: u32,
    get_name: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    create: unsafe extern "C" fn(ObsDataRaw, ObsSource) -> *mut c_void,
    /// chamado sempre que o `create` devolveu data
    destroy: unsafe extern "C" fn(*mut c_void),
}

/// O "data" da fonte: só precisa ser não-nulo.
static FEED_DUMMY: u8 = 0;

unsafe extern "C" fn feed_get_name(_type_data: *mut c_void) -> *const c_char {
    c"NoOBS audio feed".as_ptr()
}

unsafe extern "C" fn feed_create(_settings: ObsDataRaw, _source: ObsSource) -> *mut c_void {
    // Sem data a libobs loga "Failed to create source" e marca a fonte como quebrada; o
    // conteúdo não importa, a fonte não faz nada sozinha.
    &FEED_DUMMY as *const u8 as *mut c_void
}

unsafe extern "C" fn feed_destroy(_data: *mut c_void) {}

/// Registra a fonte uma vez por vida do libobs (que no NoOBS é a vida do processo).
fn ensure_feed_registered() -> bool {
    let Ok(a) = api() else { return false };
    unsafe {
        if (a.obs_get_source_output_flags)(FEED_SOURCE_ID.as_ptr()) == 0 {
            // Só AUDIO: é o que deixa o `obs_source_filter_add` aceitar filtro de áudio
            // (`filter_compatible` compara as flags).
            let info = ObsSourceInfoHead {
                id: FEED_SOURCE_ID.as_ptr(),
                ty: 0,
                output_flags: OBS_SOURCE_AUDIO,
                get_name: feed_get_name,
                create: feed_create,
                destroy: feed_destroy,
            };
            (a.obs_register_source_s)(&info as *const _ as *const c_void, std::mem::size_of::<ObsSourceInfoHead>());
        }
        let ok = (a.obs_get_source_output_flags)(FEED_SOURCE_ID.as_ptr()) != 0;
        if !ok {
            log!("AudioFilters: nao consegui registrar a fonte {}.", FEED_SOURCE_ID.to_string_lossy());
        }
        ok
    }
}

/// Saída de uma versão (original ou filtrada), já DEPOIS dos filtros: mono 16 bits para o WAV,
/// com pico e soma dos quadrados.
struct TestCapture {
    samples: Vec<i16>,
    max_samples: usize,
    peak: f32,
    sum_sq: f64,
}

/// Captura do microfone cru: float estéreo, como o OBS entrega.
struct RawCapture {
    l: Vec<f32>,
    r: Vec<f32>,
    max_samples: usize,
}

/// Saída de uma versão: mistura os canais em mono (o microfone é mono; o OBS só o espalha).
/// Chamado SÍNCRONO dentro do `obs_source_output_audio`, na main.
unsafe extern "C" fn test_capture_cb(param: *mut c_void, _source: ObsSource, audio: *const AudioData, _muted: bool) {
    let cap = &*(param as *const Mutex<TestCapture>);
    if audio.is_null() || (*audio).data[0].is_null() {
        return;
    }
    let audio = &*audio;
    let (l, r) = (audio.data[0] as *const f32, audio.data[1] as *const f32);
    let mut c = cap.lock().unwrap();
    for i in 0..audio.frames as usize {
        if c.samples.len() >= c.max_samples {
            break;
        }
        let mut v = if r.is_null() { *l.add(i) } else { (*l.add(i) + *r.add(i)) * 0.5 };
        if v.abs() > c.peak {
            c.peak = v.abs();
        }
        c.sum_sq += (v * v) as f64;
        v = v.clamp(-1.0, 1.0);
        c.samples.push((v * 32767.0).round_ties_even() as i16);
    }
}

/// Microfone cru, na thread de captura do WASAPI: só copia sob lock.
unsafe extern "C" fn raw_capture_cb(param: *mut c_void, _source: ObsSource, audio: *const AudioData, _muted: bool) {
    let cap = &*(param as *const Mutex<RawCapture>);
    if audio.is_null() || (*audio).data[0].is_null() {
        return;
    }
    let audio = &*audio;
    let l = audio.data[0] as *const f32;
    let r = if audio.data[1].is_null() { l } else { audio.data[1] as *const f32 };
    let mut c = cap.lock().unwrap();
    for i in 0..audio.frames as usize {
        if c.l.len() >= c.max_samples {
            break;
        }
        c.l.push(*l.add(i));
        c.r.push(*r.add(i));
    }
}

/// A fonte do microfone cru e o buffer que o callback do WASAPI enche. O `Arc` é emprestado ao
/// callback por ponteiro cru (`Arc::into_raw`) e retomado em `release`.
struct RawTest {
    src: ObsSource,
    cap: Arc<Mutex<RawCapture>>,
    param: *const Mutex<RawCapture>,
}

#[derive(Default)]
struct TestState {
    raw: Option<RawTest>,
    clip_l: Vec<f32>,
    clip_r: Vec<f32>,
}

thread_local! {
    static TEST: RefCell<TestState> = RefCell::new(TestState::default());
}

pub fn audio_filter_test_running() -> bool {
    TEST.with(|t| t.borrow().raw.is_some())
}

/// Há um trecho gravado para refazer? (Um teste já terminou.)
pub fn has_audio_filter_clip() -> bool {
    TEST.with(|t| !t.borrow().clip_l.is_empty())
}

/// Solta a fonte do microfone cru; com o callback removido nada mais escreve no buffer.
fn release_test() -> Option<Arc<Mutex<RawCapture>>> {
    let raw = TEST.with(|t| t.borrow_mut().raw.take())?;
    if let Ok(a) = api() {
        unsafe {
            (a.obs_source_remove_audio_capture_callback)(raw.src, raw_capture_cb, raw.param as *mut c_void);
            (a.obs_source_release)(raw.src);
        }
    }
    // Devolve a referência que o callback segurava.
    unsafe { drop(Arc::from_raw(raw.param)) };
    Some(raw.cap)
}

/// `device_id`: id do endpoint (o mesmo do WinAudioMeter/libobs) ou vazio = microfone padrão do
/// Windows. `Err` = motivo curto para o log.
pub fn start_audio_filter_test(device_id: &str) -> Result<(), String> {
    if audio_filter_test_running() {
        return Err("busy".into());
    }
    let a = api()?;
    let cap = Arc::new(Mutex::new(RawCapture { l: Vec::with_capacity(SAMPLE_RATE * (MAX_TEST_SEC + 1)), r: Vec::with_capacity(SAMPLE_RATE * (MAX_TEST_SEC + 1)), max_samples: SAMPLE_RATE * (MAX_TEST_SEC + 1) }));
    let dev = if device_id.is_empty() { "default" } else { device_id };
    let settings = ObsData::create()?;
    settings.set_string("device_id", dev);
    let src = unsafe { (a.obs_source_create_private)(cstring("wasapi_input_capture").as_ptr(), cstring("NoOBS teste de filtros").as_ptr(), settings.raw()) };
    if src.is_null() {
        return Err("source".into());
    }
    let param = Arc::into_raw(Arc::clone(&cap));
    unsafe { (a.obs_source_add_audio_capture_callback)(src, raw_capture_cb, param as *mut c_void) };
    TEST.with(|t| t.borrow_mut().raw = Some(RawTest { src, cap, param }));
    log!("AudioFilters: teste gravando (device={}).", if device_id.is_empty() { "default" } else { device_id });
    Ok(())
}

/// WAV PCM 16 bits mono 48 kHz, cabeçalho de 44 bytes. `(wav, pico dBFS, médio dBFS)`; -100 =
/// silêncio.
fn make_wav(cap: &TestCapture) -> (Vec<u8>, f64, f64) {
    let n = cap.samples.len();
    let peak_db = if cap.peak > 0.00001 { 20.0 * (cap.peak as f64).log10() } else { -100.0 };
    let rms_db = if n > 0 && cap.sum_sq / n as f64 > 1e-10 { 10.0 * (cap.sum_sq / n as f64).log10() } else { -100.0 };
    let data_bytes = (n * 2) as u32;
    let mut w = Vec::with_capacity(44 + n * 2);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
    w.extend_from_slice(&(SAMPLE_RATE as u32 * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_bytes.to_le_bytes());
    for s in &cap.samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    (w, peak_db, rms_db)
}

/// Passa o trecho guardado pela fonte `noobs_audio_feed` (com ou sem a cadeia de filtros) e
/// devolve `(wav, pico dB, médio dB)` do que saiu.
fn render_clip(with_filters: bool) -> (Vec<u8>, f64, f64) {
    let empty = (Vec::new(), -100.0, -100.0);
    let (clip_l, clip_r) = TEST.with(|t| {
        let t = t.borrow();
        (t.clip_l.clone(), t.clip_r.clone())
    });
    let n = clip_l.len();
    let Ok(a) = api() else { return empty };
    if n == 0 || !ensure_feed_registered() {
        return empty;
    }
    let Some(src) = Source::private(&FEED_SOURCE_ID.to_string_lossy(), "NoOBS teste", std::ptr::null_mut()) else { return empty };
    // `max_samples` = tamanho do original: o que sobra (atraso do filtro + o silêncio da cauda)
    // fica de fora e as duas versões têm o mesmo tamanho.
    let cap = Mutex::new(TestCapture { samples: Vec::with_capacity(n), max_samples: n, peak: 0.0, sum_sq: 0.0 });
    if with_filters {
        apply_audio_filters(src.0);
    }
    let total = n + (FEED_TAIL_SEC * SAMPLE_RATE as f64).round() as usize;
    let mut buf_l = vec![0f32; FEED_CHUNK];
    let mut buf_r = vec![0f32; FEED_CHUNK];
    unsafe {
        (a.obs_source_add_audio_capture_callback)(src.0, test_capture_cb, &cap as *const _ as *mut c_void);
        let ts = (a.os_gettime_ns)();
        let mut i = 0usize;
        while i < total {
            let k = FEED_CHUNK.min(total - i);
            for j in 0..k {
                let (l, r) = if i + j < n { (clip_l[i + j], clip_r[i + j]) } else { (0.0, 0.0) };
                buf_l[j] = l;
                buf_r[j] = r;
            }
            let mut data = [std::ptr::null::<c_void>(); MAX_AV_PLANES];
            data[0] = buf_l.as_ptr() as *const c_void;
            data[1] = buf_r.as_ptr() as *const c_void;
            let audio = ObsSourceAudio {
                data,
                frames: k as u32,
                speakers: SPEAKERS_STEREO,
                format: AUDIO_FORMAT_FLOAT_PLANAR,
                samples_per_sec: SAMPLE_RATE as u32,
                // Relógio contínuo: salto de timestamp faz a libobs reiniciar o alinhamento da
                // fonte.
                timestamp: ts + (i as u64 * 1_000_000_000) / SAMPLE_RATE as u64,
            };
            (a.obs_source_output_audio)(src.0, &audio);
            i += k;
        }
        (a.obs_source_remove_audio_capture_callback)(src.0, test_capture_cb, &cap as *const _ as *mut c_void);
    }
    let result = make_wav(&cap.lock().unwrap());
    result
}

#[derive(Default)]
pub struct TestResult {
    pub raw_wav: Vec<u8>,
    pub filt_wav: Vec<u8>,
    pub raw_peak_db: f64,
    pub filt_peak_db: f64,
    pub raw_rms_db: f64,
    pub filt_rms_db: f64,
}

/// Fecha as capturas e devolve os dois WAV (PCM 16 bits, mono, 48 kHz) e, de cada um, o pico e
/// o nível MÉDIO (RMS) em dBFS (-100 = silêncio). O médio é o que a supressão de ruído muda: os
/// picos são a voz, que ela preserva de propósito.
pub fn finish_audio_filter_test() -> TestResult {
    // Solta a fonte ANTES de ler: com o callback removido nada mais escreve.
    let cap = release_test();
    let (l, r) = match cap {
        Some(c) => {
            let g = c.lock().unwrap();
            (g.l.clone(), g.r.clone())
        }
        None => (Vec::new(), Vec::new()),
    };
    let n = l.len();
    TEST.with(|t| {
        let mut t = t.borrow_mut();
        t.clip_l = l;
        t.clip_r = r;
    });
    let (raw_wav, raw_peak_db, raw_rms_db) = render_clip(false);
    let (filt_wav, filt_peak_db, filt_rms_db) = render_clip(true);
    log!(
        "AudioFilters: teste gravado ({n} amostras, {} filtro(s); original: media {raw_rms_db:.1} / pico {raw_peak_db:.1} dB; com filtros: media {filt_rms_db:.1} / pico {filt_peak_db:.1} dB).",
        enabled_audio_filter_count()
    );
    TestResult { raw_wav, filt_wav, raw_peak_db, filt_peak_db, raw_rms_db, filt_rms_db }
}

/// Refaz só a versão COM filtros, a partir do trecho guardado e da cadeia ATUAL — chamado a cada
/// mudança de filtro. `None` = não há trecho.
pub fn rerender_audio_filter_test() -> Option<(Vec<u8>, f64, f64)> {
    if !has_audio_filter_clip() || audio_filter_test_running() {
        return None;
    }
    let r = render_clip(true);
    (!r.0.is_empty()).then_some(r)
}

pub fn cancel_audio_filter_test() {
    release_test();
    TEST.with(|t| {
        let mut t = t.borrow_mut();
        t.clip_l.clear();
        t.clip_r.clear();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_tem_cabecalho_e_niveis() {
        let cap = TestCapture { samples: vec![16384, -16384, 16384, -16384], max_samples: 4, peak: 0.5, sum_sq: 4.0 * 0.25 };
        let (w, peak, rms) = make_wav(&cap);
        assert_eq!(w.len(), 44 + 8);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 8);
        // 0,5 de amplitude = -6,02 dBFS de pico e de RMS (onda quadrada).
        assert!((peak + 6.02).abs() < 0.01);
        assert!((rms + 6.02).abs() < 0.01);
    }

    #[test]
    fn silencio_vale_menos_100() {
        let cap = TestCapture { samples: vec![0; 10], max_samples: 10, peak: 0.0, sum_sq: 0.0 };
        let (_, peak, rms) = make_wav(&cap);
        assert_eq!((peak, rms), (-100.0, -100.0));
    }

    #[test]
    fn entrada_do_config_nasce_desligada() {
        let mut cfg = Map::new();
        let e = entry_of(&mut cfg, "gain_filter");
        assert_eq!(e.get("enabled"), Some(&json!(false)));
        assert!(!is_enabled(&cfg, "gain_filter"));
        assert_eq!(settings_json_of(&cfg, "gain_filter"), "{}");
        entry_of(&mut cfg, "gain_filter").insert("enabled".into(), json!(true));
        assert!(is_enabled(&cfg, "gain_filter"));
    }
}

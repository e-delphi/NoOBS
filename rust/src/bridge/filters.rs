//! Filtros de áudio (aba Áudio) no Bridge: só a mensageria. Lista, edição e teste A/B moram em
//! [`crate::audio_filters`]. Os três precisam do libobs vivo (os filtros são tipos do plugin),
//! então sobem o core se o warmup ainda não o fez — o mesmo que o clique de gravar.

use std::cell::Cell;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::{audio_filters, engine, lang, log, ui, win_audio_meter};

use super::recording::{push_encoder_caps_once, with_engine_result};
use super::{is_shutting_down, post_json};

pub const TIMER_AUDIO_FILTER_TEST: usize = 7015;
const AUDIO_FILTER_TEST_SEC: u32 = 5;

thread_local! {
    /// Quantos filtros estavam ligados na versão filtrada que está na tela. Vai junto no push:
    /// sem isso, um teste feito com tudo desligado devolvia duas cópias idênticas e parecia que
    /// os filtros não funcionavam.
    static TEST_COUNT: Cell<usize> = const { Cell::new(0) };
}

/// Instala o gancho que pendura a cadeia em cada microfone ao montar o grafo.
pub fn install() {
    engine::set_mic_filter_hook(audio_filters::apply_audio_filters);
}

pub(super) fn ensure_obs_for_audio_filters() -> bool {
    engine::ensure_created();
    if let Err(e) = with_engine_result(|e| e.ensure_initialized()) {
        log!("AudioFilters: libobs nao subiu: {e}");
        return false;
    }
    push_encoder_caps_once();
    engine::with_engine(|e| e.is_initialized()).unwrap_or(false)
}

fn data_url(wav: &[u8]) -> String {
    // Base64 sem quebra de linha: a quebra invalidaria a `data:` URL. ~770 KB cada.
    format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(wav))
}

fn db1(v: f64) -> f64 {
    (v * 10.0).round_ties_even() / 10.0
}

pub fn handle_get_audio_filters() {
    let ready = ensure_obs_for_audio_filters();
    let filters = if ready { audio_filters::build_audio_filters_json() } else { Vec::new() };
    // Microfones para o teste: o padrão do Windows primeiro. O id é o do endpoint, que é o mesmo
    // `device_id` que o wasapi do OBS usa.
    let devs = win_audio_meter::enumerate_audio_devices();
    let mut mics: Vec<Value> = Vec::new();
    for default_pass in [true, false] {
        for d in devs.iter().filter(|d| d.kind == win_audio_meter::DeviceKind::Input && d.is_default == default_pass) {
            mics.push(json!({ "id": d.device_id, "name": d.name, "isDefault": d.is_default }));
        }
    }
    post_json(json!({
        "type": "audio_filters",
        "ready": ready,
        "filters": filters,
        "mics": mics,
        "testSec": AUDIO_FILTER_TEST_SEC,
        "testing": audio_filters::audio_filter_test_running(),
    }));
}

/// `filtered`: só a versão com filtros, refeita do trecho guardado (~0,1 s na main thread para
/// 5 s de áudio).
fn push_audio_filter_rerender() {
    let Some((wav, peak, rms)) = audio_filters::rerender_audio_filter_test() else { return };
    let count = audio_filters::enabled_audio_filter_count();
    TEST_COUNT.with(|c| c.set(count));
    post_json(json!({
        "type": "audio_filter_test",
        "state": "filtered",
        "filtered": data_url(&wav),
        "filteredPeakDb": db1(peak),
        "filteredRmsDb": db1(rms),
        "filters": count,
    }));
}

/// "Restaurar padrões": nenhum filtro ligado e os ajustes de volta aos do plugin. Não sobe o
/// libobs só para isto (é config); se ele já está vivo, a aba Áudio recebe a lista nova. Como
/// qualquer mudança de filtro, vale a partir do próximo grafo (filtro não é trocado numa fonte
/// viva).
pub fn handle_reset_audio_filters() {
    audio_filters::reset_all_audio_filters();
    log!("AudioFilters: todos restaurados ao padrao (nenhum ligado).");
    if engine::with_engine(|e| e.is_initialized()).unwrap_or(false) {
        handle_get_audio_filters();
    }
}

/// `op`: `enable` (`value`: bool) | `value` (`key` + `value`) | `reset`. Responde com o filtro
/// inteiro de novo: mudar um campo pode mostrar ou esconder outros (método da supressão de
/// ruído, preset do expansor).
pub fn handle_set_audio_filter(msg: &Value) {
    let id = msg.get("id").and_then(Value::as_str).unwrap_or("");
    let op = msg.get("op").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() || !ensure_obs_for_audio_filters() {
        return;
    }
    let value = msg.get("value").unwrap_or(&Value::Null);
    match op {
        "enable" => audio_filters::set_audio_filter_enabled(id, value.as_bool().unwrap_or(false)),
        "value" => audio_filters::set_audio_filter_value(id, msg.get("key").and_then(Value::as_str).unwrap_or(""), value),
        "reset" => audio_filters::reset_audio_filter(id),
        _ => return,
    }
    post_json(json!({ "type": "audio_filter", "filter": audio_filters::build_audio_filter_json(id) }));
    // Já há um trecho de teste gravado: refaz a versão com filtros na hora, com a cadeia nova,
    // para comparar sem precisar falar de novo.
    push_audio_filter_rerender();
}

fn push_test_state(state: &str, error: &str) {
    let mut o = json!({
        "type": "audio_filter_test",
        "state": state,
        "seconds": AUDIO_FILTER_TEST_SEC,
        "filters": TEST_COUNT.with(Cell::get),
    });
    if !error.is_empty() {
        o["error"] = json!(error);
    }
    post_json(o);
}

/// Grava `AUDIO_FILTER_TEST_SEC` segundos do microfone CRU e guarda o trecho. No fim, devolve o
/// original e a versão com a cadeia atual; depois disso cada mudança de filtro refaz só a
/// filtrada. Não passa pela gravação: nenhum arquivo nasce na biblioteca.
pub fn handle_test_audio_filters(device_id: &str) {
    if audio_filters::audio_filter_test_running() {
        return;
    }
    if !ensure_obs_for_audio_filters() {
        push_test_state("error", &lang::t("settings.audioFilters.testFailed"));
        return;
    }
    TEST_COUNT.with(|c| c.set(audio_filters::enabled_audio_filter_count()));
    if let Err(e) = audio_filters::start_audio_filter_test(device_id) {
        log!("AudioFilters: teste nao iniciou: {e}");
        push_test_state("error", &lang::t("settings.audioFilters.testFailed"));
        return;
    }
    push_test_state("recording", "");
    ui::set_timer(TIMER_AUDIO_FILTER_TEST, AUDIO_FILTER_TEST_SEC * 1000);
}

fn finish_audio_filter_test_and_push() {
    if !audio_filters::audio_filter_test_running() {
        return;
    }
    let r = audio_filters::finish_audio_filter_test();
    // A filtrada é feita AGORA, com a cadeia de agora (não a do início).
    let count = audio_filters::enabled_audio_filter_count();
    TEST_COUNT.with(|c| c.set(count));
    post_json(json!({
        "type": "audio_filter_test",
        "state": "done",
        "original": data_url(&r.raw_wav),
        "filtered": data_url(&r.filt_wav),
        "originalPeakDb": db1(r.raw_peak_db),
        "filteredPeakDb": db1(r.filt_peak_db),
        "originalRmsDb": db1(r.raw_rms_db),
        "filteredRmsDb": db1(r.filt_rms_db),
        "filters": count,
    }));
}

/// `false` = id que não é meu.
pub fn on_timer(id: usize) -> bool {
    if id != TIMER_AUDIO_FILTER_TEST {
        return false;
    }
    ui::kill_timer(TIMER_AUDIO_FILTER_TEST);
    if !is_shutting_down() {
        finish_audio_filter_test_and_push();
    }
    true
}

pub fn shutdown() {
    ui::kill_timer(TIMER_AUDIO_FILTER_TEST);
    audio_filters::cancel_audio_filter_test();
}

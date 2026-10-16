//! Há versão nova no GitHub Releases? (porte de `OBSUpdate.pas`)
//!
//! `GET https://api.github.com/repos/e-delphi/NoOBS/releases/latest`, sem
//! autenticação (60 pedidos/hora por IP — folgado para 1 por dia), pelo
//! WinHTTP do sistema (TLS do Windows, sem DLL extra).
//!
//! Sempre em worker; o resultado volta na main. Falha é silenciosa por
//! desenho: sem internet, rate limit ou API fora só vão pro log. Pior que não
//! avisar de uma atualização é avisar errado.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::win_http::{self, Request, Timeouts};
use crate::{log, mainthread, version};

const API_URL: &str = "https://api.github.com/repos/e-delphi/NoOBS/releases/latest";

#[derive(Debug, Clone, Default)]
pub struct UpdateResult {
    /// A consulta funcionou (200 + JSON válido).
    pub ok: bool,
    pub has_update: bool,
    pub latest_tag: String,
    pub release_url: String,
    /// Motivo quando `ok = false` (só pro log).
    pub error: String,
    /// Código HTTP. 0 = NÃO houve resposta (rede, DNS, timeout). A diferença
    /// importa: se o GitHub respondeu qualquer coisa (até 403/500), a checagem
    /// "aconteceu"; sem resposta, tem que ser tentada de novo em vez de
    /// queimar o intervalo de 24 h.
    pub status_code: u32,
}

static CHECKING: AtomicBool = AtomicBool::new(false);

pub fn is_checking() -> bool {
    CHECKING.load(Ordering::SeqCst)
}

fn fetch_latest() -> UpdateResult {
    let mut r = UpdateResult::default();
    // Timeouts curtos: roda em segundo plano e não vale segurar recurso.
    let req = Request::new("GET", API_URL)
        // A API RECUSA (403) pedido sem User-Agent.
        .header("User-Agent", "NoOBS-UpdateCheck")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeouts(Timeouts { connect_ms: 5_000, send_ms: 5_000, receive_ms: 8_000 });
    let resp = match win_http::request(&req) {
        Ok(resp) => resp,
        Err(e) => {
            r.error = e.to_string();
            return r;
        }
    };
    // O servidor respondeu: registra o código ANTES de qualquer saída.
    r.status_code = resp.status;
    if resp.status != 200 {
        // 404 = ainda sem release; 403 = rate limit.
        r.error = format!("HTTP {}", resp.status);
        return r;
    }
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&resp.body) else {
        r.error = "JSON inválido".into();
        return r;
    };
    r.latest_tag = json.get("tag_name").and_then(|v| v.as_str()).unwrap_or("").to_owned();
    r.release_url = json.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_owned();
    if r.latest_tag.trim().is_empty() {
        r.error = "release sem tag_name".into();
        return r;
    }
    r.ok = true;
    r.has_update = version::is_remote_newer(&r.latest_tag);
    r
}

/// Dispara em worker; `callback` roda na MAIN. No-op com uma já em curso
/// (clique repetido no botão).
pub fn check_for_updates(callback: fn(UpdateResult)) {
    if CHECKING.swap(true, Ordering::SeqCst) {
        log::log("Update: checagem já em andamento — ignorando.");
        return;
    }
    std::thread::spawn(move || {
        let r = fetch_latest();
        if r.ok {
            log!("Update: atual={} remoto={} novidade={}", version::APP_VERSION, r.latest_tag, r.has_update);
        } else {
            log!("Update: falha na checagem ({}).", r.error);
        }
        CHECKING.store(false, Ordering::SeqCst);
        mainthread::queue(move || callback(r));
    });
}

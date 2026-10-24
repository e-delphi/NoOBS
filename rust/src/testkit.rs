//! Kit dos testes que passam pelo `dispatch` (frente 1 do
//! `docs/mapa-ui-rust-dlls.md`): manda a mensagem como a página mandaria e
//! confere o que o Rust respondeu e o que mudou no config/disco.
//!
//! Isolamento:
//! * `LOCALAPPDATA`/`APPDATA` vão pra uma pasta temporária na PRIMEIRA sessão:
//!   config, cache, motor local, logs e buffer saem dali, nunca do perfil real
//!   (vários módulos leem a variável por conta própria, então desviar a
//!   variável cobre todos de uma vez);
//! * o `post_json` cai em [`sent`] em vez de ir pro WebView;
//! * o que escaparia do processo (registro do Windows, navegador, Lixeira,
//!   diálogo de pasta, rede, download) vira um registro em [`effects`] — os
//!   pontos de desvio têm `#[cfg(test)]` no próprio código;
//! * as sessões são SERIAIS (uma trava global): o config e a fila da main são
//!   globais, e dois testes ao mesmo tempo se atropelariam.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

use serde_json::Value;

static LOCK: Mutex<()> = Mutex::new(());
static SENT: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static EFFECTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static ENV: Once = Once::new();
static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Pasta temporária deste processo de teste (some no próximo `cargo test`).
pub fn sandbox_root() -> PathBuf {
    std::env::temp_dir().join(format!("noobs-test-{}", std::process::id()))
}

/// Uma sessão de teste: segura a trava, começa com config limpo e uma pasta
/// de gravação própria e vazia.
pub struct Session {
    _guard: MutexGuard<'static, ()>,
    /// Pasta de gravação desta sessão (já gravada em `recordDir`).
    pub rec: PathBuf,
}

pub fn begin() -> Session {
    ENV.call_once(|| {
        // Sobras de execuções anteriores (uma pasta por processo).
        let mine = sandbox_root();
        if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
            for e in rd.flatten() {
                let p = e.path();
                let antiga = p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("noobs-test-")) && p != mine;
                if antiga {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
        let root = sandbox_root();
        let _ = std::fs::create_dir_all(&root);
        // Antes de qualquer leitura do config (o store é carregado uma vez).
        std::env::set_var("LOCALAPPDATA", &root);
        std::env::set_var("APPDATA", &root);
    });
    let guard = lock(&LOCK);
    // O que sobrou de uma sessão anterior (worker que terminou atrasado) roda
    // agora e é descartado.
    // A worker da transcrição sobrevive ao teste que a ligou e continua avisando o
    // bridge: desligado aqui; quem testa a transcrição liga de novo
    // (`bridge::transcribe::do_init`, como o app faz).
    crate::transcribe::cancel_all();
    crate::transcribe::set_on_changed(|| {});
    crate::mainthread::drain();
    lock(&SENT).clear();
    lock(&EFFECTS).clear();
    crate::config::test_reset();
    // Mensagens com o texto de verdade (e o mesmo idioma em todo teste).
    crate::config::set_config_str("language", "pt-BR");
    crate::lang::init_language();
    crate::config::test_reset();
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let rec = sandbox_root().join(format!("rec-{n}"));
    let _ = std::fs::remove_dir_all(&rec);
    std::fs::create_dir_all(&rec).expect("pasta de gravacao do teste");
    crate::config::set_config_str("recordDir", &rec.to_string_lossy());
    crate::bridge::test_reset_browse_dir();
    Session { _guard: guard, rec }
}

impl Session {
    /// Cria um arquivo (com pastas) dentro da pasta de gravação.
    pub fn file(&self, rel: &str, bytes: &[u8]) -> String {
        let p = self.rec.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(&p, bytes).unwrap();
        p.to_string_lossy().into_owned()
    }

    pub fn dir(&self, rel: &str) -> String {
        let p = self.rec.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p.to_string_lossy().into_owned()
    }

    pub fn path(&self, rel: &str) -> String {
        self.rec.join(rel).to_string_lossy().into_owned()
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.rec.join(rel).exists()
    }
}

/// Chamado pelo `bridge::post_json` em teste.
pub fn record_sent(v: Value) {
    lock(&SENT).push(v);
}

/// Chamado nos pontos de desvio (`"autostart:on"`, `"shell_open:<alvo>"`...).
pub fn record_effect(e: impl Into<String>) {
    lock(&EFFECTS).push(e.into());
}

/// Manda uma mensagem como a página (`{"type": ..., campos}`) e roda a fila
/// da main (o que o handler enfileirou).
pub fn send(msg: Value) {
    crate::bridge::dispatch(&msg.to_string());
    crate::mainthread::drain();
}

/// Igual a [`send`], com o texto cru (pra testar JSON inválido).
pub fn send_raw(raw: &str) {
    crate::bridge::dispatch(raw);
    crate::mainthread::drain();
}

pub fn sent() -> Vec<Value> {
    lock(&SENT).clone()
}

pub fn clear_sent() {
    lock(&SENT).clear();
}

pub fn sent_of(ty: &str) -> Vec<Value> {
    sent().into_iter().filter(|v| v["type"] == ty).collect()
}

pub fn last_of(ty: &str) -> Option<Value> {
    sent_of(ty).pop()
}

/// Os textos dos `error` mandados à UI.
pub fn errors() -> Vec<String> {
    sent_of("error").iter().map(|v| v["message"].as_str().unwrap_or("").to_owned()).collect()
}

pub fn effects() -> Vec<String> {
    lock(&EFFECTS).clone()
}

/// Espera (rodando a fila da main) até chegar uma mensagem do tipo — para os
/// handlers que respondem de uma worker.
pub fn wait_for(ty: &str, timeout_ms: u64) -> Option<Value> {
    let end = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        crate::mainthread::drain();
        if let Some(v) = last_of(ty) {
            return Some(v);
        }
        if Instant::now() >= end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Roda a fila da main por um tempo (pra provar que algo NÃO chegou).
pub fn settle(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        crate::mainthread::drain();
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// Mídia (frente 4): DLLs do FFmpeg, servidor do player e amostras
// ---------------------------------------------------------------------------

/// Aponta as DLLs do FFmpeg e sobe o servidor HTTP do player (127.0.0.1, porta
/// livre) — as URLs que o Rust devolve à página saem dele. `false` = máquina
/// sem o runtime do OBS (o teste pula).
pub fn media() -> bool {
    if !crate::media_tests::dlls() {
        eprintln!("FFmpeg indisponivel — teste pulado.");
        return false;
    }
    true
}

impl Session {
    /// Copia uma amostra de `tests/fixtures/media/` pra pasta de gravação.
    pub fn sample(&self, fixture: &str, as_name: &str) -> String {
        let dst = self.rec.join(as_name);
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::copy(crate::media_tests::fixture(fixture), &dst).unwrap();
        dst.to_string_lossy().into_owned()
    }
}


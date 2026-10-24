//! As mensagens da página pelo dispatch, com o motor de captura de verdade,
//! num executável de teste PRÓPRIO — nenhum código de teste vai no `NoOBS.exe`.
//!
//! Fora da suíte padrão (grava a tela, precisa de placa de vídeo e das DLLs
//! do FFmpeg em `exe\`):
//!   cargo test --release --features app-tests --test mensagens
//!   cargo test --release --features app-tests --test mensagens -- msg-buffer,msg-area
//!
//! Roda na thread principal, sem a janela do app: uma janela ESCONDIDA faz o
//! papel da principal (timers do Bridge, fila da main), sem o mutex de
//! instância (o NoOBS instalado pode ficar aberto) e com `LOCALAPPDATA` numa
//! pasta temporária. `--janela` é a janela de OUTRO processo pro cenário de
//! gravar janela (a lista de janelas pula o próprio processo).
//!
//! Cada cenário imprime `OK <nome> <detalhe>`, `FALHA <nome> <motivo>` ou
//! `PULADO <nome> <motivo>` (máquina sem webcam, saída de som muda...). Os
//! arquivos gravados são conferidos pelo `probe` e apagados no fim; fica só o
//! log, na pasta temporária (o caminho sai na linha `FIM`).
//!
//! Os módulos do `noobs` só são públicos com a feature `app-tests` (ver
//! `modulos!` em `src/lib.rs`).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use noobs::probe::{self, ProbeReport};
use noobs::{config, log, mainthread, win_audio_meter as wam, win_preview};

const HELPER_TITLE: &str = "NoOBS autoteste janela";

struct Report {
    ok: u32,
    fail: u32,
    skip: u32,
    lines: Vec<String>,
}

impl Report {
    fn line(&mut self, s: String) {
        println!("{s}");
        noobs::log!("Teste: {s}");
        self.lines.push(s);
    }
    fn ok(&mut self, name: &str, detail: impl AsRef<str>) {
        self.ok += 1;
        self.line(format!("OK {name} {}", detail.as_ref()));
    }
    fn fail(&mut self, name: &str, why: impl AsRef<str>) {
        self.fail += 1;
        self.line(format!("FALHA {name} {}", why.as_ref()));
    }
    fn skip(&mut self, name: &str, why: impl AsRef<str>) {
        self.skip += 1;
        self.line(format!("PULADO {name} {}", why.as_ref()));
    }
    /// `Ok(detalhe)` / `Err(motivo)` de um cenário.
    fn take(&mut self, name: &str, r: Result<String, String>) {
        match r {
            Ok(d) => self.ok(name, d),
            Err(e) if e.starts_with("PULADO:") => self.skip(name, e.trim_start_matches("PULADO:").trim()),
            Err(e) => self.fail(name, e),
        }
    }
}

fn skip<T>(why: &str) -> Result<T, String> {
    Err(format!("PULADO: {why}"))
}

fn check(cond: bool, what: impl Into<String>) -> Result<(), String> {
    if cond {
        Ok(())
    } else {
        Err(what.into())
    }
}

fn near(a: f64, b: f64, tol: f64, what: &str) -> Result<(), String> {
    check((a - b).abs() <= tol, format!("{what}: {a:.2} longe de {b:.2} (±{tol})"))
}

// ---------------------------------------------------------------------------
// Laço da main: mensagens do Windows + fila da main (eventos do motor)
// ---------------------------------------------------------------------------

fn pump_once() {
    use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    mainthread::drain();
    collect_sent();
    PROGRESS.store(now_ms(), std::sync::atomic::Ordering::SeqCst);
    check_heaps();
}

/// `TESTE_HEAP=1`: valida TODOS os heaps do processo a cada 200 ms enquanto
/// bombeia. Na 1ª corrupção: registra onde estava e captura as pilhas.
fn check_heaps() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    if std::env::var_os("TESTE_HEAP").is_none() {
        return;
    }
    let now = now_ms();
    if now.saturating_sub(LAST.load(Ordering::SeqCst)) < 200 {
        return;
    }
    LAST.store(now, Ordering::SeqCst);
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcessHeaps(n: u32, heaps: *mut isize) -> u32;
        fn HeapValidate(heap: isize, flags: u32, mem: *const std::ffi::c_void) -> i32;
    }
    let mut heaps = [0isize; 256];
    let n = unsafe { GetProcessHeaps(heaps.len() as u32, heaps.as_mut_ptr()) }.min(heaps.len() as u32) as usize;
    for (i, h) in heaps[..n].iter().enumerate() {
        if unsafe { HeapValidate(*h, 0, std::ptr::null()) } == 0 {
            let passo = PASSO.with(|p| p.borrow().clone());
            noobs::log!("Teste: HEAP CORROMPIDO (heap {i} de {n}, {h:#x}) durante \"{passo}\".");
            println!("FALHA heap corrompido (heap {i}) durante \"{passo}\"");
            dump_stacks("heap");
            std::process::exit(4);
        }
    }
}

thread_local! {
    /// Onde o teste está (pra o aviso de heap corrompido dizer em que passo foi).
    static PASSO: RefCell<String> = const { RefCell::new(String::new()) };
}

fn passo(s: &str) {
    noobs::log!("Teste: passo {s}");
    PASSO.with(|p| *p.borrow_mut() = s.to_owned());
}

// ---------------------------------------------------------------------------
// Vigia: travou ou caiu → pilha de todas as threads pelo cdb
// ---------------------------------------------------------------------------

static PROGRESS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

const CDB: &str = r"C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe";

/// Prende o `cdb` (sem invadir) neste processo e grava a pilha de todas as
/// threads em `<sandbox>/pilhas-<motivo>.txt`. Só existe neste executável de teste.
fn dump_stacks(motivo: &str) -> Option<PathBuf> {
    if !Path::new(CDB).exists() {
        println!("SEM-CDB {motivo}: {CDB} ausente");
        return None;
    }
    let out = sandbox().join(format!("pilhas-{motivo}.txt"));
    let sym = std::env::temp_dir().join("noobs-sym");
    let r = std::process::Command::new(CDB)
        .args(["-pv", "-p", &std::process::id().to_string(), "-y"])
        .arg(format!("srv*{}*https://msdl.microsoft.com/download/symbols", sym.display()))
        .args(["-c", ".reload; lm; ~*kn 60; q"])
        .stdout(std::fs::File::create(&out).ok()?)
        .status();
    println!("PILHAS {motivo} {} ({r:?})", out.display());
    Some(out)
}

/// Thread que olha o relógio do `pump_once`: 40 s sem bombear = main travada.
fn start_watchdog() {
    PROGRESS.store(now_ms(), std::sync::atomic::Ordering::SeqCst);
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(2));
        let last = PROGRESS.load(std::sync::atomic::Ordering::SeqCst);
        if now_ms().saturating_sub(last) > 40_000 {
            noobs::log!("Teste: main parada ha 40 s — capturando pilhas.");
            dump_stacks("travou");
            println!("FALHA travamento main parada ha 40 s");
            std::process::exit(3);
        }
    });
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetUnhandledExceptionFilter(f: Option<unsafe extern "system" fn(*const std::ffi::c_void) -> i32>) -> *const std::ffi::c_void;
    }
    unsafe extern "system" fn filtro(ep: *const std::ffi::c_void) -> i32 {
        // EXCEPTION_POINTERS → EXCEPTION_RECORD: código (0), endereço (16), nº de parâmetros (24), parâmetros (32).
        let rec = unsafe { *(ep as *const *const u8) };
        let (code, addr, info0, info1) = unsafe {
            (*(rec as *const u32), *(rec.add(16) as *const usize), *(rec.add(32) as *const usize), *(rec.add(40) as *const usize))
        };
        noobs::log!("Teste: excecao nao tratada {code:#x} em {addr:#x} (param {info0} {info1:#x}) — capturando pilhas.");
        println!("EXCECAO {code:#x} em {addr:#x} param {info0} {info1:#x} thread {}", std::process::id());
        dump_stacks("caiu");
        println!("FALHA queda excecao nao tratada");
        0 // EXCEPTION_CONTINUE_SEARCH: segue pro fim normal (processo morre)
    }
    unsafe {
        SetUnhandledExceptionFilter(Some(filtro));
    }
}

fn pump_for(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        pump_once();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pump_until(ms: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        pump_once();
        if done() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// Ambiente
// ---------------------------------------------------------------------------

fn sandbox() -> PathBuf {
    std::env::temp_dir().join(format!("noobs-mensagens-{}", std::process::id()))
}

fn rec_dir() -> PathBuf {
    sandbox().join("gravacoes")
}

fn probe_file(path: &str) -> Result<ProbeReport, String> {
    probe::probe(Path::new(path)).ok_or_else(|| format!("probe falhou em {path}"))
}

/// Tela sem movimento (monitor apagado, nada mexendo): o encoder gasta quase
/// nada em qualquer nível e o tamanho deixa de dizer algo sobre a qualidade.
fn tela_parada(baixo: u64, alto: u64) -> bool {
    alto <= baixo + baixo / 10
}

// ---------------------------------------------------------------------------
// Mensagens: o Bridge de verdade (`install_hooks` + `ready`, como o WebView
// faria); o que ele manda à página é lido de volta pelo `take_sent_for_tests`.
// ---------------------------------------------------------------------------

thread_local! {
    static SENT_ALL: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
    static MONITORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static MIC: RefCell<Option<String>> = const { RefCell::new(None) };
    static MIC_IDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static WEBCAMS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CAPS: RefCell<Value> = const { RefCell::new(Value::Null) };
    /// Os monitores do `init` como vieram (id, x, y, width, height).
    static MON_INFO: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
    /// A última lista de áudio que a página recebeu (o `clear_sent` não a apaga).
    static LAST_AUDIO: RefCell<Value> = const { RefCell::new(Value::Null) };
}

fn collect_sent() {
    let v = noobs::bridge::take_sent_for_tests();
    if let Some(a) = v.iter().rev().find(|m| m["type"] == "audio_sources_refreshed") {
        LAST_AUDIO.with(|l| *l.borrow_mut() = a.clone());
    }
    if !v.is_empty() {
        SENT_ALL.with(|s| s.borrow_mut().extend(v));
    }
}

/// Manda como a página mandaria e roda o que o handler enfileirou.
fn send(msg: Value) {
    noobs::bridge::dispatch(&msg.to_string());
    pump_once();
}

fn clear_sent() {
    collect_sent();
    SENT_ALL.with(|s| s.borrow_mut().clear());
}

fn find_sent(ty: &str, pred: impl Fn(&Value) -> bool) -> Option<Value> {
    SENT_ALL.with(|s| s.borrow().iter().rev().find(|v| v["type"] == ty && pred(v)).cloned())
}

fn wait_msg(ty: &str, ms: u64, pred: impl Fn(&Value) -> bool) -> Result<Value, String> {
    let mut got = None;
    pump_until(ms, || {
        got = find_sent(ty, &pred);
        got.is_some()
    });
    got.ok_or_else(|| format!("\"{ty}\" nao chegou em {} s", ms / 1000))
}

fn any(_: &Value) -> bool {
    true
}

fn errors_sent() -> Vec<String> {
    SENT_ALL.with(|s| s.borrow().iter().filter(|v| v["type"] == "error").map(|v| v["message"].as_str().unwrap_or("").to_owned()).collect())
}

fn no_errors() -> Result<(), String> {
    let e = errors_sent();
    check(e.is_empty(), format!("erro na UI: {e:?}"))
}

fn expect_error(key: &str) -> Result<(), String> {
    let want = noobs::lang::t(key);
    check(errors_sent().iter().any(|e| *e == want), format!("esperava o erro \"{want}\", veio {:?}", errors_sent()))
}

/// `record_start` (ou outra mensagem que comece a gravar) → espera, `record_stop`
/// → espera o card. Devolve o arquivo conferido pelo probe.
fn msg_record(start: Value, secs: f64) -> Result<(ProbeReport, String), String> {
    clear_sent();
    send(start);
    wait_msg("recording_state", 15_000, |v| v["active"] == true)?;
    msg_finish(secs)
}

fn msg_finish(secs: f64) -> Result<(ProbeReport, String), String> {
    passo("gravando");
    pump_for((secs * 1000.0) as u64);
    passo("record_stop");
    send(json!({ "type": "record_stop" }));
    wait_msg("recording_state", 5_000, |v| v["active"] == false)?;
    let added = wait_msg("recording_added", 40_000, any)?;
    passo("depois do recording_added");
    no_errors()?;
    let path = added["item"]["id"].as_str().ok_or("recording_added sem id")?.to_owned();
    check(Path::new(&path).starts_with(rec_dir()), format!("gravou fora da pasta: {path}"))?;
    Ok((probe_file(&path)?, path))
}

fn m_ready() -> Result<String, String> {
    clear_sent();
    send(json!({ "type": "ready" }));
    // Monitores e webcams chegam pelo `View` (o gravador dos testes os registra
    // com o nome do evento antigo).
    let mons_msg = wait_msg("monitors_refreshed", 10_000, any)?;
    let mons: Vec<String> = mons_msg["monitors"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    check(!mons.is_empty(), "arranque sem monitores")?;
    MONITORS.with(|m| *m.borrow_mut() = mons.clone());
    MON_INFO.with(|m| *m.borrow_mut() = mons_msg["monitors"].as_array().cloned().unwrap_or_default());
    let cams_msg = wait_msg("webcams_refreshed", 10_000, any)?;
    let cams: Vec<String> = cams_msg["webcams"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    WEBCAMS.with(|w| *w.borrow_mut() = cams);
    // O warmup (1,5 s, no timer da janela) sobe o motor e manda as caps.
    let caps = wait_msg("encoder_caps", 20_000, any)?;
    check(caps["h264Sw"] == true, "x264 ausente nas caps")?;
    check(caps["exportEncoders"].as_array().is_some_and(|a| !a.is_empty()), "caps sem encoders de exportacao")?;
    CAPS.with(|c| *c.borrow_mut() = caps.clone());
    check(noobs::engine::with_engine(|e| e.is_initialized()).unwrap_or(false), "warmup nao subiu o motor")?;
    // Microfones: chegam de uma worker (WASAPI pode demorar, #30).
    let audio = wait_msg("audio_sources_refreshed", 15_000, any)?;
    let mic = audio["mics"].as_array().and_then(|a| a.iter().find(|m| m["enabled"] == true)).and_then(|m| m["id"].as_str()).map(str::to_owned);
    MIC.with(|m| *m.borrow_mut() = mic.clone());
    let ids: Vec<String> = audio["mics"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    MIC_IDS.with(|m| *m.borrow_mut() = ids);
    no_errors()?;
    Ok(format!("{} monitor(es), mic = {}, vendor = {}", mons.len(), mic.is_some(), caps["vendor"]))
}

/// Todos os monitores pela gravação do app (dispatch → motor): lado a lado da
/// esquerda pra direita, canvas = soma das larguras × maior altura (com um
/// encoder que aceita 8192, sem redução), uma região por monitor na ordem
/// das posições. Depois, um monitor desmarcado sai do canvas.
fn m_dois_monitores() -> Result<String, String> {
    let mut mons = MON_INFO.with(|m| m.borrow().clone());
    if mons.len() < 2 {
        return skip("um monitor so");
    }
    mons.sort_by_key(|m| m["x"].as_i64().unwrap_or(0));
    let caps = CAPS.with(|c| c.borrow().clone());
    // Codec que aceita 8192 de largura (H.264 por hardware para em 4096, #7).
    let codec = if caps["av1Hw"] == true { "av1-hw" } else if caps["hevcHw"] == true { "hevc-hw" } else { "h264-sw" };
    let antes = config::get_config_str("codec", "auto");
    send(json!({ "type": "set_codec", "codec": codec }));
    let dim = |m: &Value, k: &str| m[k].as_i64().unwrap_or(0);
    let total_w: i64 = mons.iter().map(|m| dim(m, "width")).sum();
    let max_h: i64 = mons.iter().map(|m| dim(m, "height")).max().unwrap_or(0);
    let r = (|| {
        if total_w > 8192 {
            return skip(&format!("{total_w} px de largura somada: passa de 8192 em qualquer encoder"));
        }
        let (rep, _) = msg_record(json!({ "type": "record_start" }), 2.0)?;
        let v = rep.video_stream().ok_or("sem video")?;
        check(v.width as i64 == total_w & !1 && v.height as i64 == max_h & !1, format!("{}x{} (esperado {total_w}x{max_h})", v.width, v.height))?;
        let layout = noobs::engine::with_engine(|e| e.current_layout().clone()).ok_or("sem motor")?;
        let mon_regs: Vec<_> = layout.regions.iter().filter(|r| r.kind == "monitor").collect();
        check(mon_regs.len() == mons.len(), format!("{} regiao(oes) de monitor no layout", mon_regs.len()))?;
        let mut x = 0;
        for (m, reg) in mons.iter().zip(&mon_regs) {
            check(reg.x as i64 == x && reg.w as i64 == dim(m, "width"), format!("regiao {} em x={} w={} (esperado x={x} w={})", reg.name, reg.x, reg.w, dim(m, "width")))?;
            x += dim(m, "width");
        }
        let todos = format!("{codec}: {}x{} com {} regioes lado a lado", v.width, v.height, mon_regs.len());

        // Desmarcar o da direita: o canvas fica só com os outros.
        let tira = mons.last().cloned().unwrap_or_default();
        let id = tira["id"].as_str().unwrap_or("").to_owned();
        send(json!({ "type": "toggle_source", "id": id, "enabled": false }));
        let sem = msg_record(json!({ "type": "record_start" }), 1.5);
        send(json!({ "type": "toggle_source", "id": id, "enabled": true }));
        let (rep, _) = sem?;
        let v = rep.video_stream().ok_or("sem video")?;
        let resto_w: i64 = mons[..mons.len() - 1].iter().map(|m| dim(m, "width")).sum();
        check(v.width as i64 == resto_w & !1, format!("desmarcado: {}x{} (esperado {resto_w} de largura)", v.width, v.height))?;
        no_errors()?;
        Ok(format!("{todos}; sem o da direita: {}x{}", v.width, v.height))
    })();
    send(json!({ "type": "set_codec", "codec": antes }));
    r
}

/// Deixa só o 1º monitor: canvas menor, cenários mais rápidos.
fn m_ocultar() -> Result<String, String> {
    let mons = MONITORS.with(|m| m.borrow().clone());
    if mons.len() < 2 {
        return Ok("um monitor so — nada a ocultar".into());
    }
    clear_sent();
    for id in &mons[1..] {
        send(json!({ "type": "set_source_hidden", "id": id, "hidden": true }));
    }
    no_errors()?;
    Ok(format!("{} monitor(es) ocultado(s)", mons.len() - 1))
}

fn m_idioma() -> Result<String, String> {
    clear_sent();
    send(json!({ "type": "set_language", "language": "en" }));
    let l = wait_msg("language_changed", 3_000, any)?;
    check(l["language"] == "en", format!("idioma: {}", l["language"]))?;
    // A interface lê os textos do `lang` em vigor (o pacote não viaja mais).
    let en = noobs::lang::t("settings.title");
    clear_sent();
    send(json!({ "type": "set_language", "language": "pt-BR" }));
    let _ = wait_msg("language_changed", 3_000, any)?;
    let pt = noobs::lang::t("settings.title");
    check(!en.is_empty() && en != pt, format!("os textos nao mudaram de idioma: {en:?} / {pt:?}"))?;
    no_errors()?;
    Ok(format!("{en} → {pt}"))
}

/// WAV PCM 16 bits mono 48 kHz de um tom de 440 Hz, BAIXO (-30 dBFS), em loop.
fn tone_wav(secs: f32) -> Vec<u8> {
    let rate = 48_000u32;
    let n = (rate as f32 * secs) as u32;
    let mut w = Vec::with_capacity(44 + n as usize * 2);
    let data_len = n * 2;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin() * 0.0316 * i16::MAX as f32;
        w.extend_from_slice(&(s as i16).to_le_bytes());
    }
    w
}

/// O som do SISTEMA entra na gravação, já na 1ª (com a libobs ela saía muda —
/// pegadinha #48). Microfones desligados: a mistura fica só com a saída, onde
/// toca um tom baixo.
fn m_som_sistema() -> Result<String, String> {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_LOOP, SND_MEMORY};
    let mics = MIC_IDS.with(|m| m.borrow().clone());
    for id in &mics {
        send(json!({ "type": "toggle_source", "id": id, "enabled": false }));
    }
    let wav = tone_wav(1.0);
    let tocou = unsafe { PlaySoundW(windows::core::PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_LOOP) }.as_bool();
    pump_for(500);
    let saida = wam::read_peak_levels().iter().map(|l| l.peak).fold(0.0f32, f32::max);
    noobs::log!("Teste: PlaySound = {tocou}; maior nivel de saida = {saida}");
    if !tocou || saida <= 0.0 {
        unsafe {
            let _ = PlaySoundW(windows::core::PCWSTR::null(), None, SND_ASYNC);
        }
        for id in &mics {
            send(json!({ "type": "toggle_source", "id": id, "enabled": true }));
        }
        return skip("a saida de som do Windows esta muda (volume/mudo) — sem como medir o som do sistema");
    }
    // Duas gravações seguidas: o sintoma antigo era "a 1ª sai muda, a 2ª grava".
    let mut medidas = Vec::new();
    let mut erro = None;
    for n in 1..=2 {
        match msg_record(json!({ "type": "record_start" }), 2.5) {
            Ok((rep, path)) => medidas.push((n, track_levels_db(&path, rep.audio_track_count()))),
            Err(e) => {
                erro = Some(e);
                break;
            }
        }
    }
    unsafe {
        let _ = PlaySoundW(windows::core::PCWSTR::null(), None, SND_ASYNC);
    }
    for id in &mics {
        send(json!({ "type": "toggle_source", "id": id, "enabled": true }));
    }
    if let Some(e) = erro {
        return Err(e);
    }
    let txt: Vec<String> = medidas.iter().map(|(n, l)| format!("gravacao {n}: {}", l.iter().map(|d| format!("{d:.0}")).collect::<Vec<_>>().join("/"))).collect();
    noobs::log!("Teste: som do sistema por faixa (dBFS): {txt:?}");
    let mix = |i: usize| medidas.get(i).and_then(|(_, l)| l.first().copied()).unwrap_or(-200.0);
    let (som1, som2) = (mix(0) > -50.0, mix(1) > -50.0);
    if !som1 && !som2 {
        return skip(&format!("o tom tocou mas nenhuma gravacao o captou ({}) — captura do sistema muda nesta maquina, sem como medir", txt.join("; ")));
    }
    check(som1, format!("1a gravacao muda e a 2a com som — {}", txt.join("; ")))?;
    Ok(txt.join("; "))
}

/// Pico (dBFS) de cada faixa de áudio do arquivo, na ordem dos streams.
fn track_levels_db(path: &str, n: usize) -> Vec<f32> {
    let dir = sandbox().join("faixas");
    let _ = std::fs::create_dir_all(&dir);
    let outs: Vec<String> = (0..n).map(|i| dir.join(format!("f{i}.m4a")).to_string_lossy().into_owned()).collect();
    if !noobs::ffmpeg_ops::extract_audio_tracks(path, &outs, 0, None) {
        return Vec::new();
    }
    outs.iter()
        .map(|o| {
            // Os picos vêm na raiz quadrada (a escala da forma de onda da UI).
            let p = noobs::ffmpeg_ops::compute_audio_peaks(o, 50, None).unwrap_or_default().into_iter().fold(0.0f32, f32::max);
            40.0 * p.max(1e-9).log10()
        })
        .collect()
}

/// Webcam (Media Foundation) na gravação.
fn m_webcam() -> Result<String, String> {
    let Some(cam) = WEBCAMS.with(|w| w.borrow().first().cloned()) else { return skip("sem webcam") };
    send(json!({ "type": "toggle_source", "id": cam, "enabled": true }));
    let r = msg_record(json!({ "type": "record_start" }), 2.5);
    send(json!({ "type": "toggle_source", "id": cam, "enabled": false }));
    let (rep, _) = r?;
    let v = rep.video_stream().ok_or("sem video")?;
    Ok(format!("{:.1}s com webcam, canvas {}x{}", rep.duration, v.width, v.height))
}

fn m_gravar() -> Result<String, String> {
    let hw = CAPS.with(|c| c.borrow()["h264Hw"] == true);
    let codec = if hw { "h264-hw" } else { "h264-sw" };
    send(json!({ "type": "set_codec", "codec": codec }));
    send(json!({ "type": "set_codec", "codec": "mp3" }));
    check(config::get_config_str("codec", "") == codec, format!("codec no config: {}", config::get_config_str("codec", "")))?;
    send(json!({ "type": "set_recording_fps", "fps": 30 }));
    send(json!({ "type": "set_audio_bitrate", "kbps": 150 }));
    check(config::get_config_int("audioBitrate", 0) == 160, format!("150 kbps devia virar 160, ficou {}", config::get_config_int("audioBitrate", 0)))?;
    let (r, _) = msg_record(json!({ "type": "record_start" }), 3.0)?;
    let v = r.video_stream().ok_or("sem video")?;
    check(v.codec == "h264", format!("codec {}", v.codec))?;
    // Uma faixa por dispositivo marcado + a mistura, sem teto (a libobs parava
    // em 6 e agrupava o resto): o maior `track` da lista é o total.
    let audio = LAST_AUDIO.with(|l| l.borrow().clone());
    check(!audio.is_null(), "lista de audio nao veio")?;
    let faixas = ["mics", "speakers"]
        .iter()
        .flat_map(|k| audio[*k].as_array().cloned().unwrap_or_default())
        .filter_map(|d| d["track"].as_i64())
        .max()
        .unwrap_or(0)
        .max(1) as usize;
    check(r.audio_track_count() == faixas, format!("{} faixa(s) no arquivo, a lista diz {faixas}", r.audio_track_count()))?;
    near(v.frame_rate, 30.0, 1.0, "fps")?;
    near(r.duration, 3.0, 1.0, "duracao")?;
    Ok(format!("{codec} {}x{} {:.1}s {} faixa(s)", v.width, v.height, r.duration, r.audio_track_count()))
}

fn m_pausa() -> Result<String, String> {
    clear_sent();
    // Fora da gravação: ignorado.
    send(json!({ "type": "record_pause", "paused": true }));
    check(find_sent("recording_state", |v| v["paused"] == true).is_none(), "pausou sem gravacao")?;
    send(json!({ "type": "record_start" }));
    wait_msg("recording_state", 15_000, |v| v["active"] == true)?;
    pump_for(1500);
    clear_sent();
    send(json!({ "type": "record_pause", "paused": true }));
    let p = wait_msg("recording_state", 3_000, |v| v["paused"] == true)?;
    let parado = p["elapsedMs"].as_u64().unwrap_or(0);
    pump_for(2000);
    clear_sent();
    send(json!({ "type": "record_pause", "paused": false }));
    let r = wait_msg("recording_state", 3_000, |v| v["active"] == true && v.get("paused").is_none())?;
    let depois = r["elapsedMs"].as_u64().unwrap_or(0);
    // O relógio não andou durante os 2 s de pausa.
    check(depois < parado + 600, format!("relogio andou na pausa: {parado} → {depois} ms"))?;
    let (rep, _) = msg_finish(1.5)?;
    near(rep.duration, 3.0, 0.8, "duracao sem a pausa")?;
    Ok(format!("{:.1}s gravados (relogio parado em {:.1}s durante a pausa)", rep.duration, parado as f64 / 1000.0))
}

fn m_fontes() -> Result<String, String> {
    let mon = MONITORS.with(|m| m.borrow().first().cloned()).ok_or("sem monitor")?;
    clear_sent();
    send(json!({ "type": "record_start" }));
    wait_msg("recording_state", 15_000, |v| v["active"] == true)?;
    // Monitor muda o canvas: recusado gravando, e o config fica como estava.
    send(json!({ "type": "toggle_source", "id": mon, "enabled": false }));
    expect_error("error.cantChangeSourcesWhileRecording")?;
    send(json!({ "type": "set_source_hidden", "id": mon, "hidden": true }));
    check(errors_sent().len() == 2, format!("ocultar monitor gravando: {:?}", errors_sent()))?;
    let mic = MIC.with(|m| m.borrow().clone());
    let mut feito = "monitor recusado".to_owned();
    if let Some(mic) = mic {
        // Microfone: muda na hora (mudo na fonte viva), sem erro.
        clear_sent();
        send(json!({ "type": "toggle_source", "id": mic, "enabled": false }));
        wait_msg("audio_sources_refreshed", 3_000, |v| v["silent"] == true)?;
        pump_for(500);
        send(json!({ "type": "toggle_source", "id": mic, "enabled": true }));
        send(json!({ "type": "set_mute_when_device_muted", "enabled": false }));
        check(!config::get_config_bool("muteWhenDeviceMuted", true), "muteWhenDeviceMuted nao desligou")?;
        send(json!({ "type": "set_mute_when_device_muted", "enabled": true }));
        no_errors()?;
        feito.push_str(", mic mudo e de volta gravando");
    }
    let (r, _) = msg_finish(1.0)?;
    Ok(format!("{feito}; {:.1}s", r.duration))
}

fn m_janela(exe: &str) -> Result<String, String> {
    clear_sent();
    send(json!({ "type": "record_window", "id": "w999999", "appAudio": false }));
    expect_error("windowPicker.gone")?;
    let mut child = std::process::Command::new(exe).arg("--janela").spawn().map_err(|e| format!("helper: {e}"))?;
    let r = (|| {
        let mut item = None;
        for _ in 0..10 {
            clear_sent();
            send(json!({ "type": "list_capture_windows" }));
            let list = wait_msg("capture_windows", 5_000, any)?;
            item = list["items"].as_array().and_then(|a| a.iter().find(|w| w["title"] == HELPER_TITLE)).cloned();
            if item.is_some() {
                break;
            }
            pump_for(500);
        }
        let item = item.ok_or("a janela do helper nao apareceu na lista")?;
        let id = item["id"].as_str().ok_or("item sem id")?.to_owned();
        let thumb = wait_msg("capture_window_thumb", 10_000, |v| v["id"] == id.as_str())?;
        check(thumb["thumb"].as_str().is_some_and(|u| u.starts_with("http://127.0.0.1:")), "previa sem URL")?;
        clear_sent();
        passo("record_window");
        send(json!({ "type": "record_window", "id": id, "appAudio": false }));
        let st = wait_msg("recording_state", 15_000, |v| v["active"] == true)?;
        check(st["window"] == HELPER_TITLE, format!("recording_state.window = {}", st["window"]))?;
        let (rep, _) = msg_finish(2.0)?;
        let v = rep.video_stream().ok_or("sem video")?;
        let (cw, ch) = (item["w"].as_i64().unwrap_or(0) as i32, item["h"].as_i64().unwrap_or(0) as i32);
        check((v.width - cw / 16 * 16).abs() <= 16 && (v.height - ch / 16 * 16).abs() <= 16, format!("{}x{} (janela {cw}x{ch})", v.width, v.height))?;
        Ok(format!("cliente {cw}x{ch} → {}x{} {:.1}s", v.width, v.height, rep.duration))
    })();
    passo("matando a janela do helper");
    let _ = child.kill();
    let _ = child.wait();
    passo("helper morto");
    pump_for(1000);
    r
}

fn m_area() -> Result<String, String> {
    // Dois atalhos de área (sem combinação: só pelo `trigger_hotkey`): um fora
    // de todas as telas, outro no 1º monitor com 2 s de atraso.
    let mon = win_preview::monitors_from_win_preview().into_iter().next().ok_or("sem monitor")?;
    send(json!({ "type": "set_hotkeys", "items": [
        { "mode": "area", "hotkey": "", "area": { "x": -900000, "y": -900000, "w": 640, "h": 480 } },
        { "mode": "area", "hotkey": "", "area": { "x": mon.position_x + 100, "y": mon.position_y + 100, "w": 640, "h": 360 }, "delaySec": 2 },
    ] }));
    clear_sent();
    send(json!({ "type": "trigger_hotkey", "index": 0 }));
    expect_error("error.areaOffScreen")?;
    clear_sent();
    passo("trigger_hotkey (area)");
    send(json!({ "type": "trigger_hotkey", "index": 1 }));
    // A contagem anda no timer da janela: 2 → 1 → grava.
    wait_msg("record_countdown", 2_000, |v| v["sec"] == 2)?;
    wait_msg("record_countdown", 3_000, |v| v["sec"] == 1)?;
    let st = wait_msg("recording_state", 5_000, |v| v["active"] == true)?;
    check(st["window"].as_str().is_some_and(|w| w.contains("640")), format!("recording_state.window = {}", st["window"]))?;
    send(json!({ "type": "set_hotkeys", "items": [] }));
    let (r, _) = msg_finish(1.5)?;
    let v = r.video_stream().ok_or("sem video")?;
    check(v.width == 640 && v.height == 352, format!("{}x{} (esperado 640x352)", v.width, v.height))?;
    Ok(format!("contagem 2 s → 640x360 → {}x{}", v.width, v.height))
}

fn replay_active_now() -> bool {
    noobs::engine::with_engine(|e| e.is_replay_active()).unwrap_or(false)
}

fn m_buffer() -> Result<String, String> {
    clear_sent();
    send(json!({ "type": "save_replay" }));
    pump_for(200);
    check(find_sent("replay_saved", any).is_none(), "salvou sem buffer")?;
    send(json!({ "type": "set_replay_enabled", "enabled": true }));
    wait_msg("replay_state", 10_000, |v| v["active"] == true)?;
    check(replay_active_now(), "engine sem buffer ativo")?;
    pump_for(3500);
    clear_sent();
    send(json!({ "type": "save_replay" }));
    // O trecho fecha depois do keyframe da saída nova (timer da janela, #62).
    let saved = wait_msg("replay_saved", 30_000, any)?;
    let path = saved["id"].as_str().ok_or("replay_saved sem id")?.to_owned();
    check(Path::new(&path).starts_with(rec_dir()), format!("trecho fora da pasta: {path}"))?;
    let r = probe_file(&path)?;
    near(r.duration, 3.5, 1.5, "trecho salvo")?;
    wait_msg("recording_added", 5_000, |v| v["item"]["id"] == path.as_str())?;
    check(replay_active_now(), "buffer caiu depois de salvar")?;

    // Mudar o encoder recomeça o buffer — cada mensagem com o valor NOVO.
    let codec_atual = config::get_config_str("codec", "auto");
    let outro = if codec_atual == "h264-sw" { "auto" } else { "h264-sw" };
    let trocas = [
        json!({ "type": "set_codec", "codec": outro }),
        json!({ "type": "set_recording_quality", "level": 6 }),
        json!({ "type": "set_recording_fps", "fps": 60 }),
        json!({ "type": "set_audio_bitrate", "kbps": 256 }),
        json!({ "type": "set_codec", "codec": codec_atual }),
        json!({ "type": "set_recording_quality", "level": 5 }),
        json!({ "type": "set_recording_fps", "fps": 30 }),
    ];
    for m in trocas {
        clear_sent();
        send(m.clone());
        wait_msg("replay_restarted", 15_000, any).map_err(|e| format!("{}: {e}", m))?;
        check(replay_active_now(), format!("buffer nao voltou depois de {m}"))?;
    }
    // O mesmo valor de novo: nada recomeça.
    clear_sent();
    send(json!({ "type": "set_recording_fps", "fps": 30 }));
    pump_for(300);
    check(find_sent("replay_restarted", any).is_none(), "mesmo fps recomecou o buffer")?;

    // Gravar com o buffer ligado começa pelo que ele guardou (emenda ao parar).
    pump_for(3000);
    let (rec, _) = msg_record(json!({ "type": "record_start" }), 2.5)?;
    check(rec.duration > 4.0, format!("gravacao continuada so tem {:.1}s (sem o prefixo do buffer?)", rec.duration))?;
    // Ao parar, o buffer volta sozinho (ReplayWanted).
    check(pump_until(10_000, replay_active_now), "buffer nao religou depois da gravacao")?;
    clear_sent();
    send(json!({ "type": "set_replay_enabled", "enabled": false }));
    wait_msg("replay_state", 5_000, |v| v["active"] == false)?;
    no_errors()?;
    Ok(format!("trecho {:.1}s; 7 trocas recomecaram o buffer; gravacao continuada {:.1}s", r.duration, rec.duration))
}

fn m_buffer_apps() -> Result<String, String> {
    clear_sent();
    // explorer.exe está sempre aberto: a 1ª leitura do vigia já liga (#62).
    send(json!({ "type": "set_replay_apps", "apps": "explorer" }));
    wait_msg("replay_state", 10_000, |v| v["active"] == true)?;
    check(config::get_config_str("replayAutoApps", "") == "explorer", "replayAutoApps no config")?;
    clear_sent();
    send(json!({ "type": "set_replay_apps", "apps": "  " }));
    wait_msg("replay_state", 5_000, |v| v["active"] == false)?;
    check(!replay_active_now(), "buffer seguiu ligado com a lista vazia")?;
    no_errors()?;
    Ok("explorer liga, lista vazia desliga".into())
}

fn m_teste_video() -> Result<String, String> {
    clear_sent();
    send(json!({ "type": "test_video_quality" }));
    wait_msg("video_test", 5_000, |v| v["state"] == "recording")?;
    // Gravar durante a captura da amostra: recusado.
    send(json!({ "type": "record_start" }));
    expect_error("settings.videoTest.busyRecord")?;
    let done = wait_msg("video_test", 300_000, |v| v["state"] == "done" || v["state"] == "error")?;
    check(done["state"] == "done", format!("teste falhou: {}", done["error"]))?;
    let levels = done["levels"].as_array().cloned().unwrap_or_default();
    check(levels.len() == 11, format!("{} niveis", levels.len()))?;
    let bytes: Vec<u64> = levels.iter().map(|l| l["bytes"].as_u64().unwrap_or(0)).collect();
    check(bytes.iter().all(|&b| b > 1000), format!("nivel vazio: {bytes:?}"))?;
    if tela_parada(bytes[0], bytes[10]) {
        return skip(&format!("tela parada (monitor apagado?): {} KB nos niveis 0 e 10", bytes[0] / 1024));
    }
    check(bytes[10] > bytes[0], format!("nivel 10 ({}) devia ser maior que o 0 ({})", bytes[10], bytes[0]))?;
    check(levels.iter().all(|l| l["image"].as_str().is_some_and(|u| u.starts_with("http://127.0.0.1:"))), "nivel sem quadro PNG")?;

    // Cancelar no meio da captura.
    clear_sent();
    send(json!({ "type": "test_video_quality" }));
    wait_msg("video_test", 5_000, |v| v["state"] == "recording")?;
    pump_for(1000);
    send(json!({ "type": "cancel_video_test" }));
    wait_msg("video_test", 20_000, |v| v["state"] == "canceled")?;
    send(json!({ "type": "cancel_video_test" }));
    Ok(format!("{} {}x{}: nivel 0 = {} KB, nivel 10 = {} KB; cancelar ok", done["encoder"], done["width"], done["height"], bytes[0] / 1024, bytes[10] / 1024))
}

/// Janela escondida no lugar da principal: `SetTimer` do Bridge e o
/// `WM_APP_WAKE` da fila da main chegam nela.
fn create_headless_window() -> windows::Win32::Foundation::HWND {
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_TIMER {
            noobs::ui::dispatch_timer(wp.0);
            return LRESULT(0);
        }
        if msg == mainthread::WM_APP_WAKE {
            mainthread::drain();
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst, lpszClassName: w!("NoOBSTestePrincipal"), ..Default::default() };
        RegisterClassW(&wc);
        CreateWindowExW(WINDOW_EX_STYLE(0), w!("NoOBSTestePrincipal"), w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hinst), None)
            .expect("janela escondida do teste")
    }
}

/// `HKCU\Software\NoOBS\FirstRun` (o `ready` consome): lido antes, devolvido depois.
fn first_run_flag() -> Option<u32> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut size = 4u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, w!("Software\\NoOBS"), w!("FirstRun"), RRF_RT_REG_DWORD, None, Some(&mut value as *mut u32 as *mut _), Some(&mut size)) }
        .is_ok()
        .then_some(value)
}

fn restore_first_run_flag(v: u32) {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD};
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, w!("Software\\NoOBS"), w!("FirstRun"), REG_DWORD.0, Some(&v as *const u32 as *const _), 4);
    }
}

fn run_messages(filtro: &str) -> i32 {
    let root = prepare_sandbox();
    log::log("===== NoOBS: teste das mensagens =====");
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let first_run = first_run_flag();

    noobs::lang::init_language();
    config::set_config_str("language", "pt-BR");
    noobs::lang::init_language();
    config::set_config_str("recordDir", &rec_dir().to_string_lossy());
    // Nada que saia do processo de teste: atalhos globais, bandeja, som, janela
    // de aviso, indicadores, atualização, transcrição.
    for (k, v) in [
        ("transcribeOnStop", false),
        ("minimizeOnRecord", false),
        ("playSoundOnRecord", false),
        ("closeToTray", false),
        ("recIndicator", false),
        ("replayIndicator", false),
        ("checkUpdates", false),
        ("stopOnLock", false),
        ("autoRecordOnMic", false),
    ] {
        config::set_config_bool(k, v);
    }
    // Nenhum atalho global: o teste não pode tomar combinações da máquina.
    config::set_config_json("hotkeys", Some(json!([])));
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED).ok();
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    start_watchdog();
    let hwnd = create_headless_window();
    noobs::bridge::install_hooks();
    noobs::ui::attach_headless_window(hwnd);

    let mut rep = Report { ok: 0, fail: 0, skip: 0, lines: Vec::new() };
    // Vários filtros separados por vírgula (`msg-janela,msg-area`).
    let quer = |nome: &str| filtro.is_empty() || filtro.split(',').any(|f| nome.contains(f.trim()));
    let ready = m_ready();
    let subiu = ready.is_ok();
    rep.take("msg-ready", ready);
    if subiu {
        type Cenario = fn() -> Result<String, String>;
        let antes: [(&str, Cenario); 8] = [
            ("msg-dois-monitores", m_dois_monitores),
            ("msg-som-sistema", m_som_sistema),
            ("msg-ocultar", m_ocultar),
            ("msg-idioma", m_idioma),
            ("msg-gravar", m_gravar),
            ("msg-webcam", m_webcam),
            ("msg-pausa", m_pausa),
            ("msg-fontes", m_fontes),
        ];
        for (nome, f) in antes {
            // Ocultar os outros monitores é preparação: roda sempre.
            if nome == "msg-ocultar" || quer(nome) {
                rep.take(nome, f());
            }
        }
        if quer("msg-janela") {
            rep.take("msg-janela", m_janela(&exe));
        }
        // `TESTE_REPETIR=N`: janela + área N vezes mais (caça a travamento intermitente).
        let repetir: u32 = std::env::var("TESTE_REPETIR").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        for i in 1..=repetir {
            rep.take(&format!("msg-janela#{i}"), m_janela(&exe));
            rep.take(&format!("msg-area#{i}"), m_area());
        }
        let depois: [(&str, Cenario); 4] = [
            ("msg-area", m_area),
            ("msg-buffer", m_buffer),
            ("msg-buffer-apps", m_buffer_apps),
            ("msg-teste-video", m_teste_video),
        ];
        for (nome, f) in depois {
            if quer(nome) {
                rep.take(nome, f());
            }
        }
    }
    let t = Instant::now();
    noobs::bridge::shutdown();
    pump_once();
    let ms = t.elapsed().as_millis();
    if ms < 5000 {
        rep.ok("msg-shutdown", format!("{ms} ms"));
    } else {
        rep.fail("msg-shutdown", format!("{ms} ms"));
    }
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
        noobs::ui::release_thread_com();
        windows::Win32::System::Com::CoUninitialize();
    }
    if let Some(v) = first_run {
        if first_run_flag() != Some(v) {
            restore_first_run_flag(v);
        }
    }
    let _ = std::fs::remove_dir_all(rec_dir());
    let fim = format!("FIM ok={} falhas={} pulados={} log={}", rep.ok, rep.fail, rep.skip, root.join("NoOBS").join("logs").display());
    rep.line(fim);
    rep.fail as i32
}

// ---------------------------------------------------------------------------
// Entrada
// ---------------------------------------------------------------------------

/// `--janela`: uma janela colorida que muda de cor, pro cenário de
/// gravar janela (a lista de janelas pula o próprio processo).
fn run_helper_window() -> i32 {
    use windows::core::{w, HSTRING};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, HGDIOBJ, PAINTSTRUCT};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_TIMER => {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    LRESULT(0)
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let t = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() / 100) as u32;
                    let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF((t * 37 % 255) | ((t * 91 % 255) << 8) | (0x80 << 16)));
                    FillRect(dc, &ps.rcPaint, br);
                    let _ = DeleteObject(HGDIOBJ(br.0));
                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst, lpszClassName: w!("NoOBSSelftestJanela"), ..Default::default() };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("NoOBSSelftestJanela"),
            &HSTRING::from(HELPER_TITLE),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            60,
            60,
            660,
            520,
            None,
            None,
            Some(hinst),
            None,
        ) else {
            return 1;
        };
        SetTimer(Some(hwnd), 1, 100, None);
        SetTimer(Some(hwnd), 2, 60_000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.wParam.0 == 2 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

/// Pasta temporária deste processo, ANTES de qualquer leitura do config ou do
/// log (as de execuções antigas o lançador já apagou).
fn prepare_sandbox() -> PathBuf {
    let root = sandbox();
    let _ = std::fs::create_dir_all(rec_dir());
    std::env::set_var("LOCALAPPDATA", &root);
    std::env::set_var("APPDATA", &root);
    log::init_log();
    root
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--janela") {
        std::process::exit(run_helper_window());
    }
    let filtro = args.iter().find(|a| !a.starts_with('-')).cloned().unwrap_or_default();
    // DLLs do FFmpeg do runtime de desenvolvimento, sem copiar este executável.
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exe");
    let w: Vec<u16> = runtime.to_string_lossy().encode_utf16().chain([0]).collect();
    unsafe {
        let _ = windows::Win32::System::LibraryLoader::SetDllDirectoryW(windows::core::PCWSTR(w.as_ptr()));
    }
    if !noobs::ffi::ffmpeg::ffmpeg_lib_available() {
        println!("FFmpeg indisponivel em {} — teste pulado.", runtime.display());
        return;
    }
    // Pastas de execuções antigas (só o log sobra nelas).
    if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("noobs-mensagens-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    std::process::exit(run_messages(&filtro));
}

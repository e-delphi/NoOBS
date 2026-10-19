//! Frente 3 do `docs/mapa-ui-rust-dlls.md`: a libobs de verdade, num
//! executável de teste PRÓPRIO — nenhum código de teste vai no `NoOBS.exe`.
//!
//! Fora da suíte padrão (grava a tela, precisa de placa de vídeo):
//!   cargo test --release --features obs-tests --test obs
//!   cargo test --release --features obs-tests --test obs -- area
//!
//! A libobs acha dados, shaders e plugins pela pasta do EXE e exige a mesma
//! thread do começo ao fim (#3) e o RTWQ de pé (#48). Então este executável:
//! 1. rodado pelo cargo (em `target/`), se copia pra `exe/bin/64bit/`
//!    (ao lado da obs.dll) como `NoOBS-obstest.exe` e roda a cópia com
//!    `--dentro [filtro]`, repassando a saída;
//! 2. a cópia (`--dentro`) roda os cenários na thread principal, sem janela,
//!    sem o mutex de instância (o NoOBS instalado pode ficar aberto) e com
//!    `LOCALAPPDATA` numa pasta temporária;
//! 3. `--janela` é a janela de OUTRO processo pro cenário de gravar janela (a
//!    lista de janelas pula o próprio processo).
//!
//! Cada cenário imprime `OK <nome> <detalhe>`, `FALHA <nome> <motivo>` ou
//! `PULADO <nome> <motivo>` (máquina sem aquele encoder, sem microfone...).
//! Grava a TELA de verdade por alguns segundos em cada cenário; os arquivos são
//! conferidos pelo `probe` e apagados no fim. Fica só o log, na pasta
//! temporária (o caminho sai na linha `FIM`).
//!
//! Os módulos do `noobs` só são públicos com a feature `obs-tests` (ver
//! `modulos!` em `src/lib.rs`).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use noobs::engine::{self, AreaRect, CaptureInputs, RecordRequest, WindowCapture};
use noobs::probe::{self, ProbeReport};
use noobs::types::EncoderCaps;
use noobs::{audio_filters, config, encoder, log, mainthread, win_audio_meter as wam, win_preview, win_webcam, win_windows};

const HELPER_TITLE: &str = "NoOBS autoteste janela";

thread_local! {
    static STOPPED: RefCell<Option<String>> = const { RefCell::new(None) };
    static SAVED: RefCell<Option<String>> = const { RefCell::new(None) };
    static PREFIX: RefCell<Option<(String, i32)>> = const { RefCell::new(None) };
}

struct Report {
    ok: u32,
    fail: u32,
    skip: u32,
    lines: Vec<String>,
}

impl Report {
    fn line(&mut self, s: String) {
        println!("{s}");
        noobs::log!("Obstest: {s}");
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
// Laço da main: mensagens do Windows + fila da main (sinais da libobs)
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

/// `OBSTEST_HEAP=1`: valida TODOS os heaps do processo a cada 200 ms enquanto
/// bombeia. Na 1ª corrupção: registra onde estava e captura as pilhas.
fn check_heaps() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    if std::env::var_os("OBSTEST_HEAP").is_none() {
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
            noobs::log!("Obstest: HEAP CORROMPIDO (heap {i} de {n}, {h:#x}) durante \"{passo}\".");
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
    noobs::log!("Obstest: passo {s}");
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
        // `OBSTEST_SYMS`: pastas com os .pdb da obs.dll e dos plugins (release do OBS), separadas por `;`.
        .arg(format!("{};srv*{}*https://msdl.microsoft.com/download/symbols", std::env::var("OBSTEST_SYMS").unwrap_or_default(), sym.display()))
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
            noobs::log!("Obstest: main parada ha 40 s — capturando pilhas.");
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
        noobs::log!("Obstest: excecao nao tratada {code:#x} em {addr:#x} (param {info0} {info1:#x}) — capturando pilhas.");
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
    std::env::temp_dir().join(format!("noobs-obstest-{}", std::process::id()))
}

fn rec_dir() -> PathBuf {
    sandbox().join("gravacoes")
}

fn inputs() -> CaptureInputs {
    let devs = wam::enumerate_audio_devices();
    let default_of = |kind| devs.iter().find(|d| d.kind == kind && d.is_default).map(|d| d.device_id.clone()).unwrap_or_default();
    CaptureInputs {
        monitors: win_preview::monitors_from_win_preview(),
        webcams: win_webcam::enumerate_webcams(),
        default_mic_id: default_of(wam::DeviceKind::Input),
        default_spk_id: default_of(wam::DeviceKind::Output),
    }
}

fn probe_file(path: &str) -> Result<ProbeReport, String> {
    probe::probe(Path::new(path)).ok_or_else(|| format!("probe falhou em {path}"))
}

/// Grava `secs` segundos e espera o arquivo ficar completo (sinal "stop").
fn record(name: &str, secs: f64, tweak: impl FnOnce(&mut RecordRequest)) -> Result<(ProbeReport, String), String> {
    let path = rec_dir().join(format!("{name}.mkv")).to_string_lossy().into_owned();
    let mut req = RecordRequest { output_path: path.clone(), auto_profile: false, window: None, area: None };
    tweak(&mut req);
    STOPPED.with(|s| *s.borrow_mut() = None);
    let ins = inputs();
    engine::with_engine(|e| e.build_and_start_recording(&req, &ins)).ok_or("sem engine")??;
    let enc = engine::with_engine(|e| e.video_encoder_id().to_owned()).unwrap_or_default();
    pump_for((secs * 1000.0) as u64);
    engine::with_engine(|e| e.request_stop());
    if !pump_until(20_000, || STOPPED.with(|s| s.borrow().is_some())) {
        engine::with_engine(|e| e.force_complete_stop());
        return Err("o sinal \"stop\" nao chegou em 20 s".into());
    }
    Ok((probe_file(&path)?, enc))
}

// ---------------------------------------------------------------------------
// Cenários
// ---------------------------------------------------------------------------

fn s_init() -> Result<String, String> {
    engine::ensure_created();
    let t = Instant::now();
    engine::with_engine(|e| e.ensure_initialized()).ok_or("sem engine")??;
    let ms = t.elapsed().as_millis();
    check(engine::with_engine(|e| e.is_initialized()).unwrap_or(false), "nao inicializou")?;
    // Segunda chamada: no-op.
    engine::with_engine(|e| e.ensure_initialized()).ok_or("sem engine")??;
    let app = engine::with_engine(|e| e.app_audio_available()).unwrap_or(false);
    Ok(format!("{ms} ms; som por app = {app}"))
}

fn s_caps() -> Result<(String, EncoderCaps), String> {
    let c = encoder::detect_encoder_caps();
    check(c.h264_sw, "x264 nao registrado")?;
    Ok((format!("{c:?}"), c))
}

fn codec_family(enc_id: &str) -> &'static str {
    let id = enc_id.to_lowercase();
    if id.contains("av1") {
        "av1"
    } else if id.contains("hevc") || id.contains("h265") {
        "hevc"
    } else {
        "h264"
    }
}

fn s_codec(codec: &str, caps: &EncoderCaps) -> Result<String, String> {
    let disponivel = match codec {
        "h264-sw" => caps.h264_sw,
        "av1-sw" => caps.av1_sw,
        "h264-hw" => caps.h264_hw,
        "hevc-hw" => caps.hevc_hw,
        "av1-hw" => caps.av1_hw,
        _ => false,
    };
    if !disponivel {
        return skip("sem esse encoder nesta maquina");
    }
    config::set_config_str("codec", codec);
    let (r, enc) = record(&format!("codec-{codec}"), 3.0, |_| {})?;
    let v = r.video_stream().ok_or("sem video")?;
    let fam = codec_family(&enc);
    check(v.codec == fam, format!("arquivo em {} mas encoder {enc}", v.codec))?;
    let esperado = if codec.starts_with("av1") { "av1" } else if codec.starts_with("hevc") { "hevc" } else { "h264" };
    check(fam == esperado, format!("pediu {codec}, gravou com {enc}"))?;
    near(r.duration, 3.0, 1.0, "duracao")?;
    check(r.size > 0, "arquivo vazio")?;
    check(v.width % 2 == 0 && v.height % 2 == 0, "dimensao impar")?;
    Ok(format!("{enc} {}x{} {:.1}s {} faixa(s) de audio {} KB", v.width, v.height, r.duration, r.audio_track_count(), r.size / 1024))
}

/// Tela sem movimento (monitor apagado, nada mexendo): o encoder gasta quase
/// nada em qualquer nível e o tamanho deixa de dizer algo sobre a qualidade.
fn tela_parada(baixo: u64, alto: u64) -> bool {
    alto <= baixo + baixo / 10
}

fn s_qualidade() -> Result<String, String> {
    config::set_config_str("codec", "h264-sw");
    let mut sizes = Vec::new();
    for level in [0, 10] {
        config::set_config_int("recordingQualityLevel", level);
        let (r, _) = record(&format!("qualidade-{level}"), 2.5, |_| {})?;
        sizes.push(r.size);
    }
    config::set_config_int("recordingQualityLevel", 5);
    if tela_parada(sizes[0], sizes[1]) {
        return skip(&format!("tela parada (monitor apagado?): {} KB nos dois niveis", sizes[0] / 1024));
    }
    check(sizes[1] > sizes[0], format!("nivel 10 ({} KB) devia ser maior que o 0 ({} KB)", sizes[1] / 1024, sizes[0] / 1024))?;
    Ok(format!("nivel 0 = {} KB, nivel 10 = {} KB", sizes[0] / 1024, sizes[1] / 1024))
}

fn s_fps() -> Result<String, String> {
    config::set_config_str("codec", "h264-sw");
    let mut out = Vec::new();
    for fps in [15, 60] {
        config::set_config_int("recordingFps", fps);
        let (r, _) = record(&format!("fps-{fps}"), 2.0, |_| {})?;
        let got = r.video_stream().ok_or("sem video")?.frame_rate;
        near(got, fps as f64, 1.0, &format!("fps {fps}"))?;
        out.push(format!("{fps}→{got:.1}"));
    }
    config::set_config_int("recordingFps", 30);
    Ok(out.join(", "))
}

fn s_area() -> Result<String, String> {
    config::set_config_str("codec", "h264-sw");
    let mon = win_preview::monitors_from_win_preview().into_iter().next().ok_or("sem monitor")?;
    let a = AreaRect { x: mon.position_x + 100, y: mon.position_y + 100, w: 640, h: 360 };
    let (r, _) = record("area", 2.0, |req| req.area = Some(a))?;
    let v = r.video_stream().ok_or("sem video")?;
    // Canvas = área no início, em múltiplo de 16 (#66).
    check(v.width == 640 && v.height == 352, format!("{}x{} (esperado 640x352)", v.width, v.height))?;

    // Mover e redimensionar NO MEIO: o vídeo não muda de tamanho.
    let path = rec_dir().join("area-movida.mkv").to_string_lossy().into_owned();
    let req = RecordRequest { output_path: path.clone(), auto_profile: false, window: None, area: Some(a) };
    STOPPED.with(|s| *s.borrow_mut() = None);
    let ins = inputs();
    engine::with_engine(|e| e.build_and_start_recording(&req, &ins)).ok_or("sem engine")??;
    pump_for(800);
    let moved = engine::with_engine(|e| e.update_area(AreaRect { x: a.x + 200, y: a.y + 50, w: 800, h: 300 })).unwrap_or(false);
    pump_for(800);
    engine::with_engine(|e| e.request_stop());
    check(pump_until(20_000, || STOPPED.with(|s| s.borrow().is_some())), "stop nao chegou")?;
    check(moved, "update_area recusou")?;
    let r2 = probe_file(&path)?;
    let v2 = r2.video_stream().ok_or("sem video")?;
    check(v2.width == 640 && v2.height == 352, format!("mudou de tamanho: {}x{}", v2.width, v2.height))?;
    Ok(format!("640x360 → {}x{}; movida no meio sem mudar o tamanho", v.width, v.height))
}

fn s_janela(exe: &str) -> Result<String, String> {
    config::set_config_str("codec", "h264-sw");
    // A lista pula o próprio processo: uma cópia deste exe abre a janela.
    let mut child = std::process::Command::new(exe).arg("--janela").spawn().map_err(|e| format!("helper: {e}"))?;
    let mut found = None;
    pump_until(10_000, || {
        found = win_windows::enumerate_capture_windows().into_iter().find(|w| w.title == HELPER_TITLE);
        found.is_some()
    });
    let r = (|| {
        let w = found.ok_or("a janela do helper nao apareceu")?;
        let (cw, ch) = win_windows::capture_window_size(w.handle).ok_or("sem tamanho")?;
        let wc = WindowCapture { spec: w.spec.clone(), title: w.title.clone(), width: cw, height: ch, audio: false, app: w.app_name() };
        let (r, _) = record("janela", 2.0, |req| req.window = Some(wc))?;
        let v = r.video_stream().ok_or("sem video")?;
        let (ew, eh) = (cw / 16 * 16, ch / 16 * 16);
        check((v.width - ew).abs() <= 16 && (v.height - eh).abs() <= 16, format!("{}x{} (cliente {cw}x{ch})", v.width, v.height))?;
        Ok(format!("\"{}\" cliente {cw}x{ch} → {}x{}", w.exe, v.width, v.height))
    })();
    let _ = child.kill();
    let _ = child.wait();
    r
}

fn s_pausa() -> Result<String, String> {
    config::set_config_str("codec", "h264-sw");
    let path = rec_dir().join("pausa.mkv").to_string_lossy().into_owned();
    let req = RecordRequest { output_path: path.clone(), auto_profile: false, window: None, area: None };
    STOPPED.with(|s| *s.borrow_mut() = None);
    let ins = inputs();
    engine::with_engine(|e| e.build_and_start_recording(&req, &ins)).ok_or("sem engine")??;
    pump_for(1500);
    check(engine::with_engine(|e| e.set_recording_paused(true)).unwrap_or(false), "pausar recusou")?;
    pump_for(2500);
    check(engine::with_engine(|e| e.set_recording_paused(false)).unwrap_or(false), "retomar recusou")?;
    pump_for(1500);
    engine::with_engine(|e| e.request_stop());
    check(pump_until(20_000, || STOPPED.with(|s| s.borrow().is_some())), "stop nao chegou")?;
    let r = probe_file(&path)?;
    // 1,5 + 1,5 gravados; os 2,5 s de pausa não entram (nem como buraco).
    near(r.duration, 3.0, 0.8, "duracao sem a pausa")?;
    Ok(format!("{:.1}s gravados de ~5,5s de relogio", r.duration))
}

fn s_buffer() -> Result<String, String> {
    config::set_config_str("codec", "h264-sw");
    let dir = sandbox().join("buffer");
    let _ = std::fs::create_dir_all(&dir);
    SAVED.with(|s| *s.borrow_mut() = None);
    let ins = inputs();
    engine::with_engine(|e| e.build_and_start_replay(&dir.to_string_lossy(), 30, 512, &ins)).ok_or("sem engine")??;
    check(engine::with_engine(|e| e.is_replay_active()).unwrap_or(false), "buffer nao ligou")?;
    pump_for(4000);
    check(engine::with_engine(|e| e.save_replay()).unwrap_or(false), "salvar recusou")?;
    // O trecho fecha depois do keyframe da saída nova (#62).
    if let Some((true, delay)) = engine::with_engine(|e| (e.is_replay_save_commit_pending(), e.replay_save_commit_delay_ms())) {
        pump_for(delay);
        engine::with_engine(|e| e.commit_replay_save());
    }
    check(pump_until(20_000, || SAVED.with(|s| s.borrow().is_some())), "sinal \"saved\" nao chegou")?;
    let saved = SAVED.with(|s| s.borrow().clone()).unwrap_or_default();
    let r = probe_file(&saved)?;
    near(r.duration, 4.0, 1.5, "trecho salvo")?;
    check(engine::with_engine(|e| e.is_replay_active()).unwrap_or(false), "buffer caiu depois de salvar")?;
    Ok(format!("trecho de {:.1}s, buffer continua ligado", r.duration))
}

fn s_buffer_continua_na_gravacao() -> Result<String, String> {
    // Segue do cenário anterior: buffer ligado. Guarda mais um pouco e grava
    // pendurado nos MESMOS encoders; o que o buffer tinha sai como prefixo.
    check(engine::with_engine(|e| e.is_replay_active()).unwrap_or(false), "buffer desligado")?;
    pump_for(3000);
    PREFIX.with(|p| *p.borrow_mut() = None);
    STOPPED.with(|s| *s.borrow_mut() = None);
    let path = rec_dir().join("continuacao.mkv").to_string_lossy().into_owned();
    check(engine::with_engine(|e| e.start_recording_from_replay(&path)).unwrap_or(false), "start_recording_from_replay recusou")?;
    if let Some((true, delay)) = engine::with_engine(|e| (e.is_replay_save_commit_pending(), e.replay_save_commit_delay_ms())) {
        pump_for(delay);
        engine::with_engine(|e| e.commit_replay_save());
    }
    pump_for(2500);
    engine::with_engine(|e| e.request_stop());
    check(pump_until(30_000, || STOPPED.with(|s| s.borrow().is_some())), "stop nao chegou")?;
    let (prefix, lead) = PREFIX.with(|p| p.borrow().clone()).ok_or("o prefixo do buffer nao foi salvo")?;
    let rp = probe_file(&prefix)?;
    let rr = probe_file(&path)?;
    // Emenda como o Bridge faz ao parar.
    let out = rec_dir().join("emendada.mkv").to_string_lossy().into_owned();
    let info = noobs::ffmpeg_ops::splice_continuation(&prefix, &path, &out, lead).map_err(|e| format!("emenda: {e}"))?;
    let re = probe_file(&out)?;
    check(re.duration > rr.duration + 1.0, format!("emendada {:.1}s nao e maior que a gravacao {:.1}s", re.duration, rr.duration))?;
    check(re.duration <= rp.duration + rr.duration + 0.2, "emendada maior que a soma (pacote repetido)")?;
    engine::with_engine(|e| e.stop_replay());
    Ok(format!("prefixo {:.1}s + gravacao {:.1}s → {:.1}s ({info})", rp.duration, rr.duration, re.duration))
}

fn s_filtros() -> Result<String, String> {
    let lista = audio_filters::build_audio_filters_json();
    let ids: Vec<String> = lista.iter().filter_map(|f| f["id"].as_str().map(str::to_owned)).collect();
    if ids.is_empty() {
        return skip("plugin obs-filters ausente");
    }
    for want in ["gain_filter", "noise_gate_filter", "compressor_filter", "limiter_filter"] {
        check(ids.iter().any(|i| i == want), format!("{want} fora da lista {ids:?}"))?;
    }
    check(!ids.iter().any(|i| i == "noise_suppress_filter"), "v1 obsoleta da supressao listada")?;
    // Cada filtro descreve as propriedades num JSON que a UI monta sozinha
    // (o "inverter polaridade" não tem nenhuma — liga/desliga e só).
    for f in lista.iter().filter(|f| f["id"] != "invert_polarity_filter") {
        check(f["props"].as_array().is_some_and(|p| !p.is_empty()), format!("{} sem propriedades: {f}", f["id"]))?;
    }
    audio_filters::set_audio_filter_enabled("gain_filter", true);
    audio_filters::set_audio_filter_value("gain_filter", "db", &json!(6.0));
    check(audio_filters::enabled_audio_filter_count() == 1, "contagem de ligados")?;
    let cfg = config::get_config_json("audioFilters").ok_or("audioFilters nao foi pro config")?;
    check(cfg["gain_filter"]["enabled"] == true, format!("config: {cfg}"))?;
    // Valor numérico guardado como double (um inteiro faria o plugin ler 0, #63).
    check(cfg["gain_filter"]["settings"]["db"].as_f64() == Some(6.0), format!("db guardado: {}", cfg["gain_filter"]["settings"]))?;
    // Gravar com filtro pendurado nos microfones não pode quebrar.
    config::set_config_str("codec", "h264-sw");
    let (r, _) = record("com-filtro", 1.5, |_| {})?;
    audio_filters::reset_all_audio_filters();
    check(audio_filters::enabled_audio_filter_count() == 0, "reset nao desligou")?;
    Ok(format!("{} filtros; gravou {:.1}s com ganho ligado", ids.len(), r.duration))
}

fn s_filtro_teste_ab() -> Result<String, String> {
    let mic = wam::enumerate_audio_devices().into_iter().find(|d| d.kind == wam::DeviceKind::Input && d.is_default);
    let Some(mic) = mic else { return skip("sem microfone") };
    audio_filters::set_audio_filter_enabled("gain_filter", true);
    audio_filters::set_audio_filter_value("gain_filter", "db", &json!(6.0));
    audio_filters::start_audio_filter_test(&mic.device_id)?;
    pump_for(5500);
    let t = audio_filters::finish_audio_filter_test();
    check(t.raw_wav.len() > 44 && t.filt_wav.len() > 44, "WAV vazio")?;
    check(t.raw_wav.len() == t.filt_wav.len(), "as duas versoes com tamanhos diferentes")?;
    let r = if t.raw_rms_db > -90.0 {
        near(t.filt_rms_db - t.raw_rms_db, 6.0, 1.5, "ganho de +6 dB")?;
        format!("cru {:.1} dB → com ganho {:.1} dB", t.raw_rms_db, t.filt_rms_db)
    } else {
        "microfone em silencio (sem como medir o ganho)".to_owned()
    };
    // Mudou o filtro: refaz só a versão filtrada, do MESMO trecho.
    audio_filters::set_audio_filter_value("gain_filter", "db", &json!(12.0));
    let re = audio_filters::rerender_audio_filter_test();
    audio_filters::cancel_audio_filter_test();
    audio_filters::reset_all_audio_filters();
    check(re.is_some(), "rerender nao devolveu nada")?;
    Ok(r)
}

fn s_faixas() -> Result<String, String> {
    let mics = noobs::audio_tracks::enumerate_obs_audio_devices("wasapi_input_capture");
    let outs = noobs::audio_tracks::enumerate_obs_audio_devices("wasapi_output_capture");
    Ok(format!("{} entrada(s), {} saida(s) pelo libobs", mics.len(), outs.len()))
}

fn s_teardown() -> Result<String, String> {
    let t = Instant::now();
    engine::destroy();
    let ms = t.elapsed().as_millis();
    // O watchdog encerra o processo em 5 s (#31): chegar aqui já prova que não travou.
    check(ms < 5000, format!("{ms} ms"))?;
    Ok(format!("{ms} ms"))
}

// ---------------------------------------------------------------------------
// Frente 4: as mensagens da página que chegam na obs.dll, pelo `dispatch`
// ---------------------------------------------------------------------------
//
// Outro processo (`--mensagens`), com a libobs subindo do zero: o Bridge é o
// de verdade (`install_hooks` + `ready`, como o WebView faria), uma janela
// ESCONDIDA faz o papel da principal (timers e a fila da main) e o que o
// Bridge manda à página é lido de volta pelo `take_sent_for_tests`.

thread_local! {
    static SENT_ALL: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
    static MONITORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static MIC: RefCell<Option<String>> = const { RefCell::new(None) };
    static MIC_IDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static WEBCAMS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CAPS: RefCell<Value> = const { RefCell::new(Value::Null) };
}

fn collect_sent() {
    let v = noobs::bridge::take_sent_for_tests();
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
    let init = wait_msg("init", 10_000, any)?;
    let mons: Vec<String> = init["monitors"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    check(!mons.is_empty(), "init sem monitores")?;
    MONITORS.with(|m| *m.borrow_mut() = mons.clone());
    let cams: Vec<String> = init["webcams"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    WEBCAMS.with(|w| *w.borrow_mut() = cams);
    // O warmup (1,5 s, no timer da janela) sobe a libobs e manda as caps.
    let caps = wait_msg("encoder_caps", 20_000, any)?;
    check(caps["h264Sw"] == true, "x264 ausente nas caps")?;
    check(caps["exportEncoders"].as_array().is_some_and(|a| !a.is_empty()), "caps sem encoders de exportacao")?;
    CAPS.with(|c| *c.borrow_mut() = caps.clone());
    check(noobs::engine::with_engine(|e| e.is_initialized()).unwrap_or(false), "warmup nao subiu a libobs")?;
    // Microfones: chegam de uma worker (WASAPI pode demorar, #30).
    let audio = wait_msg("audio_sources_refreshed", 15_000, any)?;
    let mic = audio["mics"].as_array().and_then(|a| a.iter().find(|m| m["enabled"] == true)).and_then(|m| m["id"].as_str()).map(str::to_owned);
    MIC.with(|m| *m.borrow_mut() = mic.clone());
    let ids: Vec<String> = audio["mics"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    MIC_IDS.with(|m| *m.borrow_mut() = ids);
    no_errors()?;
    Ok(format!("{} monitor(es), mic = {}, vendor = {}", mons.len(), mic.is_some(), caps["vendor"]))
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

fn filter_names() -> Result<Vec<String>, String> {
    clear_sent();
    send(json!({ "type": "get_audio_filters" }));
    let f = wait_msg("audio_filters", 10_000, any)?;
    check(f["ready"] == true, "audio_filters sem ready")?;
    Ok(f["filters"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(str::to_owned)).collect()).unwrap_or_default())
}

fn motor_proprio() -> bool {
    noobs::engine::native_selected()
}

fn m_idioma() -> Result<String, String> {
    if motor_proprio() {
        return skip("filtros do OBS (rótulos traduzidos pelo plugin) não existem no motor próprio");
    }
    clear_sent();
    send(json!({ "type": "set_language", "language": "en" }));
    let l = wait_msg("language_changed", 3_000, any)?;
    check(l["language"] == "en", format!("idioma: {}", l["language"]))?;
    let en = filter_names()?;
    if en.is_empty() {
        return skip("plugin obs-filters ausente");
    }
    send(json!({ "type": "set_language", "language": "pt-BR" }));
    let pt = filter_names()?;
    // Os rótulos dos filtros vêm do plugin (obs_set_locale): têm que mudar junto.
    check(en != pt, format!("os nomes dos filtros nao mudaram de idioma: {en:?}"))?;
    no_errors()?;
    Ok(format!("{} → {}", en.first().cloned().unwrap_or_default(), pt.first().cloned().unwrap_or_default()))
}

fn m_filtros() -> Result<String, String> {
    if motor_proprio() {
        return skip("filtros de áudio saíram no motor próprio");
    }
    if filter_names()?.is_empty() {
        return skip("plugin obs-filters ausente");
    }
    clear_sent();
    send(json!({ "type": "set_audio_filter", "id": "gain_filter", "op": "enable", "value": true }));
    let f = wait_msg("audio_filter", 3_000, any)?;
    check(f["filter"]["id"] == "gain_filter" && f["filter"]["enabled"] == true, format!("audio_filter: {}", f["filter"]["enabled"]))?;
    send(json!({ "type": "set_audio_filter", "id": "gain_filter", "op": "value", "key": "db", "value": 6 }));
    let cfg = config::get_config_json("audioFilters").ok_or("audioFilters fora do config")?;
    // Inteiro da página vira double (#63).
    check(cfg["gain_filter"]["settings"]["db"].as_f64() == Some(6.0), format!("db: {}", cfg["gain_filter"]["settings"]))?;
    // Op desconhecida e id vazio: nada acontece.
    clear_sent();
    send(json!({ "type": "set_audio_filter", "id": "gain_filter", "op": "explodir" }));
    send(json!({ "type": "set_audio_filter", "id": "", "op": "enable", "value": true }));
    pump_for(200);
    check(find_sent("audio_filter", any).is_none(), "op invalida respondeu")?;

    let mic = MIC.with(|m| m.borrow().clone());
    let teste = match mic {
        None => "sem microfone — teste A/B pulado".to_owned(),
        Some(mic) => {
            let dev = mic.trim_start_matches("NoOBS Mic ").to_owned();
            let devs = wam::enumerate_audio_devices();
            let id = devs.iter().find(|d| d.kind == wam::DeviceKind::Input && (d.name == dev || d.device_id == dev)).map(|d| d.device_id.clone()).unwrap_or_default();
            clear_sent();
            send(json!({ "type": "test_audio_filters", "device": id }));
            wait_msg("audio_filter_test", 5_000, |v| v["state"] == "recording")?;
            let done = wait_msg("audio_filter_test", 15_000, |v| v["state"] == "done")?;
            check(done["filters"] == 1, format!("filtros no teste: {}", done["filters"]))?;
            for k in ["original", "filtered"] {
                check(done[k].as_str().is_some_and(|s| s.starts_with("data:audio/wav;base64,")), format!("{k} nao e WAV"))?;
            }
            // O microfone ENTREGOU amostras (o caminho RTWQ sem a plataforma de pé
            // não entrega nada — #48): ~5 s de WAV são centenas de KB, só o
            // cabeçalho daria ~60 caracteres.
            let len = done["original"].as_str().map(str::len).unwrap_or(0);
            check(len > 100_000, format!("o microfone nao entregou amostras (WAV com {len} caracteres)"))?;
            // Mudou o ganho: refaz só a filtrada, do mesmo trecho.
            clear_sent();
            send(json!({ "type": "set_audio_filter", "id": "gain_filter", "op": "value", "key": "db", "value": 12.0 }));
            let re = wait_msg("audio_filter_test", 5_000, |v| v["state"] == "filtered")?;
            format!("teste A/B: original {} dB, filtrada {} → {} dB", done["originalRmsDb"], done["filteredRmsDb"], re["filteredRmsDb"])
        }
    };
    clear_sent();
    send(json!({ "type": "reset_audio_filters" }));
    let f = wait_msg("audio_filters", 5_000, any)?;
    let ligados = f["filters"].as_array().map(|a| a.iter().filter(|x| x["enabled"] == true).count()).unwrap_or(99);
    check(ligados == 0, format!("{ligados} filtro(s) ligados depois do reset"))?;
    no_errors()?;
    Ok(teste)
}

/// A `RTWorkQ.dll` está no processo? (o `win-wasapi` escolhe o modelo de captura por isso)
fn rtwq_loaded() -> bool {
    unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(windows::core::w!("RTWorkQ.dll")) }.is_ok()
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

/// O som do SISTEMA entra na gravação (pegadinha #48: sem a plataforma RTWQ de
/// pé, o caminho RTWQ do plugin sai mudo na 1ª gravação). Microfones
/// desligados: a mistura fica só com a saída, onde toca um tom baixo.
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
    noobs::log!("Obstest: PlaySound = {tocou}; maior nivel de saida = {saida}");
    if !tocou || saida <= 0.0 {
        unsafe {
            let _ = PlaySoundW(windows::core::PCWSTR::null(), None, SND_ASYNC);
        }
        for id in &mics {
            send(json!({ "type": "toggle_source", "id": id, "enabled": true }));
        }
        return skip("a saida de som do Windows esta muda (volume/mudo) — sem como medir o som do sistema");
    }
    // Duas gravações seguidas: a #48 era "a 1ª sai muda, a 2ª grava".
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
    let rtwq = rtwq_loaded();
    let txt: Vec<String> = medidas.iter().map(|(n, l)| format!("gravacao {n}: {}", l.iter().map(|d| format!("{d:.0}")).collect::<Vec<_>>().join("/"))).collect();
    noobs::log!("Obstest: som do sistema por faixa (dBFS): {txt:?}; RTWorkQ = {rtwq}");
    let mix = |i: usize| medidas.get(i).and_then(|(_, l)| l.first().copied()).unwrap_or(-200.0);
    let (som1, som2) = (mix(0) > -50.0, mix(1) > -50.0);
    if !som1 && !som2 {
        return skip(&format!("o tom tocou mas nenhuma gravacao o captou ({}) — captura do sistema muda nesta maquina, sem como medir", txt.join("; ")));
    }
    // O sintoma da #48: a 1ª muda, a 2ª grava.
    check(som1, format!("1a gravacao muda e a 2a com som (pegadinha #48) — {} (RTWorkQ carregada = {rtwq})", txt.join("; ")))?;
    Ok(format!("{}; RTWorkQ carregada = {rtwq}", txt.join("; ")))
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

/// Webcam (DirectShow) na gravação. Registra se ela traz a RTWorkQ.dll pro
/// processo (via Media Foundation): é o "outro caminho" da #48, que faria o
/// win-wasapi usar o RTWQ (ver `rtwq::ensure_if_preloaded`).
fn m_webcam() -> Result<String, String> {
    let Some(cam) = WEBCAMS.with(|w| w.borrow().first().cloned()) else { return skip("sem webcam") };
    let antes = rtwq_loaded();
    send(json!({ "type": "toggle_source", "id": cam, "enabled": true }));
    let r = msg_record(json!({ "type": "record_start" }), 2.5);
    send(json!({ "type": "toggle_source", "id": cam, "enabled": false }));
    let (rep, _) = r?;
    let depois = rtwq_loaded();
    noobs::log!("Obstest: webcam — RTWorkQ.dll antes = {antes}, depois = {depois}");
    Ok(format!("{:.1}s com webcam; RTWorkQ.dll antes = {antes}, depois = {depois}", rep.duration))
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
    noobs::log!("Obstest: RTWorkQ.dll carregada antes da janela = {}", rtwq_loaded());
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
    config::set_config_json("areaSaved", None);
    clear_sent();
    send(json!({ "type": "record_saved_area" }));
    expect_error("error.areaNotSaved")?;
    let mon = win_preview::monitors_from_win_preview().into_iter().next().ok_or("sem monitor")?;
    config::set_config_json("areaSaved", Some(json!({ "x": mon.position_x + 100, "y": mon.position_y + 100, "w": 640, "h": 360 })));
    config::set_config_int("areaDelaySec", 2);
    clear_sent();
    passo("record_saved_area");
    send(json!({ "type": "record_saved_area" }));
    // A contagem anda no timer da janela: 2 → 1 → grava.
    wait_msg("area_countdown", 2_000, |v| v["sec"] == 2)?;
    wait_msg("area_countdown", 3_000, |v| v["sec"] == 1)?;
    let st = wait_msg("recording_state", 5_000, |v| v["active"] == true)?;
    check(st["window"].as_str().is_some_and(|w| w.contains("640")), format!("recording_state.window = {}", st["window"]))?;
    config::set_config_int("areaDelaySec", 0);
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
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst, lpszClassName: w!("NoOBSObstestPrincipal"), ..Default::default() };
        RegisterClassW(&wc);
        CreateWindowExW(WINDOW_EX_STYLE(0), w!("NoOBSObstestPrincipal"), w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hinst), None)
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
    log::log("===== NoOBS autoteste das mensagens (obs.dll) =====");
    let exe = enter_runtime_dir();
    let first_run = first_run_flag();

    noobs::lang::init_language();
    config::set_config_str("language", "pt-BR");
    noobs::lang::init_language();
    config::set_config_str("recordDir", &rec_dir().to_string_lossy());
    // Nada que saia do processo de teste: atalhos globais, bandeja, som, janela
    // de aviso, indicadores, atualização, transcrição, hibernação.
    for (k, v) in [
        ("transcribeOnStop", false),
        ("minimizeOnRecord", false),
        ("playSoundOnRecord", false),
        ("closeToTray", false),
        ("hibernate", false),
        ("recIndicator", false),
        ("replayIndicator", false),
        ("checkUpdates", false),
        ("stopOnLock", false),
        ("notifyOnRecord", false),
        ("autoRecordOnMic", false),
    ] {
        config::set_config_bool(k, v);
    }
    for k in ["hotkey", "replayHotkey", "areaHotkey"] {
        config::set_config_str(k, "");
    }
    // `OBSTEST_MOTOR=native`: as mesmas mensagens com o motor de captura próprio
    // (docs/motor-proprio.md, fase 6) no lugar da libobs.
    if std::env::var("OBSTEST_MOTOR").is_ok_and(|m| m.eq_ignore_ascii_case("native")) {
        config::set_config_str("captureEngine", "native");
        log::log("Obstest: motor de captura PROPRIO.");
        println!("MOTOR proprio (captureEngine=native)");
    }
    // Como o app: sem RTWQ no arranque (ver `rtwq`). `OBSTEST_RTWQ_PRE=1` carrega
    // a RTWorkQ.dll SEM o startup, como se outro módulo (mfplat) a tivesse
    // carregado — o caso que o `rtwq::ensure_if_preloaded` cobre (#48).
    // `OBSTEST_RTWQ_BOOT=1`: o comportamento ANTIGO (plataforma de pé desde o
    // arranque), pra comparar.
    if std::env::var_os("OBSTEST_RTWQ_BOOT").is_some() {
        noobs::rtwq::start_rtwq();
    }
    if std::env::var_os("OBSTEST_RTWQ_PRE").is_some() {
        let ok = unsafe { windows::Win32::System::LibraryLoader::LoadLibraryW(windows::core::w!("RTWorkQ.dll")) }.is_ok();
        noobs::log!("Obstest: RTWorkQ.dll pre-carregada sem startup = {ok}");
    }
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
            ("msg-som-sistema", m_som_sistema),
            ("msg-ocultar", m_ocultar),
            ("msg-idioma", m_idioma),
            ("msg-filtros", m_filtros),
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
        // `OBSTEST_REPETIR=N`: janela + área N vezes mais (caça a travamento intermitente).
        let repetir: u32 = std::env::var("OBSTEST_REPETIR").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
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
    noobs::rtwq::stop_rtwq();
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
    log::init_log(false);
    root
}

/// A libobs acha os shaders por `../../data/libobs/` relativo à pasta de
/// TRABALHO — o `ui::run` faz o mesmo antes de tudo. Devolve o caminho do exe.
fn enter_runtime_dir() -> String {
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(dir) = Path::new(&exe).parent() {
        let _ = std::env::set_current_dir(dir);
    }
    exe
}

/// Dentro do runtime: roda os cenários (todos, ou os que contêm o filtro).
fn run_inside(filtro: &str) -> i32 {
    let root = prepare_sandbox();
    log::log("===== NoOBS autoteste da libobs =====");
    let exe = enter_runtime_dir();

    noobs::lang::init_language();
    config::set_config_str("recordDir", &rec_dir().to_string_lossy());
    config::set_config_int("recordingFps", 30);
    config::set_config_bool("transcribeOnStop", false);
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED).ok();
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    wam::init_audio();
    wam::refresh_audio_devices();
    engine::set_mic_filter_hook(audio_filters::apply_audio_filters);
    engine::set_on_stopped(|p| STOPPED.with(|s| *s.borrow_mut() = Some(p.to_owned())));
    engine::set_on_replay_saved(|p| SAVED.with(|s| *s.borrow_mut() = Some(p.to_owned())));
    engine::set_on_replay_prefix_saved(|p, lead| PREFIX.with(|s| *s.borrow_mut() = Some((p.to_owned(), lead))));

    let mut rep = Report { ok: 0, fail: 0, skip: 0, lines: Vec::new() };
    let quer = |nome: &str| filtro.is_empty() || nome.contains(filtro);

    let init = s_init();
    let subiu = init.is_ok();
    rep.take("init", init);
    if subiu {
        let caps = match s_caps() {
            Ok((d, c)) => {
                rep.ok("caps", d);
                c
            }
            Err(e) => {
                rep.fail("caps", e);
                EncoderCaps::default()
            }
        };
        for codec in ["h264-sw", "av1-sw", "h264-hw", "hevc-hw", "av1-hw"] {
            let nome = format!("codec-{codec}");
            if quer(&nome) {
                rep.take(&nome, s_codec(codec, &caps));
            }
        }
        type Cenario = fn() -> Result<String, String>;
        let cenarios: [(&str, Cenario); 8] = [
            ("qualidade", s_qualidade),
            ("fps", s_fps),
            ("area", s_area),
            ("pausa", s_pausa),
            ("buffer", s_buffer),
            ("buffer-continua", s_buffer_continua_na_gravacao),
            ("filtros", s_filtros),
            ("filtro-ab", s_filtro_teste_ab),
        ];
        for (nome, f) in cenarios {
            if quer(nome) {
                rep.take(nome, f());
            }
        }
        if quer("janela") {
            rep.take("janela", s_janela(&exe));
        }
        if quer("faixas") {
            rep.take("faixas", s_faixas());
        }
        rep.take("teardown", s_teardown());
    }
    wam::done_audio();
    noobs::rtwq::stop_rtwq();
    let _ = std::fs::remove_dir_all(rec_dir());
    let _ = std::fs::remove_dir_all(root.join("buffer"));
    let fim = format!("FIM ok={} falhas={} pulados={} log={}", rep.ok, rep.fail, rep.skip, root.join("NoOBS").join("logs").display());
    rep.line(fim);
    rep.fail as i32
}


// ---------------------------------------------------------------------------
// Lançador
// ---------------------------------------------------------------------------

/// Roda a cópia no runtime com `modo` e repassa a saída; devolve as linhas de falha.
fn run_child(copia: &Path, runtime: &Path, modo: &str, filtro: &str) -> (bool, Vec<String>) {
    use std::io::BufRead;
    let mut child = std::process::Command::new(copia)
        .arg(modo)
        .arg(filtro)
        .current_dir(runtime)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("rodar o autoteste");
    let out = child.stdout.take().unwrap();
    let mut falhas = Vec::new();
    for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
        println!("{line}");
        if line.starts_with("FALHA") {
            falhas.push(line);
        }
    }
    let status = child.wait().expect("esperar o autoteste");
    if !status.success() && falhas.is_empty() {
        falhas.push(format!("FALHA {modo} saiu com {status}"));
    }
    (status.success(), falhas)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--janela") {
        std::process::exit(run_helper_window());
    }
    if let Some(i) = args.iter().position(|a| a == "--dentro") {
        let filtro = args.get(i + 1).cloned().unwrap_or_default();
        std::process::exit(run_inside(&filtro));
    }
    if let Some(i) = args.iter().position(|a| a == "--mensagens") {
        let filtro = args.get(i + 1).cloned().unwrap_or_default();
        std::process::exit(run_messages(&filtro));
    }

    // Rodado pelo cargo: copia este executável pro runtime e roda a cópia —
    // um processo pros cenários do engine e outro pras mensagens (`msg-*`),
    // cada um com a libobs subindo do zero.
    let filtro = args.iter().find(|a| !a.starts_with('-')).cloned().unwrap_or_default();
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exe").join("bin").join("64bit");
    if !runtime.join("obs.dll").exists() {
        println!("obs.dll ausente em {} — autoteste pulado.", runtime.display());
        return;
    }
    // Pastas de execuções antigas (só o log sobra nelas).
    if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("noobs-obstest-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let copia = runtime.join("NoOBS-obstest.exe");
    let eu = std::env::current_exe().expect("caminho do executavel de teste");
    std::fs::copy(&eu, &copia).expect("copiar o teste pro runtime");
    let mut falhas = Vec::new();
    let mut ok = true;
    if filtro.is_empty() || !filtro.starts_with("msg") {
        let (s, f) = run_child(&copia, &runtime, "--dentro", &filtro);
        ok &= s;
        falhas.extend(f);
    }
    if filtro.is_empty() || filtro.starts_with("msg") {
        let (s, f) = run_child(&copia, &runtime, "--mensagens", &filtro);
        ok &= s;
        falhas.extend(f);
    }
    let _ = std::fs::remove_file(&copia);
    if !ok || !falhas.is_empty() {
        eprintln!("
{} falha(s):", falhas.len());
        for f in &falhas {
            eprintln!("  {f}");
        }
        std::process::exit(1);
    }
}

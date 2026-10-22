//! Log em arquivo (porte de `OBSLog.pas`).
//!
//! `%LOCALAPPDATA%\NoOBS\logs\NoOBS_yyyy-mm-dd.log`, um por dia, em append,
//! retenção dos 3 mais recentes. Linha: `HH:MM:SS.mmm  texto`.

use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Quantos dias de log ficam.
const KEEP_DAYS: usize = 3;

struct Sink {
    file: Option<File>,
}

fn sink() -> &'static Mutex<Sink> {
    static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(Sink { file: None }))
}

fn log_dir() -> PathBuf {
    crate::config::config_dir().join("logs")
}

fn log_name() -> String {
    format!("NoOBS_{}.log", Local::now().format("%Y-%m-%d"))
}

pub fn log_file_path() -> PathBuf {
    log_dir().join(log_name())
}

/// Abre o log do dia e apaga o excedente. Chamar uma vez no início.
pub fn init_log() {
    let dir = log_dir();
    let _ = fs::create_dir_all(&dir);
    prune_old(&dir);
    let file = OpenOptions::new().create(true).append(true).open(log_file_path()).ok();
    let mut s = sink().lock().unwrap_or_else(|p| p.into_inner());
    s.file = file;
}

fn prune_old(dir: &PathBuf) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut names: Vec<String> = Vec::new();
    for n in rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()) {
        if !(n.starts_with("NoOBS_") && n.ends_with(".log")) {
            continue;
        }
        // `_hib.log`: do processo de hibernação de versões anteriores.
        if n.ends_with("_hib.log") {
            let _ = fs::remove_file(dir.join(&n));
            continue;
        }
        names.push(n);
    }
    names.sort(); // a data ISO no nome ordena cronologicamente
    if names.len() > KEEP_DAYS {
        for old in &names[..names.len() - KEEP_DAYS] {
            let _ = fs::remove_file(dir.join(old));
        }
    }
}

/// Escreve uma linha. Sem arquivo aberto (init ainda não rodou) é no-op:
/// falha de log nunca derruba o app.
pub fn log(msg: impl AsRef<str>) {
    let mut s = sink().lock().unwrap_or_else(|p| p.into_inner());
    if let Some(f) = s.file.as_mut() {
        let _ = writeln!(f, "{}  {}", Local::now().format("%H:%M:%S%.3f"), msg.as_ref());
    }
}

/// `log!("fmt {}", x)` — equivalente ao `Log(fmt, [args])` do Delphi.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::log(format!($($arg)*)) };
}

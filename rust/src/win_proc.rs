//! Roda programas de console sem janela (porte do `RunHidden` do `OBSTranscribe`).
//! Existe SÓ para o diagnóstico do ambiente (wsl/docker) e para o `audiocpp_cli` do
//! motor local — toda mídia continua in-process.

use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Código de saída (`-1` = não rodou ou estourou o tempo) e stdout+stderr.
///
/// Os dois pipes são lidos DURANTE a espera, em threads: se a saída encher o buffer do
/// pipe (4 KB) o filho trava escrevendo, e a espera acabaria em timeout com o
/// processo vivo.
pub fn run_hidden(exe: &str, args: &str, timeout_ms: u64) -> (i32, String) {
    // `raw_arg`: a linha de comando é montada à mão (`--status`, `info --format "{{...}}"`),
    // sem o escape que `arg` faria nas aspas.
    let child = Command::new(exe)
        .raw_arg(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else { return (-1, String::new()) };
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            buf
        })
    };
    let out_h = child.stdout.take().map(|s| drain(Box::new(s)));
    let err_h = child.stderr.take().map(|s| drain(Box::new(s)));
    let start = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) => {}
            Err(_) => break -1,
        }
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            break -1;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut raw = Vec::new();
    for h in [out_h, err_h].into_iter().flatten() {
        if let Ok(b) = h.join() {
            raw.extend(b);
        }
    }
    (code, String::from_utf8_lossy(&raw).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programa_inexistente_devolve_menos_um() {
        assert_eq!(run_hidden("C:\\nao\\existe.exe", "", 1000).0, -1);
    }
}

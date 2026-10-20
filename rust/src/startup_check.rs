//! Validação do runtime antes de criar a janela (porte de `OBSStartupCheck.pas`).
//!
//! O runtime é o NoOBS.exe e as DLLs do FFmpeg ao lado dele — a UI e as
//! traduções vão dentro do exe (`assets`), e o loader do WebView2 também (o
//! `webview2-com-sys` liga o `WebView2LoaderStatic.lib` no alvo MSVC). As
//! dependências das DLLs do FFmpeg entram na lista: sem elas o `LoadLibrary`
//! do avcodec/avformat falha do mesmo jeito, só que com um erro obscuro.

use std::path::{Path, PathBuf};

use crate::{log, log::log};

/// Cada DLL do runtime e pra que serve (o motivo vai pro log).
pub const RUNTIME_DLLS: [(&str, &str); 9] = [
    ("avcodec-61.dll", "encoders e decoders (gravação, exportação, miniaturas)"),
    ("avformat-61.dll", "leitura e escrita de MKV/MP4"),
    ("avutil-59.dll", "utilitários comuns do FFmpeg"),
    ("swscale-8.dll", "conversão de cor e escala"),
    ("swresample-5.dll", "dependência do avcodec"),
    ("libx264-164.dll", "encoder H.264 por CPU (dependência do avcodec)"),
    ("zlib.dll", "dependência do avcodec/avformat"),
    ("srt.dll", "dependência do avformat"),
    ("librist.dll", "dependência do avformat"),
];

#[derive(Debug, Clone)]
pub struct Missing {
    pub path: PathBuf,
    pub reason: &'static str,
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

pub fn validate_runtime() -> Vec<Missing> {
    let exe = exe_dir();
    RUNTIME_DLLS
        .iter()
        .map(|(name, reason)| Missing { path: exe.join(name), reason })
        .filter(|m| !m.path.is_file())
        .collect()
}

/// Roda a validação, registra tudo e, se faltar algo, mostra um `MessageBox`.
/// `false` = quem chamou deve sair sem criar a janela.
pub fn enforce_runtime() -> bool {
    let missing = validate_runtime();
    if missing.is_empty() {
        log("Startup: todos os arquivos esperados encontrados.");
        return true;
    }
    log!("Startup: {} arquivo(s) faltando:", missing.len());
    let mut text = String::from("NoOBS não pode iniciar — arquivos essenciais não foram encontrados na pasta do app:\n\n");
    for m in &missing {
        log!("   {} — {}", m.path.display(), m.reason);
        let name = m.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        text.push_str(&format!("  - {name}\n"));
    }
    text.push_str("\nReinstale o NoOBS. Detalhes em %LOCALAPPDATA%\\NoOBS\\logs\\.");
    crate::win::message_box("NoOBS - Arquivos ausentes", &text);
    false
}

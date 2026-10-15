//! Validação do runtime antes de criar a janela (porte de `OBSStartupCheck.pas`).
//!
//! O exe mora em `bin\64bit\`, ao lado da `obs.dll` (princípio fundamental do
//! projeto): plugins em `..\..\obs-plugins\64bit`, data em `..\..\data`.

use std::path::{Path, PathBuf};

use crate::{log, log::log};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Sem isso o app não funciona.
    Critical,
    /// A feature degrada.
    Recommended,
}

#[derive(Debug, Clone)]
pub struct Missing {
    pub path: PathBuf,
    pub kind: Kind,
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
    let mut missing = Vec::new();
    let mut check = |path: PathBuf, is_dir: bool, kind: Kind, reason: &'static str| {
        let ok = if is_dir { path.is_dir() } else { path.is_file() };
        if !ok {
            missing.push(Missing { path, kind, reason });
        }
    };

    // -- críticos
    check(exe.join("obs.dll"), false, Kind::Critical, "Core libobs — sem ele não dá pra gravar.");
    check(exe.join("WebView2Loader.dll"), false, Kind::Critical, "Loader do WebView2 — a UI não abre sem isso.");
    // FFmpeg: caminho único de toda operação de mídia (probe/remux/extract/thumb).
    check(exe.join("avcodec-61.dll"), false, Kind::Critical, "avcodec — decoders/encoders.");
    check(exe.join("avformat-61.dll"), false, Kind::Critical, "avformat — leitura/escrita de containers (MKV/MP4).");
    check(exe.join("avutil-59.dll"), false, Kind::Critical, "avutil — utilitários comuns do FFmpeg.");
    check(exe.join("swscale-8.dll"), false, Kind::Critical, "swscale — conversão de pixel format/escala (thumbs).");
    check(exe.join("../../data/libobs"), true, Kind::Critical, "Data do libobs (shaders/effects). Sem isso o vídeo não renderiza.");

    // -- recomendados
    check(exe.join("lang"), true, Kind::Recommended, "Pasta de traduções (lang\\). Sem ela a UI mostra chaves brutas.");
    check(exe.join("ui/index.html"), false, Kind::Recommended, "Interface do app (ui\\index.html). Sem ela a janela abre em branco.");

    let plugin_bin = exe.join("../../obs-plugins/64bit");
    check(plugin_bin.clone(), true, Kind::Critical, "Pasta dos plugins — sem ela nada de captura/encoder funciona.");
    check(exe.join("../../data/obs-plugins"), true, Kind::Recommended, "Data dos plugins — config padrão de cada plugin.");
    check(plugin_bin.join("win-wasapi.dll"), false, Kind::Recommended, "Captura de áudio (WASAPI). Sem isso a gravação fica muda.");
    check(plugin_bin.join("win-capture.dll"), false, Kind::Recommended, "Captura de monitor. Sem isso só grava áudio.");
    check(plugin_bin.join("win-dshow.dll"), false, Kind::Recommended, "Captura de webcam.");
    check(plugin_bin.join("obs-filters.dll"), false, Kind::Recommended, "Filtros de áudio do microfone (aba Áudio).");
    check(plugin_bin.join("obs-ffmpeg.dll"), false, Kind::Critical, "Muxer/encoder de áudio. Sem isso o arquivo final não escreve.");
    check(plugin_bin.join("obs-x264.dll"), false, Kind::Recommended, "Encoder de vídeo fallback (CPU).");
    missing
}

/// Roda a validação, registra tudo e, se faltar algo crítico, mostra um
/// `MessageBox`. `false` = quem chamou deve sair sem criar a janela.
pub fn enforce_runtime() -> bool {
    let missing = validate_runtime();
    if missing.is_empty() {
        log("Startup: todos os arquivos esperados encontrados.");
        return true;
    }
    log!("Startup: {} arquivo(s) faltando:", missing.len());
    for m in &missing {
        let lbl = if m.kind == Kind::Critical { "CRÍTICO" } else { "aviso" };
        log!("   [{lbl}] {} — {}", m.path.display(), m.reason);
    }
    let critical: Vec<&Missing> = missing.iter().filter(|m| m.kind == Kind::Critical).collect();
    if critical.is_empty() {
        return true;
    }

    let mut text = String::from(
        "NoOBS não pode iniciar — arquivos essenciais não foram encontrados na pasta do app:\n\n",
    );
    for m in &critical {
        let name = m.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        text.push_str(&format!("  - {name}\n"));
    }
    text.push_str(
        "\nConfirme que o app foi extraído completo (incluindo as pastas obs-plugins\\ e data\\). \
         Detalhes em %LOCALAPPDATA%\\NoOBS\\logs\\.",
    );
    crate::win::message_box("NoOBS - Arquivos ausentes", &text);
    false
}

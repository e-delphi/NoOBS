//! Frente 2 do `docs/mapa-ui-rust-dlls.md`: as funções de mídia (FFmpeg)
//! contra amostras pequenas versionadas (`tests/fixtures/media/`, ver
//! [`amostras`]). Precisam das DLLs do FFmpeg em `exe\`; sem elas,
//! cada teste avisa e sai sem falhar (máquina sem o runtime do OBS).
//!
//! As saídas vão pra `%TEMP%\noobs-test-<pid>\media\`.

mod amostras;
mod captions;
mod export;
mod gen;
mod ops;
mod probe;
mod util;

use std::path::PathBuf;
use std::sync::Once;

/// Aponta o diretório de DLL pro runtime do OBS (o exe de teste mora em
/// `target\`, longe dele) e diz se o FFmpeg carregou.
pub fn dlls() -> bool {
    static DIR: Once = Once::new();
    DIR.call_once(|| {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exe");
        let w = crate::win::wide(&dir.to_string_lossy());
        unsafe {
            let _ = windows::Win32::System::LibraryLoader::SetDllDirectoryW(windows::core::PCWSTR(w.as_ptr()));
        }
    });
    crate::ffi::ffmpeg::ffmpeg_lib_available()
}

/// Pula o teste (sem falhar) quando o FFmpeg não está disponível.
macro_rules! need_dlls {
    () => {
        if !super::dlls() {
            eprintln!(r"FFmpeg indisponivel (exe\) — teste pulado.");
            return;
        }
    };
}
pub(crate) use need_dlls;

/// Caminho de uma amostra versionada (pros testes de fora deste módulo).
pub fn fixture(name: &str) -> String {
    amostras::path(name)
}

/// Pasta de saída dos testes de mídia, uma por processo.
pub fn out(name: &str) -> String {
    let d = crate::testkit::sandbox_root().join("media");
    let _ = std::fs::create_dir_all(&d);
    d.join(name).to_string_lossy().into_owned()
}

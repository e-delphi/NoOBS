//! "Abrir como administrador" (`runAsAdmin`) — porte de `OBSElevate.pas`.
//!
//! Jogo rodando como administrador esconde o teclado de quem não é (UIPI): o
//! atalho de gravar não chega por nenhum método (pegadinha #33). Elevado, o
//! `RegisterHotKey` de sempre chega.
//!
//! O dispatcher do `main` chama [`relaunch_elevated_if_wanted`] ANTES de tudo
//! (mutex, janela, motor). UAC recusado = segue normal: a opção é um pedido,
//! nunca um bloqueio.
//!
//! O mesmo UIPI ao contrário: janela ELEVADA não recebe mensagens acima de
//! `WM_USER` de quem não é (clique na bandeja, `TaskbarCreated`, aviso de uma
//! 2ª instância). [`allow_from_lower_il`] libera essas.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{ChangeWindowMessageFilterEx, MSGFLT_ALLOW, SW_SHOWNORMAL};

use crate::{config, log};

/// O processo atual tem token de administrador?
pub fn is_process_elevated() -> bool {
    unsafe {
        let mut tok = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok).is_err() {
            return false;
        }
        let mut elev = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            tok,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(tok);
        ok && elev.TokenIsElevated != 0
    }
}

/// Argumentos originais (sem o exe), re-entre-aspados quando têm espaço.
fn original_args() -> String {
    std::env::args()
        .skip(1)
        .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Com `runAsAdmin` ligado e sem elevação, reabre elevado e devolve `true`
/// (quem chamou sai na hora). `false` = seguir normal.
pub fn relaunch_elevated_if_wanted() -> bool {
    if !config::get_config_bool("runAsAdmin", false) {
        return false;
    }
    if is_process_elevated() {
        log::log("Elevate: rodando como administrador.");
        return false;
    }
    let exe = HSTRING::from(std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
    let args_s = original_args();
    let args = HSTRING::from(args_s.as_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(args.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) }.is_ok() {
        log!("Elevate: reaberto como administrador (args=\"{args_s}\"); este processo sai.");
        true
    } else {
        // 1223 = ERROR_CANCELLED (o usuário disse não no UAC).
        log!("Elevate: nao foi possivel elevar (erro {}) — segue sem administrador.", unsafe { GetLastError().0 });
        false
    }
}

/// Deixa a janela receber `msgs` de processos sem elevação (no-op sem elevação).
pub fn allow_from_lower_il(hwnd: HWND, msgs: &[u32]) {
    if hwnd.0.is_null() || !is_process_elevated() {
        return;
    }
    for &m in msgs.iter().filter(|m| **m != 0) {
        unsafe {
            let _ = ChangeWindowMessageFilterEx(hwnd, m, MSGFLT_ALLOW, None);
        }
    }
}

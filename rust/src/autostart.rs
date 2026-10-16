//! Entrada do NoOBS no auto-start do Windows (porte de `OBSAutostart.pas`).
//!
//! Comando registrado em `HKCU\...\Run`: `"<exe>" /autostart`. O flag é só um
//! MARCADOR de origem ("fui lançado pelo logon"): bandeja ou janela visível é
//! decidido em runtime pelo `closeToTray`.
//!
//! Quem decide se a entrada existe é o INSTALADOR (pegadinha #46). O app só lê
//! (toggle da UI e menu da bandeja) e escreve quando o USUÁRIO troca — nunca
//! força no boot.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_QUERY_VALUE, KEY_SET_VALUE, REG_BINARY, REG_SZ,
};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

use crate::log;

const AUTORUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
/// Win10/11: o Gerenciador de Tarefas > Inicializar guarda aqui o
/// ligado/desligado em paralelo ao Run. Entrada no Run marcada como desligada
/// aqui NÃO roda no logon.
const APPROVED_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const REG_VALUE: PCWSTR = w!("NoOBS");
/// Layout de 12 bytes: DWORD de estado (2 = ligado, 3 = desligado) + FILETIME.
const APPROVED_ENABLED: u32 = 2;
const APPROVED_DISABLED: u32 = 3;

/// Chave do registro aberta; fecha sozinha.
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(path: PCWSTR, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<Key> {
    let mut k = HKEY::default();
    (unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path, None, access, &mut k) } == ERROR_SUCCESS).then_some(Key(k))
}

/// Presente no Run E não desligado pelo Gerenciador de Tarefas.
pub fn is_autostart_enabled() -> bool {
    let Some(run) = open(AUTORUN_KEY, KEY_QUERY_VALUE) else { return false };
    if unsafe { RegQueryValueExW(run.0, REG_VALUE, None, None, None, None) } != ERROR_SUCCESS {
        return false;
    }
    // Sem a chave/entrada, vale ligado: o Windows só cria quando o usuário desliga.
    let Some(approved) = open(APPROVED_KEY, KEY_QUERY_VALUE) else { return true };
    let mut data = [0u8; 12];
    let mut size = data.len() as u32;
    let rc = unsafe { RegQueryValueExW(approved.0, REG_VALUE, None, None, Some(data.as_mut_ptr()), Some(&mut size)) };
    if rc != ERROR_SUCCESS || size < 4 {
        return true;
    }
    // Qualquer coisa diferente de "ligado" (3, 0x83…) conta como desligado.
    u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == APPROVED_ENABLED
}

pub fn set_autostart(enable: bool) {
    if let Some(run) = open(AUTORUN_KEY, KEY_SET_VALUE) {
        unsafe {
            if enable {
                // Aspas no caminho: instalação com espaço.
                let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                let value: Vec<u16> = format!("\"{exe}\" /autostart").encode_utf16().chain([0]).collect();
                let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2);
                let _ = RegSetValueExW(run.0, REG_VALUE, None, REG_SZ, Some(bytes));
            } else {
                let _ = RegDeleteValueW(run.0, REG_VALUE);
            }
        }
    }
    // Usuário que desligou pelo Gerenciador de Tarefas e agora liga pelo NoOBS:
    // sem limpar a marca aqui, o Windows continuaria ignorando no logon.
    if let Some(approved) = open(APPROVED_KEY, KEY_SET_VALUE | KEY_QUERY_VALUE) {
        let mut data = [0u8; 12];
        let status = if enable { APPROVED_ENABLED } else { APPROVED_DISABLED };
        data[..4].copy_from_slice(&status.to_le_bytes());
        let ft = unsafe { GetSystemTimeAsFileTime() };
        data[4..8].copy_from_slice(&ft.dwLowDateTime.to_le_bytes());
        data[8..].copy_from_slice(&ft.dwHighDateTime.to_le_bytes());
        unsafe {
            let _ = RegSetValueExW(approved.0, REG_VALUE, None, REG_BINARY, Some(&data));
        }
    }
    log!("Autostart: {enable}");
}

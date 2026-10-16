//! Modo "hibernação" (porte de `OBSHibernate.pas`): o MESMO exe com
//! `/hibernate`, carregando só o mínimo — ícone na bandeja ("NoOBS
//! (hibernando)"), o atalho global de gravar e, se ligados, os watchers de
//! microfone e de apps. Sem libobs, WebView2, FFmpeg nem medidores: ~5 MB
//! contra ~150 MB do modo completo, zero CPU/GPU parado.
//!
//! Clique na bandeja, atalho, microfone em uso ou jogo aberto PROMOVEM a
//! hibernação: libera mutex/bandeja/atalho, lança o exe (com `/start-record`,
//! `/start-record-mic` ou `/start-replay`) e sai.
//!
//! Instância única: o MESMO mutex do modo completo (pegadinha #36). Uma 2ª
//! instância manda `WM_SHOW_INSTANCE`, que aqui vira "promover a completo".

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::single_instance::{MUTEX_NAME, SHOW_MSG_NAME};
use crate::{config, elevate, hotkey, lang, log, mic_watch, proc_watch, win};

const CLASS_NAME: &str = "TNoOBSHibernate";
const TOOLTIP: &str = "NoOBS (hibernando)";

const WM_TRAYICON: u32 = WM_USER + 100;
/// Postada pela thread do watcher de mic: promove a completo para gravar.
const WM_MIC_TRIGGER: u32 = WM_USER + 101;
/// Postada pela thread do watcher de apps: promove a completo para ligar o
/// buffer (quem joga deixa o NoOBS hibernando justamente pra não pesar).
const WM_REPLAY_TRIGGER: u32 = WM_USER + 102;
const ID_TRAY_RECORD: usize = 9001;
const ID_TRAY_OPEN: usize = 9002;
const ID_TRAY_QUIT: usize = 9003;
/// Mesmos ids do modo completo.
const HK_RECORD_TOGGLE: i32 = 100;
const HK_RECORD_TOGGLE_ALT: i32 = 101;

static MAIN: AtomicIsize = AtomicIsize::new(0);
static SHOW_MSG: AtomicU32 = AtomicU32::new(0);
static MUTEX: AtomicIsize = AtomicIsize::new(0);

fn main_hwnd() -> HWND {
    HWND(MAIN.load(Ordering::SeqCst) as *mut _)
}

fn tray_data() -> NOTIFYICONDATAW {
    NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: main_hwnd(), uID: 1, ..Default::default() }
}

fn install_tray() {
    let mut d = tray_data();
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    d.uCallbackMessage = WM_TRAYICON;
    d.hIcon = win::main_icon();
    for (dst, c) in d.szTip.iter_mut().zip(TOOLTIP.encode_utf16()) {
        *dst = c;
    }
    if unsafe { Shell_NotifyIconW(NIM_ADD, &d) }.as_bool() {
        log::log("Hibernate: tray icon instalado.");
    } else {
        log::log("Hibernate: Shell_NotifyIconW(NIM_ADD) FALHOU.");
    }
}

fn remove_tray() {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &tray_data());
    }
}

/// Lê `hotkey` do config e registra (mesma regra do modo completo).
fn register_record_hotkey() {
    let spec = config::get_config_str("hotkey", "Pause/Break");
    if spec.trim().is_empty() {
        log::log("Hibernate: hotkey desativada (config vazia).");
        return;
    }
    let Some(hk) = hotkey::parse_hotkey(&spec) else {
        log!("Hibernate: hotkey spec invalido \"{spec}\".");
        return;
    };
    if let Some(reason) = hotkey::reserved_reason(hk.modifiers, hk.vk) {
        log!("Hibernate: hotkey \"{spec}\" e reservada ({reason}).");
        return;
    }
    let hwnd = main_hwnd();
    unsafe {
        if RegisterHotKey(Some(hwnd), HK_RECORD_TOGGLE, HOT_KEY_MODIFIERS(hk.modifiers), hk.vk).is_ok() {
            log!("Hibernate: hotkey \"{spec}\" registrada.");
        } else {
            log!("Hibernate: RegisterHotKey falhou pra \"{spec}\" (outro app?).");
        }
        // Ctrl+Pause chega como VK_CANCEL.
        if hk.vk == hotkey::VK_PAUSE && hk.modifiers & hotkey::MOD_CTRL != 0 {
            let _ = RegisterHotKey(Some(hwnd), HK_RECORD_TOGGLE_ALT, HOT_KEY_MODIFIERS(hk.modifiers), hotkey::VK_CANCEL);
        }
    }
}

fn unregister_record_hotkey() {
    let hwnd = main_hwnd();
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HK_RECORD_TOGGLE);
        let _ = UnregisterHotKey(Some(hwnd), HK_RECORD_TOGGLE_ALT);
    }
}

fn release_mutex() {
    let m = MUTEX.swap(0, Ordering::SeqCst);
    if m != 0 {
        unsafe {
            let _ = CloseHandle(HANDLE(m as *mut _));
        }
        log::log("Hibernate: mutex liberado.");
    }
}

/// Libera tudo que o completo vai querer (watchers, atalho, bandeja e o mutex
/// — ANTES do spawn, senão ele veria `ERROR_ALREADY_EXISTS`), lança e sai.
fn spawn_full_and_exit(args: &str) {
    let exe = win::exe_path();
    log!("Hibernate: spawn full \"{exe}\" args=\"{args}\"");
    mic_watch::stop();
    proc_watch::stop();
    unregister_record_hotkey();
    remove_tray();
    release_mutex();
    let rc = unsafe { ShellExecuteW(None, PCWSTR::null(), &HSTRING::from(exe), &HSTRING::from(args), PCWSTR::null(), SW_SHOWNORMAL) };
    if rc.0 as usize <= 32 {
        log!("Hibernate: ShellExecuteW FALHOU (codigo={}).", rc.0 as usize);
    } else {
        log::log("Hibernate: ShellExecuteW OK.");
    }
    unsafe { PostQuitMessage(0) };
}

fn show_tray_menu() {
    let hwnd = main_hwnd();
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        for (id, key) in [(ID_TRAY_RECORD, "record.start"), (ID_TRAY_OPEN, "common.open")] {
            let t = win::wide(&lang::t(key));
            let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(t.as_ptr()));
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let t = win::wide(&lang::t("header.close"));
        let _ = AppendMenuW(menu, MF_STRING, ID_TRAY_QUIT, PCWSTR(t.as_ptr()));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let show = SHOW_MSG.load(Ordering::SeqCst);
    if show != 0 && msg == show {
        log::log("Hibernate: WM_SHOW_INSTANCE recebido — promovendo a full.");
        spawn_full_and_exit("");
        return LRESULT(0);
    }
    match msg {
        WM_MIC_TRIGGER => {
            log::log("Hibernate: mic em uso — promovendo a full pra gravar (/start-record-mic).");
            spawn_full_and_exit("/start-record-mic");
            LRESULT(0)
        }
        WM_REPLAY_TRIGGER => {
            log::log("Hibernate: app monitorado aberto — promovendo a full pra ligar o buffer (/start-replay).");
            spawn_full_and_exit("/start-replay");
            LRESULT(0)
        }
        WM_HOTKEY => {
            log!("Hibernate: WM_HOTKEY id={}", wparam.0);
            if matches!(wparam.0 as i32, HK_RECORD_TOGGLE | HK_RECORD_TOGGLE_ALT) {
                spawn_full_and_exit("/start-record");
            }
            LRESULT(0)
        }
        WM_TRAYICON => {
            match lparam.0 as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    log::log("Hibernate: tray clique esquerdo — abrindo full.");
                    spawn_full_and_exit("");
                }
                WM_RBUTTONUP => show_tray_menu(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            log!("Hibernate: WM_COMMAND id={id}");
            match id {
                ID_TRAY_RECORD => spawn_full_and_exit("/start-record"),
                ID_TRAY_OPEN => spawn_full_and_exit(""),
                ID_TRAY_QUIT => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            log::log("Hibernate: WM_DESTROY.");
            mic_watch::stop();
            proc_watch::stop();
            unregister_record_hotkey();
            remove_tray();
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Callbacks dos watchers: rodam na THREAD deles; `PostMessage` é seguro.
fn on_mic(in_use: bool) {
    if in_use {
        unsafe {
            let _ = PostMessageW(Some(main_hwnd()), WM_MIC_TRIGGER, WPARAM(0), LPARAM(0));
        }
    }
}

fn on_replay_app(running: bool) {
    if running {
        unsafe {
            let _ = PostMessageW(Some(main_hwnd()), WM_REPLAY_TRIGGER, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn run() {
    log::log("Hibernate: modo minimo iniciando.");
    unsafe {
        let show = RegisterWindowMessageW(&HSTRING::from(SHOW_MSG_NAME));
        SHOW_MSG.store(show, Ordering::SeqCst);
        log!("Hibernate: WM_SHOW_INSTANCE={show}");
        let mutex = CreateMutexW(None, false, &HSTRING::from(MUTEX_NAME));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log::log("Hibernate: outra instancia ja roda — saindo.");
            if let Ok(m) = mutex {
                let _ = CloseHandle(m);
            }
            return;
        }
        if let Ok(m) = mutex {
            MUTEX.store(m.0 as isize, Ordering::SeqCst);
        }
        log!("Hibernate: mutex \"{MUTEX_NAME}\" adquirido.");

        // Tema do menu ANTES da janela, para ela já herdar o modo certo.
        let theme = match config::get_config_str("theme", "system").to_lowercase().as_str() {
            "dark" => "dark",
            "light" => "light",
            _ => win::detect_system_theme(),
        };
        log!("Hibernate: aplicando tema \"{theme}\" no menu do tray.");
        win::apply_menu_theme(None, theme == "dark");

        let hinst: HINSTANCE = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        let class = HSTRING::from(CLASS_NAME);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: hinst,
            hIcon: win::main_icon(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            log::log("Hibernate: RegisterClass falhou.");
        }
        // WS_POPUP sem WS_VISIBLE: existe, mas não aparece (nem no Alt+Tab).
        let Ok(hwnd) = CreateWindowExW(WS_EX_TOOLWINDOW, &class, w!("NoOBS"), WS_POPUP, 0, 0, 0, 0, None, None, Some(hinst), None) else {
            log::log("Hibernate: CreateWindowEx FALHOU.");
            release_mutex();
            return;
        };
        MAIN.store(hwnd.0 as isize, Ordering::SeqCst);
        log!("Hibernate: janela criada (HWND={:?}).", hwnd.0);
        // Elevado: libera o clique na bandeja e o aviso de uma 2ª instância.
        elevate::allow_from_lower_il(hwnd, &[show, WM_TRAYICON, RegisterWindowMessageW(w!("TaskbarCreated"))]);
        win::apply_menu_theme(Some(hwnd), theme == "dark");

        install_tray();
        register_record_hotkey();

        // Auto-gravação em chamadas: vigia o mic mesmo hibernando (poll de 1 s).
        if config::get_config_bool("autoRecordOnMic", false) {
            mic_watch::start(
                &config::get_config_str("autoRecordMicApps", ""),
                &config::get_config_str("autoRecordMicExcept", ""),
                on_mic,
            );
        }
        // Buffer automático: no-op com a lista vazia.
        proc_watch::start(&config::get_config_str("replayAutoApps", ""), on_replay_app);

        log::log("Hibernate: entrando no GetMessage loop.");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        log::log("Hibernate: GetMessage loop saiu (WM_QUIT recebido).");
        release_mutex();
        log::log("Hibernate: Run encerrando.");
    }
}

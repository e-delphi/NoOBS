//! Host Win32 + WebView2 (porte de `OBSUI.pas`) — SEM Tauri/wry: janela
//! nativa criada à mão, `WebView2Loader` via `webview2-com`, mesma lógica do
//! Delphi.
//!
//! * a UI (`exe\bin\64bit\ui\`) é servida do DISCO por
//!   `SetVirtualHostNameToFolderMapping` num origin `https://noobs.app` real —
//!   não `NavigateToString` (origin opaco; a Notification API exige contexto
//!   seguro). Pegadinha #16;
//! * loop `GetMessage`/`DispatchMessage` puro; o trabalho enfileirado por
//!   workers é drenado em `WM_APP_WAKE` ([`crate::mainthread`], pegadinha #4);
//! * a mesma ordem de arranque/encerramento do Delphi: RTWQ → COM → janela →
//!   loop → `on_shutdown` (com o STA ainda vivo, pegadinha #31) →
//!   `CoUninitialize` → `StopRtwq` (pegadinha #48).

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, WebMessageReceivedEventHandler,
};
use windows::core::{w, Interface, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, UpdateWindow, HBRUSH};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::single_instance::{CLASS_NAME, MUTEX_NAME, SHOW_MSG_NAME};
use crate::{bridge, config, log, mainthread, rtwq, startup_check, win};

/// Virtual host da UI. `https://` dá contexto seguro (Notification API) e
/// faz as URLs relativas (`css/`, `js/`, logos) resolverem como num site.
const UI_HOST: &str = "noobs.app";
const WM_HOTKEY_DEFAULT_DARK: bool = true;
/// IDs de comando do menu da bandeja são >= 9000.
const TRAY_COMMAND_BASE: u16 = 9000;

// Cores do splash (BGR). Acompanham o tema salvo, p/ não piscar branco.
const SPLASH_BG_DARK: u32 = 0x000C0A0A; // #0a0a0c
const SPLASH_BG_LIGHT: u32 = 0x00F5F4F4; // #f4f4f5

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static WM_SHOW_INSTANCE: AtomicU32 = AtomicU32::new(0);

struct WebState {
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
}

thread_local! {
    // Tudo do WebView2 vive e morre na main thread (STA).
    static WEB: RefCell<Option<WebState>> = const { RefCell::new(None) };
}

/// Callbacks que o orquestrador (bridge) registra — os `OnUI*` do Delphi.
#[derive(Default)]
pub struct Hooks {
    pub on_timer: Option<fn(usize)>,
    pub on_hotkey: Option<fn(i32)>,
    pub on_display_change: Option<fn()>,
    pub on_device_change: Option<fn()>,
    pub on_should_hide_on_close: Option<fn() -> bool>,
    pub on_shutdown: Option<fn()>,
}

static HOOKS: std::sync::Mutex<Hooks> = std::sync::Mutex::new(Hooks {
    on_timer: None,
    on_hotkey: None,
    on_display_change: None,
    on_device_change: None,
    on_should_hide_on_close: None,
    on_shutdown: None,
});

pub fn set_hooks(h: Hooks) {
    *HOOKS.lock().unwrap_or_else(|p| p.into_inner()) = h;
}

fn hooks<R>(f: impl FnOnce(&Hooks) -> R) -> R {
    f(&HOOKS.lock().unwrap_or_else(|p| p.into_inner()))
}

pub fn main_window() -> HWND {
    HWND(MAIN_HWND.load(Ordering::SeqCst) as *mut _)
}

// ---------------------------------------------------------------------------
// API pública usada pelo bridge
// ---------------------------------------------------------------------------

/// `PostWebMessageAsJson` — SÓ na main thread (o bridge enfileira).
pub fn post_json_now(json: &str) {
    WEB.with(|w| {
        if let Some(ws) = w.borrow().as_ref() {
            unsafe {
                let _ = ws.webview.PostWebMessageAsJson(&HSTRING::from(json));
            }
        }
    });
}

/// Barra de título escura/clara (`DWMWA_USE_IMMERSIVE_DARK_MODE`).
pub fn set_dark_mode(enable: bool) {
    let hwnd = main_window();
    if hwnd.0.is_null() {
        return;
    }
    let value = BOOL::from(enable);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const BOOL as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );
    }
}

/// Abre URL no navegador padrão (o chamador já filtrou o esquema).
pub fn shell_open(url: &str) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(url), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// `RegisterHotKey` global → `WM_HOTKEY` com `wParam = id`. Pode falhar se
/// outro app já registrou a combinação: não é fatal (pegadinha #33).
pub fn register_global_hotkey(id: i32, modifiers: u32, vk: u32) -> bool {
    let hwnd = main_window();
    if hwnd.0.is_null() {
        return false;
    }
    let ok = unsafe { RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(modifiers), vk) }.is_ok();
    if ok {
        log!("OBSUI: hotkey registrado (id={id} mod=${modifiers:x} vk=${vk:x}).");
    } else {
        log!("OBSUI: falha ao registrar hotkey (id={id} mod=${modifiers:x} vk=${vk:x}) — já em uso?");
    }
    ok
}

pub fn unregister_global_hotkey(id: i32) {
    let hwnd = main_window();
    if !hwnd.0.is_null() {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
    }
}

/// `SetTimer` na janela principal (`WM_TIMER` → hook `on_timer`). Mesmo id
/// reaproveitado substitui o timer anterior — por isso os ids são únicos.
pub fn set_timer(id: usize, interval_ms: u32) {
    let hwnd = main_window();
    if !hwnd.0.is_null() {
        unsafe {
            SetTimer(Some(hwnd), id, interval_ms, None);
        }
    }
}

pub fn kill_timer(id: usize) {
    let hwnd = main_window();
    if !hwnd.0.is_null() {
        unsafe {
            let _ = KillTimer(Some(hwnd), id);
        }
    }
}

fn window_title() -> String {
    let t = config::get_config_str("windowTitle", "NoOBS");
    if t.trim().is_empty() { "NoOBS".into() } else { t }
}

// ---------------------------------------------------------------------------
// WebView2
// ---------------------------------------------------------------------------

fn ui_folder() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ui")))
        .unwrap_or_else(|| "ui".into())
}

/// Liga o `AudioVideoTracks` do Chromium (`HTMLMediaElement.audioTracks`),
/// que o player usa para desligar o áudio acima de 2× (pegadinha #61). Vai
/// pela variável de ambiente lida na criação do ambiente, preservando o que
/// já estiver lá (ex.: porta de depuração remota).
fn enable_webview_blink_features() {
    const VAR: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
    const ARG: &str = "--enable-blink-features=AudioVideoTracks";
    let cur = std::env::var(VAR).unwrap_or_default();
    if cur.contains("AudioVideoTracks") {
        return;
    }
    let new = if cur.is_empty() { ARG.to_owned() } else { format!("{cur} {ARG}") };
    std::env::set_var(VAR, &new);
    log!("WebView2: argumentos extras = \"{new}\"");
}

/// `%LOCALAPPDATA%\TNoOBS`, restrito ao usuário e ao SYSTEM por DACL
/// protegida (SDDL). Diferença do Delphi: usa o alias `OW` (Owner Rights) no
/// lugar do SID textual do usuário — o dono do diretório é quem o criou, o
/// que dispensa `OpenProcessToken`/`ConvertSidToStringSid`.
fn secure_user_data_folder() -> Option<std::path::PathBuf> {
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW, SDDL_REVISION_1,
        SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{
        GetSecurityDescriptorDacl, ACL, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR,
    };
    use windows::Win32::Foundation::{LocalFree, HLOCAL};

    let base = std::env::var_os("LOCALAPPDATA")?;
    let dir = std::path::PathBuf::from(base).join(CLASS_NAME);
    std::fs::create_dir_all(&dir).ok()?;

    unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .is_ok()
        {
            let mut present = BOOL::default();
            let mut defaulted = BOOL::default();
            let mut dacl: *mut ACL = std::ptr::null_mut();
            if GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted).is_ok() && present.as_bool() {
                let _ = SetNamedSecurityInfoW(
                    &HSTRING::from(dir.to_string_lossy().as_ref()),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    None,
                    None,
                    Some(dacl),
                    None,
                );
            }
            let _ = LocalFree(Some(HLOCAL(sd.0)));
        }
    }
    Some(dir)
}

fn resize_webview(hwnd: HWND) {
    WEB.with(|w| {
        if let Some(ws) = w.borrow().as_ref() {
            let mut rc = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rc);
                let _ = ws.controller.SetBounds(rc);
            }
        }
    });
}

/// Mapeia a pasta da UI e navega para `https://noobs.app/index.html`.
fn start_navigate(webview: &ICoreWebView2) {
    let folder = ui_folder();
    if !folder.is_dir() {
        log!("StartNavigate: pasta UI \"{}\" não existe — abortando.", folder.display());
        return;
    }
    log!("StartNavigate: UI folder=\"{}\".", folder.display());
    match webview.cast::<ICoreWebView2_3>() {
        Ok(wv3) => unsafe {
            let hr = wv3.SetVirtualHostNameToFolderMapping(
                &HSTRING::from(UI_HOST),
                &HSTRING::from(folder.to_string_lossy().as_ref()),
                COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW,
            );
            log!("StartNavigate: SetVirtualHostNameToFolderMapping UI HR={hr:?}");
        },
        Err(_) => log::log("UI: ICoreWebView2_3 indisponível — Notification API pode não funcionar."),
    }
    unsafe {
        let _ = webview.Navigate(&HSTRING::from(format!("https://{UI_HOST}/index.html")));
    }
}

fn on_controller_ready(hwnd: HWND, controller: ICoreWebView2Controller) -> windows::core::Result<()> {
    let webview = unsafe { controller.CoreWebView2()? };

    unsafe {
        let _ = controller.SetZoomFactor(1.0);
        // Sem coisas de navegador que não fazem sentido num app: DevTools,
        // menu de contexto padrão, teclas de aceleração (F5, Ctrl+F, Ctrl+P…).
        if let Ok(settings) = webview.Settings() {
            let _ = settings.SetAreDevToolsEnabled(false);
            let _ = settings.SetAreDefaultContextMenusEnabled(false);
            if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
                let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
        }

        let mut token = Default::default();

        // JS → Rust. "dark"/"light" são o canal de sincronia da barra de título.
        let h = WebMessageReceivedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                let mut p = PWSTR::null();
                if unsafe { args.TryGetWebMessageAsString(&mut p) }.is_ok() {
                    let s = take_pwstr(p);
                    match s.as_str() {
                        "dark" => set_dark_mode(true),
                        "light" => set_dark_mode(false),
                        _ => bridge::dispatch(&s),
                    }
                }
            }
            Ok(())
        }));
        webview.add_WebMessageReceived(&h, &mut token)?;

        // Só NOTIFICATIONS é permitida; o resto é negado.
        let h = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                let mut kind = COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION;
                unsafe {
                    let _ = args.PermissionKind(&mut kind);
                    let _ = args.SetState(if kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS {
                        COREWEBVIEW2_PERMISSION_STATE_ALLOW
                    } else {
                        COREWEBVIEW2_PERMISSION_STATE_DENY
                    });
                }
            }
            Ok(())
        }));
        webview.add_PermissionRequested(&h, &mut token)?;

        // Defesa em profundidade: só a UI local navega.
        let h = NavigationStartingEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                let mut p = PWSTR::null();
                if unsafe { args.Uri(&mut p) }.is_ok() {
                    let uri = take_pwstr(p);
                    if !(uri.starts_with(&format!("https://{UI_HOST}")) || uri.starts_with("about:")) {
                        let short: String = uri.chars().take(120).collect();
                        log!("NavigationStarting: cancelando navegação pra \"{short}\"");
                        unsafe {
                            let _ = args.SetCancel(true);
                        }
                    }
                }
            }
            Ok(())
        }));
        webview.add_NavigationStarting(&h, &mut token)?;

        // Janela nova bloqueada; http(s) vai pro navegador do sistema.
        let h = NewWindowRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                unsafe {
                    let _ = args.SetHandled(true);
                    let mut p = PWSTR::null();
                    if args.Uri(&mut p).is_ok() {
                        let uri = take_pwstr(p);
                        if uri.starts_with("http://") || uri.starts_with("https://") {
                            shell_open(&uri);
                        }
                    }
                }
            }
            Ok(())
        }));
        webview.add_NewWindowRequested(&h, &mut token)?;
    }

    WEB.with(|w| *w.borrow_mut() = Some(WebState { controller, webview: webview.clone() }));
    resize_webview(hwnd);
    start_navigate(&webview);

    // Reduz o working set depois do warmup do WebView.
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
    Ok(())
}

fn create_webview(hwnd: HWND) {
    enable_webview_blink_features();
    let user_data = secure_user_data_folder();

    let options: ICoreWebView2EnvironmentOptions = CoreWebView2EnvironmentOptions::default().into();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |err, env| {
        log!("TEnvironmentHandler: err={err:?} env={}", env.is_some());
        err?;
        let env = env.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let ctrl_handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |err, controller| {
            log!("TControllerHandler: err={err:?} controller={}", controller.is_some());
            err?;
            let controller = controller.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
            on_controller_ready(hwnd, controller)
        }));
        unsafe { env.CreateCoreWebView2Controller(hwnd, &ctrl_handler) }
    }));

    let udf = user_data
        .as_ref()
        .map(|p| HSTRING::from(p.to_string_lossy().as_ref()))
        .unwrap_or_default();
    let rc = unsafe {
        CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), PCWSTR(udf.as_ptr()), &options, &handler)
    };
    if let Err(e) = rc {
        log!("CreateCoreWebView2EnvironmentWithOptions falhou: {e}");
    }
}

// ---------------------------------------------------------------------------
// Janela
// ---------------------------------------------------------------------------

fn splash_bg() -> COLORREF {
    COLORREF(if config::get_config_str("theme", "dark").eq_ignore_ascii_case("light") {
        SPLASH_BG_LIGHT
    } else {
        SPLASH_BG_DARK
    })
}

fn restore_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW });
        let _ = SetForegroundWindow(hwnd);
    }
}

fn webview_ready() -> bool {
    WEB.with(|w| w.borrow().is_some())
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 2ª instância pedindo para trazer esta à frente (single-instance).
    let show = WM_SHOW_INSTANCE.load(Ordering::SeqCst);
    if show != 0 && msg == show {
        restore_window(hwnd);
        return LRESULT(0);
    }

    match msg {
        WM_CREATE => {
            MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            mainthread::set_wake_window(hwnd);
            let light = config::get_config_str("theme", "dark").eq_ignore_ascii_case("light");
            set_dark_mode(!light && WM_HOTKEY_DEFAULT_DARK);
            create_webview(hwnd);
            LRESULT(0)
        }
        mainthread::WM_APP_WAKE => {
            mainthread::drain();
            LRESULT(0)
        }
        // Enquanto o controller não existe o fundo é o brush da classe
        // (cor do tema): sem flash branco. Com ele, o WebView cobre tudo.
        WM_ERASEBKGND if !webview_ready() => LRESULT(1),
        WM_SIZE => {
            resize_webview(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            hooks(|h| h.on_timer.map(|f| f(wparam.0)));
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            hooks(|h| h.on_display_change.map(|f| f()));
            LRESULT(0)
        }
        // DBT_DEVNODES_CHANGED (0x0007): o evento mais abrangente de mudança
        // de hardware (USB/webcam), sem RegisterDeviceNotification.
        WM_DEVICECHANGE => {
            if wparam.0 == 0x0007 {
                hooks(|h| h.on_device_change.map(|f| f()));
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            hooks(|h| h.on_hotkey.map(|f| f(wparam.0 as i32)));
            LRESULT(0)
        }
        WM_COMMAND if (wparam.0 & 0xFFFF) as u16 >= TRAY_COMMAND_BASE => {
            // TODO(tray): OBSTray.HandleTrayCommand — módulo ainda não portado.
            LRESULT(0)
        }
        WM_CLOSE => {
            // Autostart ligado ou gravando → minimiza pra bandeja em vez de
            // fechar (decisão do bridge, via hook).
            if hooks(|h| h.on_should_hide_on_close.map(|f| f()).unwrap_or(false)) {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Entry point do modo full (`OBSUI.Run`).
pub fn run() {
    if let Ok(dir) = std::env::current_exe().map(|p| p.parent().map(|d| d.to_path_buf())) {
        if let Some(dir) = dir {
            let _ = std::env::set_current_dir(dir);
        }
    }
    let cmd = win::cmdline_lower();
    log!("OBSUI.Run: cmdline=\"{cmd}\"");
    let autostart = cmd.contains("/autostart") || cmd.contains("/tray");
    let close_to_tray = config::get_config_bool("closeToTray", true);
    let replay_auto_start = config::get_config_bool("replayAutoStart", false);

    // /autostart + closeToTray + hibernate: o Delphi respawna no modo
    // hibernate (leve, sem libobs/WebView). EXCEÇÃO: replayAutoStart precisa
    // do libobs vivo no boot (pegadinha #62) — a preferência é consultada
    // ANTES do desvio.
    if autostart && close_to_tray && config::get_config_bool("hibernate", true) && !replay_auto_start {
        log::log("OBSUI.Run: /autostart + closeToTray + hibernate — TODO: hibernate::run (não portado).");
    }
    let start_in_tray = autostart && close_to_tray;
    let start_minimized = autostart && !close_to_tray;
    let start_record = cmd.contains("/start-record");
    let start_replay = cmd.contains("/start-replay");
    let hidden_start = start_in_tray || start_minimized || start_record || start_replay;

    unsafe {
        // Single-instance: a 2ª instância acorda a 1ª e sai. Mesmos literais
        // do hibernate (pegadinha #36) — vêm de `single_instance`.
        let show_msg = RegisterWindowMessageW(&HSTRING::from(SHOW_MSG_NAME));
        WM_SHOW_INSTANCE.store(show_msg, Ordering::SeqCst);
        let mutex = windows::Win32::System::Threading::CreateMutexW(None, false, &HSTRING::from(MUTEX_NAME));
        let already = windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        if already {
            if !autostart {
                let existing = FindWindowW(&HSTRING::from(CLASS_NAME), PCWSTR::null())
                    .or_else(|_| FindWindowW(w!("TNoOBSHibernate"), PCWSTR::null()));
                if let Ok(existing) = existing {
                    let _ = PostMessageW(Some(existing), show_msg, WPARAM(0), LPARAM(0));
                }
            }
            return;
        }
        let _keep_mutex = mutex; // vive até o fim do processo

        if !startup_check::enforce_runtime() {
            return;
        }

        // RTWQ ANTES do CoInitialize e de qualquer fonte WASAPI (pegadinha #48).
        rtwq::start_rtwq();
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinst: HINSTANCE = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        let class_name = HSTRING::from(CLASS_NAME);
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(CreateSolidBrush(splash_bg()).0),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        // 1700×1200, limitado a 90% da work area; centralizada nela.
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let (ww, wh) = (work.right - work.left, work.bottom - work.top);
        let (win_w, win_h) = (1700.min(ww * 9 / 10), 1200.min(wh * 9 / 10));
        let (win_x, win_y) = (work.left + (ww - win_w) / 2, work.top + (wh - win_h) / 2);

        let (x, y, ex) = if hidden_start {
            // WebView2 não inicializa direito com o HWND pai nunca mostrado:
            // mostra off-screen como tool window (fora da taskbar/Alt+Tab) e
            // esconde depois que o controller sobe.
            (-32000, -32000, WS_EX_TOOLWINDOW)
        } else {
            (win_x, win_y, WINDOW_EX_STYLE(0))
        };
        let hwnd = CreateWindowExW(
            ex,
            &class_name,
            &HSTRING::from(window_title()),
            WS_OVERLAPPEDWINDOW,
            x,
            y,
            win_w,
            win_h,
            None,
            None,
            Some(hinst),
            None,
        );
        let Ok(hwnd) = hwnd else {
            log::log("OBSUI.Run: CreateWindowExW falhou.");
            return;
        };
        let _ = ShowWindow(hwnd, if hidden_start { SW_SHOWNOACTIVATE } else { SW_SHOW });
        let _ = UpdateWindow(hwnd);

        // Loop puro; o trabalho das workers volta por WM_APP_WAKE.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup AQUI, com o STA da main ainda vivo (pegadinha #31): o
        // obs_shutdown precisa rodar na thread que chamou obs_startup.
        if let Some(f) = hooks(|h| h.on_shutdown) {
            f();
        }
        CoUninitialize();
        // RTWQ por último, depois do obs_shutdown (pegadinha #48).
        rtwq::stop_rtwq();
    }
}

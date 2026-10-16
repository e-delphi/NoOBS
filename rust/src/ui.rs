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

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, WebMessageReceivedEventHandler,
};
use windows::core::{w, Interface, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, COLORREF, E_FAIL, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetMonitorInfoW,
    MonitorFromWindow, SelectObject, SetBkMode, SetTextColor, UpdateWindow, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_BOLD, HBRUSH, HGDIOBJ, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcess, GetCurrentThreadId, SetProcessWorkingSetSize};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, SetFocus, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{ITaskbarList3, ShellExecuteW, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::single_instance::{CLASS_NAME, MUTEX_NAME, SHOW_MSG_NAME};
use crate::{bridge, config, elevate, hibernate, log, mainthread, record_icon, rtwq, startup_check, tray, win};

/// Virtual host da UI. `https://` dá contexto seguro (Notification API) e
/// faz as URLs relativas (`css/`, `js/`, logos) resolverem como num site.
const UI_HOST: &str = "noobs.app";
const WM_HOTKEY_DEFAULT_DARK: bool = true;
/// IDs de comando do menu da bandeja são >= 9000.
const TRAY_COMMAND_BASE: u16 = 9000;

// Cores do splash (BGR). Acompanham o tema salvo, p/ não piscar branco.
const SPLASH_BG_DARK: u32 = 0x000C0A0A; // #0a0a0c
const SPLASH_TEXT_DARK: u32 = 0x00E7E4E4; // #e4e4e7
const SPLASH_BG_LIGHT: u32 = 0x00F5F4F4; // #f4f4f5
const SPLASH_TEXT_LIGHT: u32 = 0x001B1818; // #18181b

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static WM_SHOW_INSTANCE: AtomicU32 = AtomicU32::new(0);
/// Mutex de instância única: liberado ANTES de lançar a hibernação.
static MUTEX: AtomicIsize = AtomicIsize::new(0);
/// "Fechar" de verdade (menu da bandeja, hibernação): o `WM_CLOSE` não esconde.
static REAL_QUIT: AtomicBool = AtomicBool::new(false);
/// Flags de linha de comando vindos da hibernação (`OBSUI.StartRecordRequested`…).
static START_RECORD: AtomicBool = AtomicBool::new(false);
static START_RECORD_MIC: AtomicBool = AtomicBool::new(false);
static START_REPLAY: AtomicBool = AtomicBool::new(false);

/// Estado de janela que só a main thread toca.
#[derive(Default)]
struct WinState {
    /// Estava maximizada antes de ir pra bandeja: volta igual.
    was_maximized: bool,
    /// Início escondido: a janela nasce off-screen com `WS_EX_TOOLWINDOW` (o
    /// WebView2 não inicializa com o pai nunca mostrado). Guarda onde ela
    /// devia estar e o que fazer quando o WebView ficar pronto.
    pending_restore_bounds: RECT,
    pending_restore_position: bool,
    pending_hide_after_ready: bool,
    pending_minimize_after_ready: bool,
    /// Overlay de "gravando" no botão da taskbar.
    taskbar: Option<ITaskbarList3>,
    taskbar_tried: bool,
    dot_icon: Option<HICON>,
    recording_now: bool,
    /// Tela cheia da janela host (`ToggleFullscreen`).
    fs_active: bool,
    fs_style: isize,
    fs_ex_style: isize,
    fs_rect: RECT,
    fs_maximized: bool,
}

thread_local! {
    static WSTATE: RefCell<WinState> = RefCell::new(WinState::default());
    static WEB_READY: Cell<bool> = const { Cell::new(false) };
}

fn wstate<R>(f: impl FnOnce(&mut WinState) -> R) -> R {
    WSTATE.with(|s| f(&mut s.borrow_mut()))
}

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
    /// Janela foi pra bandeja/taskbar e voltou: o bridge arma e desarma o
    /// timer de hibernação por ócio.
    pub on_window_hidden: Option<fn()>,
    pub on_window_restored: Option<fn()>,
    pub on_shutdown: Option<fn()>,
}

static HOOKS: std::sync::Mutex<Hooks> = std::sync::Mutex::new(Hooks {
    on_timer: None,
    on_hotkey: None,
    on_display_change: None,
    on_device_change: None,
    on_should_hide_on_close: None,
    on_window_hidden: None,
    on_window_restored: None,
    on_shutdown: None,
});

pub fn set_hooks(h: Hooks) {
    *HOOKS.lock().unwrap_or_else(|p| p.into_inner()) = h;
}

/// Lê os hooks SOB a trava e devolve só o ponteiro: o handler roda FORA dela.
/// Chamar o handler dentro travaria (deadlock) assim que ele chamasse algo do
/// `ui` que também lê um hook — ex.: gravar pelo atalho esconde a janela, e o
/// `hide_to_tray` avisa o bridge pelo `on_window_hidden`. O `Mutex` não é reentrante.
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

/// Título configurável (`windowTitle`); vazio vira `NoOBS`. Usado na janela,
/// no splash e no tooltip da bandeja.
pub fn window_title() -> String {
    let t = config::get_config_str("windowTitle", "NoOBS");
    let t = t.trim();
    if t.is_empty() { "NoOBS".into() } else { t.to_owned() }
}

/// Aplica o título ao vivo (barra de título + tooltip da bandeja).
pub fn apply_window_title() {
    let title = window_title();
    let hwnd = main_window();
    if !hwnd.0.is_null() {
        unsafe {
            let _ = SetWindowTextW(hwnd, &HSTRING::from(title.as_str()));
        }
    }
    tray::update_tooltip(&title);
}

pub fn start_record_requested() -> bool {
    START_RECORD.load(Ordering::SeqCst)
}
/// O `/start-record` veio do watcher de microfone (gravação "automática").
pub fn start_record_requested_by_mic() -> bool {
    START_RECORD_MIC.load(Ordering::SeqCst)
}
pub fn start_replay_requested() -> bool {
    START_REPLAY.load(Ordering::SeqCst)
}

/// Janela visível e não minimizada.
pub fn is_window_shown() -> bool {
    let hwnd = main_window();
    !hwnd.0.is_null() && unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() }
}

/// Esconde a janela pra bandeja (instala o ícone se faltar). A UI é avisada
/// antes (`window_hidden`): o player fecha em vez de tocar "fantasma".
pub fn hide_to_tray() {
    let hwnd = main_window();
    if hwnd.0.is_null() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return;
    }
    wstate(|s| s.was_maximized = unsafe { IsZoomed(hwnd) }.as_bool());
    post_json_now(r#"{"type":"window_hidden"}"#);
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    tray::install(hwnd, &window_title());
    if let Some(f) = hooks(|h| h.on_window_hidden) {
        f();
    }
}

/// Minimiza pra TASKBAR (sem bandeja) — `minimizeOnRecord` com `closeToTray`
/// desligado.
pub fn minimize_to_taskbar() {
    let hwnd = main_window();
    if hwnd.0.is_null() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return;
    }
    wstate(|s| s.was_maximized = unsafe { IsZoomed(hwnd) }.as_bool());
    post_json_now(r#"{"type":"window_hidden"}"#);
    unsafe {
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
    }
    if let Some(f) = hooks(|h| h.on_window_hidden) {
        f();
    }
}

/// Ícone na bandeja SEM esconder a janela (ativar `closeToTray`).
pub fn ensure_tray_icon() {
    let hwnd = main_window();
    if !hwnd.0.is_null() {
        tray::install(hwnd, &window_title());
    }
}

pub fn remove_tray_icon() {
    tray::remove();
}

/// Traz a janela de volta da bandeja OU da taskbar, no mesmo estado
/// (maximizada ou não). Idempotente.
pub fn restore_from_tray() {
    let hwnd = main_window();
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        // 1ª abertura de um início escondido: tira o tool window e põe no centro.
        let pending = wstate(|s| std::mem::take(&mut s.pending_restore_position).then_some(s.pending_restore_bounds));
        if let Some(b) = pending {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_TOOLWINDOW.0 as isize));
            let _ = SetWindowPos(hwnd, None, b.left, b.top, b.right - b.left, b.bottom - b.top,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_HIDEWINDOW);
        }
        let _ = ShowWindow(hwnd, if wstate(|s| s.was_maximized) { SW_SHOWMAXIMIZED } else { SW_RESTORE });

        // Anti-roubo de foco: clicando numa notificação, o processo em primeiro
        // plano é o shell e o Windows recusa o SetForegroundWindow (a janela
        // aparece atrás, com o botão piscando). Compartilhar o input com a
        // thread em primeiro plano por um instante libera.
        let fg = GetForegroundWindow();
        let me = GetCurrentThreadId();
        let mut attached = None;
        if !fg.0.is_null() && fg != hwnd {
            let fg_thread = GetWindowThreadProcessId(fg, None);
            if fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool() {
                attached = Some(fg_thread);
            }
        }
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if let Some(t) = attached {
            let _ = AttachThreadInput(me, t, false);
        }
    }
    // Com closeToTray o ícone fica (o [X] volta pra bandeja); sem, sai.
    if !config::get_config_bool("closeToTray", true) {
        tray::remove();
    }
    // O botão da taskbar é recriado ao reaparecer e o overlay se perde.
    let (rec, dot) = wstate(|s| (s.recording_now, s.dot_icon));
    if let (true, Some(dot)) = (rec, dot) {
        if let Some(tb) = taskbar() {
            unsafe {
                let _ = tb.SetOverlayIcon(hwnd, dot, PCWSTR::null());
            }
        }
    }
    if let Some(f) = hooks(|h| h.on_window_restored) {
        f();
    }
}

/// `ITaskbarList3` lazy; `None` se a COM falhar (degrada calado).
fn taskbar() -> Option<ITaskbarList3> {
    wstate(|s| {
        if !s.taskbar_tried {
            s.taskbar_tried = true;
            s.taskbar = unsafe { CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER) }
                .ok()
                .filter(|tb| unsafe { tb.HrInit() }.is_ok());
        }
        s.taskbar.clone()
    })
}

/// Selo de "gravando" no botão da taskbar (`SetOverlayIcon`). É a API certa: o
/// `WM_SETICON` não reflete no botão de forma garantida (pegadinha #44).
pub fn set_window_icon_recording(on: bool) {
    let hwnd = main_window();
    if hwnd.0.is_null() || wstate(|s| std::mem::replace(&mut s.recording_now, on) == on) {
        return;
    }
    let Some(tb) = taskbar() else { return };
    unsafe {
        if on {
            let dot = wstate(|s| {
                if s.dot_icon.is_none() {
                    s.dot_icon = record_icon::create_recording_dot_icon(GetSystemMetrics(SM_CXSMICON).max(16));
                }
                s.dot_icon
            });
            if let Some(dot) = dot {
                let _ = tb.SetOverlayIcon(hwnd, dot, PCWSTR::null());
            }
        } else {
            let _ = tb.SetOverlayIcon(hwnd, HICON::default(), PCWSTR::null());
        }
    }
}

/// Popup do menu da bandeja no tema do app (o bridge resolve `system` antes).
pub fn apply_tray_menu_theme(dark: bool) {
    let hwnd = main_window();
    win::apply_menu_theme((!hwnd.0.is_null()).then_some(hwnd), dark);
}

/// Tela cheia da janela host: sem bordas, cobrindo o monitor onde ela está.
pub fn toggle_fullscreen() {
    let hwnd = main_window();
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        if !wstate(|s| s.fs_active) {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
            let _ = GetWindowPlacement(hwnd, &mut wp);
            let maximized = wp.showCmd == SW_MAXIMIZE.0 as u32;
            let mut rc = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rc);
            if maximized {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            wstate(|s| {
                s.fs_style = style;
                s.fs_ex_style = ex;
                s.fs_rect = rc;
                s.fs_maximized = maximized;
                s.fs_active = true;
            });
            let strip = (WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU).0 as isize;
            let strip_ex = (WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE).0 as isize;
            SetWindowLongPtrW(hwnd, GWL_STYLE, style & !strip);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !strip_ex);
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let r = mi.rcMonitor;
            let _ = SetWindowPos(hwnd, Some(HWND_TOP), r.left, r.top, r.right - r.left, r.bottom - r.top,
                SWP_NOOWNERZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
        } else {
            let (style, ex, r, maximized) = wstate(|s| {
                s.fs_active = false;
                (s.fs_style, s.fs_ex_style, s.fs_rect, s.fs_maximized)
            });
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
            let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top,
                SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
            if maximized {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }
        }
    }
}

/// Sai do modo completo e relança o exe em `/hibernate` (1 min escondido e
/// ocioso). Ordem crítica:
/// 1. solta o mutex — senão a hibernação vê `ERROR_ALREADY_EXISTS` e desiste;
/// 2. solta os atalhos — senão ela sobe SEM atalho, calada (falhar o registro
///    não é fatal); antes isso só saía no Shutdown, depois do spawn: corrida;
/// 3. lança (fire-and-forget) e destrói a janela → loop sai → shutdown limpo.
pub fn spawn_hibernate_and_exit() {
    let exe = win::exe_path();
    log!("SpawnHibernate: respawning como /hibernate (exe=\"{exe}\").");
    let m = MUTEX.swap(0, Ordering::SeqCst);
    if m != 0 {
        unsafe {
            let _ = CloseHandle(HANDLE(m as *mut _));
        }
        log::log("SpawnHibernate: mutex liberado.");
    }
    unregister_global_hotkey(100);
    unregister_global_hotkey(101);
    log::log("SpawnHibernate: hotkey liberada (antes do spawn).");
    let rc = unsafe { ShellExecuteW(None, PCWSTR::null(), &HSTRING::from(exe), w!("/hibernate"), PCWSTR::null(), SW_SHOWNORMAL) };
    if rc.0 as usize <= 32 {
        log!("SpawnHibernate: ShellExecuteW FALHOU (codigo={}).", rc.0 as usize);
    } else {
        log::log("SpawnHibernate: ShellExecuteW OK.");
    }
    REAL_QUIT.store(true, Ordering::SeqCst);
    let hwnd = main_window();
    if !hwnd.0.is_null() {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Comandos do menu da bandeja que são do `ui` (Abrir / Fechar).
fn on_tray_command(id: i32) {
    match id {
        tray::ID_TRAY_SHOW => restore_from_tray(),
        tray::ID_TRAY_QUIT => {
            REAL_QUIT.store(true, Ordering::SeqCst);
            let hwnd = main_window();
            if !hwnd.0.is_null() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
        _ => {}
    }
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
    WEB_READY.with(|r| r.set(true));
    resize_webview(hwnd);
    start_navigate(&webview);

    // Início escondido: a janela estava off-screen como tool window só para o
    // WebView2 inicializar. Agora termina num dos dois estados.
    let (hide, minimize, bounds) = wstate(|s| {
        let r = (s.pending_hide_after_ready, s.pending_minimize_after_ready, s.pending_restore_bounds);
        if s.pending_minimize_after_ready {
            // A posição já é restaurada agora; o restore não precisa refazer.
            s.pending_restore_position = false;
        }
        s.pending_hide_after_ready = false;
        s.pending_minimize_after_ready = false;
        r
    });
    unsafe {
        if hide {
            // Modo bandeja: some; só o ícone fica.
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else if minimize {
            // Minimizado na taskbar: volta a ser janela normal, na posição
            // certa, e minimiza sem roubar o foco.
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_TOOLWINDOW.0 as isize));
            let b = bounds;
            let _ = SetWindowPos(hwnd, None, b.left, b.top, b.right - b.left, b.bottom - b.top,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_HIDEWINDOW);
            let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
        }
    }

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

fn splash_is_light() -> bool {
    config::get_config_str("theme", "dark").eq_ignore_ascii_case("light")
}

fn splash_bg() -> COLORREF {
    COLORREF(if splash_is_light() { SPLASH_BG_LIGHT } else { SPLASH_BG_DARK })
}

/// Splash nativo enquanto o WebView2 não cobre a janela: fundo do tema + o
/// título centralizado.
fn draw_splash(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let brush = CreateSolidBrush(splash_bg());
        FillRect(dc, &rc, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let font = CreateFontW(-22, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, FF_DONTCARE.0 as u32, w!("Segoe UI"));
        let old = SelectObject(dc, HGDIOBJ(font.0));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(if splash_is_light() { SPLASH_TEXT_LIGHT } else { SPLASH_TEXT_DARK }));
        let mut text: Vec<u16> = window_title().encode_utf16().collect();
        DrawTextW(dc, &mut text, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = EndPaint(hwnd, &ps);
    }
}

fn restore_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW });
        let _ = SetForegroundWindow(hwnd);
    }
}

fn webview_ready() -> bool {
    WEB_READY.with(|r| r.get())
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
        // Enquanto o controller não existe desenhamos o splash (e suprimimos o
        // erase, que pintaria branco). Com ele, o WebView cobre tudo.
        WM_PAINT if !webview_ready() => {
            draw_splash(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND if !webview_ready() => LRESULT(1),
        tray::WM_TRAYICON => {
            tray::handle_message(hwnd, lparam);
            LRESULT(0)
        }
        WM_SIZE => {
            resize_webview(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(f) = hooks(|h| h.on_timer) { f(wparam.0); }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            if let Some(f) = hooks(|h| h.on_display_change) { f(); }
            LRESULT(0)
        }
        // DBT_DEVNODES_CHANGED (0x0007): o evento mais abrangente de mudança
        // de hardware (USB/webcam), sem RegisterDeviceNotification.
        WM_DEVICECHANGE => {
            if wparam.0 == 0x0007 {
                if let Some(f) = hooks(|h| h.on_device_change) { f(); }
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            if let Some(f) = hooks(|h| h.on_hotkey) { f(wparam.0 as i32); }
            LRESULT(0)
        }
        WM_COMMAND if (wparam.0 & 0xFFFF) as u16 >= TRAY_COMMAND_BASE => {
            tray::handle_command((wparam.0 & 0xFFFF) as i32);
            LRESULT(0)
        }
        WM_CLOSE => {
            // [X] / Alt+F4: vai pra bandeja (closeToTray, gravando, buffer) em
            // vez de fechar — a decisão é do bridge. O "Fechar" do menu da
            // bandeja marca REAL_QUIT e fecha de verdade.
            if !REAL_QUIT.load(Ordering::SeqCst) && hooks(|h| h.on_should_hide_on_close).is_some_and(|f| f()) {
                hide_to_tray();
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DESTROY => {
            tray::remove();
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
        log::log("OBSUI.Run: /autostart + closeToTray=ON + hibernate=ON — redirecionando pra hibernate::run.");
        hibernate::run();
        log::log("OBSUI.Run: hibernate::run retornou — saindo.");
        return;
    }
    if autostart && replay_auto_start {
        log::log("OBSUI.Run: /autostart com buffer automatico ligado — subindo FULL escondido na bandeja (hibernacao nao tem libobs pra guardar).");
    }
    // /autostart: closeToTray ligado = bandeja; desligado = minimizado na
    // taskbar. Nenhum caso de autostart faz a janela "pular" no logon.
    let start_in_tray = autostart && close_to_tray;
    let start_minimized = autostart && !close_to_tray;
    // /start-record-mic CONTÉM /start-record; o sufixo só diz a origem.
    let start_record = cmd.contains("/start-record");
    let start_replay = cmd.contains("/start-replay");
    START_RECORD.store(start_record, Ordering::SeqCst);
    START_RECORD_MIC.store(cmd.contains("/start-record-mic"), Ordering::SeqCst);
    START_REPLAY.store(start_replay, Ordering::SeqCst);
    if start_record {
        log!("OBSUI.Run: /start-record detectado (mic={}) — gravacao iniciara apos warmup.", start_record_requested_by_mic());
    }
    if start_replay {
        log::log("OBSUI.Run: /start-replay detectado — buffer ligara apos o warmup.");
    }
    // Todo flag de promoção entra aqui E na instalação da bandeja (pegadinha
    // #62): abrir um jogo não pode fazer a janela pular na frente.
    let hidden_start = start_in_tray || start_minimized || start_record || start_replay;

    unsafe {
        // Single-instance: a 2ª instância acorda a 1ª e sai. Mesmos literais
        // do hibernate (pegadinha #36) — vêm de `single_instance`.
        let show_msg = RegisterWindowMessageW(&HSTRING::from(SHOW_MSG_NAME));
        WM_SHOW_INSTANCE.store(show_msg, Ordering::SeqCst);
        let mutex = windows::Win32::System::Threading::CreateMutexW(None, false, &HSTRING::from(MUTEX_NAME));
        let already = windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        if already {
            // Autostart duplicado é silencioso por design.
            if !autostart {
                let existing = FindWindowW(&HSTRING::from(CLASS_NAME), PCWSTR::null())
                    .or_else(|_| FindWindowW(w!("TNoOBSHibernate"), PCWSTR::null()));
                if let Ok(existing) = existing {
                    let _ = PostMessageW(Some(existing), show_msg, WPARAM(0), LPARAM(0));
                }
            }
            if let Ok(m) = mutex {
                let _ = CloseHandle(m);
            }
            return;
        }
        // Vive até o fim do processo, salvo o spawn da hibernação (que o solta).
        if let Ok(m) = mutex {
            MUTEX.store(m.0 as isize, Ordering::SeqCst);
        }

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
            hIcon: win::main_icon(),
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
            // WebView2 não inicializa direito com o HWND pai nunca mostrado
            // (render preto): mostra off-screen como tool window (fora da
            // taskbar/Alt+Tab) e termina o estado quando o controller sobe.
            (-32000, -32000, WS_EX_TOOLWINDOW)
        } else {
            (win_x, win_y, WINDOW_EX_STYLE(0))
        };
        if hidden_start {
            wstate(|s| {
                // Convenção: depois de um início escondido, restaura maximizada.
                s.was_maximized = true;
                s.pending_restore_bounds = RECT { left: win_x, top: win_y, right: win_x + win_w, bottom: win_y + win_h };
                s.pending_restore_position = true;
                if start_minimized {
                    s.pending_minimize_after_ready = true;
                } else {
                    s.pending_hide_after_ready = true;
                }
            });
        }
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
        // Elevado (runAsAdmin), o Windows barra mensagens de quem não é: o
        // clique na bandeja (vem do Explorer) e o aviso de uma 2ª instância.
        elevate::allow_from_lower_il(hwnd, &[show_msg, tray::WM_TRAYICON, RegisterWindowMessageW(w!("TaskbarCreated"))]);
        tray::set_callbacks(tray::Callbacks { on_command: Some(on_tray_command), ..Default::default() });
        let _ = ShowWindow(hwnd, if hidden_start { SW_SHOWNOACTIVATE } else { SW_SHOW });
        let _ = UpdateWindow(hwnd);
        // Bandeja só faz sentido com closeToTray ou promoção da hibernação; o
        // minimizado (closeToTray desligado) fica acessível pela taskbar.
        if start_in_tray || start_record || start_replay {
            tray::install(hwnd, &window_title());
        }

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

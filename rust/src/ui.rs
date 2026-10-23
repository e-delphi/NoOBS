//! Host Win32 + WebView2 (porte de `OBSUI.pas`) — SEM Tauri/wry: janela
//! nativa criada à mão, `WebView2Loader` via `webview2-com`, mesma lógica do
//! Delphi.
//!
//! * a UI vem embutida no exe e é servida DA MEMÓRIA pelo `WebResourceRequested`
//!   num origin `https://noobs.app` real — não `NavigateToString` (origin
//!   opaco: sem contexto seguro nem URLs relativas). Pegadinha #16;
//! * loop `GetMessage`/`DispatchMessage` puro; o trabalho enfileirado por
//!   workers é drenado em `WM_APP_WAKE` ([`crate::mainthread`], pegadinha #4);
//! * a mesma ordem de arranque/encerramento do Delphi: COM → janela →
//!   loop → `on_shutdown` (com o STA ainda vivo, pegadinha #31) →
//!   `CoUninitialize`.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, WebMessageReceivedEventHandler,
    WebResourceRequestedEventHandler,
};
use windows::core::{w, Interface, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
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
use crate::{bridge, config, elevate, lang, log, mainthread, record_icon, startup_check, tray, win};

/// Virtual host da UI. `https://` dá contexto seguro (localStorage etc.) e
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
/// Mutex de instância única: o handle fica aberto até o processo sair.
static MUTEX: AtomicIsize = AtomicIsize::new(0);
/// "Fechar" de verdade (menu da bandeja): o `WM_CLOSE` não esconde.
static REAL_QUIT: AtomicBool = AtomicBool::new(false);

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
    /// O WebView2 está sendo criado (ambiente/controller ainda não voltaram).
    static WEB_CREATING: Cell<bool> = const { Cell::new(false) };
    /// Arranque já hibernando: o `WM_CREATE` não cria o WebView2.
    static START_LIGHT: Cell<bool> = const { Cell::new(false) };
    /// Tentativas seguidas de recriar o WebView2 que falharam.
    static WEB_RETRIES: Cell<u32> = const { Cell::new(0) };
}

/// Timer interno do `ui` (não passa pelo bridge): nova tentativa de criar o
/// WebView2 depois de uma falha (o processo do navegador anterior ainda
/// saindo, por exemplo).
const TIMER_WEBVIEW_RETRY: usize = 9100;
const WEBVIEW_RETRY_MS: u32 = 700;
const WEBVIEW_MAX_RETRIES: u32 = 5;

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
    /// Janela criada, antes do loop: o backend sobe (atalhos, vigias, fila).
    /// `true` = arranque já hibernando, sem a interface.
    pub on_start: Option<fn(bool)>,
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
    on_start: None,
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

/// Solta os objetos COM guardados em `thread_local` da main. Tem que rodar ANTES
/// do `CoUninitialize`: o destrutor do `thread_local` só roda no ExitProcess,
/// com a DLL deles (explorerframe, do `ITaskbarList3`) já descarregada — a
/// saída caía. A RTWQ ligada no arranque segurava a COM viva e escondia isso.
pub fn release_thread_com() {
    wstate(|s| s.taskbar = None);
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
    // Teste: não abre navegador/Explorer de verdade.
    #[cfg(test)]
    {
        crate::testkit::record_effect(format!("shell_open:{url}"));
        return;
    }
    #[allow(unreachable_code)]
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

/// Só com a feature `app-tests` (o executável de teste `tests/obs.rs`, fora do
/// app): uma janela ESCONDIDA, criada pelo teste, faz o papel da principal — os
/// `set_timer` do Bridge disparam nela e as workers acordam a fila da main.
#[cfg(feature = "app-tests")]
pub fn attach_headless_window(hwnd: HWND) {
    MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    mainthread::set_wake_window(hwnd);
}

/// Só com `app-tests`: o `WM_TIMER` da janela do teste vai pro mesmo gancho do
/// `window_proc` (o `on_timer` do Bridge).
#[cfg(feature = "app-tests")]
pub fn dispatch_timer(id: usize) {
    if let Some(f) = hooks(|h| h.on_timer) {
        f(id);
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


/// Janela visível e não minimizada.
/// `IsWindowVisible` puro: MINIMIZADA conta como visível (o botão está na
/// taskbar). É a regra do Delphi pra "esconder ao gravar" e pra tirar o ícone
/// da bandeja — ao contrário de [`is_window_shown`], que exclui minimizada.
pub fn is_window_visible() -> bool {
    let hwnd = main_window();
    !hwnd.0.is_null() && unsafe { IsWindowVisible(hwnd).as_bool() }
}

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
        // Saindo da hibernação: a interface volta (com a janela já visível —
        // o WebView2 não inicializa direito num pai nunca mostrado).
        ensure_webview();

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

/// Liga o `AudioVideoTracks` do Chromium (`HTMLMediaElement.audioTracks`),
/// que o player usa para desligar o áudio acima de 2× (pegadinha #61),
/// preservando o que já estiver na variável de ambiente (ex.: porta de
/// depuração remota). Devolve os argumentos pras OPÇÕES do ambiente: o
/// `AdditionalBrowserArguments` delas, mesmo vazio, VENCE a variável — só
/// pôr na variável fazia os dois argumentos sumirem em silêncio.
fn webview_browser_args() -> String {
    const VAR: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
    const ARG: &str = "--enable-blink-features=AudioVideoTracks";
    let cur = std::env::var(VAR).unwrap_or_default();
    let new = if cur.contains("AudioVideoTracks") {
        cur
    } else if cur.is_empty() {
        ARG.to_owned()
    } else {
        format!("{cur} {ARG}")
    };
    log!("WebView2: argumentos extras = \"{new}\"");
    new
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

/// Tipo do arquivo da UI pela extensão (só o que existe em `rust\ui\`).
fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Caminho relativo do arquivo da UI pedido em `https://noobs.app/...`
/// (sem query/fragmento; a raiz é o `index.html`).
fn ui_path_of(uri: &str) -> Option<&str> {
    let rest = uri.strip_prefix(&format!("https://{UI_HOST}/")).or_else(|| (uri == format!("https://{UI_HOST}")).then_some(""))?;
    let path = rest.split(['?', '#']).next().unwrap_or("");
    Some(if path.is_empty() { "index.html" } else { path })
}

/// Responde um pedido da página com o arquivo embutido no exe (`assets`).
/// `SHCreateMemStream` copia os bytes num stream próprio, que o WebView2 lê
/// quando quiser — nada aponta pra memória nossa depois daqui.
fn serve_ui_request(env: &ICoreWebView2Environment, args: &ICoreWebView2WebResourceRequestedEventArgs) -> windows::core::Result<()> {
    unsafe {
        let req = args.Request()?;
        let mut p = PWSTR::null();
        req.Uri(&mut p)?;
        let uri = take_pwstr(p);
        let Some(path) = ui_path_of(&uri) else { return Ok(()) };
        let (status, reason, body, mime) = match crate::assets::ui_file(path) {
            Some(b) => (200, "OK", b, mime_of(path)),
            None => {
                // O Chromium pede o favicon sozinho; a página não tem um.
                if path != "favicon.ico" {
                    log!("UI: \"{path}\" nao existe na interface embutida (404).");
                }
                (404, "Not Found", &[][..], "text/plain")
            }
        };
        let stream = windows::Win32::UI::Shell::SHCreateMemStream(Some(body));
        // no-cache: o WebView2 guarda o cache por origin entre versões; com o
        // conteúdo vindo do exe, uma versão nova tem que valer na hora.
        let headers = format!("Content-Type: {mime}\r\nCache-Control: no-cache");
        let resp = env.CreateWebResourceResponse(stream.as_ref(), status, &HSTRING::from(reason), &HSTRING::from(headers))?;
        args.SetResponse(&resp)
    }
}

/// Liga o servidor da UI em memória e navega para `https://noobs.app/index.html`.
fn start_navigate(webview: &ICoreWebView2, env: &ICoreWebView2Environment) {
    let env = env.clone();
    let h = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
        if let Some(args) = args {
            if let Err(e) = serve_ui_request(&env, &args) {
                log!("UI: falha ao servir um arquivo da interface: {e}");
            }
        }
        Ok(())
    }));
    let mut token = Default::default();
    let filtro = HSTRING::from(format!("https://{UI_HOST}/*"));
    unsafe {
        if let Err(e) = webview.AddWebResourceRequestedFilter(&filtro, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL) {
            log!("StartNavigate: AddWebResourceRequestedFilter falhou: {e}");
            return;
        }
        if let Err(e) = webview.add_WebResourceRequested(&h, &mut token) {
            log!("StartNavigate: add_WebResourceRequested falhou: {e}");
            return;
        }
        log!("StartNavigate: interface servida da memoria ({} arquivos).", crate::assets::ui_file_count());
        let _ = webview.Navigate(&HSTRING::from(format!("https://{UI_HOST}/index.html")));
    }
}

fn on_controller_ready(hwnd: HWND, controller: ICoreWebView2Controller, env: &ICoreWebView2Environment) -> windows::core::Result<()> {
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
                if args.TryGetWebMessageAsString(&mut p).is_ok() {
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

        // A página não pede permissão nenhuma (câmera, microfone,
        // notificações...): tudo é negado.
        let h = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                let _ = args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY);
            }
            Ok(())
        }));
        webview.add_PermissionRequested(&h, &mut token)?;

        // Defesa em profundidade: só a UI local navega.
        let h = NavigationStartingEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                let mut p = PWSTR::null();
                if args.Uri(&mut p).is_ok() {
                    let uri = take_pwstr(p);
                    if !(uri.starts_with(&format!("https://{UI_HOST}")) || uri.starts_with("about:")) {
                        let short: String = uri.chars().take(120).collect();
                        log!("NavigationStarting: cancelando navegação pra \"{short}\"");
                        let _ = args.SetCancel(true);
                    }
                }
            }
            Ok(())
        }));
        webview.add_NavigationStarting(&h, &mut token)?;

        // Janela nova bloqueada; http(s) vai pro navegador do sistema.
        let h = NewWindowRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                let _ = args.SetHandled(true);
                let mut p = PWSTR::null();
                if args.Uri(&mut p).is_ok() {
                    let uri = take_pwstr(p);
                    if uri.starts_with("http://") || uri.starts_with("https://") {
                        shell_open(&uri);
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
    start_navigate(&webview, env);

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

/// Solta a interface: fecha o controller e o processo do navegador (os
/// `msedgewebview2.exe`, ~400 MB) sai quando o último objeto do ambiente é
/// liberado. A janela principal continua (escondida): bandeja, atalhos e
/// timers dependem dela.
pub fn destroy_webview() {
    let ws = WEB.with(|w| w.borrow_mut().take());
    WEB_READY.with(|r| r.set(false));
    kill_timer(TIMER_WEBVIEW_RETRY);
    if let Some(ws) = ws {
        unsafe {
            let _ = ws.controller.Close();
        }
        drop(ws);
        log::log("WebView2: interface descarregada.");
    }
}

/// A interface existe ou está subindo? Senão, cria.
pub fn ensure_webview() {
    if WEB.with(|w| w.borrow().is_some()) || WEB_CREATING.with(Cell::get) {
        return;
    }
    let hwnd = main_window();
    if hwnd.0.is_null() {
        return;
    }
    set_tray_hibernating(false);
    log::log("WebView2: recriando a interface.");
    create_webview(hwnd);
}

/// Ícone da bandeja diz se o app está hibernando.
pub fn set_tray_hibernating(on: bool) {
    let title = window_title();
    let tip = if on { lang::t_with("tray.hibernating", &[("title", &title)]) } else { title };
    tray::update_tooltip(&tip);
}

/// Devolve ao Windows o que o processo não usa mais (depois de soltar a
/// interface e o motor): apara o working set.
pub fn trim_memory() {
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

fn webview_failed(why: &str) {
    WEB_CREATING.with(|c| c.set(false));
    let n = WEB_RETRIES.with(|r| {
        r.set(r.get() + 1);
        r.get()
    });
    if n <= WEBVIEW_MAX_RETRIES {
        log!("WebView2: falhou ({why}) — tentativa {n} de {WEBVIEW_MAX_RETRIES} em {WEBVIEW_RETRY_MS} ms.");
        set_timer(TIMER_WEBVIEW_RETRY, WEBVIEW_RETRY_MS);
    } else {
        log!("WebView2: falhou ({why}) — desistindo; o próximo clique na bandeja tenta de novo.");
        WEB_RETRIES.with(|r| r.set(0));
    }
}

fn create_webview(hwnd: HWND) {
    WEB_CREATING.with(|c| c.set(true));
    let args = webview_browser_args();
    let user_data = secure_user_data_folder();

    let opts = CoreWebView2EnvironmentOptions::default();
    // SAFETY: o objeto acabou de ser criado e ainda não foi entregue ao WebView2.
    unsafe { opts.set_additional_browser_arguments(args) };
    let options: ICoreWebView2EnvironmentOptions = opts.into();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |err, env| {
        log!("TEnvironmentHandler: err={err:?} env={}", env.is_some());
        let env = match (err, env) {
            (Ok(()), Some(env)) => env,
            (e, _) => {
                webview_failed(&format!("ambiente: {e:?}"));
                return Ok(());
            }
        };
        let env_ui = env.clone();
        let ctrl_handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |err, controller| {
            log!("TControllerHandler: err={err:?} controller={}", controller.is_some());
            let controller = match (err, controller) {
                (Ok(()), Some(c)) => c,
                (e, _) => {
                    webview_failed(&format!("controller: {e:?}"));
                    return Ok(());
                }
            };
            WEB_CREATING.with(|c| c.set(false));
            WEB_RETRIES.with(|r| r.set(0));
            on_controller_ready(hwnd, controller, &env_ui)
        }));
        let r = unsafe { env.CreateCoreWebView2Controller(hwnd, &ctrl_handler) };
        if let Err(e) = &r {
            webview_failed(&format!("CreateCoreWebView2Controller: {e}"));
        }
        Ok(())
    }));

    let udf = user_data
        .as_ref()
        .map(|p| HSTRING::from(p.to_string_lossy().as_ref()))
        .unwrap_or_default();
    let rc = unsafe {
        CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), PCWSTR(udf.as_ptr()), &options, &handler)
    };
    if let Err(e) = rc {
        webview_failed(&format!("CreateCoreWebView2EnvironmentWithOptions: {e}"));
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

fn webview_ready() -> bool {
    WEB_READY.with(|r| r.get())
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 2ª instância pedindo para trazer esta à frente (single-instance).
    let show = WM_SHOW_INSTANCE.load(Ordering::SeqCst);
    if show != 0 && msg == show {
        restore_from_tray();
        return LRESULT(0);
    }

    match msg {
        WM_CREATE => {
            MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            mainthread::set_wake_window(hwnd);
            let light = config::get_config_str("theme", "dark").eq_ignore_ascii_case("light");
            set_dark_mode(!light && WM_HOTKEY_DEFAULT_DARK);
            // Arranque hibernando: a interface só sobe no clique da bandeja.
            if !START_LIGHT.with(Cell::get) {
                create_webview(hwnd);
            }
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
        WM_TIMER if wparam.0 == TIMER_WEBVIEW_RETRY => {
            kill_timer(TIMER_WEBVIEW_RETRY);
            ensure_webview();
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

    // Arranque HIBERNANDO (mesmo processo, sem a interface): /autostart com
    // bandeja e hibernação ligadas, ou o `/hibernate` de versões antigas.
    // EXCEÇÃO: replayAutoStart precisa do motor vivo no boot (pegadinha #62).
    let light_start = cmd.contains("/hibernate")
        || (autostart && close_to_tray && config::get_config_bool("hibernate", true) && !replay_auto_start);
    if light_start {
        log::log("OBSUI.Run: arranque hibernando — so bandeja, atalhos e vigias; a interface sobe no clique da bandeja.");
    } else if autostart && replay_auto_start {
        log::log("OBSUI.Run: /autostart com buffer automatico ligado — interface escondida na bandeja (o buffer precisa do motor).");
    }
    START_LIGHT.with(|s| s.set(light_start));
    // /autostart: closeToTray ligado = bandeja; desligado = minimizado na
    // taskbar. Nenhum caso de autostart faz a janela "pular" no logon.
    let start_in_tray = !light_start && autostart && close_to_tray;
    let start_minimized = !light_start && autostart && !close_to_tray;
    let hidden_start = start_in_tray || start_minimized;

    unsafe {
        // Single-instance: a 2ª instância acorda a 1ª e sai. Os literais vêm
        // de `single_instance` (pegadinha #36).
        let show_msg = RegisterWindowMessageW(&HSTRING::from(SHOW_MSG_NAME));
        WM_SHOW_INSTANCE.store(show_msg, Ordering::SeqCst);
        let mutex = windows::Win32::System::Threading::CreateMutexW(None, false, &HSTRING::from(MUTEX_NAME));
        let already = windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        if already {
            // Autostart duplicado é silencioso por design.
            if !autostart {
                if let Ok(existing) = FindWindowW(&HSTRING::from(CLASS_NAME), PCWSTR::null()) {
                    let _ = PostMessageW(Some(existing), show_msg, WPARAM(0), LPARAM(0));
                }
            }
            if let Ok(m) = mutex {
                let _ = CloseHandle(m);
            }
            return;
        }
        // Vive até o fim do processo.
        if let Ok(m) = mutex {
            MUTEX.store(m.0 as isize, Ordering::SeqCst);
        }

        if !startup_check::enforce_runtime() {
            return;
        }
        crate::assets::remove_old_ui_folder();

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
        if light_start {
            // Nasce escondida (sem ShowWindow); o clique na bandeja a mostra,
            // maximizada como depois de qualquer início escondido.
            wstate(|s| s.was_maximized = true);
        }
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
        if !light_start {
            let _ = ShowWindow(hwnd, if hidden_start { SW_SHOWNOACTIVATE } else { SW_SHOW });
            let _ = UpdateWindow(hwnd);
        }
        // Bandeja só faz sentido com closeToTray (ou hibernando); o minimizado
        // (closeToTray desligado) fica acessível pela taskbar.
        if light_start {
            tray::install(hwnd, &lang::t_with("tray.hibernating", &[("title", &window_title())]));
        } else if start_in_tray {
            tray::install(hwnd, &window_title());
        }
        // Backend (atalhos, vigias, fila de transcrição) sobe agora, com ou
        // sem a interface.
        if let Some(f) = hooks(|h| h.on_start) {
            f(light_start);
        }

        // Loop puro; o trabalho das workers volta por WM_APP_WAKE.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup AQUI, com o STA da main ainda vivo (pegadinha #31): objetos
        // COM criados na main saem antes do CoUninitialize.
        if let Some(f) = hooks(|h| h.on_shutdown) {
            f();
        }
        release_thread_com();
        CoUninitialize();
    }
}

#[cfg(test)]
mod tests_ui_files {
    use super::*;

    #[test]
    fn caminho_do_arquivo_pedido() {
        assert_eq!(ui_path_of("https://noobs.app/index.html"), Some("index.html"));
        assert_eq!(ui_path_of("https://noobs.app/"), Some("index.html"));
        assert_eq!(ui_path_of("https://noobs.app"), Some("index.html"));
        assert_eq!(ui_path_of("https://noobs.app/js/main.js?v=2#x"), Some("js/main.js"));
        assert_eq!(ui_path_of("http://127.0.0.1:5000/v/abc"), None);
        assert_eq!(mime_of("css/base.css"), "text/css; charset=utf-8");
        assert_eq!(mime_of("amd.png"), "image/png");
    }
}

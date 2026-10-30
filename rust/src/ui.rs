//! Host da interface: a janela do iron (o form `frmMain`, o que o usuário vê)
//! e uma janela Win32 OCULTA que fica com o que precisa de um HWND fixo e do
//! laço de mensagens (docs/ui-iron.md):
//!
//! * `WM_APP_WAKE` — a fila de trabalho da main ([`crate::mainthread`],
//!   pegadinha #4);
//! * `WM_TIMER` dos timers do bridge, `WM_HOTKEY` dos atalhos globais;
//! * a bandeja (`WM_TRAYICON`, menu), `WM_DISPLAYCHANGE`, `WM_DEVICECHANGE`;
//! * a 2ª instância (`WM_SHOW_INSTANCE`) e o instalador, que procuram a
//!   classe `TNoOBS` (pegadinhas #36 e #58) — um `WM_CLOSE` nela fecha o app.
//!
//! O laço é o do iron (winit). Ele despacha as mensagens de TODAS as janelas
//! da thread, então a janela oculta funciona sem laço próprio. Ordem de
//! arranque/encerramento igual à de antes: COM → janelas → laço →
//! `on_shutdown` (com o STA ainda vivo, pegadinha #31) → `CoUninitialize`.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use windows::core::{w, BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, SetFocus, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{ITaskbarList3, ShellExecuteW, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::*;

use iron::prelude::{ComponentId, Ui, WindowCommand};

use crate::app_view::{self, with_ui};
use crate::single_instance::{CLASS_NAME, MUTEX_NAME, SHOW_MSG_NAME};
use crate::{config, elevate, log, mainthread, record_icon, startup_check, tray, win};

/// IDs de comando do menu da bandeja são >= 9000.
const TRAY_COMMAND_BASE: u16 = 9000;

/// A janela oculta (mensagens, timers, atalhos, bandeja).
static HOST_HWND: AtomicIsize = AtomicIsize::new(0);
/// HWND da janela do iron, pra perguntas de qualquer thread (visível,
/// minimizada). 0 até a 1ª exibição.
static VISIBLE_HWND: AtomicIsize = AtomicIsize::new(0);
static WM_SHOW_INSTANCE: AtomicU32 = AtomicU32::new(0);
/// `TaskbarCreated` (registrada): a barra de tarefas foi recriada.
static WM_TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// Mutex de instância única: o handle fica aberto até o processo sair.
static MUTEX: AtomicIsize = AtomicIsize::new(0);
/// "Fechar" de verdade (menu da bandeja, instalador): o fechar da janela não
/// esconde.
static REAL_QUIT: AtomicBool = AtomicBool::new(false);

/// Estado de janela que só a main thread toca.
#[derive(Default)]
struct WinState {
    /// Estava maximizada antes de ir pra bandeja: volta igual.
    was_maximized: bool,
    /// Posição e tamanho da 1ª vez que a janela aparece.
    initial_bounds: Option<RECT>,
    /// Ícone do exe já aplicado no HWND do iron.
    icon_applied: bool,
    /// /autostart sem bandeja: a 1ª exibição é minimizada.
    start_minimized: bool,
    dark: bool,
    /// Overlay de "gravando" no botão da taskbar.
    taskbar: Option<ITaskbarList3>,
    taskbar_tried: bool,
    dot_icon: Option<HICON>,
    recording_now: bool,
    fullscreen: bool,
}

thread_local! {
    static WSTATE: RefCell<WinState> = RefCell::new(WinState::default());
}

fn wstate<R>(f: impl FnOnce(&mut WinState) -> R) -> R {
    WSTATE.with(|s| f(&mut s.borrow_mut()))
}

/// Callbacks que o orquestrador (bridge) registra — os `OnUI*` do Delphi.
#[derive(Default)]
pub struct Hooks {
    pub on_timer: Option<fn(usize)>,
    pub on_hotkey: Option<fn(i32)>,
    pub on_display_change: Option<fn()>,
    pub on_device_change: Option<fn()>,
    pub on_should_hide_on_close: Option<fn() -> bool>,
    pub on_window_restored: Option<fn()>,
    /// Janela criada, antes do laço: o backend e a interface sobem.
    pub on_start: Option<fn()>,
    pub on_shutdown: Option<fn()>,
}

static HOOKS: std::sync::Mutex<Hooks> = std::sync::Mutex::new(Hooks {
    on_timer: None,
    on_hotkey: None,
    on_display_change: None,
    on_device_change: None,
    on_should_hide_on_close: None,
    on_window_restored: None,
    on_start: None,
    on_shutdown: None,
});

pub fn set_hooks(h: Hooks) {
    *HOOKS.lock().unwrap_or_else(|p| p.into_inner()) = h;
}

/// Lê os hooks SOB a trava e devolve só o ponteiro: o handler roda FORA dela.
/// Chamar o handler dentro travaria (deadlock) assim que ele chamasse algo do
/// `ui` que também lê um hook. O `Mutex` não é reentrante.
fn hooks<R>(f: impl FnOnce(&Hooks) -> R) -> R {
    f(&HOOKS.lock().unwrap_or_else(|p| p.into_inner()))
}

/// Solta os objetos COM guardados em `thread_local` da main. Tem que rodar ANTES
/// do `CoUninitialize`: o destrutor do `thread_local` só roda no ExitProcess,
/// com a DLL deles (explorerframe, do `ITaskbarList3`) já descarregada — a
/// saída caía (pegadinha #68).
pub fn release_thread_com() {
    wstate(|s| s.taskbar = None);
}

/// Um pedido à janela do iron (esconder, mostrar, minimizar...).
fn window_command(command: WindowCommand) {
    with_ui(move |ui| {
        if let Some(form) = app_view::main_form() {
            ui.window_command(form, command);
        }
    });
}

fn host_window() -> HWND {
    HWND(HOST_HWND.load(Ordering::SeqCst) as *mut _)
}

/// HWND da janela visível (o form do iron), se ela já foi criada pelo winit
/// (o laço começou). Antes disso, `None`.
fn visible_hwnd() -> Option<HWND> {
    let form = app_view::main_form()?;
    iron::platform::win::hwnd(form).map(|h| HWND(h as *mut _))
}

/// Dono de diálogos e caixas de mensagem: a janela visível, senão a oculta.
/// De qualquer thread.
pub fn main_window() -> HWND {
    match VISIBLE_HWND.load(Ordering::SeqCst) {
        0 => host_window(),
        h => HWND(h as *mut _),
    }
}

// ---------------------------------------------------------------------------
// API pública usada pelo bridge
// ---------------------------------------------------------------------------

/// Barra de título escura/clara (`DWMWA_USE_IMMERSIVE_DARK_MODE`). Guardada:
/// a janela que ainda não existe recebe ao aparecer.
pub fn set_dark_mode(enable: bool) {
    wstate(|s| s.dark = enable);
    if let Some(hwnd) = visible_hwnd() {
        apply_dark_mode(hwnd, enable);
    }
}

fn apply_dark_mode(hwnd: HWND, enable: bool) {
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

/// `RegisterHotKey` global → `WM_HOTKEY` com `wParam = id` (na janela oculta).
/// Pode falhar se outro app já registrou a combinação: não é fatal (#33).
pub fn register_global_hotkey(id: i32, modifiers: u32, vk: u32) -> bool {
    let hwnd = host_window();
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
    let hwnd = host_window();
    if !hwnd.0.is_null() {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
    }
}

/// Só com a feature `app-tests` (executável de teste, fora do app): uma janela
/// ESCONDIDA, criada pelo teste, faz o papel da oculta — os `set_timer` do
/// Bridge disparam nela e as workers acordam a fila da main.
#[cfg(feature = "app-tests")]
pub fn attach_headless_window(hwnd: HWND) {
    HOST_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    mainthread::set_wake_window(hwnd);
}

/// Só com `app-tests`: o `WM_TIMER` da janela do teste vai pro mesmo gancho.
#[cfg(feature = "app-tests")]
pub fn dispatch_timer(id: usize) {
    if let Some(f) = hooks(|h| h.on_timer) {
        f(id);
    }
}

/// `SetTimer` na janela oculta (`WM_TIMER` → hook `on_timer`). Mesmo id
/// reaproveitado substitui o timer anterior — por isso os ids são únicos.
pub fn set_timer(id: usize, interval_ms: u32) {
    let hwnd = host_window();
    if !hwnd.0.is_null() {
        unsafe {
            SetTimer(Some(hwnd), id, interval_ms, None);
        }
    }
}

pub fn kill_timer(id: usize) {
    let hwnd = host_window();
    if !hwnd.0.is_null() {
        unsafe {
            let _ = KillTimer(Some(hwnd), id);
        }
    }
}

/// Título configurável (`windowTitle`); vazio vira `NoOBS`. Usado na janela e
/// no tooltip da bandeja.
pub fn window_title() -> String {
    let t = config::get_config_str("windowTitle", "NoOBS");
    let t = t.trim();
    if t.is_empty() { "NoOBS".into() } else { t.to_owned() }
}

/// Aplica o título ao vivo (barra de título + tooltip da bandeja).
pub fn apply_window_title() {
    let title = window_title();
    let caption = title.clone();
    with_ui(move |ui| {
        if let Some(form) = app_view::main_form() {
            let _ = ui.set_prop(form, "caption", caption.as_str());
        }
    });
    tray::update_tooltip(&title);
}

/// Janela visível, minimizada ou não. MINIMIZADA conta como visível (o botão
/// está na taskbar) — ao contrário de [`is_window_shown`]. De qualquer thread:
/// lê o HWND (a thread das prévias pergunta), não o estado do iron.
pub fn is_window_visible() -> bool {
    let h = VISIBLE_HWND.load(Ordering::SeqCst);
    h != 0 && unsafe { IsWindowVisible(HWND(h as *mut _)).as_bool() }
}

pub fn is_window_shown() -> bool {
    let h = VISIBLE_HWND.load(Ordering::SeqCst);
    h != 0 && unsafe { IsWindowVisible(HWND(h as *mut _)).as_bool() && !IsIconic(HWND(h as *mut _)).as_bool() }
}

/// Esconde a janela pra bandeja (instala o ícone se faltar).
pub fn hide_to_tray() {
    if !is_window_visible() {
        return;
    }
    // Sem ícone na bandeja, esconder deixaria o app sem como voltar (sem
    // janela, sem ícone — parece fechado): vai pra barra de tarefas.
    if !tray::install(host_window(), &window_title()) {
        log::log("UI: bandeja indisponivel — minimizando pra barra de tarefas em vez de esconder.");
        minimize_to_taskbar();
        return;
    }
    remember_maximized();
    crate::view::call(|v| v.window_hidden());
    window_command(WindowCommand::Hide);
}

fn remember_maximized() {
    let h = VISIBLE_HWND.load(Ordering::SeqCst);
    let max = h != 0 && unsafe { IsZoomed(HWND(h as *mut _)).as_bool() };
    wstate(|s| s.was_maximized = max);
}

/// Minimiza pra TASKBAR (sem bandeja) — `minimizeOnRecord` com `closeToTray`
/// desligado.
pub fn minimize_to_taskbar() {
    if !is_window_visible() {
        return;
    }
    remember_maximized();
    crate::view::call(|v| v.window_hidden());
    window_command(WindowCommand::Minimize);
}

/// Ícone na bandeja SEM esconder a janela (ativar `closeToTray`).
pub fn ensure_tray_icon() {
    let hwnd = host_window();
    if !hwnd.0.is_null() {
        tray::install(hwnd, &window_title());
    }
}

pub fn remove_tray_icon() {
    tray::remove();
}

/// Mostra a janela (1ª vez, ou voltando da bandeja/taskbar) no estado em que
/// estava. Idempotente.
pub fn restore_from_tray() {
    // O winit só cria o HWND dentro do laço de eventos: posição, tamanho,
    // maximizar e foco vão quando ele existir (Win32 direto).
    with_ui(|ui| finish_show(ui, 0));
    if !config::get_config_bool("closeToTray", true) {
        tray::remove();
    }
    if let Some(f) = hooks(|h| h.on_window_restored) {
        f();
    }
}

/// Segunda metade do [`restore_from_tray`], com o HWND já criado. O winit cria
/// a janela num evento do laço que pode vir depois do 1º tique: tenta de novo.
fn finish_show(ui: &mut Ui, attempt: u32) {
    let Some(hwnd) = visible_hwnd() else {
        if attempt < 60 {
            ui.set_timeout(16, move |ui| finish_show(ui, attempt + 1));
        } else {
            log::log("UI: janela sem HWND depois de mostrar.");
        }
        return;
    };
    on_visible_hwnd();
    unsafe {
        if let Some(b) = wstate(|s| s.initial_bounds.take()) {
            let _ = SetWindowPos(hwnd, None, b.left, b.top, b.right - b.left, b.bottom - b.top, SWP_NOZORDER | SWP_NOACTIVATE);
            log::log("UI: janela mostrada pela primeira vez.");
        }
        // /autostart sem bandeja: minimizado na taskbar, sem roubar o foco.
        if wstate(|s| std::mem::take(&mut s.start_minimized)) {
            let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
            return;
        }
        let max = wstate(|s| s.was_maximized);
        let _ = ShowWindow(hwnd, if max { SW_SHOWMAXIMIZED } else if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW });
    }
    bring_to_front(hwnd);
    // O botão da taskbar é recriado ao reaparecer e o overlay se perde.
    reapply_recording_overlay();
}

/// Anti-roubo de foco: clicando numa notificação, o processo em primeiro plano
/// é o shell e o Windows recusa o `SetForegroundWindow` (a janela aparece
/// atrás, com o botão piscando). Compartilhar o input com a thread em primeiro
/// plano por um instante libera.
fn bring_to_front(hwnd: HWND) {
    unsafe {
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
}

/// O HWND do iron só existe depois que o laço começa: ícone do exe e barra de
/// título no tema entram aqui.
fn on_visible_hwnd() {
    let Some(hwnd) = visible_hwnd() else { return };
    VISIBLE_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    let (dark, icon_done) = wstate(|s| (s.dark, std::mem::replace(&mut s.icon_applied, true)));
    apply_dark_mode(hwnd, dark);
    if !icon_done {
        let icon = win::main_icon();
        if !icon.0.is_null() {
            unsafe {
                SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(icon.0 as isize)));
                SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(icon.0 as isize)));
            }
        }
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

fn reapply_recording_overlay() {
    let (rec, dot) = wstate(|s| (s.recording_now, s.dot_icon));
    if let (true, Some(dot), Some(hwnd)) = (rec, dot, visible_hwnd()) {
        if let Some(tb) = taskbar() {
            unsafe {
                let _ = tb.SetOverlayIcon(hwnd, dot, PCWSTR::null());
            }
        }
    }
}

/// Selo de "gravando" no botão da taskbar (`SetOverlayIcon`). É a API certa: o
/// `WM_SETICON` não reflete no botão de forma garantida (pegadinha #44).
pub fn set_window_icon_recording(on: bool) {
    if wstate(|s| std::mem::replace(&mut s.recording_now, on) == on) {
        return;
    }
    let Some(hwnd) = visible_hwnd() else { return };
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
    let hwnd = host_window();
    win::apply_menu_theme((!hwnd.0.is_null()).then_some(hwnd), dark);
}

/// Tela cheia da janela: sem bordas, cobrindo o monitor onde ela está.
///
/// Sem bordas, no monitor da janela (o `SetFullscreen` do iron).
pub fn toggle_fullscreen() {
    let on = wstate(|s| {
        s.fullscreen = !s.fullscreen;
        s.fullscreen
    });
    window_command(WindowCommand::SetFullscreen(on));
}

/// Fecha o app de verdade (menu da bandeja, instalador).
pub fn quit() {
    REAL_QUIT.store(true, Ordering::SeqCst);
    with_ui(|ui| ui.request_close());
}

/// [X] / Alt+F4 na janela (o `on_close_query` do `frmMain`): vai pra bandeja
/// (closeToTray, gravando, buffer) em vez de fechar — a decisão é do bridge.
pub fn on_close_query(ui: &mut Ui) {
    if !REAL_QUIT.load(Ordering::SeqCst) && hooks(|h| h.on_should_hide_on_close).is_some_and(|f| f()) {
        log::log("UI: fechar a janela — vai pra bandeja.");
        ui.cancel_close();
        hide_to_tray();
        return;
    }
    log::log("UI: fechar a janela — fechando o app (bandeja desligada).");
    REAL_QUIT.store(true, Ordering::SeqCst);
}

/// Comandos do menu da bandeja que são do `ui` (Abrir / Fechar).
fn on_tray_command(id: i32) {
    match id {
        tray::ID_TRAY_SHOW => restore_from_tray(),
        tray::ID_TRAY_QUIT => {
            log::log("UI: Sair pelo menu da bandeja — fechando o app.");
            quit();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Janela oculta
// ---------------------------------------------------------------------------

extern "system" fn host_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 2ª instância pedindo para trazer esta à frente (single-instance).
    let show = WM_SHOW_INSTANCE.load(Ordering::SeqCst);
    if show != 0 && msg == show {
        restore_from_tray();
        return LRESULT(0);
    }
    let tb = WM_TASKBAR_CREATED.load(Ordering::SeqCst);
    if tb != 0 && msg == tb {
        tray::on_taskbar_created(hwnd, &window_title());
        return LRESULT(0);
    }

    match msg {
        mainthread::WM_APP_WAKE => {
            mainthread::drain();
            LRESULT(0)
        }
        tray::WM_TRAYICON => {
            tray::handle_message(hwnd, lparam);
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(f) = hooks(|h| h.on_timer) {
                f(wparam.0);
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            if let Some(f) = hooks(|h| h.on_display_change) {
                f();
            }
            LRESULT(0)
        }
        // DBT_DEVNODES_CHANGED (0x0007): o evento mais abrangente de mudança
        // de hardware (USB/webcam), sem RegisterDeviceNotification.
        WM_DEVICECHANGE => {
            if wparam.0 == 0x0007 {
                if let Some(f) = hooks(|h| h.on_device_change) {
                    f();
                }
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            if let Some(f) = hooks(|h| h.on_hotkey) {
                f(wparam.0 as i32);
            }
            LRESULT(0)
        }
        WM_COMMAND if (wparam.0 & 0xFFFF) as u16 >= TRAY_COMMAND_BASE => {
            tray::handle_command((wparam.0 & 0xFFFF) as i32);
            LRESULT(0)
        }
        // Ninguém vê esta janela: um WM_CLOSE nela é o instalador pedindo pro
        // app fechar com educação (pegadinha #58).
        WM_CLOSE => {
            log::log("UI: WM_CLOSE na janela oculta — fechando o app.");
            quit();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn create_host_window(hinst: HINSTANCE) -> Option<HWND> {
    unsafe {
        let class_name = HSTRING::from(CLASS_NAME);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: hinst,
            hIcon: win::main_icon(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        // Janela de topo NUNCA mostrada — não `HWND_MESSAGE`: janela só de
        // mensagens não recebe broadcast (`WM_DISPLAYCHANGE`, `TaskbarCreated`)
        // e o `FindWindow` da 2ª instância e do instalador não a acha.
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            &class_name,
            &HSTRING::from(window_title()),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )
        .ok()
    }
}

/// Posição e tamanho iniciais: 1700×1200, limitado a 90% da área de trabalho e
/// centralizado nela (pixels físicos: o processo é DPI-aware).
fn initial_bounds() -> RECT {
    let mut work = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    let (ww, wh) = (work.right - work.left, work.bottom - work.top);
    let (win_w, win_h) = (1700.min(ww * 9 / 10), 1200.min(wh * 9 / 10));
    let (x, y) = (work.left + (ww - win_w) / 2, work.top + (wh - win_h) / 2);
    RECT { left: x, top: y, right: x + win_w, bottom: y + win_h }
}

/// Entry point do app.
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
    // /autostart: closeToTray ligado = bandeja; desligado = minimizado na
    // taskbar. Nenhum caso de autostart faz a janela "pular" no logon.
    let start_in_tray = autostart && close_to_tray;
    let start_minimized = autostart && !close_to_tray;

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
        crate::assets::remove_old_ui_folders();

        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinst: HINSTANCE = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        let Some(host) = create_host_window(hinst) else {
            log::log("OBSUI.Run: CreateWindowExW (janela oculta) falhou.");
            return;
        };
        HOST_HWND.store(host.0 as isize, Ordering::SeqCst);
        mainthread::set_wake_window(host);
        // Elevado (runAsAdmin), o Windows barra mensagens de quem não é: o
        // clique na bandeja (vem do Explorer) e o aviso de uma 2ª instância.
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        WM_TASKBAR_CREATED.store(taskbar_created, Ordering::SeqCst);
        elevate::allow_from_lower_il(host, &[show_msg, tray::WM_TRAYICON, taskbar_created]);
        tray::set_callbacks(tray::Callbacks { on_command: Some(on_tray_command), ..Default::default() });
    }

    let mut app = match app_view::create() {
        Ok(app) => app,
        Err(e) => {
            log!("UI: falha ao abrir o projeto iron: {e}");
            unsafe { CoUninitialize() };
            return;
        }
    };
    let form: ComponentId = app.main_form;
    let _ = app.ui.set_prop(form, "caption", window_title().as_str());
    // Nasce escondida: o 1º `restore_from_tray` põe no lugar (posição,
    // maximizada, minimizada) e só então mostra; no /autostart com bandeja ela
    // fica escondida.
    app.ui.set_start_hidden(form, true);
    wstate(|s| {
        s.initial_bounds = Some(initial_bounds());
        // Convenção: depois de um início escondido, a janela volta maximizada.
        s.was_maximized = start_in_tray || start_minimized;
    });

    if start_in_tray {
        log::log("OBSUI.Run: /autostart — janela na bandeja.");
        tray::install(host_window(), &window_title());
    } else {
        wstate(|s| s.start_minimized = start_minimized);
        restore_from_tray();
    }

    // Backend (atalhos, vigias, fila de transcrição) e interface sobem agora.
    if let Some(f) = hooks(|h| h.on_start) {
        f();
    }

    app.development();
    let options = app.options();
    if let Err(e) = iron::platform::run_with(app.ui, form, options) {
        log!("UI: laco de eventos terminou com erro: {e}");
    }

    // Cleanup AQUI, com o STA da main ainda vivo (pegadinha #31): objetos COM
    // criados na main saem antes do CoUninitialize.
    tray::remove();
    if let Some(f) = hooks(|h| h.on_shutdown) {
        f();
    }
    release_thread_com();
    unsafe {
        let host = host_window();
        if !host.0.is_null() {
            let _ = DestroyWindow(host);
        }
        CoUninitialize();
    }
}

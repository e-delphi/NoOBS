{
  OBSUI — host WebView2 que carrega a UI embutida como resource RCDATA "UI".
  Baseado em NV.dpr (Eduardo, 15/03/2026), reduzido para o nosso caso.

  Mantido:
    - WebView2 com pasta de user data segura (ACLs por SID do usuario).
    - Single instance mutex (uma janela por usuario).
    - Sincronizacao de tema HTML <-> barra de titulo do Windows.
    - DPI awareness por monitor.
    - Suporte a dark mode em menus do sistema (uxtheme privado).

  Removido:
    - Retry de navegacao com timer (HTML e local; falha = bug nosso).
    - Handler de permission (UI nao pede camera/mic).

  Tray icon (OBSTray) e a config 'closeToTray' controlam minimizacao
  pra bandeja. WM_CLOSE da janela e interceptada e o callback
  OnUIShouldHideOnClose (registrado por OBSBridge) decide se minimiza
  ou fecha de verdade. Auto-start com Windows (OBSAutostart) e
  independente — so registra entrada no HKCU\Run sem afetar bandeja.

  HTML carregado direto da resource embutida no exe (sem dependencia de arquivo).
}
unit OBSUI;

interface

uses
  Winapi.Windows;

procedure Run;

// Posta uma mensagem JSON para o JS rodando na pagina.
// Usado pelo OBSBridge para empurrar atualizacoes de estado.
procedure PostJSON(const AJsonStr: string);

// Toggle fullscreen da janela host (borderless, ocupa tela inteira).
// Preserva style + bounds originais pra restaurar no toggle de volta.
procedure ToggleFullscreen;

// Handle da janela principal — exposto pra OBSBridge poder usar
// SetTimer/KillTimer ancorados nela.
function MainWindowHandle: HWND;

type
  TUIMessageProc = procedure(const AMsg: string);
  TUITimerProc   = procedure(ATimerId: UINT_PTR);
  TUIDisplayChangeProc = procedure;
  TUIDeviceChangeProc  = procedure;
  TUIHotkeyProc  = procedure(AHotkeyId: Integer);
  // Returna True se WM_CLOSE deve esconder na bandeja em vez de fechar
  // de verdade. OBSBridge implementa: True se autostart OU gravando.
  TUIShouldHideOnCloseFunc = function: Boolean;
  TUIVisibilityProc = procedure;

// Callbacks que o OBSBridge registra na sua initialization.
// Mantem OBSUI sem dependencia direta de OBSBridge, evitando ciclos.
var
  OnUIMessage:           TUIMessageProc           = nil;
  OnUITimer:             TUITimerProc             = nil;
  OnUIDisplayChange:     TUIDisplayChangeProc     = nil;
  OnUIDeviceChange:      TUIDeviceChangeProc      = nil;
  OnUIHotkey:            TUIHotkeyProc            = nil;
  OnUIShouldHideOnClose: TUIShouldHideOnCloseFunc = nil;
  // Disparados de HideToTray/MinimizeToTaskbar e RestoreFromTray
  // respectivamente. OBSBridge usa pra armar/desarmar o timer de idle
  // hibernate. Ambos sao opcionais (nil-safe).
  OnUIWindowHidden:      TUIVisibilityProc        = nil;
  OnUIWindowRestored:    TUIVisibilityProc        = nil;

// Esconde a janela principal pra bandeja (instala icone se ainda nao
// estiver). Chamado por OBSBridge quando "minimizar ao gravar" esta
// ativo + closeToTray ON, ou pelo WM_CLOSE quando o fluxo manda
// esconder.
procedure HideToTray;

// Minimiza a janela principal pra TASKBAR (sem ir pra bandeja). Usado
// quando 'minimizeOnRecord' esta ativo mas 'closeToTray' nao —
// minimize visivel na barra de tarefas em vez de sumir.
procedure MinimizeToTaskbar;

// Restaura a janela principal da bandeja OU da taskbar (mostra + traz
// pra frente). Reusa o estado WasMaximized do ultimo HideToTray/
// MinimizeToTaskbar pra restaurar no mesmo modo que estava. Idempotente
// — no-op se ja visivel e nao minimizada.
procedure RestoreFromTray;

// Instala o icone na bandeja SEM esconder a janela. Usado quando o user
// ativa "Iniciar com Windows" — esperamos o icone visivel imediato.
// Idempotente (no-op se ja instalado).
procedure EnsureTrayIcon;

// Remove o icone da bandeja. Idempotente.
procedure RemoveTrayIcon;

// Registra atalho global do Windows. AModifiers e combinacao de
// MOD_ALT/MOD_CONTROL/MOD_SHIFT/MOD_WIN (Winapi.Windows). AVk e o
// virtual-key (VK_F9, etc). Retorna True se registrado com sucesso.
// IDs > 0 e < $C000 — caller escolhe.
function RegisterGlobalHotkey(AId: Integer; AModifiers: UINT; AVk: UINT): Boolean;
procedure UnregisterGlobalHotkey(AId: Integer);

// True se o exe foi lancado com /start-record (a hibernacao spawna o
// full com esse flag quando o user aperta a hotkey). OBSBridge consulta
// no fim do warmup do libobs pra iniciar gravacao automaticamente.
function StartRecordRequested: Boolean;

// Sai do modo full e re-spawna o exe em /hibernate. Chamado pelo
// OBSBridge quando o idle timer (1min sem janela visivel) dispara.
// Fluxo: spawna NoOBS.exe /hibernate + DestroyWindow (-> WM_DESTROY ->
// PostQuitMessage -> finalizacao limpa via Shutdown).
procedure SpawnHibernateAndExit;

implementation

uses
  Winapi.Messages,
  Winapi.ActiveX,
  Winapi.WebView2,
  Winapi.DwmApi,
  Winapi.MultiMon,
  Winapi.ShellAPI,
  System.Classes,
  System.Math,
  System.SysUtils,
  OBSConfig,
  OBSLog,
  OBSStartupCheck,
  OBSTray,
  OBSHibernate;

// ---------------------------------------------------------------------
// Fallback de ICoreWebView2Settings3 pra Delphi 11
// ---------------------------------------------------------------------
// O Winapi.WebView2 do Delphi 12 ja declara essa interface. No Delphi 11
// nao — declaramos aqui com o IID oficial da Microsoft
// ({FDB5AB74-AF33-4854-84F0-0A631DEB5EBA}, publicado no SDK do WebView2)
// e reproduzimos a ordem do vtable: os 2 metodos de Settings2 (UserAgent)
// como padding, seguidos dos 2 proprios de Settings3.
//
// Nao usamos Settings2.UserAgent — esta ai so pra ocupar os slots
// corretos do vtable, ja que herdamos direto de ICoreWebView2Settings
// (que sempre existe) e nao de Settings2 (que tambem pode faltar).
{$IF not declared(ICoreWebView2Settings3)}
const
  IID_ICoreWebView2Settings3: TGUID = '{FDB5AB74-AF33-4854-84F0-0A631DEB5EBA}';
type
  ICoreWebView2Settings3 = interface(ICoreWebView2Settings)
    ['{FDB5AB74-AF33-4854-84F0-0A631DEB5EBA}']
    function Get_UserAgent(out userAgent: PWideChar): HRESULT; stdcall;
    function Set_UserAgent(userAgent: PWideChar): HRESULT; stdcall;
    function Get_AreBrowserAcceleratorKeysEnabled(out value: Integer): HRESULT; stdcall;
    function Set_AreBrowserAcceleratorKeysEnabled(value: Integer): HRESULT; stdcall;
  end;
{$IFEND}

const
  CLASS_NAME    = 'TNoOBS';
  WINDOW_TITLE  = 'NoOBS';

  MUTEX_NAME    = 'NoOBS.SingleInstance.' + CLASS_NAME;
  SHOW_MSG_NAME = 'NoOBS.ShowInstance.'   + CLASS_NAME;

  CSIDL_LOCAL_APPDATA                 = $001C;
  SE_FILE_OBJECT                      = 1;
  PROTECTED_DACL_SECURITY_INFORMATION = DWORD($80000000);
  SDDL_REVISION_1                     = 1;
  PAM_ALLOW_DARK                      = 1;

  // DwmSetWindowAttribute attribute id (Windows 10 1903+; Win11 estavel).
  // Nao esta exposto em Winapi.DwmApi do RAD Studio 11, declaramos local.
  DWMWA_USE_IMMERSIVE_DARK_MODE = 20;

type
  PTokenUserRec = ^TTokenUserRec;
  TTokenUserRec = record
    Sid: Pointer;
    Attributes: DWORD;
  end;

  TSetPreferredAppMode    = function(AppMode: Integer): Integer; stdcall;
  TAllowDarkModeForWindow = function(Wnd: HWND; Allow: BOOL): BOOL; stdcall;
  TFlushMenuThemes        = procedure; stdcall;

var
  WebView: ICoreWebView2;
  Controller: ICoreWebView2Controller;
  MainWindow: HWND;
  SingleInstanceMutex: THandle = 0;
  WM_SHOW_INSTANCE: UINT = 0;
  UserDataFolder: string;
  // True quando o user pediu pra fechar de verdade (menu tray -> Fechar).
  // WM_CLOSE checa essa flag: se False, minimiza pra bandeja em vez de
  // destruir a janela.
  RealQuitRequested: Boolean = False;
  // Lembra se a janela estava maximizada antes de minimizar pra tray —
  // restaura no mesmo estado quando o user reabrir.
  WasMaximized: Boolean = False;
  // Quando o app sobe via /tray, criamos a janela off-screen com
  // WS_EX_TOOLWINDOW (oculta da taskbar) e damos SW_SHOWNOACTIVATE
  // pra WebView2 inicializar corretamente — parent HWND nunca shown
  // durante init = rendering preto/quebrado. Esses vars guardam a
  // posicao pretendida e marcam:
  //   - PendingHideAfterWebViewReady: SW_HIDE quando WebView2 termina init
  //   - PendingRestorePosition: reposiciona + remove WS_EX_TOOLWINDOW na
  //     primeira abertura via tray
  PendingRestoreBounds: TRect = (Left: 0; Top: 0; Right: 0; Bottom: 0);
  PendingRestorePosition:       Boolean = False;
  PendingHideAfterWebViewReady: Boolean = False;

  // True se este processo foi spawnado por OBSHibernate.Run com o flag
  // /start-record (o user apertou a hotkey de gravacao enquanto no
  // modo hibernate). OBSBridge consulta em TIMER_OBS_WARMUP pra iniciar
  // gravacao automaticamente assim que libobs esta pronto.
  FStartRecordRequested: Boolean = False;

  SetPreferredAppMode:    TSetPreferredAppMode    = nil;
  AllowDarkModeForWindow: TAllowDarkModeForWindow = nil;
  FlushMenuThemes:        TFlushMenuThemes        = nil;

function CreateCoreWebView2EnvironmentWithOptions(browserExecutableFolder: PWideChar; userDataFolder: PWideChar; environmentOptions: ICoreWebView2EnvironmentOptions; environmentCreatedHandler: ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler): HRESULT; stdcall; external 'WebView2Loader.dll';

function SHGetFolderPathW(hwndOwner: HWND; nFolder: Integer; hToken: THandle; dwFlags: DWORD; pszPath: PWideChar): HRESULT; stdcall; external 'shell32.dll';
function ConvertSidToStringSidW(Sid: Pointer; var StringSid: PWideChar): BOOL; stdcall; external 'advapi32.dll';
function ConvertStringSecurityDescriptorToSecurityDescriptorW(StringSecurityDescriptor: PWideChar; StringSDRevision: DWORD; var SecurityDescriptor: Pointer; SecurityDescriptorSize: PDWORD): BOOL; stdcall; external 'advapi32.dll';
function GetSecurityDescriptorDacl(pSecurityDescriptor: Pointer; var lpbDaclPresent: BOOL; var pDacl: Pointer; var lpbDaclDefaulted: BOOL): BOOL; stdcall; external 'advapi32.dll';
function SetNamedSecurityInfoW(pObjectName: PWideChar; ObjectType: Integer; SecurityInfo: DWORD; psidOwner: Pointer; psidGroup: Pointer; pDacl: Pointer; pSacl: Pointer): DWORD; stdcall; external 'advapi32.dll';

// =====================================================================
// User-data folder seguro (per-user via SDDL)
// =====================================================================

function DirExistsW(const Dir: string): Boolean;
var
  Attr: DWORD;
begin
  Attr := GetFileAttributesW(PWideChar(Dir));
  Result := (Attr <> INVALID_FILE_ATTRIBUTES) and ((Attr and FILE_ATTRIBUTE_DIRECTORY) <> 0);
end;

function GetCurrentUserSidString: string;
var
  Token: THandle;
  Size: DWORD;
  Info: Pointer;
  SidStr: PWideChar;
begin
  Result := '';
  if not OpenProcessToken(GetCurrentProcess, TOKEN_QUERY, Token) then Exit;
  try
    Size := 0;
    GetTokenInformation(Token, TokenUser, nil, 0, Size);
    if Size = 0 then Exit;
    GetMem(Info, Size);
    try
      if GetTokenInformation(Token, TokenUser, Info, Size, Size) then
        if ConvertSidToStringSidW(PTokenUserRec(Info).Sid, SidStr) then
        begin
          Result := string(SidStr);
          LocalFree(HLOCAL(SidStr));
        end;
    finally
      FreeMem(Info);
    end;
  finally
    CloseHandle(Token);
  end;
end;

function BuildUserDataFolder: string;
var
  Buf: array[0..MAX_PATH - 1] of WideChar;
begin
  Result := '';
  if SHGetFolderPathW(0, CSIDL_LOCAL_APPDATA, 0, 0, @Buf[0]) <> S_OK then Exit;
  Result := string(PWideChar(@Buf[0]));
  if (Result <> '') and (Result[Length(Result)] <> '\') then
    Result := Result + '\';
  Result := Result + CLASS_NAME;
end;

function EnsureSecureUserDataFolder(const Dir: string): Boolean;
var
  UserSid, Sddl: string;
  SD, Dacl: Pointer;
  DaclPresent, DaclDefaulted: BOOL;
  SA: TSecurityAttributes;
begin
  Result := False;
  if Dir = '' then Exit;

  SD := nil;
  UserSid := GetCurrentUserSidString;
  if UserSid <> '' then
  begin
    Sddl := 'D:P(A;OICI;FA;;;' + UserSid + ')(A;OICI;FA;;;SY)';
    if not ConvertStringSecurityDescriptorToSecurityDescriptorW(PWideChar(Sddl), SDDL_REVISION_1, SD, nil) then
      SD := nil;
  end;

  try
    if DirExistsW(Dir) then
      Result := True
    else
    begin
      if SD <> nil then
      begin
        SA.nLength := SizeOf(SA);
        SA.lpSecurityDescriptor := SD;
        SA.bInheritHandle := False;
        Result := CreateDirectoryW(PWideChar(Dir), @SA);
      end
      else
        Result := CreateDirectoryW(PWideChar(Dir), nil);
      if not Result then Exit;
    end;

    if (SD <> nil) and GetSecurityDescriptorDacl(SD, DaclPresent, Dacl, DaclDefaulted) and DaclPresent then
      SetNamedSecurityInfoW(PWideChar(Dir), SE_FILE_OBJECT,
        DACL_SECURITY_INFORMATION or PROTECTED_DACL_SECURITY_INFORMATION,
        nil, nil, Dacl, nil);
  finally
    if SD <> nil then
      LocalFree(HLOCAL(SD));
  end;
end;

// =====================================================================
// Handlers WebView2
// =====================================================================

type
  TControllerHandler = class(TInterfacedObject, ICoreWebView2CreateCoreWebView2ControllerCompletedHandler)
  public
    function Invoke(errorCode: HRESULT; const createdController: ICoreWebView2Controller): HRESULT; stdcall;
  end;

  TEnvironmentHandler = class(TInterfacedObject, ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler)
  public
    function Invoke(errorCode: HRESULT; const createdEnvironment: ICoreWebView2Environment): HRESULT; stdcall;
  end;

  TWebMessageReceivedHandler = class(TInterfacedObject, ICoreWebView2WebMessageReceivedEventHandler)
  public
    function Invoke(const sender: ICoreWebView2; const args: ICoreWebView2WebMessageReceivedEventArgs): HRESULT; stdcall;
  end;

  // Aprova automaticamente toda permissao que o WebView2 pedir. Em
  // particular precisamos de NOTIFICATIONS (kind=4) pra que o JS possa
  // chamar `new Notification(...)` — usado pelo Bridge handler
  // 'show_notification' (avisos de inicio/fim de gravacao). Como o
  // conteudo HTML e RCDATA empacotado no proprio binario (sem origem
  // remota), liberar tudo nao tem risco — nao tem terceiro carregando
  // pagina ali.
  TPermissionRequestedHandler = class(TInterfacedObject, ICoreWebView2PermissionRequestedEventHandler)
  public
    function Invoke(const sender: ICoreWebView2;
      const args: ICoreWebView2PermissionRequestedEventArgs): HRESULT; stdcall;
  end;

procedure SetSizeWindow(Window: HWND; Ctrl: ICoreWebView2Controller);
var
  r: tagRECT;
  r2: TRect;
begin
  GetClientRect(Window, r2);
  r.left := r2.Left;
  r.top := r2.Top;
  r.right := r2.Right;
  r.bottom := r2.Bottom;
  Ctrl.SetBoundsAndZoomFactor(r, 1.0);
end;

function LoadHtmlFromResource(const AResName: string): string;
// Le um RCDATA do exe e devolve como string UTF-8.
var
  HResInfo: HRSRC;
  HResData: HGLOBAL;
  Ptr: Pointer;
  Sz: DWORD;
  Bytes: TBytes;
begin
  Result := '';
  HResInfo := FindResource(HInstance, PChar(AResName), RT_RCDATA);
  if HResInfo = 0 then Exit;
  HResData := LoadResource(HInstance, HResInfo);
  if HResData = 0 then Exit;
  Ptr := LockResource(HResData);
  if Ptr = nil then Exit;
  Sz := SizeofResource(HInstance, HResInfo);
  if Sz = 0 then Exit;
  SetLength(Bytes, Sz);
  Move(Ptr^, Bytes[0], Sz);
  Result := TEncoding.UTF8.GetString(Bytes);
end;

// Onde a UI HTML extraida fica em disco. Usada como source pro
// SetVirtualHostNameToFolderMapping — WebView2 le do disco quando
// navegamos pra https://noobs.app/index.html.
//
// Por que disco e nao NavigateToString:
// NavigateToString seta origin pra "about:blank" (origem opaca). A Web
// Notifications API exige contexto seguro (HTTPS / localhost / virtual
// host). Sem origin real, `new Notification(...)` falha silencioso —
// permissao ate pode ser concedida mas a notificacao nunca dispara.
// SetVirtualHostNameToFolderMapping da pra UI um origin https:// real,
// e a Notification API passa a funcionar como num navegador comum.
function GetUiFolderPath: string;
var
  AppData: string;
begin
  AppData := GetEnvironmentVariable('LOCALAPPDATA');
  if AppData = '' then AppData := GetEnvironmentVariable('TEMP');
  Result := IncludeTrailingPathDelimiter(AppData) + 'NoOBS\ui';
end;

function ExtractUiHtmlToDisk: string;
// Escreve o RCDATA 'UI' em <ui-folder>\index.html. Retorna o folder
// (sem o nome do arquivo). String vazia se algo falhar.
var
  Folder, FilePath, Html: string;
  Bytes: TBytes;
  Stream: TFileStream;
begin
  Result := '';
  Html := LoadHtmlFromResource('UI');
  if Html = '' then
  begin
    Log('UI: resource "UI" RCDATA nao encontrada no exe.');
    Exit;
  end;

  Folder := GetUiFolderPath;
  if not ForceDirectories(Folder) then
  begin
    Log('UI: nao consegui criar pasta "%s".', [Folder]);
    Exit;
  end;

  FilePath := IncludeTrailingPathDelimiter(Folder) + 'index.html';
  try
    Bytes := TEncoding.UTF8.GetBytes(Html);
    Stream := TFileStream.Create(FilePath, fmCreate);
    try
      if Length(Bytes) > 0 then
        Stream.WriteBuffer(Bytes[0], Length(Bytes));
    finally
      Stream.Free;
    end;
    Result := Folder;
  except
    on E: Exception do
      Log('UI: falha escrevendo index.html: %s', [E.Message]);
  end;
end;

procedure StartNavigate;
const
  // Virtual host arbitrario. Nao resolve DNS — WebView2 intercepta
  // requests pra esse host e serve do folder mapeado. "https://" da
  // contexto seguro pra Notification API + outras features modernas.
  UI_HOST = 'noobs.app';
var
  Folder: string;
  WV3: ICoreWebView2_3;
begin
  if WebView = nil then Exit;
  Folder := ExtractUiHtmlToDisk;
  if Folder = '' then Exit;

  // Mapeia o virtual host pra pasta com o index.html. ALLOW: WebView2
  // serve qualquer recurso do folder. Origin no JS vira "https://noobs.app".
  if Succeeded(WebView.QueryInterface(IID_ICoreWebView2_3, WV3)) and (WV3 <> nil) then
    WV3.SetVirtualHostNameToFolderMapping(UI_HOST, PWideChar(Folder),
      COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW)
  else
    Log('UI: ICoreWebView2_3 nao disponivel — Notification API pode nao funcionar.');

  WebView.Navigate(PWideChar('https://' + UI_HOST + '/index.html'));
end;

procedure SetDarkMode(Wnd: HWND; Enable: Boolean);
var
  Value: BOOL;
begin
  Value := Enable;
  DwmSetWindowAttribute(Wnd, DWMWA_USE_IMMERSIVE_DARK_MODE, @Value, SizeOf(Value));
end;

function TControllerHandler.Invoke(errorCode: HRESULT; const createdController: ICoreWebView2Controller): HRESULT;
var
  Token: EventRegistrationToken;
  Settings: ICoreWebView2Settings;
  Settings3: ICoreWebView2Settings3;
begin
  if Failed(errorCode) or (createdController = nil) then
  begin
    Result := errorCode;
    Exit;
  end;

  Controller := createdController;
  Controller.Get_CoreWebView2(WebView);
  SetSizeWindow(MainWindow, Controller);

  // Desabilita coisas de navegador que nao fazem sentido num app:
  //  - DevTools (F12 / Ctrl+Shift+I)
  //  - Menu de contexto default ("Reload", "View source", etc.)
  //  - Browser accelerator keys (F5 reload, Ctrl+F find, Ctrl+P, etc.)
  //  - Controle de zoom (Ctrl+Roda do mouse)
  if Succeeded(WebView.Get_Settings(Settings)) and (Settings <> nil) then
  begin
    Settings.Set_AreDevToolsEnabled(0);
    Settings.Set_AreDefaultContextMenusEnabled(0);
    if Succeeded(Settings.QueryInterface(
      IID_ICoreWebView2Settings3, Settings3)) and (Settings3 <> nil) then
      Settings3.Set_AreBrowserAcceleratorKeysEnabled(0);
  end;
  Controller.Set_ZoomFactor(1.0);

  WebView.add_WebMessageReceived(TWebMessageReceivedHandler.Create, Token);
  WebView.add_PermissionRequested(TPermissionRequestedHandler.Create, Token);

  StartNavigate;

  // Se o app subiu via /tray, a janela esta visivel off-screen com
  // WS_EX_TOOLWINDOW (truque pra WebView2 inicializar com parent visivel).
  // Agora que o WebView2 esta pronto, escondemos a janela — o icone na
  // bandeja ja foi instalado.
  if PendingHideAfterWebViewReady then
  begin
    PendingHideAfterWebViewReady := False;
    ShowWindow(MainWindow, SW_HIDE);
  end;

  // Reduz working set apos warmup do WebView.
  SetProcessWorkingSetSize(GetCurrentProcess, SIZE_T(-1), SIZE_T(-1));

  Result := S_OK;
end;

function TEnvironmentHandler.Invoke(errorCode: HRESULT; const createdEnvironment: ICoreWebView2Environment): HRESULT;
begin
  if Failed(errorCode) or (createdEnvironment = nil) then
  begin
    Result := errorCode;
    Exit;
  end;
  createdEnvironment.CreateCoreWebView2Controller(MainWindow, TControllerHandler.Create);
  Result := S_OK;
end;

function TPermissionRequestedHandler.Invoke(const sender: ICoreWebView2;
  const args: ICoreWebView2PermissionRequestedEventArgs): HRESULT;
begin
  if args <> nil then
    args.Set_State(COREWEBVIEW2_PERMISSION_STATE_ALLOW);
  Result := S_OK;
end;

function TWebMessageReceivedHandler.Invoke(const sender: ICoreWebView2; const args: ICoreWebView2WebMessageReceivedEventArgs): HRESULT;
var
  Msg: PWideChar;
  S: string;
begin
  Msg := nil;
  if (args <> nil) and Succeeded(args.TryGetWebMessageAsString(Msg)) and (Msg <> nil) then
  begin
    S := string(Msg);
    CoTaskMemFree(Msg);

    // Mensagens "dark"/"light" sao o canal de sinc da titlebar
    // (postadas pelo JS direto). Qualquer outra coisa e JSON, repassa.
    if (S = 'dark') or (S = 'light') then
      SetDarkMode(MainWindow, S = 'dark')
    else if Assigned(OnUIMessage) then
      OnUIMessage(S);
  end;
  Result := S_OK;
end;

// Implementacao das funcoes publicas
procedure PostJSON(const AJsonStr: string);
begin
  if WebView <> nil then
    WebView.PostWebMessageAsJson(PWideChar(AJsonStr));
end;

function MainWindowHandle: HWND;
begin
  Result := MainWindow;
end;

function StartRecordRequested: Boolean;
begin
  Result := FStartRecordRequested;
end;

procedure SpawnHibernateAndExit;
// Spawna NoOBS.exe /hibernate e dispara o shutdown limpo do processo
// atual. Usado pelo OBSBridge quando o idle timer (1min sem janela
// visivel + sem gravacao) dispara — libera ~150MB pra dar lugar aos
// ~5MB do modo hibernate.
//
// Ordem critica:
//   1. CloseHandle(mutex) — libera ANTES do spawn pra que o novo
//      processo /hibernate nao detecte ERROR_ALREADY_EXISTS e desista.
//   2. ShellExecute (fire-and-forget) — novo processo sobe em paralelo.
//   3. DestroyWindow -> WM_DESTROY -> PostQuitMessage.
//   4. GetMessage loop sai, OBSUI.Run retorna.
//   5. Unit finalizations rodam — OBSBridge.Shutdown faz cleanup
//      completo (timers, threads, libobs, watchers).
//
// Brevemente nenhum processo segura o mutex (entre passos 1 e o
// CreateMutex do hibernate). Single-instance ainda funciona pq:
//   - 3a tentativa de subir durante esse intervalo pegaria o mutex,
//     entao o novo /hibernate veria ERROR_ALREADY_EXISTS e sairia.
//   - Caso raro o suficiente pra nao valer protecao.
var
  ExePath: array[0..MAX_PATH - 1] of WideChar;
  HInst: THandle;
begin
  GetModuleFileNameW(0, ExePath, MAX_PATH);
  Log('SpawnHibernate: respawning como /hibernate (exe="%s").', [string(ExePath)]);

  if SingleInstanceMutex <> 0 then
  begin
    CloseHandle(SingleInstanceMutex);
    SingleInstanceMutex := 0;
    Log('SpawnHibernate: mutex liberado.');
  end;

  HInst := ShellExecuteW(0, nil, PWideChar(@ExePath[0]),
    '/hibernate', nil, SW_SHOWNORMAL);
  if NativeUInt(HInst) <= 32 then
    Log('SpawnHibernate: ShellExecuteW FALHOU (codigo=%d, LastError=%d).',
      [NativeUInt(HInst), GetLastError])
  else
    Log('SpawnHibernate: ShellExecuteW OK (HINST=%d).', [NativeUInt(HInst)]);

  // Marca quit "de verdade" pra WM_CLOSE nao tentar minimizar pra tray.
  RealQuitRequested := True;
  if MainWindow <> 0 then
    DestroyWindow(MainWindow);
end;

var
  FsActive: Boolean = False;
  FsSavedStyle: NativeInt = 0;
  FsSavedExStyle: NativeInt = 0;
  FsSavedRect: TRect = (Left: 0; Top: 0; Right: 0; Bottom: 0);
  FsSavedMaximized: Boolean = False;

procedure ToggleFullscreen;
var
  Mon: HMONITOR;
  MonInfo: TMonitorInfo;
  WP: TWindowPlacement;
begin
  if MainWindow = 0 then Exit;

  if not FsActive then
  begin
    // Salva estado atual.
    FsSavedStyle   := GetWindowLongPtr(MainWindow, GWL_STYLE);
    FsSavedExStyle := GetWindowLongPtr(MainWindow, GWL_EXSTYLE);
    WP.length := SizeOf(WP);
    GetWindowPlacement(MainWindow, @WP);
    FsSavedMaximized := (WP.showCmd = SW_MAXIMIZE);
    GetWindowRect(MainWindow, FsSavedRect);
    if FsSavedMaximized then
      ShowWindow(MainWindow, SW_RESTORE);

    // Remove bordas + title bar + system menu. Mantem o WS_VISIBLE.
    SetWindowLongPtr(MainWindow, GWL_STYLE,
      FsSavedStyle and not (WS_CAPTION or WS_THICKFRAME or WS_MINIMIZEBOX or
                             WS_MAXIMIZEBOX or WS_SYSMENU));
    SetWindowLongPtr(MainWindow, GWL_EXSTYLE,
      FsSavedExStyle and not (WS_EX_DLGMODALFRAME or WS_EX_WINDOWEDGE or
                              WS_EX_CLIENTEDGE or WS_EX_STATICEDGE));

    // Cobrir o monitor onde a janela esta (multi-monitor friendly).
    Mon := MonitorFromWindow(MainWindow, MONITOR_DEFAULTTONEAREST);
    MonInfo.cbSize := SizeOf(MonInfo);
    GetMonitorInfo(Mon, @MonInfo);
    SetWindowPos(MainWindow, HWND_TOP,
      MonInfo.rcMonitor.Left, MonInfo.rcMonitor.Top,
      MonInfo.rcMonitor.Right  - MonInfo.rcMonitor.Left,
      MonInfo.rcMonitor.Bottom - MonInfo.rcMonitor.Top,
      SWP_NOOWNERZORDER or SWP_FRAMECHANGED or SWP_SHOWWINDOW);
    FsActive := True;
  end
  else
  begin
    // Restaura.
    SetWindowLongPtr(MainWindow, GWL_STYLE,   FsSavedStyle);
    SetWindowLongPtr(MainWindow, GWL_EXSTYLE, FsSavedExStyle);
    SetWindowPos(MainWindow, 0,
      FsSavedRect.Left, FsSavedRect.Top,
      FsSavedRect.Right  - FsSavedRect.Left,
      FsSavedRect.Bottom - FsSavedRect.Top,
      SWP_NOZORDER or SWP_NOOWNERZORDER or SWP_FRAMECHANGED or SWP_SHOWWINDOW);
    if FsSavedMaximized then
      ShowWindow(MainWindow, SW_MAXIMIZE);
    FsActive := False;
  end;
end;

// =====================================================================
// Dark menus do sistema (uxtheme privado)
// =====================================================================

procedure InitSystemDarkMenuSupport;
var
  UxTheme: HMODULE;
begin
  UxTheme := LoadLibrary('uxtheme.dll');
  if UxTheme = 0 then Exit;
  @SetPreferredAppMode    := GetProcAddress(UxTheme, MAKEINTRESOURCE(135));
  @AllowDarkModeForWindow := GetProcAddress(UxTheme, MAKEINTRESOURCE(133));
  @FlushMenuThemes        := GetProcAddress(UxTheme, MAKEINTRESOURCE(136));
  if Assigned(SetPreferredAppMode) then SetPreferredAppMode(PAM_ALLOW_DARK);
  if Assigned(FlushMenuThemes)     then FlushMenuThemes;
end;

procedure RestoreWindow(Wnd: HWND);
begin
  if IsIconic(Wnd) then
    ShowWindow(Wnd, SW_RESTORE)
  else
    ShowWindow(Wnd, SW_SHOW);
  SetForegroundWindow(Wnd);
end;

// =====================================================================
// Atalhos globais (RegisterHotKey / WM_HOTKEY)
// =====================================================================

function RegisterGlobalHotkey(AId: Integer; AModifiers: UINT; AVk: UINT): Boolean;
begin
  Result := False;
  if MainWindow = 0 then Exit;
  // Win32 RegisterHotKey: hWnd recebe WM_HOTKEY com wParam=AId. Pode
  // falhar se outro app ja registrou a mesma combinacao globalmente.
  Result := Winapi.Windows.RegisterHotKey(MainWindow, AId, AModifiers, AVk);
  if Result then
    Log('OBSUI: hotkey registrado (id=%d mod=$%x vk=$%x).',
      [AId, AModifiers, AVk])
  else
    Log('OBSUI: falha ao registrar hotkey (id=%d mod=$%x vk=$%x) — ja em uso?',
      [AId, AModifiers, AVk]);
end;

procedure UnregisterGlobalHotkey(AId: Integer);
begin
  if MainWindow = 0 then Exit;
  Winapi.Windows.UnregisterHotKey(MainWindow, AId);
end;

// =====================================================================
// Window procedure
// =====================================================================

// Splash nativo: desenhado direto no WindowProc enquanto o WebView2
// nao terminou de carregar. Cores acompanham o tema salvo em config.
const
  SPLASH_BG_DARK    = $000C0A0A;  // BGR de #0a0a0c
  SPLASH_TEXT_DARK  = $00E7E4E4;  // BGR de #e4e4e7
  SPLASH_BG_LIGHT   = $00F5F4F4;  // BGR de #f4f4f5
  SPLASH_TEXT_LIGHT = $001B1818;  // BGR de #18181b

function SplashBgColor: COLORREF;
begin
  if SameText(GetConfigStr('theme', 'dark'), 'light') then
    Result := SPLASH_BG_LIGHT
  else
    Result := SPLASH_BG_DARK;
end;

function SplashTextColor: COLORREF;
begin
  if SameText(GetConfigStr('theme', 'dark'), 'light') then
    Result := SPLASH_TEXT_LIGHT
  else
    Result := SPLASH_TEXT_DARK;
end;

procedure DrawSplash(hwnd: HWND);
var
  PS: TPaintStruct;
  DC: HDC;
  R, TxtRect: TRect;
  Brush: HBRUSH;
  Font, OldFont: HFONT;
  Txt: string;
begin
  GetClientRect(hwnd, R);
  DC := BeginPaint(hwnd, PS);
  try
    Brush := CreateSolidBrush(SplashBgColor);
    FillRect(DC, R, Brush);
    DeleteObject(Brush);

    Font := CreateFont(-22, 0, 0, 0, FW_BOLD, 0, 0, 0,
      DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
      CLEARTYPE_QUALITY, DEFAULT_PITCH or FF_DONTCARE, 'Segoe UI');
    OldFont := SelectObject(DC, Font);
    SetBkMode(DC, TRANSPARENT);
    SetTextColor(DC, SplashTextColor);
    Txt := 'NoOBS';
    TxtRect := R;
    DrawText(DC, PChar(Txt), Length(Txt), TxtRect,
      DT_CENTER or DT_VCENTER or DT_SINGLELINE);
    SelectObject(DC, OldFont);
    DeleteObject(Font);
  finally
    EndPaint(hwnd, PS);
  end;
end;

function WindowProc(hwnd: HWND; msg: UINT; wParam: WPARAM; lParam: LPARAM): LRESULT; stdcall;
begin
  // Mensagem custom de single-instance (segunda instancia traz primeira ao topo).
  if (WM_SHOW_INSTANCE <> 0) and (msg = WM_SHOW_INSTANCE) then
  begin
    RestoreWindow(hwnd);
    Result := 0;
    Exit;
  end;

  case msg of
    WM_CREATE:
    begin
      MainWindow := hwnd;
      SetDarkMode(hwnd, not SameText(GetConfigStr('theme', 'dark'), 'light'));
      if Assigned(AllowDarkModeForWindow) then
        AllowDarkModeForWindow(hwnd, True);

      UserDataFolder := BuildUserDataFolder;
      if (UserDataFolder <> '') and EnsureSecureUserDataFolder(UserDataFolder) then
        CreateCoreWebView2EnvironmentWithOptions(nil, PWideChar(UserDataFolder), nil, TEnvironmentHandler.Create)
      else
        CreateCoreWebView2EnvironmentWithOptions(nil, nil, nil, TEnvironmentHandler.Create);
      Result := 0;
      Exit;
    end;
    WM_PAINT:
    begin
      // Quando o controller existe, o WebView2 ja cobre toda area
      // cliente — nao desenhamos mais nada (DefWindowProc cuida).
      if Controller = nil then
      begin
        DrawSplash(hwnd);
        Result := 0;
        Exit;
      end;
    end;
    WM_ERASEBKGND:
    begin
      // Suprime o erase padrao (que pinta branco) — DrawSplash faz fill.
      if Controller = nil then
      begin
        Result := 1;
        Exit;
      end;
    end;
    WM_SIZE:
    begin
      if Controller <> nil then
        SetSizeWindow(hwnd, Controller);
      Result := 0;
      Exit;
    end;
    WM_TIMER:
    begin
      if Assigned(OnUITimer) then
        OnUITimer(wParam);
      Result := 0;
      Exit;
    end;
    WM_DISPLAYCHANGE:
    begin
      if Assigned(OnUIDisplayChange) then
        OnUIDisplayChange;
      Result := 0;
      Exit;
    end;
    WM_DEVICECHANGE:
    begin
      // wParam = DBT_DEVNODES_CHANGED ($0007) dispara pra qualquer mudanca
      // de hardware no devnode tree (USB plug/unplug, etc). E o evento
      // mais abrangente — pega webcams sem precisar de RegisterDeviceNotification.
      if (wParam = $0007 {DBT_DEVNODES_CHANGED}) and Assigned(OnUIDeviceChange) then
        OnUIDeviceChange;
      Result := 0;
      Exit;
    end;
    WM_HOTKEY:
    begin
      // wParam = ID que passamos no RegisterHotKey.
      // lParam = LOWORD: modifiers, HIWORD: vk — nao precisamos aqui.
      if Assigned(OnUIHotkey) then
        OnUIHotkey(Integer(wParam));
      Result := 0;
      Exit;
    end;
    OBSTray.WM_TRAYICON:
    begin
      OBSTray.HandleTrayMessage(hwnd, lParam);
      Result := 0;
      Exit;
    end;
    WM_COMMAND:
    begin
      // IDs de menu tray sao >= 9000.
      if LOWORD(wParam) >= 9000 then
      begin
        OBSTray.HandleTrayCommand(hwnd, LOWORD(wParam));
        Result := 0;
        Exit;
      end;
    end;
    WM_CLOSE:
    begin
      // Intercepta o close do botao [X] / Alt+F4. Decide entre:
      //   - Fechar de verdade (RealQuitRequested = menu tray "Fechar",
      //     ou nao quer ficar em tray nesse estado)
      //   - Minimizar pra bandeja (autostart ON, ou gravando)
      //
      // OBSBridge implementa OnUIShouldHideOnClose:
      //   Result := autostart_enabled OR recording_active
      if not RealQuitRequested then
      begin
        if Assigned(OnUIShouldHideOnClose) and OnUIShouldHideOnClose() then
        begin
          HideToTray;
          Result := 0;
          Exit;
        end;
        // Caso contrario, cai pra DefWindowProc que destroi a janela
        // (gera WM_DESTROY -> PostQuitMessage -> sai do GetMessage loop).
      end;
    end;
    WM_DESTROY:
    begin
      OBSTray.RemoveTrayIcon;
      PostQuitMessage(0);
      Result := 0;
      Exit;
    end;
  end;
  Result := DefWindowProc(hwnd, msg, wParam, lParam);
end;

// =====================================================================
// Run — entry point publico
// =====================================================================

type
  // Helper de instancia pra ligar TThread.WakeMainThread (TNotifyEvent
  // = method of object) ao PostMessage no HWND principal.
  TWakeMainThread = class
  public
    Wnd: HWND;
    procedure Wake(Sender: TObject);
  end;

procedure TWakeMainThread.Wake(Sender: TObject);
begin
  if Wnd <> 0 then PostMessage(Wnd, WM_NULL, 0, 0);
end;

procedure HideToTray;
begin
  if MainWindow = 0 then Exit;
  if not IsWindowVisible(MainWindow) then Exit;
  WasMaximized := IsZoomed(MainWindow);
  // Avisa o JS antes de esconder — pode querer fechar o player de
  // video (caso contrario fica reproduzindo "fantasma" atras da
  // janela escondida, sem o user poder ver/controlar).
  // PostJSON e idempotente — se a UI nao tem handler, e no-op.
  PostJSON('{"type":"window_hidden"}');
  ShowWindow(MainWindow, SW_HIDE);
  OBSTray.InstallTrayIcon(MainWindow, WINDOW_TITLE);
  if Assigned(OnUIWindowHidden) then OnUIWindowHidden;
end;

procedure MinimizeToTaskbar;
begin
  if MainWindow = 0 then Exit;
  if not IsWindowVisible(MainWindow) then Exit;
  WasMaximized := IsZoomed(MainWindow);
  // Player de video tambem deve fechar — janela minimizada nao
  // deveria continuar reproduzindo audio "fantasma" do player.
  PostJSON('{"type":"window_hidden"}');
  ShowWindow(MainWindow, SW_MINIMIZE);
  if Assigned(OnUIWindowHidden) then OnUIWindowHidden;
end;

procedure EnsureTrayIcon;
begin
  if MainWindow = 0 then Exit;
  OBSTray.InstallTrayIcon(MainWindow, WINDOW_TITLE);
end;

procedure RemoveTrayIcon;
begin
  OBSTray.RemoveTrayIcon;
end;

procedure RestoreFromTray;
var
  FgWnd: HWND;
  FgThreadId, MyThreadId: DWORD;
  Attached: Boolean;
begin
  if MainWindow = 0 then Exit;

  // Se foi start via /tray, a janela esta com WS_EX_TOOLWINDOW e
  // off-screen (truque pra WebView2 inicializar com parent visivel).
  // Na 1a abertura via bandeja: remove o style e reposiciona no centro
  // antes do show. Subsequentes esconde/mostra rodam normais.
  if PendingRestorePosition then
  begin
    PendingRestorePosition := False;
    SetWindowLongPtr(MainWindow, GWL_EXSTYLE,
      GetWindowLongPtr(MainWindow, GWL_EXSTYLE) and not NativeInt(WS_EX_TOOLWINDOW));
    SetWindowPos(MainWindow, 0,
      PendingRestoreBounds.Left, PendingRestoreBounds.Top,
      PendingRestoreBounds.Right  - PendingRestoreBounds.Left,
      PendingRestoreBounds.Bottom - PendingRestoreBounds.Top,
      SWP_NOZORDER or SWP_NOACTIVATE or SWP_FRAMECHANGED or SWP_HIDEWINDOW);
  end;

  // SW_RESTORE serve tanto pra janela escondida (SW_HIDE) quanto
  // minimizada (SW_MINIMIZE) — em ambos os casos volta pro estado
  // anterior (normal/maximizado). Pra maximizada explicitamente
  // usamos SW_SHOWMAXIMIZED pra forcar.
  if WasMaximized then
    ShowWindow(MainWindow, SW_SHOWMAXIMIZED)
  else
    ShowWindow(MainWindow, SW_RESTORE);

  // ---- Bypass do anti-focus-stealing do Windows ----
  //
  // Cenario: usuario clica numa notificacao do Windows. O JS dispara
  // onclick -> Bridge.send('tray_show') -> nossa Dispatch chama
  // RestoreFromTray. Nesse momento o "foreground process" do sistema
  // e o shell (que ja dispensou o toast), nao a gente. Como Windows
  // bloqueia SetForegroundWindow vindo de processo nao-foreground
  // (anti-focus-stealing), a janela aparece mas fica atras com o icone
  // da taskbar piscando — sintoma exato do bug que vimos.
  //
  // Workaround padrao: AttachThreadInput temporariamente compartilha
  // o input state com a thread da janela foreground. Nesse modo o
  // Windows permite SetForegroundWindow pq tecnicamente "ja temos
  // input compartilhado". BringWindowToTop reforca o z-order.
  // Detach no finally pra nao deixar a thread amarrada.
  FgWnd      := GetForegroundWindow;
  MyThreadId := GetCurrentThreadId;
  FgThreadId := 0;
  Attached   := False;
  if (FgWnd <> 0) and (FgWnd <> MainWindow) then
  begin
    FgThreadId := GetWindowThreadProcessId(FgWnd, nil);
    if (FgThreadId <> 0) and (FgThreadId <> MyThreadId) then
      Attached := AttachThreadInput(MyThreadId, FgThreadId, True);
  end;
  try
    BringWindowToTop(MainWindow);
    SetForegroundWindow(MainWindow);
    SetFocus(MainWindow);
  finally
    if Attached then
      AttachThreadInput(MyThreadId, FgThreadId, False);
  end;

  // Mantem o icone na bandeja quando 'closeToTray' esta ativo —
  // o user vai voltar pra bandeja via [X], entao deixa o icone
  // visivel pra facilitar a alternancia.
  if not GetConfigBool('closeToTray', False) then
    OBSTray.RemoveTrayIcon;

  if Assigned(OnUIWindowRestored) then OnUIWindowRestored;
end;

procedure OnTrayCommandHandler(ACommand: Integer);
begin
  case ACommand of
    OBSTray.ID_TRAY_SHOW:
      RestoreFromTray;
    OBSTray.ID_TRAY_QUIT:
    begin
      // Saida real (nao minimiza pra tray).
      RealQuitRequested := True;
      if MainWindow <> 0 then
        DestroyWindow(MainWindow);
    end;
  end;
end;

procedure Run;
var
  Msg: TMsg;
  Wnd: HWND;
  wc: WNDCLASS;
  Existing: HWND;
  Wakeup: TWakeMainThread;
  WorkArea: TRect;
  WinX, WinY, WinW, WinH: Integer;
  StartInTray: Boolean;
  CmdLine: string;
  IsAutostartLaunch: Boolean;
begin
  SetCurrentDir(ExtractFilePath(ParamStr(0)));

  CmdLine := LowerCase(string(GetCommandLine));
  Log('OBSUI.Run: cmdline="%s"', [CmdLine]);
  // /autostart = lancado pelo logon do Windows. /tray = alias antigo.
  IsAutostartLaunch := (Pos('/autostart', CmdLine) > 0) or
                       (Pos('/tray',      CmdLine) > 0);
  Log('OBSUI.Run: IsAutostartLaunch=%s, closeToTray=%s',
    [BoolToStr(IsAutostartLaunch, True),
     BoolToStr(GetConfigBool('closeToTray', False), True)]);

  // /autostart + closeToTray + hibernate=ON: respawna em modo hibernate.
  // Mais leve que carregar tudo aqui e esconder na bandeja — libobs,
  // WebView2, FFmpeg, watchers ficam sem inicializar enquanto o user
  // nao interagir. Se 'hibernate' esta OFF no config, sobe full mode
  // direto e fica em segundo plano consumindo RAM normalmente.
  if IsAutostartLaunch and
     GetConfigBool('closeToTray', False) and
     GetConfigBool('hibernate', True) then
  begin
    Log('OBSUI.Run: /autostart + closeToTray=ON + hibernate=ON — redirecionando pra OBSHibernate.Run.');
    OBSHibernate.Run;
    Log('OBSUI.Run: OBSHibernate.Run retornou — saindo.');
    Exit;
  end;
  // Outros casos de /autostart: cai no fluxo normal (janela visivel).
  StartInTray := False;

  // /start-record: hibernate spawnou esse processo porque o user
  // apertou a hotkey de gravacao. Marca pra OBSBridge consultar em
  // TIMER_OBS_WARMUP e iniciar gravacao apos libobs estar pronto.
  FStartRecordRequested := Pos('/start-record', CmdLine) > 0;
  if FStartRecordRequested then
    Log('OBSUI.Run: /start-record detectado — gravacao iniciara apos warmup.');

  WM_SHOW_INSTANCE := RegisterWindowMessage(SHOW_MSG_NAME);
  SingleInstanceMutex := CreateMutex(nil, False, MUTEX_NAME);
  if GetLastError = ERROR_ALREADY_EXISTS then
  begin
    // Outra instancia ja esta rodando: traz pra frente e sai. Excecao:
    // se a 2a instancia veio do autostart do Windows, nao incomoda o
    // user — a 1a continua como esta (autostart e silencioso por design).
    if not IsAutostartLaunch then
    begin
      // Tenta achar tanto a janela do modo full quanto a do hibernate.
      // Se hibernate esta rodando, ele trata WM_SHOW_INSTANCE
      // promovendo a si mesmo pra full (spawn + exit). Ver
      // OBSHibernate.WindowProc.
      Existing := FindWindow(CLASS_NAME, nil);
      if Existing = 0 then
        Existing := FindWindow('TNoOBSHibernate', nil);
      if Existing <> 0 then
        PostMessage(Existing, WM_SHOW_INSTANCE, 0, 0);
    end;
    if SingleInstanceMutex <> 0 then
      CloseHandle(SingleInstanceMutex);
    Exit;
  end;

  // Validacao do runtime — confirma obs.dll, ffmpeg, WebView2 loader e
  // plugins criticos. Se falta algo critico, mostra MessageBox e sai
  // sem criar janela (evita crash mais a frente com erro obscuro).
  if not EnforceRuntime then
  begin
    if SingleInstanceMutex <> 0 then
      CloseHandle(SingleInstanceMutex);
    Exit;
  end;

  CoInitialize(nil);

  SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

  InitSystemDarkMenuSupport;

  ZeroMemory(@wc, SizeOf(wc));
  wc.style         := CS_HREDRAW or CS_VREDRAW;
  wc.lpfnWndProc   := @WindowProc;
  wc.hInstance     := HInstance;
  wc.hIcon         := LoadIcon(HInstance, 'MAINICON');
  wc.hCursor       := LoadCursor(0, IDC_ARROW);
  // Brush do tema atual, evita flash branco entre WM_CREATE e o
  // primeiro WM_PAINT do splash.
  wc.hbrBackground := CreateSolidBrush(SplashBgColor);
  wc.lpszClassName := CLASS_NAME;
  Winapi.Windows.RegisterClass(wc);

  // Tamanho inicial 1700x1200 em telas 4K (cabe folgado); clampa pra
  // 90% da work area em monitores menores. Centralizado na work area
  // do monitor primario (descontando a barra de tarefas).
  WorkArea := Default(TRect);
  SystemParametersInfo(SPI_GETWORKAREA, 0, @WorkArea, 0);
  WinW := 1700;
  WinH := 1200;
  if WinW > (WorkArea.Right - WorkArea.Left) * 9 div 10 then
    WinW := (WorkArea.Right - WorkArea.Left) * 9 div 10;
  if WinH > (WorkArea.Bottom - WorkArea.Top) * 9 div 10 then
    WinH := (WorkArea.Bottom - WorkArea.Top) * 9 div 10;
  WinX := WorkArea.Left + ((WorkArea.Right  - WorkArea.Left) - WinW) div 2;
  WinY := WorkArea.Top  + ((WorkArea.Bottom - WorkArea.Top)  - WinH) div 2;
  Wnd := CreateWindowEx(0, CLASS_NAME, WINDOW_TITLE, WS_OVERLAPPEDWINDOW,
    WinX, WinY, WinW, WinH,
    0, 0, HInstance, nil);

  // Liga o handler de comandos do tray (Abrir / Fechar).
  OBSTray.OnTrayCommand := OnTrayCommandHandler;

  // /start-record: user apertou hotkey enquanto na hibernacao.
  // App sobe pra gravar sem mostrar UI. Reusamos o caminho "off-screen
  // pra WebView2 init + SW_HIDE depois" — mesma logica do antigo
  // StartInTray (que nao e mais ativado por /autostart).
  if StartInTray or FStartRecordRequested then
  begin
    // WebView2 nao inicializa corretamente quando o parent HWND nunca
    // foi mostrado durante o setup — rendering fica preto/quebrado.
    // Workaround:
    //  1) Adiciona WS_EX_TOOLWINDOW (some da taskbar/Alt+Tab durante init).
    //  2) Posiciona off-screen.
    //  3) SW_SHOWNOACTIVATE — WebView2 ve o parent visivel e inicializa OK.
    //  4) Apos a inicializacao do WebView2 terminar (TControllerHandler),
    //     daremos SW_HIDE — sinalizado por PendingHideAfterWebViewReady.
    //  5) Na 1a abertura via tray, OnTrayCommandHandler remove o
    //     WS_EX_TOOLWINDOW e reposiciona a janela no centro.
    WasMaximized := True; // por convencao restaura maximizado depois
    PendingRestoreBounds := Rect(WinX, WinY, WinX + WinW, WinY + WinH);
    PendingRestorePosition := True;
    PendingHideAfterWebViewReady := True;

    SetWindowLongPtr(Wnd, GWL_EXSTYLE,
      GetWindowLongPtr(Wnd, GWL_EXSTYLE) or WS_EX_TOOLWINDOW);
    SetWindowPos(Wnd, 0, -32000, -32000, WinW, WinH,
      SWP_NOZORDER or SWP_NOACTIVATE or SWP_FRAMECHANGED);
    ShowWindow(Wnd, SW_SHOWNOACTIVATE);
    UpdateWindow(Wnd);

    OBSTray.InstallTrayIcon(Wnd, WINDOW_TITLE);
  end
  else
  begin
    ShowWindow(Wnd, SW_SHOW);
    UpdateWindow(Wnd);
  end;

  // Acorda o GetMessage quando uma worker thread enfileira via
  // Synchronize/Queue. Sem isso o pump fica dormindo ate proxima
  // mensagem do Windows e a sincronizacao demora a propagar.
  Wakeup := TWakeMainThread.Create;
  Wakeup.Wnd := Wnd;
  WakeMainThread := Wakeup.Wake;
  try

  while GetMessage(Msg, 0, 0, 0) do
  begin
    TranslateMessage(Msg);
    DispatchMessage(Msg);
    // Drena requisicoes de TThread.Synchronize/Queue feitas por
    // worker threads (ex: DoInit em background). Sem isso a worker
    // fica eternamente bloqueada em Synchronize.
    CheckSynchronize;
  end;

  finally
    WakeMainThread := nil;
    Wakeup.Free;
  end;
  CoUninitialize;
end;

end.

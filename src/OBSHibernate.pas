(*
  OBSHibernate - modo "hibernação" do NoOBS.

  Esse e um modo de execucao alternativo do exe. Em vez do app full
  (WebView2 + libobs warmup + watchers + thumb thread + audio meters),
  carrega o MINIMO necessario pra:
    - mostrar um icone na bandeja com tooltip "NoOBS (hibernando)"
    - capturar a hotkey global de gravacao
    - aceitar clique do tray pra abrir o app

  Memoria residual: ~4-5 MB vs ~150 MB do full. Zero CPU/GPU idle.

  Quando o user clica no tray ou aperta a hotkey, esse processo:
    1. Libera o mutex de instancia unica + tray + hotkey.
    2. ShellExecute "NoOBS.exe" (com argumento opcional /start-record).
    3. PostQuitMessage + sai.
  O processo full sobe limpo, sem ter que reativar modulos.

  Entrada:
    NoOBS.exe /hibernate              ← entra direto em hibernacao
    NoOBS.exe /autostart              ← (em OBSUI.Run) re-spawna como /hibernate
                                         se closeToTray=true no config

  Saida (spawna full):
    NoOBS.exe                         ← abre janela (clique no tray)
    NoOBS.exe /start-record           ← inicia gravacao apos warmup (hotkey)

  Single-instance:
    Comparte MUTEX_NAME com o modo full. Se uma 2a instancia subir,
    o WM_SHOW_INSTANCE chega aqui e tratamos como "user quer abrir":
    spawna full e saimos. Efetivamente o launch da 2a instancia
    "promove" a hibernacao pra full.
*)
unit OBSHibernate;

interface

procedure Run;

implementation

uses
  Winapi.Windows, Winapi.ShellAPI, Winapi.Messages,
  System.SysUtils, System.Classes,
  OBSConfig, OBSHotkey, OBSLog, OBSSingleInstance;

const
  // MUTEX_NAME e SHOW_MSG_NAME vem de OBSSingleInstance — compartilhados
  // com OBSUI. ANTES estavam duplicados aqui ('TNoOBSWindow'), derivaram
  // de um nome antigo do OBSUI ('TNoOBSWindow' -> 'TNoOBS') e os dois
  // modos paravam de detectar um ao outro: mutex distinto + UINT da
  // window message distinto = full e hibernate rodavam simultaneamente.
  CLASS_NAME    = 'TNoOBSHibernate';
  TOOLTIP_TXT   = 'NoOBS (hibernando)';

  WM_TRAYICON           = WM_USER + 100;
  ID_TRAY_RECORD        = 9001;
  ID_TRAY_OPEN          = 9002;
  ID_TRAY_QUIT          = 9003;

  // IDs internos pro RegisterHotKey. Mesmos do modo full.
  HK_RECORD_TOGGLE      = 100;
  HK_RECORD_TOGGLE_ALT  = 101;

var
  MainWindow: HWND = 0;
  TrayIcon: TNotifyIconData;
  TrayAdded: Boolean = False;
  WM_SHOW_INSTANCE: UINT = 0;
  SingleInstanceMutex: THandle = 0;
  RecordHotkeyRegistered: Boolean = False;
  RecordHotkeyAltRegistered: Boolean = False;

function GetExePath: string;
var
  Buf: array[0..MAX_PATH - 1] of WideChar;
begin
  GetModuleFileNameW(0, Buf, MAX_PATH);
  Result := Buf;
end;

procedure InstallTrayIcon;
var
  TipLen: Integer;
begin
  ZeroMemory(@TrayIcon, SizeOf(TrayIcon));
  TrayIcon.cbSize           := SizeOf(TrayIcon);
  TrayIcon.Wnd              := MainWindow;
  TrayIcon.uID              := 1;
  TrayIcon.uFlags           := NIF_ICON or NIF_MESSAGE or NIF_TIP;
  TrayIcon.uCallbackMessage := WM_TRAYICON;
  TrayIcon.hIcon            := LoadIconW(HInstance, 'MAINICON');
  if TrayIcon.hIcon = 0 then
  begin
    Log('Hibernate: MAINICON nao encontrado, usando IDI_APPLICATION.');
    TrayIcon.hIcon := LoadIconW(0, IDI_APPLICATION);
  end;
  // szTip e WideChar[128]; copia com clamp.
  TipLen := Length(TOOLTIP_TXT);
  if TipLen > 127 then TipLen := 127;
  if TipLen > 0 then
    Move(PWideChar(TOOLTIP_TXT)^, TrayIcon.szTip[0], TipLen * SizeOf(WideChar));
  TrayIcon.szTip[TipLen] := #0;
  TrayAdded := Shell_NotifyIconW(NIM_ADD, @TrayIcon);
  if TrayAdded then
    Log('Hibernate: tray icon instalado.')
  else
    Log('Hibernate: Shell_NotifyIconW(NIM_ADD) FALHOU (LastError=%d).', [GetLastError]);
end;

procedure RemoveTrayIcon;
begin
  if not TrayAdded then Exit;
  Shell_NotifyIconW(NIM_DELETE, @TrayIcon);
  TrayAdded := False;
end;

procedure RegisterRecordHotkey;
// Le 'hotkey' do config.json e registra. Mesma logica do
// OBSBridge.ApplyHotkeyFromConfig, mas standalone (sem deps de Bridge).
const
  DEFAULT_HOTKEY = 'Pause';
var
  Spec: string;
  HK: THotkeySpec;
  Reason: string;
begin
  Spec := GetConfigStr('hotkey', DEFAULT_HOTKEY);
  if Spec.Trim = '' then
  begin
    Log('Hibernate: hotkey desativada (config vazia).');
    Exit;
  end;

  HK := ParseHotkey(Spec);
  if not HK.Valid then
  begin
    Log('Hibernate: hotkey spec invalido "%s".', [Spec]);
    Exit;
  end;
  if IsReservedHotkey(HK.Modifiers, HK.Vk, Reason) then
  begin
    Log('Hibernate: hotkey "%s" e reservada (%s).', [Spec, Reason]);
    Exit;
  end;

  RecordHotkeyRegistered := RegisterHotKey(MainWindow,
    HK_RECORD_TOGGLE, HK.Modifiers, HK.Vk);
  if RecordHotkeyRegistered then
    Log('Hibernate: hotkey "%s" registrada.', [Spec])
  else
    Log('Hibernate: RegisterHotKey falhou pra "%s" (outro app?).', [Spec]);

  // Pegadinha Ctrl+Pause -> VK_CANCEL (mesma logica de OBSBridge).
  if (HK.Vk = VK_PAUSE) and ((HK.Modifiers and MOD_CONTROL) <> 0) then
    RecordHotkeyAltRegistered := RegisterHotKey(MainWindow,
      HK_RECORD_TOGGLE_ALT, HK.Modifiers, VK_CANCEL);
end;

procedure UnregisterRecordHotkey;
begin
  if RecordHotkeyRegistered then
    UnregisterHotKey(MainWindow, HK_RECORD_TOGGLE);
  if RecordHotkeyAltRegistered then
    UnregisterHotKey(MainWindow, HK_RECORD_TOGGLE_ALT);
  RecordHotkeyRegistered := False;
  RecordHotkeyAltRegistered := False;
end;

procedure SpawnFullAndExit(const AArgs: string);
// Libera mutex + tray + hotkey, spawna o full e PostQuitMessage.
// Ordem importa:
//   1. Libera hotkey/tray (full vai re-registrar)
//   2. CloseHandle do mutex (full vai recriar) — ANTES do CreateProcess
//      pra que o full nao detecte ERROR_ALREADY_EXISTS
//   3. ShellExecute (fire-and-forget)
//   4. PostQuitMessage (sai do GetMessage loop)
var
  ExePath: string;
  HInst: THandle;
begin
  ExePath := GetExePath;
  Log('Hibernate: spawn full "%s" args="%s"', [ExePath, AArgs]);

  UnregisterRecordHotkey;
  RemoveTrayIcon;
  if SingleInstanceMutex <> 0 then
  begin
    CloseHandle(SingleInstanceMutex);
    SingleInstanceMutex := 0;
    Log('Hibernate: mutex liberado.');
  end;

  HInst := ShellExecuteW(0, nil, PWideChar(ExePath),
    PWideChar(AArgs), nil, SW_SHOWNORMAL);
  // ShellExecute retorna <=32 em caso de erro. Valores comuns:
  // ERROR_FILE_NOT_FOUND=2, ERROR_PATH_NOT_FOUND=3, SE_ERR_ACCESSDENIED=5.
  if NativeUInt(HInst) <= 32 then
    Log('Hibernate: ShellExecuteW FALHOU (codigo=%d, LastError=%d).',
      [NativeUInt(HInst), GetLastError])
  else
    Log('Hibernate: ShellExecuteW OK (HINST=%d).', [NativeUInt(HInst)]);

  PostQuitMessage(0);
end;

procedure ShowTrayMenu;
var
  Menu: HMENU;
  Pt: TPoint;
begin
  Menu := CreatePopupMenu;
  AppendMenuW(Menu, MF_STRING, ID_TRAY_RECORD, 'Iniciar gravação');
  AppendMenuW(Menu, MF_STRING, ID_TRAY_OPEN,   'Abrir');
  AppendMenuW(Menu, MF_SEPARATOR, 0, nil);
  AppendMenuW(Menu, MF_STRING, ID_TRAY_QUIT,   'Fechar');
  GetCursorPos(Pt);
  // Mesmo truque do OBSTray pro menu fechar quando clica fora.
  SetForegroundWindow(MainWindow);
  TrackPopupMenu(Menu, TPM_RIGHTBUTTON, Pt.X, Pt.Y, 0, MainWindow, nil);
  PostMessage(MainWindow, WM_NULL, 0, 0);
  DestroyMenu(Menu);
end;

function WindowProc(Hwnd: HWND; Msg: UINT;
  WParam: WPARAM; LParam: LPARAM): LRESULT; stdcall;
begin
  // Outra instancia tentou subir — promove a hibernacao pra full.
  if (WM_SHOW_INSTANCE <> 0) and (Msg = WM_SHOW_INSTANCE) then
  begin
    Log('Hibernate: WM_SHOW_INSTANCE recebido — promovendo a full.');
    SpawnFullAndExit('');
    Exit(0);
  end;

  case Msg of
    WM_HOTKEY:
      begin
        Log('Hibernate: WM_HOTKEY id=%d', [Integer(WParam)]);
        if (WParam = HK_RECORD_TOGGLE) or (WParam = HK_RECORD_TOGGLE_ALT) then
        begin
          SpawnFullAndExit('/start-record');
          Exit(0);
        end;
      end;

    WM_TRAYICON:
      case LParam of
        WM_LBUTTONUP, WM_LBUTTONDBLCLK:
          begin
            Log('Hibernate: tray clique esquerdo — abrindo full.');
            SpawnFullAndExit('');
            Exit(0);
          end;
        WM_RBUTTONUP:
          ShowTrayMenu;
      end;

    WM_COMMAND:
      begin
        Log('Hibernate: WM_COMMAND id=%d', [LOWORD(WParam)]);
        case LOWORD(WParam) of
          ID_TRAY_RECORD: begin SpawnFullAndExit('/start-record'); Exit(0); end;
          ID_TRAY_OPEN:   begin SpawnFullAndExit('');               Exit(0); end;
          ID_TRAY_QUIT:   DestroyWindow(Hwnd);
        end;
      end;

    WM_DESTROY:
      begin
        Log('Hibernate: WM_DESTROY.');
        UnregisterRecordHotkey;
        RemoveTrayIcon;
        PostQuitMessage(0);
        Exit(0);
      end;
  end;

  Result := DefWindowProc(Hwnd, Msg, WParam, LParam);
end;

procedure Run;
var
  Msg: TMsg;
  wc: WNDCLASS;
  AtomResult: ATOM;
begin
  Log('Hibernate: modo minimo iniciando.');

  // Single-instance — usa o MESMO mutex que o full. Se outra instancia
  // (hibernate ou full) ja esta rodando, saimos silenciosamente.
  WM_SHOW_INSTANCE := RegisterWindowMessage(SHOW_MSG_NAME);
  Log('Hibernate: WM_SHOW_INSTANCE=%d', [WM_SHOW_INSTANCE]);
  SingleInstanceMutex := CreateMutex(nil, False, MUTEX_NAME);
  if GetLastError = ERROR_ALREADY_EXISTS then
  begin
    Log('Hibernate: outra instancia ja roda — saindo.');
    if SingleInstanceMutex <> 0 then CloseHandle(SingleInstanceMutex);
    Exit;
  end;
  Log('Hibernate: mutex "%s" adquirido.', [MUTEX_NAME]);

  // Janela invisivel pra hospedar tray + hotkey. WS_POPUP sem
  // WS_VISIBLE = janela existe mas nao aparece. WS_EX_TOOLWINDOW pra
  // garantir que nao apareca em Alt+Tab caso algo a torne visivel
  // por engano.
  ZeroMemory(@wc, SizeOf(wc));
  wc.lpfnWndProc   := @WindowProc;
  wc.hInstance     := HInstance;
  wc.hIcon         := LoadIcon(HInstance, 'MAINICON');
  wc.hCursor       := LoadCursor(0, IDC_ARROW);
  wc.lpszClassName := CLASS_NAME;
  AtomResult := Winapi.Windows.RegisterClass(wc);
  if AtomResult = 0 then
    Log('Hibernate: RegisterClass falhou (LastError=%d).', [GetLastError])
  else
    Log('Hibernate: RegisterClass OK (atom=%d).', [AtomResult]);

  MainWindow := CreateWindowEx(WS_EX_TOOLWINDOW, CLASS_NAME, 'NoOBS',
    WS_POPUP, 0, 0, 0, 0, 0, 0, HInstance, nil);
  if MainWindow = 0 then
  begin
    Log('Hibernate: CreateWindowEx FALHOU (LastError=%d).', [GetLastError]);
    if SingleInstanceMutex <> 0 then CloseHandle(SingleInstanceMutex);
    Exit;
  end;
  Log('Hibernate: janela criada (HWND=%d).', [MainWindow]);

  InstallTrayIcon;
  RegisterRecordHotkey;

  Log('Hibernate: entrando no GetMessage loop.');
  while GetMessage(Msg, 0, 0, 0) do
  begin
    TranslateMessage(Msg);
    DispatchMessage(Msg);
  end;
  Log('Hibernate: GetMessage loop saiu (WM_QUIT recebido).');

  if SingleInstanceMutex <> 0 then
    CloseHandle(SingleInstanceMutex);
  Log('Hibernate: Run encerrando.');
end;

end.

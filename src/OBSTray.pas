(*
  OBSTray - icone na bandeja do sistema.

  - Mostra icone proximo ao relogio quando ativado
  - Menu de contexto (clique direito): Abrir / Iniciar com Windows / Fechar
  - Clique esquerdo / duplo clique: restaura a janela
  - WM_CLOSE da janela principal e interceptada pra minimizar pra
    bandeja em vez de fechar (a menos que o user use o "Fechar" do menu).

  Integra com OBSUI via callbacks OnTrayShowWindow / OnTrayQuit.
*)
unit OBSTray;

interface

uses
  Winapi.Windows, Winapi.ShellApi, Winapi.Messages;

const
  WM_TRAYICON           = WM_USER + 100;
  ID_TRAY_SHOW          = 9001;
  ID_TRAY_AUTOSTART     = 9002;
  ID_TRAY_QUIT          = 9003;
  ID_TRAY_TOGGLE_RECORD = 9004;

type
  TTrayCommandProc = procedure(ACommand: Integer);
  TToggleRecordProc = procedure;
  TIsRecordingFunc  = function: Boolean;

var
  OnTrayCommand:    TTrayCommandProc  = nil;
  // Callbacks pro item de gravacao do menu — registrados pelo OBSBridge.
  // Se nao registrados, o item simplesmente nao aparece no menu.
  OnToggleRecord:   TToggleRecordProc = nil;
  OnIsRecording:    TIsRecordingFunc  = nil;

// Instala icone na bandeja. Wnd recebe WM_TRAYICON.
procedure InstallTrayIcon(AWnd: HWND; const ATooltip: string);
procedure RemoveTrayIcon;
function IsTrayInstalled: Boolean;

// Mostra um balloon tip (NIF_INFO) no icone da bandeja. Fallback pra
// notificacao do Windows quando a UI HTML nao esta pronta (caso
// classico: /start-record vindo da hibernacao — gravacao comeca antes
// do WebView2 ter terminado de inicializar). Falha silenciosa se o
// icone ainda nao esta instalado.
procedure ShowBalloon(const ATitle, AMessage: string);

// Troca o icone da bandeja entre normal e "gravando" (icone padrao com
// uma bolinha vermelha sobreposta). Idempotente; chama varias vezes
// com o mesmo valor sao no-op apos a primeira.
procedure SetTrayRecording(AOn: Boolean);

// Chamado pela window proc quando recebe WM_TRAYICON.
procedure HandleTrayMessage(AWnd: HWND; AParam: LPARAM);

// Chamado pela window proc quando recebe WM_COMMAND com IDs >= 9000.
procedure HandleTrayCommand(AWnd: HWND; ACommandId: Integer);

implementation

uses
  System.SysUtils, OBSAutostart, OBSRecordIcon, OBSLang;

var
  TrayIcon: TNotifyIconData;
  TrayAdded: Boolean = False;
  // Caches dos icones — gerados lazy na primeira chamada de
  // SetTrayRecording, destruidos no final do processo (Windows limpa
  // junto com o handle table).
  BaseTrayIcon:      HICON = 0;
  RecordingTrayIcon: HICON = 0;
  TrayRecordingNow:  Boolean = False;

function IsTrayInstalled: Boolean;
begin
  Result := TrayAdded;
end;

procedure InstallTrayIcon(AWnd: HWND; const ATooltip: string);
var
  TipBytes: Integer;
begin
  if TrayAdded then Exit;
  ZeroMemory(@TrayIcon, SizeOf(TrayIcon));
  TrayIcon.cbSize := SizeOf(TrayIcon);
  TrayIcon.Wnd := AWnd;
  TrayIcon.uID := 1;
  TrayIcon.uFlags := NIF_ICON or NIF_MESSAGE or NIF_TIP;
  TrayIcon.uCallbackMessage := WM_TRAYICON;
  TrayIcon.hIcon := LoadIconW(HInstance, 'MAINICON');
  if TrayIcon.hIcon = 0 then
    TrayIcon.hIcon := LoadIconW(0, IDI_APPLICATION);
  // szTip e WideChar[128]; copia com clamp.
  TipBytes := Length(ATooltip);
  if TipBytes > 127 then TipBytes := 127;
  if TipBytes > 0 then
    Move(PWideChar(ATooltip)^, TrayIcon.szTip[0], TipBytes * SizeOf(WideChar));
  TrayIcon.szTip[TipBytes] := #0;
  TrayAdded := Shell_NotifyIconW(NIM_ADD, @TrayIcon);
end;

procedure RemoveTrayIcon;
begin
  if not TrayAdded then Exit;
  Shell_NotifyIconW(NIM_DELETE, @TrayIcon);
  TrayAdded := False;
end;

procedure ShowBalloon(const ATitle, AMessage: string);
// Reusa o icone instalado: faz NIM_MODIFY com NIF_INFO setado e os
// campos szInfo/szInfoTitle preenchidos. Windows mostra balloon (W10)
// ou toast nativo "wrapped" (W11). NIIF_USER + dwInfoFlags tenta usar
// o icone do tray como icone do balloon.
var
  Data: TNotifyIconData;
  TLen, MLen: Integer;
begin
  if not TrayAdded then Exit;
  ZeroMemory(@Data, SizeOf(Data));
  Data.cbSize := SizeOf(Data);
  Data.Wnd    := TrayIcon.Wnd;
  Data.uID    := TrayIcon.uID;
  Data.uFlags := NIF_INFO;

  TLen := Length(ATitle);
  if TLen > 63 then TLen := 63;
  if TLen > 0 then
    Move(PWideChar(ATitle)^, Data.szInfoTitle[0], TLen * SizeOf(WideChar));
  Data.szInfoTitle[TLen] := #0;

  MLen := Length(AMessage);
  if MLen > 255 then MLen := 255;
  if MLen > 0 then
    Move(PWideChar(AMessage)^, Data.szInfo[0], MLen * SizeOf(WideChar));
  Data.szInfo[MLen] := #0;

  Data.dwInfoFlags := NIIF_USER or NIIF_LARGE_ICON;
  Shell_NotifyIconW(NIM_MODIFY, @Data);
end;

procedure SetTrayRecording(AOn: Boolean);
var
  Data: TNotifyIconData;
  Size: Integer;
begin
  if not TrayAdded then Exit;
  if AOn = TrayRecordingNow then Exit;
  TrayRecordingNow := AOn;

  // Gera (uma vez) os dois icones cache:
  //   BaseTrayIcon       — copia do MAINICON (pra restaurar depois)
  //   RecordingTrayIcon  — MAINICON + bolinha vermelha sobreposta
  if BaseTrayIcon = 0 then
    BaseTrayIcon := LoadIconW(HInstance, 'MAINICON');
  if BaseTrayIcon = 0 then
    BaseTrayIcon := LoadIconW(0, IDI_APPLICATION);
  if RecordingTrayIcon = 0 then
  begin
    Size := GetSystemMetrics(SM_CXSMICON);
    if Size < 16 then Size := 16;
    RecordingTrayIcon := CreateRecordingOverlayIcon(BaseTrayIcon, Size);
  end;

  ZeroMemory(@Data, SizeOf(Data));
  Data.cbSize := SizeOf(Data);
  Data.Wnd    := TrayIcon.Wnd;
  Data.uID    := TrayIcon.uID;
  Data.uFlags := NIF_ICON;
  if AOn and (RecordingTrayIcon <> 0) then
    Data.hIcon := RecordingTrayIcon
  else
    Data.hIcon := BaseTrayIcon;
  Shell_NotifyIconW(NIM_MODIFY, @Data);
end;

procedure ShowTrayMenu(AWnd: HWND);
var
  Menu: HMENU;
  Pt: TPoint;
  AutoFlags: UINT;
  Recording: Boolean;
begin
  Menu := CreatePopupMenu;

  // Item de gravacao (so se OBSBridge registrou os callbacks). Label
  // varia conforme o estado atual.
  if Assigned(OnToggleRecord) and Assigned(OnIsRecording) then
  begin
    Recording := False;
    try Recording := OnIsRecording; except end;
    if Recording then
      AppendMenuW(Menu, MF_STRING, ID_TRAY_TOGGLE_RECORD, PWideChar(OBSLang.T('record.stop')))
    else
      AppendMenuW(Menu, MF_STRING, ID_TRAY_TOGGLE_RECORD, PWideChar(OBSLang.T('record.start')));
    AppendMenuW(Menu, MF_SEPARATOR, 0, nil);
  end;

  AppendMenuW(Menu, MF_STRING, ID_TRAY_SHOW, PWideChar(OBSLang.T('common.open')));

  AutoFlags := MF_STRING;
  if IsAutoStartEnabled then
    AutoFlags := AutoFlags or MF_CHECKED;
  AppendMenuW(Menu, AutoFlags, ID_TRAY_AUTOSTART, PWideChar(OBSLang.T('settings.autostart.label')));

  AppendMenuW(Menu, MF_SEPARATOR, 0, nil);
  AppendMenuW(Menu, MF_STRING, ID_TRAY_QUIT, PWideChar(OBSLang.T('header.close')));

  GetCursorPos(Pt);
  // SetForegroundWindow + WM_NULL post-track e o "truque" classico pra
  // o menu fechar quando o user clicar fora.
  SetForegroundWindow(AWnd);
  TrackPopupMenu(Menu, TPM_RIGHTBUTTON, Pt.X, Pt.Y, 0, AWnd, nil);
  PostMessage(AWnd, WM_NULL, 0, 0);
  DestroyMenu(Menu);
end;

procedure HandleTrayMessage(AWnd: HWND; AParam: LPARAM);
begin
  case AParam of
    WM_LBUTTONUP, WM_LBUTTONDBLCLK:
      if Assigned(OnTrayCommand) then OnTrayCommand(ID_TRAY_SHOW);
    WM_RBUTTONUP:
      ShowTrayMenu(AWnd);
  end;
end;

procedure HandleTrayCommand(AWnd: HWND; ACommandId: Integer);
begin
  case ACommandId of
    ID_TRAY_SHOW:
      if Assigned(OnTrayCommand) then OnTrayCommand(ID_TRAY_SHOW);
    ID_TRAY_AUTOSTART:
      SetAutoStart(not IsAutoStartEnabled);
    ID_TRAY_TOGGLE_RECORD:
      if Assigned(OnToggleRecord) then OnToggleRecord;
    ID_TRAY_QUIT:
      if Assigned(OnTrayCommand) then OnTrayCommand(ID_TRAY_QUIT);
  end;
end;

end.

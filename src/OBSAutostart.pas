(*
  OBSAutostart - controla a entrada do NoOBS no auto-start do Windows
  via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.

  Comando registrado: '"<exe>" /autostart'.
  O flag "/autostart" e apenas um MARCADOR de origem (= "fui lancado
  pelo logon do Windows"), nao dita comportamento. Quem decide se vai
  pra bandeja ou abre visivel e o app em runtime, lendo o config
  'closeToTray'. Manual launches (Start Menu, atalho, etc) NAO tem
  o flag e sempre abrem com janela visivel.
*)
unit OBSAutostart;

interface

function IsAutoStartEnabled: Boolean;
procedure SetAutoStart(AEnable: Boolean);

implementation

uses
  Winapi.Windows, System.SysUtils;

const
  AUTORUN_KEY   = 'Software\Microsoft\Windows\CurrentVersion\Run';
  // Win10/11: Task Manager > Startup salva o status enabled/disabled
  // aqui em paralelo a Run. Se a entrada existe em Run mas esta marcada
  // como disabled aqui, o Windows NAO roda no logon mesmo assim. Pra
  // ser fonte de verdade real, checamos os dois.
  APPROVED_KEY  = 'Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run';
  REG_VALUE     = 'NoOBS';

  // StartupApproved value layout: 12 bytes
  //   [0..3]  DWORD status  (02 = enabled, 03 = disabled)
  //   [4..11] FILETIME      (timestamp da ultima mudanca — qualquer
  //                          valor serve, Windows nao valida)
  APPROVED_ENABLED  = $00000002;
  APPROVED_DISABLED = $00000003;

function GetExePath: string;
var
  Buf: array[0..MAX_PATH - 1] of WideChar;
begin
  GetModuleFileNameW(0, Buf, MAX_PATH);
  Result := string(PWideChar(@Buf[0]));
end;

// True se NoOBS esta presente em Run E nao foi marcado como disabled
// pelo Task Manager / Settings > Startup do Windows.
function IsAutoStartEnabled: Boolean;
var
  Key: HKEY;
  HasRunEntry: Boolean;
  ApprovedData: array[0..11] of Byte;
  ApprovedSize: DWORD;
  Status: DWORD;
begin
  Result := False;
  HasRunEntry := False;

  // 1) Tem entrada em Run?
  if RegOpenKeyExW(HKEY_CURRENT_USER, AUTORUN_KEY, 0,
    KEY_QUERY_VALUE, Key) = ERROR_SUCCESS then
  begin
    HasRunEntry := RegQueryValueExW(Key, REG_VALUE, nil, nil, nil, nil)
      = ERROR_SUCCESS;
    RegCloseKey(Key);
  end;
  if not HasRunEntry then Exit;

  // 2) StartupApproved diz que esta enabled? Se a chave nao existir
  //    ou nao tiver a entrada, o default e "enabled" (Windows so cria
  //    quando o usuario explicitamente disabilita pela UI).
  Result := True;  // assume enabled ate provar o contrario
  if RegOpenKeyExW(HKEY_CURRENT_USER, APPROVED_KEY, 0,
    KEY_QUERY_VALUE, Key) = ERROR_SUCCESS then
  begin
    ApprovedSize := SizeOf(ApprovedData);
    if RegQueryValueExW(Key, REG_VALUE, nil, nil,
        @ApprovedData[0], @ApprovedSize) = ERROR_SUCCESS then
    begin
      if ApprovedSize >= 4 then
      begin
        Status := PDWORD(@ApprovedData[0])^;
        // Status 0x03 (e variantes com bit alto set, ex: 0x83) = disabled.
        // Status 0x02 = enabled. Trata qualquer coisa != enabled como
        // disabled pra ser conservador.
        if Status <> APPROVED_ENABLED then Result := False;
      end;
    end;
    RegCloseKey(Key);
  end;
end;

procedure SetAutoStart(AEnable: Boolean);
var
  Key: HKEY;
  Value: string;
  ApprovedData: array[0..11] of Byte;
  Now: TFileTime;
begin
  // ---- Escreve/remove entrada em Run ----
  if RegOpenKeyExW(HKEY_CURRENT_USER, AUTORUN_KEY, 0,
    KEY_SET_VALUE, Key) = ERROR_SUCCESS then
  begin
    try
      if AEnable then
      begin
        // Aspas no path pra suportar instalacao com espacos.
        // /autostart = marcador "fui lancado pelo logon" — comportamento
        // (tray vs visivel) e decidido pelo app em runtime via config.
        Value := '"' + GetExePath + '" /autostart';
        RegSetValueExW(Key, REG_VALUE, 0, REG_SZ, PWideChar(Value),
          (Length(Value) + 1) * SizeOf(WideChar));
      end
      else
        RegDeleteValueW(Key, REG_VALUE);
    finally
      RegCloseKey(Key);
    end;
  end;

  // ---- Sincroniza StartupApproved ----
  // Caso classico: user desabilitou pelo Task Manager antes; entrada
  // de Run existe mas StartupApproved marca disabled. Se o user agora
  // ativa pelo NoOBS, precisamos limpar essa flag — senao Windows
  // continua ignorando no logon.
  //
  // Pra disable: nao precisamos tocar (a remocao do Run ja basta), mas
  // por defensividade tambem marcamos.
  if RegOpenKeyExW(HKEY_CURRENT_USER, APPROVED_KEY, 0,
    KEY_SET_VALUE or KEY_QUERY_VALUE, Key) = ERROR_SUCCESS then
  begin
    try
      FillChar(ApprovedData, SizeOf(ApprovedData), 0);
      if AEnable then
        PDWORD(@ApprovedData[0])^ := APPROVED_ENABLED
      else
        PDWORD(@ApprovedData[0])^ := APPROVED_DISABLED;
      // FILETIME atual nos bytes 4..11 — Windows aceita zero, mas
      // valor real e mais educado.
      GetSystemTimeAsFileTime(Now);
      Move(Now, ApprovedData[4], SizeOf(TFileTime));
      RegSetValueExW(Key, REG_VALUE, 0, REG_BINARY,
        @ApprovedData[0], SizeOf(ApprovedData));
    finally
      RegCloseKey(Key);
    end;
  end;
end;

end.

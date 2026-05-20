(*
  OBSAutostart - controla a entrada do NoOBS no auto-start do Windows
  via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.

  Quando habilitado, o registry guarda o path do exe + flag "/tray" pra
  que ao logar o user a janela suba minimizada na bandeja.
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
  REG_VALUE     = 'NoOBS';

function GetExePath: string;
var
  Buf: array[0..MAX_PATH - 1] of WideChar;
begin
  GetModuleFileNameW(0, Buf, MAX_PATH);
  Result := string(PWideChar(@Buf[0]));
end;

function IsAutoStartEnabled: Boolean;
var
  Key: HKEY;
begin
  Result := False;
  if RegOpenKeyExW(HKEY_CURRENT_USER, AUTORUN_KEY, 0,
    KEY_QUERY_VALUE, Key) = ERROR_SUCCESS then
  begin
    Result := RegQueryValueExW(Key, REG_VALUE, nil, nil, nil, nil) = ERROR_SUCCESS;
    RegCloseKey(Key);
  end;
end;

procedure SetAutoStart(AEnable: Boolean);
var
  Key: HKEY;
  Value: string;
begin
  if RegOpenKeyExW(HKEY_CURRENT_USER, AUTORUN_KEY, 0,
    KEY_SET_VALUE, Key) <> ERROR_SUCCESS then Exit;
  try
    if AEnable then
    begin
      // Aspas no path pra suportar instalacao com espacos.
      // /tray = inicia minimizado na bandeja (sem janela visivel).
      Value := '"' + GetExePath + '" /tray';
      RegSetValueExW(Key, REG_VALUE, 0, REG_SZ, PWideChar(Value),
        (Length(Value) + 1) * SizeOf(WideChar));
    end
    else
      RegDeleteValueW(Key, REG_VALUE);
  finally
    RegCloseKey(Key);
  end;
end;

end.

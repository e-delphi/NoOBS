(*
  OBSElevate - "Abrir como administrador" (config runAsAdmin).

  Por que existe: jogo que roda como ADMINISTRADOR (o GTA V) esconde o
  teclado de todo programa que nao e — o UIPI do Windows. Com ele na
  frente, o atalho de gravar do NoOBS nao chega por nenhum metodo
  (pegadinha #33). Rodando o NoOBS elevado, o RegisterHotKey de sempre
  chega na hora.

  Como: o dispatcher do .dpr chama RelaunchElevatedIfWanted ANTES de
  qualquer outra coisa (mutex de instancia unica, janela, libobs). Com a
  opcao ligada e o processo ainda sem elevacao, ele se reabre pelo verbo
  "runas" (o UAC pergunta) com os MESMOS argumentos e sai. Se o UAC for
  recusado, ou a conta nao puder elevar, o app segue NORMAL, sem elevacao
  — a opcao e um pedido, nunca um bloqueio.

  Filhos de um processo elevado nascem elevados: o vai-e-volta
  full <-> hibernacao (ShellExecute sem "runas") mantem a elevacao sem
  perguntar de novo.

  Corolario do mesmo UIPI, ao contrario: processo elevado NAO recebe
  mensagem de janela acima de WM_USER vinda de quem nao e — e isso inclui
  o Explorer (clique no icone da bandeja, "TaskbarCreated") e uma segunda
  instancia comum avisando a primeira (WM_SHOW_INSTANCE). AllowFromLowerIL
  libera essas mensagens na janela.
*)
unit OBSElevate;

interface

uses
  Winapi.Windows;

// True = o processo atual roda elevado (token de administrador).
function IsProcessElevated: Boolean;

// Com runAsAdmin ligado e o processo sem elevacao, reabre elevado e
// devolve True (quem chamou deve sair na hora). False = seguir normal:
// opcao desligada, ja elevado, ou elevacao recusada/impossivel.
function RelaunchElevatedIfWanted: Boolean;

// Deixa a janela receber AMsgs de processos sem elevacao (no-op se o
// processo nao esta elevado).
procedure AllowFromLowerIL(AWnd: HWND; const AMsgs: array of UINT);

implementation

uses
  Winapi.ShellAPI, System.SysUtils, OBSConfig, OBSLog;

const
  TOKEN_ELEVATION_CLASS = 20;   // TokenElevation
  MSGFLT_ALLOW = 1;

function ChangeWindowMessageFilterEx(hWnd: HWND; message: UINT; action: DWORD;
  pChangeFilterStruct: Pointer): BOOL; stdcall; external user32;

function IsProcessElevated: Boolean;
var
  Tok: THandle;
  Elev, Len: DWORD;
begin
  Result := False;
  if not OpenProcessToken(GetCurrentProcess, TOKEN_QUERY, Tok) then Exit;
  try
    Elev := 0;
    if GetTokenInformation(Tok, TTokenInformationClass(TOKEN_ELEVATION_CLASS),
         @Elev, SizeOf(Elev), Len) then
      Result := Elev <> 0;
  finally
    CloseHandle(Tok);
  end;
end;

// Argumentos originais, sem o exe, requotados (ParamStr tira as aspas).
function OriginalArgs: string;
var
  i: Integer;
  S: string;
begin
  Result := '';
  for i := 1 to ParamCount do
  begin
    S := ParamStr(i);
    if Pos(' ', S) > 0 then S := '"' + S + '"';
    if Result <> '' then Result := Result + ' ';
    Result := Result + S;
  end;
end;

function RelaunchElevatedIfWanted: Boolean;
var
  Info: TShellExecuteInfo;
  Exe, Args: string;
begin
  Result := False;
  if not GetConfigBool('runAsAdmin', False) then Exit;
  if IsProcessElevated then
  begin
    Log('Elevate: rodando como administrador.');
    Exit;
  end;
  Exe := ParamStr(0);
  Args := OriginalArgs;
  FillChar(Info, SizeOf(Info), 0);
  Info.cbSize := SizeOf(Info);
  Info.fMask := SEE_MASK_NOASYNC;
  Info.lpVerb := 'runas';
  Info.lpFile := PChar(Exe);
  Info.lpParameters := PChar(Args);
  Info.nShow := SW_SHOWNORMAL;
  if ShellExecuteEx(@Info) then
  begin
    Log('Elevate: reaberto como administrador (args="%s"); este processo sai.',
      [Args]);
    Result := True;
  end
  else
    // 1223 = ERROR_CANCELLED (o usuario disse nao no UAC).
    Log('Elevate: nao foi possivel elevar (erro %d) — segue sem administrador.',
      [GetLastError]);
end;

procedure AllowFromLowerIL(AWnd: HWND; const AMsgs: array of UINT);
var
  M: UINT;
begin
  if (AWnd = 0) or not IsProcessElevated then Exit;
  for M in AMsgs do
    if M <> 0 then
      ChangeWindowMessageFilterEx(AWnd, M, MSGFLT_ALLOW, nil);
end;

end.

(*
  OBSHotkey - parsing e formatacao de combinacoes de tecla pra usar
  com Win32 RegisterHotKey.

  Formato textual: tokens separados por '+', case-insensitive.
    Modificadores: Ctrl, Shift, Alt, Win
    Tecla: F1..F24, A..Z, 0..9, Space, Enter, Tab, Esc, Insert, Delete,
           Home, End, PageUp, PageDown, Arrows (Left/Right/Up/Down),
           Numpad0..Numpad9, NumpadAdd, NumpadSubtract, NumpadMultiply,
           NumpadDivide, NumpadDecimal, Backquote, etc.

  Exemplo: "Ctrl+Shift+F9" -> Modifiers=MOD_CONTROL or MOD_SHIFT, Vk=VK_F9

  Vazio ('') = nenhum atalho registrado.
*)
unit OBSHotkey;

interface

uses
  Winapi.Windows;

type
  THotkeySpec = record
    Valid: Boolean;
    Modifiers: UINT;  // MOD_CONTROL/SHIFT/ALT/WIN bitmask
    Vk: UINT;         // virtual-key code
  end;

// Parseia "Ctrl+Shift+F9" pra THotkeySpec. Result.Valid = False se
// nao reconheceu modificador ou tecla, ou se string vazia.
function ParseHotkey(const ASpec: string): THotkeySpec;

// Inverso de ParseHotkey: formata pra string canonica.
function FormatHotkey(AModifiers, AVk: UINT): string;

// Helper: dado um VK + estado dos modificadores (do JS keydown event),
// formata canonicamente. Usado na captura.
function FormatHotkeyFromVk(AVk: UINT;
  ACtrl, AShift, AAlt, AWin: Boolean): string;

// Retorna True se a combinacao e reservada pelo Windows (RegisterHotKey
// nunca dispara) e preenche AReason com a explicacao. Usado como safety
// net — a UI ja bloqueia antes de mandar, mas isso pega edicao manual de
// config.json e combinacoes que escapem.
function IsReservedHotkey(AModifiers, AVk: UINT; out AReason: string): Boolean;

implementation

uses
  System.SysUtils, System.StrUtils;

function VkToName(AVk: UINT): string;
begin
  case AVk of
    VK_F1..VK_F24:    Result := 'F' + IntToStr(AVk - VK_F1 + 1);
    Ord('0')..Ord('9'),
    Ord('A')..Ord('Z'): Result := Chr(AVk);
    VK_SPACE:    Result := 'Space';
    VK_RETURN:   Result := 'Enter';
    VK_TAB:      Result := 'Tab';
    VK_ESCAPE:   Result := 'Esc';
    VK_INSERT:   Result := 'Insert';
    VK_DELETE:   Result := 'Delete';
    VK_HOME:     Result := 'Home';
    VK_END:      Result := 'End';
    VK_PRIOR:    Result := 'PageUp';
    VK_NEXT:     Result := 'PageDown';
    VK_LEFT:     Result := 'Left';
    VK_RIGHT:    Result := 'Right';
    VK_UP:       Result := 'Up';
    VK_DOWN:     Result := 'Down';
    VK_BACK:     Result := 'Backspace';
    VK_PAUSE:    Result := 'Pause';
    VK_NUMPAD0..VK_NUMPAD9:
      Result := 'Numpad' + IntToStr(AVk - VK_NUMPAD0);
    VK_ADD:      Result := 'NumpadAdd';
    VK_SUBTRACT: Result := 'NumpadSubtract';
    VK_MULTIPLY: Result := 'NumpadMultiply';
    VK_DIVIDE:   Result := 'NumpadDivide';
    VK_DECIMAL:  Result := 'NumpadDecimal';
    VK_OEM_MINUS:  Result := '-';
    VK_OEM_PLUS:   Result := '=';
    VK_OEM_COMMA:  Result := ',';
    VK_OEM_PERIOD: Result := '.';
    VK_OEM_1:      Result := ';';
    VK_OEM_2:      Result := '/';
    VK_OEM_3:      Result := '`';
    VK_OEM_4:      Result := '[';
    VK_OEM_5:      Result := '\';
    VK_OEM_6:      Result := ']';
    VK_OEM_7:      Result := '''';
  else
    Result := '';  // tecla nao mapeada
  end;
end;

function NameToVk(const AName: string): UINT;
var
  S: string;
  N: Integer;
begin
  Result := 0;
  S := AName.Trim;
  if S = '' then Exit;

  // Letras / digitos isolados.
  if Length(S) = 1 then
  begin
    Result := UINT(Ord(UpCase(S[1])));
    Exit;
  end;

  // F1..F24
  if (Length(S) >= 2) and (UpperCase(S[1]) = 'F') and
     TryStrToInt(Copy(S, 2, MaxInt), N) and (N >= 1) and (N <= 24) then
  begin
    Result := VK_F1 + UINT(N - 1);
    Exit;
  end;

  // Numpad0..9
  if SameText(Copy(S, 1, 6), 'Numpad') and (Length(S) = 7) and
     TryStrToInt(Copy(S, 7, 1), N) and (N >= 0) and (N <= 9) then
  begin
    Result := VK_NUMPAD0 + UINT(N);
    Exit;
  end;

  // Nomes especiais.
  S := UpperCase(S);
  if S = 'SPACE'     then Result := VK_SPACE
  else if S = 'ENTER'    then Result := VK_RETURN
  else if S = 'TAB'      then Result := VK_TAB
  else if S = 'ESC'      then Result := VK_ESCAPE
  else if S = 'ESCAPE'   then Result := VK_ESCAPE
  else if S = 'INSERT'   then Result := VK_INSERT
  else if S = 'DELETE'   then Result := VK_DELETE
  else if S = 'HOME'     then Result := VK_HOME
  else if S = 'END'      then Result := VK_END
  else if S = 'PAGEUP'   then Result := VK_PRIOR
  else if S = 'PAGEDOWN' then Result := VK_NEXT
  else if S = 'LEFT'     then Result := VK_LEFT
  else if S = 'RIGHT'    then Result := VK_RIGHT
  else if S = 'UP'       then Result := VK_UP
  else if S = 'DOWN'     then Result := VK_DOWN
  else if S = 'BACKSPACE' then Result := VK_BACK
  else if S = 'PAUSE'    then Result := VK_PAUSE
  else if S = 'BREAK'    then Result := VK_PAUSE
  else if S = 'NUMPADADD' then Result := VK_ADD
  else if S = 'NUMPADSUBTRACT' then Result := VK_SUBTRACT
  else if S = 'NUMPADMULTIPLY' then Result := VK_MULTIPLY
  else if S = 'NUMPADDIVIDE'   then Result := VK_DIVIDE
  else if S = 'NUMPADDECIMAL'  then Result := VK_DECIMAL;
end;

function ParseHotkey(const ASpec: string): THotkeySpec;
var
  Tokens: TArray<string>;
  Tok: string;
  i: Integer;
  KeyTok: string;
begin
  Result.Valid := False;
  Result.Modifiers := 0;
  Result.Vk := 0;
  if ASpec.Trim = '' then Exit;

  Tokens := ASpec.Split(['+']);
  KeyTok := '';
  for i := 0 to High(Tokens) do
  begin
    Tok := Tokens[i].Trim.ToUpper;
    if Tok = '' then Continue;
    if Tok = 'CTRL'    then Result.Modifiers := Result.Modifiers or MOD_CONTROL
    else if Tok = 'CONTROL' then Result.Modifiers := Result.Modifiers or MOD_CONTROL
    else if Tok = 'SHIFT'   then Result.Modifiers := Result.Modifiers or MOD_SHIFT
    else if Tok = 'ALT'     then Result.Modifiers := Result.Modifiers or MOD_ALT
    else if Tok = 'WIN'     then Result.Modifiers := Result.Modifiers or MOD_WIN
    else
      KeyTok := Tokens[i].Trim;  // tecla principal (preserva case original)
  end;

  if KeyTok = '' then Exit;
  Result.Vk := NameToVk(KeyTok);
  Result.Valid := Result.Vk <> 0;
end;

function FormatHotkey(AModifiers, AVk: UINT): string;
var
  Parts: TArray<string>;
  KeyName: string;
begin
  Result := '';
  SetLength(Parts, 0);
  if (AModifiers and MOD_CONTROL) <> 0 then
  begin SetLength(Parts, Length(Parts) + 1); Parts[High(Parts)] := 'Ctrl'; end;
  if (AModifiers and MOD_SHIFT) <> 0 then
  begin SetLength(Parts, Length(Parts) + 1); Parts[High(Parts)] := 'Shift'; end;
  if (AModifiers and MOD_ALT) <> 0 then
  begin SetLength(Parts, Length(Parts) + 1); Parts[High(Parts)] := 'Alt'; end;
  if (AModifiers and MOD_WIN) <> 0 then
  begin SetLength(Parts, Length(Parts) + 1); Parts[High(Parts)] := 'Win'; end;

  KeyName := VkToName(AVk);
  if KeyName = '' then Exit;
  SetLength(Parts, Length(Parts) + 1);
  Parts[High(Parts)] := KeyName;
  Result := string.Join('+', Parts);
end;

function FormatHotkeyFromVk(AVk: UINT;
  ACtrl, AShift, AAlt, AWin: Boolean): string;
var
  Mods: UINT;
begin
  Mods := 0;
  if ACtrl  then Mods := Mods or MOD_CONTROL;
  if AShift then Mods := Mods or MOD_SHIFT;
  if AAlt   then Mods := Mods or MOD_ALT;
  if AWin   then Mods := Mods or MOD_WIN;
  Result := FormatHotkey(Mods, AVk);
end;

function IsReservedHotkey(AModifiers, AVk: UINT; out AReason: string): Boolean;
// Match exato — Ctrl+Alt+Del bloqueia so a combinacao com esses dois
// mods (nem mais, nem menos). Mantemos pequeno: so o que sabidamente
// nunca dispara via RegisterHotKey no Windows 10/11.
//
// Espelho do RESERVED_HOTKEYS em ui/index.html. Mudancas devem ser
// feitas nos dois lugares (a UI ja bloqueia antes de mandar, isso e
// safety net pra edicao manual de config.json).
const
  M_C  = MOD_CONTROL;
  M_S  = MOD_SHIFT;
  M_A  = MOD_ALT;
  M_W  = MOD_WIN;
begin
  Result := True;
  AReason := '';

  // Combinacoes com Ctrl / Alt.
  if (AModifiers = (M_C or M_A)) and (AVk = VK_DELETE) then
    AReason := 'Ctrl+Alt+Del é a chamada de atenção segura do Windows.'
  else if (AModifiers = M_A) and (AVk = VK_TAB) then
    AReason := 'Alt+Tab é o alternador de tarefas do Windows.'
  else if (AModifiers = M_A) and (AVk = VK_F4) then
    AReason := 'Alt+F4 fecha a janela atual no Windows.'
  else if (AModifiers = M_A) and (AVk = VK_ESCAPE) then
    AReason := 'Alt+Esc alterna entre janelas no Windows.'
  else if (AModifiers = M_A) and (AVk = VK_SPACE) then
    AReason := 'Alt+Space abre o menu de sistema da janela.'
  else if (AModifiers = M_C) and (AVk = VK_ESCAPE) then
    AReason := 'Ctrl+Esc abre o menu Iniciar do Windows.'

  // Combinacoes com Win — shell intercepta antes do RegisterHotKey.
  else if (AModifiers = M_W) and (AVk = Ord('L')) then
    AReason := 'Win+L bloqueia a estação de trabalho no Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('D')) then
    AReason := 'Win+D mostra a área de trabalho no Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('E')) then
    AReason := 'Win+E abre o Explorador de Arquivos no Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('R')) then
    AReason := 'Win+R abre a caixa Executar do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('I')) then
    AReason := 'Win+I abre as Configurações do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('X')) then
    AReason := 'Win+X abre o menu de power user do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('A')) then
    AReason := 'Win+A abre a Central de Ações do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('S')) then
    AReason := 'Win+S abre a Pesquisa do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('Q')) then
    AReason := 'Win+Q abre a Pesquisa do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('M')) then
    AReason := 'Win+M minimiza todas as janelas no Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('P')) then
    AReason := 'Win+P abre o seletor de projeção do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('V')) then
    AReason := 'Win+V abre o histórico da área de transferência do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('G')) then
    AReason := 'Win+G abre a Game Bar do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('K')) then
    AReason := 'Win+K abre o Conectar do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('H')) then
    AReason := 'Win+H abre o ditado por voz do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('U')) then
    AReason := 'Win+U abre a Central de Facilidade de Acesso do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('B')) then
    AReason := 'Win+B foca na bandeja do sistema do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('T')) then
    AReason := 'Win+T foca na barra de tarefas do Windows.'
  else if (AModifiers = M_W) and (AVk = Ord('W')) then
    AReason := 'Win+W abre os Widgets do Windows.'
  else if (AModifiers = M_W) and (AVk = VK_TAB) then
    AReason := 'Win+Tab abre a Visão de Tarefas do Windows.'
  else if (AModifiers = M_W) and ((AVk = VK_UP) or (AVk = VK_DOWN) or
                                  (AVk = VK_LEFT) or (AVk = VK_RIGHT)) then
    AReason := 'Win+Setas controla o snap de janelas no Windows.'
  else if (AModifiers = M_W) and (AVk = VK_OEM_PERIOD) then
    AReason := 'Win+. abre o seletor de emojis do Windows.'
  else if (AModifiers = M_W) and (AVk = VK_OEM_1) then
    AReason := 'Win+; abre o seletor de emojis do Windows.'
  else if (AModifiers = (M_W or M_S)) and (AVk = Ord('S')) then
    AReason := 'Win+Shift+S abre a Captura de Tela do Windows.'

  else
    Result := False;
end;

end.

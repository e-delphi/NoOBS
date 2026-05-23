(*
  OBSScrollLock - controla o LED de Scroll Lock do teclado via Win32.

  Usado como indicador visual de gravacao em curso (quando o app esta
  na bandeja/hibernando e o user nao tem como saber se ta gravando
  pela UI). LED pisca a 1Hz durante a gravacao.

  Por que Scroll Lock: e a unica das 3 luzes (Num/Caps/Scroll) que
  praticamente nenhum app moderno usa — sequestrar nao quebra fluxo
  do usuario. Acionavel sem privilegios elevados.

  Acionamento: keybd_event simula press+release de VK_SCROLL — o
  driver do teclado interpreta como toggle do estado, e o LED segue
  o estado novo. GetKeyState(VK_SCROLL) retorna o estado atual no
  bit baixo do short (0=apagado, 1=aceso).
*)
unit OBSScrollLock;

interface

// True se o LED esta aceso agora.
function IsScrollLockOn: Boolean;

// Inverte o estado (aceso -> apagado ou vice-versa). Usado pelo
// timer de piscar.
procedure ToggleScrollLock;

// Garante estado final desejado (idempotente — so envia keystroke
// se o estado atual diverge do pedido). Usado pra apagar no fim
// da gravacao.
procedure SetScrollLockState(AOn: Boolean);

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.StrUtils,
  OBSLog;

const
  KEYEVENTF_KEYUP = $0002;
  MAPVK_VK_TO_VSC = 0;

function IsScrollLockOn: Boolean;
begin
  // GetKeyState retorna SHORT; bit 0 (low bit) = estado do toggle
  // ("on" pra Caps/Num/Scroll lock). Bit alto = pressionada agora.
  Result := (GetKeyState(VK_SCROLL) and 1) <> 0;
end;

procedure ToggleScrollLock;
var
  Before, After: Boolean;
  Inputs: array[0..1] of TInput;
  Scan: Word;
  N: UINT;
begin
  Before := IsScrollLockOn;
  // SendInput em vez de keybd_event (API legada). Scan code explicito
  // via MapVirtualKey — alguns drivers/apps so honram input que tem
  // scan code valido alem do VK. Sem isso, osk e similares podem
  // ignorar o toggle.
  Scan := MapVirtualKey(VK_SCROLL, MAPVK_VK_TO_VSC);
  FillChar(Inputs, SizeOf(Inputs), 0);

  // Press
  Inputs[0].Itype := INPUT_KEYBOARD;
  Inputs[0].ki.wVk := VK_SCROLL;
  Inputs[0].ki.wScan := Scan;
  Inputs[0].ki.dwFlags := 0;

  // Release
  Inputs[1].Itype := INPUT_KEYBOARD;
  Inputs[1].ki.wVk := VK_SCROLL;
  Inputs[1].ki.wScan := Scan;
  Inputs[1].ki.dwFlags := KEYEVENTF_KEYUP;

  N := SendInput(2, Inputs[0], SizeOf(TInput));
  After := IsScrollLockOn;
  Log('ScrollLock: toggle %s -> %s (SendInput aceitou %d/2, scan=$%x)',
    [BoolToStr(Before, True), BoolToStr(After, True), N, Scan]);
end;

procedure SetScrollLockState(AOn: Boolean);
var
  Cur: Boolean;
begin
  Cur := IsScrollLockOn;
  Log('ScrollLock: SetState pedido=%s atual=%s%s',
    [BoolToStr(AOn, True), BoolToStr(Cur, True),
     IfThen(Cur = AOn, ' (no-op)', ' (toggle)')]);
  if Cur = AOn then Exit;
  ToggleScrollLock;
end;

end.

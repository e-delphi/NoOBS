{
  WinRecIndicator — indicador de gravacao NA TELA, excluido da propria
  gravacao.

  Uma janelinha overlay (topmost, layered, click-through) que mostra uma
  bolinha + um tempo. Serve a DOIS estados, um de cada vez (o buffer para
  enquanto a gravacao manual roda, entao nunca coexistem):

    imRecording — bolinha VERMELHA pulsando + tempo gravado (HH:MM:SS).
                  Clicar para a gravacao.
    imBuffer    — bolinha VERDE fixa + quanto o buffer ja guardou, sobre o
                  teto ("2:13 / 5:00"). Clicar salva o trecho.

  A bolinha do buffer NAO pulsa de proposito: pulsando, os dois estados
  ficariam quase iguais de canto de olho, e confundir "guardando na RAM"
  com "gravando em disco" e exatamente o erro caro. O truque: ela chama
  SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE), entao o usuario a ve na
  tela mas a captura de tela (WGC/DXGI do monitor_capture do OBS) a OMITE —
  o desktop atras dela e o que entra na gravacao.

  WDA_EXCLUDEFROMCAPTURE exige Windows 10 2004+ (build 19041). Se
  SetWindowDisplayAffinity falhar (Windows antigo), NAO mostramos o overlay:
  melhor nao mostrar do que arriscar ele aparecer dentro da gravacao.

  So faz sentido no modo full (a gravacao so acontece la). A janela e criada
  na main thread e depende do message loop do OBSUI.Run pra pintar/atualizar
  (WM_PAINT/WM_TIMER) — o mesmo loop que ja bombeia a janela principal. Por
  isso a atualizacao do tempo/pulso usa SetTimer na propria janela.

  Fica so no MONITOR PRINCIPAL, num dos 4 cantos (config recIndicatorCorner).
}
unit WinRecIndicator;

{$WARN SYMBOL_PLATFORM OFF}   // 'delayed' e Win-only (igual LibOBS/FFmpegLib)

interface

type
  TRecCorner = (rcTopLeft, rcTopRight, rcBottomLeft, rcBottomRight);

  // O que o overlay esta mostrando. Ver cabecalho da unit.
  TIndicatorMode = (imRecording, imBuffer);

  // Chamado (na main thread, DENTRO do WndProc do overlay) quando o usuario
  // CLICA no indicador. O consumidor (OBSBridge) decide o que fazer pelo
  // estado (parar a gravacao / salvar o trecho) e age de forma DIFERIDA
  // (TThread.Queue) — agir sincrono destruiria a janela dentro do proprio
  // WndProc dela.
  TIndicatorClickProc = procedure;

var
  // Registrado pelo OBSBridge. Disparado quando o usuario clica no overlay.
  OnClickStop: TIndicatorClickProc = nil;

// Converte o valor do config pro enum. Aceita 'top-left', 'top-right',
// 'bottom-left', 'bottom-right'. Qualquer outro -> rcTopRight (default).
function ParseCorner(const S: string): TRecCorner;

// Mostra o indicador no canto do monitor principal, contando o tempo a
// partir de AStartTickMs (base GetTickCount, igual ao RecordingStartTickMs
// do OBSBridge). AOpacityPct = opacidade 20..100. AMode escolhe o visual
// (ver TIndicatorMode) e AMaxSec e o teto de tempo do buffer, usado so no
// imBuffer pra mostrar "guardado / teto". O overlay SEMPRE e clicavel (nao
// e click-through) — clicar nele chama OnClickStop. Se ja visivel,
// reposiciona/reaplica, inclusive trocando de modo. Se o Windows nao
// suporta exclusao de captura, NAO mostra (e loga).
procedure ShowIndicator(ACorner: TRecCorner; AStartTickMs: Cardinal;
  AOpacityPct: Integer; AMode: TIndicatorMode = imRecording;
  AMaxSec: Integer = 0);

// Modo do overlay visivel agora. So faz sentido com IsShowing = True.
function CurrentMode: TIndicatorMode;

// Aplica a opacidade (20..100) na janela viva — usado pelo slider das
// Config. em tempo real. No-op se o overlay nao esta visivel.
procedure SetOpacity(AOpacityPct: Integer);

// Esconde e destroi o overlay. Idempotente.
procedure HideIndicator;

function IsShowing: Boolean;

implementation

uses
  Winapi.Windows, Winapi.Messages, System.SysUtils, System.Types, OBSLog;

const
  WDA_EXCLUDEFROMCAPTURE = $00000011;   // Win10 2004+ (build 19041)
  CLASS_NAME = 'TNoOBSRecIndicator';
  TIMER_ID   = 1;
  TICK_MS    = 500;    // pulso da bolinha (1Hz) + reavalia o tempo
  MARGIN_DIP = 24;     // distancia do canto (px logicos, escalados por DPI)
  W_DIP = 116;         // largura do pill (logica) — modo gravacao
  // O pill do buffer e mais largo: cabe "12:34 / 60:00". Mostrar so o
  // guardado escondia justamente a informacao que decide se vale salvar
  // agora ou esperar.
  W_BUF_DIP = 140;
  H_DIP = 34;          // altura do pill (logica)

// SetWindowDisplayAffinity existe desde o Win7 (a FLAG nova e que exige
// 2004+), entao o import direto e seguro pro load em qualquer Windows.
function SetWindowDisplayAffinity(hWnd: HWND; dwAffinity: DWORD): BOOL;
  stdcall; external user32;

var
  FHwnd: HWND = 0;
  FStartTick: Cardinal = 0;
  FCorner: TRecCorner = rcTopRight;
  FDotOn: Boolean = True;
  FRegistered: Boolean = False;
  FDpi: Integer = 96;
  FMode: TIndicatorMode = imRecording;
  FMaxSec: Integer = 0;

// Opacidade 20..100 -> alpha 51..255 (clampeada; abaixo de 20% seria
// invisivel demais pra ser util).
function OpacityToAlpha(APct: Integer): Byte;
begin
  if APct < 20 then APct := 20
  else if APct > 100 then APct := 100;
  Result := Byte(MulDiv(APct, 255, 100));
end;

// GetDpiForSystem e Win10 1607+; resolve via GetProcAddress pra nao quebrar
// o load em Windows antigo (onde o recurso nem funciona — cai em 96).
function SystemDpi: Integer;
type
  TGetDpiForSystem = function: UINT; stdcall;
var
  P: TGetDpiForSystem;
begin
  Result := 96;
  @P := GetProcAddress(GetModuleHandle(user32), 'GetDpiForSystem');
  if Assigned(P) then
    Result := Integer(P());
  if Result <= 0 then Result := 96;
end;

// Escala um valor logico pelo DPI do sistema.
function SX(V: Integer): Integer;
begin
  Result := MulDiv(V, FDpi, 96);
end;

function ParseCorner(const S: string): TRecCorner;
begin
  if SameText(S, 'top-left') then Result := rcTopLeft
  else if SameText(S, 'bottom-left') then Result := rcBottomLeft
  else if SameText(S, 'bottom-right') then Result := rcBottomRight
  else Result := rcTopRight;   // default e qualquer valor invalido
end;

function ElapsedStr: string;
var
  Secs: Cardinal;
begin
  Secs := (GetTickCount - FStartTick) div 1000;
  Result := Format('%.2d:%.2d:%.2d',
    [Secs div 3600, (Secs div 60) mod 60, Secs mod 60]);
end;

// Modo buffer: "guardado / teto" (m:ss). O guardado satura no teto — e o
// que o buffer faz de verdade, descartando o comeco pra caber.
function HeldStr: string;
var
  Secs: Cardinal;
begin
  Secs := (GetTickCount - FStartTick) div 1000;
  if (FMaxSec > 0) and (Secs > Cardinal(FMaxSec)) then Secs := Cardinal(FMaxSec);
  Result := Format('%d:%.2d / %d:%.2d',
    [Secs div 60, Secs mod 60, FMaxSec div 60, FMaxSec mod 60]);
end;

// ---------------------------------------------------------------------
// GDI+ — usado SO pra desenhar a bolinha com antialiasing. O Ellipse do
// GDI puro nao suaviza a borda (fica serrilhada). O texto ja e suave
// (fonte CLEARTYPE), entao so o circulo passa pelo GDI+. gdiplus.dll e
// DLL de sistema; `delayed` + guarda deixam o load seguro mesmo se faltar
// (ai cai no Ellipse GDI). ---------------------------------------------
type
  TGdiplusStartupInput = record
    GdiplusVersion: Cardinal;
    DebugEventCallback: Pointer;
    SuppressBackgroundThread: BOOL;
    SuppressExternalCodecs: BOOL;
  end;

const
  SmoothingModeAntiAlias = 4;   // GDI+ SmoothingMode enum

function GdiplusStartup(out token: ULONG_PTR;
  const input: TGdiplusStartupInput; output: Pointer): Integer; stdcall;
  external 'gdiplus.dll' delayed;
procedure GdiplusShutdown(token: ULONG_PTR); stdcall;
  external 'gdiplus.dll' delayed;
function GdipCreateFromHDC(hdc: HDC; out graphics: Pointer): Integer; stdcall;
  external 'gdiplus.dll' delayed;
function GdipDeleteGraphics(graphics: Pointer): Integer; stdcall;
  external 'gdiplus.dll' delayed;
function GdipSetSmoothingMode(graphics: Pointer; mode: Integer): Integer;
  stdcall; external 'gdiplus.dll' delayed;
function GdipCreateSolidFill(color: Cardinal; out brush: Pointer): Integer;
  stdcall; external 'gdiplus.dll' delayed;
function GdipDeleteBrush(brush: Pointer): Integer; stdcall;
  external 'gdiplus.dll' delayed;
function GdipFillEllipse(graphics, brush: Pointer;
  x, y, width, height: Single): Integer; stdcall; external 'gdiplus.dll' delayed;

var
  GGdipToken: ULONG_PTR = 0;

function EnsureGdiplus: Boolean;
var
  Inp: TGdiplusStartupInput;
begin
  if GGdipToken <> 0 then Exit(True);
  FillChar(Inp, SizeOf(Inp), 0);
  Inp.GdiplusVersion := 1;
  try
    Result := GdiplusStartup(GGdipToken, Inp, nil) = 0;
  except
    Result := False;   // gdiplus.dll ausente (delayed load falhou)
  end;
  if not Result then GGdipToken := 0;
end;

// Circulo preenchido com antialiasing. ARGB opaco. Retorna False se o GDI+
// nao esta disponivel (o chamador cai no Ellipse do GDI).
function FillCircleAA(DC: HDC; Cx, Cy, Rad: Integer; R, G, B: Byte): Boolean;
var
  Gfx, Brush: Pointer;
  Argb: Cardinal;
begin
  Result := False;
  if not EnsureGdiplus then Exit;
  if GdipCreateFromHDC(DC, Gfx) <> 0 then Exit;
  try
    GdipSetSmoothingMode(Gfx, SmoothingModeAntiAlias);
    Argb := (Cardinal($FF) shl 24) or (Cardinal(R) shl 16) or
            (Cardinal(G) shl 8) or Cardinal(B);
    if GdipCreateSolidFill(Argb, Brush) = 0 then
    begin
      GdipFillEllipse(Gfx, Brush, Cx - Rad, Cy - Rad, 2 * Rad, 2 * Rad);
      GdipDeleteBrush(Brush);
      Result := True;
    end;
  finally
    GdipDeleteGraphics(Gfx);
  end;
end;

procedure PaintTo(DC: HDC; W, H: Integer);
var
  BgBrush, DotBrush: HBRUSH;
  OldBrush, OldPen, OldFont: HGDIOBJ;
  Fnt: HFONT;
  R: TRect;
  DotR, DotG, DotB: Byte;
  Rad, Cx, Cy: Integer;
  S: string;
begin
  // Fundo escuro do pill (cantos arredondados vem do SetWindowRgn).
  BgBrush := CreateSolidBrush(RGB(24, 24, 27));
  R := Rect(0, 0, W, H);
  FillRect(DC, R, BgBrush);
  DeleteObject(BgBrush);

  // Bolinha: vermelha pulsando na gravacao; verde FIXA no buffer (ver
  // cabecalho — o pulso e o que distingue os dois de canto de olho).
  if FMode = imBuffer then
  begin DotR := 16; DotG := 185; DotB := 129; end
  else if FDotOn then
  begin DotR := 240; DotG := 62; DotB := 62; end
  else
  begin DotR := 110; DotG := 34; DotB := 34; end;
  Rad := SX(6);
  Cx := SX(17);
  Cy := H div 2;
  // Antialiasing via GDI+; se indisponivel, cai no Ellipse GDI (serrilhado).
  // A borda suave mistura com o fundo escuro do pill (ja pintado acima).
  if not FillCircleAA(DC, Cx, Cy, Rad, DotR, DotG, DotB) then
  begin
    DotBrush := CreateSolidBrush(RGB(DotR, DotG, DotB));
    OldBrush := SelectObject(DC, DotBrush);
    OldPen := SelectObject(DC, GetStockObject(NULL_PEN));
    Ellipse(DC, Cx - Rad, Cy - Rad, Cx + Rad, Cy + Rad);
    SelectObject(DC, OldBrush);
    SelectObject(DC, OldPen);
    DeleteObject(DotBrush);
  end;

  // Tempo (HH:MM:SS).
  Fnt := CreateFont(-SX(15), 0, 0, 0, FW_SEMIBOLD, 0, 0, 0,
    DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
    CLEARTYPE_QUALITY, DEFAULT_PITCH or FF_DONTCARE, 'Segoe UI');
  OldFont := SelectObject(DC, Fnt);
  SetBkMode(DC, TRANSPARENT);
  SetTextColor(DC, RGB(240, 240, 240));
  if FMode = imBuffer then S := HeldStr else S := ElapsedStr;
  R := Rect(Cx + Rad + SX(8), 0, W - SX(6), H);
  DrawText(DC, PChar(S), Length(S), R, DT_SINGLELINE or DT_VCENTER or DT_LEFT);
  SelectObject(DC, OldFont);
  DeleteObject(Fnt);
end;

// WM_PAINT com double-buffer pra nao piscar no repaint de 500ms.
procedure DoPaint;
var
  PS: TPaintStruct;
  DC, MemDC: HDC;
  Bmp, OldBmp: HBITMAP;
  RC: TRect;
  W, H: Integer;
begin
  DC := BeginPaint(FHwnd, PS);
  try
    GetClientRect(FHwnd, RC);
    W := RC.Right;
    H := RC.Bottom;
    MemDC := CreateCompatibleDC(DC);
    Bmp := CreateCompatibleBitmap(DC, W, H);
    OldBmp := SelectObject(MemDC, Bmp);
    PaintTo(MemDC, W, H);
    BitBlt(DC, 0, 0, W, H, MemDC, 0, 0, SRCCOPY);
    SelectObject(MemDC, OldBmp);
    DeleteObject(Bmp);
    DeleteDC(MemDC);
  finally
    EndPaint(FHwnd, PS);
  end;
end;

function WndProc(Wnd: HWND; Msg: UINT; wParam: WPARAM; lParam: LPARAM): LRESULT;
  stdcall;
begin
  case Msg of
    WM_PAINT:
      begin
        DoPaint;
        Result := 0;
      end;
    WM_TIMER:
      begin
        // Pulso so na gravacao; no buffer a bolinha fica acesa.
        if FMode = imBuffer then FDotOn := True else FDotOn := not FDotOn;
        InvalidateRect(Wnd, nil, False);   // repinta (tempo tambem reavalia)
        Result := 0;
      end;
    WM_MOUSEACTIVATE:
      // Clicar no overlay nunca rouba o foco da janela ativa.
      Result := MA_NOACTIVATE;
    WM_SETCURSOR:
      begin
        SetCursor(LoadCursor(0, IDC_HAND));   // sinaliza "clicavel"
        Result := 1;
      end;
    WM_LBUTTONUP:
      begin
        // Clicar no overlay para a gravacao (modo gravacao) ou salva o
        // trecho (modo buffer) — quem decide e o consumidor, pelo estado.
        // Ele age DIFERIDO (TThread.Queue), pra nao destruir esta janela
        // dentro do proprio WndProc dela.
        if Assigned(OnClickStop) then
          try OnClickStop; except end;
        Result := 0;
      end;
  else
    Result := DefWindowProc(Wnd, Msg, wParam, lParam);
  end;
end;

procedure EnsureClass;
var
  WC: WNDCLASS;
begin
  if FRegistered then Exit;
  FillChar(WC, SizeOf(WC), 0);
  WC.lpfnWndProc := @WndProc;
  WC.hInstance := HInstance;
  WC.hCursor := LoadCursor(0, IDC_ARROW);
  WC.lpszClassName := CLASS_NAME;
  Winapi.Windows.RegisterClass(WC);   // qualificado — pegadinha #5
  FRegistered := True;
end;

procedure PositionWindow(W, H: Integer);
var
  WA: TRect;
  X, Y, M: Integer;
begin
  // Area de trabalho do monitor PRINCIPAL (exclui a barra de tarefas).
  if not SystemParametersInfo(SPI_GETWORKAREA, 0, @WA, 0) then
    WA := Rect(0, 0, GetSystemMetrics(SM_CXSCREEN),
      GetSystemMetrics(SM_CYSCREEN));
  M := SX(MARGIN_DIP);
  case FCorner of
    rcTopLeft:     begin X := WA.Left + M;      Y := WA.Top + M;        end;
    rcBottomLeft:  begin X := WA.Left + M;      Y := WA.Bottom - H - M; end;
    rcBottomRight: begin X := WA.Right - W - M; Y := WA.Bottom - H - M; end;
  else // rcTopRight
                   begin X := WA.Right - W - M; Y := WA.Top + M;        end;
  end;
  SetWindowPos(FHwnd, HWND_TOPMOST, X, Y, W, H,
    SWP_NOACTIVATE or SWP_SHOWWINDOW);
end;

procedure SetOpacity(AOpacityPct: Integer);
begin
  if FHwnd = 0 then Exit;
  SetLayeredWindowAttributes(FHwnd, 0, OpacityToAlpha(AOpacityPct), LWA_ALPHA);
end;

// Largura logica do pill no modo atual.
function ModeWidthDip: Integer;
begin
  if FMode = imBuffer then Result := W_BUF_DIP else Result := W_DIP;
end;

procedure ShowIndicator(ACorner: TRecCorner; AStartTickMs: Cardinal;
  AOpacityPct: Integer; AMode: TIndicatorMode; AMaxSec: Integer);
var
  W, H: Integer;
  Rgn: HRGN;
begin
  FStartTick := AStartTickMs;
  FCorner := ACorner;
  FDotOn := True;
  FMode := AMode;
  FMaxSec := AMaxSec;

  if FHwnd <> 0 then
  begin
    // Ja visivel: reposiciona, reaplica opacidade e repinta. Trocar de modo
    // muda a LARGURA do pill, entao a regiao arredondada tem que ser
    // refeita junto — senao o novo tamanho vira um pill cortado (a regiao
    // antiga continua recortando a janela).
    FDpi := SystemDpi;
    W := SX(ModeWidthDip);
    H := SX(H_DIP);
    SetWindowPos(FHwnd, 0, 0, 0, W, H, SWP_NOMOVE or SWP_NOZORDER or SWP_NOACTIVATE);
    Rgn := CreateRoundRectRgn(0, 0, W + 1, H + 1, SX(10), SX(10));
    SetWindowRgn(FHwnd, Rgn, False);
    PositionWindow(W, H);
    SetOpacity(AOpacityPct);
    InvalidateRect(FHwnd, nil, False);
    Exit;
  end;

  EnsureClass;
  FDpi := SystemDpi;
  W := SX(ModeWidthDip);
  H := SX(H_DIP);

  // SEM WS_EX_TRANSPARENT: o overlay recebe o clique (clicar = parar). E
  // pequeno (~116x34px), entao bloquear so essa area e aceitavel.
  FHwnd := CreateWindowEx(
    WS_EX_LAYERED or WS_EX_TOOLWINDOW or WS_EX_NOACTIVATE or WS_EX_TOPMOST,
    CLASS_NAME, '', WS_POPUP,
    0, 0, W, H, 0, 0, HInstance, nil);
  if FHwnd = 0 then
  begin
    Log('WinRecIndicator: CreateWindowEx falhou (%d).', [GetLastError]);
    Exit;
  end;

  // EXCLUI DA CAPTURA. Se falhar (Windows < 2004), aborta e nao mostra —
  // senao o overlay apareceria dentro da gravacao, que e justo o oposto
  // do recurso.
  if not SetWindowDisplayAffinity(FHwnd, WDA_EXCLUDEFROMCAPTURE) then
  begin
    Log('WinRecIndicator: SetWindowDisplayAffinity falhou (%d) — Windows ' +
      'sem suporte a exclusao de captura; indicador NAO sera mostrado.',
      [GetLastError]);
    DestroyWindow(FHwnd);
    FHwnd := 0;
    Exit;
  end;

  // Opacidade configuravel + cantos arredondados.
  SetOpacity(AOpacityPct);
  Rgn := CreateRoundRectRgn(0, 0, W + 1, H + 1, SX(10), SX(10));
  SetWindowRgn(FHwnd, Rgn, False);   // a janela assume a posse da regiao

  PositionWindow(W, H);
  SetTimer(FHwnd, TIMER_ID, TICK_MS, nil);
  InvalidateRect(FHwnd, nil, False);
  Log('WinRecIndicator: overlay mostrado (modo=%d, canto=%d, opac=%d%%).',
    [Ord(FMode), Ord(FCorner), AOpacityPct]);
end;

function CurrentMode: TIndicatorMode;
begin
  Result := FMode;
end;

procedure HideIndicator;
begin
  if FHwnd = 0 then Exit;
  KillTimer(FHwnd, TIMER_ID);
  DestroyWindow(FHwnd);
  FHwnd := 0;
  Log('WinRecIndicator: overlay escondido.');
end;

function IsShowing: Boolean;
begin
  Result := FHwnd <> 0;
end;

initialization
  // GDI+ e iniciado sob demanda (EnsureGdiplus, na 1a bolinha desenhada);
  // aqui so garantimos o par de shutdown no fim do processo.

finalization
  if GGdipToken <> 0 then
  begin
    try GdiplusShutdown(GGdipToken); except end;
    GGdipToken := 0;
  end;

end.

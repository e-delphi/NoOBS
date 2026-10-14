(*
  WinWindows - janelas de aplicativo que podem ser gravadas (Win32).

  Alimenta o seletor "Gravar uma janela": enumera as janelas de topo que
  aparecem na barra de tarefas e tira uma previa de cada uma com
  PrintWindow(PW_RENDERFULLCONTENT) — funciona com a janela coberta por
  outras, porque o DWM entrega a superficie da propria janela.

  A identidade que vai pro libobs (fonte window_capture do win-capture) e
  a string "titulo:classe:exe" com ':' e '#' escapados, EXATAMENTE como o
  OBS monta (libobs/util/windows/window-helpers.c, add_window/encode_dstr).
  O plugin reencontra a janela por ela a cada tique, entao a gravacao segue
  a janela mesmo que ela seja fechada e reaberta. Pegadinha #66.
*)
unit WinWindows;

interface

uses
  Winapi.Windows, System.SysUtils;

type
  TCaptureWindow = record
    Handle: HWND;       // janela de topo (a da barra de tarefas)
    Title: string;
    ClassName: string;
    Exe: string;        // so o nome do arquivo (ex.: chrome.exe)
    Spec: string;       // "titulo:classe:exe" escapado, pro window_capture
    Width, Height: Integer;  // area CLIENTE, pixels fisicos
  end;
  TCaptureWindowArray = TArray<TCaptureWindow>;

// Janelas gravaveis, na ordem Z (a mais a frente primeiro). Pula
// invisiveis, minimizadas, "cloaked" (outra area de trabalho virtual),
// tool windows, as do proprio processo e as internas do Windows que o OBS
// tambem pula.
function EnumerateCaptureWindows: TCaptureWindowArray;

// Tamanho da area cliente (o que o window_capture grava com client_area).
// Minimizada: usa o tamanho que ela tera ao ser restaurada. False se a
// janela nao existe mais.
function GetCaptureWindowSize(AWnd: HWND; out AW, AH: Integer): Boolean;

// JPEG da area cliente reduzida pra caber em AMaxW x AMaxH (mantem a
// proporcao). Vazio se nao der (janela sumiu, travada, tamanho zero).
// Pode levar dezenas de ms por janela: chamar em worker.
function CaptureWindowThumbJpeg(AWnd: HWND; AMaxW, AMaxH: Integer): TBytes;

implementation

uses
  Winapi.DwmApi,
  Vcl.Graphics, Vcl.Imaging.Jpeg,
  System.Classes, System.Math, System.Types;

const
  // Desenha via DWM (Windows 8.1+): sem isso apps com aceleracao de GPU
  // (navegadores, Electron) saem pretos.
  PW_RENDERFULLCONTENT = $00000002;
  // Nem toda versao do Winapi.Windows declara (mesma constante do WinMicWatch).
  PROCESS_QUERY_LIMITED_INFORMATION = $1000;
  DWMWA_CLOAKED_LOCAL = 14;

  // Mesma lista do OBS (internal_microsoft_exes_exact): janelas do proprio
  // shell que aparecem na enumeracao mas nao sao "aplicativos".
  INTERNAL_EXES: array[0..14] of string = (
    'startmenuexperiencehost.exe', 'applicationframehost.exe',
    'peopleexperiencehost.exe', 'shellexperiencehost.exe',
    'microsoft.notes.exe', 'systemsettings.exe', 'textinputhost.exe',
    'searchapp.exe', 'video.ui.exe', 'searchui.exe', 'lockapp.exe',
    'cortana.exe', 'gamebar.exe', 'tabtip.exe', 'time.exe');

// Declarado aqui (e nao pego do Winapi.Windows) pra nao depender da versao
// do RTL.
function PrintWindowW(hwnd: HWND; hdcBlt: HDC; nFlags: UINT): BOOL; stdcall;
  external user32 name 'PrintWindow';
function IsHungAppWindow(hwnd: HWND): BOOL; stdcall;
  external user32 name 'IsHungAppWindow';
function QueryFullProcessImageNameW(hProcess: THandle; dwFlags: DWORD;
  lpExeName: PWideChar; var lpdwSize: DWORD): BOOL; stdcall;
  external kernel32 name 'QueryFullProcessImageNameW';

function WndTitle(H: HWND): string;
var
  Len: Integer;
begin
  Result := '';
  Len := GetWindowTextLength(H);
  if Len <= 0 then Exit;
  SetLength(Result, Len + 1);
  Len := GetWindowText(H, PChar(Result), Len + 1);
  SetLength(Result, Len);
end;

function WndClass(H: HWND): string;
var
  Buf: array[0..255] of Char;
  Len: Integer;
begin
  Len := GetClassName(H, Buf, Length(Buf));
  SetString(Result, Buf, Len);
end;

function WndPid(H: HWND): DWORD;
begin
  Result := 0;
  GetWindowThreadProcessId(H, @Result);
end;

function ProcessExeName(APid: DWORD): string;
var
  P: THandle;
  Buf: array[0..MAX_PATH] of Char;
  Sz: DWORD;
begin
  // O OBS cai em "unknown" quando nao consegue abrir o processo (elevado,
  // protegido) — repetimos, senao a string nao casaria com a dele.
  Result := 'unknown';
  P := OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, APid);
  if P = 0 then Exit;
  try
    Sz := Length(Buf);
    if QueryFullProcessImageNameW(P, 0, Buf, Sz) then
      Result := ExtractFileName(string(Buf));
  finally
    CloseHandle(P);
  end;
end;

function IsCloaked(H: HWND): Boolean;
var
  C: DWORD;
begin
  C := 0;
  Result := Succeeded(DwmGetWindowAttribute(H, DWMWA_CLOAKED_LOCAL, @C,
    SizeOf(C))) and (C <> 0);
end;

function IsInternalExe(const AExe: string): Boolean;
var
  S: string;
  L: string;
begin
  L := LowerCase(AExe);
  for S in INTERNAL_EXES do
    if L = S then Exit(True);
  Result := L.StartsWith('windowsinternal');
end;

function EncodePart(const S: string): string;
begin
  // Ordem do encode_dstr do OBS: '#' antes de ':', senao o '#' do "#3A"
  // seria escapado de novo.
  Result := StringReplace(S, '#', '#22', [rfReplaceAll]);
  Result := StringReplace(Result, ':', '#3A', [rfReplaceAll]);
end;

function UwpActualWindow(AFrame: HWND): HWND;
// App da loja: a janela de topo e do ApplicationFrameHost; o app de verdade
// e uma FILHA de outro processo (ms_get_uwp_actual_window). A identidade
// tem que ser a dela, senao o plugin casaria com o host.
var
  FramePid: DWORD;
  Child: HWND;
begin
  Result := 0;
  FramePid := WndPid(AFrame);
  Child := FindWindowEx(AFrame, 0, nil, nil);
  while Child <> 0 do
  begin
    if WndPid(Child) <> FramePid then Exit(Child);
    Child := FindWindowEx(AFrame, Child, nil, nil);
  end;
end;

function GetCaptureWindowSize(AWnd: HWND; out AW, AH: Integer): Boolean;
var
  R: TRect;
  WP: TWindowPlacement;
begin
  AW := 0; AH := 0;
  Result := (AWnd <> 0) and IsWindow(AWnd);
  if not Result then Exit;
  if IsIconic(AWnd) then
  begin
    // Minimizada a area cliente e 0x0. O tamanho "normal" inclui a borda,
    // entao e uma aproximacao — so serve pra dimensionar o canvas.
    FillChar(WP, SizeOf(WP), 0);
    WP.length := SizeOf(WP);
    if GetWindowPlacement(AWnd, @WP) then
    begin
      AW := WP.rcNormalPosition.Right - WP.rcNormalPosition.Left;
      AH := WP.rcNormalPosition.Bottom - WP.rcNormalPosition.Top;
    end;
  end
  else if GetClientRect(AWnd, R) then
  begin
    AW := R.Right - R.Left;
    AH := R.Bottom - R.Top;
  end;
end;

type
  PEnumCtx = ^TEnumCtx;
  TEnumCtx = record
    List: TCaptureWindowArray;
    SelfPid: DWORD;
  end;

function EnumProc(H: HWND; AParam: LPARAM): BOOL; stdcall;
var
  Ctx: PEnumCtx;
  Ex, St: NativeInt;
  Ident: HWND;
  W: TCaptureWindow;
begin
  Result := True;
  Ctx := PEnumCtx(AParam);
  if not IsWindowVisible(H) or IsIconic(H) or IsCloaked(H) then Exit;
  St := GetWindowLongPtr(H, GWL_STYLE);
  Ex := GetWindowLongPtr(H, GWL_EXSTYLE);
  if (Ex and WS_EX_TOOLWINDOW) <> 0 then Exit;
  if (St and WS_CHILD) <> 0 then Exit;
  // Janela com dono (dialogos, popups) nao e da barra de tarefas.
  if (GetWindow(H, GW_OWNER) <> 0) and ((Ex and WS_EX_APPWINDOW) = 0) then Exit;
  if WndPid(H) = Ctx.SelfPid then Exit;

  W := Default(TCaptureWindow);
  W.Handle := H;
  if not GetCaptureWindowSize(H, W.Width, W.Height) then Exit;
  if (W.Width < 32) or (W.Height < 32) then Exit;

  Ident := H;
  if SameText(WndClass(H), 'ApplicationFrameWindow') or
     SameText(WndClass(H), 'WinUIDesktopWin32WindowClass') then
  begin
    Ident := UwpActualWindow(H);
    if Ident = 0 then Exit;   // app da loja suspenso: nada pra gravar
  end;

  W.Exe := ProcessExeName(WndPid(Ident));
  if IsInternalExe(W.Exe) then Exit;
  W.Title := WndTitle(Ident);
  if W.Title = '' then W.Title := WndTitle(H);
  // Sem titulo nao ha como o usuario saber o que escolheu.
  if Trim(W.Title) = '' then Exit;
  W.ClassName := WndClass(Ident);
  W.Spec := EncodePart(W.Title) + ':' + EncodePart(W.ClassName) + ':' +
    EncodePart(W.Exe);

  SetLength(Ctx.List, Length(Ctx.List) + 1);
  Ctx.List[High(Ctx.List)] := W;
end;

function EnumerateCaptureWindows: TCaptureWindowArray;
var
  Ctx: TEnumCtx;
begin
  Ctx.List := nil;
  Ctx.SelfPid := GetCurrentProcessId;
  EnumWindows(@EnumProc, LPARAM(@Ctx));
  Result := Ctx.List;
end;

function CaptureWindowThumbJpeg(AWnd: HWND; AMaxW, AMaxH: Integer): TBytes;
var
  WR: TRect;
  Org: TPoint;
  CW, CH, WW, WH, TW, TH: Integer;
  Scale: Double;
  Full, Thumb: TBitmap;
  Jpeg: TJPEGImage;
  Stream: TMemoryStream;
begin
  SetLength(Result, 0);
  if (AWnd = 0) or not IsWindow(AWnd) or IsIconic(AWnd) then Exit;
  // PrintWindow manda mensagem pra thread da janela: travada, o pedido
  // ficaria preso junto.
  if IsHungAppWindow(AWnd) then Exit;
  if not GetWindowRect(AWnd, WR) then Exit;
  if not GetCaptureWindowSize(AWnd, CW, CH) then Exit;
  WW := WR.Right - WR.Left;
  WH := WR.Bottom - WR.Top;
  if (CW <= 0) or (CH <= 0) or (WW <= 0) or (WH <= 0) then Exit;
  // Onde a area cliente comeca dentro da janela inteira: a previa recorta
  // so ela, que e o que o window_capture vai gravar (client_area).
  Org := Point(0, 0);
  Winapi.Windows.ClientToScreen(AWnd, Org);
  Dec(Org.X, WR.Left);
  Dec(Org.Y, WR.Top);

  Scale := Min(AMaxW / CW, AMaxH / CH);
  if Scale > 1 then Scale := 1;
  TW := Max(1, Round(CW * Scale));
  TH := Max(1, Round(CH * Scale));

  Full := TBitmap.Create;
  try
    Full.PixelFormat := pf32bit;
    Full.SetSize(WW, WH);
    Thumb := TBitmap.Create;
    // Roda em worker: sem o Lock, o coletor de DCs da VCL (main thread)
    // pode soltar o DC do canvas no meio do desenho.
    Full.Canvas.Lock;
    Thumb.Canvas.Lock;
    try
      if not PrintWindowW(AWnd, Full.Canvas.Handle, PW_RENDERFULLCONTENT) then
        Exit;
      Thumb.PixelFormat := pf24bit;
      Thumb.SetSize(TW, TH);
      SetStretchBltMode(Thumb.Canvas.Handle, HALFTONE);
      SetBrushOrgEx(Thumb.Canvas.Handle, 0, 0, nil);
      StretchBlt(Thumb.Canvas.Handle, 0, 0, TW, TH,
        Full.Canvas.Handle, Org.X, Org.Y, CW, CH, SRCCOPY);
      Jpeg := TJPEGImage.Create;
      try
        Jpeg.CompressionQuality := 70;
        Jpeg.Assign(Thumb);
        Stream := TMemoryStream.Create;
        try
          Jpeg.SaveToStream(Stream);
          SetLength(Result, Stream.Size);
          if Stream.Size > 0 then
            Move(Stream.Memory^, Result[0], Stream.Size);
        finally
          Stream.Free;
        end;
      finally
        Jpeg.Free;
      end;
    finally
      Thumb.Canvas.Unlock;
      Full.Canvas.Unlock;
      Thumb.Free;
    end;
  finally
    Full.Free;
  end;
end;

end.

(*
  WinPreview - enumera monitores fisicos via Win32 e captura
  thumbnails via BitBlt do desktop. Substitui o screenshot do OBS
  pra UI mostrar previews sem precisar do OBS rodando.
*)
unit WinPreview;

interface

uses
  Winapi.Windows, System.SysUtils;

type
  TMonitorInfo = record
    Index: Integer;
    DeviceName: string;
    FriendlyName: string;
    X, Y, Width, Height: Integer;
    IsPrimary: Boolean;
    RefreshRate: Integer;   // Hz (ex.: 60, 120, 144, 240)
  end;
  TMonitorInfoArray = TArray<TMonitorInfo>;

function EnumerateMonitors: TMonitorInfoArray;

// Retorna a taxa de atualizacao maxima entre todos os monitores
// conectados. Fallback 60 Hz se nao houver monitor ou se a consulta
// falhar.
function GetMaxMonitorRefreshRate: Integer;

// Captura JPEG raw (bytes) do monitor, escalado pra AThumbW x AThumbH.
function CaptureMonitorAsJpeg(const AMon: TMonitorInfo;
  AThumbW, AThumbH: Integer): TBytes;

// Wrapper legado: retorna data URL base64. Usado no BuildMonitorsFromWin
// (init, uma unica vez) pra manter o thumb inline no JSON inicial.
function CaptureMonitorAsDataUrl(const AMon: TMonitorInfo;
  AThumbW, AThumbH: Integer): string;

implementation

uses
  Winapi.MultiMon,
  Vcl.Graphics, Vcl.Imaging.Jpeg,
  System.NetEncoding, System.Classes;

const
  // ENUM_CURRENT_SETTINGS = DWORD(-1) — definido em wingdi.h. Algumas
  // versoes do Delphi nao expoem o simbolo via Winapi.Windows, entao
  // declaramos local. Significa "ler os parametros ATIVOS do monitor"
  // (em vez de iterar modos disponiveis).
  ENUM_CURRENT_SETTINGS_LOCAL = DWORD(-1);

type
  PEnumState = ^TEnumState;
  TEnumState = record
    List: TMonitorInfoArray;
    Idx: Integer;
  end;

function MonitorEnumProc(hMon: HMONITOR; hdcMon: HDC;
  lprcMon: PRect; lParam: LPARAM): BOOL; stdcall;
var
  State: PEnumState;
  Info: TMonitorInfo;
  MI: TMonitorInfoEx;
  DM: TDeviceMode;
begin
  State := PEnumState(lParam);
  ZeroMemory(@MI, SizeOf(MI));
  MI.cbSize := SizeOf(MI);
  if GetMonitorInfo(hMon, @MI) then
  begin
    Info.Index := State^.Idx;
    Info.DeviceName := MI.szDevice;
    Info.FriendlyName := Info.DeviceName; // sem nome amigavel via Win32 simples
    Info.X := MI.rcMonitor.Left;
    Info.Y := MI.rcMonitor.Top;
    Info.Width  := MI.rcMonitor.Right  - MI.rcMonitor.Left;
    Info.Height := MI.rcMonitor.Bottom - MI.rcMonitor.Top;
    Info.IsPrimary := (MI.dwFlags and MONITORINFOF_PRIMARY) <> 0;
    // Taxa de atualizacao: EnumDisplaySettings com ENUM_CURRENT_SETTINGS
    // retorna os parametros ativos do monitor. Fallback 60 Hz se falhar.
    ZeroMemory(@DM, SizeOf(DM));
    DM.dmSize := SizeOf(DM);
    // dmDisplayFrequency 0 ou 1 = "hardware default" (documentado), nao
    // uma taxa real — trata como desconhecido e cai pro fallback 60.
    if EnumDisplaySettings(PWideChar(Info.DeviceName), ENUM_CURRENT_SETTINGS_LOCAL, DM)
       and (DM.dmDisplayFrequency > 1) then
      Info.RefreshRate := Integer(DM.dmDisplayFrequency)
    else
      Info.RefreshRate := 60;
    SetLength(State^.List, Length(State^.List) + 1);
    State^.List[High(State^.List)] := Info;
    Inc(State^.Idx);
  end;
  Result := True;
end;

function EnumerateMonitors: TMonitorInfoArray;
var
  State: TEnumState;
begin
  SetLength(State.List, 0);
  State.Idx := 0;
  EnumDisplayMonitors(0, nil, @MonitorEnumProc, LPARAM(@State));
  Result := State.List;
end;

function GetMaxMonitorRefreshRate: Integer;
var
  Mons: TMonitorInfoArray;
  i: Integer;
begin
  Result := 60; // fallback razoavel se nenhum monitor responder
  Mons := EnumerateMonitors;
  for i := 0 to High(Mons) do
    if Mons[i].RefreshRate > Result then
      Result := Mons[i].RefreshRate;
end;

function CaptureMonitorAsJpeg(const AMon: TMonitorInfo;
  AThumbW, AThumbH: Integer): TBytes;
var
  ScreenDC: HDC;
  VclBmp: TBitmap;
  Jpeg: TJPEGImage;
  Stream: TMemoryStream;
begin
  SetLength(Result, 0);
  if (AThumbW <= 0) or (AThumbH <= 0) then Exit;
  if (AMon.Width <= 0) or (AMon.Height <= 0) then Exit;

  ScreenDC := GetDC(0);
  if ScreenDC = 0 then Exit;
  try
    VclBmp := TBitmap.Create;
    try
      VclBmp.PixelFormat := pf24bit;
      VclBmp.SetSize(AThumbW, AThumbH);
      SetStretchBltMode(VclBmp.Canvas.Handle, HALFTONE);
      StretchBlt(VclBmp.Canvas.Handle, 0, 0, AThumbW, AThumbH,
        ScreenDC, AMon.X, AMon.Y, AMon.Width, AMon.Height,
        SRCCOPY);

      Jpeg := TJPEGImage.Create;
      try
        Jpeg.CompressionQuality := 65;
        Jpeg.Assign(VclBmp);
        Stream := TMemoryStream.Create;
        try
          Jpeg.SaveToStream(Stream);
          SetLength(Result, Stream.Size);
          Move(Stream.Memory^, Result[0], Stream.Size);
        finally
          Stream.Free;
        end;
      finally
        Jpeg.Free;
      end;
    finally
      VclBmp.Free;
    end;
  finally
    ReleaseDC(0, ScreenDC);
  end;
end;

function CaptureMonitorAsDataUrl(const AMon: TMonitorInfo;
  AThumbW, AThumbH: Integer): string;
var
  Bytes: TBytes;
begin
  Result := '';
  Bytes := CaptureMonitorAsJpeg(AMon, AThumbW, AThumbH);
  if Length(Bytes) = 0 then Exit;
  Result := 'data:image/jpeg;base64,' +
    TNetEncoding.Base64.EncodeBytesToString(Bytes, Length(Bytes));
end;

end.

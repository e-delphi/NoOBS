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
  end;
  TMonitorInfoArray = TArray<TMonitorInfo>;

function EnumerateMonitors: TMonitorInfoArray;

// Captura JPEG (data URL pronto: "data:image/jpeg;base64,...")
// do monitor especificado, escalado pra AThumbW x AThumbH.
function CaptureMonitorAsDataUrl(const AMon: TMonitorInfo;
  AThumbW, AThumbH: Integer): string;

implementation

uses
  Winapi.MultiMon,
  Vcl.Graphics, Vcl.Imaging.Jpeg,
  System.NetEncoding, System.Classes;

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

function CaptureMonitorAsDataUrl(const AMon: TMonitorInfo;
  AThumbW, AThumbH: Integer): string;
var
  ScreenDC: HDC;
  VclBmp: TBitmap;
  Jpeg: TJPEGImage;
  Stream: TMemoryStream;
  B64: string;
begin
  Result := '';
  if (AThumbW <= 0) or (AThumbH <= 0) then Exit;
  if (AMon.Width <= 0) or (AMon.Height <= 0) then Exit;

  ScreenDC := GetDC(0);
  if ScreenDC = 0 then Exit;
  try
    // TBitmap gerencia HBITMAP/HDC sem vazar — sem juggling manual
    // de SelectObject/DeleteObject.
    VclBmp := TBitmap.Create;
    try
      VclBmp.PixelFormat := pf24bit;
      VclBmp.SetSize(AThumbW, AThumbH);
      // CAPTUREBLT ($40000000) inclui janelas layered/topmost.
      SetStretchBltMode(VclBmp.Canvas.Handle, HALFTONE);
      StretchBlt(VclBmp.Canvas.Handle, 0, 0, AThumbW, AThumbH,
        ScreenDC, AMon.X, AMon.Y, AMon.Width, AMon.Height,
        SRCCOPY or DWORD($40000000));

      Jpeg := TJPEGImage.Create;
      try
        Jpeg.CompressionQuality := 65;
        Jpeg.Assign(VclBmp);
        Stream := TMemoryStream.Create;
        try
          Jpeg.SaveToStream(Stream);
          Stream.Position := 0;
          B64 := TNetEncoding.Base64.EncodeBytesToString(
            Stream.Memory, Stream.Size);
          Result := 'data:image/jpeg;base64,' + B64;
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

end.

(*
  OBSScene — tipos e funcoes puras para calculo de canvas e
  enumeracao de monitores/fontes. Sem dependencia de websocket
  ou processo externo.
*)
unit OBSScene;

interface

uses
  Winapi.Windows, System.SysUtils;

type
  TOBSMonitor = record
    Index: Integer;
    Name: string;
    Width, Height: Integer;
    PositionX, PositionY: Integer;
  end;
  TOBSMonitorArray = TArray<TOBSMonitor>;

  TAudioDevice = record
    Name: string;
    Value: string;
  end;
  TAudioDeviceArray = TArray<TAudioDevice>;

procedure ComputeCanvas(const Monitors: TOBSMonitorArray;
  out Width, Height, OriginX, OriginY: Integer);

function MonitorsFromWinPreview: TOBSMonitorArray;

// AAutoProfile=True: aplica tambem o perfil de auto-gravacao (so entra o
// monitor marcado na aba Comportamento). Usado quando a gravacao nasce do
// watcher de mic. Default False = gravacao manual (so habilitado/nao oculto).
function FilterEnabledMonitors(const AMons: TOBSMonitorArray;
  AAutoProfile: Boolean = False): TOBSMonitorArray;

implementation

uses
  OBSConfig, WinPreview;

function FilterEnabledMonitors(const AMons: TOBSMonitorArray;
  AAutoProfile: Boolean = False): TOBSMonitorArray;
var
  i: Integer;
  Active: Boolean;
begin
  SetLength(Result, 0);
  for i := 0 to High(AMons) do
  begin
    // GetSourceActive (nao GetSourceBool): monitor oculto nao entra no canvas.
    // No perfil auto, tambem precisa estar marcado na aba Comportamento.
    if AAutoProfile then
      Active := GetSourceActiveForAuto('monitors', IntToStr(AMons[i].Index), True)
    else
      Active := GetSourceActive('monitors', IntToStr(AMons[i].Index), True);
    if not Active then Continue;
    SetLength(Result, Length(Result) + 1);
    Result[High(Result)] := AMons[i];
  end;
end;

function MonitorsFromWinPreview: TOBSMonitorArray;
var
  WPM: WinPreview.TMonitorInfoArray;
  i: Integer;
begin
  WPM := WinPreview.EnumerateMonitors;
  SetLength(Result, Length(WPM));
  for i := 0 to High(WPM) do
  begin
    Result[i].Index     := WPM[i].Index;
    Result[i].Name      := WPM[i].FriendlyName;
    Result[i].Width     := WPM[i].Width;
    Result[i].Height    := WPM[i].Height;
    Result[i].PositionX := WPM[i].X;
    Result[i].PositionY := WPM[i].Y;
  end;
end;

procedure ComputeCanvas(const Monitors: TOBSMonitorArray;
  out Width, Height, OriginX, OriginY: Integer);
var
  i, MinX, MinY, MaxXPlus, MaxYPlus: Integer;
begin
  // Sem monitores: devolve canvas zerado em vez de levantar excecao
  // (esta unit e "pura" e nao deve lancar string hardcoded; o caso
  // audio-only — 0 monitores — e tratado pelo caller, pegadinha #8).
  Width := 0; Height := 0; OriginX := 0; OriginY := 0;
  if Length(Monitors) = 0 then
    Exit;
  MinX := Monitors[0].PositionX;
  MinY := Monitors[0].PositionY;
  MaxXPlus := Monitors[0].PositionX + Monitors[0].Width;
  MaxYPlus := Monitors[0].PositionY + Monitors[0].Height;
  for i := 1 to High(Monitors) do
  begin
    if Monitors[i].PositionX < MinX then MinX := Monitors[i].PositionX;
    if Monitors[i].PositionY < MinY then MinY := Monitors[i].PositionY;
    if Monitors[i].PositionX + Monitors[i].Width > MaxXPlus then
      MaxXPlus := Monitors[i].PositionX + Monitors[i].Width;
    if Monitors[i].PositionY + Monitors[i].Height > MaxYPlus then
      MaxYPlus := Monitors[i].PositionY + Monitors[i].Height;
  end;
  Width := MaxXPlus - MinX;
  Height := MaxYPlus - MinY;
  OriginX := MinX;
  OriginY := MinY;
end;

end.

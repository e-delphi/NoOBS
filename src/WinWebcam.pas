(*
  WinWebcam - enumera webcams (video input devices) via DirectShow.
  Sem dependencia do OBS — funciona com OBS dormindo.

  Cada webcam vira uma source 'dshow_input' no OBS quando a gravacao
  comeca, com video_device_id = display name do moniker.

  Resolucao default = 1280x720 quando nao consegue enumerar a real.
  OBS dshow_input aceita resolution=0x0 (auto), mas pra calcular
  canvas precisamos de um valor concreto.
*)
unit WinWebcam;

interface

uses
  System.SysUtils;

type
  TWebcamInfo = record
    DeviceId: string;     // moniker display name (passado pro OBS)
    Name: string;         // friendly name (mostrado na UI)
    Width, Height: Integer;
  end;
  TWebcamInfoArray = TArray<TWebcamInfo>;

function EnumerateWebcams: TWebcamInfoArray;

// Hook pra testes: quando setado, EnumerateWebcams retorna esse array
// em vez de tocar o DirectShow real. Liberar com `nil` no teardown.
type
  TWebcamEnumOverride = function: TWebcamInfoArray;
var
  WebcamEnumOverride: TWebcamEnumOverride = nil;

// Exposta pra teste — fallback friendly-name a partir do moniker
// display name quando IPropertyBag falha. Pura, sem efeito colateral.
function FriendlyFromMoniker(const ADeviceId: string;
  AIndex: Integer): string;

implementation

uses
  Winapi.Windows, Winapi.ActiveX, System.Variants, OBSLog;

const
  CLSID_SystemDeviceEnum:        TGUID = '{62BE5D10-60EB-11d0-BD3B-00A0C911CE86}';
  IID_ICreateDevEnum:            TGUID = '{29840822-5B84-11D0-BD3B-00A0C911CE86}';
  CLSID_VideoInputDeviceCategory:TGUID = '{860BB310-5D01-11d0-BD3B-00A0C911CE86}';
  IID_IPropertyBag:              TGUID = '{55272A00-42CB-11CE-8135-00AA004BB851}';

type
  IEnumMoniker = interface(IUnknown)
    ['{00000102-0000-0000-C000-000000000046}']
    function Next(celt: ULONG; out rgelt: IMoniker; pceltFetched: PULONG): HRESULT; stdcall;
    function Skip(celt: ULONG): HRESULT; stdcall;
    function Reset: HRESULT; stdcall;
    function Clone(out ppenum: IEnumMoniker): HRESULT; stdcall;
  end;

  ICreateDevEnum = interface(IUnknown)
    ['{29840822-5B84-11D0-BD3B-00A0C911CE86}']
    function CreateClassEnumerator(const clsidDeviceClass: TGUID;
      out ppEnumMoniker: IEnumMoniker; dwFlags: DWORD): HRESULT; stdcall;
  end;

  IPropertyBag = interface(IUnknown)
    ['{55272A00-42CB-11CE-8135-00AA004BB851}']
    function Read(pszPropName: PWideChar; var pVar: OleVariant;
      pErrorLog: Pointer): HRESULT; stdcall;
    function Write(pszPropName: PWideChar; var pVar: OleVariant): HRESULT; stdcall;
  end;

function MonikerDisplayName(const M: IMoniker): string;
var
  Bind: IBindCtx;
  P: PWideChar;
begin
  Result := '';
  if Failed(CreateBindCtx(0, Bind)) then Exit;
  if Failed(M.GetDisplayName(Bind, nil, P)) then Exit;
  if P <> nil then
  begin
    Result := string(P);
    CoTaskMemFree(P);
  end;
end;

function FriendlyFromMoniker(const ADeviceId: string;
  AIndex: Integer): string;
// Quando IPropertyBag falha (acontece em alguns drivers), monta um
// nome legivel a partir do moniker display name. Exemplo:
//   @device:pnp:\\?\usb#vid_046d&pid_085e&mi_00#6&...
//   vid_046d -> Logitech, pid_085e -> C920 (nao mapeamos pid, so vid)
// Fallback: "Webcam N".
//
// Forward declaracao em interface — implementacao logo abaixo.
const
  VENDORS: array[0..7] of array[0..1] of string = (
    ('vid_046d', 'Logitech'),
    ('vid_045e', 'Microsoft'),
    ('vid_05ac', 'Apple'),
    ('vid_1bcf', 'Sunplus'),
    ('vid_174f', 'Syntek'),
    ('vid_04f2', 'Chicony'),
    ('vid_0c45', 'SONiX'),
    ('vid_13d3', 'IMC')
  );
var
  i: Integer;
  Lower: string;
begin
  Result := '';
  Lower := LowerCase(ADeviceId);
  for i := 0 to High(VENDORS) do
    if Pos(VENDORS[i, 0], Lower) > 0 then
    begin
      Result := VENDORS[i, 1] + ' Webcam';
      if AIndex > 0 then
        Result := Result + Format(' (%d)', [AIndex + 1]);
      Exit;
    end;
  Result := Format('Webcam %d', [AIndex + 1]);
end;

function ReadFriendlyName(const M: IMoniker): string;
// Tenta IPropertyBag via varias chaves. Retorna '' se nada funcionar
// (caller usa FriendlyFromMoniker como fallback).
const
  KEYS: array[0..2] of string = ('FriendlyName', 'Description', 'Name');
var
  Bag: IPropertyBag;
  Bind: IBindCtx;
  V: OleVariant;
  HR: HRESULT;
  i: Integer;
  S: string;
begin
  Result := '';
  if Failed(CreateBindCtx(0, Bind)) then Exit;
  HR := M.BindToStorage(Bind, nil, IID_IPropertyBag, Bag);
  if Failed(HR) or (Bag = nil) then Exit;
  for i := 0 to High(KEYS) do
  begin
    V := '';
    HR := Bag.Read(PWideChar(KEYS[i]), V, nil);
    if Succeeded(HR) then
    begin
      S := VarToStr(V);
      if S <> '' then
      begin
        Result := S;
        Exit;
      end;
    end;
  end;
end;

function EnumerateWebcams: TWebcamInfoArray;
var
  DevEnum: ICreateDevEnum;
  EnumMon: IEnumMoniker;
  Moniker: IMoniker;
  Fetched: ULONG;
  Info: TWebcamInfo;
begin
  // Hook de teste: se setado, ignora DirectShow e retorna o mock.
  if Assigned(WebcamEnumOverride) then
  begin
    Result := WebcamEnumOverride();
    Exit;
  end;
  SetLength(Result, 0);
  CoInitializeEx(nil, 0);
  if Failed(CoCreateInstance(CLSID_SystemDeviceEnum, nil, CLSCTX_INPROC_SERVER,
    IID_ICreateDevEnum, DevEnum)) then Exit;
  if Failed(DevEnum.CreateClassEnumerator(CLSID_VideoInputDeviceCategory,
    EnumMon, 0)) or (EnumMon = nil) then Exit;

  while EnumMon.Next(1, Moniker, @Fetched) = S_OK do
  begin
    if Moniker = nil then Break;
    var Display := MonikerDisplayName(Moniker);
    Log('Webcam enum: Display="%s"', [Display]);
    // Filtra webcams "software" (OBS Virtual Camera, NVIDIA Broadcast,
    // Streamlabs, etc). Hardware comeca com "@device:pnp:".
    if Pos('@device:sw:', Display) = 1 then
    begin
      Log('Webcam enum: pulando software device.');
      Moniker := nil;
      Continue;
    end;
    Info.Name := ReadFriendlyName(Moniker);
    if Info.Name = '' then
      Info.Name := FriendlyFromMoniker(Display, Length(Result));
    // OBS dshow_input espera video_device_id no formato
    // "<friendly_name>:<device_path>" — DecodeDeviceId em
    // dshow-base.cpp splita no primeiro ':'. Path = moniker sem
    // "@device:pnp:" prefix.
    var Path := Display;
    if Pos('@device:pnp:', Path) = 1 then
      Delete(Path, 1, Length('@device:pnp:'));
    Info.DeviceId := Info.Name + ':' + Path;
    Log('Webcam enum: Name="%s" DeviceId="%s"',
      [Info.Name, Info.DeviceId]);
    Info.Width := 1280;
    Info.Height := 720;
    if Path <> '' then
    begin
      SetLength(Result, Length(Result) + 1);
      Result[High(Result)] := Info;
    end;
    Moniker := nil;
  end;
end;

end.

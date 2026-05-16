(*
  WinAudioMeter - enumera dispositivos de audio (mics e saidas) via
  WASAPI/MMDevice e expoe IAudioMeterInformation pra ler peak por
  dispositivo. Substitui o evento InputVolumeMeters do OBS.
*)
unit WinAudioMeter;

interface

uses
  System.SysUtils;

type
  TAudioDeviceKind = (adkInput, adkOutput);

  TAudioDeviceInfo = record
    DeviceId: string;     // unique id (do MMDevice)
    Name: string;         // friendly name
    Kind: TAudioDeviceKind;
    IsDefault: Boolean;
  end;
  TAudioDeviceInfoArray = TArray<TAudioDeviceInfo>;

  TAudioLevel = record
    DeviceId: string;
    PeakLevel: Single;    // 0..1 — peak total (max dos canais).
    PeakLeft:  Single;    // 0..1 — canal L (estereo) ou mesmo que PeakLevel se mono.
    PeakRight: Single;    // 0..1 — canal R (estereo) ou mesmo que PeakLevel se mono.
    Channels:  Integer;   // 1 = mono, 2+ = estereo (so L/R sao expostos).
  end;
  TAudioLevelArray = TArray<TAudioLevel>;

procedure InitAudio;
procedure DoneAudio;
function EnumerateAudioDevices: TAudioDeviceInfoArray;
function ReadPeakLevels: TAudioLevelArray;
// Re-enumera dispositivos do zero. Use apos hot-plug (USB connect/
// disconnect) — `EnumerateAudioDevices` cacheia o resultado da
// primeira chamada e nao detecta mudancas sozinho.
procedure RefreshAudioDevices;

implementation

uses
  Winapi.Windows, Winapi.ActiveX,
  System.Generics.Collections;

const
  CLSID_MMDeviceEnumerator: TGUID = '{BCDE0395-E52F-467C-8E3D-C4579291692E}';
  IID_IMMDeviceEnumerator:  TGUID = '{A95664D2-9614-4F35-A746-DE8DB63617E6}';
  IID_IAudioMeterInformation: TGUID = '{C02216F6-8C67-4B5B-9D00-D008E73E0064}';
  IID_IAudioClient:           TGUID = '{1CB9AD4C-DBFA-4C32-B178-C2F568A703B2}';

  eRender   = 0;
  eCapture  = 1;
  DEVICE_STATE_ACTIVE = $00000001;

  // AUDCLNT_SHAREMODE_SHARED = 0
  // Buffer de 200ms — suficiente; nao consumimos, so queremos a sessao
  // viva pra o IAudioMeterInformation funcionar em endpoints de captura.
  AUDCLNT_BUFFER_HNS    = 2_000_000; // 200ms em unidades de 100ns
  AUDCLNT_SHAREMODE_SHARED = 0;

  // PROPERTYKEY do friendly name. Definido em Functiondiscoverykeys_devpkey.h.
  // PKEY_Device_FriendlyName = (a45c254e-df1c-4efd-8020-67d146a850e0), pid 14
  PKEY_Device_FriendlyName: TGUID = '{A45C254E-DF1C-4EFD-8020-67D146A850E0}';

type
  // Subset do PROPVARIANT que precisamos.
  PROPERTYKEY = record
    fmtid: TGUID;
    pid: DWORD;
  end;

  TPropVariant = record
    vt: Word;
    wReserved1, wReserved2, wReserved3: Word;
    case Integer of
      0: (lVal: LongInt);
      1: (uiVal: Word);
      2: (pwszVal: PWideChar);
      3: (pad: array[0..3] of Int64);
  end;

  IPropertyStore = interface(IUnknown)
    ['{886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99}']
    function GetCount(out cProps: DWORD): HRESULT; stdcall;
    function GetAt(iProp: DWORD; out pkey: PROPERTYKEY): HRESULT; stdcall;
    function GetValue(const key: PROPERTYKEY; out pv: TPropVariant): HRESULT; stdcall;
    function SetValue(const key: PROPERTYKEY; const propvar: TPropVariant): HRESULT; stdcall;
    function Commit: HRESULT; stdcall;
  end;

  IAudioMeterInformation = interface(IUnknown)
    ['{C02216F6-8C67-4B5B-9D00-D008E73E0064}']
    function GetPeakValue(out pfPeak: Single): HRESULT; stdcall;
    function GetMeteringChannelCount(out pnChannelCount: Cardinal): HRESULT; stdcall;
    function GetChannelsPeakValues(u32ChannelCount: Cardinal;
      afPeakValues: PSingle): HRESULT; stdcall;
    function QueryHardwareSupport(out pdwHardwareSupportMask: DWORD): HRESULT; stdcall;
  end;

  IAudioClient = interface(IUnknown)
    ['{1CB9AD4C-DBFA-4C32-B178-C2F568A703B2}']
    function Initialize(ShareMode: DWORD; StreamFlags: DWORD;
      hnsBufferDuration: Int64; hnsPeriodicity: Int64;
      pFormat: Pointer; AudioSessionGuid: PGUID): HRESULT; stdcall;
    function GetBufferSize(out NumBufferFrames: UINT32): HRESULT; stdcall;
    function GetStreamLatency(out hnsLatency: Int64): HRESULT; stdcall;
    function GetCurrentPadding(out NumPaddingFrames: UINT32): HRESULT; stdcall;
    function IsFormatSupported(ShareMode: DWORD; pFormat: Pointer;
      out ppClosestMatch: Pointer): HRESULT; stdcall;
    function GetMixFormat(out ppDeviceFormat: Pointer): HRESULT; stdcall;
    function GetDevicePeriod(out hnsDefaultDevicePeriod: Int64;
      out hnsMinimumDevicePeriod: Int64): HRESULT; stdcall;
    function Start: HRESULT; stdcall;
    function Stop: HRESULT; stdcall;
    function Reset: HRESULT; stdcall;
    function SetEventHandle(eventHandle: THandle): HRESULT; stdcall;
    function GetService(const iid: TGUID; out Service): HRESULT; stdcall;
  end;

  IMMDevice = interface(IUnknown)
    ['{D666063F-1587-4E43-81F1-B948E807363F}']
    function Activate(const iid: TGUID; dwClsCtx: DWORD;
      pActivationParams: Pointer; out ppInterface): HRESULT; stdcall;
    function OpenPropertyStore(stgmAccess: DWORD;
      out ppProperties: IPropertyStore): HRESULT; stdcall;
    function GetId(out ppstrId: PWideChar): HRESULT; stdcall;
    function GetState(out pdwState: DWORD): HRESULT; stdcall;
  end;

  IMMDeviceCollection = interface(IUnknown)
    ['{0BD7A1BE-7A1A-44DB-8397-CC5392387B5E}']
    function GetCount(out pcDevices: Cardinal): HRESULT; stdcall;
    function Item(nDevice: Cardinal; out ppDevice: IMMDevice): HRESULT; stdcall;
  end;

  IMMDeviceEnumerator = interface(IUnknown)
    ['{A95664D2-9614-4F35-A746-DE8DB63617E6}']
    function EnumAudioEndpoints(dataFlow, dwStateMask: DWORD;
      out devices: IMMDeviceCollection): HRESULT; stdcall;
    function GetDefaultAudioEndpoint(dataFlow, role: DWORD;
      out endpoint: IMMDevice): HRESULT; stdcall;
    function GetDevice(pwstrId: PWideChar; out endpoint: IMMDevice): HRESULT; stdcall;
    function RegisterEndpointNotificationCallback(client: IUnknown): HRESULT; stdcall;
    function UnregisterEndpointNotificationCallback(client: IUnknown): HRESULT; stdcall;
  end;

  TMeterEntry = record
    Info: TAudioDeviceInfo;
    Meter: IAudioMeterInformation;
    // Pra mics: precisamos manter uma IAudioClient ativa, senao
    // IAudioMeterInformation.GetPeakValue retorna 0 (nao ha sessao
    // de captura). Speakers nao precisam — o Windows sempre tem
    // sessao de render ativa pro mix do sistema.
    Client: IAudioClient;
  end;

var
  Enumerator: IMMDeviceEnumerator = nil;
  Cache: TArray<TMeterEntry>;

procedure FreePropVariantString(var APV: TPropVariant);
begin
  // Em produção usariamos PropVariantClear; aqui o pwszVal e alocado
  // pelo COM e liberado por CoTaskMemFree.
  if APV.pwszVal <> nil then
    CoTaskMemFree(APV.pwszVal);
end;

function GetFriendlyName(const Dev: IMMDevice): string;
var
  Store: IPropertyStore;
  Key: PROPERTYKEY;
  PV: TPropVariant;
begin
  Result := '';
  if Dev = nil then Exit;
  if Failed(Dev.OpenPropertyStore(0 {STGM_READ}, Store)) then Exit;
  Key.fmtid := PKEY_Device_FriendlyName;
  Key.pid := 14;
  ZeroMemory(@PV, SizeOf(PV));
  if Succeeded(Store.GetValue(Key, PV)) then
  begin
    if PV.pwszVal <> nil then
      Result := string(PV.pwszVal);
    FreePropVariantString(PV);
  end;
end;

function GetDeviceId(const Dev: IMMDevice): string;
var P: PWideChar;
begin
  Result := '';
  if Dev = nil then Exit;
  if Succeeded(Dev.GetId(P)) and (P <> nil) then
  begin
    Result := string(P);
    CoTaskMemFree(P);
  end;
end;

procedure InitAudio;
begin
  CoInitializeEx(nil, 0);
  if Enumerator = nil then
    CoCreateInstance(CLSID_MMDeviceEnumerator, nil,
      CLSCTX_INPROC_SERVER, IID_IMMDeviceEnumerator, Enumerator);
end;

procedure DoneAudio;
var i: Integer;
begin
  // Stop nos IAudioClients de captura antes de limpar — caso contrario
  // o WASAPI mantem refs internas ate o stream timeout.
  for i := 0 to High(Cache) do
    if Cache[i].Client <> nil then
    try Cache[i].Client.Stop; except end;
  SetLength(Cache, 0);
  Enumerator := nil;
end;

procedure StartCaptureForMeter(const Dev: IMMDevice;
  out Client: IAudioClient);
// Ativa um IAudioClient de captura em shared mode, inicializa com o
// mix format do device e da Start. Nao consumimos o buffer — o sistema
// faz overflow, mas o IAudioMeterInformation continua atualizando.
// Sem isso, GetPeakValue retorna sempre 0 em endpoints de captura.
var
  pFormat: Pointer;
  HR: HRESULT;
begin
  Client := nil;
  if Dev = nil then Exit;
  if Failed(Dev.Activate(IID_IAudioClient, CLSCTX_INPROC_SERVER, nil,
    Client)) or (Client = nil) then Exit;

  pFormat := nil;
  if Failed(Client.GetMixFormat(pFormat)) or (pFormat = nil) then
  begin
    Client := nil;
    Exit;
  end;
  try
    HR := Client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0,
      AUDCLNT_BUFFER_HNS, 0, pFormat, nil);
    if Failed(HR) then
    begin
      Client := nil;
      Exit;
    end;
    if Failed(Client.Start) then
      Client := nil;
  finally
    if pFormat <> nil then CoTaskMemFree(pFormat);
  end;
end;

procedure RebuildCache;
var
  Coll: IMMDeviceCollection;
  Dev, DefDev: IMMDevice;
  Cnt, i: Cardinal;
  Entry: TMeterEntry;
  DefId: string;
  Kinds: array[0..1] of TAudioDeviceKind;
  Flows: array[0..1] of DWORD;
  k: Integer;
  Meter: IAudioMeterInformation;
begin
  SetLength(Cache, 0);
  if Enumerator = nil then Exit;

  Kinds[0] := adkInput;  Flows[0] := eCapture;
  Kinds[1] := adkOutput; Flows[1] := eRender;

  for k := 0 to 1 do
  begin
    DefId := '';
    if Succeeded(Enumerator.GetDefaultAudioEndpoint(Flows[k], 0, DefDev)) then
      DefId := GetDeviceId(DefDev);

    if Failed(Enumerator.EnumAudioEndpoints(Flows[k],
      DEVICE_STATE_ACTIVE, Coll)) then Continue;
    Cnt := 0;
    Coll.GetCount(Cnt);
    // Cnt e Cardinal — se for 0 (nenhum device do tipo), Cnt-1
    // underflowa pra $FFFFFFFF e dispara EIntOverflow.
    if Cnt = 0 then Continue;
    for i := 0 to Cnt - 1 do
    begin
      if Failed(Coll.Item(i, Dev)) or (Dev = nil) then Continue;
      Entry.Info.DeviceId := GetDeviceId(Dev);
      Entry.Info.Name := GetFriendlyName(Dev);
      Entry.Info.Kind := Kinds[k];
      Entry.Info.IsDefault := SameText(Entry.Info.DeviceId, DefId);
      Meter := nil;
      if Succeeded(Dev.Activate(IID_IAudioMeterInformation,
        CLSCTX_INPROC_SERVER, nil, Meter)) then
        Entry.Meter := Meter
      else
        Entry.Meter := nil;
      Entry.Client := nil;
      // Pra inputs: ativa um IAudioClient em modo shared so pra manter
      // a sessao de captura viva. Sem isso o meter da 0 sempre.
      if (Kinds[k] = adkInput) and (Entry.Meter <> nil) then
        StartCaptureForMeter(Dev, Entry.Client);
      SetLength(Cache, Length(Cache) + 1);
      Cache[High(Cache)] := Entry;
    end;
  end;
end;

function EnumerateAudioDevices: TAudioDeviceInfoArray;
var i: Integer;
begin
  if Length(Cache) = 0 then RebuildCache;
  SetLength(Result, Length(Cache));
  for i := 0 to High(Cache) do
    Result[i] := Cache[i].Info;
end;

procedure RefreshAudioDevices;
var i: Integer;
begin
  // Stop nos IAudioClients antigos (mics tinham sessao de captura
  // aberta). Sem stop, refs internas do WASAPI ficam presas e a
  // RebuildCache nao consegue ativar o cliente do novo device.
  for i := 0 to High(Cache) do
    if Cache[i].Client <> nil then
      try Cache[i].Client.Stop; except end;
  SetLength(Cache, 0);
  // RebuildCache eh lazy via EnumerateAudioDevices — proxima chamada
  // re-enumera. Se quiser forcar agora: descomenta a linha abaixo.
  // RebuildCache;
end;

function ReadPeakLevels: TAudioLevelArray;
// Le picos por device. Tenta extrair canais L/R individualmente via
// GetChannelsPeakValues — funciona pra devices estereo (a maioria dos
// alto-falantes e a maior parte dos mics USB). Mono fallback usa
// GetPeakValue e duplica o mesmo valor em L/R.
const
  MAX_CH = 8;  // 7.1 e o limite OBS — buffer grande chega.
var
  i, j, ChCount: Integer;
  Peak: Single;
  ChCard: Cardinal;
  Buf: array[0..MAX_CH - 1] of Single;
  Lvl: TAudioLevel;
begin
  if Length(Cache) = 0 then RebuildCache;
  SetLength(Result, 0);
  for i := 0 to High(Cache) do
  begin
    if Cache[i].Meter = nil then Continue;

    ChCount := 1;
    ChCard := 0;
    if Succeeded(Cache[i].Meter.GetMeteringChannelCount(ChCard)) then
      ChCount := Integer(ChCard);
    if ChCount < 1 then ChCount := 1;
    if ChCount > MAX_CH then ChCount := MAX_CH;

    FillChar(Buf, SizeOf(Buf), 0);
    Lvl.DeviceId := Cache[i].Info.DeviceId;
    Lvl.Channels := ChCount;

    if (ChCount >= 2) and
       Succeeded(Cache[i].Meter.GetChannelsPeakValues(ChCount, @Buf[0])) then
    begin
      // L/R sao os canais 0 e 1. Peak total = max de todos.
      Lvl.PeakLeft  := Buf[0];
      Lvl.PeakRight := Buf[1];
      Peak := 0;
      for j := 0 to ChCount - 1 do
        if Buf[j] > Peak then Peak := Buf[j];
      Lvl.PeakLevel := Peak;
    end
    else
    begin
      // Fallback mono: usa GetPeakValue e duplica em L/R.
      Peak := 0;
      if Failed(Cache[i].Meter.GetPeakValue(Peak)) then Continue;
      Lvl.PeakLevel := Peak;
      Lvl.PeakLeft  := Peak;
      Lvl.PeakRight := Peak;
    end;

    SetLength(Result, Length(Result) + 1);
    Result[High(Result)] := Lvl;
  end;
end;

end.

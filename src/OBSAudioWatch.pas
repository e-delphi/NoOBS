(*
  OBSAudioWatch -- detecta hot-plug de dispositivos de audio (mic ou
  saida) usando IMMNotificationClient da MMDevice API do Windows.
  Notificacao e nativa: chega em ~ms, sem polling.

  Uso:
    OBSAudioWatch.Start(
      procedure(const AKind: TAudioDeviceChange; const ADeviceId: string)
      begin
        // chamada na thread do WASAPI; nao toque na UI direto.
        // marshale via TThread.Queue se precisar.
      end);
    ...
    OBSAudioWatch.Stop;

  Eventos cobertos:
    Added         -- novo dispositivo apareceu (USB plugado).
    Removed       -- dispositivo sumiu (USB desplugado).
    StateChanged  -- ativo/desativado pelo SO.
    DefaultChanged-- default mudou.

  Property change e ignorado (dispara muito).
*)
unit OBSAudioWatch;

interface

uses
  System.SysUtils;

type
  TAudioDeviceChangeKind = (
    adcAdded, adcRemoved, adcStateChanged, adcDefaultChanged
  );

  TAudioDeviceChangeProc = reference to procedure(
    AKind: TAudioDeviceChangeKind; const ADeviceId: string);

procedure Start(const ACallback: TAudioDeviceChangeProc);
procedure Stop;

implementation

uses
  Winapi.Windows,
  Winapi.ActiveX,
  OBSLog;

const
  CLSID_MMDeviceEnumerator: TGUID = '{BCDE0395-E52F-467C-8E3D-C4579291692E}';
  IID_IMMDeviceEnumerator:  TGUID = '{A95664D2-9614-4F35-A746-DE8DB63617E6}';
  IID_IMMNotificationClient:TGUID = '{7991EEC9-7E89-4D85-8390-6C703CEC60C0}';

type
  // Subset minimo do IMMDeviceEnumerator que precisamos.
  IMMNotificationClient = interface(IUnknown)
    ['{7991EEC9-7E89-4D85-8390-6C703CEC60C0}']
    function OnDeviceStateChanged(pwstrDeviceId: PWideChar;
      dwNewState: DWORD): HRESULT; stdcall;
    function OnDeviceAdded(pwstrDeviceId: PWideChar): HRESULT; stdcall;
    function OnDeviceRemoved(pwstrDeviceId: PWideChar): HRESULT; stdcall;
    function OnDefaultDeviceChanged(flow, role: DWORD;
      pwstrDefaultDeviceId: PWideChar): HRESULT; stdcall;
    function OnPropertyValueChanged(pwstrDeviceId: PWideChar;
      const key: TGUID): HRESULT; stdcall;
  end;

  IMMDevice = interface(IUnknown)
    ['{D666063F-1587-4E43-81F1-B948E807363F}']
  end;

  IMMDeviceCollection = interface(IUnknown)
    ['{0BD7A1BE-7A1A-44DB-8397-CC5392387B5E}']
  end;

  IMMDeviceEnumerator = interface(IUnknown)
    ['{A95664D2-9614-4F35-A746-DE8DB63617E6}']
    function EnumAudioEndpoints(dataFlow, dwStateMask: DWORD;
      out devices: IMMDeviceCollection): HRESULT; stdcall;
    function GetDefaultAudioEndpoint(dataFlow, role: DWORD;
      out endpoint: IMMDevice): HRESULT; stdcall;
    function GetDevice(pwstrId: PWideChar; out endpoint: IMMDevice): HRESULT; stdcall;
    function RegisterEndpointNotificationCallback(
      const client: IMMNotificationClient): HRESULT; stdcall;
    function UnregisterEndpointNotificationCallback(
      const client: IMMNotificationClient): HRESULT; stdcall;
  end;

  // Implementacao do callback. Nao usa TInterfacedObject porque precisa
  // sobreviver entre Register e Unregister sem o ARC do Delphi liberar
  // sozinho. Faz refcount manual.
  TNotifClient = class(TObject, IUnknown, IMMNotificationClient)
  private
    FRef: Integer;
    FCallback: TAudioDeviceChangeProc;
  public
    constructor Create(const ACallback: TAudioDeviceChangeProc);
    function QueryInterface(const IID: TGUID; out Obj): HRESULT; stdcall;
    function _AddRef: Integer; stdcall;
    function _Release: Integer; stdcall;
    function OnDeviceStateChanged(pwstrDeviceId: PWideChar;
      dwNewState: DWORD): HRESULT; stdcall;
    function OnDeviceAdded(pwstrDeviceId: PWideChar): HRESULT; stdcall;
    function OnDeviceRemoved(pwstrDeviceId: PWideChar): HRESULT; stdcall;
    function OnDefaultDeviceChanged(flow, role: DWORD;
      pwstrDefaultDeviceId: PWideChar): HRESULT; stdcall;
    function OnPropertyValueChanged(pwstrDeviceId: PWideChar;
      const key: TGUID): HRESULT; stdcall;
  end;

var
  Enumerator: IMMDeviceEnumerator = nil;
  Notif: TNotifClient = nil;
  NotifAsClient: IMMNotificationClient = nil;

{ TNotifClient }

constructor TNotifClient.Create(const ACallback: TAudioDeviceChangeProc);
begin
  inherited Create;
  FRef := 0;
  FCallback := ACallback;
end;

function TNotifClient.QueryInterface(const IID: TGUID; out Obj): HRESULT;
begin
  if IsEqualGUID(IID, IUnknown) or
     IsEqualGUID(IID, IID_IMMNotificationClient) then
  begin
    Pointer(Obj) := Self;
    _AddRef;
    Result := S_OK;
  end
  else
  begin
    Pointer(Obj) := nil;
    Result := E_NOINTERFACE;
  end;
end;

function TNotifClient._AddRef: Integer; stdcall;
begin
  Result := InterlockedIncrement(FRef);
end;

function TNotifClient._Release: Integer; stdcall;
begin
  Result := InterlockedDecrement(FRef);
  if Result = 0 then Free;
end;

procedure TNotifClient_Notify(Self: TNotifClient;
  AKind: TAudioDeviceChangeKind; const ADeviceId: string);
begin
  if Assigned(Self) and Assigned(Self.FCallback) then
  try
    Self.FCallback(AKind, ADeviceId);
  except
    on E: Exception do
      Log('AudioWatch: callback levantou: %s', [E.Message]);
  end;
end;

function TNotifClient.OnDeviceStateChanged(pwstrDeviceId: PWideChar;
  dwNewState: DWORD): HRESULT;
begin
  TNotifClient_Notify(Self, adcStateChanged, string(pwstrDeviceId));
  Result := S_OK;
end;

function TNotifClient.OnDeviceAdded(pwstrDeviceId: PWideChar): HRESULT;
begin
  TNotifClient_Notify(Self, adcAdded, string(pwstrDeviceId));
  Result := S_OK;
end;

function TNotifClient.OnDeviceRemoved(pwstrDeviceId: PWideChar): HRESULT;
begin
  TNotifClient_Notify(Self, adcRemoved, string(pwstrDeviceId));
  Result := S_OK;
end;

function TNotifClient.OnDefaultDeviceChanged(flow, role: DWORD;
  pwstrDefaultDeviceId: PWideChar): HRESULT;
begin
  TNotifClient_Notify(Self, adcDefaultChanged, string(pwstrDefaultDeviceId));
  Result := S_OK;
end;

function TNotifClient.OnPropertyValueChanged(pwstrDeviceId: PWideChar;
  const key: TGUID): HRESULT;
begin
  // Ignorado -- dispara para tudo (volume, formato, etc), nao serve
  // pra detectar plug/unplug.
  Result := S_OK;
end;

procedure Start(const ACallback: TAudioDeviceChangeProc);
var
  HR: HRESULT;
begin
  if Notif <> nil then Exit;
  HR := CoCreateInstance(CLSID_MMDeviceEnumerator, nil, CLSCTX_INPROC_SERVER,
    IID_IMMDeviceEnumerator, Enumerator);
  if Failed(HR) or (Enumerator = nil) then
  begin
    Log('AudioWatch: CoCreateInstance MMDeviceEnumerator falhou (%x)', [HR]);
    Exit;
  end;

  Notif := TNotifClient.Create(ACallback);
  NotifAsClient := Notif;  // assignment a interface chama _AddRef.

  HR := Enumerator.RegisterEndpointNotificationCallback(NotifAsClient);
  if Failed(HR) then
  begin
    Log('AudioWatch: RegisterEndpointNotificationCallback falhou (%x)', [HR]);
    NotifAsClient := nil;
    Notif := nil;  // _Release o solta
    Enumerator := nil;
    Exit;
  end;

  Log('AudioWatch: monitorando hot-plug de dispositivos de audio.');
end;

procedure Stop;
begin
  if (Enumerator <> nil) and (NotifAsClient <> nil) then
    Enumerator.UnregisterEndpointNotificationCallback(NotifAsClient);
  NotifAsClient := nil;
  Notif := nil;
  Enumerator := nil;
end;

end.

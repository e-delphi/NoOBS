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
    // True se o dispositivo esta atras de um adapter Bluetooth — usado
    // pra avisar o user sobre limitacao do perfil HFP (quando o mic BT
    // e ativado, qualidade do audio de saida cai pra mono 8/16 kHz e
    // o volume costuma ir pro maximo automaticamente).
    IsBluetooth: Boolean;
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

// Resolve um device id (vindo de IMMNotificationClient) pro nome amigavel.
// Tenta o cache primeiro (rapido); se nao achar, consulta WASAPI direto
// (suporta devices que acabaram de aparecer ou ja foram removidos do
// cache mas ainda estao no MMDeviceEnumerator). Retorna '' se falhar.
function ResolveDeviceName(const ADeviceId: string): string;

implementation

uses
  Winapi.Windows, Winapi.ActiveX,
  System.Generics.Collections,
  System.StrUtils,
  System.SyncObjs,
  OBSLog;

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
  // PKEY_Device_EnumeratorName = (same fmtid), pid 24 — retorna o nome do
  // enumerator do device manager: "USB", "HDAUDIO", "BTHENUM" (Bluetooth),
  // etc. Usamos pra detectar dispositivos Bluetooth e mostrar warning de
  // limitacao do perfil HFP na UI.
  PKEY_Device_FriendlyName:   TGUID = '{A45C254E-DF1C-4EFD-8020-67D146A850E0}';
  PKEY_Device_EnumeratorName: TGUID = '{A45C254E-DF1C-4EFD-8020-67D146A850E0}';

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
  // Lock serializa acesso ao Cache e RebuildCache. Sem isso, worker thread
  // (DoRefreshAudio) e main thread (TIMER_AUDIO_METER -> ReadPeakLevels)
  // podem entrar em RebuildCache concorrentemente, ambos limpam o cache
  // e iteram WASAPI em paralelo — resultado: devices duplicados.
  CacheLock: TCriticalSection = nil;

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

// Le PKEY_Device_EnumeratorName via property store e retorna True se
// o device esta atras do bus Bluetooth (enumerator = "BTHENUM").
// Fallback adicional: se o friendly name contem "Hands-Free" (perfil
// HFP), tambem marca como BT — alguns drivers expoem o sub-endpoint
// HFP como device separado sem que o enumerator seja BTHENUM diretamente.
function IsBluetoothDevice(const Dev: IMMDevice; const AFriendlyName: string): Boolean;
var
  Store: IPropertyStore;
  Key: PROPERTYKEY;
  PV: TPropVariant;
  EnumName: string;
  LowerName: string;
begin
  Result := False;
  if Dev = nil then Exit;

  if Succeeded(Dev.OpenPropertyStore(0 {STGM_READ}, Store)) then
  begin
    Key.fmtid := PKEY_Device_EnumeratorName;
    Key.pid := 24;
    ZeroMemory(@PV, SizeOf(PV));
    if Succeeded(Store.GetValue(Key, PV)) then
    begin
      if PV.pwszVal <> nil then
        EnumName := string(PV.pwszVal);
      FreePropVariantString(PV);
      if SameText(EnumName, 'BTHENUM') then Exit(True);
    end;
  end;

  // Fallback heuristico — varios drivers nao reportam BTHENUM no
  // endpoint mas colocam "Hands-Free" / "Bluetooth" no nome.
  LowerName := LowerCase(AFriendlyName);
  if (Pos('hands-free', LowerName) > 0) or
     (Pos('hands free', LowerName) > 0) or
     (Pos('bluetooth',  LowerName) > 0) then
    Result := True;
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
  if CacheLock = nil then
    CacheLock := TCriticalSection.Create;
  if Enumerator = nil then
    CoCreateInstance(CLSID_MMDeviceEnumerator, nil,
      CLSCTX_INPROC_SERVER, IID_IMMDeviceEnumerator, Enumerator);
end;

procedure DoneAudio;
var i: Integer;
begin
  if CacheLock <> nil then CacheLock.Enter;
  try
    // Stop + libera refs COM uma a uma (ver comentario em
    // RefreshAudioDevices sobre devices desconectados).
    for i := 0 to High(Cache) do
    begin
      if Cache[i].Client <> nil then
      begin
        try Cache[i].Client.Stop; except end;
        try Cache[i].Client := nil; except end;
      end;
      try Cache[i].Meter := nil; except end;
    end;
    SetLength(Cache, 0);
    try Enumerator := nil; except end;
  finally
    if CacheLock <> nil then CacheLock.Leave;
  end;
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

function HasDevice(const ADeviceId: string; AKind: TAudioDeviceKind): Boolean;
// Verifica se um device ja esta no Cache (mesmo id + mesmo kind).
// Necessario porque apos hot-plug rapido WASAPI pode retornar o
// mesmo endpoint duas vezes em estado transitorio.
var j: Integer;
begin
  for j := 0 to High(Cache) do
    if (Cache[j].Info.Kind = AKind) and
       SameText(Cache[j].Info.DeviceId, ADeviceId) then
      Exit(True);
  Result := False;
end;

procedure RebuildCache;
// IMPORTANTE: caller deve segurar CacheLock. Nao adquire o lock aqui
// porque EnumerateAudioDevices ja o segura (double-check pattern).
var
  Coll: IMMDeviceCollection;
  Dev, DefDev: IMMDevice;
  Cnt, i: Cardinal;
  Entry: TMeterEntry;
  DefId, DevId: string;
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
    Log('WinAudioMeter: %s default id="%s"',
      [IfThen(Kinds[k] = adkInput, 'IN ', 'OUT'),
       IfThen(DefId = '', '<none>', DefId)]);

    if Failed(Enumerator.EnumAudioEndpoints(Flows[k],
      DEVICE_STATE_ACTIVE, Coll)) then
    begin
      Log('WinAudioMeter: EnumAudioEndpoints falhou pra %s',
        [IfThen(Kinds[k] = adkInput, 'input', 'output')]);
      Continue;
    end;
    Cnt := 0;
    Coll.GetCount(Cnt);
    Log('WinAudioMeter: %s collection.GetCount=%d',
      [IfThen(Kinds[k] = adkInput, 'IN ', 'OUT'), Cnt]);
    // Cnt e Cardinal — se for 0 (nenhum device do tipo), Cnt-1
    // underflowa pra $FFFFFFFF e dispara EIntOverflow.
    if Cnt = 0 then
    begin
      Log('WinAudioMeter: 0 devices ativos de %s',
        [IfThen(Kinds[k] = adkInput, 'input', 'output')]);
      Continue;
    end;
    for i := 0 to Cnt - 1 do
    begin
      // Importante: zera Dev antes da chamada. Sem isso, se Coll.Item
      // falhar, Dev mantem o valor da iteracao anterior — usariamos o
      // mesmo device duas vezes (causa duplicata real).
      Dev := nil;
      if Failed(Coll.Item(i, Dev)) or (Dev = nil) then
      begin
        Log('WinAudioMeter: Coll.Item(%d) falhou em %s', [i,
          IfThen(Kinds[k] = adkInput, 'input', 'output')]);
        Continue;
      end;
      DevId := GetDeviceId(Dev);
      // Dedup: se o WASAPI retornar o mesmo endpoint duas vezes (acontece
      // em estado transitorio apos hot-plug), pula a segunda ocorrencia.
      // Sem isso, o user ve o dispositivo duplicado na lista da UI.
      if (DevId <> '') and HasDevice(DevId, Kinds[k]) then
      begin
        if Kinds[k] = adkInput then
          Log('WinAudioMeter: dedup IN  id="%s"', [DevId])
        else
          Log('WinAudioMeter: dedup OUT id="%s"', [DevId]);
        Continue;
      end;
      Entry.Info.DeviceId := DevId;
      Entry.Info.Name := GetFriendlyName(Dev);
      Entry.Info.Kind := Kinds[k];
      // IsDefault so se DefId existe — evita falso-positivo quando ambos
      // ficam vazios (DefId vazio + GetDeviceId falhou).
      Entry.Info.IsDefault := (DefId <> '') and SameText(DevId, DefId);
      Entry.Info.IsBluetooth := IsBluetoothDevice(Dev, Entry.Info.Name);
      if Kinds[k] = adkInput then
        Log('WinAudioMeter: + IN  "%s" id="%s"%s%s',
          [Entry.Info.Name, DevId,
           IfThen(Entry.Info.IsDefault,   ' [default]',   ''),
           IfThen(Entry.Info.IsBluetooth, ' [bluetooth]', '')])
      else
        Log('WinAudioMeter: + OUT "%s" id="%s"%s%s',
          [Entry.Info.Name, DevId,
           IfThen(Entry.Info.IsDefault,   ' [default]',   ''),
           IfThen(Entry.Info.IsBluetooth, ' [bluetooth]', '')]);
      Meter := nil;
      try
        if Succeeded(Dev.Activate(IID_IAudioMeterInformation,
          CLSCTX_INPROC_SERVER, nil, Meter)) then
          Entry.Meter := Meter
        else
          Entry.Meter := nil;
      except
        Entry.Meter := nil;
        Log('WinAudioMeter: Activate(Meter) AV em "%s"', [Entry.Info.Name]);
      end;
      Entry.Client := nil;
      // Pra inputs: ativa um IAudioClient em modo shared so pra manter
      // a sessao de captura viva. Sem isso o meter da 0 sempre.
      // try/except defensivo — device pode estar transicionando estado
      // (ex.: Bluetooth disconnecting) e WASAPI pode AV nativo.
      if (Kinds[k] = adkInput) and (Entry.Meter <> nil) then
        try StartCaptureForMeter(Dev, Entry.Client);
        except
          Entry.Client := nil;
          Log('WinAudioMeter: StartCaptureForMeter AV em "%s"', [Entry.Info.Name]);
        end;
      SetLength(Cache, Length(Cache) + 1);
      Cache[High(Cache)] := Entry;
    end;
  end;
end;

function EnumerateAudioDevices: TAudioDeviceInfoArray;
var i: Integer;
begin
  if CacheLock = nil then InitAudio;
  CacheLock.Enter;
  try
    // Double-check: outro thread pode ter populado o cache enquanto
    // estavamos esperando o lock.
    if Length(Cache) = 0 then RebuildCache;
    SetLength(Result, Length(Cache));
    for i := 0 to High(Cache) do
      Result[i] := Cache[i].Info;
  finally
    CacheLock.Leave;
  end;
end;

function ResolveDeviceName(const ADeviceId: string): string;
var
  i: Integer;
  Dev: IMMDevice;
  LocalEnumerator: IMMDeviceEnumerator;
begin
  Result := '';
  if ADeviceId = '' then Exit;
  // Cache primeiro — rapido, sob lock.
  if CacheLock <> nil then
  begin
    CacheLock.Enter;
    try
      for i := 0 to High(Cache) do
        if SameText(Cache[i].Info.DeviceId, ADeviceId) then
          Exit(Cache[i].Info.Name);
      LocalEnumerator := Enumerator;
    finally
      CacheLock.Leave;
    end;
  end
  else
    LocalEnumerator := Enumerator;
  // Fallback fora do lock: pergunta direto pro WASAPI. Util pra devices
  // que acabaram de aparecer (ainda nao estao no cache) ou eventos de
  // remocao que ja sairam do cache. Fora do lock pra nao bloquear o
  // timer de meters durante a chamada WASAPI.
  if LocalEnumerator = nil then Exit;
  if Failed(LocalEnumerator.GetDevice(PWideChar(ADeviceId), Dev)) or (Dev = nil) then
    Exit;
  Result := GetFriendlyName(Dev);
end;

procedure RefreshAudioDevices;
var i: Integer;
begin
  if CacheLock = nil then InitAudio;
  CacheLock.Enter;
  try
    // Stop + libera refs COM uma a uma com try/except. Necessario
    // porque devices podem ter sido desconectados (Bluetooth, USB)
    // durante a gravacao — o proxy COM pode estar corrompido e a
    // liberacao implicita do interface causa AV nativo.
    for i := 0 to High(Cache) do
    begin
      if Cache[i].Client <> nil then
      begin
        try Cache[i].Client.Stop; except end;
        try Cache[i].Client := nil; except end;
      end;
      try Cache[i].Meter := nil; except end;
    end;
    SetLength(Cache, 0);
    // RebuildCache eh lazy via EnumerateAudioDevices — proxima chamada
    // re-enumera. Se quiser forcar agora: descomenta a linha abaixo.
    // RebuildCache;
  finally
    CacheLock.Leave;
  end;
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
  SetLength(Result, 0);
  if CacheLock = nil then InitAudio;
  CacheLock.Enter;
  try
    if Length(Cache) = 0 then RebuildCache;
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
  finally
    CacheLock.Leave;
  end;
end;

initialization

finalization
  if CacheLock <> nil then
  begin
    CacheLock.Free;
    CacheLock := nil;
  end;

end.

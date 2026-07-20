{
  WinMicWatch — detecta uso do microfone por OUTROS apps (chamadas de
  Teams/WhatsApp/Zoom/etc.) via sessoes de audio WASAPI, pra auto-iniciar e
  auto-parar a gravacao.

  Enumera as sessoes de audio dos endpoints de CAPTURA
  (IAudioSessionManager2 -> IAudioSessionEnumerator). Uma sessao com estado
  Active, de um processo que casa com o filtro e que NAO e o nosso PID =
  "microfone em uso".

  Roda numa thread propria (COM em MTA), com poll de ~1s. Dispara o callback
  SO na mudanca de estado (em uso <-> livre). O callback roda NA THREAD do
  watcher — o consumidor deve marshalar pra main (TThread.Queue) se for tocar
  libobs/UI (modo full) ou postar mensagem pra janela (modo hibernate).

  Usado tanto pelo modo full (OBSBridge) quanto pela hibernacao (OBSHibernate).

  CRITICO: exclui o proprio PID. O NoOBS abre o microfone pros medidores de
  nivel e pra gravar (pegadinha #12), entao sem essa exclusao o mic pareceria
  SEMPRE em uso.

  Match do filtro: "contem" (case-insensitive) — o usuario pode digitar
  "teams" e casar com "ms-teams.exe". Filtro de monitorados vazio = qualquer
  app conta. Ha tambem um filtro de EXCECOES que so vale QUANDO a lista de
  monitorados esta vazia (monitora tudo, exceto estes): apps que casam sao
  ignorados mesmo usando o mic. Com monitorados preenchido, a propria lista ja
  e o filtro e as excecoes nao se aplicam. Excecoes vazias = nada ignorado.
}
unit WinMicWatch;

interface

type
  // Disparado na MUDANCA de estado. Roda na thread do watcher.
  TMicUseProc = procedure(AInUse: Boolean);

// Inicia o monitor (no-op se ja rodando). Nomes de processo separados por
// virgula (ou ; ou quebra de linha). AAppsFilter = apps monitorados (vazio =
// qualquer). AExceptFilter = apps ignorados mesmo usando o mic (prioridade).
procedure Start(const AAppsFilter, AExceptFilter: string; ACallback: TMicUseProc);
procedure Stop;
function IsRunning: Boolean;

implementation

uses
  Winapi.Windows, Winapi.ActiveX,
  System.SysUtils, System.Classes,
  OBSLog;

const
  CLSID_MMDeviceEnumerator:  TGUID = '{BCDE0395-E52F-467C-8E3D-C4579291692E}';
  IID_IMMDeviceEnumerator:   TGUID = '{A95664D2-9614-4F35-A746-DE8DB63617E6}';
  IID_IAudioSessionManager2: TGUID = '{77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F}';

  eCapture            = 1;
  DEVICE_STATE_ACTIVE = $00000001;

  // AudioSessionState
  AudioSessionStateActive = 1;

  PROCESS_QUERY_LIMITED_INFORMATION = $1000;
  POLL_MS  = 1000;
  SLICE_MS = 50;    // fatia do sleep pra reagir rapido ao Terminate

type
  // --- Interfaces COM (vtable espelhada de WinAudioMeter + MMDeviceAPI.h) ---
  IMMDevice = interface(IUnknown)
    ['{D666063F-1587-4E43-81F1-B948E807363F}']
    function Activate(const iid: TGUID; dwClsCtx: DWORD;
      pActivationParams: Pointer; out ppInterface): HRESULT; stdcall;
    function OpenPropertyStore(stgmAccess: DWORD; out ppProperties: IUnknown): HRESULT; stdcall;
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

  IAudioSessionControl = interface(IUnknown)
    ['{F4B1A599-7266-4319-A8CA-E70ACB11E8CD}']
    function GetState(out RetVal: DWORD): HRESULT; stdcall;
    function GetDisplayName(out RetVal: PWideChar): HRESULT; stdcall;
    function SetDisplayName(Value: PWideChar; EventContext: PGUID): HRESULT; stdcall;
    function GetIconPath(out RetVal: PWideChar): HRESULT; stdcall;
    function SetIconPath(Value: PWideChar; EventContext: PGUID): HRESULT; stdcall;
    function GetGroupingParam(out RetVal: TGUID): HRESULT; stdcall;
    function SetGroupingParam(Override: PGUID; EventContext: PGUID): HRESULT; stdcall;
    function RegisterAudioSessionNotification(NewNotifications: IUnknown): HRESULT; stdcall;
    function UnregisterAudioSessionNotification(NewNotifications: IUnknown): HRESULT; stdcall;
  end;

  IAudioSessionControl2 = interface(IAudioSessionControl)
    ['{BFB7FF88-7239-4FC9-8FA2-07C950BE9C6D}']
    function GetSessionIdentifier(out RetVal: PWideChar): HRESULT; stdcall;
    function GetSessionInstanceIdentifier(out RetVal: PWideChar): HRESULT; stdcall;
    function GetProcessId(out RetVal: DWORD): HRESULT; stdcall;
    function IsSystemSoundsSession: HRESULT; stdcall;
    function SetDuckingPreference(optOut: BOOL): HRESULT; stdcall;
  end;

  IAudioSessionEnumerator = interface(IUnknown)
    ['{E2F5BB11-0570-40CA-ACDD-3AA01277DEE8}']
    function GetCount(out SessionCount: Integer): HRESULT; stdcall;
    function GetSession(SessionCount: Integer;
      out Session: IAudioSessionControl): HRESULT; stdcall;
  end;

  IAudioSessionManager2 = interface(IUnknown)
    ['{77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F}']
    // Herdados de IAudioSessionManager (nao usados, mas preservam o vtable):
    function GetAudioSessionControl(AudioSessionGuid: PGUID; StreamFlags: DWORD;
      out SessionControl: IAudioSessionControl): HRESULT; stdcall;
    function GetSimpleAudioVolume(AudioSessionGuid: PGUID; StreamFlags: DWORD;
      out AudioVolume: IUnknown): HRESULT; stdcall;
    // IAudioSessionManager2:
    function GetSessionEnumerator(out SessionEnum: IAudioSessionEnumerator): HRESULT; stdcall;
  end;

  TMicWatchThread = class(TThread)
  private
    FApps: TArray<string>;
    FExcept: TArray<string>;
    FCallback: TMicUseProc;
    FLastInUse: Boolean;
  protected
    procedure Execute; override;
  public
    constructor Create(const AApps, AExcept: TArray<string>; ACallback: TMicUseProc);
  end;

// QueryFullProcessImageNameW nao esta em todas as versoes do Winapi.Windows.
function QueryFullProcessImageNameW(hProcess: THandle; dwFlags: DWORD;
  lpExeName: PWideChar; var lpdwSize: DWORD): BOOL; stdcall; external 'kernel32.dll';

var
  WatchThread: TMicWatchThread = nil;

// Divide o filtro em nomes minusculos (aceita , ; e quebras de linha).
function ParseApps(const AFilter: string): TArray<string>;
var
  Parts: TArray<string>;
  i, n: Integer;
  t: string;
begin
  SetLength(Result, 0);
  Parts := AFilter.Replace(';', ',').Replace(#13, ',').Replace(#10, ',').Split([',']);
  for i := 0 to High(Parts) do
  begin
    t := LowerCase(Trim(Parts[i]));
    if t = '' then Continue;
    n := Length(Result);
    SetLength(Result, n + 1);
    Result[n] := t;
  end;
end;

// Nome do executavel (basename, minusculo) de um PID. '' se nao der.
function ProcessNameFromPid(APid: DWORD): string;
var
  H: THandle;
  Buf: array[0..MAX_PATH - 1] of WideChar;
  Sz: DWORD;
begin
  Result := '';
  if APid = 0 then Exit;
  H := OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, APid);
  if H = 0 then Exit;
  try
    Sz := MAX_PATH;
    FillChar(Buf, SizeOf(Buf), 0);
    if QueryFullProcessImageNameW(H, 0, @Buf[0], Sz) then
      Result := LowerCase(ExtractFileName(string(Buf)));
  finally
    CloseHandle(H);
  end;
end;

// AName (ja minusculo) casa (substring) com algum item da lista? Lista vazia
// = NAO casa. O "vazio = qualquer app" dos monitorados fica no PollMicInUse;
// pra excecoes, vazio tem que significar "nada ignorado".
function AppInList(const AName: string; const AList: TArray<string>): Boolean;
var i: Integer;
begin
  for i := 0 to High(AList) do
    if (AList[i] <> '') and (Pos(AList[i], AName) > 0) then Exit(True);
  Result := False;
end;

// Faz uma varredura: existe alguma sessao de captura Active de outro app que
// casa com o filtro? Cria os objetos COM do zero (o enumerator de sessoes e
// um snapshot — precisa ser recriado a cada poll pra ver sessoes novas).
function PollMicInUse(const AWanted, AExcept: TArray<string>): Boolean;
var
  DevEnum: IMMDeviceEnumerator;
  Coll: IMMDeviceCollection;
  Dev: IMMDevice;
  Mgr: IAudioSessionManager2;
  SessEnum: IAudioSessionEnumerator;
  Ctrl: IAudioSessionControl;
  Ctrl2: IAudioSessionControl2;
  DevCount: Cardinal;
  d, i, Cnt: Integer;
  State, Pid, OwnPid: DWORD;
  PName, OwnName: string;
begin
  Result := False;
  if Failed(CoCreateInstance(CLSID_MMDeviceEnumerator, nil,
     CLSCTX_INPROC_SERVER, IID_IMMDeviceEnumerator, DevEnum)) then Exit;
  if Failed(DevEnum.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE, Coll)) then Exit;
  DevCount := 0;
  if Failed(Coll.GetCount(DevCount)) or (DevCount = 0) then Exit;
  OwnPid := GetCurrentProcessId;
  // Nome do nosso proprio exe (ex.: 'noobs.exe'). Excluimos por NOME, nao so
  // por PID: no full -> hibernate, o app full que acabou de sair tambem e
  // NoOBS.exe e a sessao WASAPI do mic dele (medidores, pegadinha #12) fica
  // "Active" por ~1-2s. Como o processo de hibernacao tem outro PID, sem essa
  // exclusao ele veria a sessao do full e dispararia gravacao sozinho ao
  // entrar em hibernacao. Teams/WhatsApp/etc. nao sao NoOBS.exe — nao afeta.
  OwnName := LowerCase(ExtractFileName(ParamStr(0)));

  for d := 0 to Integer(DevCount) - 1 do
  begin
    Dev := nil;
    if Failed(Coll.Item(Cardinal(d), Dev)) or (Dev = nil) then Continue;
    Mgr := nil;
    if Failed(Dev.Activate(IID_IAudioSessionManager2, CLSCTX_INPROC_SERVER, nil, Mgr))
       or (Mgr = nil) then Continue;
    SessEnum := nil;
    if Failed(Mgr.GetSessionEnumerator(SessEnum)) or (SessEnum = nil) then Continue;
    Cnt := 0;
    if Failed(SessEnum.GetCount(Cnt)) then Continue;
    for i := 0 to Cnt - 1 do
    begin
      Ctrl := nil;
      if Failed(SessEnum.GetSession(i, Ctrl)) or (Ctrl = nil) then Continue;
      State := 0;
      if Failed(Ctrl.GetState(State)) or (State <> AudioSessionStateActive) then Continue;
      if not Supports(Ctrl, IAudioSessionControl2, Ctrl2) then Continue;
      if Ctrl2.IsSystemSoundsSession = S_OK then Continue;  // ignora sons do sistema
      Pid := 0;
      if Failed(Ctrl2.GetProcessId(Pid)) or (Pid = 0) or (Pid = OwnPid) then Continue;
      PName := ProcessNameFromPid(Pid);
      if PName = '' then Continue;
      if PName = OwnName then Continue;  // outra instancia NoOBS (full saindo p/ hibernar)
      if Length(AWanted) = 0 then
      begin
        // Sem lista de monitorados: qualquer app conta, EXCETO as excecoes.
        if AppInList(PName, AExcept) then Continue;  // excecao: ignora esse app
        Exit(True);
      end
      // Com lista de monitorados: a propria lista e o filtro (as excecoes nao
      // se aplicam — e por isso a UI esconde o campo de excecoes nesse caso).
      else if AppInList(PName, AWanted) then Exit(True);
    end;
  end;
end;

constructor TMicWatchThread.Create(const AApps, AExcept: TArray<string>; ACallback: TMicUseProc);
begin
  // Cria NAO-suspensa e sem chamar Start (pegadinha #45).
  inherited Create(False);
  FreeOnTerminate := False;
  FApps := AApps;
  FExcept := AExcept;
  FCallback := ACallback;
  FLastInUse := False;
end;

procedure TMicWatchThread.Execute;
var
  ComOk: Boolean;
  InUse: Boolean;
  Slept: Integer;
begin
  ComOk := Succeeded(CoInitializeEx(nil, COINIT_MULTITHREADED));
  try
    while not Terminated do
    begin
      try
        InUse := PollMicInUse(FApps, FExcept);
      except
        InUse := FLastInUse;  // erro transitorio: preserva o estado
      end;
      if InUse <> FLastInUse then
      begin
        FLastInUse := InUse;
        if Assigned(FCallback) then
          try FCallback(InUse); except end;
      end;
      Slept := 0;
      while (Slept < POLL_MS) and (not Terminated) do
      begin
        Sleep(SLICE_MS);
        Inc(Slept, SLICE_MS);
      end;
    end;
  finally
    if ComOk then CoUninitialize;
  end;
end;

procedure Start(const AAppsFilter, AExceptFilter: string; ACallback: TMicUseProc);
begin
  if WatchThread <> nil then Exit;
  WatchThread := TMicWatchThread.Create(
    ParseApps(AAppsFilter), ParseApps(AExceptFilter), ACallback);
  Log('WinMicWatch: iniciado (monitorar="%s" excecoes="%s").',
    [AAppsFilter, AExceptFilter]);
end;

procedure Stop;
begin
  if WatchThread = nil then Exit;
  WatchThread.Terminate;
  try WatchThread.WaitFor; except end;
  FreeAndNil(WatchThread);
  Log('WinMicWatch: parado.');
end;

function IsRunning: Boolean;
begin
  Result := WatchThread <> nil;
end;

end.

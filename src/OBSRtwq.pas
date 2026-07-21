(*
  OBSRtwq - inicializa a plataforma RTWQ (Real-Time Work Queue) do
  Windows no arranque do processo, e a finaliza na saida.

  POR QUE ISTO EXISTE (pegadinha #47).

  O plugin win-wasapi do OBS usa a RTWQ (Media Foundation) pra agendar a
  entrega dos buffers de audio de captura. Mas ele NAO inicializa a
  plataforma — assume que o HOST ja fez isso. Ver win-wasapi.cpp:347:

      // OBS will already load DLL on startup if it exists
      const HMODULE rtwq_module = GetModuleHandle(L"RTWorkQ.dll");
      ...
      rtwq_supported = rtwq_module != NULL;   // usa RTWQ se a DLL existe

  Repare: `GetModuleHandle`, nao `LoadLibrary`. E o comentario diz que o
  OBS "ja carregou no startup". Quem carrega E chama RtwqStartup() e o
  FRONTEND do OBS, no topo do main (obs-main.cpp:921):

      const HMODULE hRtwq = LoadLibrary(L"RTWorkQ.dll");
      if (hRtwq) {
          PFN_RtwqStartup func = GetProcAddress(hRtwq, "RtwqStartup");
          func();                              // <-- inicializa a plataforma
      }

  O NoOBS SUBSTITUI o frontend do OBS — entao essa responsabilidade e
  nossa, e estava faltando. Consequencia observada:

    - A RTWorkQ.dll acaba carregada no processo por OUTRO caminho (stack
      de audio do Windows, WASAPI, Media Foundation, D3D). Entao o
      `GetModuleHandle` do plugin acha a DLL e ele ESCOLHE o caminho RTWQ
      (`rtwq_supported = true`)...
    - ...mas a plataforma nunca foi startada. Os work items de recepcao
      nao sao servidos de forma confiavel. O `RtwqPutWaitingWorkItem`
      falha ("Could not requeue sample receive work",
      win-wasapi.cpp:1266) e a fonte de audio entrega SILENCIO.
    - Classico "1a gravacao sai muda, 2a em diante grava": a 1a chamada
      RTWQ da fonte aquece a plataforma como efeito colateral, entao as
      seguintes ja pegam ela viva. Com RtwqStartup() no arranque, TODAS
      as gravacoes — inclusive a 1a — pegam a plataforma pronta.
    - A MESMA falha e o que trava o obs_shutdown: o requeue falho poe a
      fonte em modo reconnect, e o WASAPISource::Stop() passa a esperar
      um `idleSignal` que nao vem mais, num WaitForSingleObject(INFINITE)
      (win-wasapi.cpp:440). Ou seja: um so defeito, dois sintomas.

  RtwqStartup e refcontado (par com RtwqShutdown), igual ao CoInitialize.
  Carregamos com LoadLibrary em runtime (sem import estatico) pra nao
  criar dependencia de link — a DLL e de sistema, presente desde o
  Windows 8.1 (a RTWQ so e usada a partir do Win 10 1703 pelo plugin).
*)
unit OBSRtwq;

interface

// Inicializa a plataforma RTWQ. Idempotente — chamar 2x e no-op. Deve
// rodar UMA vez por processo, ANTES de qualquer fonte WASAPI ser criada
// (ou seja, antes do 1o BuildAndStartRecording). Falha nao e fatal: se a
// RTWorkQ.dll nao existe (Windows muito antigo), o plugin cai sozinho no
// caminho da thread de captura.
procedure StartRtwq;

// Finaliza a plataforma. Idempotente. Deve rodar DEPOIS do obs_shutdown
// (as fontes WASAPI usam a fila ate serem destruidas la), na saida do
// processo.
procedure StopRtwq;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  OBSLog;

type
  PFN_RtwqStartup  = function: HRESULT; stdcall;
  PFN_RtwqShutdown = function: HRESULT; stdcall;

var
  // Handle da RTWorkQ.dll enquanto a plataforma esta ativa. <> 0 funciona
  // como guarda de idempotencia: se ja carregamos, StartRtwq nao repete e
  // StopRtwq sabe que ha o que finalizar.
  GRtwqModule: HMODULE = 0;

procedure StartRtwq;
var
  Startup: PFN_RtwqStartup;
  Hr: HRESULT;
begin
  if GRtwqModule <> 0 then Exit;   // ja iniciado

  // LoadLibrary (nao GetModuleHandle): garante a DLL carregada mesmo se
  // nada mais no processo a tiver puxado ainda. Isso tambem faz o
  // GetModuleHandle do plugin enxerga-la e escolher o caminho RTWQ — que
  // agora estara corretamente inicializado.
  GRtwqModule := LoadLibrary('RTWorkQ.dll');
  if GRtwqModule = 0 then
  begin
    // Sem RTWQ o plugin usa a thread de captura classica — funciona,
    // so nao e o caminho preferido. Nao e erro fatal.
    Log('RTWQ: RTWorkQ.dll indisponivel (%d) — win-wasapi usara a thread '
      + 'de captura.', [GetLastError]);
    Exit;
  end;

  Startup := GetProcAddress(GRtwqModule, 'RtwqStartup');
  if not Assigned(Startup) then
  begin
    Log('RTWQ: RtwqStartup ausente na DLL — abortando init da plataforma.');
    FreeLibrary(GRtwqModule);
    GRtwqModule := 0;
    Exit;
  end;

  Hr := Startup();
  if Failed(Hr) then
  begin
    Log('RTWQ: RtwqStartup falhou (0x%.8x).', [Hr]);
    FreeLibrary(GRtwqModule);
    GRtwqModule := 0;
    Exit;
  end;

  Log('RTWQ: plataforma inicializada (RtwqStartup ok).');
end;

procedure StopRtwq;
var
  ShutdownFn: PFN_RtwqShutdown;
begin
  if GRtwqModule = 0 then Exit;

  ShutdownFn := GetProcAddress(GRtwqModule, 'RtwqShutdown');
  if Assigned(ShutdownFn) then
    try ShutdownFn(); except end;

  FreeLibrary(GRtwqModule);
  GRtwqModule := 0;
  Log('RTWQ: plataforma finalizada (RtwqShutdown).');
end;

end.

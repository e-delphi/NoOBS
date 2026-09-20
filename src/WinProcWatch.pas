{
  WinProcWatch — detecta se algum programa de uma lista esta RODANDO, pra
  ligar e desligar o buffer em memoria sozinho ("ligue o buffer quando eu
  abrir o jogo").

  Varre a lista de processos (ToolHelp32) a cada poll e compara o nome do
  executavel com o filtro do usuario. Match por "contem", case-insensitive:
  quem digita "valorant" casa com "VALORANT-Win64-Shipping.exe".

  Roda numa thread propria. O callback dispara SO na mudanca de estado, e
  roda NA THREAD do watcher — o consumidor marshala pra main (TThread.Queue,
  modo full) ou posta mensagem pra janela (modo hibernate).

  DIFERENCA DELIBERADA pro WinMicWatch: aqui a PRIMEIRA leitura TAMBEM
  dispara. La a primeira leitura e so linha de base, porque um microfone ja
  em uso quando o watcher sobe e uma chamada EM CURSO, e re-gravar o que o
  usuario ja tinha parado seria errado (pegadinha #47). Aqui o sentido e
  outro: "o jogo esta aberto" e um ESTADO, nao um evento. Abrir o NoOBS com
  o jogo ja rodando tem que ligar o buffer — e nada se perde se ele ligar,
  porque buffer nao vira arquivo sozinho.

  Exclui o proprio executavel da varredura: o NoOBS nao pode se monitorar.
}
unit WinProcWatch;

interface

type
  // Disparado na MUDANCA de estado (e na primeira leitura). Roda na thread
  // do watcher.
  TProcRunProc = procedure(ARunning: Boolean);

// Inicia o monitor (no-op se ja rodando). Nomes de processo separados por
// virgula (ou ; ou quebra de linha). Filtro VAZIO = nao inicia: aqui "vazio"
// nao pode significar "qualquer app" como no mic, senao o buffer ligaria
// sozinho com qualquer programa aberto — ou seja, sempre.
procedure Start(const AAppsFilter: string; ACallback: TProcRunProc);
procedure Stop;
function IsRunning: Boolean;

// Troca o filtro em tempo real (usuario editou nas Configuracoes). Para e
// sobe de novo com o callback atual. Filtro vazio so para.
procedure UpdateFilter(const AAppsFilter: string);

implementation

uses
  Winapi.Windows, System.Classes, System.SysUtils, OBSLog;

const
  // 2s: um jogo abrindo leva bem mais que isso, e a varredura de processos
  // custa alguns ms. Poll mais rapido nao traria nada e gastaria CPU na
  // maquina que o buffer existe pra nao atrapalhar.
  POLL_MS  = 2000;
  // Fatia do sleep pra o Terminate responder rapido no shutdown.
  SLICE_MS = 100;

type
  TProcWatchThread = class(TThread)
  private
    FApps: TArray<string>;
    FCallback: TProcRunProc;
    FLast: Boolean;
    FFirst: Boolean;
  protected
    procedure Execute; override;
  public
    constructor Create(const AApps: TArray<string>; ACallback: TProcRunProc);
  end;

  TProcessEntry32W = record
    dwSize: DWORD;
    cntUsage: DWORD;
    th32ProcessID: DWORD;
    th32DefaultHeapID: ULONG_PTR;
    th32ModuleID: DWORD;
    cntThreads: DWORD;
    th32ParentProcessID: DWORD;
    pcPriClassBase: Longint;
    dwFlags: DWORD;
    szExeFile: array[0..MAX_PATH - 1] of WideChar;
  end;

const
  TH32CS_SNAPPROCESS = $00000002;

function CreateToolhelp32Snapshot(dwFlags, th32ProcessID: DWORD): THandle;
  stdcall; external kernel32 name 'CreateToolhelp32Snapshot';
function Process32FirstW(hSnapshot: THandle; var lppe: TProcessEntry32W): BOOL;
  stdcall; external kernel32 name 'Process32FirstW';
function Process32NextW(hSnapshot: THandle; var lppe: TProcessEntry32W): BOOL;
  stdcall; external kernel32 name 'Process32NextW';

var
  WatchThread: TProcWatchThread = nil;
  GCallback: TProcRunProc = nil;

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

// AName (ja minusculo) casa (substring) com algum item da lista?
function AppInList(const AName: string; const AList: TArray<string>): Boolean;
var
  i: Integer;
begin
  for i := 0 to High(AList) do
    if (AList[i] <> '') and (Pos(AList[i], AName) > 0) then Exit(True);
  Result := False;
end;

// Alguem da lista esta rodando agora?
function PollRunning(const AWanted: TArray<string>): Boolean;
var
  Snap: THandle;
  PE: TProcessEntry32W;
  Own, Name: string;
begin
  Result := False;
  if Length(AWanted) = 0 then Exit;
  Own := LowerCase(ExtractFileName(ParamStr(0)));
  Snap := CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if Snap = INVALID_HANDLE_VALUE then Exit;
  try
    FillChar(PE, SizeOf(PE), 0);
    PE.dwSize := SizeOf(PE);
    if not Process32FirstW(Snap, PE) then Exit;
    repeat
      Name := LowerCase(string(PE.szExeFile));
      // O proprio NoOBS (full ou hibernate) nunca conta.
      if Name = Own then Continue;
      if AppInList(Name, AWanted) then Exit(True);
    until not Process32NextW(Snap, PE);
  finally
    CloseHandle(Snap);
  end;
end;

constructor TProcWatchThread.Create(const AApps: TArray<string>;
  ACallback: TProcRunProc);
begin
  // Cria NAO-suspensa e sem chamar Start (pegadinha #45).
  inherited Create(False);
  FreeOnTerminate := False;
  FApps := AApps;
  FCallback := ACallback;
  FLast := False;
  FFirst := True;
end;

procedure TProcWatchThread.Execute;
var
  Running: Boolean;
  Slept: Integer;
begin
  while not Terminated do
  begin
    try
      Running := PollRunning(FApps);
    except
      Running := FLast;   // erro transitorio: preserva o estado
    end;
    // Primeira leitura TAMBEM dispara (ver cabecalho da unit): o jogo ja
    // aberto quando o NoOBS sobe precisa ligar o buffer.
    if FFirst or (Running <> FLast) then
    begin
      FFirst := False;
      FLast := Running;
      if Assigned(FCallback) then
        try FCallback(Running); except end;
    end;
    Slept := 0;
    while (Slept < POLL_MS) and (not Terminated) do
    begin
      Sleep(SLICE_MS);
      Inc(Slept, SLICE_MS);
    end;
  end;
end;

procedure Start(const AAppsFilter: string; ACallback: TProcRunProc);
var
  Apps: TArray<string>;
begin
  // Guarda o callback ANTES das saidas antecipadas. Com a lista VAZIA nao ha
  // thread pra subir, mas o UpdateFilter (usuario digitando a lista depois,
  // com o app ja aberto) precisa dele pra conseguir subir a thread. Guardando
  // so no caminho de sucesso, quem comecou a sessao sem lista ficava com
  // GCallback=nil e o watcher NUNCA subia ate reabrir o app — a pegadinha
  // #54 de novo, noutra forma.
  if Assigned(ACallback) then GCallback := ACallback;
  if WatchThread <> nil then Exit;
  Apps := ParseApps(AAppsFilter);
  if Length(Apps) = 0 then Exit;   // sem lista nao ha o que monitorar
  WatchThread := TProcWatchThread.Create(Apps, ACallback);
  Log('WinProcWatch: iniciado (monitorar="%s").', [AAppsFilter]);
end;

procedure Stop;
begin
  if WatchThread = nil then Exit;
  WatchThread.Terminate;
  try WatchThread.WaitFor; except end;
  FreeAndNil(WatchThread);
  // NAO zera GCallback: o UpdateFilter reinicia com ele. Quem desliga de
  // verdade (teardown) nao volta a chamar Start.
  Log('WinProcWatch: parado.');
end;

procedure UpdateFilter(const AAppsFilter: string);
var
  Cb: TProcRunProc;
begin
  // Snapshot do callback ANTES do Stop (o Stop preserva, mas depender disso
  // e frágil — pegadinha #54). O callback vem do ultimo Start, que o Bridge
  // faz no DoInit mesmo com a lista vazia justamente pra este caminho.
  Cb := GCallback;
  Stop;
  if Assigned(Cb) then Start(AAppsFilter, Cb);
end;

function IsRunning: Boolean;
begin
  Result := WatchThread <> nil;
end;

end.

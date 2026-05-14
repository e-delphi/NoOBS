(*
  OBSRecordWatch - file watcher pra pasta de gravacoes via API nativa
  do Windows (FindFirstChangeNotification). Dispara callback quando o
  usuario adiciona/exclui/renomeia arquivo via Explorer (ou qualquer
  outro app), e o NoOBS atualiza a lista automaticamente.

  Filtra so FILE_NAME — nao reage a writes durante gravacao (que seriam
  FILE_NOTIFY_CHANGE_SIZE).

  Thread propria com WaitForMultipleObjects(stopEvent, changeHandle).
  Stop e responsivo (~ms) porque sinaliza o event.
*)
unit OBSRecordWatch;

interface

type
  TRecordChangeCallback = procedure;

procedure Start(const ADir: string; ACallback: TRecordChangeCallback);
procedure Stop;
procedure UpdateDir(const ANewDir: string);

implementation

uses
  Winapi.Windows, System.Classes, System.SysUtils, OBSLog;

type
  TRecordWatchThread = class(TThread)
  private
    FDir: string;
    FStopEvent: THandle;
    FCallback: TRecordChangeCallback;
  protected
    procedure Execute; override;
  public
    constructor Create(const ADir: string; ACallback: TRecordChangeCallback);
    destructor Destroy; override;
    procedure SignalStop;
  end;

var
  GThread: TRecordWatchThread = nil;
  GCallback: TRecordChangeCallback = nil;

constructor TRecordWatchThread.Create(const ADir: string;
  ACallback: TRecordChangeCallback);
begin
  FDir := ADir;
  FCallback := ACallback;
  FStopEvent := CreateEvent(nil, True, False, nil);
  FreeOnTerminate := False;
  inherited Create(False);
end;

destructor TRecordWatchThread.Destroy;
begin
  if FStopEvent <> 0 then CloseHandle(FStopEvent);
  inherited;
end;

procedure TRecordWatchThread.SignalStop;
begin
  Terminate;
  if FStopEvent <> 0 then SetEvent(FStopEvent);
end;

procedure TRecordWatchThread.Execute;
var
  ChangeHandle: THandle;
  Handles: array[0..1] of THandle;
  Wait: DWORD;
begin
  if (FDir = '') or (not DirectoryExists(FDir)) then
  begin
    Log('OBSRecordWatch: pasta invalida: %s', [FDir]);
    Exit;
  end;

  ChangeHandle := FindFirstChangeNotification(
    PWideChar(FDir),
    False, // bWatchSubtree = nao recursivo
    FILE_NOTIFY_CHANGE_FILE_NAME or FILE_NOTIFY_CHANGE_DIR_NAME);

  if ChangeHandle = INVALID_HANDLE_VALUE then
  begin
    Log('OBSRecordWatch: FindFirstChangeNotification falhou (err=%d) em %s',
      [GetLastError, FDir]);
    Exit;
  end;

  Log('OBSRecordWatch: monitorando %s', [FDir]);
  Handles[0] := FStopEvent;
  Handles[1] := ChangeHandle;
  try
    while not Terminated do
    begin
      Wait := WaitForMultipleObjects(2, @Handles[0], False, INFINITE);
      if Terminated or (Wait = WAIT_OBJECT_0) then Break;

      if Wait = WAIT_OBJECT_0 + 1 then
      begin
        // Debounce — coalesce rajadas de eventos (ex: copiar varios
        // arquivos de uma vez). Espera 400ms quieto antes de disparar.
        repeat
          Wait := WaitForMultipleObjects(2, @Handles[0], False, 400);
          if Terminated or (Wait = WAIT_OBJECT_0) then Break;
          if Wait = WAIT_OBJECT_0 + 1 then
            FindNextChangeNotification(ChangeHandle);
        until Wait = WAIT_TIMEOUT;
        if Terminated then Break;

        if Assigned(FCallback) then
          TThread.Queue(nil, procedure begin FCallback(); end);

        FindNextChangeNotification(ChangeHandle);
      end;
    end;
  finally
    FindCloseChangeNotification(ChangeHandle);
    Log('OBSRecordWatch: parou de monitorar %s', [FDir]);
  end;
end;

procedure Start(const ADir: string; ACallback: TRecordChangeCallback);
begin
  if GThread <> nil then Stop;
  GCallback := ACallback;
  if (ADir = '') or (not DirectoryExists(ADir)) then Exit;
  GThread := TRecordWatchThread.Create(ADir, ACallback);
end;

procedure Stop;
var
  Wait: DWORD;
begin
  if GThread = nil then Exit;
  // Limpa callback ANTES de sinalizar — evita executar callback em
  // estado parcialmente desmontado (Bridge ja pode estar tearing down).
  GCallback := nil;
  GThread.SignalStop;
  // Timeout defensivo: se a thread nao terminar em 2s, deixa pra
  // limpeza implicita do OS no exit do processo (nao bloqueia o app).
  Wait := WaitForSingleObject(GThread.Handle, 2000);
  if Wait = WAIT_TIMEOUT then
  begin
    Log('OBSRecordWatch: thread nao parou em 2s — abandonando (cleanup pelo OS).');
    GThread := nil; // intencional: vaza pra evitar travar o process exit
    Exit;
  end;
  FreeAndNil(GThread);
end;

procedure UpdateDir(const ANewDir: string);
begin
  if GThread <> nil then Stop;
  if not Assigned(GCallback) then Exit;
  if (ANewDir = '') or (not DirectoryExists(ANewDir)) then Exit;
  GThread := TRecordWatchThread.Create(ANewDir, GCallback);
end;

end.

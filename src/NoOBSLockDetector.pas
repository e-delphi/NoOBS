// Eduardo/Claude - 30/06/2025
unit NoOBSLockDetector;

interface

uses
  Winapi.Windows,
  Winapi.Messages,
  System.SysUtils,
  System.Classes;

type
  TMachineLockEvent = procedure(Sender: TObject; ALocked: Boolean) of object;

  TMachineLockDetector = class(TThread)
  private
    FWindowHandle: HWND;
    FOnLockStateChanged: TMachineLockEvent;
    FLocked: Boolean;
    FIsInitialized: Boolean;
    procedure CreateMessageWindow;
    procedure DestroyMessageWindow;
    procedure ProcessMessages;
    procedure DoLockStateChanged(ALocked: Boolean);
  protected
    procedure Execute; override;
  public
    constructor Create;
    destructor Destroy; override;
    property OnLockStateChanged: TMachineLockEvent read FOnLockStateChanged write FOnLockStateChanged;
    property Locked: Boolean read FLocked;
    class function Instance: TMachineLockDetector;
  end;

implementation

const
  WM_WTSSESSION_CHANGE = $02B1;
  WTS_SESSION_LOCK = 7;
  WTS_SESSION_UNLOCK = 8;
  NOTIFY_FOR_THIS_SESSION = 0;

function WTSRegisterSessionNotification(hWnd: HWND; dwFlags: DWORD): BOOL; stdcall; external 'wtsapi32.dll';
function WTSUnRegisterSessionNotification(hWnd: HWND): BOOL; stdcall; external 'wtsapi32.dll';

var
  CurrentDetectorInstance: TMachineLockDetector = nil;

function WindowProc(hWnd: HWND; uMsg: UINT; wParam: WPARAM; lParam: LPARAM): LRESULT; stdcall;
begin
  case uMsg of
    WM_WTSSESSION_CHANGE:
    begin
      if Assigned(CurrentDetectorInstance) then
      begin
        case wParam of
          WTS_SESSION_LOCK:   CurrentDetectorInstance.DoLockStateChanged(True);
          WTS_SESSION_UNLOCK: CurrentDetectorInstance.DoLockStateChanged(False);
        end;
      end;
      Result := 0;
    end;
  else
    Result := DefWindowProc(hWnd, uMsg, wParam, lParam);
  end;
end;

{ TMachineLockDetector }

constructor TMachineLockDetector.Create;
begin
  // Cria SUSPENSA: precisamos setar CurrentDetectorInstance antes de a
  // thread comecar — senao Execute -> CreateMessageWindow -> WindowProc
  // poderia rodar com CurrentDetectorInstance ainda nil (evento perdido).
  inherited Create(True);
  FreeOnTerminate := False;
  FWindowHandle := 0;
  FIsInitialized := False;
  FLocked := False;
  CurrentDetectorInstance := Self;
  Start;
end;

destructor TMachineLockDetector.Destroy;
begin
  if not Finished then
  begin
    Terminate;
    WaitFor;
  end;
  DestroyMessageWindow;
  CurrentDetectorInstance := nil;
  inherited Destroy;
end;

procedure TMachineLockDetector.CreateMessageWindow;
const
  ClassName: string = 'MachineLockDetectorWindow';
var
  WindowClass: TWndClass;
begin
  ZeroMemory(@WindowClass, SizeOf(WindowClass));
  WindowClass.lpfnWndProc := @WindowProc;
  WindowClass.hInstance := HInstance;
  WindowClass.lpszClassName := PChar(ClassName);
  Winapi.Windows.RegisterClass(WindowClass);
  FWindowHandle := CreateWindow(PChar(ClassName), 'MachineLockDetector', 0, 0, 0, 0, 0, HWND_MESSAGE, 0, HInstance, nil);
  FIsInitialized := (FWindowHandle <> 0) and WTSRegisterSessionNotification(FWindowHandle, NOTIFY_FOR_THIS_SESSION);
end;

procedure TMachineLockDetector.DestroyMessageWindow;
begin
  if FWindowHandle <> 0 then
  begin
    WTSUnRegisterSessionNotification(FWindowHandle);
    DestroyWindow(FWindowHandle);
    FWindowHandle := 0;
  end;
  FIsInitialized := False;
end;

procedure TMachineLockDetector.ProcessMessages;
var
  Msg: TMsg;
begin
  while PeekMessage(Msg, FWindowHandle, 0, 0, PM_REMOVE) do
  begin
    TranslateMessage(Msg);
    DispatchMessage(Msg);
  end;
end;

procedure TMachineLockDetector.DoLockStateChanged(ALocked: Boolean);
begin
  if aLocked = FLocked then
    Exit;
  FLocked := aLocked;
  if Assigned(FOnLockStateChanged) then
    FOnLockStateChanged(Self, aLocked);
end;

procedure TMachineLockDetector.Execute;
begin
  CreateMessageWindow;
  if not FIsInitialized then
    Exit;
  while not Terminated do
  begin
    ProcessMessages;
    Sleep(10);
  end;
end;

class function TMachineLockDetector.Instance: TMachineLockDetector;
begin
  if not Assigned(CurrentDetectorInstance) then
    TMachineLockDetector.Create;

  Result := CurrentDetectorInstance;
end;

end.

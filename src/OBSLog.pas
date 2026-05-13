{
  OBSLog — log centralizado em arquivo (UTF-8) ao inves de stdout.

  Caminho: %LOCALAPPDATA%\NoOBS\NoOBS.log (append entre sessoes).

  Cada linha: HH:MM:SS.zzz<2 espacos>texto.
  Cabecalho de sessao no startup, footer no finalization.

  Uso:
    Log('mensagem simples');
    Log('formatada %d %s', [N, Texto]);
    Log;                    // linha em branco

  Thread-safe via TCriticalSection — embora hoje todas as chamadas
  ocorram na main thread (WebView2 + WindowProc), defensivo pra futuro.
}
unit OBSLog;

interface

procedure Log; overload;
procedure Log(const AMsg: string); overload;
procedure Log(const AFmt: string; const AArgs: array of const); overload;

function LogFilePath: string;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.IOUtils,
  System.Classes,
  System.SyncObjs;

var
  LogStream: TFileStream = nil;
  LogLock: TCriticalSection = nil;
  LogPath: string = '';

procedure WriteLineRaw(const ALine: string);
const
  CRLF: array[0..1] of Byte = (13, 10);
var
  Buf: TBytes;
begin
  if LogStream = nil then Exit;
  Buf := TEncoding.UTF8.GetBytes(ALine);
  if Length(Buf) > 0 then
    LogStream.WriteBuffer(Buf[0], Length(Buf));
  LogStream.WriteBuffer(CRLF[0], 2);
end;

procedure DoLog(const AMsg: string);
begin
  if (LogLock = nil) or (LogStream = nil) then Exit;
  LogLock.Enter;
  try
    if AMsg = '' then
      WriteLineRaw('')
    else
      WriteLineRaw(FormatDateTime('hh:nn:ss.zzz', Now) + '  ' + AMsg);
  finally
    LogLock.Leave;
  end;
end;

procedure Log;
begin
  DoLog('');
end;

procedure Log(const AMsg: string);
begin
  DoLog(AMsg);
end;

procedure Log(const AFmt: string; const AArgs: array of const);
begin
  try
    DoLog(Format(AFmt, AArgs));
  except
    DoLog('[falha ao formatar log: ' + AFmt + ']');
  end;
end;

function LogFilePath: string;
begin
  Result := LogPath;
end;

procedure InitLog;
var
  AppData, Dir: string;
begin
  AppData := GetEnvironmentVariable('LOCALAPPDATA');
  if AppData = '' then AppData := GetEnvironmentVariable('APPDATA');
  if AppData = '' then Exit;

  Dir := IncludeTrailingPathDelimiter(AppData) + 'NoOBS';
  try
    ForceDirectories(Dir);
  except
    Exit;
  end;
  LogPath := IncludeTrailingPathDelimiter(Dir) + 'NoOBS.log';

  LogLock := TCriticalSection.Create;

  // Apaga log da sessao anterior — comeca limpo em todo startup.
  // Caso contrario o arquivo cresce indefinidamente entre execucoes.
  if FileExists(LogPath) then
    try DeleteFile(LogPath); except end;

  try
    LogStream := TFileStream.Create(LogPath,
      fmCreate or fmShareDenyWrite);
  except
    FreeAndNil(LogStream);
  end;

  if LogStream <> nil then
  begin
    WriteLineRaw('');
    WriteLineRaw('=========================================================');
    WriteLineRaw('=== ' + FormatDateTime('yyyy-mm-dd hh:nn:ss', Now) +
      '  SESSION START ===');
    WriteLineRaw('=========================================================');
  end;
end;

procedure DoneLog;
begin
  if LogStream <> nil then
  begin
    try
      WriteLineRaw('=== ' + FormatDateTime('yyyy-mm-dd hh:nn:ss', Now) +
        '  SESSION END ===');
    except end;
    try LogStream.Free except end;
    LogStream := nil;
  end;
  if LogLock <> nil then
  begin
    LogLock.Free;
    LogLock := nil;
  end;
end;

initialization
  InitLog;

finalization
  DoneLog;

end.

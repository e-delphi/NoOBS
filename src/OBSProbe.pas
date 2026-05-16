(*
  OBSProbe - wrapper sobre ffprobe.exe pra inspecionar arquivos de
  midia. Usado em testes pra validar que a gravacao saiu correta
  (resolucao, codec, faixas de audio, duracao).

  ffprobe roda com -of json e a gente parseia a resposta. Caminho do
  binario: ffprobe.exe (ao lado do NoOBS.exe, junto das DLLs avcodec/
  avformat que o OBS ja traz, sem pasta dedicada).
*)
unit OBSProbe;

interface

uses
  System.JSON;

type
  TStreamInfo = record
    Index: Integer;
    Kind: string;          // 'video' | 'audio' | 'subtitle' | etc
    Codec: string;         // 'hevc', 'h264', 'aac', 'opus'
    Title: string;         // tags.title se presente (nome da faixa)
    Width, Height: Integer; // video apenas
    Channels: Integer;      // audio apenas
    SampleRate: Integer;    // audio apenas
    BitRate: Int64;
    Duration: Double;       // segundos
  end;
  TStreamArray = TArray<TStreamInfo>;

  TProbeReport = record
    FilePath: string;
    Format: string;         // 'matroska,webm', 'mov,mp4...'
    Duration: Double;       // segundos (do format, mais confiavel)
    BitRate: Int64;
    Size: Int64;
    Streams: TStreamArray;
    function VideoStream: TStreamInfo;        // primeiro stream de video
    function AudioStreams: TStreamArray;      // todos audio
    function HasVideo: Boolean;
    function AudioTrackCount: Integer;
  end;

// Verifica se ffprobe.exe esta disponivel.
function FFprobeAvailable: Boolean;

// Inspeciona um arquivo. Retorna True se conseguiu probar.
function Probe(const APath: string; out AReport: TProbeReport): Boolean;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.Classes,
  System.Generics.Collections;

function ExeDir: string;
begin
  Result := ExtractFilePath(ParamStr(0));
end;

function FFprobePath: string;
// ffprobe.exe mora ao lado do NoOBS.exe (mesma pasta bin\64bit do OBS).
const
  CANDIDATES: array[0..1] of string = (
    'ffprobe.exe',
    '..\..\exe\bin\64bit\ffprobe.exe' // pra test exe que roda fora de bin\64bit\
  );
var
  i: Integer;
  P: string;
begin
  for i := 0 to High(CANDIDATES) do
  begin
    P := ExeDir + CANDIDATES[i];
    if FileExists(P) then Exit(ExpandFileName(P));
  end;
  Result := '';
end;

function FFprobeAvailable: Boolean;
begin
  Result := FFprobePath <> '';
end;

function RunCapture(const ACmdLine: string; out AStdOut: string): Boolean;
const
  BUF_SIZE = 4096;
var
  Sec: TSecurityAttributes;
  ReadH, WriteH: THandle;
  StartInfo: TStartupInfo;
  ProcInfo: TProcessInformation;
  Buf: array[0..BUF_SIZE - 1] of AnsiChar;
  BytesRead: DWORD;
  SS: TStringStream;
  CmdBuf: array[0..2047] of Char;
  ExitCode: DWORD;
begin
  Result := False;
  AStdOut := '';
  Sec.nLength := SizeOf(Sec);
  Sec.bInheritHandle := True;
  Sec.lpSecurityDescriptor := nil;
  if not CreatePipe(ReadH, WriteH, @Sec, 0) then Exit;
  SetHandleInformation(ReadH, HANDLE_FLAG_INHERIT, 0);

  ZeroMemory(@StartInfo, SizeOf(StartInfo));
  StartInfo.cb := SizeOf(StartInfo);
  StartInfo.dwFlags := STARTF_USESTDHANDLES or STARTF_USESHOWWINDOW;
  StartInfo.wShowWindow := SW_HIDE;
  StartInfo.hStdOutput := WriteH;
  StartInfo.hStdError  := WriteH;
  StartInfo.hStdInput  := GetStdHandle(STD_INPUT_HANDLE);

  StrLCopy(CmdBuf, PChar(ACmdLine), High(CmdBuf));

  SS := TStringStream.Create('', TEncoding.UTF8);
  try
    if not CreateProcess(nil, CmdBuf, nil, nil, True,
      CREATE_NO_WINDOW, nil, nil, StartInfo, ProcInfo) then
    begin
      CloseHandle(ReadH);
      CloseHandle(WriteH);
      Exit;
    end;
    CloseHandle(WriteH);
    while ReadFile(ReadH, Buf, BUF_SIZE, BytesRead, nil) and (BytesRead > 0) do
      SS.WriteData(@Buf[0], BytesRead);
    WaitForSingleObject(ProcInfo.hProcess, INFINITE);
    GetExitCodeProcess(ProcInfo.hProcess, ExitCode);
    CloseHandle(ProcInfo.hProcess);
    CloseHandle(ProcInfo.hThread);
    CloseHandle(ReadH);
    AStdOut := SS.DataString;
    Result := ExitCode = 0;
  finally
    SS.Free;
  end;
end;

function ReadStr(O: TJSONObject; const K: string): string;
var V: TJSONValue;
begin
  Result := '';
  if O = nil then Exit;
  V := O.GetValue(K);
  if V <> nil then Result := V.Value;
end;

function ReadInt(O: TJSONObject; const K: string): Integer;
var V: TJSONValue;
begin
  Result := 0;
  if O = nil then Exit;
  V := O.GetValue(K);
  if V is TJSONNumber then Result := TJSONNumber(V).AsInt
  else if V <> nil then Result := StrToIntDef(V.Value, 0);
end;

function ReadInt64(O: TJSONObject; const K: string): Int64;
var V: TJSONValue;
begin
  Result := 0;
  if O = nil then Exit;
  V := O.GetValue(K);
  if V is TJSONNumber then Result := TJSONNumber(V).AsInt64
  else if V <> nil then Result := StrToInt64Def(V.Value, 0);
end;

function ReadFloat(O: TJSONObject; const K: string): Double;
var V: TJSONValue;
  FS: TFormatSettings;
begin
  Result := 0;
  if O = nil then Exit;
  V := O.GetValue(K);
  if V <> nil then
  begin
    FS := TFormatSettings.Create;
    FS.DecimalSeparator := '.';
    Result := StrToFloatDef(V.Value, 0, FS);
  end;
end;

{ TProbeReport }

function TProbeReport.HasVideo: Boolean;
var i: Integer;
begin
  for i := 0 to High(Streams) do
    if SameText(Streams[i].Kind, 'video') then Exit(True);
  Result := False;
end;

function TProbeReport.VideoStream: TStreamInfo;
var i: Integer;
begin
  for i := 0 to High(Streams) do
    if SameText(Streams[i].Kind, 'video') then Exit(Streams[i]);
  // Vazio.
  FillChar(Result, SizeOf(Result), 0);
end;

function TProbeReport.AudioStreams: TStreamArray;
var i: Integer;
begin
  SetLength(Result, 0);
  for i := 0 to High(Streams) do
    if SameText(Streams[i].Kind, 'audio') then
    begin
      SetLength(Result, Length(Result) + 1);
      Result[High(Result)] := Streams[i];
    end;
end;

function TProbeReport.AudioTrackCount: Integer;
begin
  Result := Length(AudioStreams);
end;

function Probe(const APath: string; out AReport: TProbeReport): Boolean;
var
  Cmd, Out: string;
  Root: TJSONValue;
  Obj, FmtObj, Item: TJSONObject;
  StreamsArr: TJSONArray;
  i: Integer;
  Info: TStreamInfo;
begin
  Result := False;
  FillChar(AReport, SizeOf(AReport), 0);
  AReport.FilePath := APath;
  if not FileExists(APath) then Exit;
  if not FFprobeAvailable then Exit;

  Cmd := '"' + FFprobePath +
    '" -v error -show_format -show_streams -of json "' + APath + '"';
  if not RunCapture(Cmd, Out) then Exit;

  Root := TJSONObject.ParseJSONValue(Out);
  if not (Root is TJSONObject) then
  begin
    if Root <> nil then Root.Free;
    Exit;
  end;
  Obj := TJSONObject(Root);
  try
    FmtObj := Obj.GetValue('format') as TJSONObject;
    AReport.Format   := ReadStr(FmtObj, 'format_name');
    AReport.Duration := ReadFloat(FmtObj, 'duration');
    AReport.BitRate  := ReadInt64(FmtObj, 'bit_rate');
    AReport.Size     := ReadInt64(FmtObj, 'size');

    StreamsArr := Obj.GetValue('streams') as TJSONArray;
    if StreamsArr <> nil then
    begin
      SetLength(AReport.Streams, StreamsArr.Count);
      for i := 0 to StreamsArr.Count - 1 do
      begin
        Item := StreamsArr.Items[i] as TJSONObject;
        Info.Index      := ReadInt(Item, 'index');
        Info.Kind       := ReadStr(Item, 'codec_type');
        Info.Codec      := ReadStr(Item, 'codec_name');
        Info.Width      := ReadInt(Item, 'width');
        Info.Height     := ReadInt(Item, 'height');
        Info.Channels   := ReadInt(Item, 'channels');
        Info.SampleRate := ReadInt(Item, 'sample_rate');
        Info.BitRate    := ReadInt64(Item, 'bit_rate');
        Info.Duration   := ReadFloat(Item, 'duration');
        // tags.title — metadata escrita pelo NoOBS na gravacao.
        Info.Title := '';
        if Item.GetValue('tags') is TJSONObject then
          Info.Title := ReadStr(TJSONObject(Item.GetValue('tags')), 'title');
        AReport.Streams[i] := Info;
      end;
    end;

    Result := True;
  finally
    Obj.Free;
  end;
end;

procedure AppendObsBinToPath;
// Adiciona a pasta do exe ao PATH (idempotente). NoOBS.exe roda da
// pasta bin\64bit\ do OBS — onde ffmpeg.exe, ffprobe.exe e DLLs
// avcodec* vivem. Sem isso, "ffprobe" sem caminho absoluto pode nao
// resolver dependendo do ambiente.
//
// Importante: as versoes dos binarios (ffmpeg.exe, ffprobe.exe) devem
// casar com as DLLs do OBS — FFmpeg 7.x = avcodec-61 etc.
var
  AppDir, CurPath, NewPath: string;
begin
  AppDir := ExcludeTrailingPathDelimiter(ExtractFilePath(ParamStr(0)));
  if AppDir = '' then Exit;
  CurPath := GetEnvironmentVariable('PATH');
  if Pos(AppDir, CurPath) > 0 then Exit;
  if CurPath <> '' then
    NewPath := AppDir + ';' + CurPath
  else
    NewPath := AppDir;
  SetEnvironmentVariable('PATH', PChar(NewPath));
end;

initialization
  AppendObsBinToPath;

end.

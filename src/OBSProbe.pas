(*
  OBSProbe - inspeciona arquivos de midia (codec, dimensoes, faixas,
  duracao, metadata). Usa libavformat (avformat-61.dll) diretamente
  via FFmpegLib. Sem dependencia de ffprobe.exe.

  Vantagens vs. fork de processo:
    - 100x mais rapido (sem startup do processo)
    - Sem janela de console piscando
    - Sem parser de JSON
    - Sem dependencia de exe (so a DLL)

  OBSStartupCheck garante avformat-61.dll presente — se faltar, o
  app nem chega a abrir.
*)
unit OBSProbe;

interface

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
    FrameRate: Double;      // video apenas: avg_frame_rate (0 se desconhecido)
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

// Inspeciona um arquivo. Retorna True se conseguiu probar.
function Probe(const APath: string; out AReport: TProbeReport): Boolean;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.Classes,
  FFmpegLib;

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

function MediaTypeToKind(AType: Integer): string;
begin
  case AType of
    AVMEDIA_TYPE_VIDEO:    Result := 'video';
    AVMEDIA_TYPE_AUDIO:    Result := 'audio';
    AVMEDIA_TYPE_SUBTITLE: Result := 'subtitle';
    AVMEDIA_TYPE_DATA:     Result := 'data';
  else
    Result := '';
  end;
end;

function Probe(const APath: string; out AReport: TProbeReport): Boolean;
var
  Fmt: AVFormatContext;
  PathA: UTF8String;
  Rc: Integer;
  i, N: Cardinal;
  S: PAVStream;
  Info: TStreamInfo;
  CodecName: PAnsiChar;
  FileStream: TFileStream;
begin
  Result := False;
  FillChar(AReport, SizeOf(AReport), 0);
  AReport.FilePath := APath;
  if not FileExists(APath) then Exit;
  if not FFmpegLibAvailable then Exit;

  // Tamanho do arquivo via filesystem (libavformat nao expoe direto).
  try
    FileStream := TFileStream.Create(APath, fmOpenRead or fmShareDenyNone);
    try
      AReport.Size := FileStream.Size;
    finally
      FileStream.Free;
    end;
  except
    AReport.Size := 0;
  end;

  PathA := ToUtf8(APath);
  Fmt := nil;
  Rc := avformat_open_input(@Fmt, PAnsiChar(PathA), nil, nil);
  if (Rc < 0) or (Fmt = nil) then Exit;

  try
    Rc := avformat_find_stream_info(Fmt, nil);
    if Rc < 0 then Exit;

    AReport.Format   := av_format_context_iformat_name(Fmt);
    AReport.Duration := av_format_context_duration(Fmt) / AV_TIME_BASE;
    AReport.BitRate  := av_format_context_bit_rate(Fmt);

    // Ultimo recurso de duration: varre pacotes pra achar max PTS.
    // Custa O(N) pacotes mas funciona pra MKV sem Duration EBML
    // (comum quando OBS muxa sem trailer limpo).
    if AReport.Duration = 0 then
    begin
      AReport.Duration := ScanDurationByPackets(Fmt) / AV_TIME_BASE;
      // ScanDurationByPackets consome o cursor do demuxer ate EOF. A
      // enumeracao de streams abaixo so le metadata (codecpar/time_base),
      // entao funcionaria mesmo sem isto — mas re-seek pro inicio deixa o
      // contexto limpo pra qualquer leitura de pacote futura (robustez).
      av_seek_frame(Fmt, -1, 0, AVSEEK_FLAG_BACKWARD);
    end;

    // Ultimo fallback de bit_rate: derivar do tamanho/duracao quando
    // container e streams nao expoem (comum em MKV gerado por OBS).
    if (AReport.BitRate = 0) and
       (AReport.Size > 0) and (AReport.Duration > 0) then
      AReport.BitRate := Round((AReport.Size * 8) / AReport.Duration);

    N := av_format_context_nb_streams(Fmt);
    SetLength(AReport.Streams, N);
    // Guard pegadinha #24: N e Cardinal — se 0 (arquivo corrompido sem
    // streams), `0 to N-1` faz underflow pra ~4 bilhoes e congela.
    if N > 0 then
    for i := 0 to N - 1 do
    begin
      FillChar(Info, SizeOf(Info), 0);
      S := GetStreamByIndex(Fmt, i);
      if S = nil then Continue;
      Info.Index := S.index;
      Info.Title := GetMetadataString(S.metadata, 'title');

      if S.codecpar <> nil then
      begin
        Info.Kind := MediaTypeToKind(S.codecpar.codec_type);
        CodecName := avcodec_get_name(S.codecpar.codec_id);
        if CodecName <> nil then
          Info.Codec := UTF8ToString(CodecName);
        Info.Width      := S.codecpar.width;
        Info.Height     := S.codecpar.height;
        Info.Channels   := S.codecpar.ch_layout.nb_channels;
        Info.SampleRate := S.codecpar.sample_rate;
        Info.BitRate    := S.codecpar.bit_rate;
      end;

      // FPS via avg_frame_rate (= num/den, expresso como fracao pra
      // suportar taxas NTSC como 30000/1001 = 29.97). Zero se desconhecido
      // — UI nao exibe a linha nesse caso.
      if (Info.Kind = 'video') and (S.avg_frame_rate.den > 0) then
        Info.FrameRate := S.avg_frame_rate.num / S.avg_frame_rate.den
      else
        Info.FrameRate := 0;

      if (S.duration > 0) and (S.time_base.den > 0) then
        Info.Duration := (S.duration * S.time_base.num) / S.time_base.den
      else
        Info.Duration := AReport.Duration;

      AReport.Streams[i] := Info;
    end;

    Result := True;
  finally
    avformat_close_input(@Fmt);
  end;
end;

end.

(*
  FFmpegOps - operacoes de alto nivel sobre arquivos de midia usando
  libavformat/libavcodec/libavutil/libswscale.

  Esta unit e a camada de wrapper limpa sobre FFmpegLib (que so tem
  as bindings raw das DLLs). Consumidores (OBSPlayer, OBSBridge)
  importam apenas FFmpegOps quando precisam dessas operacoes — nao
  veem structs C, ponteiros, ou detalhes de ABI.

  Operacoes:
    RemuxFile          — troca container sem reencodar (MKV->MP4 etc).
    ExtractAudioTracks — separa audio streams em arquivos M4A.
    ExtractFrameJpeg   — extrai 1 frame em timestamp e salva como JPEG.

  Todas rodam in-process — sem fork de ffmpeg.exe.
  Seguro chamar de worker thread (libav nao tem main-thread requirement).
*)
unit FFmpegOps;

interface

uses
  System.SysUtils;

// Remuxa um arquivo trocando o container. Copia streams sem reencodar
// (equivale a 'ffmpeg -i src -c copy dst'). Adiciona +faststart pra MP4.
// Retorna True em sucesso.
function RemuxFile(const ASrc, ADst: string): Boolean;

// Extrai cada faixa de audio do source pra um arquivo separado (M4A,
// AAC stream copy). AOutputs deve ter Length = numero de audio streams
// em ASrc (descobre via probe interno). Faz UMA passada de demux.
// Retorna True se todas faixas foram escritas.
function ExtractAudioTracks(const ASrc: string;
  const AOutputs: TArray<string>): Boolean;

// Extrai um frame em ATimestampSec e salva como JPEG. Faz seek pro
// keyframe anterior, decoda frames ate alcancar ATimestampSec, scala
// pra ATargetHeight preservando aspect, encoda como MJPEG.
function ExtractFrameJpeg(const ASrc, ADstJpeg: string;
  ATimestampSec, ATargetHeight: Integer): Boolean;

// Decoda a primeira faixa de audio do source e calcula peaks por
// bucket — usado pra renderizar a waveform abaixo do seek bar do
// player. Retorna array de Single (Length = ABuckets), cada valor em
// [0..1] representando o pico absoluto daquela secao do audio.
// Faz uma passada linear; ~500ms-2s pra gravacao de 10 min.
function ComputeAudioPeaks(const ASrc: string; ABuckets: Integer;
  out APeaks: TArray<Single>): Boolean;

implementation

uses
  Winapi.Windows,
  OBSLog,
  FFmpegLib;

// =====================================================================
// RemuxToContainer — base de RemuxFile e ExtractAudioTracks
// =====================================================================
//
// Faz demux do source, demuxa pacotes em loop, escreve em N outputs.
// Cada output e:
//   - filename: arquivo de saida
//   - keep_stream: bitmask de quais streams do source vao pra esse
//     output (ex.: [0]=manter index 0 do source, [1]=manter index 1...)
//   - stream_map: input_stream_idx -> output_stream_idx (-1 = skip)
// Esse design generaliza: 1 output com todos = remux MP4 inteiro;
// N outputs com 1 stream cada = audio track extraction.

type
  TOutputStream = record
    Filename: UTF8String;       // UTF-8 (passada direto pro FFmpeg).
    Ctx: AVFormatContext;
    Pb: Pointer;             // AVIOContext*
    HeaderWritten: Boolean;
    // mapeamento input stream idx -> output stream idx (-1 = skip)
    StreamMap: TArray<Integer>;
  end;
  PTOutputStream = ^TOutputStream;

function DetectContainerFromExt(const APath: string): AnsiString;
var
  Ext: string;
begin
  Ext := LowerCase(System.SysUtils.ExtractFileExt(APath));
  if (Ext = '.mp4') or (Ext = '.m4a') or (Ext = '.m4v') then Result := 'mp4'
  else if Ext = '.mkv' then Result := 'matroska'
  else if Ext = '.mov' then Result := 'mov'
  else if Ext = '.aac' then Result := 'adts'
  else Result := 'mp4'; // default
end;

function OpenOutputForStreams(const ASrcCtx: AVFormatContext;
  const ADstFilename: string;
  const AKeepStreamIdx: TArray<Cardinal>;
  out AOut: TOutputStream): Boolean;
// Aloca AVFormatContext de saida, cria streams espelhando os indices
// selecionados, abre IO, escreve header. Em sucesso AOut.StreamMap
// tem o mapeamento; em falha, libera tudo.
var
  Rc, i: Integer;
  SrcStream, DstStream: PAVStream;
  N: Cardinal;
  Pb: Pointer;
  ContainerFmt: AnsiString;
  MovOpts: AVDictionary;
begin
  Result := False;
  FillChar(AOut, SizeOf(AOut), 0);
  AOut.Filename := ToUtf8(ADstFilename);

  ContainerFmt := DetectContainerFromExt(ADstFilename);
  Rc := avformat_alloc_output_context2(@AOut.Ctx, nil,
    PAnsiChar(ContainerFmt), PAnsiChar(AOut.Filename));
  if (Rc < 0) or (AOut.Ctx = nil) then Exit;

  // Mapeia indices do source pra saida. Default = -1 (skip).
  N := av_format_context_nb_streams(ASrcCtx);
  if N = 0 then Exit;
  SetLength(AOut.StreamMap, N);
  for i := 0 to Integer(N) - 1 do AOut.StreamMap[i] := -1;

  for i := 0 to High(AKeepStreamIdx) do
  begin
    if AKeepStreamIdx[i] >= N then Continue;
    SrcStream := GetStreamByIndex(ASrcCtx, AKeepStreamIdx[i]);
    if SrcStream = nil then Continue;
    DstStream := avformat_new_stream(AOut.Ctx, nil);
    if DstStream = nil then Exit;
    if avcodec_parameters_copy(DstStream.codecpar, SrcStream.codecpar) < 0 then Exit;
    // codec_tag = 0 deixa o muxer escolher conforme container.
    DstStream.codecpar.codec_tag := 0;
    AOut.StreamMap[AKeepStreamIdx[i]] := DstStream.index;
  end;

  // Abre arquivo de saida.
  Pb := nil;
  Rc := avio_open2(@Pb, PAnsiChar(AOut.Filename), AVIO_FLAG_WRITE, nil, nil);
  if Rc < 0 then Exit;
  AOut.Pb := Pb;
  // Seta pb no AVFormatContext via helper que encapsula o offset ABI.
  av_format_context_set_pb(AOut.Ctx, Pb);

  // Header com +faststart pra MP4 (move moov pro inicio).
  MovOpts := nil;
  if (ContainerFmt = 'mp4') or (ContainerFmt = 'mov') then
    av_dict_set(@MovOpts, '+movflags', '+faststart', 0);

  Rc := avformat_write_header(AOut.Ctx, @MovOpts);
  if MovOpts <> nil then av_dict_free(@MovOpts);
  if Rc < 0 then Exit;
  AOut.HeaderWritten := True;

  Result := True;
end;

procedure CloseOutput(var AOut: TOutputStream);
var
  Pb: Pointer;
begin
  if AOut.Ctx <> nil then
  begin
    if AOut.HeaderWritten then
      try av_write_trailer(AOut.Ctx); except end;
    Pb := AOut.Pb;
    if Pb <> nil then
      try avio_closep(@Pb); except end;
    try avformat_free_context(AOut.Ctx); except end;
    AOut.Ctx := nil;
    AOut.Pb := nil;
  end;
end;

function RemuxDispatch(const ASrc: string;
  const ATargets: TArray<TArray<Cardinal>>;
  const AOutputPaths: TArray<string>): Boolean;
// Loop generico: abre source, abre N outputs, le pacotes do source e
// despacha pra cada output que mapeia o stream. Cada target[i] e a
// lista de stream indices que vao pro output[i].
var
  SrcCtx: AVFormatContext;
  SrcPath: UTF8String;
  Outs: array of TOutputStream;
  Pkt: PAVPacket;
  Rc, i: Integer;
  SrcStream, DstStream: PAVStream;
  DstStreamIdx: Integer;
  AnyHeader: Boolean;
begin
  Result := False;
  if not FFmpegLibAvailable then Exit;
  if Length(ATargets) <> Length(AOutputPaths) then Exit;
  if Length(ATargets) = 0 then Exit;

  SrcPath := ToUtf8(ASrc);
  SrcCtx := nil;
  Rc := avformat_open_input(@SrcCtx, PAnsiChar(SrcPath), nil, nil);
  if (Rc < 0) or (SrcCtx = nil) then Exit;

  Pkt := nil;
  SetLength(Outs, Length(ATargets));
  try
    if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;

    AnyHeader := False;
    for i := 0 to High(ATargets) do
      if OpenOutputForStreams(SrcCtx, AOutputPaths[i],
                              ATargets[i], Outs[i]) then
        AnyHeader := True;
    if not AnyHeader then Exit;

    Pkt := av_packet_alloc;
    if Pkt = nil then Exit;

    // Loop principal: le pacotes do source, despacha pra cada output
    // que mapeie esse stream. av_packet_rescale_ts converte timestamps
    // entre time_base do source e do output.
    while av_read_frame(SrcCtx, Pkt) = 0 do
    begin
      try
        for i := 0 to High(Outs) do
        begin
          if Outs[i].Ctx = nil then Continue;
          if Length(Outs[i].StreamMap) <= Pkt.stream_index then Continue;
          DstStreamIdx := Outs[i].StreamMap[Pkt.stream_index];
          if DstStreamIdx < 0 then Continue;

          SrcStream := GetStreamByIndex(SrcCtx, Pkt.stream_index);
          DstStream := GetStreamByIndex(Outs[i].Ctx, Cardinal(DstStreamIdx));
          if (SrcStream = nil) or (DstStream = nil) then Continue;

          // Rescale ts pro time_base do output.
          Pkt.stream_index := DstStreamIdx;
          av_packet_rescale_ts(Pkt, SrcStream.time_base, DstStream.time_base);
          Pkt.pos := -1;
          try av_interleaved_write_frame(Outs[i].Ctx, Pkt); except end;
          // Restaura stream_index pro proximo output que tambem
          // queira esse pacote — diferencas de time_base sao
          // recalculadas pelo rescale a cada output.
          Pkt.stream_index := SrcStream.index;
        end;
      finally
        av_packet_unref(Pkt);
      end;
    end;

    Result := True;
  finally
    if Pkt <> nil then av_packet_free(@Pkt);
    for i := 0 to High(Outs) do CloseOutput(Outs[i]);
    avformat_close_input(@SrcCtx);
  end;
end;

function RemuxFile(const ASrc, ADst: string): Boolean;
// Copia TODOS os streams do source pra um unico output. Equivale a
// `ffmpeg -i src -c copy dst` — sem reencode.
var
  SrcCtx: AVFormatContext;
  SrcPath: UTF8String;
  N, i: Cardinal;
  Keep: TArray<Cardinal>;
  Targets: TArray<TArray<Cardinal>>;
  Outputs: TArray<string>;
begin
  Result := False;
  if not FFmpegLibAvailable then Exit;

  // Mini-probe so pra contar streams.
  SrcPath := ToUtf8(ASrc);
  SrcCtx := nil;
  if avformat_open_input(@SrcCtx, PAnsiChar(SrcPath), nil, nil) < 0 then Exit;
  try
    if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;
    N := av_format_context_nb_streams(SrcCtx);
    if N = 0 then Exit;
    SetLength(Keep, N);
    for i := 0 to N - 1 do Keep[i] := i;
  finally
    avformat_close_input(@SrcCtx);
  end;

  SetLength(Targets, 1);
  Targets[0] := Keep;
  SetLength(Outputs, 1);
  Outputs[0] := ADst;
  Result := RemuxDispatch(ASrc, Targets, Outputs);
end;

function ExtractAudioTracks(const ASrc: string;
  const AOutputs: TArray<string>): Boolean;
// Cada audio stream do source vai pra um arquivo separado. Faz UMA
// passada de demux — performance equivalente a `ffmpeg -i ... -map ...
// -map ... -c copy`. AOutputs[i] corresponde ao i-esimo stream de
// audio (em ordem de stream index).
var
  SrcCtx: AVFormatContext;
  SrcPath: UTF8String;
  N, i: Cardinal;
  S: PAVStream;
  AudioIdxs: TArray<Cardinal>;
  Targets: TArray<TArray<Cardinal>>;
  T: TArray<Cardinal>;
  j: Integer;
begin
  Result := False;
  if not FFmpegLibAvailable then Exit;
  if Length(AOutputs) = 0 then Exit;

  SrcPath := ToUtf8(ASrc);
  SrcCtx := nil;
  if avformat_open_input(@SrcCtx, PAnsiChar(SrcPath), nil, nil) < 0 then Exit;
  try
    if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;
    N := av_format_context_nb_streams(SrcCtx);
    SetLength(AudioIdxs, 0);
    for i := 0 to N - 1 do
    begin
      S := GetStreamByIndex(SrcCtx, i);
      if (S = nil) or (S.codecpar = nil) then Continue;
      if S.codecpar.codec_type = AVMEDIA_TYPE_AUDIO then
      begin
        SetLength(AudioIdxs, Length(AudioIdxs) + 1);
        AudioIdxs[High(AudioIdxs)] := i;
      end;
    end;
  finally
    avformat_close_input(@SrcCtx);
  end;

  if Length(AudioIdxs) = 0 then Exit;
  if Length(AOutputs) > Length(AudioIdxs) then Exit;

  // Cada output recebe so um audio stream.
  SetLength(Targets, Length(AOutputs));
  for j := 0 to High(AOutputs) do
  begin
    SetLength(T, 1);
    T[0] := AudioIdxs[j];
    Targets[j] := T;
  end;
  Result := RemuxDispatch(ASrc, Targets, AOutputs);
end;

// =====================================================================
// ExtractFrameJpeg — thumbnail decoder + scaler + JPEG encoder
// =====================================================================

function ExtractFrameJpeg(const ASrc, ADstJpeg: string;
  ATimestampSec, ATargetHeight: Integer): Boolean;
// Pipeline:
//   1. open input + find streams
//   2. seek pra keyframe anterior a ATimestampSec
//   3. decoda pacotes ate receber frame com pts >= ATimestampSec
//   4. swscale pra YUVJ420P no tamanho calculado (preserva aspect)
//   5. encoda como MJPEG e grava no arquivo
//
// ATargetHeight = altura final do JPEG (240 e padrao pra thumbs).
// Largura calculada do aspect ratio (preserva proporcao).
var
  SrcCtx: AVFormatContext;
  SrcPath: UTF8String;
  DstPathW: string;
  VStream: PAVStream;
  VIdx, i: Integer;
  N: Cardinal;
  S: PAVStream;
  Decoder, Encoder: PAVCodec;
  DecCtx, EncCtx: PAVCodecContext;
  EncPar: PAVCodecParameters;
  Pkt, EncPkt: PAVPacket;
  Frame, ScaledFrame: PAVFrame;
  SeekTs: Int64;
  Rc: Integer;
  TargetW, TargetH: Integer;
  SrcW, SrcH: Integer;
  Sws: SwsContext;
  ScaledBufSize: Integer;
  ScaledBuf: PByte;
  StartTs: Int64;
  Got: Boolean;
  FH: THandle;
  Written: DWORD;
  PixFmt: AVPixelFormat;
  TB: AVRational;
begin
  Result := False;
  if not FFmpegLibAvailable then
  begin
    Log('Thumb: libavformat indisponivel.');
    Exit;
  end;
  if ATargetHeight <= 0 then ATargetHeight := 240;

  SrcCtx := nil;
  DecCtx := nil;
  EncCtx := nil;
  Pkt := nil;
  EncPkt := nil;
  Frame := nil;
  ScaledFrame := nil;
  Sws := nil;
  ScaledBuf := nil;
  EncPar := nil;

  try
  SrcPath := ToUtf8(ASrc);
  if avformat_open_input(@SrcCtx, PAnsiChar(SrcPath), nil, nil) < 0 then
  begin
    Log('Thumb: avformat_open_input falhou para %s', [ExtractFileName(ASrc)]);
    Exit;
  end;
  try
    if avformat_find_stream_info(SrcCtx, nil) < 0 then
    begin
      Log('Thumb: avformat_find_stream_info falhou.');
      Exit;
    end;

    // Acha o primeiro stream de video.
    VIdx := -1;
    N := av_format_context_nb_streams(SrcCtx);
    for i := 0 to Integer(N) - 1 do
    begin
      S := GetStreamByIndex(SrcCtx, i);
      if (S <> nil) and (S.codecpar <> nil) and
         (S.codecpar.codec_type = AVMEDIA_TYPE_VIDEO) then
      begin
        VIdx := i;
        Break;
      end;
    end;
    if VIdx < 0 then
    begin
      Log('Thumb: nenhum stream de video encontrado.');
      Exit;
    end;
    VStream := GetStreamByIndex(SrcCtx, VIdx);

    // Decoder.
    Decoder := avcodec_find_decoder(VStream.codecpar.codec_id);
    if Decoder = nil then
    begin
      Log('Thumb: decoder nao encontrado para codec_id=%d.',
        [VStream.codecpar.codec_id]);
      Exit;
    end;
    DecCtx := avcodec_alloc_context3(Decoder);
    if DecCtx = nil then Exit;
    if avcodec_parameters_to_context(DecCtx, VStream.codecpar) < 0 then
    begin
      Log('Thumb: avcodec_parameters_to_context (decoder) falhou.');
      Exit;
    end;
    Rc := avcodec_open2(DecCtx, Decoder, nil);
    if Rc < 0 then
    begin
      Log('Thumb: avcodec_open2 (decoder) falhou (rc=%d).', [Rc]);
      Exit;
    end;

    SrcW := VStream.codecpar.width;
    SrcH := VStream.codecpar.height;
    if (SrcW <= 0) or (SrcH <= 0) then
    begin
      Log('Thumb: video sem dimensoes (%dx%d).', [SrcW, SrcH]);
      Exit;
    end;

    // Tamanho do thumbnail: altura fixa, largura proporcional, par.
    TargetH := ATargetHeight;
    TargetW := (SrcW * TargetH) div SrcH;
    if Odd(TargetW) then Dec(TargetW);
    if TargetW < 16 then TargetW := 16;

    // Seek pro keyframe anterior a ATimestampSec. Se time_base for
    // invalido (den=0), pula o seek e decoda do inicio.
    SeekTs := 0;
    if VStream.time_base.den > 0 then
    begin
      SeekTs := Int64(ATimestampSec) * VStream.time_base.den div VStream.time_base.num;
      av_seek_frame(SrcCtx, VIdx, SeekTs, AVSEEK_FLAG_BACKWARD);
    end;

    // Frame alvo em PTS — pra parar de decodar quando passar.
    StartTs := SeekTs;

    Pkt := av_packet_alloc;
    Frame := av_frame_alloc;
    if (Pkt = nil) or (Frame = nil) then Exit;

    // Decoda ate pegar 1 frame >= StartTs (ou EOF).
    Got := False;
    while not Got do
    begin
      Rc := av_read_frame(SrcCtx, Pkt);
      if Rc < 0 then Break;
      if Pkt.stream_index = VIdx then
      begin
        if avcodec_send_packet(DecCtx, Pkt) = 0 then
        begin
          while avcodec_receive_frame(DecCtx, Frame) = 0 do
          begin
            if (Frame.pts < 0) or (Frame.pts >= StartTs) then
            begin
              Got := True;
              Break;
            end;
            av_frame_unref(Frame);
          end;
        end;
      end;
      av_packet_unref(Pkt);
    end;
    if not Got then
    begin
      Log('Thumb: nao conseguiu decodar nenhum frame >= ts=%d.', [StartTs]);
      Exit;
    end;

    // Scale: source -> YUVJ420P @ TargetW x TargetH.
    PixFmt := AVPixelFormat(Frame.format);
    Sws := sws_getContext(SrcW, SrcH, PixFmt,
                          TargetW, TargetH, AV_PIX_FMT_YUVJ420P,
                          SWS_BICUBIC, nil, nil, nil);
    if Sws = nil then
    begin
      Log('Thumb: sws_getContext falhou (src %dx%d fmt=%d -> %dx%d).',
        [SrcW, SrcH, Integer(PixFmt), TargetW, TargetH]);
      Exit;
    end;

    ScaledFrame := av_frame_alloc;
    if ScaledFrame = nil then Exit;
    ScaledFrame.format := Integer(AV_PIX_FMT_YUVJ420P);
    ScaledFrame.width  := TargetW;
    ScaledFrame.height := TargetH;
    ScaledBufSize := av_image_get_buffer_size(AV_PIX_FMT_YUVJ420P,
      TargetW, TargetH, 32);
    if ScaledBufSize <= 0 then
    begin
      Log('Thumb: av_image_get_buffer_size falhou.');
      Exit;
    end;
    GetMem(ScaledBuf, ScaledBufSize);
    av_image_fill_arrays(@ScaledFrame.data[0], @ScaledFrame.linesize[0],
      ScaledBuf, AV_PIX_FMT_YUVJ420P, TargetW, TargetH, 32);

    sws_scale(Sws, @Frame.data[0], @Frame.linesize[0],
              0, SrcH, @ScaledFrame.data[0], @ScaledFrame.linesize[0]);

    // Encoder MJPEG. Configura via AVCodecParameters (ABI-stable),
    // depois transfere pro AVCodecContext via avcodec_parameters_to_context.
    // Evita acesso direto a campos do AVCodecContext (que nao e ABI-stable).
    Encoder := avcodec_find_encoder(AV_CODEC_ID_MJPEG);
    if Encoder = nil then
    begin
      Log('Thumb: encoder MJPEG nao encontrado.');
      Exit;
    end;
    EncCtx := avcodec_alloc_context3(Encoder);
    if EncCtx = nil then Exit;

    EncPar := avcodec_parameters_alloc;
    if EncPar = nil then Exit;
    try
      EncPar.codec_type := AVMEDIA_TYPE_VIDEO;
      EncPar.codec_id   := AV_CODEC_ID_MJPEG;
      EncPar.width      := TargetW;
      EncPar.height     := TargetH;
      EncPar.format     := Integer(AV_PIX_FMT_YUVJ420P);
      if avcodec_parameters_to_context(EncCtx, EncPar) < 0 then
      begin
        Log('Thumb: avcodec_parameters_to_context (encoder) falhou.');
        Exit;
      end;
    finally
      avcodec_parameters_free(PPointer(@EncPar));
    end;

    // time_base — nao esta em AVCodecParameters, mas e uma AVOption
    // documentada do AVCodecContext, entao av_opt_set_q funciona.
    TB.num := 1;
    TB.den := 25;
    av_opt_set_q(EncCtx, 'time_base', TB, 0);

    Rc := avcodec_open2(EncCtx, Encoder, nil);
    if Rc < 0 then
    begin
      Log('Thumb: avcodec_open2 (encoder MJPEG) falhou (rc=%d).', [Rc]);
      Exit;
    end;

    EncPkt := av_packet_alloc;
    if EncPkt = nil then Exit;

    ScaledFrame.pts := 0;
    Rc := avcodec_send_frame(EncCtx, ScaledFrame);
    if Rc < 0 then
    begin
      Log('Thumb: avcodec_send_frame falhou (rc=%d).', [Rc]);
      Exit;
    end;
    // Flush — sinaliza fim do stream pra MJPEG produzir o packet.
    avcodec_send_frame(EncCtx, nil);
    Rc := avcodec_receive_packet(EncCtx, EncPkt);
    if Rc <> 0 then
    begin
      Log('Thumb: avcodec_receive_packet falhou (rc=%d).', [Rc]);
      Exit;
    end;

    // Grava bytes do JPEG no arquivo. CreateFileW pra suportar paths
    // com acentos (CreateFileA usa locale codepage e quebra).
    DstPathW := ADstJpeg;
    FH := CreateFileW(PWideChar(DstPathW),
      GENERIC_WRITE, 0, nil, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
    if FH = INVALID_HANDLE_VALUE then
    begin
      Log('Thumb: CreateFileW falhou para %s (err=%d).',
        [ADstJpeg, GetLastError]);
      Exit;
    end;
    try
      Written := 0;
      WriteFile(FH, EncPkt.data^, EncPkt.size, Written, nil);
      Result := Written = DWORD(EncPkt.size);
      if Result then
        Log('Thumb: %s -> %d bytes (%dx%d).',
          [ExtractFileName(ADstJpeg), EncPkt.size, TargetW, TargetH])
      else
        Log('Thumb: WriteFile incompleto (%d/%d bytes).',
          [Written, EncPkt.size]);
    finally
      CloseHandle(FH);
    end;
  finally
    if ScaledBuf <> nil then FreeMem(ScaledBuf);
    if Sws <> nil then sws_freeContext(Sws);
    if ScaledFrame <> nil then av_frame_free(@ScaledFrame);
    if Frame <> nil then av_frame_free(@Frame);
    if EncPkt <> nil then av_packet_free(@EncPkt);
    if Pkt <> nil then av_packet_free(@Pkt);
    if EncCtx <> nil then avcodec_free_context(@EncCtx);
    if DecCtx <> nil then avcodec_free_context(@DecCtx);
    if SrcCtx <> nil then avformat_close_input(@SrcCtx);
  end;
  except
    on E: Exception do
    begin
      Log('Thumb: exception %s: %s', [E.ClassName, E.Message]);
      // re-raise pra propagar pro caller (que ja loga tambem).
      raise;
    end;
  end;
end;

function ComputeAudioPeaks(const ASrc: string; ABuckets: Integer;
  out APeaks: TArray<Single>): Boolean;
// Decoda PCM da 1a faixa de audio e produz um array de peaks
// agrupados em ABuckets fatias temporais iguais. Suporta os formatos
// AAC mais comuns (FLTP/FLT/S16P/S16). Outros sao ignorados.
//
// Algoritmo: estima totalSamples via stream.duration; loop linear pelos
// frames decodados, cada sample contribui pro seu bucket via
// floor(sampleIdx * Buckets / totalSamples).
const
  // AVSampleFormat enum (libavutil/samplefmt.h).
  AV_SAMPLE_FMT_S16  = 1;
  AV_SAMPLE_FMT_FLT  = 3;
  AV_SAMPLE_FMT_S16P = 6;
  AV_SAMPLE_FMT_FLTP = 8;
var
  SrcCtx: AVFormatContext;
  AStream: PAVStream;
  CodecPar: PAVCodecParameters;
  Codec: PAVCodec;
  DecCtx: PAVCodecContext;
  Pkt: PAVPacket;
  Frame: PAVFrame;
  AudioIdx, i: Integer;
  Fmt: Integer;
  SampleRate, NumChannels: Integer;
  DurUs: Int64;
  DurSec: Double;
  SampleIdx: Int64;
  s: Integer;
  Channel0Ptr: PByte;
  V: Single;
  Vi16: SmallInt;
  NbStreams: Cardinal;
begin
  Result := False;
  if ABuckets <= 0 then Exit;
  SetLength(APeaks, ABuckets);
  for i := 0 to ABuckets - 1 do APeaks[i] := 0;

  if not FFmpegLibAvailable then Exit;
  SrcCtx := nil;
  DecCtx := nil;
  Pkt := nil;
  Frame := nil;
  try
    if avformat_open_input(@SrcCtx, PAnsiChar(ToUtf8(ASrc)), nil, nil) < 0 then Exit;
    if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;

    // Acha 1a stream de audio via os helpers do FFmpegLib.
    AudioIdx := -1;
    NbStreams := av_format_context_nb_streams(SrcCtx);
    for i := 0 to Integer(NbStreams) - 1 do
    begin
      AStream := GetStreamByIndex(SrcCtx, Cardinal(i));
      if (AStream <> nil) and (AStream.codecpar <> nil) and
         (AStream.codecpar.codec_type = AVMEDIA_TYPE_AUDIO) then
      begin
        AudioIdx := i;
        Break;
      end;
    end;
    if AudioIdx < 0 then Exit;

    AStream := GetStreamByIndex(SrcCtx, Cardinal(AudioIdx));
    CodecPar := AStream.codecpar;
    Codec := avcodec_find_decoder(CodecPar.codec_id);
    if Codec = nil then Exit;
    DecCtx := avcodec_alloc_context3(Codec);
    if DecCtx = nil then Exit;
    if avcodec_parameters_to_context(DecCtx, CodecPar) < 0 then Exit;
    if avcodec_open2(DecCtx, Codec, nil) < 0 then Exit;

    SampleRate := CodecPar.sample_rate;
    if SampleRate <= 0 then SampleRate := 48000;
    NumChannels := CodecPar.ch_layout.nb_channels;
    if NumChannels < 1 then NumChannels := 1;

    // Duracao em segundos — prefere stream.duration, fallback pro
    // av_format_context_duration (microsegundos).
    DurSec := 0;
    if (AStream.duration > 0) and (AStream.time_base.den > 0) then
      DurSec := AStream.duration *
        (AStream.time_base.num / AStream.time_base.den);
    if DurSec <= 0 then
    begin
      DurUs := av_format_context_duration(SrcCtx);
      if DurUs > 0 then DurSec := DurUs / AV_TIME_BASE;
    end;
    if DurSec <= 0 then DurSec := 60;  // chute defensivo (so pra logar)

    Pkt := av_packet_alloc;
    Frame := av_frame_alloc;
    if (Pkt = nil) or (Frame = nil) then Exit;

    Log('Waveform: audio stream=%d codec_id=%d rate=%d ch=%d declared dur=%.1fs',
      [AudioIdx, Integer(CodecPar.codec_id), SampleRate, NumChannels, DurSec]);

    // Estrategia hi-res: em vez de pre-bucketar baseado em duracao
    // declarada (que pode estar errada — vide screenshot mostrando
    // 25% finais vazios), acumula peaks num array de alta resolucao
    // proporcional ao sample count REAL, depois compacta no final.
    //
    // Vantagem: indepedente da duracao declarada na metadata, a
    // waveform sempre representa exatamente os samples decodificados,
    // ocupando 100% da largura visual.
    //
    // 4000 hi-res buckets: pra video de 1h = 0.9s/bucket; pra 30 min
    // = 0.45s/bucket; pra 5 min = 75ms/bucket. ~16KB de memoria.
    const HIRES_BUCKETS = 4000;
    var SamplesPerHiRes: Integer;
    SamplesPerHiRes := SampleRate div 20;  // 50ms por hi-res bucket
    if SamplesPerHiRes < 1 then SamplesPerHiRes := 1;

    var HiResPeaks: TArray<Single>;
    SetLength(HiResPeaks, HIRES_BUCKETS);
    var ActualHiResCount: Integer := 0;

    SampleIdx := 0;
    while av_read_frame(SrcCtx, Pkt) = 0 do
    begin
      if Pkt.stream_index = AudioIdx then
      begin
        if avcodec_send_packet(DecCtx, Pkt) = 0 then
        begin
          while avcodec_receive_frame(DecCtx, Frame) = 0 do
          begin
            Fmt := Frame.format;
            Channel0Ptr := Frame.data[0];
            if Channel0Ptr <> nil then
            begin
              for s := 0 to Frame.nb_samples - 1 do
              begin
                V := 0;
                case Fmt of
                  AV_SAMPLE_FMT_FLTP:
                    V := PSingle(Channel0Ptr + s * SizeOf(Single))^;
                  AV_SAMPLE_FMT_FLT:
                    V := PSingle(Channel0Ptr +
                      s * NumChannels * SizeOf(Single))^;
                  AV_SAMPLE_FMT_S16P:
                    begin
                      Vi16 := PSmallInt(Channel0Ptr + s * SizeOf(SmallInt))^;
                      V := Vi16 / 32768.0;
                    end;
                  AV_SAMPLE_FMT_S16:
                    begin
                      Vi16 := PSmallInt(Channel0Ptr +
                        s * NumChannels * SizeOf(SmallInt))^;
                      V := Vi16 / 32768.0;
                    end;
                end;
                if V < 0 then V := -V;

                // Hi-res bucket por sample count real (NAO duracao
                // declarada). Cresce o array se ultrapassar inicial.
                var HiResIdx: Integer := SampleIdx div SamplesPerHiRes;
                if HiResIdx >= Length(HiResPeaks) then
                  SetLength(HiResPeaks, Length(HiResPeaks) * 2);
                if V > HiResPeaks[HiResIdx] then HiResPeaks[HiResIdx] := V;
                if HiResIdx >= ActualHiResCount then
                  ActualHiResCount := HiResIdx + 1;
                Inc(SampleIdx);
              end;
            end;
            av_frame_unref(Frame);
          end;
        end;
      end;
      av_packet_unref(Pkt);
    end;

    // Flush decoder pra capturar qualquer frame final em buffer.
    avcodec_send_packet(DecCtx, nil);
    while avcodec_receive_frame(DecCtx, Frame) = 0 do
      av_frame_unref(Frame);

    // Compacta hi-res → target buckets, max por chunk.
    if ActualHiResCount = 0 then
    begin
      Log('Waveform: 0 samples processed — empty audio?');
      Exit;
    end;
    for i := 0 to ABuckets - 1 do
    begin
      var StartHi: Integer := (Int64(i) * ActualHiResCount) div ABuckets;
      var EndHi:   Integer := (Int64(i + 1) * ActualHiResCount) div ABuckets;
      if EndHi > ActualHiResCount then EndHi := ActualHiResCount;
      if EndHi <= StartHi then EndHi := StartHi + 1;
      for var j: Integer := StartHi to EndHi - 1 do
        if HiResPeaks[j] > APeaks[i] then APeaks[i] := HiResPeaks[j];
    end;

    var MaxP: Single := 0;
    for i := 0 to ABuckets - 1 do
      if APeaks[i] > MaxP then MaxP := APeaks[i];
    Log('Waveform: %d samples, %d hi-res buckets used, peak max=%.4f',
      [SampleIdx, ActualHiResCount, MaxP]);

    Result := True;
  finally
    if Frame  <> nil then av_frame_free(@Frame);
    if Pkt    <> nil then av_packet_free(@Pkt);
    if DecCtx <> nil then avcodec_free_context(@DecCtx);
    if SrcCtx <> nil then avformat_close_input(@SrcCtx);
  end;
end;

end.

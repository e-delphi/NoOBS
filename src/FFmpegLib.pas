(*
  FFmpegLib - bindings Delphi pra libavformat/libavcodec/libavutil/
  libswscale do FFmpeg 7.x (DLLs bundled: avformat-61.dll,
  avcodec-61.dll, avutil-59.dll, swscale-8.dll).

  Cobre 3 caminhos que substituem chamadas externas pra ffmpeg.exe:
    - PROBE de arquivo (codec, dimensoes, faixas, duracao, metadata)
    - REMUX pra MP4 (-c copy) e extracao de faixas de audio por stream
    - EXTRAÇÃO DE FRAME → JPEG (thumbnails de gravacoes)

  Layout dos structs:
    - AVCodecParameters: ABI-stavel, declarado por completo.
    - AVPacket: ABI-stavel (FFmpeg 5+), declarado.
    - AVFrame: campos publicos no inicio, declaracao parcial.
    - AVFormatContext / AVStream / AVCodecContext: NAO sao stable.
      Acessamos so primeiros campos publicos via offset/struct
      truncado — todos validados contra FFmpeg 7.x.
*)
unit FFmpegLib;

// Delay-loading e Win-only (esperado — esse unit so faz sentido em
// Win64). Silencia warnings W1002 SYMBOL_PLATFORM.
{$WARN SYMBOL_PLATFORM OFF}

interface

uses
  Winapi.Windows,
  OBSLog;

const
  // Carregamento delay-loaded: as DLLs precisam estar ao lado do .exe
  // (OBSStartupCheck garante isso na inicializacao).
  LIB_AVFORMAT  = 'avformat-61.dll';
  LIB_AVCODEC   = 'avcodec-61.dll';
  LIB_AVUTIL    = 'avutil-59.dll';
  LIB_SWSCALE   = 'swscale-8.dll';

  // AVMediaType (avutil)
  AVMEDIA_TYPE_UNKNOWN  = -1;
  AVMEDIA_TYPE_VIDEO    = 0;
  AVMEDIA_TYPE_AUDIO    = 1;
  AVMEDIA_TYPE_DATA     = 2;
  AVMEDIA_TYPE_SUBTITLE = 3;

  // Constantes uteis
  AV_TIME_BASE        = 1000000;     // microsegundos
  AV_LOG_QUIET        = -8;
  AV_LOG_PANIC        = 0;
  AV_LOG_FATAL        = 8;
  AV_LOG_ERROR        = 16;
  AV_LOG_WARNING      = 24;
  AV_LOG_INFO         = 32;

type
  // Ponteiros opacos pra structs grandes/instaveis. Nao acessamos
  // campos via offset — usamos accessors quando precisamos.
  AVFormatContext     = type Pointer;
  PAVFormatContext    = ^AVFormatContext;
  AVInputFormat       = type Pointer;
  AVDictionary        = type Pointer;
  PAVDictionary       = ^AVDictionary;

  // AVCodecParameters — ABI estavel. Mantemos a layout publica.
  AVChannelLayout = record
    order:    Integer;
    nb_channels: Integer;
    u:        Int64;  // union opaca aqui
    opaque:   Pointer;
  end;

  AVRational = record
    num, den: Integer;
  end;

  PAVCodecParameters = ^AVCodecParameters;
  AVCodecParameters = record
    codec_type:          Integer;     // AVMediaType
    codec_id:            Integer;     // AVCodecID
    codec_tag:           Cardinal;
    extradata:           Pointer;
    extradata_size:      Integer;
    coded_side_data:     Pointer;
    nb_coded_side_data:  Integer;
    format:              Integer;
    bit_rate:            Int64;
    bits_per_coded_sample: Integer;
    bits_per_raw_sample: Integer;
    profile:             Integer;
    level:               Integer;
    width:               Integer;
    height:              Integer;
    sample_aspect_ratio: AVRational;
    framerate:           AVRational;
    field_order:         Integer;
    color_range:         Integer;
    color_primaries:     Integer;
    color_trc:           Integer;
    color_space:         Integer;
    chroma_location:     Integer;
    video_delay:         Integer;
    ch_layout:           AVChannelLayout;
    sample_rate:         Integer;
    block_align:         Integer;
    frame_size:          Integer;
    initial_padding:     Integer;
    trailing_padding:    Integer;
    seek_preroll:        Integer;
  end;

  // AVStream — primeiros campos publicos. Ate codecpar e estavel
  // dentro de avformat-61. Campos depois nao usamos.
  PAVStream = ^AVStream;
  AVStream = record
    av_class:      Pointer;
    index:         Integer;
    id:            Integer;
    codecpar:      PAVCodecParameters;
    priv_data:     Pointer;
    time_base:     AVRational;
    start_time:    Int64;
    duration:      Int64;
    nb_frames:     Int64;
    disposition:   Integer;
    discard:       Integer;
    sample_aspect_ratio: AVRational;
    metadata:      AVDictionary;
    // ... resto truncado, nao usamos.
  end;

  PPAVStream = ^PAVStream;

  // AVDictionaryEntry — pra iterar metadata.
  PAVDictionaryEntry = ^AVDictionaryEntry;
  AVDictionaryEntry = record
    key:   PAnsiChar;
    value: PAnsiChar;
  end;

  // AVPacket — ABI estavel desde FFmpeg 5.0. Layout completo.
  PAVPacket = ^AVPacket;
  PPAVPacket = ^PAVPacket;
  AVPacket = record
    buf:              Pointer;     // AVBufferRef*
    pts:              Int64;
    dts:              Int64;
    data:             PByte;
    size:             Integer;
    stream_index:     Integer;
    flags:            Integer;
    side_data:        Pointer;     // AVPacketSideData*
    side_data_elems:  Integer;
    duration:         Int64;
    pos:              Int64;
    opaque:           Pointer;
    opaque_ref:       Pointer;     // AVBufferRef*
    time_base:        AVRational;
  end;

  // AVFrame — campos publicos no inicio. So usamos pra thumbnails.
  // (AV_NUM_DATA_POINTERS = 8 inline pra evitar quebrar o type block.)
  TAVFrameDataPtrs    = array[0..7] of PByte;
  TAVFrameLinesize    = array[0..7] of Integer;

  PAVFrame = ^AVFrame;
  PPAVFrame = ^PAVFrame;
  AVFrame = record
    data:           TAVFrameDataPtrs;
    linesize:       TAVFrameLinesize;
    extended_data:  Pointer;
    width:          Integer;
    height:         Integer;
    nb_samples:     Integer;
    format:         Integer;
    key_frame:      Integer;
    pict_type:      Integer;
    sample_aspect_ratio: AVRational;
    pts:            Int64;
    pkt_dts:        Int64;
    time_base:      AVRational;
    // ... resto truncado, allocador resolve o tamanho real.
  end;

  // AVCodec — primeiros campos publicos.
  PAVCodec = ^AVCodec;
  AVCodec = record
    name:       PAnsiChar;
    long_name:  PAnsiChar;
    codec_type: Integer;
    id:         Integer;
    // ... resto truncado.
  end;

  // AVCodecContext — campos que usamos (encode JPEG). Layout primeiros
  // campos validados pra FFmpeg 7.x. Acesso via accessors quando
  // possivel pra reduzir fragilidade.
  PAVCodecContext = type Pointer;
  PPAVCodecContext = ^PAVCodecContext;

  // SwsContext (libswscale)
  SwsContext = type Pointer;

  // Pixel formats (avutil/pixfmt.h) — so os que usamos.
  AVPixelFormat = Integer;
const
  AV_PIX_FMT_NONE     = -1;
  AV_PIX_FMT_YUV420P  = 0;
  AV_PIX_FMT_YUVJ420P = 12;   // jpeg-range (full) variant
  AV_PIX_FMT_RGB24    = 2;
  AV_PIX_FMT_NV12     = 23;

  // Codec IDs (avcodec.h) — os que usamos.
  AV_CODEC_ID_MJPEG = $0007;

  // Flags
  AVFMT_NOFILE                  = $0001;
  AVFMT_GLOBALHEADER            = $0040;
  AV_CODEC_FLAG_GLOBAL_HEADER   = $00400000;
  AVIO_FLAG_WRITE               = 2;
  AVERROR_EAGAIN                = -11;
  AVERROR_EOF                   = -541478725; // FFERRTAG('E','O','F',' ') as int32
  AVSEEK_FLAG_BACKWARD          = 1;

  // SWS_BICUBIC algorithm flag
  SWS_BICUBIC = 4;

// ---------------------------------------------------------------------
// avformat
// ---------------------------------------------------------------------

function avformat_version: Cardinal; cdecl;
  external LIB_AVFORMAT delayed;

function avformat_open_input(ps: PAVFormatContext; url: PAnsiChar;
  fmt: AVInputFormat; options: PAVDictionary): Integer; cdecl;
  external LIB_AVFORMAT delayed;

function avformat_find_stream_info(ic: AVFormatContext;
  options: PAVDictionary): Integer; cdecl;
  external LIB_AVFORMAT delayed;

procedure avformat_close_input(s: PAVFormatContext); cdecl;
  external LIB_AVFORMAT delayed;

// ---------------------------------------------------------------------
// avformat — accessors (AVFormatContext layout NAO e ABI stable)
// ---------------------------------------------------------------------
// Esses retornam direto o campo. Sao seguros entre versoes do FFmpeg.
function av_format_context_streams(ic: AVFormatContext): PPAVStream;
function av_format_context_nb_streams(ic: AVFormatContext): Cardinal;
function av_format_context_duration(ic: AVFormatContext): Int64;
function av_format_context_bit_rate(ic: AVFormatContext): Int64;
function av_format_context_iformat_name(ic: AVFormatContext): string;

// Retorna o i-esimo AVStream (nil se idx fora do range).
function GetStreamByIndex(ic: AVFormatContext; idx: Cardinal): PAVStream;

// Calcula a duracao varrendo todos os pacotes do arquivo e pegando
// o maior PTS+duration por stream. Caro (O(N) de pacotes), use so
// como ultimo recurso quando o duration global e de stream sao 0.
// Retorna duracao em AV_TIME_BASE (microsegundos).
function ScanDurationByPackets(ic: AVFormatContext): Int64;

// ---------------------------------------------------------------------
// avcodec
// ---------------------------------------------------------------------

function avcodec_version: Cardinal; cdecl;
  external LIB_AVCODEC delayed;

function avcodec_get_name(id: Integer): PAnsiChar; cdecl;
  external LIB_AVCODEC delayed;

// ---------------------------------------------------------------------
// avutil
// ---------------------------------------------------------------------

function avutil_version: Cardinal; cdecl;
  external LIB_AVUTIL delayed;

procedure av_log_set_level(level: Integer); cdecl;
  external LIB_AVUTIL delayed;

function av_dict_get(m: AVDictionary; key: PAnsiChar;
  prev: PAVDictionaryEntry; flags: Integer): PAVDictionaryEntry; cdecl;
  external LIB_AVUTIL delayed;

function av_dict_set(pm: PAVDictionary; key, value: PAnsiChar;
  flags: Integer): Integer; cdecl;
  external LIB_AVUTIL delayed;

procedure av_dict_free(pm: PAVDictionary); cdecl;
  external LIB_AVUTIL delayed;

function av_rescale_q(a: Int64; bq, cq: AVRational): Int64; cdecl;
  external LIB_AVUTIL delayed;

procedure av_freep(ptr: Pointer); cdecl;
  external LIB_AVUTIL delayed;

procedure av_free(ptr: Pointer); cdecl;
  external LIB_AVUTIL delayed;

function av_image_get_buffer_size(pix_fmt: AVPixelFormat; width, height,
  align: Integer): Integer; cdecl; external LIB_AVUTIL delayed;

function av_image_fill_arrays(dst_data: PByte; dst_linesize: PInteger;
  src: PByte; pix_fmt: AVPixelFormat; width, height, align: Integer): Integer; cdecl;
  external LIB_AVUTIL delayed;

// AVOptions — setam campos de AVCodecContext via nome (ABI-safe).
// search_flags = 0 e o caso default (procura no objeto inteiro).
function av_opt_set_int(obj: Pointer; name: PAnsiChar; val: Int64;
  search_flags: Integer): Integer; cdecl; external LIB_AVUTIL delayed;
function av_opt_set_q(obj: Pointer; name: PAnsiChar; val: AVRational;
  search_flags: Integer): Integer; cdecl; external LIB_AVUTIL delayed;
function av_opt_set(obj: Pointer; name: PAnsiChar; val: PAnsiChar;
  search_flags: Integer): Integer; cdecl; external LIB_AVUTIL delayed;

// ---------------------------------------------------------------------
// avcodec — packet, frame, decode/encode
// ---------------------------------------------------------------------

function av_packet_alloc: PAVPacket; cdecl; external LIB_AVCODEC delayed;
procedure av_packet_free(pkt: PPAVPacket{var PAVPacket}); cdecl;
  external LIB_AVCODEC delayed;
procedure av_packet_unref(pkt: PAVPacket); cdecl; external LIB_AVCODEC delayed;
procedure av_packet_rescale_ts(pkt: PAVPacket; tb_src, tb_dst: AVRational); cdecl;
  external LIB_AVCODEC delayed;

// av_frame_* sao da libavutil, NAO libavcodec — vivem em avutil-59.dll.
// Erro classico: declarar em LIB_AVCODEC e tomar C06D007F na 1a call.
function av_frame_alloc: PAVFrame; cdecl; external LIB_AVUTIL delayed;
procedure av_frame_free(frame: PPAVFrame); cdecl; external LIB_AVUTIL delayed;
procedure av_frame_unref(frame: PAVFrame); cdecl; external LIB_AVUTIL delayed;

function avcodec_find_decoder(id: Integer): PAVCodec; cdecl;
  external LIB_AVCODEC delayed;
function avcodec_find_encoder(id: Integer): PAVCodec; cdecl;
  external LIB_AVCODEC delayed;
function avcodec_find_encoder_by_name(name: PAnsiChar): PAVCodec; cdecl;
  external LIB_AVCODEC delayed;
function avcodec_alloc_context3(codec: PAVCodec): PAVCodecContext; cdecl;
  external LIB_AVCODEC delayed;
procedure avcodec_free_context(avctx: PPAVCodecContext); cdecl;
  external LIB_AVCODEC delayed;
function avcodec_parameters_to_context(codec: PAVCodecContext;
  par: PAVCodecParameters): Integer; cdecl; external LIB_AVCODEC delayed;
function avcodec_parameters_copy(dst, src: PAVCodecParameters): Integer; cdecl;
  external LIB_AVCODEC delayed;
function avcodec_parameters_alloc: PAVCodecParameters; cdecl;
  external LIB_AVCODEC delayed;
procedure avcodec_parameters_free(par: PPointer); cdecl;
  external LIB_AVCODEC delayed;
function avcodec_parameters_from_context(par: PAVCodecParameters;
  codec: PAVCodecContext): Integer; cdecl; external LIB_AVCODEC delayed;
function avcodec_open2(avctx: PAVCodecContext; codec: PAVCodec;
  options: PAVDictionary): Integer; cdecl; external LIB_AVCODEC delayed;
function avcodec_send_packet(avctx: PAVCodecContext; avpkt: PAVPacket): Integer; cdecl;
  external LIB_AVCODEC delayed;
function avcodec_receive_frame(avctx: PAVCodecContext; frame: PAVFrame): Integer; cdecl;
  external LIB_AVCODEC delayed;
function avcodec_send_frame(avctx: PAVCodecContext; frame: PAVFrame): Integer; cdecl;
  external LIB_AVCODEC delayed;
function avcodec_receive_packet(avctx: PAVCodecContext; avpkt: PAVPacket): Integer; cdecl;
  external LIB_AVCODEC delayed;

// ---------------------------------------------------------------------
// avformat — read/write, output, IO
// ---------------------------------------------------------------------

function av_read_frame(s: AVFormatContext; pkt: PAVPacket): Integer; cdecl;
  external LIB_AVFORMAT delayed;
function av_seek_frame(s: AVFormatContext; stream_index: Integer;
  timestamp: Int64; flags: Integer): Integer; cdecl;
  external LIB_AVFORMAT delayed;
function avformat_alloc_output_context2(ctx: PAVFormatContext;
  oformat: Pointer; format_name, filename: PAnsiChar): Integer; cdecl;
  external LIB_AVFORMAT delayed;
function avformat_new_stream(s: AVFormatContext; c: PAVCodec): PAVStream; cdecl;
  external LIB_AVFORMAT delayed;
function avio_open2(s: PPointer{AVIOContext**}; url: PAnsiChar;
  flags: Integer; int_cb: Pointer; options: PAVDictionary): Integer; cdecl;
  external LIB_AVFORMAT delayed;
function avio_closep(s: PPointer): Integer; cdecl;
  external LIB_AVFORMAT delayed;
function avformat_write_header(s: AVFormatContext; options: PAVDictionary): Integer; cdecl;
  external LIB_AVFORMAT delayed;
function av_write_trailer(s: AVFormatContext): Integer; cdecl;
  external LIB_AVFORMAT delayed;
function av_interleaved_write_frame(s: AVFormatContext; pkt: PAVPacket): Integer; cdecl;
  external LIB_AVFORMAT delayed;
procedure avformat_free_context(s: AVFormatContext); cdecl;
  external LIB_AVFORMAT delayed;

// ---------------------------------------------------------------------
// swscale
// ---------------------------------------------------------------------

function sws_getContext(srcW, srcH: Integer; srcFormat: AVPixelFormat;
  dstW, dstH: Integer; dstFormat: AVPixelFormat;
  flags: Integer; srcFilter, dstFilter: Pointer; param: Pointer): SwsContext; cdecl;
  external LIB_SWSCALE delayed;
procedure sws_freeContext(ctx: SwsContext); cdecl;
  external LIB_SWSCALE delayed;
function sws_scale(c: SwsContext; srcSlice: Pointer; srcStride: PInteger;
  srcSliceY, srcSliceH: Integer; dst: Pointer; dstStride: PInteger): Integer; cdecl;
  external LIB_SWSCALE delayed;

// ---------------------------------------------------------------------
// Helpers altos — usados pelo OBSProbe e OBSPlayer
// ---------------------------------------------------------------------

// Tenta carregar avformat-61.dll. Cacheia o resultado.
function FFmpegLibAvailable: Boolean;

// Converte string Delphi (UnicodeString) pra UTF-8 sem depender do
// DefaultSystemCodePage. Use sempre antes de passar pra API FFmpeg
// que recebe PAnsiChar — paths, names, metadata, etc.
function ToUtf8(const S: string): UTF8String;

// Le metadata.title (campo opcional comum em MKV).
function GetMetadataString(m: AVDictionary; const Key: string): string;

// Remuxa um arquivo trocando o container. Copia streams sem reencodar
// (equivale a 'ffmpeg -i src -c copy dst'). Adiciona +faststart pra MP4.
// Retorna True em sucesso.
function RemuxFile(const ASrc, ADst: string): Boolean;

// Extrai cada faixa de audio do source pra um arquivo separado (M4A,
// AAC stream copy). AOutputs deve ter Length = numero de audio streams
// em ASrc (descobre via Probe). Faz UMA passada de demux. Retorna
// True se todas faixas foram escritas.
function ExtractAudioTracks(const ASrc: string;
  const AOutputs: TArray<string>): Boolean;

// Extrai um frame em ATimestampSec e salva como JPEG. Faz seek pro
// keyframe anterior, decoda frames ate alcancar ATimestampSec, scala
// pra ATargetHeight preservando aspect, encoda como MJPEG.
function ExtractFrameJpeg(const ASrc, ADstJpeg: string;
  ATimestampSec, ATargetHeight: Integer): Boolean;

implementation

uses
  System.SysUtils;

// ---------------------------------------------------------------------
// Disponibilidade (cache da 1a chamada)
// ---------------------------------------------------------------------

var
  GAvailable: Integer = -1; // -1 = nao testado, 0 = nao disponivel, 1 = disponivel

function FFmpegLibAvailable: Boolean;
var
  H: HMODULE;
begin
  if GAvailable = -1 then
  begin
    H := LoadLibrary(PChar(LIB_AVFORMAT));
    if H = 0 then GAvailable := 0
    else
    begin
      GAvailable := 1;
      // Mute logs por padrao — caller pode aumentar com av_log_set_level.
      try av_log_set_level(AV_LOG_QUIET); except end;
      FreeLibrary(H);
    end;
  end;
  Result := GAvailable = 1;
end;

// ---------------------------------------------------------------------
// Accessors AVFormatContext via offsets calculados
// ---------------------------------------------------------------------
//
// AVFormatContext NAO e ABI-stable, mas os primeiros campos sao
// fixos no FFmpeg 7.x. Layout (parcial, ate bit_rate):
//
//   const AVClass*  av_class;            // 0
//   const AVInputFormat *iformat;        // 8
//   const AVOutputFormat *oformat;       // 16
//   void *priv_data;                     // 24
//   AVIOContext *pb;                     // 32
//   int ctx_flags;                       // 40 (4 bytes + 4 padding)
//   unsigned int nb_streams;             // 48 ← USAMOS
//   AVStream **streams;                  // 56 ← USAMOS
//   ... outros campos ...
//   int64_t duration;                    // offset ~104 ← USAMOS
//   int64_t bit_rate;                    // offset ~112 ← USAMOS
//
// Esses offsets sao validos pra FFmpeg 7.x (avformat-61). Se a major
// version mudar, recalcular.

type
  PCardinalUns = ^Cardinal;
  PPtr = ^Pointer;
  // Array de tamanho sentinel — so usamos via indice ate nb_streams.
  TStreamPtrArray = array[0..16383] of PAVStream;
  PStreamPtrArray = ^TStreamPtrArray;

const
  // Offsets validados contra ffmpeg n7.1 (avformat-61). Layout
  // AVFormatContext (Win64, natural alignment):
  //   av_class*    @ 0
  //   iformat*     @ 8
  //   oformat*     @ 16
  //   priv_data*   @ 24
  //   pb*          @ 32
  //   ctx_flags    @ 40 (int)
  //   nb_streams   @ 44 (uint)
  //   streams**    @ 48
  //   url*         @ 56
  //   start_time   @ 64 (int64)
  //   duration     @ 72 (int64)
  //   bit_rate     @ 80 (int64)
  OFFS_IFORMAT    = 8;
  OFFS_NB_STREAMS = 44;
  OFFS_STREAMS    = 48;
  OFFS_PB         = 32;
  OFFS_DURATION   = 72;
  OFFS_BIT_RATE   = 80;

function PtrOffset(P: Pointer; AOffset: NativeInt): Pointer; inline;
begin
  Result := Pointer(NativeUInt(P) + NativeUInt(AOffset));
end;

function av_format_context_streams(ic: AVFormatContext): PPAVStream;
begin
  if ic = nil then Result := nil
  else Result := PPAVStream(PPtr(PtrOffset(ic, OFFS_STREAMS))^);
end;

function av_format_context_nb_streams(ic: AVFormatContext): Cardinal;
begin
  if ic = nil then Result := 0
  else Result := PCardinalUns(PtrOffset(ic, OFFS_NB_STREAMS))^;
end;

function GetStreamByIndex(ic: AVFormatContext; idx: Cardinal): PAVStream;
var
  StreamsArr: PStreamPtrArray;
begin
  Result := nil;
  StreamsArr := PStreamPtrArray(av_format_context_streams(ic));
  if StreamsArr = nil then Exit;
  if idx >= av_format_context_nb_streams(ic) then Exit;
  Result := StreamsArr^[idx];
end;

function ScanDurationByPackets(ic: AVFormatContext): Int64;
// Caminho lento mas confiavel: le todos os pacotes, acompanha o max
// (pts+duration) por stream, converte da time_base do stream pra
// microsegundos. Usado quando MKV nao tem Duration EBML escrita
// (recording stoppada de forma nao-clean).
// Custo: O(numero de pacotes). Avanca o cursor do arquivo — caller
// deve fechar o ic depois (a gente fecha logo apos no Probe).
const
  AV_NOPTS_VALUE = Int64($8000000000000000);
var
  Pkt: PAVPacket;
  S: PAVStream;
  EndPts, EndUs, Best: Int64;
begin
  Result := 0;
  if ic = nil then Exit;
  Pkt := av_packet_alloc;
  if Pkt = nil then Exit;
  Best := 0;
  try
    while av_read_frame(ic, Pkt) = 0 do
    begin
      try
        S := GetStreamByIndex(ic, Cardinal(Pkt.stream_index));
        if S = nil then Continue;
        if S.time_base.den <= 0 then Continue;
        if Pkt.pts = AV_NOPTS_VALUE then Continue;

        EndPts := Pkt.pts + Pkt.duration;
        EndUs := EndPts * AV_TIME_BASE * S.time_base.num div S.time_base.den;
        if EndUs > Best then Best := EndUs;
      finally
        av_packet_unref(Pkt);
      end;
    end;
  finally
    av_packet_free(@Pkt);
  end;
  Result := Best;
end;

function av_format_context_duration(ic: AVFormatContext): Int64;
// Duracao em AV_TIME_BASE (microsegundos). Tenta o campo direto;
// se zero (comum em MKV — EBML nao force populacao desse campo),
// pega o maximo das duracoes dos streams (cada um na sua time_base).
var
  N, i: Cardinal;
  S: PAVStream;
  Best, DurUs: Int64;
begin
  Result := 0;
  if ic = nil then Exit;
  Result := PInt64(PtrOffset(ic, OFFS_DURATION))^;
  if Result > 0 then Exit;

  N := av_format_context_nb_streams(ic);
  if N = 0 then Exit;
  Best := 0;
  for i := 0 to N - 1 do
  begin
    S := GetStreamByIndex(ic, i);
    if S = nil then Continue;
    if (S.duration > 0) and (S.time_base.den > 0) then
    begin
      DurUs := S.duration * AV_TIME_BASE * S.time_base.num div S.time_base.den;
      if DurUs > Best then Best := DurUs;
    end;
  end;
  Result := Best;
end;

function av_format_context_bit_rate(ic: AVFormatContext): Int64;
// Bit rate em bps. Tenta o campo direto; se zero, soma dos streams.
var
  N, i: Cardinal;
  S: PAVStream;
begin
  Result := 0;
  if ic = nil then Exit;
  Result := PInt64(PtrOffset(ic, OFFS_BIT_RATE))^;
  if Result > 0 then Exit;

  N := av_format_context_nb_streams(ic);
  if N = 0 then Exit;
  for i := 0 to N - 1 do
  begin
    S := GetStreamByIndex(ic, i);
    if (S = nil) or (S.codecpar = nil) then Continue;
    if S.codecpar.bit_rate > 0 then
      Result := Result + S.codecpar.bit_rate;
  end;
end;

function av_format_context_iformat_name(ic: AVFormatContext): string;
// iformat e AVInputFormat*. Primeiro campo do struct e o `name`
// (const char*). AVInputFormat e ABI-stable.
type
  PIFmtHead = ^IFmtHead;
  IFmtHead = record
    name: PAnsiChar;
    // ... resto nao usado
  end;
var
  IFmt: Pointer;
begin
  Result := '';
  if ic = nil then Exit;
  IFmt := PPtr(PtrOffset(ic, OFFS_IFORMAT))^;
  if IFmt = nil then Exit;
  // FFmpeg armazena todas as strings em UTF-8; conversao via
  // AnsiString interpretaria como locale (cp1252) e quebraria
  // acentos. UTF8ToString faz a leitura correta.
  if PIFmtHead(IFmt).name <> nil then
    Result := UTF8ToString(PIFmtHead(IFmt).name);
end;

// ---------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------

function ToUtf8(const S: string): UTF8String;
begin
  Result := UTF8Encode(S);
end;

function GetMetadataString(m: AVDictionary; const Key: string): string;
// Valores de metadata em FFmpeg sao sempre UTF-8 (ate em MKV onde
// o EBML preserva o encoding original do gravador). UTF8ToString
// converte direto pra UnicodeString sem passar pela locale.
var
  Entry: PAVDictionaryEntry;
  AKey: AnsiString;
begin
  Result := '';
  if m = nil then Exit;
  AKey := AnsiString(Key);
  Entry := av_dict_get(m, PAnsiChar(AKey), nil, 0);
  if (Entry <> nil) and (Entry.value <> nil) then
    Result := UTF8ToString(Entry.value);
end;

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
  // Seta pb no AVFormatContext (campo `pb` em AVFormatContext).
  PPtr(PtrOffset(AOut.Ctx, OFFS_PB))^ := Pb;

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

end.

(*
  FFmpegLib - bindings raw Delphi pra libavformat/libavcodec/libavutil/
  libswscale do FFmpeg 7.x (DLLs bundled: avformat-61.dll,
  avcodec-61.dll, avutil-59.dll, swscale-8.dll).

  Esta unit e camada 1 (binding raw). Operacoes de alto nivel
  (RemuxFile, ExtractAudioTracks, ExtractFrameJpeg) ficam em FFmpegOps.

  Conteudo:
    - Declaracoes `external` das DLLs (avformat/avcodec/avutil/swscale).
    - Structs FFmpeg (AVPacket, AVFrame, AVCodecParameters, AVStream*,
      etc).
    - Constantes (codec IDs, pixel formats, flags).
    - Acessors low-level via offset pra structs nao-ABI-stable
      (AVFormatContext campos: streams, nb_streams, duration, ...).
    - Helpers basicos: FFmpegLibAvailable, ToUtf8, GetMetadataString,
      ScanDurationByPackets.

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

  // AVStream — primeiros campos publicos do AVStream em FFmpeg 7.x
  // (avformat-61). Layout casa com a ordem em avformat.h. Offsets
  // calculados pra Win64 (ponteiros 8B, alinhamento natural):
  //   av_class             0  | nb_frames            56
  //   index                8  | disposition          64
  //   id                  12  | discard              68
  //   codecpar            16  | sample_aspect_ratio  72
  //   priv_data           24  | metadata             80
  //   time_base           32  | avg_frame_rate       88  ← FPS aqui
  //   start_time          40  | attached_pic         96  (resto truncado)
  //   duration            48  |
  // Validar contra avformat.h se subir o major (61→62).
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
    avg_frame_rate: AVRational;  // FPS medio (para VFR e o mais util)
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

// Atribui o ponteiro AVIOContext* no campo `pb` do AVFormatContext.
// Encapsula o acesso via offset (que e detalhe de implementacao ABI)
// pra que callers (FFmpegOps) nao precisem ver PPtr/OFFS_PB.
procedure av_format_context_set_pb(ic: AVFormatContext; pb: Pointer);

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
// Helpers baixos — utilidades sobre os bindings raw acima
// ---------------------------------------------------------------------

// Tenta carregar avformat-61.dll. Cacheia o resultado.
function FFmpegLibAvailable: Boolean;

// Converte string Delphi (UnicodeString) pra UTF-8 sem depender do
// DefaultSystemCodePage. Use sempre antes de passar pra API FFmpeg
// que recebe PAnsiChar — paths, names, metadata, etc.
function ToUtf8(const S: string): UTF8String;

// Le metadata.title (campo opcional comum em MKV).
function GetMetadataString(m: AVDictionary; const Key: string): string;

// Operacoes de alto nivel (RemuxFile, ExtractAudioTracks,
// ExtractFrameJpeg) foram movidas pra unit FFmpegOps. Consumidores
// que precisam delas devem importar `FFmpegOps` direto.

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

procedure av_format_context_set_pb(ic: AVFormatContext; pb: Pointer);
begin
  if ic = nil then Exit;
  PPtr(PtrOffset(ic, OFFS_PB))^ := pb;
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


end.

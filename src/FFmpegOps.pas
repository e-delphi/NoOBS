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
    ListVideoKeyframes — tempos dos keyframes, lidos do indice (Cues).

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

// Une N arquivos em um novo output (stream copy, sem reencode), na ordem
// recebida. Os inputs precisam ter a mesma estrutura de streams/codec; quando
// isso nao for verdade, retorna False com AIncompatible=True.
function MergeFiles(const AInputs: TArray<string>; const ADst: string;
  out AIncompatible: Boolean): Boolean;

// Emenda o trecho do buffer em memoria (AHead) com a gravacao que continuou
// a partir dele (ATail), num MKV unico em ADst (stream copy). Os dois sairam
// dos MESMOS encoders, entao o comeco de ATail existe byte a byte no fim de
// AHead: a emenda acha esse ponto pelo CONTEUDO dos pacotes e pula de ATail
// exatamente o que AHead ja tem — nenhum quadro perdido, nenhum repetido.
// ALeadMs = quanto tempo antes do fim de AHead a gravacao comecou (desempata
// keyframes identicos de tela parada; <= 0 = desconhecido). Retorna False,
// com o motivo em AInfo, se nao achar o ponto de encontro: quem chama deve
// manter os dois arquivos em vez de emendar as cegas.
function SpliceContinuation(const AHead, ATail, ADst: string;
  ALeadMs: Integer; out AInfo: string): Boolean;

// Extrai faixas de audio do source pra arquivos separados (M4A, AAC stream
// copy). AOutputs[j] recebe o (j+AAudioStartIndex)-esimo audio stream — use
// AAudioStartIndex=1 pra pular o mix (track 1) e extrair so as isoladas.
// Faz UMA passada de demux. Retorna True se todas as faixas foram escritas.
function ExtractAudioTracks(const ASrc: string;
  const AOutputs: TArray<string>; AAudioStartIndex: Integer = 0): Boolean;

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

type
  // Recebe um bloco de amostras MONO em float [-1..1], na taxa original do
  // arquivo (ARate). Devolver False interrompe a decodificacao.
  TAudioBlockFunc = reference to function(AData: PSingle; ACount,
    ARate: Integer): Boolean;

// Decoda a 1a faixa de audio inteira, misturando os canais em mono, e
// entrega em blocos pra quem chamou — sem guardar o arquivo todo em
// memoria (1 h a 48 kHz em float seriam ~700 MB). Usado pela transcricao
// local (OBSLocalAsr), que reamostra pra 16 kHz no caminho. Formatos:
// FLTP/FLT/S16P/S16, os mesmos do ComputeAudioPeaks. False = nao abriu,
// nao ha audio, formato nao suportado ou o callback interrompeu.
function DecodeAudioMono(const ASrc: string; AOnBlock: TAudioBlockFunc): Boolean;

// Tempos (segundos, pts) dos keyframes do 1o stream de video, lidos do
// INDICE do container (Cues no MKV) — nao le pacote nenhum, custa poucos ms
// mesmo em horas de gravacao. AFps = avg_frame_rate (0 se desconhecido).
// Retorna False se nao ha indice (gravacao interrompida sem trailer).
function ListVideoKeyframes(const ASrc: string;
  out ATimes: TArray<Double>; out AFps: Double): Boolean;

implementation

uses
  Winapi.Windows,
  System.Generics.Collections,
  OBSLog,
  FFmpegLib;

const
  // AV_NOPTS_VALUE vive na implementation de FFmpegLib (nao exportado na
  // interface), entao redeclaramos aqui pro rebase de timestamps.
  // Mesmo valor de avutil (INT64_MIN).
  AV_NOPTS_VALUE = Int64($8000000000000000);

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

procedure CopyStreamTag(ASrc, ADst: PAVStream; const AKey: PAnsiChar);
// Copia UMA tag de metadata (ex.: 'title', 'language') de um stream pro
// outro, se existir. Usado no lugar de av_dict_copy pra NAO arrastar tags
// que ficam invalidas apos corte/remux (DURATION, _STATISTICS_*, NUMBER_OF_*
// — o demuxer le DURATION e reportaria a duracao do arquivo original).
var
  Entry: PAVDictionaryEntry;
begin
  if (ASrc = nil) or (ADst = nil) then Exit;
  Entry := av_dict_get(ASrc.metadata, AKey, nil, 0);
  if (Entry <> nil) and (Entry.value <> nil) then
    av_dict_set(@ADst.metadata, AKey, Entry.value, 0);
end;

function OpenInputWithRetry(var ACtx: AVFormatContext; const APath: string): Boolean;
// avformat_open_input com retry curto. Cobre o arquivo estar MOMENTANEAMENTE
// aberto/travado por outra thread — em especial a geracao de previa/duracao
// da biblioteca (Probe + thumbnail), que abre o video por ~200-500ms. Sem
// isto, unir/dividir logo apos adicionar videos falhava com "arquivo em uso".
// So chamado de worker thread (merge), entao o Sleep e aceitavel.
const
  MAX_RETRIES = 10;
  RETRY_MS = 200;
var
  Attempt, Rc: Integer;
begin
  Result := False;
  for Attempt := 0 to MAX_RETRIES - 1 do
  begin
    ACtx := nil;
    Rc := avformat_open_input(@ACtx, PAnsiChar(ToUtf8(APath)), nil, nil);
    if Rc >= 0 then Exit(True);
    if ACtx <> nil then avformat_close_input(@ACtx);
    ACtx := nil;
    if Attempt = 0 then
      Log('OpenInput: "%s" ocupado (rc=%d) — tentando de novo.',
        [System.SysUtils.ExtractFileName(APath), Rc]);
    Sleep(RETRY_MS);
  end;
  Log('OpenInput: desistiu de "%s" apos %d tentativas.',
    [System.SysUtils.ExtractFileName(APath), MAX_RETRIES]);
end;

function OpenOutputForStreams(const ASrcCtx: AVFormatContext;
  const ADstFilename: string;
  const AKeepStreamIdx: TArray<Cardinal>;
  out AOut: TOutputStream; const AContainer: AnsiString = ''): Boolean;
// Aloca AVFormatContext de saida, cria streams espelhando os indices
// selecionados, abre IO, escreve header. Em sucesso AOut.StreamMap
// tem o mapeamento; em falha, libera tudo. AContainer vazio = deduz pela
// extensao; quem escreve num '.part' passa o muxer explicito (pegadinha #51e).
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

  if AContainer <> '' then ContainerFmt := AContainer
  else ContainerFmt := DetectContainerFromExt(ADstFilename);
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
    // Preserva SO o title (nome da faixa de audio) e o language. NAO copia a
    // metadata inteira (av_dict_copy): o Matroska guarda tags DURATION e
    // _STATISTICS_*/NUMBER_OF_* POR STREAM que ficam ERRADAS depois do corte
    // — o demuxer usa a tag DURATION pra reportar a duracao, entao copiar
    // tudo fazia a parte mostrar a duracao do ORIGINAL na lista.
    CopyStreamTag(SrcStream, DstStream, 'title');
    CopyStreamTag(SrcStream, DstStream, 'language');
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
  AnyHeader, WriteFailed: Boolean;
  WrRc: Integer;
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
    WriteFailed := False;
    while (not WriteFailed) and (av_read_frame(SrcCtx, Pkt) = 0) do
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
          // Captura o retorno: ignorar erros de write produzia MP4
          // cacheado truncado servido ao player como se fosse sucesso
          // (disco cheio, codec incompativel). Falha de muxer e
          // efetivamente fatal — aborta e devolve False.
          try
            WrRc := av_interleaved_write_frame(Outs[i].Ctx, Pkt);
          except
            WrRc := -1;
          end;
          if WrRc < 0 then
          begin
            Log('Remux: av_interleaved_write_frame falhou (rc=%d) — abortando.',
              [WrRc]);
            WriteFailed := True;
          end;
          // Restaura stream_index pro proximo output que tambem
          // queira esse pacote — diferencas de time_base sao
          // recalculadas pelo rescale a cada output.
          Pkt.stream_index := SrcStream.index;
        end;
      finally
        av_packet_unref(Pkt);
      end;
    end;

    Result := not WriteFailed;
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

function StreamParamsCompatible(ARef, ASrc: PAVStream): Boolean;
var
  R, S: PAVCodecParameters;
begin
  Result := False;
  if (ARef = nil) or (ASrc = nil) or
     (ARef.codecpar = nil) or (ASrc.codecpar = nil) then Exit;
  R := ARef.codecpar;
  S := ASrc.codecpar;
  if R.codec_type <> S.codec_type then Exit;
  if R.codec_id <> S.codec_id then Exit;

  case R.codec_type of
    AVMEDIA_TYPE_VIDEO:
      begin
        if R.width <> S.width then Exit;
        if R.height <> S.height then Exit;
      end;
    AVMEDIA_TYPE_AUDIO:
      begin
        if R.sample_rate <> S.sample_rate then Exit;
        if R.ch_layout.nb_channels <> S.ch_layout.nb_channels then Exit;
      end;
  end;

  Result := True;
end;

function InputsCompatible(ARefCtx, ASrcCtx: AVFormatContext): Boolean;
var
  N, M, i: Cardinal;
begin
  Result := False;
  if (ARefCtx = nil) or (ASrcCtx = nil) then Exit;
  N := av_format_context_nb_streams(ARefCtx);
  M := av_format_context_nb_streams(ASrcCtx);
  if (N = 0) or (N <> M) then Exit;

  for i := 0 to N - 1 do
    if not StreamParamsCompatible(GetStreamByIndex(ARefCtx, i),
                                  GetStreamByIndex(ASrcCtx, i)) then
      Exit;

  Result := True;
end;

function TimestampToUs(ATs: Int64; ATb: AVRational): Int64;
var
  TbUs: AVRational;
begin
  if (ATs = AV_NOPTS_VALUE) or (ATb.den <= 0) then
    Exit(AV_NOPTS_VALUE);
  TbUs.num := 1;
  TbUs.den := AV_TIME_BASE;
  Result := av_rescale_q(ATs, ATb, TbUs);
end;

function UsToTimestamp(AUs: Int64; ATb: AVRational): Int64;
var
  TbUs: AVRational;
begin
  if ATb.den <= 0 then Exit(AV_NOPTS_VALUE);
  TbUs.num := 1;
  TbUs.den := AV_TIME_BASE;
  Result := av_rescale_q(AUs, TbUs, ATb);
end;

function MergeFiles(const AInputs: TArray<string>; const ADst: string;
  out AIncompatible: Boolean): Boolean;
var
  RefCtx, SrcCtx: AVFormatContext;
  OutFile: TOutputStream;
  Pkt: PAVPacket;
  AllIdx: TArray<Cardinal>;
  BaseUs: Int64;
  N, i: Cardinal;
  InputIdx: Integer;
  Sidx, DstIdx: Integer;
  SrcStream, DstStream: PAVStream;
  SrcTb, DstTb: AVRational;
  GlobalOffsetUs, LocalBestUs, DeclaredDurUs: Int64;
  PktBaseUs, PtsUs, DtsUs, EndUs, DurUs: Int64;
  WriteFailed: Boolean;
  WrRc: Integer;
begin
  Result := False;
  AIncompatible := False;
  if not FFmpegLibAvailable then Exit;
  if (Length(AInputs) < 2) or (ADst = '') then Exit;

  RefCtx := nil;
  SrcCtx := nil;
  Pkt := nil;
  FillChar(OutFile, SizeOf(OutFile), 0);
  try
    if not OpenInputWithRetry(RefCtx, AInputs[0]) then Exit;
    if avformat_find_stream_info(RefCtx, nil) < 0 then Exit;
    N := av_format_context_nb_streams(RefCtx);
    if N = 0 then Exit;

    SetLength(AllIdx, N);
    for i := 0 to N - 1 do AllIdx[i] := i;
    if not OpenOutputForStreams(RefCtx, ADst, AllIdx, OutFile) then Exit;

    Pkt := av_packet_alloc;
    if Pkt = nil then Exit;

    GlobalOffsetUs := 0;
    WriteFailed := False;
    for InputIdx := 0 to High(AInputs) do
    begin
      if WriteFailed then Break;
      if SrcCtx <> nil then avformat_close_input(@SrcCtx);
      SrcCtx := nil;
      if not OpenInputWithRetry(SrcCtx, AInputs[InputIdx]) then Exit;
      if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;
      if not InputsCompatible(RefCtx, SrcCtx) then
      begin
        AIncompatible := True;
        Exit;
      end;

      DeclaredDurUs := av_format_context_duration(SrcCtx);
      if DeclaredDurUs < 0 then DeclaredDurUs := 0;
      LocalBestUs := 0;
      // Base UNICA por arquivo (o 1o pacote lido — o demuxer entrega
      // intercalado por tempo), nao uma base POR STREAM. A diferenca de inicio
      // entre os streams de um mesmo arquivo e informacao de sync: nas
      // gravacoes do OBS o audio comeca ~20ms antes do video. Zerando cada
      // stream pelo proprio 1o pacote, essa diferenca sumia e o audio
      // escorregava alguns ms em relacao ao video a cada trecho unido.
      BaseUs := AV_NOPTS_VALUE;

      while (not WriteFailed) and (av_read_frame(SrcCtx, Pkt) = 0) do
      begin
        try
          Sidx := Pkt.stream_index;
          if (Sidx < 0) or (Cardinal(Sidx) >= N) then Continue;
          SrcStream := GetStreamByIndex(SrcCtx, Cardinal(Sidx));
          if SrcStream = nil then Continue;
          DstIdx := OutFile.StreamMap[Sidx];
          if DstIdx < 0 then Continue;
          DstStream := GetStreamByIndex(OutFile.Ctx, Cardinal(DstIdx));
          if DstStream = nil then Continue;

          SrcTb := SrcStream.time_base;
          DstTb := DstStream.time_base;
          PktBaseUs := TimestampToUs(Pkt.dts, SrcTb);
          if PktBaseUs = AV_NOPTS_VALUE then
            PktBaseUs := TimestampToUs(Pkt.pts, SrcTb);
          if PktBaseUs = AV_NOPTS_VALUE then PktBaseUs := 0;
          if BaseUs = AV_NOPTS_VALUE then BaseUs := PktBaseUs;

          if Pkt.pts <> AV_NOPTS_VALUE then
          begin
            PtsUs := TimestampToUs(Pkt.pts, SrcTb);
            if PtsUs <> AV_NOPTS_VALUE then
            begin
              Dec(PtsUs, BaseUs);
              if PtsUs < 0 then PtsUs := 0;
              Pkt.pts := UsToTimestamp(PtsUs + GlobalOffsetUs, DstTb);
            end
            else
              Pkt.pts := AV_NOPTS_VALUE;
          end;
          if Pkt.dts <> AV_NOPTS_VALUE then
          begin
            DtsUs := TimestampToUs(Pkt.dts, SrcTb);
            if DtsUs <> AV_NOPTS_VALUE then
            begin
              Dec(DtsUs, BaseUs);
              if DtsUs < 0 then DtsUs := 0;
              Pkt.dts := UsToTimestamp(DtsUs + GlobalOffsetUs, DstTb);
            end
            else
              Pkt.dts := AV_NOPTS_VALUE;
          end;
          if Pkt.duration > 0 then
          begin
            DurUs := TimestampToUs(Pkt.duration, SrcTb);
            if DurUs > 0 then
              Pkt.duration := UsToTimestamp(DurUs, DstTb)
            else
              Pkt.duration := 0;
          end;
          Pkt.stream_index := DstIdx;
          Pkt.pos := -1;

          EndUs := PktBaseUs - BaseUs;
          DurUs := TimestampToUs(Pkt.duration, DstTb);
          if DurUs > 0 then Inc(EndUs, DurUs);
          if EndUs > LocalBestUs then LocalBestUs := EndUs;

          try
            WrRc := av_interleaved_write_frame(OutFile.Ctx, Pkt);
          except
            WrRc := -1;
          end;
          if WrRc < 0 then
          begin
            Log('Merge: av_interleaved_write_frame falhou (rc=%d).', [WrRc]);
            WriteFailed := True;
          end;
        finally
          av_packet_unref(Pkt);
        end;
      end;

      if DeclaredDurUs > LocalBestUs then LocalBestUs := DeclaredDurUs;
      if LocalBestUs > 0 then Inc(GlobalOffsetUs, LocalBestUs);
    end;

    Result := not WriteFailed;
    Log('Merge: inputs=%d durationUs=%d result=%s',
      [Length(AInputs), GlobalOffsetUs, BoolToStr(Result, True)]);
  finally
    if Pkt <> nil then av_packet_free(@Pkt);
    if SrcCtx <> nil then avformat_close_input(@SrcCtx);
    if RefCtx <> nil then avformat_close_input(@RefCtx);
    CloseOutput(OutFile);
  end;
end;

// =====================================================================
// SpliceContinuation — trecho do buffer + gravacao que continuou dele
// =====================================================================
//
// Os dois arquivos sairam dos MESMOS encoders (a gravacao foi pendurada
// nos encoders do buffer, sem remontar a captura), entao os pacotes sao
// identicos byte a byte. O que NAO bate sao os timestamps: cada saida da
// libobs zera o relogio no proprio comeco (obs-output.c:2042 na gravacao,
// obs-ffmpeg-mux.c:1163 no "save" do buffer), faixa por faixa. Por isso a
// emenda se ancora pelo CONTEUDO:
//   1. copia o trecho do buffer inteiro, guardando uma assinatura de cada
//      pacote (tamanho + hash das pontas + flag de keyframe);
//   2. acha no fim dele o 1o pacote de video da gravacao (um keyframe) e
//      confirma que os seguintes tambem batem (corrida de VERIFY_PKTS);
//   3. faz o mesmo em cada faixa de audio. Faixa MUDA casa em dezenas de
//      lugares (todo pacote de silencio e igual); ela usa o deslocamento de
//      uma faixa com som que casou num lugar so — todas saem do mesmo
//      mixer, nos mesmos instantes, e os dois arquivos cortam todas juntas;
//   4. da gravacao, pula exatamente os pacotes que o buffer ja tinha e
//      escreve o resto deslocado pro relogio do buffer.
// A contagem e por PACOTE, nao por tempo: o MKV guarda ms e arredonda
// diferente nos dois arquivos, entao comparar timestamp erraria por 1 tique
// na borda (um pacote a mais ou a menos). Pacote casado nao tem essa folga.

const
  AV_PKT_FLAG_KEY = $0001;

type
  TPktSig = record
    TsUs: Int64;       // pts em us (dts quando nao ha pts)
    Size: Integer;
    Fp: UInt64;
    Key: Boolean;
  end;

{$IFOPT Q+}{$DEFINE SPLICE_Q_WAS_ON}{$ENDIF}
{$Q-} // o FNV multiplica UInt64 estourando de proposito
function PacketFingerprint(P: PAVPacket): UInt64;
// FNV-1a 64 so das PONTAS do pacote (4 KB de cada lado): barato mesmo num
// keyframe 4K de megabytes. Tamanho + pontas ja distinguem pacotes
// diferentes na janela curta da busca, e a corrida confirma o resto.
const
  EDGE = 4096;
  FNV_OFFSET: UInt64 = $CBF29CE484222325;
  FNV_PRIME: UInt64 = $100000001B3;
var
  H: UInt64;
  D: PByte;
  Sz: Integer;

  procedure Mix(AFrom, ACount: Integer);
  var
    k: Integer;
  begin
    for k := AFrom to AFrom + ACount - 1 do
    begin
      H := H xor UInt64(D[k]);
      H := H * FNV_PRIME;
    end;
  end;

begin
  D := P.data;
  Sz := P.size;
  if (D = nil) or (Sz <= 0) then Exit(0);
  H := FNV_OFFSET;
  if Sz <= 2 * EDGE then
    Mix(0, Sz)
  else
  begin
    Mix(0, EDGE);
    Mix(Sz - EDGE, EDGE);
  end;
  Result := H xor UInt64(Sz);
end;
{$IFDEF SPLICE_Q_WAS_ON}{$Q+}{$UNDEF SPLICE_Q_WAS_ON}{$ENDIF}

function SigEqual(const A, B: TPktSig): Boolean;
begin
  Result := (A.Size = B.Size) and (A.Fp = B.Fp) and (A.Key = B.Key);
end;

function RunMatches(AHead: TList<TPktSig>; J: Integer;
  ATailStart: TList<TPktSig>): Boolean;
// Os pacotes do comeco da gravacao batem com os do buffer a partir de J?
// Confere ate onde os dois tem pacote — o buffer acaba no meio da corrida.
var
  k: Integer;
begin
  Result := False;
  for k := 0 to ATailStart.Count - 1 do
  begin
    if J + k >= AHead.Count then Break;
    if not SigEqual(AHead[J + k], ATailStart[k]) then Exit;
  end;
  Result := True;
end;

function SpliceContinuation(const AHead, ATail, ADst: string;
  ALeadMs: Integer; out AInfo: string): Boolean;
const
  // Pacotes do comeco da gravacao conferidos em sequencia, por faixa.
  VERIFY_PKTS = 12;
  // So procura o ponto de encontro nos ultimos 15 s do buffer: a gravacao
  // comecou ~1 s antes do fim dele (ReplaySaveCommitDelayMs).
  SEARCH_WINDOW_US = Int64(15) * 1000000;
  // Folga do ALeadMs: latencia do encoder + atraso do timer da UI.
  LEAD_TOLERANCE_US = Int64(300) * 1000;
  // Teto do que se le da gravacao antes de decidir (cada faixa precisa de
  // VERIFY_PKTS; uma faixa sem pacote nenhum nao pode segurar o resto).
  MAX_PREREAD = 20000;
  // Audio: candidatos a mais de 2 s da estimativa nem entram.
  AUDIO_SEARCH_US = Int64(2) * 1000000;
var
  HCtx, TCtx: AVFormatContext;
  OutF: TOutputStream;
  Pkt, Cl: PAVPacket;
  N, i: Cardinal;
  S, k, j, Best, VIdx, PreRead, Dropped: Integer;
  AllIdx: TArray<Cardinal>;
  HSigs, TStart: TArray<TList<TPktSig>>;
  HLastUs, OffUs, LastDts: TArray<Int64>;
  Skip, Skipped: TArray<Integer>;
  Pending: TList<PAVPacket>;
  Cands: TList<Integer>;
  ACands: TArray<TList<Integer>>;
  Sig: TPktSig;
  Ready, WriteFailed, Eof, HasRef: Boolean;
  VAnchorUs, TV0Us, EstUs, D, BestD, ThrUs, HEndUs, RefOffUs: Int64;
  St: PAVStream;

  function SigOf(ACtx: AVFormatContext; P: PAVPacket): TPktSig;
  var
    Src: PAVStream;
    Ts: Int64;
  begin
    Src := GetStreamByIndex(ACtx, Cardinal(P.stream_index));
    Ts := P.pts;
    if Ts = AV_NOPTS_VALUE then Ts := P.dts;
    Result.TsUs := TimestampToUs(Ts, Src.time_base);
    if Result.TsUs = AV_NOPTS_VALUE then Result.TsUs := 0;
    Result.Size := P.size;
    Result.Fp := PacketFingerprint(P);
    Result.Key := (P.flags and AV_PKT_FLAG_KEY) <> 0;
  end;

  // Escreve P (lido de ACtx) na saida, deslocado de AOffUs.
  function WritePkt(ACtx: AVFormatContext; P: PAVPacket; AOffUs: Int64): Boolean;
  var
    SrcSt, DstSt: PAVStream;
    DstIdx: Integer;
    Us: Int64;
  begin
    Result := True;
    DstIdx := OutF.StreamMap[P.stream_index];
    if DstIdx < 0 then Exit;
    SrcSt := GetStreamByIndex(ACtx, Cardinal(P.stream_index));
    DstSt := GetStreamByIndex(OutF.Ctx, Cardinal(DstIdx));
    if (SrcSt = nil) or (DstSt = nil) then Exit;
    if P.pts <> AV_NOPTS_VALUE then
    begin
      Us := TimestampToUs(P.pts, SrcSt.time_base);
      P.pts := UsToTimestamp(Us + AOffUs, DstSt.time_base);
    end;
    if P.dts <> AV_NOPTS_VALUE then
    begin
      Us := TimestampToUs(P.dts, SrcSt.time_base);
      P.dts := UsToTimestamp(Us + AOffUs, DstSt.time_base);
      // O ms do MKV arredonda diferente nos dois arquivos: o 1o pacote da
      // gravacao pode cair 1 tique ANTES do ultimo do buffer. Segura no
      // ultimo escrito (o Matroska aceita dts igual, AVFMT_TS_NONSTRICT).
      if (LastDts[DstIdx] <> AV_NOPTS_VALUE) and (P.dts < LastDts[DstIdx]) then
        P.dts := LastDts[DstIdx];
      LastDts[DstIdx] := P.dts;
      if (P.pts <> AV_NOPTS_VALUE) and (P.pts < P.dts) then P.pts := P.dts;
    end;
    if P.duration > 0 then
      P.duration := av_rescale_q(P.duration, SrcSt.time_base, DstSt.time_base);
    P.stream_index := DstIdx;
    P.pos := -1;
    try
      Result := av_interleaved_write_frame(OutF.Ctx, P) >= 0;
    except
      Result := False;
    end;
    if not Result then
      Log('Splice: av_interleaved_write_frame falhou.');
  end;

  // Pacote da gravacao: pula o que o buffer ja tinha, escreve o resto.
  function WriteTail(P: PAVPacket): Boolean;
  var
    Si: Integer;
    Ts: Int64;
  begin
    Result := True;
    Si := P.stream_index;
    if (Si < 0) or (Si >= Integer(N)) then Exit;
    if Skip[Si] < 0 then
    begin
      // Faixa sem pacote casado: corta pelo tempo (ver passo 3b).
      Ts := SigOf(TCtx, P).TsUs + OffUs[Si];
      if Ts <= HLastUs[Si] then
      begin
        Inc(Dropped);
        Exit;
      end;
    end
    else if Skipped[Si] < Skip[Si] then
    begin
      Inc(Skipped[Si]);
      Inc(Dropped);
      Exit;
    end;
    Result := WritePkt(TCtx, P, OffUs[Si]);
  end;

begin
  Result := False;
  AInfo := '';
  if not FFmpegLibAvailable then
  begin
    AInfo := 'libav indisponivel';
    Exit;
  end;

  HCtx := nil;
  TCtx := nil;
  Pkt := nil;
  Pending := TList<PAVPacket>.Create;
  Cands := TList<Integer>.Create;
  FillChar(OutF, SizeOf(OutF), 0);
  N := 0;
  Dropped := 0;
  try
    if not OpenInputWithRetry(HCtx, AHead) or
       (avformat_find_stream_info(HCtx, nil) < 0) then
    begin
      AInfo := 'nao abriu o trecho do buffer';
      Exit;
    end;
    if not OpenInputWithRetry(TCtx, ATail) or
       (avformat_find_stream_info(TCtx, nil) < 0) then
    begin
      AInfo := 'nao abriu a gravacao';
      Exit;
    end;
    if not InputsCompatible(HCtx, TCtx) then
    begin
      AInfo := 'faixas diferentes nos dois arquivos';
      Exit;
    end;

    // InputsCompatible ja recusou N = 0 (pegadinha #24 no laco abaixo).
    N := av_format_context_nb_streams(HCtx);
    VIdx := -1;
    for i := 0 to N - 1 do
    begin
      St := GetStreamByIndex(HCtx, i);
      if (St <> nil) and (St.codecpar <> nil) and
         (St.codecpar.codec_type = AVMEDIA_TYPE_VIDEO) then
      begin
        VIdx := Integer(i);
        Break;
      end;
    end;
    if VIdx < 0 then
    begin
      AInfo := 'sem faixa de video';
      Exit;
    end;

    SetLength(HSigs, N);
    SetLength(TStart, N);
    SetLength(HLastUs, N);
    SetLength(OffUs, N);
    SetLength(LastDts, N);
    SetLength(Skip, N);
    SetLength(Skipped, N);
    SetLength(AllIdx, N);
    SetLength(ACands, N);
    for i := 0 to N - 1 do
    begin
      HSigs[i] := TList<TPktSig>.Create;
      TStart[i] := TList<TPktSig>.Create;
      ACands[i] := TList<Integer>.Create;
      HLastUs[i] := Low(Int64);
      LastDts[i] := AV_NOPTS_VALUE;
      AllIdx[i] := i;
    end;

    if not OpenOutputForStreams(HCtx, ADst, AllIdx, OutF, 'matroska') then
    begin
      AInfo := 'nao abriu o arquivo de saida';
      Exit;
    end;
    Pkt := av_packet_alloc;
    if Pkt = nil then Exit;

    // 1. Trecho do buffer, inteiro e sem mexer no relogio dele.
    WriteFailed := False;
    while (not WriteFailed) and (av_read_frame(HCtx, Pkt) = 0) do
    begin
      try
        S := Pkt.stream_index;
        if (S >= 0) and (S < Integer(N)) then
        begin
          Sig := SigOf(HCtx, Pkt);
          HSigs[S].Add(Sig);
          if Sig.TsUs > HLastUs[S] then HLastUs[S] := Sig.TsUs;
          if not WritePkt(HCtx, Pkt, 0) then WriteFailed := True;
        end;
      finally
        av_packet_unref(Pkt);
      end;
    end;
    if WriteFailed then
    begin
      AInfo := 'falha de escrita';
      Exit;
    end;
    if HSigs[VIdx].Count = 0 then
    begin
      AInfo := 'trecho do buffer sem video';
      Exit;
    end;

    // 2. Comeco da gravacao, guardado em memoria ate decidir a emenda.
    PreRead := 0;
    Eof := False;
    repeat
      Ready := True;
      for i := 0 to N - 1 do
        if TStart[i].Count < VERIFY_PKTS then Ready := False;
      if Ready or (PreRead >= MAX_PREREAD) then Break;
      if av_read_frame(TCtx, Pkt) <> 0 then
      begin
        Eof := True;
        Break;
      end;
      Inc(PreRead);
      S := Pkt.stream_index;
      if (S >= 0) and (S < Integer(N)) then
      begin
        if TStart[S].Count < VERIFY_PKTS then TStart[S].Add(SigOf(TCtx, Pkt));
        Cl := av_packet_clone(Pkt);
        if Cl = nil then
        begin
          av_packet_unref(Pkt);
          AInfo := 'sem memoria';
          Exit;
        end;
        Pending.Add(Cl);
      end;
      av_packet_unref(Pkt);
    until False;
    if TStart[VIdx].Count = 0 then
    begin
      AInfo := 'gravacao sem video';
      Exit;
    end;

    // 3a. Video: o 1o pacote da gravacao (keyframe) dentro do buffer.
    HEndUs := HLastUs[VIdx];
    for j := HSigs[VIdx].Count - 1 downto 0 do
    begin
      if HSigs[VIdx][j].TsUs < HEndUs - SEARCH_WINDOW_US then Break;
      if SigEqual(HSigs[VIdx][j], TStart[VIdx][0]) and
         RunMatches(HSigs[VIdx], j, TStart[VIdx]) then
        Cands.Add(j);
    end;
    if Cands.Count = 0 then
    begin
      AInfo := 'ponto de encontro do video nao encontrado';
      Exit;
    end;
    // Mais de um candidato so com tela PARADA (keyframes identicos). O certo
    // e o 1o keyframe DEPOIS do instante em que a gravacao comecou — que e o
    // fim do buffer menos ALeadMs. Cands vem do fim pro comeco, entao a
    // ultima atribuicao e o candidato mais cedo que ainda passa do limite.
    Best := -1;
    if ALeadMs > 0 then
    begin
      ThrUs := HEndUs - Int64(ALeadMs) * 1000 - LEAD_TOLERANCE_US;
      for k := 0 to Cands.Count - 1 do
        if HSigs[VIdx][Cands[k]].TsUs >= ThrUs then Best := Cands[k];
    end;
    if Best < 0 then Best := Cands[0];
    Skip[VIdx] := HSigs[VIdx].Count - Best;
    VAnchorUs := HSigs[VIdx][Best].TsUs;
    TV0Us := TStart[VIdx][0].TsUs;
    OffUs[VIdx] := VAnchorUs - TV0Us;
    Log('Splice: video casou em %d candidato(s); %d pacote(s) em comum, ' +
      'emenda em %.3f s do buffer.',
      [Cands.Count, Skip[VIdx], VAnchorUs / 1000000]);

    // 3b. Demais faixas. Primeiro os candidatos de cada uma, perto do
    // instante que o video indica.
    for S := 0 to Integer(N) - 1 do
    begin
      if (S = VIdx) or (TStart[S].Count = 0) then Continue;
      EstUs := VAnchorUs + (TStart[S][0].TsUs - TV0Us);
      for j := HSigs[S].Count - 1 downto 0 do
      begin
        if HSigs[S][j].TsUs < EstUs - AUDIO_SEARCH_US then Break;
        if SigEqual(HSigs[S][j], TStart[S][0]) and
           RunMatches(HSigs[S], j, TStart[S]) then
          ACands[S].Add(j);
      end;
    end;
    // Faixa que casou num lugar SO tem o deslocamento exato — e ele vale pras
    // outras: as faixas de audio saem do mesmo mixer, pacote a pacote nos
    // mesmos instantes, e os dois arquivos zeram todas no mesmo ponto. E o
    // que resolve a faixa muda, onde todo pacote de silencio e igual e a
    // estimativa pelo video erraria por um quadro de AAC (medido: um pacote
    // sobrando ou faltando em 5 de 8 emendas simuladas).
    HasRef := False;
    RefOffUs := 0;
    for S := 0 to Integer(N) - 1 do
      if (S <> VIdx) and (ACands[S].Count = 1) then
      begin
        RefOffUs := HSigs[S][ACands[S][0]].TsUs - TStart[S][0].TsUs;
        HasRef := True;
        Break;
      end;
    for S := 0 to Integer(N) - 1 do
    begin
      if (S = VIdx) or (TStart[S].Count = 0) then Continue;
      if HasRef then EstUs := TStart[S][0].TsUs + RefOffUs
      else EstUs := VAnchorUs + (TStart[S][0].TsUs - TV0Us);
      Best := -1;
      BestD := High(Int64);
      for k := 0 to ACands[S].Count - 1 do
      begin
        D := Abs(HSigs[S][ACands[S][k]].TsUs - EstUs);
        if D < BestD then
        begin
          BestD := D;
          Best := ACands[S][k];
        end;
      end;
      if Best >= 0 then
      begin
        Skip[S] := HSigs[S].Count - Best;
        OffUs[S] := HSigs[S][Best].TsUs - TStart[S][0].TsUs;
      end
      else
      begin
        // Nada casou (so se o buffer acabou antes do audio da gravacao
        // comecar). Vai pela estimativa e corta pelo tempo.
        Skip[S] := -1;
        OffUs[S] := EstUs - TStart[S][0].TsUs;
        Log('Splice: faixa %d sem pacote casado — emenda pelo tempo.', [S]);
      end;
    end;

    // 4. Resto da gravacao, sem o que o buffer ja tinha.
    for k := 0 to Pending.Count - 1 do
    begin
      Cl := Pending[k];
      if (not WriteFailed) and not WriteTail(Cl) then WriteFailed := True;
      av_packet_free(@Cl);
      Pending[k] := nil;
    end;
    Pending.Clear;
    while (not WriteFailed) and (not Eof) and (av_read_frame(TCtx, Pkt) = 0) do
    begin
      try
        if not WriteTail(Pkt) then WriteFailed := True;
      finally
        av_packet_unref(Pkt);
      end;
    end;
    if WriteFailed then
    begin
      AInfo := 'falha de escrita';
      Exit;
    end;

    AInfo := Format('%d pacote(s) repetido(s) descartado(s)', [Dropped]);
    Result := True;
  finally
    for k := 0 to Pending.Count - 1 do
    begin
      Cl := Pending[k];
      if Cl <> nil then av_packet_free(@Cl);
    end;
    Pending.Free;
    Cands.Free;
    for k := 0 to High(HSigs) do
    begin
      HSigs[k].Free;
      TStart[k].Free;
      ACands[k].Free;
    end;
    if Pkt <> nil then av_packet_free(@Pkt);
    CloseOutput(OutF);
    if TCtx <> nil then avformat_close_input(@TCtx);
    if HCtx <> nil then avformat_close_input(@HCtx);
  end;
end;

function ExtractAudioTracks(const ASrc: string;
  const AOutputs: TArray<string>; AAudioStartIndex: Integer): Boolean;
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
    if N = 0 then Exit; // pegadinha #24: N Cardinal, `0 to N-1` underflow.
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
  // AAudioStartIndex pula os primeiros N streams de audio (ex.: 1 = ignora
  // o mix). AOutputs[j] mapeia pro (j+offset)-esimo stream de audio.
  if (AAudioStartIndex < 0) or
     (Length(AOutputs) + AAudioStartIndex > Length(AudioIdxs)) then Exit;

  // Cada output recebe so um audio stream.
  SetLength(Targets, Length(AOutputs));
  for j := 0 to High(AOutputs) do
  begin
    SetLength(T, 1);
    T[0] := AudioIdxs[j + AAudioStartIndex];
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
  Opts: AVDictionary;
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
  // Limita o trabalho do find_stream_info: pra thumb so precisamos de
  // codecpar (w/h/codec_id) + time_base, que ja vem do header do MKV.
  // Sem isso, em canvas multi-monitor (fonte ~4K de largura) com AV1/HEVC
  // o find_stream_info decodifica varios frames em SOFTWARE -> 8s+ de
  // espera so pra gerar a thumb. analyzeduration em microsegundos.
  Opts := nil;
  av_dict_set(@Opts, 'analyzeduration', '500000', 0);  // 0.5s de stream
  av_dict_set(@Opts, 'probesize', '2000000', 0);       // 2 MB
  Rc := avformat_open_input(@SrcCtx, PAnsiChar(SrcPath), nil, @Opts);
  av_dict_free(@Opts);
  if Rc < 0 then
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

    StartTs := SeekTs;

    Pkt := av_packet_alloc;
    Frame := av_frame_alloc;
    if (Pkt = nil) or (Frame = nil) then Exit;

    // Aceita o PRIMEIRO frame decodado apos o seek (o keyframe em que o
    // seek BACKWARD nos posicionou) — NAO decodamos ate StartTs.
    // Decodar dezenas de frames ate o timestamp exato e proibitivo em
    // canvas multi-monitor (fonte ~4-5K) com AV1/HEVC por SOFTWARE: cada
    // frame leva centenas de ms (eram ~8s pra thumb). Pra thumbnail o
    // frame exato nao importa — o keyframe proximo serve. 1 frame.
    Got := False;
    while not Got do
    begin
      Rc := av_read_frame(SrcCtx, Pkt);
      if Rc < 0 then Break;
      if Pkt.stream_index = VIdx then
      begin
        if avcodec_send_packet(DecCtx, Pkt) = 0 then
        begin
          if avcodec_receive_frame(DecCtx, Frame) = 0 then
          begin
            Got := True;
            av_packet_unref(Pkt);
            Break;
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
  s, ch, ChCount: Integer;
  Channel0Ptr: PByte;
  V, SampleV: Single;
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
            // Le o pico entre TODOS os canais por sample — sem isso, audio
            // so no canal direito (ou com canal 0 mudo) gerava waveform
            // achatada. Planar usa um ponteiro por canal em data[ch],
            // limitado a 8 (AV_NUM_DATA_POINTERS); interleaved entrelaca
            // tudo em data[0] com stride NumChannels.
            ChCount := NumChannels;
            if (Fmt = AV_SAMPLE_FMT_FLTP) or (Fmt = AV_SAMPLE_FMT_S16P) then
              if ChCount > 8 then ChCount := 8;
            if Channel0Ptr <> nil then
            begin
              for s := 0 to Frame.nb_samples - 1 do
              begin
                V := 0;
                for ch := 0 to ChCount - 1 do
                begin
                  SampleV := 0;
                  case Fmt of
                    AV_SAMPLE_FMT_FLTP:
                      if Frame.data[ch] <> nil then
                        SampleV := PSingle(Frame.data[ch] +
                          s * SizeOf(Single))^;
                    AV_SAMPLE_FMT_FLT:
                      SampleV := PSingle(Channel0Ptr +
                        (s * NumChannels + ch) * SizeOf(Single))^;
                    AV_SAMPLE_FMT_S16P:
                      if Frame.data[ch] <> nil then
                        SampleV := PSmallInt(Frame.data[ch] +
                          s * SizeOf(SmallInt))^ / 32768.0;
                    AV_SAMPLE_FMT_S16:
                      SampleV := PSmallInt(Channel0Ptr +
                        (s * NumChannels + ch) * SizeOf(SmallInt))^ / 32768.0;
                  end;
                  if SampleV < 0 then SampleV := -SampleV;
                  if SampleV > V then V := SampleV;
                end;

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

// =====================================================================
// DecodeAudioMono — PCM mono em blocos, pra transcricao local
// =====================================================================

function DecodeAudioMono(const ASrc: string; AOnBlock: TAudioBlockFunc): Boolean;
const
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
  AudioIdx, i, NumChannels, SampleRate: Integer;
  NbStreams: Cardinal;
  Mono: TArray<Single>;
  Aborted: Boolean;

  // Mistura um frame decodado em Mono[] e entrega. Marca Aborted pra parar.
  procedure Deliver;
  var
    // Locais DESTA rotina: o for do Delphi nao aceita variavel de controle
    // da rotina de fora (E1019).
    s, ch, ChCount, Fmt: Integer;
    Acc: Single;
  begin
    Fmt := Frame.format;
    if (Frame.nb_samples <= 0) or (Frame.data[0] = nil) then Exit;
    if not ((Fmt = AV_SAMPLE_FMT_FLTP) or (Fmt = AV_SAMPLE_FMT_FLT) or
            (Fmt = AV_SAMPLE_FMT_S16P) or (Fmt = AV_SAMPLE_FMT_S16)) then
    begin
      Log('DecodeAudioMono: formato de amostra %d nao suportado.', [Fmt]);
      Aborted := True;
      Exit;
    end;
    ChCount := NumChannels;
    // Planar tem um ponteiro por canal em data[], limitado a 8
    // (AV_NUM_DATA_POINTERS) — mesma regra do ComputeAudioPeaks.
    if ((Fmt = AV_SAMPLE_FMT_FLTP) or (Fmt = AV_SAMPLE_FMT_S16P)) and (ChCount > 8) then
      ChCount := 8;
    if Length(Mono) < Frame.nb_samples then SetLength(Mono, Frame.nb_samples);
    for s := 0 to Frame.nb_samples - 1 do
    begin
      Acc := 0;
      for ch := 0 to ChCount - 1 do
        case Fmt of
          AV_SAMPLE_FMT_FLTP:
            if Frame.data[ch] <> nil then
              Acc := Acc + PSingle(Frame.data[ch] + s * SizeOf(Single))^;
          AV_SAMPLE_FMT_FLT:
            Acc := Acc + PSingle(Frame.data[0] +
              (s * NumChannels + ch) * SizeOf(Single))^;
          AV_SAMPLE_FMT_S16P:
            if Frame.data[ch] <> nil then
              Acc := Acc + PSmallInt(Frame.data[ch] + s * SizeOf(SmallInt))^ / 32768.0;
          AV_SAMPLE_FMT_S16:
            Acc := Acc + PSmallInt(Frame.data[0] +
              (s * NumChannels + ch) * SizeOf(SmallInt))^ / 32768.0;
        end;
      // Media, nao soma: dois canais iguais nao podem dobrar o volume.
      Mono[s] := Acc / ChCount;
    end;
    if not AOnBlock(@Mono[0], Frame.nb_samples, SampleRate) then Aborted := True;
  end;

begin
  Result := False;
  if not Assigned(AOnBlock) then Exit;
  if not FFmpegLibAvailable then Exit;
  SrcCtx := nil;
  DecCtx := nil;
  Pkt := nil;
  Frame := nil;
  Aborted := False;
  try
    if avformat_open_input(@SrcCtx, PAnsiChar(ToUtf8(ASrc)), nil, nil) < 0 then Exit;
    if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;

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
    if SampleRate <= 0 then Exit;
    NumChannels := CodecPar.ch_layout.nb_channels;
    if NumChannels < 1 then NumChannels := 1;

    Pkt := av_packet_alloc;
    Frame := av_frame_alloc;
    if (Pkt = nil) or (Frame = nil) then Exit;

    while (not Aborted) and (av_read_frame(SrcCtx, Pkt) = 0) do
    begin
      if Pkt.stream_index = AudioIdx then
        if avcodec_send_packet(DecCtx, Pkt) = 0 then
          while avcodec_receive_frame(DecCtx, Frame) = 0 do
          begin
            Deliver;
            av_frame_unref(Frame);
            if Aborted then Break;
          end;
      av_packet_unref(Pkt);
    end;
    if Aborted then Exit;

    // Flush: o AAC segura o ultimo frame ate receber o pacote nulo.
    avcodec_send_packet(DecCtx, nil);
    while avcodec_receive_frame(DecCtx, Frame) = 0 do
    begin
      Deliver;
      av_frame_unref(Frame);
      if Aborted then Exit;
    end;
    Result := True;
  finally
    if Frame  <> nil then av_frame_free(@Frame);
    if Pkt    <> nil then av_packet_free(@Pkt);
    if DecCtx <> nil then avcodec_free_context(@DecCtx);
    if SrcCtx <> nil then avformat_close_input(@SrcCtx);
  end;
end;

// =====================================================================
// ListVideoKeyframes — grade de keyframes pro avanco por saltos do player
// =====================================================================

function ListVideoKeyframes(const ASrc: string;
  out ATimes: TArray<Double>; out AFps: Double): Boolean;
// Sem avformat_find_stream_info de proposito: codecpar/time_base/
// avg_frame_rate ja vem do cabecalho do MKV, e o find_stream_info
// decodificaria quadros so pra confirmar o que o indice ja diz.
var
  Ctx: AVFormatContext;
  St, VSt: PAVStream;
  E: PAVIndexEntry;
  i, N, Count: Integer;
  NbStreams: Cardinal;
  Tb: Double;
begin
  Result := False;
  SetLength(ATimes, 0);
  AFps := 0;
  if not FFmpegLibAvailable then Exit;
  Ctx := nil;
  try
    if avformat_open_input(@Ctx, PAnsiChar(ToUtf8(ASrc)), nil, nil) < 0 then Exit;
    VSt := nil;
    NbStreams := av_format_context_nb_streams(Ctx);
    for i := 0 to Integer(NbStreams) - 1 do
    begin
      St := GetStreamByIndex(Ctx, Cardinal(i));
      if (St <> nil) and (St.codecpar <> nil) and
         (St.codecpar.codec_type = AVMEDIA_TYPE_VIDEO) then
      begin
        VSt := St;
        Break;
      end;
    end;
    if (VSt = nil) or (VSt.time_base.den = 0) then Exit;

    // O MKV adia a leitura dos Cues ate o primeiro seek — sem isto o
    // indice volta vazio mesmo num arquivo integro.
    av_seek_frame(Ctx, -1, 0, AVSEEK_FLAG_BACKWARD);

    N := avformat_index_get_entries_count(VSt);
    if N <= 0 then Exit;
    Tb := VSt.time_base.num / VSt.time_base.den;
    SetLength(ATimes, N);
    Count := 0;
    for i := 0 to N - 1 do
    begin
      E := avformat_index_get_entry(VSt, i);
      if (E = nil) or ((E.flags_size and AVINDEX_KEYFRAME) = 0) then Continue;
      ATimes[Count] := E.timestamp * Tb;
      Inc(Count);
    end;
    SetLength(ATimes, Count);
    if VSt.avg_frame_rate.den > 0 then
      AFps := VSt.avg_frame_rate.num / VSt.avg_frame_rate.den;
    Result := Count > 0;
  finally
    if Ctx <> nil then avformat_close_input(@Ctx);
  end;
end;

end.

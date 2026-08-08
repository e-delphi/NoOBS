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

type
  // Resultado da divisao. soNoCutPoint = nao ha keyframe interno pra cortar
  // nesta posicao (comum em pedacos curtos com keyframes espacados) — a UI
  // mostra uma dica em vez de "falha" generica.
  TSplitOutcome = (soOk, soNoCutPoint, soError);

// Divide o arquivo em DOIS (stream copy, sem reencode) no keyframe mais
// proximo de APosSec: ADstA = [inicio, corte), ADstB = [corte, fim) com
// timestamps rebaseados pra comecar em ~0. Como stream copy so corta em
// I-frame, o ponto real "snap" pro keyframe (pode desviar do APosSec
// conforme o keyint do encoder). Worker thread.
function SplitFileAtKeyframe(const ASrc, ADstA, ADstB: string;
  APosSec: Double): TSplitOutcome;

// Une N arquivos em um novo output (stream copy, sem reencode), na ordem
// recebida. Os inputs precisam ter a mesma estrutura de streams/codec; quando
// isso nao for verdade, retorna False com AIncompatible=True.
function MergeFiles(const AInputs: TArray<string>; const ADst: string;
  out AIncompatible: Boolean): Boolean;

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

implementation

uses
  Winapi.Windows,
  OBSLog,
  FFmpegLib;

const
  // AV_NOPTS_VALUE vive na implementation de FFmpegLib (nao exportado na
  // interface), entao redeclaramos aqui pro roteamento de pacotes do split.
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
// So chamado de worker thread (merge/split), entao o Sleep e aceitavel.
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

function FindCutKeyframe(const ASrcCtx: AVFormatContext;
  AVideoIdx: Integer; APosSec: Double;
  out ACutPtsSec, ACutDtsSec: Double): Boolean;
// Passada de descoberta: localiza o keyframe de video do corte e devolve o
// tempo dele (segundos) nas DUAS escalas — apresentacao (pts) e decodificacao
// (dts). Preferencia: o PRIMEIRO keyframe com pts_time >= APosSec (a 2a parte
// comeca num keyframe limpo). Se nao houver nenhum depois da posicao (corte
// perto do fim), usa o ULTIMO keyframe antes dela. False = sem keyframe valido.
//
// Os dois tempos sao necessarios porque com B-frames o keyframe tem
// dts = pts - delay. Quem produz B-frames: x264 (h264-sw, default i_bframe=3),
// NVENC (bf=2) e AMF quando a GPU suporta (bf=2 em AVC/AV1; HEVC nasce com 0).
// O corte no stream de video acontece em ordem de DECODIFICACAO (dts) e nos
// demais streams em ordem de APRESENTACAO (pts) — ver SplitFileAtKeyframe.
const
  AV_PKT_FLAG_KEY = 1;
var
  Pkt: PAVPacket;
  S: PAVStream;
  Tb: AVRational;
  PtsTime, DtsTime, LastPts, LastDts: Double;
begin
  Result := False;
  ACutPtsSec := -1;
  ACutDtsSec := -1;
  LastPts := -1;
  LastDts := -1;
  S := GetStreamByIndex(ASrcCtx, Cardinal(AVideoIdx));
  if (S = nil) or (S.time_base.den <= 0) then Exit;
  Tb := S.time_base;

  Pkt := av_packet_alloc;
  if Pkt = nil then Exit;
  try
    while av_read_frame(ASrcCtx, Pkt) = 0 do
    begin
      try
        if (Pkt.stream_index = AVideoIdx) and
           ((Pkt.flags and AV_PKT_FLAG_KEY) <> 0) and
           (Pkt.pts <> AV_NOPTS_VALUE) then
        begin
          PtsTime := Pkt.pts * (Tb.num / Tb.den);
          if Pkt.dts <> AV_NOPTS_VALUE then
            DtsTime := Pkt.dts * (Tb.num / Tb.den)
          else
            DtsTime := PtsTime;   // sem B-frames o demuxer entrega dts = pts
          LastPts := PtsTime;
          LastDts := DtsTime;
          if PtsTime >= APosSec then
          begin
            // 1o keyframe >= posicao (pts vem em ordem crescente aqui).
            ACutPtsSec := PtsTime;
            ACutDtsSec := DtsTime;
            Exit(True);
          end;
        end;
      finally
        av_packet_unref(Pkt);
      end;
    end;
  finally
    av_packet_free(@Pkt);
  end;
  // Nenhum keyframe depois da posicao — usa o ultimo antes dela.
  if LastPts >= 0 then
  begin
    ACutPtsSec := LastPts;
    ACutDtsSec := LastDts;
    Result := True;
  end;
end;

function SplitFileAtKeyframe(const ASrc, ADstA, ADstB: string;
  APosSec: Double): TSplitOutcome;
const
  AV_PKT_FLAG_KEY = 1;
var
  SrcCtx: AVFormatContext;
  OutA, OutB: TOutputStream;
  Pkt: PAVPacket;
  NbStreams: Cardinal;
  AllIdx: TArray<Cardinal>;
  CutInTb: TArray<Int64>;
  i, VideoIdx, Sidx, DstIdx: Integer;
  S, SrcStream, DstStream: PAVStream;
  CutPtsSec, CutDtsSec, PktSec: Double;
  TbS, TbUs: AVRational;
  RefUs: Int64;
  WriteFailed, ToB: Boolean;
  WrRc: Integer;
begin
  Result := soError;
  if not FFmpegLibAvailable then Exit;
  if (ASrc = '') or (ADstA = '') or (ADstB = '') then Exit;

  FillChar(OutA, SizeOf(OutA), 0);
  FillChar(OutB, SizeOf(OutB), 0);
  SrcCtx := nil;
  Pkt := nil;

  if not OpenInputWithRetry(SrcCtx, ASrc) then Exit;
  try
    if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;
    NbStreams := av_format_context_nb_streams(SrcCtx);
    if NbStreams = 0 then Exit;

    // 1o stream de video (referencia de keyframe).
    VideoIdx := -1;
    for i := 0 to Integer(NbStreams) - 1 do
    begin
      S := GetStreamByIndex(SrcCtx, Cardinal(i));
      if (S <> nil) and (S.codecpar <> nil) and
         (S.codecpar.codec_type = AVMEDIA_TYPE_VIDEO) then
      begin
        VideoIdx := i;
        Break;
      end;
    end;

    // Tempo do corte (keyframe), nas duas escalas. Sem video (audio-only):
    // tempo bruto pedido nas duas.
    if VideoIdx >= 0 then
    begin
      if not FindCutKeyframe(SrcCtx, VideoIdx, APosSec,
                             CutPtsSec, CutDtsSec) then
      begin
        CutPtsSec := -1;
        CutDtsSec := -1;
      end;
    end
    else
    begin
      CutPtsSec := APosSec;
      CutDtsSec := APosSec;
    end;
    // <= 0 = unico keyframe e no inicio (ou nao ha keyframe interno): o pedaco
    // nao tem onde ser cortado por stream copy. O dts entra na checagem porque
    // com B-frames o dts do 1o keyframe do arquivo pode ser <= 0 — cortar ali
    // deixaria a 1a parte vazia. Caso "nada pra cortar" — reportado distinto
    // do erro pra a UI dar dica util.
    if (CutPtsSec <= 0) or (CutDtsSec <= 0) then
    begin
      Log('Split: sem ponto de corte (pts=%.3f dts=%.3f pos=%.3f) — keyframes ' +
        'espacados demais pra cortar aqui.', [CutPtsSec, CutDtsSec, APosSec]);
      Exit(soNoCutPoint);
    end;

    // Volta o cursor pro inicio pra a passada de copia.
    av_seek_frame(SrcCtx, -1, 0, AVSEEK_FLAG_BACKWARD);

    // Dois outputs, todos os streams (copia).
    SetLength(AllIdx, NbStreams);
    for i := 0 to Integer(NbStreams) - 1 do AllIdx[i] := Cardinal(i);
    if not OpenOutputForStreams(SrcCtx, ADstA, AllIdx, OutA) then Exit;
    if not OpenOutputForStreams(SrcCtx, ADstB, AllIdx, OutB) then Exit;

    // Offset de rebase por stream (= CutDtsSec no time_base de cada um).
    // Subtrair o MESMO offset temporal de todos preserva o sync A/V na 2a
    // parte. Usa o dts (nao o pts) do keyframe pra que o 1o pacote de video
    // caia em dts 0: com B-frames o pts do keyframe fica `delay` a frente do
    // dts, entao rebasear pelo pts deixaria o dts NEGATIVO logo no 1o bloco.
    TbUs.num := 1;
    TbUs.den := AV_TIME_BASE;
    RefUs := Round(CutDtsSec * AV_TIME_BASE);
    SetLength(CutInTb, NbStreams);
    for i := 0 to Integer(NbStreams) - 1 do
    begin
      S := GetStreamByIndex(SrcCtx, Cardinal(i));
      if (S <> nil) and (S.time_base.den > 0) then
        CutInTb[i] := av_rescale_q(RefUs, TbUs, S.time_base)
      else
        CutInTb[i] := 0;
    end;

    Pkt := av_packet_alloc;
    if Pkt = nil then Exit;
    WriteFailed := False;
    while (not WriteFailed) and (av_read_frame(SrcCtx, Pkt) = 0) do
    begin
      try
        Sidx := Pkt.stream_index;
        if (Sidx < 0) or (Cardinal(Sidx) >= NbStreams) then Continue;
        SrcStream := GetStreamByIndex(SrcCtx, Cardinal(Sidx));
        if SrcStream = nil then Continue;
        DstIdx := OutA.StreamMap[Sidx];   // mesmo mapa em A e B
        if DstIdx < 0 then Continue;
        TbS := SrcStream.time_base;

        // Stream de VIDEO: corta em ordem de DECODIFICACAO — dts do pacote
        // contra o dts do keyframe. Assim o proprio keyframe e tudo que vem
        // depois dele no bitstream caem na 2a parte, e o GOP anterior fica na
        // 1a. Comparar o dts do pacote contra o PTS do keyframe (como era
        // antes) so funcionava sem B-frames: com eles o keyframe tem
        // dts = pts - delay, caia do lado errado e a 2a parte nascia SEM
        // I-frame — preta ate o keyframe seguinte, ou preta inteira quando o
        // corte era no ultimo GOP (e, por tabela, a uniao dessa parte falhava).
        //
        // Demais streams (audio): ordem de APRESENTACAO — pts do pacote contra
        // o pts do keyframe, pra o audio casar com o 1o quadro exibido.
        if Sidx = VideoIdx then
        begin
          if (Pkt.dts <> AV_NOPTS_VALUE) and (TbS.den > 0) then
            PktSec := Pkt.dts * (TbS.num / TbS.den)
          else if (Pkt.pts <> AV_NOPTS_VALUE) and (TbS.den > 0) then
            PktSec := Pkt.pts * (TbS.num / TbS.den)
          else
            PktSec := 0;
          ToB := PktSec >= CutDtsSec;
        end
        else
        begin
          if (Pkt.pts <> AV_NOPTS_VALUE) and (TbS.den > 0) then
            PktSec := Pkt.pts * (TbS.num / TbS.den)
          else if (Pkt.dts <> AV_NOPTS_VALUE) and (TbS.den > 0) then
            PktSec := Pkt.dts * (TbS.num / TbS.den)
          else
            PktSec := 0;
          ToB := PktSec >= CutPtsSec;
        end;
        if ToB then
        begin
          DstStream := GetStreamByIndex(OutB.Ctx, Cardinal(DstIdx));
          if DstStream = nil then Continue;
          if Pkt.pts <> AV_NOPTS_VALUE then Pkt.pts := Pkt.pts - CutInTb[Sidx];
          if Pkt.dts <> AV_NOPTS_VALUE then Pkt.dts := Pkt.dts - CutInTb[Sidx];
          Pkt.stream_index := DstIdx;
          av_packet_rescale_ts(Pkt, TbS, DstStream.time_base);
          Pkt.pos := -1;
          try WrRc := av_interleaved_write_frame(OutB.Ctx, Pkt); except WrRc := -1; end;
          if WrRc < 0 then WriteFailed := True;
        end
        else
        begin
          DstStream := GetStreamByIndex(OutA.Ctx, Cardinal(DstIdx));
          if DstStream = nil then Continue;
          Pkt.stream_index := DstIdx;
          av_packet_rescale_ts(Pkt, TbS, DstStream.time_base);
          Pkt.pos := -1;
          try WrRc := av_interleaved_write_frame(OutA.Ctx, Pkt); except WrRc := -1; end;
          if WrRc < 0 then WriteFailed := True;
        end;
      finally
        av_packet_unref(Pkt);
      end;
    end;
    if not WriteFailed then Result := soOk;
    Log('Split: cut pts=%.3fs dts=%.3fs result=%s',
      [CutPtsSec, CutDtsSec, BoolToStr(Result = soOk, True)]);
  finally
    if Pkt <> nil then av_packet_free(@Pkt);
    CloseOutput(OutA);
    CloseOutput(OutB);
    avformat_close_input(@SrcCtx);
  end;
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

end.

(*
  FFmpegExport - exportacao de gravacoes com RE-ENCODE.

  Camada 2 (wrapper alto), irma de FFmpegOps. Enquanto o FFmpegOps so
  COPIA pacotes (remux, split, merge, extracao de faixas), esta unit e a
  unica operacao do projeto que decodifica e reencoda video, pra produzir
  um arquivo menor e compartilhavel a partir de uma gravacao.

  O que ExportVideo sabe fazer:
    - manter N trechos do original e emendar um no outro (o usuario corta
      o video em partes e escolhe quais entram);
    - escolher quais REGIOES do canvas entram (monitores/webcams) e
      recompor as escolhidas lado a lado, sem o buraco preto de uma
      regiao pulada;
    - reduzir a resolucao e a taxa de quadros (nunca aumenta nenhuma das
      duas);
    - escolher o encoder entre os que existem no avcodec empacotado;
    - controlar a qualidade por CRF (0..51, escala do x264), sempre em
      modo de bitrate VARIAVEL (VBR de qualidade constante);
    - manter as faixas de audio escolhidas (stream copy) ou mixa-las
      numa faixa so.

  Container de saida: MP4 (default, mais compativel pra compartilhar) ou
  MKV, a escolha do usuario. A pegadinha #10 (MKV por causa de queda de
  energia) vale pra GRAVACAO, que nao da pra refazer; uma exportacao da.

  Roda SEMPRE em worker thread (pegadinha #3: libav pode e deve). O
  progresso volta pelo callback e o cancelamento e um Integer lido a cada
  pacote — quem chama marca de outra thread.
*)
unit FFmpegExport;

// Delay-loading e Win-only (esperado). Silencia W1002 SYMBOL_PLATFORM.
{$WARN SYMBOL_PLATFORM OFF}

interface

uses
  System.SysUtils,
  NoOBSTypes;

type
  TExportResult = (erOk, erCanceled, erNoEncoder, erError);

  // Um pedaco do original que ENTRA no resultado. O usuario corta o video
  // em partes e escolhe quais ficam; o que sobra vem pra ca, em ordem
  // crescente e sem sobreposicao (o chamador garante). As partes sao
  // emendadas numa linha do tempo continua na saida.
  TExportSegment = record
    StartSec, EndSec: Double;
  end;
  TExportSegmentArray = TArray<TExportSegment>;

  TExportOptions = record
    SrcPath: string;
    DstPath: string;
    // Nome do muxer no libavformat: 'mp4' ou 'matroska'. Vazio = 'mp4'.
    // Quem escolhe a extensao do DstPath e o chamador — aqui o formato e
    // sempre explicito, nunca deduzido do nome do arquivo.
    Container: AnsiString;
    // Regioes do canvas a manter, em qualquer ordem (a unit reordena
    // pela posicao X original). VAZIO = canvas inteiro.
    Regions: TRecordingRegionArray;
    // Altura final desejada. 0 = mantem a altura composta. Nunca faz
    // upscale: se for maior que a origem, e ignorada.
    TargetHeight: Integer;
    // Taxa de quadros final. 0 = mantem a da origem. Nunca aumenta (nao
    // ha como inventar quadro). Quadros fora da cadencia sao descartados
    // ANTES de compor/escalar, entao cada um economiza o trabalho todo.
    TargetFps: Integer;
    // Nome do encoder no libavcodec ('libx264', 'h264_amf', ...).
    EncoderName: AnsiString;
    // Algoritmo de reamostragem do swscale, no vocabulario do app:
    // 'bicubic' (default), 'bilinear' ou 'area'. Vazio = bicubic.
    //
    // So muda alguma coisa quando ha REDUCAO de resolucao: numa regiao
    // 1:1 o swscale nem reamostra, e os tres custam igual. A traducao pro
    // flag do swscale fica em ResolveScaleFlags.
    ScaleAlgo: AnsiString;
    // Qualidade CONSTANTE na escala do x264: 0 = sem perdas (arquivo
    // enorme), 51 = pior. Fora da faixa e clampado. Cada encoder tem sua
    // escala nativa — a traducao (e o modo VBR correspondente) fica em
    // ApplyQualityOptions.
    //
    // Nao existe alvo de bitrate: com qualidade constante o encoder gasta
    // os bits que o conteudo pedir, entao recortar uma regiao ou reduzir a
    // resolucao ja economiza sozinho, sem nenhuma escala manual por area.
    Crf: Integer;
    // Trechos que entram no resultado, em ordem. Pelo menos um; sem
    // recorte nenhum e um segmento so cobrindo o video inteiro. O
    // chamador deve mandar tempos concretos — sem eles nao da pra
    // calcular progresso nem bitrate de tamanho alvo.
    Segments: TExportSegmentArray;
    AudioStreams: TArray<Integer>;  // indices de stream DO SOURCE
    MixAudio: Boolean;          // junta as escolhidas numa faixa so
  end;

  // Chamado com o percentual 0..100 conforme a exportacao anda. Roda na
  // thread da exportacao — quem consome que marshalle pra main.
  TExportProgress = reference to procedure(APct: Double);

  // Um encoder oferecido no dropdown da UI.
  TExportEncoder = record
    Id: string;         // 'h264-hw' | 'h264-sw' | 'av1-hw' | 'hevc-hw'
    LibavName: string;  // 'h264_amf', 'libx264', ...
    Hardware: Boolean;
  end;
  TExportEncoderArray = TArray<TExportEncoder>;

const
  // Faixa do controle de qualidade, na escala do x264: 0 = sem perdas,
  // 51 = pior. A UI mostra o numero cru ("CRF 23") e usa estes mesmos
  // limites; o backend clampa por seguranca de qualquer jeito.
  EXPORT_CRF_MIN     = 0;
  EXPORT_CRF_MAX     = 51;
  EXPORT_CRF_DEFAULT = 23;   // default do x264, bom meio-termo

// Encoders que EXISTEM de fato no avcodec empacotado e servem pra este
// GPU. Cuidado: OBSEncoder.DetectEncoderCaps enumera os encoders do
// LIBOBS ('av1_texture_amf', 'obs_nvenc_*'), que NAO sao os mesmos nomes
// do libavcodec — por isso a traducao vive aqui.
function ListExportEncoders(const ACaps: TEncoderCaps): TExportEncoderArray;

// Traduz a preferencia do usuario (mesmo vocabulario do config 'codec',
// incluindo 'auto') pro nome do encoder no libavcodec. Sempre devolve
// algo utilizavel — cai pro libx264 quando nada mais serve.
function ResolveExportEncoder(const APref: string;
  const ACaps: TEncoderCaps): AnsiString;

// Exporta. Ver o cabecalho da unit. ACancelFlag pode ser nil.
function ExportVideo(const AOpts: TExportOptions; AProgress: TExportProgress;
  ACancelFlag: PInteger): TExportResult;

implementation

uses
  Winapi.Windows,
  System.Math,
  OBSLog,
  FFmpegLib;

const
  // AV_NOPTS_VALUE vive na implementation do FFmpegLib (nao exportado).
  // Mesmo valor de avutil (INT64_MIN). Igual ao FFmpegOps.
  AV_NOPTS_VALUE = Int64($8000000000000000);

  // Bitrate da faixa mixada. 192k estereo cobre voz + audio de sistema
  // com folga; a faixa mixada e conveniencia, nao arquivo mestre.
  MIX_BITRATE = 192000;

  // Intervalo de keyframe da saida, em segundos. Casa com o default de
  // gravacao do app — mantem o resultado divisivel pelo split depois.
  OUT_KEYINT_SEC = 2;

  // ------------------------------------------------------------------
  // Calibracao de qualidade entre encoders.
  //
  // CRF_ANCHOR sao pontos da escala de REFERENCIA (o CRF do x264, que e o
  // numero que a tela de exportacao mostra). Cada Q_* abaixo diz, naquele
  // mesmo ponto, qual parametro nativo do encoder entrega a MESMA
  // qualidade — medido por PSNR sobre quadros reais de tela, encodando e
  // decodificando de volta. Valores intermediarios saem por interpolacao
  // em CalibratedQuality.
  //
  // Sem isto, a tela mostrava "CRF 23" e cada encoder entendia uma coisa
  // diferente: o SVT-AV1 entregava 56,6 dB onde o x264 dava 46,5.
  //
  // Ancoras densas no meio (17..28), onde o olho discrimina, e esparsas
  // nos extremos, onde tudo satura.
  // ------------------------------------------------------------------
  CRF_ANCHOR: array[0..15] of Integer =
    (0, 4, 8, 12, 15, 17, 19, 21, 23, 26, 28, 31, 34, 38, 44, 51);

  // SVT-AV1 (crf 1..63). Piso 1: o 'crf 0' do wrapper significa "nao
  // setado" e cai no qp default (~35) — medido em 54,3 dB contra os
  // 65,0 dB do crf 1. Mesma armadilha do 'cq=0' do NVENC.
  Q_EXP_SVTAV1: array[0..15] of Integer =
    (1, 5, 17, 29, 37, 42, 46, 51, 55, 60, 61, 63, 63, 63, 63, 63);

  // AMF AV1 (qp 0..255 pelo libavcodec — aqui NAO ha o /4 do plugin
  // libobs; este e o caminho do libavcodec, que expoe a escala cheia).
  Q_EXP_AV1AMF: array[0..15] of Integer =
    (0, 0, 12, 30, 46, 57, 73, 93, 109, 131, 145, 165, 186, 212, 244, 255);

  // AMF H.264 e HEVC (qp 0..51). O HEVC pede qp maior pro mesmo PSNR.
  Q_EXP_H264AMF: array[0..15] of Integer =
    (0, 6, 11, 16, 19, 21, 23, 25, 27, 30, 32, 35, 38, 42, 48, 51);
  Q_EXP_HEVCAMF: array[0..15] of Integer =
    (0, 9, 14, 18, 21, 23, 25, 27, 29, 32, 34, 37, 40, 44, 50, 51);

  // Preto em YUV limited range (que e o que o OBS grava). Usar 0 no Y
  // daria "super preto", fora da faixa legal.
  YUV_BLACK_Y = 16;
  YUV_BLACK_C = 128;

  // Flag do av_dict_get pra iterar todas as chaves do dicionario.
  AV_DICT_IGNORE_SUFFIX = 2;

type
  // Uma regiao do canvas original mapeada pra uma faixa do canvas final.
  // Todas as coordenadas ja arredondadas pra PAR (YUV420 nao aceita
  // offset nem dimensao impar).
  TCompRegion = record
    SrcX, SrcY, SrcW, SrcH: Integer;
    DstX, DstY, DstW, DstH: Integer;
    Sws: SwsContext;
  end;
  TCompRegionArray = TArray<TCompRegion>;

  // Uma faixa de audio selecionada.
  TAudioTrack = record
    SrcIdx: Integer;          // stream index no source
    OutIdx: Integer;          // stream index na saida (-1 = entra no mix)
    SrcTb: AVRational;
    DecCtx: PAVCodecContext;  // so no modo mix
    Done: Boolean;            // ja passou do fim do trecho
  end;
  TAudioTrackArray = TArray<TAudioTrack>;

// =====================================================================
// Helpers locais
// =====================================================================
// OpenInputWithRetry e CopyStreamTag existem tambem em FFmpegOps (na
// implementation). Sao replicados aqui de proposito: exporta-los obrigaria
// a por tipos do FFmpegLib (AVFormatContext, PAVStream) na INTERFACE do
// FFmpegOps, que hoje e limpa de structs C — e essa fronteira vale mais
// que as ~20 linhas duplicadas.

function EvenDown(AValue: Integer): Integer; inline;
begin
  Result := AValue and not 1;
end;

function OpenInputWithRetry(var ACtx: AVFormatContext;
  const APath: string): Boolean;
// Cobre o arquivo estar MOMENTANEAMENTE aberto por outra thread (o Probe
// e a geracao de thumb abrem por ~200-500ms).
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
    Sleep(RETRY_MS);
  end;
  Log('Export: nao conseguiu abrir "%s".',
    [System.SysUtils.ExtractFileName(APath)]);
end;

procedure CopyStreamTag(ASrc, ADst: PAVStream; const AKey: PAnsiChar);
// Copia UMA tag (title/language). Nao copia a metadata inteira: o
// Matroska guarda DURATION/_STATISTICS_* por stream que ficariam erradas.
var
  Entry: PAVDictionaryEntry;
begin
  if (ASrc = nil) or (ADst = nil) then Exit;
  Entry := av_dict_get(ASrc.metadata, AKey, nil, 0);
  if (Entry <> nil) and (Entry.value <> nil) then
    av_dict_set(@ADst.metadata, AKey, Entry.value, 0);
end;

function IsCanceled(AFlag: PInteger): Boolean; inline;
// Leitura simples: em x86-64 um Integer alinhado le atomicamente, e a
// unica transicao possivel e 0 -> 1 (nunca volta).
begin
  Result := (AFlag <> nil) and (AFlag^ <> 0);
end;

function PtsToSec(APts: Int64; const ATb: AVRational): Double; inline;
begin
  if (APts = AV_NOPTS_VALUE) or (ATb.den <= 0) then Exit(-1);
  Result := APts * (ATb.num / ATb.den);
end;

function SecToTs(ASec: Double; const ATb: AVRational): Int64; inline;
begin
  if ATb.num <= 0 then Exit(0);
  Result := Round(ASec / (ATb.num / ATb.den));
end;

// =====================================================================
// Selecao de encoder
// =====================================================================

function EncoderExists(const AName: string): Boolean;
var
  N: AnsiString;
begin
  Result := False;
  if AName = '' then Exit;
  try
    N := AnsiString(AName);
    Result := avcodec_find_encoder_by_name(PAnsiChar(N)) <> nil;
  except
    Result := False;
  end;
end;

function HwEncoderName(const AFamily: string; AVendor: TGpuVendor): string;
// AFamily: 'h264' | 'hevc' | 'av1'. String vazia = nao ha caminho de
// hardware pra esse vendor/familia.
//
// O build do FFmpeg que vem com o OBS NAO tras nenhum *_qsv, entao Intel
// vai por Media Foundation (h264_mf/hevc_mf) — que nao tem AV1.
begin
  case AVendor of
    gvNvidia: Result := AFamily + '_nvenc';
    gvAmd:    Result := AFamily + '_amf';
    gvIntel:
      if AFamily = 'av1' then Result := ''
      else Result := AFamily + '_mf';
  else
    Result := '';
  end;
end;

function ListExportEncoders(const ACaps: TEncoderCaps): TExportEncoderArray;
// Ordem espelhando o select de Configuracoes (menos 'auto'):
// H.264 hw, H.264 sw, AV1 hw, AV1 sw, HEVC hw. Um candidato so entra se o
// avcodec_find_encoder_by_name realmente achar — nada de oferecer opcao
// que vai falhar na hora do avcodec_open2.
//
// REGRA DIFERENTE DA GRAVACAO, de proposito. Na gravacao o encoder de
// software so entra como ultimo recurso, porque a gravacao concorre com o
// que o usuario esta fazendo e nao pode comer a CPU. Aqui a exportacao
// roda em worker, o usuario escolheu esperar, e o tempo e um preco
// aceitavel por um formato melhor. Entao AV1 e oferecido MESMO SEM
// hardware AV1, via libsvtav1.
//
// Os ramos de HARDWARE continuam condicionados as caps: oferecer
// "AV1 — hardware" numa maquina sem esse encoder so daria erro no
// avcodec_open2. Quem destrava o formato sem hardware e o ramo software.
//
// Custo medido nesta maquina (1920x1080, threads=auto):
//   libx264    289 quadros/s   (praticamente empata com o hardware)
//   libsvtav1   72 quadros/s   (~4x mais lento que o x264, aceitavel)
//   libaom-av1   8 quadros/s   (ja em cpu-used=8, o ajuste mais rapido)
// Por isso o AV1 software e o libsvtav1, e NAO o libaom-av1: em 4K o
// libaom cairia pra ~2 quadros/s, e uma gravacao de 10 min viraria mais
// de duas horas de exportacao — isso nao e "tempo nao e problema", e uma
// barra de progresso que nao anda.
//
// Nao ha HEVC por software: o build empacotado nao tem libx265, e o
// 'hevc_mf' e um wrapper do Media Foundation que pega o MFT de hardware
// quando existe — nao serve de caminho por software confiavel, e pode
// nem abrir numa maquina sem HEVC no sistema.
  procedure Add(const AId, AName: string; AHw: Boolean);
  var
    E: TExportEncoder;
  begin
    if not EncoderExists(AName) then Exit;
    E.Id := AId;
    E.LibavName := AName;
    E.Hardware := AHw;
    Result := Result + [E];
  end;
begin
  Result := nil;
  if not FFmpegLibAvailable then Exit;

  if ACaps.H264Hw then Add('h264-hw', HwEncoderName('h264', ACaps.Vendor), True);
  Add('h264-sw', 'libx264', False);
  if ACaps.Av1Hw then Add('av1-hw', HwEncoderName('av1', ACaps.Vendor), True);
  Add('av1-sw', 'libsvtav1', False);
  if ACaps.HevcHw then Add('hevc-hw', HwEncoderName('hevc', ACaps.Vendor), True);
end;

function ResolveExportEncoder(const APref: string;
  const ACaps: TEncoderCaps): AnsiString;
var
  List: TExportEncoderArray;
  Pref, Name: string;

  function Find(const AId: string): string;
  var
    j: Integer;
  begin
    Result := '';
    for j := 0 to High(List) do
      if List[j].Id = AId then Exit(List[j].LibavName);
  end;

begin
  // Casts explicitos pra AnsiString em todo lugar: a conversao implicita
  // de literal Unicode dispara W1057/W1058 e o build precisa sair limpo.
  Result := AnsiString('libx264');
  List := ListExportEncoders(ACaps);
  if Length(List) = 0 then Exit;

  Pref := LowerCase(Trim(APref));
  if (Pref = '') or (Pref = 'auto') then
  begin
    // Mesma ordem de preferencia do OBSEncoder.SelectVideoEncoder:
    // H.264 hw -> x264 -> AV1 hw -> HEVC hw. Compatibilidade primeiro.
    Name := Find('h264-hw');
    if Name = '' then Name := Find('h264-sw');
    if Name = '' then Name := Find('av1-hw');
    if Name = '' then Name := Find('hevc-hw');
  end
  else
  begin
    Name := Find(Pref);
    if Name = '' then Name := Find('h264-sw');
  end;
  if Name <> '' then Result := AnsiString(Name);
end;

// =====================================================================
// Composicao das regioes
// =====================================================================

function BuildCompRegions(const ARegions: TRecordingRegionArray;
  ACanvasW, ACanvasH, ATargetHeight: Integer;
  out ARegs: TCompRegionArray; out AOutW, AOutH: Integer): Boolean;
// Monta o mapeamento origem -> destino. As regioes escolhidas ficam LADO
// A LADO na ordem do X original, encostadas: pular o monitor do meio de
// tres nao deixa faixa preta, os dois das bordas se encaixam.
//
// Mesma regra de canvas que o OBSScene.ComputeCanvas usa na gravacao:
// soma das larguras x maior altura.
var
  i, j, Cursor, NativeW, NativeH: Integer;
  Scale: Double;
  Tmp, R: TCompRegion;
begin
  Result := False;
  ARegs := nil;
  AOutW := 0;
  AOutH := 0;
  if (ACanvasW <= 0) or (ACanvasH <= 0) then Exit;

  // Sem regioes (ou gravacao antiga sem layout): canvas inteiro.
  if Length(ARegions) = 0 then
  begin
    FillChar(R, SizeOf(R), 0);
    R.SrcW := EvenDown(ACanvasW);
    R.SrcH := EvenDown(ACanvasH);
    ARegs := [R];
  end
  else
  begin
    for i := 0 to High(ARegions) do
    begin
      FillChar(R, SizeOf(R), 0);
      R.SrcX := EvenDown(Max(0, ARegions[i].X));
      R.SrcY := EvenDown(Max(0, ARegions[i].Y));
      R.SrcW := EvenDown(Min(ARegions[i].W, ACanvasW - R.SrcX));
      R.SrcH := EvenDown(Min(ARegions[i].H, ACanvasH - R.SrcY));
      if (R.SrcW < 2) or (R.SrcH < 2) then
      begin
        Log('Export: regiao %d degenerada (%dx%d) — ignorada.',
          [i, R.SrcW, R.SrcH]);
        Continue;
      end;
      ARegs := ARegs + [R];
    end;
    if Length(ARegs) = 0 then Exit;

    // Ordena pelo X original (insertion sort — sao 1-4 itens).
    for i := 1 to High(ARegs) do
    begin
      Tmp := ARegs[i];
      j := i - 1;
      while (j >= 0) and (ARegs[j].SrcX > Tmp.SrcX) do
      begin
        ARegs[j + 1] := ARegs[j];
        Dec(j);
      end;
      ARegs[j + 1] := Tmp;
    end;
  end;

  // Tamanho nativo da composicao.
  NativeW := 0;
  NativeH := 0;
  for i := 0 to High(ARegs) do
  begin
    Inc(NativeW, ARegs[i].SrcW);
    NativeH := Max(NativeH, ARegs[i].SrcH);
  end;
  if (NativeW < 2) or (NativeH < 2) then Exit;

  // Fator unico de escala, nunca acima de 1 (nao inventa pixel).
  Scale := 1.0;
  if (ATargetHeight > 0) and (ATargetHeight < NativeH) then
    Scale := ATargetHeight / NativeH;

  Cursor := 0;
  for i := 0 to High(ARegs) do
  begin
    ARegs[i].DstW := Max(2, EvenDown(Round(ARegs[i].SrcW * Scale)));
    ARegs[i].DstH := Max(2, EvenDown(Round(ARegs[i].SrcH * Scale)));
    ARegs[i].DstX := Cursor;
    Inc(Cursor, ARegs[i].DstW);
  end;

  AOutW := EvenDown(Cursor);
  AOutH := 2;
  for i := 0 to High(ARegs) do
    AOutH := Max(AOutH, ARegs[i].DstH);
  AOutH := EvenDown(AOutH);

  // Regiao mais baixa que a maior fica centrada na vertical sobre preto.
  for i := 0 to High(ARegs) do
    ARegs[i].DstY := EvenDown((AOutH - ARegs[i].DstH) div 2);

  Result := (AOutW >= 2) and (AOutH >= 2);
end;

procedure FillFrameBlack(AFrame: PAVFrame; AW, AH: Integer);
// Pinta o frame de destino de preto. So chamado quando as regioes nao
// cobrem o canvas inteiro (alturas diferentes).
var
  y: Integer;
  Row: PByte;
begin
  for y := 0 to AH - 1 do
  begin
    Row := PByte(NativeUInt(AFrame.data[0]) + NativeUInt(y) *
      NativeUInt(AFrame.linesize[0]));
    FillChar(Row^, AW, YUV_BLACK_Y);
  end;
  for y := 0 to (AH div 2) - 1 do
  begin
    Row := PByte(NativeUInt(AFrame.data[1]) + NativeUInt(y) *
      NativeUInt(AFrame.linesize[1]));
    FillChar(Row^, AW div 2, YUV_BLACK_C);
    Row := PByte(NativeUInt(AFrame.data[2]) + NativeUInt(y) *
      NativeUInt(AFrame.linesize[2]));
    FillChar(Row^, AW div 2, YUV_BLACK_C);
  end;
end;

procedure BlitRegion(const AReg: TCompRegion; ASrc, ADst: PAVFrame);
// Recorta AReg.Src* de ASrc e escala pra AReg.Dst* dentro de ADst. O
// recorte sai de graca deslocando os ponteiros de plano — o linesize
// (passo da linha) continua sendo o do frame inteiro.
//
// Assume ambos em planar YUV 4:2:0 8 bits (o chamador garante, ver
// NeedsNormalize em ExportVideo). Croma tem metade da resolucao nos dois
// eixos, dai o `div 2` nos offsets dos planos 1 e 2.
var
  SrcData, DstData: array[0..7] of PByte;
  SrcLs, DstLs: array[0..7] of Integer;
  p: Integer;
begin
  FillChar(SrcData, SizeOf(SrcData), 0);
  FillChar(DstData, SizeOf(DstData), 0);
  FillChar(SrcLs, SizeOf(SrcLs), 0);
  FillChar(DstLs, SizeOf(DstLs), 0);

  for p := 0 to 2 do
  begin
    SrcLs[p] := ASrc.linesize[p];
    DstLs[p] := ADst.linesize[p];
  end;

  SrcData[0] := PByte(NativeUInt(ASrc.data[0]) +
    NativeUInt(AReg.SrcY) * NativeUInt(SrcLs[0]) + NativeUInt(AReg.SrcX));
  SrcData[1] := PByte(NativeUInt(ASrc.data[1]) +
    NativeUInt(AReg.SrcY div 2) * NativeUInt(SrcLs[1]) +
    NativeUInt(AReg.SrcX div 2));
  SrcData[2] := PByte(NativeUInt(ASrc.data[2]) +
    NativeUInt(AReg.SrcY div 2) * NativeUInt(SrcLs[2]) +
    NativeUInt(AReg.SrcX div 2));

  DstData[0] := PByte(NativeUInt(ADst.data[0]) +
    NativeUInt(AReg.DstY) * NativeUInt(DstLs[0]) + NativeUInt(AReg.DstX));
  DstData[1] := PByte(NativeUInt(ADst.data[1]) +
    NativeUInt(AReg.DstY div 2) * NativeUInt(DstLs[1]) +
    NativeUInt(AReg.DstX div 2));
  DstData[2] := PByte(NativeUInt(ADst.data[2]) +
    NativeUInt(AReg.DstY div 2) * NativeUInt(DstLs[2]) +
    NativeUInt(AReg.DstX div 2));

  sws_scale(AReg.Sws, @SrcData[0], @SrcLs[0], 0, AReg.SrcH,
    @DstData[0], @DstLs[0]);
end;

// =====================================================================
// Mixagem de audio
// =====================================================================

{$POINTERMATH ON}
procedure MixInto(ADst, ASrc: PAVFrame);
// Soma as amostras de ASrc em ADst, plano a plano, com clamp. Os dois em
// FLTP (o decoder de AAC sempre entrega assim). Faixa mono entrando em
// acumulador estereo vai pros dois canais.
//
// Rotina de UNIDADE (e nao aninhada em ExportVideo) por causa do
// {$POINTERMATH ON}: a diretiva e lexica, e so aqui queremos aritmetica
// de ponteiro em PSingle — no resto da unit ela fica desligada.
var
  ch, n, SrcCh, Cnt: Integer;
  D, Sp: PSingle;
  V: Single;
begin
  if (ADst = nil) or (ASrc = nil) then Exit;
  Cnt := Min(ADst.nb_samples, ASrc.nb_samples);
  if Cnt <= 0 then Exit;
  // Planos ate onde os dois tem ponteiro. Audio com mais de 8 canais usa
  // extended_data, que nao cobrimos — o OBS grava 1 ou 2.
  for ch := 0 to 7 do
  begin
    if ADst.data[ch] = nil then Break;
    SrcCh := ch;
    if ASrc.data[SrcCh] = nil then SrcCh := 0;   // mono -> os dois lados
    if ASrc.data[SrcCh] = nil then Break;
    D := PSingle(ADst.data[ch]);
    Sp := PSingle(ASrc.data[SrcCh]);
    for n := 0 to Cnt - 1 do
    begin
      V := D[n] + Sp[n];
      if V > 1.0 then V := 1.0
      else if V < -1.0 then V := -1.0;
      D[n] := V;
    end;
  end;
end;
{$POINTERMATH OFF}

// =====================================================================
// Configuracao do encoder de video
// =====================================================================

function ResolveScaleFlags(const AAlgo: AnsiString): Integer;
// Vocabulario do app -> flag do swscale. Desconhecido/vazio = bicubic, que
// e o default historico da unit.
//
// Medido na swscale-8 empacotada, reduzindo 3840x2160 -> 1920x1080:
// bicubic 4,77 ms/quadro | bilinear 2,69 | area 2,54. (fast_bilinear NAO
// entra: alem de pior, mediu 6,71 — mais lento que o bicubic.)
var
  A: string;
begin
  // Converte pra string UMA vez: comparar AnsiString com literal Unicode
  // dispara W1057/W1058 e o build precisa sair limpo (mesma razao dos
  // casts explicitos do ResolveExportEncoder).
  A := LowerCase(Trim(string(AAlgo)));
  if A = 'bilinear' then Exit(SWS_BILINEAR);
  if A = 'area'     then Exit(SWS_AREA);
  Result := SWS_BICUBIC;
end;

function CalibratedQuality(const ATable: array of Integer;
  ACrf, AMin, AMax: Integer): Integer;
// Traduz o CRF de referencia (0..51, escala do x264 — a que a tela de
// exportacao mostra) pro parametro nativo do encoder, interpolando entre
// as ancoras MEDIDAS.
//
// Por que nao reescalar linearmente (o que se fazia aqui antes): porque
// "mesma posicao relativa na escala" NAO significa "mesma qualidade".
// Medido encodando quadros reais de tela, decodificando de volta e
// comparando PSNR do plano Y contra a origem, num CRF 23:
//
//   x264      crf 23 -> 46,5 dB      (referencia)
//   SVT-AV1   crf 28 -> 56,6 dB      era o que o reescalonamento dava
//   SVT-AV1   crf 55 -> 46,5 dB      e o que realmente equivale
//   AMF H.264 qp  23 -> 49,9 dB      /  qp 27 equivale
//   AMF HEVC  qp  23 -> 51,7 dB      /  qp 29 equivale
//
// Ou seja: no mesmo numero da tela, o AV1 por software entregava 10 dB a
// mais do que o pedido — arquivo muito maior, sem o usuario ter pedido.
// As ancoras vivem em CRF_ANCHOR; cada encoder tem a sua linha.
var
  i: Integer;
  T0, T1, V0, V1: Integer;
begin
  if ACrf <= CRF_ANCHOR[0] then Exit(ATable[0]);
  if ACrf >= CRF_ANCHOR[High(CRF_ANCHOR)] then Exit(ATable[High(CRF_ANCHOR)]);
  for i := 0 to High(CRF_ANCHOR) - 1 do
    if (ACrf >= CRF_ANCHOR[i]) and (ACrf <= CRF_ANCHOR[i + 1]) then
    begin
      T0 := CRF_ANCHOR[i];     T1 := CRF_ANCHOR[i + 1];
      V0 := ATable[i];         V1 := ATable[i + 1];
      if T1 = T0 then Exit(V0);
      Result := V0 + Round((V1 - V0) * (ACrf - T0) / (T1 - T0));
      if Result < AMin then Result := AMin;
      if Result > AMax then Result := AMax;
      Exit;
    end;
  Result := ATable[High(CRF_ANCHOR)];
end;

function ScaleCrf(ACrf, AMax: Integer): Integer;
// Reescalonamento LINEAR — sobrou so pros encoders ainda NAO calibrados
// (NVENC e Media Foundation, sem hardware desses aqui pra medir). Nos
// calibrados use CalibratedQuality, que e medido em vez de suposto.
begin
  Result := Round(ACrf * (AMax / EXPORT_CRF_MAX));
  if Result < 0 then Result := 0;
  if Result > AMax then Result := AMax;
end;

function ApplyQualityOptions(var ADict: AVDictionary; const AEncoder: string;
  ACrf, AAttempt: Integer): Boolean;
// Traduz o CRF 0..51 pro controle de QUALIDADE CONSTANTE de cada encoder,
// sempre em modo de bitrate VARIAVEL: o encoder gasta os bits que o
// conteudo pedir pra sustentar a qualidade pedida, em vez de perseguir um
// alvo fixo.
//
// Nada de 'b'/'maxrate'/'bufsize' por aqui — com um alvo de bitrate
// junto, todos estes encoders voltam pro modo de alvo e o parametro de
// qualidade vira enfeite (no NVENC e literalmente ignorado, dai o 'b=0'
// explicito).
//
// AAttempt = 0 e a forma preferida; 1 seria o plano B pra quando o driver
// recusa a primeira. HOJE NENHUM encoder tem plano B — o unico que tinha
// era o AMF, cuja 1a tentativa era o 'qvbr' INVERTIDO (ver abaixo); com
// ele fora, o CQP e tentativa unica. Result = False significa "nao ha
// tentativa AAttempt" — pare de tentar.
var
  E, Q: string;

  procedure SetOpt(const AKey, AVal: string);
  begin
    av_dict_set(@ADict, PAnsiChar(AnsiString(AKey)),
      PAnsiChar(AnsiString(AVal)), 0);
  end;

  function Has(const ASub: string): Boolean;
  begin
    Result := Pos(ASub, E) > 0;
  end;

begin
  Result := False;
  E := LowerCase(AEncoder);
  if ACrf < EXPORT_CRF_MIN then ACrf := EXPORT_CRF_MIN;
  if ACrf > EXPORT_CRF_MAX then ACrf := EXPORT_CRF_MAX;

  // ---- x264: CRF nativo, exatamente a mesma escala do controle.
  if E = 'libx264' then
  begin
    if AAttempt > 0 then Exit;
    SetOpt('crf', IntToStr(ACrf));
    // 'preset' so existe no x264; num encoder de hardware seria ignorado
    // (ou pior, casaria com uma opcao homonima de outro significado).
    SetOpt('preset', 'veryfast');
    Exit(True);
  end;

  // ---- SVT-AV1: o AV1 por SOFTWARE da exportacao. Tem 'crf' nativo, mas
  // numa escala 0..63 (confirmado enumerando as AVOption: crf em [0..63]),
  // nao 0..51 — mandar o CRF cru desperdicaria o topo da escala e deixaria
  // "qualidade minima" bem melhor do que o pedido.
  //
  // Sem 'preset' explicito de proposito: o default (-2, que o SVT resolve
  // internamente) foi o que rendeu os 72 quadros/s medidos a 1080p. Fixar
  // outro valor aqui exigiria re-medir.
  if E = 'libsvtav1' then
  begin
    if AAttempt > 0 then Exit;
    SetOpt('crf', IntToStr(CalibratedQuality(Q_EXP_SVTAV1, ACrf, 1, 63)));
    Exit(True);
  end;

  // ---- NVENC: 'rc=vbr' + 'cq' = VBR guiado por qualidade. O 'b=0' e
  // explicito porque um bitrate alvo faria o NVENC perseguir o alvo. A
  // escala do AV1 vai a 63, nao a 51.
  //
  // O piso de 1 nao e capricho: pro NVENC 'cq=0' quer dizer "sem alvo de
  // qualidade", entao um CRF 0 (que no x264 e SEM PERDAS) cairia no
  // controle de bitrate padrao — o oposto do pedido. cq=1 e o mais
  // proximo de sem perdas que o NVENC aceita.
  if Has('_nvenc') then
  begin
    if AAttempt > 0 then Exit;
    SetOpt('rc', 'vbr');
    if Has('av1') then SetOpt('cq', IntToStr(Max(1, ScaleCrf(ACrf, 63))))
                  else SetOpt('cq', IntToStr(Max(1, ACrf)));
    SetOpt('b', '0');
    Exit(True);
  end;

  // ---- AMF: CQP. Tentativa unica — nao ha plano B aqui.
  //
  // NAO usar 'qvbr'. O nome promete o VBR guiado por qualidade, mas o
  // 'qvbr_quality_level' do AMF NAO e um QP: e um NIVEL DE QUALIDADE em
  // que MAIOR = MELHOR, o inverso da escala do CRF. Passar o CRF cru
  // (como se fazia aqui) INVERTIA o controle da tela de exportacao —
  // pedir qualidade minima entregava arquivo maior.
  //
  // Medido no av1_amf desta maquina (60 quadros 720p, detalhe fino +
  // movimento), com o que a UI mandava em cada extremo:
  //   qvbr level 51 (user pediu MINIMA) -> 6414 KB
  //   qvbr level  1 (user pediu MAXIMA) -> 2536 KB   <- invertido
  //   cqp  qp   255 (minima) -> 862 KB | qp 100 -> 6727 KB | qp 0 -> 11712 KB
  // O CQP e monotonico na direcao certa e cobre uma faixa 13x maior que a
  // do QVBR (2,5x). Mesma conclusao do caminho de GRAVACAO — ver
  // OBSEncoder.ApplyConstantQuality e a pegadinha #53.
  if Has('_amf') then
  begin
    if AAttempt <> 0 then Exit;
    SetOpt('rc', 'cqp');
    if Has('av1') then
    begin
      // No AV1 do AMF o qp vai a 255, nao a 51 (confirmado enumerando as
      // AVOption do av1_amf: qp_i/qp_p em [-1..255]).
      Q := IntToStr(CalibratedQuality(Q_EXP_AV1AMF, ACrf, 0, 255));
      SetOpt('qp_i', Q);
      SetOpt('qp_p', Q);
    end
    else if Has('hevc') or Has('h265') then
    begin
      Q := IntToStr(CalibratedQuality(Q_EXP_HEVCAMF, ACrf, 0, 51));
      SetOpt('qp_i', Q);
      SetOpt('qp_p', Q);
      // qp_b so existe no AVC — o HEVC do AMF nao expoe.
    end
    else
    begin
      Q := IntToStr(CalibratedQuality(Q_EXP_H264AMF, ACrf, 0, 51));
      SetOpt('qp_i', Q);
      SetOpt('qp_p', Q);
      SetOpt('qp_b', Q);
    end;
    Result := True;
    Exit;
  end;

  // ---- Media Foundation (o caminho de hardware da Intel neste build):
  // 'rate_control=quality' e VBR guiado por qualidade, mas a escala e
  // 0..100 e INVERTIDA — 100 e o melhor.
  if Has('_mf') then
  begin
    if AAttempt > 0 then Exit;
    SetOpt('rate_control', 'quality');
    SetOpt('quality', IntToStr(100 - ScaleCrf(ACrf, 100)));
    Exit(True);
  end;

  // Encoder que nao conhecemos: 'crf' e o nome mais comum. Se ele nao
  // consumir, o LogLeftoverOptions avisa no log.
  if AAttempt > 0 then Exit;
  SetOpt('crf', IntToStr(ACrf));
  Result := True;
end;

procedure LogLeftoverOptions(AOpts: AVDictionary; const AWhat: string);
// Opcoes que o avcodec_open2 nao consumiu ficam no dicionario. Nao e
// fatal (um 'crf' sobra em encoder de hardware, por exemplo), mas saber
// disso economiza meia hora quando a qualidade nao muda.
var
  E: PAVDictionaryEntry;
  // Chave vazia num buffer explicito: o idioma de iteracao do libav exige
  // uma string vazia REAL, e PAnsiChar de string vazia pode virar nil.
  EmptyKey: array[0..0] of AnsiChar;
begin
  if AOpts = nil then Exit;
  EmptyKey[0] := #0;
  E := av_dict_get(AOpts, @EmptyKey[0], nil, AV_DICT_IGNORE_SUFFIX);
  while E <> nil do
  begin
    Log('Export: opcao "%s" ignorada pelo encoder %s.',
      [UTF8ToString(E.key), AWhat]);
    E := av_dict_get(AOpts, @EmptyKey[0], E, AV_DICT_IGNORE_SUFFIX);
  end;
end;

// =====================================================================
// ExportVideo
// =====================================================================

function ExportVideo(const AOpts: TExportOptions; AProgress: TExportProgress;
  ACancelFlag: PInteger): TExportResult;
var
  SrcCtx, OutCtx: AVFormatContext;
  OutPb: Pointer;
  HeaderWritten: Boolean;
  VIdx, i, j, Rc: Integer;
  NbStreams: Cardinal;
  S, VStream, OutVStream, OutAStream, OutMixStream: PAVStream;
  Decoder, Encoder, MixEncoder: PAVCodec;
  DecCtx, EncCtx, MixCtx: PAVCodecContext;
  EncPar: PAVCodecParameters;
  Pkt, EncPkt: PAVPacket;
  Frame, NormFrame, OutFrame, AccFrame: PAVFrame;
  NormSws: SwsContext;
  Regs: TCompRegionArray;
  Tracks: TAudioTrackArray;
  OutW, OutH, Fps, OutFps, SrcFmt, SegIdx: Integer;
  DropFrames: Boolean;
  FrameInterval, NextKeepSec, OutSec: Double;
  VideoTb, EncTb, MixTb, MixSrcTb, FpsRat: AVRational;
  SegStartTs, SegEndTs: Int64;
  // Ultimo pts entregue ao encoder de video, na time_base DELE. Serve pra
  // guarda de monotonicidade em EncodeVideoFrame.
  LastEncPts: Int64;
  PtsCollisionLogged: Boolean;
  // Linha do tempo da SAIDA: quanto ja foi escrito, em segundos. E o que
  // emenda um trecho no outro sem buraco. Convertido pro time_base de
  // cada stream na hora de escrever.
  OutOffsetSec, TotalSec, SegStartSec, SegEndSec: Double;
  NeedsFill, NeedsNormalize, DoMix, VideoDone, AllDone: Boolean;
  EncOpts: AVDictionary;
  Container: AnsiString;
  Crf, Attempt, ScaleFlags: Integer;
  LastPctStep: Integer;
  Canceled, Failed, SetupDone: Boolean;

  procedure ReportProgress(ASec: Double);
  // ASec = posicao dentro do trecho corrente, no tempo do ORIGINAL. O que
  // conta pro progresso e o tempo de SAIDA ja produzido.
  var
    Pct: Double;
    Step: Integer;
  begin
    if not Assigned(AProgress) then Exit;
    if TotalSec <= 0 then Exit;
    Pct := (OutOffsetSec + (ASec - SegStartSec)) / TotalSec * 100;
    if Pct < 0 then Pct := 0;
    if Pct > 100 then Pct := 100;
    // So notifica a cada 0.5% — quem consome ainda limita por tempo.
    Step := Trunc(Pct * 2);
    if Step = LastPctStep then Exit;
    LastPctStep := Step;
    AProgress(Pct);
  end;

  function DrainEncoder(ACtx: PAVCodecContext; AStream: PAVStream;
    const ACtxTb: AVRational): Boolean;
  // Tira os pacotes prontos do encoder e escreve no muxer. True = ok.
  var
    R: Integer;
  begin
    Result := True;
    if (ACtx = nil) or (AStream = nil) then Exit;
    while True do
    begin
      R := avcodec_receive_packet(ACtx, EncPkt);
      if (R = AVERROR_EAGAIN) or (R = AVERROR_EOF) then Exit;
      if R < 0 then
      begin
        Log('Export: avcodec_receive_packet falhou (%s).', [AvErrStr(R)]);
        Exit(False);
      end;
      try
        EncPkt.stream_index := AStream.index;
        av_packet_rescale_ts(EncPkt, ACtxTb, AStream.time_base);
        EncPkt.pos := -1;
        R := av_interleaved_write_frame(OutCtx, EncPkt);
        if R < 0 then
        begin
          Log('Export: av_interleaved_write_frame falhou (%s).', [AvErrStr(R)]);
          Exit(False);
        end;
      finally
        av_packet_unref(EncPkt);
      end;
    end;
  end;

  function FlushMixFrame: Boolean;
  // Manda o acumulador de audio pro encoder AAC e limpa. O pts vem no
  // time_base da FAIXA de origem (nao no do video): tira o inicio do
  // trecho e soma o offset da linha do tempo de saida.
  var
    R: Integer;
  begin
    Result := True;
    if (AccFrame = nil) or (AccFrame.nb_samples <= 0) then Exit;
    if AccFrame.pts <> AV_NOPTS_VALUE then
      AccFrame.pts :=
        av_rescale_q(AccFrame.pts - SecToTs(SegStartSec, MixSrcTb),
                     MixSrcTb, MixTb) + SecToTs(OutOffsetSec, MixTb);
    R := avcodec_send_frame(MixCtx, AccFrame);
    av_frame_unref(AccFrame);
    if R < 0 then
    begin
      Log('Export: avcodec_send_frame (mix) falhou (%s).', [AvErrStr(R)]);
      Exit(False);
    end;
    Result := DrainEncoder(MixCtx, OutMixStream, MixTb);
  end;

  function AdoptAccumulator: Boolean;
  // Adota o frame recem-decodificado como acumulador. Isso evita ter que
  // CRIAR um frame de audio do zero, o que exigiria preencher ch_layout —
  // campo que fica fora da parte declarada do AVFrame.
  begin
    av_frame_move_ref(AccFrame, Frame);
    Result := av_frame_make_writable(AccFrame) >= 0;
    if not Result then
    begin
      Log('Export: av_frame_make_writable (mix) falhou.');
      av_frame_unref(AccFrame);
    end;
  end;

  function HandleMixPacket(ATrack: Integer): Boolean;
  // Decodifica o pacote corrente de uma faixa selecionada e acumula.
  // As faixas do NoOBS saem todas do mesmo encoder de audio do OBS, com
  // os mesmos timestamps e 1024 amostras por quadro — entao acumular por
  // pts identico basta, sem buffer de realinhamento.
  var
    R: Integer;
    Sec: Double;
  begin
    Result := True;
    R := avcodec_send_packet(Tracks[ATrack].DecCtx, Pkt);
    if R < 0 then Exit;   // pacote solto logo apos o seek: ignora
    while True do
    begin
      R := avcodec_receive_frame(Tracks[ATrack].DecCtx, Frame);
      if (R = AVERROR_EAGAIN) or (R = AVERROR_EOF) then Exit;
      if R < 0 then Exit(False);
      try
        if Frame.format <> AV_SAMPLE_FMT_FLTP then Continue;
        Sec := PtsToSec(Frame.pts, Tracks[ATrack].SrcTb);
        if (Sec < SegStartSec) or (Sec >= SegEndSec) then Continue;

        if AccFrame.nb_samples <= 0 then
        begin
          if not AdoptAccumulator then Exit(False);
        end
        else if AccFrame.pts = Frame.pts then
          MixInto(AccFrame, Frame)
        else
        begin
          // Quadro de outro instante: fecha o atual e recomeca.
          if not FlushMixFrame then Exit(False);
          if not AdoptAccumulator then Exit(False);
        end;
      finally
        av_frame_unref(Frame);
      end;
    end;
  end;

  function EncodeVideoFrame(APts: Int64): Boolean;
  var
    R: Integer;
  begin
    // Guarda de monotonicidade. So morde no SVT-AV1, cuja time_base e a
    // do FPS de saida (ver EncTb): se a origem tiver cadencia irregular,
    // dois quadros podem cair no MESMO tique e o encoder recusa pts
    // repetido — falha no meio de uma exportacao longa. Nos outros
    // encoders a time_base e a da origem e isto nunca dispara.
    if (LastEncPts <> Low(Int64)) and (APts <= LastEncPts) then
    begin
      if not PtsCollisionLogged then
      begin
        PtsCollisionLogged := True;
        Log('Export: pts repetido apos quantizar pro time_base do encoder; ' +
          'empurrando 1 tique (so este aviso).');
      end;
      APts := LastEncPts + 1;
    end;
    LastEncPts := APts;
    OutFrame.pts := APts;
    R := avcodec_send_frame(EncCtx, OutFrame);
    if R < 0 then
    begin
      Log('Export: avcodec_send_frame (video) falhou (%s).', [AvErrStr(R)]);
      Exit(False);
    end;
    Result := DrainEncoder(EncCtx, OutVStream, EncTb);
  end;

  function SetupScalers: Boolean;
  // Roda uma vez, no primeiro quadro util — so ai sabemos o pixel format
  // real que o decoder entrega.
  var
    k: Integer;
  begin
    Result := False;
    SrcFmt := Frame.format;
    NeedsNormalize := not ((SrcFmt = AV_PIX_FMT_YUV420P) or
                           (SrcFmt = AV_PIX_FMT_YUVJ420P));
    if NeedsNormalize then
    begin
      // Origem exotica (10 bits, NV12, RGB...): converte o quadro inteiro
      // pra YUV420P uma vez e recorta dali. As gravacoes do proprio app
      // caem sempre no caminho rapido e nunca passam por aqui.
      Log('Export: pix_fmt %d nao e YUV420P — normalizando cada quadro.',
        [SrcFmt]);
      // Bicubic fixo aqui de proposito: origem e destino tem o MESMO
      // tamanho, entao nao ha reamostragem e o algoritmo escolhido pelo
      // usuario nao mudaria nem a imagem nem o custo.
      NormSws := sws_getContext(Frame.width, Frame.height, SrcFmt,
        Frame.width, Frame.height, AV_PIX_FMT_YUV420P,
        SWS_BICUBIC, nil, nil, nil);
      if NormSws = nil then Exit;
      NormFrame := av_frame_alloc;
      if NormFrame = nil then Exit;
      NormFrame.format := AV_PIX_FMT_YUV420P;
      NormFrame.width  := Frame.width;
      NormFrame.height := Frame.height;
      if av_frame_get_buffer(NormFrame, 0) < 0 then Exit;
    end;

    // Um SwsContext por regiao, criado uma vez e reusado em todos os
    // quadros. A escala do TargetHeight ja esta embutida aqui — uma
    // passada so, sem canvas intermediario. O algoritmo e o escolhido pelo
    // usuario; so pesa quando ha reducao (numa regiao 1:1 nao ha
    // reamostragem e os tres custam o mesmo).
    for k := 0 to High(Regs) do
    begin
      Regs[k].Sws := sws_getContext(
        Regs[k].SrcW, Regs[k].SrcH, AV_PIX_FMT_YUV420P,
        Regs[k].DstW, Regs[k].DstH, AV_PIX_FMT_YUV420P,
        ScaleFlags, nil, nil, nil);
      if Regs[k].Sws = nil then
      begin
        Log('Export: sws_getContext falhou (%dx%d -> %dx%d).',
          [Regs[k].SrcW, Regs[k].SrcH, Regs[k].DstW, Regs[k].DstH]);
        Exit;
      end;
    end;
    Result := True;
  end;

  function ComposeAndEncode: Boolean;
  // Monta o quadro de saida a partir do decodificado e manda pro encoder.
  var
    k: Integer;
    Src: PAVFrame;
  begin
    Result := False;
    // O encoder pode reter referencia do frame que recebeu, entao NUNCA
    // reescreva OutFrame sem passar por aqui.
    if av_frame_make_writable(OutFrame) < 0 then
    begin
      Log('Export: av_frame_make_writable (video) falhou.');
      Exit;
    end;
    // So precisa pintar quando alguma regiao e mais baixa que o canvas —
    // as areas cobertas por blit sao sempre reescritas.
    if NeedsFill then FillFrameBlack(OutFrame, OutW, OutH);

    if NeedsNormalize then
    begin
      sws_scale(NormSws, @Frame.data[0], @Frame.linesize[0],
        0, Frame.height, @NormFrame.data[0], @NormFrame.linesize[0]);
      Src := NormFrame;
    end
    else
      Src := Frame;

    for k := 0 to High(Regs) do BlitRegion(Regs[k], Src, OutFrame);
    // Emenda na linha do tempo de saida: posicao dentro do trecho mais o
    // que ja foi escrito pelos trechos anteriores.
    // O pts do quadro esta na time_base da ORIGEM; o encoder espera na
    // dele. Quando as duas coincidem (todo encoder menos o SVT-AV1) o
    // av_rescale_q e no-op, entao um caminho so serve pros dois.
    Result := EncodeVideoFrame(
      av_rescale_q(Frame.pts - SegStartTs, VideoTb, EncTb) +
      SecToTs(OutOffsetSec, EncTb));
  end;

  procedure PumpDecoder;
  // Tira do decoder tudo o que ja esta pronto e manda pro caminho de
  // composicao/encode. Marca Failed / VideoDone nas variaveis externas.
  //
  // Existe como rotina separada porque roda em DOIS lugares: depois de
  // cada pacote de video e no DRENO do fim do trecho. Com threading em
  // quadros o decoder segura varios quadros dentro dele, entao sem o
  // segundo uso o fim de cada trecho sairia cortado.
  var
    R: Integer;
  begin
    while True do
    begin
      R := avcodec_receive_frame(DecCtx, Frame);
      if (R = AVERROR_EAGAIN) or (R = AVERROR_EOF) then Break;
      if R < 0 then
      begin
        Log('Export: avcodec_receive_frame falhou (%s).', [AvErrStr(R)]);
        Failed := True;
        Break;
      end;
      try
        // Quadros antes do inicio existem so como referencia do GOP —
        // decodifica e descarta.
        if (Frame.pts = AV_NOPTS_VALUE) or (Frame.pts < SegStartTs) then
          Continue;
        if Frame.pts >= SegEndTs then
        begin
          // O decoder entrega em ordem de APRESENTACAO, entao dali pra
          // frente todo pts e maior: nao ha quadro do trecho preso la
          // dentro e o dreno nem e preciso neste caminho.
          VideoDone := True;
          Break;
        end;
        // Reducao de taxa de quadros: so passam os quadros que caem na
        // cadencia pedida. A decisao vem ANTES de compor/escalar pra que
        // o quadro descartado custe zero. O relogio e o da SAIDA, entao a
        // cadencia atravessa a emenda dos trechos.
        if DropFrames then
        begin
          OutSec := OutOffsetSec +
                    (PtsToSec(Frame.pts, VideoTb) - SegStartSec);
          if OutSec + 1E-9 < NextKeepSec then Continue;
          repeat
            NextKeepSec := NextKeepSec + FrameInterval;
          until NextKeepSec > OutSec;
        end;
        if not SetupDone then
        begin
          if not SetupScalers then
          begin
            Failed := True;
            Break;
          end;
          SetupDone := True;
        end;
        if not ComposeAndEncode then
        begin
          Failed := True;
          Break;
        end;
        ReportProgress(PtsToSec(Frame.pts, VideoTb));
      finally
        av_frame_unref(Frame);
      end;
    end;
  end;

begin
  Result := erError;
  if not FFmpegLibAvailable then Exit;
  if (AOpts.SrcPath = '') or (AOpts.DstPath = '') then Exit;

  ScaleFlags := ResolveScaleFlags(AOpts.ScaleAlgo);
  SrcCtx := nil;
  OutCtx := nil;
  OutPb := nil;
  HeaderWritten := False;
  DecCtx := nil;
  EncCtx := nil;
  MixCtx := nil;
  EncPar := nil;
  Pkt := nil;
  EncPkt := nil;
  Frame := nil;
  NormFrame := nil;
  OutFrame := nil;
  AccFrame := nil;
  NormSws := nil;
  Regs := nil;
  Tracks := nil;
  OutVStream := nil;
  OutMixStream := nil;
  MixEncoder := nil;
  EncOpts := nil;
  LastPctStep := -1;
  Canceled := False;
  Failed := False;
  SetupDone := False;
  NeedsNormalize := False;
  SrcFmt := AV_PIX_FMT_NONE;
  OutOffsetSec := 0;
  TotalSec := 0;
  LastEncPts := Low(Int64);
  PtsCollisionLogged := False;
  // SegStartSec/SegEndSec/SegStartTs sao lidos pelas rotinas aninhadas
  // (HandleMixPacket, ComposeAndEncode) e precisam de valor mesmo antes do
  // 1o trecho. SegEndTs so e usado depois de atribuido no laco.
  SegStartSec := 0;
  SegEndSec := 0;
  SegStartTs := 0;
  MixTb.num := 1;
  MixTb.den := 48000;
  MixSrcTb := MixTb;

  try
    if not OpenInputWithRetry(SrcCtx, AOpts.SrcPath) then Exit;
    if avformat_find_stream_info(SrcCtx, nil) < 0 then Exit;
    NbStreams := av_format_context_nb_streams(SrcCtx);
    if NbStreams = 0 then Exit;

    // ---- stream de video ----
    VIdx := -1;
    for i := 0 to Integer(NbStreams) - 1 do
    begin
      S := GetStreamByIndex(SrcCtx, Cardinal(i));
      if (S <> nil) and (S.codecpar <> nil) and
         (S.codecpar.codec_type = AVMEDIA_TYPE_VIDEO) then
      begin
        VIdx := i;
        Break;
      end;
    end;
    if VIdx < 0 then
    begin
      Log('Export: arquivo sem stream de video.');
      Exit;
    end;
    VStream := GetStreamByIndex(SrcCtx, Cardinal(VIdx));
    VideoTb := VStream.time_base;
    if VideoTb.den <= 0 then Exit;

    Fps := 30;
    if (VStream.avg_frame_rate.den > 0) and (VStream.avg_frame_rate.num > 0) then
      Fps := Max(1, Round(VStream.avg_frame_rate.num /
                          VStream.avg_frame_rate.den));

    // Taxa de saida: nunca acima da origem (nao da pra inventar quadro).
    OutFps := AOpts.TargetFps;
    if (OutFps <= 0) or (OutFps > Fps) then OutFps := Fps;
    if OutFps < 1 then OutFps := 1;
    DropFrames := OutFps < Fps;
    FrameInterval := 1 / OutFps;
    NextKeepSec := 0;

    // ---- trechos ----
    TotalSec := 0;
    for i := 0 to High(AOpts.Segments) do
      if AOpts.Segments[i].EndSec > AOpts.Segments[i].StartSec then
        TotalSec := TotalSec + (AOpts.Segments[i].EndSec -
                                AOpts.Segments[i].StartSec);
    if TotalSec <= 0 then
    begin
      Log('Export: nenhum trecho valido pra exportar.');
      Exit;
    end;
    Log('Export: %d trecho(s), %.1fs de saida.',
      [Length(AOpts.Segments), TotalSec]);

    // ---- composicao ----
    if not BuildCompRegions(AOpts.Regions,
             VStream.codecpar.width, VStream.codecpar.height,
             AOpts.TargetHeight, Regs, OutW, OutH) then
    begin
      Log('Export: nao consegui montar o canvas de saida.');
      Exit;
    end;
    NeedsFill := False;
    for i := 0 to High(Regs) do
      if Regs[i].DstH < OutH then NeedsFill := True;
    Log('Export: %d regiao(oes) -> canvas %dx%d (origem %dx%d), escala=%s.',
      [Length(Regs), OutW, OutH,
       VStream.codecpar.width, VStream.codecpar.height,
       string(AOpts.ScaleAlgo)]);

    // ---- decoder de video ----
    Decoder := avcodec_find_decoder(VStream.codecpar.codec_id);
    if Decoder = nil then
    begin
      Log('Export: decoder nao encontrado (codec_id=%d).',
        [VStream.codecpar.codec_id]);
      Exit;
    end;
    DecCtx := avcodec_alloc_context3(Decoder);
    if DecCtx = nil then Exit;
    if avcodec_parameters_to_context(DecCtx, VStream.codecpar) < 0 then Exit;
    // O default do libavcodec pra 'threads' e 1 — NAO "automatico". Sem
    // esta linha o decode de um canvas 4K roda num nucleo so: a maquina
    // parece ociosa (1 de 16 nucleos = ~6% no gerenciador) e a exportacao
    // arrasta. 0 = auto (av_cpu_count). Medido: 173 -> 435 quadros/s.
    //
    // Ligar isto EXIGE o dreno do decoder no fim do trecho (mais abaixo):
    // com threading em quadros o decoder segura varios quadros dentro
    // dele, e sem o dreno o fim da exportacao sairia cortado.
    av_opt_set_int(DecCtx, 'threads', 0, 0);
    Rc := avcodec_open2(DecCtx, Decoder, nil);
    if Rc < 0 then
    begin
      Log('Export: avcodec_open2 (decoder) falhou (%s).', [AvErrStr(Rc)]);
      Exit;
    end;

    // ---- encoder de video ----
    Encoder := avcodec_find_encoder_by_name(PAnsiChar(AOpts.EncoderName));
    if Encoder = nil then
    begin
      Log('Export: encoder "%s" nao existe no avcodec.',
        [string(AOpts.EncoderName)]);
      Exit(erNoEncoder);
    end;
    // time_base do encoder = a do stream de video da origem. Assim os pts
    // passam sem reescala e o corte fica exato.
    EncTb := VideoTb;
    // ...EXCETO no SVT-AV1. O wrapper dele deriva a taxa de quadros do
    // TIME_BASE (nao do campo 'framerate', que setamos logo abaixo e ele
    // ignora) e valida contra um teto de 240 fps. Como o MKV do OBS tem
    // time_base 1/1000, o SVT lia "1000 fps" e o avcodec_open2 devolvia -22:
    //   Svt[error]: Instance 1: The maximum allowed frame rate is 240 fps
    // Nenhum outro encoder do build valida isso, por isso so ele muda de
    // regra — manter a time_base da origem nos demais preserva o pts exato.
    //
    // Consequencia conhecida: exportar ACIMA de 240 fps por SVT-AV1 falha
    // na abertura, e falha alto de proposito. Clampar a time_base em 1/240
    // faria os quadros chegarem mais rapido que os tiques, e a guarda de
    // monotonicidade os empurraria um a um — o video sairia em camera
    // lenta, silenciosamente. Melhor recusar do que entregar errado.
    if LowerCase(string(AOpts.EncoderName)) = 'libsvtav1' then
    begin
      EncTb.num := 1;
      EncTb.den := OutFps;
    end;
    // Dica de framerate = a taxa de SAIDA. Passar a da origem depois de
    // descartar quadros faria o encoder achar que tem mais quadros do que
    // tera, e o intervalo de keyframe sairia errado.
    FpsRat.num := OutFps;
    FpsRat.den := 1;

    Crf := AOpts.Crf;
    if Crf < EXPORT_CRF_MIN then Crf := EXPORT_CRF_MIN;
    if Crf > EXPORT_CRF_MAX then Crf := EXPORT_CRF_MAX;

    // ---- abre o encoder em QUALIDADE CONSTANTE (VBR) ----
    // Cada tentativa comeca de um contexto NOVO: um avcodec_open2 que
    // falha deixa o contexto meio-desmontado, e reaproveita-lo pra segunda
    // tentativa e pedir problema. Alocar de novo custa nada aqui.
    Attempt := 0;
    Rc := -1;
    while True do
    begin
      if EncCtx <> nil then avcodec_free_context(@EncCtx);
      EncCtx := avcodec_alloc_context3(Encoder);
      if EncCtx = nil then Exit;

      EncPar := avcodec_parameters_alloc;
      if EncPar = nil then Exit;
      try
        EncPar.codec_type := AVMEDIA_TYPE_VIDEO;
        EncPar.codec_id   := Encoder.id;
        EncPar.width      := OutW;
        EncPar.height     := OutH;
        EncPar.format     := AV_PIX_FMT_YUV420P;
        // Dimensoes NUNCA via av_opt_set_int('width') — a tabela de opcoes
        // do AVCodecContext nao expoe width/height (pegadinha #28).
        if avcodec_parameters_to_context(EncCtx, EncPar) < 0 then Exit;
      finally
        avcodec_parameters_free(PPointer(@EncPar));
      end;

      av_opt_set_q(EncCtx, 'time_base', EncTb, 0);
      av_opt_set_q(EncCtx, 'framerate', FpsRat, 0);
      // Encoder de SOFTWARE ('lib*' e a convencao do libavcodec pros que
      // rodam na CPU): idem ao decoder, o default e 1 thread. Medido no
      // x264: 35,5 -> 10,3 ms por quadro em 4K. Os de hardware nao usam
      // este campo — encodam na GPU e nao ganhariam nada.
      if Copy(LowerCase(string(AOpts.EncoderName)), 1, 3) = 'lib' then
        av_opt_set_int(EncCtx, 'threads', 0, 0);
      // MP4 exige o extradata (SPS/PPS) no cabecalho do container. Sem
      // esta flag o arquivo sai sem eles e nao toca em lugar nenhum.
      av_opt_set(EncCtx, 'flags', '+global_header', 0);

      EncOpts := nil;
      if not ApplyQualityOptions(EncOpts, string(AOpts.EncoderName),
                                 Crf, Attempt) then
      begin
        av_dict_free(@EncOpts);
        Break;    // acabaram as formas de pedir qualidade constante
      end;
      av_dict_set(@EncOpts, 'g',
        PAnsiChar(AnsiString(IntToStr(OutFps * OUT_KEYINT_SEC))), 0);

      Rc := avcodec_open2(EncCtx, Encoder, @EncOpts);
      LogLeftoverOptions(EncOpts, string(AOpts.EncoderName));
      av_dict_free(@EncOpts);
      if Rc >= 0 then Break;

      Log('Export: avcodec_open2 ("%s") falhou na tentativa %d (%s).',
        [string(AOpts.EncoderName), Attempt, AvErrStr(Rc)]);
      Inc(Attempt);
    end;
    if Rc < 0 then Exit(erNoEncoder);

    // "qualidade constante" sem prometer VBR: o AMF vai por CQP (QP fixo),
    // nao por um VBR guiado por qualidade — ver ApplyQualityOptions.
    Log('Export: encoder=%s %dx%d @%dfps (origem %dfps) crf=%d ' +
      '(qualidade constante, tentativa %d)',
      [string(AOpts.EncoderName), OutW, OutH, OutFps, Fps, Crf, Attempt]);

    // ---- faixas de audio selecionadas ----
    for i := 0 to High(AOpts.AudioStreams) do
    begin
      j := AOpts.AudioStreams[i];
      if (j < 0) or (Cardinal(j) >= NbStreams) then Continue;
      S := GetStreamByIndex(SrcCtx, Cardinal(j));
      if (S = nil) or (S.codecpar = nil) or
         (S.codecpar.codec_type <> AVMEDIA_TYPE_AUDIO) then Continue;
      SetLength(Tracks, Length(Tracks) + 1);
      Tracks[High(Tracks)].SrcIdx := j;
      Tracks[High(Tracks)].OutIdx := -1;
      Tracks[High(Tracks)].SrcTb := S.time_base;
      Tracks[High(Tracks)].DecCtx := nil;
      Tracks[High(Tracks)].Done := False;
    end;
    // Mixar uma faixa so seria reencode a toa — degrada pra copia.
    DoMix := AOpts.MixAudio and (Length(Tracks) > 1);

    // ---- output ----
    // Muxer SEMPRE explicito: deduzir do nome do arquivo quebraria com o
    // ".part" temporario que o chamador usa enquanto escreve.
    Container := AOpts.Container;
    if Container = '' then Container := AnsiString('mp4');
    Rc := avformat_alloc_output_context2(@OutCtx, nil, PAnsiChar(Container),
      PAnsiChar(ToUtf8(AOpts.DstPath)));
    if (Rc < 0) or (OutCtx = nil) then
    begin
      Log('Export: avformat_alloc_output_context2 falhou (%s).', [AvErrStr(Rc)]);
      Exit;
    end;

    OutVStream := avformat_new_stream(OutCtx, nil);
    if OutVStream = nil then Exit;
    if avcodec_parameters_from_context(OutVStream.codecpar, EncCtx) < 0 then Exit;
    OutVStream.time_base := EncTb;

    if DoMix then
    begin
      MixEncoder := avcodec_find_encoder_by_name('aac');
      if MixEncoder = nil then
      begin
        Log('Export: encoder aac ausente — mixagem desligada.');
        DoMix := False;
      end;
    end;

    if DoMix then
    begin
      S := GetStreamByIndex(SrcCtx, Cardinal(Tracks[0].SrcIdx));
      MixSrcTb := Tracks[0].SrcTb;
      MixTb.num := 1;
      MixTb.den := S.codecpar.sample_rate;
      if MixTb.den <= 0 then MixTb.den := 48000;

      MixCtx := avcodec_alloc_context3(MixEncoder);
      if MixCtx = nil then Exit;
      EncPar := avcodec_parameters_alloc;
      if EncPar = nil then Exit;
      try
        EncPar.codec_type  := AVMEDIA_TYPE_AUDIO;
        EncPar.codec_id    := MixEncoder.id;
        EncPar.format      := AV_SAMPLE_FMT_FLTP;
        EncPar.sample_rate := MixTb.den;
        EncPar.bit_rate    := MIX_BITRATE;
        // ch_layout copiado do source. As faixas do OBS sao sempre ordem
        // nativa (mono/estereo), entao copiar o record e seguro — em
        // ordem CUSTOM o campo `u` seria ponteiro e viraria alias.
        EncPar.ch_layout   := S.codecpar.ch_layout;
        if avcodec_parameters_to_context(MixCtx, EncPar) < 0 then Exit;
      finally
        avcodec_parameters_free(PPointer(@EncPar));
      end;
      av_opt_set_q(MixCtx, 'time_base', MixTb, 0);
      av_opt_set(MixCtx, 'flags', '+global_header', 0);
      Rc := avcodec_open2(MixCtx, MixEncoder, nil);
      if Rc < 0 then
      begin
        Log('Export: avcodec_open2 (aac) falhou (%s) — mixagem desligada.',
          [AvErrStr(Rc)]);
        DoMix := False;
      end
      else
      begin
        OutMixStream := avformat_new_stream(OutCtx, nil);
        if OutMixStream = nil then Exit;
        if avcodec_parameters_from_context(OutMixStream.codecpar, MixCtx) < 0 then
          Exit;
        OutMixStream.time_base := MixTb;
        // Um decoder por faixa selecionada.
        for i := 0 to High(Tracks) do
        begin
          S := GetStreamByIndex(SrcCtx, Cardinal(Tracks[i].SrcIdx));
          Decoder := avcodec_find_decoder(S.codecpar.codec_id);
          if Decoder = nil then Continue;
          Tracks[i].DecCtx := avcodec_alloc_context3(Decoder);
          if Tracks[i].DecCtx = nil then Continue;
          if (avcodec_parameters_to_context(Tracks[i].DecCtx, S.codecpar) < 0) or
             (avcodec_open2(Tracks[i].DecCtx, Decoder, nil) < 0) then
          begin
            avcodec_free_context(@Tracks[i].DecCtx);
            Tracks[i].DecCtx := nil;
          end;
        end;
      end;
    end;

    if not DoMix then
    begin
      // Stream copy: um stream de saida por faixa escolhida.
      for i := 0 to High(Tracks) do
      begin
        S := GetStreamByIndex(SrcCtx, Cardinal(Tracks[i].SrcIdx));
        OutAStream := avformat_new_stream(OutCtx, nil);
        if OutAStream = nil then Exit;
        if avcodec_parameters_copy(OutAStream.codecpar, S.codecpar) < 0 then Exit;
        OutAStream.codecpar.codec_tag := 0;
        OutAStream.time_base := S.time_base;
        CopyStreamTag(S, OutAStream, 'title');
        CopyStreamTag(S, OutAStream, 'language');
        Tracks[i].OutIdx := OutAStream.index;
      end;
    end;

    Rc := avio_open2(@OutPb, PAnsiChar(ToUtf8(AOpts.DstPath)),
      AVIO_FLAG_WRITE, nil, nil);
    if Rc < 0 then
    begin
      Log('Export: avio_open2 falhou (%s).', [AvErrStr(Rc)]);
      Exit;
    end;
    av_format_context_set_pb(OutCtx, OutPb);

    // faststart move o moov pro inicio — arquivo pronto pra compartilhar.
    // So existe no muxer MP4/MOV; no Matroska o indice ja vai no lugar.
    if Container = 'mp4' then
      av_dict_set(@EncOpts, 'movflags', '+faststart', 0);
    Rc := avformat_write_header(OutCtx, @EncOpts);
    av_dict_free(@EncOpts);
    if Rc < 0 then
    begin
      Log('Export: avformat_write_header falhou (%s).', [AvErrStr(Rc)]);
      Exit;
    end;
    HeaderWritten := True;

    // ---- objetos de trabalho ----
    Pkt := av_packet_alloc;
    EncPkt := av_packet_alloc;
    Frame := av_frame_alloc;
    OutFrame := av_frame_alloc;
    AccFrame := av_frame_alloc;
    if (Pkt = nil) or (EncPkt = nil) or (Frame = nil) or
       (OutFrame = nil) or (AccFrame = nil) then Exit;

    OutFrame.format := AV_PIX_FMT_YUV420P;
    OutFrame.width  := OutW;
    OutFrame.height := OutH;
    Rc := av_frame_get_buffer(OutFrame, 0);
    if Rc < 0 then
    begin
      Log('Export: av_frame_get_buffer falhou (%s).', [AvErrStr(Rc)]);
      Exit;
    end;

    // ---- um passe por trecho, emendando na saida ----
    OutOffsetSec := 0;
    for SegIdx := 0 to High(AOpts.Segments) do
    begin
    SegStartSec := AOpts.Segments[SegIdx].StartSec;
    SegEndSec   := AOpts.Segments[SegIdx].EndSec;
    if SegEndSec <= SegStartSec then Continue;
    SegStartTs := SecToTs(SegStartSec, VideoTb);
    SegEndTs   := SecToTs(SegEndSec, VideoTb);

    // Posiciona no keyframe anterior ao inicio do trecho. O
    // avcodec_flush_buffers e obrigatorio: sem ele o decoder tentaria
    // continuar a partir de referencias que nao valem mais aqui.
    if SegStartTs > 0 then
      av_seek_frame(SrcCtx, VIdx, SegStartTs, AVSEEK_FLAG_BACKWARD)
    else
      av_seek_frame(SrcCtx, -1, 0, AVSEEK_FLAG_BACKWARD);
    avcodec_flush_buffers(DecCtx);
    for i := 0 to High(Tracks) do
    begin
      if Tracks[i].DecCtx <> nil then avcodec_flush_buffers(Tracks[i].DecCtx);
      Tracks[i].Done := False;
    end;

    VideoDone := False;
    while av_read_frame(SrcCtx, Pkt) = 0 do
    begin
      if IsCanceled(ACancelFlag) then
      begin
        av_packet_unref(Pkt);
        Canceled := True;
        Break;
      end;

      // Termina assim que o video E todas as faixas passaram do fim do
      // trecho. Checado no TOPO de proposito: os ramos abaixo usam
      // Continue, que pularia uma checagem no rodape — e num arquivo so
      // de video isso faria a leitura ir ate o EOF a toa.
      if VideoDone then
      begin
        AllDone := True;
        for i := 0 to High(Tracks) do
          if not Tracks[i].Done then AllDone := False;
        if AllDone then
        begin
          av_packet_unref(Pkt);
          Break;
        end;
      end;

      try
        if Pkt.stream_index = VIdx then
        begin
          if VideoDone then Continue;
          Rc := avcodec_send_packet(DecCtx, Pkt);
          if Rc < 0 then Continue;
          PumpDecoder;
          if Failed then Break;
        end
        else
        begin
          // Audio.
          for i := 0 to High(Tracks) do
          begin
            if Tracks[i].SrcIdx <> Pkt.stream_index then Continue;
            if Tracks[i].Done then Break;
            if (Pkt.pts <> AV_NOPTS_VALUE) and
               (PtsToSec(Pkt.pts, Tracks[i].SrcTb) >= SegEndSec) then
            begin
              Tracks[i].Done := True;
              Break;
            end;
            if (Pkt.pts <> AV_NOPTS_VALUE) and
               (PtsToSec(Pkt.pts, Tracks[i].SrcTb) < SegStartSec) then Break;

            if DoMix then
            begin
              if (Tracks[i].DecCtx <> nil) and (not HandleMixPacket(i)) then
                Failed := True;
            end
            else
            begin
              S := GetStreamByIndex(OutCtx, Cardinal(Tracks[i].OutIdx));
              if S = nil then Break;
              // Mesmo deslocamento temporal do video — preserva o sync e
              // emenda na linha do tempo de saida.
              if Pkt.pts <> AV_NOPTS_VALUE then
                Pkt.pts := Max(Int64(0),
                  Pkt.pts - SecToTs(SegStartSec, Tracks[i].SrcTb) +
                  SecToTs(OutOffsetSec, Tracks[i].SrcTb));
              if Pkt.dts <> AV_NOPTS_VALUE then
                Pkt.dts := Max(Int64(0),
                  Pkt.dts - SecToTs(SegStartSec, Tracks[i].SrcTb) +
                  SecToTs(OutOffsetSec, Tracks[i].SrcTb));
              Pkt.stream_index := S.index;
              av_packet_rescale_ts(Pkt, Tracks[i].SrcTb, S.time_base);
              Pkt.pos := -1;
              Rc := av_interleaved_write_frame(OutCtx, Pkt);
              if Rc < 0 then
              begin
                Log('Export: write (audio copy) falhou (%s).', [AvErrStr(Rc)]);
                Failed := True;
              end;
            end;
            Break;
          end;
          if Failed then Break;
        end;
      finally
        av_packet_unref(Pkt);
      end;
    end;

    if Canceled or Failed then Break;

    // Dreno do decoder. So faz falta quando o trecho terminou por EOF do
    // arquivo (nao por termos VISTO um quadro alem do fim): ai os ultimos
    // quadros ainda estao dentro do decoder, e com threading em quadros
    // sao varios — meio segundo de video sumindo do fim, calado.
    //
    // Depois de um send(nil) o decoder fica em modo dreno; quem o devolve
    // pra vida e o avcodec_flush_buffers no topo do proximo trecho.
    if not VideoDone then
    begin
      avcodec_send_packet(DecCtx, nil);
      PumpDecoder;
      if Failed then Break;
    end;

    // Fecha o acumulador de audio na borda do trecho: o proximo comeca
    // noutro ponto do original e nao pode somar por cima deste quadro.
    if DoMix and (not FlushMixFrame) then
    begin
      Failed := True;
      Break;
    end;

    OutOffsetSec := OutOffsetSec + (SegEndSec - SegStartSec);
    end;  // for SegIdx

    if Canceled then Exit(erCanceled);
    if Failed then Exit(erError);

    // ---- flush dos encoders ----
    // O acumulador de audio ja foi fechado na borda de cada trecho.
    avcodec_send_frame(EncCtx, nil);
    DrainEncoder(EncCtx, OutVStream, EncTb);
    if DoMix then
    begin
      avcodec_send_frame(MixCtx, nil);
      DrainEncoder(MixCtx, OutMixStream, MixTb);
    end;

    Rc := av_write_trailer(OutCtx);
    HeaderWritten := False;
    if Rc < 0 then
    begin
      Log('Export: av_write_trailer falhou (%s).', [AvErrStr(Rc)]);
      Exit(erError);
    end;

    Log('Export: concluido — %s (%.1fs, %dx%d).',
      [System.SysUtils.ExtractFileName(AOpts.DstPath), TotalSec, OutW, OutH]);
    if Assigned(AProgress) then AProgress(100);
    Result := erOk;
  finally
    for i := 0 to High(Regs) do
      if Regs[i].Sws <> nil then
        try sws_freeContext(Regs[i].Sws); except end;
    if NormSws <> nil then try sws_freeContext(NormSws); except end;
    if AccFrame <> nil then av_frame_free(@AccFrame);
    if OutFrame <> nil then av_frame_free(@OutFrame);
    if NormFrame <> nil then av_frame_free(@NormFrame);
    if Frame <> nil then av_frame_free(@Frame);
    if EncPkt <> nil then av_packet_free(@EncPkt);
    if Pkt <> nil then av_packet_free(@Pkt);
    for i := 0 to High(Tracks) do
      if Tracks[i].DecCtx <> nil then
        try avcodec_free_context(@Tracks[i].DecCtx); except end;
    if MixCtx <> nil then try avcodec_free_context(@MixCtx); except end;
    if EncCtx <> nil then try avcodec_free_context(@EncCtx); except end;
    if DecCtx <> nil then try avcodec_free_context(@DecCtx); except end;
    if OutCtx <> nil then
    begin
      // Se saimos no meio (erro/cancelamento) o trailer nao foi escrito;
      // o arquivo parcial e apagado por quem chamou.
      if HeaderWritten then
        try av_write_trailer(OutCtx); except end;
      if OutPb <> nil then try avio_closep(@OutPb); except end;
      try avformat_free_context(OutCtx); except end;
    end;
    if SrcCtx <> nil then avformat_close_input(@SrcCtx);
  end;
end;

end.

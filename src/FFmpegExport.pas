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
      numa faixa so;
    - exportar SO o audio (NoVideo), sem decodificar nem encodar video;
    - queimar a transcricao como legenda nos quadros (ExportCaptions).

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
  NoOBSTypes,
  ExportCaptions;

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
    // Taxa de quadros final. 0 = a maior possivel. Nunca passa da taxa da
    // origem VEZES A VELOCIDADE (nao ha como inventar quadro; acelerado,
    // cabem mais quadros da origem em cada segundo da saida), nem de
    // EXPORT_FPS_MAX. Quadros fora da cadencia sao descartados ANTES de
    // compor/escalar, entao cada um economiza o trabalho todo.
    TargetFps: Integer;
    // Velocidade da saida: 1 = normal, 2 = o dobro (metade da duracao),
    // ate EXPORT_SPEED_MAX. Fracao vale (1,5). 0 ou abaixo de 1 = 1. Todo
    // quadro da origem e decodificado — nada de pular por keyframe —, e a
    // cadencia da saida escolhe os que entram. O audio e acelerado junto,
    // mantendo o tom (TTimeStretch), e por isso deixa de ser copiado:
    // toda faixa escolhida e recodificada.
    Speed: Double;
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
    // Titulo da faixa misturada (vira o nome dela no player e em editores
    // externos). Vazio = sem titulo.
    MixTitle: string;
    // Encoder de audio da saida, no nome do libavcodec ('libmp3lame').
    // Vazio = o de sempre: copia as faixas, ou AAC quando mistura. Com ele
    // preenchido o audio e SEMPRE recodificado (uma faixa so tambem), porque
    // o container pede outro codec — o caso do .mp3 na exportacao so de audio.
    AudioCodec: AnsiString;
    // Taxa do audio recodificado, em bits/s. 0 = MIX_BITRATE.
    AudioBitrate: Integer;
    // So o audio: nenhum stream de video na saida, e o video da origem nem
    // e decodificado. Regioes, resolucao, fps, encoder e qualidade sao
    // ignorados. Origem sem video (uma exportacao so de audio reexportada)
    // cai aqui sozinha.
    NoVideo: Boolean;
    // Legenda queimada: turnos da transcricao no relogio da ORIGEM. Vazio =
    // sem legenda. Ignorado com NoVideo.
    Captions: TCaptionTurnArray;
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

  // Velocidade maxima da exportacao acelerada.
  EXPORT_SPEED_MAX = 512;
  // Teto da taxa de quadros de saida. O MKV guarda o tempo em
  // MILISSEGUNDOS: acima de 1000 fps dois quadros cairiam no mesmo
  // instante. Os encoders de hardware e o x264 aceitam mais (medido ate
  // 2000), mas nenhuma tela mostra isso. O SVT-AV1 para em 240 (#51g).
  EXPORT_FPS_MAX     = 1000;
  EXPORT_FPS_MAX_SVT = 240;

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

// Layout de monitores do ARQUIVO EXPORTADO. Cada regiao do layout original
// que aparece na saida vira uma regiao nova nas coordenadas da saida:
// recortada pelo que entrou (monitor escolhido ou recorte livre), deslocada
// pra onde a composicao a pos e escalada pela reducao de resolucao. E o que
// deixa o player do arquivo exportado continuar oferecendo "ver so o
// monitor X". Mesma composicao do ExportVideo (BuildCompRegions), entao as
// duas contas nunca divergem. False = nao ha o que calcular.
function ComputeExportLayout(const ASrc: TRecordingLayout;
  const ARegions: TRecordingRegionArray; ATargetHeight: Integer;
  out ALayout: TRecordingLayout): Boolean;

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
    DecCtx: PAVCodecContext;  // no modo mix e no acelerado
    Done: Boolean;            // ja passou do fim do trecho
    AOut: Integer;            // acelerado sem mix: indice em AOuts
  end;
  TAudioTrackArray = TArray<TAudioTrack>;

  // Acelera o audio SEM mudar o tom (WSOLA: sobreposicao de janelas com
  // busca de forma de onda). Simplesmente reamostrar deixaria a voz fina
  // (2x = uma oitava acima); o que se espera de um video acelerado e o
  // que os players fazem: mesma voz, falando mais rapido.
  //
  // Como funciona: a saida e montada com janelas de Hann de FW amostras,
  // uma a cada FHo (metade da janela — com Hann a soma das sobreposicoes
  // da exatamente 1). Na entrada, cada janela e lida FHi = FHo x
  // velocidade depois da anterior. Tirar a janela exatamente ali emendaria
  // ondas fora de fase (chiado, "eco metalico"); por isso a leitura pode
  // escorregar ate FTol pra cada lado, pro ponto que mais se parece com a
  // continuacao natural do trecho anterior (correlacao normalizada, grossa
  // de 8 em 8 amostras e depois fina em volta da melhor).
  //
  // Em velocidade alta (100x, 512x) o resultado e "pescar" 40 ms a cada
  // poucos segundos — o som de avancar uma fita, que e o esperado.
  //
  // Planar float (FLTP, o que o AAC decodifica). Um objeto por saida; um
  // trecho do video por vez — Finish fecha o trecho com a contagem exata
  // de amostras e reinicia.
  TTimeStretch = class
  private
    FCh, FW, FHo, FTol: Integer;
    FHi: Double;
    FIn: TArray<TArray<Single>>;     // entrada guardada, por canal
    FInBase, FInEnd: Int64;          // faixa absoluta guardada: [Base, End)
    FKeepFrom: Int64;                // antes disto nada mais sera lido
    FAcc: TArray<TArray<Single>>;    // sobreposicao em montagem (FW)
    FWin: TArray<Single>;
    FTail: TArray<Single>;           // continuacao natural (mono, FHo)
    FHasTail: Boolean;
    FK: Int64;                       // janelas ja montadas no trecho
    FMade: Int64;                    // amostras produzidas no trecho
    function Sample(ACh: Integer; APos: Int64): Single; inline;
    function Mono(APos: Int64): Single; inline;
    function BestStart(ANominal: Int64): Int64;
    procedure Step(AStart: Int64);
    procedure Restart;
  public
    // Saida pronta, por canal: OutN amostras. Quem consome zera OutN.
    Outp: TArray<TArray<Single>>;
    OutN: Integer;
    constructor Create(AChannels, ASampleRate: Integer; ASpeed: Double);
    // APlanes = um ponteiro por canal; nil = silencio.
    procedure Put(APlanes: PPointer; N: Integer);
    // Monta tudo o que a entrada recebida ja permite.
    procedure Process;
    // Fecha o trecho: completa com silencio e corta pra que saiam
    // EXATAMENTE AMore amostras alem das ja produzidas — e o que mantem o
    // audio grudado no video emenda apos emenda. Depois, comeca do zero.
    procedure Finish(AMore: Int64);
  end;

  // Uma saida de audio RECODIFICADA na exportacao acelerada: a mistura, ou
  // uma por faixa escolhida. A fila refaz os quadros no tamanho que o
  // encoder exige (o esticador entrega pedacos de qualquer tamanho).
  TAudioOut = record
    Enc: PAVCodecContext;
    Stream: PAVStream;
    Tb: AVRational;                  // 1/taxa de amostragem
    FrameSize: Integer;
    Ch: Integer;
    Tpl: PAVFrame;                   // modelo do ch_layout (1o quadro)
    Buf: TArray<TArray<Single>>;
    N: Integer;
    NextPts: Int64;                  // em Tb — contagem continua
    Stretch: TTimeStretch;
    OwnsEnc: Boolean;                // False = e o MixCtx (liberado a parte)
  end;
  TAudioOutArray = TArray<TAudioOut>;
  PAudioOut = ^TAudioOut;

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
// TTimeStretch
// =====================================================================

constructor TTimeStretch.Create(AChannels, ASampleRate: Integer;
  ASpeed: Double);
var
  i: Integer;
begin
  inherited Create;
  FCh := Max(1, AChannels);
  // Janela de 40 ms, passo de 20 ms, busca de +-10 ms: o meio-termo
  // classico pra voz (janela curta corta silaba; longa vira eco).
  FHo := Max(64, Round(ASampleRate * 0.020));
  FW := FHo * 2;
  FTol := Max(8, Round(ASampleRate * 0.010));
  FHi := FHo * ASpeed;
  SetLength(FWin, FW);
  // Hann PERIODICA: com passo de meia janela as sobreposicoes somam 1.
  for i := 0 to FW - 1 do
    FWin[i] := 0.5 - 0.5 * Cos(2 * Pi * i / FW);
  SetLength(FIn, FCh);
  SetLength(FAcc, FCh);
  SetLength(Outp, FCh);
  for i := 0 to FCh - 1 do
    SetLength(FAcc[i], FW);
  SetLength(FTail, FHo);
  Restart;
end;

procedure TTimeStretch.Restart;
var
  ch: Integer;
begin
  FInBase := 0;
  FInEnd := 0;
  FKeepFrom := -FTol;
  FHasTail := False;
  FK := 0;
  FMade := 0;
  for ch := 0 to FCh - 1 do
    FillChar(FAcc[ch][0], FW * SizeOf(Single), 0);
end;

function TTimeStretch.Sample(ACh: Integer; APos: Int64): Single;
begin
  if (APos < FInBase) or (APos >= FInEnd) then Result := 0
  else Result := FIn[ACh][APos - FInBase];
end;

function TTimeStretch.Mono(APos: Int64): Single;
var
  ch: Integer;
begin
  Result := 0;
  if (APos < FInBase) or (APos >= FInEnd) then Exit;
  for ch := 0 to FCh - 1 do
    Result := Result + FIn[ch][APos - FInBase];
end;

procedure TTimeStretch.Put(APlanes: PPointer; N: Integer);
var
  OldEnd, NewBase: Int64;
  Keep, Skip, Len, ch, Dst: Integer;
  Src: PSingle;
begin
  if N <= 0 then Exit;
  OldEnd := FInEnd;
  FInEnd := FInEnd + N;
  // Descarta a frente que nenhuma janela vai mais ler. Em velocidade alta
  // quase toda a entrada cai aqui: so o que esta perto da proxima leitura
  // e guardado.
  NewBase := Max(FInBase, Min(FKeepFrom, FInEnd));
  if NewBase > FInBase then
  begin
    Keep := Max(Int64(0), OldEnd - NewBase);
    if Keep > 0 then
      for ch := 0 to FCh - 1 do
        Move(FIn[ch][NewBase - FInBase], FIn[ch][0], Keep * SizeOf(Single));
    FInBase := NewBase;
  end;
  Skip := Max(Int64(0), FInBase - OldEnd);
  if Skip >= N then Exit;
  Len := FInEnd - FInBase;
  Dst := OldEnd + Skip - FInBase;
  for ch := 0 to FCh - 1 do
  begin
    if Length(FIn[ch]) < Len then SetLength(FIn[ch], Max(4096, Len * 2));
    if APlanes = nil then
      FillChar(FIn[ch][Dst], (N - Skip) * SizeOf(Single), 0)
    else
    begin
      Src := PSingle(PPointer(NativeUInt(APlanes) + NativeUInt(ch) * SizeOf(Pointer))^);
      if Src = nil then
        FillChar(FIn[ch][Dst], (N - Skip) * SizeOf(Single), 0)
      else
        Move(PSingle(NativeUInt(Src) + NativeUInt(Skip) * SizeOf(Single))^,
          FIn[ch][Dst], (N - Skip) * SizeOf(Single));
    end;
  end;
end;

function TTimeStretch.BestStart(ANominal: Int64): Int64;
const
  COARSE = 8;   // passo da busca grossa
var
  c, Lo, Hi, Best: Int64;
  i: Integer;
  Corr, En, Score, BestScore, v: Double;

  function Eval(AStart: Int64; AStep: Integer): Double;
  var
    k: Integer;
  begin
    Corr := 0;
    En := 0;
    k := 0;
    while k < FHo do
    begin
      v := Mono(AStart + k);
      Corr := Corr + v * FTail[k];
      En := En + v * v;
      Inc(k, AStep);
    end;
    if En <= 1E-12 then Result := 0
    else Result := Corr / Sqrt(En);
  end;

begin
  Result := ANominal;
  if not FHasTail then Exit;
  Lo := ANominal - FTol;
  Hi := ANominal + FTol;
  Best := ANominal;
  BestScore := -1E300;
  c := Lo;
  while c <= Hi do
  begin
    Score := Eval(c, COARSE);
    if Score > BestScore then begin BestScore := Score; Best := c; end;
    Inc(c, COARSE);
  end;
  // Refina em volta da melhor, amostra a amostra (de 2 em 2 no produto).
  c := Best;
  BestScore := -1E300;
  for i := -COARSE to COARSE do
  begin
    if (c + i < Lo) or (c + i > Hi) then Continue;
    Score := Eval(c + i, 2);
    if Score > BestScore then begin BestScore := Score; Result := c + i; end;
  end;
end;

procedure TTimeStretch.Step(AStart: Int64);
var
  ch, i, Need: Integer;
begin
  for ch := 0 to FCh - 1 do
    for i := 0 to FW - 1 do
      FAcc[ch][i] := FAcc[ch][i] + FWin[i] * Sample(ch, AStart + i);
  // A primeira metade nao recebe mais nada: sai pronta.
  Need := OutN + FHo;
  for ch := 0 to FCh - 1 do
  begin
    if Length(Outp[ch]) < Need then SetLength(Outp[ch], Max(4096, Need * 2));
    Move(FAcc[ch][0], Outp[ch][OutN], FHo * SizeOf(Single));
    Move(FAcc[ch][FHo], FAcc[ch][0], (FW - FHo) * SizeOf(Single));
    FillChar(FAcc[ch][FW - FHo], FHo * SizeOf(Single), 0);
  end;
  Inc(OutN, FHo);
  Inc(FMade, FHo);
  // Continuacao natural: o que viria logo depois da metade que acabou de
  // sair. A proxima janela procura o trecho que mais se parece com isto.
  for i := 0 to FHo - 1 do
    FTail[i] := Mono(AStart + FHo + i);
  FHasTail := True;
  Inc(FK);
  FKeepFrom := Round(FK * FHi) - FTol;
end;

procedure TTimeStretch.Process;
var
  A: Int64;
begin
  while True do
  begin
    A := Round(FK * FHi);
    if A + FTol + FW > FInEnd then Break;   // falta entrada
    Step(BestStart(A));
  end;
end;

procedure TTimeStretch.Finish(AMore: Int64);
var
  Need, ATarget: Int64;
  Extra, ch: Integer;
begin
  // OutN pendente conta como ja produzido (FMade inclui).
  ATarget := FMade + Max(Int64(0), AMore);
  // Silencio ate produzir o bastante: e ele que fecha a ultima janela com
  // a descida da Hann, sem estalo.
  while FMade < ATarget do
  begin
    Need := Round(FK * FHi) + FTol + FW - FInEnd;
    if Need > 0 then
    begin
      // Silencio que cairia antes da proxima leitura nem precisa existir.
      // Tudo o que estava guardado tambem fica antes dela: sai junto.
      if FInEnd + Need <= FKeepFrom then
      begin
        FInEnd := FInEnd + Need;
        FInBase := FInEnd;
      end
      else
        Put(nil, Integer(Min(Need, Int64(High(Integer) div 8))));
    end;
    Process;
  end;
  // Produziu demais (a ultima janela passou do alvo): o excesso esta no
  // fim da saida pronta.
  Extra := Integer(Min(Int64(OutN), FMade - ATarget));
  if Extra > 0 then Dec(OutN, Extra);
  for ch := 0 to FCh - 1 do
    FillChar(FAcc[ch][0], FW * SizeOf(Single), 0);
  Restart;
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

function ComputeExportLayout(const ASrc: TRecordingLayout;
  const ARegions: TRecordingRegionArray; ATargetHeight: Integer;
  out ALayout: TRecordingLayout): Boolean;
var
  Regs: TCompRegionArray;
  OutW, OutH, i, k, ix, iy, ix2, iy2: Integer;
  Sx, Sy: Double;
  R: TRecordingRegion;
begin
  ALayout := Default(TRecordingLayout);
  Result := False;
  if (ASrc.CanvasW <= 0) or (ASrc.CanvasH <= 0) then Exit;
  if not BuildCompRegions(ARegions, ASrc.CanvasW, ASrc.CanvasH, ATargetHeight,
           Regs, OutW, OutH) then Exit;
  ALayout.CanvasW := OutW;
  ALayout.CanvasH := OutH;
  for i := 0 to High(Regs) do
  begin
    if (Regs[i].SrcW <= 0) or (Regs[i].SrcH <= 0) then Continue;
    Sx := Regs[i].DstW / Regs[i].SrcW;
    Sy := Regs[i].DstH / Regs[i].SrcH;
    for k := 0 to High(ASrc.Regions) do
    begin
      // Intersecao do monitor original com o pedaco que entrou.
      ix := Max(ASrc.Regions[k].X, Regs[i].SrcX);
      iy := Max(ASrc.Regions[k].Y, Regs[i].SrcY);
      ix2 := Min(ASrc.Regions[k].X + ASrc.Regions[k].W, Regs[i].SrcX + Regs[i].SrcW);
      iy2 := Min(ASrc.Regions[k].Y + ASrc.Regions[k].H, Regs[i].SrcY + Regs[i].SrcH);
      // Sobra de poucos pixels (arredondamento par nas bordas) nao e monitor.
      if (ix2 - ix < 8) or (iy2 - iy < 8) then Continue;
      R := ASrc.Regions[k];
      R.X := Regs[i].DstX + Round((ix - Regs[i].SrcX) * Sx);
      R.Y := Regs[i].DstY + Round((iy - Regs[i].SrcY) * Sy);
      R.W := Round((ix2 - ix) * Sx);
      R.H := Round((iy2 - iy) * Sy);
      ALayout.Regions := ALayout.Regions + [R];
    end;
  end;
  Result := True;
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
// Destino em planar YUV 4:2:0 8 bits; origem nele ou em NV12 (o chamador
// garante, ver NeedsNormalize em ExportVideo, e o SwsContext da regiao foi
// criado com o mesmo formato). Croma tem metade da resolucao nos dois
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
  if ASrc.format = AV_PIX_FMT_NV12 then
  begin
    // NV12 (decode na placa): um plano so de croma, U e V intercalados.
    // Cada amostra de croma ocupa 2 bytes e cobre 2 pixels, entao o
    // deslocamento em bytes e o proprio SrcX (que e par).
    SrcData[1] := PByte(NativeUInt(ASrc.data[1]) +
      NativeUInt(AReg.SrcY div 2) * NativeUInt(SrcLs[1]) +
      NativeUInt(AReg.SrcX));
  end
  else
  begin
    SrcData[1] := PByte(NativeUInt(ASrc.data[1]) +
      NativeUInt(AReg.SrcY div 2) * NativeUInt(SrcLs[1]) +
      NativeUInt(AReg.SrcX div 2));
    SrcData[2] := PByte(NativeUInt(ASrc.data[2]) +
      NativeUInt(AReg.SrcY div 2) * NativeUInt(SrcLs[2]) +
      NativeUInt(AReg.SrcX div 2));
  end;

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
  // Decode na placa (OpenHwVideoDecoder): o device D3D11VA, o quadro que
  // recebe a copia da textura, e se esse caminho esta valendo. ProbeFmt e
  // o formato do quadro que o ProbeVideoDecode viu.
  HwDev: Pointer;
  HwFrame: PAVFrame;
  HwDecoding: Boolean;
  ProbeFmt: Integer;
  // Nome do decoder de VIDEO pro log. Nao use Decoder.name depois da
  // montagem das faixas: aquele laco reaproveita a variavel pro audio.
  VDecName: string;
  NormSws: SwsContext;
  Regs: TCompRegionArray;
  Tracks: TAudioTrackArray;
  OutW, OutH, Fps, OutFps, SrcFmt, SegIdx: Integer;
  DropFrames: Boolean;
  FrameInterval, NextKeepSec, OutSec, FpsMeasured: Double;
  VideoTb, EncTb, MixTb, MixSrcTb, FpsRat: AVRational;
  SegStartTs, SegEndTs: Int64;
  // Ultimo pts entregue ao encoder de video, na time_base DELE. Serve pra
  // guarda de monotonicidade em EncodeVideoFrame.
  LastEncPts: Int64;
  PtsCollisionLogged: Boolean;
  // Quadros de video entregues ao encoder e se ja logamos um pacote
  // recusado pelo decoder. Zero quadros num arquivo com video e FALHA, nao
  // "concluido": sairia so o cabecalho (261 bytes) e nada tocaria.
  VidFramesEncoded: Integer;
  VidSendErrLogged: Boolean;
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
  // So audio: sem stream de video na saida (pedido, ou origem sem video).
  AudioOnly: Boolean;
  PktSec: Double;
  Burner: TCaptionBurner;
  // Fila que refaz os quadros de audio no tamanho que o encoder exige
  // (ver FifoEmit). So usada com Rechunk (hoje: MP3, 1152 amostras).
  MixFrameSize: Int64;
  Rechunk: Boolean;
  FifoBuf: TArray<TArray<Single>>;   // um plano por canal, FLTP
  FifoN, FifoCh: Integer;
  FifoPts: Int64;                    // pts (MixTb) da 1a amostra da fila
  TplFrame: PAVFrame;                // modelo do ch_layout
  // Exportacao acelerada (AOpts.Speed). SpeedMode = velocidade diferente de
  // 1: o relogio da saida anda Speed vezes mais devagar que o da origem, o
  // video sai em CFR pela grade de quadros da saida (CurSlot = indice do
  // quadro, que e o pts com EncTb = 1/OutFps) e o audio vai pelas AOuts.
  Speed: Double;
  SpeedMode: Boolean;
  NextSlot, CurSlot: Int64;
  AOuts: TAudioOutArray;

  procedure ReportProgress(ASec: Double);
  // ASec = posicao dentro do trecho corrente, no tempo do ORIGINAL. O que
  // conta pro progresso e o tempo de SAIDA ja produzido.
  var
    Pct: Double;
    Step: Integer;
  begin
    if not Assigned(AProgress) then Exit;
    if TotalSec <= 0 then Exit;
    Pct := (OutOffsetSec + (ASec - SegStartSec) / Speed) / TotalSec * 100;
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

  function FifoEmit(AFinal: Boolean): Boolean;
  // Tira da fila quadros de EXATAMENTE MixFrameSize amostras e manda pro
  // encoder. AFinal = fim da exportacao: o que sobrar sai num quadro menor
  // (o libmp3lame aceita o ultimo quadro curto).
  //
  // O quadro e criado aqui — o que a mistura normal evita (pegadinha #51d)
  // porque exige o ch_layout, que fica fora da parte declarada do AVFrame.
  // Ele vem copiado do quadro-modelo (TplFrame) pelas posicoes medidas
  // (OFFS_FRAME_CH_LAYOUT / OFFS_FRAME_SAMPLE_RATE, FFmpegLib).
  var
    F: PAVFrame;
    N, ch, R: Integer;
  begin
    Result := True;
    while (FifoN >= MixFrameSize) or (AFinal and (FifoN > 0)) do
    begin
      N := Min(FifoN, Integer(MixFrameSize));
      F := av_frame_alloc;
      if F = nil then Exit(False);
      try
        F.nb_samples := N;
        F.format := AV_SAMPLE_FMT_FLTP;
        PInteger(PByte(F) + OFFS_FRAME_SAMPLE_RATE)^ := MixTb.den;
        if av_channel_layout_copy(PByte(F) + OFFS_FRAME_CH_LAYOUT,
             PByte(TplFrame) + OFFS_FRAME_CH_LAYOUT) < 0 then Exit(False);
        R := av_frame_get_buffer(F, 0);
        if R < 0 then
        begin
          Log('Export: av_frame_get_buffer (audio) falhou (%s).', [AvErrStr(R)]);
          Exit(False);
        end;
        for ch := 0 to FifoCh - 1 do
          if F.data[ch] <> nil then
            Move(FifoBuf[ch][0], F.data[ch]^, N * SizeOf(Single));
        F.pts := FifoPts;
        R := avcodec_send_frame(MixCtx, F);
        if R < 0 then
        begin
          Log('Export: avcodec_send_frame (audio) falhou (%s).', [AvErrStr(R)]);
          Exit(False);
        end;
      finally
        av_frame_free(@F);
      end;
      // Tira as N amostras da frente da fila.
      for ch := 0 to FifoCh - 1 do
        if FifoN > N then
          Move(FifoBuf[ch][N], FifoBuf[ch][0], (FifoN - N) * SizeOf(Single));
      Dec(FifoN, N);
      Inc(FifoPts, N);
      if not DrainEncoder(MixCtx, OutMixStream, MixTb) then Exit(False);
    end;
  end;

  function FifoPush: Boolean;
  // Poe o acumulador (ja com pts no MixTb) no fim da fila e manda o que
  // formar quadro inteiro.
  var
    ch, N: Integer;
  begin
    Result := True;
    N := AccFrame.nb_samples;
    if N <= 0 then Exit;
    if TplFrame = nil then
    begin
      // Modelo do layout de canais: o do primeiro quadro que chegou.
      TplFrame := av_frame_alloc;
      if (TplFrame = nil) or
         (av_channel_layout_copy(PByte(TplFrame) + OFFS_FRAME_CH_LAYOUT,
            PByte(AccFrame) + OFFS_FRAME_CH_LAYOUT) < 0) then Exit(False);
      FifoCh := 1;
      while (FifoCh < 8) and (AccFrame.data[FifoCh] <> nil) do Inc(FifoCh);
      SetLength(FifoBuf, FifoCh);
    end;
    if FifoN = 0 then
    begin
      if AccFrame.pts <> AV_NOPTS_VALUE then FifoPts := AccFrame.pts;
    end;
    for ch := 0 to FifoCh - 1 do
    begin
      if Length(FifoBuf[ch]) < FifoN + N then
        SetLength(FifoBuf[ch], (FifoN + N) * 2);
      if AccFrame.data[ch] <> nil then
        Move(AccFrame.data[ch]^, FifoBuf[ch][FifoN], N * SizeOf(Single))
      else
        FillChar(FifoBuf[ch][FifoN], N * SizeOf(Single), 0);
    end;
    Inc(FifoN, N);
    Result := FifoEmit(False);
  end;

  // ---- saidas de audio da exportacao acelerada (AOuts) ----
  // Mesma ideia da fila do MP3 (FifoEmit), generalizada: N saidas, cada
  // uma com o seu encoder, e o esticador de tempo na frente. Acesso por
  // ponteiro (o array nao muda de tamanho depois de montado) — e NADA de
  // `with`: os campos N/Ch colidiriam com variaveis locais (Delphi nao
  // diferencia maiuscula) e o `with` ganharia em silencio.

  function AOEmit(AIdx: Integer; AFinal: Boolean): Boolean;
  // Manda pro encoder os quadros completos da fila (AFinal: o resto num
  // quadro curto — AAC e MP3 aceitam o ultimo menor).
  var
    A: PAudioOut;
    F: PAVFrame;
    Cnt, c, R: Integer;
  begin
    Result := True;
    A := @AOuts[AIdx];
    while (A.N >= A.FrameSize) or (AFinal and (A.N > 0)) do
    begin
      if A.Tpl = nil then Exit(False);
      Cnt := Min(A.N, A.FrameSize);
      F := av_frame_alloc;
      if F = nil then Exit(False);
      try
        F.nb_samples := Cnt;
        F.format := AV_SAMPLE_FMT_FLTP;
        PInteger(PByte(F) + OFFS_FRAME_SAMPLE_RATE)^ := A.Tb.den;
        if av_channel_layout_copy(PByte(F) + OFFS_FRAME_CH_LAYOUT,
             PByte(A.Tpl) + OFFS_FRAME_CH_LAYOUT) < 0 then Exit(False);
        R := av_frame_get_buffer(F, 0);
        if R < 0 then
        begin
          Log('Export: av_frame_get_buffer (audio acelerado) falhou (%s).',
            [AvErrStr(R)]);
          Exit(False);
        end;
        for c := 0 to A.Ch - 1 do
          if F.data[c] <> nil then
            Move(A.Buf[c][0], F.data[c]^, Cnt * SizeOf(Single));
        F.pts := A.NextPts;
        R := avcodec_send_frame(A.Enc, F);
        if R < 0 then
        begin
          Log('Export: avcodec_send_frame (audio acelerado) falhou (%s).',
            [AvErrStr(R)]);
          Exit(False);
        end;
      finally
        av_frame_free(@F);
      end;
      Inc(A.NextPts, Cnt);
      for c := 0 to A.Ch - 1 do
        if A.N > Cnt then
          Move(A.Buf[c][Cnt], A.Buf[c][0], (A.N - Cnt) * SizeOf(Single));
      Dec(A.N, Cnt);
      if not DrainEncoder(A.Enc, A.Stream, A.Tb) then Exit(False);
    end;
  end;

  function AOTakeStretched(AIdx: Integer): Boolean;
  // Move a saida pronta do esticador pro fim da fila e emite.
  var
    A: PAudioOut;
    c, Cnt: Integer;
  begin
    A := @AOuts[AIdx];
    Cnt := A.Stretch.OutN;
    if Cnt > 0 then
    begin
      for c := 0 to A.Ch - 1 do
      begin
        if Length(A.Buf[c]) < A.N + Cnt then
          SetLength(A.Buf[c], Max(8192, (A.N + Cnt) * 2));
        Move(A.Stretch.Outp[c][0], A.Buf[c][A.N], Cnt * SizeOf(Single));
      end;
      Inc(A.N, Cnt);
      A.Stretch.OutN := 0;
    end;
    Result := AOEmit(AIdx, False);
  end;

  function AOPush(AIdx: Integer; AFrame: PAVFrame): Boolean;
  // Um quadro decodificado (FLTP) do trecho corrente entra na saida.
  var
    A: PAudioOut;
  begin
    Result := True;
    if (AFrame = nil) or (AFrame.nb_samples <= 0) then Exit;
    A := @AOuts[AIdx];
    if A.Tpl = nil then
    begin
      // Layout de canais da saida: o do primeiro quadro (as faixas do OBS
      // sao sempre mono ou estereo, ordem nativa).
      A.Tpl := av_frame_alloc;
      if (A.Tpl = nil) or
         (av_channel_layout_copy(PByte(A.Tpl) + OFFS_FRAME_CH_LAYOUT,
            PByte(AFrame) + OFFS_FRAME_CH_LAYOUT) < 0) then Exit(False);
      A.Ch := 1;
      while (A.Ch < 8) and (AFrame.data[A.Ch] <> nil) do Inc(A.Ch);
      SetLength(A.Buf, A.Ch);
      A.Stretch := TTimeStretch.Create(A.Ch, A.Tb.den, Speed);
    end;
    A.Stretch.Put(PPointer(@AFrame.data[0]), AFrame.nb_samples);
    A.Stretch.Process;
    Result := AOTakeStretched(AIdx);
  end;

  function AOEndSegment(AIdx: Integer): Boolean;
  // Fecha o trecho: a saida passa a ter EXATAMENTE o tamanho da linha do
  // tempo ate aqui (o esticador completa com silencio ou corta o excesso).
  // E o que impede o audio de escorregar do video emenda apos emenda.
  var
    A: PAudioOut;
    Target, Excess: Int64;
  begin
    Result := True;
    A := @AOuts[AIdx];
    Target := Round((OutOffsetSec + (SegEndSec - SegStartSec) / Speed) *
      A.Tb.den);
    if A.Stretch = nil then
    begin
      // Faixa que ainda nao entregou nenhum quadro: nao ha layout pra
      // montar silencio. O primeiro audio dela comeca no ponto certo da
      // linha do tempo (o muxer deixa o buraco antes).
      A.NextPts := Target;
      Exit;
    end;
    // Em velocidade alta a 1a janela do trecho sai "de graca" (le quase
    // nada de entrada), entao o trecho pode ter produzido ate uma janela a
    // mais que a linha do tempo. O excesso ainda na fila e cortado aqui; o
    // que ja foi pro encoder o trecho seguinte desconta (o alvo e absoluto).
    Excess := (A.NextPts + A.N) - Target;
    if Excess > 0 then Dec(A.N, Integer(Min(Int64(A.N), Excess)));
    A.Stretch.Finish(Max(Int64(0), Target - (A.NextPts + A.N)));
    Result := AOTakeStretched(AIdx);
  end;

  function AOFinish(AIdx: Integer): Boolean;
  var
    A: PAudioOut;
  begin
    Result := AOEmit(AIdx, True);
    if not Result then Exit;
    A := @AOuts[AIdx];
    if A.Enc = nil then Exit;
    avcodec_send_frame(A.Enc, nil);
    Result := DrainEncoder(A.Enc, A.Stream, A.Tb);
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
    // Acelerado: a mistura e a saida 0 das AOuts (esticador + fila). O pts
    // la e uma contagem continua, entao o deste quadro nao interessa.
    if SpeedMode then
    begin
      Result := AOPush(0, AccFrame);
      av_frame_unref(AccFrame);
      Exit;
    end;
    if AccFrame.pts <> AV_NOPTS_VALUE then
      AccFrame.pts :=
        av_rescale_q(AccFrame.pts - SecToTs(SegStartSec, MixSrcTb),
                     MixSrcTb, MixTb) + SecToTs(OutOffsetSec, MixTb);
    if Rechunk then
    begin
      Result := FifoPush;
      av_frame_unref(AccFrame);
      Exit;
    end;
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

  function HandleSpeedTrackPacket(ATrack: Integer): Boolean;
  // Acelerado sem mistura: cada faixa e decodificada e vai, esticada, pra
  // propria saida. (Copiar o pacote, como no 1x, nao da: o tempo muda.)
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
        if not AOPush(Tracks[ATrack].AOut, Frame) then Exit(False);
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
    Inc(VidFramesEncoded);
    Result := DrainEncoder(EncCtx, OutVStream, EncTb);
  end;

  function SetupScalers: Boolean;
  // Roda uma vez, no primeiro quadro util — so ai sabemos o pixel format
  // real que o decoder entrega.
  var
    k, RegFmt: Integer;
  begin
    Result := False;
    SrcFmt := Frame.format;
    // NV12 e o que o decode na placa entrega (av_hwframe_transfer_data).
    // Ele tem caminho rapido proprio no BlitRegion: normalizar pra YUV420P
    // seria uma passada extra de swscale sobre o quadro 4K inteiro.
    NeedsNormalize := not ((SrcFmt = AV_PIX_FMT_YUV420P) or
                           (SrcFmt = AV_PIX_FMT_YUVJ420P) or
                           (SrcFmt = AV_PIX_FMT_NV12));
    if NeedsNormalize then
    begin
      // Origem exotica (10 bits, RGB...): converte o quadro inteiro pra
      // YUV420P uma vez e recorta dali. As gravacoes do proprio app caem
      // sempre num caminho rapido e nunca passam por aqui.
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
    if SrcFmt = AV_PIX_FMT_NV12 then RegFmt := AV_PIX_FMT_NV12
    else RegFmt := AV_PIX_FMT_YUV420P;
    for k := 0 to High(Regs) do
    begin
      Regs[k].Sws := sws_getContext(
        Regs[k].SrcW, Regs[k].SrcH, RegFmt,
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
    // Legenda por CIMA da composicao, no relogio da ORIGEM — os turnos da
    // transcricao estao nele, e os cortes nao importam aqui.
    if Burner <> nil then
      Burner.BlendAt(PtsToSec(Frame.pts, VideoTb), OutFrame);
    // Emenda na linha do tempo de saida: posicao dentro do trecho mais o
    // que ja foi escrito pelos trechos anteriores.
    // O pts do quadro esta na time_base da ORIGEM; o encoder espera na
    // dele. Quando as duas coincidem (todo encoder menos o SVT-AV1) o
    // av_rescale_q e no-op, entao um caminho so serve pros dois.
    //
    // Acelerado: EncTb = 1/OutFps e o pts e o lugar na grade (CurSlot) —
    // saida em taxa constante, sem quantizar o relogio da origem.
    if SpeedMode then
      Result := EncodeVideoFrame(CurSlot)
    else
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
        if SpeedMode then
        begin
          // Acelerado: o quadro ocupa o ultimo lugar da grade da saida que
          // ja passou (relogio da saida = origem / velocidade). Lugar ja
          // preenchido = descarta. Lugar pulado (origem mais rala que a
          // grade) fica com o quadro anterior na tela — o pts so salta.
          OutSec := OutOffsetSec +
                    (PtsToSec(Frame.pts, VideoTb) - SegStartSec) / Speed;
          CurSlot := Floor(OutSec * OutFps + 1E-6);
          if CurSlot < NextSlot then Continue;
          NextSlot := CurSlot + 1;
        end
        else if DropFrames then
        begin
          OutSec := OutOffsetSec +
                    (PtsToSec(Frame.pts, VideoTb) - SegStartSec);
          if OutSec + 1E-9 < NextKeepSec then Continue;
          repeat
            NextKeepSec := NextKeepSec + FrameInterval;
          until NextKeepSec > OutSec;
        end;
        // Decode na placa: o quadro e uma textura. Traz pra memoria (NV12)
        // SO AGORA, depois das decisoes de descarte: a copia custa ~8 ms
        // num quadro 4K — mais que o proprio decode —, e os quadros antes
        // do trecho e os da reducao de fps nao precisam dela.
        if Frame.format = AV_PIX_FMT_D3D11 then
        begin
          R := av_hwframe_transfer_data(HwFrame, Frame, 0);
          // A transferencia so copia a imagem; pts e cia vem a parte.
          if R >= 0 then R := av_frame_copy_props(HwFrame, Frame);
          if R < 0 then
          begin
            Log('Export: copia do quadro da placa falhou (%s).', [AvErrStr(R)]);
            av_frame_unref(HwFrame);
            Failed := True;
            Break;
          end;
          av_frame_unref(Frame);
          av_frame_move_ref(Frame, HwFrame);
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

  function ProbeVideoDecode(out AErr: Integer): Boolean;
  // Decodifica do COMECO ate sair UM quadro (no maximo PROBE_PACKETS
  // pacotes de video). Existe porque um decoder pode abrir sem erro e
  // recusar TODO pacote: o libaom (o decoder de AV1 do build) com o AV1 que
  // a NVENC grava. O laco de exportacao pulava os pacotes recusados em
  // silencio e o arquivo saia com 261 bytes — so cabecalho, "concluido".
  // O seek do trecho logo depois devolve a leitura pro lugar certo.
  const
    PROBE_PACKETS = 120;
  var
    R, N: Integer;
  begin
    Result := False;
    AErr := 0;
    N := 0;
    ProbeFmt := AV_PIX_FMT_NONE;
    av_seek_frame(SrcCtx, -1, 0, AVSEEK_FLAG_BACKWARD);
    avcodec_flush_buffers(DecCtx);
    while (N < PROBE_PACKETS) and (av_read_frame(SrcCtx, Pkt) = 0) do
    try
      if Pkt.stream_index <> VIdx then Continue;
      Inc(N);
      R := avcodec_send_packet(DecCtx, Pkt);
      if R < 0 then
      begin
        AErr := R;
        Continue;
      end;
      R := avcodec_receive_frame(DecCtx, Frame);
      if R = 0 then
      begin
        ProbeFmt := Frame.format;
        av_frame_unref(Frame);
        Result := True;
        Break;
      end;
      if (R <> AVERROR_EAGAIN) and (R <> AVERROR_EOF) then AErr := R;
    finally
      av_packet_unref(Pkt);
    end;
    // Com threading em quadros o 1o quadro pode estar preso no decoder.
    if (not Result) and (avcodec_send_packet(DecCtx, nil) >= 0) and
       (avcodec_receive_frame(DecCtx, Frame) = 0) then
    begin
      ProbeFmt := Frame.format;
      av_frame_unref(Frame);
      Result := True;
    end;
    // Sai do modo dreno e esquece o que decodificou: o trecho recomeca.
    avcodec_flush_buffers(DecCtx);
  end;

  function SwitchVideoDecoder(const AName: AnsiString): Boolean;
  // Troca o decoder de video por outro do build, pelo NOME. False = nao
  // existe ou nao abriu (o atual continua).
  var
    D: PAVCodec;
    C2: PAVCodecContext;
  begin
    Result := False;
    D := avcodec_find_decoder_by_name(PAnsiChar(AName));
    if D = nil then Exit;
    C2 := avcodec_alloc_context3(D);
    if C2 = nil then Exit;
    if (avcodec_parameters_to_context(C2, VStream.codecpar) < 0) or
       (avcodec_open2(C2, D, nil) < 0) then
    begin
      avcodec_free_context(@C2);
      Exit;
    end;
    avcodec_free_context(@DecCtx);
    DecCtx := C2;
    Decoder := D;
    VDecName := string(AnsiString(D.name));
    Result := True;
  end;

  function OpenSwVideoDecoder: Boolean;
  // Decoder de SOFTWARE padrao do build pro codec da origem (pro AV1, o
  // libaom). Substitui o atual, se houver.
  var
    R: Integer;
  begin
    Result := False;
    if DecCtx <> nil then avcodec_free_context(@DecCtx);
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
    VDecName := string(AnsiString(Decoder.name));
    R := avcodec_open2(DecCtx, Decoder, nil);
    if R < 0 then
    begin
      Log('Export: avcodec_open2 (decoder) falhou (%s).', [AvErrStr(R)]);
      Exit;
    end;
    Result := True;
  end;

  function OpenHwVideoDecoder: Boolean;
  // Decoder NATIVO do libavcodec com aceleracao D3D11VA: o decode vai pra
  // placa (qualquer fabricante). Medido numa gravacao AV1 4K: 538 quadros/s
  // na placa contra ~45-60 do libaom — que era o gargalo da exportacao
  // inteira (o encoder de hardware ficava ~90% do tempo esperando quadro).
  //
  // O decoder escolhido e o NATIVO ('av1', nao o libaom que o
  // avcodec_find_decoder devolve): so o nativo tem hwaccel. Sem a placa
  // ele nao decodifica AV1 nenhum, entao o ProbeVideoDecode confere que
  // sairam quadros D3D11 de verdade antes de confiar nele.
  //
  // O get_format padrao do libavcodec escolhe sozinho o formato de
  // hardware quando o hw_device_ctx esta preenchido — por isso basta o
  // campo, sem callback. threads fica no default (1): o trabalho e da
  // placa, e threading em quadros so multiplicaria as texturas presas.
  var
    Name: AnsiString;
    D: PAVCodec;
    C2: PAVCodecContext;
    R: Integer;
  begin
    Result := False;
    case VStream.codecpar.codec_id of
      AV_CODEC_ID_AV1:  Name := 'av1';
      AV_CODEC_ID_HEVC: Name := 'hevc';
      AV_CODEC_ID_H264: Name := 'h264';
    else
      Exit;
    end;
    D := avcodec_find_decoder_by_name(PAnsiChar(Name));
    if D = nil then Exit;
    if HwDev = nil then
    begin
      R := av_hwdevice_ctx_create(@HwDev, AV_HWDEVICE_TYPE_D3D11VA,
        nil, nil, 0);
      if R < 0 then
      begin
        HwDev := nil;
        Log('Export: sem D3D11VA pra decodificar na placa (%s).', [AvErrStr(R)]);
        Exit;
      end;
    end;
    C2 := avcodec_alloc_context3(D);
    if C2 = nil then Exit;
    if avcodec_parameters_to_context(C2, VStream.codecpar) < 0 then
    begin
      avcodec_free_context(@C2);
      Exit;
    end;
    // Ref proprio do contexto: o avcodec_free_context o solta.
    PPointer(NativeUInt(C2) + OFFS_CODECCTX_HW_DEVICE_CTX)^ :=
      av_buffer_ref(HwDev);
    R := avcodec_open2(C2, D, nil);
    if R < 0 then
    begin
      Log('Export: decoder "%s" com D3D11VA nao abriu (%s).',
        [string(Name), AvErrStr(R)]);
      avcodec_free_context(@C2);
      Exit;
    end;
    if DecCtx <> nil then avcodec_free_context(@DecCtx);
    DecCtx := C2;
    Decoder := D;
    VDecName := string(AnsiString(D.name));
    Result := True;
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
  MixFrameSize := 0;
  Rechunk := False;
  FifoN := 0;
  FifoCh := 0;
  FifoPts := 0;
  TplFrame := nil;
  EncPar := nil;
  Pkt := nil;
  EncPkt := nil;
  Frame := nil;
  NormFrame := nil;
  OutFrame := nil;
  AccFrame := nil;
  HwDev := nil;
  HwFrame := nil;
  HwDecoding := False;
  ProbeFmt := AV_PIX_FMT_NONE;
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
  VidFramesEncoded := 0;
  VidSendErrLogged := False;
  Burner := nil;
  AudioOnly := AOpts.NoVideo;
  Speed := AOpts.Speed;
  if (Speed < 1) or IsNan(Speed) then Speed := 1;
  if Speed > EXPORT_SPEED_MAX then Speed := EXPORT_SPEED_MAX;
  SpeedMode := Abs(Speed - 1) > 1E-6;
  if not SpeedMode then Speed := 1;
  NextSlot := 0;
  CurSlot := 0;
  AOuts := nil;
  VideoTb.num := 1;
  VideoTb.den := 1000;
  OutW := 0;
  OutH := 0;
  EncTb := VideoTb;
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
      // Origem so de audio (uma exportacao so de audio reexportada): nao ha
      // o que compor, e o caminho de so audio serve inteiro.
      if not AudioOnly then
        Log('Export: origem sem stream de video — exportando so o audio.');
      AudioOnly := True;
    end;
    VStream := nil;
    if VIdx >= 0 then
    begin
      VStream := GetStreamByIndex(SrcCtx, Cardinal(VIdx));
      VideoTb := VStream.time_base;
      if VideoTb.den <= 0 then Exit;
    end;
    if AudioOnly and (Length(AOpts.AudioStreams) = 0) then
    begin
      Log('Export: so audio pedido sem nenhuma faixa de audio.');
      Exit;
    end;

    Fps := 30;
    if (VStream <> nil) and (VStream.avg_frame_rate.den > 0) and
       (VStream.avg_frame_rate.num > 0) then
      Fps := Max(1, Round(VStream.avg_frame_rate.num /
                          VStream.avg_frame_rate.den))
    else if VStream <> nil then
    begin
      // Sem taxa declarada (o MKV de uma uniao sai assim): mede. O chute
      // de 30 numa gravacao de 60 dava o teto errado no acelerado e um
      // "origem 30fps" falso no log.
      FpsMeasured := EstimateFpsFromPackets(SrcCtx, VIdx);
      if FpsMeasured > 0 then Fps := Max(1, Round(FpsMeasured));
      Log('Export: origem sem taxa declarada — medida %.2f fps.', [FpsMeasured]);
    end;

    // Taxa de saida: nunca acima da origem (nao da pra inventar quadro).
    // Acelerado, cada segundo da saida contem Speed segundos da origem, ou
    // seja Fps x Speed quadros — esse e o teto (limitado ao que o
    // container e o encoder aguentam, ver EXPORT_FPS_MAX).
    OutFps := AOpts.TargetFps;
    if SpeedMode then
    begin
      i := Max(1, Floor(Fps * Speed + 1E-6));
      if LowerCase(string(AOpts.EncoderName)) = 'libsvtav1' then
        i := Min(i, EXPORT_FPS_MAX_SVT)
      else
        i := Min(i, EXPORT_FPS_MAX);
      if (OutFps <= 0) or (OutFps > i) then OutFps := i;
    end
    else if (OutFps <= 0) or (OutFps > Fps) then OutFps := Fps;
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
    // Duracao da SAIDA (e o que o progresso mede).
    TotalSec := TotalSec / Speed;
    Log('Export: %d trecho(s), %.1fs de saida.',
      [Length(AOpts.Segments), TotalSec]);
    if SpeedMode then
      Log('Export: acelerado %.2fx — %d fps na saida (origem %d fps, teto %d).',
        [Speed, OutFps, Fps, Max(1, Floor(Fps * Speed + 1E-6))]);

    if not AudioOnly then
    begin
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
    // Placa primeiro (confirmado no ProbeVideoDecode, mais abaixo); sem
    // ela, o software de sempre.
    HwDecoding := OpenHwVideoDecoder;
    if not HwDecoding then
      if not OpenSwVideoDecoder then Exit;

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
    //
    // Acelerado vale o mesmo pra todos: o pts e o lugar na grade da saida
    // (ver PumpDecoder), entao a time_base E a grade.
    if SpeedMode or (LowerCase(string(AOpts.EncoderName)) = 'libsvtav1') then
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

    // Legenda queimada: sabe-se o tamanho de saida so agora. Falhar aqui
    // (GDI indisponivel, algo estranho na fonte) nao derruba a exportacao:
    // o video sai sem a legenda e o log diz por que.
    if Length(AOpts.Captions) > 0 then
      try
        Burner := TCaptionBurner.Create(OutW, OutH, AOpts.Captions);
        if Burner.ChunkCount = 0 then FreeAndNil(Burner);
      except
        on E: Exception do
        begin
          Log('Export: legenda desligada — %s', [E.Message]);
          FreeAndNil(Burner);
        end;
      end;
    end   // if not AudioOnly
    else
      Log('Export: so audio (%d faixa(s)).', [Length(AOpts.AudioStreams)]);

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
    // Mixar uma faixa so seria reencode a toa — degrada pra copia. Exceto
    // com AudioCodec: ai o container nao aceita o AAC da origem (mp3), e o
    // caminho da mistura e o que decodifica e recodifica.
    DoMix := (AOpts.MixAudio and (Length(Tracks) > 1)) or
             ((AOpts.AudioCodec <> '') and (Length(Tracks) > 0));

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

    if not AudioOnly then
    begin
      OutVStream := avformat_new_stream(OutCtx, nil);
      if OutVStream = nil then Exit;
      if avcodec_parameters_from_context(OutVStream.codecpar, EncCtx) < 0 then Exit;
      OutVStream.time_base := EncTb;
    end;

    if DoMix then
    begin
      if AOpts.AudioCodec <> '' then
        MixEncoder := avcodec_find_encoder_by_name(PAnsiChar(AOpts.AudioCodec))
      else
        MixEncoder := avcodec_find_encoder_by_name('aac');
      if MixEncoder = nil then
      begin
        Log('Export: encoder de audio %s ausente.', [string(AOpts.AudioCodec)]);
        // Sem o encoder pedido o container nao tem o que receber: copiar o
        // AAC num .mp3 geraria um arquivo que nada toca.
        if AOpts.AudioCodec <> '' then Exit(erNoEncoder);
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
        if AOpts.AudioBitrate > 0 then EncPar.bit_rate := AOpts.AudioBitrate
        else EncPar.bit_rate := MIX_BITRATE;
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
        Log('Export: avcodec_open2 (%s) falhou (%s) — mixagem desligada.',
          [string(AnsiString(MixEncoder.name)), AvErrStr(Rc)]);
        if AOpts.AudioCodec <> '' then Exit(erNoEncoder);
        DoMix := False;
      end
      else
      begin
        // Tamanho de quadro que o encoder exige (definido no open2). O AAC
        // do OBS decodifica em 1024; o MP3 (libmp3lame) quer 1152 e recusa
        // outro tamanho a nao ser no ultimo quadro. Diferente de 1024 =>
        // as amostras passam pela fila que refaz os quadros (FifoPush).
        MixFrameSize := 0;
        av_opt_get_int(MixCtx, 'frame_size', 0, @MixFrameSize);
        Rechunk := (MixFrameSize > 0) and (MixFrameSize <> 1024);
        if Rechunk then
          Log('Export: audio em quadros de %d amostras (%s).',
            [MixFrameSize, string(AnsiString(MixEncoder.name))]);
        OutMixStream := avformat_new_stream(OutCtx, nil);
        if OutMixStream = nil then Exit;
        if avcodec_parameters_from_context(OutMixStream.codecpar, MixCtx) < 0 then
          Exit;
        OutMixStream.time_base := MixTb;
        if AOpts.MixTitle <> '' then
          av_dict_set(@OutMixStream.metadata, 'title',
            PAnsiChar(ToUtf8(AOpts.MixTitle)), 0);
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

    // Acelerado + mistura: a mistura sai pela AOuts[0], com o encoder que
    // acabou de ser aberto (o MixCtx continua sendo liberado a parte).
    if DoMix and SpeedMode then
    begin
      SetLength(AOuts, 1);
      AOuts[0] := Default(TAudioOut);
      AOuts[0].Enc := MixCtx;
      AOuts[0].OwnsEnc := False;
      AOuts[0].Stream := OutMixStream;
      AOuts[0].Tb := MixTb;
      AOuts[0].FrameSize := MixFrameSize;
      if AOuts[0].FrameSize <= 0 then AOuts[0].FrameSize := 1024;
    end;

    // Acelerado SEM mistura: copiar pacote nao serve (o tempo muda), entao
    // cada faixa escolhida e decodificada, esticada e recodificada em AAC
    // na sua propria stream — as faixas continuam separadas como no 1x.
    if (not DoMix) and SpeedMode then
    begin
      MixEncoder := avcodec_find_encoder_by_name('aac');
      if (MixEncoder = nil) and (Length(Tracks) > 0) then
      begin
        Log('Export: encoder AAC ausente — sem audio acelerado.');
        Exit(erNoEncoder);
      end;
      SetLength(AOuts, Length(Tracks));
      for i := 0 to High(Tracks) do
      begin
        AOuts[i] := Default(TAudioOut);
        AOuts[i].OwnsEnc := True;
        Tracks[i].AOut := i;
        S := GetStreamByIndex(SrcCtx, Cardinal(Tracks[i].SrcIdx));
        AOuts[i].Tb.num := 1;
        AOuts[i].Tb.den := S.codecpar.sample_rate;
        if AOuts[i].Tb.den <= 0 then AOuts[i].Tb.den := 48000;

        AOuts[i].Enc := avcodec_alloc_context3(MixEncoder);
        if AOuts[i].Enc = nil then Exit;
        EncPar := avcodec_parameters_alloc;
        if EncPar = nil then Exit;
        try
          EncPar.codec_type  := AVMEDIA_TYPE_AUDIO;
          EncPar.codec_id    := MixEncoder.id;
          EncPar.format      := AV_SAMPLE_FMT_FLTP;
          EncPar.sample_rate := AOuts[i].Tb.den;
          if AOpts.AudioBitrate > 0 then EncPar.bit_rate := AOpts.AudioBitrate
          else EncPar.bit_rate := MIX_BITRATE;
          // Mesmo cuidado da mistura: ordem nativa, copiar o record e seguro.
          EncPar.ch_layout   := S.codecpar.ch_layout;
          if avcodec_parameters_to_context(AOuts[i].Enc, EncPar) < 0 then Exit;
        finally
          avcodec_parameters_free(PPointer(@EncPar));
        end;
        av_opt_set_q(AOuts[i].Enc, 'time_base', AOuts[i].Tb, 0);
        av_opt_set(AOuts[i].Enc, 'flags', '+global_header', 0);
        Rc := avcodec_open2(AOuts[i].Enc, MixEncoder, nil);
        if Rc < 0 then
        begin
          Log('Export: avcodec_open2 (aac, faixa %d) falhou (%s).',
            [Tracks[i].SrcIdx, AvErrStr(Rc)]);
          Exit(erNoEncoder);
        end;
        AOuts[i].FrameSize := 0;
        MixFrameSize := 0;
        av_opt_get_int(AOuts[i].Enc, 'frame_size', 0, @MixFrameSize);
        AOuts[i].FrameSize := MixFrameSize;
        if AOuts[i].FrameSize <= 0 then AOuts[i].FrameSize := 1024;

        OutAStream := avformat_new_stream(OutCtx, nil);
        if OutAStream = nil then Exit;
        if avcodec_parameters_from_context(OutAStream.codecpar,
             AOuts[i].Enc) < 0 then Exit;
        OutAStream.time_base := AOuts[i].Tb;
        CopyStreamTag(S, OutAStream, 'title');
        CopyStreamTag(S, OutAStream, 'language');
        AOuts[i].Stream := OutAStream;
        Tracks[i].OutIdx := OutAStream.index;

        // Decoder da faixa (o mesmo do caminho da mistura).
        Decoder := avcodec_find_decoder(S.codecpar.codec_id);
        if Decoder <> nil then
        begin
          Tracks[i].DecCtx := avcodec_alloc_context3(Decoder);
          if (Tracks[i].DecCtx <> nil) and
             ((avcodec_parameters_to_context(Tracks[i].DecCtx, S.codecpar) < 0) or
              (avcodec_open2(Tracks[i].DecCtx, Decoder, nil) < 0)) then
            avcodec_free_context(@Tracks[i].DecCtx);
        end;
      end;
    end
    else if not DoMix then
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
    HwFrame := av_frame_alloc;
    if (Pkt = nil) or (EncPkt = nil) or (Frame = nil) or
       (OutFrame = nil) or (AccFrame = nil) or (HwFrame = nil) then Exit;

    if not AudioOnly then
    begin
      OutFrame.format := AV_PIX_FMT_YUV420P;
      OutFrame.width  := OutW;
      OutFrame.height := OutH;
      Rc := av_frame_get_buffer(OutFrame, 0);
      if Rc < 0 then
      begin
        Log('Export: av_frame_get_buffer falhou (%s).', [AvErrStr(Rc)]);
        Exit;
      end;

      // Decode na placa: so vale se sairam quadros D3D11 de verdade. O
      // decoder nativo de H.264/HEVC cai calado pro software quando a placa
      // recusa o perfil (e com 1 thread so, mais lento que o caminho de
      // software normal); o de AV1 nem isso. Nos dois casos, volta pro
      // decoder de software de sempre.
      if HwDecoding then
      begin
        if ProbeVideoDecode(Rc) and (ProbeFmt = AV_PIX_FMT_D3D11) then
          Log('Export: decodificando na placa (D3D11VA, decoder "%s").',
            [VDecName])
        else
        begin
          Log('Export: decode na placa nao entregou quadro (%s, formato %d) — usando software.',
            [AvErrStr(Rc), ProbeFmt]);
          HwDecoding := False;
          if not OpenSwVideoDecoder then Exit;
        end;
      end;

      // O decoder escolhido decodifica ESTE arquivo? Abrir sem erro nao
      // garante (o libaom recusa o AV1 da NVENC pacote a pacote). Senao,
      // tenta os decoders de hardware do build pro mesmo codec — o *_cuvid
      // e o NVDEC, e entrega NV12 em memoria, que o NeedsNormalize converte.
      if (not HwDecoding) and (not ProbeVideoDecode(Rc)) then
      begin
        Log('Export: decoder "%s" nao entregou nenhum quadro (%s); tentando outro.',
          [VDecName, AvErrStr(Rc)]);
        var Found: Boolean := False;
        for var AltName in TArray<AnsiString>.Create('av1_cuvid', 'hevc_cuvid', 'h264_cuvid') do
        begin
          var Alt: PAVCodec := avcodec_find_decoder_by_name(PAnsiChar(AltName));
          if (Alt = nil) or (Alt.id <> VStream.codecpar.codec_id) then Continue;
          if not SwitchVideoDecoder(AltName) then
          begin
            Log('Export: decoder "%s" nao abriu.', [string(AltName)]);
            Continue;
          end;
          if ProbeVideoDecode(Rc) then
          begin
            Log('Export: usando o decoder "%s".', [string(AltName)]);
            Found := True;
            Break;
          end;
          Log('Export: decoder "%s" tambem nao entregou quadro (%s).',
            [string(AltName), AvErrStr(Rc)]);
        end;
        if not Found then
        begin
          Log('Export: nenhum decoder conseguiu ler o video da origem.');
          Exit;
        end;
      end;
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
    if (SegStartTs > 0) and (VIdx >= 0) then
      av_seek_frame(SrcCtx, VIdx, SegStartTs, AVSEEK_FLAG_BACKWARD)
    else if SegStartSec > 0 then
      // Sem video: stream -1 = tempo em AV_TIME_BASE (microssegundos).
      av_seek_frame(SrcCtx, -1, Round(SegStartSec * 1000000), AVSEEK_FLAG_BACKWARD)
    else
      av_seek_frame(SrcCtx, -1, 0, AVSEEK_FLAG_BACKWARD);
    if DecCtx <> nil then avcodec_flush_buffers(DecCtx);
    for i := 0 to High(Tracks) do
    begin
      if Tracks[i].DecCtx <> nil then avcodec_flush_buffers(Tracks[i].DecCtx);
      Tracks[i].Done := False;
    end;

    // So audio: o "video" ja nasce terminado, e o fim do trecho passa a ser
    // decidido so pelas faixas.
    VideoDone := AudioOnly;
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
          if Rc < 0 then
          begin
            // Um pacote ruim isolado nao derruba a exportacao, mas tem que
            // aparecer no log: recusa em TODO pacote passava calada.
            if not VidSendErrLogged then
            begin
              VidSendErrLogged := True;
              Log('Export: decoder recusou um pacote de video (%s) — so este aviso.',
                [AvErrStr(Rc)]);
            end;
            Continue;
          end;
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
            // Posicao ANTES do rebase abaixo: e o relogio da origem, o
            // mesmo do ReportProgress.
            PktSec := PtsToSec(Pkt.pts, Tracks[i].SrcTb);
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
            else if SpeedMode then
            begin
              if (Tracks[i].DecCtx <> nil) and (not HandleSpeedTrackPacket(i)) then
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
            // Sem video quem anda o progresso e a 1a faixa.
            if AudioOnly and (i = 0) and (PktSec >= 0) then
              ReportProgress(PktSec);
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
    if (not VideoDone) and (DecCtx <> nil) then
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
    // Acelerado: cada saida fecha o trecho com o tamanho exato da linha do
    // tempo (usa o OutOffsetSec ANTES de somar este trecho).
    for i := 0 to High(AOuts) do
      if not AOEndSegment(i) then
      begin
        Failed := True;
        Break;
      end;
    if Failed then Break;

    OutOffsetSec := OutOffsetSec + (SegEndSec - SegStartSec) / Speed;
    end;  // for SegIdx

    if Canceled then Exit(erCanceled);
    if Failed then Exit(erError);

    // ---- flush dos encoders ----
    // O acumulador de audio ja foi fechado na borda de cada trecho.
    if EncCtx <> nil then
    begin
      avcodec_send_frame(EncCtx, nil);
      DrainEncoder(EncCtx, OutVStream, EncTb);
    end;
    if SpeedMode then
    begin
      // Inclui a mistura (AOuts[0] usa o MixCtx): o que sobrou na fila sai
      // num ultimo quadro curto e o encoder e drenado.
      for i := 0 to High(AOuts) do
        if not AOFinish(i) then Exit(erError);
    end
    else if DoMix then
    begin
      // O que sobrou na fila sai num ultimo quadro curto.
      if Rechunk and (not FifoEmit(True)) then Exit(erError);
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

    if (not AudioOnly) and (VidFramesEncoded = 0) then
    begin
      Log('Export: nenhum quadro de video chegou ao encoder — arquivo descartado.');
      Exit(erError);
    end;

    Log('Export: concluido — %s (%.1fs, %dx%d).',
      [System.SysUtils.ExtractFileName(AOpts.DstPath), TotalSec, OutW, OutH]);
    if Assigned(AProgress) then AProgress(100);
    Result := erOk;
  finally
    Burner.Free;
    for i := 0 to High(AOuts) do
    begin
      AOuts[i].Stretch.Free;
      if AOuts[i].Tpl <> nil then av_frame_free(@AOuts[i].Tpl);
      if AOuts[i].OwnsEnc and (AOuts[i].Enc <> nil) then
        try avcodec_free_context(@AOuts[i].Enc); except end;
    end;
    for i := 0 to High(Regs) do
      if Regs[i].Sws <> nil then
        try sws_freeContext(Regs[i].Sws); except end;
    if NormSws <> nil then try sws_freeContext(NormSws); except end;
    if AccFrame <> nil then av_frame_free(@AccFrame);
    if TplFrame <> nil then av_frame_free(@TplFrame);
    if OutFrame <> nil then av_frame_free(@OutFrame);
    if NormFrame <> nil then av_frame_free(@NormFrame);
    if HwFrame <> nil then av_frame_free(@HwFrame);
    if Frame <> nil then av_frame_free(@Frame);
    if EncPkt <> nil then av_packet_free(@EncPkt);
    if Pkt <> nil then av_packet_free(@Pkt);
    for i := 0 to High(Tracks) do
      if Tracks[i].DecCtx <> nil then
        try avcodec_free_context(@Tracks[i].DecCtx); except end;
    if MixCtx <> nil then try avcodec_free_context(@MixCtx); except end;
    if EncCtx <> nil then try avcodec_free_context(@EncCtx); except end;
    if DecCtx <> nil then try avcodec_free_context(@DecCtx); except end;
    // Depois do decoder, que segura um ref proprio do device.
    if HwDev <> nil then try av_buffer_unref(@HwDev); except end;
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

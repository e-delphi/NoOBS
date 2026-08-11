(*
  OBSEncoder - seleção de encoder de vídeo via libobs.

  Logica isolada de codec: detecta encoders registrados, classifica por
  vendor (NVIDIA/AMD/Intel/x264), tenta criar instancia priorizando o
  que o user pediu (config.codec). Auto (e fallback de qualquer escolha
  que falhe) prioriza COMPATIBILIDADE: H.264 hw → x264 (sw) → AV1 hw →
  HEVC hw — H.264 abre em qualquer player/editor; x264 esta sempre
  presente; AV1/HEVC ficam por ultimo (requerem hw moderno).

  Esta unit nao guarda estado — `DetectEncoderCaps` e `SelectVideoEncoder`
  sao funcoes puras (apenas leem do config e enumeram libobs).
  Requer libobs ja inicializado (apos warmup ou EnsureInitialized).
*)
unit OBSEncoder;

interface

uses
  System.SysUtils,
  LibOBS,
  NoOBSTypes;

// Detecta encoders disponiveis enumerando obs_enum_encoder_types e
// classifica por vendor + tipo (AV1/HEVC/H264 hw, x264 sw).
function DetectEncoderCaps: TEncoderCaps;

// Cria encoder de video conforme preferencia do user (config 'codec'),
// caindo pra fallback se a primeira opcao falhar. Levanta excecao se
// nenhum encoder estiver disponivel (caso impossivel se obs-x264 carregou).
function SelectVideoEncoder: obs_encoder_t;

// Retorna a maior dimensao (W ou H) de canvas que o codec preferido
// pelo user consegue aceitar. Usado pelo OBSEngine pra clampar o
// bounding antes de obs_reset_video.
//
// H.264 hardware tem limite de 4096 em GPUs mais antigas (NVENC pre-
// Turing, AMD pre-RDNA3, Intel QSV legacy). HEVC/AV1 hardware
// suportam 8192 em todas as GPUs com esse encoder. x264 (CPU) nao
// tem limite pratico — 8192 e so sanity check.
function GetEncoderMaxDimension: Integer;

// Nivel do slider de qualidade da gravacao (0..10, padrao 5), ja migrado
// da escala antiga (-4..+2) e clampado. Fonte unica pra quem le a
// preferencia — o OBSBridge empurra este valor pra UI e o encoder deriva
// o CRF dele.
function GetRecordingQualityLevel: Integer;

// Traduz um ID de encoder do libobs ('av1_texture_amf', 'obs_x264',
// 'ffmpeg_svt_av1', ...) no rotulo curto de familia que a UI mostra
// ('AV1', 'H.264', 'HEVC') e se e hardware. Serve pra registrar na meta
// da gravacao COMO ela foi feita. Familia desconhecida devolve ''.
procedure DescribeEncoderId(const AId: string; out AFamily: string;
  out AHardware: Boolean);

implementation

uses
  System.AnsiStrings,
  System.Math,
  OBSLog,
  OBSConfig;

type
  // Um valor de qualidade por nivel do slider (0..10).
  TQualityTable = array[0..10] of Integer;

const
  // Escala de referencia do controle de qualidade: a do x264 (0..51), a
  // mesma que a exportacao usa.
  CRF_SCALE_MAX = 51;

  // ------------------------------------------------------------------
  // Tabelas de qualidade CALIBRADAS, uma por encoder.
  //
  // Reescalar linearmente entre as escalas nativas (o que se fazia antes)
  // parte da premissa de que "mesma posicao relativa na escala = mesma
  // qualidade". A premissa e FALSA, e por larga margem. Medido nesta
  // maquina encodando quadros REAIS de tela, decodando de volta e
  // comparando PSNR do plano Y contra a origem — no nivel 5 (padrao),
  // que deveria dar os 46,5 dB do x264 em crf 23:
  //
  //   encoder      valor antigo   PSNR entregue   valor calibrado
  //   x264            crf 23         46,5 dB           23
  //   SVT-AV1         crf 28         56,6 dB  (+10!)   54
  //   AMF AV1         qp  112        46,0 dB           109
  //   AMF H.264       qp   23        49,9 dB  (+3,4)    27
  //   AMF HEVC        qp   23        51,7 dB  (+5,2)    29
  //
  // O SVT-AV1 entregava 10 dB a mais do que o pedido — o usuario escolhia
  // "equilibrado" e recebia quase-lossless, com arquivo 2,4x maior que o
  // mesmo nivel no AV1 por hardware. Cada linha abaixo e o parametro que
  // IGUALA o PSNR do x264 naquele nivel, resolvido por interpolacao sobre
  // a curva medida de cada encoder.
  //
  // Referencia = x264 porque a escala do app sempre foi definida como
  // "CRF do x264", e o 23 do nivel 5 e a ancora herdada do OBS.
  // ------------------------------------------------------------------

  // x264: e a propria referencia.
  Q_X264: TQualityTable =
    (50, 44, 38, 33, 28, 23, 21, 19, 17, 15, 12);

  // SVT-AV1 (crf 0..63). Os niveis 0..3 saturam em 63: no maximo da
  // escala ele ainda entrega ~40 dB, bem acima do que o x264 da em crf
  // 38..50. Nao e defeito — o quantizador do AV1 simplesmente nao desce
  // tanto. Efeito pratico: nos dois niveis mais baixos o arquivo sai um
  // pouco maior que o pedido, porem com qualidade melhor.
  Q_SVT_AV1: TQualityTable =
    (63, 63, 63, 63, 61, 54, 50, 46, 41, 37, 29);

  // AMF AV1. A chave 'cqp' do plugin vai de 0..63 e ele multiplica por 4
  // internamente (texture-amf.cpp: qp := cq_value * 4), chegando na faixa
  // 0..255 do AMF — que foi a que se mediu. Estes valores ja sao o /4.
  Q_AMF_AV1: TQualityTable =
    (63, 61, 53, 45, 36, 27, 23, 18, 14, 12, 7);

  // AMF H.264 e HEVC (qp 0..51). O HEVC precisa de qp mais alto que o
  // H.264 pro mesmo PSNR — e mais eficiente, entrega a mesma qualidade
  // jogando fora mais informacao.
  Q_AMF_H264: TQualityTable =
    (51, 48, 42, 37, 32, 27, 25, 23, 21, 19, 16);
  Q_AMF_HEVC: TQualityTable =
    (51, 50, 44, 39, 34, 29, 27, 25, 23, 21, 18);

  // AV1 por CPU, registrado pelo obs-ffmpeg (obs-ffmpeg-av1.c). Escolha
  // EXPLICITA do usuario — nunca entra na cadeia automatica, porque satura
  // todos os nucleos e a gravacao nao pode competir com o que o usuario
  // esta fazendo. O 'ffmpeg_aom_av1' existe no mesmo plugin e foi
  // descartado: medido a ~8 quadros/s em 1080p, nem de longe tempo real.
  AV1_SW_ID: AnsiString = 'ffmpeg_svt_av1';

  // Preset do SVT-AV1 (0 = mais lento/melhor, 13 = mais rapido). FIXO, sem
  // controle na UI: medido com quadros REAIS de tela nesta maquina (16
  // threads), o tamanho do arquivo praticamente nao muda entre presets
  // (96..120 KB nos mesmos 40 quadros — tela e conteudo facil), mas a
  // velocidade muda em ordem de grandeza:
  //
  //   preset |   720p   |  1080p   |   4K
  //        8 | 121 q/s  |  40 q/s  |  23 q/s
  //       10 | 241 q/s  | 133 q/s  |  35 q/s
  //       12 | 344 q/s  | 186 q/s  |  36 q/s
  //
  // 10 e o joelho da curva: abaixo dele nao da tempo real em 1080p e nao
  // se ganha qualidade perceptivel; acima, o ganho ja saturou. Expor isso
  // como opcao so daria ao usuario um jeito de piorar as duas coisas.
  AV1_SW_PRESET = 10;

  // Teto de pico (kbps) aplicado ao 'max_bitrate' do NVENC em CQVBR.
  // Generoso de proposito: serve de rede de seguranca pra um canvas
  // multi-monitor em movimento pesado nao saturar o disco, sem estorvar a
  // qualidade constante no uso normal. O default do PLUGIN e 10000, baixo
  // demais — por isso setamos sempre. 0 = sem teto.
  DEFAULT_MAX_BITRATE = 100000;

// Listas de encoder IDs por codec, em ordem de prioridade.
const
  AV1_IDS: array[0..3] of AnsiString = (
    'obs_nvenc_av1_tex',
    'obs_nvenc_av1',
    'av1_texture_amf',
    'obs_qsv11_av1'
  );
  HEVC_IDS: array[0..5] of AnsiString = (
    'obs_nvenc_hevc_tex',
    'jim_hevc_nvenc',
    'obs_qsv11_hevc',
    'h265_texture_amf',
    'obs_nvenc_hevc',
    'amd_amf_hevc'
  );
  H264_IDS: array[0..5] of AnsiString = (
    'obs_nvenc_h264_tex',
    'jim_nvenc',
    'obs_qsv11_h264',
    'h264_texture_amf',
    'obs_nvenc_h264',
    'obs_x264'
  );

function EncoderTypeExists(const AId: AnsiString): Boolean;
var
  i: NativeUInt;
  P: PAnsiChar;
begin
  i := 0;
  while obs_enum_encoder_types(i, P) do
  begin
    if (P <> nil) and (System.AnsiStrings.StrComp(P, PAnsiChar(AId)) = 0) then
      Exit(True);
    Inc(i);
  end;
  Result := False;
end;

function DetectEncoderCaps: TEncoderCaps;
// Enumera os encoder types registrados em libobs e classifica por vendor.
// Considera que libobs ja foi inicializado (caller garante).
var
  i: NativeUInt;
  P: PAnsiChar;
  Id: string;
begin
  Result.Av1Hw  := False;
  Result.HevcHw := False;
  Result.H264Hw := False;
  Result.H264Sw := False;
  Result.Av1Sw  := False;
  Result.Vendor := gvUnknown;

  i := 0;
  while obs_enum_encoder_types(i, P) do
  begin
    if P <> nil then
    begin
      Id := LowerCase(string(AnsiString(P)));
      // x264 = CPU.
      if (Id = 'obs_x264') or (Id = 'ffmpeg_x264') then
        Result.H264Sw := True
      // AV1 por CPU. Vem no obs-ffmpeg (que ja carregamos), entao esta
      // disponivel em qualquer maquina — nao depende de GPU nenhuma e por
      // isso NAO mexe no Vendor. O irmao 'ffmpeg_aom_av1' e ignorado de
      // proposito: lento demais pra tempo real (ver AV1_SW_ID).
      else if Id = string(AV1_SW_ID) then
        Result.Av1Sw := True
      // NVIDIA: obs_nvenc_*, jim_nvenc, jim_hevc_nvenc
      else if (Pos('nvenc', Id) > 0) or (Pos('jim_nvenc', Id) > 0) or
              (Pos('jim_hevc_nvenc', Id) > 0) then
      begin
        if Result.Vendor = gvUnknown then Result.Vendor := gvNvidia;
        if Pos('av1', Id) > 0 then Result.Av1Hw := True
        else if Pos('hevc', Id) > 0 then Result.HevcHw := True
        else Result.H264Hw := True;
      end
      // AMD: *_amf
      else if Pos('amf', Id) > 0 then
      begin
        if Result.Vendor = gvUnknown then Result.Vendor := gvAmd;
        if Pos('av1', Id) > 0 then Result.Av1Hw := True
        else if Pos('h265', Id) > 0 then Result.HevcHw := True
        else if Pos('h264', Id) > 0 then Result.H264Hw := True;
      end
      // Intel QSV: obs_qsv11_*
      else if Pos('qsv', Id) > 0 then
      begin
        if Result.Vendor = gvUnknown then Result.Vendor := gvIntel;
        if Pos('av1', Id) > 0 then Result.Av1Hw := True
        else if Pos('hevc', Id) > 0 then Result.HevcHw := True
        else if Pos('h264', Id) > 0 then Result.H264Hw := True;
      end;
    end;
    Inc(i);
  end;
end;

function QualityLevelToCrf(ALevel: Integer): Integer;
// Mapa do nivel do slider (0..10) pra CRF na escala do x264 (0 = sem
// perdas, 51 = pior). MENOR = MELHOR = MAIOR. O slider vai na direcao
// INTUITIVA (10 = melhor), entao a tabela e decrescente.
//
//    0: 50  (compactacao maxima, arquivo minusculo)
//    1: 44
//    2: 38
//    3: 33
//    4: 28
//    5: 23  (PADRAO — ancora do preset padrao do OBS, SimpleOutput.cpp)
//    6: 21
//    7: 19
//    8: 17
//    9: 15
//   10: 12  (alem da ancora "Indistinguishable Quality" do OBS, que e 16)
//
// Os degraus sao DE PROPOSITO desiguais: 5-6 pontos na metade de baixo e
// 2 na de cima. CRF e perceptualmente logaritmico (cada +6 ~ metade da
// taxa) e a faixa que o olho discrimina fica em ~18..28 — gastar
// resolucao do slider entre 38 e 44, onde tudo ja e ruim, seria
// desperdicio. Steps iguais dariam um slider com metade das posicoes
// indistinguiveis entre si.
//
// Sem espelho no JS de proposito: a legenda da tela de Configuracoes
// descreve a qualidade em palavras e nao mostra numero nenhum. O espelho
// que existia (Settings._qualityBitrate) so servia pra imprimir os kbps —
// com qualidade constante nao ha taxa fixa pra anunciar, e um espelho que
// ninguem le sai de sincronia calado.
begin
  if (ALevel < 0) or (ALevel > 10) then ALevel := 5;   // 5 = padrao
  Result := Q_X264[ALevel];
end;

function GetRecordingQualityLevel: Integer;
// Le o nivel do slider do config, ja migrado e clampado. Fonte unica —
// o OBSBridge (que empurra pra UI) e o TryCreateVideoEncoder leem daqui,
// pra nao existirem duas interpretacoes da mesma chave.
//
// MIGRACAO: a escala antiga era -4..+2 com 0 = padrao, e morava na chave
// 'recordingQuality'. A nova e 0..10 com 5 = padrao, em
// 'recordingQualityLevel'. As duas faixas SE SOBREPOEM em 0/1/2, entao e
// impossivel distinguir pelo valor — dai a chave nova. Sem isso, o 0 de
// quem nunca mexeu no slider ("padrao") seria lido como o 0 novo
// ("compactacao maxima"), rebaixando a qualidade de todo mundo em
// silencio.
//
// Nao da pra usar o discriminator 'version' do config.json pra isso: ele
// DESCARTA o arquivo inteiro quando nao bate, e perder todas as
// preferencias por causa de um slider seria desproporcional.
var
  Old: Integer;
begin
  Result := GetConfigInt('recordingQualityLevel', -1);

  if Result < 0 then
  begin
    // Chave nova ausente: primeira execucao apos a mudanca de escala.
    // Traduz a antiga preservando a INTENCAO (distancia do padrao), nao
    // o CRF exato — o que o usuario escolheu foi "um tanto acima/abaixo
    // do padrao", e e isso que tem que sobreviver.
    Old := GetConfigInt('recordingQuality', 0);
    case Old of
      -4: Result := 1;
      -3: Result := 2;
      -2: Result := 3;
      -1: Result := 4;
      +1: Result := 7;
      +2: Result := 9;
    else
      Result := 5;
    end;
  end;

  if Result < 0  then Result := 0;
  if Result > 10 then Result := 10;
end;

procedure DescribeEncoderId(const AId: string; out AFamily: string;
  out AHardware: Boolean);
// A familia sai do NOME do encoder, nao de tabela de IDs conhecidos: os
// IDs variam entre versoes do OBS (jim_nvenc -> obs_nvenc_h264_tex, etc)
// e uma lista fixa envelheceria calada. 'Hardware' = nao e um dos dois
// encoders de CPU que carregamos.
var
  Id: string;
begin
  Id := LowerCase(AId);
  AFamily := '';
  if Pos('av1', Id) > 0 then AFamily := 'AV1'
  else if (Pos('hevc', Id) > 0) or (Pos('h265', Id) > 0) then AFamily := 'HEVC'
  else if (Pos('h264', Id) > 0) or (Pos('x264', Id) > 0) or
          (Pos('avc', Id) > 0) or (Pos('jim_nvenc', Id) > 0) then
    AFamily := 'H.264';
  AHardware := (Pos('x264', Id) = 0) and (Id <> string(AV1_SW_ID));
end;

function ScaleQuality(ACrf, AMax: Integer): Integer;
// Reescala o CRF 0..51 pra escala nativa de um encoder que vai ate AMax.
// SO serve pros encoders ainda NAO CALIBRADOS (NVENC e QSV) — os
// calibrados usam as tabelas Q_* acima, porque reescalar linearmente
// assume que "mesma posicao na escala = mesma qualidade", e a medicao
// mostrou que isso e falso (ver comentario de TQualityTable).
begin
  if AMax = CRF_SCALE_MAX then Exit(ACrf);
  Result := Round(ACrf * (AMax / CRF_SCALE_MAX));
end;

procedure ApplyConstantQuality(ASettings: obs_data_t; const AId: AnsiString;
  ALevel: Integer);
// Aplica o nivel do slider (0..10) no modo de QUALIDADE CONSTANTE
// ADAPTATIVA de cada encoder — aquele que varia o QP conforme a cena,
// gastando menos onde o olho nao percebe. Cada fabricante batizou o seu:
//
//   x264   -> CRF    (crf)
//   NVENC  -> CQVBR  (target_quality)   <- chave PROPRIA, nao e o 'cqp'
//   AMF    -> CQP    (cqp)              <- o QVBR dele e INVERTIDO, ver abaixo
//   QSV    -> ICQ    (icq_quality)      <- plugin nao carregado hoje
//
// NUNCA setar 'bitrate' junto: com alvo de taxa presente todos eles
// voltam pro modo de alvo e o controle de qualidade vira enfeite (mesma
// regra da exportacao, pegadinha #51b).
//
// O CBR que ficava aqui antes por omissao era ativamente ruim pra
// gravacao de tela: os tres encoders que carregamos ENCHEM o arquivo de
// bits de lixo pra bater o alvo (x264 b_filler, NVENC
// enableFillerDataInsertion, AMF FILLER_DATA_ENABLE), entao tela parada
// gravava megabits de nada.
var
  Id: string;
  IsAv1: Boolean;
  MaxPeak, ACrf, V: Integer;
begin
  if (ALevel < 0) or (ALevel > 10) then ALevel := 5;
  // CRF equivalente na escala de referencia. So os ramos NAO CALIBRADOS
  // (NVENC, QSV e o fallback generico) usam este valor; os calibrados
  // indexam a propria tabela pelo nivel.
  ACrf := Q_X264[ALevel];

  // Mesmo estilo do DetectEncoderCaps: compara como string, nao AnsiString
  // (evita ambiguidade entre System.Pos e System.AnsiStrings.Pos).
  Id := LowerCase(string(AId));
  IsAv1 := Pos('av1', Id) > 0;

  // ---- x264: CRF nativo — e a propria referencia da calibracao.
  if Pos('x264', Id) > 0 then
  begin
    obs_data_set_string(ASettings, 'rate_control', 'CRF');
    obs_data_set_int(ASettings, 'crf', Q_X264[ALevel]);
    Log('Encoder rc: CRF crf=%d (nivel %d)', [Q_X264[ALevel], ALevel]);
    Exit;
  end;

  // ---- SVT-AV1 (CPU): o plugin compara `astrcmpi(rc, "cqp")` e aplica o
  // valor da chave 'cqp' TANTO no 'crf' quanto no 'qp' do libsvtav1
  // (obs-ffmpeg-av1.c:103-110). Escala nativa 0..63, nao 0..51.
  //
  // O nome do modo aqui e 'cqp' MINUSCULO e tem que ser exatamente esse:
  // qualquer outra string (inclusive o 'CRF' do ramo generico la embaixo)
  // nao casa no astrcmpi e cai no ramo final do plugin, que e **CBR**
  // (obs-ffmpeg-av1.c:112). Seria a pegadinha #53 de novo, num encoder
  // novo — silenciosa, porque o arquivo sai, so que gordo.
  if Id = string(AV1_SW_ID) then
  begin
    obs_data_set_string(ASettings, 'rate_control', 'cqp');
    obs_data_set_int(ASettings, 'cqp', Q_SVT_AV1[ALevel]);
    obs_data_set_int(ASettings, 'preset', AV1_SW_PRESET);
    Log('Encoder rc: cqp=%d (nivel %d, calibrado)', [Q_SVT_AV1[ALevel], ALevel]);
    Exit;
  end;

  // ---- NVENC: CQVBR = VBR guiado por qualidade (rateControlMode VBR com
  // averageBitRate/vbvBufferSize zerados). O valor vai em 'target_quality',
  // NAO em 'cqp' — em CQVBR o 'cqp' e ignorado.
  //
  // O 'max_bitrate' PRECISA ser explicito: o default do plugin e 10000
  // kbps e ele CONTINUA valendo em CQVBR (nvenc.c trata cqvbr como vbr e
  // aplica maxBitRate), entao sem isto a gravacao ficaria com teto de 10
  // Mbps — estrangulando justamente as cenas de movimento, que sao as que
  // a qualidade constante deveria deixar gastar mais.
  if Pos('nvenc', Id) > 0 then
  begin
    obs_data_set_string(ASettings, 'rate_control', 'CQVBR');
    if IsAv1 then
      obs_data_set_int(ASettings, 'target_quality',
        Max(1, ScaleQuality(ACrf, 63)))
    else
      obs_data_set_int(ASettings, 'target_quality', Max(1, ACrf));

    MaxPeak := GetConfigInt('recordingMaxBitrate', DEFAULT_MAX_BITRATE);
    if MaxPeak < 0 then MaxPeak := 0;   // 0 = sem teto
    obs_data_set_int(ASettings, 'max_bitrate', MaxPeak);
    // NAO CALIBRADO: a calibracao por PSNR foi feita em maquina AMD, sem
    // NVIDIA disponivel pra medir. Segue no reescalonamento linear, que a
    // medicao dos outros mostrou ser so uma aproximacao. Quando houver uma
    // maquina NVIDIA, repetir o metodo e trocar por uma tabela Q_NVENC_*.
    Log('Encoder rc: CQVBR target_quality~%d (nivel %d, NAO calibrado)',
      [ACrf, ALevel]);
    Exit;
  end;

  // ---- AMF: CQP, com o QP na chave 'cqp'. Mesma escolha do frontend do
  // OBS pra gravacao (SimpleOutput.cpp UpdateRecordingSettings_amd_cqp).
  //
  // NAO usar 'QVBR' aqui, por mais que o nome prometa o VBR guiado por
  // qualidade. O 'QVBR_QUALITY_LEVEL' do AMF NAO e um QP: e um NIVEL DE
  // QUALIDADE em que MAIOR = MELHOR — o inverso da escala do CRF. E o
  // plugin do OBS alimenta essa propriedade com a MESMA chave 'cqp'
  // (texture-amf.cpp:1341-1344), entao mandar o CRF cru pra QVBR inverte
  // o controle: pedir qualidade minima entrega qualidade quase maxima.
  //
  // Medido com o proprio encoder AMF desta maquina (90 quadros 720p, via
  // libavcodec sobre as DLLs empacotadas):
  //   QVBR nivel 51 -> 10980 KB | nivel 12 -> 6815 KB | nivel 1 -> 5190 KB
  //   CQP  qp    50 ->  1997 KB | qp    12 -> 16776 KB
  // Alem de invertido, o QVBR tem faixa util muito menor: o piso dele
  // (5190 KB) e 2,6x o piso do CQP (1997 KB), entao "qualidade minima"
  // seria simplesmente inalcancavel por QVBR.
  //
  // Custo aceito: o OBS nao liga ENABLE_VBAQ no modo CONSTANT_QP
  // (texture-amf.cpp:1520-1524), entao a AMD perde a quantizacao
  // adaptativa por variancia. Um controle que responde na direcao certa
  // vale mais que um adaptativo que anda pra tras.
  if Pos('amf', Id) > 0 then
  begin
    if IsAv1 then
      V := Q_AMF_AV1[ALevel]
    // O HEVC do AMF precisa de qp MAIOR que o H.264 pro mesmo PSNR — e
    // mais eficiente, entrega a mesma qualidade jogando fora mais dado.
    else if (Pos('h265', Id) > 0) or (Pos('hevc', Id) > 0) then
      V := Q_AMF_HEVC[ALevel]
    else
      V := Q_AMF_H264[ALevel];
    obs_data_set_string(ASettings, 'rate_control', 'CQP');
    obs_data_set_int(ASettings, 'cqp', V);
    Log('Encoder rc: CQP cqp=%d (nivel %d, calibrado)', [V, ALevel]);
    Exit;
  end;

  // ---- QSV (Intel): ICQ. Hoje INALCANCAVEL — 'obs-qsv11' nao esta na
  // whitelist do OBSEngine.LoadModules, entao esses IDs nunca se
  // registram e maquina Intel grava em x264. Fica mapeado pra que ligar o
  // plugin um dia nao reintroduza o CBR por omissao.
  if Pos('qsv', Id) > 0 then
  begin
    obs_data_set_string(ASettings, 'rate_control', 'ICQ');
    if IsAv1 then
      obs_data_set_int(ASettings, 'icq_quality', Max(1, ScaleQuality(ACrf, 63)))
    else
      obs_data_set_int(ASettings, 'icq_quality', Max(1, ACrf));
    Exit;
  end;

  // Encoder que nao conhecemos: 'CRF' e o nome mais comum. Se ele nao
  // aceitar, cai no default dele — nada quebra.
  obs_data_set_string(ASettings, 'rate_control', 'CRF');
  obs_data_set_int(ASettings, 'crf', ACrf);
end;

function TryCreateVideoEncoder(const AId: AnsiString): obs_encoder_t;
var
  Settings: obs_data_t;
  QLevel, Crf: Integer;
begin
  // OBS recente retorna "phantom" encoder pra IDs nao registrados — checar
  // existencia via obs_enum_encoder_types antes de criar.
  if not EncoderTypeExists(AId) then Exit(nil);

  Settings := obs_data_create;
  try
    // Quality slider -> qualidade constante adaptativa (nunca alvo de
    // bitrate). TODO nivel aplica CRF, inclusive o padrao: nao existe
    // "sem override", porque o default de todos os encoders e CBR e era
    // exatamente ele que enchia a gravacao de tela parada de bits de lixo.
    QLevel := GetRecordingQualityLevel;
    Crf := QualityLevelToCrf(QLevel);
    Log('Encoder quality: nivel=%d (equivale a crf %d na referencia x264)',
      [QLevel, Crf]);
    // A partir daqui cada encoder resolve o proprio parametro pela tabela
    // calibrada — nao pelo CRF acima, que serve so de referencia comum.
    ApplyConstantQuality(Settings, AId, QLevel);

    // Intervalo de keyframe (chave padrao de TODOS os encoders do OBS: x264,
    // NVENC, AMF, QSV). Sem isto o default e 0 = "auto", que poe keyframes
    // muito espacados (~8s); ai pedacos curtos ficam com 1 so keyframe e nao
    // dao pra dividir/subdividir no player (stream copy so corta em I-frame).
    // Configuravel pelo usuario (1..10s, default 2 = padrao de streaming).
    var KeyframeSec: Integer := GetConfigInt('recordingKeyframeSec', 2);
    if KeyframeSec < 1  then KeyframeSec := 1;
    if KeyframeSec > 10 then KeyframeSec := 10;
    obs_data_set_int(Settings, 'keyint_sec', KeyframeSec);
    Log('Encoder keyint: %ds', [KeyframeSec]);

    Result := obs_video_encoder_create(PAnsiChar(AId),
      'NoOBS Video Encoder', Settings, nil);
  finally
    obs_data_release(Settings);
  end;
end;

function TryAv1Hw: obs_encoder_t;
var i: Integer;
begin
  for i := 0 to High(AV1_IDS) do
  begin
    Result := TryCreateVideoEncoder(AV1_IDS[i]);
    if Result <> nil then
    begin
      Log('Encoder: %s', [string(AV1_IDS[i])]);
      Exit;
    end;
  end;
  Result := nil;
end;

function TryHevcHw: obs_encoder_t;
var i: Integer;
begin
  for i := 0 to High(HEVC_IDS) do
  begin
    Result := TryCreateVideoEncoder(HEVC_IDS[i]);
    if Result <> nil then
    begin
      Log('Encoder: %s', [string(HEVC_IDS[i])]);
      Exit;
    end;
  end;
  Result := nil;
end;

function TryH264Hw: obs_encoder_t;
var i: Integer;
begin
  // H264_IDS termina com 'obs_x264' (CPU). Excluir esse pra "hardware only".
  for i := 0 to High(H264_IDS) do
  begin
    if H264_IDS[i] = 'obs_x264' then Continue;
    Result := TryCreateVideoEncoder(H264_IDS[i]);
    if Result <> nil then
    begin
      Log('Encoder: %s', [string(H264_IDS[i])]);
      Exit;
    end;
  end;
  Result := nil;
end;

function TryH264Sw: obs_encoder_t;
begin
  Result := TryCreateVideoEncoder('obs_x264');
  if Result <> nil then Log('Encoder: obs_x264');
end;

function TryAv1Sw: obs_encoder_t;
begin
  Result := TryCreateVideoEncoder(AV1_SW_ID);
  if Result <> nil then Log('Encoder: %s (CPU)', [string(AV1_SW_ID)]);
end;

function SelectVideoEncoder: obs_encoder_t;
var
  Pref: string;
begin
  // Le preferencia do usuario. Valores:
  //   auto | av1-hw | hevc-hw | h264-hw | h264-sw | av1-sw
  // Default 'auto': deixa o app decidir o melhor codec via fallback
  // chain (H.264 hw -> H.264 sw -> AV1 hw -> HEVC hw). Compatibilidade
  // primeiro, com fallback automatico pra software quando o hw nao
  // suporta.
  //
  // O 'av1-sw' NAO entra na cadeia automatica em hipotese nenhuma — so
  // roda quando o usuario escolhe na mao. Ele satura todos os nucleos, e
  // gravacao concorre com o que o usuario esta fazendo; cair nele sozinho
  // transformaria uma maquina sem AV1 por hardware numa maquina lenta sem
  // o usuario ter pedido nada.
  Pref := LowerCase(GetConfigStr('codec', 'auto'));
  Log('Codec preferido: %s', [Pref]);

  if Pref = 'av1-hw' then
  begin
    Result := TryAv1Hw;
    if Result <> nil then Exit;
    Log('Codec av1-hw indisponivel, caindo pro fallback.');
  end
  else if Pref = 'hevc-hw' then
  begin
    Result := TryHevcHw;
    if Result <> nil then Exit;
    Log('Codec hevc-hw indisponivel, caindo pro fallback.');
  end
  else if Pref = 'h264-hw' then
  begin
    Result := TryH264Hw;
    if Result <> nil then Exit;
    Log('Codec h264-hw indisponivel, caindo pro fallback.');
  end
  else if Pref = 'h264-sw' then
  begin
    Result := TryH264Sw;
    if Result <> nil then Exit;
    Log('Codec h264-sw indisponivel (estranho), caindo pro fallback.');
  end
  else if Pref = 'av1-sw' then
  begin
    Result := TryAv1Sw;
    if Result <> nil then Exit;
    Log('Codec av1-sw indisponivel, caindo pro fallback.');
  end;

  // Auto (ou fallback de qualquer escolha que falhou):
  // H.264 hw -> H.264 sw -> AV1 hw -> HEVC hw.
  // Ordem priorizando compatibilidade: H.264 abre em qualquer player
  // ou editor sem dor de cabeca. Cai em x264 (sempre presente) antes
  // de tentar AV1/HEVC, que requerem hw moderno e podem ter problemas
  // de playback em editores legados.
  Result := TryH264Hw;  if Result <> nil then Exit;
  Result := TryH264Sw;  if Result <> nil then Exit;
  Result := TryAv1Hw;   if Result <> nil then Exit;
  Result := TryHevcHw;  if Result <> nil then Exit;

  raise Exception.Create('Nenhum encoder de video disponivel.');
end;

function GetEncoderMaxDimension: Integer;
const
  // H.264 hardware (NVENC/AMF/QSV) e limitado a 4096 por dimensao em
  // TODAS as geracoes de GPU — nao e uma limitacao "legacy", e uma
  // decisao dos fabricantes pra manter o encoder H.264 dentro do Level
  // 5.2 do padrao. NVIDIA NVENC, AMD AMF e Intel QSV mantem 4096 pra
  // H.264 mesmo nas placas mais novas (Ada/RDNA3/Arc). HEVC e AV1 dos
  // mesmos chips sobem pra 8192 sem problema.
  MAX_H264_HW = 4096;
  // HEVC/AV1 hw: 8192 universal nas GPUs com esses encoders.
  // x264 CPU: sem limite real, 8192 e so sanity.
  MAX_OTHER = 8192;
var
  Pref: string;
  Caps: TEncoderCaps;

  // Max-dim do fallback automatico. A chain do SelectVideoEncoder e
  // H.264 hw → x264 → AV1 hw → HEVC hw; como x264 esta SEMPRE presente,
  // o fallback sempre aterrissa em H.264 hw (4096, se existir) ou x264
  // (8192). Nunca chega em AV1/HEVC pelo fallback (x264 intercepta).
  function FallbackMax: Integer;
  begin
    if Caps.H264Hw then Result := MAX_H264_HW else Result := MAX_OTHER;
  end;

begin
  Pref := LowerCase(GetConfigStr('codec', 'auto'));
  Caps := DetectEncoderCaps;

  // Espelha o encoder que SelectVideoEncoder vai REALMENTE criar — senao
  // o clamp e o encoder podem discordar: pedir 'hevc-hw' (clamp 8192) mas,
  // se o HEVC-hw nao existe, o fallback pega H.264 hw (max 4096) e o
  // obs_output_start falha com canvas > 4096 (reintroduzia a pegadinha #7).
  if Pref = 'h264-hw' then
  begin
    if Caps.H264Hw then Exit(MAX_H264_HW);
    Exit(FallbackMax);
  end;
  if Pref = 'hevc-hw' then
  begin
    if Caps.HevcHw then Exit(MAX_OTHER);
    Exit(FallbackMax);
  end;
  if Pref = 'av1-hw' then
  begin
    if Caps.Av1Hw then Exit(MAX_OTHER);
    Exit(FallbackMax);
  end;
  if Pref = 'h264-sw' then Exit(MAX_OTHER);  // x264 sempre presente
  // SVT-AV1 aceita bem alem de 8192; MAX_OTHER aqui e sanity, igual x264.
  if Pref = 'av1-sw' then
  begin
    if Caps.Av1Sw then Exit(MAX_OTHER);
    Exit(FallbackMax);
  end;

  // 'auto' (ou valor desconhecido): comeca do topo da chain = FallbackMax.
  Result := FallbackMax;
end;

end.

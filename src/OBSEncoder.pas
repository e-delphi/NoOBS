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

implementation

uses
  System.AnsiStrings,
  OBSLog,
  OBSConfig;

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

function QualityLevelToBitrate(ALevel: Integer): Integer;
// Mapa de nivel de qualidade (config 'recordingQuality') pra bitrate
// em kbps. Nivel 0 = nao seta nada (usa default do encoder), permitindo
// "modo padrao" identico ao comportamento antes do recurso existir.
// Niveis estendem ate +/-2 — 5 pontos no slider total.
//
// Valores escolhidos pra 1080p H.264:
//   -2: 2500 kbps  (compressao agressiva, arquivo bem pequeno)
//   -1: 4000 kbps
//    0: 0 (= sem override, usa default do encoder)
//   +1: 10000 kbps (visivelmente melhor)
//   +2: 20000 kbps (proximo de lossless visual)
//
// Em resolucoes/codecs diferentes a percepcao de qualidade muda mas
// a escala relativa (mais baixo = pior, mais alto = melhor) se preserva.
begin
  case ALevel of
    -2: Result := 2500;
    -1: Result := 4000;
    +1: Result := 10000;
    +2: Result := 20000;
  else
    Result := 0;  // = nao setar, usar default do encoder
  end;
end;

function TryCreateVideoEncoder(const AId: AnsiString): obs_encoder_t;
var
  Settings: obs_data_t;
  QLevel, Bitrate: Integer;
begin
  // OBS recente retorna "phantom" encoder pra IDs nao registrados — checar
  // existencia via obs_enum_encoder_types antes de criar.
  if not EncoderTypeExists(AId) then Exit(nil);

  Settings := obs_data_create;
  try
    // Quality slider — so seta bitrate se nivel != 0 (modo padrao).
    // Bitrate funciona com TODOS os encoders (NVENC, AMF, QSV, x264).
    // Outras chaves (cqp/crf) variam por encoder e nao valem a complexidade
    // pro escopo "5 niveis".
    QLevel := GetConfigInt('recordingQuality', 0);
    Bitrate := QualityLevelToBitrate(QLevel);
    if Bitrate > 0 then
    begin
      obs_data_set_int(Settings, 'bitrate', Bitrate);
      Log('Encoder quality: nivel=%d bitrate=%d kbps', [QLevel, Bitrate]);
    end
    else
      Log('Encoder quality: nivel=0 (usando default do encoder).');

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

function SelectVideoEncoder: obs_encoder_t;
var
  Pref: string;
begin
  // Le preferencia do usuario. Valores: auto | av1-hw | hevc-hw | h264-hw | h264-sw.
  // Default 'auto': deixa o app decidir o melhor codec via fallback
  // chain (H.264 hw -> H.264 sw -> AV1 hw -> HEVC hw). Compatibilidade
  // primeiro, com fallback automatico pra software quando o hw nao
  // suporta.
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

  // 'auto' (ou valor desconhecido): comeca do topo da chain = FallbackMax.
  Result := FallbackMax;
end;

end.

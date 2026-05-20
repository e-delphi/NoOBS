(*
  OBSEncoder - seleção de encoder de vídeo via libobs.

  Logica isolada de codec: detecta encoders registrados, classifica por
  vendor (NVIDIA/AMD/Intel/x264), tenta criar instancia priorizando o
  que o user pediu (config.codec) com fallback AV1 → HEVC → H.264 hw
  → x264 CPU.

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

function TryCreateVideoEncoder(const AId: AnsiString): obs_encoder_t;
var
  Settings: obs_data_t;
begin
  // OBS recente retorna "phantom" encoder pra IDs nao registrados — checar
  // existencia via obs_enum_encoder_types antes de criar.
  if not EncoderTypeExists(AId) then Exit(nil);

  Settings := obs_data_create;
  try
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
  // AV1 hw -> HEVC hw -> H.264 hw -> H.264 sw.
  Result := TryAv1Hw;   if Result <> nil then Exit;
  Result := TryHevcHw;  if Result <> nil then Exit;
  Result := TryH264Hw;  if Result <> nil then Exit;
  Result := TryH264Sw;  if Result <> nil then Exit;

  raise Exception.Create('Nenhum encoder de video disponivel.');
end;

end.

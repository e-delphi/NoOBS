(*
  NoOBSTypes - tipos compartilhados entre 2+ units do NoOBS.

  Critério pra entrar aqui: o tipo é consumido por unidades em camadas
  diferentes (ex: engine + bridge + UI), OU ficaria ambíguo dizer qual
  unit "é dona dele". Tipos com dono claro (ex: TAudioDeviceInfo em
  WinAudioMeter, TMonitorInfo em WinPreview) ficam na unit produtora.

  Esta unit nao tem dependencias alem do SysUtils — pode ser usada
  por qualquer outra sem risco de ciclo.
*)
unit NoOBSTypes;

interface

uses
  System.SysUtils;

type
  // ---------------------------------------------------------------------
  // Encoder / GPU vendor
  // ---------------------------------------------------------------------

  // Vendor detectado a partir dos encoder IDs registrados no libobs.
  // Usado pra decidir logo+label na UI e priorizar codecs.
  TGpuVendor = (gvUnknown, gvNvidia, gvAmd, gvIntel);

  // Quais classes de encoder estao disponiveis em runtime. Detectado
  // via obs_enum_encoder_types apos o warmup do libobs.
  TEncoderCaps = record
    Av1Hw:  Boolean;   // qualquer encoder AV1 hardware
    HevcHw: Boolean;   // qualquer encoder HEVC hardware
    H264Hw: Boolean;   // qualquer encoder H.264 hardware (excluindo x264)
    H264Sw: Boolean;   // x264 (CPU) — sempre True na pratica
    Vendor: TGpuVendor;
  end;

  // ---------------------------------------------------------------------
  // Audio devices vistos pelo libobs
  // ---------------------------------------------------------------------

  // Representa um dispositivo de audio enumerado via obs_properties de
  // uma fonte wasapi_input_capture/wasapi_output_capture.
  // Name = friendly name; DeviceId = identificador interno do libobs
  // (geralmente igual ao WASAPI device_id, mas tratamos como opaco).
  TObsAudioDev = record
    Name: string;
    DeviceId: AnsiString;
  end;
  TObsAudioDevArray = TArray<TObsAudioDev>;

implementation

end.

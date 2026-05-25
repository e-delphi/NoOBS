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

  // ---------------------------------------------------------------------
  // Layout / metadata da gravacao
  // ---------------------------------------------------------------------

  // Uma regiao do canvas final ocupada por um source (monitor ou webcam).
  // Posicao e tamanho em PIXELS do canvas (vide TRecordingLayout.CanvasW/H).
  // Usado no player pra permitir "zoom" em um monitor especifico.
  TRecordingRegion = record
    Name: string;     // friendly name visivel no menu do player
    Kind: string;     // 'monitor' | 'webcam'
    X, Y, W, H: Integer;
  end;
  TRecordingRegionArray = TArray<TRecordingRegion>;

  // Layout do canvas no momento da gravacao. Capturado por OBSEngine
  // durante BuildAndStartRecording e persistido em <hash>.json na cache.
  TRecordingLayout = record
    CanvasW, CanvasH: Integer;
    Regions: TRecordingRegionArray;
  end;

  // Metadata completa de uma gravacao — duracao + layout. E o que vai
  // em disco no arquivo <hash>.json. Em gravacoes antigas (que so tinham
  // o .dur legado) o Layout fica zerado/vazio e a UI cai pro modo "tela
  // cheia" sem seletor de monitor.
  TRecordingMeta = record
    DurationSec: Integer;
    Layout: TRecordingLayout;
  end;

implementation

end.

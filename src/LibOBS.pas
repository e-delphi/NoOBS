(*
  LibOBS - bindings Delphi para a API C do libobs (obs.dll).

  Declaracoes de tipos opacos, structs, enums e funcoes exportadas
  pelo obs.dll. Usa delayed loading — a DLL so e carregada na 1a
  chamada. Windows resolve a partir do diretorio do .exe (DLL search
  order padrao desde Win7), entao basta obs.dll estar ao lado.

  Calling convention: cdecl (padrao do libobs no Windows).
  Target: Win64 apenas.
*)
unit LibOBS;

{$WARN SYMBOL_PLATFORM OFF}

interface

uses
  Winapi.Windows;

// -----------------------------------------------------------------------
// Tipos opacos (ponteiros para structs internas do libobs)
// -----------------------------------------------------------------------

type
  obs_source_t     = type Pointer;
  obs_scene_t      = type Pointer;
  obs_sceneitem_t  = type Pointer;
  obs_output_t     = type Pointer;
  obs_encoder_t    = type Pointer;
  obs_data_t       = type Pointer;
  obs_properties_t = type Pointer;
  obs_property_t   = type Pointer;
  video_t          = type Pointer;
  audio_t          = type Pointer;

// -----------------------------------------------------------------------
// Enums (MSVC x64: 4 bytes = Integer)
// -----------------------------------------------------------------------

const
  // video_format (media-io/video-io.h)
  VIDEO_FORMAT_NONE = 0;
  VIDEO_FORMAT_I420 = 1;
  VIDEO_FORMAT_NV12 = 2;
  VIDEO_FORMAT_RGBA = 6;
  VIDEO_FORMAT_BGRA = 7;

  // video_colorspace
  VIDEO_CS_DEFAULT  = 0;
  VIDEO_CS_601      = 1;
  VIDEO_CS_709      = 2;
  VIDEO_CS_SRGB     = 3;

  // video_range_type
  VIDEO_RANGE_DEFAULT = 0;
  VIDEO_RANGE_PARTIAL = 1;
  VIDEO_RANGE_FULL    = 2;

  // obs_scale_type (obs.h) — NAO confundir com video_scale_type
  OBS_SCALE_DISABLE  = 0;
  OBS_SCALE_POINT    = 1;
  OBS_SCALE_BICUBIC  = 2;
  OBS_SCALE_BILINEAR = 3;
  OBS_SCALE_LANCZOS  = 4;
  OBS_SCALE_AREA     = 5;

  // speaker_layout (media-io/audio-io.h)
  SPEAKERS_UNKNOWN  = 0;
  SPEAKERS_MONO     = 1;
  SPEAKERS_STEREO   = 2;
  SPEAKERS_2POINT1  = 3;
  SPEAKERS_4POINT0  = 4;
  SPEAKERS_4POINT1  = 5;
  SPEAKERS_5POINT1  = 6;
  SPEAKERS_7POINT1  = 8;  // gap intencional: 7 nao existe

  // obs_bounds_type (obs.h)
  OBS_BOUNDS_NONE            = 0;
  OBS_BOUNDS_STRETCH         = 1;
  OBS_BOUNDS_SCALE_INNER     = 2;
  OBS_BOUNDS_SCALE_OUTER     = 3;
  OBS_BOUNDS_SCALE_TO_WIDTH  = 4;
  OBS_BOUNDS_SCALE_TO_HEIGHT = 5;
  OBS_BOUNDS_MAX_ONLY        = 6;

  // obs_reset_video return codes (obs-defs.h)
  OBS_VIDEO_SUCCESS          =  0;
  OBS_VIDEO_FAIL             = -1;
  OBS_VIDEO_NOT_SUPPORTED    = -2;
  OBS_VIDEO_INVALID_PARAM    = -3;
  OBS_VIDEO_CURRENTLY_ACTIVE = -4;
  OBS_VIDEO_MODULE_NOT_FOUND = -5;

// -----------------------------------------------------------------------
// Structs
// -----------------------------------------------------------------------

type
  TVec2 = record
    x, y: Single;
  end;
  PVec2 = ^TVec2;

  obs_video_info = record
    graphics_module: PAnsiChar;
    fps_num: Cardinal;
    fps_den: Cardinal;
    base_width: Cardinal;
    base_height: Cardinal;
    output_width: Cardinal;
    output_height: Cardinal;
    output_format: Integer;
    adapter: Cardinal;
    gpu_conversion: ByteBool;
    _pad0: array[0..2] of Byte;
    colorspace: Integer;
    range: Integer;
    scale_type: Integer;
  end;
  Pobs_video_info = ^obs_video_info;

  obs_audio_info = record
    samples_per_sec: Cardinal;
    speakers: Integer;
  end;
  Pobs_audio_info = ^obs_audio_info;

// -----------------------------------------------------------------------
// Callback types
// -----------------------------------------------------------------------

type
  obs_enum_sources_proc = function(param: Pointer;
    source: obs_source_t): ByteBool; cdecl;

  obs_scene_enum_items_proc = function(scene: obs_scene_t;
    item: obs_sceneitem_t; param: Pointer): ByteBool; cdecl;

  // log_level: 100=ERROR, 200=WARNING, 300=INFO, 400=DEBUG
  log_handler_t = procedure(log_level: Integer; msg: PAnsiChar;
    args: Pointer; p: Pointer); cdecl;

const
  LOG_ERROR   = 100;
  LOG_WARNING = 200;
  LOG_INFO    = 300;
  LOG_DEBUG   = 400;

// -----------------------------------------------------------------------
// Logging (do util/base.h, exportado pela libobs)
// -----------------------------------------------------------------------

procedure base_set_log_handler(handler: log_handler_t; param: Pointer);
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Core lifecycle
// -----------------------------------------------------------------------

function obs_startup(locale: PAnsiChar; module_config_path: PAnsiChar;
  store: Pointer): ByteBool; cdecl; external 'obs.dll' delayed;

procedure obs_shutdown; cdecl; external 'obs.dll' delayed;

function obs_initialized: ByteBool; cdecl; external 'obs.dll' delayed;

function obs_reset_video(ovi: Pobs_video_info): Integer;
  cdecl; external 'obs.dll' delayed;

function obs_reset_audio(oai: Pobs_audio_info): ByteBool;
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Module loading
// -----------------------------------------------------------------------

procedure obs_add_module_path(bin: PAnsiChar; data: PAnsiChar);
  cdecl; external 'obs.dll' delayed;

procedure obs_add_data_path(path: PAnsiChar);
  cdecl; external 'obs.dll' delayed;

procedure obs_load_all_modules;
  cdecl; external 'obs.dll' delayed;

procedure obs_post_load_modules;
  cdecl; external 'obs.dll' delayed;

// Carrega UM modulo especifico (sem dependencia de obs_add_module_path).
// Retorna 0 em sucesso. data_path pode ser nil.
function obs_open_module(out module_: Pointer; path: PAnsiChar;
  data_path: PAnsiChar): Integer; cdecl; external 'obs.dll' delayed;

function obs_init_module(module_: Pointer): ByteBool;
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Video/Audio subsystem
// -----------------------------------------------------------------------

function obs_get_video: video_t;
  cdecl; external 'obs.dll' delayed;

function obs_get_audio: audio_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_set_output_source(channel: Cardinal; source: obs_source_t);
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// obs_data (settings)
// -----------------------------------------------------------------------

function obs_data_create: obs_data_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_data_release(data: obs_data_t);
  cdecl; external 'obs.dll' delayed;

procedure obs_data_set_string(data: obs_data_t; name: PAnsiChar;
  val: PAnsiChar); cdecl; external 'obs.dll' delayed;

procedure obs_data_set_int(data: obs_data_t; name: PAnsiChar;
  val: Int64); cdecl; external 'obs.dll' delayed;

procedure obs_data_set_bool(data: obs_data_t; name: PAnsiChar;
  val: ByteBool); cdecl; external 'obs.dll' delayed;

function obs_data_get_string(data: obs_data_t;
  name: PAnsiChar): PAnsiChar; cdecl; external 'obs.dll' delayed;

function obs_data_get_int(data: obs_data_t;
  name: PAnsiChar): Int64; cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Sources
// -----------------------------------------------------------------------

function obs_source_create(id: PAnsiChar; name: PAnsiChar;
  settings: obs_data_t; hotkey_data: obs_data_t): obs_source_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_source_release(source: obs_source_t);
  cdecl; external 'obs.dll' delayed;

procedure obs_source_update(source: obs_source_t; settings: obs_data_t);
  cdecl; external 'obs.dll' delayed;

procedure obs_source_set_muted(source: obs_source_t; muted: ByteBool);
  cdecl; external 'obs.dll' delayed;

procedure obs_source_set_audio_mixers(source: obs_source_t;
  mixers: Cardinal); cdecl; external 'obs.dll' delayed;

procedure obs_enum_sources(enum_proc: obs_enum_sources_proc;
  param: Pointer); cdecl; external 'obs.dll' delayed;

function obs_source_get_name(source: obs_source_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Scenes
// -----------------------------------------------------------------------

function obs_scene_create(name: PAnsiChar): obs_scene_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_scene_release(scene: obs_scene_t);
  cdecl; external 'obs.dll' delayed;

function obs_scene_get_source(scene: obs_scene_t): obs_source_t;
  cdecl; external 'obs.dll' delayed;

function obs_scene_add(scene: obs_scene_t;
  source: obs_source_t): obs_sceneitem_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_scene_enum_items(scene: obs_scene_t;
  callback: obs_scene_enum_items_proc; param: Pointer);
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Scene items
// -----------------------------------------------------------------------

procedure obs_sceneitem_set_pos(item: obs_sceneitem_t;
  const pos: PVec2); cdecl; external 'obs.dll' delayed;

procedure obs_sceneitem_set_scale(item: obs_sceneitem_t;
  const scale: PVec2); cdecl; external 'obs.dll' delayed;

function obs_sceneitem_set_visible(item: obs_sceneitem_t;
  visible: ByteBool): ByteBool; cdecl; external 'obs.dll' delayed;

procedure obs_sceneitem_set_bounds_type(item: obs_sceneitem_t;
  bounds_type: Integer); cdecl; external 'obs.dll' delayed;

procedure obs_sceneitem_set_bounds(item: obs_sceneitem_t;
  const bounds: PVec2); cdecl; external 'obs.dll' delayed;

function obs_sceneitem_get_source(item: obs_sceneitem_t): obs_source_t;
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Encoders
// -----------------------------------------------------------------------

function obs_video_encoder_create(id: PAnsiChar; name: PAnsiChar;
  settings: obs_data_t; hotkey_data: obs_data_t): obs_encoder_t;
  cdecl; external 'obs.dll' delayed;

function obs_audio_encoder_create(id: PAnsiChar; name: PAnsiChar;
  settings: obs_data_t; mixer_idx: NativeUInt;
  hotkey_data: obs_data_t): obs_encoder_t;
  cdecl; external 'obs.dll' delayed;

// Enumera IDs de encoders registrados. idx 0..N, retorna False quando
// acabar. *id aponta pra string estatica do libobs.
function obs_enum_encoder_types(idx: NativeUInt; var id: PAnsiChar): ByteBool;
  cdecl; external 'obs.dll' delayed;

procedure obs_encoder_release(encoder: obs_encoder_t);
  cdecl; external 'obs.dll' delayed;

procedure obs_encoder_set_video(encoder: obs_encoder_t; video: video_t);
  cdecl; external 'obs.dll' delayed;

procedure obs_encoder_set_audio(encoder: obs_encoder_t; audio: audio_t);
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Outputs
// -----------------------------------------------------------------------

function obs_output_create(id: PAnsiChar; name: PAnsiChar;
  settings: obs_data_t; hotkey_data: obs_data_t): obs_output_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_output_release(output: obs_output_t);
  cdecl; external 'obs.dll' delayed;

procedure obs_output_set_video_encoder(output: obs_output_t;
  encoder: obs_encoder_t); cdecl; external 'obs.dll' delayed;

procedure obs_output_set_audio_encoder(output: obs_output_t;
  encoder: obs_encoder_t; idx: NativeUInt);
  cdecl; external 'obs.dll' delayed;

function obs_output_start(output: obs_output_t): ByteBool;
  cdecl; external 'obs.dll' delayed;

procedure obs_output_stop(output: obs_output_t);
  cdecl; external 'obs.dll' delayed;

function obs_output_active(output: obs_output_t): ByteBool;
  cdecl; external 'obs.dll' delayed;

function obs_output_get_last_error(output: obs_output_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Properties (enumeracao de monitor_id, device_id, etc.)
// -----------------------------------------------------------------------

function obs_get_source_properties(id: PAnsiChar): obs_properties_t;
  cdecl; external 'obs.dll' delayed;

function obs_source_properties(source: obs_source_t): obs_properties_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_properties_destroy(props: obs_properties_t);
  cdecl; external 'obs.dll' delayed;

function obs_properties_first(props: obs_properties_t): obs_property_t;
  cdecl; external 'obs.dll' delayed;

function obs_properties_get(props: obs_properties_t;
  prop_name: PAnsiChar): obs_property_t;
  cdecl; external 'obs.dll' delayed;

function obs_property_name(p: obs_property_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;

function obs_property_next(var p: obs_property_t): ByteBool;
  cdecl; external 'obs.dll' delayed;

function obs_property_list_item_count(p: obs_property_t): NativeUInt;
  cdecl; external 'obs.dll' delayed;

function obs_property_list_item_name(p: obs_property_t;
  idx: NativeUInt): PAnsiChar; cdecl; external 'obs.dll' delayed;

function obs_property_list_item_string(p: obs_property_t;
  idx: NativeUInt): PAnsiChar; cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

function MakeVec2(AX, AY: Single): TVec2; inline;

implementation

function MakeVec2(AX, AY: Single): TVec2;
begin
  Result.x := AX;
  Result.y := AY;
end;

end.

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
  signal_handler_t = type Pointer;
  proc_handler_t   = type Pointer;
  calldata_t       = type Pointer;

  // struct calldata (callback/calldata.h), pra quem CHAMA um procedimento e
  // precisa de um calldata proprio. calldata_t acima e o PONTEIRO (e assim
  // que os callbacks o recebem). Zerado = calldata_init (que e inline no C).
  // Se o procedimento gravar algo (ex.: get_last_replay), 'stack' vem
  // alocado pelo bmalloc da obs.dll e TEM que voltar por bfree
  // (calldata_free tambem e inline).
  TObsCallData = record
    stack:    Pointer;
    size:     NativeUInt;
    capacity: NativeUInt;
    fixed:    ByteBool;
    _pad:     array[0..6] of Byte;
  end;

// Callback de sinal do libobs (callback/signal.h):
//   typedef void (*signal_callback_t)(void *data, calldata_t *cd);
// cdecl. Chamado numa thread INTERNA do libobs — o handler so pode
// fazer trabalho marshalado pra main thread (TThread.Queue).
type
  TOBSSignalCallback = procedure(data: Pointer; cd: calldata_t); cdecl;

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

// Estatisticas do pipeline de video (monitor de desempenho do OBSBridge).
// Contadores ACUMULADOS desde o obs_reset_video — quem usa faz o delta.
//   lagged  = quadros que a RENDERIZACAO nao montou a tempo (GPU ocupada)
//   skipped = quadros que o ENCODER nao consumiu a tempo
function obs_get_total_frames: Cardinal; cdecl; external 'obs.dll' delayed;
function obs_get_lagged_frames: Cardinal; cdecl; external 'obs.dll' delayed;
function obs_get_average_frame_time_ns: UInt64; cdecl; external 'obs.dll' delayed;
function obs_get_frame_interval_ns: UInt64; cdecl; external 'obs.dll' delayed;
function video_output_get_skipped_frames(video: video_t): Cardinal;
  cdecl; external 'obs.dll' delayed;
function video_output_get_total_frames(video: video_t): Cardinal;
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

// Cenas sao enumeradas separadamente das sources (obs_enum_sources nao
// as inclui). Mesmo tipo de callback.
procedure obs_enum_scenes(enum_proc: obs_enum_sources_proc;
  param: Pointer); cdecl; external 'obs.dll' delayed;

// Remove a source do core (sinaliza "removed" e tira das listas internas).
// Diferente de obs_source_release, que so solta UMA referencia nossa: se o
// core ainda tem a source registrada, quem tem que desmonta-la e o
// obs_shutdown — e e ai que ele trava. O frontend do OBS chama isto em
// TODA source antes de encerrar (ClearSceneData em OBSBasic_SceneCollections).
procedure obs_source_remove(source: obs_source_t); cdecl;
  external 'obs.dll' delayed;

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

// ID do tipo com que o encoder foi criado ('obs_x264', 'av1_texture_amf',
// 'ffmpeg_svt_av1', ...). Devolve ponteiro pra string estatica do libobs,
// ou nil se o handle nao for valido.
function obs_encoder_get_id(encoder: obs_encoder_t): PAnsiChar;
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
// Sinais do output (callback/signal.h). O output emite "stop" quando a
// gravacao terminou de verdade (encoders drenados, trailer/cues escritos,
// arquivo completo, threads internas encerradas) — mesmo evento que o
// frontend do OBS usa pra so entao liberar e processar o arquivo.
// -----------------------------------------------------------------------

function obs_output_get_signal_handler(output: obs_output_t): signal_handler_t;
  cdecl; external 'obs.dll' delayed;

procedure signal_handler_connect(handler: signal_handler_t; const signal: PAnsiChar;
  callback: TOBSSignalCallback; data: Pointer);
  cdecl; external 'obs.dll' delayed;

procedure signal_handler_disconnect(handler: signal_handler_t; const signal: PAnsiChar;
  callback: TOBSSignalCallback; data: Pointer);
  cdecl; external 'obs.dll' delayed;

// -----------------------------------------------------------------------
// Procedimentos do output (callback/proc.h). O replay_buffer registra
// "save" (grava o buffer em arquivo, assincrono — conclui no sinal
// "saved") e "get_last_replay" (out string path).
// -----------------------------------------------------------------------

function obs_output_get_proc_handler(output: obs_output_t): proc_handler_t;
  cdecl; external 'obs.dll' delayed;

function proc_handler_call(handler: proc_handler_t; const name: PAnsiChar;
  params: calldata_t): ByteBool; cdecl; external 'obs.dll' delayed;

// Exportada (as outras calldata_get_* sao inline no C). 'str' aponta pra
// dentro do stack do calldata — copie antes de liberar.
function calldata_get_string(data: calldata_t; const name: PAnsiChar;
  str: PPAnsiChar): ByteBool; cdecl; external 'obs.dll' delayed;

// Alocador da obs.dll (util/bmem.h). Memoria alocada pela obs.dll volta
// por aqui, nunca pelo FreeMem do Delphi.
procedure bfree(ptr: Pointer); cdecl; external 'obs.dll' delayed;

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
// Filtros de audio (OBSAudioFilters): tipos registrados, editor generico
// de propriedades e captura do audio de uma fonte pro teste.
// -----------------------------------------------------------------------

const
  // Flags de saida de source (obs-source.h)
  OBS_SOURCE_AUDIO        = 1 shl 1;
  OBS_SOURCE_DEPRECATED   = 1 shl 8;
  OBS_SOURCE_CAP_DISABLED = 1 shl 10;   // = OBS_SOURCE_CAP_OBSOLETE

  // enum obs_property_type (obs-properties.h)
  OBS_PROPERTY_INVALID = 0;
  OBS_PROPERTY_BOOL    = 1;
  OBS_PROPERTY_INT     = 2;
  OBS_PROPERTY_FLOAT   = 3;
  OBS_PROPERTY_TEXT    = 4;
  OBS_PROPERTY_LIST    = 6;
  OBS_PROPERTY_GROUP   = 12;

  // enum obs_combo_format
  OBS_COMBO_FORMAT_INT    = 1;
  OBS_COMBO_FORMAT_FLOAT  = 2;
  OBS_COMBO_FORMAT_STRING = 3;
  OBS_COMBO_FORMAT_BOOL   = 4;

  // enum obs_number_type
  OBS_NUMBER_SCROLLER = 0;
  OBS_NUMBER_SLIDER   = 1;

  // enum obs_text_type
  OBS_TEXT_INFO = 3;

  MAX_AV_PLANES = 8;

type
  // struct audio_data (media-io/audio-io.h). Planos float (FLTP) no
  // formato do audio do OBS (48 kHz, estereo no NoOBS).
  TObsAudioData = record
    data:      array[0..MAX_AV_PLANES - 1] of PByte;
    frames:    Cardinal;
    timestamp: UInt64;
  end;
  PObsAudioData = ^TObsAudioData;

  // typedef void (*obs_source_audio_capture_t)(void *param,
  //   obs_source_t *source, const struct audio_data *audio_data, bool muted);
  // Chamado na THREAD DE CAPTURA da fonte, ja DEPOIS dos filtros.
  TObsAudioCaptureCallback = procedure(param: Pointer; source: obs_source_t;
    audio: PObsAudioData; muted: ByteBool); cdecl;

const
  // enum audio_format (media-io/audio-io.h)
  AUDIO_FORMAT_FLOAT_PLANAR = 8;

type
  // struct obs_source_audio (obs.h): audio EMPURRADO numa fonte por
  // obs_source_output_audio. 64 + 4*4 = 80, timestamp em 80, total 88.
  TObsSourceAudio = record
    data:            array[0..MAX_AV_PLANES - 1] of Pointer;
    frames:          Cardinal;
    speakers:        Integer;
    format:          Integer;
    samples_per_sec: Cardinal;
    timestamp:       UInt64;
  end;
  PObsSourceAudio = ^TObsSourceAudio;

// Registra um tipo de fonte. ASize = quantos bytes de obs_source_info o
// chamador preencheu; a libobs zera o resto (obs-module.c:958).
procedure obs_register_source_s(info: Pointer; size: NativeUInt);
  cdecl; external 'obs.dll' delayed;
// Empurra audio numa fonte: passa pelos filtros dela e chega nos callbacks
// de captura ANTES de voltar (sincrono, na thread de quem chama).
procedure obs_source_output_audio(source: obs_source_t; audio: PObsSourceAudio);
  cdecl; external 'obs.dll' delayed;
// Relogio da libobs (util/platform.h), em nanossegundos.
function os_gettime_ns: UInt64; cdecl; external 'obs.dll' delayed;

procedure obs_set_locale(locale: PAnsiChar); cdecl; external 'obs.dll' delayed;

function obs_enum_filter_types(idx: NativeUInt; var id: PAnsiChar): ByteBool;
  cdecl; external 'obs.dll' delayed;
function obs_get_source_output_flags(id: PAnsiChar): Cardinal;
  cdecl; external 'obs.dll' delayed;
function obs_source_get_display_name(id: PAnsiChar): PAnsiChar;
  cdecl; external 'obs.dll' delayed;
function obs_get_source_defaults(id: PAnsiChar): obs_data_t;
  cdecl; external 'obs.dll' delayed;
function obs_source_create_private(id: PAnsiChar; name: PAnsiChar;
  settings: obs_data_t): obs_source_t; cdecl; external 'obs.dll' delayed;
// Pega a PROPRIA referencia do filtro: quem criou libera a sua depois.
procedure obs_source_filter_add(source: obs_source_t; filter: obs_source_t);
  cdecl; external 'obs.dll' delayed;
procedure obs_source_add_audio_capture_callback(source: obs_source_t;
  callback: TObsAudioCaptureCallback; param: Pointer);
  cdecl; external 'obs.dll' delayed;
procedure obs_source_remove_audio_capture_callback(source: obs_source_t;
  callback: TObsAudioCaptureCallback; param: Pointer);
  cdecl; external 'obs.dll' delayed;

function obs_data_create_from_json(json_string: PAnsiChar): obs_data_t;
  cdecl; external 'obs.dll' delayed;
function obs_data_get_json(data: obs_data_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;
procedure obs_data_apply(target: obs_data_t; apply_data: obs_data_t);
  cdecl; external 'obs.dll' delayed;
procedure obs_data_set_double(data: obs_data_t; name: PAnsiChar; val: Double);
  cdecl; external 'obs.dll' delayed;
function obs_data_get_double(data: obs_data_t; name: PAnsiChar): Double;
  cdecl; external 'obs.dll' delayed;
function obs_data_get_bool(data: obs_data_t; name: PAnsiChar): ByteBool;
  cdecl; external 'obs.dll' delayed;

// Configuracoes da fonte COM os defaults do plugin. Referencia nova: quem
// chama libera com obs_data_release.
function obs_source_get_settings(source: obs_source_t): obs_data_t;
  cdecl; external 'obs.dll' delayed;

procedure obs_properties_apply_settings(props: obs_properties_t;
  settings: obs_data_t); cdecl; external 'obs.dll' delayed;
// Roda o callback "modificado" do campo (o que presets usam pra preencher
// outros campos). True = a lista de propriedades precisa ser refeita.
function obs_property_modified(p: obs_property_t; settings: obs_data_t): ByteBool;
  cdecl; external 'obs.dll' delayed;
function obs_property_get_type(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_description(p: obs_property_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;
function obs_property_long_description(p: obs_property_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;
function obs_property_visible(p: obs_property_t): ByteBool;
  cdecl; external 'obs.dll' delayed;
function obs_property_enabled(p: obs_property_t): ByteBool;
  cdecl; external 'obs.dll' delayed;
function obs_property_int_min(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_int_max(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_int_step(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_int_type(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_int_suffix(p: obs_property_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;
function obs_property_float_min(p: obs_property_t): Double;
  cdecl; external 'obs.dll' delayed;
function obs_property_float_max(p: obs_property_t): Double;
  cdecl; external 'obs.dll' delayed;
function obs_property_float_step(p: obs_property_t): Double;
  cdecl; external 'obs.dll' delayed;
function obs_property_float_type(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_float_suffix(p: obs_property_t): PAnsiChar;
  cdecl; external 'obs.dll' delayed;
function obs_property_text_type(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_list_format(p: obs_property_t): Integer;
  cdecl; external 'obs.dll' delayed;
function obs_property_list_item_int(p: obs_property_t; idx: NativeUInt): Int64;
  cdecl; external 'obs.dll' delayed;
function obs_property_list_item_float(p: obs_property_t; idx: NativeUInt): Double;
  cdecl; external 'obs.dll' delayed;
function obs_property_list_item_disabled(p: obs_property_t;
  idx: NativeUInt): ByteBool; cdecl; external 'obs.dll' delayed;
function obs_property_group_content(p: obs_property_t): obs_properties_t;
  cdecl; external 'obs.dll' delayed;

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

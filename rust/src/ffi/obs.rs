//! Bindings do libobs (porte de `LibOBS.pas`). Cabeçalhos de referência:
//! <https://github.com/obsproject/obs-studio> (`libobs/obs.h`, `obs-data.h`,
//! `obs-properties.h`, `callback/signal.h`, `callback/proc.h`).
//!
//! REGRA (pegadinha #3): toda chamada ao libobs sai da MAIN thread —
//! inclusive `obs_shutdown`, que fecha o STA aberto pelo `obs_startup`.

use std::ffi::{c_char, c_void, CString};
use std::sync::OnceLock;

use crate::dll_api;

// --- tipos opacos -----------------------------------------------------------
pub type ObsSource = *mut c_void;
pub type ObsScene = *mut c_void;
pub type ObsSceneItem = *mut c_void;
pub type ObsOutput = *mut c_void;
pub type ObsEncoder = *mut c_void;
pub type ObsDataRaw = *mut c_void;
pub type ObsProperties = *mut c_void;
pub type ObsProperty = *mut c_void;
pub type VideoT = *mut c_void;
pub type AudioT = *mut c_void;
pub type SignalHandler = *mut c_void;
pub type ProcHandler = *mut c_void;
pub type CalldataPtr = *mut c_void;

/// `struct calldata` (callback/calldata.h) para quem CHAMA um procedimento.
/// `calldata_init`/`calldata_free` são `static inline` no header — não existem
/// na DLL: nasce zerado e o `stack` volta por `bfree`.
#[repr(C)]
pub struct CallData {
    pub stack: *mut c_void,
    pub size: usize,
    pub capacity: usize,
    pub fixed: bool,
    _pad: [u8; 7],
}

impl CallData {
    pub const fn zeroed() -> Self {
        Self { stack: std::ptr::null_mut(), size: 0, capacity: 0, fixed: false, _pad: [0; 7] }
    }
}

/// `void (*signal_callback_t)(void *data, calldata_t *cd)` — chamado numa
/// thread INTERNA do libobs: só pode enfileirar trabalho para a main.
pub type SignalCallback = unsafe extern "C" fn(data: *mut c_void, cd: CalldataPtr);
/// `bool (*)(void *param, obs_source_t *source)`; `true` continua a enumeração.
pub type EnumSourceCallback = unsafe extern "C" fn(param: *mut c_void, source: ObsSource) -> bool;
pub type LogHandler =
    unsafe extern "C" fn(level: i32, msg: *const c_char, args: *mut c_void, p: *mut c_void);
pub type AudioCaptureCallback = unsafe extern "C" fn(
    param: *mut c_void,
    source: ObsSource,
    audio: *const AudioData,
    muted: bool,
);

pub const LOG_ERROR: i32 = 100;
pub const LOG_WARNING: i32 = 200;

// --- enums (MSVC x64: 4 bytes) ---------------------------------------------
pub const VIDEO_FORMAT_NV12: i32 = 2;
pub const VIDEO_CS_709: i32 = 2;
pub const VIDEO_RANGE_PARTIAL: i32 = 1;
pub const OBS_SCALE_BICUBIC: i32 = 2;
pub const SPEAKERS_STEREO: i32 = 2;
pub const OBS_BOUNDS_STRETCH: i32 = 1;
pub const OBS_BOUNDS_SCALE_INNER: i32 = 2;
pub const OBS_VIDEO_SUCCESS: i32 = 0;

pub const OBS_SOURCE_AUDIO: u32 = 1 << 1;
pub const OBS_SOURCE_DEPRECATED: u32 = 1 << 8;
pub const OBS_SOURCE_CAP_DISABLED: u32 = 1 << 10;

pub const OBS_PROPERTY_BOOL: i32 = 1;
pub const OBS_PROPERTY_INT: i32 = 2;
pub const OBS_PROPERTY_FLOAT: i32 = 3;
pub const OBS_PROPERTY_TEXT: i32 = 4;
pub const OBS_PROPERTY_LIST: i32 = 6;
pub const OBS_PROPERTY_GROUP: i32 = 12;

pub const MAX_AV_PLANES: usize = 8;
pub const AUDIO_FORMAT_FLOAT_PLANAR: i32 = 8;

// --- structs ----------------------------------------------------------------
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

/// `struct obs_sceneitem_crop`: pixels cortados de cada lado da fonte.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct SceneItemCrop {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// `struct obs_video_info`. O `repr(C)` já insere os 3 bytes de padding depois
/// de `gpu_conversion` (pegadinha #18); o `_pad0` explícito documenta isso.
#[repr(C)]
pub struct ObsVideoInfo {
    pub graphics_module: *const c_char,
    pub fps_num: u32,
    pub fps_den: u32,
    pub base_width: u32,
    pub base_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub output_format: i32,
    pub adapter: u32,
    pub gpu_conversion: bool,
    pub _pad0: [u8; 3],
    pub colorspace: i32,
    pub range: i32,
    pub scale_type: i32,
}

#[repr(C)]
pub struct ObsAudioInfo {
    pub samples_per_sec: u32,
    pub speakers: i32,
}

/// `struct audio_data` (media-io/audio-io.h).
#[repr(C)]
pub struct AudioData {
    pub data: [*mut u8; MAX_AV_PLANES],
    pub frames: u32,
    pub timestamp: u64,
}

/// `struct obs_source_audio` (obs.h): 64 + 4*4 = 80, timestamp em 80, total 88.
#[repr(C)]
pub struct ObsSourceAudio {
    pub data: [*const c_void; MAX_AV_PLANES],
    pub frames: u32,
    pub speakers: i32,
    pub format: i32,
    pub samples_per_sec: u32,
    pub timestamp: u64,
}

dll_api! {
    /// Tabela de funções da `obs.dll`.
    pub struct ObsApi;

    fn base_set_log_handler(handler: LogHandler, param: *mut c_void);

    // ciclo de vida
    fn obs_startup(locale: *const c_char, module_config_path: *const c_char, store: *mut c_void) -> bool;
    fn obs_shutdown();
    fn obs_reset_video(ovi: *mut ObsVideoInfo) -> i32;
    fn obs_reset_audio(oai: *const ObsAudioInfo) -> bool;
    fn obs_set_locale(locale: *const c_char);

    // módulos (obs_open_module + obs_init_module por plugin, não
    // obs_load_all_modules: filtra o obs-websocket, que crasha — pegadinha #13)
    fn obs_open_module(module: *mut *mut c_void, path: *const c_char, data_path: *const c_char) -> i32;
    fn obs_init_module(module: *mut c_void) -> bool;
    fn obs_post_load_modules();

    // vídeo/áudio
    fn obs_get_video() -> VideoT;
    fn obs_get_audio() -> AudioT;
    fn obs_get_total_frames() -> u32;
    fn obs_get_lagged_frames() -> u32;
    fn obs_get_average_frame_time_ns() -> u64;
    fn obs_get_frame_interval_ns() -> u64;
    fn video_output_get_skipped_frames(video: VideoT) -> u32;
    fn video_output_get_total_frames(video: VideoT) -> u32;
    fn obs_set_output_source(channel: u32, source: ObsSource);
    fn obs_enum_scenes(cb: EnumSourceCallback, param: *mut c_void);
    fn obs_enum_sources(cb: EnumSourceCallback, param: *mut c_void);

    // obs_data
    fn obs_data_create() -> ObsDataRaw;
    fn obs_data_release(data: ObsDataRaw);
    fn obs_data_set_string(data: ObsDataRaw, name: *const c_char, val: *const c_char);
    fn obs_data_set_int(data: ObsDataRaw, name: *const c_char, val: i64);
    fn obs_data_set_bool(data: ObsDataRaw, name: *const c_char, val: bool);
    fn obs_data_set_double(data: ObsDataRaw, name: *const c_char, val: f64);
    fn obs_data_get_string(data: ObsDataRaw, name: *const c_char) -> *const c_char;
    fn obs_data_get_int(data: ObsDataRaw, name: *const c_char) -> i64;
    fn obs_data_get_double(data: ObsDataRaw, name: *const c_char) -> f64;
    fn obs_data_get_bool(data: ObsDataRaw, name: *const c_char) -> bool;
    fn obs_data_create_from_json(json: *const c_char) -> ObsDataRaw;
    fn obs_data_get_json(data: ObsDataRaw) -> *const c_char;
    fn obs_data_apply(target: ObsDataRaw, apply: ObsDataRaw);

    // sources
    fn obs_source_create(id: *const c_char, name: *const c_char, settings: ObsDataRaw, hotkey: ObsDataRaw) -> ObsSource;
    fn obs_source_create_private(id: *const c_char, name: *const c_char, settings: ObsDataRaw) -> ObsSource;
    fn obs_source_release(source: ObsSource);
    fn obs_source_remove(source: ObsSource);
    fn obs_source_update(source: ObsSource, settings: ObsDataRaw);
    fn obs_source_set_muted(source: ObsSource, muted: bool);
    fn obs_source_set_audio_mixers(source: ObsSource, mixers: u32);
    fn obs_source_get_settings(source: ObsSource) -> ObsDataRaw;
    fn obs_get_latest_input_type_id(unversioned_id: *const c_char) -> *const c_char;
    fn obs_source_filter_add(source: ObsSource, filter: ObsSource);
    fn obs_source_output_audio(source: ObsSource, audio: *const ObsSourceAudio);
    fn obs_source_add_audio_capture_callback(source: ObsSource, cb: AudioCaptureCallback, param: *mut c_void);
    fn obs_source_remove_audio_capture_callback(source: ObsSource, cb: AudioCaptureCallback, param: *mut c_void);
    fn obs_register_source_s(info: *const c_void, size: usize);
    fn obs_enum_filter_types(idx: usize, id: *mut *const c_char) -> bool;
    fn obs_get_source_output_flags(id: *const c_char) -> u32;
    fn obs_source_get_display_name(id: *const c_char) -> *const c_char;
    fn os_gettime_ns() -> u64;

    // scenes
    fn obs_scene_create(name: *const c_char) -> ObsScene;
    fn obs_scene_release(scene: ObsScene);
    fn obs_scene_get_source(scene: ObsScene) -> ObsSource;
    fn obs_scene_add(scene: ObsScene, source: ObsSource) -> ObsSceneItem;
    fn obs_sceneitem_set_pos(item: ObsSceneItem, pos: *const Vec2);
    fn obs_sceneitem_set_scale(item: ObsSceneItem, scale: *const Vec2);
    fn obs_sceneitem_set_bounds_type(item: ObsSceneItem, ty: i32);
    fn obs_sceneitem_set_bounds(item: ObsSceneItem, bounds: *const Vec2);
    fn obs_sceneitem_set_crop(item: ObsSceneItem, crop: *const SceneItemCrop);
    fn obs_sceneitem_set_visible(item: ObsSceneItem, visible: bool) -> bool;

    // encoders
    fn obs_video_encoder_create(id: *const c_char, name: *const c_char, settings: ObsDataRaw, hotkey: ObsDataRaw) -> ObsEncoder;
    fn obs_audio_encoder_create(id: *const c_char, name: *const c_char, settings: ObsDataRaw, mixer_idx: usize, hotkey: ObsDataRaw) -> ObsEncoder;
    fn obs_enum_encoder_types(idx: usize, id: *mut *const c_char) -> bool;
    fn obs_encoder_release(encoder: ObsEncoder);
    fn obs_encoder_get_id(encoder: ObsEncoder) -> *const c_char;
    fn obs_encoder_set_video(encoder: ObsEncoder, video: VideoT);
    fn obs_encoder_set_audio(encoder: ObsEncoder, audio: AudioT);

    // outputs
    fn obs_output_create(id: *const c_char, name: *const c_char, settings: ObsDataRaw, hotkey: ObsDataRaw) -> ObsOutput;
    fn obs_output_release(output: ObsOutput);
    fn obs_output_set_video_encoder(output: ObsOutput, encoder: ObsEncoder);
    fn obs_output_set_audio_encoder(output: ObsOutput, encoder: ObsEncoder, idx: usize);
    fn obs_output_start(output: ObsOutput) -> bool;
    fn obs_output_stop(output: ObsOutput);
    fn obs_output_active(output: ObsOutput) -> bool;
    // Pausa/retoma uma saída com OBS_OUTPUT_CAN_PAUSE (o ffmpeg_muxer tem). A
    // pausa é nos ENCODERS da saída: o intervalo pausado não vira pacote e os
    // timestamps seguintes são descontados dele (obs-output.c).
    fn obs_output_pause(output: ObsOutput, pause: bool) -> bool;
    fn obs_output_get_last_error(output: ObsOutput) -> *const c_char;

    // sinais e procedimentos (stop assíncrono — #41; replay_buffer — #62)
    fn obs_output_get_signal_handler(output: ObsOutput) -> SignalHandler;
    fn signal_handler_connect(handler: SignalHandler, signal: *const c_char, cb: SignalCallback, data: *mut c_void);
    fn signal_handler_disconnect(handler: SignalHandler, signal: *const c_char, cb: SignalCallback, data: *mut c_void);
    fn obs_output_get_proc_handler(output: ObsOutput) -> ProcHandler;
    fn proc_handler_call(handler: ProcHandler, name: *const c_char, params: CalldataPtr) -> bool;
    fn calldata_get_string(data: CalldataPtr, name: *const c_char, out: *mut *const c_char) -> bool;
    fn bfree(ptr: *mut c_void);

    // properties
    fn obs_get_source_properties(id: *const c_char) -> ObsProperties;
    fn obs_source_properties(source: ObsSource) -> ObsProperties;
    fn obs_properties_destroy(props: ObsProperties);
    fn obs_properties_first(props: ObsProperties) -> ObsProperty;
    fn obs_properties_get(props: ObsProperties, name: *const c_char) -> ObsProperty;
    fn obs_properties_apply_settings(props: ObsProperties, settings: ObsDataRaw);
    fn obs_property_name(p: ObsProperty) -> *const c_char;
    fn obs_property_next(p: *mut ObsProperty) -> bool;
    fn obs_property_get_type(p: ObsProperty) -> i32;
    fn obs_property_description(p: ObsProperty) -> *const c_char;
    fn obs_property_long_description(p: ObsProperty) -> *const c_char;
    fn obs_property_visible(p: ObsProperty) -> bool;
    fn obs_property_enabled(p: ObsProperty) -> bool;
    fn obs_property_modified(p: ObsProperty, settings: ObsDataRaw) -> bool;
    fn obs_property_int_min(p: ObsProperty) -> i32;
    fn obs_property_int_max(p: ObsProperty) -> i32;
    fn obs_property_int_step(p: ObsProperty) -> i32;
    fn obs_property_int_type(p: ObsProperty) -> i32;
    fn obs_property_int_suffix(p: ObsProperty) -> *const c_char;
    fn obs_property_float_min(p: ObsProperty) -> f64;
    fn obs_property_float_max(p: ObsProperty) -> f64;
    fn obs_property_float_step(p: ObsProperty) -> f64;
    fn obs_property_float_type(p: ObsProperty) -> i32;
    fn obs_property_float_suffix(p: ObsProperty) -> *const c_char;
    fn obs_property_text_type(p: ObsProperty) -> i32;
    fn obs_property_group_content(p: ObsProperty) -> ObsProperties;
    fn obs_property_list_format(p: ObsProperty) -> i32;
    fn obs_property_list_item_count(p: ObsProperty) -> usize;
    fn obs_property_list_item_name(p: ObsProperty, idx: usize) -> *const c_char;
    fn obs_property_list_item_string(p: ObsProperty, idx: usize) -> *const c_char;
    fn obs_property_list_item_int(p: ObsProperty, idx: usize) -> i64;
    fn obs_property_list_item_float(p: ObsProperty, idx: usize) -> f64;
    fn obs_property_list_item_disabled(p: ObsProperty, idx: usize) -> bool;
}

/// `obs.dll` carregada sob demanda (resolve a partir do diretório do exe).
pub fn api() -> Result<&'static ObsApi, String> {
    static API: OnceLock<Result<ObsApi, String>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let lib = libloading::Library::new("obs.dll").map_err(|e| format!("obs.dll: {e}"))?;
        ObsApi::load(lib).map_err(|e| format!("obs.dll (símbolo): {e}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Texto → C string UTF-8. Bytes nulos internos (não esperados) são cortados.
pub fn cstring(s: &str) -> CString {
    CString::new(s.split('\0').next().unwrap_or("")).unwrap_or_default()
}

/// `obs_data_t` com liberação automática (o `try/finally` + `obs_data_release`
/// do Delphi vira `Drop`).
pub struct ObsData(ObsDataRaw);

impl ObsData {
    pub fn create() -> Result<Self, String> {
        let a = api()?;
        Ok(Self(unsafe { (a.obs_data_create)() }))
    }

    pub fn raw(&self) -> ObsDataRaw {
        self.0
    }

    pub fn set_string(&self, key: &str, val: &str) {
        if let Ok(a) = api() {
            unsafe { (a.obs_data_set_string)(self.0, cstring(key).as_ptr(), cstring(val).as_ptr()) }
        }
    }

    pub fn set_int(&self, key: &str, val: i64) {
        if let Ok(a) = api() {
            unsafe { (a.obs_data_set_int)(self.0, cstring(key).as_ptr(), val) }
        }
    }

    pub fn set_bool(&self, key: &str, val: bool) {
        if let Ok(a) = api() {
            unsafe { (a.obs_data_set_bool)(self.0, cstring(key).as_ptr(), val) }
        }
    }

}

impl Drop for ObsData {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.obs_data_release)(self.0) }
        }
    }
}

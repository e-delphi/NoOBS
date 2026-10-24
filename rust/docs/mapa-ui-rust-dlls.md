# Mapa: página → Rust → DLLs

> **Histórico.** Este levantamento é de quando a obs.dll ainda era o motor
> de gravação. A libobs saiu na fase 7 do `motor-proprio.md`: as partes
> sobre `ffi/obs.rs`, `audio_filters`, RTWQ e a suíte `tests/obs.rs` (hoje
> `tests/mensagens.rs`) descrevem código que não existe mais. As tabelas do
> FFmpeg e das mensagens continuam úteis como ponto de partida.
>
> E a página saiu também (`ui-slint.md`): a interface é Slint e chama os
> handlers direto. As "mensagens" das tabelas são a entrada JSON que sobrou
> só pros testes (`bridge::dispatch`); as que só a página web usava (tocar
> por URL, transcodificar, faixas em M4A, keyframes, validar atalho, abrir
> gravação, hibernação) não existem mais.

Levantamento feito a partir do código (outubro de 2026), como base para a
cobertura de testes. As tabelas A, B e C foram **geradas por script** a partir
de `src/bridge.rs` e das declarações `dll_api!` em `src/ffi/`; se o código
mudar, elas envelhecem.

## Resumo

| Camada | Quantidade | Onde está |
|---|---|---|
| Mensagens que a página manda ao Rust | **114** tipos | `dispatch` em `src/bridge.rs` → handlers em `src/bridge/*.rs` |
| Funções do FFmpeg chamadas | **64** (avformat 15, avcodec 23, avutil 23, swscale 3) | `src/ffi/ffmpeg.rs` |
| Funções da obs.dll chamadas | **117** | `src/ffi/obs.rs` |
| Funções Rust que falam direto com as DLLs | **86** | `ffmpeg_ops`, `ffmpeg_export`, `probe`, `engine`, `encoder`, `audio_filters`, `audio_tracks`, `ffi/` |
| Testes que exercitam uma DLL de verdade | **0** | os 110 testes atuais são de funções puras |

**Como as DLLs são carregadas:** cada `dll_api!` resolve TODAS as funções da
tabela na primeira chamada (`libloading`). Faltar um símbolo derruba o
carregamento inteiro — um teste que só carrega as DLLs já valida as 181
assinaturas por nome (não por tipo).

## Achados do levantamento

1. **Nenhuma mensagem enviada pela página fica sem tratamento.** O cruzamento
   de todos os `Bridge.send('…')` dos `js/*.js` com o `dispatch` não deixa
   sobra.
2. **Duas mensagens tratadas nunca são enviadas pela página** (⚠️ na Tabela
   A): `open_recording` e `cancel_countdown`. (O `set_check_updates` era a
   terceira: a página lia a caixa e não mandava — a opção nunca era salva.) A
   última ficou sem botão (a contagem do atraso se cancela pelo botão de
   gravar ou pelo atalho). Candidatas a remover ou a ganhar uso.
3. **Todas as 181 funções declaradas das DLLs são usadas** — não há binding
   morto.
4. **Funções de mídia sem nenhum teste hoje:** `probe::probe`,
   `ffmpeg_ops::{remux_file, extract_audio_tracks, merge_files,
   splice_continuation, extract_frame_jpeg, extract_frame_png,
   compute_audio_peaks, list_video_keyframes}`, `export_captions` inteiro,
   e o caminho de `export_video`. Os 9 testes de `ffmpeg_export` e os 2 de
   `ffmpeg_ops` cobrem só contas puras.
5. **80 das 114 mensagens não chegam em DLL nenhuma** (config, janela,
   transcrição, pastas). Essas dá pra testar sem `obs.dll` e sem FFmpeg,
   desde que o `post_json` possa ser capturado num teste (hoje ele vai direto
   pro WebView).

### Como ler a coluna "DLLs alcançadas"

Saiu de um grafo de chamadas por nome: a partir do handler, segue as funções
chamadas até achar uma que chama a DLL. É uma aproximação:

- nomes genéricos (`open`, `new`, `set`…) foram ignorados pra não ligar tudo
  em tudo;
- **¹** = marcado à mão: o trabalho roda numa worker (fila de transcrição,
  exportação) que o grafo por nome não segue;
- várias mensagens de configuração alcançam a obs.dll/FFmpeg porque, com a
  gravação ou o buffer ligados, elas reiniciam o buffer ou reempurram as caps
  de encoder (`ffmpeg_export::encoder_exists` vem da lista de encoders da
  exportação).

### Tabela A — 114 mensagens da página (`dispatch` em `src/bridge.rs`)


#### Gravação (19)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `record_start` | — | `recording::handle_record_start` | FFmpeg + obs.dll |
| `record_stop` | — | `recording::handle_record_stop` | obs.dll |
| `record_pause` | `paused` | `recording::handle_record_pause` | obs.dll |
| `set_rec_indicator` | `enabled` | `recording::handle_set_rec_indicator` | — |
| `set_rec_indicator_corner` | `corner` | `recording::handle_set_rec_indicator_corner` | — |
| `set_rec_indicator_opacity` | `opacity` | `recording::handle_set_rec_indicator_opacity` | — |
| `set_rec_indicator_clickable` | `enabled` | `recording::handle_set_rec_indicator_clickable` | — |
| `set_fullscreen_detect` | `enabled` | `recording::handle_set_fullscreen_detect` | — |
| `list_capture_windows` | — | `window_record::handle_list_capture_windows` | obs.dll |
| `record_window` | `appAudio`, `delaySec`, `id` | `window_record::handle_record_window` | FFmpeg + obs.dll |
| `select_capture_area` | — | `area::handle_select_capture_area` | — |
| `cancel_countdown` ⚠️ | — | `countdown::cancel` | — |
| `get_hotkeys` | — | `hotkeys::push_state` | — |
| `set_hotkeys` | `items` | `hotkeys::handle_set_hotkeys` | — |
| `reset_hotkeys` | — | `hotkeys::handle_reset` | — |
| `define_hotkey_area` | `index` | `hotkeys::handle_define_area` | — |
| `trigger_hotkey` | `index` | `hotkeys::trigger` | FFmpeg |
| `test_video_quality` | — | `video_test::handle_test_video_quality` | FFmpeg + obs.dll |
| `cancel_video_test` | — | `video_test::handle_cancel_video_test` | obs.dll |

#### Buffer em memória (11)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `get_replay_state` | — | `replay::push_replay_state` | — |
| `set_replay_enabled` | `enabled` | `replay::handle_set_replay_enabled` | FFmpeg + obs.dll |
| `save_replay` | — | `replay::handle_save_replay` | obs.dll |
| `set_replay_apps` | `apps` | `replay::handle_set_replay_apps` | obs.dll |
| `set_replay_indicator` | `enabled` | `replay::handle_set_replay_indicator` | — |
| `set_replay_indicator_corner` | `corner` | `replay::handle_set_replay_indicator_corner` | — |
| `set_replay_indicator_opacity` | `opacity` | `replay::handle_set_replay_indicator_opacity` | — |
| `set_replay_indicator_clickable` | `enabled` | `replay::handle_set_replay_indicator_clickable` | — |
| `set_replay_autostart` | `autoStart` | `replay::handle_set_replay_autostart` | — |
| `set_replay_into_recording` | `enabled` | `replay::handle_set_replay_into_recording` | — |
| `set_replay_limits` | `maxMb`, `maxSec` | `replay::handle_set_replay_limits` | — |

#### Dispositivos (4)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `toggle_source` | `enabled`, `id` | `devices::handle_toggle_source` | FFmpeg + obs.dll |
| `set_source_hidden` | `hidden`, `id` | `devices::handle_set_source_hidden` | FFmpeg + obs.dll |
| `set_auto_source` | `enabled`, `id` | `devices::handle_set_auto_source` | — |
| `player_state` | `open` | `devices::set_player_open_all` | — |

#### Biblioteca e player (16)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `rename_recording` | `id`, `newName` | `library::handle_rename_recording` | — |
| `open_recording` ⚠️ | `id` | `library::handle_open_recording` | — |
| `open_record_dir` | — | `library::handle_open_record_dir` | — |
| `play_recording` | `id` | `library::handle_play_recording` | — |
| `request_transcode` | `id` | `library::handle_request_transcode` | FFmpeg |
| `request_video_info` | `id` | `library::handle_request_video_info` | FFmpeg |
| `request_audio_tracks` | `id` | `library::handle_request_audio_tracks` | FFmpeg |
| `request_waveform` | `buckets`, `hi`, `id` | `library::handle_request_waveform` | FFmpeg |
| `request_keyframes` | `id` | `library::handle_request_keyframes` | FFmpeg |
| `delete_recording` | `id` | `library::handle_delete_recording` | — |
| `open_folder` | `id` | `folders::handle_open_folder` | FFmpeg |
| `create_folder` | `name` | `folders::handle_create_folder` | — |
| `rename_folder` | `id`, `newName` | `folders::handle_rename_folder` | — |
| `delete_folder` | `id` | `folders::handle_delete_folder` | — |
| `move_items` | `ids`, `target` | `folders::handle_move_items` | — |
| `merge_recordings` | `ids` | `folders::handle_merge_recordings` | FFmpeg |

#### Exportação (2)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `export_recording` | `audioStreams`, `burnCaptions`, `container`, `crf`, `crop`, `encoder`, `fps`, `id`, `keepTranscript`, `mixAudio`, `mixTrackIndex`, `name`, `noVideo`, `originalAudio`, `regions`, `scaleAlgo`, `segments`, `speed`, `targetHeight` | `export::handle_export_recording` | FFmpeg ¹ |
| `cancel_export` | — | `export::handle_cancel_export` | — |

#### Filtros de áudio (4)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `get_audio_filters` | — | `filters::handle_get_audio_filters` | FFmpeg + obs.dll |
| `set_audio_filter` | `id`, `op (enable/value/reset)`, `key`, `value` | `filters::handle_set_audio_filter` | FFmpeg + obs.dll |
| `reset_audio_filters` | — | `filters::handle_reset_audio_filters` | FFmpeg + obs.dll |
| `test_audio_filters` | `device` | `filters::handle_test_audio_filters` | FFmpeg + obs.dll |

#### Transcrição (18)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `set_transcribe_language` | `language` | `transcribe::handle_set_transcribe_language` | — |
| `set_transcribe_on_stop` | `enabled` | `transcribe::handle_set_transcribe_on_stop` | — |
| `local_asr_install` | — | `local_asr::start_install` | — |
| `local_asr_cancel` | — | `local_asr::cancel_install` | — |
| `local_asr_remove` | — | `transcribe::handle_local_asr_remove` | — |
| `get_local_asr_state` | — | `transcribe::push_local_asr_state` | — |
| `transcribe_recording` | `id` | `transcribe::handle_transcribe_recording` | FFmpeg ¹ |
| `transcribe_pending` | — | `transcribe::handle_transcribe_pending` | FFmpeg ¹ |
| `cancel_transcribe` | — | `transcribe::handle_cancel_transcribe` | — |
| `move_transcribe_item` | `id`, `to` | `transcribe::handle_move_transcribe_item` | — |
| `remove_transcribe_item` | `id` | `transcribe::handle_remove_transcribe_item` | — |
| `get_transcribe_state` | — | `transcribe::handle_get_transcribe_state` | FFmpeg |
| `request_transcript` | `id` | `transcribe::handle_request_transcript` | — |
| `set_speaker_name` | `id`, `name`, `speaker` | `transcribe::handle_set_speaker_name` | — |
| `search_transcripts` | `query` | `transcribe::handle_search_transcripts` | — |
| `get_root_folders` | — | `transcribe::push_root_folders` | — |
| `reset_hidden_lists` | — | `transcribe::handle_reset_hidden_lists` | — |
| `set_folder_hidden` | `hidden`, `name` | `transcribe::handle_set_folder_hidden` | — |

#### Configurações e janela (32)

| Mensagem | Campos | Handler | DLLs alcançadas |
|---|---|---|---|
| `ui_log` | `message` | `log!` | — |
| `ready` | — | `do_init` | — |
| `set_theme` | `theme` | `handle_set_theme` | — |
| `set_language` | `language` | `handle_set_language` | obs.dll |
| `open_url` | `url` | `handle_open_url` | — |
| `get_settings` | — | `settings::push_settings` | — |
| `set_codec` | `codec` | `settings::handle_set_codec` | FFmpeg + obs.dll |
| `set_recording_quality` | `level` | `settings::handle_set_recording_quality` | FFmpeg + obs.dll |
| `set_recording_fps` | `fps` | `settings::handle_set_recording_fps` | FFmpeg + obs.dll |
| `set_recording_keyframe` | `sec` | `settings::handle_set_recording_keyframe` | — |
| `set_audio_bitrate` | `kbps` | `settings::handle_set_audio_bitrate` | FFmpeg + obs.dll |
| `set_filename_pattern` | `pattern` | `settings::handle_set_filename_pattern` | — |
| `set_mute_when_device_muted` | `enabled` | `settings::handle_set_mute_when_device_muted` | obs.dll |
| `pick_record_dir` | — | `settings::handle_pick_record_dir` | — |
| `set_record_dir` | `path` | `settings::handle_set_record_dir` | FFmpeg |
| `set_library_thumbs` | `mode` | `settings::handle_set_library_thumbs` | FFmpeg |
| `set_auto_record_on_mic` | `enabled` | `settings::handle_set_auto_record_on_mic` | — |
| `set_auto_record_mic_apps` | `apps` | `settings::handle_set_auto_record_mic_apps` | — |
| `set_auto_record_mic_except` | `apps` | `settings::handle_set_auto_record_mic_except` | — |
| `check_updates` | — | `settings::handle_check_updates` | — |
| `set_check_updates` | `enabled` | `settings::handle_set_check_updates` | — |
| `tray_show` | — | `shell::handle_tray_show` | — |
| `toggle_fullscreen` | — | `ui::toggle_fullscreen` | — |
| `set_window_title` | `title` | `shell::handle_set_window_title` | — |
| `validate_hotkey` | `hotkey` | `shell::handle_validate_hotkey` | — |
| `set_autostart` | `enabled` | `shell::handle_set_autostart` | — |
| `set_run_as_admin` | `enabled` | `shell::handle_set_run_as_admin` | — |
| `set_close_to_tray` | `enabled` | `shell::handle_set_close_to_tray` | — |
| `set_hibernate` | `enabled` | `shell::handle_set_hibernate` | — |
| `set_minimize_on_record` | `enabled` | `shell::handle_set_config_bool` | — |
| `set_play_sound_on_record` | `enabled` | `shell::handle_set_config_bool` | — |
| `set_stop_on_lock` | `enabled` | `shell::handle_set_config_bool` | — |


### Tabela B — 181 funções das DLLs (`dll_api!` em `src/ffi/`)

"Quem chama" = a função Rust que contém a chamada (`arquivo::função`). As de `ffi/` são os embrulhos finos (RAII, `open`, `drop`).


#### avformat-61.dll — `AvFormatApi` (15)

| Função | Quem chama |
|---|---|
| `avformat_open_input` | `ffi/ffmpeg.rs::open`, `ffmpeg_ops.rs::extract_frame_jpeg_inner` |
| `avformat_find_stream_info` | `ffi/ffmpeg.rs::find_stream_info` |
| `avformat_close_input` | `ffi/ffmpeg.rs::drop`, `ffi/ffmpeg.rs::open` |
| `av_read_frame` | `ffi/ffmpeg.rs::estimate_fps_from_packets`, `ffi/ffmpeg.rs::scan_duration_by_packets`, `ffmpeg_export.rs::probe_video_decode`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::compute_audio_peaks`, `ffmpeg_ops.rs::decode_audio_mono`, `ffmpeg_ops.rs::extract_frame_jpeg_inner`, `ffmpeg_ops.rs::extract_frame_png_inner`, `ffmpeg_ops.rs::merge_files`, `ffmpeg_ops.rs::remux_dispatch`, `ffmpeg_ops.rs::splice_continuation` |
| `av_seek_frame` | `ffi/ffmpeg.rs::estimate_fps_from_packets`, `ffi/ffmpeg.rs::rewind`, `ffmpeg_export.rs::probe_video_decode`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::extract_frame_jpeg_inner` |
| `avformat_index_get_entries_count` | `ffmpeg_ops.rs::list_video_keyframes` |
| `avformat_index_get_entry` | `ffmpeg_ops.rs::list_video_keyframes` |
| `avformat_alloc_output_context2` | `ffmpeg_export.rs::alloc`, `ffmpeg_ops.rs::open_for_streams` |
| `avformat_new_stream` | `ffmpeg_export.rs::new_stream`, `ffmpeg_ops.rs::open_for_streams` |
| `avio_open2` | `ffmpeg_export.rs::open_io`, `ffmpeg_ops.rs::open_for_streams` |
| `avio_closep` | `ffmpeg_export.rs::drop`, `ffmpeg_ops.rs::drop` |
| `avformat_write_header` | `ffmpeg_export.rs::write_header`, `ffmpeg_ops.rs::open_for_streams` |
| `av_write_trailer` | `ffmpeg_export.rs::drop`, `ffmpeg_export.rs::write_trailer`, `ffmpeg_ops.rs::drop` |
| `av_interleaved_write_frame` | `ffmpeg_export.rs::write`, `ffmpeg_ops.rs::write` |
| `avformat_free_context` | `ffmpeg_export.rs::drop`, `ffmpeg_ops.rs::drop` |

#### avcodec-61.dll — `AvCodecApi` (23)

| Função | Quem chama |
|---|---|
| `avcodec_get_name` | `probe.rs::probe` |
| `av_packet_alloc` | `ffi/ffmpeg.rs::alloc`, `ffi/ffmpeg.rs::estimate_fps_from_packets`, `ffi/ffmpeg.rs::scan_duration_by_packets` |
| `av_packet_free` | `ffi/ffmpeg.rs::drop`, `ffi/ffmpeg.rs::estimate_fps_from_packets`, `ffi/ffmpeg.rs::scan_duration_by_packets` |
| `av_packet_unref` | `ffi/ffmpeg.rs::estimate_fps_from_packets`, `ffi/ffmpeg.rs::scan_duration_by_packets`, `ffi/ffmpeg.rs::unref` |
| `av_packet_rescale_ts` | `ffmpeg_export.rs::drain_encoder`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::remux_dispatch` |
| `av_packet_clone` | `ffi/ffmpeg.rs::clone_pkt` |
| `avcodec_find_decoder` | `ffi/ffmpeg.rs::open_decoder_for`, `ffmpeg_export.rs::open_sw_video_decoder` |
| `avcodec_find_encoder` | `ffmpeg_ops.rs::extract_frame_jpeg_inner` |
| `avcodec_find_encoder_by_name` | `ffmpeg_export.rs::encoder_exists`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::extract_frame_png_inner` |
| `avcodec_find_decoder_by_name` | `ffmpeg_export.rs::open_hw_video_decoder`, `ffmpeg_export.rs::run`, `ffmpeg_export.rs::switch_video_decoder` |
| `avcodec_alloc_context3` | `ffi/ffmpeg.rs::alloc` |
| `avcodec_free_context` | `ffi/ffmpeg.rs::drop` |
| `avcodec_parameters_to_context` | `ffi/ffmpeg.rs::open_decoder_for`, `ffmpeg_export.rs::open_audio_encoder`, `ffmpeg_export.rs::open_hw_video_decoder`, `ffmpeg_export.rs::open_sw_video_decoder`, `ffmpeg_export.rs::run`, `ffmpeg_export.rs::switch_video_decoder`, `ffmpeg_ops.rs::encode_still` |
| `avcodec_parameters_copy` | `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::open_for_streams` |
| `avcodec_parameters_alloc` | `ffmpeg_export.rs::open_audio_encoder`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::encode_still` |
| `avcodec_parameters_free` | `ffmpeg_export.rs::open_audio_encoder`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::encode_still` |
| `avcodec_parameters_from_context` | `ffmpeg_export.rs::run` |
| `avcodec_open2` | `ffi/ffmpeg.rs::open`, `ffmpeg_export.rs::run` |
| `avcodec_send_packet` | `ffmpeg_export.rs::handle_mix_packet`, `ffmpeg_export.rs::handle_speed_track_packet`, `ffmpeg_export.rs::probe_video_decode`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::compute_audio_peaks`, `ffmpeg_ops.rs::decode_audio_mono`, `ffmpeg_ops.rs::extract_frame_jpeg_inner`, `ffmpeg_ops.rs::extract_frame_png_inner` |
| `avcodec_receive_frame` | `ffmpeg_export.rs::handle_mix_packet`, `ffmpeg_export.rs::handle_speed_track_packet`, `ffmpeg_export.rs::probe_video_decode`, `ffmpeg_export.rs::pump_decoder`, `ffmpeg_ops.rs::compute_audio_peaks`, `ffmpeg_ops.rs::decode_audio_mono`, `ffmpeg_ops.rs::extract_frame_jpeg_inner`, `ffmpeg_ops.rs::extract_frame_png_inner` |
| `avcodec_send_frame` | `ffmpeg_export.rs::ao_emit`, `ffmpeg_export.rs::ao_finish`, `ffmpeg_export.rs::encode_video_frame`, `ffmpeg_export.rs::fifo_emit`, `ffmpeg_export.rs::flush_mix_frame`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::encode_still` |
| `avcodec_receive_packet` | `ffmpeg_export.rs::drain_encoder`, `ffmpeg_ops.rs::encode_still` |
| `avcodec_flush_buffers` | `ffmpeg_export.rs::probe_video_decode`, `ffmpeg_export.rs::run` |

#### avutil-59.dll — `AvUtilApi` (23)

| Função | Quem chama |
|---|---|
| `av_dict_get` | `ffi/ffmpeg.rs::copy_stream_tag`, `ffi/ffmpeg.rs::leftover_keys`, `ffi/ffmpeg.rs::metadata_string` |
| `av_dict_set` | `ffi/ffmpeg.rs::copy_stream_tag`, `ffi/ffmpeg.rs::set`, `ffmpeg_export.rs::run` |
| `av_dict_free` | `ffi/ffmpeg.rs::drop` |
| `av_rescale_q` | `ffi/ffmpeg.rs::container_duration_us`, `ffi/ffmpeg.rs::scan_duration_by_packets`, `ffi/ffmpeg.rs::ts_to_us`, `ffi/ffmpeg.rs::us_to_ts`, `ffmpeg_export.rs::compose_and_encode`, `ffmpeg_export.rs::flush_mix_frame`, `ffmpeg_ops.rs::write_pkt` |
| `av_strerror` | `ffi/ffmpeg.rs::av_err_str` |
| `av_image_get_buffer_size` | `ffmpeg_ops.rs::image_frame` |
| `av_image_fill_arrays` | `ffmpeg_ops.rs::image_frame` |
| `av_frame_alloc` | `ffi/ffmpeg.rs::alloc` |
| `av_frame_free` | `ffi/ffmpeg.rs::drop` |
| `av_frame_unref` | `ffi/ffmpeg.rs::unref` |
| `av_frame_get_buffer` | `ffmpeg_export.rs::ao_emit`, `ffmpeg_export.rs::fifo_emit`, `ffmpeg_export.rs::run`, `ffmpeg_export.rs::setup_scalers` |
| `av_frame_make_writable` | `ffmpeg_export.rs::adopt_accumulator`, `ffmpeg_export.rs::compose_and_encode` |
| `av_frame_move_ref` | `ffmpeg_export.rs::adopt_accumulator`, `ffmpeg_export.rs::pump_decoder`, `ffmpeg_ops.rs::extract_frame_png_inner` |
| `av_frame_copy_props` | `ffmpeg_export.rs::pump_decoder` |
| `av_opt_set_int` | `ffi/ffmpeg.rs::set_threads_auto` |
| `av_opt_set_q` | `ffmpeg_export.rs::open_audio_encoder`, `ffmpeg_export.rs::run`, `ffmpeg_ops.rs::encode_still` |
| `av_opt_set` | `ffmpeg_export.rs::open_audio_encoder`, `ffmpeg_export.rs::run` |
| `av_opt_get_int` | `ffmpeg_export.rs::frame_size_of` |
| `av_channel_layout_copy` | `ffmpeg_export.rs::copy_ch_layout` |
| `av_hwdevice_ctx_create` | `ffmpeg_export.rs::open_hw_video_decoder` |
| `av_hwframe_transfer_data` | `ffmpeg_export.rs::pump_decoder` |
| `av_buffer_ref` | `ffmpeg_export.rs::open_hw_video_decoder` |
| `av_buffer_unref` | `ffmpeg_export.rs::drop` |

#### swscale-8.dll — `SwScaleApi` (3)

| Função | Quem chama |
|---|---|
| `sws_getContext` | `ffi/ffmpeg.rs::new` |
| `sws_freeContext` | `ffi/ffmpeg.rs::drop` |
| `sws_scale` | `ffi/ffmpeg.rs::scale` |

#### obs.dll — `ObsApi` (117)

| Função | Quem chama |
|---|---|
| `base_set_log_handler` | `engine.rs::ensure_initialized` |
| `obs_startup` | `engine.rs::ensure_initialized` |
| `obs_shutdown` | `engine.rs::ensure_initialized`, `engine.rs::teardown` |
| `obs_reset_video` | `engine.rs::build_capture_graph`, `engine.rs::ensure_initialized` |
| `obs_reset_audio` | `engine.rs::ensure_initialized` |
| `obs_set_locale` | `audio_filters.rs::set_obs_locale_from_app`, `engine.rs::ensure_initialized` |
| `obs_open_module` | `engine.rs::load_modules` |
| `obs_init_module` | `engine.rs::load_modules` |
| `obs_post_load_modules` | `engine.rs::load_modules` |
| `obs_get_video` | `engine.rs::build_capture_graph`, `engine.rs::video_stats` |
| `obs_get_audio` | `engine.rs::build_capture_graph` |
| `obs_get_total_frames` | `engine.rs::video_stats` |
| `obs_get_lagged_frames` | `engine.rs::video_stats` |
| `obs_get_average_frame_time_ns` | `engine.rs::video_stats` |
| `obs_get_frame_interval_ns` | `engine.rs::video_stats` |
| `video_output_get_skipped_frames` | `engine.rs::video_stats` |
| `video_output_get_total_frames` | `engine.rs::video_stats` |
| `obs_set_output_source` | `engine.rs::build_capture_graph`, `engine.rs::clear_all_obs_data`, `engine.rs::release_recording_objects` |
| `obs_enum_scenes` | `engine.rs::clear_all_obs_data` |
| `obs_enum_sources` | `engine.rs::clear_all_obs_data` |
| `obs_data_create` | `audio_filters.rs::create_filter_instance`, `audio_filters.rs::set_audio_filter_value`, `ffi/obs.rs::create` |
| `obs_data_release` | `audio_filters.rs::drop`, `ffi/obs.rs::drop` |
| `obs_data_set_string` | `audio_filters.rs::set_audio_filter_value`, `ffi/obs.rs::set_string` |
| `obs_data_set_int` | `audio_filters.rs::set_audio_filter_value`, `ffi/obs.rs::set_int` |
| `obs_data_set_bool` | `audio_filters.rs::set_audio_filter_value`, `ffi/obs.rs::set_bool` |
| `obs_data_set_double` | `audio_filters.rs::set_audio_filter_value` |
| `obs_data_get_string` | `audio_filters.rs::add_props` |
| `obs_data_get_int` | `audio_filters.rs::add_props` |
| `obs_data_get_double` | `audio_filters.rs::add_props` |
| `obs_data_get_bool` | `audio_filters.rs::add_props` |
| `obs_data_create_from_json` | `audio_filters.rs::create_filter_instance`, `audio_filters.rs::set_audio_filter_value` |
| `obs_data_get_json` | `audio_filters.rs::set_audio_filter_value` |
| `obs_data_apply` | `audio_filters.rs::set_audio_filter_value` |
| `obs_source_create` | `engine.rs::create_source` |
| `obs_source_create_private` | `audio_filters.rs::private`, `audio_filters.rs::start_audio_filter_test` |
| `obs_source_release` | `audio_filters.rs::drop`, `audio_filters.rs::release_test`, `engine.rs::release_recording_objects` |
| `obs_source_remove` | `engine.rs::remove_source_cb` |
| `obs_source_update` | `audio_filters.rs::set_audio_filter_value` |
| `obs_source_set_muted` | `engine.rs::build_capture_graph`, `engine.rs::set_source_muted` |
| `obs_source_set_audio_mixers` | `engine.rs::build_capture_graph` |
| `obs_source_get_settings` | `audio_filters.rs::filter_json`, `audio_filters.rs::set_audio_filter_value` |
| `obs_get_latest_input_type_id` | `engine.rs::app_audio_available` |
| `obs_source_filter_add` | `audio_filters.rs::apply_audio_filters` |
| `obs_source_output_audio` | `audio_filters.rs::render_clip` |
| `obs_source_add_audio_capture_callback` | `audio_filters.rs::render_clip`, `audio_filters.rs::start_audio_filter_test` |
| `obs_source_remove_audio_capture_callback` | `audio_filters.rs::release_test`, `audio_filters.rs::render_clip` |
| `obs_register_source_s` | `audio_filters.rs::ensure_feed_registered` |
| `obs_enum_filter_types` | `audio_filters.rs::registered_audio_filter_ids` |
| `obs_get_source_output_flags` | `audio_filters.rs::ensure_feed_registered`, `audio_filters.rs::registered_audio_filter_ids` |
| `obs_source_get_display_name` | `audio_filters.rs::filter_json` |
| `os_gettime_ns` | `audio_filters.rs::render_clip` |
| `obs_scene_create` | `engine.rs::build_capture_graph` |
| `obs_scene_release` | `engine.rs::release_recording_objects` |
| `obs_scene_get_source` | `engine.rs::build_capture_graph` |
| `obs_scene_add` | `engine.rs::build_capture_graph` |
| `obs_sceneitem_set_pos` | `engine.rs::apply_area`, `engine.rs::build_capture_graph` |
| `obs_sceneitem_set_scale` | `engine.rs::apply_area`, `engine.rs::build_capture_graph` |
| `obs_sceneitem_set_bounds_type` | `engine.rs::build_capture_graph` |
| `obs_sceneitem_set_bounds` | `engine.rs::build_capture_graph` |
| `obs_sceneitem_set_crop` | `engine.rs::apply_area` |
| `obs_sceneitem_set_visible` | `engine.rs::apply_area` |
| `obs_video_encoder_create` | `encoder.rs::try_create_video_encoder` |
| `obs_audio_encoder_create` | `engine.rs::build_capture_graph` |
| `obs_enum_encoder_types` | `encoder.rs::registered_encoder_ids` |
| `obs_encoder_release` | `engine.rs::release_recording_objects` |
| `obs_encoder_get_id` | `engine.rs::build_capture_graph` |
| `obs_encoder_set_video` | `engine.rs::build_capture_graph` |
| `obs_encoder_set_audio` | `engine.rs::build_capture_graph` |
| `obs_output_create` | `engine.rs::create_muxer_output`, `engine.rs::create_replay_output` |
| `obs_output_release` | `engine.rs::create_replay_output`, `engine.rs::drop_saving_replay`, `engine.rs::finalize_stop`, `engine.rs::release_recording_objects`, `engine.rs::start_recording_from_replay` |
| `obs_output_set_video_encoder` | `engine.rs::create_replay_output`, `engine.rs::start_recording_from_replay`, `engine.rs::start_recording_inner` |
| `obs_output_set_audio_encoder` | `engine.rs::create_replay_output`, `engine.rs::start_recording_from_replay`, `engine.rs::start_recording_inner` |
| `obs_output_start` | `engine.rs::create_replay_output`, `engine.rs::start_recording_from_replay`, `engine.rs::start_recording_inner` |
| `obs_output_stop` | `engine.rs::drop_saving_replay`, `engine.rs::request_stop`, `engine.rs::stop_recording`, `engine.rs::teardown` |
| `obs_output_active` | `engine.rs::stop_recording` |
| `obs_output_pause` | `engine.rs::set_recording_paused` |
| `obs_output_get_last_error` | `engine.rs::create_replay_output`, `engine.rs::start_recording_from_replay`, `engine.rs::start_recording_inner` |
| `obs_output_get_signal_handler` | `engine.rs::connect_saved_signal`, `engine.rs::connect_stop_signal` |
| `signal_handler_connect` | `engine.rs::connect_saved_signal`, `engine.rs::connect_stop_signal` |
| `signal_handler_disconnect` | `engine.rs::disconnect_stop_signal`, `engine.rs::drop_saving_replay` |
| `obs_output_get_proc_handler` | `engine.rs::commit_replay_save`, `engine.rs::on_replay_saved_signal` |
| `proc_handler_call` | `engine.rs::commit_replay_save`, `engine.rs::on_replay_saved_signal` |
| `calldata_get_string` | `engine.rs::on_replay_saved_signal` |
| `bfree` | `engine.rs::commit_replay_save`, `engine.rs::on_replay_saved_signal` |
| `obs_get_source_properties` | `audio_tracks.rs::enumerate_raw`, `engine.rs::resolve_monitor_id` |
| `obs_source_properties` | `audio_filters.rs::filter_json`, `audio_filters.rs::set_audio_filter_value` |
| `obs_properties_destroy` | `audio_filters.rs::drop`, `audio_tracks.rs::enumerate_raw`, `engine.rs::resolve_monitor_id` |
| `obs_properties_first` | `audio_filters.rs::add_props` |
| `obs_properties_get` | `audio_filters.rs::set_audio_filter_value`, `audio_tracks.rs::enumerate_raw`, `engine.rs::resolve_monitor_id` |
| `obs_properties_apply_settings` | `audio_filters.rs::filter_json` |
| `obs_property_name` | `audio_filters.rs::add_props` |
| `obs_property_next` | `audio_filters.rs::add_props` |
| `obs_property_get_type` | `audio_filters.rs::add_props`, `audio_filters.rs::set_audio_filter_value` |
| `obs_property_description` | `audio_filters.rs::add_props` |
| `obs_property_long_description` | `audio_filters.rs::add_props` |
| `obs_property_visible` | `audio_filters.rs::add_props` |
| `obs_property_enabled` | `audio_filters.rs::add_props` |
| `obs_property_modified` | `audio_filters.rs::set_audio_filter_value` |
| `obs_property_int_min` | `audio_filters.rs::add_props` |
| `obs_property_int_max` | `audio_filters.rs::add_props` |
| `obs_property_int_step` | `audio_filters.rs::add_props` |
| `obs_property_int_type` | `audio_filters.rs::add_props` |
| `obs_property_int_suffix` | `audio_filters.rs::add_props` |
| `obs_property_float_min` | `audio_filters.rs::add_props` |
| `obs_property_float_max` | `audio_filters.rs::add_props` |
| `obs_property_float_step` | `audio_filters.rs::add_props` |
| `obs_property_float_type` | `audio_filters.rs::add_props` |
| `obs_property_float_suffix` | `audio_filters.rs::add_props` |
| `obs_property_text_type` | `audio_filters.rs::add_props` |
| `obs_property_group_content` | `audio_filters.rs::add_props` |
| `obs_property_list_format` | `audio_filters.rs::add_props`, `audio_filters.rs::set_audio_filter_value` |
| `obs_property_list_item_count` | `audio_filters.rs::add_props`, `audio_tracks.rs::enumerate_raw`, `engine.rs::resolve_monitor_id` |
| `obs_property_list_item_name` | `audio_filters.rs::add_props`, `audio_tracks.rs::enumerate_raw`, `engine.rs::resolve_monitor_id` |
| `obs_property_list_item_string` | `audio_filters.rs::add_props`, `audio_tracks.rs::enumerate_raw`, `engine.rs::resolve_monitor_id` |
| `obs_property_list_item_int` | `audio_filters.rs::add_props` |
| `obs_property_list_item_float` | `audio_filters.rs::add_props` |
| `obs_property_list_item_disabled` | `audio_filters.rs::add_props` |


### Tabela C — funções Rust que falam com as DLLs (86)

A mesma informação da Tabela B, pelo lado de quem chama: é a lista de alvos dos testes de integração.

| Função Rust | Nº | Funções de DLL |
|---|---|---|
| `audio_filters.rs::add_props` | 31 | obs_data_get_bool, obs_data_get_double, obs_data_get_int, obs_data_get_string, obs_properties_first, obs_property_description, obs_property_enabled, obs_property_float_max, obs_property_float_min, obs_property_float_step, obs_property_float_suffix, obs_property_float_type, obs_property_get_type, obs_property_group_content, obs_property_int_max, obs_property_int_min, obs_property_int_step, obs_property_int_suffix, obs_property_int_type, obs_property_list_format, obs_property_list_item_count, obs_property_list_item_disabled, obs_property_list_item_float, obs_property_list_item_int, obs_property_list_item_name, obs_property_list_item_string, obs_property_long_description, obs_property_name, obs_property_next, obs_property_text_type, obs_property_visible |
| `audio_filters.rs::apply_audio_filters` | 1 | obs_source_filter_add |
| `audio_filters.rs::create_filter_instance` | 2 | obs_data_create, obs_data_create_from_json |
| `audio_filters.rs::drop` | 3 | obs_data_release, obs_properties_destroy, obs_source_release |
| `audio_filters.rs::ensure_feed_registered` | 2 | obs_get_source_output_flags, obs_register_source_s |
| `audio_filters.rs::filter_json` | 4 | obs_properties_apply_settings, obs_source_get_display_name, obs_source_get_settings, obs_source_properties |
| `audio_filters.rs::private` | 1 | obs_source_create_private |
| `audio_filters.rs::registered_audio_filter_ids` | 2 | obs_enum_filter_types, obs_get_source_output_flags |
| `audio_filters.rs::release_test` | 2 | obs_source_release, obs_source_remove_audio_capture_callback |
| `audio_filters.rs::render_clip` | 4 | obs_source_add_audio_capture_callback, obs_source_output_audio, obs_source_remove_audio_capture_callback, os_gettime_ns |
| `audio_filters.rs::set_audio_filter_value` | 15 | obs_data_apply, obs_data_create, obs_data_create_from_json, obs_data_get_json, obs_data_set_bool, obs_data_set_double, obs_data_set_int, obs_data_set_string, obs_properties_get, obs_property_get_type, obs_property_list_format, obs_property_modified, obs_source_get_settings, obs_source_properties, obs_source_update |
| `audio_filters.rs::set_obs_locale_from_app` | 1 | obs_set_locale |
| `audio_filters.rs::start_audio_filter_test` | 2 | obs_source_add_audio_capture_callback, obs_source_create_private |
| `audio_tracks.rs::enumerate_raw` | 6 | obs_get_source_properties, obs_properties_destroy, obs_properties_get, obs_property_list_item_count, obs_property_list_item_name, obs_property_list_item_string |
| `encoder.rs::registered_encoder_ids` | 1 | obs_enum_encoder_types |
| `encoder.rs::try_create_video_encoder` | 1 | obs_video_encoder_create |
| `engine.rs::app_audio_available` | 1 | obs_get_latest_input_type_id |
| `engine.rs::apply_area` | 4 | obs_sceneitem_set_crop, obs_sceneitem_set_pos, obs_sceneitem_set_scale, obs_sceneitem_set_visible |
| `engine.rs::build_capture_graph` | 17 | obs_audio_encoder_create, obs_encoder_get_id, obs_encoder_set_audio, obs_encoder_set_video, obs_get_audio, obs_get_video, obs_reset_video, obs_scene_add, obs_scene_create, obs_scene_get_source, obs_sceneitem_set_bounds, obs_sceneitem_set_bounds_type, obs_sceneitem_set_pos, obs_sceneitem_set_scale, obs_set_output_source, obs_source_set_audio_mixers, obs_source_set_muted |
| `engine.rs::clear_all_obs_data` | 3 | obs_enum_scenes, obs_enum_sources, obs_set_output_source |
| `engine.rs::commit_replay_save` | 3 | bfree, obs_output_get_proc_handler, proc_handler_call |
| `engine.rs::connect_saved_signal` | 2 | obs_output_get_signal_handler, signal_handler_connect |
| `engine.rs::connect_stop_signal` | 2 | obs_output_get_signal_handler, signal_handler_connect |
| `engine.rs::create_muxer_output` | 1 | obs_output_create |
| `engine.rs::create_replay_output` | 6 | obs_output_create, obs_output_get_last_error, obs_output_release, obs_output_set_audio_encoder, obs_output_set_video_encoder, obs_output_start |
| `engine.rs::create_source` | 1 | obs_source_create |
| `engine.rs::disconnect_stop_signal` | 1 | signal_handler_disconnect |
| `engine.rs::drop_saving_replay` | 3 | obs_output_release, obs_output_stop, signal_handler_disconnect |
| `engine.rs::ensure_initialized` | 6 | base_set_log_handler, obs_reset_audio, obs_reset_video, obs_set_locale, obs_shutdown, obs_startup |
| `engine.rs::finalize_stop` | 1 | obs_output_release |
| `engine.rs::load_modules` | 3 | obs_init_module, obs_open_module, obs_post_load_modules |
| `engine.rs::on_replay_saved_signal` | 4 | bfree, calldata_get_string, obs_output_get_proc_handler, proc_handler_call |
| `engine.rs::release_recording_objects` | 5 | obs_encoder_release, obs_output_release, obs_scene_release, obs_set_output_source, obs_source_release |
| `engine.rs::remove_source_cb` | 1 | obs_source_remove |
| `engine.rs::request_stop` | 1 | obs_output_stop |
| `engine.rs::resolve_monitor_id` | 6 | obs_get_source_properties, obs_properties_destroy, obs_properties_get, obs_property_list_item_count, obs_property_list_item_name, obs_property_list_item_string |
| `engine.rs::set_recording_paused` | 1 | obs_output_pause |
| `engine.rs::set_source_muted` | 1 | obs_source_set_muted |
| `engine.rs::start_recording_from_replay` | 5 | obs_output_get_last_error, obs_output_release, obs_output_set_audio_encoder, obs_output_set_video_encoder, obs_output_start |
| `engine.rs::start_recording_inner` | 4 | obs_output_get_last_error, obs_output_set_audio_encoder, obs_output_set_video_encoder, obs_output_start |
| `engine.rs::stop_recording` | 2 | obs_output_active, obs_output_stop |
| `engine.rs::teardown` | 2 | obs_output_stop, obs_shutdown |
| `engine.rs::video_stats` | 7 | obs_get_average_frame_time_ns, obs_get_frame_interval_ns, obs_get_lagged_frames, obs_get_total_frames, obs_get_video, video_output_get_skipped_frames, video_output_get_total_frames |
| `ffmpeg_export.rs::adopt_accumulator` | 2 | av_frame_make_writable, av_frame_move_ref |
| `ffmpeg_export.rs::alloc` | 1 | avformat_alloc_output_context2 |
| `ffmpeg_export.rs::ao_emit` | 2 | av_frame_get_buffer, avcodec_send_frame |
| `ffmpeg_export.rs::ao_finish` | 1 | avcodec_send_frame |
| `ffmpeg_export.rs::compose_and_encode` | 2 | av_frame_make_writable, av_rescale_q |
| `ffmpeg_export.rs::copy_ch_layout` | 1 | av_channel_layout_copy |
| `ffmpeg_export.rs::drain_encoder` | 2 | av_packet_rescale_ts, avcodec_receive_packet |
| `ffmpeg_export.rs::drop` | 4 | av_buffer_unref, av_write_trailer, avformat_free_context, avio_closep |
| `ffmpeg_export.rs::encode_video_frame` | 1 | avcodec_send_frame |
| `ffmpeg_export.rs::encoder_exists` | 1 | avcodec_find_encoder_by_name |
| `ffmpeg_export.rs::fifo_emit` | 2 | av_frame_get_buffer, avcodec_send_frame |
| `ffmpeg_export.rs::flush_mix_frame` | 2 | av_rescale_q, avcodec_send_frame |
| `ffmpeg_export.rs::frame_size_of` | 1 | av_opt_get_int |
| `ffmpeg_export.rs::handle_mix_packet` | 2 | avcodec_receive_frame, avcodec_send_packet |
| `ffmpeg_export.rs::handle_speed_track_packet` | 2 | avcodec_receive_frame, avcodec_send_packet |
| `ffmpeg_export.rs::new_stream` | 1 | avformat_new_stream |
| `ffmpeg_export.rs::open_audio_encoder` | 5 | av_opt_set, av_opt_set_q, avcodec_parameters_alloc, avcodec_parameters_free, avcodec_parameters_to_context |
| `ffmpeg_export.rs::open_hw_video_decoder` | 4 | av_buffer_ref, av_hwdevice_ctx_create, avcodec_find_decoder_by_name, avcodec_parameters_to_context |
| `ffmpeg_export.rs::open_io` | 1 | avio_open2 |
| `ffmpeg_export.rs::open_sw_video_decoder` | 2 | avcodec_find_decoder, avcodec_parameters_to_context |
| `ffmpeg_export.rs::probe_video_decode` | 5 | av_read_frame, av_seek_frame, avcodec_flush_buffers, avcodec_receive_frame, avcodec_send_packet |
| `ffmpeg_export.rs::pump_decoder` | 4 | av_frame_copy_props, av_frame_move_ref, av_hwframe_transfer_data, avcodec_receive_frame |
| `ffmpeg_export.rs::run` | 18 | av_dict_set, av_frame_get_buffer, av_opt_set, av_opt_set_q, av_packet_rescale_ts, av_read_frame, av_seek_frame, avcodec_find_decoder_by_name, avcodec_find_encoder_by_name, avcodec_flush_buffers, avcodec_open2, avcodec_parameters_alloc, avcodec_parameters_copy, avcodec_parameters_free, avcodec_parameters_from_context, avcodec_parameters_to_context, avcodec_send_frame, avcodec_send_packet |
| `ffmpeg_export.rs::setup_scalers` | 1 | av_frame_get_buffer |
| `ffmpeg_export.rs::switch_video_decoder` | 2 | avcodec_find_decoder_by_name, avcodec_parameters_to_context |
| `ffmpeg_export.rs::write` | 1 | av_interleaved_write_frame |
| `ffmpeg_export.rs::write_header` | 1 | avformat_write_header |
| `ffmpeg_export.rs::write_trailer` | 1 | av_write_trailer |
| `ffmpeg_ops.rs::compute_audio_peaks` | 3 | av_read_frame, avcodec_receive_frame, avcodec_send_packet |
| `ffmpeg_ops.rs::decode_audio_mono` | 3 | av_read_frame, avcodec_receive_frame, avcodec_send_packet |
| `ffmpeg_ops.rs::drop` | 3 | av_write_trailer, avformat_free_context, avio_closep |
| `ffmpeg_ops.rs::encode_still` | 6 | av_opt_set_q, avcodec_parameters_alloc, avcodec_parameters_free, avcodec_parameters_to_context, avcodec_receive_packet, avcodec_send_frame |
| `ffmpeg_ops.rs::extract_frame_jpeg_inner` | 6 | av_read_frame, av_seek_frame, avcodec_find_encoder, avcodec_receive_frame, avcodec_send_packet, avformat_open_input |
| `ffmpeg_ops.rs::extract_frame_png_inner` | 5 | av_frame_move_ref, av_read_frame, avcodec_find_encoder_by_name, avcodec_receive_frame, avcodec_send_packet |
| `ffmpeg_ops.rs::image_frame` | 2 | av_image_fill_arrays, av_image_get_buffer_size |
| `ffmpeg_ops.rs::list_video_keyframes` | 2 | avformat_index_get_entries_count, avformat_index_get_entry |
| `ffmpeg_ops.rs::merge_files` | 1 | av_read_frame |
| `ffmpeg_ops.rs::open_for_streams` | 5 | avcodec_parameters_copy, avformat_alloc_output_context2, avformat_new_stream, avformat_write_header, avio_open2 |
| `ffmpeg_ops.rs::remux_dispatch` | 2 | av_packet_rescale_ts, av_read_frame |
| `ffmpeg_ops.rs::splice_continuation` | 1 | av_read_frame |
| `ffmpeg_ops.rs::write` | 1 | av_interleaved_write_frame |
| `ffmpeg_ops.rs::write_pkt` | 1 | av_rescale_q |
| `probe.rs::probe` | 1 | avcodec_get_name |


## Andamento

| Frente | Situação |
|---|---|
| Estrutura | ✅ crate virou **lib + exe** (`src/lib.rs` com os módulos; `src/main.rs` só chama `noobs::run_app()`) |
| 1. Mensagens sem DLL | ✅ **106 testes** em `src/bridge/tests/` (8 arquivos), todos pelo `dispatch` |
| 2. FFmpeg | ✅ **63 testes** em `src/media_tests/` contra 11 amostras versionadas (344 KB) |
| 3. obs.dll | ✅ **18 cenários** num executável de teste próprio (`tests/obs.rs`), fora da suíte padrão e fora do app |
| 4. Mensagens que chegam nas DLLs | ✅ **23 testes** de mídia em `src/bridge/tests/midia.rs` (FFmpeg, no `cargo test`) + **13 cenários** `msg-*` em `tests/obs.rs` (obs.dll, pelo `dispatch`) |

Rodar: `cargo test --release` (dentro de `rust\`). Tudo junto (302 testes) leva ~13 s.

### Como a frente 1 isola o teste (`src/testkit.rs`)

- `LOCALAPPDATA`/`APPDATA` apontam pra `%TEMP%
oobs-test-<pid>` antes do
  primeiro uso — config, cache, motor local, buffer e logs saem dali. As
  pastas de execuções antigas são apagadas na largada.
- Cada teste abre uma sessão (`testkit::begin()`): trava global (os testes do
  bridge rodam um por vez), config zerado, idioma pt-BR, pasta de gravação
  própria e vazia.
- `post_json` cai numa lista (`testkit::sent()`), não no WebView.
- Efeitos que sairiam do processo viram registro (`testkit::effects()`), com
  `#[cfg(test)]` no ponto exato: registro do Windows (`autostart`),
  navegador/Explorer (`ui::shell_open`), Lixeira (`delete_to_recycle_bin` —
  apaga de verdade, mas só na pasta temporária), diálogo de pasta, checagem
  de atualização (rede), instalação do motor local (download) e a seleção de
  área (janela que escurece os monitores; `area_select::test_finish` faz o
  papel do usuário).
- Fora da frente 1 de propósito: `ready` (é o arranque inteiro) e tudo que
  grava, toca ou lê mídia.

### Defeitos que os testes da frente 1 acharam (corrigidos)

1. **Atalhos repetidos entre recursos.** O de gravar não conferia o do buffer
   nem o da área, e o do buffer não conferia o da área. Com dois iguais o
   Windows registra só o primeiro e o outro para de funcionar sem aviso.
   Agora os três se recusam entre si — no backend e na tela (toast antes de
   mandar), com as mensagens que já existiam.
2. **Opacidade do indicador de gravação sem limite.** Gravava qualquer
   número (0, 500); o do buffer já limitava a 20–100. Agora os dois limitam.

### Como a frente 2 testa a mídia (`src/media_tests/`)

- **Amostras** em `tests/fixtures/media/`, geradas pelas próprias DLLs do
  FFmpeg (não há `ffmpeg.exe` na máquina) pelo teste ignorado
  `gerar_amostras` — regerar: `cargo test --release gerar_amostras -- --ignored`.
  O gerador (`gen.rs`) codifica tudo na memória (vídeo sintético em que cada
  quadro é diferente, um seno por faixa) e escreve JANELAS dos pacotes, com o
  relógio zerado como as saídas da libobs: é assim que nasce o par
  buffer + continuação com 1 s em comum.
- As DLLs vêm de `exe\bin\64bit\` (`SetDllDirectoryW` na largada). Máquina
  sem o runtime: cada teste avisa e sai sem falhar.
- Conferência pelo conteúdo, não só pelo `probe`: os pacotes são relidos e
  comparados por SHA-1 (cópia sem perda, sem repetição, na ordem); quadros
  exatos em PNG; tamanho de JPEG/PNG pelo cabeçalho.
- `hevc.mkv` só existe porque esta placa (AMD) codifica HEVC; em máquina sem
  ele os testes de HEVC pulam a amostra.

### Como a frente 3 testa a libobs (`tests/obs.rs`)

```
cargo test --release --features obs-tests --test obs            # todos (~80 s)
cargo test --release --features obs-tests --test obs -- area    # só os que contêm "area"
```

**Nenhum código de teste vai no `NoOBS.exe`.** Os testes rodam sempre FORA do
app: `#[cfg(test)]` (frentes 1 e 2) só existe no binário do `cargo test`, e a
frente 3 é um executável de teste próprio. Conferido no exe do build normal:
nenhum texto do autoteste, do kit nem dos desvios de teste.

- A libobs não roda de dentro de `target\`: acha dados, shaders e plugins
  pela pasta do EXE, exige a MESMA thread do começo ao fim (#3) e o RTWQ de pé
  (#48). Então `tests/obs.rs` (`harness = false`, a `main` é a thread
  principal) se COPIA pra `exe/bin/64bit/NoOBS-obstest.exe` e roda a cópia com
  `--dentro [filtro]`, repassando as linhas `OK`/`FALHA`/`PULADO`. A cópia é
  apagada no fim. Sem janela, sem mutex de instância (o NoOBS instalado pode
  ficar aberto) e com `LOCALAPPDATA` numa pasta temporária.
- O executável de teste chama o engine direto: com a feature `obs-tests` os
  módulos da lib ficam públicos (macro `modulos!` em `src/lib.rs`); no build
  normal continuam privados — a feature só muda a visibilidade. `log` e `ffi`
  ficam fora da macro porque definem macros `#[macro_export]`.
- Como o `ui::run`, a cópia troca a pasta de TRABALHO pra pasta do exe: a
  libobs procura `../../data/libobs/` a partir dela (sem isso, "Failed to find
  file default.effect" e `obs_reset_video` −1).
- **Grava a tela de verdade** por alguns segundos em cada cenário; os arquivos
  são conferidos pelo `probe` e apagados no fim. Fica só o log, na pasta
  temporária (o caminho sai na linha `FIM`).
- Encoder que a máquina não tem vira `PULADO`, não falha — o mesmo comando
  roda numa máquina NVIDIA e mede os ramos que esta AMD não exercita (NVENC
  continua NÃO calibrado, #53).

| Cenário | O que confere |
|---|---|
| `init` / `teardown` | sobe e derruba a libobs (tempo; o watchdog de 5 s não dispara) |
| `caps` | encoders detectados |
| `codec-*` (5) | cada codec disponível grava; o arquivo sai no codec pedido; clamp do H.264 hw em 4096 (#7) |
| `qualidade` | nível 10 gera arquivo maior que o nível 0 |
| `fps` | 15 e 60 fps saem com essa taxa |
| `area` | canvas em múltiplo de 16; mover/redimensionar no meio não muda o tamanho do vídeo |
| `janela` | grava a janela de OUTRO processo (o próprio executável de teste em `--janela`) |
| `pausa` | o trecho pausado não entra nem como buraco |
| `buffer` | salvar trecho gera arquivo e o buffer continua ligado |
| `buffer-continua` | gravar com o buffer ligado gera prefixo + gravação, e a emenda descarta os repetidos |
| `filtros` / `filtro-ab` | lista dos filtros do OBS, valor salvo como double (#63), gravar com filtro, teste A/B do microfone (com som: mediu exatamente +6 dB) |
| `faixas` | dispositivos de áudio vistos pelo libobs |

Medido nesta máquina (AMD, 2 monitores 4K): 18/18 em ~80 s.

### Como a frente 4 testa as mensagens que chegam nas DLLs

As mensagens que só chegam no **FFmpeg** rodam no `cargo test` comum
(`src/bridge/tests/midia.rs`), com o mesmo kit da frente 1 mais as amostras da
frente 2 e o servidor do player de verdade (as URLs devolvidas à página são
baixadas e conferidas): `play_recording`, `request_transcode`,
`request_video_info`, `request_waveform`, `request_keyframes`,
`request_audio_tracks`, `open_folder`/`set_library_thumbs` (miniaturas),
`set_record_dir`, `export_recording`, `merge_recordings` e a transcrição pela
mistura e por faixa contra um motor FALSO (`transcribe::test_engine`, que só
existe no `cargo test`).

As que chegam na **obs.dll** rodam no executável da frente 3, num SEGUNDO
processo (`--mensagens`, a libobs sobe do zero), pelo `dispatch` de verdade:

- `bridge::install_hooks()` + a mensagem `ready`, como o WebView faria — o
  warmup sobe a libobs no timer, como no app;
- uma janela ESCONDIDA (`HWND_MESSAGE`) faz o papel da principal: os
  `set_timer` do Bridge (warmup, contagem da área, fechamento do trecho do
  buffer, teste de filtro) e o `WM_APP_WAKE` da fila da main chegam nela;
- o que o Bridge manda à página é lido por `bridge::take_sent_for_tests()`.

Os três ganchos (`ui::attach_headless_window`, `ui::dispatch_timer`,
`bridge::take_sent_for_tests`) só existem com a feature `obs-tests` — o exe do
build normal não tem nenhum deles. O config de teste desliga tudo que sairia
do processo (atalhos globais, bandeja, som, indicadores, atualização,
transcrição) e o `FirstRun` do registro é devolvido se o `ready` o consumir.

```
cargo test --release --features obs-tests --test obs -- msg                      # só as mensagens
cargo test --release --features obs-tests --test obs -- msg-janela,msg-area      # vários filtros
```

| Cenário | Mensagens | O que confere |
|---|---|---|
| `msg-ready` | `ready` | `init` com monitores, `encoder_caps` depois do warmup, microfones da worker |
| `msg-ocultar` | `set_source_hidden` | deixa só o 1º monitor (canvas menor pro resto) |
| `msg-idioma` | `set_language` | `language_changed` e os NOMES dos filtros trocam de idioma (`obs_set_locale`) |
| `msg-filtros` | `get_audio_filters`, `set_audio_filter` (enable/value/op inválida), `test_audio_filters`, `reset_audio_filters` | valor inteiro vira double (#63); teste A/B gera dois WAV e refaz só a filtrada ao mudar o ganho; reset desliga tudo |
| `msg-gravar` | `set_codec` (+ valor inválido), `set_recording_fps`, `set_audio_bitrate` (150 → 160), `record_start`, `record_stop` | arquivo na pasta, codec e fps pedidos, card `recording_added` |
| `msg-pausa` | `record_pause` | ignorado sem gravação; relógio parado na pausa; o arquivo não tem o trecho pausado |
| `msg-fontes` | `toggle_source`, `set_source_hidden`, `set_mute_when_device_muted` | monitor recusado gravando (erro na UI, config intacto); mic muda na fonte viva |
| `msg-janela` | `list_capture_windows`, `record_window` | id fora da lista = erro; janela de outro processo listada com prévia; vídeo no tamanho da área cliente |
| `msg-area` | `set_hotkeys`, `trigger_hotkey` | área fora das telas = erro; contagem 2 → 1 no timer; 640x360 → 640x352 |
| `msg-buffer` | `set_replay_enabled`, `save_replay`, `set_codec`/`set_recording_quality`/`set_recording_fps`/`set_audio_bitrate` | salvar sem buffer não faz nada; trecho salvo vira card; cada troca de encoder manda `replay_restarted` e o mesmo valor não; gravar continua o buffer (emenda) e ele religa no fim |
| `msg-buffer-apps` | `set_replay_apps` | `explorer` liga o buffer na 1ª leitura; lista vazia desliga |
| `msg-teste-video` | `test_video_quality`, `cancel_video_test` | gravar durante a amostra = erro; 11 níveis com PNG, nível 10 maior que o 0; cancelar no meio |
| `msg-shutdown` | (fecho) | `bridge::shutdown` derruba tudo em < 5 s |

Medido nesta máquina: 13/13 (~2 min, a maior parte no teste de qualidade em 4K).

Cenários que vieram com a caça às corridas da #68:

| Cenário | O que confere |
|---|---|
| `msg-som-sistema` | toca um tom baixo (`PlaySoundW`) com os microfones desligados e grava DUAS vezes: falha só no sintoma da #48 (1ª muda, 2ª com som); as duas mudas = `PULADO` (captura do sistema muda na máquina) |
| `msg-webcam` | grava com a webcam e registra se a `RTWorkQ.dll` entrou no processo (entra: Media Foundation) |

Opções do executável de teste pra caçar defeito intermitente (todas fora do app):

| Variável | Efeito |
|---|---|
| `OBSTEST_REPETIR=N` | janela + área N vezes mais no mesmo processo |
| `OBSTEST_HEAP=1` | `HeapValidate` em todos os heaps a cada 200 ms; na 1ª corrupção registra o passo e captura as pilhas |
| `OBSTEST_SYMS` | pastas com os `.pdb` do OBS (release oficial) pro `cdb` nomear as funções da obs.dll e dos plugins |
| `OBSTEST_RTWQ_BOOT=1` | sobe a RTWQ no arranque (o comportamento antigo), pra comparar |
| `OBSTEST_RTWQ_PRE=1` | carrega a `RTWorkQ.dll` sem o startup, como se outro módulo a tivesse carregado |

Sempre ligado: um vigia que, com a main parada 40 s ou numa exceção não
tratada, prende o `cdb` (`-pv`) no próprio processo e grava a pilha de todas
as threads em `<sandbox>\pilhas-*.txt`. Os filtros aceitam vírgula
(`-- msg-janela,msg-area`).

Com o monitor APAGADO (economia de energia) a captura sai preta e o tamanho
deixa de depender da qualidade: `qualidade` e `msg-teste-video` viram
`PULADO` ("tela parada") quando o nível 10 não sai pelo menos 10% maior que o
0. Rode com a tela ligada e algo se mexendo pra medir de verdade.

### Defeitos que os testes da frente 2 acharam (corrigidos)

3. **Unir gravações com B-frames falhava** (`merge_files` → `Err(false)`). No
   MKV o demuxer entrega os primeiros quadros de vídeo SEM dts; a união
   repassava o vazio e o muxer morria em "non monotonically increasing dts".
   Pega toda gravação em x264 e NVENC (#50) — nunca apareceu nesta máquina
   porque o AMF grava sem B-frames. Agora o começo de cada arquivo é retido, o
   dts que falta é preenchido de trás pra frente e a parte seguinte começa
   depois do último dts escrito.
4. **MP4 perdia o nome das faixas.** O muxer de MP4 não guarda `title` de
   faixa; remux e exportação só copiavam `title`, então todo MP4 exportado
   chegava sem os nomes (o que a #51h prometia). Agora o título vai também no
   `handler_name`, e a leitura usa o `handler_name` quando falta o `title`
   (menos os genéricos `SoundHandler`/`VideoHandler`). Vale pra faixa mixada.
5. **MP4 exportado dizia 30,25 fps em vez de 30.** Os pacotes de vídeo saíam
   do encoder sem duração (time_base 1/1000), e o MP4 fechava a faixa no
   começo do último quadro. Agora cada pacote leva a duração de um quadro.
6. **Documentação errada do `list_video_keyframes`**: dizia `None` pra
   gravação interrompida; na verdade o demuxer monta um índice parcial com os
   keyframes que sobraram (medido). O comportamento é o bom; o comentário foi
   corrigido.

7. **Remux e emenda repassavam pacote sem dts ao muxer** (fonte com
   B-frames). Funcionava pelo preenchimento obsoleto do libavformat ("Timestamps
   are unset … will stop working in the future") e quebraria numa troca de
   versão do FFmpeg. Agora os dois retêm o começo de cada arquivo e preenchem o
   dts como a união (`fill_missing_dts`); na emenda as assinaturas que acham o
   ponto de encontro continuam saindo dos pacotes originais. O aviso sumiu da
   suíte inteira.

8. **A transcrição pela mistura falhava em máquina nova** ("não foi possível
   extrair o áudio"): o m4a temporário ia pra pasta de cache, que só existe
   depois da 1ª miniatura. Agora a pasta é criada antes (achado pela frente 4,
   com a API falsa).

9. **Queda e heap corrompido ao parar gravações — duas corridas da libobs**
   (achado pela frente 4: ~1 em 10 processos de `msg-janela,msg-area`
   travava no `obs_reset_video` ou caía). Não era a janela: era destruir
   fontes WASAPI, o que acontece a cada fim de gravação. Detalhes e medidas na
   pegadinha #68 do CLAUDE.md:
   - **deduplicação do monitoramento**: a thread de áudio lia a captura do som
     do sistema já liberada. Corrigido apontando o monitoramento (que o NoOBS
     não usa) pra um dispositivo inexistente;
   - **caminho RTWQ do `win-wasapi`**: `--refCount` na fonte já apagada.
     Corrigido sem RTWQ no arranque + desvio do `GetModuleHandleW` do plugin
     (a webcam carrega a `RTWorkQ.dll`; sem o desvio o plugin voltaria ao
     RTWQ). Rede de segurança: `rtwq::ensure_if_preloaded` (#48).
   - de brinde, a saída do app caía depois de gravar com a RTWQ fora do
     arranque: `ITaskbarList3` num `thread_local` solto depois do
     `CoUninitialize`. Corrigido (`ui::release_thread_com`).

### Riscos anotados (não corrigidos)

- **Travamento ao parar com webcam (visto 1 vez em 29 rodadas, não
  reproduzido)**: depois da correção da #68, numa série
  `msg-webcam,msg-janela,msg-area` sob o heap de depuração, a gravação de área
  não recebeu o sinal `"stop"`; o `ForceCompleteStop` ficou preso no
  `obs_output_destroy` (espera o `stopping_event`) e a thread de gráficos
  estava parada no `pthread_mutex_lock` do `encoder_group_mutex`
  (`obs-video.c:833`, `video_sleep`). O NoOBS não usa grupos de encoders: esse
  mutex nunca é disputado — parado nele, ele estava CORROMPIDO, ou seja, ainda
  há alguma escrita em memória errada. As outras 28 rodadas com webcam (e 40
  sem) passaram. A pilha simbolizada ficou nas notas da sessão
  (`travamento-encoder_group_mutex-pilhas.txt`); pra caçar: `OBSTEST_HEAP=1`
  + `OBSTEST_SYMS` em laço com a webcam.

- A exportação descarta o pacote de pré-rolagem do AAC (pts −21 ms) por estar
  antes do trecho — esperado, mas os primeiros ~21 ms do áudio exportado
  dependem do decoder lidar sem ele.

## Proposta de plano de testes

Quatro frentes, da mais barata (roda em qualquer `cargo test`) pra mais cara
(precisa de placa de vídeo e monitor). Cada uma é independente.

### 1. Mensagens da página sem DLL (80 tipos) — `cargo test` comum

- Um "ralo" de teste no `post_json`: em teste, as mensagens de volta caem
  numa lista em vez de ir pro WebView, e o teste confere o que o Rust
  respondeu.
- `config.json` apontado para uma pasta temporária por teste (hoje é fixo em
  `%LOCALAPPDATA%\NoOBS`): sem isso os testes mexeriam nas preferências reais.
- Por mensagem: valor normal, valor fora da faixa (clamp), campo ausente
  (default), tipo errado (string no lugar de número), e o efeito no config.
  Ex.: `set_hotkeys` com atraso −5 / 3 / 999 e combinação repetida; `set_replay_limits` acima da RAM; `set_filename_pattern`
  com caracteres inválidos.

### 2. FFmpeg (as funções da Tabela C de `probe`, `ffmpeg_ops`, `ffmpeg_export`, `export_captions`) — precisa das DLLs, não de GPU

- **Amostras pequenas versionadas** (poucos segundos, centenas de KB cada):
  H.264, HEVC e AV1; 1 e 5 faixas de áudio; MKV fechado e MKV interrompido
  (sem trailer); MP4. Geradas uma vez gravando pelo próprio NoOBS.
- Por função, uma matriz de parâmetros. Ex.:
  - `probe`: cada codec, sem duração no cabeçalho, arquivo truncado,
    arquivo inexistente, arquivo que não é vídeo;
  - `export_video`: cada encoder disponível × CRF 0/23/51 × recorte (par,
    ímpar, fora do canvas) × trechos (1, vários, adjacentes) × velocidade
    1/1,5/8/512 × só áudio (MP3) × legenda gravada × cancelamento no meio;
  - `merge_files` / `splice_continuation`: emenda válida, sem ponto de
    encontro, faixas diferentes;
  - `extract_frame_*`, `compute_audio_peaks`, `list_video_keyframes`,
    `extract_audio_tracks`, `remux_file`.
- Conferência do resultado lendo o arquivo de volta pelo próprio `probe`
  (duração, tamanho do quadro, nº de faixas, codec).
- Os encoders de hardware (`*_amf`, `*_nvenc`) só existem com a placa certa:
  o teste pula com aviso quando o encoder não abre.

### 3. obs.dll (as funções da Tabela C de `engine`, `encoder`, `audio_filters`, `audio_tracks`) — precisa de GPU e monitor

- A libobs exige rodar sempre na MESMA thread (pegadinha #3) e o
  `cargo test` usa várias threads. Por isso: um alvo de teste próprio com
  `harness = false` (laço único, na thread principal), rodado à parte.
- Cenários: subir e derrubar a libobs; gravar ~3 s com cada codec
  disponível; área (um monitor, dois monitores, mudar no meio); janela;
  pausa; buffer (ligar, salvar trecho, gravar continuando o buffer);
  filtros de áudio (listar, ligar, mudar valor, teste A/B); faixas de áudio.
  Cada gravação conferida pelo `probe`.
- Não roda em servidor de CI: só nesta máquina (ou em outra com GPU).

### 4. As 34 mensagens que chegam nas DLLs

Testadas por cima das frentes 2 e 3: o mesmo "ralo" da frente 1, com a
libobs/FFmpeg reais por baixo.

### Decisão estrutural (tomada)

O crate era só binário. Virou **lib + exe**: `src/lib.rs` com os módulos e o
`main.rs` só chamando `noobs::run_app()`. Regra que vale pra todas as frentes:
**teste nunca entra no app** — nada de modo escondido no exe; os testes rodam
em binários de teste (`cargo test`) ou em executáveis de teste próprios
(`tests/`).

//! Exportação (re-encode) no Bridge (porte do bloco "Exportação" do
//! `OBSBridge.pas`): valida o pedido da UI, monta o `ExportOptions`, roda o
//! `ffmpeg_export::export_video` numa worker e devolve progresso/resultado.
//!
//! Regras que seguem valendo:
//! * UMA exportação por vez; o botão fica desabilitado na UI, mas o guard daqui é
//!   a garantia de verdade (mensagem forjada, duplo clique);
//! * escreve num `<final>.part` e só renomeia no fim (pegadinha #51e): o watcher da
//!   pasta não enxerga o arquivo enquanto cresce, e um `.part` órfão de um crash
//!   também fica invisível;
//! * enquanto exporta, o ARQUIVO DE ORIGEM não pode ser excluído, renomeado,
//!   movido nem unido (`export_in_use`);
//! * o arquivo exportado HERDA o que a gravação tinha (pegadinha #51h): layout
//!   recalculado, selo de codec, duração. A transcrição e os nomes de falante
//!   entram com o `OBSTranscribe`.
//! * PRÉVIA (`preview` no pedido): a MESMA exportação, com as mesmas opções, só
//!   que de poucos segundos a partir do cursor e numa pasta temporária fora da
//!   biblioteca (`export-preview`). Não vira card, não leva transcrição; o player
//!   da galeria a abre (os pedidos só de leitura aceitam [`is_preview_path`]).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::time::{Instant, SystemTime};

use serde_json::{json, Value};

use crate::export_captions::CaptionTurn;
use crate::ffmpeg_export::{
    compute_export_layout, export_video, resolve_export_encoder, ExportOptions, ExportResult, ExportSegment, EXPORT_CRF_DEFAULT,
    EXPORT_FPS_MAX, EXPORT_FPS_MAX_SVT, EXPORT_SPEED_MAX,
};
use crate::types::{RecordingMeta, RecordingRegion};
use crate::{encoder, lang, log, mainthread, player};

use super::devices::record_dir_free_bytes;
use super::transcribe::SPEAKERS_META_KEY;
use super::library::{format_bytes_short, is_path_in_record_dir, is_recording_ext, list_recordings_recursive, push_recording_added, sanitize_file_name};
use super::recording::{last_encoder_caps, record_dir, recording_active};
use super::{is_shutting_down, post_error, post_json};

/// Intervalo mínimo entre pushes de progresso: sem isso o JS recebe centenas de
/// mensagens por segundo e a fila do WebView engasga.
const EXPORT_PROGRESS_MS: u64 = 250;

/// Lido pela thread de exportação a cada pacote. 0 = segue, 1 = aborta.
static EXPORT_CANCEL: AtomicI32 = AtomicI32::new(0);
/// Último push de progresso (ms desde `t0`).
static LAST_PUSH_MS: AtomicU64 = AtomicU64::new(0);

/// Durações de prévia aceitas (segundos do arquivo final).
const PREVIEW_SECS: [f64; 3] = [5.0, 10.0, 30.0];

thread_local! {
    static BUSY: Cell<bool> = const { Cell::new(false) };
    /// A exportação em curso é uma prévia (progresso e fim vão por outras
    /// mensagens: a barra do card é da exportação de verdade).
    static PREVIEW: Cell<bool> = const { Cell::new(false) };
    /// Gravação de ORIGEM da exportação em curso. A UI desenha a barra no card
    /// certo, e os caminhos que mexem no arquivo recusam enquanto ele é lido.
    static SOURCE: RefCell<String> = const { RefCell::new(String::new()) };
    /// Último percentual empurrado: a página recriada (volta da hibernação)
    /// recebe ele no `on_page_ready`, sem esperar o próximo tique.
    static LAST_PCT: Cell<f64> = const { Cell::new(0.0) };
}

pub fn export_busy() -> bool {
    BUSY.with(Cell::get)
}

/// `APath` é a gravação que está sendo exportada agora? Quem for excluir/
/// renomear/mover o arquivo tem que perguntar antes: o `ffmpeg_export` lê esse
/// mesmo arquivo do começo ao fim, e mexer nele no meio quebra a exportação (no
/// pior caso deixa um `.part` órfão e a gravação na lixeira).
pub fn export_in_use(path: &str) -> bool {
    export_busy() && SOURCE.with(|s| !s.borrow().is_empty() && s.borrow().eq_ignore_ascii_case(path))
}

/// Shutdown: o laço de exportação olha o próprio flag, não o de shutdown geral.
pub fn signal_cancel() {
    EXPORT_CANCEL.store(1, Ordering::SeqCst);
}

pub fn handle_cancel_export() {
    if !export_busy() {
        return;
    }
    log!("Export: cancelamento pedido pelo usuario.");
    EXPORT_CANCEL.store(1, Ordering::SeqCst);
}

fn source() -> String {
    SOURCE.with(|s| s.borrow().clone())
}

fn push_export_progress(pct: f64) {
    LAST_PCT.with(|p| p.set(pct));
    if PREVIEW.with(Cell::get) {
        post_json(json!({ "type": "export_preview_progress", "pct": pct }));
        return;
    }
    // Id da ORIGEM: com a tela fechada, a barra vive no card dela.
    post_json(json!({ "type": "export_progress", "pct": pct, "id": source() }));
}

/// Página (re)carregada. Hibernar descarrega a interface mas a exportação
/// segue rodando no mesmo processo; a página nova nasce sem saber dela, e
/// sem este push a barra do card (e o cancelar) só voltaria no próximo
/// progresso — que a UI ainda ignorava por não estar "exportando".
pub fn on_page_ready() {
    if export_busy() {
        push_export_progress(LAST_PCT.with(Cell::get));
    }
}

fn push_export_done(ok: bool, canceled: bool, path: &str) {
    if PREVIEW.with(Cell::get) {
        post_json(json!({ "type": "export_preview_done", "ok": ok, "canceled": canceled, "path": path }));
        return;
    }
    // `sourceId`: o card da origem, onde a UI apaga a barra de progresso.
    post_json(json!({ "type": "export_done", "ok": ok, "canceled": canceled, "path": path, "sourceId": source() }));
}

// ---------------------------------------------------------------------------
// Prévia
// ---------------------------------------------------------------------------

/// Pasta das prévias: fora da biblioteca (o watcher não as vê, a galeria não as
/// lista) e esvaziada a cada prévia nova e no fechamento.
fn preview_dir() -> PathBuf {
    crate::config::config_dir().join("export-preview")
}

/// O path é uma prévia de exportação (dentro da pasta delas)? Só os pedidos de
/// LEITURA do player aceitam; excluir/renomear/mover continuam só na biblioteca.
pub fn is_preview_path(path: &str) -> bool {
    let dir = preview_dir();
    let (Ok(dir), Ok(full)) = (std::fs::canonicalize(&dir), std::fs::canonicalize(path)) else { return false };
    full.starts_with(&dir) && full != dir
}

/// Apaga as prévias (a anterior, ao gerar outra; todas, no fechamento).
pub fn clear_previews() {
    let _ = std::fs::remove_dir_all(preview_dir());
}

/// Trechos da prévia: `out_sec` segundos do arquivo FINAL (já com a velocidade)
/// a partir de `from` (tempo da origem), seguindo só os trechos mantidos. Cursor
/// num trecho tirado começa no próximo mantido; depois do último, volta ao
/// primeiro. A prévia pode sair mais curta se o fim chegar antes.
pub fn preview_segments(segs: &[ExportSegment], from: f64, out_sec: f64, speed: f64) -> Vec<ExportSegment> {
    let mut budget = out_sec * speed.max(1.0);
    // Cursor depois do último trecho mantido: começa do primeiro.
    let (first, from) = match segs.iter().position(|s| s.end_sec > from + 0.05) {
        Some(i) => (i, from),
        None => (0, f64::MIN),
    };
    let mut out = Vec::new();
    for (k, s) in segs.iter().enumerate().skip(first) {
        if budget <= 0.0 {
            break;
        }
        let start = if k == first { s.start_sec.max(from).min(s.end_sec) } else { s.start_sec };
        let end = (start + budget).min(s.end_sec);
        if end - start > 0.01 {
            out.push(ExportSegment { start_sec: start, end_sec: end });
            budget -= end - start;
        }
    }
    out
}

/// `<pasta da ORIGEM>\<nome><ext>`, com sufixo ` (N)` se já existir. Fica ao lado
/// da gravação de onde saiu: exportar um vídeo guardado numa subpasta jogava o
/// arquivo na raiz, longe dele.
fn make_export_path(src_path: &str, base_name: &str, ext: &str) -> PathBuf {
    let mut dir = Path::new(src_path).parent().map(Path::to_path_buf).unwrap_or_default();
    if dir.as_os_str().is_empty() {
        dir = record_dir();
    }
    let mut base = base_name.trim().to_owned();
    if base.is_empty() {
        base = "Exportado".into();
    }
    let base = sanitize_file_name(&base);
    let mut cand = dir.join(format!("{base}{ext}"));
    let mut n = 2;
    while cand.exists() {
        cand = dir.join(format!("{base} ({n}){ext}"));
        n += 1;
    }
    cand
}

/// Turnos da transcrição para a legenda queimada: só tempo e texto, lidos do
/// cache — nunca da UI. `<hash>.transcript.json`.
fn load_caption_turns(path: &str) -> Vec<CaptionTurn> {
    let file = crate::transcribe::transcript_path(path);
    let Ok(body) = crate::text_file::read(file) else { return Vec::new() };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(&body) else { return Vec::new() };
    let Some(turns) = root.get("turns").and_then(Value::as_array) else { return Vec::new() };
    turns
        .iter()
        .filter_map(|t| {
            let text = t.get("text").and_then(Value::as_str).unwrap_or("").trim().to_owned();
            let start = t.get("start").and_then(Value::as_f64).unwrap_or(0.0);
            let end = t.get("end").and_then(Value::as_f64).unwrap_or(0.0);
            (!text.is_empty() && end > start).then_some(CaptionTurn { start_sec: start, end_sec: end, text })
        })
        .collect()
}

fn int_array(msg: &Value, key: &str) -> Vec<i32> {
    msg.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|v| v.as_f64().map_or(-1, |f| f as i32)).collect())
        .unwrap_or_default()
}

fn int_field(msg: &Value, key: &str, default: i32) -> i32 {
    msg.get(key).and_then(Value::as_f64).map_or(default, |f| f as i32)
}

fn fail(msg_key: &str) {
    push_export_done(false, false, "");
    post_error(lang::t(msg_key));
}

/// Exporta um trecho da gravação com re-encode. Roda em worker (pode levar
/// minutos). Valida pasta, arquivo e espaço antes de começar.
pub fn handle_export_recording(msg: &Value) {
    if export_busy() {
        post_error(lang::t("error.exportBusy"));
        return;
    }
    // Durante a gravação a captura já disputa CPU/GPU — mesma lógica que bloqueia
    // trocar monitor gravando.
    if recording_active() {
        fail("error.exportWhileRecording");
        return;
    }
    if !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        fail("error.mediaLibUnavailable");
        return;
    }
    let src_path = msg.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
    if src_path.is_empty() || !is_path_in_record_dir(&src_path) || !is_recording_ext(&src_path) || !Path::new(&src_path).is_file() {
        fail("error.fileNotFound");
        return;
    }

    // Prévia: segundos do arquivo final a partir do cursor.
    let preview = msg.get("preview").filter(|p| p.is_object()).map(|p| {
        let sec = p.get("sec").and_then(Value::as_f64).unwrap_or(10.0);
        let sec = PREVIEW_SECS.iter().copied().find(|&s| (s - sec).abs() < 0.5).unwrap_or(10.0);
        (p.get("fromMs").and_then(Value::as_f64).unwrap_or(0.0).max(0.0) / 1000.0, sec)
    });

    let mut opts = ExportOptions { src_path: src_path.clone(), ..Default::default() };
    // Meta da origem (layout de monitores, fps): serve às regiões, ao layout do
    // arquivo exportado e ao selo de fps.
    let src_meta = player::load_recording_meta(&src_path);
    // Só o áudio: nenhuma tela escolhida na exportação.
    opts.no_video = msg.get("noVideo").and_then(Value::as_bool).unwrap_or(false);

    // Trechos que o usuário decidiu manter, em ordem. A UI manda pares de ms; só
    // entram os válidos e o total tem que ser > 0.
    let mut total_sec = 0.0;
    if let Some(segs) = msg.get("segments").and_then(Value::as_array) {
        for s in segs {
            let (start_ms, end_ms) = (int_field(s, "startMs", 0), int_field(s, "endMs", 0));
            if end_ms <= start_ms {
                continue;
            }
            opts.segments.push(ExportSegment { start_sec: start_ms as f64 / 1000.0, end_sec: end_ms as f64 / 1000.0 });
            total_sec += (end_ms - start_ms) as f64 / 1000.0;
        }
    }
    if opts.segments.is_empty() || total_sec <= 0.0 {
        fail("error.exportBadRange");
        return;
    }

    opts.target_height = int_field(msg, "targetHeight", 0);
    // 0 = mantém a taxa da origem; o ffmpeg_export nunca aumenta.
    opts.target_fps = int_field(msg, "fps", 0);
    // Velocidade (1 = normal, até EXPORT_SPEED_MAX, fração vale): o teto de fps vira
    // fps da origem × velocidade e o áudio é acelerado mantendo o tom.
    opts.speed = msg.get("speed").and_then(Value::as_f64).unwrap_or(1.0);
    if opts.speed < 1.0 || opts.speed.is_nan() {
        opts.speed = 1.0;
    }
    opts.speed = opts.speed.min(EXPORT_SPEED_MAX);
    if let Some((from, sec)) = preview {
        opts.segments = preview_segments(&opts.segments, from, sec, opts.speed);
        total_sec = opts.segments.iter().map(|s| s.end_sec - s.start_sec).sum();
        if opts.segments.is_empty() || total_sec <= 0.0 {
            fail("error.exportBadRange");
            return;
        }
        log!("Export: previa de {sec}s a partir de {from:.1}s ({total_sec:.1}s da origem).");
    }
    opts.mix_audio = msg.get("mixAudio").and_then(Value::as_bool).unwrap_or(false);
    // Qualidade: CRF na escala do x264. O ffmpeg_export traduz para o controle
    // nativo de cada encoder, sempre em VBR — não existe alvo de bitrate.
    opts.crf = int_field(msg, "crf", EXPORT_CRF_DEFAULT);
    // Algoritmo de reamostragem: a UI só oferece a escolha com redução de resolução.
    opts.scale_algo = msg.get("scaleAlgo").and_then(Value::as_str).unwrap_or("bicubic").to_owned();

    // Encoder: a UI manda o ID do vocabulário do app ('h264-hw'), nunca o nome do
    // libavcodec — a tradução (e o fallback) fica no backend.
    let enc_pref = msg.get("encoder").and_then(Value::as_str).unwrap_or("auto");
    opts.encoder_name = resolve_export_encoder(enc_pref, &last_encoder_caps().unwrap_or_default());

    // Regiões: a UI manda ÍNDICES do layout; o layout de verdade vem do
    // <hash>.json. Lista vazia = canvas inteiro.
    let region_idx = int_array(msg, "regions");
    if !region_idx.is_empty() {
        match &src_meta {
            Some(m) if !m.layout.regions.is_empty() => {
                for k in region_idx {
                    if k >= 0 && (k as usize) < m.layout.regions.len() {
                        opts.regions.push(m.layout.regions[k as usize].clone());
                    }
                }
            }
            _ => log!("Export: layout ausente — exportando o canvas inteiro."),
        }
    }

    // Recorte livre: retângulo em coordenadas do canvas ORIGINAL, desenhado
    // arrastando as bordas da prévia. SUBSTITUI a lista de regiões — um recorte é
    // uma região de forma livre, e o build_comp_regions já trata retângulo arbitrário
    // (clamp no canvas + arredondamento par). A UI só manda os dois juntos por
    // engano; se acontecer, o recorte ganha (o gesto mais direto).
    if let Some(crop) = msg.get("crop").filter(|c| c.is_object()) {
        let r = RecordingRegion {
            name: String::new(),
            kind: "crop".into(),
            x: int_field(crop, "x", 0),
            y: int_field(crop, "y", 0),
            w: int_field(crop, "w", 0),
            h: int_field(crop, "h", 0),
        };
        // Degenerado (ou negativo) = sem recorte. O clamp contra o tamanho real do
        // quadro é do build_comp_regions; aqui só barramos o que nunca poderia valer.
        if r.w >= 2 && r.h >= 2 && r.x >= 0 && r.y >= 0 {
            log!("Export: recorte {}x{} em ({},{}).", r.w, r.h, r.x, r.y);
            opts.regions = vec![r];
        } else {
            log!("Export: recorte invalido ({}x{} em {},{}) — ignorado.", r.w, r.h, r.x, r.y);
        }
    }

    // Faixas de áudio. A faixa 1 é o MIX de tudo, então não pode conviver com as
    // isoladas (o mesmo áudio entraria duas vezes). A UI já impede; aqui é a rede de
    // segurança. Exceção: o "áudio original" — reproduz as faixas como o OBS gravou
    // (mistura E isoladas, cada uma na sua stream, copiadas sem reprocessar) e deixa
    // o mix desligado (juntaria a mistura com as vozes que ela já contém).
    let audio_sel = int_array(msg, "audioStreams");
    let mix_track_idx = int_field(msg, "mixTrackIndex", -1);
    let original_audio = msg.get("originalAudio").and_then(Value::as_bool).unwrap_or(false);
    if original_audio {
        opts.mix_audio = false;
    }
    let has_first = audio_sel.iter().any(|&s| s >= 0 && s == mix_track_idx);
    let has_other = audio_sel.iter().any(|&s| s >= 0 && s != mix_track_idx);
    if has_first && has_other && !original_audio {
        log!("Export: faixa do mix + isoladas selecionadas — mantendo so o mix.");
        opts.audio_streams = vec![mix_track_idx];
    } else {
        opts.audio_streams = audio_sel.iter().copied().filter(|&s| s >= 0).collect();
    }
    // Sem tela e sem som não há o que exportar.
    if opts.no_video && opts.audio_streams.is_empty() {
        fail("error.exportNothing");
        return;
    }
    // Nome da faixa misturada, no idioma de quem exporta.
    opts.mix_title = lang::t("export.mixTrackTitle");

    // Legenda queimada: os turnos vêm do cache da transcrição, nunca da UI.
    if msg.get("burnCaptions").and_then(Value::as_bool).unwrap_or(false) && !opts.no_video {
        opts.captions = load_caption_turns(&src_path);
        log!("Export: legenda queimada com {} turno(s).", opts.captions.len());
    }

    // A transcrição só acompanha o arquivo se o ÁUDIO dele ainda for o que foi
    // transcrito: a mistura, ou todas as faixas isoladas. Quem sabe quantas faixas
    // existem é a UI, que manda a conclusão. A prévia não leva.
    let keep_transcript = preview.is_none() && msg.get("keepTranscript").and_then(Value::as_bool).unwrap_or(true);
    let seg_starts: Vec<f64> = opts.segments.iter().map(|s| s.start_sec).collect();
    let seg_ends: Vec<f64> = opts.segments.iter().map(|s| s.end_sec).collect();

    // Meta do arquivo exportado: layout recalculado para o que entrou (recorte,
    // monitores, redução de resolução), selo do codec e duração. Sem isto o player
    // do arquivo novo não oferecia mais "ver só o monitor X".
    let mut out_meta = RecordingMeta { duration_sec: (total_sec / opts.speed).round_ties_even() as i32, quality_level: -1, ..Default::default() };
    if !opts.no_video {
        if let Some(m) = &src_meta {
            if let Some(l) = compute_export_layout(&m.layout, &opts.regions, opts.target_height) {
                out_meta.layout = l;
            }
        }
        let (family, hw) = crate::encoder::describe_encoder_id(&opts.encoder_name);
        out_meta.codec = family.to_owned();
        out_meta.codec_hw = hw;
        out_meta.fps = opts.target_fps;
        if out_meta.fps <= 0 {
            if let Some(m) = &src_meta {
                // Sem escolha = o teto, que o ffmpeg_export calcula igual: origem ×
                // velocidade, limitado ao EXPORT_FPS_MAX.
                out_meta.fps = ((m.fps as f64 * opts.speed + 1e-6).trunc() as i32).max(1).min(EXPORT_FPS_MAX);
                if opts.encoder_name == "libsvtav1" && out_meta.fps > EXPORT_FPS_MAX_SVT {
                    out_meta.fps = EXPORT_FPS_MAX_SVT;
                }
            }
        }
    }

    // Container: MP4 (padrão, mais compatível) ou MKV; o muxer vai explícito. Só
    // áudio sai em MP3 (um .mp4 só com som parece vídeo quebrado em muito player):
    // o AAC da gravação não cabe num .mp3, então é recodificado com o libmp3lame na
    // taxa da aba Áudio. MP3 guarda UMA faixa — com o áudio original (ou a mistura
    // marcada) vai a mistura, que já contém todas (pegadinha #51j).
    let ext;
    if opts.no_video {
        opts.container = "mp3".into();
        ext = ".mp3";
        opts.audio_codec = "libmp3lame".into();
        opts.audio_bitrate = encoder::audio_bitrate_kbps() * 1000;
        if mix_track_idx >= 0 && (original_audio || has_first) {
            opts.audio_streams = vec![mix_track_idx];
        }
    } else if msg.get("container").and_then(Value::as_str).unwrap_or("mp4").to_lowercase() == "mkv" {
        opts.container = "matroska".into();
        ext = ".mkv";
    } else {
        opts.container = "mp4".into();
        ext = ".mp4";
    }

    // Escreve num ".part" e só renomeia no fim (#51e).
    let name = msg.get("name").and_then(Value::as_str).unwrap_or("");
    let final_path = if preview.is_some() {
        clear_previews();
        let _ = std::fs::create_dir_all(preview_dir());
        let base = sanitize_file_name(&format!("{} ({})", if name.trim().is_empty() { "Exportado" } else { name.trim() }, lang::t("export.preview.fileSuffix")));
        preview_dir().join(format!("{base}{ext}"))
    } else {
        make_export_path(&src_path, name, ext)
    };
    let part_path = format!("{}.part", final_path.to_string_lossy());
    opts.dst_path = part_path.clone();

    // Espaço em disco. Não dá para saber o tamanho final (depende de quanto o
    // conteúdo comprime), então o tamanho do ORIGINAL é o teto — generoso de
    // propósito: a exportação só reduz, e é melhor recusar do que encher o disco.
    let src_size = std::fs::metadata(&src_path).map(|m| m.len() as i64).unwrap_or(0);
    let needed = src_size + 64 * 1024 * 1024;
    let free = record_dir_free_bytes();
    if preview.is_none() && free >= 0 && free < needed {
        log!("Export: espaco insuficiente — precisa ~{needed}, livre {free}.");
        push_export_done(false, false, "");
        post_error(lang::t_with("error.exportNoSpace", &[("needed", &format_bytes_short(needed)), ("free", &format_bytes_short(free))]));
        return;
    }

    BUSY.with(|b| b.set(true));
    PREVIEW.with(|p| p.set(preview.is_some()));
    SOURCE.with(|s| *s.borrow_mut() = src_path.clone());
    EXPORT_CANCEL.store(0, Ordering::SeqCst);
    LAST_PUSH_MS.store(0, Ordering::SeqCst);
    push_export_progress(0.0);
    log!(
        "Export: iniciando \"{}\" -> \"{}\" ({} trecho(s), {total_sec:.1}s, {:.2}x, encoder={}).",
        file_name(&src_path),
        final_path.file_name().unwrap_or_default().to_string_lossy(),
        opts.segments.len(),
        opts.speed,
        opts.encoder_name
    );

    let speed = opts.speed;
    let is_preview = preview.is_some();
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let progress = |pct: f64| {
            if is_shutting_down() {
                return;
            }
            let now = t0.elapsed().as_millis() as u64 + 1;
            let last = LAST_PUSH_MS.load(Ordering::Relaxed);
            if pct < 100.0 && last != 0 && now.saturating_sub(last) < EXPORT_PROGRESS_MS {
                return;
            }
            LAST_PUSH_MS.store(now, Ordering::Relaxed);
            // Closure `move` com o valor: não existe a captura de variável de laço
            // por referência do Delphi (pegadinha #6).
            mainthread::queue(move || {
                if !is_shutting_down() {
                    push_export_progress(pct);
                }
            });
        };
        let mut res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| export_video(&opts, Some(&progress), Some(&EXPORT_CANCEL))))
            .unwrap_or_else(|_| {
                log!("Export: excecao (panic).");
                ExportResult::Error
            });

        // Saída vazia conta como falha mesmo com Ok.
        if res == ExportResult::Ok && std::fs::metadata(&opts.dst_path).map(|m| m.len()).unwrap_or(0) == 0 {
            res = ExportResult::Error;
        }

        // Sucesso: o arquivo já está COMPLETO (o trailer voltou) — só agora ele ganha
        // o nome final e passa a existir para a lista.
        if res == ExportResult::Ok {
            // Datas da ORIGEM no arquivo novo: a galeria agrupa pela data de
            // modificação, e um arquivo recém-escrito cairia no dia de hoje. Antes do
            // rename, para o watcher já ver a data certa. Falhar aqui só muda o dia
            // em que ele aparece.
            if !is_preview {
                if let Err(e) = copy_file_times(&src_path, &opts.dst_path) {
                    log!("Export: nao copiei as datas da origem: {e}");
                }
            }
            let _ = std::fs::remove_file(&final_path);
            if let Err(e) = std::fs::rename(&opts.dst_path, &final_path) {
                log!(
                    "Export: falha ao renomear \"{}\" -> \"{}\": {e}",
                    file_name(&opts.dst_path),
                    final_path.file_name().unwrap_or_default().to_string_lossy()
                );
                res = ExportResult::Error;
            }
        }

        // Cancelou ou falhou: o .part está pela metade, não deixa lixo.
        if res != ExportResult::Ok {
            let _ = std::fs::remove_file(&opts.dst_path);
        }

        // Transcrição herdada, no relógio do arquivo novo. Aqui na worker: é leitura e escrita
        // de arquivo, e numa gravação longa o JSON com palavras passa de megabytes.
        if res == ExportResult::Ok
            && keep_transcript
            && crate::transcribe::export_transcript(&src_path, &final_path.to_string_lossy(), &seg_starts, &seg_ends, speed)
        {
            log!("Export: transcricao levada pro arquivo exportado.");
        }

        let final_path_s = final_path.to_string_lossy().into_owned();
        mainthread::queue(move || {
            // O source só é limpo DEPOIS dos pushes abaixo: é ele que diz à UI em
            // qual card apagar a barra (push_export_done).
            BUSY.with(|b| b.set(false));
            if is_shutting_down() {
                SOURCE.with(|s| s.borrow_mut().clear());
                PREVIEW.with(|p| p.set(false));
                return;
            }
            // Prévia pronta: só a meta (layout dos monitores pro player) e o aviso.
            // Nada de card, GC ou cópia no fim do arquivo — ela é descartável.
            if is_preview && res == ExportResult::Ok {
                player::save_recording_meta(&final_path_s, &out_meta);
                log!("Export: previa pronta em \"{final_path_s}\".");
                push_export_done(true, false, &final_path_s);
                SOURCE.with(|s| s.borrow_mut().clear());
                PREVIEW.with(|p| p.set(false));
                return;
            }
            match res {
                ExportResult::Ok => {
                    player::garbage_collect_cache(&list_recordings_recursive(&record_dir().to_string_lossy()));
                    // Meta ANTES do card: o push_recording_added lê layout e duração dela.
                    // Os nomes de falante vão depois (o save_recording_meta reescreve o
                    // <hash>.json inteiro).
                    player::save_recording_meta(&final_path_s, &out_meta);
                    if keep_transcript {
                        let names = player::load_meta_sub_object_json(&src_path, SPEAKERS_META_KEY);
                        if names.trim().len() > 2 {
                            player::save_meta_sub_object_json(&final_path_s, SPEAKERS_META_KEY, &names);
                        }
                    }
                    // Meta + transcrição + falantes numa escrita só no fim do arquivo exportado.
                    crate::file_meta::mark_changed(&final_path_s);
                    push_recording_added(&final_path_s, out_meta.duration_sec);
                    push_export_done(true, false, &final_path_s);
                }
                ExportResult::Canceled => push_export_done(false, true, ""),
                ExportResult::NoEncoder => {
                    push_export_done(false, false, "");
                    post_error(lang::t("error.exportNoEncoder"));
                }
                ExportResult::Error => {
                    push_export_done(false, false, "");
                    post_error(lang::t("error.exportFailed"));
                }
            }
            SOURCE.with(|s| s.borrow_mut().clear());
            PREVIEW.with(|p| p.set(false));
        });
    });
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Cópia das datas de criação e modificação (`SetCreationTime`/`SetLastWriteTime`).
fn copy_file_times(from: &str, to: &str) -> std::io::Result<()> {
    use std::os::windows::fs::FileTimesExt;
    let md = std::fs::metadata(from)?;
    let modified: SystemTime = md.modified()?;
    let mut times = std::fs::FileTimes::new().set_modified(modified);
    if let Ok(created) = md.created() {
        times = times.set_created(created);
    }
    std::fs::OpenOptions::new().write(true).open(to)?.set_times(times)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrays_de_inteiros_da_ui() {
        let v = json!({ "regions": [0, 2, "x", 1.0] });
        assert_eq!(int_array(&v, "regions"), vec![0, 2, -1, 1]);
        assert!(int_array(&v, "nada").is_empty());
    }
}

#[cfg(test)]
mod tests_previa {
    use super::*;

    fn seg(a: f64, b: f64) -> ExportSegment {
        ExportSegment { start_sec: a, end_sec: b }
    }

    fn pairs(v: &[ExportSegment]) -> Vec<(f64, f64)> {
        v.iter().map(|s| ((s.start_sec * 100.0).round() / 100.0, (s.end_sec * 100.0).round() / 100.0)).collect()
    }

    #[test]
    fn previa_comeca_no_cursor_e_atravessa_os_cortes() {
        let segs = [seg(0.0, 10.0), seg(20.0, 30.0), seg(40.0, 60.0)];
        assert_eq!(pairs(&preview_segments(&segs, 5.0, 10.0, 1.0)), vec![(5.0, 10.0), (20.0, 25.0)]);
        // Cursor num trecho tirado: começa no próximo mantido.
        assert_eq!(pairs(&preview_segments(&segs, 15.0, 5.0, 1.0)), vec![(20.0, 25.0)]);
        // Depois do último: volta ao começo.
        assert_eq!(pairs(&preview_segments(&segs, 70.0, 5.0, 1.0)), vec![(0.0, 5.0)]);
        // Acelerado: 5 s de saída a 4x = 20 s da origem.
        assert_eq!(pairs(&preview_segments(&segs, 0.0, 5.0, 4.0)), vec![(0.0, 10.0), (20.0, 30.0)]);
        // Perto do fim, sai mais curta.
        assert_eq!(pairs(&preview_segments(&segs, 55.0, 30.0, 1.0)), vec![(55.0, 60.0)]);
    }
}

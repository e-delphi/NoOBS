//! Servidor HTTP local + cache de mídia para tocar gravações no WebView2
//! (porte de `OBSPlayer.pas`).
//!
//! Por que existe: o WebView2 não toca `file://`, e MKV não toca direto no
//! Chromium. A solução é remuxar para MP4 (sem reencode, só troca de container)
//! e servir por `127.0.0.1` com suporte a Range (essencial para o seek).
//!
//! * HTTP em `127.0.0.1:porta-livre` (porta 0 = o SO escolhe, pegadinha #17);
//! * cache em `%LOCALAPPDATA%\NoOBS\cache\<hash>.*` — `<hash>` = 10 primeiros
//!   bytes do SHA-1 do path em minúsculas (pegadinha #55: mover/renomear muda
//!   o hash);
//! * `<hash>.json` guarda duração, layout e os caches de probe/waveform, sempre
//!   sob `META_LOCK` (read-modify-write concorrente perderia chaves).
//!
//! Remux, extração de áudio e miniatura rodam in-process (libav), mas levam
//! dezenas/centenas de ms: chamar de worker.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};

use crate::types::{RecordingMeta, RecordingRegion};
use crate::{ffmpeg_ops, log, probe};

/// Por quanto tempo o Chromium pode reter os bytes de mídia servidos em `/v/`.
/// Uma hora cobre a sessão de quem revê uma gravação; o que importa é NÃO ser
/// `no-store`, que proibia o cache de mídia de guardar qualquer coisa — todo
/// seek (e cada passo de uma varredura em velocidade alta) voltava pela rede.
const MEDIA_CACHE_SECONDS: u32 = 3600;

/// Serializa leitura/escrita do `<hash>.json`. Sem isso dois saves concorrentes
/// (info + waveform) fazem read-modify-write em corrida e um perde a chave do outro.
static META_LOCK: Mutex<()> = Mutex::new(());

/// token (hash + sufixo) → arquivo no disco. Só servimos o que registramos.
static TOKEN_MAP: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);
/// Prévias voláteis dos monitores: id → JPEG. Servidas em `/thumb/<id>.jpg`.
static THUMB_MAP: Mutex<Option<HashMap<String, Vec<u8>>>> = Mutex::new(None);
static THUMB_VERSION: AtomicU32 = AtomicU32::new(0);
static SERVER_PORT: AtomicU16 = AtomicU16::new(0);
static SERVER_STOP: AtomicBool = AtomicBool::new(false);
static SERVER_UP: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------------------
// Cache: caminhos e hash
// ---------------------------------------------------------------------------

/// `%LOCALAPPDATA%\NoOBS\cache\` — servida ao player pelo HTTP local.
pub fn cache_root_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)))
        .unwrap_or_default();
    base.join("NoOBS").join("cache")
}

fn cache_dir_for() -> PathBuf {
    let d = cache_root_dir();
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Hash curto do path: a CHAVE de tudo que é cacheado por gravação
/// (`<hash>.jpg/.dur/.json/.mp4/_aN.m4a`). Exportado para quem grava mais um
/// arquivo por gravação (a transcrição) usar a MESMA chave e não sair de
/// sincronia com o GC e o `rename_cache_entries` (pegadinha #55).
pub fn hash_name(path: &str) -> String {
    // THashSHA1.GetHashBytes(string) codifica em UTF-8.
    let digest = Sha1::digest(path.to_lowercase().as_bytes());
    digest[..10].iter().map(|b| format!("{b:02x}")).collect()
}

/// Migra os arquivos de cache do hash do path antigo para o do novo, depois de
/// um rename. Sem isso o cache fica órfão (só limpo no próximo GC) e a gravação
/// renomeada perde miniatura/duração até regenerar.
pub fn rename_cache_entries(old_path: &str, new_path: &str) {
    // TODO(OBSFileMeta): `OBSFileMeta.RenamePath` — a escrita pendente no fim do
    // arquivo segue o arquivo.
    let dir = cache_dir_for();
    let (old_hash, new_hash) = (hash_name(old_path), hash_name(new_path));
    if old_hash == new_hash {
        return;
    }
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    // `<hash>*` cobre .dur/.jpg/.mp4/.json e _aN.m4a (todos prefixados pelo hash
    // de 20 hex). Colisão com outro arquivo é improvável (SHA-1 de paths distintos).
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(tail) = name.strip_prefix(&old_hash) else { continue };
        let dest = dir.join(format!("{new_hash}{tail}"));
        let _ = std::fs::remove_file(&dest);
        if let Err(err) = std::fs::rename(e.path(), &dest) {
            log!("RenameCacheEntries: move {name} falhou: {err}");
        }
    }
}

/// Remove do cache o que não pertence a nenhuma gravação viva. O hash é o
/// prefixo antes do 1º `.` OU `_` (`<hash>.<ext>` ou `<hash>_aN.m4a`); cortar só
/// na extensão apagava TODAS as faixas de áudio a cada GC.
pub fn garbage_collect_cache(live_paths: &[String]) {
    let dir = cache_root_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let live: std::collections::HashSet<String> = live_paths.iter().map(|p| hash_name(p)).collect();
    let mut removed = 0;
    for e in rd.flatten() {
        if !e.path().is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let hash = name.split(['.', '_']).next().unwrap_or("").to_lowercase();
        if !live.contains(&hash) && std::fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        log!("Cache GC: {removed} arquivo(s) orfao(s) removido(s).");
    }
}

// ---------------------------------------------------------------------------
// Prévias dos monitores
// ---------------------------------------------------------------------------

/// Guarda o JPEG de um monitor (id = `"NoOBS Monitor N"`). Thread-safe.
pub fn set_monitor_thumb(id: &str, jpeg: Vec<u8>) {
    lock(&THUMB_MAP).get_or_insert_with(HashMap::new).insert(id.to_owned(), jpeg);
    THUMB_VERSION.fetch_add(1, Ordering::SeqCst);
}

/// URL da prévia (`/thumb/<id>.jpg?v=N`); vazio se o servidor não subiu ou não há prévia.
pub fn get_monitor_thumb_url(id: &str) -> String {
    if !SERVER_UP.load(Ordering::SeqCst) {
        return String::new();
    }
    if !lock(&THUMB_MAP).as_ref().is_some_and(|m| m.contains_key(id)) {
        return String::new();
    }
    format!("http://127.0.0.1:{}/thumb/{id}.jpg?v={}", SERVER_PORT.load(Ordering::SeqCst), THUMB_VERSION.load(Ordering::SeqCst))
}

// ---------------------------------------------------------------------------
// URLs
// ---------------------------------------------------------------------------

/// Registra token → arquivo e devolve a URL. Vazio sem servidor ou sem arquivo.
fn make_url(token_suffix: &str, file: &Path, ext: &str) -> String {
    if !SERVER_UP.load(Ordering::SeqCst) || !file.exists() {
        return String::new();
    }
    let token = format!("{}{token_suffix}", hash_name(&file.to_string_lossy()));
    lock(&TOKEN_MAP).get_or_insert_with(HashMap::new).insert(token.clone(), file.to_path_buf());
    format!("http://127.0.0.1:{}/v/{token}{ext}", SERVER_PORT.load(Ordering::SeqCst))
}

fn ext_of(path: &str) -> String {
    Path::new(path).extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase())).unwrap_or_default()
}

/// Serve o arquivo original sem transcode (pode falhar no player se o codec não
/// for suportado pelo WebView2, ex.: HEVC sem a extensão). Instantâneo.
pub fn get_direct_url(path: &str) -> String {
    make_url("-direct", Path::new(path), &ext_of(path))
}

/// URL de um arquivo qualquer (ex.: quadros PNG do teste de qualidade). Leva um
/// sufixo de tempo: o mesmo caminho é reescrito a cada teste e o `<img>` não
/// pode reaproveitar a imagem do teste anterior.
pub fn get_file_url(path: &str) -> String {
    let url = make_url("-file", Path::new(path), &ext_of(path));
    if url.is_empty() {
        return url;
    }
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{url}?t={t}")
}

/// Remux MKV → MP4 (`-c copy`; só troca de container). O Chromium toca H.264 em
/// MP4 sempre; HEVC depende de extensão/hardware — responsabilidade do
/// navegador, sem transcode (custo evitado).
fn ensure_cached_mp4(path: &str) -> Option<PathBuf> {
    if !Path::new(path).exists() {
        return None;
    }
    if !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        log!("Player: libavformat indisponivel — nao da pra preparar cache.");
        return None;
    }
    let cache_file = cache_dir_for().join(format!("{}.mp4", hash_name(path)));
    // Cache existe e tem conteúdo: reusa. Inválido: apaga.
    if cache_file.exists() {
        let src = std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(-1);
        let cached = std::fs::metadata(&cache_file).map(|m| m.len()).unwrap_or(0);
        if cached > 0 && src > 0 {
            return Some(cache_file);
        }
        let _ = std::fs::remove_file(&cache_file);
    }
    let t0 = Instant::now();
    if !ffmpeg_ops::remux_file(path, &cache_file.to_string_lossy()) {
        log!("Player: remux falhou para {}", file_name(path));
        return None;
    }
    log!("Player: remux em {}ms -> {}", t0.elapsed().as_millis(), cache_file.file_name().unwrap_or_default().to_string_lossy());
    Some(cache_file)
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// URL "transcodada": garante MP4 jogável. Pode demorar — worker.
pub fn get_transcoded_url(path: &str) -> String {
    match ensure_cached_mp4(path) {
        Some(f) => make_url("-tx", &f, ".mp4"),
        None => String::new(),
    }
}

/// Extrai cada faixa de áudio ISOLADA como `.m4a` (AAC stream copy) e devolve as
/// URLs. Cacheia por hash do path. A faixa 0 (o mix) NÃO é extraída: o `<video>`
/// do player toca o mix da própria fonte e o JS pula `urls[0]` (extrair seria um
/// arquivo nunca consumido), então `urls[0]` fica vazio.
pub fn get_audio_track_urls(path: &str) -> Option<Vec<String>> {
    if !Path::new(path).exists() || !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        return None;
    }
    let report = probe::probe(Path::new(path))?;
    let track_count = report.audio_track_count();
    if track_count == 0 {
        return None;
    }
    let dir = cache_dir_for();
    let token = hash_name(path);
    let mut urls = vec![String::new(); track_count];
    if track_count == 1 {
        return Some(urls); // só o mix: nada isolado a extrair
    }

    // Faixas 1..N-1 = isoladas (cache <hash>_aN.m4a).
    let files: Vec<PathBuf> = (1..track_count).map(|i| dir.join(format!("{token}_a{i}.m4a"))).collect();
    if files.iter().any(|f| !f.exists()) {
        let t0 = Instant::now();
        log!("Player: extraindo {} faixa(s) de audio isolada(s) de {}", track_count - 1, file_name(path));
        // O offset 1 do extract pula o stream do mix.
        let outs: Vec<String> = files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
        if !ffmpeg_ops::extract_audio_tracks(path, &outs, 1, None) {
            log!("Player: extract audio falhou.");
            return None;
        }
        log!("Player: extract em {}ms.", t0.elapsed().as_millis());
    }
    for (i, f) in files.iter().enumerate() {
        urls[i + 1] = make_url(&format!("-a{}", i + 1), f, ".m4a");
    }
    Some(urls)
}

// ---------------------------------------------------------------------------
// Meta unificada (<hash>.json)
// ---------------------------------------------------------------------------

fn meta_file_path(path: &str) -> PathBuf {
    cache_dir_for().join(format!("{}.json", hash_name(path)))
}

fn read_meta_root(file: &Path) -> Option<Map<String, Value>> {
    let text = std::fs::read_to_string(file).ok()?;
    match serde_json::from_str::<Value>(&text).ok()? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

fn write_meta_root(file: &Path, root: &Map<String, Value>) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, Value::Object(root.clone()).to_string())
}

fn as_i32(v: Option<&Value>) -> Option<i32> {
    v.and_then(Value::as_f64).map(|f| f as i32)
}

/// Lê `<hash>.json`. `None` se não existe/não parseia. `quality_level` -1 =
/// desconhecido: 0 é nível VÁLIDO (compactação máxima), então o zero do default
/// não serve de "sem dado".
pub fn load_recording_meta(path: &str) -> Option<RecordingMeta> {
    let file = meta_file_path(path);
    if !file.exists() {
        return None;
    }
    let _g = lock(&META_LOCK);
    let root = read_meta_root(&file)?;
    let mut meta = RecordingMeta { quality_level: -1, ..Default::default() };
    if let Some(d) = as_i32(root.get("duration")) {
        meta.duration_sec = d;
    }
    // Como foi gravado. Ausente em gravações anteriores: codec '' e quality -1
    // e a UI omite os selos.
    if let Some(c) = root.get("codec").and_then(Value::as_str) {
        meta.codec = c.to_owned();
    }
    meta.codec_hw = root.get("codecHw").and_then(Value::as_bool).unwrap_or(false);
    if let Some(f) = as_i32(root.get("fps")) {
        meta.fps = f;
    }
    meta.quality_level = as_i32(root.get("quality")).unwrap_or(-1);
    if let Some(c) = root.get("canvas").and_then(Value::as_object) {
        meta.layout.canvas_w = as_i32(c.get("w")).unwrap_or(0);
        meta.layout.canvas_h = as_i32(c.get("h")).unwrap_or(0);
    }
    if let Some(arr) = root.get("monitors").and_then(Value::as_array) {
        meta.layout.regions = arr
            .iter()
            .map(|r| RecordingRegion {
                name: r.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                kind: r.get("kind").and_then(Value::as_str).unwrap_or("").to_owned(),
                x: as_i32(r.get("x")).unwrap_or(0),
                y: as_i32(r.get("y")).unwrap_or(0),
                w: as_i32(r.get("w")).unwrap_or(0),
                h: as_i32(r.get("h")).unwrap_or(0),
            })
            .collect();
    }
    Some(meta)
}

/// SOBRESCREVE `<hash>.json` (duration/canvas/monitors). Seguro porque só é
/// chamado no stop da gravação, ANTES de qualquer cache de videoInfo/waveform
/// (que entra depois por merge, sob o lock).
pub fn save_recording_meta(path: &str, meta: &RecordingMeta) {
    let file = meta_file_path(path);
    let _g = lock(&META_LOCK);
    let mut obj = Map::new();
    obj.insert("duration".into(), json!(meta.duration_sec));
    // Só grava o que existe: gravações antigas ficam sem estas chaves.
    if !meta.codec.is_empty() {
        obj.insert("codec".into(), json!(meta.codec));
        obj.insert("codecHw".into(), json!(meta.codec_hw));
    }
    if meta.fps > 0 {
        obj.insert("fps".into(), json!(meta.fps));
    }
    if meta.quality_level >= 0 {
        obj.insert("quality".into(), json!(meta.quality_level));
    }
    if meta.layout.canvas_w > 0 && meta.layout.canvas_h > 0 {
        obj.insert("canvas".into(), json!({ "w": meta.layout.canvas_w, "h": meta.layout.canvas_h }));
    }
    if !meta.layout.regions.is_empty() {
        let regs: Vec<Value> = meta
            .layout
            .regions
            .iter()
            .map(|r| json!({ "name": r.name, "kind": r.kind, "x": r.x, "y": r.y, "w": r.w, "h": r.h }))
            .collect();
        obj.insert("monitors".into(), Value::Array(regs));
    }
    if let Err(e) = write_meta_root(&file, &obj) {
        log!("SaveRecordingMeta: erro escrevendo {}: {e}", file.file_name().unwrap_or_default().to_string_lossy());
    }
}

/// Texto JSON de um sub-objeto do `<hash>.json`; vazio se não houver.
pub fn load_meta_sub_object_json(path: &str, key: &str) -> String {
    let file = meta_file_path(path);
    let _g = lock(&META_LOCK);
    if !file.exists() {
        return String::new();
    }
    read_meta_root(&file).and_then(|r| r.get(key).map(Value::to_string)).unwrap_or_default()
}

/// Mescla `value_json` (parseado) sob `key`, preservando o resto.
pub fn save_meta_sub_object_json(path: &str, key: &str, value_json: &str) {
    if value_json.is_empty() {
        return;
    }
    let Ok(new_val) = serde_json::from_str::<Value>(value_json) else { return }; // inválido: não grava
    let file = meta_file_path(path);
    let _g = lock(&META_LOCK);
    let mut root = read_meta_root(&file).unwrap_or_default();
    root.insert(key.to_owned(), new_val);
    if let Err(e) = write_meta_root(&file, &root) {
        log!("SaveMetaSubObjectJson({key}): {e}");
    }
}

/// Várias chaves de uma vez (uma leitura do `<hash>.json`, que pode ter MB de
/// waveform): objeto só com as chaves pedidas que existem (`{}` se nada).
pub fn load_meta_subset(path: &str, keys: &[&str]) -> String {
    let file = meta_file_path(path);
    let _g = lock(&META_LOCK);
    let Some(root) = read_meta_root(&file) else { return "{}".into() };
    let res: Map<String, Value> = keys.iter().filter_map(|k| root.get(*k).map(|v| ((*k).to_owned(), v.clone()))).collect();
    Value::Object(res).to_string()
}

/// Mescla as chaves de `obj_json` no `<hash>.json`, preservando o resto.
pub fn save_meta_subset(path: &str, obj_json: &str) {
    let Ok(Value::Object(src)) = serde_json::from_str::<Value>(obj_json) else { return };
    let file = meta_file_path(path);
    let _g = lock(&META_LOCK);
    let mut root = read_meta_root(&file).unwrap_or_default();
    for (k, v) in src {
        root.insert(k, v);
    }
    if let Err(e) = write_meta_root(&file, &root) {
        log!("SaveMetaSubset: {e}");
    }
}

// ---------------------------------------------------------------------------
// Meta de gravação (duração + miniatura)
// ---------------------------------------------------------------------------

/// Garante duração e miniatura cacheadas. Duração: do `<hash>.json` ou do Probe
/// (e guarda); miniatura: `extract_frame_jpeg` a ~10% (altura 240; o card tem
/// ~212×122 com object-fit:cover). Worker. Devolve `(duração_s, url_da_miniatura)`.
pub fn ensure_recording_meta(path: &str) -> Option<(i32, String)> {
    if !Path::new(path).exists() || !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        return None;
    }
    let thumb = cache_dir_for().join(format!("{}.jpg", hash_name(path)));

    // TODO(OBSFileMeta): `ImportIfNewer(path)` ANTES de ler a meta — arquivo vindo
    // de outra máquina traz layout, transcrição e falantes no fim dele.

    let mut meta = load_recording_meta(path).unwrap_or(RecordingMeta { quality_level: -1, ..Default::default() });
    let mut duration = meta.duration_sec;
    if duration == 0 {
        if let Some(rep) = probe::probe(Path::new(path)) {
            duration = rep.duration.round() as i32;
        }
        if duration > 0 {
            meta.duration_sec = duration;
            save_recording_meta(path, &meta);
        }
    }

    // Miniatura vazia/quebrada de uma corrida anterior: apaga, senão `exists`
    // segue verdadeiro e a imagem fica quebrada para sempre.
    if thumb.exists() && std::fs::metadata(&thumb).map(|m| m.len() < 100).unwrap_or(false) {
        log!("Player: thumb cacheado vazio/curto, regenerando: {}", thumb.file_name().unwrap_or_default().to_string_lossy());
        let _ = std::fs::remove_file(&thumb);
    }
    if !thumb.exists() {
        let seek = if duration > 10 { duration / 10 } else { 1 };
        if !ffmpeg_ops::extract_frame_jpeg(path, &thumb.to_string_lossy(), seek, 240) {
            log!("Player: thumbnail falhou para {}", file_name(path));
        }
    }
    let url = if thumb.exists() { make_url("-thumb", &thumb, ".jpg") } else { String::new() };
    ((duration > 0) || !url.is_empty()).then_some((duration, url))
}

/// Só lê o cache (instantâneo, sem libav): para a main thread.
pub fn get_cached_meta(path: &str) -> (i32, String, RecordingMeta) {
    let mut meta = RecordingMeta { quality_level: -1, ..Default::default() };
    if !Path::new(path).exists() {
        return (0, String::new(), meta);
    }
    let dir = cache_root_dir();
    if !dir.is_dir() {
        return (0, String::new(), meta);
    }
    // `meta` sai inteira (duração, layout, codec/fps/qualidade): o card usa os
    // três últimos para os selos sem reabrir o <hash>.json.
    if let Some(m) = load_recording_meta(path) {
        meta = m;
    }
    let thumb = dir.join(format!("{}.jpg", hash_name(path)));
    let url = if thumb.exists() && SERVER_UP.load(Ordering::SeqCst) { make_url("-thumb", &thumb, ".jpg") } else { String::new() };
    (meta.duration_sec, url, meta)
}

// ---------------------------------------------------------------------------
// Servidor HTTP
// ---------------------------------------------------------------------------

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("m4a") => "audio/mp4",
        Some("aac") => "audio/aac",
        Some("mp3") => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

struct Request {
    method: String,
    doc: String,
    range: Option<String>,
}

/// Respostas já com status e cabeçalhos; o corpo (se houver) é transmitido do
/// arquivo em blocos — memória por requisição = o buffer, independente do tamanho do vídeo.
enum Body {
    None,
    Bytes(Vec<u8>),
    File { path: PathBuf, start: u64, len: u64 },
}

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Body,
}

impl Response {
    fn simple(status: u16, text: &str) -> Self {
        Self { status, headers: vec![("Content-Type".into(), "text/plain".into())], body: Body::Bytes(text.as_bytes().to_vec()) }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn serve_file_with_range(req: &Request, file: &Path) -> Response {
    let Ok(meta) = std::fs::metadata(file) else { return Response::simple(404, "not found") };
    let total = meta.len() as i64;
    // Arquivo vazio/inválido: 200 vazio (evita range negativo).
    if total <= 0 {
        return Response { status: 200, headers: vec![], body: Body::None };
    }
    let (mut start, mut end) = (0i64, total - 1);
    let mut status = 200;
    let mut headers: Vec<(String, String)> = Vec::new();

    if let Some(range) = req.range.as_deref().and_then(|r| r.strip_prefix("bytes=")) {
        // "bytes=START-END", "bytes=START-" ou "bytes=-N" (sufixo).
        if let Some((s, e)) = range.split_once('-') {
            if s.is_empty() {
                // Sufixo: últimos N bytes (RFC 7233).
                let n: i64 = e.parse().unwrap_or(0);
                if n > 0 {
                    start = total - n.min(total);
                    end = total - 1;
                }
            } else {
                start = s.parse().unwrap_or(0);
                if !e.is_empty() {
                    end = e.parse().unwrap_or(total - 1);
                }
            }
        }
        end = end.min(total - 1);
        if start > end {
            return Response { status: 416, headers: vec![("Content-Range".into(), format!("bytes */{total}"))], body: Body::None };
        }
        // Serve EXATAMENTE o range pedido (sem cap): cortar por chunk QUEBRAVA MP4
        // com moov no fim (o Chromium pede a cauda para achar o moov e entrava em
        // laço re-pedindo, disco a 8 MB/s sem parar).
        status = 206;
        headers.push(("Content-Range".into(), format!("bytes {start}-{end}/{total}")));
    }
    let len = (end - start + 1) as u64;
    let ct = content_type_for(file);
    headers.push(("Content-Type".into(), ct.into()));
    headers.push(("Accept-Ranges".into(), "bytes".into()));
    // Mídia cacheável (o conteúdo de uma URL /v/ não muda: o token é o hash do
    // PATH e os arquivos por trás são escritos uma vez; a porta é efêmera, então
    // nada sobrevive para a sessão seguinte). `private`: conteúdo do usuário.
    // IMAGEM fica de fora: a miniatura PODE ser regerada na mesma sessão com a
    // mesma URL.
    let cc = if ct.starts_with("image/") { "no-store".to_owned() } else { format!("private, max-age={MEDIA_CACHE_SECONDS}") };
    headers.push(("Cache-Control".into(), cc));
    Response { status, headers, body: Body::File { path: file.to_path_buf(), start: start as u64, len } }
}

fn handle_request(req: &Request) -> Response {
    let doc = &req.doc;
    let mut resp = if let Some(rest) = doc.strip_prefix("/thumb/") {
        // /thumb/<id>.jpg — prévias voláteis de monitor.
        let id = rest.split('.').next().unwrap_or("");
        match lock(&THUMB_MAP).as_ref().and_then(|m| m.get(id)) {
            Some(j) if !j.is_empty() => Response {
                status: 200,
                headers: vec![("Content-Type".into(), "image/jpeg".into()), ("Cache-Control".into(), "no-store".into())],
                body: Body::Bytes(j.clone()),
            },
            _ => Response::simple(404, "no thumb"),
        }
    } else if let Some(rest) = doc.strip_prefix("/v/") {
        // /v/<token>.ext — arquivos registrados.
        let token = rest.split('.').next().unwrap_or("");
        let file = lock(&TOKEN_MAP).as_ref().and_then(|m| m.get(token).cloned());
        match file {
            Some(f) => serve_file_with_range(req, &f),
            None => Response::simple(404, "unknown token"),
        }
    } else {
        Response::simple(404, "not found")
    };
    // CORS restrito ao origin EXATO da UI (https://noobs.app), não '*': a UI
    // roda lá e o áudio das faixas vem de 127.0.0.1 (cross-origin; sem ACAO o
    // Web Audio marca o recurso como "tainted" e o GainNode dá SILÊNCIO). '*'
    // deixaria qualquer página no navegador normal do usuário ler cross-origin
    // as prévias de monitor (id ADIVINHÁVEL = screenshot do desktop ao vivo).
    resp.headers.push(("Access-Control-Allow-Origin".into(), "https://noobs.app".into()));
    resp.headers.push(("Vary".into(), "Origin".into()));
    resp
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn write_response(stream: &mut TcpStream, resp: Response, head_only: bool) -> std::io::Result<()> {
    let body_len: u64 = match &resp.body {
        Body::None => 0,
        Body::Bytes(b) => b.len() as u64,
        Body::File { len, .. } => *len,
    };
    let mut head = format!("HTTP/1.1 {} {}\r\n", resp.status, reason(resp.status));
    for (k, v) in &resp.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!("Content-Length: {body_len}\r\nConnection: keep-alive\r\n\r\n"));
    stream.write_all(head.as_bytes())?;
    if head_only {
        return Ok(());
    }
    match resp.body {
        Body::None => {}
        Body::Bytes(b) => stream.write_all(&b)?,
        Body::File { path, start, len } => {
            // FILE_SHARE_READ | FILE_SHARE_DELETE: permite mover/renomear o arquivo
            // enquanto o player o serve (sem isso o .mkv fica travado e "excluir
            // enquanto toca" falha com sharing violation).
            let mut f = match std::fs::OpenOptions::new().read(true).share_mode(1 | 4).open(&path) {
                Ok(f) => f,
                Err(_) => return Ok(()), // cabeçalho já saiu; a conexão cai
            };
            f.seek(SeekFrom::Start(start))?;
            let mut remaining = len;
            let mut buf = vec![0u8; 256 * 1024];
            while remaining > 0 {
                let want = remaining.min(buf.len() as u64) as usize;
                let n = f.read(&mut buf[..want])?;
                if n == 0 {
                    break;
                }
                stream.write_all(&buf[..n])?;
                remaining -= n as u64;
            }
        }
    }
    stream.flush()
}

/// Lê uma requisição (linha + cabeçalhos). `None` = conexão fechada/inválida.
fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();
    let mut range = None;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 {
            return None;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("range") {
                range = Some(v.trim().to_owned());
            }
        }
    }
    let doc = percent_decode(target.split('?').next().unwrap_or(""));
    Some(Request { method, doc, range })
}

fn serve_connection(stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_nodelay(true);
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    let mut stream = stream;
    while !SERVER_STOP.load(Ordering::SeqCst) {
        let Some(req) = read_request(&mut reader) else { return };
        let head_only = req.method.eq_ignore_ascii_case("HEAD");
        if write_response(&mut stream, handle_request(&req), head_only).is_err() {
            return; // cliente abortou (seek/troca de vídeo): normal
        }
    }
}

/// Sobe o servidor em `127.0.0.1:porta-livre` (porta 0). Idempotente.
pub fn start_player_server() {
    if SERVER_UP.load(Ordering::SeqCst) {
        return;
    }
    lock(&TOKEN_MAP).get_or_insert_with(HashMap::new);
    lock(&THUMB_MAP).get_or_insert_with(HashMap::new);
    THUMB_VERSION.store(0, Ordering::SeqCst);
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            log!("Player: falha ao subir servidor: {e}");
            return;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    SERVER_PORT.store(port, Ordering::SeqCst);
    SERVER_STOP.store(false, Ordering::SeqCst);
    SERVER_UP.store(true, Ordering::SeqCst);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if SERVER_STOP.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(s) = conn {
                std::thread::spawn(move || serve_connection(s));
            }
        }
    });
    log!("Player: HTTP server em 127.0.0.1:{port}");
}

pub fn stop_player_server() {
    if !SERVER_UP.swap(false, Ordering::SeqCst) {
        return;
    }
    SERVER_STOP.store(true, Ordering::SeqCst);
    // Acorda o accept para o laço ver o flag.
    let port = SERVER_PORT.swap(0, Ordering::SeqCst);
    let _ = TcpStream::connect(("127.0.0.1", port));
    *lock(&TOKEN_MAP) = None;
    *lock(&THUMB_MAP) = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_tem_20_hex_e_ignora_caixa() {
        let a = hash_name("C:\\Videos\\A.mkv");
        assert_eq!(a.len(), 20);
        assert_eq!(a, hash_name("c:\\videos\\a.MKV"));
        assert_ne!(a, hash_name("C:\\Videos\\B.mkv"));
    }

    #[test]
    fn percent_decode_espaco() {
        assert_eq!(percent_decode("/thumb/NoOBS%20Monitor%200.jpg"), "/thumb/NoOBS Monitor 0.jpg");
        assert_eq!(percent_decode("/v/abc"), "/v/abc");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn range_sufixo_e_aberto() {
        let dir = std::env::temp_dir().join("noobs_player_test.bin");
        std::fs::write(&dir, vec![0u8; 1000]).unwrap();
        let mk = |r: &str| Request { method: "GET".into(), doc: String::new(), range: Some(r.into()) };
        let r = serve_file_with_range(&mk("bytes=-100"), &dir);
        assert_eq!(r.status, 206);
        assert!(r.headers.iter().any(|(k, v)| k == "Content-Range" && v == "bytes 900-999/1000"));
        let r = serve_file_with_range(&mk("bytes=10-"), &dir);
        assert!(r.headers.iter().any(|(k, v)| k == "Content-Range" && v == "bytes 10-999/1000"));
        let r = serve_file_with_range(&mk("bytes=2000-"), &dir);
        assert_eq!(r.status, 416);
        let _ = std::fs::remove_file(&dir);
    }
}

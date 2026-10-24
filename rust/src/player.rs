//! Cache de mídia por gravação (porte da parte de cache do `OBSPlayer.pas`).
//! O servidor HTTP local que servia o WebView2 saiu com a interface web: o
//! player e a exportação leem o arquivo direto (`playback`).
//!
//! * cache em `%LOCALAPPDATA%\NoOBS\cache\<hash>.*` — `<hash>` = 10 primeiros
//!   bytes do SHA-1 do path em minúsculas (pegadinha #55: mover/renomear muda
//!   o hash);
//! * `<hash>.json` guarda duração, layout e os caches de probe/waveform, sempre
//!   sob `META_LOCK` (read-modify-write concorrente perderia chaves).
//!
//! A miniatura roda in-process (libav), mas leva dezenas/centenas de ms:
//! chamar de worker.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};

use crate::types::{RecordingMeta, RecordingRegion};
use crate::{ffmpeg_ops, log, probe};

/// Serializa leitura/escrita do `<hash>.json`. Sem isso dois saves concorrentes
/// (info + waveform) fazem read-modify-write em corrida e um perde a chave do outro.
static META_LOCK: Mutex<()> = Mutex::new(());

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------------------
// Cache: caminhos e hash
// ---------------------------------------------------------------------------

/// `%LOCALAPPDATA%\NoOBS\cache\`.
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
    // A escrita pendente no fim do arquivo segue o arquivo.
    crate::file_meta::rename_path(old_path, new_path);
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
// Meta unificada (<hash>.json)
// ---------------------------------------------------------------------------

fn meta_file_path(path: &str) -> PathBuf {
    cache_dir_for().join(format!("{}.json", hash_name(path)))
}

fn read_meta_root(file: &Path) -> Option<Map<String, Value>> {
    let text = crate::text_file::read(file).ok()?;
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
/// ~212×122 com object-fit:cover). Worker. Devolve `(duração_s, arquivo_da_miniatura)`.
pub fn ensure_recording_meta(path: &str) -> Option<(i32, String)> {
    if !Path::new(path).exists() || !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        return None;
    }
    let thumb = cache_dir_for().join(format!("{}.jpg", hash_name(path)));

    // ANTES de ler a meta: arquivo vindo de outra máquina traz layout, transcrição
    // e falantes no fim dele (e a duração do bloco evita o Probe).
    crate::file_meta::import_if_newer(path);

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
            log!("Player: thumbnail falhou para {}", Path::new(path).file_name().unwrap_or_default().to_string_lossy());
        }
    }
    // A janela decodifica o JPG do cache direto.
    let file = if thumb.exists() { thumb.to_string_lossy().into_owned() } else { String::new() };
    ((duration > 0) || !file.is_empty()).then_some((duration, file))
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
    let file = if thumb.exists() { thumb.to_string_lossy().into_owned() } else { String::new() };
    (meta.duration_sec, file, meta)
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
}

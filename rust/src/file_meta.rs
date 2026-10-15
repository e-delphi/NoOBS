//! Dados do NoOBS GRAVADOS NO PRÓPRIO ARQUIVO DE VÍDEO (porte de `OBSFileMeta.pas`,
//! pegadinha #65).
//!
//! O cache (`%LOCALAPPDATA%\NoOBS\cache\<hash>.json/.txt/...`) continua sendo a
//! cópia de trabalho: rápido, local, nunca puxa da nuvem. O que não dá para refazer
//! — layout dos monitores, transcrição, nomes de falante, como foi gravado — ganha
//! uma cópia DENTRO do arquivo, que viaja com ele para outra máquina.
//!
//! Onde: um bloco acrescentado ao FIM do arquivo, sem tocar em nenhum byte do vídeo,
//! disfarçado de elemento que o formato manda ignorar:
//! * MKV: elemento EBML Void (ID `$EC`, tamanho em vint de 8 bytes)
//! * MP4: caixa `free` (tamanho u32 big-endian + `free`)
//!
//! Conteúdo (igual nos dois): `'NOOBSMETA' + zlib(JSON) + rodapé de 24 bytes`, SEMPRE
//! no fim: `'NOOBSEND' | versão u16 | reservado u16 | tamanho do envelope u64 |
//! crc32 do zlib u32` (little-endian). Ler = seek para os últimos 24 bytes; o vídeo
//! nunca é lido.
//!
//! MKV interrompido NO MEIO de um cluster: o cluster declara um tamanho que passa do
//! fim do arquivo e o leitor engoliria o começo do bloco como pacote. Por isso só se
//! escreve quando a estrutura termina num limite de elemento (`mkv_data_end` /
//! `mp4_data_end`); senão os dados ficam só no cache (`WriteResult::Unsafe`).
//!
//! Sincronia cache ↔ arquivo por uma marca de revisão (UTC ISO):
//! * [`mark_changed`]: algo mudou no cache → carimba `tailRev` no `<hash>.json` e
//!   agenda a escrita;
//! * [`import_if_newer`]: bloco mais novo que o cache (arquivo veio de outra
//!   máquina) → popula o cache. Cache mais novo → agenda a escrita. `tailSig`
//!   (tamanho + data do arquivo) evita rereler o rodapé de quem não mudou.
//!
//! A escrita roda numa worker, com lista de pendentes PERSISTIDA: arquivo aberto por
//! outro processo, só na nuvem ou com erro fica para depois.

use std::collections::HashMap;
use std::fs::{File, FileTimes, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde_json::{json, Map, Value};

use crate::{log, player};

const MAGIC_HEAD: &[u8; 9] = b"NOOBSMETA";
const MAGIC_FOOT: &[u8; 8] = b"NOOBSEND";
const FOOT_SIZE: usize = 24;
const BLOCK_VERSION: u16 = 1;

/// Chaves do `<hash>.json` que viajam. `waveform`/`waveformHi`/`videoInfo` ficam de
/// fora: são refeitas sozinhas na outra máquina e pesam MB.
const PORTABLE_KEYS: [&str; 8] = ["duration", "canvas", "monitors", "codec", "codecHw", "fps", "quality", "speakers"];
const KEY_REV: &str = "tailRev";
const KEY_SIG: &str = "tailSig";

const PENDING_FILE: &str = "filemeta-pending.json";
/// Debounce: transcrição + nomes costumam chegar em rajada.
const DELAY_CHANGE_MS: u64 = 1500;
const RETRY_LOCKED_MS: u64 = 20_000;
const RETRY_CLOUD_MS: u64 = 5 * 60_000;
const RETRY_ERROR_MS: u64 = 60_000;
const STARTUP_GRACE_MS: u64 = 10_000;

const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
const FILE_SHARE_READ: u32 = 1;
const FILE_SHARE_WRITE: u32 = 2;
const FILE_SHARE_DELETE: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteResult {
    Ok,
    Nothing,
    Locked,
    Cloud,
    Unsafe,
    Gone,
    Error,
}

// ---------------------------------------------------------------------------
// util
// ---------------------------------------------------------------------------

/// Revisão = agora em UTC ISO com milésimos. Comparável como string.
fn now_rev() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// `mkv` | `mp4` | vazio. `.mp3` e o resto: sem bloco "ignore isto" confiável.
fn kind_of(path: &str) -> &'static str {
    match Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("mkv") => "mkv",
        Some("mp4") => "mp4",
        _ => "",
    }
}

/// Mesma regra do `IsFileCloudOnly` do Bridge: o atributo é metadado local e não
/// dispara o download. Ler (ou escrever) o fim do arquivo dispararia.
fn is_cloud_only(path: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::metadata(path).map(|m| m.file_attributes() & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS != 0).unwrap_or(false)
}

/// Tamanho + data de modificação: muda quando o arquivo foi trocado ou copiado por
/// cima. Igual ao guardado = nem precisa ler o rodapé.
fn file_sig(path: &str) -> String {
    use std::os::windows::fs::MetadataExt;
    match std::fs::metadata(path) {
        Ok(m) => format!("{}:{}", m.len(), m.last_write_time()),
        Err(_) => String::new(),
    }
}

fn read_at(f: &mut File, pos: u64, count: usize) -> Vec<u8> {
    if count == 0 || f.seek(SeekFrom::Start(pos)).is_err() {
        return Vec::new();
    }
    let mut buf = vec![0u8; count];
    let mut got = 0;
    while got < count {
        match f.read(&mut buf[got..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => got += n,
        }
    }
    buf.truncate(got);
    buf
}

fn be(b: &[u8], off: usize, len: usize) -> u64 {
    b[off..off + len].iter().fold(0u64, |a, &v| (a << 8) | v as u64)
}

fn le(b: &[u8], off: usize, len: usize) -> u64 {
    b[off..off + len].iter().rev().fold(0u64, |a, &v| (a << 8) | v as u64)
}

fn crc32(data: &[u8]) -> u32 {
    let mut h = crc32fast_free::Hasher::new();
    h.update(data);
    h.finalize()
}

/// CRC-32 (IEEE, o mesmo do `BlockCrc32` do Delphi) sem depender de mais uma crate:
/// a tabela é calculada uma vez.
mod crc32fast_free {
    use std::sync::OnceLock;

    fn table() -> &'static [u32; 256] {
        static T: OnceLock<[u32; 256]> = OnceLock::new();
        T.get_or_init(|| {
            let mut t = [0u32; 256];
            for (i, slot) in t.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
                *slot = c;
            }
            t
        })
    }

    pub struct Hasher(u32);

    impl Hasher {
        pub fn new() -> Self {
            Self(0xFFFF_FFFF)
        }
        pub fn update(&mut self, data: &[u8]) {
            let t = table();
            for &b in data {
                self.0 = t[((self.0 ^ b as u32) & 0xFF) as usize] ^ (self.0 >> 8);
            }
        }
        pub fn finalize(self) -> u32 {
            self.0 ^ 0xFFFF_FFFF
        }
    }
}

// ---------------------------------------------------------------------------
// rodapé
// ---------------------------------------------------------------------------

/// `(tamanho total do envelope, crc)` se os últimos 24 bytes forem um rodapé nosso.
fn read_footer(f: &mut File, size: u64) -> Option<(u64, u32)> {
    if size < FOOT_SIZE as u64 {
        return None;
    }
    let foot = read_at(f, size - FOOT_SIZE as u64, FOOT_SIZE);
    if foot.len() != FOOT_SIZE || &foot[..8] != MAGIC_FOOT {
        return None;
    }
    if le(&foot, 8, 2) != BLOCK_VERSION as u64 {
        return None;
    }
    let total = le(&foot, 12, 8);
    let crc = le(&foot, 20, 4) as u32;
    (total >= (FOOT_SIZE + MAGIC_HEAD.len()) as u64 && total <= size).then_some((total, crc))
}

/// `(json, tamanho do envelope)`.
fn read_tail_block_h(f: &mut File, size: u64) -> Option<(String, u64)> {
    let (total, crc) = read_footer(f, size)?;
    if total > 64 * 1024 * 1024 {
        return None; // sanidade
    }
    let env = read_at(f, size - total, total as usize);
    if env.len() as u64 != total {
        return None;
    }
    // Envelope: 8 (mp4) ou 9 (mkv) bytes antes da assinatura.
    let z_start = (0..=16usize).find(|&i| env.len() >= i + MAGIC_HEAD.len() && &env[i..i + MAGIC_HEAD.len()] == MAGIC_HEAD)? + MAGIC_HEAD.len();
    let z_end = env.len().checked_sub(FOOT_SIZE)?;
    if z_end <= z_start {
        return None;
    }
    let z = &env[z_start..z_end];
    if crc32(z) != crc {
        return None;
    }
    let mut raw = Vec::new();
    ZlibDecoder::new(z).read_to_end(&mut raw).ok()?;
    Some((String::from_utf8_lossy(&raw).into_owned(), total))
}

fn open_read(path: &str) -> std::io::Result<File> {
    OpenOptions::new().read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).open(path)
}

/// Lê o bloco do fim do arquivo (JSON), se houver e estiver íntegro.
pub fn read_tail_block(path: &str) -> Option<String> {
    let mut f = open_read(path).ok()?;
    let size = f.metadata().ok()?.len();
    read_tail_block_h(&mut f, size).map(|(j, _)| j)
}

// ---------------------------------------------------------------------------
// estrutura do container
// ---------------------------------------------------------------------------

/// `(id, tamanho, tamanho_desconhecido, bytes consumidos pelo cabeçalho)`.
fn parse_ebml(b: &[u8]) -> Option<(u32, u64, bool, usize)> {
    let mut p = 0usize;
    let first = *b.get(p)?;
    let mut len = 1usize;
    let mut m = 0x80u8;
    while len <= 4 && first & m == 0 {
        len += 1;
        m >>= 1;
    }
    if len > 4 || p + len > b.len() {
        return None;
    }
    let id = be(b, p, len) as u32;
    p += len;
    let first = *b.get(p)?;
    let mut len = 1usize;
    let mut m = 0x80u8;
    while len <= 8 && first & m == 0 {
        len += 1;
        m >>= 1;
    }
    if len > 8 || p + len > b.len() {
        return None;
    }
    let mut size = (first & (m.wrapping_sub(1))) as u64;
    for i in 1..len {
        size = (size << 8) | b[p + i] as u64;
    }
    // Todos os bits de valor em 1 = tamanho desconhecido.
    let unknown = size == (1u64 << (7 * len)) - 1;
    p += len;
    Some((id, size, unknown, p))
}

/// Onde o vídeo termina (= onde o bloco entra), ou `None` se escrever ali cairia
/// dentro de um elemento incompleto. `end` = fim do arquivo SEM o nosso bloco anterior.
fn mkv_data_end(f: &mut File, end: u64) -> Option<u64> {
    let b = read_at(f, 0, 64);
    let (id, sz, unk, p) = parse_ebml(&b)?;
    if id != 0x1A45_DFA3 || unk {
        return None;
    }
    let mut pos = p as u64 + sz;
    let b = read_at(f, pos, 16);
    let (id, sz, unk, p) = parse_ebml(&b)?;
    if id != 0x1853_8067 {
        return None;
    }
    pos += p as u64;
    if !unk {
        // Segmento fechado (trailer escrito): o que vier depois dele é nosso (ou
        // sobra de uma escrita interrompida) e sai junto.
        return (pos + sz <= end).then_some(pos + sz);
    }
    // Tamanho desconhecido (gravação interrompida): anda pelos filhos, um cabeçalho
    // por elemento (um cluster a cada poucos segundos de vídeo).
    while pos < end {
        let start = pos;
        let b = read_at(f, pos, 12);
        let (id, sz, unk, p) = parse_ebml(&b)?;
        if unk {
            return None;
        }
        let next = pos + p as u64 + sz;
        if next > end {
            // Passa do fim: só serve se for um Void (sobra nossa); um cluster aberto
            // é a exceção que o protótipo mediu.
            return (id == 0xEC).then_some(start);
        }
        pos = next;
    }
    Some(pos)
}

fn mp4_data_end(f: &mut File, end: u64) -> Option<u64> {
    let mut pos = 0u64;
    while pos < end {
        let b = read_at(f, pos, 16);
        if b.len() < 8 {
            return None;
        }
        let mut sz = be(&b, 0, 4);
        let typ = &b[4..8];
        let mut hdr = 8u64;
        if sz == 1 {
            if b.len() < 16 {
                return None;
            }
            sz = be(&b, 8, 8);
            hdr = 16;
        }
        if sz == 0 || sz < hdr {
            return None; // 0 = "até o fim do arquivo"
        }
        let next = pos + sz;
        if next > end {
            return (typ == b"free" || typ == b"skip").then_some(pos);
        }
        pos = next;
    }
    Some(pos)
}

// ---------------------------------------------------------------------------
// escrita
// ---------------------------------------------------------------------------

fn build_envelope(json: &str, kind: &str) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
    let _ = enc.write_all(json.as_bytes());
    let z = enc.finish().unwrap_or_default();
    let mut body = Vec::with_capacity(MAGIC_HEAD.len() + z.len());
    body.extend_from_slice(MAGIC_HEAD);
    body.extend_from_slice(&z);
    let n = (body.len() + FOOT_SIZE) as u64;
    let head: Vec<u8> = if kind == "mkv" {
        // ID Void + tamanho em vint de 8 bytes (0x01 + 7 bytes).
        let mut h = vec![0xEC, 0x01];
        h.extend((0..7).map(|i| (n >> (8 * (6 - i))) as u8));
        h
    } else {
        let total = 8 + n;
        let mut h: Vec<u8> = (0..4).map(|i| (total >> (8 * (3 - i))) as u8).collect();
        h.extend_from_slice(b"free");
        h
    };
    let total = head.len() as u64 + n;
    let mut foot = Vec::with_capacity(FOOT_SIZE);
    foot.extend_from_slice(MAGIC_FOOT);
    foot.extend_from_slice(&[BLOCK_VERSION as u8, 0, 0, 0]);
    foot.extend((0..8).map(|i| (total >> (8 * i)) as u8));
    foot.extend((0..4).map(|i| (crc32(&z) >> (8 * i)) as u8));
    let mut out = head;
    out.extend(body);
    out.extend(foot);
    out
}

/// Troca (ou cria) o bloco. Ordem pensada para queda no meio: primeiro CORTA (o
/// arquivo volta a ser o original limpo), depois escreve o novo. Datas restauradas
/// no fim — a galeria agrupa pela modificação.
fn write_tail_block(path: &str, json: &str) -> WriteResult {
    let kind = kind_of(path);
    if kind.is_empty() {
        return WriteResult::Nothing;
    }
    if !Path::new(path).is_file() {
        return WriteResult::Gone;
    }
    if is_cloud_only(path) {
        return WriteResult::Cloud;
    }
    // Só FILE_SHARE_READ: se OUTRO processo tem o arquivo aberto para escrita (o OBS
    // gravando, uma cópia em curso), a abertura falha e tentamos depois — nunca
    // escrevemos por baixo de quem está escrevendo.
    let mut f = match OpenOptions::new().read(true).write(true).share_mode(FILE_SHARE_READ).open(path) {
        Ok(f) => f,
        Err(e) => {
            return match e.raw_os_error() {
                Some(ERROR_SHARING_VIOLATION) | Some(ERROR_LOCK_VIOLATION) => WriteResult::Locked,
                _ => {
                    log!("FileMeta: nao abri \"{}\" pra escrita ({e}).", file_name(path));
                    WriteResult::Error
                }
            };
        }
    };
    let Ok(md) = f.metadata() else { return WriteResult::Error };
    let times = {
        let mut t = FileTimes::new();
        if let Ok(a) = md.accessed() {
            t = t.set_accessed(a);
        }
        if let Ok(m) = md.modified() {
            t = t.set_modified(m);
        }
        if let Ok(c) = md.created() {
            t = t.set_created(c);
        }
        t
    };
    let size = md.len();
    let old_len = read_tail_block_h(&mut f, size).map(|(_, l)| l).unwrap_or(0);
    let data_end = if kind == "mkv" { mkv_data_end(&mut f, size - old_len) } else { mp4_data_end(&mut f, size - old_len) };
    let Some(data_end) = data_end else { return WriteResult::Unsafe };
    let env = build_envelope(json, kind);
    if f.set_len(data_end).is_err() || f.seek(SeekFrom::Start(data_end)).is_err() {
        return WriteResult::Error;
    }
    if f.write_all(&env).is_err() || f.flush().is_err() {
        log!("FileMeta: escrita incompleta em \"{}\".", file_name(path));
        return WriteResult::Error;
    }
    let _ = f.set_times(times);
    drop(f);
    // De novo depois de fechar: garante que o fechamento não carimbou "agora".
    if let Ok(g) = OpenOptions::new().write(true).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).open(path) {
        let _ = g.set_times(times);
    }
    WriteResult::Ok
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// cache
// ---------------------------------------------------------------------------

use crate::transcribe::{transcript_path, transcript_text_path};

fn read_cache_marks(path: &str) -> (String, String) {
    let v: Value = serde_json::from_str(&player::load_meta_subset(path, &[KEY_REV, KEY_SIG])).unwrap_or(Value::Null);
    let get = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    (get(KEY_REV), get(KEY_SIG))
}

fn write_cache_mark(path: &str, key: &str, value: &str) {
    player::save_meta_subset(path, &json!({ key: value }).to_string());
}

fn build_block_json(path: &str, rev: &str) -> String {
    let mut root = Map::new();
    root.insert("app".into(), json!("NoOBS"));
    root.insert("v".into(), json!(BLOCK_VERSION));
    root.insert("rev".into(), json!(rev));
    if let Ok(meta @ Value::Object(_)) = serde_json::from_str::<Value>(&player::load_meta_subset(path, &PORTABLE_KEYS)) {
        root.insert("meta".into(), meta);
    }
    let tr = transcript_path(path);
    if let Ok(body) = std::fs::read_to_string(&tr) {
        if let Ok(v) = serde_json::from_str::<Value>(&body) {
            root.insert("transcript".into(), v);
            let text = std::fs::read_to_string(transcript_text_path(path)).unwrap_or_default();
            root.insert("transcriptText".into(), json!(text));
        }
    }
    Value::Object(root).to_string()
}

/// `(raiz, rev)` se o JSON é um bloco do NoOBS.
fn block_rev(json: &str) -> Option<(Map<String, Value>, String)> {
    let Value::Object(root) = serde_json::from_str::<Value>(json).ok()? else { return None };
    if root.get("app").and_then(Value::as_str) != Some("NoOBS") {
        return None;
    }
    let rev = root.get("rev").and_then(Value::as_str).unwrap_or("").to_owned();
    Some((root, rev))
}

// ---------------------------------------------------------------------------
// fila
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Queue {
    /// chave minúscula → quando vence
    due: HashMap<String, Instant>,
    /// chave minúscula → caminho
    paths: HashMap<String, String>,
    dirty: bool,
    stop: bool,
}

struct Shared {
    q: Mutex<Queue>,
    wake: Condvar,
}

struct Worker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
type OnImported = Box<dyn Fn(&str) + Send + Sync>;
static ON_IMPORTED: OnceLock<Mutex<Option<OnImported>>> = OnceLock::new();

fn on_imported_slot() -> &'static Mutex<Option<OnImported>> {
    ON_IMPORTED.get_or_init(|| Mutex::new(None))
}

fn shared() -> Option<Arc<Shared>> {
    WORKER.lock().ok()?.as_ref().map(|w| Arc::clone(&w.shared))
}

fn pending_path() -> PathBuf {
    let cache = player::cache_root_dir();
    cache.parent().map(Path::to_path_buf).unwrap_or(cache).join(PENDING_FILE)
}

fn enqueue(path: &str, delay_ms: u64) {
    let Some(sh) = shared() else { return };
    let k = path.to_lowercase();
    {
        let mut q = sh.q.lock().unwrap();
        q.due.insert(k.clone(), Instant::now() + Duration::from_millis(delay_ms));
        if !q.paths.contains_key(&k) {
            q.dirty = true;
        }
        q.paths.insert(k, path.to_owned());
    }
    sh.wake.notify_one();
}

fn drop_path(sh: &Shared, path: &str) {
    let k = path.to_lowercase();
    let mut q = sh.q.lock().unwrap();
    if q.paths.contains_key(&k) {
        q.dirty = true;
    }
    q.due.remove(&k);
    q.paths.remove(&k);
}

fn save_pending(sh: &Shared) {
    let list: Vec<String> = {
        let mut q = sh.q.lock().unwrap();
        q.dirty = false;
        q.paths.values().cloned().collect()
    };
    let body = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
    if let Err(e) = std::fs::write(pending_path(), body) {
        log!("FileMeta: falha ao gravar a lista de pendentes: {e}");
    }
}

fn load_pending() {
    let Ok(body) = std::fs::read_to_string(pending_path()) else { return };
    match serde_json::from_str::<Vec<Value>>(&body) {
        // Folga no arranque: o resto do app está subindo.
        Ok(list) => list.iter().filter_map(Value::as_str).for_each(|p| enqueue(p, STARTUP_GRACE_MS)),
        Err(e) => log!("FileMeta: lista de pendentes ilegivel: {e}"),
    }
}

/// Escreve no arquivo o que o cache tem, se o cache for mais novo.
fn sync_one(path: &str) -> WriteResult {
    if !Path::new(path).is_file() {
        return WriteResult::Gone;
    }
    if kind_of(path).is_empty() {
        return WriteResult::Nothing;
    }
    let (cache_rev, _) = read_cache_marks(path);
    if cache_rev.is_empty() {
        return WriteResult::Nothing; // cache sem nada a levar
    }
    if is_cloud_only(path) {
        return WriteResult::Cloud;
    }
    if let Some(json) = read_tail_block(path) {
        if block_rev(&json).map(|(_, r)| r >= cache_rev).unwrap_or(false) {
            write_cache_mark(path, KEY_SIG, &file_sig(path));
            return WriteResult::Nothing; // o arquivo já está em dia
        }
    }
    let r = write_tail_block(path, &build_block_json(path, &cache_rev));
    if r == WriteResult::Ok {
        write_cache_mark(path, KEY_SIG, &file_sig(path));
        log!("FileMeta: dados gravados no fim de \"{}\".", file_name(path));
    }
    r
}

fn worker_loop(sh: Arc<Shared>) {
    loop {
        {
            let q = sh.q.lock().unwrap();
            if q.stop {
                return;
            }
            let (q, _) = sh.wake.wait_timeout(q, Duration::from_millis(500)).unwrap();
            if q.stop {
                return;
            }
        }
        loop {
            // Pega o primeiro vencido e o TIRA do relógio enquanto processa: um
            // `mark_changed` no meio o recoloca, e ele roda de novo depois.
            let path = {
                let mut q = sh.q.lock().unwrap();
                if q.stop {
                    return;
                }
                let now = Instant::now();
                let key = q.due.iter().find(|(_, t)| **t <= now).map(|(k, _)| k.clone());
                match key {
                    Some(k) => {
                        q.due.remove(&k);
                        q.paths.get(&k).cloned()
                    }
                    None => None,
                }
            };
            let Some(path) = path else { break };
            let r = std::panic::catch_unwind(|| sync_one(&path)).unwrap_or_else(|_| {
                log!("FileMeta: erro em \"{}\": panic", file_name(&path));
                WriteResult::Error
            });
            // Recolocado durante o processamento (mudou de novo): fica na fila com o
            // horário novo, seja qual for o resultado desta volta.
            let requeued = sh.q.lock().unwrap().due.contains_key(&path.to_lowercase());
            if requeued {
                continue;
            }
            match r {
                WriteResult::Locked => enqueue(&path, RETRY_LOCKED_MS),
                WriteResult::Cloud => enqueue(&path, RETRY_CLOUD_MS),
                WriteResult::Error => enqueue(&path, RETRY_ERROR_MS),
                WriteResult::Unsafe => {
                    log!("FileMeta: \"{}\" termina num elemento incompleto (gravacao interrompida) — os dados ficam so no cache.", file_name(&path));
                    drop_path(&sh, &path);
                }
                _ => drop_path(&sh, &path),
            }
        }
        let dirty = sh.q.lock().unwrap().dirty;
        if dirty {
            save_pending(&sh);
        }
    }
}

// ---------------------------------------------------------------------------
// público
// ---------------------------------------------------------------------------

/// Sobe a worker e restaura os pendentes da sessão anterior.
pub fn start() {
    let mut w = WORKER.lock().unwrap();
    if w.is_some() {
        return;
    }
    let sh = Arc::new(Shared { q: Mutex::new(Queue::default()), wake: Condvar::new() });
    let sh2 = Arc::clone(&sh);
    let handle = std::thread::Builder::new().name("filemeta".into()).spawn(move || worker_loop(sh2)).ok();
    *w = Some(Worker { shared: sh, handle });
    drop(w);
    load_pending();
    if let Some(sh) = shared() {
        sh.q.lock().unwrap().dirty = false;
    }
}

/// Para a worker; o que estava pendente fica no arquivo da lista.
pub fn shutdown() {
    let w = WORKER.lock().unwrap().take();
    if let Some(mut w) = w {
        w.shared.q.lock().unwrap().stop = true;
        w.shared.wake.notify_all();
        if let Some(h) = w.handle.take() {
            let _ = h.join();
        }
        save_pending(&w.shared);
    }
    *on_imported_slot().lock().unwrap() = None;
}

/// O cache de `path` mudou: carimba a revisão e agenda a escrita no arquivo.
pub fn mark_changed(path: &str) {
    if path.is_empty() || kind_of(path).is_empty() {
        return;
    }
    write_cache_mark(path, KEY_REV, &now_rev());
    enqueue(path, DELAY_CHANGE_MS);
}

/// Chamado (na worker) quando um arquivo teve o cache populado pelo bloco.
pub fn set_on_imported(cb: impl Fn(&str) + Send + Sync + 'static) {
    *on_imported_slot().lock().unwrap() = Some(Box::new(cb));
}

/// Worker thread. Lê o bloco do fim do arquivo e, se for mais novo que o cache,
/// popula o cache (meta + transcrição). `true` se importou.
pub fn import_if_newer(path: &str) -> bool {
    if kind_of(path).is_empty() || !Path::new(path).is_file() || is_cloud_only(path) {
        return false;
    }
    let sig = file_sig(path);
    let (cache_rev, cache_sig) = read_cache_marks(path);
    if !sig.is_empty() && sig == cache_sig {
        return false; // nada mudou no arquivo
    }
    let block = read_tail_block(path).and_then(|j| block_rev(&j));
    let mut imported = false;
    match block {
        Some((root, rev)) if rev > cache_rev => {
            // Bloco mais novo: o arquivo veio de outra máquina (ou foi trocado por
            // uma cópia mais nova). Popula o cache.
            let mut upd = match root.get("meta") {
                Some(Value::Object(m)) => m.clone(),
                _ => Map::new(),
            };
            upd.insert(KEY_REV.into(), json!(rev));
            upd.insert(KEY_SIG.into(), json!(sig));
            player::save_meta_subset(path, &Value::Object(upd).to_string());
            if let Some(tr) = root.get("transcript") {
                let _ = std::fs::create_dir_all(player::cache_root_dir());
                let _ = std::fs::write(transcript_path(path), tr.to_string());
                let text = root.get("transcriptText").and_then(Value::as_str).unwrap_or("");
                let _ = std::fs::write(transcript_text_path(path), text);
            }
            log!("FileMeta: cache de \"{}\" preenchido pelos dados do arquivo (rev {rev}).", file_name(path));
            imported = true;
        }
        other => {
            // Cache mais novo que o arquivo (ou arquivo sem bloco com edição local):
            // o arquivo precisa ser atualizado.
            let rev = other.map(|(_, r)| r).unwrap_or_default();
            if !cache_rev.is_empty() && cache_rev > rev {
                enqueue(path, DELAY_CHANGE_MS);
            }
            write_cache_mark(path, KEY_SIG, &sig);
        }
    }
    if imported {
        if let Some(cb) = on_imported_slot().lock().unwrap().as_ref() {
            cb(path);
        }
    }
    imported
}

/// Arquivo (ou pasta, por prefixo) renomeado/movido: os pendentes seguem.
pub fn rename_path(old: &str, new: &str) {
    let Some(sh) = shared() else { return };
    let old_l = old.trim_end_matches(['\\', '/']).to_lowercase();
    let mut q = sh.q.lock().unwrap();
    let keys: Vec<String> = q.paths.keys().cloned().collect();
    for k in keys {
        let new_path = if k == old_l {
            new.to_owned()
        } else if k.starts_with(&format!("{old_l}\\")) {
            let orig = &q.paths[&k];
            // O resto vem do caminho ORIGINAL (caixa preservada), depois do prefixo.
            let rest: String = orig.chars().skip(old_l.chars().count() + 1).collect();
            format!("{}\\{rest}", new.trim_end_matches(['\\', '/']))
        } else {
            continue;
        };
        let due = q.due.get(&k).copied().unwrap_or_else(|| Instant::now() + Duration::from_millis(DELAY_CHANGE_MS));
        q.due.remove(&k);
        q.paths.remove(&k);
        let nk = new_path.to_lowercase();
        q.due.insert(nk.clone(), due);
        q.paths.insert(nk, new_path);
        q.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_confere_com_o_valor_conhecido() {
        // CRC-32/IEEE de "123456789".
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn envelope_mkv_ida_e_volta() {
        let json = r#"{"app":"NoOBS","rev":"2026-01-01T00:00:00.000Z"}"#;
        let env = build_envelope(json, "mkv");
        assert_eq!(env[0], 0xEC);
        assert_eq!(env[1], 0x01);
        let dir = std::env::temp_dir().join(format!("noobs-fm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.bin");
        let mut data = vec![7u8; 100];
        data.extend(&env);
        std::fs::write(&p, &data).unwrap();
        let mut f = open_read(p.to_str().unwrap()).unwrap();
        let (got, len) = read_tail_block_h(&mut f, data.len() as u64).unwrap();
        assert_eq!(got, json);
        assert_eq!(len as usize, env.len());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn envelope_mp4_usa_caixa_free() {
        let env = build_envelope("{}", "mp4");
        assert_eq!(&env[4..8], b"free");
        assert_eq!(be(&env, 0, 4) as usize, env.len());
    }

    #[test]
    fn parse_ebml_void_de_8_bytes() {
        let b = [0xEC, 0x01, 0, 0, 0, 0, 0, 0x01, 0x00];
        let (id, sz, unk, used) = parse_ebml(&b).unwrap();
        assert_eq!((id, sz, unk, used), (0xEC, 256, false, 9));
    }
}

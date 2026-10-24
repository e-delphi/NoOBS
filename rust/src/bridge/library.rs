//! Biblioteca de gravações no Bridge (porte do trecho de listagem/navegação,
//! metadados e player do `OBSBridge.pas`): cards da galeria, pastas, scan de
//! duração/miniatura em worker, `play_recording`, transcode sob demanda,
//! `request_video_info` (com cache), waveform, keyframes, faixas de áudio,
//! renomear, abrir e excluir (lixeira).
//!
//! Regras que seguem valendo:
//! * a pasta de gravação (`recordDir`) é a RAIZ; subpastas viram cards antes
//!   dos grupos de data. `current_browse_dir` REVALIDA a cada leitura e volta
//!   à raiz se a pasta navegada sumiu (pegada de "lista vazia pra sempre");
//! * todo path vindo da UI passa por `is_path_in_record_dir`/`is_dir_in_record_dir`
//!   antes de virar operação de disco — defesa contra mensagem forjada;
//! * o cache é indexado por hash do PATH (pegadinha #55): o GC olha a ÁRVORE
//!   inteira (`list_recordings_recursive`), a listagem da tela só a pasta
//!   navegada. São coisas diferentes, não unifique.

use std::cell::RefCell;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local, Timelike};
use serde_json::{json, Value};

use crate::view::{self, Crumb, FolderItem, LayoutRegion, Library, RecItem, StreamInfo, VideoInfo, VideoLayout};
use crate::{config, ffmpeg_ops, lang, log, mainthread, player, probe, record_watch, ui};

use super::recording::{last_recording_path, record_dir, recording_active};
use super::{is_shutting_down, post_error};

/// Extensões de vídeo reconhecidas como gravação. Fonte única da listagem e da
/// whitelist de `open_recording` (não deixar a UI mandar `ShellExecute('open')`
/// num `.exe`). `.mp3` entrou pela exportação só de áudio.
pub const RECORDING_EXTS: [&str; 8] = [".mkv", ".mp4", ".mov", ".m4v", ".ts", ".flv", ".webm", ".mp3"];

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
/// Placeholder "online-only" do OneDrive (Files On-Demand).
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

thread_local! {
    /// Pasta que a lista está mostrando; '' = raiz. NÃO persiste: a biblioteca
    /// sempre abre na raiz, único lugar onde gravação nova cai. Sempre um caminho
    /// DENTRO de `recordDir` — `current_browse_dir` revalida a cada leitura.
    static BROWSE_DIR: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn set_browse_dir(dir: &str) {
    BROWSE_DIR.with(|b| *b.borrow_mut() = dir.to_owned());
}

// ---------------------------------------------------------------------------
// Formatação e utilidades de caminho
// ---------------------------------------------------------------------------

/// `812 KB | 45 MB | 1,5 GB` — tamanho estático e compacto dos cards.
pub fn format_bytes_short(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;
    if bytes >= GB {
        crate::win::localize_decimal(&format!("{:.1} GB", bytes as f64 / GB as f64))
    } else if bytes >= MB {
        format!("{} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
    }
}

pub fn is_recording_ext(path: &str) -> bool {
    let ext = Path::new(path).extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase())).unwrap_or_default();
    RECORDING_EXTS.contains(&ext.as_str())
}

fn full_path(p: &str) -> Option<PathBuf> {
    std::path::absolute(p).ok()
}

fn lower_with_sep(p: &Path) -> String {
    let mut s = p.to_string_lossy().to_lowercase();
    if !s.ends_with('\\') {
        s.push('\\');
    }
    s
}

/// Só operamos em arquivos DENTRO da pasta de gravação. Toda gravação legítima
/// vem da listagem, então o path da UI sempre cai aqui; bloqueia mensagens
/// forjadas (XSS/navegação indevida) de mirar arquivos arbitrários.
pub fn is_path_in_record_dir(path: &str) -> bool {
    let root = record_dir();
    if path.is_empty() || root.as_os_str().is_empty() {
        return false;
    }
    let (Some(base), Some(full)) = (full_path(&root.to_string_lossy()), full_path(path)) else { return false };
    // Base com barra final: "C:\Vids\" não casa com o vizinho "C:\VidsOutro\rec.mkv".
    full.to_string_lossy().to_lowercase().starts_with(&lower_with_sep(&base))
}

/// Igual ao anterior, mas para PASTA e aceitando a própria raiz (destino
/// legítimo de navegação e de "colar aqui").
pub fn is_dir_in_record_dir(dir: &str) -> bool {
    let root = record_dir();
    if dir.is_empty() || root.as_os_str().is_empty() {
        return false;
    }
    let (Some(base), Some(full)) = (full_path(&root.to_string_lossy()), full_path(dir)) else { return false };
    lower_with_sep(&full).starts_with(&lower_with_sep(&base))
}

/// Pasta que a UI está listando. SEMPRE válida: se a navegada sumiu (Explorer,
/// drive removido, `recordDir` trocado), zera o estado e devolve a raiz.
pub fn current_browse_dir() -> String {
    let root = record_dir().to_string_lossy().into_owned();
    let cur = BROWSE_DIR.with(|b| b.borrow().clone());
    if cur.is_empty() || root.is_empty() {
        return root;
    }
    if is_dir_in_record_dir(&cur) && Path::new(&cur).is_dir() {
        cur
    } else {
        set_browse_dir("");
        root
    }
}

fn trim_sep(s: &str) -> &str {
    s.trim_end_matches(['\\', '/'])
}

pub fn is_browse_root() -> bool {
    trim_sep(&current_browse_dir()).eq_ignore_ascii_case(trim_sep(&record_dir().to_string_lossy()))
}

/// Pasta acima da navegada; '' na raiz. Nunca sobe da raiz.
pub fn browse_parent_dir() -> String {
    if is_browse_root() {
        return String::new();
    }
    let cur = current_browse_dir();
    let parent = Path::new(trim_sep(&cur)).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    if is_dir_in_record_dir(&parent) {
        parent
    } else {
        trim_sep(&record_dir().to_string_lossy()).to_owned()
    }
}

// ---------------------------------------------------------------------------
// Listagens
// ---------------------------------------------------------------------------

/// Arquivos de vídeo da pasta (não recursivo).
pub fn list_recordings(dir: &str) -> Vec<String> {
    if dir.is_empty() || !Path::new(dir).is_dir() {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.path().to_string_lossy().into_owned())
        .filter(|p| is_recording_ext(p))
        .collect()
}

/// Igual, descendo nas subpastas. Usado pelo GC do cache: o cache é por hash do
/// path COMPLETO, então passar só o topo apagaria thumb/duração de tudo que o
/// usuário organizou em pastas (pegadinha #55).
pub fn list_recordings_recursive(dir: &str) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if is_recording_ext(&p.to_string_lossy()) {
                out.push(p.to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    if !dir.is_empty() && Path::new(dir).is_dir() {
        walk(Path::new(dir), &mut out);
    }
    out
}

/// Subpastas diretas. Oculta/sistema ficam de fora: são pastas do Windows (ex.:
/// "System Volume Information" quando a raiz é um drive), não organização do
/// usuário. Atributo inválido (a pasta sumiu entre a listagem e a leitura):
/// mostra — some sozinha no próximo refresh, e esconder seria pior.
pub fn list_sub_folders(dir: &str) -> Vec<String> {
    if dir.is_empty() || !Path::new(dir).is_dir() {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            e.metadata().map(|m| m.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) == 0).unwrap_or(true)
        })
        .map(|e| trim_sep(&e.path().to_string_lossy()).to_owned())
        .collect()
}

/// Quantas gravações a pasta tem dentro (recursivo) e quanto ocupam. Vai junto da
/// listagem: o aviso de exclusão precisa do número sem round-trip.
pub fn folder_stats(dir: &str) -> (usize, i64) {
    let files = list_recordings_recursive(dir);
    let size = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len() as i64).sum();
    (files.len(), size)
}

// ---- pastas ocultas (config `hiddenFolders`) -----------------------------------
//
// Subpastas DIRETAS da pasta de gravação que o usuário não quer ver (as que o
// Windows — Captures —, a NVIDIA e a AMD criam em Vídeos). Guardadas pelo NOME,
// em minúsculas: trocar a pasta de gravação por outra com as mesmas pastas
// continua escondendo. Ocultar NÃO apaga nem move nada.

pub fn hidden_folder_names() -> Vec<String> {
    match config::get_config_json("hiddenFolders") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_lowercase).collect(),
        _ => Vec::new(),
    }
}

pub fn is_hidden_folder_name(name: &str, hidden: &[String]) -> bool {
    hidden.iter().any(|h| *h == name.to_lowercase())
}

/// A gravação mora dentro de uma pasta oculta? Decide pelo PRIMEIRO nível abaixo
/// da pasta de gravação (as ocultas são subpastas diretas dela).
pub fn is_in_hidden_folder(path: &str, hidden: &[String]) -> bool {
    if hidden.is_empty() {
        return false;
    }
    let mut root = record_dir().to_string_lossy().into_owned();
    if !root.ends_with('\\') {
        root.push('\\');
    }
    let lower = path.to_lowercase();
    if !lower.starts_with(&root.to_lowercase()) {
        return false;
    }
    let rel = &path[root.len()..];
    match rel.find('\\') {
        None => false, // arquivo solto na raiz
        Some(p) => is_hidden_folder_name(&rel[..p], hidden),
    }
}

/// Placeholder só-na-nuvem: abrir forçaria download. `GetFileAttributes` lê só
/// metadado (sempre local) e NÃO dispara o recall.
pub fn is_file_cloud_only(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    std::fs::metadata(path).map(|m| m.file_attributes() & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS != 0).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// JSON dos cards
// ---------------------------------------------------------------------------

fn iso_local(t: SystemTime) -> String {
    DateTime::<Local>::from(t).format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn file_stem(p: &str) -> String {
    Path::new(p).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Um card da galeria, só com o que o cache já sabe (instantâneo, sem libav).
/// Os selos de COMO foi gravada só vêm se conhecidos: gravação anterior a
/// esses campos não tem nenhum e a interface não desenha selo.
fn build_rec_item(path: &str, mtime: SystemTime, size: i64, duration: i32) -> RecItem {
    let (dur, thumb, meta) = player::get_cached_meta(path);
    RecItem {
        id: path.to_owned(),
        name: file_stem(path),
        date: iso_local(mtime),
        size,
        size_text: format_bytes_short(size),
        duration: if duration > 0 { duration } else { dur.max(0) },
        thumb,
        codec: meta.codec.clone(),
        codec_hw: meta.codec_hw,
        fps: meta.fps,
        quality: meta.quality_level,
        // Só aparece quando JÁ foi transcrita.
        transcript: crate::transcribe::transcript_state(path).to_owned(),
    }
}

fn build_recordings() -> Vec<RecItem> {
    // Lista a pasta NAVEGADA, não a raiz (na raiz as duas são a mesma coisa).
    let dir = current_browse_dir();
    if dir.is_empty() || !Path::new(&dir).is_dir() {
        return Vec::new();
    }
    let recording = recording_active();
    let last = last_recording_path();
    let mut items = Vec::new();
    for path in list_recordings(&dir) {
        // Esconde o arquivo da gravação em andamento (tamanho parcial até o muxer
        // finalizar); `push_recording_added` o põe na lista quando termina.
        if recording && !last.is_empty() && path.eq_ignore_ascii_case(&last) {
            continue;
        }
        let Ok(md) = std::fs::metadata(&path) else { continue };
        let Ok(mtime) = md.modified() else { continue };
        items.push(build_rec_item(&path, mtime, md.len() as i64, 0));
    }
    items
}

/// Subpastas da pasta navegada, cada uma com quantas gravações guarda
/// (recursivo) e quanto ocupam. Na raiz, as pastas ocultas ficam de fora.
fn build_folders() -> Vec<FolderItem> {
    let hidden = if is_browse_root() { hidden_folder_names() } else { Vec::new() };
    list_sub_folders(&current_browse_dir())
        .into_iter()
        .filter(|d| !is_hidden_folder_name(&file_name(d), &hidden))
        .map(|d| {
            let (cnt, sz) = folder_stats(&d);
            FolderItem { name: file_name(&d), count: cnt as i64, size: sz, size_text: format_bytes_short(sz), id: d }
        })
        .collect()
}

/// Caminho da raiz até a pasta navegada, um item por nível. O primeiro é sempre
/// a pasta de gravação (a interface desenha como "início").
fn build_breadcrumb() -> Vec<Crumb> {
    let root = record_dir().to_string_lossy().into_owned();
    let root = trim_sep(&root).to_owned();
    let mut items = vec![Crumb { id: root.clone(), name: file_name(&root), root: true }];
    if !is_browse_root() {
        let cur = current_browse_dir();
        let cur = trim_sep(&cur);
        let rel = cur.get(root.len() + 1..).unwrap_or("");
        let mut acc = root.clone();
        for part in rel.split('\\').filter(|p| !p.is_empty()) {
            acc = format!("{acc}\\{part}");
            items.push(Crumb { id: acc.clone(), name: part.to_owned(), root: false });
        }
    }
    items
}

/// A biblioteca inteira da pasta navegada: lista e navegação no MESMO push,
/// pra nunca divergirem.
pub fn build_library() -> Library {
    Library {
        recordings: build_recordings(),
        folders: build_folders(),
        current_dir: trim_sep(&current_browse_dir()).to_owned(),
        parent_dir: browse_parent_dir(),
        at_root: is_browse_root(),
        breadcrumb: build_breadcrumb(),
        record_dir: record_dir().to_string_lossy().into_owned(),
        free_bytes: super::devices::record_dir_free_bytes(),
    }
}

pub fn push_recordings() {
    let lib = build_library();
    view::call(move |v| v.library(&lib));
}

pub fn push_recording_meta(path: &str, duration: i32, thumb: &str) {
    let (path, thumb) = (path.to_owned(), thumb.to_owned());
    view::call(move |v| v.recording_meta(&path, duration, &thumb));
}

/// Um card novo (fim da gravação, exportação, união). Só entra se o arquivo
/// pertence à pasta que a interface mostra: gravação nova sempre cai na RAIZ, e
/// empurrar o card enquanto o usuário navega numa subpasta o poria visualmente
/// onde ele não está.
pub fn push_recording_added(path: &str, duration_sec: i32) {
    log!("PushRecordingAdded: path=\"{path}\" duration={duration_sec}");
    if !Path::new(path).is_file() {
        log!("PushRecordingAdded: arquivo NAO existe (cancelado).");
        return;
    }
    let dir_of = |p: &str| Path::new(p).parent().map(|d| trim_sep(&d.to_string_lossy()).to_lowercase()).unwrap_or_default();
    if dir_of(path) != trim_sep(&current_browse_dir()).to_lowercase() {
        log!("PushRecordingAdded: fora da pasta navegada — card nao adicionado.");
        // A meta (thumb/duração) ainda é gerada: o card aparece pronto ao navegar até lá.
        scan_single_recording_meta(path);
        return;
    }
    // O arquivo já está COMPLETO (o sinal "stop" garante o trailer): sem Sleep.
    let Ok(md) = std::fs::metadata(path) else {
        log!("PushRecordingAdded: erro lendo arquivo");
        return;
    };
    let size = md.len() as i64;
    log!("PushRecordingAdded: size={size} bytes");
    // O SaveRecordingMeta já rodou (o stop chama antes deste push): os selos já
    // estão no <hash>.json.
    let item = build_rec_item(path, md.modified().unwrap_or(SystemTime::now()), size, duration_sec);
    // Gravação nova consumiu disco: atualiza o "X GB livre" e o aviso (<5 GB).
    let free = super::devices::record_dir_free_bytes();
    view::call(move |v| v.recording_added(&item, free));
    // Gera a miniatura em background; chega depois via recording_meta.
    scan_single_recording_meta(path);
}
// ---------------------------------------------------------------------------
// Scan de metadados (duração + miniatura) em worker
// ---------------------------------------------------------------------------

fn process_single_meta(path: &str) {
    let r = std::panic::catch_unwind(|| player::ensure_recording_meta(path));
    match r {
        Ok(Some((dur, thumb))) if dur > 0 || !thumb.is_empty() => {
            let path = path.to_owned();
            mainthread::queue(move || push_recording_meta(&path, dur, &thumb));
        }
        Ok(_) => {}
        Err(_) => log!("ProcessSingleMeta: panic em {}", file_name(path)),
    }
}

/// Um arquivo só (fim de uma gravação). Cada chamada move seu próprio `String`
/// para a thread: não há captura compartilhada de variável de laço (#6).
pub fn scan_single_recording_meta(path: &str) {
    if is_shutting_down() || !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        return;
    }
    let path = path.to_owned();
    std::thread::spawn(move || {
        if !is_shutting_down() {
            process_single_meta(&path);
        }
    });
}

/// Versões antigas guardavam cache em `<rec-dir>\.cache`; agora vai para
/// `%LOCALAPPDATA%\NoOBS\cache`. Remove a pasta legada.
fn cleanup_legacy_cache() {
    let legacy = record_dir().join(".cache");
    if legacy.is_dir() {
        match std::fs::remove_dir_all(&legacy) {
            Ok(()) => log!("Cache legado removido: {}", legacy.display()),
            Err(e) => log!("   . falha removendo cache legado: {e}"),
        }
    }
}

/// Em worker: para cada gravação da pasta visível garante duração + miniatura
/// (cacheado) e empurra `recording_meta` por arquivo; faz o GC do cache. A
/// prévia só vale para a pasta VISÍVEL — navegar dispara o scan de novo.
pub fn scan_recordings_meta() {
    let root = record_dir().to_string_lossy().into_owned();
    if root.is_empty() || !Path::new(&root).is_dir() || !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        return;
    }
    let mut files = list_recordings(&current_browse_dir());
    // O GC olha a árvore INTEIRA: gravação guardada numa subpasta segue viva.
    let live = list_recordings_recursive(&root);
    std::thread::spawn(move || {
        if is_shutting_down() {
            return;
        }
        cleanup_legacy_cache();
        // GC primeiro, para liberar espaço antes de gerar caches novos.
        player::garbage_collect_cache(&live);

        // Mais recentes primeiro: são os que o usuário mais provavelmente vai ver.
        let mtime = |p: &String| std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        files.sort_by_key(|p| std::cmp::Reverse(mtime(p)));

        // Modo de prévia da biblioteca (`libraryThumbs`, padrão auto):
        //   always — todos · off — só as gravações feitas no NoOBS · auto — só os
        //   disponíveis LOCALMENTE (pula os só-na-nuvem, p/ não baixar a biblioteca).
        let mode = config::get_config_str("libraryThumbs", "auto").to_lowercase();
        let last = last_recording_path();
        for f in &files {
            if is_shutting_down() {
                return;
            }
            // Pula a gravação em andamento: o muxer ainda não escreveu o trailer.
            if recording_active() && !last.is_empty() && f.eq_ignore_ascii_case(&last) {
                continue;
            }
            if mode == "off" {
                continue;
            }
            if mode != "always" && is_file_cloud_only(f) {
                log!("ScanMeta: pulando previa (so na nuvem): {}", file_name(f));
                continue;
            }
            process_single_meta(f);
        }
    });
}

/// Callback do watcher: o Explorer (ou outro app) mexeu na pasta.
pub fn on_record_dir_changed() {
    push_recordings();
    scan_recordings_meta();
}

/// Sobe o watcher da pasta navegada.
pub fn start_record_watch() {
    record_watch::start(&current_browse_dir(), std::sync::Arc::new(on_record_dir_changed));
}

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

/// Segundos desde a meia-noite (0..86399) da hora em que a gravação COMEÇOU, para
/// o player exibir a hora-do-relógio da posição atual. -1 = indeterminado.
/// 1. Nome `NoOBS_yyyy-mm-dd_hh-nn-ss` (instante exato; sobrevive à cópia, não ao
///    rename). 2. Data de CRIAÇÃO do arquivo (o muxer o cria no início).
pub fn resolve_recording_start_clock_sec(path: &str) -> i32 {
    let name = file_stem(path);
    if name.chars().count() >= 25 && name.to_lowercase().starts_with("noobs_") {
        let n: Vec<char> = name.chars().collect();
        let num = |a: usize, len: usize| n[a - 1..a - 1 + len].iter().collect::<String>().parse::<i32>().ok();
        if let (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) =
            (num(7, 4), num(12, 2), num(15, 2), num(18, 2), num(21, 2), num(24, 2))
        {
            if y >= 2000 && (1..=12).contains(&mo) && (1..=31).contains(&d) && (0..=23).contains(&h) && (0..=59).contains(&mi) && (0..=59).contains(&s) {
                return h * 3600 + mi * 60 + s;
            }
        }
    }
    match std::fs::metadata(path).and_then(|m| m.created()) {
        Ok(t) => {
            let dt = DateTime::<Local>::from(t);
            (dt.hour() * 3600 + dt.minute() * 60 + dt.second()) as i32
        }
        Err(_) => -1,
    }
}

/// Valida o path da UI: dentro da pasta de gravação (ou uma prévia de
/// exportação — só LEITURA passa por aqui) e existente (senão avisa).
pub fn checked_recording(path: &str, warn_missing: bool) -> bool {
    if path.is_empty() || !(is_path_in_record_dir(path) || super::export::is_preview_path(path)) {
        return false;
    }
    if !Path::new(path).is_file() {
        if warn_missing {
            post_error(lang::t("error.fileNotFound"));
        }
        return false;
    }
    true
}

fn video_info_has_rateless_video(v: &Value) -> bool {
    v.get("streams").and_then(Value::as_array).is_some_and(|a| {
        a.iter().any(|s| {
            s.get("kind").and_then(Value::as_str) == Some("video")
                && s.get("frameRate").and_then(Value::as_f64).is_some_and(|r| r <= 0.0)
        })
    })
}

/// Probe via libavformat em worker (10–50 ms, mas pode picar em arquivo grande).
/// O resultado fica cacheado no `<hash>.json` (`videoInfo`) e é reusado.
pub fn handle_request_video_info(path: &str) {
    if !checked_recording(path, true) {
        return;
    }
    if !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        post_error(lang::t("error.mediaLibUnavailable"));
        return;
    }
    let path = path.to_owned();
    std::thread::spawn(move || {
        if is_shutting_down() {
            return;
        }
        // Cache hit? Valida pelo tamanho do arquivo (gravação é imutável, mas é
        // defensivo). Vídeo com frameRate 0 no cache é de antes de o Probe medir a
        // taxa de quem não a declara (MKV de uma união): reprobe uma vez.
        let cached = player::load_meta_sub_object_json(&path, "videoInfo");
        if !cached.is_empty() {
            if let Ok(c) = serde_json::from_str::<Value>(&cached) {
                let cur = std::fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(-1);
                let sz = c.get("size").and_then(Value::as_i64);
                if sz == Some(cur) && cur > 0 && !video_info_has_rateless_video(&c) {
                    if let Ok(mut info) = serde_json::from_value::<VideoInfo>(c) {
                        // O cache guarda id/nome de quando foi criado; depois de um
                        // rename o conteúdo é o mesmo, o caminho não.
                        info.id = path.clone();
                        info.file_name = file_name(&path);
                        view::call(move |v| v.video_info(&info));
                        return;
                    }
                }
            }
        }

        let report = std::panic::catch_unwind(|| probe::probe(Path::new(&path))).ok().flatten();
        if is_shutting_down() {
            return;
        }
        let Some(report) = report else {
            mainthread::queue(|| post_error(lang::t("error.probeFailed")));
            return;
        };

        let streams = report
            .streams
            .iter()
            .map(|s| {
                let video = s.kind == "video";
                let audio = s.kind == "audio";
                StreamInfo {
                    index: s.index as i64,
                    kind: s.kind.to_owned(),
                    codec: s.codec.clone(),
                    title: s.title.clone(),
                    bitrate: s.bit_rate,
                    duration: s.duration,
                    width: video.then_some(s.width as i64),
                    height: video.then_some(s.height as i64),
                    // FPS: 0 quando desconhecido (a interface esconde a linha).
                    frame_rate: video.then_some(s.frame_rate),
                    channels: audio.then_some(s.channels as i64),
                    sample_rate: audio.then_some(s.sample_rate as i64),
                }
            })
            .collect();
        // Layout (canvas + regiões) do <hash>.json: a "Visualização" do painel.
        // Gravação antiga sem .json cai em "Tela cheia".
        let layout = player::load_recording_meta(&path).filter(|m| !m.layout.regions.is_empty()).map(|m| VideoLayout {
            canvas_w: m.layout.canvas_w as i64,
            canvas_h: m.layout.canvas_h as i64,
            regions: m
                .layout
                .regions
                .iter()
                .map(|r| LayoutRegion { name: r.name.clone(), kind: r.kind.clone(), x: r.x as i64, y: r.y as i64, w: r.w as i64, h: r.h as i64 })
                .collect(),
        });
        let info = VideoInfo {
            id: path.clone(),
            file_name: file_name(&path),
            format: report.format.clone(),
            duration: report.duration,
            bitrate: report.bit_rate,
            size: report.size as i64,
            streams,
            layout,
        };
        // Cacheado pra acelerar reaberturas. Com o `type` da mensagem antiga: a
        // versão web, que divide o cache, manda o que está lá direto pra tela.
        if let Ok(mut js) = serde_json::to_value(&info) {
            js["type"] = json!("video_info");
            player::save_meta_sub_object_json(&path, "videoInfo", &js.to_string());
        }
        view::call(move |v| v.video_info(&info));
    });
}

/// Picos da 1ª faixa de áudio via libav (worker) para a waveform. Cacheado no
/// `<hash>.json` (`waveform`; `waveformHi` na linha do tempo da exportação, que
/// precisa de muito mais resolução — numa chave só player e exportação se
/// revezariam invalidando o cache um do outro).
pub fn handle_request_waveform(path: &str, buckets: i64, hi: bool) {
    if !checked_recording(path, false) {
        return;
    }
    // Clamp dos dois lados: < 1 → 100; teto para uma mensagem forjada não pedir
    // bilhões de buckets (OOM).
    let buckets: usize = if buckets <= 0 {
        100
    } else if !hi && buckets > 20_000 {
        20_000
    } else if buckets > 150_000 {
        150_000
    } else {
        buckets as usize
    };
    let key = if hi { "waveformHi" } else { "waveform" };
    let path = path.to_owned();
    std::thread::spawn(move || {
        if is_shutting_down() {
            return;
        }
        // Cache hit só se o número de buckets bate com o pedido.
        let cached = player::load_meta_sub_object_json(&path, key);
        if !cached.is_empty() {
            if let Ok(c @ Value::Object(_)) = serde_json::from_str::<Value>(&cached) {
                if c.get("buckets").and_then(Value::as_u64) == Some(buckets as u64) {
                    // Os picos são do conteúdo: valem mesmo depois de um rename (o id
                    // que vai é o caminho de agora).
                    let peaks: Vec<f32> = c["peaks"].as_array().map(|a| a.iter().filter_map(Value::as_f64).map(|v| v as f32).collect()).unwrap_or_default();
                    view::call(move |v| v.waveform(&path, &peaks, hi));
                    return;
                }
            }
        }
        let peaks = std::panic::catch_unwind(|| ffmpeg_ops::compute_audio_peaks(&path, buckets, None)).ok().flatten();
        if is_shutting_down() {
            return;
        }
        let Some(peaks) = peaks else {
            log!("Waveform: ComputeAudioPeaks falhou pra \"{path}\"");
            return;
        };
        // Arredondado a 3 casas: a alta resolução são até 150 mil números.
        let arr: Vec<Value> = peaks.iter().map(|p| json!((*p as f64 * 1000.0).round() / 1000.0)).collect();
        let obj = json!({ "type": "waveform_ready", "id": path, "buckets": buckets, "hi": hi, "peaks": arr });
        player::save_meta_sub_object_json(&path, key, &obj.to_string());
        view::call(move |v| v.waveform(&path, &peaks, hi));
    });
}

// ---------------------------------------------------------------------------
// Renomear, abrir, excluir
// ---------------------------------------------------------------------------

/// Troca por `_` os caracteres que o Windows não aceita em nome de arquivo.
pub fn sanitize_file_name(name: &str) -> String {
    let mut s = name.trim().to_owned();
    for ch in ['\\', '/', ':', '*', '?', '"', '<', '>', '|'] {
        s = s.replace(ch, "_");
    }
    s
}

pub fn handle_rename_recording(old_path: &str, new_name: &str) {
    if old_path.is_empty() || new_name.is_empty() {
        return;
    }
    if super::export::export_in_use(old_path) {
        post_error(lang::t("error.exportBusyFile"));
        return;
    }
    if !is_path_in_record_dir(old_path) {
        log!("HandleRenameRecording: path fora da pasta de gravacao, ignorado: {old_path}");
        return;
    }
    if !Path::new(old_path).is_file() {
        post_error(lang::t("error.fileNotFound"));
        return;
    }
    let sanitized = sanitize_file_name(new_name);
    if sanitized.is_empty() {
        return;
    }
    let old = Path::new(old_path);
    let dir = old.parent().unwrap_or(Path::new(""));
    let ext = old.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();

    let mut new_path = dir.join(format!("{sanitized}{ext}"));
    let mut suffix = 2;
    while new_path.exists() && !new_path.to_string_lossy().eq_ignore_ascii_case(old_path) {
        new_path = dir.join(format!("{sanitized} ({suffix}){ext}"));
        suffix += 1;
    }
    let new_path_s = new_path.to_string_lossy().into_owned();
    if new_path_s.eq_ignore_ascii_case(old_path) {
        return; // sem mudança
    }
    if std::fs::rename(old_path, &new_path).is_err() {
        post_error(lang::t("error.renameFailed"));
        return;
    }
    // Migra o cache (thumb/duração/mp4/json/faixas) do hash antigo para o novo —
    // senão fica órfão até o próximo GC e a gravação regenera tudo do zero.
    player::rename_cache_entries(old_path, &new_path_s);
    let (old_id, name) = (old_path.to_owned(), file_stem(&new_path_s));
    view::call(move |v| v.recording_renamed(&old_id, &new_path_s, &name));
}

pub fn handle_open_record_dir() {
    let dir = record_dir();
    if dir.as_os_str().is_empty() || !dir.is_dir() {
        post_error(lang::t("error.folderNotFound"));
        return;
    }
    ui::shell_open(&dir.to_string_lossy());
}

/// Manda para a Lixeira (recuperável), com retry curto: logo após uma gravação a
/// worker de miniatura abre o arquivo com FILE_SHARE_READ e bloqueia o DELETE —
/// o `SHFileOperation` falha por sharing violation. A miniatura leva ~200–500 ms;
/// 1,5 s de retry cobre quase todos os casos sem travar a UI.
pub fn delete_to_recycle_bin(path: &str) -> bool {
    use windows::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW};
    const MAX_RETRIES: u32 = 6;
    const RETRY_DELAY_MS: u64 = 250;
    // Teste: apaga de verdade (na pasta temporária), sem encher a Lixeira.
    #[cfg(test)]
    {
        crate::testkit::record_effect(format!("recycle:{path}"));
        let p = Path::new(path);
        return if p.is_dir() { std::fs::remove_dir_all(p).is_ok() } else { std::fs::remove_file(p).is_ok() };
    }
    // pFrom exige path duplo-NUL-terminado.
    #[allow(unreachable_code)]
    let mut buf: Vec<u16> = path.encode_utf16().collect();
    buf.extend([0, 0]);
    let mut rc = 0;
    for attempt in 0..MAX_RETRIES {
        let mut op = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: windows::core::PCWSTR(buf.as_ptr()),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
            ..Default::default()
        };
        rc = unsafe { SHFileOperationW(&mut op) };
        if rc == 0 {
            if attempt > 0 {
                log!("DeleteToRecycleBin: ok apos {attempt} retries ({}ms).", attempt as u64 * RETRY_DELAY_MS);
            }
            return true;
        }
        if attempt == 0 {
            log!("DeleteToRecycleBin: 1a tentativa falhou (rc={rc}) — retrying.");
        }
        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
    }
    log!("DeleteToRecycleBin: desistiu apos {MAX_RETRIES} tentativas (rc={rc}).");
    false
}

/// Assíncrono para não travar a UI: o delete pode bloquear até ~1,5 s (retry de
/// sharing violation) e um bulk-delete síncrono congelaria a janela. A UI já
/// removeu o card otimisticamente; o `recording_removed` (+ GC + freeBytes) volta
/// pela fila da main.
pub fn handle_delete_recording(path: &str) {
    if path.is_empty() {
        return;
    }
    if super::export::export_in_use(path) {
        post_error(lang::t("error.exportBusyFile"));
        return;
    }
    if !is_path_in_record_dir(path) {
        log!("HandleDeleteRecording: path fora da pasta de gravacao, ignorado: {path}");
        return;
    }
    if !Path::new(path).is_file() {
        // Pode ser race: outro delete (ou GC, ou shell) já removeu. Sucesso silencioso.
        log!("HandleDeleteRecording: arquivo ja nao existe: {path}");
        return;
    }
    let path = path.to_owned();
    std::thread::spawn(move || {
        let ok = delete_to_recycle_bin(&path);
        mainthread::queue(move || {
            if is_shutting_down() {
                return;
            }
            if !ok {
                post_error(lang::t_with("error.deleteFailed", &[("file", &file_name(&path))]));
                return;
            }
            // Limpa o cache desse arquivo agora (o GC pegaria no próximo start).
            player::garbage_collect_cache(&list_recordings_recursive(&record_dir().to_string_lossy()));
            // A transcrição é por gravação: sem isto ficaria órfã no cache e ainda
            // apareceria na busca por texto; esperando na fila, sai junto, sem aviso.
            super::transcribe::forget_recording(&path);
            // O delete liberou espaço: refresh do indicador na interface.
            let free = super::devices::record_dir_free_bytes();
            view::call(move |v| v.recording_removed(&path, free));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensoes_de_gravacao() {
        assert!(is_recording_ext("C:\\v\\a.MKV"));
        assert!(is_recording_ext("a.mp3"));
        assert!(!is_recording_ext("a.exe"));
        assert!(!is_recording_ext("semextensao"));
    }

    #[test]
    fn nome_seguro_de_arquivo() {
        assert_eq!(sanitize_file_name("  a:b/c*d?  "), "a_b_c_d_");
        assert_eq!(sanitize_file_name("ok"), "ok");
    }

    #[test]
    fn pastas_ocultas_pelo_primeiro_nivel() {
        // is_in_hidden_folder depende do recordDir do config; testa só o nome.
        let hidden = vec!["captures".to_owned()];
        assert!(is_hidden_folder_name("Captures", &hidden));
        assert!(!is_hidden_folder_name("Jogos", &hidden));
    }

    #[test]
    fn hora_do_relogio_pelo_nome() {
        assert_eq!(resolve_recording_start_clock_sec("C:\\v\\NoOBS_2026-03-07_09-05-02.mkv"), 9 * 3600 + 5 * 60 + 2);
    }

    #[test]
    fn taxa_zero_forca_reprobe() {
        let v = json!({ "streams": [ { "kind": "video", "frameRate": 0 }, { "kind": "audio" } ] });
        assert!(video_info_has_rateless_video(&v));
        let ok = json!({ "streams": [ { "kind": "video", "frameRate": 60.0 } ] });
        assert!(!video_info_has_rateless_video(&ok));
    }
}

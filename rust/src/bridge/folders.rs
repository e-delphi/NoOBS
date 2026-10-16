//! Biblioteca em pastas e união de gravações (partes do `OBSBridge.pas`).
//!
//! A pasta de gravação é a raiz; subpastas viram cards e o usuário organiza
//! arrastando ou por recortar/colar (os dois mandam `move_items`). Todo caminho
//! de pasta vindo da UI passa por `is_dir_in_record_dir` antes de virar
//! operação de disco.
//!
//! O cache (miniatura, duração, MP4, faixas) é indexado por hash do CAMINHO:
//! mover ou renomear sem migrar faria o próximo GC apagar tudo e o scan
//! regerar do zero (pegadinha #55).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::ffi::ffmpeg::ffmpeg_lib_available;
use crate::types::RecordingLayout;
use crate::{ffmpeg_ops, file_meta, lang, log, mainthread, player, record_watch};

use super::library::{
    current_browse_dir, delete_to_recycle_bin, format_bytes_short, is_dir_in_record_dir, is_path_in_record_dir,
    is_recording_ext, list_recordings_recursive, push_recording_added, push_recordings, sanitize_file_name,
    scan_recordings_meta, set_browse_dir,
};
use super::recording::{last_recording_path, record_dir, recording_active};
use super::{devices, export, is_shutting_down, post_error, post_json};

fn trim_sep(s: &str) -> String {
    s.trim_end_matches(['\\', '/']).to_owned()
}

fn record_root() -> String {
    trim_sep(&record_dir().to_string_lossy())
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn parent_dir(p: &str) -> String {
    Path::new(p).parent().map(|d| trim_sep(&d.to_string_lossy())).unwrap_or_default()
}

/// Depois de mover/renomear uma PASTA: lista no DESTINO e reconstrói o caminho
/// antigo pelo prefixo (o relativo dentro dela não mudou).
fn migrate_cache_for_tree(old_root: &str, new_root: &str) {
    let prefix = format!("{}\\", trim_sep(new_root));
    for f in list_recordings_recursive(new_root) {
        if let Some(rel) = f.strip_prefix(&prefix) {
            player::rename_cache_entries(&format!("{}\\{rel}", trim_sep(old_root)), &f);
        }
    }
}

/// `<dir>\<base><ext>` com sufixo ` (N)` até não colidir. Testa arquivo E
/// pasta: uma pasta "Aulas" impede outra "Aulas".
fn unique_target_path(dir: &str, base: &str, ext: &str) -> String {
    let dir = trim_sep(dir);
    let mut cand = format!("{dir}\\{base}{ext}");
    let mut n = 2;
    while Path::new(&cand).exists() {
        cand = format!("{dir}\\{base} ({n}){ext}");
        n += 1;
    }
    cand
}

/// Nome aceitável pro Windows: além dos caracteres ilegais, sem ponto/espaço
/// no FIM — o Windows os descarta calado e a pasta ficaria com nome diferente
/// do que a UI acabou de mostrar.
fn clean_folder_name(name: &str) -> String {
    sanitize_file_name(name).trim_end_matches(['.', ' ']).to_owned()
}

/// Navega (`""` = raiz). Re-lista, re-agenda o scan e RE-APONTA o watcher (ele
/// não é recursivo: sem isso, mudança pelo Explorer na pasta aberta não
/// atualizaria a tela).
pub fn handle_open_folder(path: &str) {
    let root = record_root();
    let target = if path.is_empty() { root.clone() } else { trim_sep(path) };
    if !is_dir_in_record_dir(&target) {
        log!("HandleOpenFolder: fora da pasta de gravacao, ignorado: {path}");
        return;
    }
    if !Path::new(&target).is_dir() {
        // Sumiu entre a listagem e o clique: volta pra raiz.
        post_error(lang::t("error.folderNotFound"));
        set_browse_dir("");
        push_recordings();
        return;
    }
    set_browse_dir(if target.eq_ignore_ascii_case(&root) { "" } else { &target });
    push_recordings();
    scan_recordings_meta();
    record_watch::update_dir(&current_browse_dir());
}

pub fn handle_create_folder(name: &str) {
    let nm = match name.trim() {
        "" => lang::t("recordings.newFolderName"),
        n => n.to_owned(),
    };
    let nm = clean_folder_name(&nm);
    if nm.is_empty() {
        return;
    }
    let cur = current_browse_dir();
    if !Path::new(&cur).is_dir() {
        post_error(lang::t("error.folderNotFound"));
        return;
    }
    let new_path = unique_target_path(&cur, &nm, "");
    if let Err(e) = std::fs::create_dir_all(&new_path) {
        log!("HandleCreateFolder: falha criando \"{new_path}\": {e}");
        post_error(lang::t("error.createFolderFailed"));
        return;
    }
    log!("Pasta criada: {new_path}");
    push_recordings();
    // DEPOIS da lista: a UI acha o card recém-criado e já abre a edição do
    // nome (o gesto do Explorer). Antes, o card ainda não existiria no DOM.
    post_json(json!({ "type": "folder_created", "id": new_path }));
}

pub fn handle_rename_folder(path: &str, new_name: &str) {
    let old = trim_sep(path);
    if old.is_empty() || new_name.is_empty() {
        return;
    }
    if !is_dir_in_record_dir(&old) {
        log!("HandleRenameFolder: fora da pasta de gravacao, ignorado: {path}");
        return;
    }
    // A raiz é a pasta das Configurações: renomeá-la mexeria no destino das
    // gravações por um caminho que não avisa isso.
    if old.eq_ignore_ascii_case(&record_root()) {
        return;
    }
    if !Path::new(&old).is_dir() {
        post_error(lang::t("error.folderNotFound"));
        return;
    }
    let nm = clean_folder_name(new_name);
    let cur_name = file_name(&old);
    if nm.is_empty() || cur_name == nm {
        return;
    }
    let parent = parent_dir(&old);
    // Só maiúscula/minúscula: rename válido no NTFS, mas o unique_target_path
    // veria a própria pasta como colisão e entregaria "Aulas (2)".
    let new_path = if cur_name.eq_ignore_ascii_case(&nm) {
        format!("{parent}\\{nm}")
    } else {
        unique_target_path(&parent, &nm, "")
    };
    if let Err(e) = std::fs::rename(&old, &new_path) {
        log!("HandleRenameFolder: falha \"{old}\" -> \"{new_path}\": {e}");
        post_error(lang::t("error.renameFailed"));
        return;
    }
    migrate_cache_for_tree(&old, &new_path);
    log!("Pasta renomeada: \"{old}\" -> \"{new_path}\"");
    push_recordings();
}

/// A pasta INTEIRA pra Lixeira (recuperável), em worker: o recycle pode
/// bloquear ~1,5 s com a worker de miniatura segurando um arquivo lá dentro.
/// O aviso de "tem N gravações" é da UI (a contagem vem no card).
pub fn handle_delete_folder(path: &str) {
    let p = trim_sep(path);
    if p.is_empty() {
        return;
    }
    if !is_dir_in_record_dir(&p) {
        log!("HandleDeleteFolder: fora da pasta de gravacao, ignorado: {path}");
        return;
    }
    if p.eq_ignore_ascii_case(&record_root()) {
        log::log("HandleDeleteFolder: recusado — e a propria pasta de gravacao.");
        return;
    }
    if !Path::new(&p).is_dir() {
        return;
    }
    std::thread::spawn(move || {
        let ok = delete_to_recycle_bin(&p);
        mainthread::queue(move || {
            if is_shutting_down() {
                return;
            }
            if !ok {
                post_error(lang::t_with("error.deleteFolderFailed", &[("name", &file_name(&p))]));
                return;
            }
            log!("Pasta excluida: {p}");
            // Se a pasta navegada estava dentro, current_browse_dir volta à
            // raiz sozinho na primeira leitura (o push logo abaixo).
            player::garbage_collect_cache(&list_recordings_recursive(&record_root()));
            push_recordings();
            record_watch::update_dir(&current_browse_dir());
        });
    });
}

/// Move gravações e/ou pastas para dentro de outra pasta (arrastar e colar).
pub fn handle_move_items(ids: &[Value], target: &str) {
    let target = if target.is_empty() { trim_sep(&current_browse_dir()) } else { trim_sep(target) };
    if !is_dir_in_record_dir(&target) {
        log!("HandleMoveItems: destino fora da pasta de gravacao: {target}");
        return;
    }
    if !Path::new(&target).is_dir() {
        post_error(lang::t("error.folderNotFound"));
        return;
    }
    let root = record_root();
    let mut moved = 0;
    for v in ids {
        let src = trim_sep(v.as_str().unwrap_or("").trim());
        if src.is_empty() {
            continue;
        }
        let is_dir = Path::new(&src).is_dir();
        if is_dir {
            if !is_dir_in_record_dir(&src) || src.eq_ignore_ascii_case(&root) {
                continue;
            }
            // Pasta dentro de si mesma ou de uma descendente: a árvore sumiria
            // dentro da própria origem. A UI já bloqueia; isto é a rede.
            let src_l = format!("{}\\", src.to_lowercase());
            if src.eq_ignore_ascii_case(&target) || format!("{}\\", target.to_lowercase()).starts_with(&src_l) {
                post_error(lang::t("error.moveIntoItself"));
                continue;
            }
        } else {
            if !is_path_in_record_dir(&src) || !is_recording_ext(&src) || !Path::new(&src).is_file() {
                continue;
            }
            // O muxer ainda está escrevendo nela.
            let last = last_recording_path();
            if recording_active() && !last.is_empty() && src.eq_ignore_ascii_case(&last) {
                post_error(lang::t("error.moveWhileRecording"));
                continue;
            }
            // A exportação está lendo este arquivo.
            if export::export_in_use(&src) {
                post_error(lang::t("error.exportBusyFile"));
                continue;
            }
        }
        // Já está no destino (arrastar pra pasta atual).
        if parent_dir(&src).eq_ignore_ascii_case(&target) {
            continue;
        }
        let nm = file_name(&src);
        let dest = if is_dir {
            unique_target_path(&target, &nm, "")
        } else {
            let p = PathBuf::from(&nm);
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(nm.clone());
            let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            unique_target_path(&target, &stem, &ext)
        };
        // Mesmo volume: é rename, instantâneo.
        if let Err(e) = std::fs::rename(&src, &dest) {
            log!("HandleMoveItems: falha movendo \"{src}\" -> \"{dest}\": {e}");
            post_error(lang::t_with("error.moveFailed", &[("file", &nm)]));
            continue;
        }
        // O cache segue o arquivo (hash do caminho).
        if is_dir {
            migrate_cache_for_tree(&src, &dest);
        } else {
            player::rename_cache_entries(&src, &dest);
        }
        moved += 1;
    }
    if moved > 0 {
        log!("HandleMoveItems: {moved} item(ns) movido(s) para \"{target}\".");
        push_recordings();
    }
}

// ---------------------------------------------------------------------------
// União
// ---------------------------------------------------------------------------

/// `<yyyy-mm-dd_hh-nn-ss> - U.mkv` (o " - U" é de "Unido"), com ` (N)`.
fn make_merge_path() -> String {
    let base = format!("{} - U", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"));
    unique_target_path(&record_root(), &base, ".mkv")
}

fn same_layout(a: &RecordingLayout, b: &RecordingLayout) -> bool {
    a.canvas_w == b.canvas_w
        && a.canvas_h == b.canvas_h
        && a.regions.len() == b.regions.len()
        && a.regions.iter().zip(&b.regions).all(|(x, y)| {
            x.name == y.name && x.kind == y.kind && x.x == y.x && x.y == y.y && x.w == y.w && x.h == y.h
        })
}

/// O layout só vai pro arquivo unido se TODAS as partes tiverem o mesmo.
fn common_merge_meta(paths: &[String]) -> Option<crate::types::RecordingMeta> {
    let first = player::load_recording_meta(paths.first()?)?;
    if first.layout.canvas_w <= 0 || first.layout.canvas_h <= 0 || first.layout.regions.is_empty() {
        return None;
    }
    for p in &paths[1..] {
        if !same_layout(&first.layout, &player::load_recording_meta(p)?.layout) {
            return None;
        }
    }
    Some(first)
}

fn push_merge_done(ok: bool) {
    post_json(json!({ "type": "merge_done", "ok": ok }));
}

/// Une N gravações (na ordem da seleção) num MKV novo por stream copy; os
/// originais vão pra Lixeira SÓ depois de dar certo. Em worker: arquivos
/// grandes levam segundos mesmo sem re-encode.
pub fn handle_merge_recordings(ids: &[Value]) {
    if ids.len() < 2 {
        return;
    }
    if !ffmpeg_lib_available() {
        push_merge_done(false);
        post_error(lang::t("error.mediaLibUnavailable"));
        return;
    }
    let mut paths = Vec::with_capacity(ids.len());
    let mut total: i64 = 0;
    for v in ids {
        let p = v.as_str().unwrap_or("").to_owned();
        if p.is_empty() || !is_path_in_record_dir(&p) || !is_recording_ext(&p) || !Path::new(&p).is_file() {
            push_merge_done(false);
            post_error(lang::t("error.fileNotFound"));
            return;
        }
        // Unir recicla os originais: não pode levar o que a exportação lê.
        if export::export_in_use(&p) {
            push_merge_done(false);
            post_error(lang::t("error.exportBusyFile"));
            return;
        }
        total += std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(0);
        paths.push(p);
    }
    // O unido tem ~a soma, e os originais ainda estão no disco até o fim.
    // Folga: +5% +16 MB (cabeçalhos/cues).
    let free = devices::record_dir_free_bytes();
    let needed = total + total / 20 + 16 * 1024 * 1024;
    if total > 0 && free >= 0 && free < needed {
        log!("HandleMergeRecordings: espaco insuficiente — precisa ~{needed}, livre {free}.");
        push_merge_done(false);
        post_error(lang::t_with(
            "error.mergeNoSpace",
            &[("needed", &format_bytes_short(needed)), ("free", &format_bytes_short(free))],
        ));
        return;
    }
    let out = make_merge_path();
    post_json(json!({ "type": "merge_pending" }));

    std::thread::spawn(move || {
        if is_shutting_down() {
            return;
        }
        let res = ffmpeg_ops::merge_files(&paths, &out);
        let ok = res.is_ok() && std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false);
        if !ok {
            let _ = std::fs::remove_file(&out);
            let incompatible = res == Err(true);
            mainthread::queue(move || {
                if is_shutting_down() {
                    return;
                }
                push_merge_done(false);
                post_error(lang::t(if incompatible { "error.mergeIncompatible" } else { "error.mergeFailed" }));
            });
            return;
        }
        if let Some(mut meta) = common_merge_meta(&paths) {
            meta.duration_sec = 0;
            player::save_recording_meta(&out, &meta);
            file_meta::mark_changed(&out);
        }
        // O merge já fechou os handles do libav. Um original em uso (aberto no
        // player) fica — e só os reciclados perdem o card.
        let recycled: Vec<String> = paths
            .iter()
            .filter(|p| {
                let ok = delete_to_recycle_bin(p);
                if !ok {
                    log!("HandleMergeRecordings: \"{}\" nao foi pra lixeira (em uso?).", file_name(p));
                }
                ok
            })
            .cloned()
            .collect();
        mainthread::queue(move || {
            if is_shutting_down() {
                return;
            }
            for p in &recycled {
                post_json(json!({ "type": "recording_removed", "id": p }));
            }
            player::garbage_collect_cache(&list_recordings_recursive(&record_root()));
            push_recording_added(&out, 0);
            push_merge_done(true);
        });
    });
}

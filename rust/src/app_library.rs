//! Presenter da biblioteca: recebe a lista do core pelo [`View`](crate::view::View),
//! agrupa por período, filtra pela busca, guarda a seleção, o recorte e a edição
//! de nome, e devolve os gestos ao core por chamada direta. O desenho é a parte
//! `lyLibrary` do `forms/main.json`; os cartões (`MediaCard`) são montados aqui.
//!
//! O estado que importa (pasta navegada, o que tem nela) NÃO é deduzido aqui:
//! chega inteiro em [`on_library`] a cada mudança. O que é só da tela — seleção,
//! recorte pendente, busca — mora aqui.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};
use iron::prelude::*;

use crate::app_view::{main_form, with_ui};
use crate::view::{Library, RecItem};
use crate::{bridge, lang};

/// Abaixo disso o "livres" ganha o aviso.
const LOW_DISK_BYTES: i64 = 5 * 1024 * 1024 * 1024;
/// Espera da busca na transcrição: cada tecla varreria o disco.
const SEARCH_DEBOUNCE_MS: u64 = 300;
/// Miniaturas guardadas; o resto sai pela ordem de uso.
const THUMB_CACHE_MAX: usize = 160;
/// O cartão de gravação, o de pasta e o espaço entre eles.
const CARD_W: f32 = 170.0;
const CARD_H: f32 = 148.0;
const FOLDER_H: f32 = 54.0;
const GAP: f32 = 12.0;
/// A altura do título de um grupo (com a linha embaixo e o respiro).
/// Título de grupo: 6 + 15,4 + 6 + 1 de linha, e 8 até os cartões.
const GROUP_HEAD: f32 = 28.4 + 8.0;
/// O vão antes de um grupo: 14 depois do cabeçalho, 18 entre grupos.
const FIRST_GAP: f32 = 14.0;
const GROUP_GAP: f32 = 18.0;

#[derive(Default)]
struct State {
    lib: Library,
    /// Lista que chegou durante uma edição de nome: aplica quando ela acabar.
    pending: Option<Library>,
    query: String,
    tr_hits: HashSet<String>,
    search_job: Option<u64>,
    /// Em ordem de seleção: é a ordem da união.
    selection: Vec<String>,
    clipboard: Vec<String>,
    /// Id em edição e se é pasta.
    editing: Option<(String, bool)>,
    merge_busy: bool,
    /// O cartão de cada gravação/pasta na tela (id → `MediaCard`).
    cards: HashMap<String, ComponentId>,
    /// As caixas "selecionar todos" de cada grupo (ids dos itens, a caixa).
    groups: Vec<(Vec<String>, ComponentId)>,
    /// Os ids do caminho (na ordem do `Breadcrumb`).
    crumbs: Vec<String>,
    thumbs: HashMap<String, (Image, u64)>,
    thumb_clock: u64,
    /// O menu de contexto aberto: (tipo, id).
    ctx: (String, String),
    menu: Option<ComponentId>,
    /// Quantas colunas cabem (a altura das grades sai daí).
    cols: usize,
    /// O que está sendo arrastado (gravações e/ou pastas).
    drag: Vec<String>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn c(ui: &Ui, name: &str) -> Option<ComponentId> {
    ui.find_in(main_form()?, name)
}

fn set(ui: &mut Ui, name: &str, prop: &str, value: impl Into<PropValue>) {
    if let Some(id) = c(ui, name) {
        let _ = ui.set_prop(id, prop, value);
    }
}

// ---------------------------------------------------------------------------
// Formatação
// ---------------------------------------------------------------------------

/// `mm:ss` ou `hh:mm:ss`; vazio sem duração.
fn format_duration(sec: i32) -> String {
    if sec <= 0 {
        return String::new();
    }
    let (h, m, s) = (sec / 3600, (sec % 3600) / 60, sec % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn format_bytes(b: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else if f >= MB {
        format!("{} MB", (f / MB).round())
    } else if f >= KB {
        format!("{} KB", (f / KB).round())
    } else {
        format!("{b} B")
    }
}

fn parse_date(iso: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S").ok()
}

fn format_full_date(iso: &str) -> String {
    parse_date(iso).map(|d| d.format("%d/%m/%Y %H:%M:%S").to_string()).unwrap_or_default()
}

fn file_count_text(n: i64) -> String {
    let key = if n == 1 { "recordings.fileCount_one" } else { "recordings.fileCount_other" };
    lang::t_with(key, &[("count", &n.to_string())])
}

/// (chave que ordena, rótulo): hoje, ontem, esta semana, e daí por mês.
fn period(iso: &str, today: NaiveDate) -> (String, String) {
    let Some(d) = parse_date(iso) else {
        return ("zzz-unknown".into(), lang::t("dateGroups.noDate"));
    };
    let diff = (today - d.date()).num_days();
    if diff <= 0 {
        return ("00-hoje".into(), lang::t("dateGroups.today"));
    }
    if diff < 2 {
        return ("01-ontem".into(), lang::t("dateGroups.yesterday"));
    }
    if diff < 7 {
        return ("02-semana".into(), lang::t("dateGroups.thisWeek"));
    }
    let month = lang::t(&format!("months.{}", d.month0()));
    // Mais novo primeiro: a chave inverte o ano/mês.
    let key = format!("99-{:06}", 999_999 - (d.year() * 100 + d.month0() as i32));
    (key, format!("{month} {}", d.year()))
}

/// O texto em que a busca procura.
fn search_text(it: &RecItem) -> String {
    let mut s = format!("{} {} {} {}", it.name, it.size_text, format_duration(it.duration), it.codec);
    if it.fps > 0 {
        s.push_str(&format!(" {}{}", it.fps, lang::t("recordings.badge.fpsSuffix")));
    }
    if it.quality >= 0 {
        s.push(' ');
        s.push_str(&lang::t_with("recordings.badge.qualityShort", &[("level", &it.quality.to_string())]));
    }
    s.to_lowercase()
}

/// Os selos do cartão: codec (em destaque quando é por hardware), fps, nível
/// de qualidade e o ícone da transcrição.
fn badges(it: &RecItem) -> String {
    let mut out = Vec::new();
    if !it.codec.is_empty() {
        // Âmbar quando a codificação foi por software (como no original).
        out.push(if it.codec_hw { it.codec.clone() } else { format!("*{}", it.codec) });
    }
    if it.fps > 0 {
        out.push(format!("{}{}", it.fps, lang::t("recordings.badge.fpsSuffix")));
    }
    if it.quality >= 0 {
        out.push(lang::t_with("recordings.badge.qualityShort", &[("level", &it.quality.to_string())]));
    }
    if it.transcript == "ok" {
        out.push("icon:list".into());
    }
    out.join(";")
}

fn folder_meta(count: i64, size_text: &str) -> String {
    let mut s = file_count_text(count);
    if count > 0 && !size_text.is_empty() {
        s.push_str(" · ");
        s.push_str(size_text);
    }
    s
}

// ---------------------------------------------------------------------------
// Tela
// ---------------------------------------------------------------------------

/// Quantos cartões cabem numa linha da largura da biblioteca.
fn columns(ui: &Ui) -> usize {
    let w = c(ui, "lyLibContent").and_then(|id| ui.bounds(id)).map(|b| b.width).filter(|w| *w > 0.0);
    let w = w.or_else(|| c(ui, "lyLibrary").and_then(|id| ui.bounds(id)).map(|b| b.width - 44.0)).unwrap_or(1100.0);
    (((w + GAP) / (CARD_W + GAP)).floor() as usize).max(1)
}

fn grid_height(n: usize, cols: usize, h: f32) -> f32 {
    let rows = n.div_ceil(cols.max(1));
    if rows == 0 { 0.0 } else { rows as f32 * (h + GAP) - GAP }
}

/// O título de um grupo: o rótulo em caixa-alta, a linha e (nas gravações) a
/// caixa de selecionar o grupo todo.
fn add_group_head(ui: &mut Ui, parent: ComponentId, label: &str, check: Option<(bool, Vec<String>)>, gap: f32) -> Option<ComponentId> {
    let head = ui.create("Layout", Some(parent)).ok()?;
    if let Some(cb) = ui.control_mut(head) {
        cb.set_align(Align::Top).set_height(28.4).set_margins([0.0, gap, 0.0, 8.0]).set_hit_test(false);
    }
    let line = ui.create("Rectangle", Some(head)).ok()?;
    let _ = ui.set_prop(line, "style", "sep");
    if let Some(cb) = ui.control_mut(line) {
        cb.set_align(Align::Bottom).set_height(1.0).set_margins([-22.0, 0.0, -22.0, 0.0]);
    }
    let mut boxed = None;
    if let Some((all, ids)) = check {
        if let Ok(b) = ui.create("CheckBox", Some(head)) {
            let _ = ui.set_prop(b, "checked", all);
            let _ = ui.set_prop(b, "hint", lang::t("recordings.selectAllInGroup"));
            if let Some(cb) = ui.control_mut(b) {
                cb.set_align(Align::Right).set_width(16.0).set_margins([0.0, 5.7, 0.0, 6.7]);
            }
            let ids2 = ids.clone();
            ui.on(b, "on_change", move |ui, _, _| group_toggle(ui, &ids2));
            boxed = Some(b);
            with(|st| st.groups.push((ids, b)));
        }
    }
    if let Ok(l) = ui.create("Label", Some(head)) {
        let _ = ui.set_prop(l, "style", "section");
        let _ = ui.set_prop(l, "text", label);
        let _ = ui.set_prop(l, "text_case", "upper");
        let _ = ui.set_prop(l, "letter_spacing", 0.66);
        if let Some(cb) = ui.control_mut(l) {
            cb.set_align(Align::Client).set_margins([0.0, 6.0, 0.0, 7.0]);
        }
    }
    let _ = boxed;
    Some(head)
}

fn add_grid(ui: &mut Ui, parent: ComponentId, n: usize, cols: usize, h: f32) -> Option<ComponentId> {
    let grid = ui.create("FlowLayout", Some(parent)).ok()?;
    let _ = ui.set_prop(grid, "gap", GAP);
    let _ = ui.set_prop(grid, "line_gap", GAP);
    if let Some(cb) = ui.control_mut(grid) {
        // Os vãos entre os cartões são do fundo (o menu do botão direito).
        cb.set_align(Align::Top).set_height(grid_height(n, cols, h)).set_clip_children(false).set_hit_test(false);
    }
    Some(grid)
}

/// Refaz a biblioteca inteira a partir de `lib` + busca + seleção.
fn render(ui: &mut Ui) {
    let today = Local::now().date_naive();
    let cols = columns(ui);
    // Seleção órfã (o arquivo sumiu): sem isso o clique em outro card entraria
    // em "modo seleção" em vez de tocar.
    let (items, groups, folders, meta, low_disk, at_root, crumbs, empty, query) = with(|st| {
        st.cols = cols;
        let ids: HashSet<&str> = st.lib.recordings.iter().map(|r| r.id.as_str()).collect();
        st.selection.retain(|id| ids.contains(id.as_str()));
        let q = st.query.clone();
        let searching = !q.is_empty();
        let mut items: Vec<RecItem> =
            st.lib.recordings.iter().filter(|it| !searching || search_text(it).contains(&q) || st.tr_hits.contains(&it.id)).cloned().collect();
        items.sort_by(|a, b| b.date.cmp(&a.date));
        // Agrupa preservando a ordem (mais novo → mais antigo).
        let mut order: Vec<(String, String)> = Vec::new();
        let mut by_key: HashMap<String, Vec<RecItem>> = HashMap::new();
        for it in &items {
            let (key, label) = period(&it.date, today);
            by_key.entry(key.clone()).or_default().push(it.clone());
            if !order.iter().any(|(k, _)| k == &key) {
                order.push((key, label));
            }
        }
        order.sort_by(|a, b| a.0.cmp(&b.0));
        let groups: Vec<(String, Vec<RecItem>)> = order.into_iter().map(|(k, label)| (label, by_key.remove(&k).unwrap_or_default())).collect();
        // Pastas: o ".." vem primeiro fora da raiz; na busca, só as que casam.
        let mut folders: Vec<(String, String, String, bool)> = Vec::new();
        if !st.lib.at_root && !searching {
            folders.push((st.lib.parent_dir.clone(), "..".into(), lang::t("recordings.folderUp"), true));
        }
        for f in &st.lib.folders {
            let meta = folder_meta(f.count, &f.size_text);
            if searching && !format!("{} {}", f.name, meta).to_lowercase().contains(&q) {
                continue;
            }
            folders.push((f.id.clone(), f.name.clone(), meta, false));
        }
        let crumbs: Vec<(String, String)> = st
            .lib
            .breadcrumb
            .iter()
            .map(|c| (c.id.clone(), if c.root { lang::t("recordings.libraryRoot") } else { c.name.clone() }))
            .collect();
        let total: i64 = items.iter().map(|it| it.size).sum();
        let mut meta = vec![file_count_text(items.len() as i64)];
        if total > 0 {
            meta.push(lang::t_with("recordings.spaceUsed", &[("size", &format_bytes(total))]));
        }
        let free = st.lib.free_bytes;
        if free >= 0 {
            meta.push(lang::t_with("recordings.spaceFree", &[("size", &format_bytes(free))]));
        }
        let empty = searching && items.is_empty() && folders.is_empty();
        (items, groups, folders, meta, free >= 0 && free < LOW_DISK_BYTES, st.lib.at_root, crumbs, empty, q)
    });
    let _ = items;
    set(ui, "lblLibMeta", "text", meta.join("   |   "));
    set(ui, "lblLibMeta", "color", if low_disk { "amber" } else { "" });
    set(ui, "lblLibMeta", "hint", if low_disk { lang::t("recordings.lowSpaceHint") } else { String::new() });
    // O caminho, fora da raiz.
    set(ui, "brdLib", "visible", !at_root);
    set(ui, "brdLib", "items", crumbs.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join(";"));
    with(|st| st.crumbs = crumbs.iter().map(|(id, _)| id.clone()).collect());
    set(ui, "lblLibEmpty", "visible", empty);
    set(ui, "lblLibEmpty", "text", if empty { lang::t_with("recordings.empty", &[("query", &query)]) } else { String::new() });

    let Some(content) = c(ui, "lyLibContent") else { return };
    for child in ui.children(content).to_vec() {
        ui.free(child);
    }
    with(|st| {
        st.cards.clear();
        st.groups.clear();
    });
    let mut height = 0.0;
    let mut gap = FIRST_GAP;
    if !folders.is_empty() {
        add_group_head(ui, content, &lang::t("recordings.foldersGroup"), None, gap);
        height += gap + GROUP_HEAD;
        if let Some(grid) = add_grid(ui, content, folders.len(), cols, FOLDER_H) {
            for (id, name, meta, up) in &folders {
                add_folder_card(ui, grid, id, name, meta, *up);
            }
            height += grid_height(folders.len(), cols, FOLDER_H);
        }
        gap = GROUP_GAP;
    }
    for (label, list) in &groups {
        let ids: Vec<String> = list.iter().map(|it| it.id.clone()).collect();
        let all = with(|st| ids.iter().all(|id| st.selection.contains(id)));
        add_group_head(ui, content, label, Some((all, ids)), gap);
        height += gap + GROUP_HEAD;
        if let Some(grid) = add_grid(ui, content, list.len(), cols, CARD_H) {
            for it in list {
                add_rec_card(ui, grid, it);
            }
            height += grid_height(list.len(), cols, CARD_H);
        }
        gap = GROUP_GAP;
    }
    if let Some(cb) = ui.control_mut(content) {
        cb.set_height(height);
    }
    let crumb_h = if at_root { 0.0 } else { 42.0 };
    let empty_h = if empty { 60.0 } else { 0.0 };
    if let Some(cb) = c(ui, "lyLibrary").and_then(|id| ui.control_mut(id)) {
        // 20 + título 22 + 10 + ações 30, o caminho, o aviso, os grupos e 30 embaixo.
        cb.set_height(20.0 + 22.0 + 10.0 + 30.0 + crumb_h + empty_h + height + 30.0);
    }
    sync_bar(ui);
    ui.layout();
}

/// O cartão de uma gravação. A miniatura vem do cache (o JPG que o core gerou).
fn add_rec_card(ui: &mut Ui, grid: ComponentId, it: &RecItem) {
    let Ok(card) = ui.create("MediaCard", Some(grid)) else { return };
    let (order, cut, selection_mode) = with(|st| {
        (st.selection.iter().position(|x| x == &it.id).map_or(0, |p| p + 1), st.clipboard.contains(&it.id), !st.selection.is_empty())
    });
    for (p, v) in [("title", it.name.clone()), ("subtitle", it.size_text.clone()), ("duration", format_duration(it.duration)), ("badges", badges(it)), ("icon", "video".into())] {
        let _ = ui.set_prop(card, p, v);
    }
    let _ = ui.set_prop(card, "hint", format_full_date(&it.date));
    let _ = ui.set_prop(card, "selected", order > 0);
    let _ = ui.set_prop(card, "order", order as i64);
    let _ = ui.set_prop(card, "dimmed", cut);
    let _ = ui.set_prop(card, "selection_mode", selection_mode);
    if let Some(cb) = ui.control_mut(card) {
        cb.set_rect(0.0, 0.0, CARD_W, CARD_H).set_draggable(true);
    }
    let id = it.id.clone();
    ui.on(card, "on_drag_start", move |ui, _, _| drag_start(ui, &id));
    ui.on(card, "on_drag_end", |ui, _, _| drag_end(ui));
    if let Some(image) = thumb_for(&it.id, &it.thumb) {
        if let Some(mc) = ui.get_mut::<MediaCard>(card) {
            mc.set_image(Some(image));
        }
    }
    let id = it.id.clone();
    ui.on(card, "on_click", move |ui, _, _| card_clicked(ui, &id));
    let id = it.id.clone();
    ui.on(card, "on_select", move |ui, _, _| toggle_select(ui, &id));
    let id = it.id.clone();
    ui.on(card, "on_context", move |ui, sender, args| {
        if let EventArgs::Pointer(e) = args {
            open_context(ui, sender, "recording", &id, e.position);
        }
    });
    with(|st| st.cards.insert(it.id.clone(), card));
}

fn add_folder_card(ui: &mut Ui, grid: ComponentId, id: &str, name: &str, meta: &str, up: bool) {
    let Ok(card) = ui.create("MediaCard", Some(grid)) else { return };
    let _ = ui.set_prop(card, "compact", true);
    let _ = ui.set_prop(card, "icon", if up { "arrow-up" } else { "folder" });
    let _ = ui.set_prop(card, "title", name);
    let _ = ui.set_prop(card, "subtitle", meta);
    let _ = ui.set_prop(card, "hint", if up { lang::t("recordings.folderUpHint") } else { id.to_owned() });
    let cut = with(|st| st.clipboard.iter().any(|c| c == id));
    let _ = ui.set_prop(card, "dimmed", cut && !up);
    if let Some(cb) = ui.control_mut(card) {
        // Pasta recebe o que é solto nela (o ".." manda para a de cima); as outras
        // também se arrastam.
        cb.set_rect(0.0, 0.0, CARD_W, FOLDER_H).set_accepts_drops(true).set_draggable(!up);
    }
    let target = id.to_owned();
    ui.on(card, "on_drag_over", move |ui, _, _| {
        if !can_drop(&target) {
            ui.reject_drop();
        }
    });
    let target = id.to_owned();
    ui.on(card, "on_drag_drop", move |ui, _, _| drop_on_folder(ui, &target));
    if !up {
        let source = id.to_owned();
        ui.on(card, "on_drag_start", move |ui, _, _| drag_start(ui, &source));
        ui.on(card, "on_drag_end", |ui, _, _| drag_end(ui));
    }
    let target = id.to_owned();
    ui.on(card, "on_click", move |ui, _, _| {
        let _ = ui;
        bridge::handle_open_folder(&target);
    });
    if !up {
        let target = id.to_owned();
        ui.on(card, "on_context", move |ui, sender, args| {
            if let EventArgs::Pointer(e) = args {
                open_context(ui, sender, "folder", &target, e.position);
            }
        });
        with(|st| st.cards.insert(id.to_owned(), card));
    }
}

/// A miniatura decodificada (guardada por ordem de uso).
fn thumb_for(id: &str, path: &str) -> Option<Image> {
    let hit = with(|st| {
        st.thumb_clock += 1;
        let clock = st.thumb_clock;
        st.thumbs.get_mut(id).map(|e| {
            e.1 = clock;
            e.0.clone()
        })
    });
    if hit.is_some() || path.is_empty() {
        return hit;
    }
    let bytes = std::fs::read(path).ok()?;
    let image = Image::encoded(bytes);
    with(|st| {
        st.thumb_clock += 1;
        let clock = st.thumb_clock;
        st.thumbs.insert(id.to_owned(), (image.clone(), clock));
        while st.thumbs.len() > THUMB_CACHE_MAX {
            let Some(old) = st.thumbs.iter().min_by_key(|(_, (_, c))| *c).map(|(k, _)| k.clone()) else { break };
            st.thumbs.remove(&old);
        }
    });
    Some(image)
}

/// Botões da barra conforme a seleção.
fn sync_bar(ui: &mut Ui) {
    let (n, busy) = with(|st| (st.selection.len(), st.merge_busy));
    set(ui, "btnLibDelete", "enabled", n > 0);
    set(
        ui,
        "btnLibDelete",
        "hint",
        match n {
            0 => lang::t("recordings.selectToDelete"),
            1 => lang::t("recordings.deleteOne"),
            _ => lang::t_with("recordings.deleteN", &[("count", &n.to_string())]),
        },
    );
    set(ui, "btnLibMerge", "enabled", n >= 2 && !busy);
    set(
        ui,
        "btnLibMerge",
        "hint",
        if busy {
            lang::t("recordings.merging")
        } else if n < 2 {
            lang::t("recordings.selectToMerge")
        } else {
            lang::t_with("recordings.mergeN", &[("count", &n.to_string())])
        },
    );
    // Exportar opera sobre UMA gravação, e uma exportação por vez.
    set(ui, "btnLibExport", "enabled", n == 1 && crate::app_export::running_id().is_empty());
    set(ui, "btnLibExport", "hint", if n == 1 { lang::t("export.buttonHint") } else { lang::t("export.selectOne") });
}

/// Só as marcas de seleção/recorte, sem refazer os cartões.
fn refresh_marks(ui: &mut Ui) {
    let (cards, selection, clipboard, groups) = with(|st| (st.cards.clone(), st.selection.clone(), st.clipboard.clone(), st.groups.clone()));
    let mode = !selection.is_empty();
    for (id, card) in cards {
        let order = selection.iter().position(|x| x == &id).map_or(0, |p| p + 1);
        if let Some(mc) = ui.get_mut::<MediaCard>(card) {
            mc.selected = order > 0;
            mc.order = order as u32;
            mc.selection_mode = mode && !mc.compact;
            mc.dimmed = clipboard.contains(&id);
        }
    }
    for (ids, b) in groups {
        let all = !ids.is_empty() && ids.iter().all(|id| selection.contains(id));
        let _ = ui.set_prop(b, "checked", all);
    }
    sync_bar(ui);
    ui.invalidate();
}

/// A exportação começou/terminou: o botão "Exportar" acompanha.
pub fn sync_export_button() {
    with_ui(sync_bar);
}

/// Exportação concluída: a seleção (a gravação de origem) sai.
pub fn clear_selection() {
    with(|st| st.selection.clear());
    with_ui(refresh_marks);
}

/// A barra de progresso da exportação no cartão da gravação de origem.
pub fn set_export_progress(source: &str, pct: Option<f64>, label: &str) {
    let (source, label) = (source.to_owned(), label.to_owned());
    with_ui(move |ui| {
        let Some(card) = with(|st| st.cards.get(&source).copied()) else { return };
        if let Some(mc) = ui.get_mut::<MediaCard>(card) {
            mc.progress = pct.map_or(-1.0, |p| (p / 100.0) as f32);
            mc.progress_text = label;
        }
        ui.invalidate();
    });
}

// ---------------------------------------------------------------------------
// Do core (View)
// ---------------------------------------------------------------------------

pub fn on_library(lib: &Library) {
    let lib = lib.clone();
    with_ui(move |ui| {
        let editing = with(|st| {
            if st.editing.is_some() {
                st.pending = Some(lib.clone());
                true
            } else {
                st.lib = lib;
                false
            }
        });
        if !editing {
            render(ui);
        }
    });
}

pub fn on_recording_added(item: &RecItem, free: i64) {
    let item = item.clone();
    with_ui(move |ui| {
        with(|st| {
            if !st.lib.recordings.iter().any(|r| r.id == item.id) {
                st.lib.recordings.push(item);
            }
            st.lib.free_bytes = free;
        });
        render(ui);
    });
}

pub fn on_recording_meta(id: &str, duration: i32, thumb: &str) {
    let (id, thumb) = (id.to_owned(), thumb.to_owned());
    with_ui(move |ui| {
        let found = with(|st| {
            let Some(r) = st.lib.recordings.iter_mut().find(|r| r.id == id) else { return false };
            if duration > 0 {
                r.duration = duration;
            }
            if !thumb.is_empty() {
                r.thumb = thumb.clone();
                st.thumbs.remove(&id);
            }
            true
        });
        if !found {
            return;
        }
        let Some(card) = with(|st| st.cards.get(&id).copied()) else { return };
        let image = if thumb.is_empty() { None } else { thumb_for(&id, &thumb) };
        if let Some(mc) = ui.get_mut::<MediaCard>(card) {
            if duration > 0 {
                mc.duration = format_duration(duration);
            }
            if image.is_some() {
                mc.set_image(image);
            }
        }
        ui.invalidate();
    });
}

pub fn on_recording_renamed(old_id: &str, new_id: &str, new_name: &str) {
    let (old_id, new_id, new_name) = (old_id.to_owned(), new_id.to_owned(), new_name.to_owned());
    with_ui(move |ui| {
        with(|st| {
            if let Some(r) = st.lib.recordings.iter_mut().find(|r| r.id == old_id) {
                r.id = new_id.clone();
                r.name = new_name.clone();
            }
            for s in st.selection.iter_mut().chain(st.clipboard.iter_mut()) {
                if *s == old_id {
                    *s = new_id.clone();
                }
            }
            if let Some(t) = st.thumbs.remove(&old_id) {
                st.thumbs.insert(new_id.clone(), t);
            }
        });
        render(ui);
    });
}

pub fn on_recording_removed(id: &str, free: i64) {
    let id = id.to_owned();
    with_ui(move |ui| {
        with(|st| {
            st.lib.recordings.retain(|r| r.id != id);
            st.selection.retain(|s| *s != id);
            st.thumbs.remove(&id);
            if free >= 0 {
                st.lib.free_bytes = free;
            }
        });
        render(ui);
    });
}

pub fn on_folder_created(id: &str) {
    let id = id.to_owned();
    with_ui(move |ui| {
        let name = with(|st| st.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.clone()));
        if let Some(name) = name {
            begin_edit(ui, &id, &name, true);
        }
    });
}

pub fn on_transcript_search(query: &str, ids: &[String]) {
    let (query, ids) = (query.to_owned(), ids.to_vec());
    with_ui(move |ui| {
        let current = with(|st| {
            if query.to_lowercase().trim() != st.query {
                return false;
            }
            st.tr_hits = ids.into_iter().collect();
            true
        });
        if current {
            render(ui);
        }
    });
}

pub fn on_merge_pending() {
    with(|st| st.merge_busy = true);
    with_ui(sync_bar);
    crate::app_view::show_toast(&lang::t("toast.mergeStarted"), &lang::t("toast.mergeStartedMsg"), false, 3000);
}

pub fn on_merge_done(ok: bool) {
    with(|st| {
        st.merge_busy = false;
        if ok {
            st.selection.clear();
        }
    });
    with_ui(refresh_marks);
    if ok {
        crate::app_view::show_toast(&lang::t("toast.mergeDone"), &lang::t("toast.mergeDoneMsg"), false, 5000);
    }
}

// ---------------------------------------------------------------------------
// Gestos
// ---------------------------------------------------------------------------

/// O item sob o clique e, se ele está numa seleção de vários, todos eles.
fn target_ids(id: &str) -> Vec<String> {
    with(|st| if st.selection.len() > 1 && st.selection.iter().any(|s| s == id) { st.selection.clone() } else { vec![id.to_owned()] })
}

fn is_folder(id: &str) -> bool {
    with(|st| st.lib.folders.iter().any(|f| f.id.eq_ignore_ascii_case(id)))
}

// ----- arrastar -----

/// Começa a arrastar o cartão (com a seleção, se ele estiver nela). A lixeira acende.
fn drag_start(ui: &mut Ui, id: &str) {
    if with(|st| st.editing.is_some()) {
        return;
    }
    let ids = target_ids(id);
    let label = with(|st| {
        let name_of = |id: &str| {
            st.lib
                .recordings
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.name.clone())
                .or_else(|| st.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.clone()))
                .unwrap_or_default()
        };
        let first = name_of(&ids[0]);
        if ids.len() > 1 { format!("{first} +{}", ids.len() - 1) } else { first }
    });
    with(|st| st.drag = ids.clone());
    ui.begin_drag(ids.join("\n"), label);
    set(ui, "btnLibDelete", "enabled", true);
    set(ui, "btnLibDelete", "text_color", "red");
}

/// Acabou (solto ou cancelado): a barra volta ao normal.
fn drag_end(ui: &mut Ui) {
    with(|st| st.drag.clear());
    set(ui, "btnLibDelete", "text_color", "");
    sync_bar(ui);
}

/// Nem a própria pasta, nem pasta dentro dela mesma, nem onde já está.
fn can_drop(target: &str) -> bool {
    let t = target.to_lowercase();
    if t.is_empty() {
        return false;
    }
    with(|st| {
        !st.drag.is_empty()
            && st.drag.iter().all(|s| {
                let s = s.to_lowercase();
                if s == t || t.starts_with(&format!("{s}\\")) {
                    return false;
                }
                !matches!(s.rfind('\\'), Some(cut) if cut > 0 && s[..cut] == t)
            })
    })
}

fn drop_on_folder(ui: &mut Ui, target: &str) {
    let ids = with(|st| st.drag.clone());
    if ids.is_empty() || !can_drop(target) || export_blocked(&ids) {
        return;
    }
    // Solto: a seleção não descreve mais o que está na tela.
    with(|st| st.selection.clear());
    refresh_marks(ui);
    bridge::handle_move_items(&ids, target);
}

/// Solto na lixeira: pasta tem confirmação própria (avisa quantas gravações vão
/// junto); arrastar uma pasta leva só ela.
pub fn drop_on_trash(ui: &mut Ui) {
    let ids = with(|st| st.drag.clone());
    if ids.is_empty() {
        return;
    }
    if ids.len() == 1 && is_folder(&ids[0]) {
        delete_folder(ui, &ids[0]);
    } else {
        delete_recordings(ui, ids.into_iter().filter(|x| !is_folder(x)).collect());
    }
}

fn export_blocked(ids: &[String]) -> bool {
    if ids.iter().any(|id| bridge::export_in_use(id)) {
        crate::app_view::show_toast(&lang::t("toast.errorTitle"), &lang::t("error.exportBusyFile"), true, 6000);
        return true;
    }
    false
}

fn toggle_select(ui: &mut Ui, id: &str) {
    with(|st| {
        if let Some(p) = st.selection.iter().position(|s| s == id) {
            st.selection.remove(p);
        } else {
            st.selection.push(id.to_owned());
        }
    });
    refresh_marks(ui);
}

fn group_toggle(ui: &mut Ui, ids: &[String]) {
    with(|st| {
        let all = ids.iter().all(|id| st.selection.contains(id));
        for id in ids {
            let has = st.selection.contains(id);
            if all && has {
                st.selection.retain(|s| s != id);
            } else if !all && !has {
                st.selection.push(id.clone());
            }
        }
    });
    refresh_marks(ui);
}

/// Com algo selecionado, o clique seleciona; senão, abre no player.
fn card_clicked(ui: &mut Ui, id: &str) {
    if with(|st| !st.selection.is_empty()) {
        toggle_select(ui, id);
        return;
    }
    crate::app_player::open(ui, id);
}

pub fn search(ui: &mut Ui, text: &str) {
    let q = text.to_lowercase().trim().to_owned();
    let (changed, job) = with(|st| {
        if st.query == q {
            return (false, None);
        }
        st.query = q.clone();
        st.tr_hits.clear();
        (true, st.search_job.take())
    });
    if !changed {
        return;
    }
    if let Some(job) = job {
        ui.clear_timeout(job);
    }
    if q.is_empty() {
        bridge::handle_search_transcripts("");
    } else {
        let q2 = q.clone();
        let job = ui.set_timeout(SEARCH_DEBOUNCE_MS, move |_| bridge::handle_search_transcripts(&q2));
        with(|st| st.search_job = Some(job));
    }
    render(ui);
}

fn cut(ui: &mut Ui, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    let n = ids.len();
    with(|st| st.clipboard = ids);
    refresh_marks(ui);
    // As pastas recortadas também apagam (a marca está nos cartões de pasta).
    let msg = if n == 1 { lang::t("toast.cutOne") } else { lang::t_with("toast.cutN", &[("count", &n.to_string())]) };
    crate::app_view::show_toast(&lang::t("toast.cutTitle"), &msg, false, 3500);
}

fn paste(ui: &mut Ui, target: &str) {
    let ids = with(|st| st.clipboard.clone());
    if ids.is_empty() || export_blocked(&ids) {
        return;
    }
    with(|st| st.clipboard.clear());
    refresh_marks(ui);
    bridge::handle_move_items(&ids, target);
}

/// O diálogo de confirmação (o `confirm` do iron); `on_ok` roda no OK.
fn confirm(ui: &mut Ui, title: String, message: String, ok: String, danger: bool, on_ok: impl FnOnce(&mut Ui) + 'static) {
    let icon = if danger { crate::app_view::ConfirmIcon::Delete } else { crate::app_view::ConfirmIcon::Merge };
    crate::app_view::confirm(ui, &title, &message, &ok, danger, icon, on_ok);
}

fn do_delete(ui: &mut Ui, ids: Vec<String>) {
    with(|st| {
        st.lib.recordings.retain(|r| !ids.contains(&r.id));
        st.selection.retain(|s| !ids.contains(s));
    });
    render(ui);
    for id in &ids {
        bridge::handle_delete_recording(id);
    }
}

fn delete_recordings(ui: &mut Ui, ids: Vec<String>) {
    let ids: Vec<String> = ids.into_iter().filter(|s| !s.is_empty()).collect();
    if ids.is_empty() || export_blocked(&ids) {
        return;
    }
    let msg = if ids.len() == 1 {
        lang::t("recordings.confirmDeleteOne")
    } else {
        lang::t_with("recordings.confirmDeleteN", &[("count", &ids.len().to_string())])
    };
    confirm(ui, lang::t("recordings.confirmDeleteTitle"), msg, lang::t("common.delete"), true, move |ui| do_delete(ui, ids));
}

fn delete_one(ui: &mut Ui, id: &str) {
    if export_blocked(&[id.to_owned()]) {
        return;
    }
    let name = with(|st| st.lib.recordings.iter().find(|r| r.id == id).map(|r| r.name.clone())).unwrap_or_else(|| id.to_owned());
    let ids = vec![id.to_owned()];
    confirm(
        ui,
        lang::t("recordings.confirmDeleteSingleTitle"),
        lang::t_with("recordings.confirmDeleteNamed", &[("name", &name)]),
        lang::t("common.delete"),
        true,
        move |ui| do_delete(ui, ids),
    );
}

fn delete_folder(ui: &mut Ui, id: &str) {
    let (name, count) = with(|st| st.lib.folders.iter().find(|f| f.id == id).map(|f| (f.name.clone(), f.count)).unwrap_or((id.to_owned(), 0)));
    let msg = match count {
        0 => lang::t_with("recordings.confirmDeleteFolderEmpty", &[("name", &name)]),
        1 => lang::t_with("recordings.confirmDeleteFolder_one", &[("name", &name), ("count", "1")]),
        n => lang::t_with("recordings.confirmDeleteFolder_other", &[("name", &name), ("count", &n.to_string())]),
    };
    let id = id.to_owned();
    confirm(ui, lang::t("recordings.confirmDeleteFolderTitle"), msg, lang::t("common.delete"), true, move |_| {
        with(|st| st.clipboard.retain(|c| *c != id));
        bridge::handle_delete_folder(&id);
    });
}

pub fn merge_selected(ui: &mut Ui) {
    let ids = with(|st| st.selection.clone());
    if ids.len() < 2 || export_blocked(&ids) {
        return;
    }
    let msg = lang::t_with("recordings.confirmMergeN", &[("count", &ids.len().to_string())]);
    confirm(ui, lang::t("recordings.confirmMergeTitle"), msg, lang::t("recordings.mergeAction"), false, move |ui| {
        with(|st| st.merge_busy = true);
        sync_bar(ui);
        bridge::handle_merge_recordings(&ids);
    });
}

pub fn delete_selected(ui: &mut Ui) {
    let ids = with(|st| st.selection.clone());
    delete_recordings(ui, ids);
}

pub fn export_selected(ui: &mut Ui) {
    let ids = with(|st| st.selection.clone());
    if ids.len() == 1 {
        crate::app_export::open_for(ui, &ids[0]);
    }
}

// ----- renomear -----

fn clean_name(s: &str) -> String {
    let s: String = s.replace(['\r', '\n'], " ").chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
    s.trim().chars().take(150).collect()
}

/// O campo do nome por cima do título do cartão (Enter grava, Esc cancela,
/// sair do campo grava).
fn begin_edit(ui: &mut Ui, id: &str, name: &str, folder: bool) {
    let Some(card) = with(|st| st.cards.get(id).copied()) else { return };
    with(|st| st.editing = Some((id.to_owned(), folder)));
    let Ok(edit) = ui.create("Edit", Some(card)) else { return };
    let _ = ui.set_prop(edit, "text", name);
    let rect = if folder { (52.0, 8.0, CARD_W - 60.0, 24.0) } else { (8.0, CARD_W * 9.0 / 16.0 + 4.0, CARD_W - 16.0, 26.0) };
    if let Some(cb) = ui.control_mut(edit) {
        cb.set_rect(rect.0, rect.1, rect.2, rect.3);
    }
    let id_owned = id.to_owned();
    ui.on(edit, "on_key_down", move |ui, sender, args| {
        let EventArgs::Key(k) = args else { return };
        match k.key {
            Key::Enter => commit_edit(ui, sender, &id_owned),
            Key::Escape => end_edit(ui, sender, false),
            _ => {}
        }
    });
    let id_owned = id.to_owned();
    ui.on(edit, "on_exit", move |ui, sender, _| {
        if with(|st| st.editing.is_some()) {
            commit_edit(ui, sender, &id_owned);
        }
    });
    ui.layout();
    ui.set_focus(edit);
    if let Some(e) = ui.get_mut::<Edit>(edit) {
        e.select_all();
    }
}

fn commit_edit(ui: &mut Ui, edit: ComponentId, id: &str) {
    let Some((eid, folder)) = with(|st| st.editing.clone()) else { return };
    if eid != id {
        return;
    }
    let text = match ui.get_prop(edit, "text") {
        Ok(PropValue::String(t)) => t,
        _ => String::new(),
    };
    let new_name = clean_name(&text);
    let original = with(|st| {
        if folder {
            st.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.clone())
        } else {
            st.lib.recordings.iter().find(|r| r.id == id).map(|r| r.name.clone())
        }
    })
    .unwrap_or_default();
    if new_name.is_empty() || new_name == original || (!folder && export_blocked(&[id.to_owned()])) {
        end_edit(ui, edit, false);
        return;
    }
    if folder {
        bridge::handle_rename_folder(id, &new_name);
    } else {
        with(|st| {
            if let Some(r) = st.lib.recordings.iter_mut().find(|r| r.id == id) {
                r.name = new_name.clone();
            }
        });
        if let Some(card) = with(|st| st.cards.get(id).copied()) {
            let _ = ui.set_prop(card, "title", new_name.as_str());
        }
        bridge::handle_rename_recording(id, &new_name);
    }
    end_edit(ui, edit, true);
}

fn end_edit(ui: &mut Ui, edit: ComponentId, renamed: bool) {
    let pending = with(|st| {
        st.editing = None;
        st.pending.take()
    });
    // O campo sai depois do evento (ele ainda está em uso agora).
    ui.set_timeout(0, move |ui| {
        if ui.contains(edit) {
            ui.free(edit);
        }
        match pending {
            Some(lib) if !renamed => {
                with(|st| st.lib = lib);
                render(ui);
            }
            Some(lib) => with(|st| st.lib = lib),
            None => {}
        }
    });
}

// ----- menu de contexto -----

fn open_context(ui: &mut Ui, card: ComponentId, kind: &str, id: &str, local: PointF) {
    let can_paste = with(|st| !st.clipboard.is_empty());
    // (texto, ação, habilitado); ação vazia = separador.
    let mut items: Vec<(String, &'static str, bool)> = Vec::new();
    match kind {
        "recording" => {
            let ids = target_ids(id);
            let bulk = ids.len() > 1;
            let busy = !crate::app_export::running_id().is_empty();
            items.push((lang::t("export.menuItem"), "export", !bulk && !busy));
            items.push((if bulk { lang::t_with("recordings.cutN", &[("count", &ids.len().to_string())]) } else { lang::t("recordings.cut") }, "cut", true));
            items.push((lang::t("recordings.transcribe"), "transcribe", true));
            items.push((String::new(), "", true));
            items.push((
                if bulk { lang::t_with("recordings.deleteNShort", &[("count", &ids.len().to_string())]) } else { lang::t("common.delete") },
                "delete",
                true,
            ));
        }
        "folder" => {
            items.push((lang::t("recordings.folderOpen"), "open", true));
            items.push((lang::t("common.rename"), "rename", true));
            items.push((lang::t("recordings.cut"), "cut-folder", true));
            items.push((lang::t("recordings.pasteInto"), "paste-into", can_paste));
            items.push((String::new(), "", true));
            items.push((lang::t("common.delete"), "delete-folder", true));
        }
        _ => {
            items.push((lang::t("recordings.newFolder"), "new-folder", true));
            items.push((lang::t("recordings.paste"), "paste", can_paste));
            if !with(|st| st.lib.at_root) {
                items.push((String::new(), "", true));
                items.push((lang::t("recordings.folderUp"), "up", true));
            }
        }
    }
    let Some(form) = main_form() else { return };
    let menu = match with(|st| st.menu).filter(|m| ui.contains(*m)) {
        Some(m) => m,
        None => {
            let Ok(m) = ui.create("PopupMenu", Some(form)) else { return };
            with(|st| st.menu = Some(m));
            m
        }
    };
    for child in ui.children(menu).to_vec() {
        ui.free(child);
    }
    for (text, action, enabled) in items {
        let Ok(item) = ui.create("MenuItem", Some(menu)) else { continue };
        let _ = ui.set_prop(item, "text", if action.is_empty() { "-".to_string() } else { text });
        let _ = ui.set_prop(item, "enabled", enabled);
        // Apagar é destrutivo: vermelho, como no original.
        let _ = ui.set_prop(item, "danger", action.starts_with("delete"));
        if !action.is_empty() {
            ui.on(item, "on_click", move |ui, _, _| ctx_run(ui, action));
        }
    }
    with(|st| st.ctx = (kind.to_owned(), id.to_owned()));
    if let Some(cb) = ui.control_mut(card) {
        cb.set_popup_menu(Some(menu));
    }
    let origin = ui.absolute_rect(card).map_or(PointF::ZERO, |r| PointF::new(r.x, r.y));
    ui.open_popup_menu(card, PointF::new(origin.x + local.x, origin.y + local.y));
}

fn ctx_run(ui: &mut Ui, action: &str) {
    let (_, id) = with(|st| st.ctx.clone());
    match action {
        "export" => crate::app_export::open_for(ui, &id),
        "cut" => cut(ui, target_ids(&id)),
        "transcribe" => bridge::handle_transcribe_recording(&id),
        "delete" => {
            let ids = target_ids(&id);
            if ids.len() > 1 {
                delete_recordings(ui, ids);
            } else {
                delete_one(ui, &id);
            }
        }
        "open" => bridge::handle_open_folder(&id),
        "rename" => {
            let name = with(|st| st.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.clone()));
            if let Some(name) = name {
                begin_edit(ui, &id, &name, true);
            }
        }
        "cut-folder" => cut(ui, vec![id]),
        "paste-into" => paste(ui, &id),
        "delete-folder" => delete_folder(ui, &id),
        "new-folder" => new_folder(ui),
        "paste" => paste(ui, ""),
        "up" => go_up(ui),
        _ => {}
    }
}

/// O fundo da biblioteca: o menu de pasta nova, colar e voltar.
pub fn background_context(ui: &mut Ui, sender: ComponentId, args: &EventArgs) {
    if let EventArgs::Pointer(e) = args {
        if e.button == PointerButton::Secondary {
            open_context(ui, sender, "empty", "", e.position);
        }
    }
}

pub fn new_folder(_ui: &mut Ui) {
    bridge::handle_create_folder(&lang::t("recordings.newFolderName"));
}

fn go_up(_ui: &mut Ui) {
    let (at_root, parent) = with(|st| (st.lib.at_root, st.lib.parent_dir.clone()));
    if !at_root {
        bridge::handle_open_folder(&parent);
    }
}

pub fn crumb(_ui: &mut Ui, args: &EventArgs) {
    let EventArgs::Value(v) = args else { return };
    let Ok(i) = v.to_i64() else { return };
    let id = with(|st| st.crumbs.get(i as usize).cloned());
    if let Some(id) = id {
        bridge::handle_open_folder(&id);
    }
}

/// A largura mudou: as grades refazem as linhas.
pub fn resized(ui: &mut Ui) {
    let cols = columns(ui);
    if with(|st| st.cols) != cols {
        render(ui);
    }
}

/// Ctrl+X / Ctrl+V / Backspace na biblioteca.
pub fn key(ui: &mut Ui, k: &KeyEvent) -> bool {
    let ctrl = k.modifiers.ctrl;
    match k.key {
        Key::Char('x') | Key::Char('X') if ctrl => {
            let ids = with(|st| st.selection.clone());
            cut(ui, ids);
            true
        }
        Key::Char('v') | Key::Char('V') if ctrl => {
            paste(ui, "");
            true
        }
        Key::Backspace => {
            go_up(ui);
            true
        }
        _ => false,
    }
}

/// Os textos fixos (troca de idioma).
pub fn apply_texts(ui: &mut Ui) {
    set(ui, "lblRecordings", "text", lang::t("recordings.title"));
    set(ui, "edLibSearch", "placeholder", lang::t("recordings.search"));
    set(ui, "btnLibOpenDir", "hint", lang::t("recordings.openFolder"));
    set(ui, "btnLibNewFolder", "hint", lang::t("recordings.newFolder"));
    let w = ui.measure_text(&lang::t("recordings.title"), &TextStyle::default()).width * 16.0 / 14.0 + 8.0;
    if let Some(cb) = c(ui, "lblRecordings").and_then(|id| ui.control_mut(id)) {
        cb.set_width(w);
    }
}

pub fn on_language_changed(ui: &mut Ui) {
    apply_texts(ui);
    render(ui);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duracao_como_a_web() {
        assert_eq!(format_duration(0), "");
        assert_eq!(format_duration(65), "01:05");
        assert_eq!(format_duration(3725), "01:02:05");
    }

    #[test]
    fn bytes_como_a_web() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "2 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5 MB");
        assert_eq!(format_bytes(1536 * 1024 * 1024), "1.5 GB");
    }

    #[test]
    fn nome_limpo() {
        assert_eq!(clean_name("  a/b:c*?\"<>|\nd  "), "abc d");
        assert_eq!(clean_name(&"x".repeat(200)).len(), 150);
    }

    #[test]
    fn periodos() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        assert_eq!(period("2026-10-23T08:00:00", today).0, "00-hoje");
        assert_eq!(period("2026-10-22T23:59:59", today).0, "01-ontem");
        assert_eq!(period("2026-10-18T10:00:00", today).0, "02-semana");
        let (k1, _) = period("2026-09-01T10:00:00", today);
        let (k2, _) = period("2025-12-01T10:00:00", today);
        assert!(k1 < k2, "mês mais novo antes");
        assert_eq!(period("lixo", today).0, "zzz-unknown");
    }
}

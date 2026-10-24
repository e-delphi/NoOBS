//! Presenter da biblioteca (`recordings.js` + `folders.js` + o menu de
//! contexto do `main.js` da versão web): recebe a lista do core pelo
//! [`View`](crate::view::View), agrupa por período, filtra pela busca, guarda a
//! seleção, o recorte e a edição de nome, e devolve os gestos ao core por
//! chamada direta.
//!
//! O estado que importa (pasta navegada, o que tem nela) NÃO é deduzido aqui:
//! chega inteiro em [`on_library`] a cada mudança. O que é só da tela — seleção,
//! recorte pendente, busca — mora aqui.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};
use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, SharedString, VecModel};

use crate::app_view::{AppWindow, Confirm, CtxItem, FolderCard, Lib, PathCrumb, RecCard, RecGroup};
use crate::view::{Library, RecItem};
use crate::{bridge, lang, log, mainthread};

/// Abaixo disso o "livres" ganha o aviso (`LOW_DISK_THRESHOLD`).
const LOW_DISK_BYTES: i64 = 5 * 1024 * 1024 * 1024;
/// Espera da busca na transcrição: cada tecla varreria o disco.
const SEARCH_DEBOUNCE_MS: u64 = 300;
/// Miniaturas decodificadas guardadas (cada uma ~400 KB na placa). Os cards
/// à vista são umas dezenas; o resto sai pela ordem de uso.
const THUMB_CACHE_MAX: usize = 120;
/// Alvo de soltar que é o botão de excluir.
const TRASH_TARGET: &str = "::trash";

#[derive(Default)]
struct State {
    lib: Library,
    /// Lista que chegou durante uma edição de nome: aplica quando ela acabar
    /// (pegadinha #56 — refazer a lista no meio mataria o campo).
    pending: Option<Library>,
    cols: i32,
    query: String,
    tr_hits: HashSet<String>,
    search_timer: Option<slint::Timer>,
    /// Em ordem de seleção: é a ordem da união.
    selection: Vec<String>,
    clipboard: Vec<String>,
    /// Id em edição e se é pasta.
    editing: Option<(String, bool)>,
    merge_busy: bool,
    drag: Vec<String>,
    /// Ids pedidos num diálogo de confirmação aberto.
    confirm_ids: Vec<String>,
    /// O que está nos modelos agora: (grupo, linha) de cada id.
    rows: HashMap<String, (usize, usize)>,
    group_models: Vec<Rc<VecModel<RecCard>>>,
    groups: Rc<VecModel<RecGroup>>,
    folders: Rc<VecModel<FolderCard>>,
    thumbs: HashMap<String, (slint::Image, u64)>,
    thumb_clock: u64,
    thumb_inflight: HashSet<String>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn window() -> Option<AppWindow> {
    crate::app_view::window()
}

fn lib_global(w: &AppWindow) -> Lib<'_> {
    w.global::<Lib>()
}

// ---------------------------------------------------------------------------
// Formatação (as funções do recordings.js)
// ---------------------------------------------------------------------------

/// `formatDuration`: `mm:ss` ou `hh:mm:ss`; vazio sem duração.
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

/// `formatBytes` (com ponto, como o `toFixed` do JS).
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

/// `formatFullDate`: a dica do card.
fn format_full_date(iso: &str) -> String {
    parse_date(iso).map(|d| d.format("%d/%m/%Y %H:%M:%S").to_string()).unwrap_or_default()
}

fn file_count_text(n: i64) -> String {
    let key = if n == 1 { "recordings.fileCount_one" } else { "recordings.fileCount_other" };
    lang::t_with(key, &[("count", &n.to_string())])
}

/// `periodKey`: (chave que ordena, rótulo). Hoje, ontem, esta semana, e daí
/// por mês.
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

// ---------------------------------------------------------------------------
// Montagem dos modelos
// ---------------------------------------------------------------------------

/// O texto em que a busca procura (o `textContent` do card na web).
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

fn build_card(it: &RecItem, st: &State) -> RecCard {
    let order = st.selection.iter().position(|x| x == &it.id).map_or(0, |i| i as i32 + 1);
    let thumb = st.thumbs.get(&it.id).map(|(img, _)| img.clone());
    let codec_hint = if it.codec.is_empty() {
        String::new()
    } else if it.codec_hw {
        lang::t_with("recordings.badge.codecHw", &[("codec", &it.codec)])
    } else {
        lang::t_with("recordings.badge.codecSw", &[("codec", &it.codec)])
    };
    let fps = it.fps.to_string();
    let level = it.quality.to_string();
    RecCard {
        id: it.id.clone().into(),
        name: it.name.clone().into(),
        hint: format_full_date(&it.date).into(),
        size_text: it.size_text.clone().into(),
        duration_text: format_duration(it.duration).into(),
        has_thumb: thumb.is_some(),
        thumb: thumb.unwrap_or_default(),
        codec: it.codec.clone().into(),
        codec_hw: it.codec_hw,
        codec_hint: codec_hint.into(),
        fps_text: if it.fps > 0 { format!("{}{}", it.fps, lang::t("recordings.badge.fpsSuffix")).into() } else { SharedString::new() },
        fps_hint: if it.fps > 0 { lang::t_with("recordings.badge.fps", &[("fps", &fps)]).into() } else { SharedString::new() },
        quality_text: if it.quality >= 0 {
            lang::t_with("recordings.badge.qualityShort", &[("level", &level)]).into()
        } else {
            SharedString::new()
        },
        quality_hint: if it.quality >= 0 { lang::t_with("recordings.badge.quality", &[("level", &level)]).into() } else { SharedString::new() },
        transcript: match it.transcript.as_str() {
            "ok" => 1,
            "empty" => 2,
            _ => 0,
        },
        selected: order > 0,
        order,
        order_hint: if order > 0 { lang::t_with("recordings.selectionOrder", &[("order", &order.to_string())]).into() } else { SharedString::new() },
        cut: st.clipboard.contains(&it.id),
    }
}

fn folder_meta(count: i64, size_text: &str) -> String {
    let mut s = file_count_text(count);
    if count > 0 && !size_text.is_empty() {
        s.push_str(" · ");
        s.push_str(size_text);
    }
    s
}

/// Linhas de cards antes de cada grupo (a posição na tela sai daí).
fn rows_before(models: &[Rc<VecModel<RecCard>>], cols: i32) -> Vec<i32> {
    let cols = cols.max(1) as usize;
    let mut acc = 0;
    models
        .iter()
        .map(|m| {
            let r = acc;
            acc += m.row_count().div_ceil(cols) as i32;
            r
        })
        .collect()
}

fn group_check(model: &VecModel<RecCard>) -> i32 {
    let n = model.row_count();
    let sel = (0..n).filter(|&i| model.row_data(i).is_some_and(|c| c.selected)).count();
    if sel == 0 {
        0
    } else if sel == n {
        2
    } else {
        1
    }
}

/// Refaz a tela inteira a partir de `lib` + busca + seleção.
pub fn render() {
    let Some(w) = window() else { return };
    let g = lib_global(&w);
    let today = Local::now().date_naive();
    let (groups, folders, crumbs, meta, low_disk, at_root, parent, empty) = with(|st| {
        // Seleção órfã (o arquivo sumiu): sem isso o clique em outro card
        // entraria em "modo seleção" em vez de tocar.
        let ids: HashSet<&str> = st.lib.recordings.iter().map(|r| r.id.as_str()).collect();
        st.selection.retain(|id| ids.contains(id.as_str()));

        let q = st.query.clone();
        let searching = !q.is_empty();
        let mut items: Vec<&RecItem> = st
            .lib
            .recordings
            .iter()
            .filter(|it| !searching || search_text(it).contains(&q) || st.tr_hits.contains(&it.id))
            .collect();
        items.sort_by(|a, b| b.date.cmp(&a.date));

        // Agrupa preservando a ordem (mais novo → mais antigo).
        let mut order: Vec<(String, String)> = Vec::new();
        let mut by_key: HashMap<String, Vec<&RecItem>> = HashMap::new();
        for it in &items {
            let (key, label) = period(&it.date, today);
            by_key.entry(key.clone()).or_default().push(it);
            if !order.iter().any(|(k, _)| k == &key) {
                order.push((key, label));
            }
        }
        order.sort_by(|a, b| a.0.cmp(&b.0));

        st.rows.clear();
        let mut models = Vec::new();
        for (gi, (key, _)) in order.iter().enumerate() {
            let cards: Vec<RecCard> = by_key[key].iter().map(|it| build_card(it, st)).collect();
            for (ri, c) in cards.iter().enumerate() {
                st.rows.insert(c.id.to_string(), (gi, ri));
            }
            models.push(Rc::new(VecModel::from(cards)));
        }
        let before = rows_before(&models, st.cols);
        let groups: Vec<RecGroup> = order
            .iter()
            .zip(&models)
            .zip(&before)
            .map(|(((key, label), m), rb)| RecGroup {
                key: key.clone().into(),
                label: label.to_uppercase().into(),
                rows_before: *rb,
                check: group_check(m),
                items: ModelRc::from(m.clone()),
            })
            .collect();
        st.group_models = models;

        // Pastas: o ".." vem primeiro fora da raiz; na busca, só as que casam
        // (e o ".." some: navegar não é o assunto).
        let mut folders = Vec::new();
        if !st.lib.at_root && !searching {
            folders.push(FolderCard {
                id: st.lib.parent_dir.clone().into(),
                name: "..".into(),
                meta: lang::t("recordings.folderUp").into(),
                hint: lang::t("recordings.folderUpHint").into(),
                up: true,
                cut: false,
            });
        }
        for f in &st.lib.folders {
            let meta = folder_meta(f.count, &f.size_text);
            if searching && !format!("{} {}", f.name, meta).to_lowercase().contains(&q) {
                continue;
            }
            folders.push(FolderCard {
                id: f.id.clone().into(),
                name: f.name.clone().into(),
                meta: meta.into(),
                hint: f.id.clone().into(),
                up: false,
                cut: st.clipboard.contains(&f.id),
            });
        }

        let n = st.lib.breadcrumb.len();
        let crumbs: Vec<PathCrumb> = st
            .lib
            .breadcrumb
            .iter()
            .enumerate()
            .map(|(i, c)| PathCrumb {
                // A raiz é a pasta das Configurações: o nome real dela ("Vídeos")
                // confundiria com uma subpasta qualquer.
                label: if c.root { lang::t("recordings.libraryRoot").into() } else { c.name.clone().into() },
                hint: c.id.clone().into(),
                current: i + 1 == n,
                id: c.id.clone().into(),
            })
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
        (groups, folders, crumbs, meta, free >= 0 && free < LOW_DISK_BYTES, st.lib.at_root, st.lib.parent_dir.clone(), empty)
    });

    let groups_model = with(|st| {
        st.groups = Rc::new(VecModel::from(groups));
        st.folders = Rc::new(VecModel::from(folders));
        (st.groups.clone(), st.folders.clone())
    });
    g.set_groups(ModelRc::from(groups_model.0));
    g.set_folders(ModelRc::from(groups_model.1));
    g.set_crumbs(ModelRc::from(Rc::new(VecModel::from(crumbs))));
    let meta: Vec<SharedString> = meta.into_iter().map(Into::into).collect();
    g.set_meta_parts(ModelRc::from(Rc::new(VecModel::from(meta))));
    g.set_low_disk(low_disk);
    g.set_at_root(at_root);
    g.set_parent_dir(parent.into());
    g.set_empty(empty);
    sync_bar(&w);
}

/// Botões da barra (`_syncMode`): excluir/unir/exportar conforme a seleção.
fn sync_bar(w: &AppWindow) {
    let g = lib_global(w);
    let (n, busy) = with(|st| (st.selection.len(), st.merge_busy));
    g.set_sel_count(n as i32);
    g.set_merge_busy(busy);
    g.set_delete_hint(
        match n {
            0 => lang::t("recordings.selectToDelete"),
            1 => lang::t("recordings.deleteOne"),
            _ => lang::t_with("recordings.deleteN", &[("count", &n.to_string())]),
        }
        .into(),
    );
    g.set_merge_hint(
        if n < 2 { lang::t("recordings.selectToMerge") } else { lang::t_with("recordings.mergeN", &[("count", &n.to_string())]) }.into(),
    );
    // A exportação entra na fase 5 (docs/ui-slint.md): até lá o botão fica
    // apagado.
    g.set_export_enabled(false);
    g.set_export_hint(if n == 1 { lang::t("export.buttonHint") } else { lang::t("export.selectOne") }.into());
}

/// Só as marcas de seleção/recorte, sem refazer os modelos (refazer recriaria
/// todos os cards à toa a cada clique).
fn refresh_marks() {
    let Some(w) = window() else { return };
    with(|st| {
        for (gi, m) in st.group_models.iter().enumerate() {
            for i in 0..m.row_count() {
                let Some(mut c) = m.row_data(i) else { continue };
                let id = c.id.to_string();
                let order = st.selection.iter().position(|x| x == &id).map_or(0, |p| p as i32 + 1);
                let cut = st.clipboard.contains(&id);
                if c.order != order || c.cut != cut {
                    c.order = order;
                    c.selected = order > 0;
                    c.order_hint =
                        if order > 0 { lang::t_with("recordings.selectionOrder", &[("order", &order.to_string())]).into() } else { SharedString::new() };
                    c.cut = cut;
                    m.set_row_data(i, c);
                }
            }
            if let Some(mut gr) = st.groups.row_data(gi) {
                let check = group_check(m);
                if gr.check != check {
                    gr.check = check;
                    st.groups.set_row_data(gi, gr);
                }
            }
        }
        for i in 0..st.folders.row_count() {
            if let Some(mut f) = st.folders.row_data(i) {
                let cut = !f.up && st.clipboard.iter().any(|c| c.as_str() == f.id.as_str());
                if f.cut != cut {
                    f.cut = cut;
                    st.folders.set_row_data(i, f);
                }
            }
        }
    });
    sync_bar(&w);
}

fn on_cols_changed(cols: i32) {
    with(|st| {
        st.cols = cols.max(1);
        let before = rows_before(&st.group_models, st.cols);
        for (i, rb) in before.into_iter().enumerate() {
            if let Some(mut g) = st.groups.row_data(i) {
                if g.rows_before != rb {
                    g.rows_before = rb;
                    st.groups.set_row_data(i, g);
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Miniaturas: JPG do cache decodificado numa worker, guardado por ordem de uso
// ---------------------------------------------------------------------------

struct ThumbQueue {
    jobs: Mutex<VecDeque<(String, String)>>,
    cv: Condvar,
}

fn thumb_queue() -> &'static ThumbQueue {
    static Q: OnceLock<ThumbQueue> = OnceLock::new();
    Q.get_or_init(|| {
        std::thread::spawn(thumb_worker);
        ThumbQueue { jobs: Mutex::new(VecDeque::new()), cv: Condvar::new() }
    })
}

fn thumb_worker() {
    let q = thumb_queue();
    loop {
        let job = {
            let mut jobs = q.jobs.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                // O último pedido primeiro: é o card que acabou de aparecer.
                if let Some(j) = jobs.pop_back() {
                    break j;
                }
                jobs = q.cv.wait(jobs).unwrap_or_else(|e| e.into_inner());
            }
        };
        let (id, path) = job;
        let decoded = std::fs::read(&path).ok().and_then(|bytes| {
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).ok().map(|img| img.to_rgb8())
        });
        let buf = decoded.map(|rgb| {
            let (w, h) = rgb.dimensions();
            SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(rgb.as_raw(), w, h)
        });
        mainthread::queue(move || on_thumb_decoded(id, buf));
    }
}

fn request_thumb(id: &str) {
    let path = with(|st| {
        if st.thumb_inflight.contains(id) {
            return None;
        }
        let p = st.lib.recordings.iter().find(|r| r.id == id).map(|r| r.thumb.clone()).filter(|p| !p.is_empty())?;
        st.thumb_inflight.insert(id.to_owned());
        Some(p)
    });
    if let Some(p) = path {
        let q = thumb_queue();
        q.jobs.lock().unwrap_or_else(|e| e.into_inner()).push_back((id.to_owned(), p));
        q.cv.notify_one();
    }
}

/// O card ficou visível sem miniatura.
fn on_need_thumb(id: &str) {
    let hit = with(|st| {
        st.thumb_clock += 1;
        let clock = st.thumb_clock;
        st.thumbs.get_mut(id).map(|e| {
            e.1 = clock;
            e.0.clone()
        })
    });
    match hit {
        Some(img) => set_card_thumb(id, Some(img)),
        None => request_thumb(id),
    }
}

fn on_thumb_decoded(id: String, buf: Option<SharedPixelBuffer<slint::Rgb8Pixel>>) {
    with(|st| st.thumb_inflight.remove(&id));
    let Some(buf) = buf else {
        log!("Biblioteca: miniatura ilegivel ({id})");
        return;
    };
    let img = slint::Image::from_rgb8(buf);
    let evicted = with(|st| {
        st.thumb_clock += 1;
        let clock = st.thumb_clock;
        st.thumbs.insert(id.clone(), (img.clone(), clock));
        let mut out = Vec::new();
        while st.thumbs.len() > THUMB_CACHE_MAX {
            let Some(old) = st.thumbs.iter().min_by_key(|(_, (_, c))| *c).map(|(k, _)| k.clone()) else { break };
            st.thumbs.remove(&old);
            out.push(old);
        }
        out
    });
    // A imagem sai do card também, senão a memória fica presa no modelo.
    for old in evicted {
        set_card_thumb(&old, None);
    }
    set_card_thumb(&id, Some(img));
}

fn set_card_thumb(id: &str, img: Option<slint::Image>) {
    with(|st| {
        let Some(&(gi, ri)) = st.rows.get(id) else { return };
        let Some(m) = st.group_models.get(gi) else { return };
        if let Some(mut c) = m.row_data(ri) {
            c.has_thumb = img.is_some();
            c.thumb = img.unwrap_or_default();
            m.set_row_data(ri, c);
        }
    });
}

// ---------------------------------------------------------------------------
// Do core (View)
// ---------------------------------------------------------------------------

pub fn on_library(lib: &Library) {
    let editing = with(|st| {
        if st.editing.is_some() {
            st.pending = Some(lib.clone());
            true
        } else {
            st.lib = lib.clone();
            false
        }
    });
    if !editing {
        render();
    }
}

pub fn on_recording_added(item: &RecItem, free: i64) {
    with(|st| {
        // Nunca dois cards pro mesmo id (o watcher pode ter listado antes).
        if !st.lib.recordings.iter().any(|r| r.id == item.id) {
            st.lib.recordings.push(item.clone());
        }
        st.lib.free_bytes = free;
    });
    render();
}

pub fn on_recording_meta(id: &str, duration: i32, thumb: &str) {
    let found = with(|st| {
        let Some(r) = st.lib.recordings.iter_mut().find(|r| r.id == id) else { return false };
        if duration > 0 {
            r.duration = duration;
        }
        if !thumb.is_empty() {
            r.thumb = thumb.to_owned();
            // Miniatura nova (regerada): a decodificada antes não vale mais.
            st.thumbs.remove(id);
        }
        true
    });
    if !found {
        return;
    }
    with(|st| {
        let Some(&(gi, ri)) = st.rows.get(id) else { return };
        if let Some(m) = st.group_models.get(gi) {
            if let Some(mut c) = m.row_data(ri) {
                c.duration_text = format_duration(duration).into();
                m.set_row_data(ri, c);
            }
        }
    });
    if !thumb.is_empty() {
        request_thumb(id);
    }
}

pub fn on_recording_renamed(old_id: &str, new_id: &str, new_name: &str) {
    with(|st| {
        if let Some(r) = st.lib.recordings.iter_mut().find(|r| r.id == old_id) {
            r.id = new_id.to_owned();
            r.name = new_name.to_owned();
        }
        for s in st.selection.iter_mut().chain(st.clipboard.iter_mut()) {
            if s == old_id {
                *s = new_id.to_owned();
            }
        }
        if let Some(t) = st.thumbs.remove(old_id) {
            st.thumbs.insert(new_id.to_owned(), t);
        }
    });
    render();
}

pub fn on_recording_removed(id: &str, free: i64) {
    with(|st| {
        st.lib.recordings.retain(|r| r.id != id);
        st.selection.retain(|s| s != id);
        st.thumbs.remove(id);
        if free >= 0 {
            st.lib.free_bytes = free;
        }
    });
    render();
}

/// A pasta criada pela interface já está na lista: entra na edição do nome
/// (o gesto do Explorer).
pub fn on_folder_created(id: &str) {
    let name = with(|st| st.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.clone()));
    if let Some(name) = name {
        begin_edit(id, &name, true);
    }
}

pub fn on_transcript_search(query: &str, ids: &[String]) {
    // Resposta atrasada de uma busca que o usuário já abandonou: ignora.
    let current = with(|st| {
        if query.to_lowercase().trim() != st.query {
            return false;
        }
        st.tr_hits = ids.iter().cloned().collect();
        true
    });
    if current {
        render();
    }
}

pub fn on_merge_pending() {
    with(|st| st.merge_busy = true);
    if let Some(w) = window() {
        sync_bar(&w);
    }
    crate::app_view::show_toast(&lang::t("toast.mergeStarted"), &lang::t("toast.mergeStartedMsg"), false, 3000);
}

pub fn on_merge_done(ok: bool) {
    with(|st| {
        st.merge_busy = false;
        if ok {
            st.selection.clear();
        }
    });
    refresh_marks();
    if ok {
        crate::app_view::show_toast(&lang::t("toast.mergeDone"), &lang::t("toast.mergeDoneMsg"), false, 5000);
    }
}

// ---------------------------------------------------------------------------
// Gestos
// ---------------------------------------------------------------------------

/// Ids sobre os quais uma ação deve agir: a seleção inteira quando o item faz
/// parte dela, senão só ele (`targetIds`).
fn target_ids(id: &str) -> Vec<String> {
    with(|st| {
        if st.selection.len() > 1 && st.selection.iter().any(|s| s == id) {
            st.selection.clone()
        } else {
            vec![id.to_owned()]
        }
    })
}

fn is_folder(id: &str) -> bool {
    with(|st| st.lib.folders.iter().any(|f| f.id.eq_ignore_ascii_case(id)))
}

/// Algum dos arquivos está sendo exportado: mover/excluir/renomear agora
/// tiraria o arquivo de baixo da exportação.
fn export_blocked(ids: &[String]) -> bool {
    if ids.iter().any(|id| bridge::export_in_use(id)) {
        crate::app_view::show_toast(&lang::t("toast.errorTitle"), &lang::t("error.exportBusyFile"), true, 6000);
        return true;
    }
    false
}

fn toggle_select(id: &str) {
    with(|st| {
        if let Some(p) = st.selection.iter().position(|s| s == id) {
            st.selection.remove(p);
        } else {
            st.selection.push(id.to_owned());
        }
    });
    refresh_marks();
}

fn group_toggle(key: &str) {
    with(|st| {
        let Some(gi) = (0..st.groups.row_count()).find(|&i| st.groups.row_data(i).is_some_and(|g| g.key.as_str() == key)) else {
            return;
        };
        let m = st.group_models[gi].clone();
        let ids: Vec<String> = (0..m.row_count()).filter_map(|i| m.row_data(i)).map(|c| c.id.to_string()).collect();
        let all = group_check(&m) == 2;
        for id in ids {
            let has = st.selection.contains(&id);
            if all && has {
                st.selection.retain(|s| s != &id);
            } else if !all && !has {
                st.selection.push(id);
            }
        }
    });
    refresh_marks();
}

fn card_clicked(id: &str) {
    focus_library();
    crate::app_player::open(id);
}

fn focus_library() {
    if let Some(w) = window() {
        let g = lib_global(&w);
        g.set_focus_seq(g.get_focus_seq() + 1);
    }
}

fn search(text: &str) {
    let q = text.to_lowercase().trim().to_owned();
    let changed = with(|st| {
        if st.query == q {
            return false;
        }
        st.query = q.clone();
        st.tr_hits.clear();
        st.search_timer = None;
        true
    });
    if !changed {
        return;
    }
    if q.is_empty() {
        bridge::handle_search_transcripts("");
    } else {
        let timer = slint::Timer::default();
        let q2 = q.clone();
        timer.start(slint::TimerMode::SingleShot, Duration::from_millis(SEARCH_DEBOUNCE_MS), move || {
            bridge::handle_search_transcripts(&q2);
        });
        with(|st| st.search_timer = Some(timer));
    }
    render();
}

// ---- recortar / colar ----

fn cut(ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    let n = ids.len();
    with(|st| st.clipboard = ids);
    refresh_marks();
    let msg = if n == 1 { lang::t("toast.cutOne") } else { lang::t_with("toast.cutN", &[("count", &n.to_string())]) };
    crate::app_view::show_toast(&lang::t("toast.cutTitle"), &msg, false, 3500);
}

/// `target` vazio = a pasta aberta ("Colar aqui").
fn paste(target: &str) {
    let ids = with(|st| st.clipboard.clone());
    if ids.is_empty() || export_blocked(&ids) {
        return;
    }
    with(|st| st.clipboard.clear());
    refresh_marks();
    bridge::handle_move_items(&ids, target);
}

// ---- excluir / unir ----

fn confirm(title: String, message: String, ok: String, danger: bool, icon: &str, action: &str, ids: Vec<String>) {
    let Some(w) = window() else { return };
    with(|st| st.confirm_ids = ids);
    w.global::<Confirm>().invoke_open(title.into(), message.into(), ok.into(), danger, icon.into(), action.into(), SharedString::new());
}

/// Pergunta e exclui gravações (o botão da barra, o menu e soltar na lixeira).
fn delete_recordings(ids: Vec<String>) {
    let ids: Vec<String> = ids.into_iter().filter(|s| !s.is_empty()).collect();
    if ids.is_empty() || export_blocked(&ids) {
        return;
    }
    let msg = if ids.len() == 1 {
        lang::t("recordings.confirmDeleteOne")
    } else {
        lang::t_with("recordings.confirmDeleteN", &[("count", &ids.len().to_string())])
    };
    confirm(lang::t("recordings.confirmDeleteTitle"), msg, lang::t("common.delete"), true, "delete", "lib-delete", ids);
}

fn delete_one(id: &str) {
    if export_blocked(&[id.to_owned()]) {
        return;
    }
    let name = with(|st| st.lib.recordings.iter().find(|r| r.id == id).map(|r| r.name.clone())).unwrap_or_else(|| id.to_owned());
    confirm(
        lang::t("recordings.confirmDeleteSingleTitle"),
        lang::t_with("recordings.confirmDeleteNamed", &[("name", &name)]),
        lang::t("common.delete"),
        true,
        "delete",
        "lib-delete",
        vec![id.to_owned()],
    );
}

fn delete_folder(id: &str) {
    let (name, count) = with(|st| {
        st.lib.folders.iter().find(|f| f.id == id).map(|f| (f.name.clone(), f.count)).unwrap_or((id.to_owned(), 0))
    });
    // Pasta com conteúdo é o caso que precisa de aviso: o que vai pra Lixeira
    // são N gravações junto com ela.
    let msg = match count {
        0 => lang::t_with("recordings.confirmDeleteFolderEmpty", &[("name", &name)]),
        1 => lang::t_with("recordings.confirmDeleteFolder_one", &[("name", &name), ("count", "1")]),
        n => lang::t_with("recordings.confirmDeleteFolder_other", &[("name", &name), ("count", &n.to_string())]),
    };
    confirm(lang::t("recordings.confirmDeleteFolderTitle"), msg, lang::t("common.delete"), true, "delete", "lib-delete-folder", vec![id.to_owned()]);
}

fn merge_selected() {
    let ids = with(|st| st.selection.clone());
    if ids.len() < 2 || export_blocked(&ids) {
        return;
    }
    let msg = lang::t_with("recordings.confirmMergeN", &[("count", &ids.len().to_string())]);
    confirm(lang::t("recordings.confirmMergeTitle"), msg, lang::t("recordings.mergeAction"), false, "merge", "lib-merge", ids);
}

/// O "ok" de um diálogo aberto daqui. Devolve `false` se a ação não é da
/// biblioteca.
pub fn on_confirm(action: &str) -> bool {
    let ids = with(|st| std::mem::take(&mut st.confirm_ids));
    match action {
        "lib-delete" => {
            // Otimista: o card sai agora (o core exclui um por vez).
            with(|st| {
                st.lib.recordings.retain(|r| !ids.contains(&r.id));
                st.selection.retain(|s| !ids.contains(s));
            });
            render();
            for id in &ids {
                bridge::handle_delete_recording(id);
            }
        }
        "lib-delete-folder" => {
            with(|st| st.clipboard.retain(|c| !ids.contains(c)));
            for id in &ids {
                bridge::handle_delete_folder(id);
            }
        }
        "lib-merge" => {
            with(|st| st.merge_busy = true);
            if let Some(w) = window() {
                sync_bar(&w);
            }
            bridge::handle_merge_recordings(&ids);
        }
        _ => return false,
    }
    true
}

// ---- renomear no próprio card ----

fn begin_edit(id: &str, name: &str, folder: bool) {
    let Some(w) = window() else { return };
    with(|st| st.editing = Some((id.to_owned(), folder)));
    let g = lib_global(&w);
    g.set_editing_text(name.into());
    g.set_editing_id(id.into());
}

fn rename_recording(id: &str) {
    let name = with(|st| st.lib.recordings.iter().find(|r| r.id == id).map(|r| r.name.clone()));
    if let Some(name) = name {
        begin_edit(id, &name, false);
    }
}

fn rename_folder(id: &str) {
    let name = with(|st| st.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.clone()));
    if let Some(name) = name {
        begin_edit(id, &name, true);
    }
}

/// Sanitiza como a web: sem quebras nem `\ / : * ? " < > |`, até 150.
fn clean_name(s: &str) -> String {
    let s: String = s.replace(['\r', '\n'], " ").chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
    s.trim().chars().take(150).collect()
}

/// Fim da edição. `renamed` = mandou renomear: o core vai empurrar a lista
/// nova, e a adiada é descartada (mostraria o nome antigo por um instante).
fn end_edit(renamed: bool) {
    let pending = with(|st| {
        st.editing = None;
        st.pending.take()
    });
    if let Some(w) = window() {
        lib_global(&w).set_editing_id(SharedString::new());
    }
    match pending {
        Some(lib) if !renamed => {
            with(|st| st.lib = lib);
            render();
        }
        Some(lib) => with(|st| st.lib = lib),
        None => {}
    }
}

fn rename_commit(id: &str, text: &str) {
    let Some((eid, folder)) = with(|st| st.editing.clone()) else { return };
    if eid != id {
        return;
    }
    let new_name = clean_name(text);
    let original = with(|st| {
        if folder {
            st.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.clone())
        } else {
            st.lib.recordings.iter().find(|r| r.id == id).map(|r| r.name.clone())
        }
    })
    .unwrap_or_default();
    if new_name.is_empty() || new_name == original || (!folder && export_blocked(&[id.to_owned()])) {
        end_edit(false);
        return;
    }
    if folder {
        bridge::handle_rename_folder(id, &new_name);
    } else {
        // O nome novo aparece já (o core confirma com `recording_renamed`).
        with(|st| {
            if let Some(r) = st.lib.recordings.iter_mut().find(|r| r.id == id) {
                r.name = new_name.clone();
            }
            if let Some(&(gi, ri)) = st.rows.get(id) {
                if let Some(m) = st.group_models.get(gi) {
                    if let Some(mut c) = m.row_data(ri) {
                        c.name = new_name.clone().into();
                        m.set_row_data(ri, c);
                    }
                }
            }
        });
        bridge::handle_rename_recording(id, &new_name);
    }
    end_edit(true);
}

// ---- arrastar ----

fn drag_start(id: &str) {
    let Some(w) = window() else { return };
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
        st.drag = ids.clone();
        if ids.len() > 1 {
            format!("{first} +{}", ids.len() - 1)
        } else {
            first
        }
    });
    let g = lib_global(&w);
    g.set_drop_target(SharedString::new());
    g.set_drag_label(label.into());
    g.set_dragging(true);
}

/// `_canDrop`: nem a própria pasta, nem pasta dentro dela mesma, nem onde já está.
fn can_drop(target: &str) -> bool {
    let t = target.to_lowercase();
    if t.is_empty() {
        return false;
    }
    if t == TRASH_TARGET {
        return true;
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

fn drag_end(dropped: bool) {
    let Some(w) = window() else { return };
    let g = lib_global(&w);
    let target = g.get_drop_target().to_string();
    g.set_dragging(false);
    g.set_drop_target(SharedString::new());
    let ids = with(|st| std::mem::take(&mut st.drag));
    if !dropped || target.is_empty() || ids.is_empty() {
        return;
    }
    if target == TRASH_TARGET {
        // Pasta tem confirmação própria (avisa quantas gravações vão junto);
        // arrastar uma pasta leva só ela.
        if ids.len() == 1 && is_folder(&ids[0]) {
            delete_folder(&ids[0]);
        } else {
            delete_recordings(ids.into_iter().filter(|x| !is_folder(x)).collect());
        }
        return;
    }
    if export_blocked(&ids) {
        return;
    }
    // Solto: a seleção não descreve mais o que está na tela.
    with(|st| st.selection.clear());
    refresh_marks();
    bridge::handle_move_items(&ids, &target);
}

// ---- menu de contexto ----

fn item(label: String, action: &str) -> CtxItem {
    CtxItem { label: label.into(), action: action.into(), ..Default::default() }
}

fn sep() -> CtxItem {
    CtxItem { sep: true, ..Default::default() }
}

thread_local! {
    /// Alvo do menu aberto: (tipo, id).
    static CTX: RefCell<(String, String)> = RefCell::new((String::new(), String::new()));
}

fn open_context(kind: &str, id: &str, x: f32, y: f32) {
    let Some(w) = window() else { return };
    let can_paste = with(|st| !st.clipboard.is_empty());
    let paste_hint = if can_paste { String::new() } else { lang::t("recordings.nothingCut") };
    let mut items = Vec::new();
    match kind {
        "recording" => {
            let ids = target_ids(id);
            let bulk = ids.len() > 1;
            // Exportar opera sobre UMA gravação; a tela entra na fase 5.
            items.push(CtxItem { disabled: true, hint: lang::t("export.selectOne").into(), ..item(lang::t("export.menuItem"), "export") });
            items.push(item(
                if bulk { lang::t_with("recordings.cutN", &[("count", &ids.len().to_string())]) } else { lang::t("recordings.cut") },
                "cut",
            ));
            // Transcrever vale sempre pra gravação CLICADA.
            items.push(item(lang::t("recordings.transcribe"), "transcribe"));
            items.push(sep());
            items.push(CtxItem {
                danger: true,
                ..item(
                    if bulk { lang::t_with("recordings.deleteNShort", &[("count", &ids.len().to_string())]) } else { lang::t("common.delete") },
                    "delete",
                )
            });
        }
        "folder" => {
            items.push(item(lang::t("recordings.folderOpen"), "open"));
            items.push(item(lang::t("common.rename"), "rename"));
            items.push(item(lang::t("recordings.cut"), "cut-folder"));
            items.push(CtxItem { disabled: !can_paste, hint: paste_hint.clone().into(), ..item(lang::t("recordings.pasteInto"), "paste-into") });
            items.push(sep());
            items.push(CtxItem { danger: true, ..item(lang::t("common.delete"), "delete-folder") });
        }
        _ => {
            items.push(item(lang::t("recordings.newFolder"), "new-folder"));
            items.push(CtxItem { disabled: !can_paste, hint: paste_hint.into(), ..item(lang::t("recordings.paste"), "paste") });
            if !with(|st| st.lib.at_root) {
                items.push(sep());
                items.push(item(lang::t("recordings.folderUp"), "up"));
            }
        }
    }
    CTX.with(|c| *c.borrow_mut() = (kind.to_owned(), id.to_owned()));
    crate::app_main::set_hint_layer(&w, 1);
    let g = lib_global(&w);
    g.set_ctx_items(ModelRc::from(Rc::new(VecModel::from(items))));
    g.set_ctx_x(x);
    g.set_ctx_y(y);
    g.set_ctx_open(true);
}

fn close_context() {
    if let Some(w) = window() {
        lib_global(&w).set_ctx_open(false);
        crate::app_main::set_hint_layer(&w, 0);
    }
}

fn ctx_run(action: &str) {
    close_context();
    let (_, id) = CTX.with(|c| c.borrow().clone());
    match action {
        "cut" => cut(target_ids(&id)),
        "transcribe" => bridge::handle_transcribe_recording(&id),
        "delete" => {
            let ids = target_ids(&id);
            if ids.len() > 1 {
                delete_recordings(ids);
            } else {
                delete_one(&id);
            }
        }
        "open" => bridge::handle_open_folder(&id),
        "rename" => rename_folder(&id),
        "cut-folder" => cut(vec![id]),
        "paste-into" => paste(&id),
        "delete-folder" => delete_folder(&id),
        "new-folder" => new_folder(),
        "paste" => paste(""),
        "up" => go_up(),
        _ => {}
    }
}

fn new_folder() {
    // O core cria com o nome padrão (sem repetir) e responde `folder_created`;
    // aí o card entra em edição.
    bridge::handle_create_folder(&lang::t("recordings.newFolderName"));
}

fn go_up() {
    let (at_root, parent) = with(|st| (st.lib.at_root, st.lib.parent_dir.clone()));
    if !at_root {
        bridge::handle_open_folder(&parent);
    }
}

// ---------------------------------------------------------------------------
// Ligação
// ---------------------------------------------------------------------------

pub fn setup(w: &AppWindow) {
    let g = lib_global(w);
    g.on_cols_changed(on_cols_changed);
    g.on_search(|t| search(t.as_str()));
    g.on_open_record_dir(bridge::handle_open_record_dir);
    g.on_new_folder(new_folder);
    g.on_merge_selected(merge_selected);
    g.on_export_selected(|| {});
    g.on_delete_selected(|| delete_recordings(with(|st| st.selection.clone())));
    g.on_card_clicked(|id| card_clicked(id.as_str()));
    g.on_toggle_select(|id| {
        focus_library();
        toggle_select(id.as_str());
    });
    g.on_group_toggle(|k| group_toggle(k.as_str()));
    g.on_context(|kind, id, x, y| open_context(kind.as_str(), id.as_str(), x, y));
    g.on_ctx_run(|a| ctx_run(a.as_str()));
    g.on_ctx_close(close_context);
    g.on_open_folder(|id| bridge::handle_open_folder(id.as_str()));
    g.on_go_up(go_up);
    g.on_rename_recording(|id| rename_recording(id.as_str()));
    g.on_rename_commit(|id, t| rename_commit(id.as_str(), t.as_str()));
    g.on_rename_cancel(|| end_edit(false));
    g.on_drag_start(|id| drag_start(id.as_str()));
    g.on_drag_end(drag_end);
    g.on_can_drop(|t| can_drop(t.as_str()));
    g.on_need_thumb(|id| on_need_thumb(id.as_str()));
    g.on_key_cut(|| {
        let ids = with(|st| st.selection.clone());
        cut(ids);
    });
    g.on_key_paste(|| paste(""));
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

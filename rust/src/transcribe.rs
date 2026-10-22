//! Fila de transcrição contra o motor LOCAL ([`crate::local_asr`]: Qwen3 + audio.cpp na
//! placa de vídeo). Porte de `OBSTranscribe.pas` — que também falava com a Transcritor API
//! (um container com faster-whisper); essa opção saiu, só o motor local ficou.
//!
//! Desenho em pontos que não são óbvios:
//!
//! 1. TRANSCREVE O ÁUDIO, NUNCA O VÍDEO. O áudio sai em m4a por
//!    [`ffmpeg_ops::extract_audio_tracks`]. E vão as faixas ISOLADAS, uma transcrição por
//!    faixa, não a mistura: cada faixa é um dispositivo, com o nome dele escrito no MKV — a
//!    atribuição de falante sai de graça (o motor local não separa falantes). Custa N vezes
//!    mais tempo, então só vale com 2+ isoladas; faixa muda nem é transcrita
//!    ([`plan_tracks`]). O preço é o ECO: o que sai pelos alto-falantes volta pelo
//!    microfone ([`merge`]).
//! 2. UMA POR VEZ. O motor carrega os modelos na placa uma vez só. Uma thread, uma fila — e
//!    o cancelamento corta até o bloco em voo ([`local_asr::abort_request`]).
//! 3. MOTOR NÃO INSTALADO NÃO É FALHA: o item espera na fila (persistida) e anda quando a
//!    instalação termina.
//!
//! O resultado vai para dois arquivos no cache: `<hash>.transcript.json` (resposta inteira)
//! e `<hash>.txt` (só o texto puro, para a BUSCA — carregar o JSON inteiro de cada gravação
//! para filtrar a lista seria vários MB por item).

pub mod merge;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{config, ffmpeg_ops, file_meta, lang, local_asr, log, mainthread, player, probe};

/// Faixa cujo pico nunca passa disso não é transcrita: é o microfone que ninguém usou.
/// Transcrever assim só gastaria minutos de fila para receber zero turnos.
const SILENCE_PEAK_MIN: f32 = 0.02;
const SILENCE_BUCKETS: usize = 400;

/// Entre olhadas com o motor ainda não instalado (o fim da instalação acorda antes).
const INSTALL_RETRY_MS: u64 = 10_000;

const QUEUE_FILE: &str = "transcribe-queue.json";

/// Por que um item parou. Os dois primeiros NÃO são falha: cancelamento do usuário, e
/// motor não instalado (o item volta para a frente da espera e anda quando a instalação
/// terminar).
#[derive(Debug)]
enum Stop {
    Canceled,
    LocalMissing,
    Msg(String),
}

type Res<T = ()> = Result<T, Stop>;

fn fail<T>(m: impl Into<String>) -> Res<T> {
    Err(Stop::Msg(m.into()))
}

// ---------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Inner {
    queue: Vec<String>,
    /// Caminhos (minúsculos) pedidos A MÃO — rodam mesmo com a fila pausada. Um conjunto à
    /// parte em vez de mudar o tipo da fila: a marca só interessa a quem escolhe o próximo
    /// item. Sai daqui quando o item termina de verdade (não quando a worker o pega:
    /// motor não instalado devolve o item para a fila, e ele tem que voltar ainda manual).
    manual: HashSet<String>,
    running: bool,
    current_name: String,
    current_path: String,
    current_start: Option<Instant>,
    stage: String,
    /// -1 = ainda sem número
    progress: f64,
    /// -1 = ainda sem estimativa
    eta: i32,
    cancel_current: bool,
    done: i32,
    failed: i32,
    batch_total: i32,
    last_error: String,
    /// Nome da gravação que falhou. Sem ele, "1 com falha" num lote de 7 não diz QUAL.
    last_error_name: String,
    /// Sobe a cada mudança de COMPOSIÇÃO da fila.
    queue_rev: i32,
    track: i32,
    track_count: i32,
    /// Fila parada esperando a instalação do motor local.
    waiting_install: bool,
    /// Fila segurada pelo Bridge (gravação ou buffer ligado).
    paused: bool,
    /// O item em curso JÁ terminou e só falta a próxima volta do laço tirá-lo do estado.
    /// Sem esta marca, um fechamento nesse intervalo persistiria o item concluído de novo.
    current_finished: bool,
}

fn inner() -> &'static Mutex<Inner> {
    static I: OnceLock<Mutex<Inner>> = OnceLock::new();
    I.get_or_init(|| Mutex::new(Inner { progress: -1.0, eta: -1, track_count: 1, ..Default::default() }))
}

fn with<R>(f: impl FnOnce(&mut Inner) -> R) -> R {
    f(&mut inner().lock().unwrap())
}

/// Sinais da worker: `stop` (manual-reset, ligado no fechamento) e `wake` (auto-reset, o
/// `NudgeQueue`).
struct Signals {
    state: Mutex<(bool, bool)>,
    cv: Condvar,
}

fn signals() -> &'static Signals {
    static S: OnceLock<Signals> = OnceLock::new();
    S.get_or_init(|| Signals { state: Mutex::new((false, false)), cv: Condvar::new() })
}

fn stop_set() -> bool {
    signals().state.lock().unwrap().0
}

/// Espera até `ms`, só pelo `stop`. `true` = fechamento.
fn wait_stop(ms: u64) -> bool {
    let s = signals();
    let deadline = Instant::now() + Duration::from_millis(ms);
    let mut g = s.state.lock().unwrap();
    while !g.0 {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        g = s.cv.wait_timeout(g, deadline - now).unwrap().0;
    }
    true
}

/// Espera até `ms` por `stop` OU `wake` (consome o `wake`). `true` = fechamento.
fn wait_stop_or_wake(ms: u64) -> bool {
    let s = signals();
    let deadline = Instant::now() + Duration::from_millis(ms);
    let mut g = s.state.lock().unwrap();
    loop {
        if g.0 {
            return true;
        }
        if g.1 {
            g.1 = false;
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        g = s.cv.wait_timeout(g, deadline - now).unwrap().0;
    }
}

/// Acorda a worker que espera a instalação, sem aguardar o próximo ciclo.
pub fn nudge_queue() {
    let s = signals();
    s.state.lock().unwrap().1 = true;
    s.cv.notify_all();
}

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
type Changed = Arc<dyn Fn() + Send + Sync>;
static ON_CHANGED: Mutex<Option<Changed>> = Mutex::new(None);
static SAVE_LOCK: Mutex<()> = Mutex::new(());
static WORKER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Callback de mudança de estado — a UI redesenha a partir daqui. Chamado SEMPRE na main
/// thread (o módulo faz o marshalling).
pub fn set_on_changed(cb: impl Fn() + Send + Sync + 'static) {
    *ON_CHANGED.lock().unwrap() = Some(Arc::new(cb));
}

/// Sempre marshalla para a main: a UI só pode ser tocada de lá, e este callback é chamado
/// da worker em quase todos os pontos.
fn notify_changed() {
    mainthread::queue(|| {
        let cb = ON_CHANGED.lock().unwrap().clone();
        if let Some(cb) = cb {
            cb();
        }
    });
}

fn shutting_down() -> bool {
    stop_set()
}

fn cancel_requested() -> bool {
    with(|i| i.cancel_current)
}

// ---------------------------------------------------------------------------
// Leitura do estado (para a UI)
// ---------------------------------------------------------------------------

pub fn is_running() -> bool {
    with(|i| i.running)
}

/// Cancelamento pedido e o item em curso ainda não parou. A UI trava o botão e diz
/// "Cancelando…" enquanto isto for `true` — sem isso o clique parecia não ter pegado.
pub fn is_canceling() -> bool {
    with(|i| i.cancel_current && i.running)
}

/// Inclui o item em curso.
pub fn queue_length() -> usize {
    with(|i| i.queue.len() + usize::from(i.running))
}

pub fn current_name() -> String {
    with(|i| i.current_name.clone())
}

pub fn current_path() -> String {
    with(|i| i.current_path.clone())
}

pub fn current_elapsed_sec() -> i32 {
    with(|i| if i.running { i.current_start.map_or(0, |t| t.elapsed().as_secs() as i32) } else { 0 })
}

/// Progresso REAL (0..1), medido pelo trecho de áudio já coberto pelos segmentos — não é
/// barra estimada. -1 = ainda não há número.
pub fn current_progress() -> f64 {
    with(|i| i.progress)
}

/// Segundos restantes (extrapolados do progresso). -1 = ainda sem estimativa (antes de ~10%).
pub fn current_eta_sec() -> i32 {
    with(|i| i.eta)
}

/// `extracting` | `uploading` | `queued` | `decoding` | `transcribing` | `aligning` |
/// `diarizing` | `done` | `checking` | `paused` | `waiting` | `waitingInstall`. Vazio = parado.
pub fn current_stage() -> String {
    with(|i| i.stage.clone())
}

pub fn current_track() -> i32 {
    with(|i| i.track)
}

/// Quantas faixas há, quando a gravação está sendo mandada por faixas isoladas. 1 = faixa
/// única — a UI não mostra nada nesse caso.
pub fn current_track_count() -> i32 {
    with(|i| i.track_count)
}

pub fn done_count() -> i32 {
    with(|i| i.done)
}

pub fn failed_count() -> i32 {
    with(|i| i.failed)
}

pub fn batch_total() -> i32 {
    with(|i| i.batch_total)
}

pub fn last_error() -> String {
    with(|i| i.last_error.clone())
}

/// Nome (sem extensão) da gravação a que o `last_error` se refere.
pub fn last_error_name() -> String {
    with(|i| i.last_error_name.clone())
}

pub fn waiting_for_install() -> bool {
    with(|i| i.waiting_install)
}

/// Contador que muda quando a COMPOSIÇÃO da fila muda (entrou, saiu, mudou de lugar) — e
/// não quando só o progresso andou. O Bridge compara com o último que empurrou para não
/// remandar a lista inteira a cada tique de 1 s.
pub fn queue_revision() -> i32 {
    with(|i| i.queue_rev)
}

/// Caminhos que AINDA ESPERAM, na ordem em que serão processados. O item em curso NÃO
/// entra aqui — ele já saiu da fila quando a worker o pegou.
pub fn queued_paths() -> Vec<String> {
    with(|i| i.queue.clone())
}

/// Segura os itens AUTOMÁTICOS sem esvaziar a fila: nenhum automático NOVO começa enquanto
/// pausado (o que já está em curso segue até o fim). Usado pelo Bridge enquanto grava ou o
/// buffer em memória está ligado — transcrever lê a gravação inteira do disco e sobe dezenas
/// de MB, disputando máquina com o que está sendo capturado. Itens pedidos A MÃO não esperam:
/// ali o usuário escolheu gastar a máquina agora.
pub fn set_paused(paused: bool) {
    let changed = with(|i| std::mem::replace(&mut i.paused, paused) != paused);
    if !changed {
        return;
    }
    if paused {
        log!("Transcribe: automaticas PAUSADAS (captura em andamento; pedidas a mao seguem).");
    } else {
        log!("Transcribe: fila liberada.");
    }
    // A tela precisa saber: pausado com itens na fila vira a etapa "paused".
    notify_changed();
}

fn set_track(track: i32, count: i32) {
    let changed = with(|i| {
        let c = i.track != track || i.track_count != count;
        i.track = track;
        i.track_count = count;
        c
    });
    if changed {
        notify_changed();
    }
}

/// Publica a etapa/progresso e avisa a UI. Só notifica quando algo mudou de fato: o
/// progresso anda aos saltos, então a maioria dos tiques é igual ao anterior.
fn set_stage(stage: &str, progress: f64, eta: i32) {
    let changed = with(|i| {
        let c = i.stage != stage || (i.progress - progress).abs() > 0.0005 || i.eta != eta;
        i.stage = stage.to_owned();
        i.progress = progress;
        i.eta = eta;
        c
    });
    if changed {
        notify_changed();
    }
}

// ---------------------------------------------------------------------------
// Arquivos de transcrição no cache
// ---------------------------------------------------------------------------

/// `<hash>.transcript.json` — a resposta inteira.
pub fn transcript_path(path: &str) -> PathBuf {
    player::cache_root_dir().join(format!("{}.transcript.json", player::hash_name(path)))
}

/// `<hash>.txt` — só o texto puro, usado pela busca.
pub fn transcript_text_path(path: &str) -> PathBuf {
    player::cache_root_dir().join(format!("{}.txt", player::hash_name(path)))
}

pub fn has_transcript(path: &str) -> bool {
    !path.is_empty() && transcript_path(path).exists()
}

/// Estado da transcrição em UMA palavra, para os cards da biblioteca: vazio = nunca foi
/// transcrita; `ok` = transcrita, com fala; `empty` = transcrita, e o áudio não tinha fala
/// nenhuma. Só faz stat — nada de ler o conteúdo, porque isto roda uma vez por gravação a
/// cada listagem da biblioteca.
pub fn transcript_state(path: &str) -> &'static str {
    if path.is_empty() || !transcript_path(path).exists() {
        return "";
    }
    match std::fs::metadata(transcript_text_path(path)) {
        Err(_) => "empty",
        Ok(m) if m.len() > 0 => "ok",
        Ok(_) => "empty",
    }
}

/// Apaga os dois arquivos de uma gravação (usado ao excluir/mover).
pub fn delete_transcript(path: &str) {
    let _ = std::fs::remove_file(transcript_path(path));
    let _ = std::fs::remove_file(transcript_text_path(path));
}

/// Leva a transcrição de `src` para o arquivo EXPORTADO `dst`, no relógio dele: só os
/// trechos mantidos (`starts[i]..ends[i]`, no relógio da origem, em ordem) entram, emendados
/// sem buraco. `false` = origem sem transcrição ou ilegível.
pub fn export_transcript(src: &str, dst: &str, starts: &[f64], ends: &[f64], speed: f64) -> bool {
    if !has_transcript(src) {
        return false;
    }
    let Ok(body) = crate::text_file::read(transcript_path(src)) else { return false };
    let Some((json, text)) = merge::export_transcript_body(&body, starts, ends, speed) else { return false };
    let _ = std::fs::create_dir_all(player::cache_root_dir());
    match std::fs::write(transcript_path(dst), json).and_then(|_| std::fs::write(transcript_text_path(dst), text)) {
        Ok(()) => true,
        Err(e) => {
            log!("Transcribe: nao consegui gravar a transcricao exportada: {e}");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Configuração
// ---------------------------------------------------------------------------

/// Idioma pedido ao motor. O padrão é MANDAR, nunca deixar detectar: detectando, o modelo
/// decide pelo começo do áudio, e quando erra (silêncio, música, ruído — comum numa faixa
/// isolada) ele "transcreve" no idioma errado, que na prática é TRADUZIR a fala inteira.
/// `''`/`app` → idioma da interface do NoOBS; `auto` → vazio (o motor detecta; só por
/// escolha explícita); outro → o código escolhido na tela.
pub fn resolve_transcribe_language() -> String {
    let pref = config::get_config_str("transcribeLanguage", "").trim().to_lowercase();
    if pref == "auto" {
        return String::new();
    }
    if !pref.is_empty() && pref != "app" {
        return pref;
    }
    lang::current_language().split('-').next().unwrap_or("").trim().to_lowercase()
}

// ---------------------------------------------------------------------------
// Persistência da fila
// ---------------------------------------------------------------------------

fn queue_file_path() -> PathBuf {
    config::config_file_path().parent().map(Path::to_path_buf).unwrap_or_default().join(QUEUE_FILE)
}

/// Grava a fila no disco. Chamada FORA do lock, depois de toda mudança de composição.
///
/// O item em curso entra PRIMEIRO: se o app fechar no meio da transcrição, ele é o
/// primeiro a voltar. Só sai do arquivo quando termina de verdade (`current_finished`).
///
/// No fechamento NÃO grava: a volta final do laço da worker trataria o item interrompido
/// como "terminado" e o tiraria do arquivo — justo o item que a persistência existe para
/// preservar. `SAVE_LOCK` serializa snapshot + escrita (duas gravações cruzadas
/// escreveriam o estado velho por último).
fn save_queue() {
    if SHUTTING_DOWN.load(Ordering::SeqCst) {
        return;
    }
    let _g = SAVE_LOCK.lock().unwrap();
    let (items, manual) = with(|i| {
        let mut items = Vec::new();
        if i.running && !i.current_finished && !i.current_path.is_empty() {
            items.push(i.current_path.clone());
        }
        items.extend(i.queue.iter().cloned());
        let manual: Vec<String> = items.iter().filter(|p| i.manual.contains(&p.to_lowercase())).cloned().collect();
        (items, manual)
    });
    // Chave a mais, mesma versão: um arquivo antigo sem ela só restaura tudo como automático.
    let body = json!({ "version": 1, "items": items, "manual": manual }).to_string();
    let dst = queue_file_path();
    let tmp = PathBuf::from(format!("{}.tmp", dst.to_string_lossy()));
    if let Some(d) = dst.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // Troca atômica: fechar no meio da escrita nunca deixa o arquivo pela metade — o que
    // zeraria a fila na próxima leitura.
    if let Err(e) = std::fs::write(&tmp, body).and_then(|_| std::fs::rename(&tmp, &dst)) {
        log!("Transcribe: falha ao gravar a fila: {e}");
    }
}

fn read_saved_array(key: &str) -> Vec<String> {
    let Ok(body) = crate::text_file::read(queue_file_path()) else { return Vec::new() };
    let Ok(root) = serde_json::from_str::<Value>(&body) else { return Vec::new() };
    root.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).filter(|s| !s.trim().is_empty()).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Caminhos salvos na sessão anterior (item em curso primeiro, depois a espera). Quem
/// restaura é o Bridge, que filtra o que não faz mais sentido (arquivo sumiu, já
/// transcrito, só na nuvem) antes do `enqueue_many`.
pub fn load_saved_queue() -> Vec<String> {
    read_saved_array("items")
}

/// Os que, dentre eles, tinham sido pedidos à mão — para a restauração não rebaixar um
/// pedido do usuário a automático (que esperaria a captura).
pub fn load_saved_manual_paths() -> Vec<String> {
    read_saved_array("manual")
}

// ---------------------------------------------------------------------------
// Edição da fila
// ---------------------------------------------------------------------------

fn index_in_queue(i: &Inner, path: &str) -> Option<usize> {
    i.queue.iter().position(|p| p.eq_ignore_ascii_case(path))
}

/// Move um item da espera para a posição `new_index` (0 = próximo a rodar). `false` se o
/// path não está esperando. Não mexe no item em curso.
pub fn move_in_queue(path: &str, new_index: i32) -> bool {
    if path.is_empty() {
        return false;
    }
    let moved = with(|i| {
        let old = index_in_queue(i, path)?;
        // Clamp em vez de recusar: a UI manda um índice calculado de posição de mouse, e a
        // fila pode ter encolhido entre o gesto e a mensagem.
        let new = (new_index.max(0) as usize).min(i.queue.len() - 1);
        if new == old {
            return None;
        }
        let item = i.queue.remove(old);
        i.queue.insert(new, item);
        i.queue_rev += 1;
        Some(new)
    });
    let Some(new) = moved else { return false };
    log!("Transcribe: \"{path}\" movido pra posicao {new} da fila.");
    save_queue();
    notify_changed();
    true
}

/// Tira um item da espera. `false` se não estava lá. O item EM CURSO não sai por aqui —
/// para ele existe o [`cancel_all`].
pub fn remove_from_queue(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let removed = with(|i| {
        let idx = index_in_queue(i, path)?;
        i.queue.remove(idx);
        i.manual.remove(&path.to_lowercase());
        i.queue_rev += 1;
        // O total do lote conta o que VAI rodar. Sem descontar, a linha de progresso ficaria
        // presa em "6 de 7" com a fila vazia.
        if i.batch_total > 0 {
            i.batch_total -= 1;
        }
        Some(())
    });
    if removed.is_none() {
        return false;
    }
    log!("Transcribe: \"{path}\" tirado da fila.");
    save_queue();
    notify_changed();
    true
}

fn already_queued(i: &Inner, path: &str) -> bool {
    // Compara por PATH, nunca pelo nome de exibição: duas pastas podem ter gravações
    // homônimas.
    index_in_queue(i, path).is_some() || (i.running && i.current_path.eq_ignore_ascii_case(path))
}

/// Zera os contadores do lote quando um NOVO lote começa. Não dá para zerar ao esvaziar a
/// fila: a UI tem que continuar mostrando o resultado ("7 de 7") depois que tudo termina.
fn reset_batch_if_idle(i: &mut Inner) {
    if i.running || !i.queue.is_empty() {
        return;
    }
    i.batch_total = 0;
    i.done = 0;
    i.failed = 0;
    i.last_error.clear();
    i.last_error_name.clear();
}

/// Pedido NOVO do usuário tira da tela a falha da gravação anterior, mesmo com a fila
/// andando. Só a falha de GRAVAÇÃO (nome preenchido): a frase de espera do servidor continua
/// valendo enquanto ele não voltar. O contador de falhas do lote fica.
fn clear_item_error(i: &mut Inner) -> bool {
    if i.last_error_name.is_empty() {
        return false;
    }
    i.last_error.clear();
    i.last_error_name.clear();
    true
}

/// Enfileira uma gravação. Ignora silenciosamente se já está na fila. `manual` = o USUÁRIO
/// pediu (menu da gravação, "transcrever pendentes"): só a transcrição automática espera o
/// fim da captura. Pedir à mão um item que já estava na fila como automático o promove.
pub fn enqueue(path: &str, manual: bool) {
    if path.is_empty() {
        return;
    }
    ensure_started();
    let key = path.to_lowercase();
    let (promoted, cleared) = with(|i| {
        let cleared = manual && clear_item_error(i);
        if already_queued(i, path) {
            if manual && !i.manual.contains(&key) {
                i.manual.insert(key.clone());
                i.queue_rev += 1;
                return (true, cleared);
            }
            return (false, cleared);
        }
        reset_batch_if_idle(i);
        i.queue.push(path.to_owned());
        if manual {
            i.manual.insert(key.clone());
        }
        i.batch_total += 1;
        i.queue_rev += 1;
        (true, cleared)
    });
    if !promoted {
        if cleared {
            notify_changed();
        }
        return;
    }
    log!("Transcribe: enfileirado \"{path}\"{} (fila={})", if manual { " a pedido do usuario" } else { "" }, queue_length());
    save_queue();
    notify_changed();
}

/// Enfileira várias de uma vez. `manual` vale para todas; `manual_paths` marca uma a uma
/// (restauração da fila salva, que guarda quem era manual).
pub fn enqueue_many(paths: &[String], manual: bool, manual_paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    ensure_started();
    let added = with(|i| {
        reset_batch_if_idle(i);
        if manual || !manual_paths.is_empty() {
            clear_item_error(i);
        }
        let mut added = 0;
        for p in paths.iter().filter(|p| !p.is_empty()) {
            let key = p.to_lowercase();
            let is_man = manual || manual_paths.iter().any(|m| m.eq_ignore_ascii_case(p));
            if already_queued(i, p) {
                if is_man && !i.manual.contains(&key) {
                    i.manual.insert(key);
                    i.queue_rev += 1;
                    added += 1;
                }
                continue;
            }
            i.queue.push(p.clone());
            if is_man {
                i.manual.insert(key);
            }
            i.batch_total += 1;
            i.queue_rev += 1;
            added += 1;
        }
        added
    });
    log!("Transcribe: {added} item(ns) enfileirado(s){}.", if manual { " a pedido do usuario" } else { "" });
    if added > 0 {
        save_queue();
    }
    notify_changed();
}

/// Cancela a fila inteira E aborta o item em voo: o pedido em andamento no motor local é
/// derrubado na hora.
pub fn cancel_all() {
    with(|i| {
        i.queue.clear();
        i.manual.clear();
        i.queue_rev += 1;
        i.cancel_current = true;
        i.batch_total = 0;
        i.done = 0;
        i.failed = 0;
        i.last_error.clear();
        i.last_error_name.clear();
        i.waiting_install = false;
    });
    log!("Transcribe: fila cancelada.");
    // O bloco em voo leva de segundos (GPU) a ~30 s (CPU) — derruba o pedido agora em vez
    // de esperar ele voltar.
    local_asr::abort_request();
    save_queue();
    // Se a worker esperava a instalação, acorda: com a fila vazia ela sai do modo de espera
    // na hora.
    nudge_queue();
    notify_changed();
}

/// Para a thread e limpa — chamada no shutdown.
pub fn shutdown() {
    // ANTES de parar a worker: a volta final do laço dela trataria o item interrompido como
    // terminado e o tiraria do arquivo da fila.
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    *ON_CHANGED.lock().unwrap() = None;
    // Matar o servidor local derruba o POST em voo na hora, e a worker sai sem precisar ser
    // abandonada.
    local_asr::shutdown();
    {
        let s = signals();
        s.state.lock().unwrap().0 = true;
        s.cv.notify_all();
    }
    let handle = WORKER.lock().unwrap().take();
    if let Some(h) = handle {
        // Espera curta: se o POST em curso ainda está em voo, não dá para abortá-lo — deixa
        // o SO limpar no fim do processo em vez de segurar o fechamento do app.
        let deadline = Instant::now() + Duration::from_secs(2);
        while !h.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if h.is_finished() {
            let _ = h.join();
        } else {
            log!("Transcribe: worker nao parou em 2s — abandonando (cleanup pelo OS).");
        }
    }
}

/// Cria a thread na primeira necessidade.
fn ensure_started() {
    let mut w = WORKER.lock().unwrap();
    if w.is_none() && !SHUTTING_DOWN.load(Ordering::SeqCst) {
        *w = std::thread::Builder::new().name("transcribe".into()).spawn(worker_loop).ok();
    }
}

// ---------------------------------------------------------------------------
// HTTP (servidor remoto / container)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Planejamento das faixas
// ---------------------------------------------------------------------------

/// Decide QUAIS streams de áudio vão para a transcrição: `(nomes, índices)` das ISOLADAS, ou
/// `None` para mandar a mistura (stream 0).
///
/// Stream 0 é a MISTURA; 1..N-1 são as ISOLADAS por dispositivo. Mandar as isoladas separadas
/// dá a atribuição de falante DE GRAÇA — o nome do dispositivo foi escrito como título da
/// stream no MKV. O custo é uma transcrição POR FAIXA, e o motor roda uma de cada vez.
/// Daí o corte: só vale com DUAS ou mais isoladas. Com uma só, a mistura tem exatamente o
/// mesmo conteúdo e sairia o mesmo texto pelo dobro do tempo.
fn plan_tracks(path: &str) -> Option<(Vec<String>, Vec<i32>)> {
    if !config::get_config_bool("transcribePerTrack", true) {
        return None;
    }
    let rep = probe::probe(Path::new(path))?;
    let auds: Vec<_> = rep.audio_streams().collect();
    if auds.len() < 3 {
        return None; // mistura + no mínimo 2 isoladas
    }
    let mut names = Vec::new();
    let mut streams = Vec::new();
    for (i, a) in auds.iter().enumerate().skip(1) {
        streams.push(i as i32);
        // O título vem do `build_track_names`, gravado como metadata da stream. Sem ele
        // (gravação de outra ferramenta) cai num rótulo genérico.
        let title = a.title.trim();
        names.push(if title.is_empty() { lang::t_with("transcript.trackN", &[("n", &(i + 1).to_string())]) } else { title.to_owned() });
    }
    Some((names, streams))
}

/// Decodifica a faixa e olha o pico. Faixa muda (microfone que ninguém usou, alto-falante de
/// uma gravação silenciosa) não é transcrita: gastaria minutos de fila para voltar com zero
/// turnos. Decodar custa segundos; a transcrição custaria minutos.
fn track_has_sound(audio_path: &str) -> bool {
    // Na dúvida, MANDA: perder fala é pior que gastar tempo. Cancelado sai "com som" e quem
    // chamou vê o cancelamento logo em seguida.
    let Some(peaks) = ffmpeg_ops::compute_audio_peaks(audio_path, SILENCE_BUCKETS, Some(&cancel_requested)) else { return true };
    peaks.is_empty() || peaks.iter().any(|&p| p >= SILENCE_PEAK_MIN)
}

// ---------------------------------------------------------------------------
// Execução de um arquivo de áudio
// ---------------------------------------------------------------------------

/// Transcreve UM arquivo de áudio. O progresso publicado é o do CONJUNTO de faixas, não o
/// da faixa: com 3 faixas, a segunda a 50% vale 50% do total, senão a barra voltaria para
/// zero três vezes. E o "faltam" soma as faixas que ainda nem começaram, pela régua da
/// primeira que terminou.
fn run_job(audio_path: &str, track: i32, track_count: i32, est_per_track: &mut i32) -> Res<String> {
    let start = Instant::now();
    set_track(track, track_count);
    let est = *est_per_track;
    let on_progress = |stage: &str, fraction: f64| {
        let elapsed = start.elapsed().as_secs() as i32;
        let mut track_eta = -1;
        // Extrapolar antes de 10% chuta demais (a decodificação é rápida e distorce a conta).
        if fraction >= 0.1 {
            track_eta = (elapsed as f64 * (1.0 - fraction) / fraction).round_ties_even() as i32;
        }
        if track_eta >= 0 && est > 0 {
            track_eta += (track_count - track - 1) * est;
        }
        let total = if track_count <= 1 { fraction } else { (track as f64 + fraction) / track_count as f64 };
        set_stage(stage, total, track_eta);
    };
    let cancel = || cancel_requested() || shutting_down();
    match engine_transcribe(audio_path, &resolve_transcribe_language(), &on_progress, &cancel) {
        local_asr::Outcome::Canceled => {
            if shutting_down() {
                return fail(lang::t("error.transcribe.shuttingDown"));
            }
            Err(Stop::Canceled)
        }
        local_asr::Outcome::Failed(m) => fail(m),
        local_asr::Outcome::Done(body) => {
            if *est_per_track <= 0 {
                *est_per_track = start.elapsed().as_secs() as i32;
            }
            if track_count <= 1 {
                set_stage("done", 1.0, -1);
            } else {
                set_stage("done", (track + 1) as f64 / track_count as f64, -1);
            }
            Ok(body)
        }
    }
}

fn engine_transcribe(audio: &str, language: &str, on_progress: &dyn Fn(&str, f64), cancel: &dyn Fn() -> bool) -> local_asr::Outcome {
    #[cfg(test)]
    if let Some(f) = test_engine::get() {
        return f(audio);
    }
    local_asr::transcribe(audio, language, Some(on_progress), Some(cancel))
}

fn engine_installed() -> bool {
    #[cfg(test)]
    if test_engine::get().is_some() {
        return true;
    }
    local_asr::is_installed()
}

/// Só no `cargo test`: um motor falso no lugar do audio.cpp (que exige 3,7 GB instalados e
/// a placa de vídeo), para testar o caminho inteiro — extrair, escolher faixas, juntar e
/// gravar. Nada disto entra no exe.
#[cfg(test)]
pub mod test_engine {
    use std::sync::Mutex;

    pub type Engine = fn(&str) -> crate::local_asr::Outcome;
    static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

    pub fn set(f: Option<Engine>) {
        *ENGINE.lock().unwrap() = f;
    }

    pub fn get() -> Option<Engine> {
        *ENGINE.lock().unwrap()
    }
}

/// Transcreve UMA gravação. Roda inteiro na worker thread.
///
/// Duas formas, decididas pelo [`plan_tracks`]: 1 faixa — a MISTURA (stream 0), como sempre
/// foi; N faixas — as ISOLADAS por dispositivo, uma transcrição cada, o resultado juntado e
/// desduplicado pelo [`merge::merge_transcripts`]. A segunda custa N vezes mais tempo e
/// entrega atribuição de falante de verdade, com o nome do dispositivo em vez de SPEAKER_00.
fn process_one(path: &str) -> Res {
    if !Path::new(path).is_file() {
        return fail(lang::t("error.transcribe.fileMissing"));
    }

    // Motor não instalado: o item espera na fila (não é falha), sem gastar a extração.
    if !engine_installed() {
        return Err(Stop::LocalMissing);
    }

    // GRAVAÇÃO SEM ÁUDIO nenhum (só monitores, nenhum microfone marcado): não há o que
    // transcrever. Sai como "sem fala", que é o que a gravação é.
    if let Some(rep) = probe::probe(Path::new(path)) {
        if rep.audio_streams().next().is_none() {
            log!("Transcribe: \"{path}\" nao tem faixa de audio — gravada como sem fala.");
            write_transcript(path, "{\"turns\":[],\"text\":\"\"}", "")?;
            return Ok(());
        }
    }

    // A pasta do cache pode não existir ainda (instalação nova, cache apagado): sem ela o
    // m4a de saída não abre e a transcrição falhava em "não foi possível extrair o áudio".
    let _ = std::fs::create_dir_all(player::cache_root_dir());
    let cache_base = player::cache_root_dir().join(player::hash_name(path));
    let plan = plan_tracks(path);
    let per_track = plan.is_some();
    // Uma faixa só: a MISTURA (stream 0).
    let (names, streams) = plan.unwrap_or_else(|| (vec![String::new()], vec![0]));

    let files: Vec<String> = streams.iter().map(|s| format!("{}_tr{s}.m4a", cache_base.to_string_lossy())).collect();
    // Os m4a são só veículo da transcrição — regeneram em segundos, e uma hora de áudio ocupa
    // ~70 MB que não vale guardar.
    struct Cleanup<'a>(&'a [String]);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            for f in self.0 {
                let _ = std::fs::remove_file(f);
            }
            set_track(0, 1);
        }
    }
    let _cleanup = Cleanup(&files);
    for f in &files {
        let _ = std::fs::remove_file(f);
    }

    set_stage("extracting", 0.0, -1);
    // UMA passada de demux para todas as faixas. `plan_tracks` devolve sempre um intervalo
    // contíguo começando em `streams[0]`, então o start index cobre o resto. Com o
    // cancelamento lido a cada pacote: numa gravação longa extrair leva segundos.
    if !ffmpeg_ops::extract_audio_tracks(path, &files, streams[0] as usize, Some(&cancel_requested)) {
        if cancel_requested() {
            return Err(Stop::Canceled);
        }
        return fail(lang::t("error.transcribe.extractFailed"));
    }
    if cancel_requested() {
        return Err(Stop::Canceled);
    }

    // Faixa muda não é transcrita. Com várias faixas isto é o que impede o microfone que
    // ninguém usou de gastar uma fila inteira.
    let mut send: Vec<&String> = Vec::new();
    let mut used_names: Vec<String> = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if cancel_requested() {
            return Err(Stop::Canceled);
        }
        if !Path::new(f).exists() {
            return fail(lang::t("error.transcribe.extractMissing"));
        }
        if per_track && !track_has_sound(f) {
            log!("Transcribe: faixa \"{}\" sem som — nao sera transcrita.", names[i]);
            continue;
        }
        used_names.push(names[i].clone());
        send.push(f);
    }

    let (merged, txt);
    if send.is_empty() {
        // Todas mudas: é uma gravação sem fala, não uma falha. Escreve a transcrição vazia — o
        // app distingue "sem fala" de "não transcrita".
        log!("Transcribe: nenhuma faixa com som em \"{path}\".");
        merged = "{\"turns\":[],\"text\":\"\"}".to_owned();
        txt = String::new();
    } else {
        let mut est = 0;
        let mut bodies = Vec::with_capacity(send.len());
        for (i, f) in send.iter().enumerate() {
            // Cancelado durante a extração (ou entre faixas): nem começa a faixa.
            if cancel_requested() {
                return Err(Stop::Canceled);
            }
            bodies.push(run_job(f, i as i32, send.len() as i32, &mut est)?);
        }
        // Só a MISTURA passa direto: o JSON do motor já está no formato final, e reescrevê-lo
        // jogaria fora os segmentos com timestamp por palavra que o merge não carrega. Por
        // faixa, mesmo com UMA sobrando (as outras mudas), o merge roda: é ele quem troca o
        // SPEAKER_00 pelo nome do dispositivo.
        if !per_track {
            let mut body = bodies.swap_remove(0);
            // Mesmo aqui os turnos são remontados: o `turns` do motor quebra só em troca de
            // falante, e o player destaca o bloco inteiro. O resto da resposta é preservado.
            if let Some(re) = merge::rechunk_turns(&body) {
                body = re;
            }
            let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(&body) else {
                return fail(lang::t("error.transcribe.resultNotJson"));
            };
            txt = merge::extract_plain_text(&v);
            merged = body;
        } else {
            let Some((m, t)) = merge::merge_transcripts(&bodies, &used_names) else {
                return fail(lang::t("error.transcribe.resultNotJson"));
            };
            merged = m;
            txt = t;
        }
    }
    // Dois arquivos: o JSON para o player, o texto puro para a busca.
    write_transcript(path, &merged, &txt)?;
    Ok(())
}

/// Grava os dois arquivos e leva a cópia para o fim do arquivo de vídeo (OBSFileMeta).
fn write_transcript(path: &str, json: &str, text: &str) -> Res {
    let _ = std::fs::create_dir_all(player::cache_root_dir());
    std::fs::write(transcript_path(path), json)
        .and_then(|_| std::fs::write(transcript_text_path(path), text))
        .map_err(|e| Stop::Msg(lang::t_with("error.transcribe.writeFailed", &[("error", &e.to_string())])))?;
    file_meta::mark_changed(path);
    Ok(())
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

/// Próximo item a rodar, ou `None`. Pausado, pula os automáticos e pega o primeiro pedido à
/// mão, na ordem da fila.
fn next_index(i: &Inner) -> Option<usize> {
    if i.queue.is_empty() {
        return None;
    }
    if !i.paused {
        return Some(0);
    }
    i.queue.iter().position(|p| i.manual.contains(&p.to_lowercase()))
}

fn base_name(path: &str) -> String {
    Path::new(path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Tira o próximo item da espera e o marca como em curso, ou zera o estado se não há nenhum.
fn pick_next() -> Option<String> {
    with(|i| {
        // Pausado: NÃO pega item AUTOMÁTICO novo, mas a fila fica como está; pedido à mão
        // passa na frente. Só com automáticos parados a etapa vira `paused`, para a aba de
        // Transcrição explicar a espera — fila parada sem motivo aparente se lê como defeito.
        let idx = next_index(i);
        if idx.is_none() && i.paused && !i.queue.is_empty() {
            i.stage = "paused".into();
        }
        if let Some(idx) = idx {
            let path = i.queue.remove(idx);
            i.queue_rev += 1;
            i.running = true;
            i.current_finished = false;
            i.current_path = path.clone();
            i.current_name = base_name(&path);
            i.current_start = Some(Instant::now());
            i.stage = "extracting".into();
            i.progress = -1.0;
            i.eta = -1;
            i.track = 0;
            i.track_count = 1;
            // O cancelamento vale para o item que estava em curso quando o usuário clicou; um
            // item NOVO começa limpo.
            i.cancel_current = false;
            Some(path)
        } else {
            if i.running {
                i.queue_rev += 1; // a linha "em curso" sai da lista
            }
            i.running = false;
            i.current_path.clear();
            i.current_name.clear();
            i.current_start = None;
            if i.stage != "paused" {
                i.stage.clear();
            }
            i.progress = -1.0;
            i.eta = -1;
            i.cancel_current = false;
            // Fila vazia: não há mais o que esperar — nem o aviso de espera.
            if i.waiting_install && i.last_error_name.is_empty() {
                i.last_error.clear();
            }
            i.waiting_install = false;
            // Os contadores do lote NÃO são zerados aqui: a UI precisa continuar mostrando
            // "7 de 7 concluídas" depois que a fila esvazia. Quem zera é o próximo lote.
            None
        }
    })
}

fn worker_loop() {
    while !stop_set() {
        let Some(path) = pick_next() else {
            notify_changed();
            // Fila vazia: o servidor local, se de pé, cai depois de ocioso por alguns minutos
            // e devolve a memória de vídeo.
            local_asr::stop_if_idle();
            if wait_stop(400) {
                break;
            }
            continue;
        };

        // O item saiu da espera e virou "em curso": continua no arquivo, agora na primeira
        // posição.
        save_queue();
        notify_changed();
        log!("Transcribe: iniciando \"{path}\"");
        let mut result = std::panic::catch_unwind(|| process_one(&path)).unwrap_or_else(|_| Err(Stop::Msg("panic".into())));

        // Cancelado enquanto este item rodava: o que quer que ele tenha devolvido depois
        // (motor ausente, falha de extração) é consequência do cancelamento ou
        // já não interessa. Um sucesso passa (a transcrição já foi gravada), mas não conta no
        // lote zerado.
        let was_canceled = cancel_requested();
        if was_canceled && result.is_err() {
            result = Err(Stop::Canceled);
        }
        // Gravação APAGADA enquanto esperava ou rodava (o usuário excluiu, a pasta foi
        // movida): não é falha, e ninguém precisa de aviso por um arquivo que ele mesmo tirou.
        if matches!(result, Err(Stop::Msg(_) | Stop::LocalMissing)) && !Path::new(&path).exists() {
            log!("Transcribe: \"{path}\" nao existe mais — tirado da fila sem aviso.");
            result = Err(Stop::Canceled);
        }

        // MOTOR NÃO INSTALADO: não é defeito do arquivo. Devolve o item para a FRENTE da
        // espera, sem contar como falha nem soltar aviso por item, e olha de novo daqui a
        // `INSTALL_RETRY_MS` — ou antes, quando a instalação terminar (`nudge_queue`).
        if matches!(result, Err(Stop::LocalMissing)) && !stop_set() {
            with(|i| {
                i.queue.insert(0, path.clone());
                i.queue_rev += 1;
                i.running = false;
                i.current_finished = true;
                i.current_path.clear();
                i.current_name.clear();
                i.current_start = None;
                i.stage = "waitingInstall".into();
                i.progress = -1.0;
                i.eta = -1;
                if !i.waiting_install {
                    i.waiting_install = true;
                    // Nome vazio de propósito: não é a gravação que falhou, é o motor que falta.
                    i.last_error = lang::t("error.localAsr.notInstalled");
                    i.last_error_name.clear();
                    log!("Transcribe: motor local nao instalado — fila em espera.");
                }
            });
            save_queue();
            notify_changed();
            if wait_stop_or_wake(INSTALL_RETRY_MS) {
                break;
            }
            continue;
        }

        with(|i| {
            i.current_finished = true;
            // Terminou de verdade (sucesso, falha ou cancelamento): a marca de manual morre
            // aqui, não quando a worker pegou o item.
            i.manual.remove(&path.to_lowercase());
            // O motor rodou: tira a frase de espera da tela — só ela (nome vazio), nunca a
            // falha real de uma gravação.
            if i.waiting_install {
                i.waiting_install = false;
                if i.last_error_name.is_empty() {
                    i.last_error.clear();
                }
            }
            match &result {
                // Cancelado pelo usuário (ou fechamento): não conta como concluído nem como
                // falha, e não vira mensagem de erro na tela.
                Err(Stop::Canceled | Stop::LocalMissing) => log!("Transcribe: cancelado \"{path}\""),
                Ok(()) => {
                    if !was_canceled {
                        i.done += 1;
                    }
                }
                Err(Stop::Msg(m)) => {
                    i.failed += 1;
                    i.last_error = m.clone();
                    i.last_error_name = base_name(&path);
                }
            }
        });
        // Terminou de verdade: sai do arquivo. (No fechamento o `save_queue` não grava, então o
        // item interrompido fica lá para voltar na próxima vez.)
        save_queue();
        match &result {
            Ok(()) => log!("Transcribe: concluida \"{path}\""),
            Err(Stop::Msg(m)) => log!("Transcribe: FALHOU \"{path}\": {m}"),
            Err(_) => {}
        }
        notify_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estado_de_transcricao_so_com_stat() {
        assert_eq!(transcript_state(""), "");
    }
}

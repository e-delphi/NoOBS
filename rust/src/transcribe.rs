//! Fila de transcrição contra a Transcritor API (<https://github.com/e-delphi/transcritor-api>,
//! um container local com faster-whisper + diarização por falante) ou contra o motor
//! local ([`crate::local_asr`]). Porte de `OBSTranscribe.pas`.
//!
//! Desenho em pontos que não são óbvios:
//!
//! 1. MANDA ÁUDIO, NUNCA O VÍDEO. A API tem `MAX_UPLOAD_MB` por padrão e uma gravação 4K
//!    passa disso fácil. O áudio sai em m4a por [`ffmpeg_ops::extract_audio_tracks`]: 1
//!    hora dá ~70 MB. E manda as faixas ISOLADAS, uma transcrição por faixa, não a
//!    mistura: cada faixa é um dispositivo, com o nome dele escrito no MKV — a atribuição
//!    de falante sai de graça. Custa N vezes mais tempo, então só vale com 2+ isoladas;
//!    faixa muda nem é enviada ([`plan_tracks`]). O preço é o ECO: o que sai pelos
//!    alto-falantes volta pelo microfone ([`merge`]).
//! 2. MODELO ASSÍNCRONO, COM PROGRESSO REAL. `POST /jobs` devolve um id na hora;
//!    `GET /jobs/{id}` dá estágio, percentual e ETA — e o `result` só com
//!    `?incluir_resultado=true`. O id do job é o SHA-256 do áudio + as opções, então
//!    reenviar a mesma gravação devolve o resultado pronto.
//! 3. UMA POR VEZ. O container serializa as requisições, então paralelizar aqui só
//!    encheria a fila do outro lado. Uma thread, uma fila — e o cancelamento aborta até o
//!    item em voo, via `DELETE /jobs/{id}`.
//! 4. DOIS MOTORES. `server` é a Transcritor API; `local` é o OBSLocalAsr (Qwen3 +
//!    audio.cpp na GPU). Tudo o que vem depois do `run_job` (turnos, eco, cache) não sabe
//!    qual motor rodou — os dois devolvem o MESMO JSON.
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

use crate::win_http::{self, Request, Response, Timeouts};
use crate::{config, ffmpeg_ops, file_meta, lang, local_asr, log, mainthread, player, probe};

// Rotas FIXAS de propósito: o usuário informa host e porta, não o caminho.
const JOBS_PATH: &str = "/jobs";
const HEALTH_PATH: &str = "/health";
/// A v2 da API separou andamento de resultado: o JSON da transcrição vem neste parâmetro.
const STATUS_QUERY: &str = "?incluir_resultado=true";
const DEFAULT_HOST: &str = "http://localhost:8000";

const HEALTH_TIMEOUT_MS: i32 = 4000;
const CONNECT_TIMEOUT_MS: i32 = 5000;
/// O `POST /jobs` só espera o UPLOAD (a API responde ao enfileirar), mas uma gravação longa
/// vira dezenas de MB — daí a folga.
const SUBMIT_TIMEOUT_MS: i32 = 5 * 60 * 1000;
const POLL_TIMEOUT_MS: i32 = 10_000;
/// Consulta barata e local. O progresso anda aos saltos (janelas de 30 s do Whisper), então
/// 1 s já mostra cada degrau.
const POLL_INTERVAL_MS: u64 = 1000;

/// Faixa cujo pico nunca passa disso não vai para o servidor: é o microfone que ninguém
/// usou. Mandar assim só gastaria minutos de fila para receber zero turnos.
const SILENCE_PEAK_MIN: f32 = 0.02;
const SILENCE_BUCKETS: usize = 400;

/// Entre tentativas com o servidor fora. O teste é um `GET /health` local.
const SERVER_RETRY_MS: u64 = 10_000;

const QUEUE_FILE: &str = "transcribe-queue.json";

/// Timeouts do diagnóstico. O `docker info` é o mais lento: com o Docker Desktop subindo
/// ele pode segurar vários segundos antes de responder.
const DIAG_WSL_TIMEOUT_MS: u64 = 10_000;
const DIAG_DOCKER_TIMEOUT_MS: u64 = 20_000;

/// Por que um item parou. Os dois primeiros NÃO são falha: cancelamento do usuário, e
/// servidor/motor indisponível (o item volta para a frente da espera — com a transcrição
/// automática ao fim da gravação, gravar com o Docker ainda subindo é o caso COMUM).
#[derive(Debug)]
enum Stop {
    Canceled,
    ServerDown,
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
    /// servidor fora do ar devolve o item para a fila, e ele tem que voltar ainda manual).
    manual: HashSet<String>,
    running: bool,
    current_name: String,
    current_path: String,
    current_start: Option<Instant>,
    stage: String,
    /// -1 = ainda sem número
    progress: f64,
    /// -1 = a API ainda não arrisca
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
    waiting_server: bool,
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

/// Acorda a worker que espera o servidor, sem aguardar o próximo ciclo. Chamado quando um
/// teste ou diagnóstico acabou de ver o servidor de pé.
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

/// Segundos restantes segundo a API. -1 = ela ainda não arrisca (só aparece a partir de ~10%).
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

pub fn waiting_for_server() -> bool {
    with(|i| i.waiting_server)
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

/// Base do servidor, do config (`transcribeHost`). Sempre sem barra final.
pub fn host_base() -> String {
    let h = config::get_config_str("transcribeHost", DEFAULT_HOST);
    let h = h.trim().trim_end_matches('/');
    if h.is_empty() { DEFAULT_HOST.to_owned() } else { h.to_owned() }
}

/// Padrão: o motor LOCAL (audio.cpp na placa de vídeo) — não exige Docker, e o item só
/// espera na fila até a instalação. Exceção para quem já usava o servidor antes deste
/// padrão: a chave não existia, mas quem configurou um host o escreveu no config.
pub fn use_local_engine() -> bool {
    let engine = config::get_config_str("transcribeEngine", "");
    let engine = engine.trim();
    if engine.is_empty() {
        config::get_config_str("transcribeHost", "").trim().is_empty()
    } else {
        engine.eq_ignore_ascii_case("local")
    }
}

/// Idioma que vai para a API. O padrão é MANDAR, nunca deixar detectar: sem `language`, o
/// faster-whisper decide pelos primeiros 30 s do áudio, e quando erra (silêncio, música,
/// ruído — comum numa faixa isolada) ele "transcreve" no idioma errado, que na prática é
/// TRADUZIR a fala inteira. `''`/`app` → idioma da interface do NoOBS; `auto` → vazio (a API
/// detecta; só por escolha explícita); outro → o código escolhido na tela.
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

/// Cancela a fila inteira E aborta o item em voo (`DELETE /jobs/{id}`; no motor local, o
/// pedido em voo é derrubado na hora). A worker vê a flag na próxima consulta (no máximo
/// 1 s), manda o DELETE e desiste. Cancelar é melhor-esforço.
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
        i.waiting_server = false;
    });
    log!("Transcribe: fila cancelada.");
    // Motor local: o bloco em voo leva de segundos (GPU) a ~30 s (CPU) — derruba o pedido
    // agora em vez de esperar ele voltar.
    local_asr::abort_request();
    save_queue();
    // Se a worker esperava o servidor, acorda: com a fila vazia ela sai do modo de espera
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

/// "HTTP 422" sozinho não diz NADA — quem sabe o motivo é o CORPO da resposta. A
/// Transcritor API é FastAPI, então erro sai como `{"detail": ...}`: string nos erros de
/// negócio e ARRAY nos de validação (422). Pega o `detail` quando dá, senão o corpo cru.
/// Trunca e tira quebras de linha porque isto vira UMA linha na tela.
fn http_err_text(resp: &Response) -> String {
    const MAX_LEN: usize = 300;
    let mut result = format!("HTTP {}", resp.status);
    let body = resp.text();
    let body = body.trim();
    if body.is_empty() {
        return result;
    }
    let mut detail = body.to_owned();
    if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(body) {
        match o.get("detail") {
            Some(Value::String(s)) => detail = s.clone(),
            Some(v) => detail = v.to_string(),
            None => {}
        }
    }
    let detail = detail.replace(['\r', '\n'], " ");
    let detail = detail.trim();
    if detail.is_empty() {
        return result;
    }
    result.push_str(": ");
    if detail.chars().count() > MAX_LEN {
        result.extend(detail.chars().take(MAX_LEN));
        result.push_str("...");
    } else {
        result.push_str(detail);
    }
    result
}

/// Falha de CONEXÃO chega aqui como texto cru do WinHTTP, que descreve o sintoma e não o que
/// fazer. Container parado dá justamente isso, e o usuário não tem como ligar uma coisa à
/// outra. Troca pela frase que diz o que houve e ONDE; o texto técnico vai para o log.
fn net_err_text(msg: &str) -> String {
    log!("Transcribe: falha de rede contra {}: {msg}", host_base());
    lang::t_with("error.transcribe.serverDown", &[("host", &host_base())])
}

fn get(url: &str, ms: i32) -> Result<Response, win_http::HttpError> {
    win_http::request(&Request::new("GET", url).header("User-Agent", "NoOBS").timeouts(Timeouts { connect_ms: ms, send_ms: ms, receive_ms: ms }))
}

/// `GET /health`. `Ok` = servidor de pé. Roda na worker (a UI chama por um botão "Testar").
/// Devolve o texto CRU da falha de propósito: aqui o usuário apertou "Testar" e quer o
/// diagnóstico. Quem transcreve usa o `serverDown`.
pub fn check_health(host: &str) -> Result<(), String> {
    let base = host.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(lang::t("error.transcribe.emptyHost"));
    }
    match get(&format!("{base}{HEALTH_PATH}"), HEALTH_TIMEOUT_MS) {
        Err(e) => Err(e.to_string()),
        Ok(r) if r.status != 200 => Err(http_err_text(&r)),
        Ok(_) => Ok(()),
    }
}

/// A Transcritor API tem duas versões: o container (WhisperX), que separa falantes, e a
/// nativa do Windows (Qwen3 + audio.cpp, na GPU), que não separa e RECUSA
/// `diarization=true` com HTTP 400. O `/health` da nativa diz o motor (`"engine": "audiocpp"`);
/// o container 2.0 não manda o campo. Sem `engine`, ou `whisperx`, pede a separação; qualquer
/// outro, não. Na dúvida (health sem resposta) pede, que é o comportamento de sempre.
fn server_diarizes() -> bool {
    let Ok(r) = get(&format!("{}{HEALTH_PATH}", host_base()), HEALTH_TIMEOUT_MS) else { return true };
    if r.status != 200 {
        return true;
    }
    match serde_json::from_str::<Value>(&r.text()) {
        Ok(v) => v.get("engine").and_then(Value::as_str).map_or(true, |e| e.eq_ignore_ascii_case("whisperx")),
        Err(_) => true,
    }
}

/// `POST /jobs` — devolve na hora um identificador (HTTP 202), sem esperar a transcrição. O
/// upload é a única parte demorada aqui, e ele é rápido porque só vai áudio.
fn submit_job(audio_path: &str) -> Result<String, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let boundary = format!("----NoOBS{nanos:032x}");
    let audio = std::fs::read(audio_path).map_err(|e| e.to_string())?;
    let file_name = Path::new(audio_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "audio.m4a".into());

    let mut payload: Vec<u8> = Vec::with_capacity(audio.len() + 1024);
    let mut field = |name: &str, value: &str| {
        payload.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    };
    // Idioma SEMPRE explícito, salvo escolha de "detectar" — deixar a API adivinhar era o
    // que traduzia a transcrição.
    let language = resolve_transcribe_language();
    if !language.is_empty() {
        field("language", &language);
    }
    if language.is_empty() {
        log!("Transcribe: idioma: deteccao automatica.");
    } else {
        log!("Transcribe: idioma: {language}");
    }
    // Separação por falante — é o que torna o painel do player útil. Só quando o servidor
    // sabe fazer: a versão nativa recusa com 400.
    if server_diarizes() {
        field("diarization", "true");
    } else {
        log!("Transcribe: servidor sem separacao por falante; enviando sem diarizacao.");
    }
    // Alinhamento por palavra. O padrão da API já é true, mas vai EXPLÍCITO porque o recorte
    // de turnos depende dele (pegadinha #60k).
    field("alignment", "true");
    payload.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: audio/mp4\r\n\r\n").as_bytes(),
    );
    payload.extend_from_slice(&audio);
    payload.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    drop(audio);

    let url = format!("{}{JOBS_PATH}", host_base());
    let req = Request::new("POST", &url)
        .header("User-Agent", "NoOBS")
        .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .body(&payload)
        .timeouts(Timeouts { connect_ms: CONNECT_TIMEOUT_MS, send_ms: SUBMIT_TIMEOUT_MS, receive_ms: SUBMIT_TIMEOUT_MS });
    let resp = win_http::request(&req).map_err(|e| net_err_text(&e.to_string()))?;
    // 202 é o esperado; aceita 200 também para não quebrar se a API mudar o código de sucesso.
    if resp.status != 202 && resp.status != 200 {
        return Err(http_err_text(&resp));
    }
    let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&resp.text()) else {
        return Err(lang::t("error.transcribe.jobsNotJson"));
    };
    let id = o.get("job_id").and_then(Value::as_str).unwrap_or("").trim().to_owned();
    if id.is_empty() {
        return Err(lang::t("error.transcribe.noJobId"));
    }
    Ok(id)
}

/// Melhor esforço: se o cancelamento não chegar, o job termina sozinho e expira pelo
/// `JOB_TTL_SECONDS` da API. Não vale falhar por causa disso.
fn delete_job(job_id: &str) {
    if job_id.is_empty() {
        return;
    }
    let url = format!("{}{JOBS_PATH}/{job_id}", host_base());
    let t = Timeouts { connect_ms: CONNECT_TIMEOUT_MS, send_ms: POLL_TIMEOUT_MS, receive_ms: POLL_TIMEOUT_MS };
    let _ = win_http::request(&Request::new("DELETE", &url).timeouts(t));
    log!("Transcribe: job {job_id} cancelado no servidor.");
}

#[derive(Default)]
struct JobStatus {
    status: String,
    stage: String,
    error: String,
    progress: f64,
    eta: i32,
    result: String,
}

/// `GET /jobs/{id}`. `Ok` se a consulta em si funcionou (mesmo com status `error` do job —
/// isso vem em `status`/`error`); senão a falha de rede.
fn poll_job(job_id: &str) -> Result<JobStatus, String> {
    let url = format!("{}{JOBS_PATH}/{job_id}{STATUS_QUERY}", host_base());
    let t = Timeouts { connect_ms: CONNECT_TIMEOUT_MS, send_ms: POLL_TIMEOUT_MS, receive_ms: POLL_TIMEOUT_MS };
    let resp = win_http::request(&Request::new("GET", &url).header("User-Agent", "NoOBS").timeouts(t)).map_err(|e| net_err_text(&e.to_string()))?;
    if resp.status == 404 {
        return Err(lang::t("error.transcribe.jobGone"));
    }
    if resp.status != 200 {
        return Err(http_err_text(&resp));
    }
    let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&resp.text()) else {
        return Err(lang::t("error.transcribe.statusNotJson"));
    };
    let s = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    Ok(JobStatus {
        status: s("status"),
        stage: s("stage"),
        error: s("error"),
        progress: o.get("progress").and_then(Value::as_f64).unwrap_or(0.0),
        // `eta_seconds` só aparece a partir de ~10% (antes disso extrapolar não faz sentido).
        eta: o.get("eta_seconds").and_then(Value::as_f64).map_or(-1, |e| e.round_ties_even() as i32),
        // O `result` só vem quando `status = done`, e só porque pedimos (`STATUS_QUERY`).
        result: o.get("result").filter(|r| r.is_object()).map(Value::to_string).unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------
// Planejamento das faixas
// ---------------------------------------------------------------------------

/// Decide QUAIS streams de áudio vão para a transcrição: `(nomes, índices)` das ISOLADAS, ou
/// `None` para mandar a mistura (stream 0).
///
/// Stream 0 é a MISTURA; 1..N-1 são as ISOLADAS por dispositivo. Mandar as isoladas separadas
/// dá a atribuição de falante DE GRAÇA — o nome do dispositivo foi escrito como título da
/// stream no MKV. O custo é uma transcrição POR FAIXA, e o container roda uma de cada vez.
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
/// uma gravação silenciosa) não vai para o servidor: gastaria minutos de fila para voltar com
/// zero turnos. Decodar custa segundos; a transcrição custaria minutos.
fn track_has_sound(audio_path: &str) -> bool {
    // Na dúvida, MANDA: perder fala é pior que gastar tempo. Cancelado sai "com som" e quem
    // chamou vê o cancelamento logo em seguida.
    let Some(peaks) = ffmpeg_ops::compute_audio_peaks(audio_path, SILENCE_BUCKETS, Some(&cancel_requested)) else { return true };
    peaks.is_empty() || peaks.iter().any(|&p| p >= SILENCE_PEAK_MIN)
}

// ---------------------------------------------------------------------------
// Execução de um arquivo de áudio
// ---------------------------------------------------------------------------

/// O `run_job` do motor LOCAL. Mesmas regras de progresso do caminho da API: fração do
/// CONJUNTO de faixas e "faltam" somando as faixas que nem começaram, pela régua da primeira
/// que terminou.
fn run_local(audio_path: &str, track: i32, track_count: i32, start: Instant, est_per_track: &mut i32) -> Res<String> {
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
    match local_asr::transcribe(audio_path, &resolve_transcribe_language(), Some(&on_progress), Some(&cancel)) {
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

/// Sobe UM arquivo de áudio e acompanha até o fim. O progresso publicado é o do CONJUNTO de
/// faixas, não o da faixa: com 3 faixas, a segunda a 50% vale 50% do total, senão a barra
/// voltaria para zero três vezes. E o "faltam" soma as faixas que ainda nem começaram,
/// usando o tempo medido da primeira que terminou.
fn run_job(audio_path: &str, track: i32, track_count: i32, est_per_track: &mut i32) -> Res<String> {
    let start = Instant::now();
    set_track(track, track_count);

    if use_local_engine() {
        // Motor local: mesmo contrato (JSON da API); progresso e ETA calculados aqui — não há
        // servidor que os mande.
        return run_local(audio_path, track, track_count, start, est_per_track);
    }

    let overall = |inner: f64| -> f64 {
        if track_count <= 1 {
            return inner;
        }
        (track as f64 + inner.max(0.0)) / track_count as f64
    };

    set_stage("uploading", overall(0.0), -1);
    let job_id = submit_job(audio_path).map_err(Stop::Msg)?;
    log!("Transcribe: job {job_id} aceito (faixa {} de {track_count}).", track + 1);

    loop {
        if shutting_down() {
            delete_job(&job_id);
            return fail(lang::t("error.transcribe.shuttingDown"));
        }
        if cancel_requested() {
            delete_job(&job_id);
            return Err(Stop::Canceled);
        }
        let st = poll_job(&job_id).map_err(Stop::Msg)?;
        if st.status == "done" {
            if st.result.trim().is_empty() {
                return fail(lang::t("error.transcribe.emptyResult"));
            }
            // A primeira faixa concluída vira a régua para as que faltam.
            if *est_per_track <= 0 {
                *est_per_track = start.elapsed().as_secs() as i32;
            }
            set_stage("done", overall(1.0), -1);
            return Ok(st.result);
        }
        if st.status == "error" {
            return fail(if st.error.is_empty() { lang::t("error.transcribe.serverSilent") } else { st.error });
        }
        let mut total_eta = st.eta;
        if total_eta >= 0 && *est_per_track > 0 {
            total_eta += (track_count - track - 1) * *est_per_track;
        }
        set_stage(&st.stage, overall(st.progress), total_eta);

        // Espera interrompível: o stop tira a thread daqui na hora, em vez de deixar o
        // fechamento do app esperando um sleep.
        if wait_stop(POLL_INTERVAL_MS) {
            delete_job(&job_id);
            return fail(lang::t("error.transcribe.shuttingDown"));
        }
    }
}

/// O `/health` acabou de responder. Se a fila estava esperando o servidor, tira a frase de
/// espera da tela AGORA — antes ela só saía quando o item terminava de transcrever, minutos
/// depois, e parecia que o erro tinha ficado preso. Só a frase de espera (nome vazio),
/// nunca a falha real de uma gravação.
fn server_back_up() {
    let changed = with(|i| {
        if !i.waiting_server {
            return false;
        }
        i.waiting_server = false;
        if i.last_error_name.is_empty() {
            i.last_error.clear();
        }
        true
    });
    if changed {
        log!("Transcribe: servidor voltou ({}) — fila retomada.", host_base());
        notify_changed();
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

    // PING ANTES DE QUALQUER TRABALHO: (1) servidor fora do ar só aparecia depois de extrair
    // o áudio — dezenas de MB jogados fora por item do lote; (2) o erro que chegava era o do
    // WinHTTP no meio do upload, que não tem como ser lido como "o container não está
    // rodando". O `/health` é barato e local. Motor LOCAL: não há servidor para pingar; o que
    // falta, se falta, é a instalação. O servidor local sobe sozinho dentro do `run_local`.
    if use_local_engine() {
        if !local_asr::is_installed() {
            return Err(Stop::LocalMissing);
        }
    } else {
        set_stage("checking", 0.0, -1);
        if let Err(e) = check_health(&host_base()) {
            // Uma linha por queda, não uma a cada tentativa.
            if !waiting_for_server() {
                log!("Transcribe: /health nao respondeu ({}): {e}", host_base());
            }
            // O worker poe o item de volta na frente da espera e tenta de novo depois.
            return Err(Stop::ServerDown);
        }
        server_back_up();
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

    let cache_base = player::cache_root_dir().join(player::hash_name(path));
    let plan = plan_tracks(path);
    let per_track = plan.is_some();
    // Caminho antigo: SÓ O ÁUDIO da mistura. O vídeo passaria do `MAX_UPLOAD_MB` da API numa
    // gravação 4K de poucos minutos.
    let (names, streams) = plan.unwrap_or_else(|| (vec![String::new()], vec![0]));

    let files: Vec<String> = streams.iter().map(|s| format!("{}_tr{s}.m4a", cache_base.to_string_lossy())).collect();
    // Os m4a são só veículo do upload — regeneram em segundos, e uma hora de áudio ocupa ~70 MB
    // que não vale guardar.
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

    // Faixa muda não vai para o servidor. Com várias faixas isto é o que impede o microfone
    // que ninguém usou de gastar uma fila inteira.
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
            log!("Transcribe: faixa \"{}\" sem som — nao vai pro servidor.", names[i]);
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
            // Cancelado durante a extração (ou entre faixas): nem sobe o áudio.
            if cancel_requested() {
                return Err(Stop::Canceled);
            }
            match run_job(f, i as i32, send.len() as i32, &mut est) {
                Ok(b) => bodies.push(b),
                Err(Stop::Msg(m)) => {
                    // O container pode cair NO MEIO do job (Docker reiniciado, computador
                    // suspenso). Isso chega como erro de rede, mas é o mesmo caso do servidor
                    // fora antes de começar: se o `/health` agora não responde, o item espera
                    // em vez de ser descartado.
                    if !shutting_down() && !use_local_engine() && check_health(&host_base()).is_err() {
                        return Err(Stop::ServerDown);
                    }
                    return Err(Stop::Msg(m));
                }
                Err(e) => return Err(e),
            }
        }
        // Só a MISTURA passa direto: o JSON da API já está no formato final, e reescrevê-lo
        // jogaria fora os segmentos com timestamp por palavra que o merge não carrega. Por
        // faixa, mesmo com UMA sobrando (as outras mudas), o merge roda: é ele quem troca o
        // SPEAKER_00 pelo nome do dispositivo.
        if !per_track {
            let mut body = bodies.swap_remove(0);
            // Mesmo aqui os turnos são remontados: o `turns` da API quebra só em troca de
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
            if i.waiting_server && i.last_error_name.is_empty() {
                i.last_error.clear();
            }
            i.waiting_server = false;
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
        // (erro de rede, servidor fora, falha de extração) é consequência do cancelamento ou
        // já não interessa. Um sucesso passa (a transcrição já foi gravada), mas não conta no
        // lote zerado.
        let was_canceled = cancel_requested();
        if was_canceled && result.is_err() {
            result = Err(Stop::Canceled);
        }
        // Gravação APAGADA enquanto esperava ou rodava (o usuário excluiu, a pasta foi
        // movida): não é falha, e ninguém precisa de aviso por um arquivo que ele mesmo tirou.
        if matches!(result, Err(Stop::Msg(_) | Stop::ServerDown | Stop::LocalMissing)) && !Path::new(&path).exists() {
            log!("Transcribe: \"{path}\" nao existe mais — tirado da fila sem aviso.");
            result = Err(Stop::Canceled);
        }

        // SERVIDOR FORA DO AR: não é defeito do arquivo. Devolve o item para a FRENTE da
        // espera, sem contar como falha nem soltar aviso de erro por item, e tenta de novo
        // daqui a `SERVER_RETRY_MS` — ou antes, se alguém chamar `nudge_queue`.
        if matches!(result, Err(Stop::ServerDown | Stop::LocalMissing)) && !stop_set() {
            let missing = matches!(result, Err(Stop::LocalMissing));
            with(|i| {
                i.queue.insert(0, path.clone());
                i.queue_rev += 1;
                i.running = false;
                i.current_finished = true;
                i.current_path.clear();
                i.current_name.clear();
                i.current_start = None;
                i.stage = if missing { "waitingInstall" } else { "waiting" }.into();
                i.progress = -1.0;
                i.eta = -1;
                if !i.waiting_server {
                    i.waiting_server = true;
                    // Nome vazio de propósito: não é a gravação que falhou, é o servidor.
                    if missing {
                        i.last_error = lang::t("error.localAsr.notInstalled");
                        log!("Transcribe: motor local nao instalado — fila em espera.");
                    } else {
                        i.last_error = lang::t_with("error.transcribe.serverDown", &[("host", &host_base())]);
                        log!("Transcribe: servidor fora do ar — fila em espera.");
                    }
                    i.last_error_name.clear();
                }
            });
            save_queue();
            notify_changed();
            if wait_stop_or_wake(SERVER_RETRY_MS) {
                break;
            }
            continue;
        }

        with(|i| {
            i.current_finished = true;
            // Terminou de verdade (sucesso, falha ou cancelamento): a marca de manual morre
            // aqui, não quando a worker pegou o item.
            i.manual.remove(&path.to_lowercase());
            // Passou do /health: o servidor voltou. Tira a frase de espera da tela — só ela
            // (nome vazio), nunca a falha real de uma gravação.
            if i.waiting_server {
                i.waiting_server = false;
                if i.last_error_name.is_empty() {
                    i.last_error.clear();
                }
            }
            match &result {
                // Cancelado pelo usuário (ou fechamento): não conta como concluído nem como
                // falha, e não vira mensagem de erro na tela.
                Err(Stop::Canceled | Stop::ServerDown | Stop::LocalMissing) => log!("Transcribe: cancelado \"{path}\""),
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

// ---------------------------------------------------------------------------
// Diagnóstico do ambiente (aba Transcrição)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Setup {
    pub server_ok: bool,
    pub server_error: String,
    /// o host aponta para esta máquina
    pub is_local: bool,
    pub port: u16,
    pub wsl_ok: bool,
    pub docker_installed: bool,
    pub docker_running: bool,
    /// vazio = não achou o executável
    pub docker_desktop_path: String,
    /// vazio = nenhum container publica a porta
    pub container_id: String,
    pub container_image: String,
    /// `running` | `exited` | `created` | ...
    pub container_state: String,
    /// texto do docker ("Up 3 minutes")
    pub container_status: String,
    /// `always` | `no` | ...
    pub restart_policy: String,
}

/// `http://localhost:8000/` → `("localhost", 8000)`. Aceita IPv6 entre colchetes. Sem porta,
/// vale a do esquema.
fn parse_host(host: &str) -> (String, u16) {
    let mut s = host.trim();
    let mut port: u16 = if s.to_lowercase().starts_with("https://") { 443 } else { 80 };
    if let Some(p) = s.find("://") {
        s = &s[p + 3..];
    }
    if let Some(p) = s.find('/') {
        s = &s[..p];
    }
    let mut name = s.to_owned();
    if s.starts_with('[') {
        if let Some(p) = s.find(']') {
            name = s[1..p].to_owned();
            if let Some(rest) = s[p + 1..].strip_prefix(':') {
                port = rest.parse().unwrap_or(port);
            }
        }
    } else if let Some(p) = s.rfind(':') {
        name = s[..p].to_owned();
        port = s[p + 1..].parse().unwrap_or(port);
    }
    (name.trim().to_lowercase(), port)
}

fn find_in_path(exe: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(exe)).find(|f| f.is_file()))
}

/// Em ETAPAS, e cada uma só roda se a anterior passou — a tela mostra o primeiro degrau que
/// falta, com a instrução dele, e não uma lista de coisas que talvez estejam erradas:
///
/// 1. o servidor responde? sim → pronto, nada mais importa;
/// 2. o host é desta máquina? não → servidor remoto: Docker local não tem nada a ver;
/// 3. WSL instalado? (o Docker Desktop precisa dele);
/// 4. Docker instalado? 5. Docker em execução?
/// 6. há container publicando a porta? parado, rodando, de outra imagem?
///
/// O passo 6 existe para NÃO mandar o usuário rodar `docker run` de novo quando o container
/// já existe e só está parado: o segundo brigaria pela mesma porta. O container é achado pela
/// PORTA publicada, não pelo nome da imagem: quem fez build local tem `transcritor-api:latest`.
///
/// Roda processos wsl/docker e pode levar alguns segundos: chame SÓ de worker thread.
pub fn diagnose_setup(host: &str) -> Setup {
    use crate::win_proc::run_hidden;
    let mut r = Setup::default();
    match check_health(host) {
        Ok(()) => r.server_ok = true,
        Err(e) => r.server_error = e,
    }
    let (host_name, port) = parse_host(host);
    r.port = port;
    let computer = std::env::var("COMPUTERNAME").unwrap_or_default().to_lowercase();
    r.is_local = ["localhost", "127.0.0.1", "::1", "0.0.0.0"].contains(&host_name.as_str()) || (!computer.is_empty() && host_name == computer);
    if r.server_ok || !r.is_local {
        return r;
    }

    // WSL: em versões novas do Windows o wsl.exe existe como "stub" mesmo sem WSL instalado,
    // então a existência do arquivo não basta — o que decide é o `--status` sair com 0.
    let wsl = PathBuf::from(std::env::var("WINDIR").unwrap_or_default()).join("System32").join("wsl.exe");
    r.wsl_ok = wsl.is_file() && run_hidden(&wsl.to_string_lossy(), "--status", DIAG_WSL_TIMEOUT_MS).0 == 0;

    // Docker: primeiro pelo PATH, depois no lugar padrão do Docker Desktop. O PATH do NoOBS é
    // o do momento em que ele abriu — quem instalou o Docker com o app aberto não o teria ali.
    let prog_files = PathBuf::from(std::env::var("ProgramFiles").unwrap_or_default());
    let mut docker = find_in_path("docker.exe");
    let default_docker = prog_files.join("Docker\\Docker\\resources\\bin\\docker.exe");
    if docker.is_none() && default_docker.is_file() {
        docker = Some(default_docker);
    }
    let desktop = prog_files.join("Docker\\Docker\\Docker Desktop.exe");
    if desktop.is_file() {
        r.docker_desktop_path = desktop.to_string_lossy().into_owned();
    }
    r.docker_installed = docker.is_some() || !r.docker_desktop_path.is_empty();
    let Some(docker) = docker else { return r };
    let docker = docker.to_string_lossy().into_owned();

    r.docker_running = run_hidden(&docker, "info --format \"{{.ServerVersion}}\"", DIAG_DOCKER_TIMEOUT_MS).0 == 0;
    if !r.docker_running {
        return r;
    }

    let (code, out) = run_hidden(
        &docker,
        &format!("ps -a --filter \"publish={port}\" --format \"{{{{.ID}}}}|{{{{.Image}}}}|{{{{.State}}}}|{{{{.Status}}}}\""),
        DIAG_DOCKER_TIMEOUT_MS,
    );
    if code == 0 {
        // Vários parados podem publicar a mesma porta; só um roda. Prefere o que está
        // rodando, senão o primeiro listado (o mais recente).
        for line in out.split(['\r', '\n']).filter(|l| !l.is_empty()) {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 4 {
                continue;
            }
            if r.container_id.is_empty() || parts[2].trim().eq_ignore_ascii_case("running") {
                r.container_id = parts[0].trim().to_owned();
                r.container_image = parts[1].trim().to_owned();
                r.container_state = parts[2].trim().to_lowercase();
                r.container_status = parts[3].trim().to_owned();
                if r.container_state == "running" {
                    break;
                }
            }
        }
    }
    if !r.container_id.is_empty() {
        let (code, out) = run_hidden(&docker, &format!("inspect {} --format \"{{{{.HostConfig.RestartPolicy.Name}}}}\"", r.container_id), DIAG_DOCKER_TIMEOUT_MS);
        if code == 0 {
            r.restart_policy = out.trim().to_lowercase();
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_com_porta_e_esquema() {
        assert_eq!(parse_host("http://localhost:8000/"), ("localhost".into(), 8000));
        assert_eq!(parse_host("https://exemplo.com"), ("exemplo.com".into(), 443));
        assert_eq!(parse_host("http://[::1]:9000/x"), ("::1".into(), 9000));
        assert_eq!(parse_host(" 192.168.0.5:8080 "), ("192.168.0.5".into(), 8080));
    }

    #[test]
    fn erro_http_pega_o_detail_do_fastapi() {
        let r = Response { status: 422, body: br#"{"detail":[{"msg":"campo obrigatorio"}]}"#.to_vec() };
        let t = http_err_text(&r);
        assert!(t.starts_with("HTTP 422: "));
        assert!(t.contains("campo obrigatorio"));
        let r = Response { status: 400, body: br#"{"detail":"sem diarizacao"}"#.to_vec() };
        assert_eq!(http_err_text(&r), "HTTP 400: sem diarizacao");
        let r = Response { status: 500, body: Vec::new() };
        assert_eq!(http_err_text(&r), "HTTP 500");
    }

    #[test]
    fn estado_de_transcricao_so_com_stat() {
        assert_eq!(transcript_state(""), "");
    }
}

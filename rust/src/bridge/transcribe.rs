//! Transcrição no Bridge (porte do bloco "Transcricao" do `OBSBridge.pas`): estado da fila
//! para a aba, fila editável, motor local, diagnóstico do servidor, falantes, busca e a
//! lista de pastas ocultas (que divide a aba Arquivos com o "transcrever pendentes").
//!
//! O estado vive em [`crate::transcribe`] / [`crate::local_asr`]; aqui só se empurra o que
//! mudou para a UI e se traduzem as mensagens dela.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::{config, file_meta, lang, local_asr, log, mainthread, player, transcribe};

use super::library::{
    current_browse_dir, hidden_folder_names, is_file_cloud_only, is_hidden_folder_name, is_in_hidden_folder, is_path_in_record_dir,
    list_recordings, list_recordings_recursive, list_sub_folders, push_recordings,
};
use super::recording::record_dir;
use super::{is_shutting_down, post_json};

/// Chave do sub-objeto de nomes de falante no `<hash>.json`.
pub const SPEAKERS_META_KEY: &str = "speakers";

thread_local! {
    /// Última revisão de fila JÁ empurrada. A composição da fila muda raro, mas o
    /// `transcribe_state` sai a cada tique de 1 s — remandar a lista inteira junto seria
    /// desperdício.
    static LAST_QUEUE_REV: Cell<i32> = const { Cell::new(-1) };
    /// Uma varredura de duração por vez (ver `fill_queue_durations`).
    static QUEUE_DUR_BUSY: Cell<bool> = const { Cell::new(false) };
    /// Paths que JÁ passaram pelo probe. Sem esta marca a varredura reentraria para sempre:
    /// o re-push no fim dela reencontra como "sem duração" justamente os arquivos que o
    /// probe não conseguiu ler, e dispararia outra varredura.
    static QUEUE_DUR_TRIED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    /// Último estado de "rodando" visto pelo callback, para detectar a BORDA de fim de lote.
    static LAST_RUNNING: Cell<bool> = const { Cell::new(false) };
    /// Último status visto do motor local, para acordar a fila SÓ na transição para pronto.
    static LAST_LOCAL_READY: Cell<bool> = const { Cell::new(false) };
}

// ---------------------------------------------------------------------------
// Estado da fila
// ---------------------------------------------------------------------------

/// Estado da fila para a aba de Transcrição. O progresso é REAL: a API tem jobs e reporta o
/// trecho de áudio já coberto. Nada de barra estimada aqui.
pub fn push_transcribe_state() {
    let stage = transcribe::current_stage();
    post_json(json!({
        "type": "transcribe_state",
        "running": transcribe::is_running(),
        // Cancelar já pedido, item em curso ainda parando: a UI trava o botão e mostra
        // "Cancelando…".
        "canceling": transcribe::is_canceling(),
        "queue": transcribe::queue_length(),
        "total": transcribe::batch_total(),
        "done": transcribe::done_count(),
        "failed": transcribe::failed_count(),
        "current": transcribe::current_name(),
        "elapsed": transcribe::current_elapsed_sec(),
        // Progresso REAL da API (0..1). -1 = ainda sem número.
        "progress": transcribe::current_progress(),
        // Segundos restantes segundo a API; -1 enquanto ela não arrisca.
        "eta": transcribe::current_eta_sec(),
        "stage": stage,
        // Com faixas isoladas a gravação vira N jobs, e o `progress` acima já é o do CONJUNTO
        // — estes dois existem para a UI dizer QUAL faixa está rodando.
        "track": transcribe::current_track(),
        "trackCount": transcribe::current_track_count(),
        "lastError": transcribe::last_error(),
        // Sem o NOME, "1 com falha" num lote de 7 não diz QUAL gravação foi.
        "lastErrorName": transcribe::last_error_name(),
        // Fila parada esperando o servidor voltar (não é falha).
        "waiting": transcribe::waiting_for_server(),
    }));
}

/// Gravações da árvore INTEIRA que ainda não têm transcrição, e quantas ficaram de fora por
/// estarem SÓ NA NUVEM. Recursivo de propósito: o botão "transcrever pendentes" fala da
/// biblioteca, não da pasta que por acaso está aberta.
///
/// PULA O QUE ESTÁ SÓ NA NUVEM. Transcrever lê o arquivo inteiro para extrair o áudio, então
/// um placeholder do OneDrive viraria download — e "transcrever pendentes" numa biblioteca
/// sincronizada baixaria a biblioteca INTEIRA de uma vez. Trabalho em LOTE não puxa da nuvem.
/// Vale para a CONTAGEM e para o enfileiramento, que saem os dois daqui. Transcrever UMA
/// gravação pelo menu dela continua funcionando mesmo na nuvem: ali o usuário pediu.
pub fn transcribable_paths() -> (Vec<String>, usize) {
    let files = list_recordings_recursive(&record_dir().to_string_lossy());
    // Pasta oculta não entra: o usuário disse que aquilo não é dele.
    let hidden = hidden_folder_names();
    let mut cloud = 0;
    let mut out = Vec::new();
    for f in files {
        if is_in_hidden_folder(&f, &hidden) || transcribe::has_transcript(&f) {
            continue;
        }
        if is_file_cloud_only(&f) {
            cloud += 1;
            continue;
        }
        out.push(f);
    }
    if cloud > 0 {
        log!("Transcribe: {cloud} pendente(s) fora da conta — so na nuvem.");
    }
    (out, cloud)
}

pub fn push_transcribe_pending() {
    let (paths, cloud) = transcribable_paths();
    // `cloud`: pendentes que ficaram DE FORA. Sem este número, uma biblioteca inteira no
    // OneDrive mostraria "Nada pendente" com dezenas de gravações sem transcrição.
    post_json(json!({ "type": "transcribe_pending", "count": paths.len(), "cloud": cloud }));
}

/// Subpastas diretas da pasta de gravação, para a lista "Pastas ocultas" da aba Arquivos:
/// nome, se está oculta e quantas gravações tem. Oculta que não existe mais também vai
/// (`missing`), senão não haveria como desmarcar.
pub fn push_root_folders() {
    let hidden = hidden_folder_names();
    let dirs = list_sub_folders(&record_dir().to_string_lossy());
    let mut seen: Vec<String> = Vec::new();
    let mut arr: Vec<Value> = Vec::new();
    for d in &dirs {
        let name = Path::new(d).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        seen.push(name.to_lowercase());
        arr.push(json!({
            "name": name,
            "hidden": is_hidden_folder_name(&name, &hidden),
            "count": list_recordings_recursive(d).len(),
        }));
    }
    for h in &hidden {
        if !seen.contains(h) {
            arr.push(json!({ "name": h, "hidden": true, "missing": true }));
        }
    }
    post_json(json!({ "type": "root_folders", "folders": arr }));
}

pub fn handle_set_folder_hidden(name: &str, hidden: bool) {
    let key = name.trim().to_lowercase();
    if key.is_empty() || key.contains('\\') {
        return;
    }
    let mut arr: Vec<Value> = hidden_folder_names().into_iter().filter(|s| *s != key).map(Value::String).collect();
    if hidden {
        arr.push(Value::String(key));
    }
    config::set_config_json("hiddenFolders", Some(Value::Array(arr)));
    log!("Biblioteca: pasta \"{name}\" {}.", if hidden { "oculta" } else { "visivel" });
    push_recordings();
    push_transcribe_pending();
    push_root_folders();
}

// ---------------------------------------------------------------------------
// Fila visível/editável
// ---------------------------------------------------------------------------

/// A fila COMPLETA para a tela: o item em curso primeiro (marcado, e não reordenável),
/// depois os que esperam na ordem em que vão rodar. A duração sai do cache (instantâneo);
/// quem não tem vai com 0 e a UI mostra um traço, até o `fill_queue_durations` preencher — o
/// scan de metadata do startup só cobre a pasta VISÍVEL, e a fila costuma juntar gravações da
/// árvore inteira.
pub fn push_transcribe_queue() {
    let cur = transcribe::current_path();
    let waiting = transcribe::queued_paths();
    let mut missing: Vec<String> = Vec::new();
    let mut arr: Vec<Value> = Vec::new();
    let mut add = |path: &str, current: bool| {
        let (dur, _, _) = player::get_cached_meta(path);
        arr.push(json!({
            "id": path,
            "name": Path::new(path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            "duration": dur,
            "current": current,
        }));
        if dur <= 0 && !current && !QUEUE_DUR_TRIED.with(|t| t.borrow().contains(&path.to_lowercase())) {
            missing.push(path.to_owned());
        }
    };
    if !cur.is_empty() && transcribe::is_running() {
        add(&cur, true);
    }
    for w in &waiting {
        add(w, false);
    }
    post_json(json!({ "type": "transcribe_queue", "items": arr }));
    if !missing.is_empty() {
        fill_queue_durations(missing);
    }
}

/// Worker: preenche as durações que faltam (libav, pegadinha #3) e re-empurra a fila UMA vez
/// no fim. Por item seria um push por gravação — a lista inteira piscando dezenas de vezes.
fn fill_queue_durations(paths: Vec<String>) {
    if QUEUE_DUR_BUSY.with(Cell::get) || !crate::ffi::ffmpeg::ffmpeg_lib_available() {
        return;
    }
    QUEUE_DUR_BUSY.with(|b| b.set(true));
    QUEUE_DUR_TRIED.with(|t| t.borrow_mut().extend(paths.iter().map(|p| p.to_lowercase())));
    std::thread::spawn(move || {
        for p in &paths {
            if is_shutting_down() {
                break;
            }
            let _ = player::ensure_recording_meta(p);
        }
        mainthread::queue(|| {
            QUEUE_DUR_BUSY.with(|b| b.set(false));
            if is_shutting_down() {
                return;
            }
            // Força o re-push: a composição não mudou (mesma revisão), só os números que
            // faltavam.
            LAST_QUEUE_REV.with(|r| r.set(-1));
            push_transcribe_queue();
            LAST_QUEUE_REV.with(|r| r.set(transcribe::queue_revision()));
        });
    });
}

/// Callback da fila — já chega na main thread.
fn on_transcribe_changed() {
    if is_shutting_down() {
        return;
    }
    push_transcribe_state();
    // Só quando a COMPOSIÇÃO da fila mudou — o progresso sozinho não justifica remandar a
    // lista a cada segundo.
    if transcribe::queue_revision() != LAST_QUEUE_REV.with(Cell::get) {
        push_transcribe_queue();
        LAST_QUEUE_REV.with(|r| r.set(transcribe::queue_revision()));
    }
    // Lote terminou: re-lista para os selos de "transcrita" aparecerem nos cards. Só na
    // BORDA — re-listar a cada item concluído faria uma biblioteca grande piscar.
    let running = transcribe::is_running();
    if LAST_RUNNING.with(Cell::get) && !running {
        push_recordings();
        push_transcribe_pending();
    }
    LAST_RUNNING.with(|r| r.set(running));
}

/// A aba acabou de abrir: manda tudo, mesmo sem a composição ter mudado, senão a lista só
/// apareceria na próxima alteração.
pub fn handle_get_transcribe_state() {
    push_transcribe_state();
    push_transcribe_pending();
    push_transcribe_queue();
    LAST_QUEUE_REV.with(|r| r.set(transcribe::queue_revision()));
}

pub fn handle_transcribe_recording(path: &str) {
    if !is_path_in_record_dir(path) || !Path::new(path).is_file() {
        return;
    }
    // Pedido pelo menu da gravação: roda mesmo com gravação ou buffer ligado.
    transcribe::enqueue(path, true);
}

pub fn handle_transcribe_pending() {
    // Botão "transcrever pendentes": também é pedido do usuário.
    let (paths, _) = transcribable_paths();
    transcribe::enqueue_many(&paths, true, &[]);
}

pub fn handle_cancel_transcribe() {
    transcribe::cancel_all();
}

/// Reordena a ESPERA. O índice vem da posição final na lista da tela, já descontando a linha
/// do item em curso (que a UI não deixa arrastar).
pub fn handle_move_transcribe_item(path: &str, index: i32) {
    if !is_path_in_record_dir(path) {
        return;
    }
    transcribe::move_in_queue(path, index);
}

/// Tira da fila SEM apagar nada do disco — o arquivo continua lá, só não vai ser transcrito
/// neste lote.
pub fn handle_remove_transcribe_item(path: &str) {
    if !is_path_in_record_dir(path) {
        return;
    }
    if transcribe::remove_from_queue(path) {
        // A gravação volta a contar como "pendente": o botão "Transcrever pendentes" tem que
        // voltar a oferecê-la.
        push_transcribe_pending();
    }
}

// ---------------------------------------------------------------------------
// Configuração
// ---------------------------------------------------------------------------

pub fn handle_set_transcribe_host(host: &str) {
    let h = host.trim().trim_end_matches('/');
    config::set_config_str("transcribeHost", h);
    log!("TranscribeHost: \"{h}\"");
}

/// `app`, `auto` ou um código ISO de 2-3 letras. Qualquer outra coisa é ignorada — o valor
/// vai direto para o campo `language` da API.
pub fn handle_set_transcribe_language(language: &str) {
    let l = language.trim().to_lowercase();
    let ok = l == "app" || l == "auto" || ((2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_lowercase()));
    if !ok {
        log!("TranscribeLanguage: valor recusado \"{language}\"");
        return;
    }
    config::set_config_str("transcribeLanguage", &l);
    log!("TranscribeLanguage: {l} (enviado a API: \"{}\")", transcribe::resolve_transcribe_language());
}

pub fn handle_set_transcribe_on_stop(enable: bool) {
    config::set_config_bool("transcribeOnStop", enable);
    log!("TranscribeOnStop: {enable}");
}

pub fn handle_set_transcribe_engine(engine: &str) {
    if engine != "server" && engine != "local" {
        log!("TranscribeEngine: valor recusado \"{engine}\"");
        return;
    }
    config::set_config_str("transcribeEngine", engine);
    log!("TranscribeEngine: {engine}");
    // Fila parada esperando o motor anterior: tenta já com o novo.
    transcribe::nudge_queue();
    push_local_asr_state(false);
}

// ---------------------------------------------------------------------------
// Motor local
// ---------------------------------------------------------------------------

pub fn push_local_asr_state(remove_refused: bool) {
    let s = local_asr::get_state();
    let status = match s.status {
        local_asr::Status::Installing => "installing",
        local_asr::Status::Ready => "ready",
        local_asr::Status::Error => "error",
        local_asr::Status::Missing => "missing",
    };
    post_json(json!({
        "type": "local_asr_state",
        "status": status,
        "stage": s.stage,
        "done": s.bytes_done,
        "total": s.bytes_total,
        "error": s.error,
        "device": s.device,
        "engine": if transcribe::use_local_engine() { "local" } else { "server" },
        "removeRefused": remove_refused,
    }));
}

/// Callback do motor local — já chega na main thread.
fn on_local_asr_changed() {
    if is_shutting_down() {
        return;
    }
    push_local_asr_state(false);
    // Acabou de instalar: a fila que esperava pelo motor anda agora, sem aguardar o próximo
    // ciclo de 10 s. (A instalação já trocou o motor para 'local' — quem instala quer usar.)
    let ready = local_asr::get_state().status == local_asr::Status::Ready;
    if ready && !LAST_LOCAL_READY.with(Cell::get) {
        transcribe::nudge_queue();
    }
    LAST_LOCAL_READY.with(|r| r.set(ready));
}

pub fn handle_local_asr_remove() {
    // Apagar 3,7 GB leva alguns segundos: fora da main.
    std::thread::spawn(|| {
        if !local_asr::uninstall() {
            mainthread::queue(|| {
                if !is_shutting_down() {
                    push_local_asr_state(true);
                }
            });
        }
    });
}

// ---------------------------------------------------------------------------
// Servidor: teste e diagnóstico
// ---------------------------------------------------------------------------

/// Diagnóstico em worker: roda wsl.exe/docker.exe e pode levar segundos. O `seq` volta junto
/// para a UI descartar a resposta de uma verificação já superada.
pub fn handle_check_transcribe_setup(host: &str, seq: i32) {
    let host = if host.trim().is_empty() { transcribe::host_base() } else { host.trim().to_owned() };
    std::thread::spawn(move || {
        let r = transcribe::diagnose_setup(&host);
        if r.server_ok {
            transcribe::nudge_queue();
        }
        mainthread::queue(move || {
            if is_shutting_down() {
                return;
            }
            post_json(json!({
                "type": "transcribe_setup",
                "seq": seq,
                "serverOk": r.server_ok,
                "serverError": r.server_error,
                "isLocal": r.is_local,
                "port": r.port,
                "wslOk": r.wsl_ok,
                "dockerInstalled": r.docker_installed,
                "dockerRunning": r.docker_running,
                // Só o booleano: o caminho do executável não sai do Rust (o "abrir" é
                // resolvido de novo aqui, nunca vindo da UI).
                "canStartDocker": !r.docker_desktop_path.is_empty(),
                "containerId": r.container_id,
                "containerImage": r.container_image,
                "containerState": r.container_state,
                "containerStatus": r.container_status,
                "restartPolicy": r.restart_policy,
            }));
        });
    });
}

/// Alvos FIXOS. A UI escolhe a ação, nunca o caminho ou a URL — mandar ShellExecute num alvo
/// vindo da página abriria qualquer coisa.
pub fn handle_transcribe_setup_action(action: &str) {
    const DOCKER_SITE: &str = "https://www.docker.com/products/docker-desktop/";
    match action {
        "openDockerSite" => shell_open(DOCKER_SITE),
        "startDockerDesktop" => {
            let exe = Path::new(&std::env::var("ProgramFiles").unwrap_or_default()).join("Docker\\Docker\\Docker Desktop.exe");
            if exe.is_file() {
                shell_open(&exe.to_string_lossy());
            } else {
                log!("TranscribeSetup: Docker Desktop nao encontrado em \"{}\"", exe.display());
            }
        }
        _ => {}
    }
}

fn shell_open(target: &str) {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let t = crate::win::wide(target);
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(t.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// `GET /health` em worker: o servidor pode estar carregando os modelos e travar a UI pelo
/// tempo do timeout.
pub fn handle_test_transcribe_host(host: &str) {
    let host = if host.trim().is_empty() { transcribe::host_base() } else { host.trim().to_owned() };
    std::thread::spawn(move || {
        let r = transcribe::check_health(&host);
        // Servidor de pé: se a fila estava esperando por ele, não tem por que aguardar o
        // próximo ciclo.
        if r.is_ok() {
            transcribe::nudge_queue();
        }
        mainthread::queue(move || {
            if is_shutting_down() {
                return;
            }
            let err = r.err().unwrap_or_default();
            post_json(json!({ "type": "transcribe_health", "ok": err.is_empty(), "error": err }));
        });
    });
}

// ---------------------------------------------------------------------------
// Falantes, transcrição no player e busca
// ---------------------------------------------------------------------------

/// Mapa `{"SPEAKER_00": "Eduardo"}` da gravação; vazio quando ninguém nomeou ninguém ainda.
/// Mora no `<hash>.json` (meta), NÃO no `<hash>.transcript.json`: os nomes são do usuário e
/// devem sobreviver a uma retranscrição, que reescreve o arquivo da transcrição inteiro.
fn load_speaker_names(path: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(&player::load_meta_sub_object_json(path, SPEAKERS_META_KEY)) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

/// Renomeia UM falante da gravação. Nome vazio = volta para o rótulo padrão ("Falante 1"),
/// que é como o usuário desfaz.
pub fn handle_set_speaker_name(path: &str, speaker: &str, name: &str) {
    if !is_path_in_record_dir(path) || speaker.trim().is_empty() {
        return;
    }
    // Mesmo limite do rename de gravação: nome de tela, não um texto.
    let nm: String = name.trim().chars().take(60).collect();
    let mut names = load_speaker_names(path);
    names.remove(speaker);
    if !nm.is_empty() {
        names.insert(speaker.to_owned(), Value::String(nm.clone()));
    }
    player::save_meta_sub_object_json(path, SPEAKERS_META_KEY, &Value::Object(names).to_string());
    log!("Falante \"{speaker}\" de \"{}\" -> \"{nm}\"", Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    // Nome de falante é dado do usuário: vai para o fim do arquivo também.
    file_meta::mark_changed(path);
}

/// Manda para o player SÓ os TURNOS (falante, início, fim, texto). A resposta da API traz
/// também segmentos com timestamp por PALAVRA — são megabytes que o painel não usa.
pub fn handle_request_transcript(path: &str) {
    if !is_path_in_record_dir(path) {
        return;
    }
    let path = path.to_owned();
    std::thread::spawn(move || {
        let body = std::fs::read_to_string(transcribe::transcript_path(&path)).unwrap_or_default();
        // Nomes de falante escolhidos pelo usuário. Vão junto para o painel não precisar de um
        // segundo round-trip só para rotular.
        let names = load_speaker_names(&path);
        let mut turns: Vec<Value> = Vec::new();
        if let Ok(root) = serde_json::from_str::<Value>(&body) {
            for t in root.get("turns").and_then(Value::as_array).into_iter().flatten().filter(|t| t.is_object()) {
                let text = t.get("text").and_then(Value::as_str).unwrap_or("").trim().to_owned();
                if text.is_empty() {
                    continue;
                }
                turns.push(json!({
                    "speaker": t.get("speaker").and_then(Value::as_str).unwrap_or(""),
                    "start": t.get("start").and_then(Value::as_f64).unwrap_or(0.0),
                    "end": t.get("end").and_then(Value::as_f64).unwrap_or(0.0),
                    "text": text,
                }));
            }
        }
        mainthread::queue(move || {
            if is_shutting_down() {
                return;
            }
            post_json(json!({
                "type": "transcript",
                "id": path,
                // `transcribed` = o arquivo existe. `has` = tem turnos. Os dois são
                // independentes: uma gravação sem NINGUÉM falando volta transcribed=true e
                // has=false, e o painel precisa dizer "nenhuma fala", não "ainda não foi
                // transcrita".
                "transcribed": transcribe::has_transcript(&path),
                "has": !turns.is_empty(),
                "turns": turns,
                "speakers": names,
            }));
        });
    });
}

/// Varre os `.txt` (texto puro) das gravações da pasta NAVEGADA e devolve os ids que casam.
/// Roda em worker: são leituras de disco, uma por gravação. A busca é do BACKEND de
/// propósito: mandar as transcrições inteiras para o JS filtrar seriam megabytes por gravação
/// atravessando o postMessage a cada tecla.
pub fn handle_search_transcripts(query: &str) {
    let q = query.trim().to_owned();
    let dir = current_browse_dir();
    std::thread::spawn(move || {
        let mut hits: Vec<String> = Vec::new();
        if !q.is_empty() {
            let needle = q.to_lowercase();
            for f in list_recordings(&dir) {
                if is_shutting_down() {
                    break;
                }
                let Ok(txt) = std::fs::read_to_string(transcribe::transcript_text_path(&f)) else { continue };
                if !txt.is_empty() && txt.to_lowercase().contains(&needle) {
                    hits.push(f);
                }
            }
        }
        mainthread::queue(move || {
            if !is_shutting_down() {
                post_json(json!({ "type": "transcript_search", "query": q, "ids": hits }));
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Ciclo de vida
// ---------------------------------------------------------------------------

/// A fila sobrevive a reinício: o que estava pendente volta agora. Filtra o que não faz mais
/// sentido: gravação apagada ou movida pelo Explorer; já transcrita (o app fechou logo
/// depois de terminar); só na nuvem (restaurar é trabalho AUTOMÁTICO, e trabalho automático
/// não puxa arquivo do OneDrive).
fn restore_transcribe_queue() {
    let saved = transcribe::load_saved_queue();
    if saved.is_empty() {
        return;
    }
    let keep: Vec<String> = saved.iter().filter(|p| Path::new(p).is_file() && !transcribe::has_transcript(p) && !is_file_cloud_only(p)).cloned().collect();
    log!("Transcribe: fila restaurada — {} item(ns), {} descartado(s).", keep.len(), saved.len() - keep.len());
    // O `enqueue_many` regrava o arquivo, então os descartados saem dele aqui. Quem tinha sido
    // pedido à mão volta manual: senão, restaurado com a captura ligada, ele passaria a
    // esperar como se fosse automático.
    if !keep.is_empty() {
        transcribe::enqueue_many(&keep, false, &transcribe::load_saved_manual_paths());
    }
}

/// `DoInit`: só registra os callbacks (a thread sobe sozinha no primeiro `enqueue` — sem
/// transcrição pedida, nada roda) e restaura a fila salva.
pub fn do_init() {
    transcribe::set_on_changed(on_transcribe_changed);
    // Motor local: só o callback. Nada sobe nem baixa sem o usuário pedir.
    local_asr::set_on_changed(on_local_asr_changed);
    restore_transcribe_queue();
}

/// Transcrever lê a gravação INTEIRA do disco (extrair o áudio de um arquivo de vários GB) e
/// sobe dezenas de MB para o servidor. Fazer isso enquanto se grava — ou enquanto o buffer
/// guarda, que é quando tem jogo rodando — tira da máquina justamente o que a captura
/// precisa. Então a transcrição AUTOMÁTICA fica segura nos dois casos e anda quando a captura
/// termina. O que o usuário pediu à mão roda mesmo assim. Os itens NÃO se perdem: a fila é
/// persistida em disco (pegadinha #60n).
pub fn sync_pause(capturing: bool) {
    transcribe::set_paused(capturing);
}

/// Transcrição automática ao fim de uma gravação (`transcribeOnStop`, padrão ligado). Com o
/// servidor fora do ar o item espera na fila persistida — nada se perde por o Docker ainda
/// estar subindo.
pub fn enqueue_after_capture(path: &str) {
    if config::get_config_bool("transcribeOnStop", true) {
        transcribe::enqueue(path, false);
    }
}

/// Gravação excluída: a transcrição é por gravação (sem isto ficaria órfã no cache e ainda
/// apareceria na busca por texto), e esperando na fila ela sai junto, sem aviso. Se já estava
/// EM CURSO, a fila vê o arquivo sumido e encerra o item calada.
pub fn forget_recording(path: &str) {
    transcribe::delete_transcript(path);
    transcribe::remove_from_queue(path);
}

pub fn shutdown() {
    transcribe::shutdown();
    local_asr::shutdown();
}

/// Há transcrição andando? (a hibernação re-arma em vez de desistir). Esperando o servidor
/// NÃO conta: o item não está andando, e segurar o app acordado por um Docker que talvez
/// nunca suba mataria a hibernação de quem grava sem usar a transcrição.
pub fn busy_for_hibernation() -> bool {
    (transcribe::queue_length() > 0 && !transcribe::waiting_for_server()) || local_asr::get_state().status == local_asr::Status::Installing
}

#[allow(dead_code)]
fn _lang_is_used() -> String {
    lang::t("common.ok")
}

//! Transcrição LOCAL, na GPU, sem Docker nem servidor a instalar à parte (porte de
//! `OBSLocalAsr.pas`, pegadinha #60t).
//!
//! O motor é o Qwen3-ASR (texto) com o Qwen3-ForcedAligner (instante de cada
//! palavra), servidos pelo audio.cpp (<https://github.com/0xShug0/audio.cpp>) com
//! backend Vulkan — roda em placa AMD, NVIDIA e Intel sem CUDA/ROCm. Não separa
//! falantes: com faixas isoladas o falante continua vindo do nome do dispositivo.
//!
//! Três pontos que não são óbvios:
//!
//! 1. O audio.cpp NÃO tem biblioteca (DLL) — só executáveis. O motor é o
//!    `audiocpp_server.exe` rodando ESCONDIDO, numa porta livre de 127.0.0.1, subido
//!    sob demanda e derrubado depois de `IDLE_STOP_MS` sem uso (os modelos ocupam
//!    ~4 GB de memória de vídeo, que o jogo gravado precisa). Fica num Job Object com
//!    KILL_ON_JOB_CLOSE: se o NoOBS morrer de qualquer jeito, o servidor morre junto.
//! 2. A LÓGICA é a mesma da Transcritor API (`qwen/windows/app/engine.py`),
//!    traduzida: só as REGIÕES DE FALA vão para o modelo (ruído e silêncio longo viram
//!    texto inventado), em blocos de até ~60 s cortados no trecho mais silencioso,
//!    DUAS passadas (transcrever tudo, depois alinhar tudo — trocar de modelo a cada
//!    bloco custaria segundos por bloco), a pontuação do texto recolocada nas palavras
//!    do alinhador (que as devolve sem ela) e os números por extenso em algarismos
//!    ([`crate::numbers_pt`]). Mudou lá, mude aqui.
//! 3. O REAMOSTRADOR é nosso: sinc janelado (Blackman), 8 cruzamentos por lado, tabela
//!    com interpolação linear. Usar o swresample exigiria declarar structs de ABI da
//!    libav; a conta é curta e foi validada em Python antes (48 kHz → 16 kHz deu erro
//!    mediano de 22 ms no início das palavras, igual ao swresample). Filtro simétrico =
//!    sem atraso de fase — um reamostrador com atraso deslocaria TODAS as palavras.
//!
//! Arquivos em `%LOCALAPPDATA%\NoOBS\asr\` (`audiocpp\`, `models\`, `server.json`,
//! `audiocpp.log`). A instalação baixa ~3,7 GB, retoma download interrompido e confere
//! o SHA-256 de cada arquivo antes de aceitá-lo. Se o GitHub ou o HuggingFace falharem,
//! cada arquivo é buscado num espelho no Google Drive (`ENGINE_MIRROR`).

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::win_http::{self, AbortSlot, Request, Timeouts};
use crate::{config, ffmpeg_ops, lang, log, mainthread, numbers_pt, win_proc};

// --- o que se baixa (versões FIXAS, conferidas por SHA-256) ---
const ENGINE_ZIP: &str = "audio-v0.8.1-bin-windows-x64-vulkan.zip";
const ENGINE_URL: &str = "https://github.com/0xShug0/audio.cpp/releases/download/v0.8.1/audio-v0.8.1-bin-windows-x64-vulkan.zip";
const ENGINE_SHA: &str = "c787971e025ba8ef900f0482a2cc36a049367081fe89f4841aae521a0b49de32";
const ENGINE_SIZE: i64 = 58_059_664;
const SERVER_EXE: &str = "audiocpp_server.exe";
const CLI_EXE: &str = "audiocpp_cli.exe";

const ASR_FILE: &str = "qwen3-asr-1.7b-q8_0.gguf";
const ASR_URL: &str = "https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/main/Qwen3-ASR-1.7B-GGUF/qwen3-asr-1.7b-q8_0.gguf";
const ASR_SHA: &str = "da4fc2ac7f24dee784d1684eb1f35836cdbf559519452ae11777670734c0a4f8";
const ASR_SIZE: i64 = 2_473_010_048;
const ALIGN_FILE: &str = "qwen3-forced-aligner-0.6b-q8_0.gguf";
const ALIGN_URL: &str = "https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/main/Qwen3-ForcedAligner-0.6B-GGUF/qwen3-forced-aligner-0.6b-q8_0.gguf";
const ALIGN_SHA: &str = "75209490b11cec2b0db749ca5f4ff92266f58efd30f7fd04d9eb2a3ac9cc929f";
const ALIGN_SIZE: i64 = 1_129_966_496;

/// Escrito só depois de TUDO baixado, conferido e detectado. Sem ele a instalação
/// conta como incompleta, mesmo com os arquivos no lugar.
const MARKER_FILE: &str = "instalado.json";

const ASR_MODEL_ID: &str = "qwen3-asr";
const ALIGN_MODEL_ID: &str = "qwen3-align";

// --- áudio ---
const RATE: i64 = 16_000;
/// 30 ms a 16 kHz: a unidade do corte em blocos.
const FRAME: usize = 480;
/// Blocos de até ~60 s: o alinhador recusa áudio acima de ~120 s. O corte é procurado
/// nos últimos 15 s do bloco. (1999, e não 2000: é o que o `int(60 / 0.03)` do Python
/// dá, e os dois lados cortam no mesmo lugar.)
const MAX_CHUNK_FRAMES: usize = 1999;
const SEARCH_FRAMES: usize = 500;
/// Média móvel da energia antes de achar o vale.
const SMOOTH_FRAMES: usize = 10;
/// Percentil de energia tratado como silêncio.
const SILENCE_PCT: f64 = 20.0;
/// Bloco com menos que isto de quadros "com voz" é silêncio: não vai para o modelo. O
/// Qwen3 inventa texto em silêncio longo.
const MIN_VOICED: f64 = 0.05;
/// 120 ms: acima do limiar por menos que isso é estalo.
const MIN_RUN_FRAMES: usize = 4;
/// 0,5 s: região com menos fala que isso é descartada.
const MIN_SPEECH_FRAMES: usize = 17;
/// 5 s: silêncio maior que isso separa blocos.
const SPLIT_GAP_FRAMES: usize = 167;
/// 0,3 s de folga antes e depois da fala.
const PAD_FRAMES: usize = 10;

// Laço do modelo: em música ou ruído contínuo o Qwen3 às vezes repete a mesma palavra
// ou frase até o limite. Frase de até LOOP_MAX_UNIT palavras repetida LOOP_MIN_REPS
// vezes seguidas fica com LOOP_KEEP.
const LOOP_MAX_UNIT: usize = 8;
const LOOP_MIN_REPS: usize = 4;
const LOOP_KEEP: usize = 2;
/// Pontuação colada no fim da palavra (a da última repetição é mantida).
const LOOP_PUNCT: &str = ".,;:!?…»\"')]";

/// Cruzamentos de zero do sinc por lado.
const RESAMPLE_ZEROS: f64 = 8.0;
/// Pontos da tabela por amostra de entrada.
const TABLE_RES: f64 = 512.0;

/// Peso da 1ª passada no progresso; o alinhamento leva o resto.
const TRANSCRIBE_SHARE: f64 = 0.5;

// --- servidor ---
const SERVER_START_TIMEOUT_MS: u64 = 90_000;
const HEALTH_TIMEOUT_MS: i32 = 2000;
/// Espera por UM pedido. Um bloco de 60 s leva ~5 s na GPU; 3 min só estoura com o
/// servidor travado — e aí vale reiniciar e repetir, não esperar 15 min. Na CPU o bloco
/// pode passar de um minuto de verdade.
const REQUEST_TIMEOUT_GPU_MS: i32 = 3 * 60 * 1000;
const REQUEST_TIMEOUT_CPU_MS: i32 = 15 * 60 * 1000;
/// Conexão + envio dos ~2 MB do bloco para 127.0.0.1. Estourar isto é o servidor vivo
/// mas sem aceitar pedido (visto numa gravação de 2 h com o buffer do OBS usando a
/// mesma placa).
const SEND_TIMEOUT_MS: i32 = 30_000;
/// Servidor travado: derruba, sobe de novo e repete o MESMO bloco até tantas vezes.
const REQUEST_RETRIES: u32 = 2;
/// Ocioso por mais que isto, o servidor cai e libera a memória de vídeo.
const IDLE_STOP_MS: u64 = 3 * 60 * 1000;

/// Idiomas que o alinhador aceita, pelo nome que ele espera. O ASR devolve o CÓDIGO
/// quando o idioma é pedido ("pt") e o NOME quando detecta sozinho ("Portuguese").
const LANG_CODES: [&str; 11] = ["pt", "en", "es", "fr", "de", "it", "ru", "ko", "ja", "zh", "yue"];
const LANG_NAMES: [&str; 11] =
    ["Portuguese", "English", "Spanish", "French", "German", "Italian", "Russian", "Korean", "Japanese", "Chinese", "Cantonese"];

/// O audio.cpp responde 500 com esta frase quando o bloco não tem texto reconhecível.
/// Não é erro do arquivo: o bloco fica vazio e segue.
const NO_SPEECH_TEXT: &str = "did not contain transcript text";
/// O servidor responde 503 com este tipo quando o modelo não cabe na memória (da placa
/// ou do sistema). Visto numa máquina só com vídeo integrado.
const NO_MEMORY_TYPE: &str = "insufficient_memory";

// --- espelho no Google Drive ---
// Segunda fonte, para quem não alcança o GitHub ou o HuggingFace. Mesmos arquivos,
// mesmos SHA-256. O link direto do drive.usercontent aceita Range (retoma) e, com
// confirm=t, não para na página de aviso de antivírus dos arquivos grandes. O Drive não
// tem o zip do motor, só os arquivos soltos: vão os 14 que o servidor precisa. Os ids
// mudam se o arquivo for enviado de novo — aí é preciso atualizar aqui.
const DRIVE_URL: &str = "https://drive.usercontent.google.com/download?export=download&confirm=t&id=";
const ASR_DRIVE_ID: &str = "1BF6NgHuCLrrSkn8xlSbaPe6WfECoYhTb";
const ALIGN_DRIVE_ID: &str = "1jC-4mX6-TLZHUh8dL7rWLyGrJivL28kL";
const ENGINE_MIRROR_SIZE: i64 = 167_614_090;

struct MirrorFile {
    path: &'static str,
    id: &'static str,
    size: i64,
    sha: &'static str,
}

const ENGINE_MIRROR: [MirrorFile; 14] = [
    MirrorFile { path: "audiocpp_server.exe", id: "1EH-dt419IuyxIEcg-3QvFpTRQwvoViXE", size: 87_112_704, sha: "737ae419af34bfe31764cd7741fb227e3d7764e23a34224db8cce611c0f5d4c2" },
    MirrorFile { path: "audiocpp_cli.exe", id: "1u4BntT8rF4SSVEl9Jvjg54vCUvcfnZXw", size: 79_118_336, sha: "7256d03a7e708a30d4c6dbf9865425ffb6004a4c46126fb9a2350fc00ae0f713" },
    MirrorFile { path: "LICENSE", id: "15gmk-WXo8dwuGOsuPJdZVugYamK49U0W", size: 10_466, sha: "7e125adced85b7f84c04371179d15d51fbd702cefd060a37043fe38d6eb1722e" },
    MirrorFile { path: "msvcp140.dll", id: "1CNp0dZ90mdzmOBpgH8fj2E9k-locXaDR", size: 557_728, sha: "0f885b509a685d2bbfa652fed26b5fb31d88fbdab0a978c641d1c7b8aa460aa9" },
    MirrorFile { path: "msvcp140_1.dll", id: "1twhHlvSpV6Np1fckWDdfkH22GysXIHGr", size: 35_952, sha: "bfad5aef4c63a669e3c140655cdfdf395b6c979b400a447bd5dcb65ed8826c3d" },
    MirrorFile { path: "msvcp140_2.dll", id: "1_YWdULRF1F55HvSZvzbLhl-JlcVQR3Qf", size: 280_200, sha: "3ea06f0ee098b4823cb79599df3780e7f23cce52c19aac31d2a0d47efe33a5e9" },
    MirrorFile { path: "msvcp140_atomic_wait.dll", id: "1sqaL9xT0P-BHVrBGgzwqjcpjQ1FlkARX", size: 50_304, sha: "640b2aefced484d0368eea5bdd06addd0658a3a70a49256e560d6923b404a479" },
    MirrorFile { path: "msvcp140_codecvt_ids.dll", id: "1sVSL-HHpHXUujUZOV7lAJRPB0UZ9U64i", size: 31_872, sha: "f2069a52880ec885ee7f0511186100eb7fada0411a2b4948fafea7735b878a18" },
    MirrorFile { path: "vcomp140.dll", id: "1vBNF18_EOOOf9YQbJJFZldV-a0dzMg53", size: 193_152, sha: "55aba23cdcd6484fbb06f4155b8ca75adfce7a881f10afd0c49457165e677164" },
    MirrorFile { path: "vcruntime140.dll", id: "1CejUkn0s-NKjTOVLhPzeRmAS6hfxZ4lp", size: 124_544, sha: "d5e4d9a3e835fa679450145d6a7d94e36573a509317111904d9b3712c30d9066" },
    MirrorFile { path: "vcruntime140_1.dll", id: "14KDGWf9qFpzgbRvgrxCPFRB0UWIrJio0", size: 49_792, sha: "1f2d41c4aa5db0bc33ebf7b66d72943a817d7ce6cbe880502a9403823633093f" },
    MirrorFile { path: "vcruntime140_threads.dll", id: "1PU2Tk0uk5ZabwZrWIELMZilMFy27xoZZ", size: 38_528, sha: "219915cf20822f34d5e7c1fdd4e21ae7f3396881096c51036225fb8f84b47afa" },
    MirrorFile { path: "model_specs\\qwen3_asr.json", id: "13xrvMCAYpdmYuDm5E5CF7_ljIYQubg-t", size: 6_117, sha: "af83793bf6e75af3bd99c0dabd43102fe3f85ca030b9c5d12aafcc3f87cd23b4" },
    MirrorFile { path: "model_specs\\qwen3_forced_aligner.json", id: "12sJ9a7OZDVp-vqhKTO7DUFox6xIm8x0J", size: 4_395, sha: "18e4805a2224e93968892038d7c02c42ae4f75a9bc105f13293cc42aed3a0f94" },
];

// ---------------------------------------------------------------------------
// Estado público
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Missing,
    Installing,
    Ready,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub status: Status,
    /// Durante a instalação: `engine` | `asr` | `align` | `verifying` | `extracting` |
    /// `detecting` (+ `engineMirror`/`asrMirror`/`alignMirror`).
    pub stage: String,
    pub bytes_done: i64,
    pub bytes_total: i64,
    /// Motivo quando `status = Error`.
    pub error: String,
    /// GPU em uso ("AMD Radeon RX 9070 XT"); vazio = CPU.
    pub device: String,
}

/// Resultado de [`transcribe`].
#[derive(Debug)]
pub enum Outcome {
    /// JSON no MESMO formato da Transcritor API.
    Done(String),
    /// Parou pelo `cancel` (não é erro).
    Canceled,
    Failed(String),
}

#[derive(Debug)]
enum Fail {
    Canceled,
    Msg(String),
}

type Res<T = ()> = Result<T, Fail>;

fn msg<T>(s: impl Into<String>) -> Res<T> {
    Err(Fail::Msg(s.into()))
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
type Changed = std::sync::Arc<dyn Fn() + Send + Sync>;
static ON_CHANGED: Mutex<Option<Changed>> = Mutex::new(None);
static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static LAST_NOTIFY: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicI32 = AtomicI32::new(0);
static LAST_USE: AtomicU64 = AtomicU64::new(0);
/// Pedido em voo ao servidor local: o cancelamento o fecha de outra thread.
static REQ: AbortSlot = AbortSlot::new();

fn tick() -> u64 {
    static T0: OnceLock<Instant> = OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

fn touch() {
    LAST_USE.store(tick(), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Caminhos
// ---------------------------------------------------------------------------

fn root_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")).map(PathBuf::from).unwrap_or_default();
    base.join("NoOBS").join("asr")
}

fn engine_dir() -> PathBuf {
    root_dir().join("audiocpp")
}

fn models_dir() -> PathBuf {
    root_dir().join("models")
}

fn log_path() -> PathBuf {
    root_dir().join("audiocpp.log")
}

/// `-1` = não existe. Lê metadado, não abre o arquivo.
fn file_size64(p: &Path) -> i64 {
    std::fs::metadata(p).map(|m| m.len() as i64).unwrap_or(-1)
}

pub fn install_size_bytes() -> i64 {
    ENGINE_SIZE + ASR_SIZE + ALIGN_SIZE
}

/// Motor e os dois modelos presentes e conferidos.
pub fn is_installed() -> bool {
    root_dir().join(MARKER_FILE).exists()
        && engine_dir().join(SERVER_EXE).exists()
        && file_size64(&models_dir().join(ASR_FILE)) == ASR_SIZE
        && file_size64(&models_dir().join(ALIGN_FILE)) == ALIGN_SIZE
}

// ---------------------------------------------------------------------------
// Estado e notificação
// ---------------------------------------------------------------------------

/// Chamado SEMPRE na main thread.
pub fn set_on_changed(cb: impl Fn() + Send + Sync + 'static) {
    *ON_CHANGED.lock().unwrap() = Some(std::sync::Arc::new(cb));
}

/// Marshalla para a main. O download avisa a cada pedaço recebido; sem o limite de
/// 250 ms seriam milhares de mensagens por segundo para a UI.
fn notify_changed(force: bool) {
    let t0 = tick();
    if !force && t0.saturating_sub(LAST_NOTIFY.load(Ordering::Relaxed)) < 250 {
        return;
    }
    LAST_NOTIFY.store(t0, Ordering::Relaxed);
    mainthread::queue(|| {
        let cb = ON_CHANGED.lock().unwrap().clone();
        if let Some(cb) = cb {
            cb();
        }
    });
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap();
    let st = g.get_or_insert_with(|| {
        let mut s = State { bytes_total: install_size_bytes(), device: config::get_config_str("localAsrDevice", ""), ..Default::default() };
        if is_installed() {
            s.status = Status::Ready;
            s.bytes_done = s.bytes_total;
        }
        s
    });
    f(st)
}

pub fn get_state() -> State {
    with_state(|s| s.clone())
}

fn set_install_progress(stage: &str, done: i64) {
    with_state(|s| {
        s.stage = stage.to_owned();
        s.bytes_done = done;
    });
    notify_changed(false);
}

// ---------------------------------------------------------------------------
// Instalação
// ---------------------------------------------------------------------------

fn stop_requested() -> bool {
    INSTALL_CANCEL.load(Ordering::SeqCst) || SHUTTING_DOWN.load(Ordering::SeqCst)
}

fn dl_err(name: &str, err: &str) -> String {
    lang::t_with("error.localAsr.download", &[("file", name), ("error", err)])
}

/// Baixa `url` em `part`, RETOMANDO de onde parou (Range). GitHub e HuggingFace
/// redirecionam para a CDN; o WinHTTP segue o redirecionamento.
fn download_resume(url: &str, part: &Path, name: &str, size: i64, base: i64, stage: &str, scale: f64) -> Res {
    for _attempt in 1..=2 {
        let mut have = file_size64(part);
        if have > size {
            let _ = std::fs::remove_file(part);
            have = -1;
        }
        if have == size {
            return Ok(());
        }
        let mut file = if have > 0 {
            OpenOptions::new().append(true).open(part)
        } else {
            have = 0;
            OpenOptions::new().write(true).create(true).truncate(true).open(part)
        }
        .map_err(|e| Fail::Msg(dl_err(name, &e.to_string())))?;
        let have = have as u64;

        let mut req = Request::new("GET", url).header("User-Agent", "NoOBS");
        req.timeouts = Timeouts { connect_ms: 15_000, send_ms: 30_000, // Tempo sem receber NADA, não tempo total: 2,4 GB levam minutos.
            receive_ms: 60_000 };
        if have > 0 {
            req = req.header("Range", &format!("bytes={have}-"));
        }
        let mut received = 0u64;
        let mut write_err = String::new();
        let mut ignored_range = false;
        let mut bad_status = false;
        let result = win_http::stream(&req, &mut |status, chunk| {
            if stop_requested() {
                return false;
            }
            match status {
                206 => {}
                200 if have == 0 => {}
                // O servidor ignorou o Range e mandou o arquivo INTEIRO depois do pedaço que
                // já existia. Não dá para salvar: recomeça do zero.
                200 => {
                    ignored_range = true;
                    return false;
                }
                _ => {
                    bad_status = true;
                    return false;
                }
            }
            if let Err(e) = file.write_all(chunk) {
                write_err = e.to_string();
                return false;
            }
            received += chunk.len() as u64;
            set_install_progress(stage, base + ((have + received) as f64 * scale).round() as i64);
            true
        });
        drop(file);

        if stop_requested() {
            return Err(Fail::Canceled);
        }
        if !write_err.is_empty() {
            return msg(dl_err(name, &write_err));
        }
        // Status decidido também quando o corpo veio VAZIO (o `sink` nem foi chamado):
        // um 404/416 sem corpo virava "0 de N bytes" em vez do código, e um 200 vazio
        // depois de um pedaço não recomeçava como o Delphi.
        let status = match result {
            Err(e) => return msg(dl_err(name, &e.to_string())),
            Ok(status) => status,
        };
        if ignored_range || (status == 200 && have > 0) {
            log!("LocalAsr: {name} ignorou o Range — recomecando o download.");
            let _ = std::fs::remove_file(part);
            continue;
        }
        if bad_status || !(status == 200 || status == 206) {
            return msg(dl_err(name, &format!("HTTP {status}")));
        }
        let got = file_size64(part);
        if got != size {
            return msg(dl_err(name, &format!("{got} de {size} bytes")));
        }
        return Ok(());
    }
    msg(dl_err(name, "Range"))
}

fn sha256_file(p: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Baixa, confere o SHA-256 e só então põe no lugar definitivo. O arquivo final existir
/// = já foi conferido (só o `.part` pode estar pela metade).
fn fetch(stage: &str, url: &str, dest: &Path, sha: &str, size: i64, base: i64, scale: f64) -> Res {
    if file_size64(dest) == size {
        return Ok(());
    }
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let part = PathBuf::from(format!("{}.part", dest.to_string_lossy()));
    log!("LocalAsr: baixando {name}");
    download_resume(url, &part, &name, size, base, stage, scale)?;
    set_install_progress("verifying", base + (size as f64 * scale).round() as i64);
    let ok = sha256_file(&part).map(|h| h.eq_ignore_ascii_case(sha)).unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&part);
        return msg(lang::t_with("error.localAsr.checksum", &[("file", &name)]));
    }
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| Fail::Msg(dl_err(&name, &e.to_string())))
}

fn extract_err(e: &str) -> Fail {
    Fail::Msg(lang::t_with("error.localAsr.extract", &[("error", e)]))
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Some(p);
        }
    }
    None
}

fn extract_engine(zip_path: &Path) -> Res {
    let tmp = root_dir().join("audiocpp.tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    let file = std::fs::File::open(zip_path).map_err(|e| extract_err(&e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| extract_err(&e.to_string()))?;
    archive.extract(&tmp).map_err(|e| extract_err(&e.to_string()))?;
    // O zip traz os executáveis na raiz; se um dia vier numa subpasta, acha a pasta do
    // servidor em vez de falhar.
    let src = if tmp.join(SERVER_EXE).exists() {
        tmp.clone()
    } else {
        match find_file(&tmp, SERVER_EXE).and_then(|f| f.parent().map(Path::to_path_buf)) {
            Some(d) => d,
            None => return Err(extract_err(&format!("{SERVER_EXE}?"))),
        }
    };
    let _ = std::fs::remove_dir_all(engine_dir());
    std::fs::rename(&src, engine_dir()).map_err(|e| extract_err(&e.to_string()))?;
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_file(zip_path);
    Ok(())
}

/// O motor pelo Google Drive, arquivo a arquivo (o Drive não tem o zip). Pasta própria:
/// o `extract_engine` apaga a `audiocpp.tmp`, e o que o espelho já baixou tem de sobrar
/// para retomar.
fn fetch_engine_mirror() -> Res {
    let tmp = root_dir().join("audiocpp.mirror");
    let scale = ENGINE_SIZE as f64 / ENGINE_MIRROR_SIZE as f64;
    let mut done = 0i64;
    for f in &ENGINE_MIRROR {
        let dest = tmp.join(f.path);
        if let Some(d) = dest.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        fetch("engineMirror", &format!("{DRIVE_URL}{}", f.id), &dest, f.sha, f.size, (done as f64 * scale).round() as i64, scale)?;
        done += f.size;
    }
    let _ = std::fs::remove_dir_all(engine_dir());
    std::fs::rename(&tmp, engine_dir()).map_err(|e| extract_err(&e.to_string()))?;
    // Sobra do zip do GitHub que não chegou a completar.
    let _ = std::fs::remove_file(root_dir().join(format!("{ENGINE_ZIP}.part")));
    let _ = std::fs::remove_file(root_dir().join(ENGINE_ZIP));
    Ok(())
}

/// Falhou a fonte principal (qualquer motivo que não seja cancelamento): tenta o
/// espelho. Falhando os dois, a mensagem leva os dois motivos.
fn with_mirror(primary: Res, what: &str, mirror: impl FnOnce() -> Res) -> Res {
    let Err(Fail::Msg(first)) = primary else { return primary };
    log!("LocalAsr: {what} falhou na fonte principal ({first}) — tentando o Google Drive.");
    match mirror() {
        Err(Fail::Msg(second)) => msg(format!("{first} | Google Drive: {second}")),
        other => other,
    }
}

/// Pergunta ao próprio audio.cpp que placas ele enxerga. Linha que importa:
/// `Vulkan:0 "AMD Radeon RX 9070 XT" [GPU]`. Sem Vulkan, roda na CPU — funciona, só que
/// ~10× mais devagar. Devolve `(backend, device)`.
fn detect_device() -> Res<(String, String)> {
    let (code, output) = win_proc::run_hidden(&engine_dir().join(CLI_EXE).to_string_lossy(), "--list-devices", 60_000);
    log!("LocalAsr: --list-devices saiu com {code}");
    // -1/erro de carga: o executável nem abriu (tipicamente falta a vulkan-1.dll, que
    // vem com o driver de vídeo).
    if code != 0 {
        return msg(lang::t("error.localAsr.noVulkan"));
    }
    for line in output.split('\n') {
        if line.trim().starts_with("Vulkan:") {
            let device = line.split('"').nth(1).unwrap_or("").to_owned();
            return Ok(("vulkan".into(), device));
        }
    }
    Ok(("cpu".into(), String::new()))
}

fn install_worker() {
    let mut backend = String::new();
    let mut device = String::new();
    let root = root_dir();
    let result: Res = (|| {
        std::fs::create_dir_all(models_dir()).map_err(|e| Fail::Msg(e.to_string()))?;
        if !engine_dir().join(SERVER_EXE).exists() {
            let zip = root.join(ENGINE_ZIP);
            let mut r = fetch("engine", ENGINE_URL, &zip, ENGINE_SHA, ENGINE_SIZE, 0, 1.0);
            if r.is_ok() {
                set_install_progress("extracting", ENGINE_SIZE);
                r = extract_engine(&zip);
            }
            with_mirror(r, "o motor", fetch_engine_mirror)?;
        }
        // Modelos: o .part que parou no meio pelo HuggingFace continua pelo Drive — são
        // os mesmos bytes.
        let asr = models_dir().join(ASR_FILE);
        with_mirror(fetch("asr", ASR_URL, &asr, ASR_SHA, ASR_SIZE, ENGINE_SIZE, 1.0), ASR_FILE, || {
            fetch("asrMirror", &format!("{DRIVE_URL}{ASR_DRIVE_ID}"), &asr, ASR_SHA, ASR_SIZE, ENGINE_SIZE, 1.0)
        })?;
        let align = models_dir().join(ALIGN_FILE);
        with_mirror(fetch("align", ALIGN_URL, &align, ALIGN_SHA, ALIGN_SIZE, ENGINE_SIZE + ASR_SIZE, 1.0), ALIGN_FILE, || {
            fetch("alignMirror", &format!("{DRIVE_URL}{ALIGN_DRIVE_ID}"), &align, ALIGN_SHA, ALIGN_SIZE, ENGINE_SIZE + ASR_SIZE, 1.0)
        })?;
        set_install_progress("detecting", install_size_bytes());
        (backend, device) = detect_device()?;
        config::set_config_str("localAsrBackend", &backend);
        config::set_config_str("localAsrDevice", &device);
        config::set_config_int("localAsrMaxModels", 2);
        let marker = json!({ "engine": ENGINE_ZIP, "asr": ASR_FILE, "align": ALIGN_FILE, "backend": backend, "device": device });
        std::fs::write(root.join(MARKER_FILE), marker.to_string()).map_err(|e| Fail::Msg(e.to_string()))?;
        // Quem instalou quer usar: vira o motor da transcrição na hora.
        config::set_config_str("transcribeEngine", "local");
        log!("LocalAsr: instalado ({backend}{}).", if device.is_empty() { String::new() } else { format!(", {device}") });
        Ok(())
    })();

    with_state(|s| {
        match &result {
            Err(Fail::Canceled) => {
                s.status = Status::Missing;
                s.error.clear();
                log!("LocalAsr: instalacao cancelada (o que ja baixou fica pra retomar).");
            }
            Err(Fail::Msg(e)) => {
                s.status = Status::Error;
                s.error = e.clone();
                log!("LocalAsr: instalacao falhou: {e}");
            }
            Ok(()) => {
                s.status = Status::Ready;
                s.error.clear();
                s.bytes_done = s.bytes_total;
                s.device = device.clone();
            }
        }
        s.stage.clear();
    });
    notify_changed(true);
}

/// Instala em worker thread. No-op se já instalado ou instalando.
pub fn start_install() {
    // Teste: sem baixar 3,7 GB.
    #[cfg(test)]
    {
        crate::testkit::record_effect("local_asr_install");
        return;
    }
    #[allow(unreachable_code)]
    let go = with_state(|s| {
        if matches!(s.status, Status::Installing | Status::Ready) {
            return false;
        }
        s.status = Status::Installing;
        s.error.clear();
        s.stage = "engine".into();
        s.bytes_done = 0;
        INSTALL_CANCEL.store(false, Ordering::SeqCst);
        true
    });
    if !go {
        return;
    }
    notify_changed(true);
    std::thread::spawn(|| {
        if std::panic::catch_unwind(install_worker).is_err() {
            with_state(|s| {
                s.status = Status::Error;
                s.error = "panic".into();
                s.stage.clear();
            });
            notify_changed(true);
        }
    });
}

pub fn cancel_install() {
    INSTALL_CANCEL.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Servidor
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Server {
    /// Handle do processo do `audiocpp_server` (0 = parado).
    proc: usize,
    port: u16,
}

static SERVER: Mutex<Server> = Mutex::new(Server { proc: 0, port: 0 });
/// Job Object com KILL_ON_JOB_CLOSE: o handle morre com o NoOBS (fechado, travado,
/// derrubado pelo Gerenciador de Tarefas) e o Windows mata o servidor junto. Sem isso
/// sobraria um processo segurando 4 GB de VRAM.
static JOB: Mutex<usize> = Mutex::new(0);

mod sys {
    pub use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    pub use windows::Win32::Security::SECURITY_ATTRIBUTES;
    pub use windows::Win32::Storage::FileSystem::{CreateFileW, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_READ};
    pub use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    pub use windows::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        PROCESS_INFORMATION, STARTF_USESHOWWINDOW, STARTF_USESTDHANDLES, STARTUPINFOW,
    };
    pub use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
}

fn handle(h: usize) -> sys::HANDLE {
    sys::HANDLE(h as *mut std::ffi::c_void)
}

/// Porta livre de 127.0.0.1: o SO escolhe (bind na porta 0) e a gente solta. Entre soltar
/// e o servidor pegar há uma corrida teórica; se perder, o servidor não sobe e a próxima
/// tentativa escolhe outra.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(0)
}

fn health_ok(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    let url = format!("http://127.0.0.1:{port}/health");
    let t = Timeouts { connect_ms: HEALTH_TIMEOUT_MS, send_ms: HEALTH_TIMEOUT_MS, receive_ms: HEALTH_TIMEOUT_MS };
    matches!(win_http::request(&Request::new("GET", &url).timeouts(t).no_proxy()), Ok(r) if r.status == 200)
}

fn process_alive(s: &Server) -> bool {
    s.proc != 0 && unsafe { sys::WaitForSingleObject(handle(s.proc), 0) } == sys::WAIT_TIMEOUT
}

/// Caller segura o lock do servidor.
fn stop_server_locked(s: &mut Server) {
    if s.proc != 0 {
        unsafe {
            if sys::WaitForSingleObject(handle(s.proc), 0) == sys::WAIT_TIMEOUT {
                let _ = sys::TerminateProcess(handle(s.proc), 0);
                sys::WaitForSingleObject(handle(s.proc), 3000);
                log!("LocalAsr: servidor encerrado.");
            }
            let _ = sys::CloseHandle(handle(s.proc));
        }
        s.proc = 0;
    }
    s.port = 0;
}

fn ensure_job() -> usize {
    let mut j = JOB.lock().unwrap();
    if *j != 0 {
        return *j;
    }
    unsafe {
        let Ok(job) = sys::CreateJobObjectW(None, windows::core::PCWSTR::null()) else { return 0 };
        let mut info = sys::JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = sys::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Err(e) = sys::SetInformationJobObject(
            job,
            sys::JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<sys::JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) {
            log!("LocalAsr: SetInformationJobObject falhou ({e}).");
        }
        *j = job.0 as usize;
    }
    *j
}

fn write_server_config(port: u16, backend: &str) {
    let path = |f: &str| models_dir().join(f).to_string_lossy().replace('\\', "/");
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let cfg = json!({
        "host": "127.0.0.1",
        "port": port,
        "backend": backend,
        "device": 0,
        "threads": threads,
        "lazy_load": true,
        // Os dois modelos juntos na memória: é o que evita recarregar a cada troca entre a
        // passada de transcrição e a de alinhamento.
        "max_loaded_models": config::get_config_int("localAsrMaxModels", 2).clamp(1, 2),
        "idle_unload_ms": 0,
        "min_free_memory_mb": 0,
        "models": [
            { "id": ASR_MODEL_ID, "family": "qwen3_asr", "task": "asr", "mode": "offline", "path": path(ASR_FILE) },
            { "id": ALIGN_MODEL_ID, "family": "qwen3_forced_aligner", "task": "align", "mode": "offline", "path": path(ALIGN_FILE) },
        ],
    });
    let _ = std::fs::write(root_dir().join("server.json"), cfg.to_string());
}

fn server_start_err() -> Fail {
    Fail::Msg(lang::t_with("error.localAsr.serverStart", &[("log", &log_path().to_string_lossy())]))
}

/// Sobe o servidor se ele não está de pé. A espera checa o cancelamento e o fechamento a
/// cada 250 ms, então o `shutdown` nunca fica preso aqui.
fn ensure_server(cancel: Option<&dyn Fn() -> bool>) -> Res {
    let mut s = SERVER.lock().unwrap();
    if process_alive(&s) && health_ok(s.port) {
        return Ok(());
    }
    stop_server_locked(&mut s);
    s.port = free_port();
    if s.port == 0 {
        return Err(server_start_err());
    }
    let backend = config::get_config_str("localAsrBackend", "vulkan");
    write_server_config(s.port, &backend);

    unsafe {
        // stdout/stderr num arquivo: é onde o audio.cpp diz POR QUE não subiu (driver,
        // memória de vídeo, modelo). Recriado a cada subida.
        let sa = sys::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<sys::SECURITY_ATTRIBUTES>() as u32,
            bInheritHandle: true.into(),
            ..Default::default()
        };
        let log_w = crate::win::wide(&log_path().to_string_lossy());
        let log_h = sys::CreateFileW(
            windows::core::PCWSTR(log_w.as_ptr()),
            sys::FILE_GENERIC_WRITE.0,
            sys::FILE_SHARE_READ,
            Some(&sa),
            sys::CREATE_ALWAYS,
            sys::FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok();
        let mut si = sys::STARTUPINFOW { cb: std::mem::size_of::<sys::STARTUPINFOW>() as u32, ..Default::default() };
        si.dwFlags = sys::STARTF_USESHOWWINDOW;
        si.wShowWindow = sys::SW_HIDE.0 as u16;
        if let Some(h) = log_h {
            si.dwFlags |= sys::STARTF_USESTDHANDLES;
            si.hStdOutput = h;
            si.hStdError = h;
        }
        let mut cmd = crate::win::wide(&format!(
            "\"{}\" --config \"{}\" --no-ui",
            engine_dir().join(SERVER_EXE).to_string_lossy(),
            root_dir().join("server.json").to_string_lossy()
        ));
        let cwd = crate::win::wide(&engine_dir().to_string_lossy());
        let mut pi = sys::PROCESS_INFORMATION::default();
        // SUSPENSO até entrar no Job: senão ele poderia rodar (e deixar filhos) fora do
        // alcance do KILL_ON_JOB_CLOSE.
        let created = sys::CreateProcessW(
            windows::core::PCWSTR::null(),
            Some(windows::core::PWSTR(cmd.as_mut_ptr())),
            None,
            None,
            log_h.is_some(),
            sys::CREATE_NO_WINDOW | sys::CREATE_SUSPENDED,
            None,
            windows::core::PCWSTR(cwd.as_ptr()),
            &si,
            &mut pi,
        );
        if let Some(h) = log_h {
            let _ = sys::CloseHandle(h);
        }
        if let Err(e) = created {
            log!("LocalAsr: CreateProcess falhou ({e}).");
            return Err(server_start_err());
        }
        let job = ensure_job();
        if job != 0 {
            let _ = sys::AssignProcessToJobObject(handle(job), pi.hProcess);
        }
        sys::ResumeThread(pi.hThread);
        let _ = sys::CloseHandle(pi.hThread);
        s.proc = pi.hProcess.0 as usize;
    }
    log!("LocalAsr: servidor iniciando na porta {} ({backend}).", s.port);

    let start = Instant::now();
    loop {
        if SHUTTING_DOWN.load(Ordering::SeqCst) || cancel.is_some_and(|c| c()) {
            stop_server_locked(&mut s);
            return Err(Fail::Canceled);
        }
        unsafe {
            if sys::WaitForSingleObject(handle(s.proc), 0) == sys::WAIT_OBJECT_0 {
                let mut code = 0u32;
                let _ = sys::GetExitCodeProcess(handle(s.proc), &mut code);
                log!("LocalAsr: servidor terminou ao subir (codigo {code}). Ver {}", log_path().display());
                stop_server_locked(&mut s);
                return Err(server_start_err());
            }
        }
        if health_ok(s.port) {
            break;
        }
        if start.elapsed() > Duration::from_millis(SERVER_START_TIMEOUT_MS) {
            stop_server_locked(&mut s);
            return Err(server_start_err());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    touch();
    Ok(())
}

/// Derruba o servidor se está ocioso há `IDLE_STOP_MS`. Chamado pela worker da transcrição
/// quando a fila está vazia (barato: só compara tempos).
pub fn stop_if_idle() {
    if BUSY.load(Ordering::SeqCst) > 0 {
        return;
    }
    let mut s = SERVER.lock().unwrap();
    if s.proc == 0 || BUSY.load(Ordering::SeqCst) > 0 {
        return;
    }
    if tick().saturating_sub(LAST_USE.load(Ordering::Relaxed)) >= IDLE_STOP_MS {
        log!("LocalAsr: ocioso — derrubando o servidor pra liberar a memoria de video.");
        stop_server_locked(&mut s);
    }
}

/// Apaga motor e modelos. `false` (sem fazer nada) se estiver instalando ou transcrevendo.
pub fn uninstall() -> bool {
    if with_state(|s| s.status == Status::Installing) || BUSY.load(Ordering::SeqCst) > 0 {
        return false;
    }
    stop_server_locked(&mut SERVER.lock().unwrap());
    if let Err(e) = std::fs::remove_dir_all(root_dir()) {
        if root_dir().exists() {
            log!("LocalAsr: falha ao apagar {}: {e}", root_dir().display());
        }
    }
    // Sempre explícito: o padrão da chave é o motor LOCAL, então só apagar a escolha
    // deixaria a fila esperando uma instalação que não existe mais.
    config::set_config_str("transcribeEngine", "server");
    with_state(|s| {
        s.status = Status::Missing;
        s.stage.clear();
        s.error.clear();
        s.bytes_done = 0;
        s.device.clear();
    });
    log!("LocalAsr: motor local removido.");
    notify_changed(true);
    true
}

/// Derruba o servidor e para a instalação. Chamado no fechamento.
pub fn shutdown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    INSTALL_CANCEL.store(true, Ordering::SeqCst);
    *ON_CHANGED.lock().unwrap() = None;
    stop_server_locked(&mut SERVER.lock().unwrap());
    let mut j = JOB.lock().unwrap();
    if *j != 0 {
        unsafe {
            let _ = sys::CloseHandle(handle(*j));
        }
        *j = 0;
    }
}

/// Interrompe NA HORA o pedido em voo ao servidor local (um bloco leva ~5 s na GPU e ~30 s
/// na CPU): fecha o handle do WinHTTP de outra thread, e a chamada síncrona volta com
/// erro. Quem transcreve vê o cancelamento e não repete o bloco. O servidor continua de
/// pé para a próxima transcrição.
pub fn abort_request() {
    if REQ.abort() {
        log!("LocalAsr: pedido em voo interrompido (cancelamento).");
    }
}

// ---------------------------------------------------------------------------
// Reamostragem (ver o cabeçalho, ponto 3)
// ---------------------------------------------------------------------------

/// Reamostrador de fluxo: recebe blocos na taxa original e acumula a saída em 16 kHz,
/// 16 bits. Guarda da entrada só a janela ainda necessária.
#[derive(Default)]
struct Resampler {
    ready: bool,
    src_rate: i32,
    /// entrada por saída (48000/16000 = 3)
    ratio: f64,
    /// meia largura do filtro, em amostras de entrada
    width: f64,
    table: Vec<f64>,
    /// Razão inteira (48k, 32k): os instantes de saída caem em amostras inteiras e os pesos
    /// são sempre os mesmos — calculados uma vez.
    int_ratio: i64,
    int_taps: Vec<f64>,
    int_half: i64,
    buf: Vec<f32>,
    /// índice absoluto de `buf[0]`
    buf_start: i64,
    buf_len: usize,
    total_in: i64,
    next_out: i64,
    output: Vec<i16>,
}

impl Resampler {
    fn setup(&mut self, rate: i32) {
        self.src_rate = rate;
        self.ratio = rate as f64 / RATE as f64;
        // Corte em 90% da Nyquist de SAÍDA quando desce (anti-aliasing); na subida, 90% da
        // Nyquist de entrada.
        let fc = if self.ratio > 1.0 { 0.9 / self.ratio } else { 0.9 };
        self.width = RESAMPLE_ZEROS / fc;
        let n = (self.width * TABLE_RES).ceil() as usize + 2;
        self.table = (0..n)
            .map(|j| {
                let d = j as f64 / TABLE_RES;
                if d >= self.width {
                    return 0.0;
                }
                let x = fc * d;
                let s = if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                let u = d / self.width;
                let win = 0.42 + 0.5 * (std::f64::consts::PI * u).cos() + 0.08 * (2.0 * std::f64::consts::PI * u).cos();
                fc * s * win
            })
            .collect();
        self.int_ratio = 0;
        if rate as i64 % RATE == 0 {
            self.int_ratio = rate as i64 / RATE;
            self.int_half = self.width.floor() as i64;
            let half = self.int_half;
            self.int_taps = (-half..=half).map(|k| if (k.abs() as f64) < self.width { self.kernel(k.abs() as f64) } else { 0.0 }).collect();
        }
        self.buf = vec![0.0; 1 << 16];
        self.buf_len = 0;
        self.buf_start = 0;
        self.total_in = 0;
        self.next_out = 0;
        self.output = Vec::with_capacity(RATE as usize * 60 * 5);
        self.ready = true;
    }

    fn kernel(&self, d: f64) -> f64 {
        let p = d * TABLE_RES;
        let j = p.trunc() as usize;
        if j + 1 >= self.table.len() {
            return 0.0;
        }
        let f = p - j as f64;
        self.table[j] + (self.table[j + 1] - self.table[j]) * f
    }

    fn emit(&mut self, v: f64) {
        let i = (v * 32767.0).round_ties_even() as i64;
        self.output.push(i.clamp(-32768, 32767) as i16);
    }

    fn produce(&mut self, finalize: bool) {
        let total_out = (self.total_in as f64 / self.ratio).trunc() as i64;
        loop {
            if finalize && self.next_out >= total_out {
                break;
            }
            let t = self.next_out as f64 * self.ratio;
            let hi = (t + self.width).floor() as i64;
            // Sem amostras suficientes à frente: espera o próximo bloco.
            if !finalize && hi >= self.buf_start + self.buf_len as i64 {
                break;
            }
            let mut acc = 0.0f64;
            if self.int_ratio > 0 {
                // Razão inteira: T é inteiro, pesos fixos.
                let i = self.next_out * self.int_ratio;
                for c in -self.int_half..=self.int_half {
                    let last = i + c;
                    if last < 0 || last >= self.total_in {
                        continue;
                    }
                    let k = last - self.buf_start;
                    if k < 0 || k >= self.buf_len as i64 {
                        continue;
                    }
                    acc += self.buf[k as usize] as f64 * self.int_taps[(c + self.int_half) as usize];
                }
            } else {
                let lo = (t - self.width).ceil() as i64;
                let mut i = lo;
                while i <= hi {
                    // Fora do sinal conta como zero.
                    if i >= 0 && i < self.total_in {
                        let k = i - self.buf_start;
                        let d = (i as f64 - t).abs();
                        if k >= 0 && k < self.buf_len as i64 && d < self.width {
                            acc += self.buf[k as usize] as f64 * self.kernel(d);
                        }
                    }
                    i += 1;
                }
            }
            self.emit(acc);
            self.next_out += 1;
        }
        // Descarta a entrada que nenhuma saída futura vai usar. Em lotes grandes, para não
        // mover memória a cada bloco.
        let keep = (self.next_out as f64 * self.ratio - self.width).floor() as i64 - 1;
        let mut drop = keep - self.buf_start;
        if drop > 65536 {
            if drop > self.buf_len as i64 {
                drop = self.buf_len as i64;
            }
            let d = drop as usize;
            self.buf.copy_within(d..self.buf_len, 0);
            self.buf_len -= d;
            self.buf_start += drop;
        }
    }

    fn push(&mut self, data: &[f32], rate: i32) -> bool {
        if !self.ready {
            self.setup(rate);
        } else if rate != self.src_rate {
            // Taxa mudando no meio do arquivo não acontece no que o OBS grava.
            log!("LocalAsr: taxa mudou no meio do audio ({} -> {rate}) — abortando.", self.src_rate);
            return false;
        }
        if self.buf_len + data.len() > self.buf.len() {
            let new_len = (self.buf.len() * 2).max(self.buf_len + data.len());
            self.buf.resize(new_len, 0.0);
        }
        self.buf[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
        self.buf_len += data.len();
        self.total_in += data.len() as i64;
        self.produce(false);
        true
    }

    fn finish(&mut self) {
        if self.ready {
            self.produce(true);
        }
    }
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Chunk {
    /// em quadros de `FRAME` amostras
    start_f: usize,
    end_f: usize,
    text: String,
    /// código (`pt`) ou nome em minúsculas
    lang: String,
    aligned: bool,
    words: Vec<Value>,
}

/// Parte `[start, end)` em blocos de até `MAX_CHUNK_FRAMES`, cortando no vale da energia
/// suavizada nos últimos `SEARCH_FRAMES` de cada um. Janela `[i-5, i+4]`: o `mode='same'`
/// do `np.convolve` com 10 pontos.
fn split_long(rms: &[f64], start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = start;
    while start < end {
        let stop = if end - start <= MAX_CHUNK_FRAMES {
            end
        } else {
            let lo = start + MAX_CHUNK_FRAMES - SEARCH_FRAMES;
            let (mut best, mut best_v) = (0usize, f64::MAX);
            for i in 0..SEARCH_FRAMES {
                let mut v = 0.0;
                for j in (i as i64 - 5)..=(i as i64 + 4) {
                    if j >= 0 && (j as usize) < SEARCH_FRAMES {
                        v += rms[lo + j as usize];
                    }
                }
                v /= SMOOTH_FRAMES as f64;
                if v < best_v {
                    best_v = v;
                    best = i;
                }
            }
            lo + best
        };
        out.push((start, stop));
        start = stop;
    }
    out
}

/// Mesma conta do `energy_chunks` do Python (ver o cabeçalho, ponto 2):
/// 1. quadro "alto" = energia acima do dobro do percentil 20 (o chão de ruído);
/// 2. sequência alta mais curta que `MIN_RUN_FRAMES` é estalo, não conta;
/// 3. regiões separadas por menos de `SPLIT_GAP_FRAMES` ficam no mesmo bloco;
/// 4. região com menos de `MIN_SPEECH_FRAMES` de fala é descartada;
/// 5. região longa é partida no vale de energia perto de ~60 s.
fn energy_chunks(s: &[i16]) -> Vec<Chunk> {
    let nf = s.len() / FRAME;
    if nf == 0 {
        return Vec::new();
    }
    let rms: Vec<f64> = (0..nf)
        .map(|f| {
            let sum: f64 = s[f * FRAME..(f + 1) * FRAME].iter().map(|&v| v as f64 * v as f64).sum();
            (sum / FRAME as f64).sqrt()
        })
        .collect();
    // Percentil com interpolação linear, igual ao numpy.percentile.
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let ppos = SILENCE_PCT / 100.0 * (nf - 1) as f64;
    let i = ppos.floor() as usize;
    let frac = ppos - i as f64;
    let mut silence = sorted[i];
    if i + 1 < nf {
        silence += (sorted[i + 1] - sorted[i]) * frac;
    }
    let is_loud: Vec<bool> = rms.iter().map(|&r| r > silence * 2.0).collect();

    // Sequências altas longas o bastante, juntadas em regiões: (início, fim, quadros de fala).
    let mut regions: Vec<(usize, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < nf {
        if !is_loud[i] {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < nf && is_loud[j] {
            j += 1;
        }
        if j - i >= MIN_RUN_FRAMES {
            match regions.last_mut() {
                Some(r) if i - r.1 < SPLIT_GAP_FRAMES => {
                    r.1 = j;
                    r.2 += j - i;
                }
                _ => regions.push((i, j, j - i)),
            }
        }
        i = j;
    }

    let mut list = Vec::new();
    for (ri, rj, rk) in regions {
        if rk < MIN_SPEECH_FRAMES {
            continue;
        }
        for (a, b) in split_long(&rms, ri.saturating_sub(PAD_FRAMES), (rj + PAD_FRAMES).min(nf)) {
            let loud = is_loud[a..b].iter().filter(|&&l| l).count();
            if loud as f64 / (b - a) as f64 >= MIN_VOICED {
                list.push(Chunk { start_f: a, end_f: b, ..Default::default() });
            }
        }
    }
    list
}

/// WAV PCM 16 bits mono 16 kHz em memória (cabeçalho de 44 bytes).
fn wav_bytes(s: &[i16]) -> Vec<u8> {
    let data_len = (s.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + s.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // tamanho do bloco fmt
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&(RATE as u32).to_le_bytes());
    b.extend_from_slice(&(RATE as u32 * 2).to_le_bytes()); // bytes por segundo
    b.extend_from_slice(&2u16.to_le_bytes()); // bytes por quadro
    b.extend_from_slice(&16u16.to_le_bytes()); // bits por amostra
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for v in s {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

fn request_timeout_ms() -> i32 {
    if config::get_config_str("localAsrBackend", "vulkan").eq_ignore_ascii_case("cpu") {
        REQUEST_TIMEOUT_CPU_MS
    } else {
        REQUEST_TIMEOUT_GPU_MS
    }
}

/// `{"error":{"message":"..."}}` → a mensagem; senão o corpo encurtado.
fn error_message_of(body: &str) -> String {
    let short: String = body.chars().take(300).collect();
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or(short)
}

/// Motivo de uma falha de [`post_audio`] que o chamador trata diferente das demais.
enum PostErr {
    /// Nem chegou resposta (conexão recusada, tempo esgotado): é o servidor, não o bloco.
    Transport(String),
    NoMemory(String),
    NoSpeech,
    Other(String),
}

/// POST multipart com o bloco em WAV. `fields` = pares nome, valor (valor vazio não vai).
fn post_audio(route: &str, fields: &[(&str, &str)], wav: &[u8]) -> Result<Value, PostErr> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let boundary = format!("----NoOBS{nanos:032x}");
    let mut payload: Vec<u8> = Vec::with_capacity(wav.len() + 1024);
    for (name, value) in fields {
        if !value.is_empty() {
            payload.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        }
    }
    payload.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"bloco.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes(),
    );
    payload.extend_from_slice(wav);
    payload.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let port = SERVER.lock().unwrap().port;
    let url = format!("http://127.0.0.1:{port}{route}");
    let t = Timeouts { connect_ms: SEND_TIMEOUT_MS, send_ms: SEND_TIMEOUT_MS, receive_ms: request_timeout_ms() };
    let req = Request::new("POST", &url)
        .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .body(&payload)
        .timeouts(t)
        .no_proxy()
        .abort_slot(&REQ);
    let resp = match win_http::request(&req) {
        Ok(r) => r,
        Err(e) => return Err(PostErr::Transport(lang::t_with("error.localAsr.request", &[("error", &e.to_string())]))),
    };
    let body = resp.text();
    if resp.status == 503 && body.contains(NO_MEMORY_TYPE) {
        return Err(PostErr::NoMemory(error_message_of(&body)));
    }
    if resp.status != 200 && body.contains(NO_SPEECH_TEXT) {
        return Err(PostErr::NoSpeech);
    }
    if resp.status != 200 {
        let short: String = body.chars().take(300).collect();
        return Err(PostErr::Other(lang::t_with("error.localAsr.request", &[("error", &format!("HTTP {} {short}", resp.status))])));
    }
    match serde_json::from_str::<Value>(&body) {
        Ok(v @ Value::Object(_)) => {
            touch();
            Ok(v)
        }
        _ => {
            let short: String = body.chars().take(300).collect();
            Err(PostErr::Other(lang::t_with("error.localAsr.request", &[("error", &short)])))
        }
    }
}

/// Memória insuficiente: desce UM degrau e fica nele (config), para a próxima transcrição
/// já começar onde cabe: GPU, 2 modelos → GPU, 1 modelo → CPU, 2 modelos → CPU, 1 modelo.
/// Um modelo por vez custa quase nada: as duas passadas trocam de modelo UMA vez por
/// arquivo. A CPU custa ~10× (bloco de 60 s em 30 s, contra ~2 s na RX 9070 XT). `false` =
/// já estava no último degrau.
fn step_down_memory(reason: &str) -> bool {
    let on_cpu = config::get_config_str("localAsrBackend", "vulkan").eq_ignore_ascii_case("cpu");
    if config::get_config_int("localAsrMaxModels", 2) > 1 {
        config::set_config_int("localAsrMaxModels", 1);
    } else if !on_cpu {
        config::set_config_str("localAsrBackend", "cpu");
        config::set_config_str("localAsrDevice", "");
        config::set_config_int("localAsrMaxModels", 2);
        with_state(|s| s.device.clear());
        notify_changed(true);
    } else {
        return false;
    }
    log!(
        "LocalAsr: memoria insuficiente ({reason}) — agora {} com {} modelo(s) carregado(s).",
        config::get_config_str("localAsrBackend", "vulkan"),
        config::get_config_int("localAsrMaxModels", 2)
    );
    true
}

/// `post_audio` com recuperação, repetindo o MESMO bloco:
/// * servidor sem responder (travou — visto com o buffer do OBS disputando a placa numa
///   gravação de 2 h): derruba, sobe de novo, até `REQUEST_RETRIES`;
/// * memória insuficiente: desce um degrau, sobe o servidor no degrau novo e tenta de novo.
fn post_chunk(route: &str, fields: &[(&str, &str)], wav: &[u8], cancel: Option<&dyn Fn() -> bool>) -> Res<Option<Value>> {
    let restart = || -> Res {
        stop_server_locked(&mut SERVER.lock().unwrap());
        ensure_server(cancel)
    };
    let mut attempt = 0;
    loop {
        let r = post_audio(route, fields, wav);
        if SHUTTING_DOWN.load(Ordering::SeqCst) {
            return Err(Fail::Canceled);
        }
        // Cancelado (o abort derrubou o pedido, ou chegou durante ele): nada de repetir o
        // bloco nem reiniciar o servidor.
        if cancel.is_some_and(|c| c()) {
            return Err(Fail::Canceled);
        }
        match r {
            Ok(v) => return Ok(Some(v)),
            Err(PostErr::NoSpeech) => return Ok(None),
            Err(PostErr::Other(m)) => return msg(m),
            Err(PostErr::NoMemory(reason)) => {
                if !step_down_memory(&reason) {
                    return msg(lang::t_with("error.localAsr.memory", &[("detail", &reason)]));
                }
                restart()?;
            }
            Err(PostErr::Transport(m)) => {
                if attempt >= REQUEST_RETRIES {
                    return msg(m);
                }
                attempt += 1;
                log!("LocalAsr: servidor sem resposta ({m}) — reiniciando e repetindo o bloco ({attempt} de {REQUEST_RETRIES}).");
                restart()?;
            }
        }
    }
}

/// `Portuguese` → `pt`; `pt` → `pt`; idioma fora da tabela: minúsculas.
fn lang_code(l: &str) -> String {
    let l = l.trim().to_lowercase();
    LANG_NAMES.iter().position(|n| n.to_lowercase() == l).map_or(l, |k| LANG_CODES[k].to_owned())
}

/// Nome que o alinhador espera, ou vazio se ele não cobre o idioma.
fn align_name(code: &str) -> &'static str {
    let code = code.to_lowercase();
    if let Some(k) = LANG_CODES.iter().position(|c| *c == code) {
        return LANG_NAMES[k];
    }
    let two: String = code.chars().take(2).collect();
    LANG_CODES.iter().position(|c| *c == two).map_or("", |k| LANG_NAMES[k])
}

/// Como o `str.split()` do Python: separa em espaço e descarta vazios.
fn split_words(text: &str) -> Vec<&str> {
    text.split([' ', '\t', '\n', '\r']).filter(|s| !s.is_empty()).collect()
}

/// Forma de comparação: minúsculas, só letras e dígitos (acento incluso).
fn word_key(w: &str) -> String {
    w.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    Equal,
    Replace,
    Delete,
    Insert,
}

#[derive(Debug, Clone, Copy)]
struct Opcode {
    tag: Tag,
    a0: usize,
    a1: usize,
    b0: usize,
    b1: usize,
}

/// Porte do `difflib.SequenceMatcher(autojunk=False).get_opcodes()`: maior trecho comum,
/// recursivo dos dois lados, e as diferenças entre eles. Conferido em Python contra o
/// difflib: 3000 casos aleatórios idênticos.
fn opcodes(a: &[String], b: &[String]) -> Vec<Opcode> {
    // (i, j, k)
    let mut blocks: Vec<(usize, usize, usize)> = Vec::new();
    let mut pending: Vec<[usize; 4]> = vec![[0, a.len(), 0, b.len()]];
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    // LIFO, como a lista do difflib (pop do fim).
    while let Some([alo, ahi, blo, bhi]) = pending.pop() {
        // find_longest_match: comprimento da sequência comum terminando em (i, j); em
        // empate fica a de menor i, depois menor j.
        let (mut besti, mut bestj, mut best) = (alo, blo, 0usize);
        prev.iter_mut().for_each(|v| *v = 0);
        for i in alo..ahi {
            cur.iter_mut().for_each(|v| *v = 0);
            for j in blo..bhi {
                if a[i] == b[j] {
                    let k = if j > blo { prev[j - 1] + 1 } else { 1 };
                    cur[j] = k;
                    if k > best {
                        besti = i + 1 - k;
                        bestj = j + 1 - k;
                        best = k;
                    }
                }
            }
            prev.copy_from_slice(&cur);
        }
        if best > 0 {
            blocks.push((besti, bestj, best));
            if alo < besti && blo < bestj {
                pending.push([alo, besti, blo, bestj]);
            }
            if besti + best < ahi && bestj + best < bhi {
                pending.push([besti + best, ahi, bestj + best, bhi]);
            }
        }
    }
    blocks.sort();
    // Junta blocos encostados.
    let mut merged: Vec<(usize, usize, usize)> = Vec::new();
    let mut m = (0usize, 0usize, 0usize);
    for bl in blocks {
        if m.0 + m.2 == bl.0 && m.1 + m.2 == bl.1 {
            m.2 += bl.2;
        } else {
            if m.2 > 0 {
                merged.push(m);
            }
            m = bl;
        }
    }
    if m.2 > 0 {
        merged.push(m);
    }
    merged.push((a.len(), b.len(), 0));

    let mut ops = Vec::new();
    let (mut ii, mut jj) = (0usize, 0usize);
    for (bi, bj, bk) in merged {
        let tag = if ii < bi && jj < bj {
            Some(Tag::Replace)
        } else if ii < bi {
            Some(Tag::Delete)
        } else if jj < bj {
            Some(Tag::Insert)
        } else {
            None
        };
        if let Some(tag) = tag {
            ops.push(Opcode { tag, a0: ii, a1: bi, b0: jj, b1: bj });
        }
        ii = bi + bk;
        jj = bj + bk;
        if bk > 0 {
            ops.push(Opcode { tag: Tag::Equal, a0: bi, a1: ii, b0: bj, b1: jj });
        }
    }
    ops
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn word_obj(word: &str, time: Option<(f64, f64)>) -> Value {
    let mut o = Map::new();
    o.insert("word".into(), json!(word));
    if let Some((s, e)) = time {
        o.insert("start".into(), json!(round3(s)));
        o.insert("end".into(), json!(round3(e)));
    }
    Value::Object(o)
}

/// Palavras do texto ORIGINAL (com pontuação) com o tempo do alinhador, que as devolve sem
/// pontuação e às vezes partidas ("A.C." → "A", "C"). Os dois lados são casados pela forma
/// sem pontuação (`word_key`).
fn attach_times(text: &str, aligned: &[Value], offset: f64) -> Vec<Value> {
    let tokens = split_words(text);
    let ak: Vec<String> = tokens.iter().map(|t| word_key(t)).collect();
    let bk: Vec<String> = aligned.iter().map(|w| word_key(w.get("word").and_then(Value::as_str).unwrap_or(""))).collect();
    let mut times: Vec<Option<(f64, f64)>> = vec![None; tokens.len()];

    let mut take_time = |idx: usize, from_b: usize, to_b: usize| {
        let (Some(w1), Some(w2)) = (aligned.get(from_b), aligned.get(to_b)) else { return };
        let Some(s1) = w1.get("start").and_then(Value::as_f64) else { return };
        let e1 = w2.get("end").and_then(Value::as_f64).unwrap_or(s1);
        times[idx] = Some((s1, e1));
    };

    for op in opcodes(&ak, &bk) {
        if op.tag == Tag::Equal || (op.tag == Tag::Replace && op.a1 - op.a0 == op.b1 - op.b0) {
            for k in 0..op.a1 - op.a0 {
                take_time(op.a0 + k, op.b0 + k, op.b0 + k);
            }
        } else if op.tag == Tag::Replace && op.a1 - op.a0 == 1 {
            let joined: String = bk[op.b0..op.b1].concat();
            if joined == ak[op.a0] {
                take_time(op.a0, op.b0, op.b1 - 1);
            }
        }
    }
    tokens.iter().zip(&times).map(|(t, tm)| word_obj(t, tm.map(|(s, e)| (round3(s) + offset, round3(e) + offset)))).collect()
}

/// Idioma sem alinhador: reparte o bloco pelas palavras, pelo tamanho.
fn spread_times(text: &str, start: f64, end: f64) -> Vec<Value> {
    let tokens = split_words(text);
    let total = tokens.iter().map(|t| t.chars().count() + 1).sum::<usize>().max(1);
    let mut cursor = start;
    tokens
        .iter()
        .map(|t| {
            let span = (end - start) * (t.chars().count() + 1) as f64 / total as f64;
            let w = word_obj(t, Some((cursor, cursor + span)));
            cursor += span;
            w
        })
        .collect()
}

fn normalize_spaces(text: &str) -> String {
    split_words(text).join(" ")
}

fn strip_trail(w: &str) -> &str {
    w.trim_end_matches(|c| LOOP_PUNCT.contains(c))
}

/// Corta repetições em laço ("vai vai vai vai vai" → "vai vai"). Compara sem pontuação e
/// sem caixa (`word_key`); a pontuação final da última repetição fica ("sim, sim, sim, sim."
/// → "sim, sim."). Mesma regra do `collapse_loops` do Python.
fn collapse_loops(text: &str) -> String {
    let w = split_words(text);
    let keys: Vec<String> = w.iter().map(|x| word_key(x)).collect();
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < w.len() {
        let (mut best_n, mut best_reps) = (0usize, 0usize);
        for n in 1..=LOOP_MAX_UNIT {
            if i + n > w.len() {
                break;
            }
            if keys[i..i + n].iter().all(String::is_empty) {
                continue;
            }
            let mut reps = 1;
            let mut j = i + n;
            while j + n <= w.len() && (0..n).all(|k| keys[j + k] == keys[i + k]) {
                reps += 1;
                j += n;
            }
            if reps >= LOOP_MIN_REPS && reps * n > best_reps * best_n {
                best_n = n;
                best_reps = reps;
            }
        }
        if best_n == 0 {
            parts.push(w[i].to_owned());
            i += 1;
            continue;
        }
        for k in i..i + best_n * LOOP_KEEP {
            parts.push(w[k].to_owned());
        }
        let last = w[i + best_n * best_reps - 1];
        let kept = parts.last().cloned().unwrap_or_default();
        let tail = &last[strip_trail(last).len()..];
        *parts.last_mut().unwrap() = format!("{}{tail}", strip_trail(&kept));
        i += best_n * best_reps;
    }
    parts.join(" ")
}

/// Transcreve UM arquivo de áudio (qualquer formato que a libav leia). `language` = código
/// ISO (`pt`, `en`) ou vazio para detectar. `on_progress` recebe etapa (`decoding` |
/// `starting` | `transcribing` | `aligning`) e a fração 0..1 do arquivo inteiro, na thread de
/// quem transcreve; `cancel` devolve `true` para parar.
pub fn transcribe(audio_path: &str, language: &str, on_progress: Option<&dyn Fn(&str, f64)>, cancel: Option<&dyn Fn() -> bool>) -> Outcome {
    match transcribe_inner(audio_path, language, on_progress, cancel) {
        Ok(body) => Outcome::Done(body),
        Err(Fail::Canceled) => Outcome::Canceled,
        Err(Fail::Msg(m)) => Outcome::Failed(m),
    }
}

fn transcribe_inner(audio_path: &str, language: &str, on_progress: Option<&dyn Fn(&str, f64)>, cancel: Option<&dyn Fn() -> bool>) -> Res<String> {
    if !is_installed() {
        return msg(lang::t("error.localAsr.notInstalled"));
    }
    // "Portuguese" vira "pt": o alinhador e a conversão de números procuram o código, e é o
    // código que vai no JSON.
    let req_lang = lang_code(language);
    let stop = || SHUTTING_DOWN.load(Ordering::SeqCst) || cancel.is_some_and(|c| c());
    let report = |stage: &str, frac: f64| {
        if let Some(p) = on_progress {
            p(stage, frac.clamp(0.0, 1.0));
        }
    };

    struct BusyGuard;
    impl Drop for BusyGuard {
        fn drop(&mut self) {
            touch();
            BUSY.fetch_sub(1, Ordering::SeqCst);
        }
    }
    BUSY.fetch_add(1, Ordering::SeqCst);
    let _busy = BusyGuard;

    // 1. Áudio → 16 kHz mono, sem guardar a taxa original inteira.
    report("decoding", 0.0);
    let mut rs = Resampler::default();
    let decoded = ffmpeg_ops::decode_audio_mono(audio_path, &mut |data, rate| !stop() && rs.push(data, rate));
    if !decoded {
        if stop() {
            return Err(Fail::Canceled);
        }
        return msg(lang::t("error.localAsr.decode"));
    }
    rs.finish();
    let duration = rs.output.len() as f64 / RATE as f64;
    let mut chunks = energy_chunks(&rs.output);
    log!("LocalAsr: {duration:.1} s de audio, {} blocos com fala.", chunks.len());
    let n = chunks.len();

    if n > 0 {
        report("starting", 0.0);
        ensure_server(cancel)?;
    }
    let chunk_wav = |c: &Chunk| wav_bytes(&rs.output[c.start_f * FRAME..c.end_f * FRAME]);

    // 2. Primeira passada: texto de todos os blocos.
    for i in 0..n {
        if stop() {
            return Err(Fail::Canceled);
        }
        report("transcribing", TRANSCRIBE_SHARE * i as f64 / n as f64);
        let wav = chunk_wav(&chunks[i]);
        let reply = post_chunk("/v1/audio/transcriptions/details", &[("model", ASR_MODEL_ID), ("language", &req_lang)], &wav, cancel)?;
        let Some(obj) = reply else {
            // Bloco sem fala reconhecível: fica vazio e o arquivo segue.
            chunks[i].text.clear();
            chunks[i].lang = req_lang.clone();
            continue;
        };
        let mut l = lang_code(obj.get("language").and_then(Value::as_str).unwrap_or(""));
        if l.is_empty() {
            l = req_lang.clone();
        }
        let mut text = collapse_loops(&normalize_spaces(obj.get("text").and_then(Value::as_str).unwrap_or("")));
        if l.starts_with("pt") {
            text = numbers_pt::to_digits(&text, 10);
        }
        chunks[i].text = text;
        chunks[i].lang = l;
    }

    // 3. Segunda passada: o instante de cada palavra.
    for i in 0..n {
        if stop() {
            return Err(Fail::Canceled);
        }
        report("aligning", TRANSCRIBE_SHARE + (1.0 - TRANSCRIBE_SHARE) * i as f64 / n as f64);
        let start_s = (chunks[i].start_f * FRAME) as f64 / RATE as f64;
        let end_s = (chunks[i].end_f * FRAME) as f64 / RATE as f64;
        let name = align_name(&chunks[i].lang);
        if chunks[i].text.is_empty() {
            chunks[i].words = Vec::new();
        } else if !name.is_empty() {
            let wav = chunk_wav(&chunks[i]);
            let text = chunks[i].text.clone();
            let reply = post_chunk("/v1/audio/alignments", &[("model", ALIGN_MODEL_ID), ("language", name), ("text", &text)], &wav, cancel)?;
            let words = reply.as_ref().and_then(|o| o.get("words")).and_then(Value::as_array).cloned().unwrap_or_default();
            chunks[i].words = attach_times(&text, &words, start_s);
            chunks[i].aligned = true;
        } else {
            chunks[i].words = spread_times(&chunks[i].text, start_s, end_s);
        }
    }
    report("aligning", 1.0);

    // 4. JSON no formato da Transcritor API (sem falantes).
    let mut segs = Vec::new();
    let mut langs: HashMap<String, usize> = HashMap::new();
    let (mut any_aligned, mut all_text) = (false, String::new());
    for c in &mut chunks {
        if !c.lang.is_empty() {
            *langs.entry(c.lang.clone()).or_insert(0) += 1;
        }
        if c.text.is_empty() {
            continue;
        }
        segs.push(json!({
            "id": segs.len(),
            "start": round3((c.start_f * FRAME) as f64 / RATE as f64),
            "end": round3((c.end_f * FRAME) as f64 / RATE as f64),
            "speaker": Value::Null,
            "text": c.text,
            "aligned": c.aligned,
            "words": std::mem::take(&mut c.words),
        }));
        any_aligned |= c.aligned;
        if !all_text.is_empty() {
            all_text.push(' ');
        }
        all_text.push_str(&c.text);
    }
    // Empate no idioma: o mais frequente vence; entre iguais, o de nome menor (determinístico).
    let best_lang = langs.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map(|(k, _)| k.clone());
    let root = json!({
        "language": best_lang,
        "duration": round3(duration),
        "alignment": any_aligned,
        "diarization": false,
        "num_speakers": 0,
        "speakers": [],
        "text": all_text.trim(),
        "turns": [],
        "by_speaker": {},
        "segments": segs,
        "engine": "noobs:qwen3",
    });
    Ok(root.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn codigo_e_nome_de_idioma() {
        assert_eq!(lang_code("Portuguese"), "pt");
        assert_eq!(lang_code(" pt "), "pt");
        assert_eq!(lang_code("Klingon"), "klingon");
        assert_eq!(align_name("pt"), "Portuguese");
        assert_eq!(align_name("pt-BR"), "Portuguese");
        assert_eq!(align_name("xx"), "");
    }

    #[test]
    fn laco_repetido_e_cortado() {
        assert_eq!(collapse_loops("vai vai vai vai vai"), "vai vai");
        assert_eq!(collapse_loops("sim, sim, sim, sim."), "sim, sim.");
        assert_eq!(collapse_loops("um dois tres"), "um dois tres");
        assert_eq!(collapse_loops("a b a b a b a b c"), "a b a b c");
    }

    #[test]
    fn opcodes_como_o_difflib() {
        // difflib: a=[a b c d], b=[a x c d] → equal(0,1), replace(1,2), equal(2,4)
        let ops = opcodes(&s(&["a", "b", "c", "d"]), &s(&["a", "x", "c", "d"]));
        let tags: Vec<_> = ops.iter().map(|o| (o.tag, o.a0, o.a1, o.b0, o.b1)).collect();
        assert_eq!(tags, vec![(Tag::Equal, 0, 1, 0, 1), (Tag::Replace, 1, 2, 1, 2), (Tag::Equal, 2, 4, 2, 4)]);
    }

    #[test]
    fn tempo_vai_para_a_palavra_com_pontuacao() {
        let aligned = vec![json!({"word": "Ola", "start": 0.5, "end": 0.9}), json!({"word": "mundo", "start": 1.0, "end": 1.4})];
        let w = attach_times("Olá, mundo!", &aligned, 10.0);
        // "Olá," e "Ola" diferem pelo acento, mas é uma troca 1 por 1: leva o tempo dela.
        assert_eq!(w[0]["word"], "Olá,");
        assert_eq!(w[0]["start"], json!(10.5));
        assert_eq!(w[1]["word"], "mundo!");
        assert_eq!(w[1]["start"], json!(11.0));
        assert_eq!(w[1]["end"], json!(11.4));
    }

    #[test]
    fn palavra_partida_pelo_alinhador() {
        // "A.C." → "A", "C"
        let aligned = vec![json!({"word": "A", "start": 1.0, "end": 1.1}), json!({"word": "C", "start": 1.1, "end": 1.3})];
        let w = attach_times("A.C.", &aligned, 0.0);
        assert_eq!(w[0]["start"], json!(1.0));
        assert_eq!(w[0]["end"], json!(1.3));
    }

    #[test]
    fn wav_tem_cabecalho_de_44_bytes() {
        let w = wav_bytes(&[1, 2, 3]);
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 6);
    }

    #[test]
    fn reamostra_48k_para_16k_mantem_a_duracao() {
        let mut rs = Resampler::default();
        let input: Vec<f32> = (0..48_000).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin() * 0.5).collect();
        for chunk in input.chunks(1024) {
            assert!(rs.push(chunk, 48_000));
        }
        rs.finish();
        assert_eq!(rs.output.len(), 16_000);
        // Tom puro de 0,5 de amplitude: o pico fica perto de 16384.
        let peak = rs.output.iter().map(|v| v.unsigned_abs()).max().unwrap();
        assert!((15_000..17_500).contains(&peak), "pico {peak}");
    }

    #[test]
    fn silencio_nao_gera_bloco() {
        assert!(energy_chunks(&vec![0i16; FRAME * 100]).is_empty());
    }

    #[test]
    fn fala_no_meio_do_silencio_vira_um_bloco() {
        let mut s = vec![0i16; FRAME * 400];
        // 1 s de "fala" (tom alto) no meio.
        for (i, v) in s.iter_mut().enumerate().skip(FRAME * 150).take(FRAME * 34) {
            *v = if i % 2 == 0 { 8000 } else { -8000 };
        }
        // Ruído de fundo baixo para o percentil não ser zero.
        for (i, v) in s.iter_mut().enumerate() {
            if *v == 0 {
                *v = if i % 2 == 0 { 3 } else { -3 };
            }
        }
        let c = energy_chunks(&s);
        assert_eq!(c.len(), 1);
        assert!(c[0].start_f <= 150 && c[0].end_f >= 184);
    }
}

//! Montagem da transcrição a partir da resposta da API (porte do miolo do
//! `OBSTranscribe.pas`): palavras → dedup de eco entre faixas → turnos, e a
//! transcrição herdada pelo arquivo exportado.
//!
//! Tudo aqui é função pura sobre JSON/strings — sem rede, sem fila.

use serde_json::{json, Map, Value};

use crate::log;

// --- deduplicação entre faixas, POR TURNO (rede de segurança) ---
// Com faixas isoladas, o que sai pelos alto-falantes VOLTA pelo microfone (a menos que
// o usuário use fone). Dois testes, os DOIS obrigatórios — cada um sozinho tem falso
// positivo óbvio: só tempo derrubaria duas pessoas falando junto, e só texto derrubaria
// uma repetição legítima minutos depois. Só vale para faixa sem timestamp por palavra.
const DEDUP_OVERLAP_MIN: f64 = 0.5; // fração do turno mais curto em comum
const DEDUP_SIM_MIN: f64 = 0.7; // fração das palavras do turno CURTO no longo
/// Turno de uma palavra só não entra: "sim"/"certo" está contido em quase qualquer frase.
const DEDUP_MIN_WORDS: usize = 2;

// --- deduplicação POR PALAVRA ---
// Casa a MESMA palavra nas duas faixas no MESMO instante, forma corridas tolerando
// palavras faltando (o eco chega picado) e derruba a cópia. Valores escolhidos por
// varredura sobre 624 palavras REAIS de uma ligação, em 6 cenários.
const WDEDUP_TOL: f64 = 0.8; // s entre a mesma palavra nas duas faixas
const WDEDUP_MAXSKIP: i64 = 3; // palavras puladas entre dois pares da corrida
const WDEDUP_RUN_GAP: f64 = 1.5; // s de silêncio que quebra a corrida
const WDEDUP_MIN_RUN: usize = 4; // corrida LONGA: eco com certeza
/// Corrida CURTA (resposta de 1-3 palavras) só conta se for SIMULTÂNEA — quem repete a
/// fala do outro fala DEPOIS; o eco acontece no mesmo instante do original.
const WDEDUP_MIN_SHORT: usize = 2; // 1 palavra só coincide por acaso ("oi", "tá")
const WDEDUP_SHORT_TOL: f64 = 0.35; // s: a folga apertada da corrida curta
const WDEDUP_PAD: f64 = 0.5; // s em volta da corrida curta para achar "sobra"

// --- recorte de turnos a partir das PALAVRAS ---
// O `turns` da API agrupa por MUDANÇA DE FALANTE; numa faixa isolada a faixa inteira
// vira um turno único. Por isso os turnos são remontados a partir do timestamp POR
// PALAVRA. Corta em: troca de falante, pausa, fim de frase depois de um tempo, e um teto
// duro para fala contínua que nunca respira.
const TURN_GAP_SEC: f64 = 1.0; // silêncio que já separa duas falas
const TURN_SOFT_MAX_SEC: f64 = 8.0; // a partir daqui, corta no fim de frase
const TURN_HARD_MAX_SEC: f64 = 20.0; // fala contínua: corta de qualquer jeito

#[derive(Debug, Clone, Default)]
pub struct Turn {
    /// rótulo final (nome da faixa, + falante se houver)
    pub speaker: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// de qual faixa veio — o dedup só cruza faixas
    pub track: usize,
    pub drop: bool,
}

/// Uma palavra alinhada de UMA faixa. É a unidade do dedup: comparar turnos inteiros
/// apagava fala legítima junto com o eco (4,8% das palavras legítimas perdidas contra
/// 0,3% por palavra).
#[derive(Debug, Clone, Default)]
pub struct TimedWord {
    /// como vai para o turno (inclui o texto de palavras sem timestamp que vieram logo antes)
    pub text: String,
    /// SÓ a própria palavra normalizada; vazio = fora do casamento
    pub norm: String,
    pub speaker: String,
    pub start: f64,
    pub end: f64,
    /// confiança do alinhador: o eco chega mais fraco
    pub score: f64,
    /// segmento sem NENHUMA palavra alinhada, inteiro
    pub is_segment: bool,
    pub drop: bool,
}

/// Minúsculas, só letras/dígitos, espaço único. A pontuação do Whisper varia entre duas
/// passagens do MESMO trecho (o eco chega mais fraco), então compará-la geraria falso
/// negativo no dedup.
pub fn normalize_for_compare(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_owned()
}

/// CONTENÇÃO: quantas das palavras do turno CURTO aparecem no longo. Não é Dice, e a
/// diferença decide o caso que mais importa: o eco PARCIAL ("e adiar tudo" dentro de uma
/// frase longa dá 0,33 no Dice e 1,00 aqui).
fn word_similarity(a: &str, b: &str) -> f64 {
    let wa: Vec<&str> = a.split(' ').filter(|s| !s.is_empty()).collect();
    let wb: Vec<&str> = b.split(' ').filter(|s| !s.is_empty()).collect();
    if wa.is_empty() || wb.is_empty() {
        return 0.0;
    }
    let shortest = wa.len().min(wb.len());
    if shortest < DEDUP_MIN_WORDS {
        return 0.0;
    }
    let mut used = vec![false; wb.len()];
    let mut common = 0;
    for x in &wa {
        if let Some(j) = (0..wb.len()).find(|&j| !used[j] && wb[j] == *x) {
            used[j] = true;
            common += 1;
        }
    }
    common as f64 / shortest as f64
}

/// Fração do turno mais CURTO coberta pela interseção. Dividir pelo mais curto é
/// deliberado: o eco costuma sair picado em turnos menores.
fn time_overlap_ratio(a: &Turn, b: &Turn) -> f64 {
    let inter = a.end.min(b.end) - a.start.max(b.start);
    if inter <= 0.0 {
        return 0.0;
    }
    let shortest = (a.end - a.start).min(b.end - b.start);
    if shortest <= 0.0 {
        return 0.0;
    }
    inter / shortest
}

/// Última letra útil é ponto final / interrogação / exclamação.
fn ends_sentence(s: &str) -> bool {
    matches!(s.trim_end().chars().last(), Some('.' | '?' | '!'))
}

/// Junta a palavra ao texto do turno inserindo o espaço SÓ quando falta. O formato mudou
/// com o WhisperX: o faster-whisper devolvia a palavra com o espaço na frente (" Bom"), o
/// alinhador devolve ela pelada ("Bom"). Somar direto emendava tudo; somar sempre um
/// espaço dobraria no formato antigo, que ainda aparece nos segmentos que não passaram
/// pelo alinhamento.
pub fn append_word(dest: &mut String, word: &str) {
    if word.is_empty() {
        return;
    }
    if !dest.is_empty() && !dest.ends_with(' ') && !word.starts_with(' ') {
        dest.push(' ');
    }
    dest.push_str(word);
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn string(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_owned()
}

/// Lê `segments[].words` de UMA resposta da API, na ordem do tempo.
///
/// Dois casos da v2 que moldam isto (pegadinha #60a):
/// * palavra SEM start/end (número, símbolo que o alinhador não reconheceu): o texto
///   dela entra na PRÓXIMA palavra alinhada, sem mexer em tempo nenhum;
/// * segmento sem nenhuma palavra alinhada: vira uma pseudo-palavra com o tempo e o
///   texto do próprio segmento (o fallback do `_build_turns` da API), fora do dedup.
pub fn collect_words(obj: &Value) -> Vec<TimedWord> {
    let mut words: Vec<TimedWord> = Vec::new();
    let Some(segs) = obj.get("segments").and_then(Value::as_array) else { return words };
    for seg in segs.iter().filter(|s| s.is_object()) {
        let seg_first = words.len();
        let mut pending = String::new();
        if let Some(ws) = seg.get("words").and_then(Value::as_array) {
            for w in ws.iter().filter(|w| w.is_object()) {
                let wtext = string(w, "word");
                if wtext.trim().is_empty() {
                    continue;
                }
                let Some(wstart) = num(w, "start") else {
                    append_word(&mut pending, &wtext);
                    continue;
                };
                let wend = num(w, "end").unwrap_or(wstart);
                let mut spk = string(w, "speaker");
                // A API não põe speaker na palavra em toda configuração.
                if spk.is_empty() {
                    spk = string(seg, "speaker");
                }
                // `score` na v2; `probability` nas respostas antigas guardadas.
                let score = num(w, "score").or_else(|| num(w, "probability")).unwrap_or(0.0);
                let mut text = String::new();
                append_word(&mut text, &pending);
                append_word(&mut text, &wtext);
                pending.clear();
                words.push(TimedWord { text, norm: normalize_for_compare(&wtext), speaker: spk, start: wstart, end: wend, score, ..Default::default() });
            }
        }
        if words.len() > seg_first {
            // Sobrou pendente no fim do segmento: vai para a última palavra dele.
            if !pending.is_empty() {
                let last = words.last_mut().unwrap();
                append_word(&mut last.text, &pending);
            }
        } else {
            let seg_text = string(seg, "text");
            if !seg_text.trim().is_empty() {
                words.push(TimedWord {
                    text: seg_text.trim().to_owned(),
                    speaker: string(seg, "speaker"),
                    start: num(seg, "start").unwrap_or(0.0),
                    end: num(seg, "end").unwrap_or(0.0),
                    is_segment: true,
                    ..Default::default()
                });
            }
        }
    }
    words
}

/// Remonta os turnos a partir das palavras que SOBRARAM do dedup.
pub fn turns_from_word_list(words: &[TimedWord], track: usize) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut cur = Turn::default();
    let mut have = false;
    let flush = |turns: &mut Vec<Turn>, cur: &mut Turn, have: &mut bool| {
        if !*have {
            return;
        }
        cur.text = cur.text.trim().to_owned();
        if !cur.text.is_empty() {
            turns.push(cur.clone());
        }
        *have = false;
    };
    for w in words {
        if w.drop {
            continue;
        }
        if w.is_segment {
            // Segmento sem alinhamento: turno próprio, com o tempo dele.
            flush(&mut turns, &mut cur, &mut have);
            cur = Turn { speaker: w.speaker.clone(), start: w.start, end: w.end, text: w.text.clone(), track, drop: false };
            have = true;
            flush(&mut turns, &mut cur, &mut have);
            continue;
        }
        if have
            && (w.speaker != cur.speaker
                || w.start - cur.end > TURN_GAP_SEC
                || w.end - cur.start > TURN_HARD_MAX_SEC
                || (w.end - cur.start > TURN_SOFT_MAX_SEC && ends_sentence(&cur.text)))
        {
            flush(&mut turns, &mut cur, &mut have);
        }
        if !have {
            cur = Turn { speaker: w.speaker.clone(), start: w.start, end: 0.0, text: String::new(), track, drop: false };
            have = true;
        }
        append_word(&mut cur.text, &w.text);
        cur.end = w.end;
    }
    flush(&mut turns, &mut cur, &mut have);
    turns
}

/// Caminho de UMA faixa (mistura): palavras → turnos, sem dedup.
fn turns_from_words(obj: &Value, track: usize) -> Vec<Turn> {
    turns_from_word_list(&collect_words(obj), track)
}

/// Texto puro para a BUSCA. Prefere o campo `text` da resposta; se faltar, remonta a
/// partir dos turnos. Nunca falha — busca com texto vazio só não acha nada, o que é
/// melhor que abortar a transcrição inteira.
pub fn extract_plain_text(obj: &Value) -> String {
    let text = string(obj, "text");
    if !text.trim().is_empty() {
        return text;
    }
    let Some(turns) = obj.get("turns").and_then(Value::as_array) else { return String::new() };
    let mut out = String::new();
    for t in turns.iter().filter(|t| t.is_object()) {
        if let Some(s) = t.get("text").and_then(Value::as_str) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(s.trim());
        }
    }
    out
}

/// Troca o `turns` da resposta pelos turnos remontados das PALAVRAS, preservando todo o
/// resto (segments, words, language, by_speaker...). Vale para o caminho da MISTURA, que
/// guarda o JSON da API como veio. `None` = sem word timestamps (nada a remontar).
pub fn rechunk_turns(body: &str) -> Option<String> {
    let Value::Object(mut root) = serde_json::from_str::<Value>(body).ok()? else { return None };
    let turns = turns_from_words(&Value::Object(root.clone()), 0);
    if turns.is_empty() {
        return None;
    }
    let arr: Vec<Value> = turns.iter().map(turn_json).collect();
    root.insert("turns".into(), Value::Array(arr));
    Some(Value::Object(root).to_string())
}

fn turn_json(t: &Turn) -> Value {
    json!({ "speaker": t.speaker, "start": t.start, "end": t.end, "text": t.text })
}

/// Resposta SEM palavra nenhuma (API sem alinhamento): usa o `turns` como veio. Fica
/// grosso, mas é melhor que não ter turno nenhum.
fn fallback_turns(obj: &Value, track: usize) -> Vec<Turn> {
    let Some(arr) = obj.get("turns").and_then(Value::as_array) else { return Vec::new() };
    arr.iter()
        .filter(|t| t.is_object())
        .filter_map(|t| {
            let text = string(t, "text");
            (!text.trim().is_empty()).then(|| Turn {
                speaker: string(t, "speaker").trim().to_owned(),
                start: num(t, "start").unwrap_or(0.0),
                end: num(t, "end").unwrap_or(0.0),
                text: text.trim().to_owned(),
                track,
                drop: false,
            })
        })
        .collect()
}

/// A regra do rótulo: se a faixa tem um falante só (o caso do microfone), o nome do
/// DISPOSITIVO já diz tudo e o SPEAKER_00 seria ruído. Se tem mais de um (o alto-falante
/// numa reunião), o nome da faixa vira prefixo e a diarização continua distinguindo quem
/// é quem dentro dela.
fn label_track_speakers(turns: &mut [Turn], track_name: &str) {
    let distinct: std::collections::BTreeSet<&str> = turns.iter().map(|t| t.speaker.as_str()).filter(|s| !s.is_empty()).collect();
    let many = distinct.len() > 1;
    let labels: Vec<String> = turns
        .iter()
        .map(|t| if many && !t.speaker.is_empty() { format!("{track_name} · {}", t.speaker) } else { track_name.to_owned() })
        .collect();
    for (t, l) in turns.iter_mut().zip(labels) {
        t.speaker = l;
    }
}

/// DEDUPLICAÇÃO POR PALAVRA entre faixas. Devolve quantas caíram.
///
/// O que sai pelos alto-falantes volta pelo microfone, e dois microfones na mesma sala
/// captam a mesma fala. Comparar TURNOS não funcionava: cada faixa corta os turnos em
/// pontos diferentes, então a cópia quase nunca se sobrepunha o bastante, e quando casava
/// o turno inteiro caía — levando junto as palavras legítimas dele.
///
/// Aqui, por par de faixas:
/// 1. CASA a mesma palavra nas duas, dentro de `WDEDUP_TOL`, em ordem (guloso e monótono:
///    cada palavra de A pega a primeira igual em B depois do último par);
/// 2. forma CORRIDAS de pares seguidos, tolerando até `WDEDUP_MAXSKIP` palavras puladas
///    de cada lado — o eco chega picado;
/// 3. corrida LONGA (≥ 4 pares) é eco: cai o trecho da faixa que capturou MENOS palavras
///    (empate: a de menor confiança);
/// 4. corrida CURTA (2-3 pares) só é eco se TODOS os pares forem simultâneos e se ao
///    menos um lado não tiver falado mais nada em volta; cai a cópia de MENOR confiança.
///
/// Por que "menor confiança" na curta e não "o lado sem sobra": a resposta do falante
/// local logo depois põe sobra justamente no lado do ECO, e a regra apagava o original.
pub fn dedup_words(tracks: &mut [Vec<TimedWord>]) -> usize {
    let mut dropped = 0;
    for a in 0..tracks.len() {
        for b in a + 1..tracks.len() {
            let ia = filter_idx(&tracks[a]);
            let ib = filter_idx(&tracks[b]);
            if ia.is_empty() || ib.is_empty() {
                continue;
            }
            // Pares (índice em `ia`, índice em `ib`).
            let mut pairs: Vec<(usize, usize)> = Vec::new();
            let mut low = 0usize;
            let mut last: i64 = -1;
            for i in 0..ia.len() {
                let wa = &tracks[a][ia[i]];
                while low < ib.len() && tracks[b][ib[low]].start < wa.start - WDEDUP_TOL {
                    low += 1;
                }
                let mut j = low.max((last + 1) as usize);
                while j < ib.len() && tracks[b][ib[j]].start <= wa.start + WDEDUP_TOL {
                    if tracks[b][ib[j]].norm == wa.norm {
                        pairs.push((i, j));
                        last = j as i64;
                        break;
                    }
                    j += 1;
                }
            }
            if pairs.is_empty() {
                continue;
            }
            let continues = |p: usize, q: usize, tracks: &[Vec<TimedWord>]| -> bool {
                (pairs[q].0 as i64 - pairs[p].0 as i64) <= WDEDUP_MAXSKIP + 1
                    && (pairs[q].1 as i64 - pairs[p].1 as i64) <= WDEDUP_MAXSKIP + 1
                    && tracks[a][ia[pairs[q].0]].start - tracks[a][ia[pairs[p].0]].end <= WDEDUP_RUN_GAP
            };
            let mut run_start = 0;
            for k in 1..=pairs.len() {
                if k == pairs.len() || !continues(k - 1, k, tracks) {
                    handle_run(tracks, a, b, &ia, &ib, &pairs[run_start..k], &mut dropped);
                    run_start = k;
                }
            }
        }
    }
    dropped
}

fn filter_idx(w: &[TimedWord]) -> Vec<usize> {
    (0..w.len()).filter(|&x| !w[x].norm.is_empty() && !w[x].is_segment).collect()
}

fn handle_run(tracks: &mut [Vec<TimedWord>], a: usize, b: usize, ia: &[usize], ib: &[usize], run: &[(usize, usize)], dropped: &mut usize) {
    let len = run.len();
    let (ia0, ia1) = (run[0].0, run[len - 1].0);
    let (jb0, jb1) = (run[0].1, run[len - 1].1);
    let count_a = ia1 - ia0 + 1;
    let count_b = jb1 - jb0 + 1;
    let score_a = (ia0..=ia1).map(|x| tracks[a][ia[x]].score).sum::<f64>() / count_a as f64;
    let score_b = (jb0..=jb1).map(|x| tracks[b][ib[x]].score).sum::<f64>() / count_b as f64;

    let lose_b;
    if len >= WDEDUP_MIN_RUN {
        lose_b = count_a > count_b || (count_a == count_b && score_a >= score_b);
    } else {
        if len < WDEDUP_MIN_SHORT {
            return;
        }
        if run.iter().any(|&(x, y)| (tracks[a][ia[x]].start - tracks[b][ib[y]].start).abs() > WDEDUP_SHORT_TOL) {
            return;
        }
        let t0 = tracks[a][ia[ia0]].start.min(tracks[b][ib[jb0]].start) - WDEDUP_PAD;
        let t1 = tracks[a][ia[ia1]].end.max(tracks[b][ib[jb1]].end) + WDEDUP_PAD;
        // "Sobra" = palavra do lado que não entrou na corrida: no meio dela, ou colada
        // antes/depois (as palavras vêm em ordem de tempo).
        let extra_a = count_a > len
            || (ia0 > 0 && tracks[a][ia[ia0 - 1]].start >= t0)
            || (ia1 + 1 < ia.len() && tracks[a][ia[ia1 + 1]].start <= t1);
        let extra_b = count_b > len
            || (jb0 > 0 && tracks[b][ib[jb0 - 1]].start >= t0)
            || (jb1 + 1 < ib.len() && tracks[b][ib[jb1 + 1]].start <= t1);
        // Os dois lados falaram mais coisa ali: é conversa, não eco.
        if extra_a && extra_b {
            return;
        }
        lose_b = score_a >= score_b;
    }
    let (t, idx, lo, hi) = if lose_b { (b, ib, jb0, jb1) } else { (a, ia, ia0, ia1) };
    for x in lo..=hi {
        let w = &mut tracks[t][idx[x]];
        if !w.drop {
            w.drop = true;
            *dropped += 1;
        }
    }
}

/// Junta as faixas numa transcrição só, no MESMO formato que a API devolve para uma faixa
/// única (`{turns:[{speaker,start,end,text}], text}`). Manter o formato é o que faz o
/// painel do player e a busca continuarem funcionando sem saber que isto existe.
///
/// A ordem importa: PALAVRAS de todas as faixas → dedup POR PALAVRA → turnos só com as
/// palavras que sobraram. Montar os turnos antes e deduplicar depois era o que falhava.
///
/// O dedup por TURNO sobrou como rede de segurança só para faixa sem timestamp por palavra
/// (idioma sem alinhador): lá não há palavra para casar. Entre duas faixas COM palavras
/// ele não roda — por turno ele apagaria fala legítima.
///
/// Devolve `(json, texto)`.
pub fn merge_transcripts(bodies: &[String], names: &[String]) -> Option<(String, String)> {
    let mut roots = Vec::with_capacity(bodies.len());
    let mut words: Vec<Vec<TimedWord>> = Vec::with_capacity(bodies.len());
    let mut has_words = Vec::with_capacity(bodies.len());
    for b in bodies {
        if b.trim().is_empty() {
            return None;
        }
        let v = serde_json::from_str::<Value>(b).ok().filter(Value::is_object)?;
        let w = collect_words(&v);
        has_words.push(w.iter().any(|x| !x.norm.is_empty() && !x.is_segment));
        words.push(w);
        roots.push(v);
    }
    let word_drops = dedup_words(&mut words);

    let mut turns: Vec<Turn> = Vec::new();
    for i in 0..bodies.len() {
        let mut tmp = turns_from_word_list(&words[i], i);
        if tmp.is_empty() {
            tmp = fallback_turns(&roots[i], i);
        }
        label_track_speakers(&mut tmp, &names[i]);
        turns.extend(tmp);
    }

    // Ordena por início: o player acompanha o destaque nessa ordem, e o dedup por turno
    // abaixo só olha a janela vizinha.
    turns.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.track.cmp(&b.track)));
    let norm: Vec<String> = turns.iter().map(|t| normalize_for_compare(&t.text)).collect();

    let mut turn_drops = 0;
    for i in 0..turns.len() {
        if turns[i].drop {
            continue;
        }
        let mut j = i + 1;
        while j < turns.len() && turns[j].start < turns[i].end {
            // Só a rede de segurança: entre duas faixas com palavras o dedup já foi feito
            // por palavra.
            if turns[j].drop || turns[j].track == turns[i].track || (has_words[turns[i].track] && has_words[turns[j].track]) {
                j += 1;
                continue;
            }
            if time_overlap_ratio(&turns[i], &turns[j]) >= DEDUP_OVERLAP_MIN && word_similarity(&norm[i], &norm[j]) >= DEDUP_SIM_MIN {
                if norm[j].chars().count() > norm[i].chars().count() {
                    turns[i].drop = true;
                } else {
                    turns[j].drop = true;
                }
                turn_drops += 1;
                if turns[i].drop {
                    break;
                }
            }
            j += 1;
        }
    }

    let kept: Vec<&Turn> = turns.iter().filter(|t| !t.drop).collect();
    let text = kept.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
    let merged = json!({
        "turns": kept.iter().map(|t| turn_json(t)).collect::<Vec<_>>(),
        "text": text,
        // Proveniência: quais faixas entraram. Ninguém lê hoje, mas é a única pista de que
        // este arquivo veio de várias faixas.
        "tracks": names,
    });
    log!("Transcribe: {} faixas -> {} turnos (eco: {word_drops} palavra(s) por palavra, {turn_drops} turno(s) por turno).", bodies.len(), kept.len());
    Some((merged.to_string(), text))
}

/// Transcrição do arquivo exportado, no relógio DELE: só o que foi dito nos trechos
/// mantidos, com os trechos emendados sem buraco (a mesma linha do tempo que a exportação
/// produz). Devolve `(json, texto puro)`, ou `None` se a origem é ilegível.
///
/// Turno inteiro dentro de um trecho só muda de relógio. Turno que um corte PARTE (começa
/// antes, termina depois, ou atravessa um trecho tirado) vira um turno por pedaço mantido,
/// e o TEXTO de cada pedaço sai só das palavras que caíram nele — senão o arquivo novo
/// "diria" o que foi cortado fora. Quem decide o lado de cada palavra: com palavras
/// alinhadas, o meio da palavra; sem elas (transcrição por faixa, que só guarda turnos), o
/// tempo do turno dividido pelos caracteres, a mesma regra da legenda do player.
pub fn export_transcript_body(src_body: &str, starts: &[f64], ends: &[f64], speed: f64) -> Option<(String, String)> {
    if starts.is_empty() || starts.len() != ends.len() {
        return None;
    }
    let speed = if speed <= 0.0 { 1.0 } else { speed };
    let mut offs = Vec::with_capacity(starts.len());
    let mut acc = 0.0;
    for i in 0..starts.len() {
        offs.push(acc);
        acc += (ends[i] - starts[i]).max(0.0);
    }
    let src = serde_json::from_str::<Value>(src_body).ok().filter(Value::is_object)?;
    let words = collect_words(&src);
    let x = Exporter { starts, ends, offs: &offs, speed, words: &words };

    // Tudo que não é tempo (idioma, motor...) vai como veio. O `text` sai: é remontado dos
    // turnos que ficaram.
    let mut dst: Map<String, Value> =
        src.as_object().unwrap().iter().filter(|(k, _)| !["turns", "segments", "text"].contains(&k.to_lowercase().as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();

    let mut new_turns = Vec::new();
    if let Some(arr) = src.get("turns").and_then(Value::as_array) {
        for t in arr.iter().filter(|t| t.is_object()) {
            x.add_turn(t, &mut new_turns);
        }
    }
    dst.insert("turns".into(), Value::Array(new_turns));

    if let Some(arr) = src.get("segments").and_then(Value::as_array) {
        let mut new_segs = Vec::new();
        for s in arr.iter().filter(|s| s.is_object()) {
            let Some(mut seg) = x.remapped(s) else { continue };
            if let Some(ws) = seg.get("words").and_then(Value::as_array).cloned() {
                let (mut timed, mut timed_kept) = (0, 0);
                let mut seg_text = String::new();
                let mut new_words = Vec::new();
                for w in ws.iter().filter(|w| w.is_object()) {
                    if w.get("start").is_some() {
                        timed += 1;
                    }
                    if let Some(item) = x.remapped(w) {
                        if item.get("start").is_some() {
                            timed_kept += 1;
                        }
                        append_word(&mut seg_text, &string(&item, "word"));
                        new_words.push(item);
                    }
                }
                seg["words"] = Value::Array(new_words);
                // Segmento partido por um corte: o texto dele sai das palavras que ficaram.
                // Nenhuma palavra alinhada ficou = o segmento só encostava nos trechos
                // mantidos, sai inteiro.
                if timed > 0 && timed_kept == 0 {
                    continue;
                }
                if timed_kept < timed {
                    seg["text"] = json!(seg_text.trim());
                }
            }
            new_segs.push(seg);
        }
        dst.insert("segments".into(), Value::Array(new_segs));
    }
    let dst = Value::Object(dst);
    let text = extract_plain_text(&dst);
    Some((dst.to_string(), text))
}

struct Exporter<'a> {
    starts: &'a [f64],
    ends: &'a [f64],
    offs: &'a [f64],
    speed: f64,
    words: &'a [TimedWord],
}

impl Exporter<'_> {
    /// Mapeia `[from, to]` da origem para a saída. Intervalo sem nenhum pedaço dentro dos
    /// trechos mantidos = `None`. Intervalo de duração zero (palavra sem fim) vale se cair
    /// dentro de um trecho.
    fn map_range(&self, from: f64, to: f64) -> Option<(f64, f64)> {
        let (mut first, mut last) = (None, None);
        for x in 0..self.starts.len() {
            if to >= self.starts[x] && from <= self.ends[x] && (to > self.starts[x] || from == to) && (from < self.ends[x] || from == to) {
                first.get_or_insert(x);
                last = Some(x);
            }
        }
        let (first, last) = (first?, last?);
        // Exportação acelerada: o relógio da saída anda `speed` vezes mais rápido.
        let a = (from.max(self.starts[first]) - self.starts[first] + self.offs[first]) / self.speed;
        let b = (to.min(self.ends[last]) - self.starts[last] + self.offs[last]) / self.speed;
        Some((a, b.max(a)))
    }

    /// Clona o objeto trocando start/end. Sem start/end (palavra que o alinhador não
    /// reconheceu, pegadinha #60a) o clone vai como veio. `None` = caiu fora dos trechos.
    fn remapped(&self, obj: &Value) -> Option<Value> {
        let Some(s) = num(obj, "start") else { return Some(obj.clone()) };
        let has_e = obj.get("end").and_then(Value::as_f64).is_some();
        let e = num(obj, "end").unwrap_or(s);
        let (a, b) = self.map_range(s, e)?;
        let mut out = obj.clone();
        out["start"] = json!(a);
        if has_e {
            out["end"] = json!(b);
        }
        Some(out)
    }

    /// Texto do turno `[ts, te]` que cai no pedaço mantido `[pa, pb]`.
    fn piece_text(&self, text: &str, ts: f64, te: f64, pa: f64, pb: f64) -> String {
        let mut result = String::new();
        // Palavras alinhadas que pertencem a ESTE turno (pelo meio delas).
        let mut any = false;
        for w in self.words {
            let mid = (w.start + w.end) / 2.0;
            if mid < ts || mid > te {
                continue;
            }
            any = true;
            if mid >= pa && mid <= pb {
                append_word(&mut result, &w.text);
            }
        }
        if any {
            return result;
        }
        // Sem palavras: proporcional aos caracteres.
        let tokens: Vec<&str> = text.split(' ').filter(|s| !s.is_empty()).collect();
        let total: usize = tokens.iter().map(|t| t.chars().count() + 1).sum();
        if total == 0 {
            return result;
        }
        let mut cum = 0usize;
        for tok in tokens {
            let n = tok.chars().count();
            let t = ts + (te - ts) * ((cum as f64 + n as f64 / 2.0) / total as f64);
            cum += n + 1;
            if t >= pa && t <= pb {
                append_word(&mut result, tok);
            }
        }
        result
    }

    fn add_turn(&self, turn: &Value, dest: &mut Vec<Value>) {
        let Some(s) = num(turn, "start") else {
            dest.push(turn.clone());
            return;
        };
        let e = num(turn, "end").unwrap_or(s);
        // Inteiro dentro de um trecho: só troca o relógio.
        if (0..self.starts.len()).any(|x| s >= self.starts[x] && e <= self.ends[x]) {
            if let Some(c) = self.remapped(turn) {
                dest.push(c);
            }
            return;
        }
        if e <= s {
            return; // instante fora de todos os trechos
        }
        let txt = string(turn, "text");
        for x in 0..self.starts.len() {
            let pa = s.max(self.starts[x]);
            let pb = e.min(self.ends[x]);
            if pb <= pa {
                continue;
            }
            let piece = self.piece_text(&txt, s, e, pa, pb);
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            let mut clone = turn.clone();
            clone["start"] = json!((pa - self.starts[x] + self.offs[x]) / self.speed);
            clone["end"] = json!((pb - self.starts[x] + self.offs[x]) / self.speed);
            clone["text"] = json!(piece);
            dest.push(clone);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, end: f64, score: f64) -> Value {
        json!({ "word": text, "start": start, "end": end, "score": score })
    }

    fn resp(words: Vec<Value>) -> String {
        json!({ "segments": [{ "id": 0, "start": 0.0, "end": 100.0, "text": "", "words": words }], "turns": [], "text": "" }).to_string()
    }

    #[test]
    fn palavra_sem_espaco_na_frente_nao_emenda() {
        let mut s = String::new();
        append_word(&mut s, "Bom");
        append_word(&mut s, "dia");
        append_word(&mut s, " pessoal");
        assert_eq!(s, "Bom dia pessoal");
    }

    #[test]
    fn palavra_sem_tempo_vai_para_a_proxima() {
        let v: Value = serde_json::from_str(&resp(vec![w("Custou", 0.0, 0.5, 0.9), json!({ "word": "1500" }), w("reais", 0.6, 1.0, 0.9)])).unwrap();
        let words = collect_words(&v);
        assert_eq!(words.len(), 2);
        assert_eq!(words[1].text, "1500 reais");
        assert_eq!(words[1].norm, "reais");
    }

    #[test]
    fn turnos_quebram_por_pausa_e_por_falante() {
        let words = vec![
            TimedWord { text: "oi".into(), norm: "oi".into(), start: 0.0, end: 0.4, ..Default::default() },
            TimedWord { text: "tudo".into(), norm: "tudo".into(), start: 0.5, end: 0.8, ..Default::default() },
            // pausa de 5 s
            TimedWord { text: "bem".into(), norm: "bem".into(), start: 5.8, end: 6.0, ..Default::default() },
            TimedWord { text: "sim".into(), norm: "sim".into(), speaker: "B".into(), start: 6.1, end: 6.3, ..Default::default() },
        ];
        let t = turns_from_word_list(&words, 0);
        let texts: Vec<_> = t.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["oi tudo", "bem", "sim"]);
    }

    #[test]
    fn eco_longo_derruba_a_faixa_com_menos_palavras() {
        let mk = |text: &str, start: f64| TimedWord { text: text.into(), norm: text.into(), start, end: start + 0.2, score: 0.9, ..Default::default() };
        let a: Vec<_> = ["vamos", "adiar", "tudo", "pra", "semana", "que", "vem"].iter().enumerate().map(|(i, t)| mk(t, i as f64 * 0.3)).collect();
        // O microfone pegou só um pedaço, com 0,1 s de atraso.
        let b: Vec<_> = ["adiar", "tudo", "pra", "semana"].iter().enumerate().map(|(i, t)| mk(t, 0.3 + i as f64 * 0.3 + 0.1)).collect();
        let mut tracks = vec![a, b];
        let dropped = dedup_words(&mut tracks);
        assert_eq!(dropped, 4);
        assert!(tracks[1].iter().all(|w| w.drop));
        assert!(tracks[0].iter().all(|w| !w.drop));
    }

    #[test]
    fn fala_simultanea_de_duas_pessoas_nao_e_eco() {
        let mk = |text: &str, start: f64| TimedWord { text: text.into(), norm: text.into(), start, end: start + 0.2, score: 0.9, ..Default::default() };
        let a = vec![mk("eu", 0.0), mk("acho", 0.3), mk("que", 0.6), mk("sim", 0.9)];
        let b = vec![mk("nao", 0.0), mk("espera", 0.3), mk("calma", 0.6)];
        let mut tracks = vec![a, b];
        assert_eq!(dedup_words(&mut tracks), 0);
    }

    #[test]
    fn merge_poe_o_nome_da_faixa_como_falante() {
        let body = |t: &str| resp(vec![w(t, 1.0, 1.4, 0.9)]);
        let (merged, text) = merge_transcripts(&[body("ola"), body("tchau")], &["Mic".into(), "Sys".into()]).unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["turns"][0]["speaker"], "Mic");
        assert_eq!(v["turns"][1]["speaker"], "Sys");
        assert_eq!(text, "ola tchau");
    }

    #[test]
    fn exportacao_remapeia_o_relogio_dos_turnos() {
        let src = json!({
            "turns": [
                { "speaker": "A", "start": 10.0, "end": 12.0, "text": "dentro" },
                { "speaker": "A", "start": 50.0, "end": 52.0, "text": "fora" },
            ],
            "text": "dentro fora",
        })
        .to_string();
        // Mantém 8..20 e 30..40 → o primeiro turno vai para 2..4.
        let (body, text) = export_transcript_body(&src, &[8.0, 30.0], &[20.0, 40.0], 1.0).unwrap();
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["turns"].as_array().unwrap().len(), 1);
        assert_eq!(v["turns"][0]["start"], json!(2.0));
        assert_eq!(text, "dentro");
    }

    #[test]
    fn exportacao_parte_o_turno_que_atravessa_um_corte() {
        let src = json!({ "turns": [{ "speaker": "A", "start": 0.0, "end": 10.0, "text": "aaaa bbbb cccc dddd" }] }).to_string();
        // Tira o miolo (4..6): sobra [0,4] e [6,10].
        let (body, _) = export_transcript_body(&src, &[0.0, 6.0], &[4.0, 10.0], 1.0).unwrap();
        let v: Value = serde_json::from_str(&body).unwrap();
        let turns = v["turns"].as_array().unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0]["text"], "aaaa bbbb");
        assert_eq!(turns[1]["text"], "cccc dddd");
        // O segundo pedaço começa onde o primeiro terminou (emenda sem buraco).
        assert_eq!(turns[1]["start"], json!(4.0));
    }

    #[test]
    fn exportacao_acelerada_divide_o_relogio() {
        let src = json!({ "turns": [{ "speaker": "A", "start": 10.0, "end": 14.0, "text": "oi" }] }).to_string();
        let (body, _) = export_transcript_body(&src, &[0.0], &[20.0], 2.0).unwrap();
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["turns"][0]["start"], json!(5.0));
        assert_eq!(v["turns"][0]["end"], json!(7.0));
    }
}

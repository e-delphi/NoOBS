//! Números por extenso em português viram algarismos (porte de `OBSNumbersPt.pas`).
//!
//! O Qwen3-ASR (transcrição local) escreve os números por extenso ("setenta"), o que
//! alonga a legenda. Segue a convenção comum em legendagem: de zero a dez por
//! extenso, acima disso em algarismos. Manter os pequenos por extenso também evita
//! converter o artigo "um".
//!
//! O "e" só liga uma parte a outra MENOR, como na fala: dezena a unidade ("vinte e
//! cinco"), centena a dezena ("cento e dez"), milhar ao resto ("dois mil e vinte").
//! Assim "dois e três" continua sendo dois números, e não vira 5.
//!
//! Tradução direta do `qwen/windows/app/numbers_pt.py` da Transcritor API, validado
//! lá contra 19 frases reais. Mudou um, mude o outro.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    None,
    Unit,
    Teen,
    Ten,
    Hundred,
    Scale,
}

#[derive(Debug, Clone, Copy)]
struct NumWord {
    kind: Kind,
    value: i64,
}

/// Pontuação que pode vir colada na palavra, antes e depois. A palavra é casada SEM
/// ela, e ela volta intacta em volta do número.
const LEAD_CHARS: &str = "«\"'([";
const TRAIL_CHARS: &str = ".,;:!?»\"')]…";
/// Pontuação que ENCERRA a frase numérica ("vinte, trinta" são dois).
const STOP_CHARS: &str = ".,;:!?…";

fn words() -> &'static HashMap<&'static str, NumWord> {
    static W: OnceLock<HashMap<&'static str, NumWord>> = OnceLock::new();
    W.get_or_init(|| {
        let mut m = HashMap::new();
        let mut add = |list: &[&'static str], kind: Kind, values: &[i64]| {
            for (w, v) in list.iter().zip(values) {
                m.insert(*w, NumWord { kind, value: *v });
            }
        };
        add(
            &["zero", "um", "uma", "dois", "duas", "três", "tres", "quatro", "cinco", "seis", "sete", "oito", "nove"],
            Kind::Unit,
            &[0, 1, 1, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9],
        );
        add(
            &[
                "dez", "onze", "doze", "treze", "quatorze", "catorze", "quinze", "dezesseis", "dezasseis", "dezessete", "dezassete", "dezoito",
                "dezenove", "dezanove",
            ],
            Kind::Teen,
            &[10, 11, 12, 13, 14, 14, 15, 16, 16, 17, 17, 18, 19, 19],
        );
        add(&["vinte", "trinta", "quarenta", "cinquenta", "sessenta", "setenta", "oitenta", "noventa"], Kind::Ten, &[20, 30, 40, 50, 60, 70, 80, 90]);
        add(
            &[
                "cem", "cento", "duzentos", "duzentas", "trezentos", "trezentas", "quatrocentos", "quatrocentas", "quinhentos", "quinhentas",
                "seiscentos", "seiscentas", "setecentos", "setecentas", "oitocentos", "oitocentas", "novecentos", "novecentas",
            ],
            Kind::Hundred,
            &[100, 100, 200, 200, 300, 300, 400, 400, 500, 500, 600, 600, 700, 700, 800, 800, 900, 900],
        );
        add(&["mil", "milhão", "milhões", "bilhão", "bilhões"], Kind::Scale, &[1000, 1_000_000, 1_000_000, 1_000_000_000, 1_000_000_000]);
        m
    })
}

/// Mesma regra do regex do Python: pontuação inicial gulosa, depois a maior
/// sequência de pontuação final possível, e o meio é a palavra.
fn split_token(word: &str) -> (&str, &str, &str) {
    let a = word.char_indices().find(|(_, c)| !LEAD_CHARS.contains(*c)).map_or(word.len(), |(i, _)| i);
    let rest = &word[a..];
    let b = rest.char_indices().rev().find(|(_, c)| !TRAIL_CHARS.contains(*c)).map_or(0, |(i, c)| i + c.len_utf8());
    (&word[..a], &rest[..b], &rest[b..])
}

fn core_of(word: &str) -> &str {
    split_token(word).1
}

fn kind_of(core: &str) -> NumWord {
    words().get(core.to_lowercase().as_str()).copied().unwrap_or(NumWord { kind: Kind::None, value: 0 })
}

/// Depois de cada tipo de parte, o maior valor que pode vir ligado por "e".
/// 0 = esse tipo não aceita "e" depois (unidade, dezena de 10 a 19).
fn after_e_limit(kind: Kind) -> i64 {
    match kind {
        Kind::Ten => 10,
        Kind::Hundred => 100,
        Kind::Scale => 1000,
        _ => 0,
    }
}

/// Lê a maior frase numérica a partir de `w[from]`: `(valor, palavras lidas)`.
fn parse_number(w: &[&str], from: usize) -> Option<(i64, usize)> {
    let (mut total, mut group) = (0i64, 0i64);
    let mut used = 0usize;
    let mut last = Kind::None;
    let mut i = from;
    while i < w.len() {
        let (_, core, trail0) = split_token(w[i]);
        let mut trail = trail0;
        let mut cur = kind_of(core);
        let mut connector = false;
        if cur.kind == Kind::None && core.to_lowercase() == "e" && after_e_limit(last) > 0 && i + 1 < w.len() {
            let (_, nxt, t2) = split_token(w[i + 1]);
            let next_w = kind_of(nxt);
            if next_w.kind != Kind::None && next_w.kind != Kind::Scale && next_w.value < after_e_limit(last) {
                connector = true;
                i += 1;
                trail = t2;
                cur = next_w;
            }
        }
        if cur.kind == Kind::None {
            break;
        }
        // Sem "e", só valem: parte seguida de escala ("dois mil") ou escala seguida de
        // centena/dezena/unidade ("mil novecentos").
        if !connector && last != Kind::None && !(cur.kind == Kind::Scale || last == Kind::Scale) {
            break;
        }
        if cur.kind == Kind::Scale {
            // "mil milhões" e "milhão" sem número antes ficam fora do escopo.
            if last == Kind::Scale || (last == Kind::None && cur.value > 1000) {
                break;
            }
            total += if group == 0 { cur.value } else { group * cur.value };
            group = 0;
        } else {
            group += cur.value;
        }
        used = i + 1 - from;
        last = cur.kind;
        i += 1;
        // Pontuação encerra a frase numérica.
        if trail.chars().last().is_some_and(|c| STOP_CHARS.contains(c)) {
            break;
        }
    }
    (used > 0).then_some((total + group, used))
}

/// 1234567 → "1.234.567" (separador de milhar do português).
fn group_thousands(value: i64) -> String {
    let s = value.to_string();
    let n = s.len();
    let mut out = String::with_capacity(n + n / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Milhão e bilhão redondos ficam com a palavra: "3 milhões" lê melhor que
/// "3.000.000". O resto vira algarismo, com ponto a partir de 10.000.
fn render(value: i64) -> String {
    const SCALES: [i64; 2] = [1_000_000, 1_000_000_000];
    const SINGULAR: [&str; 2] = ["milhão", "bilhão"];
    const PLURAL: [&str; 2] = ["milhões", "bilhões"];
    for k in 0..SCALES.len() {
        if value >= SCALES[k] && value % SCALES[k] == 0 && value < SCALES[k] * 1000 {
            let n = value / SCALES[k];
            return format!("{n} {}", if n == 1 { SINGULAR[k] } else { PLURAL[k] });
        }
    }
    if value >= 10_000 {
        return group_thousands(value);
    }
    value.to_string()
}

/// Troca por algarismos as frases numéricas acima de `keep_up_to` (padrão 10).
/// "dois mil e vinte e seis," → "2026,"; "trinta e cinco por cento" → "35%".
pub fn to_digits(text: &str, keep_up_to: i64) -> String {
    // Split por espaço ÚNICO, como o Python: espaços seguidos viram palavras vazias
    // e o join devolve o texto com o mesmo espaçamento.
    let w: Vec<&str> = text.split(' ').collect();
    let mut parts: Vec<String> = Vec::with_capacity(w.len());
    let mut i = 0;
    while i < w.len() {
        let parsed = if w[i].is_empty() { None } else { parse_number(&w, i) };
        let Some((value, used)) = parsed else {
            parts.push(w[i].to_owned());
            i += 1;
            continue;
        };
        let trail = split_token(w[i + used - 1]).2;
        let single_mil = used == 1 && core_of(w[i]).to_lowercase() == "mil";
        if value <= keep_up_to || single_mil {
            // Pequenos e "mil" sozinho ficam por extenso.
            parts.extend(w[i..i + used].iter().map(|s| (*s).to_owned()));
        } else {
            let rendered = format!("{}{}", split_token(w[i]).0, render(value));
            if trail.is_empty() && i + used + 1 < w.len() && w[i + used].to_lowercase() == "por" && core_of(w[i + used + 1]).to_lowercase() == "cento" {
                parts.push(format!("{rendered}%{}", split_token(w[i + used + 1]).2));
                i += used + 2;
                continue;
            }
            parts.push(format!("{rendered}{trail}"));
        }
        i += used;
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> String {
        to_digits(s, 10)
    }

    #[test]
    fn ano_por_extenso() {
        assert_eq!(d("em dois mil e vinte e seis, ele foi"), "em 2026, ele foi");
    }

    #[test]
    fn porcentagem() {
        assert_eq!(d("trinta e cinco por cento"), "35%");
        assert_eq!(d("foi trinta e cinco por cento."), "foi 35%.");
    }

    #[test]
    fn pequenos_ficam_por_extenso() {
        assert_eq!(d("tenho dois gatos e um cachorro"), "tenho dois gatos e um cachorro");
        assert_eq!(d("dois e três"), "dois e três");
        assert_eq!(d("mil"), "mil");
    }

    #[test]
    fn virgula_encerra_a_frase_numerica() {
        assert_eq!(d("vinte, trinta"), "20, 30");
    }

    #[test]
    fn milhoes_e_milhares() {
        assert_eq!(d("três milhões"), "3 milhões");
        assert_eq!(d("um milhão"), "1 milhão");
        assert_eq!(d("cento e vinte mil"), "120.000");
        assert_eq!(d("mil novecentos e noventa"), "1990");
    }

    #[test]
    fn preserva_pontuacao_e_espacos() {
        assert_eq!(d("(vinte e cinco)"), "(25)");
        assert_eq!(d("a  vinte"), "a  20");
    }
}

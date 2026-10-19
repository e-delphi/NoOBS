//! Decisões das saídas, sem FFmpeg (testáveis puras): a FILA do buffer em
//! memória, o CORTE de um trecho salvo e a PAUSA da gravação. O gravador
//! (`pipeline`) só aplica o que estas peças decidem sobre os pacotes reais.
//!
//! Tudo em tempo de sessão (ns desde a largada), na ordem em que os pacotes
//! chegam do encoder. O áudio chega ~150 ms depois do vídeo do mesmo instante
//! (o mixer anda atrás do relógio), então as regras de áudio são por TEMPO,
//! não por ordem de chegada.
//!
//! Comparado à libobs (pegadinha #62): lá salvar não esvazia o buffer e a
//! emenda exigia trocar a saída, com sobreposição de até 1 s. Aqui o corte é
//! num keyframe FORÇADO no instante do pedido: tudo antes vai pro trecho,
//! tudo depois fica na fila — sem buraco e sem repetição.

use std::collections::VecDeque;

/// O que importa de um pacote pra estas decisões.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meta {
    pub stream: usize,
    /// Instante de apresentação, ns de sessão.
    pub ns: i64,
    pub video: bool,
    pub key: bool,
    pub size: usize,
}

#[derive(Debug)]
pub struct Item<T> {
    pub meta: Meta,
    pub data: T,
}

/// Começo útil de uma lista de pacotes: o 1º keyframe de vídeo; antes dele,
/// vídeo não decodifica e áudio ficaria sem imagem. Devolve (instante, itens a partir dele).
pub fn from_first_keyframe<T>(items: Vec<Item<T>>) -> (Option<i64>, Vec<Item<T>>) {
    let Some(k) = items.iter().find(|i| i.meta.video && i.meta.key).map(|i| i.meta.ns) else { return (None, Vec::new()) };
    let mut seen_key = false;
    let out = items
        .into_iter()
        .filter(|i| {
            if i.meta.video {
                if i.meta.key && i.meta.ns == k {
                    seen_key = true;
                }
                seen_key
            } else {
                i.meta.ns >= k
            }
        })
        .collect();
    (Some(k), out)
}

// ---------------------------------------------------------------------------
// Buffer em memória
// ---------------------------------------------------------------------------

/// Corte pedido e ainda não fechado.
struct Cut<T> {
    id: u64,
    /// Instante do keyframe forçado (`None` = ainda esperando ele chegar).
    at: Option<i64>,
    clip: Vec<Item<T>>,
    /// Faixas de áudio que já passaram do corte.
    audio_done: Vec<bool>,
}

/// Fila do buffer: guarda os últimos `max_ns`/`max_bytes`, sempre começando
/// num keyframe de vídeo.
pub struct ReplayRing<T> {
    items: VecDeque<Item<T>>,
    bytes: usize,
    max_ns: i64,
    max_bytes: usize,
    audio_streams: Vec<usize>,
    cut: Option<Cut<T>>,
}

impl<T> ReplayRing<T> {
    pub fn new(max_sec: u32, max_mb: u32, audio_streams: Vec<usize>) -> Self {
        Self { items: VecDeque::new(), bytes: 0, max_ns: max_sec as i64 * 1_000_000_000, max_bytes: max_mb as usize * 1024 * 1024, audio_streams, cut: None }
    }

    pub fn set_limits(&mut self, max_sec: u32, max_mb: u32) {
        self.max_ns = max_sec as i64 * 1_000_000_000;
        self.max_bytes = max_mb as usize * 1024 * 1024;
        self.trim();
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Duração guardada (do 1º keyframe ao último pacote de vídeo).
    pub fn held_ns(&self) -> i64 {
        let first = self.items.iter().find(|i| i.meta.video && i.meta.key).map(|i| i.meta.ns);
        let last = self.items.iter().rev().find(|i| i.meta.video).map(|i| i.meta.ns);
        match (first, last) {
            (Some(a), Some(b)) => b - a,
            _ => 0,
        }
    }

    pub fn cut_pending(&self) -> bool {
        self.cut.is_some()
    }

    /// Pede um corte: o próximo keyframe de vídeo que chegar fecha o trecho
    /// (quem pede força esse keyframe no encoder). Um corte de cada vez.
    pub fn request_cut(&mut self, id: u64) -> bool {
        if self.cut.is_some() {
            return false;
        }
        self.cut = Some(Cut { id, at: None, clip: Vec::new(), audio_done: vec![false; self.audio_streams.len()] });
        true
    }

    /// Tira TUDO da fila (a gravação que continua o buffer começa por isso).
    pub fn take_all(&mut self) -> Vec<Item<T>> {
        self.bytes = 0;
        self.items.drain(..).collect()
    }

    /// Recebe um pacote. Devolve `(id, trecho)` quando um corte fecha.
    pub fn push(&mut self, it: Item<T>) -> Option<(u64, Vec<Item<T>>)> {
        if let Some(cut) = &mut self.cut {
            match cut.at {
                None if it.meta.video && it.meta.key => {
                    // O keyframe do corte: tudo o que está na fila é do trecho.
                    cut.at = Some(it.meta.ns);
                    cut.clip.extend(self.items.drain(..));
                    self.bytes = 0;
                }
                Some(at) if !it.meta.video && it.meta.ns < at => {
                    // Áudio atrasado de ANTES do corte: vai pro trecho.
                    cut.clip.push(it);
                    return None;
                }
                Some(at) if !it.meta.video => {
                    if let Some(k) = self.audio_streams.iter().position(|&s| s == it.meta.stream) {
                        cut.audio_done[k] = true;
                    }
                    let _ = at;
                }
                _ => {}
            }
        }
        self.bytes += it.meta.size;
        self.items.push_back(it);
        self.trim();
        // Fechou: o keyframe chegou e todas as faixas de áudio passaram dele.
        let done = self.cut.as_ref().is_some_and(|c| c.at.is_some() && c.audio_done.iter().all(|&d| d));
        if done {
            let c = self.cut.take().unwrap();
            return Some((c.id, c.clip));
        }
        None
    }

    /// Fecha o corte pendente com o que tiver (fim da sessão).
    pub fn force_close_cut(&mut self) -> Option<(u64, Vec<Item<T>>)> {
        let c = self.cut.take()?;
        let mut clip = c.clip;
        if c.at.is_none() {
            clip.extend(self.items.drain(..));
            self.bytes = 0;
        }
        Some((c.id, clip))
    }

    /// Respeita os limites e mantém a fila começando num keyframe.
    fn trim(&mut self) {
        let newest = self.items.back().map(|i| i.meta.ns).unwrap_or(0);
        loop {
            let over = self.items.front().is_some_and(|f| newest - f.meta.ns > self.max_ns) || self.bytes > self.max_bytes;
            if !over {
                break;
            }
            // Tira o GOP inteiro da frente: até o PRÓXIMO keyframe de vídeo.
            let next_key = self.items.iter().skip(1).position(|i| i.meta.video && i.meta.key).map(|p| p + 1);
            let Some(n) = next_key else { break };
            for it in self.items.drain(..n) {
                self.bytes -= it.meta.size;
            }
        }
        // Áudio da frente mais velho que o 1º keyframe não tem imagem.
        if let Some(k) = self.items.iter().find(|i| i.meta.video && i.meta.key).map(|i| i.meta.ns) {
            while self.items.front().is_some_and(|f| !f.meta.video && f.meta.ns < k) || self.items.front().is_some_and(|f| f.meta.video && !f.meta.key && f.meta.ns < k) {
                let it = self.items.pop_front().unwrap();
                self.bytes -= it.meta.size;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pausa da gravação
// ---------------------------------------------------------------------------

/// Decide, pacote a pacote, se ele entra no arquivo e com quanto de desconto.
/// O trecho pausado não entra NEM como buraco (#67): a retomada recomeça num
/// keyframe forçado, e o tempo parado é descontado de tudo que vem depois.
///
/// Guarda CADA intervalo pausado `[início, keyframe da retomada)` e decide
/// pelo INSTANTE do pacote, não pela ordem de chegada: o áudio chega ~150 ms
/// atrasado, e o de antes da pausa que chega depois da retomada tem que levar
/// o desconto ANTIGO (senão o tempo anda pra trás e o muxer recusa).
#[derive(Default)]
pub struct PauseGate {
    pauses: Vec<(i64, Option<i64>)>,
    resuming: bool,
}

impl PauseGate {
    pub fn paused(&self) -> bool {
        self.pauses.last().is_some_and(|p| p.1.is_none())
    }

    pub fn pause(&mut self, now_ns: i64) {
        if !self.paused() {
            self.pauses.push((now_ns, None));
        }
    }

    /// Quem chama força um keyframe no encoder.
    pub fn resume(&mut self) {
        if self.paused() {
            self.resuming = true;
        }
    }

    /// `Some(desconto)` = grava com o instante menos o desconto; `None` = descarta.
    pub fn admit(&mut self, m: &Meta) -> Option<i64> {
        if self.resuming && m.video && m.key {
            if let Some(last) = self.pauses.last_mut() {
                if m.ns >= last.0 {
                    last.1 = Some(m.ns);
                    self.resuming = false;
                }
            }
        }
        let mut offset = 0;
        for &(start, end) in &self.pauses {
            match end {
                Some(e) if m.ns >= e => offset += e - start,
                _ if m.ns >= start => return None, // dentro de um intervalo pausado
                _ => {}
            }
        }
        Some(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: i64 = 1_000_000;

    fn v(ns_ms: i64, key: bool) -> Item<()> {
        Item { meta: Meta { stream: 0, ns: ns_ms * MS, video: true, key, size: 100 }, data: () }
    }
    fn a(ns_ms: i64) -> Item<()> {
        Item { meta: Meta { stream: 1, ns: ns_ms * MS, video: false, key: true, size: 10 }, data: () }
    }
    fn times(items: &[Item<()>]) -> Vec<(bool, i64)> {
        items.iter().map(|i| (i.meta.video, i.meta.ns / MS)).collect()
    }

    #[test]
    fn primeiro_keyframe_corta_o_comeco() {
        let (k, out) = from_first_keyframe(vec![v(0, false), a(0), v(33, true), a(20), v(66, false), a(40)]);
        assert_eq!(k, Some(33 * MS));
        assert_eq!(times(&out), vec![(true, 33), (true, 66), (false, 40)]);
    }

    #[test]
    fn fila_respeita_o_tempo_e_comeca_num_keyframe() {
        // keyframe a cada 1 s, um quadro a cada 100 ms; limite 2 s.
        let mut r = ReplayRing::new(2, 1000, vec![1]);
        for t in 0..50 {
            r.push(v(t * 100, t % 10 == 0));
        }
        let first = r.items.front().unwrap().meta;
        assert!(first.video && first.key, "a fila tem que começar num keyframe");
        assert!(r.held_ns() <= 2900 * MS && r.held_ns() >= 1900 * MS, "guardou {} ms", r.held_ns() / MS);
    }

    #[test]
    fn fila_respeita_a_memoria() {
        let mut r = ReplayRing::new(3600, 1, vec![]);
        for t in 0..30_000 {
            r.push(v(t * 10, t % 100 == 0)); // 100 bytes cada
        }
        assert!(r.bytes() <= 1024 * 1024);
    }

    #[test]
    fn corte_no_keyframe_forcado_sem_buraco_nem_repeticao() {
        let mut r = ReplayRing::new(60, 100, vec![1]);
        for t in 0..10 {
            assert!(r.push(v(t * 100, t == 0)).is_none());
        }
        // áudio chega atrasado (~150 ms)
        for t in 0..8 {
            r.push(a(t * 100));
        }
        assert!(r.request_cut(7));
        assert!(!r.request_cut(8), "um corte de cada vez");
        // quadro 1000 já veio antes do keyframe forçado
        assert!(r.push(v(1000, false)).is_none());
        assert!(r.push(v(1100, true)).is_none(), "keyframe chegou, falta o áudio passar do corte");
        assert!(r.push(a(800)).is_none());
        assert!(r.push(a(900)).is_none());
        assert!(r.push(a(1000)).is_none());
        let (id, clip) = r.push(a(1100)).expect("o áudio passou do corte: fecha");
        assert_eq!(id, 7);
        let clip_v: Vec<i64> = clip.iter().filter(|i| i.meta.video).map(|i| i.meta.ns / MS).collect();
        let clip_a: Vec<i64> = clip.iter().filter(|i| !i.meta.video).map(|i| i.meta.ns / MS).collect();
        assert_eq!(clip_v, (0..=10).map(|t| t * 100).collect::<Vec<_>>(), "vídeo até ANTES do keyframe");
        assert_eq!(clip_a, (0..=10).map(|t| t * 100).collect::<Vec<_>>(), "áudio até ANTES do corte");
        // A fila agora começa exatamente no keyframe do corte.
        assert_eq!(times(&r.items.iter().map(|i| Item { meta: i.meta, data: () }).collect::<Vec<_>>()), vec![(true, 1100), (false, 1100)]);
    }

    #[test]
    fn pausa_nao_vira_buraco() {
        let mut g = PauseGate::default();
        assert_eq!(g.admit(&v(1000, false).meta), Some(0));
        g.pause(1500 * MS);
        assert_eq!(g.admit(&a(1400).meta), Some(0), "áudio de antes da pausa ainda entra");
        assert_eq!(g.admit(&v(1600, false).meta), None);
        assert_eq!(g.admit(&a(1700).meta), None);
        g.resume();
        assert_eq!(g.admit(&v(4000, false).meta), None, "espera o keyframe forçado");
        assert_eq!(g.admit(&v(4033, true).meta), Some(2533 * MS), "desconta 4033 - 1500");
        assert_eq!(g.admit(&a(4100).meta), Some(2533 * MS));
        // 2ª pausa acumula.
        g.pause(5000 * MS);
        g.resume();
        assert_eq!(g.admit(&v(6000, true).meta), Some(3533 * MS));
        assert!(!g.paused());
    }

    #[test]
    fn audio_atrasado_de_antes_da_pausa_leva_o_desconto_antigo() {
        let mut g = PauseGate::default();
        g.pause(1000 * MS);
        g.resume();
        assert_eq!(g.admit(&v(3000, true).meta), Some(2000 * MS), "retomou");
        // Chegando DEPOIS da retomada, mas de ANTES da pausa: sem o desconto.
        assert_eq!(g.admit(&a(950).meta), Some(0));
        // De dentro da pausa: fora.
        assert_eq!(g.admit(&a(1500).meta), None);
        assert_eq!(g.admit(&a(3100).meta), Some(2000 * MS));
    }
}

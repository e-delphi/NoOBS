//! Acelera o áudio SEM mudar o tom — WSOLA: sobreposição de janelas com busca de
//! forma de onda (o `TTimeStretch` do `FFmpegExport.pas`, pegadinha #51m).
//!
//! Simplesmente reamostrar deixaria a voz fina (2× = uma oitava acima); o que se
//! espera de um vídeo acelerado é o que os players fazem: mesma voz, falando mais
//! rápido. Sem libavfilter (não está bindado).
//!
//! Como funciona: a saída é montada com janelas de Hann de `w` amostras, uma a
//! cada `ho` (metade da janela — com Hann a soma das sobreposições dá exatamente
//! 1). Na entrada, cada janela é lida `hi = ho × velocidade` depois da anterior.
//! Tirar a janela exatamente ali emendaria ondas fora de fase (chiado, "eco
//! metálico"); por isso a leitura pode escorregar até `tol` para cada lado, para o
//! ponto que mais se parece com a continuação natural do trecho anterior
//! (correlação normalizada, grossa de 8 em 8 amostras e depois fina em volta da
//! melhor).
//!
//! Em velocidade alta (100×, 512×) o resultado é "pescar" 40 ms a cada poucos
//! segundos — o som de avançar uma fita, que é o esperado. Validado portando a
//! classe linha a linha para Python sobre o áudio real de uma gravação: duração
//! EXATA no alvo, nível igual e tom igual.
//!
//! Planar float (FLTP, o que o AAC decodifica). Um objeto por saída; um trecho do
//! vídeo por vez — `finish` fecha o trecho com a contagem exata de amostras e
//! reinicia.

/// `Round` do Delphi (arredonda para o par mais próximo) como `i64`.
fn round_i64(v: f64) -> i64 {
    v.round_ties_even() as i64
}

pub struct TimeStretch {
    ch: usize,
    w: usize,
    ho: usize,
    tol: i64,
    hi: f64,
    /// entrada guardada, por canal
    inp: Vec<Vec<f32>>,
    /// faixa absoluta guardada: `[in_base, in_end)`
    in_base: i64,
    in_end: i64,
    /// antes disto nada mais será lido
    keep_from: i64,
    /// sobreposição em montagem (`w`)
    acc: Vec<Vec<f32>>,
    win: Vec<f32>,
    /// continuação natural (mono, `ho`)
    tail: Vec<f32>,
    has_tail: bool,
    /// janelas já montadas no trecho
    k: i64,
    /// amostras produzidas no trecho
    made: i64,
    /// Saída pronta, por canal: `out_n` amostras. Quem consome zera `out_n`.
    pub outp: Vec<Vec<f32>>,
    pub out_n: usize,
}

impl TimeStretch {
    pub fn new(channels: usize, sample_rate: i32, speed: f64) -> Self {
        let ch = channels.max(1);
        // Janela de 40 ms, passo de 20 ms, busca de ±10 ms: o meio-termo clássico
        // para voz (janela curta corta sílaba; longa vira eco).
        let ho = ((sample_rate as f64 * 0.020).round() as usize).max(64);
        let w = ho * 2;
        let tol = ((sample_rate as f64 * 0.010).round() as i64).max(8);
        // Hann PERIÓDICA: com passo de meia janela as sobreposições somam 1.
        let win = (0..w).map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / w as f64).cos()) as f32).collect();
        let mut s = Self {
            ch,
            w,
            ho,
            tol,
            hi: ho as f64 * speed,
            inp: vec![Vec::new(); ch],
            in_base: 0,
            in_end: 0,
            keep_from: -tol,
            acc: vec![vec![0.0; w]; ch],
            win,
            tail: vec![0.0; ho],
            has_tail: false,
            k: 0,
            made: 0,
            outp: vec![Vec::new(); ch],
            out_n: 0,
        };
        s.restart();
        s
    }

    fn restart(&mut self) {
        self.in_base = 0;
        self.in_end = 0;
        self.keep_from = -self.tol;
        self.has_tail = false;
        self.k = 0;
        self.made = 0;
        for a in &mut self.acc {
            a.iter_mut().for_each(|v| *v = 0.0);
        }
    }

    fn sample(&self, ch: usize, pos: i64) -> f32 {
        if pos < self.in_base || pos >= self.in_end {
            0.0
        } else {
            self.inp[ch][(pos - self.in_base) as usize]
        }
    }

    fn mono(&self, pos: i64) -> f32 {
        if pos < self.in_base || pos >= self.in_end {
            return 0.0;
        }
        (0..self.ch).map(|c| self.inp[c][(pos - self.in_base) as usize]).sum()
    }

    /// `planes` = um ponteiro por canal (FLTP); `None` = silêncio.
    ///
    /// # Safety
    /// Cada ponteiro de `planes[..ch]` aponta para `n` floats válidos (ou é nulo).
    pub unsafe fn put(&mut self, planes: Option<&[*mut u8; 8]>, n: usize) {
        if n == 0 {
            return;
        }
        let n = n as i64;
        let old_end = self.in_end;
        self.in_end += n;
        // Descarta a frente que nenhuma janela vai mais ler. Em velocidade alta
        // quase toda a entrada cai aqui: só o que está perto da próxima leitura é
        // guardado.
        let new_base = self.in_base.max(self.keep_from.min(self.in_end));
        if new_base > self.in_base {
            let keep = (old_end - new_base).max(0) as usize;
            if keep > 0 {
                let from = (new_base - self.in_base) as usize;
                for c in 0..self.ch {
                    self.inp[c].copy_within(from..from + keep, 0);
                }
            }
            self.in_base = new_base;
        }
        let skip = (self.in_base - old_end).max(0);
        if skip >= n {
            return;
        }
        let len = (self.in_end - self.in_base) as usize;
        let dst = (old_end + skip - self.in_base) as usize;
        let count = (n - skip) as usize;
        for c in 0..self.ch {
            if self.inp[c].len() < len {
                self.inp[c].resize(4096.max(len * 2), 0.0);
            }
            let src = planes.map(|p| p[c]).filter(|p| !p.is_null());
            match src {
                None => self.inp[c][dst..dst + count].iter_mut().for_each(|v| *v = 0.0),
                Some(p) => {
                    let s = std::slice::from_raw_parts((p as *const f32).add(skip as usize), count);
                    self.inp[c][dst..dst + count].copy_from_slice(s);
                }
            }
        }
    }

    fn best_start(&self, nominal: i64) -> i64 {
        const COARSE: i64 = 8; // passo da busca grossa
        if !self.has_tail {
            return nominal;
        }
        let eval = |start: i64, step: usize| -> f64 {
            let (mut corr, mut en) = (0f64, 0f64);
            let mut k = 0usize;
            while k < self.ho {
                let v = self.mono(start + k as i64) as f64;
                corr += v * self.tail[k] as f64;
                en += v * v;
                k += step;
            }
            if en <= 1e-12 { 0.0 } else { corr / en.sqrt() }
        };
        let (lo, hi) = (nominal - self.tol, nominal + self.tol);
        let mut best = nominal;
        let mut best_score = -1e300;
        let mut c = lo;
        while c <= hi {
            let score = eval(c, COARSE as usize);
            if score > best_score {
                best_score = score;
                best = c;
            }
            c += COARSE;
        }
        // Refina em volta da melhor (de 2 em 2 no produto).
        let mut result = nominal;
        best_score = -1e300;
        for i in -COARSE..=COARSE {
            if best + i < lo || best + i > hi {
                continue;
            }
            let score = eval(best + i, 2);
            if score > best_score {
                best_score = score;
                result = best + i;
            }
        }
        result
    }

    fn step(&mut self, start: i64) {
        for c in 0..self.ch {
            for i in 0..self.w {
                let s = self.sample(c, start + i as i64);
                self.acc[c][i] += self.win[i] * s;
            }
        }
        // A primeira metade não recebe mais nada: sai pronta.
        let need = self.out_n + self.ho;
        for c in 0..self.ch {
            if self.outp[c].len() < need {
                self.outp[c].resize(4096.max(need * 2), 0.0);
            }
            self.outp[c][self.out_n..self.out_n + self.ho].copy_from_slice(&self.acc[c][..self.ho]);
            self.acc[c].copy_within(self.ho..self.w, 0);
            self.acc[c][self.w - self.ho..].iter_mut().for_each(|v| *v = 0.0);
        }
        self.out_n += self.ho;
        self.made += self.ho as i64;
        // Continuação natural: o que viria logo depois da metade que acabou de
        // sair. A próxima janela procura o trecho que mais se parece com isto.
        for i in 0..self.ho {
            self.tail[i] = self.mono(start + self.ho as i64 + i as i64);
        }
        self.has_tail = true;
        self.k += 1;
        self.keep_from = round_i64(self.k as f64 * self.hi) - self.tol;
    }

    /// Monta tudo o que a entrada recebida já permite.
    pub fn process(&mut self) {
        loop {
            let a = round_i64(self.k as f64 * self.hi);
            if a + self.tol + self.w as i64 > self.in_end {
                break; // falta entrada
            }
            let s = self.best_start(a);
            self.step(s);
        }
    }

    /// Fecha o trecho: completa com silêncio e corta para que saiam EXATAMENTE
    /// `more` amostras além das já produzidas — o que mantém o áudio grudado no
    /// vídeo emenda após emenda. Depois, começa do zero.
    pub fn finish(&mut self, more: i64) {
        // `out_n` pendente conta como já produzido (`made` inclui).
        let target = self.made + more.max(0);
        // Silêncio até produzir o bastante: é ele que fecha a última janela com a
        // descida da Hann, sem estalo.
        while self.made < target {
            let need = round_i64(self.k as f64 * self.hi) + self.tol + self.w as i64 - self.in_end;
            if need > 0 {
                // Silêncio que cairia antes da próxima leitura nem precisa existir.
                // Tudo o que estava guardado também fica antes dela: sai junto.
                if self.in_end + need <= self.keep_from {
                    self.in_end += need;
                    self.in_base = self.in_end;
                } else {
                    unsafe { self.put(None, need.min(i32::MAX as i64 / 8) as usize) };
                }
            }
            self.process();
        }
        // Produziu demais (a última janela passou do alvo): o excesso está no fim
        // da saída pronta.
        let extra = (self.out_n as i64).min(self.made - target);
        if extra > 0 {
            self.out_n -= extra as usize;
        }
        for a in &mut self.acc {
            a.iter_mut().for_each(|v| *v = 0.0);
        }
        self.restart();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, rate: f32, hz: f32) -> Vec<f32> {
        (0..n).map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / rate).sin() * 0.5).collect()
    }

    /// Duração EXATA no alvo: é a propriedade que segura o A/V sync emenda a emenda.
    #[test]
    fn duracao_exata_no_alvo() {
        let rate = 48000;
        let mut ts = TimeStretch::new(1, rate, 2.0);
        let input = tone(rate as usize * 2, rate as f32, 440.0);
        let mut planes = [std::ptr::null_mut::<u8>(); 8];
        let mut produced = 0usize;
        for chunk in input.chunks(1024) {
            planes[0] = chunk.as_ptr() as *mut u8;
            unsafe { ts.put(Some(&planes), chunk.len()) };
            ts.process();
            produced += ts.out_n;
            ts.out_n = 0;
        }
        // 2 s a 2× = 1 s = 48000 amostras.
        let target = 48000i64;
        ts.finish((target - produced as i64).max(0));
        produced += ts.out_n;
        assert_eq!(produced as i64, target);
    }

    #[test]
    fn silencio_nao_vira_ruido() {
        let mut ts = TimeStretch::new(2, 48000, 4.0);
        unsafe { ts.put(None, 48000) };
        ts.process();
        assert!(ts.out_n > 0);
        for c in 0..2 {
            assert!(ts.outp[c][..ts.out_n].iter().all(|v| *v == 0.0));
        }
    }
}

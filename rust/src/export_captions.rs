//! Legenda "queimada" no vídeo exportado (porte de `ExportCaptions.pas`).
//!
//! A transcrição (turnos: início, fim, texto) vira texto desenhado DENTRO dos
//! quadros de saída, no estilo da legenda do player: branco, negrito, contorno
//! escuro, centralizado embaixo, no máximo DUAS linhas por vez. Um turno tem até
//! 20 s de fala (pegadinha #60k) — em letra de legenda são cinco, seis linhas —,
//! então cada turno é partido em BLOCOS que cabem em duas linhas, medidos com a
//! fonte e a largura REAIS da saída, e o tempo do turno é dividido entre os
//! blocos na proporção dos caracteres (a regra do `Player._captionChunks`).
//!
//! GDI e não filtro do libav: o projeto não binda libavfilter/libass, e o texto
//! é simples. O GDI rasteriza em escala de cinza, o contorno sai de uma dilatação
//! da cobertura, e a mistura é feita direto nos planos YUV 4:2:0 do quadro (sem
//! converter para RGB). Os objetos GDI nascem e morrem na MESMA thread (exigência
//! do GDI): a da exportação.

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush, GetTextMetricsW,
    SelectObject, SetBkMode, SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DIB_RGB_COLORS, DRAW_TEXT_FORMAT, FF_SWISS, FW_BOLD, HBITMAP, HDC, HFONT, HGDIOBJ, OUT_TT_PRECIS,
    TEXTMETRICW, TRANSPARENT, DEFAULT_PITCH,
};

use crate::ffi::ffmpeg::AVFrame;
use crate::log;

/// Tamanho da letra em fração da ALTURA do quadro de saída: 4,5% é ~48 px em
/// 1080p (legenda de TV). Pela altura, não pela largura: num canvas de dois
/// monitores lado a lado a largura dobra e a letra ficaria enorme.
const CAPTION_FONT_FRACTION: f64 = 0.045;
const CAPTION_FONT_MIN: i32 = 14;
const CAPTION_FONT_MAX: i32 = 110;
/// Largura máxima do texto: 86% do quadro e no máximo ~32 letras — as proporções
/// do `.player-caption` (min(86%, 960px) com fonte de 30 px).
const CAPTION_WIDTH_FRACTION: f64 = 0.86;
const CAPTION_WIDTH_EMS: i32 = 32;
/// Distância da borda de baixo, em fração da altura.
const CAPTION_BOTTOM_FRACTION: f64 = 0.06;
/// Y em faixa limitada (TV range), o que os encoders do projeto recebem em YUV420P.
const Y_WHITE: i32 = 235;
const Y_BLACK: i32 = 16;

/// Um turno da transcrição, no relógio da ORIGEM (segundos).
#[derive(Debug, Clone, Default)]
pub struct CaptionTurn {
    pub start_sec: f64,
    pub end_sec: f64,
    pub text: String,
}

#[derive(Debug, Clone)]
struct Chunk {
    start_sec: f64,
    end_sec: f64,
    text: String,
}

pub struct CaptionBurner {
    out_w: i32,
    out_h: i32,
    line_h: i32,
    max_text_w: i32,
    radius: i32,
    pad: i32,
    bottom: i32,
    chunks: Vec<Chunk>,
    dc: HDC,
    font: HFONT,
    bmp: HBITMAP,
    old_font: HGDIOBJ,
    old_bmp: HGDIOBJ,
    /// DIB 32 bits, top-down.
    bits: *mut u8,
    bmp_w: i32,
    bmp_h: i32,
    // Bloco rasterizado em cache: só o CORRENTE. A legenda anda em ordem, e
    // guardar todos custaria centenas de MB numa transcrição longa.
    cur_idx: Option<usize>,
    cur_x: i32,
    cur_y: i32,
    cur_w: i32,
    cur_h: i32,
    /// cobertura final 0..255 (texto + contorno)
    alpha: Vec<u8>,
    /// Y do pixel da legenda (branco no texto, preto no contorno)
    luma: Vec<u8>,
    /// alfa médio do bloco 2x2, para U/V
    chroma_a: Vec<u8>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl CaptionBurner {
    pub fn new(out_w: i32, out_h: i32, turns: &[CaptionTurn]) -> Result<Self, String> {
        let font_px = ((out_h as f64 * CAPTION_FONT_FRACTION).round_ties_even() as i32).clamp(CAPTION_FONT_MIN, CAPTION_FONT_MAX);
        let max_text_w = ((out_w as f64 * CAPTION_WIDTH_FRACTION).round_ties_even() as i32).min(font_px * CAPTION_WIDTH_EMS);
        // Contorno proporcional à letra: ~1/14 da altura dela.
        let radius = (font_px / 14).max(2);
        let pad = radius + 2;
        let bottom = (out_h as f64 * CAPTION_BOTTOM_FRACTION).round_ties_even() as i32;

        let mut me = Self {
            out_w,
            out_h,
            line_h: 1,
            max_text_w,
            radius,
            pad,
            bottom,
            chunks: Vec::new(),
            dc: HDC::default(),
            font: HFONT::default(),
            bmp: HBITMAP::default(),
            old_font: HGDIOBJ::default(),
            old_bmp: HGDIOBJ::default(),
            bits: std::ptr::null_mut(),
            bmp_w: 0,
            bmp_h: 0,
            cur_idx: None,
            cur_x: 0,
            cur_y: 0,
            cur_w: 0,
            cur_h: 0,
            alpha: Vec::new(),
            luma: Vec::new(),
            chroma_a: Vec::new(),
        };
        unsafe {
            me.dc = CreateCompatibleDC(None);
            if me.dc.0.is_null() {
                return Err("CreateCompatibleDC falhou".into());
            }
            // ANTIALIASED_QUALITY e não ClearType: o ClearType rasteriza cada canal
            // de cor numa posição de subpixel diferente, e aqui a cobertura é lida
            // de UM canal só — as bordas das letras sairiam serrilhadas.
            me.font = CreateFontW(
                -font_px,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_TT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
                windows::core::w!("Segoe UI"),
            );
            if me.font.0.is_null() {
                return Err("CreateFont falhou".into());
            }
            me.old_font = SelectObject(me.dc, HGDIOBJ(me.font.0));
            let mut tm = TEXTMETRICW::default();
            let _ = GetTextMetricsW(me.dc, &mut tm);
            me.line_h = tm.tmHeight.max(1);

            // Um DIB só, do tamanho do maior bloco possível (duas linhas na largura
            // máxima + folga do contorno). Top-down (altura negativa): a linha 0 é a
            // de cima, como nos planos do quadro.
            me.bmp_w = me.max_text_w + me.pad * 2 + 4;
            me.bmp_h = me.line_h * 3 + me.pad * 2;
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: me.bmp_w,
                    biHeight: -me.bmp_h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            me.bmp = CreateDIBSection(Some(me.dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|_| "CreateDIBSection falhou".to_owned())?;
            if bits.is_null() {
                return Err("CreateDIBSection falhou".into());
            }
            me.bits = bits as *mut u8;
            me.old_bmp = SelectObject(me.dc, HGDIOBJ(me.bmp.0));
            SetBkMode(me.dc, TRANSPARENT);
            SetTextColor(me.dc, COLORREF(0x00FF_FFFF));
        }
        me.build_chunks(turns);
        log!("Export: legenda {} bloco(s), fonte {font_px}px, largura max {}px.", me.chunks.len(), me.max_text_w);
        Ok(me)
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Altura que o texto ocupa quebrando palavra na largura máxima.
    fn measure_height(&self, text: &str) -> i32 {
        let mut w = wide(text);
        let mut r = RECT { left: 0, top: 0, right: self.max_text_w, bottom: 0 };
        unsafe {
            DrawTextW(self.dc, &mut w, &mut r, DRAW_TEXT_FORMAT(0x400 | 0x10 | 0x1 | 0x800)); // CALCRECT|WORDBREAK|CENTER|NOPREFIX
        }
        r.bottom - r.top
    }

    /// Guloso palavra a palavra: fecha o bloco quando a próxima palavra o faria
    /// passar de duas linhas. O tempo do turno é dividido entre os blocos na
    /// proporção dos caracteres (mesma regra do player: prévia e arquivo iguais).
    fn build_chunks(&mut self, turns: &[CaptionTurn]) {
        let max_h = self.line_h * 2 + self.line_h / 2; // folga de arredondamento
        let mut chunks = Vec::new();
        for t in turns {
            let words: Vec<&str> = t.text.trim().split([' ', '\t', '\n', '\r']).filter(|w| !w.is_empty()).collect();
            if words.is_empty() {
                continue;
            }
            let mut texts: Vec<String> = Vec::new();
            let mut cur = String::new();
            for w in words {
                let next = if cur.is_empty() { w.to_owned() } else { format!("{cur} {w}") };
                if !cur.is_empty() && self.measure_height(&next) > max_h {
                    texts.push(std::mem::take(&mut cur));
                    cur = w.to_owned();
                } else {
                    cur = next;
                }
            }
            if !cur.is_empty() {
                texts.push(cur);
            }
            let total = texts.iter().map(|s| s.encode_utf16().count()).sum::<usize>().max(1) as f64;
            let dur = (t.end_sec - t.start_sec).max(0.0);
            let mut acc = 0usize;
            for text in texts {
                let len = text.encode_utf16().count();
                let start_sec = t.start_sec + dur * acc as f64 / total;
                acc += len;
                chunks.push(Chunk { start_sec, end_sec: t.start_sec + dur * acc as f64 / total, text });
            }
        }
        // Em ordem de INÍCIO: turnos sobrepostos (faixas isoladas) põem blocos de
        // um turno no meio dos do anterior, e a busca do find_chunk é binária.
        chunks.sort_by(|a, b| a.start_sec.total_cmp(&b.start_sec));
        self.chunks = chunks;
    }

    /// Bloco no ar no instante `sec`. Turnos podem se SOBREPOR: vale o que
    /// COMEÇOU POR ÚLTIMO entre os que contêm o instante (pegadinha #60p).
    fn find_chunk(&self, sec: f64) -> Option<usize> {
        if self.chunks.is_empty() || self.chunks[0].start_sec > sec {
            return None;
        }
        // Último bloco com início <= sec.
        let (mut lo, mut hi) = (0usize, self.chunks.len() - 1);
        while lo < hi {
            let mid = (lo + hi + 1) / 2;
            if self.chunks[mid].start_sec <= sec {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        // Volta alguns blocos: com turnos sobrepostos o último que começou pode já
        // ter acabado enquanto um anterior ainda está no ar.
        let stop = lo.saturating_sub(64);
        (stop..=lo).rev().find(|&k| self.chunks[k].start_sec <= sec && sec < self.chunks[k].end_sec)
    }

    /// Desenha o bloco no DIB, tira a cobertura, gera o contorno por dilatação e
    /// guarda alfa + luma prontos para misturar.
    fn rasterize(&mut self, idx: usize) {
        self.cur_idx = Some(idx);
        let mut text = wide(&self.chunks[idx].text);
        // Medida do texto do bloco (centralizado, quebrando na largura máxima).
        let mut r = RECT { left: 0, top: 0, right: self.max_text_w, bottom: 0 };
        unsafe {
            DrawTextW(self.dc, &mut text, &mut r, DRAW_TEXT_FORMAT(0x400 | 0x10 | 0x1 | 0x800));
        }
        let text_w = self.max_text_w.min(r.right - r.left);
        let text_h = (self.bmp_h - self.pad * 2).min(r.bottom - r.top);
        // Dimensões PARES: o croma do YUV420 tem metade da resolução, e bloco com
        // offset/lado ímpar desalinharia a mistura do U/V.
        let mut w = (self.bmp_w.min(text_w + self.pad * 2) + 1) & !1;
        let mut h = (self.bmp_h.min(text_h + self.pad * 2) + 1) & !1;
        w = w.min(self.out_w & !1);
        h = h.min(self.out_h & !1);
        self.cur_w = w;
        self.cur_h = h;
        self.cur_x = ((self.out_w - w) / 2) & !1;
        self.cur_y = (self.out_h - self.bottom - h).max(0) & !1;

        // Fundo preto, texto branco: a cobertura é o canal B do pixel.
        unsafe {
            std::ptr::write_bytes(self.bits, 0, (self.bmp_w * self.bmp_h * 4) as usize);
            let mut rr = RECT { left: self.pad, top: self.pad, right: self.pad + text_w, bottom: self.pad + text_h };
            DrawTextW(self.dc, &mut text, &mut rr, DRAW_TEXT_FORMAT(0x10 | 0x1 | 0x800)); // WORDBREAK|CENTER|NOPREFIX
            let _ = GdiFlush();
        }

        let (wu, hu) = (w as usize, h as usize);
        let mut cov = vec![0u8; wu * hu];
        for y in 0..hu {
            let row = unsafe { std::slice::from_raw_parts(self.bits.add(y * self.bmp_w as usize * 4), wu * 4) };
            for x in 0..wu {
                cov[y * wu + x] = row[x * 4];
            }
        }

        // Contorno = cobertura dilatada (máximo num quadrado de raio `radius`),
        // separável: primeiro na horizontal, depois na vertical.
        let rad = self.radius as i64;
        let mut tmp = vec![0u8; wu * hu];
        for y in 0..hu {
            for x in 0..wu {
                let mut m = 0u8;
                for dx in -rad..=rad {
                    let ix = x as i64 + dx;
                    if ix >= 0 && (ix as usize) < wu {
                        m = m.max(cov[y * wu + ix as usize]);
                    }
                }
                tmp[y * wu + x] = m;
            }
        }
        let mut outl = vec![0u8; wu * hu];
        for y in 0..hu {
            for x in 0..wu {
                let mut m = 0u8;
                for dy in -rad..=rad {
                    let iy = y as i64 + dy;
                    if iy >= 0 && (iy as usize) < hu {
                        m = m.max(tmp[iy as usize * wu + x]);
                    }
                }
                outl[y * wu + x] = m;
            }
        }

        self.alpha = vec![0; wu * hu];
        self.luma = vec![0; wu * hu];
        for i in 0..wu * hu {
            let f = cov[i] as i32;
            // Contorno um pouco translúcido: preto chapado a 100% pesa demais.
            let o = (outl[i] as i32 * 230) / 255;
            let a = f.max(o);
            self.alpha[i] = a as u8;
            // Branco onde há letra, preto no resto do contorno, com a borda da
            // letra misturando os dois.
            self.luma[i] = if a == 0 { Y_BLACK as u8 } else { ((f * Y_WHITE + (a - f) * Y_BLACK) / a) as u8 };
        }

        // Alfa do croma: média do bloco 2x2 correspondente.
        let (cw, ch) = (wu / 2, hu / 2);
        self.chroma_a = vec![0; cw * ch];
        for y in 0..ch {
            for x in 0..cw {
                let s = self.alpha[2 * y * wu + 2 * x] as u32
                    + self.alpha[2 * y * wu + 2 * x + 1] as u32
                    + self.alpha[(2 * y + 1) * wu + 2 * x] as u32
                    + self.alpha[(2 * y + 1) * wu + 2 * x + 1] as u32;
                self.chroma_a[y * cw + x] = (s / 4) as u8;
            }
        }
    }

    /// Desenha no quadro (YUV420P, `out_w × out_h`) o bloco que estiver no ar no
    /// instante `src_sec` da origem. Sem bloco no ar, não faz nada.
    pub fn blend_at(&mut self, src_sec: f64, frame: &mut AVFrame) {
        let Some(idx) = self.find_chunk(src_sec) else { return };
        if self.cur_idx != Some(idx) {
            self.rasterize(idx);
        }
        if self.cur_w <= 0 || self.cur_h <= 0 {
            return;
        }
        let (cw_full, ch_full) = (self.cur_w as usize, self.cur_h as usize);

        // Luma. Mistura com o alfa da legenda; o que o texto não cobre fica
        // intocado (A = 0 pula o pixel).
        for y in 0..ch_full {
            let off = (self.cur_y as usize + y) * frame.linesize[0] as usize + self.cur_x as usize;
            let py = unsafe { std::slice::from_raw_parts_mut(frame.data[0].add(off), cw_full) };
            for x in 0..cw_full {
                let a = self.alpha[y * cw_full + x] as i32;
                if a == 0 {
                    continue;
                }
                py[x] = ((py[x] as i32 * (255 - a) + self.luma[y * cw_full + x] as i32 * a) / 255) as u8;
            }
        }

        // Croma: a legenda é branca/preta, então o U/V dela é neutro (128) —
        // misturar puxa a cor de baixo para o cinza na medida do alfa.
        let cw = cw_full / 2;
        for y in 0..ch_full / 2 {
            let row = (self.cur_y as usize / 2 + y, self.cur_x as usize / 2);
            let pu = unsafe { std::slice::from_raw_parts_mut(frame.data[1].add(row.0 * frame.linesize[1] as usize + row.1), cw) };
            let pv = unsafe { std::slice::from_raw_parts_mut(frame.data[2].add(row.0 * frame.linesize[2] as usize + row.1), cw) };
            for x in 0..cw {
                let a = self.chroma_a[y * cw + x] as i32;
                if a == 0 {
                    continue;
                }
                pu[x] = ((pu[x] as i32 * (255 - a) + 128 * a) / 255) as u8;
                pv[x] = ((pv[x] as i32 * (255 - a) + 128 * a) / 255) as u8;
            }
        }
    }
}

impl Drop for CaptionBurner {
    fn drop(&mut self) {
        unsafe {
            if !self.dc.0.is_null() {
                if !self.old_bmp.0.is_null() {
                    SelectObject(self.dc, self.old_bmp);
                }
                if !self.old_font.0.is_null() {
                    SelectObject(self.dc, self.old_font);
                }
                let _ = DeleteDC(self.dc);
            }
            if !self.bmp.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            }
            if !self.font.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
        }
    }
}

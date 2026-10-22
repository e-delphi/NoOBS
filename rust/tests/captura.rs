//! Motor de captura próprio (docs/motor-proprio.md), testado FORA do app.
//!
//!   cargo test --release --features app-tests --test captura
//!   cargo test --release --features app-tests --test captura -- monitor
//!
//! Grava a tela de verdade por alguns segundos e confere cada arquivo pelo
//! `probe`. Não usa a libobs: só D3D11, WGC e as DLLs do FFmpeg de
//! `exe\` (achadas por `SetDllDirectoryW`, sem copiar o executável).
//!
//! Cada cenário imprime `OK`/`FALHA`/`PULADO <nome> <detalhe>`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use noobs::capture::encode::{AvVideoEncoder, VideoEncoder, VideoEncoderConfig};
use noobs::capture::gpu::{Compositor, Device, Nv12Converter, Rect};
use noobs::capture::audio::AudioDevice;
use noobs::capture::pipeline::{AudioConfig, AudioInputSpec, Session, SessionConfig, SourceSpec, VideoTarget};
use noobs::probe;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};

const HELPER_TITLE: &str = "NoOBS captura janela de teste";
const FLASH_TITLE: &str = "NoOBS captura pisca e bipa";

struct Report {
    ok: u32,
    fail: u32,
    skip: u32,
}

impl Report {
    fn take(&mut self, name: &str, r: Result<String, String>) {
        match r {
            Ok(d) => {
                self.ok += 1;
                println!("OK {name} {d}");
            }
            Err(e) if e.starts_with("PULADO:") => {
                self.skip += 1;
                println!("PULADO {name} {}", e.trim_start_matches("PULADO:").trim());
            }
            Err(e) => {
                self.fail += 1;
                println!("FALHA {name} {e}");
            }
        }
    }
}

fn check(c: bool, what: impl Into<String>) -> Result<(), String> {
    if c {
        Ok(())
    } else {
        Err(what.into())
    }
}

fn near(a: f64, b: f64, tol: f64, what: &str) -> Result<(), String> {
    check((a - b).abs() <= tol, format!("{what}: {a:.2} longe de {b:.2} (±{tol})"))
}

fn out_dir() -> PathBuf {
    std::env::temp_dir().join(format!("noobs-captura-{}", std::process::id()))
}

fn ffmpeg_exists(name: &str) -> bool {
    let Ok(a) = noobs::ffi::ffmpeg::api() else { return false };
    let n = noobs::ffi::ffmpeg::to_utf8(name);
    !unsafe { (a.codec.avcodec_find_encoder_by_name)(n.as_ptr()) }.is_null()
}

/// O encoder de vídeo que este hardware abre: AMF/NVENC; sem nenhum, x264.
fn best_h264(dev: &Device) -> String {
    for c in ["h264_amf", "h264_nvenc"] {
        if ffmpeg_exists(c) {
            let cfg = VideoEncoderConfig { codec: c.into(), width: 256, height: 144, fps: 30, crf: 23, keyframe_sec: 2 };
            if AvVideoEncoder::open(dev, &cfg).is_ok() {
                return c.into();
            }
        }
    }
    "libx264".into()
}

/// Monitores: (handle, x, y, w, h), na ordem do Windows.
fn monitors() -> Vec<(isize, i32, i32, u32, u32)> {
    noobs::win_preview::enumerate_monitors()
        .into_iter()
        .map(|m| {
            let h = unsafe { MonitorFromPoint(POINT { x: m.x + m.width / 2, y: m.y + m.height / 2 }, MONITOR_DEFAULTTONEAREST) };
            (h.0 as isize, m.x, m.y, m.width as u32, m.height as u32)
        })
        .collect()
}

fn even(v: u32) -> u32 {
    v & !1
}

// ---------------------------------------------------------------------------
// Cenários
// ---------------------------------------------------------------------------

/// Textura BGRA de uma cor só.
fn solid(dev: &Device, w: u32, h: u32, bgra: [u8; 4]) -> Result<ID3D11ShaderResourceView, String> {
    let px: Vec<u8> = (0..w * h).flat_map(|_| bgra).collect();
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let init = D3D11_SUBRESOURCE_DATA { pSysMem: px.as_ptr() as *const _, SysMemPitch: w * 4, SysMemSlicePitch: 0 };
    let mut t = None;
    unsafe { dev.device.CreateTexture2D(&desc, Some(&init), Some(&mut t)) }.map_err(|e| e.to_string())?;
    dev.srv(&t.unwrap(), DXGI_FORMAT_B8G8R8A8_UNORM)
}

/// Lê de volta o NV12: (Y, U, V) num ponto.
fn read_nv12(dev: &Device, nv12: &ID3D11Texture2D, w: u32, h: u32, x: u32, y: u32) -> Result<(u8, u8, u8), String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        ..Default::default()
    };
    let mut st = None;
    unsafe { dev.device.CreateTexture2D(&desc, None, Some(&mut st)) }.map_err(|e| e.to_string())?;
    let st = st.unwrap();
    unsafe {
        dev.ctx.CopyResource(&st, nv12);
        let mut m = D3D11_MAPPED_SUBRESOURCE::default();
        dev.ctx.Map(&st, 0, D3D11_MAP_READ, 0, Some(&mut m)).map_err(|e| e.to_string())?;
        let p = m.pData as *const u8;
        let pitch = m.RowPitch as usize;
        let yv = *p.add(y as usize * pitch + x as usize);
        let uvbase = p.add(pitch * h as usize + (y as usize / 2) * pitch + (x as usize / 2) * 2);
        let (u, v) = (*uvbase, *uvbase.add(1));
        dev.ctx.Unmap(&st, 0);
        Ok((yv, u, v))
    }
}

fn s_cor() -> Result<String, String> {
    let dev = Device::create()?;
    let (w, h) = (64u32, 32u32);
    let comp = Compositor::new(&dev, w, h)?;
    let conv = Nv12Converter::new(&dev, w, h)?;
    // Esquerda vermelha, direita branca; a faixa de baixo à direita preta (sem fonte).
    let red = solid(&dev, 8, 8, [0, 0, 255, 255])?;
    let white = solid(&dev, 8, 8, [255, 255, 255, 255])?;
    comp.clear(&dev);
    let full = Rect { x: 0.0, y: 0.0, w: 8.0, h: 8.0 };
    comp.draw(&dev, &red, 8, 8, full, Rect { x: 0.0, y: 0.0, w: 32.0, h: 32.0 });
    comp.draw(&dev, &white, 8, 8, full, Rect { x: 32.0, y: 0.0, w: 32.0, h: 16.0 });
    conv.convert(&dev, comp.canvas_srv());
    // BT.709 faixa limitada (os coeficientes do OBS): branco 235/128/128,
    // preto 16/128/128, vermelho 63/102/240.
    let casos = [((40, 4), (235, 128, 128), "branco"), ((40, 28), (16, 128, 128), "preto"), ((8, 8), (63, 102, 240), "vermelho")];
    let mut out = Vec::new();
    for ((x, y), (ey, eu, ev), nome) in casos {
        let (yy, u, v) = read_nv12(&dev, &conv.nv12, w, h, x, y)?;
        let ok = (yy as i32 - ey).abs() <= 1 && (u as i32 - eu).abs() <= 1 && (v as i32 - ev).abs() <= 1;
        check(ok, format!("{nome}: {yy}/{u}/{v}, esperado {ey}/{eu}/{ev}"))?;
        out.push(format!("{nome} {yy}/{u}/{v}"));
    }
    Ok(out.join(", "))
}

fn s_encoder(codec: &str) -> Result<String, String> {
    if !ffmpeg_exists(codec) {
        return Err("PULADO: encoder fora do build".into());
    }
    let dev = Device::create()?;
    let (w, h) = (1280u32, 720u32);
    let comp = Compositor::new(&dev, w, h)?;
    let conv = Nv12Converter::new(&dev, w, h)?;
    let cfg = VideoEncoderConfig { codec: codec.into(), width: w, height: h, fps: 30, crf: 23, keyframe_sec: 1 };
    let mut enc = match AvVideoEncoder::open(&dev, &cfg) {
        Ok(e) => e,
        Err(e) if codec.ends_with("_amf") || codec.ends_with("_nvenc") => return Err(format!("PULADO: sem esse hardware ({e})")),
        Err(e) => return Err(e),
    };
    let mut pkts = Vec::new();
    for i in 0..60 {
        // Quadro que muda: uma faixa branca andando.
        let white = solid(&dev, 8, 8, [255, 255, 255, 255])?;
        comp.clear(&dev);
        comp.draw(&dev, &white, 8, 8, Rect { x: 0.0, y: 0.0, w: 8.0, h: 8.0 }, Rect { x: (i * 20) as f32, y: 0.0, w: 40.0, h: h as f32 });
        conv.convert(&dev, comp.canvas_srv());
        enc.encode(&dev, &conv.nv12, i, i == 45, &mut pkts)?;
    }
    enc.flush(&mut pkts);
    check(pkts.len() == 60, format!("{} pacotes para 60 quadros", pkts.len()))?;
    let keys = pkts.iter().filter(|p| p.flags & 1 != 0).count();
    check(keys >= 2, format!("{keys} keyframe(s) em 2 s com keyint de 1 s"))?;
    // O quadro 45 foi FORÇADO (fora da grade de 30 em 30).
    check(pkts.iter().any(|p| p.pts == 45 && p.flags & 1 != 0), "o keyframe forçado no quadro 45 não saiu como keyframe")?;
    let info = enc.stream_info()?;
    let extradata = unsafe { (*info.params()).extradata_size };
    check(extradata > 0, "sem extradata (SPS/PPS) — o MKV sairia sem CodecPrivate")?;
    let bytes: usize = pkts.iter().map(|p| p.size as usize).sum();
    Ok(format!("60 quadros 1280x720 → {} KB, {keys} keyframes, extradata {extradata} B", bytes / 1024))
}

fn record(name: &str, secs: f64, mut cfg: SessionConfig) -> Result<(probe::ProbeReport, u64, u64, f64), String> {
    let path = out_dir().join(format!("{name}.mkv")).to_string_lossy().into_owned();
    cfg.output_path = path.clone();
    let s = Session::start(cfg)?;
    std::thread::sleep(Duration::from_secs_f64(secs));
    let busy = s.stats.busy_us.load(std::sync::atomic::Ordering::Relaxed);
    let end = s.stop();
    if let Some(e) = end.error {
        return Err(e);
    }
    let r = probe::probe(Path::new(&path)).ok_or_else(|| format!("probe falhou em {path}"))?;
    let per_frame_ms = if end.frames > 0 { busy as f64 / end.frames as f64 / 1000.0 } else { 0.0 };
    Ok((r, end.frames, end.lagged, per_frame_ms))
}

fn s_monitor(dev_codec: &str) -> Result<String, String> {
    let mons = monitors();
    let (h, _, _, w, hh) = *mons.first().ok_or("sem monitor")?;
    let (cw, ch) = (even(w), even(hh));
    let cfg = SessionConfig {
        width: cw,
        height: ch,
        fps: 30,
        sources: vec![SourceSpec { name: "Monitor 1".into(), target: VideoTarget::Monitor(h), crop: None, dest: Some(Rect { x: 0.0, y: 0.0, w: cw as f32, h: ch as f32 }) }],
        encoder: VideoEncoderConfig { codec: dev_codec.into(), width: cw, height: ch, fps: 30, crf: 23, keyframe_sec: 2 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    let (r, frames, lagged, ms) = record("monitor", 3.0, cfg)?;
    let v = r.video_stream().ok_or("sem vídeo")?;
    check(v.codec == "h264", format!("codec {}", v.codec))?;
    check(v.width as u32 == cw && v.height as u32 == ch, format!("{}x{} (esperado {cw}x{ch})", v.width, v.height))?;
    near(r.duration, 3.0, 0.5, "duração")?;
    near(v.frame_rate, 30.0, 1.0, "fps")?;
    check(lagged * 10 <= frames, format!("{lagged} de {frames} quadros atrasados"))?;
    Ok(format!("{dev_codec} {cw}x{ch} {:.2}s, {frames} quadros ({lagged} atrasados), {ms:.1} ms/quadro, {} KB", r.duration, r.size / 1024))
}

fn s_dois_monitores(codec: &str) -> Result<String, String> {
    let mons = monitors();
    if mons.len() < 2 {
        return Err("PULADO: um monitor só".into());
    }
    // Lado a lado, compacto (soma das larguras × maior altura) — #11 — e
    // reduzido à metade pra caber em qualquer encoder H.264 (4096, #7).
    let total_w: u32 = mons.iter().map(|m| m.3).sum();
    let max_h = mons.iter().map(|m| m.4).max().unwrap_or(0);
    let scale = (4096.0 / total_w as f32).min(1.0);
    let (cw, ch) = (even((total_w as f32 * scale) as u32), even((max_h as f32 * scale) as u32));
    let mut x = 0.0;
    let mut sources = Vec::new();
    for (i, m) in mons.iter().enumerate() {
        let (dw, dh) = (m.3 as f32 * scale, m.4 as f32 * scale);
        sources.push(SourceSpec { name: format!("Monitor {}", i + 1), target: VideoTarget::Monitor(m.0), crop: None, dest: Some(Rect { x, y: 0.0, w: dw, h: dh }) });
        x += dw;
    }
    let cfg = SessionConfig {
        width: cw,
        height: ch,
        fps: 30,
        sources,
        encoder: VideoEncoderConfig { codec: codec.into(), width: cw, height: ch, fps: 30, crf: 23, keyframe_sec: 2 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    let (r, frames, lagged, ms) = record("dois-monitores", 2.0, cfg)?;
    let v = r.video_stream().ok_or("sem vídeo")?;
    check(v.width as u32 == cw && v.height as u32 == ch, format!("{}x{}", v.width, v.height))?;
    near(r.duration, 2.0, 0.5, "duração")?;
    Ok(format!("{} monitores → {cw}x{ch}, {frames} quadros ({lagged} atrasados), {ms:.1} ms/quadro", mons.len()))
}

fn s_area(codec: &str) -> Result<String, String> {
    let mons = monitors();
    let (h, _, _, _, _) = *mons.first().ok_or("sem monitor")?;
    // 640x360 a partir de (100, 100) do 1º monitor, num canvas do mesmo tamanho.
    let cfg = SessionConfig {
        width: 640,
        height: 360,
        fps: 60,
        sources: vec![SourceSpec {
            name: "Area".into(),
            target: VideoTarget::Monitor(h),
            crop: Some(Rect { x: 100.0, y: 100.0, w: 640.0, h: 360.0 }),
            dest: Some(Rect { x: 0.0, y: 0.0, w: 640.0, h: 360.0 }),
        }],
        encoder: VideoEncoderConfig { codec: codec.into(), width: 640, height: 360, fps: 60, crf: 23, keyframe_sec: 2 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    let (r, frames, lagged, ms) = record("area", 2.0, cfg)?;
    let v = r.video_stream().ok_or("sem vídeo")?;
    check(v.width == 640 && v.height == 360, format!("{}x{}", v.width, v.height))?;
    near(v.frame_rate, 60.0, 1.0, "fps")?;
    Ok(format!("640x360 @60, {frames} quadros ({lagged} atrasados), {ms:.1} ms/quadro"))
}

fn s_software() -> Result<String, String> {
    let mons = monitors();
    let (h, _, _, _, _) = *mons.first().ok_or("sem monitor")?;
    let cfg = SessionConfig {
        width: 1280,
        height: 720,
        fps: 30,
        sources: vec![SourceSpec { name: "Monitor 1".into(), target: VideoTarget::Monitor(h), crop: None, dest: None }],
        encoder: VideoEncoderConfig { codec: "libx264".into(), width: 1280, height: 720, fps: 30, crf: 23, keyframe_sec: 2 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    let (r, frames, lagged, ms) = record("x264", 2.0, cfg)?;
    let v = r.video_stream().ok_or("sem vídeo")?;
    check(v.codec == "h264", format!("codec {}", v.codec))?;
    near(r.duration, 2.0, 0.5, "duração")?;
    Ok(format!("libx264 1280x720 (monitor encaixado), {frames} quadros ({lagged} atrasados), {ms:.1} ms/quadro"))
}

fn s_janela(exe: &str, codec: &str) -> Result<String, String> {
    let mut child = std::process::Command::new(exe).arg("--janela").spawn().map_err(|e| format!("helper: {e}"))?;
    let r = (|| {
        let mut found = None;
        for _ in 0..50 {
            found = noobs::win_windows::enumerate_capture_windows().into_iter().find(|w| w.title == HELPER_TITLE);
            if found.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let w = found.ok_or("a janela do helper não apareceu")?;
        let (cw, chh) = noobs::win_windows::capture_window_size(w.handle).ok_or("sem tamanho")?;
        let (cw, chh) = (even(cw as u32), even(chh as u32));
        let cfg = SessionConfig {
            width: cw,
            height: chh,
            fps: 30,
            sources: vec![SourceSpec { name: w.title.clone(), target: VideoTarget::Window { hwnd: w.handle, client_area: true }, crop: None, dest: None }],
            encoder: VideoEncoderConfig { codec: codec.into(), width: cw, height: chh, fps: 30, crf: 23, keyframe_sec: 2 },
            audio: None,
            replay: None,
            output_path: String::new(),
        };
        let (r, frames, _, _) = record("janela", 2.0, cfg)?;
        let v = r.video_stream().ok_or("sem vídeo")?;
        check(v.width as u32 == cw && v.height as u32 == chh, format!("{}x{}", v.width, v.height))?;
        Ok(format!("cliente {cw}x{chh}, {frames} quadros"))
    })();
    let _ = child.kill();
    let _ = child.wait();
    r
}

/// Fecha a janela gravada NO MEIO da gravação: a sessão segue (o espaço fica
/// preto) e o arquivo fecha íntegro — o caso que corrompia a libobs.
fn s_janela_fecha(exe: &str, codec: &str) -> Result<String, String> {
    let mut child = std::process::Command::new(exe).arg("--janela").spawn().map_err(|e| format!("helper: {e}"))?;
    let mut found = None;
    for _ in 0..50 {
        found = noobs::win_windows::enumerate_capture_windows().into_iter().find(|w| w.title == HELPER_TITLE);
        if found.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let Some(w) = found else {
        let _ = child.kill();
        return Err("a janela do helper não apareceu".into());
    };
    let path = out_dir().join("janela-fecha.mkv").to_string_lossy().into_owned();
    let cfg = SessionConfig {
        width: 640,
        height: 480,
        fps: 30,
        sources: vec![SourceSpec { name: w.title.clone(), target: VideoTarget::Window { hwnd: w.handle, client_area: true }, crop: None, dest: None }],
        encoder: VideoEncoderConfig { codec: codec.into(), width: 640, height: 480, fps: 30, crf: 23, keyframe_sec: 2 },
        audio: None,
        replay: None,
        output_path: path.clone(),
    };
    let s = Session::start(cfg)?;
    std::thread::sleep(Duration::from_millis(1000));
    let _ = child.kill();
    let _ = child.wait();
    std::thread::sleep(Duration::from_millis(1500));
    let end = s.stop();
    if let Some(e) = end.error {
        return Err(e);
    }
    let r = probe::probe(Path::new(&path)).ok_or("probe falhou")?;
    near(r.duration, 2.5, 0.5, "duração com a janela fechada no meio")?;
    Ok(format!("{:.1}s, {} quadros — gravação seguiu depois de a janela fechar", r.duration, end.frames))
}

/// Mistura + microfone + som do sistema: três faixas AAC com nome, do mesmo
/// tamanho do vídeo.
fn s_audio_faixas(codec: &str) -> Result<String, String> {
    let mons = monitors();
    let (h, _, _, _, _) = *mons.first().ok_or("sem monitor")?;
    let cfg = SessionConfig {
        width: 640,
        height: 360,
        fps: 30,
        sources: vec![SourceSpec { name: "Monitor 1".into(), target: VideoTarget::Monitor(h), crop: None, dest: None }],
        encoder: VideoEncoderConfig { codec: codec.into(), width: 640, height: 360, fps: 30, crf: 23, keyframe_sec: 2 },
        audio: Some(AudioConfig {
            inputs: vec![
                AudioInputSpec { name: "Microfone".into(), device: AudioDevice::Input(String::new()), tracks: vec![0, 1] },
                AudioInputSpec { name: "Sistema".into(), device: AudioDevice::Output(String::new()), tracks: vec![0, 2] },
            ],
            track_titles: vec!["Mistura".into(), "Microfone".into(), "Sistema".into()],
            kbps: 192,
        }),
        replay: None,
        output_path: String::new(),
    };
    let (r, frames, _, _) = record("audio-faixas", 3.0, cfg)?;
    let auds: Vec<_> = r.audio_streams().collect();
    check(auds.len() == 3, format!("{} faixas de áudio", auds.len()))?;
    let titles: Vec<String> = auds.iter().map(|a| a.title.clone()).collect();
    check(titles == ["Mistura", "Microfone", "Sistema"], format!("títulos {titles:?}"))?;
    for a in &auds {
        check(a.codec == "aac" && a.sample_rate == 48000 && a.channels == 2, format!("faixa {}: {} {} Hz {} canais", a.title, a.codec, a.sample_rate, a.channels))?;
    }
    near(r.duration, 3.0, 0.5, "duração")?;
    Ok(format!("3 faixas AAC 48 kHz estéreo {titles:?}, {:.2}s, {frames} quadros", r.duration))
}

/// Primeiros pacotes de cada faixa: (faixa, pts em s, keyframe).
fn first_packets(path: &str, n: usize) -> Vec<(u32, f64, bool)> {
    use noobs::ffi::ffmpeg::*;
    let mut out = Vec::new();
    let Ok(a) = api() else { return out };
    let Ok(inp) = Input::open(path) else { return out };
    let mut pkt = Packet::alloc().unwrap();
    let mut per = std::collections::HashMap::<u32, usize>::new();
    while unsafe { (a.format.av_read_frame)(inp.ptr(), pkt.ptr()) } >= 0 && out.len() < n * 3 {
        let s = pkt.stream_index as u32;
        let c = per.entry(s).or_default();
        if *c < n {
            *c += 1;
            let tb = inp.stream(s).unwrap().time_base;
            out.push((s, pkt.pts as f64 * tb.num as f64 / tb.den as f64, pkt.flags & 1 != 0));
        }
        pkt.unref();
    }
    out
}

/// Instante (s) do 1º quadro claro: média do Y no centro acima de `limiar`.
fn first_bright_frame(path: &str, limiar: f64) -> Result<f64, String> {
    use noobs::ffi::ffmpeg::*;
    let a = api()?;
    let inp = Input::open(path).map_err(|e| format!("abrir {path}: {e}"))?;
    if !inp.find_stream_info() {
        return Err("find_stream_info".into());
    }
    let vi = (0..inp.nb_streams())
        .find(|&i| inp.stream(i).and_then(|s| unsafe { s.codecpar.as_ref() }).is_some_and(|c| c.codec_type == AVMEDIA_TYPE_VIDEO))
        .ok_or("sem vídeo")?;
    let st = inp.stream(vi).ok_or("stream")?;
    let tb = st.time_base;
    let dec = CodecCtx::open_decoder_for(unsafe { &*st.codecpar }, true).ok_or("decoder")?;
    let mut pkt = Packet::alloc().ok_or("pkt")?;
    let mut fr = Frame::alloc().ok_or("frame")?;
    loop {
        let rc = unsafe { (a.format.av_read_frame)(inp.ptr(), pkt.ptr()) };
        let eof = rc < 0;
        if !eof && pkt.stream_index as u32 != vi {
            pkt.unref();
            continue;
        }
        unsafe { (a.codec.avcodec_send_packet)(dec.ptr(), if eof { std::ptr::null() } else { pkt.ptr() }) };
        pkt.unref();
        while unsafe { (a.codec.avcodec_receive_frame)(dec.ptr(), fr.ptr()) } >= 0 {
            let (w, h, ls) = (fr.width as usize, fr.height as usize, fr.linesize[0] as usize);
            let mut sum = 0u64;
            let mut n = 0u64;
            for y in (h / 4..h * 3 / 4).step_by(4) {
                for x in (w / 4..w * 3 / 4).step_by(4) {
                    sum += unsafe { *fr.data[0].add(y * ls + x) } as u64;
                    n += 1;
                }
            }
            let mean = sum as f64 / n.max(1) as f64;
            let ts = fr.pts as f64 * tb.num as f64 / tb.den as f64;
            fr.unref();
            if mean > limiar {
                return Ok(ts);
            }
        }
        if eof {
            return Err("nenhum quadro claro".into());
        }
    }
}

/// Só o loopback: uma fonte de saída e o bipe do helper, lidos do anel.
fn s_audio_loopback(exe: &str) -> Result<String, String> {
    use noobs::capture::audio::{AudioSource, WasapiSource};
    use noobs::capture::clock::now_ns;
    let src = WasapiSource::start("Sistema", AudioDevice::Output(String::new()))?;
    let mut child = std::process::Command::new(exe).arg("--pisca").spawn().map_err(|e| format!("helper: {e}"))?;
    let t_start = now_ns();
    std::thread::sleep(Duration::from_millis(2800));
    let _ = child.kill();
    let _ = child.wait();
    // Lê de 10 em 10 ms desde o começo.
    let mut best = (0f32, 0u64);
    let mut buf = vec![0f32; 480 * 2];
    let mut t = t_start;
    while t + 10_000_000 < now_ns() {
        buf.fill(0.0);
        src.mix_into(t, &mut buf);
        let pk = buf.iter().fold(0f32, |m, s| m.max(s.abs()));
        if pk > best.0 {
            best = (pk, t);
        }
        t += 10_000_000;
    }
    // Depois do volume do sistema: com o volume baixo o 0,8 do bipe chega bem menor.
    check(best.0 > 0.002, format!("pico máximo {:.4} — o loopback não viu o bipe", best.0))?;
    Ok(format!("pico {:.2} em {:.2}s depois de abrir o helper", best.0, (best.1 - t_start) as f64 / 1e9))
}

/// Pico máximo lido de uma fonte, de `t0` até agora, em janelas de 10 ms.
fn peak_since(src: &dyn noobs::capture::audio::AudioSource, t0: u64) -> (f32, u64) {
    use noobs::capture::clock::now_ns;
    let mut best = (0f32, 0u64);
    let mut buf = vec![0f32; 480 * 2];
    let mut t = t0;
    while t + 10_000_000 < now_ns() {
        buf.fill(0.0);
        src.mix_into(t, &mut buf);
        let pk = buf.iter().fold(0f32, |m, s| m.max(s.abs()));
        if pk > best.0 {
            best = (pk, t);
        }
        t += 10_000_000;
    }
    best
}

/// Som de UM app: o PID do helper que bipa tem o bipe; o de OUTRO helper
/// (mudo) fica em silêncio enquanto o primeiro bipa. (O PID deste teste não
/// serve de referência: o modo inclui a árvore de FILHOS, e os helpers são.)
fn s_audio_app(exe: &str) -> Result<String, String> {
    use noobs::capture::audio::WasapiSource;
    use noobs::capture::clock::now_ns;
    let mut child = std::process::Command::new(exe).arg("--pisca").spawn().map_err(|e| format!("helper: {e}"))?;
    let mut mudo = std::process::Command::new(exe).arg("--janela").spawn().map_err(|e| format!("helper mudo: {e}"))?;
    let r = (|| {
        let app = match WasapiSource::start("App", AudioDevice::App(child.id())) {
            Ok(s) => s,
            Err(e) if e.contains("2004") => return Err(format!("PULADO: {e}")),
            Err(e) => return Err(e),
        };
        let eu = WasapiSource::start("Mudo", AudioDevice::App(mudo.id()))?;
        let t0 = now_ns();
        std::thread::sleep(Duration::from_millis(2800));
        let (pk_app, t_app) = peak_since(&app, t0);
        let (pk_eu, _) = peak_since(&eu, t0);
        check(pk_app > 0.05, format!("o loopback do helper não viu o bipe (pico {pk_app:.4})"))?;
        check(pk_eu < 0.001, format!("o loopback do helper mudo pegou som de outro processo (pico {pk_eu:.4})"))?;
        Ok(format!("helper que bipa: pico {pk_app:.2} em {:.2}s (antes do volume do sistema); helper mudo: {pk_eu:.4}", (t_app - t0) as f64 / 1e9))
    })();
    for c in [&mut child, &mut mudo] {
        let _ = c.kill();
        let _ = c.wait();
    }
    r
}

/// Webcam: 2 s da primeira câmera, encaixada num canvas 640x480.
fn s_webcam(codec: &str) -> Result<String, String> {
    let cams = noobs::capture::webcam::list_webcams();
    let Some(cam) = cams.first().cloned() else { return Err("PULADO: nenhuma câmera".into()) };
    let cfg = SessionConfig {
        width: 640,
        height: 480,
        fps: 30,
        sources: vec![SourceSpec { name: cam.clone(), target: VideoTarget::Webcam(cam.clone()), crop: None, dest: None }],
        encoder: VideoEncoderConfig { codec: codec.into(), width: 640, height: 480, fps: 30, crf: 23, keyframe_sec: 2 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    let r = match record("webcam", 2.0, cfg) {
        Ok(r) => r,
        Err(e) if e.contains("privacidade") || e.contains("em uso") => return Err(format!("PULADO: {e}")),
        Err(e) => return Err(e),
    };
    let (rep, frames, lagged, _) = r;
    let v = rep.video_stream().ok_or("sem vídeo")?;
    check(v.width == 640 && v.height == 480, format!("{}x{}", v.width, v.height))?;
    near(rep.duration, 2.0, 0.5, "duração")?;
    Ok(format!("\"{cam}\" ({} câmera(s)) → 640x480, {frames} quadros ({lagged} atrasados)", cams.len()))
}

/// Área cliente de uma janela em pixels físicos (este processo é DPI-aware).
fn client_rect(hwnd: isize) -> Option<noobs::capture::layout::DesktopRect> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    let h = HWND(hwnd as *mut _);
    let mut r = RECT::default();
    let mut o = POINT::default();
    unsafe {
        GetClientRect(h, &mut r).ok()?;
        if !ClientToScreen(h, &mut o).as_bool() {
            return None;
        }
    }
    Some(noobs::capture::layout::DesktopRect { x: o.x, y: o.y, w: r.right, h: r.bottom })
}

fn find_window(title: &str) -> Option<isize> {
    for _ in 0..50 {
        if let Some(w) = noobs::win_windows::enumerate_capture_windows().into_iter().find(|w| w.title == title) {
            return Some(w.handle);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

/// Mover a área no meio: começa sobre uma janela PRETA e, aos 1,2 s, vai pra
/// uma BRANCA. O vídeo não muda de tamanho e fica claro a partir dali.
fn s_area_move(exe: &str, codec: &str) -> Result<String, String> {
    use noobs::capture::layout::{area_layout, DesktopRect};
    let mut preta = std::process::Command::new(exe).args(["--cor", "preta"]).spawn().map_err(|e| format!("helper: {e}"))?;
    let mut branca = std::process::Command::new(exe).args(["--cor", "branca"]).spawn().map_err(|e| format!("helper: {e}"))?;
    let r = (|| {
        let hp = find_window("NoOBS captura cor preta").ok_or("janela preta não apareceu")?;
        let hb = find_window("NoOBS captura cor branca").ok_or("janela branca não apareceu")?;
        std::thread::sleep(Duration::from_millis(300));
        let (cp, cb) = (client_rect(hp).ok_or("rect preta")?, client_rect(hb).ok_or("rect branca")?);
        let area = |c: DesktopRect| DesktopRect { x: c.x + c.w / 2 - 160, y: c.y + c.h / 2 - 120, w: 320, h: 240 };
        let mons = monitors();
        let desk: Vec<DesktopRect> = mons.iter().map(|m| DesktopRect { x: m.1, y: m.2, w: m.3 as i32, h: m.4 as i32 }).collect();
        let canvas = (320u32, 240u32);
        let hide = Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        let to_specs = |l: &[noobs::capture::layout::Placement]| -> Vec<(Option<Rect>, Option<Rect>)> {
            l.iter().map(|p| match p {
                Some((c, d)) => (Some(*c), Some(*d)),
                None => (None, Some(hide)),
            }).collect()
        };
        let first = to_specs(&area_layout(area(cp), canvas, &desk));
        let sources = mons
            .iter()
            .enumerate()
            .map(|(i, m)| SourceSpec { name: format!("Area Monitor {}", i + 1), target: VideoTarget::Monitor(m.0), crop: first[i].0, dest: first[i].1 })
            .collect();
        let path = out_dir().join("area-move.mkv").to_string_lossy().into_owned();
        let cfg = SessionConfig {
            width: canvas.0,
            height: canvas.1,
            fps: 30,
            sources,
            encoder: VideoEncoderConfig { codec: codec.into(), width: canvas.0, height: canvas.1, fps: 30, crf: 23, keyframe_sec: 1 },
            audio: None,
            replay: None,
            output_path: path.clone(),
        };
        let s = Session::start(cfg)?;
        std::thread::sleep(Duration::from_millis(1200));
        for (i, (c, d)) in to_specs(&area_layout(area(cb), canvas, &desk)).into_iter().enumerate() {
            check(s.set_placement(i, c, d), "set_placement recusou")?;
        }
        std::thread::sleep(Duration::from_millis(1300));
        let end = s.stop();
        if let Some(e) = end.error {
            return Err(e);
        }
        let rep = probe::probe(Path::new(&path)).ok_or("probe")?;
        let v = rep.video_stream().ok_or("sem vídeo")?;
        check(v.width == 320 && v.height == 240, format!("mudou de tamanho: {}x{}", v.width, v.height))?;
        let t_claro = first_bright_frame(&path, 128.0)?;
        check((0.9..=1.7).contains(&t_claro), format!("ficou claro em {t_claro:.2}s (a área foi movida em ~1,2 s)"))?;
        Ok(format!("320x240 o tempo todo; preto → branco em {t_claro:.2}s (movida em ~1,2 s)"))
    })();
    for c in [&mut preta, &mut branca] {
        let _ = c.kill();
        let _ = c.wait();
    }
    r
}

/// Janela de uma cor num retângulo FÍSICO da área de trabalho (helper `--cor
/// --pos`); morre junto com o valor.
struct Painel(std::process::Child);

impl Drop for Painel {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn painel(exe: &str, cor: &str, tag: &str, r: noobs::capture::layout::DesktopRect) -> Result<Painel, String> {
    let args = ["--cor".to_owned(), cor.to_owned(), "--pos".into(), r.x.to_string(), r.y.to_string(), r.w.to_string(), r.h.to_string(), "--tag".into(), tag.to_owned()];
    let p = Painel(std::process::Command::new(exe).args(args).spawn().map_err(|e| format!("helper: {e}"))?);
    let title = format!("NoOBS captura cor {cor} {tag}");
    let h = find_window(&title).ok_or_else(|| format!("\"{title}\" não apareceu"))?;
    let c = client_rect(h).ok_or("rect do painel")?;
    check(c.x == r.x && c.y == r.y, format!("painel {tag} em {},{} (pedido {},{})", c.x, c.y, r.x, r.y))?;
    Ok(p)
}

/// Luma média (bloco 9×9) em cada ponto do 1º quadro com pts ≥ `t`.
fn luma_at(path: &str, t: f64, pts: &[(u32, u32)]) -> Result<Vec<u8>, String> {
    use noobs::ffi::ffmpeg::*;
    let a = api()?;
    let inp = Input::open(path).map_err(|e| format!("abrir {path}: {e}"))?;
    if !inp.find_stream_info() {
        return Err("find_stream_info".into());
    }
    let vi = (0..inp.nb_streams())
        .find(|&i| inp.stream(i).and_then(|s| unsafe { s.codecpar.as_ref() }).is_some_and(|c| c.codec_type == AVMEDIA_TYPE_VIDEO))
        .ok_or("sem vídeo")?;
    let st = inp.stream(vi).ok_or("stream")?;
    let tb = st.time_base;
    let dec = CodecCtx::open_decoder_for(unsafe { &*st.codecpar }, true).ok_or("decoder")?;
    let mut pkt = Packet::alloc().ok_or("pkt")?;
    let mut fr = Frame::alloc().ok_or("frame")?;
    loop {
        let rc = unsafe { (a.format.av_read_frame)(inp.ptr(), pkt.ptr()) };
        let eof = rc < 0;
        if !eof && pkt.stream_index as u32 != vi {
            pkt.unref();
            continue;
        }
        unsafe { (a.codec.avcodec_send_packet)(dec.ptr(), if eof { std::ptr::null() } else { pkt.ptr() }) };
        pkt.unref();
        while unsafe { (a.codec.avcodec_receive_frame)(dec.ptr(), fr.ptr()) } >= 0 {
            let ts = fr.pts as f64 * tb.num as f64 / tb.den as f64;
            if ts < t {
                fr.unref();
                continue;
            }
            let (w, h, ls) = (fr.width as i64, fr.height as i64, fr.linesize[0] as usize);
            let out = pts
                .iter()
                .map(|&(px, py)| {
                    let (mut sum, mut n) = (0u64, 0u64);
                    for y in (py as i64 - 4).max(0)..(py as i64 + 5).min(h) {
                        for x in (px as i64 - 4).max(0)..(px as i64 + 5).min(w) {
                            sum += unsafe { *fr.data[0].add(y as usize * ls + x as usize) } as u64;
                            n += 1;
                        }
                    }
                    (sum / n.max(1)) as u8
                })
                .collect();
            fr.unref();
            return Ok(out);
        }
        if eof {
            return Err(format!("nenhum quadro depois de {t:.1}s"));
        }
    }
}

/// Monitores da esquerda pra direita — a ordem em que o app os põe lado a lado.
fn monitors_by_x() -> Vec<(isize, i32, i32, u32, u32)> {
    let mut m = monitors();
    m.sort_by_key(|m| m.1);
    m
}

/// Cada monitor no SEU lugar do quadro: uma janela branca no centro do da
/// esquerda e uma preta no centro do da direita; no arquivo, o centro da
/// metade esquerda tem que sair claro e o da direita escuro (trocar a ordem
/// ou repetir um monitor nos dois lados dá outra coisa).
fn s_monitores_conteudo(exe: &str, codec: &str) -> Result<String, String> {
    use noobs::capture::layout::DesktopRect;
    let mons = monitors_by_x();
    if mons.len() < 2 {
        return Err("PULADO: um monitor só".into());
    }
    let centro = |m: &(isize, i32, i32, u32, u32)| DesktopRect { x: m.1 + m.3 as i32 / 2 - 200, y: m.2 + m.4 as i32 / 2 - 150, w: 400, h: 300 };
    let cores: Vec<&str> = (0..mons.len()).map(|i| if i % 2 == 0 { "branca" } else { "preta" }).collect();
    let mut paineis = Vec::new();
    for (i, m) in mons.iter().enumerate() {
        paineis.push(painel(exe, cores[i], &format!("m{i}"), centro(m))?);
    }
    std::thread::sleep(Duration::from_millis(400));
    let total_w: u32 = mons.iter().map(|m| m.3).sum();
    let max_h = mons.iter().map(|m| m.4).max().unwrap_or(0);
    let scale = (4096.0 / total_w as f32).min(1.0);
    let (cw, ch) = (even((total_w as f32 * scale) as u32), even((max_h as f32 * scale) as u32));
    let mut x = 0.0f32;
    let mut sources = Vec::new();
    let mut pontos = Vec::new();
    for (i, m) in mons.iter().enumerate() {
        let (dw, dh) = (m.3 as f32 * scale, m.4 as f32 * scale);
        sources.push(SourceSpec { name: format!("Monitor {}", i + 1), target: VideoTarget::Monitor(m.0), crop: None, dest: Some(Rect { x, y: 0.0, w: dw, h: dh }) });
        pontos.push(((x + dw / 2.0) as u32, (dh / 2.0) as u32));
        x += dw;
    }
    let cfg = SessionConfig {
        width: cw,
        height: ch,
        fps: 30,
        sources,
        encoder: VideoEncoderConfig { codec: codec.into(), width: cw, height: ch, fps: 30, crf: 23, keyframe_sec: 1 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    let (_, frames, lagged, _) = record("monitores-conteudo", 1.5, cfg)?;
    drop(paineis);
    let path = out_dir().join("monitores-conteudo.mkv").to_string_lossy().into_owned();
    let lumas = luma_at(&path, 1.0, &pontos)?;
    for (i, l) in lumas.iter().enumerate() {
        let claro = cores[i] == "branca";
        check(if claro { *l > 200 } else { *l < 40 }, format!("monitor {} ({}) saiu com luma {l} no centro da sua faixa — lumas {lumas:?}", i + 1, cores[i]))?;
    }
    Ok(format!("{} monitores → {cw}x{ch}, centros {lumas:?} (branco/preto alternados, na ordem da esquerda pra direita), {frames} quadros ({lagged} atrasados)", mons.len()))
}

/// Área que ATRAVESSA a divisa entre dois monitores lado a lado: branca à
/// esquerda da divisa, preta à direita. O quadro sai com a metade esquerda
/// clara e a direita escura — os dois pedaços, cada um do seu monitor.
fn s_area_entre_monitores(exe: &str, codec: &str) -> Result<String, String> {
    use noobs::capture::layout::{area_layout, DesktopRect};
    let mons = monitors_by_x();
    let Some((m0, m1)) = mons.windows(2).map(|w| (w[0], w[1])).find(|(a, b)| a.1 + a.3 as i32 == b.1) else {
        return Err("PULADO: sem dois monitores encostados lado a lado".into());
    };
    let div = m1.1;
    let y0 = m0.2.max(m1.2) + 200;
    let _b = painel(exe, "branca", "divisa-esq", DesktopRect { x: div - 300, y: y0, w: 300, h: 300 })?;
    let _p = painel(exe, "preta", "divisa-dir", DesktopRect { x: div, y: y0, w: 300, h: 300 })?;
    std::thread::sleep(Duration::from_millis(400));
    let area = DesktopRect { x: div - 200, y: y0 + 50, w: 400, h: 200 };
    let canvas = (400u32, 200u32);
    let desk: Vec<DesktopRect> = mons.iter().map(|m| DesktopRect { x: m.1, y: m.2, w: m.3 as i32, h: m.4 as i32 }).collect();
    let place = area_layout(area, canvas, &desk);
    let usados = place.iter().filter(|p| p.is_some()).count();
    check(usados == 2, format!("a área devia cair em 2 monitores, caiu em {usados}"))?;
    let hide = Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
    let sources = mons
        .iter()
        .zip(place)
        .enumerate()
        .map(|(i, (m, p))| {
            let (crop, dest) = match p {
                Some((c, d)) => (Some(c), Some(d)),
                None => (None, Some(hide)),
            };
            SourceSpec { name: format!("Area Monitor {}", i + 1), target: VideoTarget::Monitor(m.0), crop, dest }
        })
        .collect();
    let cfg = SessionConfig {
        width: canvas.0,
        height: canvas.1,
        fps: 30,
        sources,
        encoder: VideoEncoderConfig { codec: codec.into(), width: canvas.0, height: canvas.1, fps: 30, crf: 23, keyframe_sec: 1 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    record("area-entre-monitores", 1.5, cfg)?;
    let path = out_dir().join("area-entre-monitores.mkv").to_string_lossy().into_owned();
    // Longe da costura (x = 200): 2 pontos de cada lado.
    let l = luma_at(&path, 1.0, &[(60, 100), (160, 100), (240, 100), (340, 100)])?;
    check(l[0] > 200 && l[1] > 200, format!("metade esquerda (monitor {}) não saiu branca: {l:?}", m0.1))?;
    check(l[2] < 40 && l[3] < 40, format!("metade direita (monitor {}) não saiu preta: {l:?}", m1.1))?;
    Ok(format!("divisa em x={div}: área 400x200 → esquerda {:?} / direita {:?}", &l[..2], &l[2..]))
}

/// `--cor preta|branca`: janela de uma cor só (preta à esquerda, branca à direita).
/// Com `--pos X Y W H` (pixels FÍSICOS da área de trabalho virtual) a janela
/// é um retângulo sem borda exatamente ali, e `--tag T` entra no título — é
/// assim que os testes de vários monitores põem uma cor em cada tela.
fn run_color_window(cor: &str, pos: Option<(i32, i32, i32, i32)>, tag: &str) -> i32 {
    use windows::core::{w, HSTRING};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HGDIOBJ, PAINTSTRUCT};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    static WHITE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let c = if WHITE.load(std::sync::atomic::Ordering::SeqCst) { 0x00FF_FFFF } else { 0 };
                    let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF(c));
                    FillRect(dc, &ps.rcPaint, br);
                    let _ = DeleteObject(HGDIOBJ(br.0));
                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_USER => {
                    let v = WHITE.load(std::sync::atomic::Ordering::SeqCst);
                    WHITE.store(!v, std::sync::atomic::Ordering::SeqCst);
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, false);
                    let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }
    let branca = cor == "branca";
    WHITE.store(branca, std::sync::atomic::Ordering::SeqCst);
    unsafe {
        if pos.is_some() {
            // Coordenadas físicas: sem isto o Windows as reescalaria por monitor.
            let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let hinst = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst, lpszClassName: w!("NoOBSCapturaCor"), ..Default::default() };
        RegisterClassW(&wc);
        let x = if branca { 760 } else { 60 };
        let (style, (x, y, ww, hh)) = match pos {
            Some(p) => (WS_POPUP | WS_VISIBLE, p),
            None => (WS_OVERLAPPEDWINDOW | WS_VISIBLE, (x, 80, 600, 450)),
        };
        let title = if tag.is_empty() { format!("NoOBS captura cor {cor}") } else { format!("NoOBS captura cor {cor} {tag}") };
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("NoOBSCapturaCor"),
            &HSTRING::from(title),
            style,
            x,
            y,
            ww,
            hh,
            None,
            None,
            Some(hinst),
            None,
        ) else {
            return 1;
        };
        SetTimer(Some(hwnd), 2, 30_000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.wParam.0 == 2 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

/// Sessão de 640x360 sobre o 1º monitor, com o som do sistema.
fn cfg_fase4(codec: &str, replay: Option<(u32, u32)>, output_path: String) -> Result<SessionConfig, String> {
    let (h, _, _, _, _) = *monitors().first().ok_or("sem monitor")?;
    Ok(SessionConfig {
        width: 640,
        height: 360,
        fps: 30,
        sources: vec![SourceSpec { name: "Monitor 1".into(), target: VideoTarget::Monitor(h), crop: None, dest: None }],
        encoder: VideoEncoderConfig { codec: codec.into(), width: 640, height: 360, fps: 30, crf: 23, keyframe_sec: 1 },
        audio: Some(AudioConfig {
            inputs: vec![AudioInputSpec { name: "Sistema".into(), device: AudioDevice::Output(String::new()), tracks: vec![0] }],
            track_titles: vec!["Mistura".into()],
            kbps: 128,
        }),
        replay,
        output_path,
    })
}

fn probe_ok(path: &str) -> Result<probe::ProbeReport, String> {
    let r = probe::probe(Path::new(path)).ok_or_else(|| format!("probe falhou em {path}"))?;
    check(r.video_stream().is_some() && r.audio_track_count() == 1, format!("{path}: faltou vídeo ou áudio"))?;
    Ok(r)
}

fn out(name: &str) -> String {
    out_dir().join(name).to_string_lossy().into_owned()
}

fn sleep_s(s: f64) {
    std::thread::sleep(Duration::from_secs_f64(s));
}

/// A sessão sobe sem gravar; a gravação começa e termina no meio, duas vezes.
fn s_gravar_depois(codec: &str) -> Result<String, String> {
    let s = Session::start(cfg_fase4(codec, None, String::new())?)?;
    sleep_s(1.0);
    s.start_recording(&out("depois-1.mkv"), false)?;
    check(s.start_recording(&out("x.mkv"), false).is_err(), "aceitou duas gravações ao mesmo tempo")?;
    sleep_s(2.0);
    let a = s.stop_recording()?;
    sleep_s(0.5);
    s.start_recording(&out("depois-2.mkv"), false)?;
    sleep_s(1.0);
    let b = s.stop_recording()?;
    check(s.stop_recording().is_err(), "parou uma gravação que não existia")?;
    if let Some(e) = s.stop().error {
        return Err(e);
    }
    let (ra, rb) = (probe_ok(&a.path)?, probe_ok(&b.path)?);
    near(ra.duration, 2.0, 0.3, "1ª gravação")?;
    near(rb.duration, 1.0, 0.3, "2ª gravação")?;
    Ok(format!("{:.2}s e {:.2}s, cada uma começando em 0 num keyframe forçado", ra.duration, rb.duration))
}

/// 1,5 s gravando, 2 s pausado, 1,5 s gravando: o arquivo tem ~3 s.
fn s_pausa(codec: &str) -> Result<String, String> {
    let path = out("pausa.mkv");
    let s = Session::start(cfg_fase4(codec, None, path.clone())?)?;
    sleep_s(1.5);
    s.set_paused(true);
    sleep_s(2.0);
    s.set_paused(false);
    sleep_s(1.5);
    let end = s.stop();
    if let Some(e) = end.error {
        return Err(e);
    }
    let r = probe_ok(&path)?;
    near(r.duration, 3.0, 0.35, "duração sem a pausa")?;
    Ok(format!("{:.2}s de arquivo em ~5 s de relógio", r.duration))
}

/// Buffer: dois trechos salvos em sequência somam o tempo que passou — sem
/// repetir (sobreposição) nem perder (buraco) nada entre eles.
fn s_buffer_trechos(codec: &str) -> Result<String, String> {
    let s = Session::start(cfg_fase4(codec, Some((30, 512)), String::new())?)?;
    sleep_s(3.0);
    let r1 = s.save_replay(&out("trecho-1.mkv"));
    sleep_s(2.0);
    let r2 = s.save_replay(&out("trecho-2.mkv"));
    let a = r1.recv_timeout(Duration::from_secs(10)).map_err(|_| "o 1º trecho não fechou")??;
    let b = r2.recv_timeout(Duration::from_secs(10)).map_err(|_| "o 2º trecho não fechou")??;
    if let Some(e) = s.stop().error {
        return Err(e);
    }
    let (pa, pb) = (probe_ok(&a.path)?, probe_ok(&b.path)?);
    // O 1º só começa no 1º keyframe da sessão (o quadro 0): ~3 s. O 2º, ~2 s.
    near(pa.duration, 3.0, 0.3, "1º trecho")?;
    near(pb.duration, 2.0, 0.2, "2º trecho")?;
    near(pa.duration + pb.duration, 5.0, 0.35, "soma dos trechos")?;
    Ok(format!("{:.2}s + {:.2}s = {:.2}s em ~5 s (sem buraco nem repetição)", pa.duration, pb.duration, pa.duration + pb.duration))
}

/// O buffer guarda só o limite: 2 s pedidos, 4 s passados.
fn s_buffer_limite(codec: &str) -> Result<String, String> {
    let s = Session::start(cfg_fase4(codec, Some((2, 512)), String::new())?)?;
    sleep_s(4.0);
    let held = s.router.replay_held_ms.load(std::sync::atomic::Ordering::Relaxed);
    let r = s.save_replay(&out("limite.mkv")).recv_timeout(Duration::from_secs(10)).map_err(|_| "o trecho não fechou")??;
    if let Some(e) = s.stop().error {
        return Err(e);
    }
    let p = probe_ok(&r.path)?;
    // O corte é por GOP inteiro (keyframe a cada 1 s): entre 2 e 3 s.
    check((1.9..=3.1).contains(&p.duration), format!("guardou {:.2}s com limite de 2 s", p.duration))?;
    Ok(format!("limite 2 s → {:.2}s salvos (fila dizia {held} ms)", p.duration))
}

/// Gravar com o buffer ligado começa pelo que ele guardou: UM arquivo, sem emenda.
fn s_buffer_continua(codec: &str) -> Result<String, String> {
    let s = Session::start(cfg_fase4(codec, Some((30, 512)), String::new())?)?;
    sleep_s(3.0);
    s.start_recording(&out("continua.mkv"), true)?;
    sleep_s(2.0);
    let rec = s.stop_recording()?;
    sleep_s(1.0);
    // O buffer seguiu guardando desde o começo da gravação.
    let clip = s.save_replay(&out("depois-da-gravacao.mkv")).recv_timeout(Duration::from_secs(10)).map_err(|_| "o trecho não fechou")??;
    if let Some(e) = s.stop().error {
        return Err(e);
    }
    let pr = probe_ok(&rec.path)?;
    near(pr.duration, 5.0, 0.35, "gravação = buffer (3 s) + ao vivo (2 s)")?;
    let pc = probe_ok(&clip.path)?;
    check(pc.duration > 1.5 && pc.duration < 3.5, format!("buffer depois da gravação: {:.2}s", pc.duration))?;
    Ok(format!("gravação {:.2}s (3 s do buffer + 2 s ao vivo, um arquivo só); buffer seguiu: {:.2}s", pr.duration, pc.duration))
}

/// Latência da captura WGC sozinha (sem encoder, roteador nem arquivo): o
/// helper `--cor` muda de preto pra branco quando recebe WM_USER, e a fonte é
/// lida de volta até enxergar o branco. Separa "o Windows entrega a imagem
/// atrasada" de "o nosso caminho atrasa".
fn s_latencia_wgc(exe: &str) -> Result<String, String> {
    use noobs::capture::video::{VideoSource, WgcSource, WgcTarget};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};
    let mut child = std::process::Command::new(exe).args(["--cor", "vira"]).spawn().map_err(|e| format!("helper: {e}"))?;
    let r = (|| {
        let h = find_window("NoOBS captura cor vira").ok_or("janela não apareceu")?;
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let dev = Device::create()?;
        let mut src = WgcSource::new(&dev, WgcTarget::Window { hwnd: h, client_area: true }, "lat")?;
        let mean = |dev: &Device, src: &WgcSource| -> Option<f64> {
            let (view, w, hh) = src.image()?;
            let res = unsafe { view.GetResource() }.ok()?;
            let tex: ID3D11Texture2D = windows::core::Interface::cast(&res).ok()?;
            let desc = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: hh,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..Default::default()
            };
            let mut st = None;
            unsafe { dev.device.CreateTexture2D(&desc, None, Some(&mut st)) }.ok()?;
            let st = st?;
            unsafe {
                dev.ctx.CopyResource(&st, &tex);
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                dev.ctx.Map(&st, 0, D3D11_MAP_READ, 0, Some(&mut m)).ok()?;
                let p = m.pData as *const u8;
                let v = *p.add((hh as usize / 2) * m.RowPitch as usize + (w as usize / 2) * 4) as f64;
                dev.ctx.Unmap(&st, 0);
                Some(v)
            }
        };
        // Espera ver o preto primeiro.
        let t0 = std::time::Instant::now();
        while t0.elapsed() < Duration::from_secs(3) {
            src.poll(&dev);
            if mean(&dev, &src).is_some_and(|v| v < 20.0) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let mut medidas = Vec::new();
        for _ in 0..5 {
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), WM_USER, WPARAM(0), LPARAM(0));
            }
            let t = std::time::Instant::now();
            let mut seen = None;
            while t.elapsed() < Duration::from_secs(2) {
                src.poll(&dev);
                let want_white = medidas.len() % 2 == 0;
                if mean(&dev, &src).is_some_and(|v| if want_white { v > 200.0 } else { v < 20.0 }) {
                    seen = Some(t.elapsed().as_secs_f64() * 1000.0);
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            medidas.push(seen.ok_or("a mudança de cor nunca chegou na captura")?);
            std::thread::sleep(Duration::from_millis(300));
        }
        let pior = medidas.iter().cloned().fold(0.0, f64::max);
        let txt: Vec<String> = medidas.iter().map(|m| format!("{m:.0}")).collect();
        // Tela apagada/monitor desligado: o compositor do Windows passa a entregar
        // a imagem com ~250 ms de atraso (medido). Não é defeito nosso — o
        // cenário diz isso e a sincronia é pulada.
        if pior >= 100.0 {
            return Err(format!("PULADO: o Windows está entregando a imagem com {} ms de atraso (tela apagada ou monitor desligado?)", txt.join("/")));
        }
        Ok(format!("mudança vista em {} ms", txt.join("/")))
    })();
    let _ = child.kill();
    let _ = child.wait();
    r
}

/// Detecção sem libobs: placa + abertura de teste de cada encoder.
fn s_caps() -> Result<String, String> {
    let dev = Device::create()?;
    let c = noobs::capture::caps::detect(&dev);
    check(c.h264_sw, "x264 não abriu")?;
    let auto = noobs::capture::caps::resolve("auto", &c);
    Ok(format!("{c:?}; auto = {auto}"))
}

/// Grava `secs` da janela de ruído com o encoder `codec` e qualidade `level`.
fn record_noise(name: &str, codec: &str, level: i32, secs: f64, hwnd: isize) -> Result<(probe::ProbeReport, u64, u64), String> {
    let (w, h) = (1280u32, 720u32);
    let cfg = SessionConfig {
        width: w,
        height: h,
        fps: 30,
        sources: vec![SourceSpec { name: "ruido".into(), target: VideoTarget::Window { hwnd, client_area: true }, crop: None, dest: Some(Rect { x: 0.0, y: 0.0, w: w as f32, h: h as f32 }) }],
        encoder: VideoEncoderConfig { codec: codec.into(), width: w, height: h, fps: 30, crf: noobs::encoder::quality_level_to_crf(level), keyframe_sec: 2 },
        audio: None,
        replay: None,
        output_path: String::new(),
    };
    let (r, frames, lagged, ms) = record(name, secs, cfg)?;
    if std::env::var_os("NOOBS_KEEP").is_some() {
        eprintln!("{name}: {frames} quadros, {lagged} atrasados, {ms:.1} ms/quadro");
    }
    Ok((r, frames, lagged))
}

fn with_noise<T>(exe: &str, f: impl FnOnce(isize) -> Result<T, String>) -> Result<T, String> {
    let mut child = std::process::Command::new(exe).arg("--ruido").spawn().map_err(|e| format!("helper: {e}"))?;
    let r = find_window("NoOBS captura ruido").ok_or_else(|| "a janela de ruído não apareceu".to_string()).and_then(f);
    let _ = child.kill();
    let _ = child.wait();
    r
}

/// Cada escolha de codec do app grava no codec certo, sem atrasar quadros.
fn s_codec_pref(exe: &str, pref: &str) -> Result<String, String> {
    let dev = Device::create()?;
    let caps = noobs::capture::caps::detect(&dev);
    drop(dev);
    let tem = match pref {
        "h264-hw" => caps.h264_hw,
        "hevc-hw" => caps.hevc_hw,
        "av1-hw" => caps.av1_hw,
        "h264-sw" => caps.h264_sw,
        "av1-sw" => caps.av1_sw,
        _ => true,
    };
    if !tem {
        return Err("PULADO: esta máquina não tem".into());
    }
    let codec = noobs::capture::caps::resolve(pref, &caps);
    let familia = if codec.starts_with("av1") || codec == "libsvtav1" { "av1" } else if codec.starts_with("hevc") { "hevc" } else { "h264" };
    with_noise(exe, |h| {
        let (r, frames, lagged) = record_noise(&format!("codec-{pref}"), &codec, 5, 2.0, h)?;
        let v = r.video_stream().ok_or("sem vídeo")?;
        check(v.codec == familia, format!("{pref} → {codec}: arquivo em {}", v.codec))?;
        check(lagged * 10 <= frames.max(1), format!("{lagged} de {frames} quadros atrasados"))?;
        near(v.frame_rate, 30.0, 1.0, "fps")?;
        Ok(format!("{pref} → {codec}: {} 1280x720, {frames} quadros ({lagged} atrasados), {} KB", v.codec, r.size / 1024))
    })
}

/// O nível 10 gera arquivo bem maior que o 0 (com conteúdo que muda).
fn s_qualidade(exe: &str, codec: &str) -> Result<String, String> {
    let dev = Device::create()?;
    let caps = noobs::capture::caps::detect(&dev);
    drop(dev);
    let codec = if codec == "auto" { noobs::capture::caps::resolve("auto", &caps) } else { codec.to_owned() };
    with_noise(exe, |h| {
        let mut sizes = Vec::new();
        for level in [0, 5, 10] {
            let (r, _, _) = record_noise(&format!("qualidade-{codec}-{level}"), &codec, level, 2.0, h)?;
            sizes.push(r.size);
        }
        check(sizes[0] < sizes[1] && sizes[1] < sizes[2], format!("tamanhos não crescem com o nível: {:?} KB", sizes.iter().map(|s| s / 1024).collect::<Vec<_>>()))?;
        Ok(format!("{codec}: nível 0 = {} KB, 5 = {} KB, 10 = {} KB", sizes[0] / 1024, sizes[1] / 1024, sizes[2] / 1024))
    })
}

/// `--ruido`: janela que pinta ruído a cada ~33 ms (conteúdo difícil de comprimir).
fn run_noise_window() -> i32 {
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_TIMER => {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    LRESULT(0)
                }
                WM_PAINT => {
                    static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x9E37_79B9_7F4A_7C15);
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let (bw, bh) = (320i32, 180i32);
                    let mut s = SEED.load(std::sync::atomic::Ordering::Relaxed);
                    let px: Vec<u32> = (0..bw * bh)
                        .map(|_| {
                            s ^= s << 13;
                            s ^= s >> 7;
                            s ^= s << 17;
                            s as u32
                        })
                        .collect();
                    SEED.store(s, std::sync::atomic::Ordering::Relaxed);
                    let bi = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: bw, biHeight: -bh, biPlanes: 1, biBitCount: 32, ..Default::default() },
                        ..Default::default()
                    };
                    let mut rc = windows::Win32::Foundation::RECT::default();
                    let _ = GetClientRect(hwnd, &mut rc);
                    StretchDIBits(dc, 0, 0, rc.right, rc.bottom, 0, 0, bw, bh, Some(px.as_ptr() as *const _), &bi, DIB_RGB_COLORS, SRCCOPY);
                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst, lpszClassName: w!("NoOBSCapturaRuido"), ..Default::default() };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(WS_EX_TOPMOST, w!("NoOBSCapturaRuido"), w!("NoOBS captura ruido"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 120, 120, 800, 480, None, None, Some(hinst), None) else {
            return 1;
        };
        SetTimer(Some(hwnd), 1, 33, None);
        SetTimer(Some(hwnd), 2, 120_000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.wParam.0 == 2 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

/// A janela pisca branco e bipa no mesmo instante: no arquivo, o 1º quadro
/// claro e o 1º som têm que cair juntos.
fn s_audio_sincronia(exe: &str, codec: &str) -> Result<String, String> {
    // A medida só vale com a captura de imagem rápida (ver `latencia-wgc`).
    if let Err(e) = s_latencia_wgc(exe) {
        return Err(if e.starts_with("PULADO:") { e } else { format!("PULADO: latência da captura não medida ({e})") });
    }
    let mut child = std::process::Command::new(exe).arg("--pisca").spawn().map_err(|e| format!("helper: {e}"))?;
    let r = (|| {
        let mut found = None;
        for _ in 0..50 {
            found = noobs::win_windows::enumerate_capture_windows().into_iter().find(|w| w.title == FLASH_TITLE);
            if found.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let w = found.ok_or("a janela do helper não apareceu")?;
        let cfg = SessionConfig {
            width: 640,
            height: 480,
            fps: 60,
            sources: vec![SourceSpec { name: w.title.clone(), target: VideoTarget::Window { hwnd: w.handle, client_area: true }, crop: None, dest: None }],
            encoder: VideoEncoderConfig { codec: codec.into(), width: 640, height: 480, fps: 60, crf: 23, keyframe_sec: 1 },
            audio: Some(AudioConfig {
                inputs: vec![AudioInputSpec { name: "Sistema".into(), device: AudioDevice::Output(String::new()), tracks: vec![0] }],
                track_titles: vec!["Mistura".into()],
                kbps: 192,
            }),
            replay: None,
            output_path: String::new(),
        };
        // O helper pisca 2 s depois de abrir; a gravação cobre antes e depois.
        let (r, _, _, _) = record("sincronia", 4.0, cfg)?;
        let path = out_dir().join("sincronia.mkv").to_string_lossy().into_owned();
        if std::env::var_os("NOOBS_KEEP").is_some() {
            eprintln!("pacotes: {:?}", first_packets(&path, 4));
        }
        let video_t = first_bright_frame(&path, 128.0)?;
        let buckets = (r.duration * 1000.0) as usize; // 1 ms
        let peaks = noobs::ffmpeg_ops::compute_audio_peaks(&path, buckets, None).ok_or("picos do áudio")?;
        // O loopback vem DEPOIS do volume do sistema (com o volume baixo o bipe
        // de 0,8 chega a 0,02): o início é relativo ao pico, não um limiar fixo.
        let max = peaks.iter().cloned().fold(0f32, f32::max);
        check(max > 0.002, format!("nenhum som no arquivo (pico {max:.4})"))?;
        let idx = peaks.iter().position(|&p| p > max * 0.5).ok_or("sem início do som")?;
        let audio_t = idx as f64 / buckets as f64 * r.duration;
        let diff_ms = (audio_t - video_t) * 1000.0;
        // Tolerância: o caminho de tela (DWM, 1-2 quadros) e o de áudio
        // (buffer de reprodução) do PRÓPRIO helper já somam dezenas de ms.
        check(
            diff_ms.abs() <= 80.0,
            format!("áudio {:.0} ms {} do vídeo (som em {audio_t:.3}s, quadro claro em {video_t:.3}s)", diff_ms.abs(), if diff_ms > 0.0 { "depois" } else { "antes" }),
        )?;
        Ok(format!("som {diff_ms:+.0} ms em relação ao quadro claro (quadro {video_t:.3}s, som {audio_t:.3}s)"))
    })();
    let _ = child.kill();
    let _ = child.wait();
    r
}

/// `--pisca`: janela preta que, 2 s depois de abrir, fica branca e toca um
/// bipe de 1 kHz no dispositivo de saída padrão no mesmo instante.
fn run_flash_window() -> i32 {
    use windows::core::{w, HSTRING};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, UpdateWindow, HGDIOBJ, PAINTSTRUCT};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    static WHITE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let c = if WHITE.load(std::sync::atomic::Ordering::SeqCst) { 0x00FF_FFFF } else { 0 };
                    let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF(c));
                    FillRect(dc, &ps.rcPaint, br);
                    let _ = DeleteObject(HGDIOBJ(br.0));
                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED).ok();
        // Saída pronta ANTES do flash (abrir o dispositivo leva dezenas de ms).
        let en: IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(e) => e,
            Err(_) => return 2,
        };
        let Ok(dev) = en.GetDefaultAudioEndpoint(eRender, eConsole) else { return 2 };
        let Ok(client) = dev.Activate::<IAudioClient>(CLSCTX_ALL, None) else { return 2 };
        let wfx = WAVEFORMATEX { wFormatTag: 3, nChannels: 2, nSamplesPerSec: 48000, nAvgBytesPerSec: 384000, nBlockAlign: 8, wBitsPerSample: 32, cbSize: 0 };
        if client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0x8000_0000 | 0x0800_0000, 3_000_000, 0, &wfx, None).is_err() {
            return 2;
        }
        let Ok(render) = client.GetService::<IAudioRenderClient>() else { return 2 };
        let buf_frames = client.GetBufferSize().unwrap_or(0);

        let hinst = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst, lpszClassName: w!("NoOBSCapturaPisca"), ..Default::default() };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("NoOBSCapturaPisca"),
            &HSTRING::from(FLASH_TITLE),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            660,
            520,
            None,
            None,
            Some(hinst),
            None,
        ) else {
            return 1;
        };
        // Uma thread conta os 2 s e avisa a janela UMA vez (WM_APP).
        let h = hwnd.0 as isize;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2000));
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP, WPARAM(0), LPARAM(0));
        });
        SetTimer(Some(hwnd), 2, 30_000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.wParam.0 == 2 {
                break;
            }
            if msg.message == WM_APP {
                // 300 ms de 1 kHz no buffer, tocado quando a janela fica branca.
                let n = (48000 * 3 / 10).min(buf_frames);
                if let Ok(p) = render.GetBuffer(n) {
                    let s = std::slice::from_raw_parts_mut(p as *mut f32, n as usize * 2);
                    for i in 0..n as usize {
                        let v = (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 48000.0).sin() * 0.8;
                        s[i * 2] = v;
                        s[i * 2 + 1] = v;
                    }
                    let _ = render.ReleaseBuffer(n, 0);
                }
                WHITE.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = InvalidateRect(Some(hwnd), None, false);
                let _ = UpdateWindow(hwnd);
                let st = client.Start();
                if std::env::var_os("NOOBS_PISCA_DEBUG").is_some() {
                    for _ in 0..5 {
                        eprintln!("pisca: start={:?} padding={:?}", st.as_ref().err(), client.GetCurrentPadding());
                        std::thread::sleep(std::time::Duration::from_millis(80));
                    }
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Entrada
// ---------------------------------------------------------------------------

fn run_helper_window() -> i32 {
    use windows::core::{w, HSTRING};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, HGDIOBJ, PAINTSTRUCT};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_TIMER => {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    LRESULT(0)
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let t = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() / 100) as u32;
                    let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF((t * 37 % 255) | ((t * 91 % 255) << 8) | (0x80 << 16)));
                    FillRect(dc, &ps.rcPaint, br);
                    let _ = DeleteObject(HGDIOBJ(br.0));
                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst, lpszClassName: w!("NoOBSCapturaJanela"), ..Default::default() };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), w!("NoOBSCapturaJanela"), &HSTRING::from(HELPER_TITLE), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 80, 80, 660, 520, None, None, Some(hinst), None) else {
            return 1;
        };
        SetTimer(Some(hwnd), 1, 100, None);
        SetTimer(Some(hwnd), 2, 60_000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.wParam.0 == 2 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--janela") {
        std::process::exit(run_helper_window());
    }
    if let Some(i) = args.iter().position(|a| a == "--cor") {
        let pos = args.iter().position(|a| a == "--pos").and_then(|p| {
            let n: Vec<i32> = args.iter().skip(p + 1).take(4).filter_map(|v| v.parse().ok()).collect();
            (n.len() == 4).then(|| (n[0], n[1], n[2], n[3]))
        });
        let tag = args.iter().position(|a| a == "--tag").and_then(|p| args.get(p + 1)).cloned().unwrap_or_default();
        std::process::exit(run_color_window(args.get(i + 1).map(String::as_str).unwrap_or("preta"), pos, &tag));
    }
    if args.iter().any(|a| a == "--ruido") {
        std::process::exit(run_noise_window());
    }
    if args.iter().any(|a| a == "--pisca") {
        std::process::exit(run_flash_window());
    }
    let filtro = args.iter().find(|a| !a.starts_with('-')).cloned().unwrap_or_default();
    let quer = |n: &str| filtro.is_empty() || filtro.split(',').any(|f| n.contains(f.trim()));

    // DLLs do FFmpeg do runtime de desenvolvimento; sem copiar este executável.
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exe");
    let w: Vec<u16> = runtime.to_string_lossy().encode_utf16().chain([0]).collect();
    unsafe {
        let _ = windows::Win32::System::LibraryLoader::SetDllDirectoryW(windows::core::PCWSTR(w.as_ptr()));
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // Tela acesa enquanto o teste roda: com o monitor apagado por inatividade
    // o compositor do Windows atrasa a imagem (~300 ms medidos) e as medidas de
    // sincronia e de cor deixam de valer. Pedido temporário deste processo.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }
    // ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
    unsafe { SetThreadExecutionState(0x8000_0000 | 0x1 | 0x2) };
    std::thread::sleep(Duration::from_millis(1500));
    if !noobs::ffi::ffmpeg::ffmpeg_lib_available() {
        println!("FFmpeg indisponível em {} — pulado.", runtime.display());
        return;
    }
    let root = out_dir();
    let _ = std::fs::create_dir_all(&root);
    std::env::set_var("LOCALAPPDATA", &root);
    noobs::log::init_log();
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();

    let mut rep = Report { ok: 0, fail: 0, skip: 0 };
    if quer("cor") {
        rep.take("cor", s_cor());
    }
    for c in ["h264_amf", "hevc_amf", "av1_amf", "h264_nvenc", "hevc_nvenc", "av1_nvenc", "libx264"] {
        let n = format!("encoder-{c}");
        if quer(&n) {
            rep.take(&n, s_encoder(c));
        }
    }
    let codec = match Device::create() {
        Ok(d) => best_h264(&d),
        Err(_) => "libx264".into(),
    };
    if quer("monitor") {
        rep.take("monitor", s_monitor(&codec));
    }
    if quer("dois-monitores") {
        rep.take("dois-monitores", s_dois_monitores(&codec));
    }
    if quer("area") {
        rep.take("area", s_area(&codec));
    }
    if quer("software") {
        rep.take("software", s_software());
    }
    if quer("janela") {
        rep.take("janela", s_janela(&exe, &codec));
    }
    if quer("janela-fecha") {
        rep.take("janela-fecha", s_janela_fecha(&exe, &codec));
    }
    if quer("audio-faixas") {
        rep.take("audio-faixas", s_audio_faixas(&codec));
    }
    if quer("audio-loopback") {
        rep.take("audio-loopback", s_audio_loopback(&exe));
    }
    if quer("audio-app") {
        rep.take("audio-app", s_audio_app(&exe));
    }
    if quer("webcam") {
        rep.take("webcam", s_webcam(&codec));
    }
    if quer("monitores-conteudo") {
        rep.take("monitores-conteudo", s_monitores_conteudo(&exe, &codec));
    }
    if quer("area-entre-monitores") {
        rep.take("area-entre-monitores", s_area_entre_monitores(&exe, &codec));
    }
    if quer("area-move") {
        rep.take("area-move", s_area_move(&exe, &codec));
    }
    for (nome, f) in [
        ("gravar-depois", s_gravar_depois as fn(&str) -> Result<String, String>),
        ("pausa", s_pausa),
        ("buffer-trechos", s_buffer_trechos),
        ("buffer-limite", s_buffer_limite),
        ("buffer-continua", s_buffer_continua),
    ] {
        if quer(nome) {
            rep.take(nome, f(&codec));
        }
    }
    if quer("caps") {
        rep.take("caps", s_caps());
    }
    for pref in ["auto", "h264-hw", "hevc-hw", "av1-hw", "h264-sw", "av1-sw"] {
        let n = format!("codec-{pref}");
        if quer(&n) {
            rep.take(&n, s_codec_pref(&exe, pref));
        }
    }
    for c in ["auto", "libx264"] {
        let n = format!("qualidade-{c}");
        if quer(&n) {
            rep.take(&n, s_qualidade(&exe, c));
        }
    }
    if quer("latencia-wgc") {
        rep.take("latencia-wgc", s_latencia_wgc(&exe));
    }
    if quer("audio-sincronia") {
        rep.take("audio-sincronia", s_audio_sincronia(&exe, &codec));
    }
    if quer("audio-sincronia-x264") {
        rep.take("audio-sincronia-x264", s_audio_sincronia(&exe, "libx264"));
    }
    if std::env::var_os("NOOBS_KEEP").is_none() {
        let _ = std::fs::remove_dir_all(root.join("NoOBS"));
    }
    for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        if e.path().extension().is_some_and(|x| x == "mkv") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    println!("FIM ok={} falhas={} pulados={}", rep.ok, rep.fail, rep.skip);
    if rep.fail > 0 {
        std::process::exit(1);
    }
}

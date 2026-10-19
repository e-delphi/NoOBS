//! Relógio do motor: QPC em nanossegundos (um só pro processo — vídeo e áudio
//! marcam o tempo pela mesma régua) e a grade de quadros do laço de vídeo.
//!
//! A grade é a do `video_sleep` do OBS (`obs-video.c`): o quadro N acontece em
//! `início + N × intervalo`. Se o laço atrasa, os tiques perdidos NÃO são
//! recuperados — o próximo quadro já sai com o número certo da grade (o pts), e
//! a contagem de atrasados sobe.

use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    CreateWaitableTimerExW, SetWaitableTimer, WaitForSingleObject, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};

fn qpc_freq() -> i64 {
    static FREQ: OnceLock<i64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0i64;
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1)
    })
}

/// Agora, em ns, pelo QPC.
pub fn now_ns() -> u64 {
    let mut c = 0i64;
    let _ = unsafe { QueryPerformanceCounter(&mut c) };
    qpc_to_ns(c)
}

/// Um valor de QPC (o WASAPI entrega o instante de cada pacote nessa unidade,
/// em 100 ns — ver `audio`) convertido pra ns.
pub fn qpc_to_ns(c: i64) -> u64 {
    let f = qpc_freq() as i128;
    ((c as i128) * 1_000_000_000 / f) as u64
}

/// Espera precisa: timer de alta resolução (Windows 10 1803+). Sem ele, o
/// `Sleep` do Windows anda de 15,6 em 15,6 ms e a 60 fps a grade viraria serra.
pub struct Sleeper {
    timer: HANDLE,
}

impl Sleeper {
    pub fn new() -> Self {
        let timer = unsafe { CreateWaitableTimerExW(None, None, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0) }.unwrap_or_default();
        Self { timer }
    }

    /// Dorme até `deadline_ns` (relógio do `now_ns`). Já passou = volta na hora.
    pub fn sleep_until(&self, deadline_ns: u64) {
        let now = now_ns();
        if deadline_ns <= now {
            return;
        }
        let wait_100ns = ((deadline_ns - now) / 100) as i64;
        if self.timer.is_invalid() {
            std::thread::sleep(std::time::Duration::from_nanos(deadline_ns - now));
            return;
        }
        // Negativo = relativo, em unidades de 100 ns.
        let due = -wait_100ns.max(1);
        unsafe {
            if SetWaitableTimer(self.timer, &due, 0, None, None, false).is_ok() {
                WaitForSingleObject(self.timer, INFINITE);
            } else {
                std::thread::sleep(std::time::Duration::from_nanos(deadline_ns - now));
            }
        }
    }
}

impl Default for Sleeper {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        if !self.timer.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.timer);
            }
        }
    }
}

/// Um tique da grade.
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    /// Número do quadro desde o início (= pts com time_base 1/fps).
    pub index: u64,
    /// Instante do tique no relógio do `now_ns`.
    pub time_ns: u64,
    /// Tiques pulados ANTES deste (o laço atrasou).
    pub skipped: u64,
}

/// Grade fixa de quadros: `início + N × intervalo`.
pub struct FrameGrid {
    start_ns: u64,
    interval_ns: u64,
    next_index: u64,
    sleeper: Sleeper,
    pub lagged: u64,
}

impl FrameGrid {
    pub fn new(fps: u32) -> Self {
        Self::starting_at(fps, now_ns())
    }

    /// Grade que começa num instante combinado (vídeo e áudio da mesma sessão
    /// partem do MESMO `start_ns` — é o que os mantém juntos).
    pub fn starting_at(fps: u32, start_ns: u64) -> Self {
        let interval_ns = 1_000_000_000 / fps.max(1) as u64;
        Self { start_ns, interval_ns, next_index: 0, sleeper: Sleeper::new(), lagged: 0 }
    }

    pub fn start_ns(&self) -> u64 {
        self.start_ns
    }

    pub fn interval_ns(&self) -> u64 {
        self.interval_ns
    }

    /// Espera o próximo tique. Atrasado: pula pro tique corrente da grade.
    pub fn wait(&mut self) -> Tick {
        let target = self.start_ns + self.next_index * self.interval_ns;
        self.sleeper.sleep_until(target);
        let now = now_ns();
        let current = (now.saturating_sub(self.start_ns)) / self.interval_ns;
        let index = current.max(self.next_index);
        let skipped = index - self.next_index;
        self.lagged += skipped;
        self.next_index = index + 1;
        Tick { index, time_ns: self.start_ns + index * self.interval_ns, skipped }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_anda_um_quadro_por_tique() {
        let mut g = FrameGrid::new(100);
        let a = g.wait();
        let b = g.wait();
        let c = g.wait();
        assert_eq!((a.index, b.index, c.index), (0, 1, 2));
        assert!(c.time_ns - a.time_ns == 2 * g.interval_ns());
    }

    #[test]
    fn atraso_pula_quadros_sem_recuperar() {
        let mut g = FrameGrid::new(100); // 10 ms
        g.wait();
        std::thread::sleep(std::time::Duration::from_millis(35));
        let t = g.wait();
        assert!(t.index >= 3, "devia pular pra grade corrente, veio {}", t.index);
        assert!(t.skipped >= 2);
        assert_eq!(g.lagged, t.skipped);
    }

    #[test]
    fn sleeper_e_preciso() {
        let s = Sleeper::new();
        let t0 = now_ns();
        s.sleep_until(t0 + 5_000_000);
        let dt = now_ns() - t0;
        assert!(dt >= 5_000_000 && dt < 9_000_000, "dormiu {dt} ns pra pedir 5 ms");
    }
}

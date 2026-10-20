//! Frente 1 do `docs/mapa-ui-rust-dlls.md`: as mensagens da página que não
//! chegam em DLL, mandadas pelo `dispatch` como a página mandaria.
//!
//! Cada teste abre uma sessão do [`crate::testkit`] (config limpo, pasta de
//! gravação própria, sem efeito fora do processo) e confere três coisas: o que
//! o Rust respondeu (`sent`), o que mudou no config/disco e o que teria saído
//! do processo (`effects`).
//!
//! Fora daqui: `ready` (sobe servidor do player, vigias e timers — é o
//! arranque inteiro, coberto pela frente 4) e as mensagens que chegam na
//! motor ou no FFmpeg (frentes 2–4).

mod area;
mod biblioteca;
mod buffer;
mod configuracoes;
mod geral;
mod janela;
mod midia;
mod pastas;
mod transcricao;

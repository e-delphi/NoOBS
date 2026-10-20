//! Motor de captura (docs/motor-proprio.md): substituiu a libobs.
//!
//! Camadas, cada uma atrás de um trait pequeno:
//! - `clock`: relógio QPC e a grade de quadros;
//! - `gpu`: dispositivo D3D11, compositor e conversor BGRA → NV12;
//! - `video`: fontes (`VideoSource`) — WGC de monitor e de janela;
//! - `webcam`: fonte de webcam (Media Foundation);
//! - `layout`: contas de posição (área cruzando monitores, encaixe);
//! - `audio`: fontes (`AudioSource`) — WASAPI, microfone e som do sistema;
//! - `mixer`: blocos de 1024 amostras, mistura + faixas isoladas;
//! - `encode`: `VideoEncoder`/`AudioEncoder` sobre o libavcodec;
//! - `sink`: saídas (`PacketSink`) — MKV;
//! - `output`: decisões puras do buffer (fila, corte) e da pausa;
//! - `router`: o gravador — gravação, buffer e trechos salvos;
//! - `pipeline`: a `Session`, que só liga as peças numa thread de vídeo.

pub mod audio;
pub mod caps;
pub mod clock;
pub mod encode;
pub mod gpu;
pub mod layout;
pub mod mixer;
pub mod output;
pub mod pipeline;
pub mod router;
pub mod sink;
pub mod video;
pub mod webcam;

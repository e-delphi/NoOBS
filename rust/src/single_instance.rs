//! Literais compartilhados entre o modo full e o hibernate (pegadinha #36).
//! Nunca duplicar estes nomes: mutex/mensagem diferentes = dois modos que
//! não enxergam um ao outro.

/// Mutex nomeado verificado no início dos dois modos.
pub const MUTEX_NAME: &str = "NoOBS.SingleInstance.TNoOBS";

/// Registrada via `RegisterWindowMessage`; mesmo UINT nos dois processos.
pub const SHOW_MSG_NAME: &str = "NoOBS.ShowInstance.TNoOBS";

/// Classe da janela do modo full (`OBSUI.CLASS_NAME`); o instalador a procura.
pub const CLASS_NAME: &str = "TNoOBS";

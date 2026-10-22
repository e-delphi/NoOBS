//! Literais da instância única (pegadinha #36): a 2ª instância acha a 1ª pelo
//! mutex e a acorda pela mensagem registrada. O instalador procura a classe.
//! Nunca duplicar estes nomes.

/// Mutex nomeado verificado no início.
pub const MUTEX_NAME: &str = "NoOBS.SingleInstance.TNoOBS";

/// Registrada via `RegisterWindowMessage`; mesmo UINT nas duas instâncias.
pub const SHOW_MSG_NAME: &str = "NoOBS.ShowInstance.TNoOBS";

/// Classe da janela principal (`OBSUI.CLASS_NAME`); o instalador a procura.
pub const CLASS_NAME: &str = "TNoOBS";

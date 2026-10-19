//! NoOBS — o exe. Tudo mora na biblioteca (`src/lib.rs`); aqui fica só o
//! subsistema GUI (sem console) e a chamada de entrada.

#![windows_subsystem = "windows"]

fn main() {
    noobs::run_app();
}

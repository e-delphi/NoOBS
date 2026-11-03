//! Passeio pelas telas, para comparar com as capturas da versão anterior (só com a
//! feature `dev`): `NOOBS_TOUR=pasta` junto com `IRON_SNAPSHOT=outra` faz o app abrir cada
//! tela (a principal, as 11 abas das Configurações, o player com os painéis e a exportação),
//! esperar ela assentar e copiar o quadro desenhado para `pasta/NN_nome.png`; no fim, fecha.
//! `NOOBS_TOUR_FILE` escolhe a gravação do player e da exportação.

use std::path::PathBuf;

use iron::core::Ui;

/// Um passo: o nome da captura e o que fazer antes dela.
type Step = (&'static str, fn(&mut Ui));

const STEPS: &[Step] = &[
    ("principal", |_| {}),
    ("config_geral", |ui| crate::app_settings::open(ui, None)),
    ("config_1", |ui| crate::app_settings::show_tab_at(ui, 1)),
    ("config_2", |ui| crate::app_settings::show_tab_at(ui, 2)),
    ("config_3", |ui| crate::app_settings::show_tab_at(ui, 3)),
    ("config_4", |ui| crate::app_settings::show_tab_at(ui, 4)),
    ("config_5", |ui| crate::app_settings::show_tab_at(ui, 5)),
    ("config_6", |ui| crate::app_settings::show_tab_at(ui, 6)),
    ("config_7", |ui| crate::app_settings::show_tab_at(ui, 7)),
    ("config_8", |ui| crate::app_settings::show_tab_at(ui, 8)),
    ("config_9", |ui| crate::app_settings::show_tab_at(ui, 9)),
    ("config_10", |ui| crate::app_settings::show_tab_at(ui, 10)),
    ("player", |ui| {
        crate::app_settings::close(ui);
        crate::app_player::open(ui, &file());
    }),
    ("player_pausado", |ui| crate::app_player::toggle_play(ui)),
    ("player_info", |ui| crate::app_player::toggle_info(ui)),
    ("player_transcricao", |ui| {
        crate::app_player::toggle_info(ui);
        crate::app_player::toggle_transcript(ui);
    }),
    ("exportacao", |ui| {
        crate::app_player::close(ui);
        crate::app_export::open_for(ui, &file());
    }),
];

fn file() -> String {
    std::env::var("NOOBS_TOUR_FILE").unwrap_or_default()
}

/// Liga o passeio se `NOOBS_TOUR` e `IRON_SNAPSHOT` estiverem definidos.
pub fn start(ui: &mut Ui) {
    let (Some(out), Some(shots)) = (std::env::var_os("NOOBS_TOUR"), std::env::var_os("IRON_SNAPSHOT")) else { return };
    let (out, shots) = (PathBuf::from(out), PathBuf::from(shots));
    let _ = std::fs::create_dir_all(&out);
    // Três segundos para a tela principal carregar (prévia das telas, biblioteca).
    ui.set_timeout(3000, move |ui| step(ui, 0, out, shots));
}

fn step(ui: &mut Ui, index: usize, out: PathBuf, shots: PathBuf) {
    let Some((name, action)) = STEPS.get(index) else {
        std::process::exit(0);
    };
    action(ui);
    ui.request_repaint();
    // O player espera o vídeo abrir; o resto, a tela assentar.
    let wait = if name.starts_with("player") || *name == "exportacao" { 4000 } else { 1200 };
    ui.set_timeout(wait, move |ui| {
        // O último quadro da janela principal (o nome do form).
        let newest = std::fs::read_dir(&shots)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        if let Some(src) = newest {
            let _ = std::fs::copy(src.path(), out.join(format!("{index:02}_{name}.png")));
        }
        step(ui, index + 1, out, shots);
    });
}

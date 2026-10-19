//! Biblioteca em pastas (`bridge/folders.rs`): navegar, criar, renomear,
//! excluir e mover. Tudo na pasta de gravação temporária da sessão.

use serde_json::json;

use crate::lang;
use crate::testkit::{self, begin, send};

fn listing() -> serde_json::Value {
    testkit::last_of("recordings_loaded").expect("recordings_loaded")
}

// ---- navegar ----------------------------------------------------------------

#[test]
fn navegar_pra_subpasta_e_voltar() {
    let s = begin();
    let aulas = s.dir("Aulas");
    s.dir("Aulas\\2026");
    s.file("Aulas\\a.mkv", b"x");

    send(json!({ "type": "open_folder", "id": aulas }));
    let l = listing();
    assert_eq!(l["atRoot"], false);
    assert_eq!(l["currentDir"], aulas.as_str());
    assert_eq!(l["parentDir"], s.rec.to_string_lossy().as_ref());
    let crumbs: Vec<_> = l["breadcrumb"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(crumbs.last().unwrap(), "Aulas");
    assert_eq!(l["recordings"].as_array().unwrap().len(), 1);
    assert_eq!(l["folders"].as_array().unwrap().len(), 1);

    // "" = raiz.
    send(json!({ "type": "open_folder", "id": "" }));
    assert_eq!(listing()["atRoot"], true);
}

#[test]
fn navegar_pra_fora_ou_pra_pasta_sumida() {
    let s = begin();
    // Fora da pasta de gravação: ignorado.
    send(json!({ "type": "open_folder", "id": "C:\\Windows" }));
    assert!(testkit::sent().is_empty());
    // Dentro, mas não existe: avisa e volta à raiz.
    send(json!({ "type": "open_folder", "id": s.path("Sumiu") }));
    assert_eq!(testkit::errors(), vec![lang::t("error.folderNotFound")]);
    assert_eq!(listing()["atRoot"], true);
}

// ---- criar ------------------------------------------------------------------

#[test]
fn criar_pasta_com_nome() {
    let s = begin();
    send(json!({ "type": "create_folder", "name": "  Jogos  " }));
    assert!(s.exists("Jogos"));
    assert_eq!(testkit::last_of("folder_created").unwrap()["id"], s.path("Jogos"));
    // A lista vem ANTES do aviso de criada (a UI procura o card nela).
    let tipos: Vec<_> = testkit::sent().iter().map(|v| v["type"].as_str().unwrap().to_owned()).collect();
    let i_lista = tipos.iter().position(|t| t == "recordings_loaded").unwrap();
    let i_criada = tipos.iter().position(|t| t == "folder_created").unwrap();
    assert!(i_lista < i_criada, "{tipos:?}");
}

#[test]
fn criar_pasta_sem_nome_usa_o_padrao_traduzido() {
    let s = begin();
    send(json!({ "type": "create_folder", "name": "" }));
    let padrao = lang::t("recordings.newFolderName");
    assert!(!padrao.starts_with('['));
    assert!(s.exists(&padrao));
    // De novo: ganha sufixo.
    send(json!({ "type": "create_folder" }));
    assert!(s.exists(&format!("{padrao} (2)")));
}

#[test]
fn criar_pasta_limpa_o_nome() {
    let s = begin();
    send(json!({ "type": "create_folder", "name": "a:b*c?" }));
    assert!(s.exists("a_b_c_"));
    // Ponto e espaço no FIM somem (o Windows os descartaria calado).
    send(json!({ "type": "create_folder", "name": "Final. . ." }));
    assert!(s.exists("Final"));
    // Nome que vira vazio: nada.
    testkit::clear_sent();
    send(json!({ "type": "create_folder", "name": "...   " }));
    assert!(testkit::sent_of("folder_created").is_empty());
}

#[test]
fn criar_pasta_dentro_da_subpasta_navegada() {
    let s = begin();
    let aulas = s.dir("Aulas");
    send(json!({ "type": "open_folder", "id": aulas }));
    send(json!({ "type": "create_folder", "name": "Semana 1" }));
    assert!(s.exists("Aulas\\Semana 1"));
}

#[test]
fn criar_pasta_com_nome_de_arquivo_existente_ganha_sufixo() {
    let s = begin();
    s.file("Jogos", b"arquivo sem extensao");
    send(json!({ "type": "create_folder", "name": "Jogos" }));
    assert!(s.rec.join("Jogos (2)").is_dir());
}

// ---- renomear ---------------------------------------------------------------

#[test]
fn renomear_pasta() {
    let s = begin();
    let p = s.dir("Velha");
    s.file("Velha\\v.mkv", b"x");
    send(json!({ "type": "rename_folder", "id": p, "newName": "Nova" }));
    assert!(s.exists("Nova\\v.mkv"));
    assert!(!s.exists("Velha"));
    assert!(testkit::last_of("recordings_loaded").is_some());
}

#[test]
fn renomear_pasta_so_maiuscula_nao_vira_sufixo() {
    let s = begin();
    let p = s.dir("aulas");
    send(json!({ "type": "rename_folder", "id": p, "newName": "Aulas" }));
    let nomes: Vec<String> = std::fs::read_dir(&s.rec).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(nomes, vec!["Aulas"]);
}

#[test]
fn renomear_pasta_pra_nome_existente_ganha_sufixo() {
    let s = begin();
    s.dir("A");
    let b = s.dir("B");
    send(json!({ "type": "rename_folder", "id": b, "newName": "A" }));
    assert!(s.exists("A (2)"));
    assert!(s.exists("A"));
}

#[test]
fn renomear_pasta_recusa() {
    let s = begin();
    let p = s.dir("P");
    // A própria raiz, fora da pasta, nome vazio/igual: nada muda.
    send(json!({ "type": "rename_folder", "id": s.rec.to_string_lossy(), "newName": "X" }));
    send(json!({ "type": "rename_folder", "id": "C:\\Windows", "newName": "X" }));
    send(json!({ "type": "rename_folder", "id": p, "newName": "" }));
    send(json!({ "type": "rename_folder", "id": p, "newName": "P" }));
    send(json!({ "type": "rename_folder", "id": p, "newName": "..." }));
    assert!(testkit::sent().is_empty(), "{:?}", testkit::sent());
    assert!(s.exists("P"));
    // Sumiu: avisa.
    send(json!({ "type": "rename_folder", "id": s.path("Sumiu"), "newName": "X" }));
    assert_eq!(testkit::errors(), vec![lang::t("error.folderNotFound")]);
}

// ---- excluir ----------------------------------------------------------------

#[test]
fn excluir_pasta_com_tudo_dentro() {
    let s = begin();
    let p = s.dir("Lixo");
    s.file("Lixo\\a.mkv", b"x");
    s.file("Lixo\\sub\\b.mp4", b"y");
    send(json!({ "type": "delete_folder", "id": p }));
    assert!(testkit::wait_for("recordings_loaded", 5000).is_some());
    assert!(!s.exists("Lixo"));
    assert_eq!(testkit::effects(), vec![format!("recycle:{p}")]);
}

#[test]
fn excluir_pasta_recusa_raiz_e_fora() {
    let s = begin();
    send(json!({ "type": "delete_folder", "id": s.rec.to_string_lossy() }));
    send(json!({ "type": "delete_folder", "id": format!("{}\\", s.rec.to_string_lossy()) }));
    send(json!({ "type": "delete_folder", "id": "C:\\Windows" }));
    send(json!({ "type": "delete_folder", "id": "" }));
    send(json!({ "type": "delete_folder", "id": s.path("NaoExiste") }));
    testkit::settle(300);
    assert!(s.rec.is_dir());
    assert!(testkit::effects().is_empty());
}

// ---- mover ------------------------------------------------------------------

#[test]
fn mover_gravacoes_pra_pasta() {
    let s = begin();
    let destino = s.dir("Destino");
    let a = s.file("a.mkv", b"a");
    let b = s.file("b.mp4", b"b");
    send(json!({ "type": "move_items", "ids": [a, b], "target": destino }));
    assert!(s.exists("Destino\\a.mkv"));
    assert!(s.exists("Destino\\b.mp4"));
    assert!(!s.exists("a.mkv"));
}

#[test]
fn mover_com_nome_repetido_ganha_sufixo() {
    let s = begin();
    s.file("Destino\\a.mkv", b"antigo");
    let a = s.file("a.mkv", b"novo");
    send(json!({ "type": "move_items", "ids": [a], "target": s.path("Destino") }));
    assert_eq!(std::fs::read(s.path("Destino\\a.mkv")).unwrap(), b"antigo");
    assert_eq!(std::fs::read(s.path("Destino\\a (2).mkv")).unwrap(), b"novo");
}

#[test]
fn mover_pasta_pra_dentro_de_outra() {
    let s = begin();
    let a = s.dir("A");
    s.file("A\\v.mkv", b"x");
    let b = s.dir("B");
    send(json!({ "type": "move_items", "ids": [a], "target": b }));
    assert!(s.exists("B\\A\\v.mkv"));
}

#[test]
fn mover_pasta_pra_dentro_dela_mesma_e_recusado() {
    let s = begin();
    let a = s.dir("A");
    let filha = s.dir("A\\Filha");
    send(json!({ "type": "move_items", "ids": [a.clone()], "target": filha }));
    send(json!({ "type": "move_items", "ids": [a.clone()], "target": a }));
    assert!(s.exists("A\\Filha"));
    assert_eq!(testkit::errors(), vec![lang::t("error.moveIntoItself"); 2]);
}

#[test]
fn mover_ignora_o_que_nao_e_gravacao() {
    let s = begin();
    let destino = s.dir("D");
    let txt = s.file("notas.txt", b"x");
    let raiz = s.rec.to_string_lossy().into_owned();
    let fora = "C:\\Windows\\notepad.exe";
    send(json!({ "type": "move_items", "ids": [txt, raiz, fora, "", 42], "target": destino }));
    assert!(s.exists("notas.txt"));
    assert!(testkit::sent().is_empty(), "nada movido = sem relistar: {:?}", testkit::sent());
}

#[test]
fn mover_pra_onde_ja_esta_nao_relista() {
    let s = begin();
    let a = s.file("a.mkv", b"x");
    send(json!({ "type": "move_items", "ids": [a], "target": "" }));
    assert!(s.exists("a.mkv"));
    assert!(testkit::sent().is_empty());
}

#[test]
fn mover_pra_destino_invalido() {
    let s = begin();
    let a = s.file("a.mkv", b"x");
    send(json!({ "type": "move_items", "ids": [a.clone()], "target": "C:\\Windows" }));
    assert!(testkit::sent().is_empty());
    send(json!({ "type": "move_items", "ids": [a], "target": s.path("NaoExiste") }));
    assert_eq!(testkit::errors(), vec![lang::t("error.folderNotFound")]);
    // Sem a lista de ids: nada.
    send(json!({ "type": "move_items", "target": s.path("NaoExiste") }));
    assert_eq!(testkit::errors().len(), 1);
}

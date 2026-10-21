//! Captura de área (`bridge/area.rs`): a seleção do "Capturar... → Área". A
//! seleção de verdade escurece os monitores; aqui ela fica "aberta" até
//! `area_select::test_finish` dizer o que o usuário arrastou. Os atalhos de
//! área (retângulo salvo, atraso) são de `atalhos.rs`.
//!
//! Gravar de fato a área é dos testes externos (`tests/`). Aqui só os caminhos
//! que param antes: cancelar, recusar.

use serde_json::json;

use crate::area_select;
use crate::testkit::{self, begin, send};

#[test]
fn selecionar_area_e_cancelar() {
    let _s = begin();
    send(json!({ "type": "select_capture_area" }));
    assert_eq!(testkit::effects(), vec!["area_select:areaSelect.hint"]);
    // Aberta: outro pedido não abre outra seleção.
    send(json!({ "type": "select_capture_area" }));
    assert_eq!(testkit::effects().len(), 1);
    area_select::test_finish(None);
    assert_eq!(testkit::sent_of("capture_area_canceled").len(), 1);
}

#[test]
fn cancelar_contagem_sem_contagem_nao_faz_nada() {
    let _s = begin();
    send(json!({ "type": "cancel_countdown" }));
    assert!(testkit::sent().is_empty());
}

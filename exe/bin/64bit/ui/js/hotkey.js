// =====================================================================
// Settings modal
// =====================================================================
// Teclas principais disponiveis no dropdown de atalho. Os nomes batem
// EXATAMENTE com o que OBSHotkey.ParseHotkey/NameToVk espera no Delphi.
// Mudancas aqui precisam de mudanca correspondente la. Modificadores
// (Ctrl/Shift/Alt/Win) sao checkboxes a parte, fora desta lista.
// labelKey (nao label): o nome do grupo e resolvido via T() no render do
// dropdown (em _buildHotkeyDropdown), pois esta const e avaliada antes
// do bundle de i18n chegar do backend.
const HOTKEY_KEY_GROUPS = [
  { labelKey: 'settings.hotkey.groups.function', keys: [
      'F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12']
  },
  { labelKey: 'settings.hotkey.groups.letters', keys: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('') },
  { labelKey: 'settings.hotkey.groups.numbers', keys: '0123456789'.split('') },
  { labelKey: 'settings.hotkey.groups.special', keys: [
      { k: 'Space', labelKey: 'settings.hotkey.keys.space' },
      'Enter','Tab','Esc','Insert','Delete','Home','End',
      { k: 'PageUp',   labelKey: 'settings.hotkey.keys.pageUp' },
      { k: 'PageDown', labelKey: 'settings.hotkey.keys.pageDown' },
      'Backspace','Pause/Break']
  },
  { labelKey: 'settings.hotkey.groups.arrows', keys: [
      { k: 'Left',  labelKey: 'settings.hotkey.keys.left' },
      { k: 'Right', labelKey: 'settings.hotkey.keys.right' },
      { k: 'Up',    labelKey: 'settings.hotkey.keys.up' },
      { k: 'Down',  labelKey: 'settings.hotkey.keys.down' }]
  },
  // Numpad: labels literais (iguais em todos os idiomas), com os simbolos
  // impressos nas teclas (+ - * / .). "NumpadMultiply" e obscuro; "Num *"
  // casa com o teclado. O valor (k) continua canonico pro backend.
  { labelKey: 'settings.hotkey.groups.numpad', keys: [
      { k:'Numpad0', label:'Num 0' }, { k:'Numpad1', label:'Num 1' },
      { k:'Numpad2', label:'Num 2' }, { k:'Numpad3', label:'Num 3' },
      { k:'Numpad4', label:'Num 4' }, { k:'Numpad5', label:'Num 5' },
      { k:'Numpad6', label:'Num 6' }, { k:'Numpad7', label:'Num 7' },
      { k:'Numpad8', label:'Num 8' }, { k:'Numpad9', label:'Num 9' },
      { k:'NumpadAdd',      label:'Num +' },
      { k:'NumpadSubtract', label:'Num -' },
      { k:'NumpadMultiply', label:'Num *' },
      { k:'NumpadDivide',   label:'Num /' },
      { k:'NumpadDecimal',  label:'Num .' }]
  },
  { labelKey: 'settings.hotkey.groups.chars', keys: ['-','=',',','.',';','/','`','[','\\',']',"'"] },
  // Teclas de midia (controle de reproducao em teclados multimidia). Aqui
  // cada item e um objeto { k, labelKey } em vez de string: o VALOR (k) e o
  // nome canonico que o backend entende ('MediaPlayPause' etc.), e o LABEL
  // visivel e traduzido via labelKey (com simbolo). Normalmente usadas SEM
  // modificador.
  { labelKey: 'settings.hotkey.groups.media', keys: [
      { k: 'MediaPlayPause', labelKey: 'settings.hotkey.media.playPause' },
      { k: 'MediaStop',      labelKey: 'settings.hotkey.media.stop' },
      { k: 'MediaPrev',      labelKey: 'settings.hotkey.media.prev' },
      { k: 'MediaNext',      labelKey: 'settings.hotkey.media.next' },
  ]},
];
const HOTKEY_MODIFIERS = ['Ctrl','Shift','Alt','Win'];
function isHotkeyModifier(k) { return HOTKEY_MODIFIERS.includes(k); }

// Validacao de hotkey e centralizada no backend (OBSHotkey.IsReservedHotkey +
// HandleValidateHotkey em OBSBridge). Frontend manda 'validate_hotkey' e
// espera 'hotkey_validation_result' { hotkey, ok, reason }. O backend e
// a single source of truth — lista de combinacoes reservadas nao se
// duplica aqui.
let _pendingHotkeyValidation = null;
function validateHotkeyWithBackend(hotkey) {
  return new Promise((resolve) => {
    // Cancela validacao anterior (se o user clicou Salvar duas vezes).
    if (_pendingHotkeyValidation) _pendingHotkeyValidation({ ok: true, reason: '' });
    _pendingHotkeyValidation = resolve;
    Bridge.send('validate_hotkey', { hotkey });
    // Timeout safety: se o backend nao responder em 2s, deixa passar
    // (fail-open). Evita travar a UI se algo der errado.
    setTimeout(() => {
      if (_pendingHotkeyValidation === resolve) {
        _pendingHotkeyValidation = null;
        resolve({ ok: true, reason: '' });
      }
    }, 2000);
  });
}


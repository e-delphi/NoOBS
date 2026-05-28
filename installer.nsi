!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "Sections.nsh"
!include "x64.nsh"

;--------------------------------
; Geral
;--------------------------------
Name "NoOBS"
OutFile "NoOBS-Setup.exe"
InstallDir "$LOCALAPPDATA\NoOBS"
InstallDirRegKey HKCU "Software\NoOBS" "InstallDir"
RequestExecutionLevel user
Unicode True

; Compactacao LZMA solid — instalador final menor (~40-50% comparado
; ao default zlib). /SOLID compacta todos os arquivos como um bloco
; unico, melhorando a razao de compressao em troca de descompactacao
; sequencial (irrelevante porque o instalador extrai tudo no install).
SetCompressor /SOLID lzma

!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"
!define MUI_ABORTWARNING

;--------------------------------
; Paginas do instalador — minimo essencial: Directory pra escolher
; pasta, Components pros opcionais (autostart / atalho), InstFiles
; com log. Welcome/Finish/License removidas. App abre automaticamente
; apos o install (ver .onInstSuccess) e o installer fecha sozinho
; (SetAutoClose true dentro da SecMain).
;
; Sobre a licenca: GPL v3 nao exige que o user "aceite" durante o
; install — so exige que a licenca acompanhe a distribuicao. O arquivo
; LICENSE eh empacotado dentro do exe da NSIS (referenciado pelo
; tema/branding) e tambem pode ser visto no repo do GitHub e em
; "Sobre" dentro do app.
;--------------------------------
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES

;--------------------------------
; Paginas do desinstalador
;--------------------------------
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

;--------------------------------
; Idioma
;--------------------------------
!insertmacro MUI_LANGUAGE "PortugueseBR"

;--------------------------------
; Verificacao de 64-bit + close + force kill se ja estiver rodando
;--------------------------------
Function .onInit
    ${IfNot} ${RunningX64}
        MessageBox MB_OK|MB_ICONSTOP "NoOBS requer Windows 64-bit."
        Abort
    ${EndIf}

    ; Checa se ha alguma instancia rodando (modo full OU hibernate — os
    ; dois usam o titulo "NoOBS"). FindWindow acha tambem janelas
    ; invisiveis/WS_POPUP, entao pega o hibernate tambem.
    FindWindow $0 "" "NoOBS"
    ${If} $0 != 0
        MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "NoOBS está em execução. Clique OK para fechar e continuar com a instalação." IDOK close IDCANCEL abort
        close:
            ; Fase 1 — graceful: WM_CLOSE em loop pra cada janela "NoOBS"
            ; encontrada. Respeita o shutdown limpo (Engine.Teardown,
            ; libobs, FFmpeg, watchers). Itera ate 5 vezes pra cobrir
            ; multiplas janelas (raro, mas defensivo).
            ;
            ; Cuidado: com closeToTray=ON, WM_CLOSE manda pra bandeja em
            ; vez de fechar — por isso a Fase 2 sempre roda em seguida.
            StrCpy $1 0
            loop_close:
                FindWindow $0 "" "NoOBS"
                ${If} $0 == 0
                    Goto force_kill
                ${EndIf}
                SendMessage $0 ${WM_CLOSE} 0 0
                Sleep 400
                IntOp $1 $1 + 1
                ${If} $1 < 5
                    Goto loop_close
                ${EndIf}
            force_kill:
                ; Fase 2 — hard kill: taskkill /F /IM /T captura qualquer
                ; processo NoOBS.exe que sobrou (hibernate em mid-task,
                ; instancia minimizada pra bandeja por WM_CLOSE, ou
                ; processo "ghost" segurando arquivos). /T mata filhos
                ; tambem. Nao falha se nao houver processo (so loga).
                nsExec::ExecToLog 'taskkill /F /IM NoOBS.exe /T'
                Pop $0
                Sleep 500
            Goto done
        abort:
            Abort
        done:
    ${EndIf}

    ; Sincroniza o checkbox "Iniciar com Windows" com o estado real do
    ; registro. Caso classico: user ja tinha autostart ligado (via
    ; install anterior OU via toggle no app), reinstala, e clica
    ; "Avancar" no painel de Componentes sem mexer em nada. Sem o
    ; pre-check, o checkbox vem desmarcado (default /o) e a etapa
    ; "AutostartSync" remove silenciosamente a entrada que o user
    ; tinha. Com o pre-check, o checkbox reflete a config atual e o
    ; user precisa DESMARCAR explicitamente pra remover.
    Call PreCheckAutostart
FunctionEnd

;--------------------------------
; .onInstSuccess — chamada pelo NSIS automaticamente quando o install
; termina sem erro. Lanca o NoOBS direto e o installer fecha sozinho
; (SetAutoClose true), sem precisar de tela final.
;--------------------------------
Function .onInstSuccess
    ; Exec (fire-and-forget) em vez de ExecShell pra nao abrir cmd ou
    ; passar pelo shell. Garante working dir certa pro app encontrar
    ; obs.dll + plugins.
    SetOutPath "$INSTDIR\bin\64bit"
    Exec '"$INSTDIR\bin\64bit\NoOBS.exe"'
FunctionEnd

;--------------------------------
; Instalacao
;--------------------------------
Section "NoOBS" SecMain
    SectionIn RO

    ; Fecha a janela do installer automaticamente quando o install
    ; termina — combinado com .onInstSuccess (que abre o NoOBS), o
    ; user nao precisa clicar Concluir nem ver tela final.
    SetAutoClose true

    SetOutPath "$INSTDIR\bin\64bit"
    File /r "exe\bin\64bit\*.*"

    SetOutPath "$INSTDIR\data"
    File /r "exe\data\*.*"

    SetOutPath "$INSTDIR\obs-plugins"
    File /r "exe\obs-plugins\*.*"

    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Atalhos do Menu Iniciar (sempre criados — SetOutPath define o "Start in" do .lnk)
    SetOutPath "$INSTDIR\bin\64bit"
    CreateDirectory "$SMPROGRAMS\NoOBS"
    CreateShortcut "$SMPROGRAMS\NoOBS\NoOBS.lnk" "$INSTDIR\bin\64bit\NoOBS.exe"
    CreateShortcut "$SMPROGRAMS\NoOBS\Desinstalar.lnk" "$INSTDIR\uninstall.exe"

    ; Marca primeira execucao — OBSBridge le e abre o modal de Configuracoes
    ; automaticamente. Sai sozinho do flag depois de aberto.
    WriteRegDWORD HKCU "Software\NoOBS" "FirstRun" 1

    ; Registro - Adicionar/Remover Programas
    WriteRegStr HKCU "Software\NoOBS" "InstallDir" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoOBS" "DisplayName" "NoOBS"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoOBS" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoOBS" "DisplayIcon" "$INSTDIR\bin\64bit\NoOBS.exe"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoOBS" "Publisher" "NoOBS"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoOBS" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoOBS" "NoRepair" 1
SectionEnd

;--------------------------------
; Componentes opcionais (vem DESMARCADOS por padrao — /o)
;   - Iniciar com Windows: registra entrada no Run do HKCU (sem /tray
;     no install — o flag e adicionado depois pelo app se o user
;     ligar "Minimizar para bandeja" nas configuracoes).
;   - Atalho na area de trabalho
;--------------------------------
Section /o "Iniciar com o Windows" SecAutostart
    ; Flag /autostart e MARCADOR de origem (= "fui lancado pelo logon").
    ; Comportamento (bandeja vs janela visivel) e decidido pelo app
    ; em runtime, lendo o config 'closeToTray'.
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NoOBS" '"$INSTDIR\bin\64bit\NoOBS.exe" /autostart'
    ; Limpa StartupApproved se ficou em estado "disabled" (user tinha
    ; desativado pelo Task Manager/Settings > Inicializar do Windows
    ; numa instalacao anterior). Sem isso, o Windows continua ignorando
    ; a entrada de Run mesmo recem-escrita. Remover a chave faz o
    ; Windows tratar como "default enabled".
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "NoOBS"
SectionEnd

; Hidden section (prefixo "-" = obrigatoria, nao aparece no painel de
; Componentes). SEMPRE roda — independente do user ter marcado ou
; desmarcado o checkbox acima. Aqui sincronizamos a uncheck: se o
; SecAutostart NAO foi selecionado, removemos qualquer entrada de
; autostart que uma instalacao anterior (ou o toggle pelo app via UI)
; tenha deixado.
;
; Sem essa secao, o checkbox era "additive-only": marcado escrevia,
; desmarcado nao fazia nada. Resultado: reinstalar com checkbox
; desmarcado nao removia a entrada antiga — surpresa pra quem espera
; que o instalador seja autoritativo.
Section "-AutostartSync"
    SectionGetFlags ${SecAutostart} $0
    IntOp $0 $0 & ${SF_SELECTED}
    ${If} $0 == 0
        DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NoOBS"
        ; Tambem limpa StartupApproved — assim o estado fica realmente
        ; "nao configurado" (e nao "Run vazio + disabled marcado").
        DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "NoOBS"
    ${EndIf}
SectionEnd

Section /o "Atalho na Area de Trabalho" SecDesktopShortcut
    SetOutPath "$INSTDIR\bin\64bit"
    CreateShortcut "$DESKTOP\NoOBS.lnk" "$INSTDIR\bin\64bit\NoOBS.exe"
SectionEnd

LangString DESC_SecMain             ${LANG_PORTUGUESEBR} "Arquivos do NoOBS (obrigatorio)."
LangString DESC_SecAutostart        ${LANG_PORTUGUESEBR} "Inicia o NoOBS automaticamente quando o Windows logar. Voce pode mudar isso depois nas configuracoes."
LangString DESC_SecDesktopShortcut  ${LANG_PORTUGUESEBR} "Cria um atalho do NoOBS na area de trabalho."

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain}            $(DESC_SecMain)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart}       $(DESC_SecAutostart)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktopShortcut} $(DESC_SecDesktopShortcut)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

;--------------------------------
; Pre-check do checkbox "Iniciar com Windows" baseado no estado atual
; do registro. Declarada APOS as Sections pra que ${SecAutostart} ja
; tenha sido resolvido. Chamada via Call a partir de .onInit (NSIS
; aceita forward-reference de funcoes via Call).
;
; Logica: se a entrada de Run ja existe (de uma instalacao anterior
; OU do toggle do app via Settings), pre-marca o checkbox. User
; precisa DESMARCAR explicitamente pra remover — sem isso, o checkbox
; vinha desmarcado no default /o e a etapa AutostartSync removia
; silenciosamente uma config que o user nao quis tocar.
;--------------------------------
Function PreCheckAutostart
    ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NoOBS"
    ${If} $0 != ""
        SectionGetFlags ${SecAutostart} $1
        IntOp $1 $1 | ${SF_SELECTED}
        SectionSetFlags ${SecAutostart} $1
    ${EndIf}
FunctionEnd

;--------------------------------
; Desinstalacao
;--------------------------------
Function un.onInit
    ; Mesma estrategia do .onInit (graceful WM_CLOSE em loop + taskkill
    ; defensivo) — mas silenciosa, sem MessageBox. Desinstalar implica
    ; que o user quer fechar tudo de qualquer jeito.
    StrCpy $1 0
    un_loop_close:
        FindWindow $0 "" "NoOBS"
        ${If} $0 == 0
            Goto un_force_kill
        ${EndIf}
        SendMessage $0 ${WM_CLOSE} 0 0
        Sleep 400
        IntOp $1 $1 + 1
        ${If} $1 < 5
            Goto un_loop_close
        ${EndIf}
    un_force_kill:
        nsExec::ExecToLog 'taskkill /F /IM NoOBS.exe /T'
        Pop $0
        Sleep 500
FunctionEnd

Section "Uninstall"
    RMDir /r "$INSTDIR\bin"
    RMDir /r "$INSTDIR\data"
    RMDir /r "$INSTDIR\obs-plugins"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    Delete "$DESKTOP\NoOBS.lnk"
    Delete "$SMPROGRAMS\NoOBS\NoOBS.lnk"
    Delete "$SMPROGRAMS\NoOBS\Desinstalar.lnk"
    RMDir "$SMPROGRAMS\NoOBS"

    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoOBS"
    DeleteRegKey HKCU "Software\NoOBS"

    ; Remove autostart se estiver registrado (instalador pode ter adicionado).
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NoOBS"
    ; Tambem limpa StartupApproved — Task Manager/Settings > Inicializar
    ; podem ter armazenado o status (enabled/disabled) la em paralelo.
    ; Sem essa limpeza, a chave fica orfa apontando pra um NoOBS que
    ; nao existe mais.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "NoOBS"

    ; Pergunta se quer remover configuracoes e cache
    MessageBox MB_YESNO "Deseja remover as configuracoes e cache do NoOBS?" IDNO skip
        RMDir /r "$LOCALAPPDATA\NoOBS"
        RMDir /r "$LOCALAPPDATA\TNoOBS"
    skip:
SectionEnd

!include "MUI2.nsh"
!include "LogicLib.nsh"
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

!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"
!define MUI_ABORTWARNING

;--------------------------------
; Paginas do instalador
;--------------------------------
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\bin\64bit\NoOBS.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Abrir NoOBS"
!insertmacro MUI_PAGE_FINISH

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
; Verificacao de 64-bit
;--------------------------------
Function .onInit
    ${IfNot} ${RunningX64}
        MessageBox MB_OK|MB_ICONSTOP "NoOBS requer Windows 64-bit."
        Abort
    ${EndIf}

    ; Fecha o NoOBS se estiver rodando
    FindWindow $0 "" "NoOBS"
    ${If} $0 != 0
        MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "NoOBS esta em execucao. Clique OK para fechar e continuar." IDOK close IDCANCEL abort
        close:
            SendMessage $0 ${WM_CLOSE} 0 0
            Sleep 1000
            Goto done
        abort:
            Abort
        done:
    ${EndIf}
FunctionEnd

;--------------------------------
; Instalacao
;--------------------------------
Section "NoOBS" SecMain
    SectionIn RO

    SetOutPath "$INSTDIR\bin\64bit"
    File /r "exe\bin\64bit\*.*"

    SetOutPath "$INSTDIR\data"
    File /r "exe\data\*.*"

    SetOutPath "$INSTDIR\obs-plugins"
    File /r "exe\obs-plugins\*.*"

    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Atalhos — SetOutPath define o "Start in" do .lnk
    SetOutPath "$INSTDIR\bin\64bit"
    CreateDirectory "$SMPROGRAMS\NoOBS"
    CreateShortcut "$SMPROGRAMS\NoOBS\NoOBS.lnk" "$INSTDIR\bin\64bit\NoOBS.exe"
    CreateShortcut "$SMPROGRAMS\NoOBS\Desinstalar.lnk" "$INSTDIR\uninstall.exe"
    CreateShortcut "$DESKTOP\NoOBS.lnk" "$INSTDIR\bin\64bit\NoOBS.exe"

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
; Componente opcional: Iniciar com Windows
; Habilitado por padrao. User pode desmarcar na pagina Componentes.
; Registra "/tray" — abre minimizado na bandeja ao logar.
;--------------------------------
Section "Iniciar com o Windows" SecAutostart
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NoOBS" '"$INSTDIR\bin\64bit\NoOBS.exe" /tray'
SectionEnd

LangString DESC_SecMain      ${LANG_PORTUGUESEBR} "Arquivos do NoOBS (obrigatorio)."
LangString DESC_SecAutostart ${LANG_PORTUGUESEBR} "Inicia o NoOBS automaticamente quando o Windows logar, minimizado na bandeja do sistema. Voce pode mudar isso depois nas configuracoes."

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain}      $(DESC_SecMain)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} $(DESC_SecAutostart)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

;--------------------------------
; Desinstalacao
;--------------------------------
Function un.onInit
    ; Fecha o NoOBS se estiver rodando
    FindWindow $0 "" "NoOBS"
    ${If} $0 != 0
        SendMessage $0 ${WM_CLOSE} 0 0
        Sleep 1000
    ${EndIf}
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

    ; Pergunta se quer remover configuracoes e cache
    MessageBox MB_YESNO "Deseja remover as configuracoes e cache do NoOBS?" IDNO skip
        RMDir /r "$LOCALAPPDATA\NoOBS"
        RMDir /r "$LOCALAPPDATA\TNoOBS"
    skip:
SectionEnd

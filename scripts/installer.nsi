; Maolan Editor Installer
; Run with: makensis.exe installer.nsi
; Requires all binaries and DLLs to be staged in C:\maolan-staging\editor

Unicode true

!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef MAOLAN_EDITOR_VERSION
!define MAOLAN_EDITOR_VERSION "0.0.6"
!endif

!ifndef MAOLAN_EDITOR_PRODUCT_VERSION
!define MAOLAN_EDITOR_PRODUCT_VERSION "${MAOLAN_EDITOR_VERSION}.0"
!endif

;--------------------------------
; General
;--------------------------------
Name "Maolan Editor"
OutFile "maolan-editor-setup.exe"
InstallDir "$LOCALAPPDATA\Maolan\bin"
InstallDirRegKey HKCU "Software\MaolanEditor" "InstallDir"
RequestExecutionLevel user

;--------------------------------
; Version Info
;--------------------------------
VIProductVersion "${MAOLAN_EDITOR_PRODUCT_VERSION}"
VIAddVersionKey "ProductName" "Maolan Editor"
VIAddVersionKey "ProductVersion" "${MAOLAN_EDITOR_VERSION}"
VIAddVersionKey "FileVersion" "${MAOLAN_EDITOR_VERSION}"
VIAddVersionKey "FileDescription" "Maolan audio editor"
VIAddVersionKey "LegalCopyright" "BSD-2-Clause"

;--------------------------------
; Interface Settings
;--------------------------------
!define MUI_ABORTWARNING
!ifndef MAOLAN_EDITOR_ICON
!define MAOLAN_EDITOR_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!endif
!define MUI_ICON "${MAOLAN_EDITOR_ICON}"
!define MUI_UNICON "${MAOLAN_EDITOR_ICON}"

;--------------------------------
; Pages
;--------------------------------
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

;--------------------------------
; Languages
;--------------------------------
!insertmacro MUI_LANGUAGE "English"

;--------------------------------
; Installer Sections
;--------------------------------
Section "Install"
    SetOutPath "$INSTDIR"

    ; Copy all staged binaries and DLLs
    File "C:\maolan-staging\editor\*.*"

    ; Run VC++ Redistributable installer
    ExecWait '"$INSTDIR\vc_redist.x64.exe" /install /quiet /norestart' $0
    Delete "$INSTDIR\vc_redist.x64.exe"

    ; Store installation folder
    WriteRegStr HKCU "Software\MaolanEditor" "InstallDir" $INSTDIR

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; Add to Add/Remove Programs
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\MaolanEditor" \
        "DisplayName" "Maolan Editor"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\MaolanEditor" \
        "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\MaolanEditor" \
        "DisplayVersion" "${MAOLAN_EDITOR_VERSION}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\MaolanEditor" \
        "Publisher" "Maolan Team"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\MaolanEditor" \
        "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\MaolanEditor" \
        "NoRepair" 1

    ; Create Start Menu shortcuts
    CreateDirectory "$SMPROGRAMS\Maolan Editor"
    CreateShortcut "$SMPROGRAMS\Maolan Editor\Maolan Editor.lnk" "$INSTDIR\maolan-editor.exe" "" "$INSTDIR\maolan-editor.exe" 0
    CreateShortcut "$SMPROGRAMS\Maolan Editor\Uninstall.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\Uninstall.exe" 0

    ; Create desktop shortcut
    CreateShortcut "$DESKTOP\Maolan Editor.lnk" "$INSTDIR\maolan-editor.exe" "" "$INSTDIR\maolan-editor.exe" 0
SectionEnd

;--------------------------------
; Uninstaller Section
;--------------------------------
Section "Uninstall"
    Delete "$INSTDIR\maolan-editor.exe"
    Delete "$INSTDIR\Uninstall.exe"

    Delete "$SMPROGRAMS\Maolan Editor\Maolan Editor.lnk"
    Delete "$SMPROGRAMS\Maolan Editor\Uninstall.lnk"
    RMDir "$SMPROGRAMS\Maolan Editor"

    Delete "$DESKTOP\Maolan Editor.lnk"

    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\MaolanEditor"
    DeleteRegKey HKCU "Software\MaolanEditor"

    RMDir "$INSTDIR"
SectionEnd

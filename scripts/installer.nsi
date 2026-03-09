# CURD Windows Installer Script (NSIS)
# Requires: makensis

!include "MUI2.nsh"

Name "CURD - Universal Semantic Control Plane"
OutFile "curd-setup-x64.exe"
InstallDir "$PROGRAMFILES64\CURD"
InstallDirRegKey HKCU "Software\CURD" ""
RequestExecutionLevel admin

Var StartMenuFolder

!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"

# Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

Section "Install"
    SetOutPath "$INSTDIR"
    
    # Files to include
    File "target\x86_64-pc-windows-gnu\release\curd.exe"
    File "LICENSE"
    File "README.md"

    # Store installation folder
    WriteRegStr HKCU "Software\CURD" "" $INSTDIR
    
    # Create uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    # Add to PATH (requires a helper or simple registry edit)
    # For now, we recommend the user run 'curd hook powershell'
    
    # Shortcuts
    CreateDirectory "$SMPROGRAMS\CURD"
    CreateShortcut "$SMPROGRAMS\CURD\Uninstall CURD.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
    Delete "$INSTDIR\curd.exe"
    Delete "$INSTDIR\LICENSE"
    Delete "$INSTDIR\README.md"
    Delete "$INSTDIR\Uninstall.exe"

    RMDir "$INSTDIR"
    Delete "$SMPROGRAMS\CURD\Uninstall CURD.lnk"
    RMDir "$SMPROGRAMS\CURD"

    DeleteRegKey /ifempty HKCU "Software\CURD"
SectionEnd

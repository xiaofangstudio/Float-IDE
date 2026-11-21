Unicode True
!include "MUI2.nsh"

!ifndef OUTFILE
  !define OUTFILE "Copper-Env-1.9.2-Setup.exe"
!endif

Name "Copper 环境 1.9.2"
OutFile "${OUTFILE}"
InstallDir "$APPDATA\Float\copper"
InstallDirRegKey HKCU "Software\Copper" "Home"
RequestExecutionLevel user
BrandingText "小方工作室"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "SimpChinese"

Section "Copper 环境"
  SetOutPath "$INSTDIR"
  ; 环境文件（脚本相对路径：installer 的上一级；跳过 installer 源码目录）
  File /r /x installer "..\*"
  ; 把安装脚本也放进来，安装器负责 PATH / COPPER_HOME
  File /oname=install.ps1 "..\install.ps1"

  DetailPrint "写入 PATH 与 COPPER_HOME..."
  ExecWait 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\install.ps1" -Dest "$INSTDIR"'

  WriteRegStr HKCU "Software\Copper" "Home" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CopperEnv" "DisplayName" "Copper 环境 1.9.2"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CopperEnv" "Publisher" "小方工作室"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CopperEnv" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CopperEnv" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\install.ps1"
  RMDir /r "$INSTDIR"
  DeleteRegKey HKCU "Software\Copper"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CopperEnv"
SectionEnd

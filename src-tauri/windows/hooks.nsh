; The app adds "Open in Nebula Terminal" to File Explorer and the nebula-terminal
; command to the user's PATH itself (launcher.rs). Uninstalling removes both; an
; update, which runs the old uninstaller, keeps them.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall'
    ; In case the app couldn't run: the menu entries are the visible part.
    DeleteRegKey HKCU "Software\Classes\Directory\shell\NebulaTerminal"
    DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\NebulaTerminal"
    DeleteRegKey HKCU "Software\Classes\Drive\shell\NebulaTerminal"
  ${EndIf}
!macroend

; Windows keeps the icons of shortcuts and pinned apps in a cache, so the taskbar can
; go on showing an old icon after an update. Tell the shell that icons changed, and
; have it rebuild the cache. The installer is 32-bit: Sysnative reaches the 64-bit
; System32 instead of SysWOW64.
!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  ${If} ${FileExists} "$WINDIR\Sysnative\ie4uinit.exe"
    ExecWait '"$WINDIR\Sysnative\ie4uinit.exe" -show'
  ${ElseIf} ${FileExists} "$SYSDIR\ie4uinit.exe"
    ExecWait '"$SYSDIR\ie4uinit.exe" -show'
  ${EndIf}
!macroend

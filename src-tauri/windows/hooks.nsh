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

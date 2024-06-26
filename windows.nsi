OutFile "aw-watcher-media-player-installer.exe"
InstallDir "$LOCALAPPDATA\aw-watcher-media-player"

RequestExecutionLevel user

Page directory
Page instfiles

Section "Install"
    SetOutPath $INSTDIR
    File "target\x86_64-pc-windows-msvc\release\aw-watcher-media-player.exe"

    SetOutPath $INSTDIR\visualization
    File /r "visualization\*.*"

    EnVar::AddValue "PATH" "$INSTDIR"
SectionEnd

Section "Uninstall"
    Delete "$INSTDIR\aw-watcher-media-player.exe"
    RMDir /r "$INSTDIR\visualization" 
    RMDir "$INSTDIR"

    EnVar::DeleteValue "PATH" "$INSTDIR"
SectionEnd

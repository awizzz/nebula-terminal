# Changelog

## 1.0.0

Nebula Terminal is now the whole project. The old standalone Nebula Shell (last release `v0.5.0`) is replaced by **Nebula**, a new interpreter that ships inside the terminal.

### Added

- **Nebula**, a Linux-style command interpreter and the default shell: GNU coreutils from uutils (`ls`, `cp`, `rm`, `head`, `sort`, `wc`… about 70 commands), plus `grep`, `find`, `tree`, `ps`, `kill`, `xargs` and `open`; pipes, `&&`/`||`, redirections, variables, `$(…)`, globs, aliases, `!!`; a prompt with Git status, command duration and exit code; colors while typing, history suggestions, Tab completion; `ls`/`tree` icons; "did you mean" and Windows-to-Linux command tips
- bundled Nerd Font icon subset so icons render without installing a font
- PowerShell 7, Windows PowerShell, Command Prompt, Git Bash and WSL are detected automatically; the first one found is the default
- nine themes with full 16-color palettes, including two light themes; the window follows the theme
- right-click menus on the terminal and on tabs
- command palette groups, fuzzy matching, theme switching and zoom commands
- shortcut recorder with conflict warnings, plus next/previous tab, zoom, Ctrl+Alt+1–9 and Alt+arrow pane navigation
- match count in Find
- Restart / Close bar when a shell exits
- activity dot on background tabs that receive output
- Ctrl+click opens links in the default browser
- About page with version and links
- starting folder accepts `~` and `%VARIABLES%`
- GPU acceleration switch (Settings → Terminal) for machines where WebGL rendering misbehaves

### Changed

- new interface: tabs that merge into the terminal, Windows 11 style settings, quieter colors, purposeful animations
- new app icon
- Ctrl+C copies a selection (and interrupts otherwise), Ctrl+V pastes
- Find moved from Ctrl+F to Ctrl+Shift+F so Ctrl+F reaches the shell; saved settings are migrated
- new shells start in your user folder instead of the app's folder
- background opacity now makes the terminal itself translucent
- confirmations are in-app dialogs instead of browser pop-ups; the paste guard shows the pasted text
- releases are published from `v*` tags only and are no longer marked as previews

### Fixed

- closing the window from the title bar, Alt+F4 or the last tab
- reordering tabs by dragging, which WebView2 blocked
- Mica no longer leaves the window see-through on Windows 10; it falls back to a solid background
- zoom shortcuts work on AZERTY, QWERTZ and the numeric keypad
- a restarted shell gets keyboard focus back
- large background images are resized instead of silently breaking settings persistence
- the terminal no longer reflows lines on top of ConPTY's own reflow when resizing
- PTY commands run off the UI thread, so a large paste cannot freeze the window

### Removed

- the old Nebula Shell (v0.5) and its separate installer
- the "atmosphere" glow setting

## 0.2.0 (preview)

### Added

- Solar Noir design language with a new orbital application icon and six original color palettes
- restrained tab, session, palette and settings motion with reduced-motion and no-motion modes
- drag-resizable split panes with restored pane proportions
- multiline paste confirmation, configurable scrollback and a configurable starting directory
- live terminal titles, previous/next search controls and one-click session restart after process exit

### Fixed

- global shortcuts now work while the xterm input has focus
- terminal search now runs while its search field is focused
- PTY sessions created during an unmount race are closed instead of leaking
- PTY resize events stay synchronized after font and layout changes
- UTF-8 characters split across PTY reads are decoded without corruption
- saved preferences, themes and workspace snapshots are validated and bounded before use
- theme files only carry visual settings and never expose local paths or change paste safeguards and shortcuts
- dragged file paths are always quoted before they are inserted into the command line
- the selected default profile is honored on a fresh workspace

### Changed

- Settings and the command palette load separately from the terminal surface
- workspace restoration now describes its scope accurately: layout is restored and processes start fresh
- the CI and release pipelines run the PTY backend tests
- release builds skip unavailable public provenance attestations for private repositories
- newer pushes replace stale in-progress release builds on the same branch

## 0.1.0 (preview)

### Added

- native Tauri 2 desktop host with custom Windows chrome and Mica support
- xterm.js terminal renderer backed by native PTY sessions
- Nebula, CMD, Windows PowerShell, PowerShell 7 and WSL profile detection
- draggable tabs with middle-click close and a profile picker
- vertical and horizontal split panes with independent PTY sessions
- command palette and in-terminal search
- persistent workspace restoration for tabs and splits
- configurable keyboard shortcuts
- theme gallery with Nebula, Tokyo Night, Catppuccin, Rose Pine, Nord and Gruvbox
- theme import/export, custom accent, background image and opacity controls
- live font, line height, padding, cursor and animation settings
- Ctrl+mouse-wheel terminal zoom, copy-on-select, Ctrl+Shift+C/V and file path drag-and-drop
- Windows MSI/NSIS and portable package release pipeline
- bundled Nebula Shell resource in release builds

### Status

0.1.0 is a preview release. Settings and PTY behavior may still change before the stable desktop line.

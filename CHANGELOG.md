# Nebula Terminal changelog

## 0.2.0

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

## 0.1.0

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

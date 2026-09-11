# Changelog

All notable user-facing changes to Nebula are documented here.

## 0.3.0

### Interface

- Added a dedicated UI layer in `src/ui.rs`.
- Added short ANSI startup animation with configurable speed.
- Added `aurora`, `minimal`, `compact` and `off` banner modes.
- Added `ui` commands for animations, banner layout, tips and command separators.
- Added `ui demo` and `ui reset`.
- Added live theme gallery with color previews.
- Added Rose Pine and Gruvbox themes.
- Added optional right-side local clock.

### Shell experience

- Added current-directory file and folder entries to Tab completion.
- Completion is rebuilt automatically after directory changes.
- Added persistent alias management from the shell.
- Added `backend` command with availability checks.
- Added `history clear` and `history path`.
- Added `about` and `doctor` commands.
- Alias lookup is now case-insensitive.

### Windows integration

- Added terminal detection for diagnostics.
- Added native Win32 local-time retrieval.
- Kept native UAC elevation and Windows display-language detection.

### Releases

- Bumped the package version to 0.3.0.
- Version tags still produce `Nebula.exe` and `Nebula.exe.sha256`.
- SignPath is now optional: releases are signed when configured and published as clearly marked unsigned releases otherwise.

## 0.2.0

- Replaced the Python prototype with a native Rust executable.
- Added persistent history, completion and command hints.
- Added configurable CMD, Windows PowerShell and PowerShell 7 backends.
- Added Windows display-language detection and FR/EN localization.
- Added native UAC elevation with visible administrator state.
- Added configurable prompt templates, RGB themes, aliases and environment variables.
- Added Windows CI builds and tag-based GitHub Releases.

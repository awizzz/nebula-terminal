# Changelog

Notable user-facing changes are listed here.

## 0.3.0 — 2026-09-11

### Added

- optional startup animation
- `aurora`, `minimal`, `compact` and `off` banner modes
- `ui` command for interface settings
- live theme preview
- Rose Pine and Gruvbox themes
- optional right-side clock
- file and directory entries in Tab completion
- persistent alias management
- backend availability checks
- `history clear`, `history path`, `about` and `doctor`
- terminal detection for diagnostics

### Changed

- completion is rebuilt after directory changes
- alias lookup is case-insensitive
- release builds can be published without a configured code-signing provider
- CI now checks formatting and Clippy warnings before building

## 0.2.0 — 2026-09-11

### Changed

- replaced the Python prototype with a native Rust executable

### Added

- persistent history and command hints
- CMD, Windows PowerShell and PowerShell 7 backends
- Windows UI-language detection
- built-in English and French localization
- native UAC elevation
- configurable prompt templates, themes, aliases and environment variables
- Windows CI builds and GitHub Releases

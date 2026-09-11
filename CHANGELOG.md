# Changelog

Notable user-facing changes are listed here.

## 0.4.0 — 2026-09-11

### Added

- `cd -` to return to the previous working directory
- remembered working directory for each Windows drive
- `history on`, `history off` and `history status`
- optional exclusion of commands beginning with a space from persistent history
- `config check` for configuration validation
- backend-aware `sudo` for CMD, Windows PowerShell and PowerShell 7
- nested alias expansion with cycle protection
- automated tests for configuration, locale parity, aliases, parsing and prompt duration formatting
- conventional `nebula --help`, `nebula -h` and startup-option validation
- a verified per-user PowerShell installer and matching uninstaller
- versioned portable ZIP release packages with SHA-256 files
- build-provenance attestations for both the standalone executable and portable ZIP
- scheduled RustSec dependency audit
- MIT license

### Changed

- split the monolithic runtime into dedicated shell and editor modules
- validate configuration before loading or saving it
- save configuration through atomic file replacement
- use the Windows environment-string API for `%VARIABLE%` expansion
- honor the `EDITOR` environment variable before falling back to Notepad
- moved remaining built-in UI/help strings into the locale files
- pin the Rust toolchain and commit `Cargo.lock`
- build, lint and test with Cargo `--locked` mode
- run the test suite across all Cargo targets in CI and release workflows
- validate PowerShell distribution scripts in CI before publishing
- separate normal CI from release publishing
- publish releases only from an explicit version tag or manual release workflow
- serialize release workflows and cancel stale normal CI runs
- pin GitHub Actions to immutable commit SHAs
- use changelog sections as GitHub Release notes

### Security

- normal CI now runs with read-only repository permissions
- persistent command history can be disabled
- sensitive one-off commands can be excluded from persistent history with a leading space
- the installer verifies the downloaded executable against the release SHA-256 before installation
- dependency changes landing on `main` trigger a RustSec audit
- release signing remains optional, but signed artifacts are verified before publication

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

# Nebula

[![CI](https://github.com/awizzz/nebula-shell/actions/workflows/ci.yml/badge.svg)](https://github.com/awizzz/nebula-shell/actions/workflows/ci.yml)
[![Terminal CI](https://github.com/awizzz/nebula-shell/actions/workflows/terminal-ci.yml/badge.svg)](https://github.com/awizzz/nebula-shell/actions/workflows/terminal-ci.yml)
[![Security audit](https://github.com/awizzz/nebula-shell/actions/workflows/security.yml/badge.svg)](https://github.com/awizzz/nebula-shell/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Nebula is a Windows command-line environment built around two independent pieces:

- **Nebula Shell** — a native Rust shell whose default engine launches programs directly instead of routing every command through CMD or PowerShell.
- **Nebula Terminal** — a graphical Windows terminal host with native PTY sessions, tabs, split panes, profiles and deep visual customization.

Nebula Shell works on its own in Windows Terminal, ConHost or another compatible host. Nebula Terminal makes it the default graphical experience while still supporting CMD, Windows PowerShell, PowerShell 7 and WSL.

## Nebula Shell

### Install

The stable Shell release is `v0.5.0`.

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/awizzz/nebula-shell/main/scripts/install.ps1 -OutFile .\install-nebula.ps1
Get-Content .\install-nebula.ps1
Unblock-File .\install-nebula.ps1
.\install-nebula.ps1
```

The installer is per-user, verifies the published SHA-256 checksum and installs under `%LOCALAPPDATA%\Programs\Nebula` by default. It can also add Nebula to the user `PATH`.

Useful options:

```powershell
.\install-nebula.ps1 -Version v0.5.0
.\install-nebula.ps1 -InstallDir D:\Tools\Nebula
.\install-nebula.ps1 -NoPath
```

Portable assets and `uninstall.ps1` are available from the GitHub Release. Unsigned releases may trigger Windows SmartScreen.

### Shell highlights

- direct native executable launch without a CMD or PowerShell intermediary
- native `&&`, `||`, `;`, pipelines and input/output/error redirection
- native filesystem and utility built-ins (`ls`, `cat`, `mkdir`, `touch`, `which`, `cp`, `mv` and Windows aliases)
- explicit CMD, Windows PowerShell and PowerShell 7 compatibility modes
- persistent working directory, `pushd` / `popd`, `cd -` and remembered directories per drive
- persistent history with private-command exclusion
- completion for Nebula commands, aliases, PATH executables and local files
- configurable prompt, RGB colors, aliases, themes and environment variables
- English/French localization with automatic Windows UI-language detection
- administrator relaunch, one-command UAC elevation and runtime diagnostics

Native mode is the default:

```text
backend native
git status
python app.py
ping 1.1.1.1
dir | findstr src
```

Compatibility remains explicit and available:

```text
cmd dir /b
powershell Get-Process
pwsh Get-ChildItem
```

Useful starting commands:

```text
help
doctor
theme tokyo-night
language auto
alias gs=git status
```

Nebula stores Shell configuration under `%APPDATA%\Nebula\config.toml`. See [`config.example.toml`](config.example.toml).

## Nebula Terminal

Nebula Terminal is currently a **0.2.x preview** and is versioned independently from the stable Shell.

Current desktop features include:

- Tauri 2 native Windows window with custom application chrome and Mica support
- xterm.js rendering with WebGL fallback
- native Rust PTY/ConPTY sessions
- Nebula, CMD, Windows PowerShell, PowerShell 7 and WSL profiles
- draggable tabs, middle-click close, profile picker and live session titles
- vertical and horizontal split panes with drag resizing
- workspace restoration for tabs, splits and pane proportions
- keyboard-driven command palette and previous/next terminal search
- Solar Noir interface, six original palettes, optional background image and live typography controls
- full, reduced and disabled animation modes
- editable keyboard shortcuts
- visual-only theme import/export that keeps local and safety settings private
- configurable starting directory and scrollback
- Ctrl+mouse-wheel zoom, multiline paste protection and safely quoted file path drag-and-drop
- automatic restart control when a terminal process exits

### Run the Terminal from source

Requirements: Node.js 24, Rust 1.98.1, Windows MSVC build tools and WebView2.

```powershell
git clone https://github.com/awizzz/nebula-shell.git
cd nebula-shell\apps\nebula-terminal
npm install
npm run tauri dev
```

The dedicated Terminal release pipeline is designed to publish an NSIS installer, MSI installer, standalone EXE and portable ZIP. Preview tags use `terminal-v<version>` so they do not conflict with Shell tags such as `v0.5.0`.

See [`apps/nebula-terminal/README.md`](apps/nebula-terminal/README.md) for desktop-specific documentation.

## Build from source

### Shell

```powershell
git clone https://github.com/awizzz/nebula-shell.git
cd nebula-shell
cargo build --locked --release
```

### Quality checks

```powershell
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

Nebula Terminal has its own Windows CI covering the frontend, Rust backend tests and desktop build.

## Current limitations

### Shell

- Windows only; current prebuilt releases are x64
- native pipelines are buffered between stages rather than streamed concurrently
- `.bat` / `.cmd` scripts require explicit CMD compatibility
- compatibility-backend PowerShell state is not persistent between separate commands
- completion is still generic rather than command-aware
- Authenticode signing depends on an external trusted signing provider

### Terminal preview

- Windows desktop preview is x64-first
- settings and PTY behavior may still evolve before the stable desktop line
- custom SSH/executable profile editing, structured Shell metadata, pane drag-resizing and automatic updates remain future milestones

## Documentation

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — development and pull requests
- [`SECURITY.md`](SECURITY.md) — security policy and local-data notes
- [`docs/architecture.md`](docs/architecture.md) — Shell runtime architecture
- [`docs/terminal-ui.md`](docs/terminal-ui.md) — Terminal architecture and product direction
- [`docs/releasing.md`](docs/releasing.md) — Shell release process
- [`docs/troubleshooting.md`](docs/troubleshooting.md) — common problems and diagnostics
- [`docs/roadmap.md`](docs/roadmap.md) — product direction before 1.0
- [`CHANGELOG.md`](CHANGELOG.md) — Shell release history
- [`apps/nebula-terminal/CHANGELOG.md`](apps/nebula-terminal/CHANGELOG.md) — Terminal preview history

Nebula is available under the [MIT License](LICENSE).

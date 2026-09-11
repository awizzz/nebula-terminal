# Nebula

[![CI](https://github.com/awizzz/nebula-shell/actions/workflows/ci.yml/badge.svg)](https://github.com/awizzz/nebula-shell/actions/workflows/ci.yml)
[![Security audit](https://github.com/awizzz/nebula-shell/actions/workflows/security.yml/badge.svg)](https://github.com/awizzz/nebula-shell/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Nebula is a native Windows shell written in Rust. Its default execution engine launches programs directly without routing every command through CMD or PowerShell, while keeping both shells available for compatibility when their syntax is needed.

Nebula is a **shell frontend**, not a terminal emulator. It runs inside Windows Terminal, ConHost or another compatible terminal host.

## Install

### PowerShell installer

Download the installer, inspect it if desired, then run it:

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/awizzz/nebula-shell/main/scripts/install.ps1 -OutFile .\install-nebula.ps1
Get-Content .\install-nebula.ps1
Unblock-File .\install-nebula.ps1
.\install-nebula.ps1
```

The installer runs per-user, does not require administrator privileges, downloads the selected GitHub Release, verifies `Nebula.exe` against the published SHA-256 file and installs it under `%LOCALAPPDATA%\Programs\Nebula`. It also adds that directory to the user `PATH` unless `-NoPath` is supplied.

Useful installer options:

```powershell
.\install-nebula.ps1 -Version v0.4.0
.\install-nebula.ps1 -InstallDir D:\Tools\Nebula
.\install-nebula.ps1 -NoPath
```

The release also contains `uninstall.ps1` for removing the per-user installation and its `PATH` entry.

### Portable install

Download either `Nebula.exe` directly or the versioned `Nebula-<version>-windows-x64.zip` from the latest GitHub Release. The executable is standalone; Python and Rust are not required at runtime.

Release assets include SHA-256 files. To verify the standalone executable in PowerShell:

```powershell
Get-FileHash .\Nebula.exe -Algorithm SHA256
Get-Content .\Nebula.exe.sha256
```

Unsigned releases can trigger a Windows SmartScreen warning. Releases are marked `(unsigned)` when Authenticode signing is not configured.

## Requirements

- Windows 10 or Windows 11
- x64 for the current prebuilt release
- a terminal with ANSI/VT support is recommended; Windows Terminal provides the best experience

## Features

- native executable launch without a CMD or PowerShell intermediary
- native `&&`, `||`, `;`, pipelines and input/output/error redirection
- native filesystem and utility built-ins (`ls`, `cat`, `mkdir`, `touch`, `which`, `cp`, `mv` and Windows aliases)
- explicit CMD, Windows PowerShell and PowerShell 7 compatibility modes
- persistent working directory, `pushd` / `popd`, `cd -` and remembered directories per drive
- persistent command history with an on/off switch and private-command exclusion
- Tab completion for Nebula commands, aliases, executables in `PATH` and local files
- configurable prompt, RGB colors, aliases and environment variables
- built-in themes and interface presets
- automatic Windows UI-language detection
- built-in English and French localization with user locale overrides
- administrator relaunch and backend-aware one-command UAC elevation
- Git branch, exit status, command duration and optional local clock in the prompt
- configuration validation and runtime diagnostics
- conventional non-interactive `--help` and `--version` startup options

## Command-line startup

```text
nebula
nebula --help
nebula --version
nebula --admin
```

Running `nebula` without arguments starts the interactive shell. Unknown startup options fail with exit code `2` rather than being silently ignored.

## Quick start

```text
help
doctor
theme tokyo-night
language auto
backend native
alias gs=git status
```

Useful navigation:

```text
cd C:\Projects
cd -
pushd D:\Work
popd
D:
```

Administrator mode:

```text
admin
sudo <command>
```

## History and privacy

Persistent history is enabled by default and stored under `%APPDATA%\Nebula`.

```text
history
history off
history on
history clear
history path
```

By default, a command entered with a leading space is excluded from persistent history. This is useful for commands that contain temporary sensitive arguments:

```text
 secret-tool --token ...
```

This is only a convenience feature. Prefer environment variables, secure credential stores or stdin for secrets instead of command-line arguments.

## Configuration

Nebula stores its configuration at:

```text
%APPDATA%\Nebula\config.toml
```

Open, locate or validate it with:

```text
config
config path
config check
```

Reload changes without restarting:

```text
reload
```

Configuration writes are performed through a temporary file and atomic replacement to reduce the chance of a partially written config after interruption.

See [`config.example.toml`](config.example.toml) for all current options.

### Themes

Built-in presets:

```text
hypr
tokyo-night
catppuccin
nord
dracula
rose-pine
gruvbox
```

Run `theme` for a preview.

### Language

`language = "auto"` follows the Windows user-interface language. English and French are built in.

Custom locale overrides can be placed in:

```text
%APPDATA%\Nebula\locales\<locale>.toml
```

### Command execution

Native mode is the default:

```text
backend native
git status
python app.py
ping 1.1.1.1
dir | findstr src
```

Programs are launched directly by Nebula. CMD and PowerShell are still available when you need shell-specific commands or syntax:

```text
cmd dir /b
powershell Get-Process
pwsh Get-ChildItem
```

You can also delegate the whole session command path to a compatibility backend:

```text
backend cmd
backend powershell
backend pwsh
```

Compatibility backends still use a fresh shell process for each command, so backend-specific variables, functions and imported modules do not persist between separate commands. Nebula-owned state does persist.

## Diagnostics

Run:

```text
doctor
```

The diagnostic checks the data directory, configuration, selected backend, Git availability, terminal mode, history state and administrator state.

## Build from source

The repository pins the Rust toolchain and dependency graph for reproducible application builds.

```powershell
git clone https://github.com/awizzz/nebula-shell.git
cd nebula-shell
cargo build --locked --release
```

The executable is written to:

```text
target\release\nebula.exe
```

Before submitting a change:

```powershell
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

## Current limitations

- Windows only
- prebuilt releases are currently x64 only
- no native terminal-emulator window, tabs, panes, blur or GPU renderer
- native pipelines are currently buffered between stages rather than streamed concurrently
- `.bat` / `.cmd` scripts require explicit CMD compatibility
- compatibility-backend PowerShell state is not persistent between commands
- completion is generic rather than command-aware
- Authenticode signing depends on an external signing provider

## Project documentation

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — development and pull requests
- [`SECURITY.md`](SECURITY.md) — security policy and local-data notes
- [`docs/architecture.md`](docs/architecture.md) — runtime architecture
- [`docs/releasing.md`](docs/releasing.md) — release process
- [`docs/troubleshooting.md`](docs/troubleshooting.md) — common problems and diagnostics
- [`docs/roadmap.md`](docs/roadmap.md) — product direction before 1.0
- [`CHANGELOG.md`](CHANGELOG.md) — release history

Nebula is available under the [MIT License](LICENSE).

# Nebula

Nebula is a Windows shell frontend written in Rust. It adds a configurable prompt, history, completion, themes, localization and UAC helpers while keeping normal Windows command execution available through CMD, Windows PowerShell or PowerShell 7.

> Nebula is a shell frontend, not a terminal emulator. It runs inside Windows Terminal, ConHost or another compatible terminal.

## Install

Download `Nebula.exe` from the latest GitHub Release and run it directly. No Python runtime is required.

Unsigned releases may trigger a Windows SmartScreen warning. Release assets include `Nebula.exe.sha256` for integrity checks.

## Features

- CMD compatibility for normal commands, pipes, redirects and `.bat` / `.cmd` files
- optional Windows PowerShell and PowerShell 7 command backends
- persistent working directory, drive switching and environment variables
- persistent history, history hints and Tab completion
- configurable prompt, colors, aliases and environment variables
- built-in themes and interface presets
- automatic Windows UI-language detection
- built-in English and French localization
- administrator relaunch and one-command UAC elevation
- Git branch, exit status and command duration in the prompt
- runtime diagnostics through `doctor`

## Basic usage

```text
help
about
doctor
config
reload
```

Common settings can also be changed from the shell:

```text
theme tokyo-night
language auto
backend pwsh
ui banner minimal
ui animations off
alias gs=git status
```

Administrator mode:

```text
admin
sudo <command>
```

## Configuration

Configuration is stored in:

```text
%APPDATA%\Nebula\config.toml
```

Open it with:

```text
config
```

Reload changes without restarting:

```text
reload
```

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

Custom locale files can be placed in:

```text
%APPDATA%\Nebula\locales\<locale>.toml
```

### Backends

```text
backend cmd
backend powershell
backend pwsh
```

Nebula launches backend commands as child processes. Backend-specific session state such as PowerShell variables, functions and imported modules does not currently persist between commands.

## Build from source

Requirements:

- Windows
- Rust stable with Cargo

```powershell
git clone https://github.com/awizzz/custom-shell.git
cd custom-shell
cargo build --release
```

The executable is written to:

```text
target\release\nebula.exe
```

## Current limitations

- Windows only
- no native terminal-emulator window, tabs, panes or GPU rendering
- PowerShell backend state is not persistent between commands
- completion is currently generic rather than command-aware
- releases are not Authenticode-signed unless a signing provider is configured

## Project docs

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — development and pull requests
- [`SECURITY.md`](SECURITY.md) — reporting security issues
- [`docs/architecture.md`](docs/architecture.md) — code layout and runtime model
- [`docs/releasing.md`](docs/releasing.md) — maintainer release process
- [`CHANGELOG.md`](CHANGELOG.md) — release history

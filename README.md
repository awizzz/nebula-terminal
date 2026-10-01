# Nebula Terminal

[![CI](https://github.com/awizzz/nebula-shell/actions/workflows/ci.yml/badge.svg)](https://github.com/awizzz/nebula-shell/actions/workflows/ci.yml)
[![Security audit](https://github.com/awizzz/nebula-shell/actions/workflows/security.yml/badge.svg)](https://github.com/awizzz/nebula-shell/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A terminal for Windows. Tabs, split panes, a command palette and a settings screen you can actually click through. It runs PowerShell, Command Prompt, Git Bash and WSL.

![Nebula Terminal with two panes and the Nebula theme](docs/images/terminal.png)

## Install

Download the latest installer from [Releases](https://github.com/awizzz/nebula-shell/releases/latest):

- `Nebula Terminal_<version>_x64-setup.exe`: per-user installer (recommended)
- `Nebula Terminal_<version>_x64_en-US.msi`: MSI for managed machines
- `Nebula-Terminal-<version>-windows-x64-portable.zip`: no installation, just unzip and run

Releases are not code-signed yet, so Windows SmartScreen may warn you the first time. Check the file against `SHA256SUMS.txt` from the same release before you continue:

```powershell
Get-FileHash '.\Nebula Terminal_1.0.0_x64-setup.exe' -Algorithm SHA256
```

Requires Windows 10 1809 or later (ConPTY) and the WebView2 runtime, which ships with Windows 11 and current Windows 10 builds.

## What you get

- **Your shells, detected automatically.** PowerShell 7, Windows PowerShell, Command Prompt, Git Bash and WSL. The first one found becomes the default, and you can pick another in Settings.
- **Tabs and split panes.** Drag tabs to reorder them, middle-click to close, and right-click for more. Split a tab into up to four panes and resize them by dragging.
- **Command palette** (`Ctrl+Shift+P`). Every action, shell and theme in one searchable list.
- **Nine themes**, including light ones. The window chrome follows the terminal colors, so a light theme gives you a light app. You can also pick an accent color, a Mica, solid or image background, and adjust transparency.
- **Settings without a config file.** Fonts, cursor, padding, scrollback, shells, starting folder and shortcuts. Every change applies immediately.
- **Safe paste.** Pasting several lines shows exactly what will run before it reaches the shell.
- **Windows habits.** `Ctrl+C` copies when text is selected and interrupts otherwise, `Ctrl+V` pastes, and dropping files inserts their quoted paths.
- **Picks up where you left off.** Your tabs and splits are restored on launch. The shells themselves start fresh.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| New tab | `Ctrl+Shift+T` |
| Close tab | `Ctrl+Shift+W` |
| Next / previous tab | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Go to tab 1–9 | `Ctrl+Alt+1` … `Ctrl+Alt+9` |
| Split right / down | `Ctrl+Shift+D` / `Ctrl+Shift+E` |
| Move between panes | `Alt+Arrow` |
| Close pane | `Ctrl+Shift+Q` |
| Find | `Ctrl+Shift+F` |
| Command palette | `Ctrl+Shift+P` |
| Settings | `Ctrl+,` |
| Zoom in / out / reset | `Ctrl+=` / `Ctrl+-` / `Ctrl+0` (or `Ctrl+Wheel`) |
| Open a link | `Ctrl+Click` |

All of these except the last three rows can be changed in **Settings → Keyboard**. The recorder warns you when two actions share a shortcut.

## Build from source

You need Node.js 24, Rust 1.98.1 (pinned in `rust-toolchain.toml`), the MSVC build tools and WebView2.

```powershell
git clone https://github.com/awizzz/nebula-shell.git
cd nebula-shell
npm ci
npm run tauri dev      # run with hot reload
npm run tauri build    # build the installers into src-tauri/target/release/bundle
```

`npm run dev` opens the interface in a browser with a fake session. That's handy for UI work, but no shell runs there.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the checks CI runs and [docs/architecture.md](docs/architecture.md) for how the code is organized.

## About Nebula Shell

This repository used to also contain **Nebula Shell**, a native command shell written in Rust. It has been discontinued so the project can focus on the terminal. Its last release, [v0.5.0](https://github.com/awizzz/nebula-shell/releases/tag/v0.5.0), and its source code (tag `v0.5.0`) remain available. Nebula Terminal does not need it.

## License

[MIT](LICENSE)

# Nebula Terminal

Nebula Terminal is the graphical desktop host for Nebula Shell. The shell remains usable independently; the Terminal adds a native Windows window, PTY sessions, tabs, split panes and deep visual customization.

The desktop line is versioned separately from Nebula Shell. `0.2.x` focuses on daily use, terminal reliability and Nebula's Solar Noir visual identity.

## Highlights

- Tauri 2 native desktop host with custom Windows chrome and Mica support
- React + TypeScript + Vite UI
- xterm.js viewport with WebGL fallback
- native PTY/ConPTY sessions through the Rust backend
- Nebula, CMD, Windows PowerShell, PowerShell 7 and WSL profiles
- draggable tabs, middle-click close, vertical and horizontal split panes with drag resizing
- persistent workspace layout for tabs, profiles, splits and pane sizes
- command palette (`Ctrl+Shift+P`) and terminal search (`Ctrl+F`)
- clickable Settings for themes, fonts, opacity, cursor, profiles, animation level, scrollback and shortcuts
- Solar Noir interface with six original Nebula palettes, an orbital app mark and reduced-motion support
- theme JSON import/export and optional local background image
- Ctrl+mouse-wheel zoom, Ctrl+Shift+C/V, multiline paste protection and file path drag-and-drop
- live process titles, restartable closed sessions, configurable starting directory and lossless UTF-8 output across PTY reads

## Development

Requirements: Node.js 24, Rust 1.98.1, the Windows MSVC build tools and WebView2.

The npm and Cargo dependency graphs are committed. Use the locked install/build path used by CI:

```powershell
cd apps/nebula-terminal
npm ci
cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
npm run tauri dev
```

For visual work that does not need a PTY:

```powershell
npm run dev
```

The browser preview uses simulated terminal output. Native profiles only run in the Tauri host.

## Release builds

The `Terminal Release` GitHub Actions workflow validates the committed lockfiles and produces a Windows prerelease with:

- NSIS installer
- MSI installer
- portable ZIP
- standalone `Nebula-Terminal.exe`
- SHA-256 files
- bundled `Nebula.exe`
- GitHub provenance attestations

Terminal tags use `terminal-v<version>` so they do not conflict with Nebula Shell tags such as `v0.5.0`.

## Keyboard defaults

| Action | Shortcut |
| --- | --- |
| New default terminal | `Ctrl+Shift+T` |
| Close tab | `Ctrl+Shift+W` |
| Command palette | `Ctrl+Shift+P` |
| Settings | `Ctrl+,` |
| Find in terminal | `Ctrl+F` |
| Split vertically | `Ctrl+Shift+D` |
| Split horizontally | `Ctrl+Shift+E` |
| Close active pane | `Ctrl+Shift+Q` |

All shortcuts are editable from Settings.

## Architecture

See [`../../docs/terminal-ui.md`](../../docs/terminal-ui.md) for the product and architecture direction and [`CHANGELOG.md`](CHANGELOG.md) for preview changes.

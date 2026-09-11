# Nebula Terminal

Nebula Terminal is the graphical desktop host for Nebula Shell. It is currently an early preview and is versioned independently from the stable shell.

## Stack

- Tauri 2 native desktop host
- React + TypeScript + Vite UI
- xterm.js terminal viewport with WebGL fallback
- Rust PTY service using ConPTY on Windows through `portable-pty`

## Development

```powershell
cd apps/nebula-terminal
npm install
npm run tauri dev
```

For visual work that does not need a PTY, `npm run dev` opens a browser preview with a simulated terminal session.

## Current preview scope

- custom titlebar and integrated tabs
- Nebula/CMD/PowerShell profile detection
- real PTY session streaming in the Tauri host
- xterm.js WebGL rendering with fallback
- command palette (`Ctrl+Shift+P`)
- live appearance panel (`Ctrl+,`)
- per-user appearance preferences stored locally
- Windows Mica or solid background mode

The desktop preview is intentionally separate from the shell's `0.5.x` release line. See `docs/terminal-ui.md` for the product and architecture direction.

# Architecture

Nebula Terminal is a [Tauri 2](https://tauri.app) application. A Rust process owns the window and the pseudo-consoles. A React interface draws everything inside the window, and xterm.js renders the terminal. Nebula, the built-in command interpreter, is a separate program (`nebula-sh.exe`) that the terminal runs in a pseudo-console like any other shell.

The repository is a Cargo workspace:

```text
src/                 React interface
src-tauri/           desktop host (Rust)
crates/nebula-sh/    Nebula interpreter (Rust)
scripts/             sidecar build, icon font subset, preview recording
```

```text
┌──────────────────────────── WebView2 ────────────────────────────┐
│ React UI: tabs, panes, palette, settings                         │
│ xterm.js: one instance per pane                                  │
└──────────────┬───────────────────────────────▲───────────────────┘
               │ invoke(start/write/resize/close)  │ Channel<PtyEvent>
┌──────────────▼───────────────────────────────┴───────────────────┐
│ Rust: profile detection, ConPTY sessions (portable-pty), Mica    │
└──────────────────────────────────────────────────────────────────┘
```

## Backend (`src-tauri/src`)

| File | Responsibility |
| --- | --- |
| `main.rs` | Builds the Tauri app, registers commands and the opener plugin, applies Mica. |
| `profiles.rs` | Finds the supported shells, resolves a profile id to an executable and arguments, expands `~` and `%VAR%` in the starting folder. |
| `pty.rs` | One ConPTY session per pane: spawn, write, resize, close. A reader thread streams output to the pane over a Tauri `Channel`, decoding UTF-8 that may be split across reads. |

The webview cannot start arbitrary programs. It sends a **profile id** (`pwsh`, `cmd`, …), and Rust decides which executable that means. Unknown ids are rejected.

## Nebula interpreter (`crates/nebula-sh/src`)

| File | Responsibility |
| --- | --- |
| `main.rs` | Entry point: interactive mode, `-c "line"`, a script file, or `nebula-sh <command> [args]` to run one of its commands directly. |
| `parse.rs` | Words, quotes, `$` expansions, pipelines, `&&`/`||`/`;` and redirections into a small syntax tree. |
| `expand.rs` | `~`, variables, `$(…)`, word splitting and globs. |
| `exec.rs` | Runs pipelines with real OS pipes, so stages stream concurrently; applies redirections; resolves aliases, builtins, Nebula commands and PATH programs. |
| `builtins.rs` | Commands that change the shell itself: `cd`, `export`, `alias`, `history`, `source`… |
| `coreutils.rs` | The uutils coreutils commands, dispatched by name. |
| `extras/` | Commands Nebula implements: `ls` (icon view), `tree`, `grep`, `find`, `ps`, `kill`, `open`, `xargs`, `less`. |
| `editor.rs` | reedline setup: highlighting, history hints, completion, multi-line input, history expansion, window title. |
| `prompt.rs` | Folder, Git status (with a 300 ms budget), duration and exit code. |
| `suggest.rs` | "command not found" with a close match or the Linux equivalent of a Windows command. |
| `sys.rs` | Paths (`/c/…`, `/dev/null`, `~`), PATH lookup with PATHEXT, terminal capabilities. |

Every command except the builtins runs as its own process, `nebula-sh <name> …`, the same way coreutils' multi-call binary works. This keeps pipes and redirections uniform: a pipeline of Nebula commands and Windows programs is just a chain of processes.

The terminal finds the interpreter next to its own executable. `scripts/build-sidecar.mjs` builds it and stages it as a Tauri sidecar (`src-tauri/binaries/nebula-sh-<target>.exe`), which the installers then place beside `Nebula Terminal.exe`.

## Frontend (`src`)

| File | Responsibility |
| --- | --- |
| `App.tsx` | Workspace state: tabs, panes, menus, overlays, global shortcuts, persistence. |
| `components/TerminalPane.tsx` | One xterm.js instance bound to one PTY session: input, clipboard, paste guard, search, exit state. |
| `components/Titlebar.tsx` | Tabs, new-tab button, caption buttons. |
| `components/SettingsPanel.tsx` | Every setting, grouped by page. |
| `components/CommandPalette.tsx` | Fuzzy-searchable list of actions. |
| `components/Menu.tsx` | Context menus and dropdowns. |
| `themes.ts` | Terminal color schemes. The UI chrome is derived from them in CSS. |
| `preferences.ts` | Defaults, validation and migration of saved settings; theme import and export. |
| `session.ts` | Saves and restores the tab and split layout. |
| `keys.ts` | Shortcut parsing, matching and recording. |
| `paneRegistry.ts` | Lets menus and the palette reach the focused pane (copy, paste, clear…). |
| `preview-session.ts` | Real Nebula output replayed by the browser preview (`scripts/record-preview.py`). |

The icon glyphs that `ls`, `tree` and the prompt print come from `assets/fonts/nebula-symbols.woff2`, a subset of the Nerd Fonts symbols built by `scripts/subset-icons.py`. It's added as a fallback after the user's terminal font and loaded before the first terminal renders.

Terminal output never goes through React state. It is written straight into xterm, so a busy shell doesn't re-render the UI.

### Styling

`styles.css` starts with design tokens. `App.tsx` sets the theme's background, foreground and accent as CSS variables, and every other color (chrome, surfaces, text levels, lines) is mixed from those. This is how one stylesheet covers both dark and light themes. Motion uses three durations and is turned down or off through `data-animation` on the root element, as well as by the system's reduced-motion setting.

### Saved data

Settings and the tab layout are stored in the webview's local storage under the `nebula-terminal.*` keys. Everything read back from storage, or from an imported theme file, goes through validation first (`sanitizePreferences`, `loadSession`). Theme files only carry visual settings, never paths or shortcuts.

## Security

- The capability file (`src-tauri/capabilities/default.json`) grants only window controls and opening `http(s)` links.
- Profiles are resolved in Rust. The UI never passes a command line.
- Links in terminal output open only on `Ctrl+Click`.
- Multi-line pastes are confirmed by default.
- Process output is rendered by xterm.js as text, never as HTML.

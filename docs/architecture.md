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
| `main.rs` | Builds the Tauri app, registers commands and the opener and notification plugins, applies Mica, reports the Windows build number to xterm.js. |
| `profiles.rs` | Lists everything a tab can open (built-in shells, WSL distributions, SSH hosts, custom profiles), resolves a profile id to an executable and arguments, expands `~` and `%VAR%` in the starting folder. |
| `wsl.rs` | Installed WSL distributions, read from `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss`. If that key is missing, it asks `wsl.exe -l -q` (UTF-16 output) with a 3 second limit. Docker Desktop's and Rancher Desktop's internal distributions are skipped. |
| `ssh.rs` | `Host` names from `~/.ssh/config` and the files it includes (one level deep, wildcards allowed in file names). Patterns with `*`, `?` or `!` are skipped. |
| `custom.rs` | Custom profiles: validation, the JSON file they are stored in, and the commands the Profiles page uses to list, save and delete them. |
| `integration.rs` | Shell integration for PowerShell, Git Bash and WSL: the arguments that load Nebula's script into each one, and the bash script written to disk. |
| `launcher.rs` | The folder a launch asks for (`nebula-terminal C:\Projects`), the File Explorer menu entry, and the `nebula-terminal` command on the user's PATH. |
| `defterm.rs` | Nebula Terminal as the default terminal of Windows 11: the COM class Windows hands consoles to, and the registry values that choose it. |
| `cmdline.rs` | Splits an arguments line with the Windows (MSVC) rules, and quotes arguments back into a line. |
| `pty.rs` | One ConPTY session per pane: spawn, write, resize, close. A reader thread streams output to the pane over a Tauri `Channel`, decoding UTF-8 that may be split across reads. Input and resizes go through a queue to one I/O thread per session, so they arrive in order. |
| `updater.rs` | Asks GitHub for the latest release, and installs it: picks the setup.exe or MSI that matches how this copy was installed, checks it against `SHA256SUMS.txt`, runs it in passive mode and quits. |

The webview cannot start arbitrary programs. It sends a **profile id**, and Rust decides which executable that means:

| Id | Runs |
| --- | --- |
| `nebula`, `pwsh`, `powershell`, `cmd`, `gitbash`, `wsl` | The built-in shells, found on disk or on PATH. |
| `wsl:<distribution>` | `wsl.exe -d <distribution> --cd ~`, only if that distribution is installed. |
| `ssh:<host>` | `ssh.exe <host>`, only if the host is in your ssh config and `ssh.exe` is on PATH. |
| `custom:<uuid>` | The program and arguments saved for that profile. |

Unknown ids are rejected. Distribution and host names must start with a letter or digit and contain only letters, digits, `.`, `_` and `-`, so they can never be read as options. With a custom starting folder, WSL profiles drop `--cd ~` and start in that folder.

Detection runs on a blocking thread when the app starts and again after a custom profile is saved. The registry lookup, the ssh config and the custom profiles file are all local reads; the only thing that can take time is the `wsl.exe` fallback, and it runs in parallel with the rest.

### Custom profiles

Custom profiles are kept in `custom-profiles.json` in the app's config folder (`%APPDATA%\dev.awizz.nebula-terminal`). Only Rust writes that file, through a temporary file so it is never left half written. Saving checks every field: a name of up to 60 characters, a program that is either an absolute path to an existing file or a bare name found on PATH (`%VAR%` and `~` are expanded; relative paths with folders are refused), at most 64 arguments, a starting folder that exists, and a `#rrggbb` color. There can be 50 of them. Entries read back from the file are checked again, and a profile whose program has been uninstalled stays listed as unavailable instead of disappearing.

The editor sends the arguments as one line. Rust splits it with the Windows rules, stores the list, and the editor shows the split result as you type (`split_arguments`). Starting a custom profile uses the stored entry only; the webview never sends a program or arguments to run.

### Opening a folder

The app runs once per user (`tauri-plugin-single-instance`). A second launch hands its arguments and working folder to the running app, which opens a tab and comes to the front. The first launch reads its own arguments with `launch_folder`.

Two things point at the app, and `sync_launchers` adds or removes them when the app starts or the user flips them in Settings → Behavior. Until the user decides, an installed copy adds both and a portable copy adds neither.

| Entry | Where |
| --- | --- |
| Open in Nebula Terminal | `HKCU\Software\Classes\{Directory, Directory\Background, Drive}\shell\NebulaTerminal`, running `"nebula-terminal.exe" "%V"`. Explorer writes a drive root as `C:"`, which `folder_argument` repairs. |
| `nebula-terminal` command | `%LOCALAPPDATA%\dev.awizz.nebula-terminal\bin`, added to the user's PATH (`HKCU\Environment`). It holds a `.cmd` for cmd, PowerShell and Nebula, and a `sh` script for Git Bash and WSL, like npm's shims. The app's own folder isn't used because it also holds `uninstall.exe`. |

Uninstalling runs `nebula-terminal --uninstall` before the files go (an NSIS hook in `src-tauri/windows/hooks.nsh`, a WiX custom action in `launchers.wxs`). It removes both entries and exits without opening a window. Updates don't run it.

### Default terminal

On Windows 11 22H2 and later, a console program that starts on its own doesn't have to get a conhost window. The console host reads `HKCU\Console\%%Startup`: `DelegationConsole` names the console server that takes over, and `DelegationTerminal` the terminal that shows it. Turning the setting on writes Windows Terminal's OpenConsole (`{2EACA947-7F5F-4CFA-BA87-8F7FBEEFBE69}`, installed with Windows 11) and Nebula's class (`{01EDD6E6-6F42-4E94-910B-C8D5B1F3231C}`), registered as a local server in `HKCU\Software\Classes\CLSID`. Turning it off sets both back to "let Windows decide", unless another terminal took over meanwhile.

OpenConsole creates Nebula's class over COM and calls `ITerminalHandoff3::EstablishPtyHandoff` (or `ITerminalHandoff2` with an older Windows Terminal), from microsoft/terminal's `ITerminalHandoff.idl`. The app registers the class on a thread of its own in the multithreaded apartment, so the call never waits for the UI. If the app isn't running, COM starts it with `-Embedding`, and it opens only the console's tab.

| Handle | Use |
| --- | --- |
| input, output | Pipes to the console: what the user types, and its VT output. Nebula creates them for version 3; version 2 passes its own. |
| signal | Resizes, written as ConPTY does (`PTY_SIGNAL_RESIZE_WINDOW`). Closing it ends the console. |
| reference, server | Keep the console alive while the tab is open. |
| client | The program: its exit closes the session with its exit code, and closing the tab terminates it. |

`pty.rs` keeps the console until a tab attaches (`take_handoffs`, then `attach_handoff`), and from there it runs like any other session. A console that asked to start hidden runs without a tab until its program exits. The proxy that carries these calls between processes comes with Windows Terminal, which is why it has to be installed.

### Shell integration

Nebula marks its prompts and commands with OSC 133 and reports its folder with OSC 7. PowerShell, Git Bash and WSL get the same marks from a script in `src-tauri/shell-integration`, unless it's turned off in Settings → Profiles:

| Shell | How the script gets in |
| --- | --- |
| PowerShell 5.1 and 7 | `-NoExit -EncodedCommand <script>`, after the profile. Inline code isn't subject to the execution policy, which blocks script files on many machines. The script wraps `prompt` and PSReadLine's `PSConsoleHostReadLine`. |
| Git Bash | `--init-file nebula.bash -i` instead of `--login -i`. Bash can't combine the two, so the script reads `/etc/profile` and the first of `~/.bash_profile`, `~/.bash_login` and `~/.profile` itself, as a login shell does. |
| WSL | `-e sh -c <launcher>`, which finds the user's login shell. Bash gets the same script as Git Bash (through `wslpath`); any other shell starts as a login shell, without marks. |

The bash script lives in `%LOCALAPPDATA%\dev.awizz.nebula-terminal\shell-integration` and is rewritten when it's missing or out of date. If it can't be written, Git Bash and WSL start without it. A prompt that already prints OSC 133 marks (oh-my-posh, for example) keeps its own.

## Nebula interpreter (`crates/nebula-sh/src`)

| File | Responsibility |
| --- | --- |
| `main.rs` | Entry point: interactive mode, `-c "line"`, a script file with arguments, or `nebula-sh <command> [args]` to run one of its commands directly. The interpreter runs on a thread with a large stack, so deep function recursion is safe. |
| `parse.rs` | A recursive-descent parser: words, quotes, `$` expansions, pipelines, lists (`&` included), redirections and here-documents, assignments to arrays and elements, and the compound commands (`if`, `for`, `while`, `until`, `case`, `{ }`, `( )`, `(( ))`, `[[ ]]`, functions). |
| `expand.rs` | Brace expansion, `~`, parameters and their `${…}` operators (on array elements too), `$(…)`, `$((…))`, `"$@"` and `"${a[@]}"`, word splitting and globs. |
| `arith.rs` | Shell arithmetic: 64-bit integers, C operators and precedence, assignments, `++`/`--`. |
| `test.rs` | Conditional expressions for `test`, `[` and `[[ ]]`. |
| `exec.rs` | Runs the syntax tree: pipelines with real OS pipes, so stages stream concurrently; redirections; control flow (`break`, `continue`, `return` unwind through a flag); functions with local variables; `set -e`/`-u`/`-x`/`pipefail`; arrays, which live in the shell rather than the environment; background jobs, started in their own process group with an empty standard input; `trap`. Resolves aliases, functions, builtins, Nebula commands and PATH programs. |
| `builtins.rs` | Commands that run inside the shell: `cd`, `export`, `alias`, `source`, `echo`, `test`, `read`, `mapfile`, `local`, `declare`, `set`, `eval`, `command`, `jobs`, `wait`, `fg`, `trap`… |
| `complete.rs` | Tab completion: commands, paths, options read from `--help`, Git subcommands and refs, variables, SSH hosts. |
| `coreutils.rs` | The uutils coreutils commands, dispatched by name. |
| `extras/` | Commands Nebula implements: `ls` (icon view), `tree`, `grep`, `find`, `awk`, `ps`, `kill`, `open`, `xargs`, `less`; and `sed`, `diff` and `cmp` on top of the uutils libraries. |
| `extras/awk/` | `awk`: a lexer, a recursive-descent parser for the POSIX grammar, and a tree-walking interpreter. Fields split lazily, arrays keep insertion order, `print \| cmd`, `cmd \| getline` and `system()` run through `nebula-sh -c`. |
| `editor.rs` | reedline setup: highlighting, history hints, multi-line input, history expansion, window title, and the OSC 133 marks around each command. |
| `fuzzy.rs` | Fuzzy matching for `Ctrl+R`: substrings first, then letters in order, with bonuses for word starts and runs. |
| `frecency.rs` | The folders `z` knows, ranked by visits and age as rupa's z does, in `dirs.txt`. |
| `prompt.rs` | Folder, Git status (with a 300 ms budget), duration and exit code. |
| `suggest.rs` | "command not found" with a close match or the Linux equivalent of a Windows command. |
| `sys.rs` | Paths (`/c/…`, `/dev/null`, `~`), PATH lookup with PATHEXT, terminal capabilities. |

Every command except the builtins runs as its own process, `nebula-sh <name> …`, the same way coreutils' multi-call binary works. This keeps pipes and redirections uniform: a pipeline of Nebula commands and Windows programs is just a chain of processes.

Shell variables are environment variables, so every child process sees them. Functions, aliases and blocks can't run concurrently inside one process, so when they appear in a pipeline (`myfunc | grep x`) or in `( … )`, Nebula writes the function and alias definitions, the options and the arguments to a temporary script and runs it in a child `nebula-sh --subshell`. `$(…)` runs in the shell itself, then puts back the directory, variables and functions it found, as a sub-shell would.

The terminal finds the interpreter next to its own executable. `scripts/build-sidecar.mjs` builds it and stages it as a Tauri sidecar (`src-tauri/binaries/nebula-sh-<target>.exe`), which the installers then place beside `Nebula Terminal.exe`.

## Frontend (`src`)

| File | Responsibility |
| --- | --- |
| `App.tsx` | Workspace state: tabs, panes, menus, overlays, global shortcuts, persistence. |
| `components/TerminalPane.tsx` | One xterm.js instance bound to one PTY session: input, clipboard, paste guard, search, exit state, the OSC 133 marks behind notifications and command jumps, OSC 7 folders, and OSC 9 / 777 notifications. |
| `components/UpdateBanner.tsx` | The card that offers a new version. |
| `components/Titlebar.tsx` | Tabs, new-tab button, caption buttons. |
| `components/SettingsPanel.tsx` | Every setting, grouped by page, including the custom profile editor. |
| `components/CommandPalette.tsx` | Fuzzy-searchable list of actions. |
| `components/Menu.tsx` | Context menus and dropdowns. |
| `themes.ts` | Terminal color schemes. The UI chrome is derived from them in CSS. |
| `preferences.ts` | Defaults, validation and migration of saved settings; theme import and export. |
| `themeImport.ts` | Reading Windows Terminal and iTerm2 color schemes, and checking imported schemes read back from storage. |
| `layout.ts` | The split layout of a tab as a tree: split, close, resize, find the pane next to another one, and validate a saved tree. |
| `session.ts` | Saves and restores tabs (names, colors, layouts), and migrates layouts saved before mixed splits. A pane whose profile no longer exists opens the default profile instead. |
| `profiles.ts` | Picking a profile, grouping profiles for menus (shells, WSL, SSH, custom), and the fake profiles of the browser preview. |
| `customProfiles.ts` | Calls to the custom profile commands, with an in-memory stand-in for the browser preview. |
| `keys.ts` | Shortcut parsing, matching and recording. |
| `commandMarks.ts` | Finding the next prompt for command jumps, and joining wrapped lines of a command's output. |
| `folders.ts` | Reading the folder a shell reports (OSC 7) and checking a folder before a new shell starts in it. |
| `terminalInput.ts` | Quoting dropped paths for each shell, and telling pastes from typing. |
| `notify.ts` | Windows notifications, and the text for a finished command. |
| `updates.ts` | Update checks: when to check, which version was put off. |
| `platform.ts` | The Windows build number, for xterm's ConPTY handling. |
| `paneRegistry.ts` | Lets menus and the palette reach the focused pane (copy, paste, clear…). |
| `preview-session.ts` | Real Nebula output replayed by the browser preview (`scripts/record-preview.py`). |

The icon glyphs that `ls`, `tree` and the prompt print come from `assets/fonts/nebula-symbols.woff2`, a subset of the Nerd Fonts symbols built by `scripts/subset-icons.py`. It's added as a fallback after the user's terminal font and loaded before the first terminal renders.

Terminal output never goes through React state. It is written straight into xterm, so a busy shell doesn't re-render the UI.

### Split layouts

Each tab keeps its panes twice: as a flat list of pane models, in the order they were opened, and as a layout tree (`layout.ts`) whose leaves are pane ids and whose splits hold a direction and the share of each child. `App.tsx` turns the tree into rectangles and renders every pane of a tab as a sibling in that flat list, absolutely positioned, with the dividers drawn on top. Because a pane's place in the React tree never depends on the shape of the layout, splitting, closing or resizing never remounts a terminal, and its shell keeps running.

Splitting a pane in the direction of its parent adds a sibling instead of nesting, and closing a pane collapses any split left with one child, so the tree stays as flat as the layout allows.

### Styling

`styles.css` starts with design tokens. `App.tsx` sets the theme's background, foreground and accent as CSS variables, and every other color (chrome, surfaces, text levels, lines) is mixed from those. This is how one stylesheet covers both dark and light themes. Motion uses three durations and is turned down or off through `data-animation` on the root element, as well as by the system's reduced-motion setting.

### Saved data

Settings and the tab layout are stored in the webview's local storage under the `nebula-terminal.*` keys. Custom profiles are the exception: they live in a file that only Rust writes (see above), because they decide what runs. Everything read back from storage, or from an imported theme file, goes through validation first (`sanitizePreferences`, `loadSession`). Theme files only carry visual settings, never paths or shortcuts.

## Security

- The capability file (`src-tauri/capabilities/default.json`) grants only window controls and opening `http(s)` links.
- Profiles are resolved in Rust. The UI never passes a command line. Custom profiles are the one way to run a program of your choice: they are validated and stored by Rust, and started by id.
- Links in terminal output open only on `Ctrl+Click`.
- Multi-line pastes are confirmed by default.
- Process output is rendered by xterm.js as text, never as HTML.

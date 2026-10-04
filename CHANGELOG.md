# Changelog

## Unreleased

### Added

- a Scoop manifest with every release: `scoop install https://github.com/awizzz/nebula-terminal/releases/latest/download/nebula-terminal.json`, and `scoop update nebula-terminal` follows new versions

## 1.2.0

Tabs now open where you are. PowerShell, Git Bash and WSL mark their commands like Nebula does, and the app fits into Windows: an entry in File Explorer's menu, a `nebula-terminal` command, and the option to be the default terminal of Windows 11.

### Added

- new tabs and splits open in the folder of the shell you're in (Nebula reports it with OSC 7), and restored tabs reopen in the folder they were in
- a choice of starting folder in Settings → Profiles: your user folder (still the default), Desktop, Documents or a folder you type. Desktop and Documents follow OneDrive and other redirections
- `Ctrl+↑` and `Ctrl+↓` jump between commands in the history, and click a prompt to select that command's output. Failed commands get a red mark next to the scrollbar. This works in Nebula and in any shell that marks its commands (OSC 133); in other shells the keys reach the shell as before
- a Copy last command output action, in the command palette and the terminal's right-click menu
- shell integration for PowerShell, Git Bash and WSL: they now mark their commands and report their folder like Nebula, with nothing to set up, so notifications, command jumps and new tabs in the current folder work there too. Your profile and startup files still run first. In WSL this covers bash; other shells start as before. It can be turned off in Settings → Profiles
- Open in Nebula Terminal, in the right-click menu of folders and drives in File Explorer (under Show more options on Windows 11)
- a `nebula-terminal` command: `nebula-terminal .` opens a tab in the current folder, from cmd, PowerShell, Nebula, Git Bash or WSL
- the installed app adds both, and Settings → Behavior turns them off. The portable version only adds them if you turn them on there
- Nebula Terminal can be the default terminal of Windows 11 (version 22H2 and later): console programs started from the Start menu, Explorer or Run open in a tab instead of a window of their own. Turn it on in Settings → Behavior. Like Windows Terminal, it relies on the console that comes with Windows Terminal

### Changed

- starting the app again, from a shortcut, Explorer or the command, opens a tab in the window that's already open instead of a second copy

## 1.1.1

A small update: Ctrl+Wheel zoom works anywhere in the history, and the project has a new name and license.

### Changed

- the repository is now `awizzz/nebula-terminal`; old links redirect, and the app checks for updates at the new address
- the license is now the PolyForm Shield License 1.0.0: use the app for anything and contribute freely, but the code can't be used to build a competing product. Versions up to 1.1.0 stay under the MIT License

### Fixed

- Ctrl+Wheel zooms wherever the scrollbar is; it only worked at the very top or bottom of the history (#30)
- `SHA256SUMS.txt` lists files under the names GitHub publishes them with (`Nebula.Terminal_…` instead of `Nebula Terminal_…`), which the in-app updater needs to verify a download

## 1.1.0

Nebula runs real scripts now, panes split in any direction, and the app can notify you and update itself. From this version on, new releases install from inside the app.

### Nebula

- **scripts**: `if` / `elif` / `else`, `for` (lists and `for ((…))`), `while`, `until`, `case`, `{ … }`, `( … )`, `break` and `continue`
- functions with `local` variables, `return`, `$1`…`$9`, `$#`, `"$@"` and `shift`
- arithmetic with `$((…))`, `((…))` and `let`; `test`, `[` and `[[ … ]]` with patterns and `=~`
- `${x:-default}`, `${#x}`, `${x%.txt}`, `${x/old/new}`, `${x:1:3}`, `${x^^}` and the other parameter forms; `$'…'` strings
- here-documents (`<<`, `<<-`) and here-strings (`<<<`)
- `read`, `echo`, `eval`, `command -v`, `declare`, `set -e`, `set -u`, `set -x` and `set -o pipefail`
- `nebula-sh script.sh args` sets `$0` and the arguments, and `.sh` files run with Nebula
- Tab completes the options of every Nebula command, Git subcommands and branches, `$VARIABLES` and SSH hosts; keywords and functions are highlighted
- `kill` sends the signal you ask for (`kill -0` only checks), `find -ok` asks first, `xargs` gains `-r`, `-d`, `-L` and `-t`
- `$(…)` runs like a sub-shell: a `cd` inside it no longer moves the shell, and its exit status is kept
- Ctrl+C stops the whole command line, loops included
- fixed: aliases in pipelines could hang; an alias calling the command of the same name failed; `grep` kept running after `| head`; `open` cut URLs at `&`; `cd C:\` and `\\server\share` needed quotes; `!$` broke quoted arguments; completing names with spaces or parentheses

### Terminal

- mixed split layouts: split any pane right or down, in any combination, and drag every divider to resize
- up to eight panes per tab (was four)
- rename a tab by double-clicking it, from its right-click menu or from the command palette; an empty name brings back the shell's title
- tab colors: eight colors that work on dark and light themes, shown as a thin bar on the tab
- one profile for each installed WSL distribution (`wsl -d <name>`), read from the registry; Docker Desktop's internal distributions are left out
- one profile for each host in `~/.ssh/config` (and the files it includes) when OpenSSH is installed
- custom profiles: any program with its own arguments, starting folder and color, added in Settings → Profiles. The arguments are split with the Windows rules and shown before you save
- the new-tab menu and the default profile picker group shells, WSL distributions, SSH hosts and custom profiles once the list gets long; the menu scrolls when it no longer fits
- "Connect to …" and "New … tab" commands in the palette for every profile, and a "Profile settings" command
- a Windows notification when a long command finishes in the background, and a green or red dot on its tab; programs can send their own with OSC 9 and OSC 777
- updates from inside the app: a daily check on GitHub, then the matching installer is downloaded, checked against `SHA256SUMS.txt` and run

### Changed

- `Alt+Arrow` moves to the nearest pane in that direction, across nested splits
- a pane too small to split shows a notice instead of producing an unusable pane
- saved sessions use a new format; layouts saved by 1.0 are migrated on first launch
- Settings → Shells is now Settings → Profiles and shows what each profile runs
- tabs whose profile was removed keep running; on the next launch they open the default profile
- a single pasted line no longer runs on its own (like Windows Terminal)

### Fixed

- Find or the palette opened from a menu could send what you typed to the shell
- dropping a file with `$(…)` or backticks in its name could run a command in PowerShell or bash
- the multi-line paste check could be skipped with a trailing newline
- keys typed while a shell was starting were lost
- input and resizes could reach the shell out of order
- ConPTY reflow was done twice on Windows 10
- OSC 8 links ignored the Ctrl+click rule, and Ctrl+C / Ctrl+V failed on Cyrillic and Greek layouts
- importing a theme or a background could undo settings changed at the same time
- shortcuts could be set to a key without a modifier, which then never reached the shell

## 1.0.0

Nebula Terminal is now the whole project. The old standalone Nebula Shell (last release `v0.5.0`) is replaced by **Nebula**, a new interpreter that ships inside the terminal.

### Added

- **Nebula**, a Linux-style command interpreter and the default shell: GNU coreutils from uutils (`ls`, `cp`, `rm`, `head`, `sort`, `wc`… about 70 commands), plus `grep`, `find`, `tree`, `ps`, `kill`, `xargs` and `open`; pipes, `&&`/`||`, redirections, variables, `$(…)`, globs, aliases, `!!`; a prompt with Git status, command duration and exit code; colors while typing, history suggestions, Tab completion; `ls`/`tree` icons; "did you mean" and Windows-to-Linux command tips
- bundled Nerd Font icon subset so icons render without installing a font
- PowerShell 7, Windows PowerShell, Command Prompt, Git Bash and WSL are detected automatically; the first one found is the default
- nine themes with full 16-color palettes, including two light themes; the window follows the theme
- right-click menus on the terminal and on tabs
- command palette groups, fuzzy matching, theme switching and zoom commands
- shortcut recorder with conflict warnings, plus next/previous tab, zoom, Ctrl+Alt+1–9 and Alt+arrow pane navigation
- match count in Find
- Restart / Close bar when a shell exits
- activity dot on background tabs that receive output
- Ctrl+click opens links in the default browser
- About page with version and links
- starting folder accepts `~` and `%VARIABLES%`
- GPU acceleration switch (Settings → Terminal) for machines where WebGL rendering misbehaves

### Changed

- new interface: tabs that merge into the terminal, Windows 11 style settings, quieter colors, purposeful animations
- new app icon
- Ctrl+C copies a selection (and interrupts otherwise), Ctrl+V pastes
- Find moved from Ctrl+F to Ctrl+Shift+F so Ctrl+F reaches the shell; saved settings are migrated
- new shells start in your user folder instead of the app's folder
- background opacity now makes the terminal itself translucent
- confirmations are in-app dialogs instead of browser pop-ups; the paste guard shows the pasted text
- releases are published from `v*` tags only and are no longer marked as previews

### Fixed

- closing the window from the title bar, Alt+F4 or the last tab
- reordering tabs by dragging, which WebView2 blocked
- Mica no longer leaves the window see-through on Windows 10; it falls back to a solid background
- zoom shortcuts work on AZERTY, QWERTZ and the numeric keypad
- a restarted shell gets keyboard focus back
- large background images are resized instead of silently breaking settings persistence
- the terminal no longer reflows lines on top of ConPTY's own reflow when resizing
- PTY commands run off the UI thread, so a large paste cannot freeze the window

### Removed

- the old Nebula Shell (v0.5) and its separate installer
- the "atmosphere" glow setting

## 0.2.0 (preview)

### Added

- Solar Noir design language with a new orbital application icon and six original color palettes
- restrained tab, session, palette and settings motion with reduced-motion and no-motion modes
- drag-resizable split panes with restored pane proportions
- multiline paste confirmation, configurable scrollback and a configurable starting directory
- live terminal titles, previous/next search controls and one-click session restart after process exit

### Fixed

- global shortcuts now work while the xterm input has focus
- terminal search now runs while its search field is focused
- PTY sessions created during an unmount race are closed instead of leaking
- PTY resize events stay synchronized after font and layout changes
- UTF-8 characters split across PTY reads are decoded without corruption
- saved preferences, themes and workspace snapshots are validated and bounded before use
- theme files only carry visual settings and never expose local paths or change paste safeguards and shortcuts
- dragged file paths are always quoted before they are inserted into the command line
- the selected default profile is honored on a fresh workspace

### Changed

- Settings and the command palette load separately from the terminal surface
- workspace restoration now describes its scope accurately: layout is restored and processes start fresh
- the CI and release pipelines run the PTY backend tests
- release builds skip unavailable public provenance attestations for private repositories
- newer pushes replace stale in-progress release builds on the same branch

## 0.1.0 (preview)

### Added

- native Tauri 2 desktop host with custom Windows chrome and Mica support
- xterm.js terminal renderer backed by native PTY sessions
- Nebula, CMD, Windows PowerShell, PowerShell 7 and WSL profile detection
- draggable tabs with middle-click close and a profile picker
- vertical and horizontal split panes with independent PTY sessions
- command palette and in-terminal search
- persistent workspace restoration for tabs and splits
- configurable keyboard shortcuts
- theme gallery with Nebula, Tokyo Night, Catppuccin, Rose Pine, Nord and Gruvbox
- theme import/export, custom accent, background image and opacity controls
- live font, line height, padding, cursor and animation settings
- Ctrl+mouse-wheel terminal zoom, copy-on-select, Ctrl+Shift+C/V and file path drag-and-drop
- Windows MSI/NSIS and portable package release pipeline
- bundled Nebula Shell resource in release builds

### Status

0.1.0 is a preview release. Settings and PTY behavior may still change before the stable desktop line.

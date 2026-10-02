# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, profiles, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (1.2)

The things people notice in their first hour.

- **New tabs and splits open in the current folder.** Nebula reports its directory (OSC 7), so `Ctrl+Shift+T` or a split starts where you are, and restored tabs reopen in their last folder.
- **Jump between commands.** `Ctrl+↑` / `Ctrl+↓` move from one prompt to the next, a click on a prompt selects its output, and "Copy last output" lands in the palette. Failed commands get a red mark in the scrollbar.
- **Shell integration for PowerShell, Git Bash and WSL**, added automatically, so notifications and command jumps work there too, not just in Nebula.
- **Open Nebula Terminal here** from the right-click menu in File Explorer, and a `nebula-terminal` command to open a folder from anywhere.
- **winget and Scoop packages:** `winget install Awizz.NebulaTerminal`.
- **French interface.** The app and Nebula's messages follow the Windows language, starting with English and French.

## Soon (1.3 and 1.4)

### Terminal

- **Signed releases** (Authenticode), so SmartScreen stops warning and updates are checked against a signature as well as a checksum.
- **Quake mode:** a drop-down window on a global hotkey that slides over whatever you're doing.
- **Run a profile as administrator**, in its own clearly marked tab.
- **Several windows:** drag a tab out to make a new window, or back in to merge.
- **Broadcast input:** type once, send to every pane of a tab.
- **Per-profile looks:** each profile can have its own theme, font and background.
- **Theme editor**, and import of Windows Terminal and iTerm2 color schemes.
- **Images in the terminal** (Sixel and the iTerm2 protocol), for tools like `chafa` or `viu`.
- **Font ligatures** for fonts that have them (Cascadia Code, Fira Code, JetBrains Mono).
- **Better search:** regular expressions, match case and whole word, and results highlighted in the scrollbar.

### Nebula

- **Arrays and associative arrays**, `getopts`, brace expansion (`{a,b}`, `{1..10}`) and `**` globs.
- **Background jobs:** `&`, `jobs`, `fg`, `bg` and `wait`, plus `trap`.
- **`sed` and `awk`**, the two commands Linux users miss most after `grep`.
- **Fuzzy history search** on `Ctrl+R`, and directory jumping that learns where you go (`z projects`).
- **A configurable prompt** in `~/.nebularc`: choose the segments (Git, time, Node or Rust version, battery), their order and colors, or keep using Starship.
- **Completion for more programs:** npm and pnpm scripts, cargo, docker, kubectl, winget, and your own completions in a simple file.

## Later

- ARM64 builds for Windows on Arm laptops.
- Set Nebula Terminal as the default terminal of Windows 11, so console programs open in it.
- Settings sync through a file you choose (OneDrive, Dropbox, a Git repository), with no account.
- Saved SSH connections with their own folder, keys and port forwarding, from the profile editor.
- `help <command>` with short, practical examples, the way tldr does it.
- A screen-reader mode and a high-contrast theme, checked with Narrator and NVDA.
- More languages for the interface, contributed by users.
- A small website with a demo, the documentation and the changelog.

## Exploring

Ideas that need more thought before they get a slot.

- Nebula outside the terminal: install `nebula-sh` on its own and add it to Windows Terminal automatically.
- Command blocks: each command and its output as one unit you can collapse, copy or share.
- Saved workspaces: a set of tabs, splits and folders you open in one click ("frontend", "server").
- A Linux build of the terminal for people who use both systems.

## Not planned

- Accounts, cloud sync or telemetry. Nothing leaves your machine except the update check.
- An extension marketplace.
- Becoming an editor or IDE.

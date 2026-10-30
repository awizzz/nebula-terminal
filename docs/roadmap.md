# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, profiles and SSH connections, workspaces, clickable file paths, shell integration, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (1.7)

Installing in one command, and making the window your own.

- **A winget package:** `winget install Awizz.NebulaTerminal`. It's waiting for review in the winget repository; Scoop already works (see the README).
- **Quake mode:** a drop-down window on a global hotkey that slides over whatever you're doing.
- **Per-profile looks:** each profile can have its own theme, font and background, and admin windows a look that sets them apart.
- **A configurable prompt** in `~/.nebularc`: choose the segments (Git, time, Node or Rust version, the last command's duration, an admin marker), their order and colors, or keep using Starship.

## Soon (1.8 and 1.9)

### Terminal

- **Several windows:** drag a tab out to make a new window, or back in to merge.
- **Images in the terminal** (Sixel and the iTerm2 protocol), for tools like `chafa` or `viu`.
- **Font ligatures** for fonts that have them (Cascadia Code, Fira Code, JetBrains Mono).

### Nebula

- **Completion for more programs:** kubectl and the rest, and your own completions in a simple file.
- **Ctrl+Z** to pause the command in front, then `fg` or `bg` to resume it.

## Later

- **Signed releases** (Authenticode), once a code-signing certificate fits the project's budget. Until then SmartScreen may warn on first run, and every release lists its SHA-256 checksums.
- ARM64 builds for Windows on Arm laptops.
- Settings sync through a file you choose (OneDrive, Dropbox, a Git repository), with no account.
- `help <command>` with short, practical examples, the way tldr does it.
- A screen-reader mode and a high-contrast theme, checked with Narrator and NVDA.
- A French interface, then more languages contributed by users.
- The documentation and the changelog on [nebula.awizz.space](https://nebula.awizz.space), next to the demo.

## Exploring

Ideas that need more thought before they get a slot.

- Nebula outside the terminal: install `nebula-sh` on its own and add it to Windows Terminal automatically.
- Command blocks: each command and its output as one unit you can collapse, copy or share.
- A Linux build of the terminal for people who use both systems.

## Not planned

- Accounts, cloud sync or telemetry. Nothing leaves your machine except the update check.
- An extension marketplace.
- Becoming an editor or IDE.

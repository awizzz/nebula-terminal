# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, profiles with their own look, SSH connections, workspaces, a quick terminal, images, clickable file paths, shell integration, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (1.9)

More done in fewer keys, in the terminal and in Nebula.

### Terminal

- **Zoom a pane:** make the pane in front fill its tab, and put it back, with one shortcut.
- **Saved commands:** keep the commands you type often, and run them from the palette in the pane in front.
- **Font ligatures** for fonts that have them (Cascadia Code, Fira Code, JetBrains Mono).

### Nebula

- **Completion for more programs:** kubectl, gh, az, terraform and the rest, and your own completions in a simple file.
- **`help <command>`** with short, practical examples, the way tldr does it.
- **Ctrl+Z** to pause the command in front, then `fg` or `bg` to resume it.

### Packages

- **The winget package:** `winget install Awizz.NebulaTerminal`, once Microsoft's reviewers get to it. Scoop already works (see the README).

## Later

- **Signed releases** (Authenticode), once a code-signing certificate fits the project's budget. Until then SmartScreen may warn on first run, and every release lists its SHA-256 checksums.
- ARM64 builds for Windows on Arm laptops.
- Settings sync through a file you choose (OneDrive, Dropbox, a Git repository), with no account.
- A tray icon, so the quick terminal can stay when the main window is closed.
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

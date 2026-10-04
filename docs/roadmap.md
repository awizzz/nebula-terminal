# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, profiles, shell integration, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (1.3)

Installing and updating the way Windows users expect.

- **A winget package:** `winget install Awizz.NebulaTerminal`. (Scoop already works, see the README.)
- **Signed releases** (Authenticode), so SmartScreen stops warning and updates are checked against a signature as well as a checksum.

## Soon (1.4 and 1.5)

### Terminal

- **Quake mode:** a drop-down window on a global hotkey that slides over whatever you're doing.
- **Run a profile as administrator**, in its own clearly marked tab.
- **Several windows:** drag a tab out to make a new window, or back in to merge.
- **Per-profile looks:** each profile can have its own theme, font and background.
- **Theme editor:** change any color of a scheme and keep it as your own.
- **Images in the terminal** (Sixel and the iTerm2 protocol), for tools like `chafa` or `viu`.
- **Font ligatures** for fonts that have them (Cascadia Code, Fira Code, JetBrains Mono).

### Nebula

- **`getopts`** and `**` globs.
- **A configurable prompt** in `~/.nebularc`: choose the segments (Git, time, Node or Rust version, battery), their order and colors, or keep using Starship.
- **Completion for more programs:** kubectl and the rest, and your own completions in a simple file.

## Later

- ARM64 builds for Windows on Arm laptops.
- Settings sync through a file you choose (OneDrive, Dropbox, a Git repository), with no account.
- Saved SSH connections with their own folder, keys and port forwarding, from the profile editor.
- `help <command>` with short, practical examples, the way tldr does it.
- A screen-reader mode and a high-contrast theme, checked with Narrator and NVDA.
- A French interface, then more languages contributed by users.
- The documentation and the changelog on [nebula.awizz.space](https://nebula.awizz.space), next to the demo.

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

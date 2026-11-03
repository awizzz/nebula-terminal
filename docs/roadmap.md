# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, pinned tabs, tabs dropped onto panes, profiles with their own look, SSH connections that reconnect, workspaces that start commands, a quick terminal, images, pane zoom, saved commands with blanks and shortcuts, command blocks, find in every tab, a tab switcher, pane logs, restored output, watched tabs, highlights, mark mode, search in Settings, panes from the keyboard, clickable file paths, shell integration, settings sync, accessibility, Windows on Arm, a profile in Windows Terminal, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (2.8)

- **Pane titles:** in a split tab, each pane can show its shell's title and folder above it.
- **Read-only panes:** lock a pane that shows a log or a dashboard, so a stray key never reaches it.
- **The folder in File Explorer:** open the folder a pane is in from its right-click menu.
- **Back up your settings:** save every setting to a file and bring them back, on this PC or another, without setting up sync.
- **The winget package:** `winget install Awizz.NebulaTerminal`, once Microsoft's reviewers get to it. Scoop already works (see the README).

## Later

- **Signed releases** (Authenticode), once a code-signing certificate fits the project's budget. Until then SmartScreen may warn on first run, and every release lists its SHA-256 checksums.
- A French interface, then more languages contributed by users.

## Exploring

Ideas that need more thought before they get a slot.

- A Linux build of the terminal for people who use both systems.

## Not planned

- Accounts, a cloud service of our own, or telemetry. Nothing leaves your machine except the update check, and your settings if you sync them through a file you chose.
- An extension marketplace.
- Becoming an editor or IDE.

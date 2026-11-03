# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, pinned tabs, profiles with their own look, SSH connections, workspaces that start commands, a quick terminal, images, pane zoom, saved commands with blanks, command blocks, find in every tab, a tab switcher, pane logs, restored output, watched tabs, highlights, clickable file paths, shell integration, settings sync, accessibility, Windows on Arm, a profile in Windows Terminal, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (2.5)

- **Mark mode:** select and copy text with the keyboard alone, as in Windows Terminal.
- **Drop a tab onto a pane:** split that pane with the tab, on the side you drop it.
- **`imgcat` in Nebula:** show an image file right in the terminal.
- **SSH that reconnects:** a dropped connection tries again on its own, with a countdown you can cancel.
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

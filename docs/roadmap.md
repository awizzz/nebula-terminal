# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, pinned tabs, tabs dropped onto panes, profiles with their own look, SSH connections that reconnect or start from the palette, environment variables per profile, closed tabs that come back, workspaces that start commands, a quick terminal, images, pane zoom, saved commands with blanks and shortcuts, command blocks, find in every tab, a tab switcher, pane logs, restored output, watched tabs, highlights, mark mode, tabs on the side, command durations, search in Settings, backups, panes from the keyboard, pane titles, read-only panes, clickable file paths, shell integration, settings sync, accessibility, Windows on Arm, a profile in Windows Terminal, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next

- **The winget package:** `winget install Awizz.NebulaTerminal`, once Microsoft's reviewers get to it. Scoop already works (see the README).
- What you ask for: the list above came from using the terminal every day, and the next items should come from you. Open an issue, or vote on one with a 👍.

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

# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, profiles with their own look, SSH connections, workspaces, a quick terminal, images, pane zoom, saved commands, command blocks, find in every tab, clickable file paths, shell integration, settings sync, accessibility, Windows on Arm, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (2.2)

- **Nebula in Windows Terminal:** one switch adds Nebula to Windows Terminal's profiles, for people who live there.
- **A workspace at startup:** open a saved workspace when the app starts, instead of the last session.
- **Copy with colors:** copy a selection as HTML, to paste output into a document or a chat with its colors.
- **A tab switcher:** jump to any tab by typing a few letters of its name or folder.
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

# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, profiles with their own look, SSH connections, workspaces, a quick terminal, images, pane zoom, saved commands, clickable file paths, shell integration, settings sync, accessibility, Windows on Arm, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (2.1)

Getting more out of what's already on screen.

- **Command blocks:** each command and its output as one unit you can collapse, copy or share.
- **Find in every tab** at once, with the results by tab.
- **Recent folders:** open a tab in a folder you used lately, from the new-tab menu or the palette.
- **Save output:** a tab's text, or one command's output, to a file.
- **The winget package:** `winget install Awizz.NebulaTerminal`, once Microsoft's reviewers get to it. Scoop already works (see the README).

## Later

- **Signed releases** (Authenticode), once a code-signing certificate fits the project's budget. Until then SmartScreen may warn on first run, and every release lists its SHA-256 checksums.
- A French interface, then more languages contributed by users.

## Exploring

Ideas that need more thought before they get a slot.

- Nebula outside the terminal: install `nebula-sh` on its own and add it to Windows Terminal automatically.
- A Linux build of the terminal for people who use both systems.

## Not planned

- Accounts, a cloud service of our own, or telemetry. Nothing leaves your machine except the update check, and your settings if you sync them through a file you chose.
- An extension marketplace.
- Becoming an editor or IDE.

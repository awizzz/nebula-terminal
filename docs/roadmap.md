# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, profiles with their own look, SSH connections, workspaces, a quick terminal, images, pane zoom, saved commands, command blocks, find in every tab, a tab switcher, clickable file paths, shell integration, settings sync, accessibility, Windows on Arm, a profile in Windows Terminal, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (2.3)

- **Bring back what panes showed:** a restored session shows each pane's last output above the new prompt, as Windows Terminal does, instead of an empty screen.
- **Saved commands with blanks:** `ssh {host}` or `git checkout {branch}` asks for the missing parts before it runs.
- **Log a pane to a file:** everything a pane prints, written as plain text to a file you pick until you stop it, for long jobs and support sessions.
- **Watch a tab:** a notification when a quiet tab prints something, or when a busy one has said nothing for a while.
- **The winget package:** `winget install Awizz.NebulaTerminal`, once Microsoft's reviewers get to it. Scoop already works (see the README).

## Later

- **Signed releases** (Authenticode), once a code-signing certificate fits the project's budget. Until then SmartScreen may warn on first run, and every release lists its SHA-256 checksums.
- A French interface, then more languages contributed by users.
- **Pinned tabs:** a tab kept on the left that a stray `Ctrl+Shift+W` doesn't close.
- **Commands in workspaces:** a workspace that also starts `npm run dev` in one pane and `git status` in another.

## Exploring

Ideas that need more thought before they get a slot.

- A Linux build of the terminal for people who use both systems.

## Not planned

- Accounts, a cloud service of our own, or telemetry. Nothing leaves your machine except the update check, and your settings if you sync them through a file you chose.
- An extension marketplace.
- Becoming an editor or IDE.

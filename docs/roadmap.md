# Roadmap

Nebula Terminal covers daily use: shells, scripts, tabs, mixed splits, several windows, pinned tabs, tabs dropped onto panes, profiles with their own look, SSH connections that reconnect or start from the palette, environment variables per profile, closed tabs that come back, workspaces that start commands, a quick terminal, images, pane zoom, saved commands with blanks and shortcuts, command blocks, find in every tab, a tab switcher, pane logs, restored output, watched tabs, highlights, mark mode, tabs on the side, command durations, search in Settings, backups, panes from the keyboard, pane titles, read-only panes, clickable file paths, shell integration, settings sync, accessibility, Windows on Arm, a profile in Windows Terminal, notifications and updates. This is what comes next, roughly in order. Nothing here is a promise or has a date, and ideas move up when people ask for them: open an issue or vote on one with a 👍.

## Next (2.11)

- **Every command, from every shell:** the commands you ran in Nebula, PowerShell, Git Bash and WSL, in one history you search from the palette and run again in the pane in front.
- **Record a pane:** keep what a pane prints with its timing, play it back in a tab, or export it as an asciinema `.cast` file to share a demo or a bug.
- **A portable copy that stays portable:** the portable zip keeps its settings in a folder next to the app, so it can live on a USB stick.
- **Tabs from the keyboard:** `F2` renames the tab in front, and two shortcuts move it left or right.
- **The winget package:** `winget install Awizz.NebulaTerminal`, once Microsoft's reviewers get to it. Scoop already works (see the README).

## Then (2.12)

- **Tab groups:** in the side list, gather tabs under a name and a color, and fold a group away when you don't need it.
- **Icons for profiles:** pick an icon for each custom profile and SSH connection, shown on its tabs and in the new-tab menu.
- **Edit a paste before it runs:** the multi-line paste check lets you change the text, or join it into one line.
- **Finished commands, out loud:** with a screen reader, hear when a command ends and whether it failed.

## Later

- **Signed releases** (Authenticode), once a code-signing certificate fits the project's budget. Until then SmartScreen may warn on first run, and every release lists its SHA-256 checksums.
- A French interface, then more languages contributed by users.

## Exploring

Ideas that need more thought before they get a slot.

- A Linux build of the terminal for people who use both systems.
- **The kitty image protocol**, which yazi and `kitten icat` use, next to Sixel and iTerm2 images.
- **Drop a file on an SSH pane** to upload it with scp into the folder the remote shell is in.
- **Workspaces that find you:** going into a project's folder offers to open the workspace saved for it.
- **Settings in a text file**, for people who would rather edit JSON, kept in step with the Settings window.

## Not planned

- Accounts, a cloud service of our own, or telemetry. Nothing leaves your machine except the update check, and your settings if you sync them through a file you chose.
- An extension marketplace.
- Becoming an editor or IDE.

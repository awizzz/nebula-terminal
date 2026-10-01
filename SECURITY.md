# Security

## Supported versions

Fixes go into the latest release.

## Reporting a vulnerability

Please don't post exploit details, credentials or tokens in a public issue. Use GitHub's private vulnerability reporting for this repository (Security → Report a vulnerability). If that isn't available, open a short issue asking for a private contact without any details.

Include the Nebula Terminal version, the Windows version, how to reproduce the problem and what an attacker could do with it.

## What Nebula Terminal stores

Settings and the tab layout live in the WebView2 profile of the app on your machine. Custom profiles are saved in `custom-profiles.json` in the app's config folder. Nothing is sent anywhere, except the update check: once a day (if enabled), the app asks GitHub's public API for the latest release of this repository. Command history belongs to each shell: Nebula keeps it in `%APPDATA%\Nebula\history.txt`, PowerShell in PSReadLine's file, bash in `.bash_history`. Nebula also runs `~/.nebularc` at startup, so treat that file like any other script.

## Design choices that matter for security

- The interface cannot start arbitrary programs. It asks for a profile by id and the Rust side decides which executable that is.
- Links in terminal output open in your browser only on `Ctrl+Click`, and only for `http` and `https`.
- Pasting several lines asks for confirmation by default, so a copied snippet can't silently run several commands.
- Imported theme files are validated and can only change visual settings.
- Dropped file paths are quoted for the shell that receives them, so a file name can't run a command.
- Custom profiles are stored and validated by the Rust side; the interface still only sends a profile id.
- Updates: Rust picks the installer from the GitHub release itself (the interface can't pass a URL), downloads it over HTTPS and runs it only if it matches the release's `SHA256SUMS.txt`. Releases aren't code-signed yet, so this protects against corrupted downloads, not against a compromised GitHub account.
- Dependencies are locked (`package-lock.json`, `Cargo.lock`) and audited weekly by the Security audit workflow.

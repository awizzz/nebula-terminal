# Security

## Supported versions

Fixes go into the latest release.

## Reporting a vulnerability

Please don't post exploit details, credentials or tokens in a public issue. Use GitHub's private vulnerability reporting for this repository (Security → Report a vulnerability). If that isn't available, open a short issue asking for a private contact without any details.

Include the Nebula Terminal version, the Windows version, how to reproduce the problem and what an attacker could do with it.

## What Nebula Terminal stores

Settings and the tab layout live in the WebView2 profile of the app on your machine. Nothing is sent anywhere. Command history belongs to your shell (PowerShell's PSReadLine, bash's `.bash_history`…), not to Nebula Terminal.

## Design choices that matter for security

- The interface cannot start arbitrary programs. It asks for a profile by id and the Rust side decides which executable that is.
- Links in terminal output open in your browser only on `Ctrl+Click`, and only for `http` and `https`.
- Pasting several lines asks for confirmation by default, so a copied snippet can't silently run several commands.
- Imported theme files are validated and can only change visual settings.
- Dependencies are locked (`package-lock.json`, `Cargo.lock`) and audited weekly by the Security audit workflow.

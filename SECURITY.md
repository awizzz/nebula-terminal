# Security

## Supported versions

Security fixes are applied to the latest released version unless stated otherwise.

## Reporting a vulnerability

Do not publish credentials, private keys, tokens or exploit details in a public issue.

If GitHub private vulnerability reporting is enabled for this repository, use it. Otherwise, open a minimal public issue requesting a private contact method without including reproduction details that would expose users.

Include the affected Nebula version, Windows version, reproduction conditions and expected impact.

## Local data

Nebula stores configuration and optional command history under `%APPDATA%\Nebula` by default.

Command history can contain sensitive command-line arguments. Persistent history can be disabled with:

```text
history off
```

With the default configuration, commands beginning with a space are excluded from persistent history:

```text
 secret-tool --token ...
```

This is not a substitute for secure secret handling. Prefer environment variables, stdin or the appropriate Windows credential mechanism instead of placing credentials directly in command-line arguments.

The history file can be removed from Nebula with:

```text
history clear
```

## Configuration and release integrity

Nebula validates configuration values before saving or loading them. Configuration changes are written through a temporary file and replaced atomically.

Published releases include a SHA-256 checksum and a GitHub Artifact Attestation for the final executable. A recent GitHub CLI can verify that provenance with:

```powershell
gh attestation verify .\Nebula.exe --repo awizzz/custom-shell
```

When the optional signing provider is configured, the release workflow also verifies the Authenticode signature before publishing the executable.

The dependency graph is committed in `Cargo.lock`, normal CI builds use `--locked`, dependency changes are audited before merge, and a scheduled workflow runs `cargo audit` against the RustSec advisory database.

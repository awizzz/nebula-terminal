# Contributing

Nebula is a Windows-focused Rust project. Small, focused pull requests are preferred.

## Development setup

Requirements:

- Windows
- Rust stable
- Cargo

Before opening a pull request, run:

```powershell
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

## Pull requests

- keep unrelated changes in separate pull requests
- explain user-visible behavior changes
- update the changelog for release-worthy changes
- avoid committing generated build output
- do not commit credentials, tokens, certificates or private keys
- keep user-facing strings in the locale files when practical

## Code style

Use `rustfmt` and keep Clippy clean. Prefer small modules with clear ownership over adding more logic to `src/main.rs`.

Windows-specific code belongs in `src/platform.rs` or a dedicated platform module. UI-only code belongs in `src/ui.rs` or a dedicated UI module.

## Issues

Bug reports should include:

- Nebula version
- Windows version
- terminal host
- configured backend
- steps to reproduce
- expected and actual behavior

Remove secrets, usernames and private paths from logs before posting them publicly.

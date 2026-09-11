# Contributing

Nebula is a Windows-focused Rust project. Small, focused pull requests are preferred.

## Development setup

Requirements:

- Windows for runtime testing
- the Rust toolchain declared in `rust-toolchain.toml`
- Cargo

Clone the repository and build with the committed dependency graph:

```powershell
git clone https://github.com/awizzz/custom-shell.git
cd custom-shell
cargo build --locked
```

Before opening a pull request, run:

```powershell
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

These commands match the normal CI pipeline.

## Pull requests

- keep unrelated changes in separate pull requests
- explain user-visible behavior changes
- update `CHANGELOG.md` for release-worthy changes
- avoid committing generated build output under `target/`
- commit `Cargo.lock` when dependency resolution changes
- do not commit credentials, tokens, certificates or private keys
- keep user-facing strings in the locale files
- add or update tests for behavior that can be exercised without an interactive terminal

## Code layout

Use `rustfmt` and keep Clippy clean.

- `src/main.rs` — process entry point only
- `src/shell.rs` — runtime state, built-ins and command dispatch
- `src/editor.rs` — Reedline, prompt, completion and history
- `src/config.rs` — configuration schema, validation and theme presets
- `src/platform.rs` — Windows/platform integration
- `src/i18n.rs` — localization
- `src/ui.rs` — terminal presentation

Avoid growing `src/main.rs` back into a general-purpose module. Platform-specific behavior belongs behind `platform.rs` or a dedicated platform module.

## Localization

Built-in locale files must expose the same keys. The test suite checks this automatically.

When adding user-facing text, add the corresponding English and French entries under `locales/` rather than embedding presentation text in command logic.

## Issues

Bug reports should include the Nebula version, Windows version, terminal host, configured backend, reproduction steps, expected behavior and actual behavior.

Remove secrets, usernames and private paths from logs before posting them publicly.

# Contributing

Thanks for helping out. Small, focused pull requests are easiest to review.

## Setup

You need Windows, Node.js 24, Rust 1.98.1 (installed automatically from `rust-toolchain.toml` by rustup), the MSVC build tools and WebView2.

```powershell
npm ci
npm run tauri dev
```

For interface-only work, `npm run dev` serves the UI in a browser with a recorded session, and you don't need Rust for it.

Working on the interpreter alone is quickest with `cargo run -p nebula-sh` in any terminal. `cargo test -p nebula-sh` runs its unit and end-to-end tests.

## Before opening a pull request

These are the checks CI runs:

```powershell
npm run build                     # type-check and bundle the UI
npm test                          # frontend unit tests
npm run sidecar                   # build nebula-sh; Tauri needs it before any cargo check of src-tauri
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

## Guidelines

- Explain user-visible changes in the pull request, and add a line to `CHANGELOG.md` under `Unreleased`.
- Use the existing tokens in `src/styles.css` for colors, spacing and motion. Don't hard-code colors in components.
- Every new action should get a command palette entry, and a shortcut if it's frequent.
- Anything read from storage or from a file must be validated (see `sanitizePreferences`).
- Don't add Tauri commands that run arbitrary programs. New shells are new profile ids resolved in `profiles.rs`; programs the user picks go through custom profiles, which Rust validates and stores (`custom.rs`).
- Nebula commands should behave like their GNU counterparts. When output goes to a pipe or a file, print plain text (no colors, no icons).
- New icons go in `crates/nebula-sh/src/icons.rs`; then rerun `scripts/subset-icons.py`.
- Test behavior that can run without a real terminal (Rust unit tests, `*.test.ts`).
- Commit lockfiles when dependencies change. Never commit build output, credentials or signing material.

See [docs/architecture.md](docs/architecture.md) for a map of the code.

## License of contributions

Nebula Terminal is source-available under the [PolyForm Shield License](LICENSE). By opening a pull request, you agree that your contribution is licensed under the same terms, and you allow the maintainer to use, change and relicense it as part of Nebula Terminal. Only send code you wrote or have the right to give under these terms.

## Reporting bugs

Use the bug report template. Include your Nebula Terminal version (Settings → About), your Windows version, the shell, and the steps to reproduce. Remove usernames, tokens and private paths from anything you paste.

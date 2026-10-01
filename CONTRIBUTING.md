# Contributing

Thanks for helping out. Small, focused pull requests are easiest to review.

## Setup

You need Windows, Node.js 24, Rust 1.98.1 (installed automatically from `rust-toolchain.toml` by rustup), the MSVC build tools and WebView2.

```powershell
npm ci
npm run tauri dev
```

For interface-only work, `npm run dev` serves the UI in a browser with a fake session, and you don't need Rust for it.

## Before opening a pull request

These are the checks CI runs:

```powershell
npm run build                     # type-check and bundle the UI
npm test                          # frontend unit tests
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## Guidelines

- Explain user-visible changes in the pull request, and add a line to `CHANGELOG.md` under `Unreleased`.
- Use the existing tokens in `src/styles.css` for colors, spacing and motion. Don't hard-code colors in components.
- Every new action should get a command palette entry, and a shortcut if it's frequent.
- Anything read from storage or from a file must be validated (see `sanitizePreferences`).
- Don't add Tauri commands that run arbitrary programs. New shells are new profile ids resolved in `profiles.rs`.
- Test behavior that can run without a real terminal (Rust unit tests, `*.test.ts`).
- Commit lockfiles when dependencies change. Never commit build output, credentials or signing material.

See [docs/architecture.md](docs/architecture.md) for a map of the code.

## Reporting bugs

Use the bug report template. Include your Nebula Terminal version (Settings → About), your Windows version, the shell, and the steps to reproduce. Remove usernames, tokens and private paths from anything you paste.

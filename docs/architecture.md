# Architecture

Nebula is a Windows shell frontend. It owns the interactive editor and selected shell state, while normal commands are delegated to a configured Windows command backend.

## Modules

`src/main.rs` is intentionally small. It only starts the shell and handles a fatal startup error.

`src/shell.rs` owns the interactive runtime, built-in command dispatch and persistent Nebula state such as the working directory, drive locations, aliases and environment variables.

`src/editor.rs` owns Reedline integration, history setup, Tab completion, prompt rendering, Git branch display and terminal-title updates.

`src/config.rs` defines the configuration schema, validation, defaults and theme presets. Configuration is stored in `%APPDATA%\Nebula\config.toml` and saved through atomic file replacement.

`src/i18n.rs` selects built-in or user-provided locale files. Tests ensure the built-in English and French locale files expose the same keys.

`src/platform.rs` isolates Windows integration such as UAC elevation, Windows UI-language detection, local time, environment expansion and atomic file replacement.

`src/ui.rs` contains terminal presentation code, theme previews, status output and optional animations.

## Runtime model

The main loop follows this sequence:

```text
read input
   ↓
expand Nebula aliases
   ↓
handle Nebula built-in command
   or
launch selected backend for an external command
   ↓
record status and duration
   ↓
render the next prompt
```

Built-in commands such as `cd`, `set`, `theme`, `alias`, `history` and `language` are handled by Nebula so their state can persist across commands.

External commands are currently forwarded to one of:

```text
cmd.exe
powershell.exe
pwsh.exe
```

Each external command launches a fresh backend process. This preserves broad compatibility with normal command syntax but means backend-specific process state does not persist. For example, a PowerShell variable or function created by one external command is not available to the next external command.

A future persistent-backend implementation should use a Windows pseudoconsole/ConPTY boundary rather than mixing terminal emulation into the current shell runtime.

## Shell state

Nebula currently persists these values for the lifetime of the process:

- current working directory
- previous working directory for `cd -`
- `pushd` / `popd` directory stack
- last known working directory for each Windows drive
- environment variables changed with `set`
- last exit code and command duration

Aliases and user configuration are persisted to disk.

## Local data

Default data directory:

```text
%APPDATA%\Nebula
```

Files and directories can include:

```text
config.toml
history.txt
locales\
```

Persistent history is optional. With the default configuration, commands beginning with a space are excluded from the history file.

Nebula does not install a service and does not require a background process.

## Reliability boundaries

Configuration is validated before being accepted or saved. Invalid backends, invalid theme colors and out-of-range UI/history settings are rejected.

The project pins a Rust toolchain in `rust-toolchain.toml` and commits `Cargo.lock`. CI uses Cargo's `--locked` mode so dependency resolution cannot silently change during a build.

## CI and releases

`.github/workflows/ci.yml` validates normal pushes and pull requests with read-only repository permissions.

`.github/workflows/security.yml` runs a scheduled RustSec dependency audit.

`.github/workflows/release.yml` is the only workflow intended to publish release assets. It runs the same validation, optionally signs the executable, verifies the signature, creates a SHA-256 checksum and publishes the GitHub Release.


## Native execution engine

Starting with 0.5.0, `native` is the default backend. Nebula parses command chains itself, launches ordinary executables directly with the Windows process model, and implements core pipelines, conditional execution and redirection without invoking CMD or PowerShell.

CMD, Windows PowerShell and PowerShell 7 are compatibility layers, not runtime dependencies of the native engine. Users can call them explicitly (`cmd <command>`, `powershell <command>`, `pwsh <command>`) or select one as the session backend. Batch files remain a CMD format and therefore require CMD compatibility.

Native pipelines currently buffer one stage before feeding the next. Streaming pipelines and richer job control remain future work.

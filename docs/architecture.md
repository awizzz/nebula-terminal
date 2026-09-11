# Architecture

Nebula is a shell frontend. It owns the interactive prompt and selected shell state, then delegates normal command execution to a configured Windows shell backend.

## Runtime model

`src/main.rs` currently owns the read-eval loop, command dispatch, prompt construction, completion setup and most built-in commands.

`src/config.rs` loads and saves `%APPDATA%\Nebula\config.toml` and contains theme presets.

`src/i18n.rs` selects built-in or user-provided locale files.

`src/platform.rs` contains Windows integration such as UAC elevation, UI-language detection and local time.

`src/ui.rs` contains terminal presentation code, theme previews and animations.

## Command execution

Built-in commands such as `cd`, `set`, `theme` and `language` are handled by Nebula so their state can persist.

Other input is forwarded to one of these backends:

- `cmd.exe`
- `powershell.exe`
- `pwsh.exe`

Each forwarded command currently runs in a child process. Backend-specific process state therefore does not persist between commands.

## Data

Default data directory:

```text
%APPDATA%\Nebula
```

Current files include:

```text
config.toml
history.txt
locales\
```

Nebula does not require a service or background process.

## Refactoring direction

`src/main.rs` should continue to shrink as the project grows. Natural module boundaries are command dispatch, prompt rendering, completion, history and backend process management.

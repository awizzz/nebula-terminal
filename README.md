# Nebula

Nebula is a native, customizable shell frontend for Windows. It keeps compatibility with normal Windows command workflows while adding a modern prompt, persistent history, completion, themes, localization and built-in elevation.

## What it does

- Runs as a standalone `Nebula.exe` — Python is not required.
- Sends normal commands to `cmd.exe` by default, including pipes, redirects, `.bat`/`.cmd`, Git, Python, SSH, Winget and Windows utilities.
- Can use `powershell` or `pwsh` as the command backend instead.
- Keeps shell state for `cd`, `pushd`, `popd` and `set`.
- Persistent history with arrow-key navigation and history hints.
- Tab completion for Nebula commands and executables found in `PATH`.
- Native UAC elevation with `admin` and one-command elevation with `sudo <command>`.
- Clearly marks elevated sessions as `ADMIN`.
- Detects the Windows UI locale when `language = "auto"`.
- Built-in English and French, with external locale overrides for additional languages.
- Fully editable colors, prompt templates, symbols, aliases and environment variables.

## Download / build

Every successful GitHub Actions build produces a `Nebula-windows-x64` artifact containing `Nebula.exe`.

Tagged builds such as `v0.2.0` also create a GitHub Release automatically.

To build locally:

```powershell
cargo build --release
```

The executable is written to:

```text
target\release\nebula.exe
```

## Configuration

Nebula creates its configuration on first launch:

```text
%APPDATA%\Nebula\config.toml
```

Open it from Nebula with:

```text
config
```

Reload it without restarting:

```text
reload
```

### Language

The default is:

```toml
[general]
language = "auto"
```

`auto` uses the Windows user locale. You can override it from the shell:

```text
language fr-FR
language en-US
language auto
```

Additional translations can be placed in:

```text
%APPDATA%\Nebula\locales\<locale>.toml
```

A custom locale inherits the built-in fallback and overrides any keys it defines.

### Themes

Built-in presets:

```text
hypr
tokyo-night
catppuccin
nord
dracula
```

Change one live:

```text
theme tokyo-night
```

Every color is also directly editable in `config.toml` using `#RRGGBB` values.

### Prompt

The prompt is template-based. The default template supports:

```text
{status}
{identity}
{cwd}
{git}
{duration}
{exit}
{indicator}
```

Example:

```toml
[prompt]
template = "╭─ {status} {identity}{cwd}{git}{duration}{exit}"
indicator_line = "╰─{indicator} "
indicator = "❯"
show_git = true
show_duration = true
show_exit_code = true
show_user = false
show_hostname = false
```

### Aliases

```toml
[aliases]
ll = "dir"
gs = "git status"
gl = "git log --oneline --decorate"
```

### Environment variables

Values in this section are applied whenever Nebula starts or reloads its configuration:

```toml
[env]
EDITOR = "code"
```

## Administrator mode

Start a new elevated Nebula session:

```text
admin
```

Or start the executable elevated from outside Nebula:

```text
Nebula.exe --admin
```

Run only one command elevated:

```text
sudo sfc /scannow
```

## Development

The current native rewrite is written in Rust. `reedline` provides the interactive line editor, completion and history layer; the Windows-specific locale and UAC integration use native Win32 APIs.

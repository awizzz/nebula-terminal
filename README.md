# Nebula

Nebula is a native, customizable shell frontend for Windows. It keeps normal Windows command compatibility while adding a modern prompt, history, completion, themes, localization, diagnostics and built-in UAC elevation.

## Highlights

- Standalone `Nebula.exe` — Python is not required.
- Normal commands are executed through `cmd.exe` by default, including pipes, redirects, `.bat` / `.cmd`, Git, Python, SSH, Winget and Windows utilities.
- Optional `powershell` and `pwsh` backends.
- Persistent shell state for `cd`, `pushd`, `popd`, `set` and drive switching (`D:`, `E:`, ...).
- Persistent history with arrow-key navigation and history hints.
- Tab completion for Nebula commands, aliases, executables in `PATH`, and files/folders in the current directory.
- Native UAC elevation through `admin` and one-command elevation through `sudo <command>`.
- Elevated sessions are clearly marked as `ADMIN` in the prompt and window title.
- Windows display-language detection when `language = "auto"`.
- Built-in French and English plus user-provided locale overrides.
- Animated UI that can be disabled completely.
- Multiple banner layouts, theme previews and an optional right-side clock.
- Runtime diagnostics with `doctor`.
- Fully editable prompt, RGB colors, aliases, environment variables and interface settings.

## Interface

The default `aurora` banner is designed to look clean in Windows Terminal without requiring a Nerd Font. Other layouts are available:

```text
ui banner aurora
ui banner minimal
ui banner compact
ui banner off
```

Animations are intentionally short and optional:

```text
ui animations on
ui animations off
ui speed 28
```

Other UI controls:

```text
ui tips on
ui separator on
ui demo
ui reset
```

Run `ui` to see the current interface settings.

## Themes

Run:

```text
theme
```

Nebula displays a color preview of every built-in theme.

Available presets:

```text
hypr
tokyo-night
catppuccin
nord
dracula
rose-pine
gruvbox
```

Apply one live:

```text
theme tokyo-night
```

Every color can also be changed manually in `config.toml`.

## Useful commands

```text
help
about
doctor
config
reload
language
backend
ui
theme
alias
history
admin
sudo <command>
```

Examples:

```text
backend pwsh
language fr-FR
alias gst=git status
alias rm gst
history clear
doctor
```

## Downloads and releases

Every successful Windows CI build produces a `Nebula-windows-x64` GitHub Actions artifact containing `Nebula.exe`.

Version tags (`v*`) create a real GitHub Release containing:

```text
Nebula.exe
Nebula.exe.sha256
```

Code signing is optional for now:

- when SignPath is configured, the exact CI artifact is signed and its Authenticode signature is verified before release;
- when SignPath is not configured, the release is still published and is clearly titled `(unsigned)`.

This means development is not blocked by the signing provider. Signing can be enabled later without changing the release format.

The signing setup is documented in `.github/workflows/RELEASE-SIGNING.md`.

To build locally:

```powershell
cargo build --release
```

Output:

```text
target\release\nebula.exe
```

## Configuration

Nebula creates its configuration on first launch:

```text
%APPDATA%\Nebula\config.toml
```

Open it with:

```text
config
```

Reload it without restarting:

```text
reload
```

A complete example is available in `config.example.toml`.

### Language

Default:

```toml
[general]
language = "auto"
```

`auto` follows the Windows user-interface language.

You can override it live:

```text
language fr-FR
language en-US
language auto
```

Additional translations can be placed in:

```text
%APPDATA%\Nebula\locales\<locale>.toml
```

A custom locale inherits the built-in fallback and overrides only the keys it defines.

### Backend

```text
backend cmd
backend powershell
backend pwsh
```

Nebula checks that the requested backend exists before saving it.

### Prompt

The default prompt is template-based:

```toml
[prompt]
template = "╭─ {status} {identity}{cwd}{git}{duration}{exit}"
indicator_line = "╰─{indicator} "
indicator = "❯"
multiline_indicator = "· "
show_git = true
show_duration = true
show_exit_code = true
show_user = false
show_hostname = false
show_time = false
```

`show_time = true` enables a right-side clock using the native Windows local time.

### Aliases

Aliases can be edited in TOML or from Nebula itself:

```text
alias ll=dir
alias gs=git status
alias rm gs
```

### Environment variables

```toml
[env]
EDITOR = "code"
```

Configured variables are applied whenever Nebula starts or reloads its configuration.

## Administrator mode

Start a new elevated Nebula session:

```text
admin
```

Or launch the executable elevated:

```text
Nebula.exe --admin
```

Run a single command elevated:

```text
sudo sfc /scannow
```

## Development

Nebula is written in Rust. `reedline` provides interactive line editing, history and completion. Windows locale, local time and UAC integration use native Win32 APIs. The presentation layer is kept in `src/ui.rs` so visual changes do not need to be mixed into command execution logic.

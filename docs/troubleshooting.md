# Troubleshooting

## Windows SmartScreen blocks the executable

Unsigned releases can trigger SmartScreen because Windows cannot verify a trusted publisher.

Before running an unsigned build, download `Nebula.exe.sha256` from the same GitHub Release and compare it with:

```powershell
Get-FileHash .\Nebula.exe -Algorithm SHA256
```

Do not bypass a warning for a file whose checksum does not match the published release asset.

## The prompt has broken colors or box characters

Use Windows Terminal or another terminal host with ANSI/VT and Unicode support.

You can reduce presentation features with:

```text
ui animations off
ui banner minimal
```

Or disable the banner:

```text
ui banner off
```

## A PowerShell variable disappears after the next command

This is currently expected. Nebula launches each external backend command in a fresh child process, so backend-specific state such as PowerShell variables, functions and imported modules does not persist between commands.

Nebula-owned state such as its current directory, aliases and environment variables changed with `set` does persist.

## `backend pwsh` is unavailable

PowerShell 7 uses `pwsh.exe` and is separate from the Windows PowerShell installation included with Windows.

Run:

```text
doctor
```

or select another installed backend:

```text
backend cmd
backend powershell
```

## Configuration no longer loads

Validate the file:

```text
config check
```

The configuration path is available with:

```text
config path
```

If necessary, move the invalid `config.toml` out of `%APPDATA%\Nebula` and start Nebula again to create a default configuration. Keep the old file if you need to copy custom aliases or colors back afterward.

## History contains something sensitive

Clear persistent history with:

```text
history clear
```

Disable it entirely with:

```text
history off
```

With the default configuration, starting a command with a space prevents that command from being persisted. This does not remove copies created by the invoked program, Windows, logs or other shells.

## `sudo` opens another window

Nebula's current UAC helper launches an elevated backend process in a separate window. It is not the same execution model as Unix `sudo`.

The selected backend is respected, so `backend pwsh` followed by `sudo <command>` launches the elevated command through PowerShell 7.

## A drive switch opens the wrong folder

Nebula remembers the latest folder visited on each drive during the current Nebula session. For example, after visiting `D:\Projects`, switching away and entering `D:` later returns to that remembered directory.

If a drive has not been visited during the session, its root is used.

## Collecting diagnostics for a bug report

Run:

```text
about
doctor
```

Include the Nebula version, Windows version, terminal host, backend and minimal reproduction steps. Remove usernames, tokens, private paths and other sensitive information before posting logs publicly.

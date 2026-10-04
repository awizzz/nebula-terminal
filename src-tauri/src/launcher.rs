//! The ways Windows starts the app in a folder: "Open in Nebula Terminal" in File
//! Explorer, the `nebula-terminal` command, and the folder argument they both pass.

use crate::updater::{install_kind, InstallKind};
use serde::Serialize;
use std::{
    env,
    path::{Path, PathBuf},
};

/// The folder in a command line, if it names one that exists. Relative paths are read
/// from `cwd`, the folder the command was typed in.
pub fn folder_argument(args: &[String], cwd: &Path) -> Option<PathBuf> {
    let argument = args.iter().skip(1).find(|arg| !arg.starts_with('-'))?;
    // Explorer's "%V" turns a drive root into `C:"`: the backslash escapes the quote.
    let mut folder = argument.trim().trim_end_matches('"').to_owned();
    if folder.len() == 2 && folder.ends_with(':') {
        folder.push('\\');
    }
    let path = PathBuf::from(folder);
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    path.is_dir().then_some(path)
}

/// The folder this copy was started with, for its first tab.
#[tauri::command]
pub fn launch_folder() -> Option<String> {
    let args: Vec<String> = env::args().collect();
    let cwd = env::current_dir().unwrap_or_default();
    folder_argument(&args, &cwd).map(|path| path.display().to_string())
}

/// Where the `nebula-terminal` command lives. It gets a folder of its own because the
/// app's folder also holds `uninstall.exe`, which must never be a command.
fn command_directory() -> Option<PathBuf> {
    let local = env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("dev.awizz.nebula-terminal")
            .join("bin"),
    )
}

/// For cmd, PowerShell and Nebula. `start` returns at once instead of waiting for the app.
fn cmd_shim(exe: &Path) -> String {
    let exe = exe.display().to_string().replace('%', "%%");
    format!("@echo off\r\nrem Opens a Nebula Terminal tab, in the folder given if there is one.\r\nstart \"\" \"{exe}\" %*\r\n")
}

/// For Git Bash and WSL, which don't look at `.cmd` files. Same layout as npm's shims.
fn sh_shim(exe: &Path) -> String {
    let exe = exe.display().to_string().replace('\'', r"'\''");
    format!(
        r#"#!/bin/sh
# Opens a Nebula Terminal tab, in the folder given if there is one.
exe='{exe}'
if command -v wslpath >/dev/null 2>&1; then convert=wslpath; else convert=cygpath; fi
if [ $# -gt 0 ]; then
  "$($convert -u "$exe")" "$($convert -w "$1")" >/dev/null 2>&1 &
else
  "$($convert -u "$exe")" >/dev/null 2>&1 &
fi
"#
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Launchers {
    explorer_menu: bool,
    command: bool,
}

/// Adds or removes the Explorer entry and the command, and says which are there. Each
/// value is the user's choice, or `None` if they never made one: an installed copy
/// then adds both, and a portable copy leaves the system alone.
#[tauri::command]
pub fn sync_launchers(
    explorer_menu: Option<bool>,
    command: Option<bool>,
) -> Result<Launchers, String> {
    let installed = install_kind() != InstallKind::Manual;
    let exe = env::current_exe().map_err(|error| error.to_string())?;
    let launchers = Launchers {
        explorer_menu: explorer_menu.unwrap_or(installed),
        command: command.unwrap_or(installed),
    };
    if explorer_menu.is_some() || installed {
        system::sync_explorer_menu(&exe, launchers.explorer_menu)
            .map_err(|error| format!("Could not update the File Explorer menu: {error}"))?;
    }
    if command.is_some() || installed {
        let directory = command_directory().ok_or("LOCALAPPDATA is not set.")?;
        sync_command(&directory, &exe, launchers.command)
            .map_err(|error| format!("Could not update the nebula-terminal command: {error}"))?;
    }
    Ok(launchers)
}

fn sync_command(directory: &Path, exe: &Path, enabled: bool) -> std::io::Result<()> {
    sync_shims(directory, exe, enabled)?;
    system::sync_user_path(directory, enabled)
}

fn sync_shims(directory: &Path, exe: &Path, enabled: bool) -> std::io::Result<()> {
    if enabled {
        std::fs::create_dir_all(directory)?;
        write_if_changed(&directory.join("nebula-terminal.cmd"), &cmd_shim(exe))?;
        write_if_changed(&directory.join("nebula-terminal"), &sh_shim(exe))
    } else if directory.exists() {
        std::fs::remove_dir_all(directory)
    } else {
        Ok(())
    }
}

fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(());
    }
    std::fs::write(path, content)
}

/// `nebula-terminal --uninstall`, run by the installers: removes what the app added
/// to Windows, without starting the app.
pub fn uninstall() {
    if let Ok(exe) = env::current_exe() {
        let _ = system::sync_explorer_menu(&exe, false);
    }
    if let Some(directory) = command_directory() {
        let _ = std::fs::remove_dir_all(&directory);
        let _ = system::sync_user_path(&directory, false);
    }
}

/// Entries in a `;`-separated PATH, with `folder` added once or removed everywhere.
/// Everything else is kept as it was, empty entries included.
fn edit_path(path: &str, folder: &str, present: bool) -> Option<String> {
    let same = |entry: &str| {
        entry.trim().trim_end_matches('\\').to_lowercase()
            == folder.trim_end_matches('\\').to_lowercase()
    };
    let found = path.split(';').any(same);
    match (present, found) {
        (true, false) if path.trim().is_empty() => Some(folder.to_owned()),
        (true, false) => Some(format!("{};{folder}", path.trim_end_matches(';'))),
        (false, true) => Some(
            path.split(';')
                .filter(|entry| !same(entry))
                .collect::<Vec<_>>()
                .join(";"),
        ),
        _ => None,
    }
}

#[cfg(windows)]
mod system {
    use std::{io, path::Path};
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ},
        RegKey, RegValue,
    };

    const LABEL: &str = "Open in Nebula Terminal";
    /// A folder, the empty space inside a folder, and a drive. The installers remove
    /// the same keys when the app is uninstalled.
    const MENU_KEYS: [&str; 3] = [
        r"Software\Classes\Directory\shell\NebulaTerminal",
        r"Software\Classes\Directory\Background\shell\NebulaTerminal",
        r"Software\Classes\Drive\shell\NebulaTerminal",
    ];

    fn menu_command(exe: &Path) -> String {
        format!("\"{}\" \"%V\"", exe.display())
    }

    pub fn sync_explorer_menu(exe: &Path, enabled: bool) -> io::Result<()> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        if !enabled {
            for key in MENU_KEYS {
                match root.delete_subkey_all(key) {
                    Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                    _ => {}
                }
            }
            return Ok(());
        }
        let command = menu_command(exe);
        let current = MENU_KEYS.iter().all(|key| {
            root.open_subkey(format!(r"{key}\command"))
                .and_then(|entry| entry.get_value::<String, _>(""))
                .is_ok_and(|value| value == command)
        });
        if current {
            return Ok(());
        }
        for key in MENU_KEYS {
            let (entry, _) = root.create_subkey(key)?;
            entry.set_value("", &LABEL)?;
            entry.set_value("Icon", &format!("\"{}\",0", exe.display()))?;
            let (command_key, _) = entry.create_subkey("command")?;
            command_key.set_value("", &command)?;
        }
        Ok(())
    }

    fn decode(value: &RegValue) -> String {
        let units: Vec<u16> = value
            .bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_owned()
    }

    /// Adds or removes `folder` in the user's PATH (`HKCU\Environment`), keeping the
    /// value's type so `%VARIABLES%` in it still expand, and tells Windows it changed.
    pub fn sync_user_path(folder: &Path, present: bool) -> io::Result<()> {
        let environment = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;
        let (path, vtype) = match environment.get_raw_value("Path") {
            Ok(value) => (decode(&value), value.vtype),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (String::new(), REG_EXPAND_SZ),
            Err(error) => return Err(error),
        };
        let Some(updated) = super::edit_path(&path, &folder.display().to_string(), present) else {
            return Ok(());
        };
        let bytes: Vec<u8> = updated
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        environment.set_raw_value(
            "Path",
            &RegValue {
                bytes: bytes.into(),
                vtype,
            },
        )?;
        announce_environment_change();
        Ok(())
    }

    /// New shells started from Explorer read the new PATH without signing out.
    fn announce_environment_change() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let area: Vec<u16> = "Environment\0".encode_utf16().collect();
        // SAFETY: `area` is a NUL-terminated UTF-16 string that outlives the call, and
        // the result pointer may be null.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                area.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                2000,
                std::ptr::null_mut(),
            );
        }
    }
}

#[cfg(not(windows))]
mod system {
    use std::{io, path::Path};

    pub fn sync_explorer_menu(_exe: &Path, _enabled: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn sync_user_path(_folder: &Path, _present: bool) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("nebula-terminal.exe")
            .chain(list.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn reads_the_folder_argument() {
        let base = env::temp_dir().join(format!("nebula-launch-{}", std::process::id()));
        fs::create_dir_all(base.join("project")).unwrap();
        let absolute = base.join("project");

        assert_eq!(
            folder_argument(&args(&[&absolute.display().to_string()]), Path::new("/")),
            Some(absolute.clone())
        );
        assert_eq!(
            folder_argument(&args(&["--flag", "project"]), &base),
            Some(base.join("project"))
        );
        assert_eq!(folder_argument(&args(&["missing"]), &base), None);
        assert_eq!(folder_argument(&args(&[]), &base), None);
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(windows)]
    #[test]
    fn repairs_a_drive_root_from_explorer() {
        let drive = env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        assert_eq!(
            folder_argument(&args(&[&format!("{drive}\"")]), Path::new(r"C:\")),
            Some(PathBuf::from(format!("{drive}\\")))
        );
    }

    #[test]
    fn adds_and_removes_one_path_entry() {
        let folder = r"C:\Users\me\AppData\Local\dev.awizz.nebula-terminal\bin";
        assert_eq!(edit_path("", folder, true).as_deref(), Some(folder));
        assert_eq!(
            edit_path(r"C:\tools;", folder, true),
            Some(format!(r"C:\tools;{folder}"))
        );
        assert_eq!(
            edit_path(&format!(r"C:\tools;{folder}\"), folder, true),
            None
        );
        assert_eq!(
            edit_path(
                &format!(r"%USERPROFILE%\bin;;{}\;C:\tools", folder.to_uppercase()),
                folder,
                false
            )
            .as_deref(),
            Some(r"%USERPROFILE%\bin;;C:\tools")
        );
        assert_eq!(edit_path(r"C:\tools", folder, false), None);
    }

    #[test]
    fn shims_quote_the_app_path_for_each_shell() {
        let exe = Path::new(r"C:\Users\O'Brien 100%\Nebula Terminal\nebula-terminal.exe");
        assert!(cmd_shim(exe).contains(
            r#"start "" "C:\Users\O'Brien 100%%\Nebula Terminal\nebula-terminal.exe" %*"#
        ));
        assert!(sh_shim(exe)
            .contains(r"exe='C:\Users\O'\''Brien 100%\Nebula Terminal\nebula-terminal.exe'"));
        assert!(!sh_shim(exe).contains('\r'));
    }

    #[test]
    fn the_command_folder_holds_only_the_shims() {
        let directory = env::temp_dir().join(format!("nebula-command-{}", std::process::id()));
        let exe = Path::new(r"C:\Nebula Terminal\nebula-terminal.exe");
        sync_shims(&directory, exe, true).unwrap();
        let mut names: Vec<String> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["nebula-terminal", "nebula-terminal.cmd"]);
        sync_shims(&directory, exe, false).unwrap();
        assert!(!directory.exists());
    }
}

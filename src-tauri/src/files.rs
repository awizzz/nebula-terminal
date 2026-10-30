//! Paths printed in a terminal: which of them exist, and opening one at its line in an
//! editor. Ctrl+Click on a path lands here.
//!
//! Only local paths count: drives and WSL's own shares. Other network paths are never
//! touched, because an unreachable server would stall the check.

use crate::{cmdline, profiles};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tauri::AppHandle;

const MAX_CANDIDATES: usize = 20;
const MAX_LENGTH: usize = 512;

/// Files that would run if the default app "opened" them. Those are shown in File
/// Explorer instead: a path printed by a program is no reason to start it.
const RUNNABLE: &[&str] = &[
    "exe",
    "com",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "msi",
    "msp",
    "scr",
    "pif",
    "lnk",
    "url",
    "hta",
    "cpl",
    "msc",
    "jar",
    "reg",
    "appx",
    "msix",
    "application",
    "gadget",
];

/// `\\wsl.localhost\Ubuntu` or `\\wsl$\Ubuntu`, the share of one distribution.
fn wsl_share(path: &str) -> Option<&str> {
    let rest = ["\\\\wsl.localhost\\", "\\\\wsl$\\"]
        .iter()
        .find_map(|prefix| {
            path.get(..prefix.len())
                .filter(|start| start.eq_ignore_ascii_case(prefix))
                .map(|_| prefix.len())
        })?;
    let distro_end = path[rest..]
        .find('\\')
        .map_or(path.len(), |index| rest + index);
    (distro_end > rest).then(|| &path[..distro_end])
}

fn is_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
}

/// Drops `.` and `..` from a Windows path, never climbing above its drive or share.
fn normalize(path: &str) -> Option<String> {
    let root = if is_drive_path(path) {
        path[..2].to_owned()
    } else {
        wsl_share(path)?.to_owned()
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in path[root.len()..].split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    Some(format!("{root}\\{}", parts.join("\\")))
}

/// A path as a shell printed it, read from `cwd` (a drive folder or a WSL share), with
/// `/c/Users` (Git Bash), `~` and, in WSL, `/home/me` understood. None for anything
/// that isn't local.
pub fn resolve(cwd: Option<&str>, home: Option<&str>, text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_LENGTH || text.chars().any(|c| c.is_control()) {
        return None;
    }
    let windows = text.replace('/', "\\");
    let bytes = text.as_bytes();
    let absolute = if is_drive_path(&windows) {
        windows
    } else if bytes.len() >= 2
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && (bytes.len() == 2 || bytes[2] == b'/')
    {
        format!(
            "{}:\\{}",
            (bytes[1] as char).to_ascii_uppercase(),
            windows.get(3..).unwrap_or("")
        )
    } else if text == "~" || text.starts_with("~/") || text.starts_with("~\\") {
        format!("{}\\{}", home?, &windows[1..])
    } else if windows.starts_with("\\\\") {
        wsl_share(&windows)?;
        windows
    } else if windows.starts_with('\\') {
        // An absolute Linux path means something only inside a WSL distribution.
        format!("{}{windows}", wsl_share(cwd?)?)
    } else {
        let cwd = cwd?;
        if !is_drive_path(cwd) && wsl_share(cwd).is_none() {
            return None;
        }
        format!("{}\\{windows}", cwd.trim_end_matches('\\'))
    };
    normalize(&absolute)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundPath {
    index: usize,
    path: String,
    directory: bool,
}

/// The candidates that exist, with their full path.
#[tauri::command]
pub fn find_paths(cwd: Option<String>, candidates: Vec<String>) -> Vec<FoundPath> {
    let home = profiles::home_directory().map(|path| path.display().to_string());
    candidates
        .iter()
        .take(MAX_CANDIDATES)
        .enumerate()
        .filter_map(|(index, text)| {
            let path = resolve(cwd.as_deref(), home.as_deref(), text)?;
            let metadata = std::fs::metadata(&path).ok()?;
            Some(FoundPath {
                index,
                path,
                directory: metadata.is_dir(),
            })
        })
        .collect()
}

fn program_file(folder: &str, parts: &[&str]) -> Option<PathBuf> {
    let mut path = PathBuf::from(std::env::var_os(folder)?);
    path.extend(parts);
    path.is_file().then_some(path)
}

fn vscode() -> Option<PathBuf> {
    program_file(
        "LOCALAPPDATA",
        &["Programs", "Microsoft VS Code", "Code.exe"],
    )
    .or_else(|| program_file("ProgramFiles", &["Microsoft VS Code", "Code.exe"]))
    // `code` on PATH is `<install>\bin\code.cmd`.
    .or_else(|| {
        let script = profiles::find_executable("code")?;
        let exe = script.parent()?.parent()?.join("Code.exe");
        exe.is_file().then_some(exe)
    })
}

fn cursor() -> Option<PathBuf> {
    program_file("LOCALAPPDATA", &["Programs", "cursor", "Cursor.exe"])
}

/// `-g file:line:column`, which VS Code and Cursor both read.
fn goto(path: &str, line: Option<u32>, column: Option<u32>) -> Vec<String> {
    match (line, column) {
        (Some(line), Some(column)) => vec!["-g".into(), format!("{path}:{line}:{column}")],
        (Some(line), None) => vec!["-g".into(), format!("{path}:{line}")],
        _ => vec![path.to_owned()],
    }
}

/// A command the user typed, with `{file}`, `{line}` and `{column}` put into its
/// arguments one by one, so a path can never turn into more arguments or a shell
/// command. Without `{file}`, the file goes last.
pub fn custom_command(
    template: &str,
    path: &str,
    line: Option<u32>,
    column: Option<u32>,
) -> Option<(String, Vec<String>)> {
    let mut parts = cmdline::split(template);
    if parts.is_empty() {
        return None;
    }
    let program = parts.remove(0);
    let line = line.unwrap_or(1).to_string();
    let column = column.unwrap_or(1).to_string();
    let mut has_file = false;
    let mut args: Vec<String> = parts
        .iter()
        .map(|arg| {
            has_file |= arg.contains("{file}");
            arg.replace("{file}", path)
                .replace("{line}", &line)
                .replace("{column}", &column)
        })
        .collect();
    if !has_file {
        args.push(path.to_owned());
    }
    Some((program, args))
}

fn runnable(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            RUNNABLE
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
}

fn spawn(program: &Path, args: &[String]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start {}: {error}", program.display()))
}

fn reveal(path: &str) -> Result<(), String> {
    spawn(Path::new("explorer.exe"), &[format!("/select,{path}")])
}

/// Opens a path found by `find_paths`: a folder in File Explorer, a file in the chosen
/// editor at its line. `opener` is "auto", "vscode", "cursor", "default" or "custom".
#[tauri::command]
pub fn open_path(
    app: AppHandle,
    path: String,
    line: Option<u32>,
    column: Option<u32>,
    opener: String,
    command: String,
) -> Result<(), String> {
    // The path comes from the page: check again that it's local and there.
    let path = resolve(None, None, &path).ok_or("Only files on this PC can be opened.")?;
    let metadata = std::fs::metadata(&path).map_err(|_| format!("{path} isn't there any more."))?;
    if metadata.is_dir() {
        return spawn(Path::new("explorer.exe"), &[path]);
    }
    let editor = match opener.as_str() {
        "vscode" => Some(vscode().ok_or("VS Code isn't installed.")?),
        "cursor" => Some(cursor().ok_or("Cursor isn't installed.")?),
        "auto" => vscode().or_else(cursor),
        _ => None,
    };
    if let Some(editor) = editor {
        return spawn(&editor, &goto(&path, line, column));
    }
    if opener == "custom" {
        let (program, args) = custom_command(&command, &path, line, column)
            .ok_or("Set the command to open files with in Settings → Behavior.")?;
        let program = custom_program(&program).ok_or_else(|| format!("{program} wasn't found."))?;
        return spawn(&program, &args);
    }
    if runnable(Path::new(&path)) {
        return reveal(&path);
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|error| format!("Could not open {path}: {error}"))
}

fn custom_program(program: &str) -> Option<PathBuf> {
    let path = PathBuf::from(program);
    if path.is_absolute() {
        return path.is_file().then_some(path);
    }
    profiles::find_executable(program)
}

#[cfg(test)]
mod tests {
    use super::{custom_command, resolve};

    #[test]
    fn resolves_what_shells_print() {
        let cwd = Some("C:\\Users\\me\\app");
        let home = Some("C:\\Users\\me");
        assert_eq!(
            resolve(cwd, home, "src/main.rs").as_deref(),
            Some("C:\\Users\\me\\app\\src\\main.rs")
        );
        assert_eq!(
            resolve(cwd, home, "..\\notes.txt").as_deref(),
            Some("C:\\Users\\me\\notes.txt")
        );
        assert_eq!(
            resolve(cwd, home, "D:/data/log.txt").as_deref(),
            Some("D:\\data\\log.txt")
        );
        assert_eq!(
            resolve(cwd, home, "/c/Windows/win.ini").as_deref(),
            Some("C:\\Windows\\win.ini")
        );
        assert_eq!(
            resolve(cwd, home, "~/.bashrc").as_deref(),
            Some("C:\\Users\\me\\.bashrc")
        );
        // Never above the drive.
        assert_eq!(
            resolve(cwd, home, "../../../../../x").as_deref(),
            Some("C:\\x")
        );
    }

    #[test]
    fn reads_linux_paths_inside_wsl_only() {
        let wsl = Some("\\\\wsl.localhost\\Ubuntu\\home\\me");
        assert_eq!(
            resolve(wsl, None, "/etc/hosts").as_deref(),
            Some("\\\\wsl.localhost\\Ubuntu\\etc\\hosts")
        );
        assert_eq!(
            resolve(wsl, None, "src/lib.rs").as_deref(),
            Some("\\\\wsl.localhost\\Ubuntu\\home\\me\\src\\lib.rs")
        );
        assert_eq!(resolve(Some("C:\\app"), None, "/etc/hosts"), None);
    }

    #[test]
    fn leaves_network_paths_and_garbage_alone() {
        assert_eq!(
            resolve(Some("C:\\app"), None, "\\\\server\\share\\x.txt"),
            None
        );
        assert_eq!(resolve(Some("\\\\server\\share"), None, "x.txt"), None);
        assert_eq!(resolve(None, None, "src/main.rs"), None);
        assert_eq!(resolve(Some("C:\\app"), None, "a\u{7}b"), None);
    }

    #[test]
    fn fills_a_custom_command_without_a_shell() {
        let (program, args) = custom_command(
            "notepad++ -n{line} \"{file}\"",
            "C:\\a b\\c.txt",
            Some(12),
            None,
        )
        .unwrap();
        assert_eq!(program, "notepad++");
        assert_eq!(args, ["-n12", "C:\\a b\\c.txt"]);
        let (_, args) = custom_command("subl", "C:\\x & calc.txt", None, None).unwrap();
        assert_eq!(args, ["C:\\x & calc.txt"]);
        assert!(custom_command("   ", "C:\\x", None, None).is_none());
    }
}

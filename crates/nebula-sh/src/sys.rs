//! Platform helpers: home directory, path display and translation, PATH lookup.

use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};

pub fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// A path as shown in the prompt and titles: `~` for home, forward slashes, and
/// Git Bash style drive prefixes (`/c/Windows`) on Windows.
pub fn display_path(path: &Path) -> String {
    let mut text = path.to_string_lossy().into_owned();
    if let Some(stripped) = text.strip_prefix(r"\\?\") {
        text = stripped.to_owned();
    }
    if let Some(home) = home() {
        let home = home.to_string_lossy().into_owned();
        let matches_home = if cfg!(windows) {
            text.to_lowercase().starts_with(&home.to_lowercase())
        } else {
            text.starts_with(&home)
        };
        if matches_home
            && (text.len() == home.len()
                || matches!(text.as_bytes().get(home.len()), Some(b'/' | b'\\')))
        {
            text = format!("~{}", &text[home.len()..]);
        }
    }
    if cfg!(windows) {
        text = text.replace('\\', "/");
        let bytes = text.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            let drive = (bytes[0] as char).to_ascii_lowercase();
            let rest = text[2..].trim_end_matches('/');
            text = format!("/{drive}{rest}");
        }
    }
    text
}

/// Translates Linux-style paths for Nebula's own commands on Windows:
/// `/c/Users` → `C:/Users`, `/dev/null` → `NUL`. Other arguments pass through.
pub fn translate_path(arg: &str) -> String {
    if !cfg!(windows) {
        return arg.to_owned();
    }
    if arg == "/dev/null" {
        return "NUL".to_owned();
    }
    let bytes = arg.as_bytes();
    if bytes.len() >= 2
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && (bytes.len() == 2 || bytes[2] == b'/')
    {
        let drive = (bytes[1] as char).to_ascii_uppercase();
        let rest = if bytes.len() == 2 { "/" } else { &arg[2..] };
        return format!("{drive}:{rest}");
    }
    arg.to_owned()
}

fn path_extensions() -> Vec<String> {
    if !cfg!(windows) {
        return vec![String::new()];
    }
    let mut extensions: Vec<String> = env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|item| !item.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    extensions.push(".ps1".into());
    extensions
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Finds an executable by name: a path is used as is, a bare name is looked up in PATH.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    let extensions = path_extensions();
    let with_extensions = |base: &Path| -> Option<PathBuf> {
        if base.extension().is_some() && is_executable(base) {
            return Some(base.to_path_buf());
        }
        extensions.iter().find_map(|ext| {
            let path = if ext.is_empty() {
                base.to_path_buf()
            } else {
                PathBuf::from(format!("{}{ext}", base.display()))
            };
            is_executable(&path).then_some(path)
        })
    };
    if name.contains('/') || name.contains('\\') {
        return with_extensions(candidate);
    }
    env::split_paths(&env::var_os("PATH")?).find_map(|dir| with_extensions(&dir.join(name)))
}

/// Names of every executable on PATH, without extensions (for completion and highlighting).
pub fn path_commands() -> BTreeSet<String> {
    let extensions = path_extensions();
    let mut names = BTreeSet::new();
    let Some(path) = env::var_os("PATH") else {
        return names;
    };
    for dir in env::split_paths(&path) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if cfg!(windows) {
                let lower = name.to_ascii_lowercase();
                if let Some(ext) = extensions
                    .iter()
                    .find(|ext| !ext.is_empty() && lower.ends_with(ext.as_str()))
                {
                    names.insert(name[..name.len() - ext.len()].to_owned());
                }
            } else if is_executable(&entry.path()) {
                names.insert(name);
            }
        }
    }
    names
}

/// Turns on ANSI escape processing for the console (needed on Windows 10).
pub fn enable_ansi() {
    #[cfg(windows)]
    {
        let _ = nu_ansi_term::enable_ansi_support();
    }
}

/// True when icons can be drawn: Nebula Terminal bundles a Nerd Font, and
/// `NEBULA_ICONS=1/0` forces the choice elsewhere.
pub fn icons_enabled() -> bool {
    match env::var("NEBULA_ICONS").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => env::var_os("NEBULA_TERMINAL").is_some(),
    }
}

pub fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(width, _)| usize::from(width.0))
        .filter(|w| *w > 0)
        .unwrap_or(80)
}

/// Where Nebula keeps its history and folder ranks: `%APPDATA%\Nebula` on Windows.
pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().unwrap_or_default())
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().unwrap_or_default().join(".local/share"))
    };
    base.join(if cfg!(windows) { "Nebula" } else { "nebula" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn translates_drive_paths() {
        assert_eq!(translate_path("/c/Users"), "C:/Users");
        assert_eq!(translate_path("/d"), "D:/");
        assert_eq!(translate_path("/dev/null"), "NUL");
        assert_eq!(translate_path("/usr"), "/usr");
        assert_eq!(translate_path("-c"), "-c");
    }

    #[test]
    #[cfg(not(windows))]
    fn leaves_paths_alone_elsewhere() {
        assert_eq!(translate_path("/c/Users"), "/c/Users");
    }

    #[test]
    fn shows_home_as_tilde() {
        let home = home().expect("home");
        assert_eq!(display_path(&home), "~");
        assert_eq!(display_path(&home.join("projects")), "~/projects");
    }
}

use serde::Serialize;
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalProfile {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub available: bool,
    pub executable: Option<String>,
    pub accent: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedProfile {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

/// Built-in profiles in order of preference. The first available one is the
/// default for a fresh install.
const PROFILES: &[(&str, &str, &str)] = &[
    ("nebula", "Nebula", "#e8a33d"),
    ("pwsh", "PowerShell", "#5b8def"),
    ("powershell", "Windows PowerShell", "#3f7cc4"),
    ("cmd", "Command Prompt", "#9aa3ab"),
    ("gitbash", "Git Bash", "#e5734a"),
    ("wsl", "WSL", "#e0a040"),
];

fn path_extensions() -> Vec<String> {
    env::var_os("PATHEXT")
        .and_then(|value| value.into_string().ok())
        .map(|value| {
            value
                .split(';')
                .filter(|item| !item.is_empty())
                .map(|item| item.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_else(|| vec![".exe".into(), ".cmd".into(), ".bat".into(), ".com".into()])
}

fn find_in_path(name: &str, path: Option<OsString>, extensions: &[String]) -> Option<PathBuf> {
    let input = Path::new(name);
    if input.components().count() > 1 {
        return input.is_file().then(|| input.to_path_buf());
    }

    let has_extension = input.extension().is_some();
    env::split_paths(&path?).find_map(|directory| {
        let direct = directory.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        if has_extension {
            return None;
        }
        extensions.iter().find_map(|extension| {
            let candidate = directory.join(format!("{name}{extension}"));
            candidate.is_file().then_some(candidate)
        })
    })
}

fn find_executable(name: &str) -> Option<PathBuf> {
    find_in_path(name, env::var_os("PATH"), &path_extensions())
}

/// Git for Windows ships `bash.exe` under `<root>\bin`. `bash.exe` on PATH is
/// usually the WSL launcher in System32, so it is never used directly.
fn git_bash_candidates(git_on_path: Option<&Path>, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = git_on_path.and_then(Path::parent).and_then(Path::parent) {
        candidates.push(root.join("bin").join("bash.exe"));
    }
    for root in roots {
        candidates.push(root.join("Git").join("bin").join("bash.exe"));
    }
    candidates
}

fn find_git_bash() -> Option<PathBuf> {
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .iter()
        .filter_map(|key| env::var_os(key).map(PathBuf::from))
        .chain(env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("Programs")))
        .collect();
    let git = find_executable("git.exe");
    git_bash_candidates(git.as_deref(), &roots)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// Places where the bundled `nebula-sh` interpreter can be: next to the app (installed
/// builds, where Tauri puts sidecars), the sidecar staging folder, or a Cargo target
/// folder during development. `NEBULA_SH` overrides them all.
fn nebula_candidates(app_dir: &Path) -> Vec<PathBuf> {
    let name = if cfg!(windows) {
        "nebula-sh.exe"
    } else {
        "nebula-sh"
    };
    let mut candidates = vec![app_dir.join(name)];
    for ancestor in app_dir.ancestors().take(6) {
        candidates.push(ancestor.join("target").join("debug").join(name));
        candidates.push(ancestor.join("target").join("release").join(name));
    }
    candidates
}

fn find_nebula() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("NEBULA_SH").map(PathBuf::from) {
        return explicit.is_file().then_some(explicit);
    }
    let exe = env::current_exe().ok()?;
    nebula_candidates(exe.parent()?)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn locate(id: &str) -> Option<ResolvedProfile> {
    let (executable, args): (Option<PathBuf>, &[&str]) = match id {
        "nebula" => (find_nebula(), &[]),
        "pwsh" => (find_executable("pwsh.exe"), &["-NoLogo"]),
        "powershell" => (find_executable("powershell.exe"), &["-NoLogo"]),
        "cmd" => (find_executable("cmd.exe"), &[]),
        "gitbash" => (find_git_bash(), &["--login", "-i"]),
        "wsl" => (find_executable("wsl.exe"), &["--cd", "~"]),
        _ => (None, &[]),
    };
    executable.map(|executable| ResolvedProfile {
        executable,
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    })
}

#[tauri::command]
pub async fn detect_profiles() -> Vec<TerminalProfile> {
    PROFILES
        .iter()
        .map(|(id, name, accent)| {
            let resolved = locate(id);
            TerminalProfile {
                id: (*id).into(),
                name: (*name).into(),
                kind: (*id).into(),
                available: resolved.is_some(),
                executable: resolved
                    .map(|profile| profile.executable.to_string_lossy().into_owned()),
                accent: (*accent).into(),
            }
        })
        .collect()
}

/// Resolves a profile id sent by the UI. With a custom starting folder, WSL starts
/// there (as /mnt/...) instead of the Linux home directory.
pub fn resolve_profile(profile_id: &str, custom_cwd: bool) -> Result<ResolvedProfile, String> {
    if !PROFILES.iter().any(|(id, _, _)| *id == profile_id) {
        return Err(format!("Unknown profile '{profile_id}'."));
    }
    let mut profile = locate(profile_id)
        .ok_or_else(|| format!("Profile '{profile_id}' is not available on this system."))?;
    if profile_id == "wsl" && custom_cwd {
        profile.args.clear();
    }
    Ok(profile)
}

pub fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

/// Expands a leading `~` and `%VAR%` references in a user-supplied directory.
pub fn expand_directory(input: &str) -> PathBuf {
    let trimmed = input.trim();
    let mut expanded = String::with_capacity(trimmed.len());
    let mut rest = trimmed;
    while let Some(start) = rest.find('%') {
        let Some(length) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + length];
        expanded.push_str(&rest[..start]);
        match env::var(name) {
            Ok(value) if !name.is_empty() => expanded.push_str(&value),
            _ => expanded.push_str(&rest[start..start + length + 2]),
        }
        rest = &rest[start + length + 2..];
    }
    expanded.push_str(rest);

    if expanded == "~" || expanded.starts_with("~/") || expanded.starts_with("~\\") {
        if let Some(home) = home_directory() {
            let tail = expanded[1..].trim_start_matches(['/', '\\']);
            return if tail.is_empty() {
                home
            } else {
                home.join(tail)
            };
        }
    }
    PathBuf::from(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            env::temp_dir().join(format!("nebula-profiles-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn finds_executables_with_pathext() {
        let directory = scratch("pathext");
        fs::write(directory.join("tool.exe"), b"").unwrap();
        let path = env::join_paths([directory.clone()]).unwrap();
        let found = find_in_path("tool", Some(path.clone()), &[".com".into(), ".exe".into()]);
        assert_eq!(found, Some(directory.join("tool.exe")));
        assert_eq!(find_in_path("missing", Some(path), &[".exe".into()]), None);
    }

    #[test]
    fn explicit_extension_is_not_extended() {
        let directory = scratch("explicit");
        fs::write(directory.join("tool.exe.exe"), b"").unwrap();
        let path = env::join_paths([directory]).unwrap();
        assert_eq!(find_in_path("tool.exe", Some(path), &[".exe".into()]), None);
    }

    #[test]
    fn git_bash_is_derived_from_git_install_root() {
        let git = Path::new("C:/Tools/Git/cmd/git.exe");
        let candidates = git_bash_candidates(Some(git), &[PathBuf::from("C:/Program Files")]);
        assert_eq!(candidates[0], Path::new("C:/Tools/Git/bin/bash.exe"));
        assert_eq!(
            candidates[1],
            Path::new("C:/Program Files/Git/bin/bash.exe")
        );
    }

    #[test]
    fn looks_for_nebula_next_to_the_app_first() {
        let app = Path::new("/apps/nebula");
        let candidates = nebula_candidates(app);
        let name = if cfg!(windows) {
            "nebula-sh.exe"
        } else {
            "nebula-sh"
        };
        assert_eq!(candidates[0], app.join(name));
        assert!(candidates.contains(&Path::new("/target/debug").join(name)));
    }

    #[test]
    fn rejects_unknown_profiles() {
        assert!(resolve_profile("cmd.exe", false)
            .unwrap_err()
            .contains("Unknown"));
        assert!(resolve_profile("anything.exe", true).is_err());
    }

    #[test]
    fn profile_ids_are_unique() {
        let mut ids: Vec<_> = PROFILES.iter().map(|(id, _, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROFILES.len());
    }

    #[test]
    fn expands_environment_variables_and_keeps_unknown_ones() {
        env::set_var("NEBULA_TEST_DIR", "projects");
        assert_eq!(
            expand_directory("C:/%NEBULA_TEST_DIR%/app"),
            PathBuf::from("C:/projects/app")
        );
        assert_eq!(
            expand_directory("%NEBULA_UNSET_VAR%/x"),
            PathBuf::from("%NEBULA_UNSET_VAR%/x")
        );
        assert_eq!(expand_directory("50%"), PathBuf::from("50%"));
    }
}

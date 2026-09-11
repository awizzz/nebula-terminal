use serde::Serialize;
use std::{
    env,
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

fn candidate_exists(path: &Path) -> bool {
    path.is_file()
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let input = Path::new(name);
    if input.components().count() > 1 && candidate_exists(input) {
        return Some(input.to_path_buf());
    }

    let has_extension = input.extension().is_some();
    let extensions = path_extensions();
    env::split_paths(&env::var_os("PATH")?).find_map(|directory| {
        let direct = directory.join(name);
        if candidate_exists(&direct) {
            return Some(direct);
        }
        if has_extension {
            return None;
        }
        extensions.iter().find_map(|extension| {
            let candidate = directory.join(format!("{name}{extension}"));
            candidate_exists(&candidate).then_some(candidate)
        })
    })
}

fn find_nebula() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("NEBULA_SHELL") {
        let path = PathBuf::from(explicit);
        if candidate_exists(&path) {
            return Some(path);
        }
    }

    if let Ok(current) = env::current_exe() {
        if let Some(parent) = current.parent() {
            for file in ["Nebula.exe", "nebula.exe"] {
                let candidate = parent.join(file);
                if candidate_exists(&candidate) && candidate != current {
                    return Some(candidate);
                }
            }
        }
    }

    find_executable("nebula")
}

fn profile(
    id: &str,
    name: &str,
    kind: &str,
    executable: Option<PathBuf>,
    accent: &str,
) -> TerminalProfile {
    TerminalProfile {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        available: executable.is_some(),
        executable: executable.map(|path| path.to_string_lossy().into_owned()),
        accent: accent.into(),
    }
}

#[tauri::command]
pub fn detect_profiles() -> Vec<TerminalProfile> {
    vec![
        profile("nebula", "Nebula", "nebula", find_nebula(), "#8b7cf6"),
        profile("cmd", "Command Prompt", "cmd", find_executable("cmd.exe"), "#78dba9"),
        profile(
            "powershell",
            "Windows PowerShell",
            "powershell",
            find_executable("powershell.exe"),
            "#72a9f7",
        ),
        profile("pwsh", "PowerShell 7", "pwsh", find_executable("pwsh.exe"), "#a98df4"),
        profile("wsl", "WSL", "wsl", find_executable("wsl.exe"), "#f2c877"),
    ]
}

pub fn resolve_profile(profile_id: &str) -> Result<ResolvedProfile, String> {
    let (executable, args): (Option<PathBuf>, Vec<&str>) = match profile_id {
        "nebula" => (find_nebula(), vec![]),
        "cmd" => (find_executable("cmd.exe"), vec!["/Q"]),
        "powershell" => (find_executable("powershell.exe"), vec!["-NoLogo"]),
        "pwsh" => (find_executable("pwsh.exe"), vec!["-NoLogo"]),
        "wsl" => (find_executable("wsl.exe"), vec![]),
        _ => return Err(format!("Unknown profile '{profile_id}'.")),
    };

    executable
        .map(|executable| ResolvedProfile {
            executable,
            args: args.into_iter().map(str::to_owned).collect(),
        })
        .ok_or_else(|| format!("Profile '{profile_id}' is not available on this system."))
}

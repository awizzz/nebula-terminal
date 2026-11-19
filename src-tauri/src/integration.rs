//! Shell integration for PowerShell, Git Bash, WSL, and bash and zsh on Linux. Each
//! shell gets a small script that marks its prompts and commands (OSC 133) and reports
//! its folder (OSC 7), as Nebula does on its own. The user's profile and startup files
//! still run first.

use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    fs,
    path::{Path, PathBuf},
};

const BASH_SCRIPT: &str = include_str!("../shell-integration/nebula.bash");
const POWERSHELL_SCRIPT: &str = include_str!("../shell-integration/nebula.ps1");
const ZSH_SCRIPT: &str = include_str!("../shell-integration/nebula.zsh");
const BASH_FILE: &str = "nebula.bash";
/// Zsh reads `.zshenv` from ZDOTDIR, so the script gets a folder of its own.
const ZSH_FOLDER: &str = "zsh";

/// Starts the distribution's login shell. Bash reads Nebula's script, which reads the
/// usual startup files itself; any other shell starts as it always does.
const WSL_LAUNCHER: &str = r#"f=$(wslpath -u "$1" 2>/dev/null); s=${SHELL:-$(getent passwd "$(id -un)" | cut -d: -f7)}; s=${s:-/bin/sh}; case ${s##*/} in bash) if [ -r "$f" ]; then exec "$s" --init-file "$f" -i; fi; exec "$s" -l;; zsh|fish|ksh|mksh|dash|ash|sh|tcsh|csh) exec "$s" -l;; esac; exec "$s""#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    PowerShell,
    /// Git Bash on Windows, bash itself elsewhere: the same script and arguments.
    Bash,
    Zsh,
    Wsl,
}

impl Kind {
    pub fn of(profile_id: &str) -> Option<Self> {
        match profile_id {
            "pwsh" | "powershell" => Some(Self::PowerShell),
            "gitbash" | "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "wsl" => Some(Self::Wsl),
            id if id.starts_with("wsl:") => Some(Self::Wsl),
            _ => None,
        }
    }
}

/// Where the bash script is written, so Git Bash and WSL can read it.
pub struct Scripts {
    directory: Option<PathBuf>,
}

impl Scripts {
    pub fn new(directory: Option<PathBuf>) -> Self {
        Self { directory }
    }

    /// A script on disk, rewritten when it's missing or from another version.
    fn script(&self, file: &Path, content: &str) -> Option<PathBuf> {
        let path = self.directory.as_ref()?.join(file);
        // A checkout with Windows line endings must not reach a Unix shell.
        let content = content.replace("\r\n", "\n");
        if fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
            fs::create_dir_all(path.parent()?).ok()?;
            fs::write(&path, content).ok()?;
        }
        Some(path)
    }

    fn bash(&self) -> Option<PathBuf> {
        self.script(Path::new(BASH_FILE), BASH_SCRIPT)
    }

    /// Adds the integration to a shell's arguments, and returns the environment it
    /// needs. Both stay as they were when the script can't be written.
    pub fn apply(&self, kind: Kind, args: &mut Vec<String>) -> Vec<(String, String)> {
        match kind {
            Kind::PowerShell => powershell_args(args),
            Kind::Bash => {
                if let Some(script) = self.bash() {
                    bash_args(args, &script);
                }
            }
            Kind::Wsl => {
                if let Some(script) = self.bash() {
                    wsl_args(args, &script);
                }
            }
            Kind::Zsh => {
                if let Some(script) =
                    self.script(&Path::new(ZSH_FOLDER).join(".zshenv"), ZSH_SCRIPT)
                {
                    return zsh_environment(&script, std::env::var("ZDOTDIR").ok());
                }
            }
        }
        Vec::new()
    }
}

/// The script goes inline, so the execution policy, which blocks script files on
/// many machines, doesn't apply to it.
fn powershell_args(args: &mut Vec<String>) {
    let utf16: Vec<u8> = POWERSHELL_SCRIPT
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    args.extend([
        "-NoExit".to_owned(),
        "-EncodedCommand".to_owned(),
        STANDARD.encode(utf16),
    ]);
}

/// ZDOTDIR points zsh at the script's folder; the script puts the user's own back
/// from NEBULA_ZDOTDIR (empty when they had none, so zsh uses their home).
fn zsh_environment(script: &Path, user_zdotdir: Option<String>) -> Vec<(String, String)> {
    let Some(folder) = script.parent() else {
        return Vec::new();
    };
    vec![
        ("ZDOTDIR".to_owned(), folder.to_string_lossy().into_owned()),
        (
            "NEBULA_ZDOTDIR".to_owned(),
            user_zdotdir.unwrap_or_default(),
        ),
    ]
}

/// `--init-file` replaces `--login`, which bash doesn't combine with it: the script
/// reads the login files instead.
fn bash_args(args: &mut Vec<String>, script: &Path) {
    args.retain(|arg| arg != "--login" && arg != "-i");
    args.extend([
        "--init-file".to_owned(),
        script.to_string_lossy().replace('\\', "/"),
        "-i".to_owned(),
    ]);
}

fn wsl_args(args: &mut Vec<String>, script: &Path) {
    args.extend([
        "-e".to_owned(),
        "sh".to_owned(),
        "-c".to_owned(),
        WSL_LAUNCHER.to_owned(),
        "nebula".to_owned(),
        script.to_string_lossy().into_owned(),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("nebula-integration-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn knows_which_profiles_get_it() {
        assert_eq!(Kind::of("pwsh"), Some(Kind::PowerShell));
        assert_eq!(Kind::of("powershell"), Some(Kind::PowerShell));
        assert_eq!(Kind::of("gitbash"), Some(Kind::Bash));
        assert_eq!(Kind::of("bash"), Some(Kind::Bash));
        assert_eq!(Kind::of("zsh"), Some(Kind::Zsh));
        assert_eq!(Kind::of("fish"), None);
        assert_eq!(Kind::of("wsl"), Some(Kind::Wsl));
        assert_eq!(Kind::of("wsl:Ubuntu"), Some(Kind::Wsl));
        assert_eq!(Kind::of("nebula"), None);
        assert_eq!(Kind::of("cmd"), None);
        assert_eq!(Kind::of("ssh:server"), None);
    }

    #[test]
    fn powershell_runs_the_script_inline_and_stays_open() {
        let mut args = vec!["-NoLogo".to_owned()];
        powershell_args(&mut args);
        assert_eq!(args[..3], ["-NoLogo", "-NoExit", "-EncodedCommand"]);
        let bytes = STANDARD.decode(&args[3]).unwrap();
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), POWERSHELL_SCRIPT);
        // Windows caps a command line at 32,767 characters.
        assert!(args.iter().map(String::len).sum::<usize>() < 30_000);
    }

    #[test]
    fn git_bash_reads_the_script_instead_of_logging_in() {
        let mut args = vec!["--login".to_owned(), "-i".to_owned()];
        bash_args(
            &mut args,
            Path::new(r"C:\Users\me\AppData\Local\nebula\nebula.bash"),
        );
        assert_eq!(
            args,
            [
                "--init-file",
                "C:/Users/me/AppData/Local/nebula/nebula.bash",
                "-i"
            ]
        );
    }

    #[test]
    fn wsl_keeps_its_distribution_and_folder() {
        let mut args = vec![
            "-d".to_owned(),
            "Debian".to_owned(),
            "--cd".to_owned(),
            "~".to_owned(),
        ];
        wsl_args(&mut args, Path::new(r"C:\nebula\nebula.bash"));
        assert_eq!(args[..6], ["-d", "Debian", "--cd", "~", "-e", "sh"]);
        assert_eq!(args.last().unwrap(), r"C:\nebula\nebula.bash");
    }

    #[test]
    fn writes_the_bash_script_with_unix_line_endings() {
        let directory = scratch("write");
        let scripts = Scripts::new(Some(directory.clone()));
        let path = scripts.bash().unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(!written.contains('\r'));
        assert!(written.contains("133;A"));

        fs::write(&path, "stale").unwrap();
        scripts.bash().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), written);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn leaves_bash_alone_without_a_folder_for_the_script() {
        let scripts = Scripts::new(None);
        let mut args = vec!["--login".to_owned(), "-i".to_owned()];
        assert!(scripts.apply(Kind::Bash, &mut args).is_empty());
        assert_eq!(args, ["--login", "-i"]);
    }

    #[test]
    fn zsh_reads_the_script_through_zdotdir_and_gets_its_own_back() {
        let folder = scratch("zsh");
        let scripts = Scripts::new(Some(folder.clone()));
        let mut args = Vec::new();
        let environment = scripts.apply(Kind::Zsh, &mut args);
        assert!(args.is_empty());
        let zdotdir = folder.join(ZSH_FOLDER);
        assert_eq!(
            environment,
            [
                ("ZDOTDIR".to_owned(), zdotdir.to_string_lossy().into_owned()),
                (
                    "NEBULA_ZDOTDIR".to_owned(),
                    std::env::var("ZDOTDIR").unwrap_or_default()
                ),
            ]
        );
        let script = fs::read_to_string(zdotdir.join(".zshenv")).unwrap();
        assert!(script.contains("133;A") && !script.contains('\r'));
    }
}

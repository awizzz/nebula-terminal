//! Installed WSL distributions, so each one can have its own profile.
//!
//! The list comes from the registry, which is instant. When that fails, `wsl.exe -l -q`
//! is asked instead, with a time limit because it may have to start the WSL service.

use std::{
    io::Read,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// How long `wsl.exe -l -q` may take before profile detection moves on without it.
const LIST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_OUTPUT: u64 = 64 * 1024;
const MAX_DISTRIBUTIONS: usize = 32;

/// Names WSL accepts for a distribution: letters, digits, `.`, `_` and `-`. Anything else
/// is skipped rather than passed to `wsl.exe -d`.
pub fn is_valid_name(name: &str) -> bool {
    name.len() <= 64
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Docker Desktop and Rancher Desktop keep their engines in WSL distributions that are not
/// meant to be opened as a shell.
pub fn is_internal(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("docker-desktop") || lower.starts_with("rancher-desktop")
}

fn looks_like_utf16(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xff, 0xfe]) {
        return true;
    }
    let odd = bytes.len() / 2;
    let zeros = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|byte| **byte == 0)
        .count();
    odd > 0 && zeros * 2 >= odd
}

/// Decodes the output of `wsl.exe -l -q`. It is UTF-16LE (UTF-8 when `WSL_UTF8=1` is set),
/// may start with a byte order mark and sometimes carries stray NULs.
pub fn decode_list_output(bytes: &[u8]) -> Vec<String> {
    let text = if looks_like_utf16(bytes) {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    text.lines()
        .map(|line| {
            line.chars()
                .filter(|c| *c != '\0' && *c != '\u{feff}')
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// Drops internal, invalid and duplicate names, keeping the original order.
fn openable(names: Vec<String>) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim().to_owned();
        if is_valid_name(&name)
            && !is_internal(&name)
            && !result.iter().any(|known| known.eq_ignore_ascii_case(&name))
        {
            result.push(name);
        }
    }
    result.truncate(MAX_DISTRIBUTIONS);
    result
}

/// Distributions registered for the current user, the default one first.
#[cfg(windows)]
fn from_registry() -> Option<Vec<String>> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let lxss = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Lxss")
        .ok()?;
    let default: Option<String> = lxss.get_value("DefaultDistribution").ok();
    let mut entries: Vec<(bool, String)> = lxss
        .enum_keys()
        .filter_map(Result::ok)
        .filter_map(|guid| {
            let name: String = lxss
                .open_subkey(&guid)
                .ok()?
                .get_value("DistributionName")
                .ok()?;
            let is_default = default
                .as_deref()
                .is_some_and(|default| default.eq_ignore_ascii_case(&guid));
            Some((is_default, name))
        })
        .collect();
    entries.sort_by_key(|(is_default, _)| !is_default);
    Some(entries.into_iter().map(|(_, name)| name).collect())
}

#[cfg(not(windows))]
fn from_registry() -> Option<Vec<String>> {
    None
}

fn wait_until(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Runs `<wsl> -l -q` and returns its decoded output, or nothing if it fails or takes
/// longer than `timeout`.
fn from_command(wsl: &Path, timeout: Duration) -> Vec<String> {
    let mut command = Command::new(wsl);
    command
        .args(["--list", "--quiet"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(mut child) = command.spawn() else {
        return Vec::new();
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Vec::new();
    };

    // Read on another thread: a stuck wsl.exe must not hold detection past the deadline.
    let deadline = Instant::now() + timeout;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.take(MAX_OUTPUT).read_to_end(&mut output);
        let _ = sender.send(output);
    });
    let output = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let status = wait_until(&mut child, deadline);
    match (output, status) {
        (Ok(output), Some(status)) if status.success() => decode_list_output(&output),
        _ => Vec::new(),
    }
}

/// Installed distributions that can be opened, given the path to `wsl.exe`.
pub fn distributions(wsl: &Path) -> Vec<String> {
    openable(from_registry().unwrap_or_else(|| from_command(wsl, LIST_TIMEOUT)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str, bom: bool) -> Vec<u8> {
        let mut bytes = if bom { vec![0xff, 0xfe] } else { Vec::new() };
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    #[test]
    fn decodes_utf16_output_with_or_without_bom() {
        let expected = vec!["Ubuntu-22.04".to_owned(), "Debian".to_owned()];
        assert_eq!(
            decode_list_output(&utf16("Ubuntu-22.04\r\nDebian\r\n", true)),
            expected
        );
        assert_eq!(
            decode_list_output(&utf16("Ubuntu-22.04\r\nDebian\r\n", false)),
            expected
        );
    }

    #[test]
    fn decodes_utf8_output_and_strips_nuls() {
        assert_eq!(
            decode_list_output(b"\xef\xbb\xbfUbuntu\r\n\r\nkali-linux\0\n"),
            vec!["Ubuntu".to_owned(), "kali-linux".to_owned()]
        );
        assert!(decode_list_output(b"").is_empty());
        assert!(decode_list_output(&utf16("\r\n\0", true)).is_empty());
    }

    #[test]
    fn odd_length_utf16_ignores_the_last_byte() {
        let mut bytes = utf16("Alpine\r\n", false);
        bytes.push(0);
        assert_eq!(decode_list_output(&bytes), vec!["Alpine".to_owned()]);
    }

    #[test]
    fn skips_internal_invalid_and_duplicate_distributions() {
        let names = [
            "Ubuntu",
            "docker-desktop",
            "docker-desktop-data",
            "Rancher-Desktop",
            "ubuntu",
            "-d",
            "has space",
            "Debian",
        ];
        assert_eq!(
            openable(names.iter().map(|name| (*name).to_owned()).collect()),
            vec!["Ubuntu".to_owned(), "Debian".to_owned()]
        );
    }

    #[test]
    fn validates_names() {
        assert!(is_valid_name("Ubuntu-24.04"));
        assert!(is_valid_name("openSUSE_Tumbleweed"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("--exec"));
        assert!(!is_valid_name("a;b"));
        assert!(!is_valid_name(&"a".repeat(65)));
    }

    #[cfg(unix)]
    fn script(name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("nebula-wsl-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("wsl");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn reads_the_list_from_the_command() {
        let wsl = script("list", r"printf 'Ubuntu\r\nDebian\r\n'");
        assert_eq!(
            from_command(&wsl, Duration::from_secs(5)),
            vec!["Ubuntu".to_owned(), "Debian".to_owned()]
        );
        let failing = script("fail", "echo 'not installed'; exit 1");
        assert!(from_command(&failing, Duration::from_secs(5)).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn gives_up_on_a_command_that_hangs() {
        let wsl = script("hang", "exec sleep 10");
        let started = Instant::now();
        assert!(from_command(&wsl, Duration::from_millis(300)).is_empty());
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}

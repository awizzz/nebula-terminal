//! What the page needs to know about the system before its first line runs.

/// Sets `window.__NEBULA_PLATFORM__` ("windows", "linux" or "macos") and
/// `window.__NEBULA_HOST__`, this machine's name, which tells its own folders from a
/// server's in the folder reports (OSC 7) shells send.
pub fn page_script() -> String {
    let platform = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let host = serde_json::to_string(&hostname()).unwrap_or_else(|_| "\"\"".to_owned());
    format!("window.__NEBULA_PLATFORM__ = \"{platform}\"; window.__NEBULA_HOST__ = {host};")
}

/// WebKitGTK's DMA-BUF renderer leaves the window blank with NVIDIA's own driver; the
/// older path draws fine there. Someone who set the variable either way keeps their choice.
#[cfg(target_os = "linux")]
pub fn prepare_webview() {
    const VARIABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(VARIABLE).is_none() && std::path::Path::new("/proc/driver/nvidia").exists()
    {
        std::env::set_var(VARIABLE, "1");
    }
}

/// What an AppImage's launcher puts in the environment for the app's own libraries
/// (paths inside the mounted image, GTK modules), to take back out of the shells it
/// starts: programs run from the terminal would pick up the app's copies otherwise.
/// Each entry is a variable to set to a new value, or to remove (None).
#[cfg(target_os = "linux")]
pub fn appimage_cleanup() -> Vec<(String, Option<String>)> {
    match std::env::var("APPDIR") {
        Ok(appdir) if !appdir.is_empty() => without_appdir(std::env::vars(), &appdir),
        _ => Vec::new(),
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn without_appdir(
    variables: impl Iterator<Item = (String, String)>,
    appdir: &str,
) -> Vec<(String, Option<String>)> {
    // GTK_THEME too: the launcher sets Adwaita for the app, over the user's own.
    const LAUNCHER: [&str; 5] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD", "GTK_THEME"];
    variables
        .filter_map(|(name, value)| {
            if LAUNCHER.contains(&name.as_str()) {
                return Some((name, None));
            }
            if !value.contains(appdir) {
                return None;
            }
            // A list of paths keeps the system's; a single path inside the image goes.
            let kept: Vec<&str> = value
                .split(':')
                .filter(|part| !part.is_empty() && !part.starts_with(appdir))
                .collect();
            Some((name, (!kept.is_empty()).then(|| kept.join(":"))))
        })
        .collect()
}

#[cfg(windows)]
fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

#[cfg(unix)]
fn hostname() -> String {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer and its length; gethostname writes at most that many bytes.
    let written = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } == 0;
    let end = buffer
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(buffer.len());
    if written {
        String::from_utf8_lossy(&buffer[..end]).into_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn takes_the_appimage_out_of_the_environment() {
        let variables = [
            ("APPDIR", "/tmp/.mount_NebulaX"),
            ("APPIMAGE", "/home/me/Nebula.AppImage"),
            (
                "PATH",
                "/tmp/.mount_NebulaX/usr/bin:/usr/local/bin:/usr/bin",
            ),
            ("XDG_DATA_DIRS", "/tmp/.mount_NebulaX/usr/share:/usr/share"),
            (
                "GDK_PIXBUF_MODULE_FILE",
                "/tmp/.mount_NebulaX/usr/lib/loaders.cache",
            ),
            ("GTK_THEME", "Adwaita:light"),
            ("HOME", "/home/me"),
        ]
        .map(|(name, value)| (name.to_owned(), value.to_owned()));
        let changes = super::without_appdir(variables.into_iter(), "/tmp/.mount_NebulaX");
        let change = |name: &str| {
            changes
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, value)| value.as_deref())
        };
        assert_eq!(change("APPDIR"), Some(None));
        assert_eq!(change("APPIMAGE"), Some(None));
        assert_eq!(change("PATH"), Some(Some("/usr/local/bin:/usr/bin")));
        assert_eq!(change("XDG_DATA_DIRS"), Some(Some("/usr/share")));
        assert_eq!(change("GDK_PIXBUF_MODULE_FILE"), Some(None));
        assert_eq!(change("GTK_THEME"), Some(None));
        assert_eq!(change("HOME"), None);
    }

    #[test]
    fn names_the_platform_and_host() {
        let script = super::page_script();
        assert!(script.starts_with("window.__NEBULA_PLATFORM__ = \""));
        assert!(script.contains("window.__NEBULA_HOST__ = \""));
    }
}

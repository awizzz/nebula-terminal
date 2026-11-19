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
    fn names_the_platform_and_host() {
        let script = super::page_script();
        assert!(script.starts_with("window.__NEBULA_PLATFORM__ = \""));
        assert!(script.contains("window.__NEBULA_HOST__ = \""));
    }
}

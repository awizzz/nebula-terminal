use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

pub fn data_dir() -> PathBuf {
    if let Some(path) = env::var_os("APPDATA") {
        return PathBuf::from(path).join("Nebula");
    }

    if let Some(home) = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) {
        return PathBuf::from(home).join(".nebula");
    }

    PathBuf::from(".nebula")
}

pub fn hostname() -> String {
    env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .unwrap_or_else(|_| "PC".into())
}

pub fn username() -> String {
    env::var("USERNAME")
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| "user".into())
}

pub fn terminal_name() -> String {
    if env::var_os("WT_SESSION").is_some() {
        return "Windows Terminal".into();
    }
    if let Ok(value) = env::var("TERM_PROGRAM") {
        if !value.trim().is_empty() {
            return value;
        }
    }
    if env::var_os("ConEmuANSI").is_some() {
        return "ConEmu".into();
    }
    "Windows Console".into()
}

pub fn command_exists(command: &str) -> bool {
    #[cfg(windows)]
    {
        Command::new("where.exe")
            .arg(command)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        Command::new("sh")
            .args(["-c", &format!("command -v {command}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

#[cfg(windows)]
mod windows {
    use std::{
        ffi::{c_void, OsStr, OsString},
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::Path,
        ptr,
    };

    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    const SW_SHOWNORMAL: i32 = 1;

    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[link(name = "Kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
        fn LCIDToLocaleName(
            locale: u32,
            locale_name: *mut u16,
            locale_name_count: i32,
            flags: u32,
        ) -> i32;
        fn GetUserDefaultLocaleName(locale_name: *mut u16, locale_name_count: i32) -> i32;
        fn GetLocalTime(system_time: *mut SystemTime);
    }

    #[link(name = "Shell32")]
    extern "system" {
        fn IsUserAnAdmin() -> i32;
        fn ShellExecuteW(
            hwnd: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_cmd: i32,
        ) -> isize;
    }

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    fn wide_str(value: &str) -> Vec<u16> {
        wide(OsStr::new(value))
    }

    fn buffer_to_string(buffer: &[u16], len: i32) -> Option<String> {
        if len <= 1 {
            return None;
        }

        Some(
            OsString::from_wide(&buffer[..(len as usize - 1)])
                .to_string_lossy()
                .into_owned(),
        )
    }

    pub fn ui_locale() -> Option<String> {
        let language_id = unsafe { GetUserDefaultUILanguage() };
        if language_id != 0 {
            let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
            let len = unsafe {
                LCIDToLocaleName(
                    language_id as u32,
                    buffer.as_mut_ptr(),
                    LOCALE_NAME_MAX_LENGTH as i32,
                    0,
                )
            };

            if let Some(locale) = buffer_to_string(&buffer, len) {
                return Some(locale);
            }
        }

        let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
        let len =
            unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32) };
        buffer_to_string(&buffer, len)
    }

    pub fn local_time() -> String {
        let mut value = SystemTime {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        unsafe { GetLocalTime(&mut value) };
        format!("{:02}:{:02}", value.hour, value.minute)
    }

    pub fn is_admin() -> bool {
        unsafe { IsUserAnAdmin() != 0 }
    }

    pub fn relaunch_elevated() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let operation = wide_str("runas");
        let file = wide(exe.as_os_str());
        let parameters = wide_str("--elevated-child");
        let directory = wide(cwd.as_os_str());

        let result = unsafe {
            ShellExecuteW(
                ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                parameters.as_ptr(),
                directory.as_ptr(),
                SW_SHOWNORMAL,
            )
        };

        if result > 32 {
            Ok(())
        } else {
            Err(format!("ShellExecuteW failed with code {result}"))
        }
    }

    pub fn run_command_elevated(command: &str, cwd: &Path) -> Result<(), String> {
        let operation = wide_str("runas");
        let file = wide_str("cmd.exe");
        let escaped_cwd = cwd.display().to_string().replace('"', "\"\"");
        let parameters = wide_str(&format!(
            "/d /k \"cd /d \\\"{escaped_cwd}\\\" && {command}\""
        ));
        let directory = wide(cwd.as_os_str());

        let result = unsafe {
            ShellExecuteW(
                ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                parameters.as_ptr(),
                directory.as_ptr(),
                SW_SHOWNORMAL,
            )
        };

        if result > 32 {
            Ok(())
        } else {
            Err(format!("ShellExecuteW failed with code {result}"))
        }
    }
}

pub fn system_locale() -> String {
    #[cfg(windows)]
    if let Some(locale) = windows::ui_locale() {
        return locale;
    }

    env::var("LANG")
        .ok()
        .and_then(|value| value.split('.').next().map(str::to_owned))
        .unwrap_or_else(|| "en-US".into())
}

pub fn local_time() -> String {
    #[cfg(windows)]
    {
        windows::local_time()
    }

    #[cfg(not(windows))]
    {
        String::new()
    }
}

pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        windows::is_admin()
    }

    #[cfg(not(windows))]
    {
        false
    }
}

pub fn relaunch_elevated() -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::relaunch_elevated()
    }

    #[cfg(not(windows))]
    {
        Err("administrator elevation is only available on Windows".into())
    }
}

pub fn run_command_elevated(command: &str, cwd: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::run_command_elevated(command, cwd)
    }

    #[cfg(not(windows))]
    {
        let _ = (command, cwd);
        Err("administrator elevation is only available on Windows".into())
    }
}

pub fn open_in_editor(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("notepad.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

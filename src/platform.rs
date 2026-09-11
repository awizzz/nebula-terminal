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
            // MAKELCID(language_id, SORT_DEFAULT) is the LANGID itself because
            // SORT_DEFAULT is zero. LCIDToLocaleName gives us a BCP-47-style
            // locale name such as fr-FR or en-US.
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

        // Keep a safe fallback for unusual/custom Windows locale setups where
        // a UI LANGID cannot be converted to a locale name.
        let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
        let len = unsafe {
            GetUserDefaultLocaleName(buffer.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32)
        };
        buffer_to_string(&buffer, len)
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

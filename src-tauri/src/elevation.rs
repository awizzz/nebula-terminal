//! Admin windows. A shell that runs as administrator can't safely share a window with
//! ordinary ones: any program running as the user could type into it. So, as in
//! Windows Terminal, "Run as administrator" starts another copy of the app through UAC,
//! with a window of its own where every tab is elevated.
//!
//! The ordinary window writes what to open (a profile, a folder, and its settings so
//! both windows look alike) to a file, and passes only that file's name. Nothing else
//! goes from the ordinary copy to the admin one, and nothing comes back.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

/// Starts an admin copy: `--administrator <request id>`.
pub const FLAG: &str = "--administrator";

/// What the command answers when the user says no to UAC.
const CANCELLED: &str = "cancelled";

/// Where admin windows keep their web data. An elevated WebView2 can't share the
/// ordinary window's folder while that one is open.
const DATA_FOLDER: &str = "EBWebView-administrator";

/// A request no admin copy picked up within this time never will be.
const STALE: Duration = Duration::from_secs(60 * 60);

/// Settings hold at most a background image; anything bigger isn't a request.
const MAX_REQUEST: u64 = 32 * 1024 * 1024;

/// What an admin window opens first.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
    /// The asking window's settings. The page checks them like any saved settings.
    #[serde(default)]
    preferences: Option<serde_json::Value>,
}

/// Whether this copy runs elevated while the user's other programs don't: UAC is on and
/// this one was started as administrator. With UAC off, every program of an admin runs
/// elevated, and there's no ordinary window to keep apart from.
#[cfg(windows)]
pub fn is_admin_copy() -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{
            GetTokenInformation, TokenElevationType, TokenElevationTypeFull, TOKEN_ELEVATION_TYPE,
            TOKEN_QUERY,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let mut token = HANDLE::default();
    let mut kind = TOKEN_ELEVATION_TYPE::default();
    let mut size = 0u32;
    // SAFETY: the token is opened and closed here, and TOKEN_ELEVATION_TYPE is a plain
    // integer of the size passed.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let read = GetTokenInformation(
            token,
            TokenElevationType,
            Some(std::ptr::from_mut(&mut kind).cast()),
            std::mem::size_of::<TOKEN_ELEVATION_TYPE>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        read.is_ok() && kind == TokenElevationTypeFull
    }
}

#[cfg(not(windows))]
pub fn is_admin_copy() -> bool {
    false
}

fn requests_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|dir| dir.join("administrator-requests"))
}

/// The request id after `--administrator`, as a canonical UUID: never a path.
fn request_id(args: &[String]) -> Option<String> {
    let index = args.iter().position(|arg| arg == FLAG)?;
    let id = uuid::Uuid::try_parse(args.get(index + 1)?).ok()?;
    Some(id.hyphenated().to_string())
}

/// The request a copy was started for (its arguments), read once and deleted.
pub fn take_request(app: &AppHandle, args: &[String]) -> Option<Request> {
    let (Some(id), Some(dir)) = (request_id(args), requests_dir(app)) else {
        return None;
    };
    let path = dir.join(format!("{id}.json"));
    let request = fs::metadata(&path)
        .ok()
        .filter(|metadata| metadata.len() <= MAX_REQUEST)
        .and_then(|_| fs::read(&path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let _ = fs::remove_file(&path);
    request
}

/// Requests left behind when UAC was never answered or the admin copy failed to start.
fn remove_stale(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > STALE);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Builds the main window. An admin copy gets its own web data, a title that says
/// what it is, and its request, which the page reads as `window.__NEBULA_ADMIN__`.
pub fn create_window(app: &tauri::App, administrator: bool) -> tauri::Result<()> {
    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
    else {
        return Ok(());
    };
    let mut builder = WebviewWindowBuilder::from_config(app.handle(), config)?;
    if administrator {
        let args: Vec<String> = std::env::args().collect();
        let request = take_request(app.handle(), &args).unwrap_or_default();
        let request = serde_json::to_string(&request).unwrap_or_else(|_| "{}".to_owned());
        builder = builder
            .title("Administrator: Nebula Terminal")
            .initialization_script(format!("window.__NEBULA_ADMIN__ = {request};"));
        if let Ok(dir) = app.path().app_local_data_dir() {
            builder = builder.data_directory(dir.join(DATA_FOLDER));
        }
    }
    builder.build()?;
    Ok(())
}

/// Opens an admin window on `profile_id`, through UAC. Fails with "cancelled" when the
/// user says no.
#[tauri::command]
pub async fn open_as_administrator(
    app: AppHandle,
    window: WebviewWindow,
    profile_id: Option<String>,
    folder: Option<String>,
    preferences: Option<serde_json::Value>,
) -> Result<(), String> {
    let dir = requests_dir(&app).ok_or("There's no folder to pass the request through.")?;
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    remove_stale(&dir);
    let id = uuid::Uuid::new_v4().hyphenated().to_string();
    let path = dir.join(format!("{id}.json"));
    let request = Request {
        profile_id,
        folder,
        preferences,
    };
    let bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    fs::write(&path, bytes).map_err(|error| error.to_string())?;

    // The UAC prompt belongs to this window, so it comes up in front of it.
    #[cfg(windows)]
    let owner = window.hwnd().ok().map(|hwnd| hwnd.0 as isize);
    #[cfg(not(windows))]
    let owner = {
        let _ = window;
        None
    };
    // UAC can wait a long time for an answer; keep it off the main thread.
    let result = tauri::async_runtime::spawn_blocking(move || run_elevated(owner, &id))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result
}

#[cfg(windows)]
fn run_elevated(owner: Option<isize>, id: &str) -> Result<(), String> {
    use windows::{
        core::{HSTRING, PCWSTR},
        Win32::{
            Foundation::{ERROR_CANCELLED, HWND},
            System::Com::{
                CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            UI::{
                Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW},
                WindowsAndMessaging::SW_SHOWNORMAL,
            },
        },
    };
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let verb = HSTRING::from("runas");
    let file = HSTRING::from(exe.as_os_str());
    let parameters = HSTRING::from(format!("{FLAG} {id}"));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        hwnd: owner
            .map(|handle| HWND(handle as *mut std::ffi::c_void))
            .unwrap_or_default(),
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: the strings outlive the call, and COM is set up for this thread only, as
    // ShellExecuteEx asks.
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let result = ShellExecuteExW(&mut info);
        if com.is_ok() {
            CoUninitialize();
        }
        result.map_err(|error| {
            if error.code() == ERROR_CANCELLED.to_hresult() {
                CANCELLED.to_owned()
            } else {
                format!(
                    "Could not open an administrator window: {}",
                    error.message()
                )
            }
        })
    }
}

#[cfg(not(windows))]
fn run_elevated(_owner: Option<isize>, _id: &str) -> Result<(), String> {
    Err("Administrator windows exist on Windows only.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::request_id;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn reads_only_a_uuid_after_the_flag() {
        let id = "0f8fad5b-d9cb-469f-a165-70867728950e";
        assert_eq!(
            request_id(&args(&["app", "--administrator", id])).as_deref(),
            Some(id)
        );
        // Other spellings of the same UUID come back in the one form used for files.
        assert_eq!(
            request_id(&args(&[
                "app",
                "--administrator",
                "{0F8FAD5B-D9CB-469F-A165-70867728950E}"
            ]))
            .as_deref(),
            Some(id)
        );
        assert_eq!(
            request_id(&args(&["app", "--administrator", "..\\..\\secrets"])),
            None
        );
        assert_eq!(request_id(&args(&["app", "--administrator"])), None);
        assert_eq!(request_id(&args(&["app", id])), None);
    }
}

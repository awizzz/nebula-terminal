#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cmdline;
mod custom;
mod profiles;
mod pty;
mod ssh;
mod wsl;

use tauri::{Manager, WebviewWindow};

#[tauri::command]
fn set_window_effect(window: WebviewWindow, mode: String, dark: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_mica, clear_mica};
        let _ = clear_mica(&window);
        if mode == "mica" {
            apply_mica(&window, Some(dark)).map_err(|error| error.to_string())?;
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = (window, mode, dark);

    Ok(())
}

/// The Windows build number. xterm.js matches ConPTY's line wrapping with it.
#[tauri::command]
fn windows_build() -> Option<u32> {
    #[cfg(target_os = "windows")]
    {
        use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
        let key = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
            .ok()?;
        let build: String = key.get_value("CurrentBuildNumber").ok()?;
        build.trim().parse().ok()
    }

    #[cfg(not(target_os = "windows"))]
    None
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(pty::PtyState::default())
        .setup(|app| {
            let path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|directory| directory.join("custom-profiles.json"));
            app.manage(custom::CustomProfiles::new(path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            profiles::detect_profiles,
            custom::list_custom_profiles,
            custom::save_custom_profile,
            custom::delete_custom_profile,
            custom::split_arguments,
            pty::start_session,
            pty::write_session,
            pty::resize_session,
            pty::close_session,
            set_window_effect,
            windows_build,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nebula Terminal");
}

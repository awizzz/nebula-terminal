#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod profiles;
mod pty;

use tauri::WebviewWindow;

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

fn main() {
    tauri::Builder::default()
        .manage(pty::PtyState::default())
        .invoke_handler(tauri::generate_handler![
            profiles::detect_profiles,
            pty::start_session,
            pty::write_session,
            pty::resize_session,
            pty::close_session,
            set_window_effect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nebula Terminal");
}

//! The icon in the notification area, while the quick terminal keeps the app running
//! without a window: it opens a window again, shows the quick terminal, or quits.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};

const ID: &str = "nebula";

pub fn show(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id(ID).is_some() {
        return Ok(());
    }
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "new-window", "New window", true, None::<&str>)?,
            &MenuItem::with_id(app, "quick", "Quick terminal", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Nebula Terminal", true, None::<&str>)?,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id(ID)
        .tooltip("Nebula Terminal")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "new-window" => open_window(app),
            "quick" => crate::quick::toggle(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                open_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn hide(app: &AppHandle) {
    let _ = app.remove_tray_by_id(ID);
}

/// The window that's open, in front, or a new one when there's none.
fn open_window(app: &AppHandle) {
    match crate::windows::launch_target(app) {
        Some(window) => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        None => {
            let _ = crate::windows::open_window(app, None);
        }
    }
}

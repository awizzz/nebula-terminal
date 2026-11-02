//! More than one terminal window. The main window comes from the config; tabs dragged
//! out of a window go to a new one labelled `tabs-<uuid>`, or into another window.
//!
//! A moving tab travels as JSON the page builds (its panes, layout and the screen of
//! each terminal), kept here until the window it goes to asks for it. The shells keep
//! running: their output waits in `pty.rs` until the new pane attaches.

use std::{collections::HashMap, sync::Mutex};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewWindow, WebviewWindowBuilder,
};

pub const MAIN: &str = "main";
const TABS_PREFIX: &str = "tabs-";

/// Windows that hold tabs and keep the app open: not the quick terminal.
pub fn is_terminal_window(label: &str) -> bool {
    label == MAIN || label.starts_with(TABS_PREFIX)
}

/// The window a second launch or a handed-over console opens its tab in.
pub fn launch_target(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN).or_else(|| {
        app.webview_windows()
            .into_iter()
            .filter(|(label, _)| is_terminal_window(label))
            .map(|(_, window)| window)
            .next()
    })
}

#[derive(Default)]
pub struct Transfers(Mutex<HashMap<String, serde_json::Value>>);

/// Opens a terminal window. `transfer` names what it opens with: a moved tab, or the
/// tabs another window had when the app closed.
pub fn open_window(app: &AppHandle, transfer: Option<&str>) -> tauri::Result<WebviewWindow> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    config.label = format!("{TABS_PREFIX}{}", uuid::Uuid::new_v4().simple());
    config.center = false;
    let transfer = serde_json::to_string(&transfer).unwrap_or_else(|_| "null".to_owned());
    let mut script =
        format!("window.__NEBULA_WINDOW__ = 'tabs'; window.__NEBULA_TRANSFER__ = {transfer};");
    let mut builder = WebviewWindowBuilder::from_config(app, &config)?;
    // An admin copy's windows are all admin windows, with the same web data.
    if crate::elevation::is_admin_copy() {
        script.push_str(" window.__NEBULA_ADMIN__ = {};");
        builder = builder.title("Administrator: Nebula Terminal");
        if let Some(dir) = crate::elevation::admin_data_dir(app) {
            builder = builder.data_directory(dir);
        }
    }
    builder.initialization_script(script).build()
}

/// The terminal window under the pointer, other than `except`, for a tab dropped on it.
fn window_at_cursor(app: &AppHandle, except: &str) -> Option<WebviewWindow> {
    let cursor = app.cursor_position().ok()?;
    app.webview_windows().into_values().find(|window| {
        let label = window.label();
        if label == except
            || !is_terminal_window(label)
            || !window.is_visible().unwrap_or(false)
            || window.is_minimized().unwrap_or(true)
        {
            return false;
        }
        let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
            return false;
        };
        let (x, y) = (cursor.x, cursor.y);
        x >= f64::from(position.x)
            && y >= f64::from(position.y)
            && x < f64::from(position.x) + f64::from(size.width)
            && y < f64::from(position.y) + f64::from(size.height)
    })
}

/// Sends a tab to the window under the pointer, or to a new one there. `here` is true
/// when the drop was on the window it came from, which then keeps it.
#[tauri::command]
pub fn move_tab(
    app: AppHandle,
    window: WebviewWindow,
    transfers: State<'_, Transfers>,
    transfer: serde_json::Value,
    new_window: bool,
) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    transfers
        .0
        .lock()
        .map_err(|_| "The tab couldn't be moved.".to_owned())?
        .insert(id.clone(), transfer);
    let target = if new_window {
        None
    } else {
        window_at_cursor(&app, window.label())
    };
    if let Some(target) = target {
        app.emit_to(target.label(), "tab-arrived", &id)
            .map_err(|error| error.to_string())?;
        let _ = target.set_focus();
        return Ok(());
    }
    let opened = open_window(&app, Some(&id)).map_err(|error| error.to_string())?;
    // Under the pointer, so the tab lands where it was dropped.
    if let Ok(cursor) = app.cursor_position() {
        let scale = opened.scale_factor().unwrap_or(1.0);
        let offset = (120.0 * scale, 18.0 * scale);
        let _ = opened.set_position(PhysicalPosition::new(
            (cursor.x - offset.0) as i32,
            (cursor.y - offset.1) as i32,
        ));
    }
    Ok(())
}

/// Whether a tab dragged out of this window is over another terminal window.
#[tauri::command]
pub fn window_under_cursor(app: AppHandle, window: WebviewWindow) -> bool {
    window_at_cursor(&app, window.label()).is_some()
}

/// What a window was opened or sent to receive. Each transfer is given out once.
#[tauri::command]
pub fn take_transfer(transfers: State<'_, Transfers>, id: String) -> Option<serde_json::Value> {
    transfers.0.lock().ok()?.remove(&id)
}

/// Opens one window per set of saved tabs, for the windows that were open at exit.
#[tauri::command]
pub fn restore_windows(
    app: AppHandle,
    transfers: State<'_, Transfers>,
    snapshots: Vec<serde_json::Value>,
) -> Result<(), String> {
    for snapshot in snapshots.into_iter().take(8) {
        let id = uuid::Uuid::new_v4().simple().to_string();
        transfers
            .0
            .lock()
            .map_err(|_| "Windows couldn't be restored.".to_owned())?
            .insert(id.clone(), snapshot);
        open_window(&app, Some(&id)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// How many terminal windows are open, this one included.
#[tauri::command]
pub fn terminal_window_count(app: AppHandle) -> usize {
    app.webview_windows()
        .keys()
        .filter(|label| is_terminal_window(label))
        .count()
}

#[cfg(test)]
mod tests {
    use super::is_terminal_window;

    #[test]
    fn the_quick_terminal_is_not_a_terminal_window() {
        assert!(is_terminal_window("main"));
        assert!(is_terminal_window("tabs-0f8fad5bd9cb469fa16570867728950e"));
        assert!(!is_terminal_window("quick"));
        assert!(!is_terminal_window("mainly"));
    }
}

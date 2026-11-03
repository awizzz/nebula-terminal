//! Saving what a pane shows to a text file the user picks, at once or as it comes (a log).

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// More output than a pane holds; a mistake beyond this.
const MAX_BYTES: usize = 64 * 1024 * 1024;

/// A name that can't point somewhere else: no folders, no reserved characters.
fn file_name(name: &str, extension: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"\/:*?"<>|"#.contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let clean = clean
        .trim()
        .trim_matches('.')
        .chars()
        .take(120)
        .collect::<String>();
    let clean = if clean.is_empty() {
        "output".to_owned()
    } else {
        clean
    };
    if clean
        .to_ascii_lowercase()
        .ends_with(&format!(".{extension}"))
    {
        clean
    } else {
        format!("{clean}.{extension}")
    }
}

/// Asks where to save, with `name` suggested. None when the user cancels.
async fn choose_file(
    app: AppHandle,
    window: WebviewWindow,
    title: &'static str,
    name: String,
    filter: (&'static str, &'static [&'static str]),
) -> Result<Option<PathBuf>, String> {
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .add_filter(filter.0, filter.1)
            .set_file_name(name)
            .set_can_create_directories(true)
            .blocking_save_file()
    })
    .await
    .map_err(|error| error.to_string())?;
    Ok(chosen.and_then(|path| path.into_path().ok()))
}

const TEXT: (&str, &[&str]) = ("Text", &["txt", "log"]);
const WEB_PAGE: (&str, &[&str]) = ("Web page", &["html", "htm"]);

/// Asks where to save `contents`, plain text or a web page, and saves it there. None
/// when the user cancels.
#[tauri::command]
pub async fn save_text_file(
    app: AppHandle,
    window: WebviewWindow,
    name: String,
    contents: String,
    html: Option<bool>,
) -> Result<Option<String>, String> {
    if contents.len() > MAX_BYTES {
        return Err("This output is too large to save.".to_owned());
    }
    let (extension, filter) = if html == Some(true) {
        ("html", WEB_PAGE)
    } else {
        ("txt", TEXT)
    };
    let Some(path) = choose_file(
        app,
        window,
        "Save output",
        file_name(&name, extension),
        filter,
    )
    .await?
    else {
        return Ok(None);
    };
    std::fs::write(&path, contents)
        .map_err(|error| format!("Could not save the output: {error}"))?;
    Ok(Some(path.display().to_string()))
}

/// The logs being written. The file stays open here, and the webview only names a log by
/// its number, so it can't append to a file the user didn't pick.
#[derive(Default)]
pub struct Logs {
    open: Mutex<HashMap<u32, std::fs::File>>,
    next: AtomicU32,
}

/// What one append may carry: far more than a second of output.
const MAX_APPEND: usize = 8 * 1024 * 1024;

#[derive(serde::Serialize)]
pub struct StartedLog {
    id: u32,
    path: String,
}

/// Asks where to log a pane's output and creates that file. None when the user cancels.
#[tauri::command]
pub async fn start_log(
    app: AppHandle,
    window: WebviewWindow,
    logs: State<'_, Logs>,
    name: String,
) -> Result<Option<StartedLog>, String> {
    let Some(path) =
        choose_file(app, window, "Log output to", file_name(&name, "log"), TEXT).await?
    else {
        return Ok(None);
    };
    let file = std::fs::File::create(&path)
        .map_err(|error| format!("Could not create the log: {error}"))?;
    let id = logs.next.fetch_add(1, Ordering::Relaxed);
    logs.open
        .lock()
        .map_err(|error| error.to_string())?
        .insert(id, file);
    Ok(Some(StartedLog {
        id,
        path: path.display().to_string(),
    }))
}

#[tauri::command]
pub fn append_log(logs: State<'_, Logs>, id: u32, text: String) -> Result<(), String> {
    if text.len() > MAX_APPEND {
        return Err("Too much output at once for the log.".to_owned());
    }
    let mut open = logs.open.lock().map_err(|error| error.to_string())?;
    let file = open.get_mut(&id).ok_or("This log was stopped.")?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("Could not write the log: {error}"))
}

#[tauri::command]
pub fn stop_log(logs: State<'_, Logs>, id: u32) {
    if let Ok(mut open) = logs.open.lock() {
        open.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::file_name;

    #[test]
    fn names_stay_names() {
        assert_eq!(
            file_name("Nebula 2026-11-02", "txt"),
            "Nebula 2026-11-02.txt"
        );
        assert_eq!(
            file_name("..\\..\\Windows\\win.ini", "txt"),
            "-..-Windows-win.ini.txt"
        );
        assert_eq!(file_name("  ", "txt"), "output.txt");
        assert_eq!(file_name("build.TXT", "txt"), "build.TXT");
        assert_eq!(file_name("a:b|c?", "txt"), "a-b-c-.txt");
        assert_eq!(file_name("deploy", "log"), "deploy.log");
    }
}

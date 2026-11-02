//! Saving what a pane shows to a text file the user picks.

use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// More output than a pane holds; a mistake beyond this.
const MAX_BYTES: usize = 64 * 1024 * 1024;

/// A name that can't point somewhere else: no folders, no reserved characters.
fn file_name(name: &str) -> String {
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
    if clean.to_ascii_lowercase().ends_with(".txt") {
        clean
    } else {
        format!("{clean}.txt")
    }
}

/// Asks where to save `contents`, and saves it there. None when the user cancels.
#[tauri::command]
pub async fn save_text_file(
    app: AppHandle,
    window: WebviewWindow,
    name: String,
    contents: String,
) -> Result<Option<String>, String> {
    if contents.len() > MAX_BYTES {
        return Err("This output is too large to save.".to_owned());
    }
    let name = file_name(&name);
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_parent(&window)
            .set_title("Save output")
            .add_filter("Text", &["txt", "log"])
            .set_file_name(name)
            .set_can_create_directories(true)
            .blocking_save_file()
    })
    .await
    .map_err(|error| error.to_string())?;
    let Some(path) = chosen.and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };
    std::fs::write(&path, contents)
        .map_err(|error| format!("Could not save the output: {error}"))?;
    Ok(Some(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::file_name;

    #[test]
    fn names_stay_names() {
        assert_eq!(file_name("Nebula 2026-11-02"), "Nebula 2026-11-02.txt");
        assert_eq!(
            file_name("..\\..\\Windows\\win.ini"),
            "-..-Windows-win.ini.txt"
        );
        assert_eq!(file_name("  "), "output.txt");
        assert_eq!(file_name("build.TXT"), "build.TXT");
        assert_eq!(file_name("a:b|c?"), "a-b-c-.txt");
    }
}

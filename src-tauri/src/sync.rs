//! Settings sync through a file the user picks, in OneDrive, Dropbox or a Git
//! repository: the page writes its settings there and reads them back when the file
//! changes. Nothing else is synced and nothing goes anywhere else.

use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// A settings file with a background image fits easily; anything bigger isn't one.
const MAX_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFile {
    contents: String,
    /// Milliseconds since 1970, to notice another PC writing the file.
    modified: u64,
}

/// Only `.json` files, by absolute path.
fn checked(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    let json = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if !path.is_absolute() || !json {
        return Err("Choose a .json file.".to_owned());
    }
    Ok(path)
}

fn modified(path: &Path) -> Option<u64> {
    let time = fs::metadata(path).ok()?.modified().ok()?;
    Some(time.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

/// Asks where the settings file goes. An existing file is fine: its settings are used.
#[tauri::command]
pub async fn choose_sync_file(app: AppHandle, window: WebviewWindow) -> Option<String> {
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_parent(&window)
            .set_title("Sync settings with a file")
            .add_filter("Settings", &["json"])
            .set_file_name("nebula-terminal-settings.json")
            .set_can_create_directories(true)
            .blocking_save_file()
    })
    .await
    .ok()??;
    chosen
        .into_path()
        .ok()
        .map(|path| path.display().to_string())
}

/// The file's contents, or None when it isn't there yet.
#[tauri::command]
pub fn read_sync_file(path: String) -> Result<Option<SyncFile>, String> {
    let path = checked(&path)?;
    let Ok(metadata) = fs::metadata(&path) else {
        return Ok(None);
    };
    if metadata.len() > MAX_BYTES {
        return Err("This file is too large to be a settings file.".to_owned());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read the settings file: {error}"))?;
    Ok(Some(SyncFile {
        contents,
        modified: modified(&path).unwrap_or(0),
    }))
}

/// Writes the file through a temporary one, so a sync client never uploads half of it.
#[tauri::command]
pub fn write_sync_file(path: String, contents: String) -> Result<u64, String> {
    let path = checked(&path)?;
    if contents.len() as u64 > MAX_BYTES {
        return Err("These settings are too large to sync.".to_owned());
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, contents)
        .and_then(|()| fs::rename(&temporary, &path))
        .map_err(|error| {
            let _ = fs::remove_file(&temporary);
            format!("Could not write the settings file: {error}")
        })?;
    Ok(modified(&path).unwrap_or(0))
}

/// When the file last changed, to look for changes made on another PC.
#[tauri::command]
pub fn sync_file_modified(path: String) -> Option<u64> {
    modified(&checked(&path).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_only_json_files() {
        let folder = std::env::temp_dir().join(format!("nebula-sync-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let file = folder.join("settings.json");
        let path = file.display().to_string();
        assert!(read_sync_file(path.clone()).unwrap().is_none());
        let modified = write_sync_file(path.clone(), "{\"a\":1}".into()).unwrap();
        assert!(modified > 0);
        let read = read_sync_file(path.clone()).unwrap().unwrap();
        assert_eq!(read.contents, "{\"a\":1}");
        assert_eq!(sync_file_modified(path), Some(read.modified));
        assert!(!folder.join("settings.json.tmp").exists());

        assert!(
            write_sync_file(folder.join("notes.txt").display().to_string(), "x".into()).is_err()
        );
        assert!(read_sync_file("relative.json".into()).is_err());
        let _ = fs::remove_dir_all(folder);
    }
}

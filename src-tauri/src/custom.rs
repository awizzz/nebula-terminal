//! Profiles the user adds in Settings. They live in a JSON file in the app's config
//! folder, written only here and validated on the way in and on the way out. The UI
//! edits them through the commands below and starts them by id, like any other profile.

use crate::{cmdline, profiles};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::State;

pub const ID_PREFIX: &str = "custom:";
const FILE_VERSION: u32 = 1;
const MAX_PROFILES: usize = 50;
const MAX_NAME: usize = 60;
const MAX_PATH: usize = 1024;
const MAX_ARGUMENTS: usize = 64;
const MAX_ARGUMENTS_LENGTH: usize = 4096;
const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProfile {
    pub id: String,
    pub name: String,
    /// Absolute path or a program name looked up on PATH; `%VAR%` and `~` are expanded.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub accent: String,
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    profiles: Vec<CustomProfile>,
}

/// What the editor sends: the arguments as one line, split here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub executable: String,
    pub arguments: String,
    pub cwd: String,
    pub accent: String,
}

/// A stored profile plus its arguments as one line, for the editor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProfileView {
    #[serde(flatten)]
    pub profile: CustomProfile,
    pub arguments: String,
}

impl From<CustomProfile> for CustomProfileView {
    fn from(profile: CustomProfile) -> Self {
        let arguments = cmdline::join(&profile.args);
        Self { profile, arguments }
    }
}

/// A validation failure the editor can show next to the field it belongs to.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: Option<&'static str>,
    pub message: String,
}

impl FieldError {
    fn on(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field: Some(field),
            message: message.into(),
        }
    }

    fn general(message: impl Into<String>) -> Self {
        Self {
            field: None,
            message: message.into(),
        }
    }
}

fn has_control_characters(value: &str) -> bool {
    value.chars().any(char::is_control)
}

pub fn is_valid_id(id: &str) -> bool {
    id.strip_prefix(ID_PREFIX).is_some_and(|rest| {
        rest.len() == 36
            && uuid::Uuid::try_parse(rest).is_ok_and(|uuid| uuid.hyphenated().to_string() == rest)
    })
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Finds the program a profile runs: an absolute path to an existing file, or a bare
/// name found on PATH. Relative paths with folders are refused, so what runs never
/// depends on the current directory.
pub fn resolve_executable(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() || has_control_characters(value) {
        return None;
    }
    let expanded = profiles::expand_directory(value);
    if expanded.is_absolute() {
        return expanded.is_file().then_some(expanded);
    }
    let text = expanded.to_string_lossy();
    if text.contains(['/', '\\', ':']) {
        return None;
    }
    profiles::find_executable(&text)
}

/// Checks the shape of a stored entry. Used when reading the file, where a program that
/// has since been uninstalled is not a reason to drop the profile.
fn is_well_formed(profile: &CustomProfile) -> bool {
    let name = profile.name.trim();
    is_valid_id(&profile.id)
        && !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && !has_control_characters(&profile.name)
        && !profile.executable.trim().is_empty()
        && profile.executable.len() <= MAX_PATH
        && !has_control_characters(&profile.executable)
        && profile.args.len() <= MAX_ARGUMENTS
        && profile.args.iter().map(String::len).sum::<usize>() <= MAX_ARGUMENTS_LENGTH
        && !profile.args.iter().any(|arg| arg.contains('\0'))
        && profile
            .cwd
            .as_ref()
            .is_none_or(|cwd| cwd.len() <= MAX_PATH && !has_control_characters(cwd))
        && is_hex_color(&profile.accent)
}

pub fn split_arguments_line(line: &str) -> Result<Vec<String>, String> {
    if line.len() > MAX_ARGUMENTS_LENGTH {
        return Err(format!(
            "Arguments can be at most {MAX_ARGUMENTS_LENGTH} characters."
        ));
    }
    if line.contains('\0') {
        return Err("Arguments cannot contain NUL characters.".into());
    }
    let args = cmdline::split(line);
    if args.len() > MAX_ARGUMENTS {
        return Err(format!("Use at most {MAX_ARGUMENTS} arguments."));
    }
    Ok(args)
}

/// Validates what the editor sent and turns it into a stored profile with the given id.
/// `find_program` decides whether the program exists (see [`resolve_executable`]).
pub fn validate(
    input: &CustomProfileInput,
    id: String,
    find_program: impl Fn(&str) -> Option<PathBuf>,
) -> Result<CustomProfile, FieldError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(FieldError::on("name", "Enter a name."));
    }
    if name.chars().count() > MAX_NAME || has_control_characters(name) {
        return Err(FieldError::on(
            "name",
            format!("Use up to {MAX_NAME} characters, on one line."),
        ));
    }

    let executable = input.executable.trim().trim_matches('"').trim();
    if executable.is_empty() {
        return Err(FieldError::on("executable", "Enter the program to run."));
    }
    if executable.len() > MAX_PATH || has_control_characters(executable) {
        return Err(FieldError::on("executable", "This path is not valid."));
    }
    let expanded = profiles::expand_directory(executable);
    let bare = !expanded.to_string_lossy().contains(['/', '\\', ':']);
    if !expanded.is_absolute() && !bare {
        return Err(FieldError::on(
            "executable",
            "Use a full path, such as C:\\Tools\\app.exe, or a program name found on PATH.",
        ));
    }
    if find_program(executable).is_none() {
        return Err(FieldError::on(
            "executable",
            if bare {
                format!("No program named \u{201c}{executable}\u{201d} was found on PATH.")
            } else {
                "This file does not exist.".to_owned()
            },
        ));
    }

    let args = split_arguments_line(&input.arguments)
        .map_err(|message| FieldError::on("arguments", message))?;

    let cwd = input.cwd.trim();
    let cwd = if cwd.is_empty() {
        None
    } else {
        if cwd.len() > MAX_PATH || has_control_characters(cwd) {
            return Err(FieldError::on("cwd", "This path is not valid."));
        }
        if !profiles::expand_directory(cwd).is_dir() {
            return Err(FieldError::on("cwd", "This folder does not exist."));
        }
        Some(cwd.to_owned())
    };

    let accent = input.accent.trim().to_ascii_lowercase();
    if !is_hex_color(&accent) {
        return Err(FieldError::on("accent", "Use a color like #5b8def."));
    }

    Ok(CustomProfile {
        id,
        name: name.to_owned(),
        executable: executable.to_owned(),
        args,
        cwd,
        accent,
    })
}

/// Adds or replaces a profile in the list and returns the saved entry.
pub fn upsert(
    profiles: &mut Vec<CustomProfile>,
    input: &CustomProfileInput,
    find_program: impl Fn(&str) -> Option<PathBuf>,
) -> Result<CustomProfile, FieldError> {
    let existing = match input.id.as_deref() {
        Some(id) => Some(
            profiles
                .iter()
                .position(|profile| profile.id == id)
                .ok_or_else(|| FieldError::general("This profile was removed in the meantime."))?,
        ),
        None if profiles.len() >= MAX_PROFILES => {
            return Err(FieldError::general(format!(
                "You can have up to {MAX_PROFILES} custom profiles."
            )));
        }
        None => None,
    };
    let id = match existing {
        Some(index) => profiles[index].id.clone(),
        None => format!("{ID_PREFIX}{}", uuid::Uuid::new_v4().hyphenated()),
    };
    let profile = validate(input, id, find_program)?;
    match existing {
        Some(index) => profiles[index] = profile.clone(),
        None => profiles.push(profile.clone()),
    }
    Ok(profile)
}

/// Reads the profiles file. A missing or unreadable file means no custom profiles;
/// entries that are not well formed are dropped.
pub fn load_from(path: &Path) -> Vec<CustomProfile> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut text = String::new();
    if file.take(MAX_FILE_BYTES).read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    let Ok(stored) = serde_json::from_str::<StoreFile>(&text) else {
        return Vec::new();
    };
    let mut profiles: Vec<CustomProfile> = Vec::new();
    for profile in stored.profiles {
        if is_well_formed(&profile) && !profiles.iter().any(|known| known.id == profile.id) {
            profiles.push(profile);
        }
    }
    profiles.truncate(MAX_PROFILES);
    profiles
}

/// Writes the file through a temporary file so a crash never leaves half a file behind.
fn write_to(path: &Path, profiles: &[CustomProfile]) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("Could not save profiles: {error}");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    let text = serde_json::to_string_pretty(&StoreFile {
        version: FILE_VERSION,
        profiles: profiles.to_vec(),
    })
    .map_err(|error| format!("Could not save profiles: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text).map_err(failed)?;
    fs::rename(&temporary, path).map_err(failed)
}

/// Where custom profiles are kept. The lock serializes edits; reads need none because
/// the file is replaced in one step.
pub struct CustomProfiles {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

impl CustomProfiles {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn load(&self) -> Vec<CustomProfile> {
        self.path().map(load_from).unwrap_or_default()
    }

    fn edit<T, E: From<String>>(
        &self,
        change: impl FnOnce(&mut Vec<CustomProfile>) -> Result<T, E>,
    ) -> Result<T, E> {
        let path = self
            .path()
            .ok_or_else(|| "The settings folder is not available.".to_owned())?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Internal profile lock is poisoned.".to_owned())?;
        let mut profiles = load_from(path);
        let result = change(&mut profiles)?;
        write_to(path, &profiles)?;
        Ok(result)
    }

    /// The stored profile with this id, with its program located now.
    pub fn resolve(&self, id: &str) -> Result<profiles::ResolvedProfile, String> {
        let profile = self
            .load()
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| format!("Unknown profile '{id}'."))?;
        let executable = resolve_executable(&profile.executable).ok_or_else(|| {
            format!(
                "The program of '{}' was not found: {}",
                profile.name, profile.executable
            )
        })?;
        Ok(profiles::ResolvedProfile {
            executable,
            args: profile.args,
            cwd: profile.cwd,
        })
    }
}

impl From<String> for FieldError {
    fn from(message: String) -> Self {
        FieldError::general(message)
    }
}

#[tauri::command]
pub async fn list_custom_profiles(
    store: State<'_, CustomProfiles>,
) -> Result<Vec<CustomProfileView>, String> {
    Ok(store.load().into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn save_custom_profile(
    profile: CustomProfileInput,
    store: State<'_, CustomProfiles>,
) -> Result<CustomProfileView, FieldError> {
    store
        .edit(|profiles| upsert(profiles, &profile, resolve_executable))
        .map(Into::into)
}

#[tauri::command]
pub async fn delete_custom_profile(
    id: String,
    store: State<'_, CustomProfiles>,
) -> Result<(), String> {
    store.edit(|profiles| {
        profiles.retain(|profile| profile.id != id);
        Ok::<(), String>(())
    })
}

/// Lets the editor preview how its arguments line will be split.
#[tauri::command]
pub fn split_arguments(line: String) -> Result<Vec<String>, String> {
    split_arguments_line(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            env::temp_dir().join(format!("nebula-custom-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn input(name: &str, executable: &str) -> CustomProfileInput {
        CustomProfileInput {
            id: None,
            name: name.into(),
            executable: executable.into(),
            arguments: String::new(),
            cwd: String::new(),
            accent: "#5B8DEF".into(),
        }
    }

    fn found(_: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/bin/tool"))
    }

    fn missing(_: &str) -> Option<PathBuf> {
        None
    }

    fn id() -> String {
        format!("{ID_PREFIX}{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn validates_and_normalizes_a_profile() {
        let mut request = input("  Python  ", "\"python.exe\"");
        request.arguments = r#"-m "http.server" 8000"#.into();
        let profile = validate(&request, id(), found).unwrap();
        assert_eq!(profile.name, "Python");
        assert_eq!(profile.executable, "python.exe");
        assert_eq!(profile.args, vec!["-m", "http.server", "8000"]);
        assert_eq!(profile.cwd, None);
        assert_eq!(profile.accent, "#5b8def");
    }

    #[test]
    fn reports_the_field_that_is_wrong() {
        let field = |request: CustomProfileInput, find: fn(&str) -> Option<PathBuf>| {
            validate(&request, id(), find).unwrap_err().field
        };
        assert_eq!(field(input(" ", "tool"), found), Some("name"));
        assert_eq!(field(input(&"x".repeat(61), "tool"), found), Some("name"));
        assert_eq!(field(input("Line\nbreak", "tool"), found), Some("name"));
        assert_eq!(field(input("Tool", ""), found), Some("executable"));
        assert_eq!(
            field(input("Tool", "bin/tool.exe"), found),
            Some("executable")
        );
        assert_eq!(
            field(input("Tool", "..\\tool.exe"), found),
            Some("executable")
        );
        assert_eq!(
            field(input("Tool", "C:tool.exe"), found),
            Some("executable")
        );
        assert_eq!(field(input("Tool", "tool"), missing), Some("executable"));

        let mut request = input("Tool", "tool");
        request.accent = "blue".into();
        assert_eq!(field(request, found), Some("accent"));

        let mut request = input("Tool", "tool");
        request.arguments = "a ".repeat(MAX_ARGUMENTS + 1);
        assert_eq!(field(request, found), Some("arguments"));

        let mut request = input("Tool", "tool");
        request.cwd = "/surely/not/a/real/folder".into();
        assert_eq!(field(request, found), Some("cwd"));
    }

    #[test]
    fn accepts_an_existing_starting_folder() {
        let folder = scratch("cwd");
        let mut request = input("Tool", "tool");
        request.cwd = folder.to_string_lossy().into_owned();
        assert_eq!(
            validate(&request, id(), found).unwrap().cwd,
            Some(request.cwd)
        );
    }

    #[test]
    fn finds_programs_by_absolute_path_only_when_they_exist() {
        let folder = scratch("exe");
        let program = folder.join("tool.exe");
        fs::write(&program, b"").unwrap();
        assert_eq!(
            resolve_executable(&program.to_string_lossy()),
            Some(program.clone())
        );
        assert_eq!(
            resolve_executable(&folder.join("other.exe").to_string_lossy()),
            None
        );
        assert_eq!(resolve_executable(&folder.to_string_lossy()), None);
        assert_eq!(resolve_executable("sub/tool.exe"), None);
        assert_eq!(resolve_executable(""), None);
    }

    #[test]
    fn checks_ids() {
        assert!(is_valid_id("custom:0f8fad5b-d9cb-469f-a165-70867728950e"));
        assert!(!is_valid_id("custom:0F8FAD5B-D9CB-469F-A165-70867728950E"));
        assert!(!is_valid_id("custom:0f8fad5bd9cb469fa16570867728950e"));
        assert!(!is_valid_id("custom:../../etc"));
        assert!(!is_valid_id("pwsh"));
    }

    #[test]
    fn adds_updates_and_limits_profiles() {
        let mut profiles = Vec::new();
        let added = upsert(&mut profiles, &input("One", "tool"), found).unwrap();
        assert!(is_valid_id(&added.id));

        let mut rename = input("Uno", "tool");
        rename.id = Some(added.id.clone());
        let updated = upsert(&mut profiles, &rename, found).unwrap();
        assert_eq!(updated.id, added.id);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Uno");

        let mut gone = input("Gone", "tool");
        gone.id = Some(id());
        assert_eq!(upsert(&mut profiles, &gone, found).unwrap_err().field, None);

        while profiles.len() < MAX_PROFILES {
            upsert(&mut profiles, &input("More", "tool"), found).unwrap();
        }
        assert!(upsert(&mut profiles, &input("Too many", "tool"), found).is_err());
        // Editing still works at the limit.
        assert!(upsert(&mut profiles, &rename, found).is_ok());
    }

    #[test]
    fn a_failed_validation_changes_nothing() {
        let mut profiles = Vec::new();
        let added = upsert(&mut profiles, &input("One", "tool"), found).unwrap();
        let mut broken = input("", "tool");
        broken.id = Some(added.id.clone());
        assert!(upsert(&mut profiles, &broken, found).is_err());
        assert_eq!(profiles, vec![added]);
    }

    #[test]
    fn saves_and_loads_the_file() {
        let folder = scratch("store");
        let path = folder.join("nested").join("custom-profiles.json");
        assert!(load_from(&path).is_empty());

        let mut profiles = Vec::new();
        let mut request = input("Python", "python");
        request.arguments = r#"-X "dev mode""#.into();
        upsert(&mut profiles, &request, found).unwrap();
        write_to(&path, &profiles).unwrap();
        assert_eq!(load_from(&path), profiles);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn drops_malformed_entries_when_loading() {
        let folder = scratch("malformed");
        let path = folder.join("custom-profiles.json");
        let good = id();
        let text = format!(
            r##"{{"version":1,"profiles":[
                {{"id":"{good}","name":"Good","executable":"tool","args":["-x"],"accent":"#112233"}},
                {{"id":"{good}","name":"Duplicate","executable":"tool","accent":"#112233"}},
                {{"id":"custom:nope","name":"Bad id","executable":"tool","accent":"#112233"}},
                {{"id":"{}","name":"Bad color","executable":"tool","accent":"red"}},
                {{"id":"{}","name":"","executable":"tool","accent":"#112233"}}
            ]}}"##,
            id(),
            id()
        );
        fs::write(&path, text).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Good");

        fs::write(&path, "not json").unwrap();
        assert!(load_from(&path).is_empty());
    }

    #[test]
    fn editor_view_round_trips_arguments() {
        let profile = CustomProfile {
            id: id(),
            name: "Tool".into(),
            executable: "tool".into(),
            args: vec!["--path".into(), r"C:\My Files\".into(), String::new()],
            cwd: None,
            accent: "#112233".into(),
        };
        let view = CustomProfileView::from(profile.clone());
        assert_eq!(split_arguments_line(&view.arguments).unwrap(), profile.args);
    }

    #[test]
    fn the_store_resolves_saved_profiles_only() {
        let folder = scratch("resolve");
        let program = folder.join("tool.exe");
        fs::write(&program, b"").unwrap();
        let store = CustomProfiles::new(Some(folder.join("custom-profiles.json")));
        let mut request = input("Tool", &program.to_string_lossy());
        request.arguments = "-a b".into();
        request.cwd = folder.to_string_lossy().into_owned();
        let saved = store
            .edit(|profiles| upsert(profiles, &request, resolve_executable))
            .unwrap();

        let resolved = store.resolve(&saved.id).unwrap();
        assert_eq!(resolved.executable, program);
        assert_eq!(resolved.args, vec!["-a", "b"]);
        assert_eq!(resolved.cwd.as_deref(), Some(request.cwd.as_str()));
        assert!(store.resolve(&id()).is_err());

        fs::remove_file(&program).unwrap();
        assert!(store.resolve(&saved.id).unwrap_err().contains("not found"));
    }
}

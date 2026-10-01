//! Checks GitHub for a newer release and installs it.
//!
//! The webview only asks "is there an update?" and "install it". Which file to
//! download is decided here, from the release itself, and the download must match
//! the release's SHA256SUMS.txt before it runs.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tauri::AppHandle;

const REPOSITORY: &str = "awizzz/nebula-shell";
/// Installers are around 10 MB; anything far bigger is not ours.
const MAX_DOWNLOAD: u64 = 300 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallKind {
    /// The per-user setup.exe: updated in place, then relaunched.
    Installer,
    /// The MSI: updated through msiexec.
    Msi,
    /// The portable zip, or a build from source: the user downloads it.
    Manual,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: String,
    pub url: String,
    pub install: InstallKind,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .user_agent(format!("NebulaTerminal/{}", env!("CARGO_PKG_VERSION")))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

fn latest_release(agent: &ureq::Agent) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("Could not reach GitHub: {error}"))?
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_json::<Release>()
        .map_err(|error| format!("Unexpected answer from GitHub: {error}"))
}

/// The release's version when it is newer than `current`.
fn newer_version(tag: &str, current: &str) -> Option<semver::Version> {
    let version = semver::Version::parse(tag.trim().trim_start_matches('v')).ok()?;
    let current = semver::Version::parse(current).ok()?;
    (version > current && version.pre.is_empty()).then_some(version)
}

/// How this copy was installed. The NSIS installer leaves `uninstall.exe` next to
/// the app; the MSI bundle marks the executable it ships.
fn install_kind() -> InstallKind {
    let Some(directory) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return InstallKind::Manual;
    };
    if directory.join("uninstall.exe").is_file() {
        return InstallKind::Installer;
    }
    if cfg!(windows)
        && tauri::utils::platform::bundle_type() == Some(tauri::utils::config::BundleType::Msi)
    {
        return InstallKind::Msi;
    }
    InstallKind::Manual
}

fn installer_asset(release: &Release, kind: InstallKind) -> Option<&Asset> {
    release.assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        match kind {
            InstallKind::Installer => name.ends_with("_x64-setup.exe"),
            InstallKind::Msi => name.ends_with(".msi") && name.contains("_x64"),
            InstallKind::Manual => false,
        }
    })
}

/// GitHub publishes `Nebula Terminal_1.1.0.exe` as `Nebula.Terminal_1.1.0.exe`.
fn published_name(name: &str) -> String {
    name.replace(' ', ".")
}

/// Finds `name` in a `sha256sum`-style list (1.1.0 listed files under their names
/// before GitHub renamed them).
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (published_name(file) == published_name(name)
            && hash.len() == 64
            && hash.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
    })
}

/// Downloads `url` to `path` and returns the SHA-256 of what was written.
fn download(agent: &ureq::Agent, url: &str, path: &Path, limit: u64) -> Result<String, String> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| format!("Download failed: {error}"))?;
    let mut reader = response.body_mut().with_config().limit(limit).reader();
    let mut file =
        File::create(path).map_err(|error| format!("Cannot save the update: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("Download failed: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|error| format!("Cannot save the update: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Cannot save the update: {error}"))?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn check() -> Result<Option<UpdateInfo>, String> {
    let agent = agent();
    let release = latest_release(&agent)?;
    let current = env!("CARGO_PKG_VERSION");
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some(version) = newer_version(&release.tag_name, current) else {
        return Ok(None);
    };
    let kind = install_kind();
    let install = if installer_asset(&release, kind).is_some() {
        kind
    } else {
        InstallKind::Manual
    };
    let notes: String = release
        .body
        .unwrap_or_default()
        .chars()
        .take(4000)
        .collect();
    Ok(Some(UpdateInfo {
        version: version.to_string(),
        current: current.to_owned(),
        notes,
        url: release.html_url,
        install,
    }))
}

/// Downloads and verifies the installer, then returns its path and kind.
fn prepare() -> Result<(PathBuf, InstallKind), String> {
    let agent = agent();
    let release = latest_release(&agent)?;
    if newer_version(&release.tag_name, env!("CARGO_PKG_VERSION")).is_none() {
        return Err("Nebula Terminal is already up to date.".into());
    }
    let kind = install_kind();
    let asset = installer_asset(&release, kind)
        .ok_or("This copy can't update itself. Download the new version from GitHub.")?;
    if asset.size > MAX_DOWNLOAD {
        return Err("The update is larger than expected; it was not downloaded.".into());
    }
    let sums_asset = release
        .assets
        .iter()
        .find(|candidate| candidate.name == "SHA256SUMS.txt")
        .ok_or("The release has no SHA256SUMS.txt, so the update can't be verified.")?;

    let directory = std::env::temp_dir().join("NebulaTerminal-update");
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).map_err(|error| format!("Cannot save the update: {error}"))?;
    let sums_path = directory.join("SHA256SUMS.txt");
    download(
        &agent,
        &sums_asset.browser_download_url,
        &sums_path,
        64 * 1024,
    )?;
    let sums = fs::read_to_string(&sums_path).map_err(|error| error.to_string())?;
    let expected = expected_hash(&sums, &asset.name)
        .ok_or("SHA256SUMS.txt does not list the installer, so the update can't be verified.")?;

    // Only the file name from the release is used, never a path.
    let file_name = Path::new(&asset.name)
        .file_name()
        .ok_or("Unexpected installer name.")?;
    let installer = directory.join(file_name);
    let actual = download(
        &agent,
        &asset.browser_download_url,
        &installer,
        MAX_DOWNLOAD,
    )?;
    if actual != expected {
        let _ = fs::remove_file(&installer);
        return Err("The downloaded update does not match its checksum and was deleted.".into());
    }
    Ok((installer, kind))
}

fn launch(installer: &Path, kind: InstallKind) -> Result<(), String> {
    let mut command = match kind {
        // Passive install with a progress bar, keep shortcuts, start the app afterwards.
        InstallKind::Installer => {
            let mut command = Command::new(installer);
            command.args(["/P", "/R", "/UPDATE"]);
            command
        }
        InstallKind::Msi => {
            let mut command = Command::new("msiexec.exe");
            command
                .arg("/i")
                .arg(installer)
                .args(["/passive", "AUTOLAUNCHAPP=True"]);
            command
        }
        InstallKind::Manual => return Err("This copy can't update itself.".into()),
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start the installer: {error}"))
}

#[tauri::command]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    tauri::async_runtime::spawn_blocking(check)
        .await
        .map_err(|error| error.to_string())?
}

/// Downloads, verifies and starts the installer, then quits so it can replace the app.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let (installer, kind) = tauri::async_runtime::spawn_blocking(prepare)
        .await
        .map_err(|error| error.to_string())??;
    launch(&installer, kind)?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert_eq!(
            newer_version("v1.1.0", "1.0.0").map(|v| v.to_string()),
            Some("1.1.0".into())
        );
        assert!(newer_version("v1.0.0", "1.0.0").is_none());
        assert!(newer_version("v0.9.9", "1.0.0").is_none());
        assert!(newer_version("v2.0.0-beta.1", "1.0.0").is_none());
        assert!(newer_version("nightly", "1.0.0").is_none());
    }

    #[test]
    fn reads_checksum_lists() {
        let sums = format!(
            "{}  Nebula.Terminal_1.1.0_x64-setup.exe\n{} *Nebula.Terminal_1.1.0_x64_en-US.msi\n",
            "a".repeat(64),
            "B".repeat(64)
        );
        assert_eq!(
            expected_hash(&sums, "Nebula.Terminal_1.1.0_x64-setup.exe"),
            Some("a".repeat(64))
        );
        assert_eq!(
            expected_hash(&sums, "Nebula.Terminal_1.1.0_x64_en-US.msi"),
            Some("b".repeat(64))
        );
        assert_eq!(expected_hash(&sums, "other.zip"), None);
        let spaced = format!("{}  Nebula Terminal_1.1.0_x64-setup.exe\n", "c".repeat(64));
        assert_eq!(
            expected_hash(&spaced, "Nebula.Terminal_1.1.0_x64-setup.exe"),
            Some("c".repeat(64))
        );
        assert_eq!(expected_hash("short  file.exe", "file.exe"), None);
    }

    #[test]
    fn picks_the_installer_for_this_copy() {
        let asset = |name: &str| Asset {
            name: name.into(),
            browser_download_url: String::new(),
            size: 1,
        };
        let release = Release {
            tag_name: "v1.1.0".into(),
            html_url: String::new(),
            body: None,
            draft: false,
            prerelease: false,
            assets: vec![
                asset("Nebula-Terminal-1.1.0-windows-x64-portable.zip"),
                asset("Nebula.Terminal_1.1.0_x64-setup.exe"),
                asset("Nebula.Terminal_1.1.0_x64_en-US.msi"),
                asset("SHA256SUMS.txt"),
            ],
        };
        assert_eq!(
            installer_asset(&release, InstallKind::Installer).map(|a| a.name.as_str()),
            Some("Nebula.Terminal_1.1.0_x64-setup.exe")
        );
        assert_eq!(
            installer_asset(&release, InstallKind::Msi).map(|a| a.name.as_str()),
            Some("Nebula.Terminal_1.1.0_x64_en-US.msi")
        );
        assert!(installer_asset(&release, InstallKind::Manual).is_none());
    }
}

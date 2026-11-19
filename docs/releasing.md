# Releasing

Releases are built by `.github/workflows/release.yml`: one job checks the version, Windows and Linux (x64 and arm64) build at the same time, and a last job adds up the checksums and publishes everything. Pushing to `main` never publishes anything. Only a `v*` tag, or a manual run of the workflow, does.

## 1. Bump the version

The version lives in four files and they must match:

- `package.json` (also run `npm install` so `package-lock.json` follows)
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml` and `crates/nebula-sh/Cargo.toml` (then `cargo check --workspace` to update `Cargo.lock`)

Add a `## <version>` section at the top of `CHANGELOG.md`. The workflow uses it as the release notes.

## 2. Let CI pass on `main`

## 3. Tag

```powershell
git tag v1.0.0
git push origin v1.0.0
```

Instead of tagging, you can run the **Release** workflow from the Actions tab with the version (`1.0.0`). It checks the version, runs the same validation as CI, builds the installers and publishes the release. It refuses to run if the tag and the files disagree, or if the release already exists. A version containing `-` (like `1.1.0-beta.1`) is published as a pre-release.

## What gets published

```text
Nebula.Terminal_<version>_x64-setup.exe          NSIS installer
Nebula.Terminal_<version>_x64_en-US.msi          MSI installer
Nebula-Terminal-<version>-windows-x64-portable.zip   includes nebula-sh.exe
Nebula.Terminal_<version>_arm64-setup.exe        NSIS installer for Windows on Arm
Nebula-Terminal-<version>-windows-arm64-portable.zip
nebula-terminal.json                             Scoop manifest for the portable zip
Nebula.Terminal_<version>_amd64.deb              Debian and Ubuntu
Nebula.Terminal-<version>-1.x86_64.rpm           Fedora, openSUSE, RHEL
Nebula.Terminal_<version>_amd64.AppImage         any distribution, updates itself
Nebula.Terminal_<version>_arm64.deb              the same for 64-bit Arm
Nebula.Terminal-<version>-1.aarch64.rpm
Nebula.Terminal_<version>_aarch64.AppImage
SHA256SUMS.txt
```

`scripts/scoop-manifest.ps1` writes the Scoop manifest. People install from `releases/latest/download/nebula-terminal.json`, and Scoop reads that address again on `scoop update`, so a release needs nothing else for Scoop. Pre-releases never reach it, since `latest` skips them.

When the repository is public, each file also gets a GitHub build-provenance attestation:

```powershell
gh attestation verify '.\Nebula.Terminal_1.0.0_x64-setup.exe' --repo awizzz/nebula-terminal
```

Releases are not Authenticode-signed yet. Never commit certificates, signing tokens or private keys.

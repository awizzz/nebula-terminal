# Releasing

Releases are built by `.github/workflows/release.yml` on a Windows runner. Pushing to `main` never publishes anything. Only a `v*` tag, or a manual run of the workflow, does.

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
SHA256SUMS.txt
```

When the repository is public, each file also gets a GitHub build-provenance attestation:

```powershell
gh attestation verify '.\Nebula.Terminal_1.0.0_x64-setup.exe' --repo awizzz/nebula-shell
```

Releases are not Authenticode-signed yet. Never commit certificates, signing tokens or private keys.

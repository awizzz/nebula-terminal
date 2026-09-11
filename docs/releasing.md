# Releasing

Nebula releases are built and published by the dedicated `Release` GitHub Actions workflow.

A normal push to `main` never creates a release automatically.

## Before publishing

Update the version in `Cargo.toml` and regenerate `Cargo.lock`. Update `CHANGELOG.md` with a matching section and make sure the user-facing documentation reflects the new behavior.

Run locally:

```powershell
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

Merge the release changes only after CI passes.

## Publish from a tag

Create a tag that exactly matches the Cargo package version:

```powershell
git tag v0.4.0
git push origin v0.4.0
```

The release workflow rejects a tag whose version does not match `Cargo.toml`.

## Publish manually

The same workflow can be started from GitHub Actions with **Run workflow**. Enter the package version without the leading `v`, for example `0.4.0`.

The workflow validates that the requested version matches `Cargo.toml` and publishes the release from the selected commit.

## Release contents

A successful release contains:

```text
Nebula.exe
Nebula.exe.sha256
```

Release notes are taken from the matching version section in `CHANGELOG.md`.

## Build provenance

The final `Nebula.exe` is submitted to GitHub Artifact Attestations before publication. The attestation records SLSA build provenance and is signed through GitHub's Sigstore-backed attestation service.

Users can verify provenance with a recent GitHub CLI:

```powershell
gh attestation verify .\Nebula.exe --repo awizzz/custom-shell
```

This provenance is separate from Authenticode. It proves which GitHub repository and workflow produced the artifact; Authenticode provides Windows publisher trust when a signing provider is configured.

## Code signing

Signing is optional. When the required SignPath settings are configured, the exact CI artifact is submitted for signing and the returned Authenticode signature is verified before publication.

Repository variables:

```text
SIGNPATH_ORGANIZATION_ID
SIGNPATH_PROJECT_SLUG
SIGNPATH_SIGNING_POLICY_SLUG
```

Repository secret:

```text
SIGNPATH_API_TOKEN
```

If signing is not configured, the same release is published as unsigned and its title contains `(unsigned)`.

Never commit signing tokens, certificates or private keys.

## Release workflow guarantees

Before publishing, the workflow:

1. checks the requested/tagged version against `Cargo.toml`
2. verifies formatting
3. runs Clippy with warnings denied
4. runs the test suite with the committed lockfile
5. builds the release executable with the committed lockfile
6. optionally signs the exact uploaded build artifact
7. verifies Authenticode when signing is enabled
8. generates `Nebula.exe.sha256`
9. creates a GitHub/Sigstore build-provenance attestation for the final executable
10. publishes release notes from `CHANGELOG.md`

The workflow is defined in `.github/workflows/release.yml`.

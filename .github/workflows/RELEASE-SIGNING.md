# Signed release setup

Nebula release tags (`v*`) are published only after the Windows executable has been signed and the Authenticode signature has been verified by the CI workflow.

The release pipeline is wired for SignPath. Until SignPath is configured, normal CI builds still work, but tagged release jobs stop before publishing so an unsigned executable can never become a GitHub Release asset.

## SignPath setup

1. Apply for SignPath Foundation Open Source Code Signing and create/link the Nebula project to this GitHub repository.
2. Configure the project's default artifact configuration to sign `Nebula.exe` inside the GitHub Actions artifact.
3. Configure a release signing policy and restrict it to trusted GitHub-hosted builds from this repository.
4. Add these GitHub repository variables:
   - `SIGNPATH_ORGANIZATION_ID`
   - `SIGNPATH_PROJECT_SLUG`
   - `SIGNPATH_SIGNING_POLICY_SLUG`
5. Add the repository secret:
   - `SIGNPATH_API_TOKEN`

Never commit the API token or any private signing material to the repository.

## Creating a release

Make sure the Cargo package version matches the tag, then push a version tag:

```powershell
git tag v0.2.0
git push origin v0.2.0
```

The workflow will:

1. run `cargo check` and `cargo test`;
2. build the release executable;
3. upload the unsigned build artifact to GitHub Actions;
4. submit that exact artifact to SignPath;
5. download the signed executable;
6. verify its Authenticode signature locally on the GitHub Windows runner;
7. generate a SHA-256 checksum;
8. publish `Nebula.exe` and `Nebula.exe.sha256` to the GitHub Release.

If signing or signature verification fails, the GitHub Release is not created.

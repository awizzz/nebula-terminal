# Releasing

Nebula releases are built by GitHub Actions on Windows.

## Current flow

The package version is defined in `Cargo.toml`.

On a push to `main`, CI runs formatting, Clippy, checks, tests and a release build. If no GitHub Release exists for `v<package-version>`, the workflow publishes one with:

```text
Nebula.exe
Nebula.exe.sha256
```

If that version already has a release, CI builds normally and skips publication.

## Code signing

Signing is optional. When the SignPath repository settings are present, the release artifact is submitted for signing and the returned Authenticode signature is verified before publication.

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

Without these values, the release is published as unsigned and the release title includes `(unsigned)`.

Never commit signing tokens, certificates or private keys.

## Release checklist

Before changing the package version:

1. update `CHANGELOG.md`
2. run the local checks from `CONTRIBUTING.md`
3. confirm user-facing docs match the new behavior
4. merge the version change into `main`
5. verify the GitHub Actions run and release assets

The release workflow is defined in `.github/workflows/build.yml`.

# Release signing setup

Nebula can publish releases with or without a code-signing provider.

The release workflow always builds and tests the Windows executable, checks that the Git tag matches the Cargo version, generates a SHA-256 checksum, and publishes `Nebula.exe` plus `Nebula.exe.sha256`.

## Without SignPath

No signing configuration is required.

A version tag such as `v0.3.0` creates a GitHub Release automatically. The release title contains `(unsigned)` so users can immediately see that Authenticode signing was not used.

This is the default until a trusted signing provider is configured.

## With SignPath

When all SignPath values are present, the same workflow automatically switches to the signed path.

Required repository variables:

- `SIGNPATH_ORGANIZATION_ID`
- `SIGNPATH_PROJECT_SLUG`
- `SIGNPATH_SIGNING_POLICY_SLUG`

Required repository secret:

- `SIGNPATH_API_TOKEN`

The workflow submits the exact GitHub Actions artifact to SignPath, waits for the result, downloads the signed executable, and verifies the Authenticode signature on the Windows runner before publishing it.

Never commit API tokens, certificates or private signing material to the repository.

## Signing provider notes

The amount of manual intervention depends on the SignPath policy:

- a policy without approval can provide a fully automatic `tag -> sign -> verify -> release` flow;
- SignPath Foundation can be suitable for eligible open-source projects, but its approval policy may still require a manual signing approval.

A self-signed certificate should not be used as a substitute for public code signing because it does not provide normal Windows publisher trust.

## Creating a release

The Cargo package version and Git tag must match. For Nebula 0.3.0:

```powershell
git tag v0.3.0
git push origin v0.3.0
```

The workflow then:

1. runs `cargo check`;
2. runs `cargo test`;
3. builds the release executable;
4. uploads the CI artifact;
5. validates `v0.3.0` against `version = "0.3.0"`;
6. signs and verifies the executable when SignPath is configured;
7. otherwise keeps the executable unsigned;
8. generates `Nebula.exe.sha256`;
9. publishes the GitHub Release.

The release asset names remain identical whether signing is enabled or not, so enabling signing later does not change the download workflow for users.

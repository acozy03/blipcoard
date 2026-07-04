# Release Automation

`blipcoard` uses semantic-release for lockstep versioning across the
`@cosentinode/blipcoard` npm package and Rust workspace packages.

## Channels

| Branch | Release type | npm dist-tag | Git tag | GitHub release |
| --- | --- | --- | --- | --- |
| `develop` | prerelease | `next` | `v0.2.0-next.1` | prerelease |
| `main` | stable | `latest` | `v0.2.0` | release |

Prereleases from `develop` are for validation and early adopters. Stable
releases come only from `main`.

## Version Source

Versions are lockstep:

- root `package.json`
- root `package-lock.json`
- `[workspace.package] version` in `Cargo.toml`
- workspace package entries in `Cargo.lock`
- Git tag `v${version}`

Do not manually edit these version fields for normal releases. The release
workflow updates them from the semantic-release result and commits the release
metadata with `[skip ci]`.

## Package Manager

Release automation uses npm because the published artifact is an npm package,
the release plugin publishes through npm registry semantics, and this repository
tracks npm lockfiles for the root tooling, docs site, and web app. pnpm or Bun
would be reasonable for local development later, but switching package managers
should be a deliberate lockfile and CI migration rather than part of release
automation.

## Trusted Publishing

The release workflow publishes to npm through npm Trusted Publishing. Do not add
an `NPM_TOKEN` repository secret for normal releases.

Configure the npm package trusted publisher with:

| Field | Value |
| --- | --- |
| Package | `@cosentinode/blipcoard` |
| Publisher | GitHub Actions |
| Repository owner | `blipcoard` |
| Repository name | `blipcoard` |
| Workflow filename | `release.yml` |
| Environment | Leave blank unless the workflow later adds one. |

GitHub provides `GITHUB_TOKEN` automatically for tags, release notes, and release
assets. The workflow grants `id-token: write` so npm can verify the GitHub
Actions identity and publish with provenance.

Release validation still runs before publishing. Semantic-release is skipped
until a `v*` baseline tag exists, so the first manual npm bootstrap cannot race
an automated prerelease from `develop`.

## Initial Bootstrap

There are no historical release tags before semantic-release. To start at
`0.1.0`, create the baseline tag on `main` once after the first stable commit is
ready:

```bash title="One-time release bootstrap"
git checkout main
git pull --ff-only origin main
git tag v0.1.0
git push origin v0.1.0
```

After that, semantic-release computes the next version from conventional commits.

## Release Assets

Each release builds a Linux archive from the Rust workspace:

```text
blipcoard-${version}-linux-x64.tar.gz
```

The archive contains:

- `blip`
- `blipd`
- `blip-cloud`
- `README.md`
- `LICENSE`
- `VERSION`

The npm package installs `blip` and `blipd` shims. During global npm install,
the package builds the native Rust binaries from source with Cargo and stores
them inside the installed package.

## Local Release Checks

Run these before changing release automation:

```bash
npm ci
npm run release:sync-version -- 0.1.0
npm run release:build-assets -- 0.1.0
npm pack --dry-run
git diff --check
```

If you run `release:sync-version` locally with a test version, restore the
version files before committing unless the change is an intentional release
metadata update.

## Manual Recovery

If a release fails after publishing a tag but before npm publish:

1. Check the GitHub Actions log for the failing semantic-release step.
2. Do not delete a published npm version.
3. If the GitHub tag exists but npm publish failed, fix the release workflow and
   rerun it from the same commit.
4. If npm published but GitHub release assets failed, upload assets manually to
   the existing GitHub release or rerun the fixed workflow from the same commit.

Release commits use `[skip ci]` to avoid recursive release workflows.

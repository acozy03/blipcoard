# Contributing

Thanks for contributing to `blipcoard`. Work from `develop` unless a maintainer
asks for a release branch.

## Setup

```bash
curl -fsSL https://raw.githubusercontent.com/blipcoard/blipcoard/develop/scripts/setup-dev.sh | bash -s -- --no-runtime --no-service
```

For an existing checkout:

```bash
npm install
npm run hooks:install
```

## Workflow

1. Sync `develop`.
2. Create a short-lived branch for one issue or one tightly scoped change.
3. Keep unrelated refactors out of the branch.
4. Include docs updates for user-facing behavior, CLI output, config,
   packaging, architecture, or API changes.
5. Open a pull request back into `develop`.

Use conventional commit and PR titles:

```text
feat: add hosted workspace presence
fix: handle missing image payload blobs
docs: document setup script
```

## Checks

Run the checks that match your change:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --lib --bins --tests --locked
cargo build --workspace --locked
```

Docs:

```bash
npm run docs:build
npm run docs:check-search
```

Desktop:

```bash
npm --prefix apps/desktop test -- --run
npm --prefix apps/desktop run build
npm --prefix apps/desktop run bundle:check
```

Hosted web:

```bash
npm run web:build
```

## Pull Requests

Every PR should include:

- what changed
- why it changed
- affected issues
- verification commands
- screenshots or local preview notes for UI/docs changes
- migration, config, API, packaging, or security notes when relevant

Do not commit local build artifacts, dependency directories, `.beads/`, local
databases, or personal config.

More detail lives in the docs site:

- `site/docs/contributor/development-workflow.md`
- `site/docs/contributor/docs-maintenance.md`
- `site/docs/contributor/repository-layout.md`

# Development Workflow

Contributors should work from `develop` and open pull requests back into
`develop` unless a maintainer explicitly asks for a release branch.

## Branches And Issues

1. Sync `develop`.
2. Create a short-lived feature branch.
3. Keep the branch scoped to one GitHub issue or one closely related slice.
4. Include docs updates when behavior, CLI output, config, packaging, or
   architecture changes.
5. Open a pull request against `develop`.

Use `main` for production or release promotion work only.

## Commit And PR Titles

The repository uses conventional commit-style linting for PR titles and outgoing
commit messages.

Install the repo hooks:

```sh
npm install
npm run hooks:install
```

Examples:

```text
feat: add workspace policy reference
fix: handle stale daemon sockets
docs: document CLI payload export
```

## Local Checks

Run the checks that match your change.

Rust formatting, type checks, linting, tests, and build:

```sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --lib --bins --locked
cargo test --workspace --tests --locked
cargo build --workspace --locked
```

Docs:

```sh
npm run docs:build
```

Desktop:

```sh
cd apps/desktop
npm test
npm run build
npm run bundle:check
```

`npm run bundle:check` also prepares sidecar binaries and checks the Tauri Rust
crate.

## CI

GitHub Actions run on pull requests into `develop` and `main`:

- quality: formatting, `cargo check`, and clippy
- unit tests: workspace lib and bin tests
- e2e tests: workspace integration tests
- build: workspace build
- docs: Docusaurus build when docs or site files change
- PR title lint: conventional PR title validation

Do not merge a PR with failing required checks unless the failure is unrelated
and a maintainer explicitly accepts the risk.

## Review Expectations

Keep pull requests easy to review:

- describe the behavior change and the affected issue
- list verification commands that were run
- call out migrations, config changes, API changes, packaging changes, and
  known limitations
- keep unrelated refactors out of the branch
- do not commit local build artifacts, dependency directories, or `.beads/`

When a change spans code and docs, review both together. Docs that describe a
feature should land with the feature or in the same phase of work.

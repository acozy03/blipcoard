# blipcoard

`blipcoard` is a cross-platform clipboard routing system for agent workflows.

Instead of letting agents read the raw OS clipboard directly, `blipcoard` mirrors
clipboard events into a local, structured, auditable store and scopes agent access
to specific workspaces chosen by the user.

Core product surfaces:

- `blipd`: background daemon
- `blip`: CLI client
- `blipcoard` desktop app

Start with the design docs in [docs/architecture.md](./docs/architecture.md),
[docs/project-breakdown.md](./docs/project-breakdown.md), and
[docs/runtime-distribution.md](./docs/runtime-distribution.md). Desktop bundle
requirements live in [docs/desktop-bundles.md](./docs/desktop-bundles.md).
CLI-only install and operations guidance lives in
[docs/cli-operations.md](./docs/cli-operations.md).
Upgrade and migration guidance lives in
[docs/upgrade-migrations.md](./docs/upgrade-migrations.md).
Documentation site architecture lives in
[docs/docs-site-architecture.md](./docs/docs-site-architecture.md).
The browsable documentation site source lives under [site/docs](./site/docs).

## Current Status

Phase 8 rich clipboard content is complete on `develop`; Phase 9 packaging and
distribution is in progress.

Today the workspace includes:

- `blip-core`: SQLite-backed domain and storage layer
- `blip-config`: local config discovery and persistence
- `blip-clipboard`: platform detection and typed clipboard watcher support
- `blip-api`: shared API models
- `blip-cli`: daemon client for workspace, blip, policy, and payload workflows
- `blip-daemon`: runtime owner for clipboard ingestion, policy, and audit
- `apps/desktop`: desktop UI backed by the daemon API
- GitHub Actions for format, lint, test, and build checks

Current work is defining the runtime-first distribution model so installs keep
the daemon, CLI, desktop app, and local store ownership aligned.

## Running the Daemon

For a foreground development daemon:

```bash
cargo run -p blip-daemon
```

For installed macOS and Linux users, manage the per-user daemon service through
the CLI. See [docs/cli-operations.md](./docs/cli-operations.md) for full
install, config, logs, policy, and backup guidance.

```bash
blip service install
blip service start
blip service status
blip service logs
blip service uninstall
```

Windows service startup is intentionally unavailable until the Windows daemon IPC
transport lands.

## Tooling

Install the repo-local commitlint tooling and enable the checked-in git hooks:

```bash
npm install
npm run hooks:install
```

This enables the `.githooks/pre-push` hook, which lints outgoing commit messages,
and the GitHub Actions PR title lint workflow enforces conventional PR titles.

Contributor workflow and docs maintenance guidance live in
[site/docs/contributor/development-workflow.md](./site/docs/contributor/development-workflow.md)
and
[site/docs/contributor/docs-maintenance.md](./site/docs/contributor/docs-maintenance.md).

# blipcoard

`blipcoard` is a cross-platform clipboard routing system for agent workflows.

Instead of letting agents read the raw OS clipboard directly, `blipcoard` mirrors
clipboard events into a local, structured, auditable store and scopes agent access
to specific workspaces chosen by the user.

Core product surfaces:

- `blipd`: background daemon
- `blip`: CLI client
- `blipcoard` desktop app

Start with the design docs in [docs/architecture.md](./docs/architecture.md) and
[docs/project-breakdown.md](./docs/project-breakdown.md).

## Current Status

The Phase 8 rich clipboard pipeline is in progress on `develop`.

Today the workspace includes:

- `blip-core`: SQLite-backed domain and storage layer
- `blip-config`: local config discovery and persistence
- `blip-clipboard`: platform detection and typed clipboard watcher support
- `blip-api`: shared API models
- `blip-cli`: daemon client for workspace, blip, policy, and payload workflows
- `blip-daemon`: runtime owner for clipboard ingestion, policy, and audit
- `apps/desktop`: desktop UI backed by the daemon API
- GitHub Actions for format, lint, test, and build checks

Current work is hardening rich payload reliability: blob lifecycle behavior,
retention cleanup, recovery, and cross-platform manual checks.

## Tooling

Install the repo-local commitlint tooling and enable the checked-in git hooks:

```bash
npm install
npm run hooks:install
```

This enables the `.githooks/pre-push` hook, which lints outgoing commit messages,
and the GitHub Actions PR title lint workflow enforces conventional PR titles.

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

## Tooling

Install the repo-local commitlint tooling and enable the checked-in git hooks:

```bash
npm install
npm run hooks:install
```

This enables the `.githooks/pre-push` hook, which lints outgoing commit messages,
and the GitHub Actions PR title lint workflow enforces conventional PR titles.

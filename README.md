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

Phase 1 is merged on `develop`.

Today the workspace includes:

- `blip-core`: SQLite-backed domain and storage layer
- `blip-config`: local config discovery and persistence
- `blip-clipboard`: platform detection and watcher placeholders
- `blip-api`: shared API models
- `blip-cli`: bootstrap CLI for workspace and blip management
- `blip-daemon`: bootstrap daemon binary with health-style output
- GitHub Actions for format, lint, test, and build checks

Phase 2 is the next implementation target. Its focus is turning `blipd` into the
runtime owner for clipboard ingestion and establishing the local daemon/API path
that the CLI and future desktop app will consume.

## Tooling

Install the repo-local commitlint tooling and enable the checked-in git hooks:

```bash
npm install
npm run hooks:install
```

This enables the `.githooks/pre-push` hook, which lints outgoing commit messages,
and the GitHub Actions PR title lint workflow enforces conventional PR titles.

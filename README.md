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

## Tooling

Install the repo-local commitlint tooling and enable the checked-in git hooks:

```bash
npm install
npm run hooks:install
```

This enables the `.githooks/pre-push` hook, which lints outgoing commit messages,
and the GitHub Actions PR title lint workflow enforces conventional PR titles.

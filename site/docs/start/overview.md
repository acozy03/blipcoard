# blipcoard

`blipcoard` is a cross-platform clipboard routing system for agent workflows.

Instead of letting agents read the raw OS clipboard directly, `blipcoard`
mirrors clipboard events into a local, structured, auditable store and scopes
agent access to workspaces chosen by the user.

Core product surfaces:

- `blipd`: background daemon and runtime owner
- `blip`: CLI client
- `blipcoard`: desktop app backed by the daemon API

## Start Here

- [CLI install and operations](../operations/cli-operations.md)
- [Runtime distribution model](../operations/runtime-distribution.md)
- [Desktop bundle notes](../operations/desktop-bundles.md)
- [Upgrade and migration strategy](../operations/upgrade-migrations.md)

## Architecture

- [Runtime model](../architecture/runtime-model.md)
- [Storage and rich payload reliability](../architecture/storage-and-blobs.md)
- [Daemon API reference](../reference/daemon-api.md)

## Roadmap and Contribution

- [MVP phases](../roadmap/mvp-phases.md)
- [Project breakdown](../roadmap/project-breakdown.md)
- [Repository layout](../contributor/repository-layout.md)
- [Docs site architecture](../contributor/docs-site-architecture.md)

## Runtime Rule

Supported installs keep `blipd` as the only clipboard watcher:

- Full install: `blipd`, `blip`, and the desktop app
- CLI-only install: `blipd` and `blip`
- Desktop-only install: unsupported

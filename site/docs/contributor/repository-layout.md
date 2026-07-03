# Proposed Repository Layout

```text
blipcoard/
  README.md
  docs/
    architecture.md
    daemon-api.md
    project-breakdown.md
    mvp-phases.md
    repo-layout.md
    runtime-distribution.md
    desktop-bundles.md
  crates/
    blip-core/
    blip-daemon/
    blip-cli/
    blip-api/
    blip-clipboard/
    blip-config/
  apps/
    desktop/
    web/
  services/
    cloud/
```

## Crate Responsibilities

### `blip-core`

- domain types
- workspace logic
- SQLite migrations and store invariants
- bundle generation
- classification interfaces

### `blip-daemon`

- daemon process
- clipboard event loop
- storage orchestration
- policy enforcement
- local API server

Current state:

- bootstrap binary exists
- phase 2 should add the long-running runtime loop and client boundary

### `blip-cli`

- terminal UX
- JSON output
- human table output
- bootstrap/admin store commands until daemon APIs are available

Current state:

- bootstrap commands talk directly to `blip-core`
- this is temporary until phase 2 establishes the daemon/API path

### `blip-api`

- request/response models shared by daemon, CLI, and desktop app
- schema types for the local daemon IPC boundary

### `blip-clipboard`

- platform abstraction for clipboard access and clipboard watching
- no storage or policy ownership

### `blip-config`

- config parsing
- defaults
- validation

## Desktop App

`apps/desktop` should contain:

- Tauri shell in `src-tauri/`
- React frontend
- workspace, inbox, audit, and settings screens

## Hosted Workspaces

Phase 11 keeps hosted collaboration in the monorepo for the MVP.

Planned additions:

### `services/cloud`

- hosted workspace API
- membership and join-code lifecycle
- hosted blip publish/list APIs
- real-time update transport
- hosted audit and retention jobs
- deployment configuration while the service is still MVP-owned by this repo

### `apps/web`

- browser join-by-code flow
- hosted workspace list/detail experience
- member-visible copy/export actions with audit logging

### `crates/blip-sync`

Use this crate only if hosted sync client/protocol logic needs to be shared by
the daemon, CLI, and desktop app.

The local runtime boundary remains unchanged: `blipd` owns local capture, and
hosted clients publish only daemon-approved blips.

See [Hosted workspace architecture](../architecture/hosted-workspaces.md).

## Branch / Release Flow

- `develop`: default branch for integration and staging
- `main`: production branch
- feature branches open PRs into `develop`

## CI Location

GitHub Actions workflows should live in:

```text
.github/workflows/
```

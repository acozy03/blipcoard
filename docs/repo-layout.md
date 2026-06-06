# Proposed Repository Layout

```text
blipcoard/
  README.md
  docs/
    architecture.md
    project-breakdown.md
    mvp-phases.md
    repo-layout.md
  crates/
    blip-core/
    blip-daemon/
    blip-cli/
    blip-api/
    blip-clipboard/
    blip-config/
  apps/
    desktop/
```

## Crate Responsibilities

### `blip-core`

- domain types
- workspace logic
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

Current state:

- bootstrap commands talk directly to `blip-core`
- this is temporary until phase 2 establishes the daemon/API path

### `blip-api`

- request/response models shared by daemon, CLI, and desktop app

### `blip-clipboard`

- platform abstraction for clipboard access and clipboard watching

### `blip-config`

- config parsing
- defaults
- validation

## Desktop App

`apps/desktop` should contain:

- Tauri shell
- React frontend
- workspace, inbox, audit, and settings screens

## Branch / Release Flow

- `develop`: default branch for integration and staging
- `main`: production branch
- feature branches open PRs into `develop`

## CI Location

GitHub Actions workflows should live in:

```text
.github/workflows/
```

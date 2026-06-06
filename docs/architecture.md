# blipcoard Architecture

## Goal

`blipcoard` is an abstraction layer on top of the OS clipboard for developer-agent
workflows.

The system should:

- mirror clipboard copies into a local store
- let users route copied items into named workspaces
- let agents read only the workspace explicitly attached to their session
- support desktop and CLI workflows equally well
- remain fully local-first and auditable

## Product Model

There are three layers:

1. OS clipboard
2. `blipcoard` workspace store
3. agent-scoped retrieval

The OS clipboard remains unchanged for normal human app usage.

`blipcoard` passively ingests clipboard events into an internal `inbox`, then lets
the user move or auto-route those events into workspaces such as:

- `auth-bug`
- `infra`
- `resume`
- `design`

Agents should never read the raw OS clipboard directly. They should read from the
`blipcoard` workspace store.

## Main Components

### 1. `blipd`

Background daemon responsible for:

- clipboard watching
- event ingestion
- classification
- redaction
- persistence
- workspace routing
- local API / IPC
- policy enforcement

This is the source of truth for runtime behavior.

### 2. `blip`

CLI client responsible for:

- listing inbox items
- routing latest items into workspaces
- setting the active workspace
- bundling workspace contents
- searching history
- scripting and shell integration

The CLI should be a client of `blipd`, not a separate clipboard manager.

### 3. Desktop app

Desktop UI responsible for:

- visual inbox/workspace inspection
- drag/drop or quick-route workflows
- search and detail inspection
- policy and hotkey configuration
- active workspace switching
- audit viewing

The desktop app should use the same daemon API as the CLI.

## Recommended Tech Stack

- systems/core: Rust
- daemon: Rust + Tokio
- CLI: Rust + Clap
- desktop shell: Tauri
- desktop frontend: React + TypeScript
- local DB: SQLite + FTS5
- config: TOML
- serialization: Serde

## Cross-Platform Support

Supported targets:

- macOS
- Linux
- Windows

The hardest portability problem is clipboard observation, not storage or routing.

The architecture should isolate platform-specific clipboard watching behind one
crate so the rest of the system remains platform-agnostic.

## Security Model

The security model should assume the clipboard often contains sensitive material.

Defaults:

- all copies go into `inbox`
- agents cannot read `inbox` by default
- agents can only read the active workspace by default
- cross-workspace reads require explicit permission
- all agent reads should be auditable

Optional policy features:

- redact secret-like content on ingestion
- allowlist app sources
- sticky workspace mode
- retention windows per workspace
- lock specific workspaces from agent access

## Data Flow

1. User copies text in any app.
2. OS clipboard changes.
3. `blipd` observes the change.
4. `blipd` creates a `blip` record in `inbox`.
5. User routes the `blip` into a workspace manually or via sticky mode.
6. CLI or desktop sets active workspace.
7. Agent tools query only that workspace.

## Core Domain Objects

### `Blip`

Represents one mirrored clipboard event.

Suggested fields:

- `id`
- `created_at`
- `workspace`
- `source_app`
- `content_type`
- `language`
- `content`
- `size_bytes`
- `token_estimate`
- `is_redacted`
- `tags`

### `Workspace`

Represents a named context bucket for a task.

Suggested fields:

- `name`
- `description`
- `color`
- `agent_access`
- `sticky_capture`
- `retention_days`
- `created_at`

### `AuditEvent`

Represents a read/write/routing/policy action.

Suggested fields:

- `id`
- `timestamp`
- `actor_type`
- `actor_id`
- `event_type`
- `target_blip_id`
- `target_workspace`
- `details_json`

## Interfaces

### CLI

Human-facing commands, for example:

- `blip inbox`
- `blip send auth-bug`
- `blip use auth-bug`
- `blip current`
- `blip list auth-bug`
- `blip bundle auth-bug`

### Agent-facing commands

Scoped reads only:

- `blip agent recent`
- `blip agent search "jwt"`
- `blip agent bundle`

### Desktop UI

Visual management layer:

- inbox queue
- workspace list
- active workspace indicator
- timeline of blips
- audit panel
- settings and hotkeys


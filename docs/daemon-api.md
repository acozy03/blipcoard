# Daemon API and IPC Boundary

## Goal

`blipd` should expose one local daemon boundary for non-bootstrap clients. The
CLI and desktop app should use this boundary for runtime behavior so clipboard
ownership, policy enforcement, and audit writes stay centralized in the daemon.

## Transport

Phase 2 should use newline-delimited JSON over a local, user-scoped IPC channel.

- macOS and Linux: Unix domain socket under the user's runtime or config
  directory
- Windows: named pipe scoped to the current user
- no TCP listener by default
- no unauthenticated cross-user access
- one request produces one response; clients may either open one connection per
  command or keep a short-lived connection for a batch

This keeps the protocol simple for shell workflows while leaving room for a
longer-lived desktop connection later.

## Schema Ownership

The `blip-api` crate owns request and response types shared by `blipd`, `blip`,
and the desktop app.

The daemon owns translation from those API types into:

- `blip-core` store calls
- clipboard runtime state
- policy checks
- audit writes
- user-facing error responses

`blip-core` should not know about sockets, pipes, request envelopes, or client
identity. `blip-clipboard` should not know about API requests or storage.

## Envelope

Every request should carry:

- `api_version`
- `request_id`
- `command`
- `payload`

Every response should carry:

- `api_version`
- `request_id`
- `status`
- `payload` for success responses
- `error` for failure responses

Errors should be typed with a stable code plus a human-readable message. Store
errors, policy denials, missing resources, daemon startup failures, and
unsupported clipboard payloads should remain distinguishable.

## Initial Commands

The first daemon API surface should cover the commands needed by Phase 3 CLI
work and future desktop inspection:

| Area | Commands |
| --- | --- |
| Health | daemon health, API version, store path |
| Workspaces | list, create, activate, inspect active workspace |
| Inbox and blips | list inbox, list workspace, inspect blip by id |
| Routing | move a blip from inbox to workspace, route latest blip |
| Audit | list recent audit events |
| Clipboard runtime | report watcher status and unsupported clipboard capability notes |

These commands should be added incrementally. A CLI command may keep using direct
`blip-core` access only until the matching daemon command exists.

## Policy-Sensitive Commands

These operations must go through `blipd` once the daemon API exists:

- clipboard ingestion and duplicate suppression
- active workspace changes
- inbox reads
- workspace reads intended for agent use
- blip routing and move operations
- audit log reads
- policy or privacy setting changes
- rich payload preview, export, or raw byte access in later phases

Agent-facing reads are scoped by workspace. The current agent read command,
`blip agent recent <workspace>`, is daemon-mediated and returns full blip
content only when that workspace has `agent_access` enabled. The default `inbox`
workspace is created with `agent_access = false`, so `blip agent recent inbox`
returns an `access_denied` daemon error unless a future explicit policy change
grants broader access. Human-facing inbox commands remain separate from agent
read commands.

The daemon should attach the client kind to policy and audit decisions where it
matters. Expected client kinds are:

- CLI
- desktop
- agent integration
- daemon internal runtime

## Client Boundary

`blip` should be a thin terminal client for daemon commands. It may format
responses for humans or JSON output, but it should not duplicate clipboard
watching, routing policy, or agent access policy.

The desktop app should use the same daemon API as the CLI. Tauri commands should
call the daemon client layer rather than opening the SQLite database directly for
runtime behavior.

Bootstrap or repair commands may keep direct store access when the daemon is not
running or cannot start. Those commands should be explicitly labeled as local
admin paths and should not watch the clipboard.

## Versioning

The first public daemon API version is `1`.

Breaking request or response changes should increment the API version. Additive
fields may be introduced within a version when older clients can ignore them.

The CLI and desktop app should show an actionable version mismatch error when
their supported API version is incompatible with the running daemon.

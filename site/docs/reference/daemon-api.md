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

## Current Commands

The current daemon API version is `1`.

`DaemonCommand` values are serialized as snake_case strings:

| Area | Commands |
| --- | --- |
| Health/version | `health`, `version` |
| Workspaces | `current_workspace`, `activate_workspace`, `set_sticky_capture`, `set_workspace_policy`, `list_workspaces` |
| Blips | `list_blips`, `search_blips`, `get_blip` |
| Payloads | `get_payload_metadata`, `get_payload_preview`, `export_payload` |
| Audit | `list_audit_events` |
| Routing | `route_blip`, `route_latest_inbox_blip` |
| Agent reads | `agent_recent_blips`, `agent_search_blips`, `agent_bundle` |

Workspace creation still has a bootstrap/admin CLI path while the daemon API
does not expose a create-workspace command. That path should not watch the
clipboard and should move behind the daemon when the API grows.

## Policy-Sensitive Commands

These operations must go through `blipd` once the daemon API exists:

- clipboard ingestion and duplicate suppression
- active workspace changes
- inbox reads
- workspace reads intended for agent use
- blip routing and move operations
- audit log reads
- policy or privacy setting changes
- rich payload preview, export, or raw byte access

Rich payload inspection is additive on the existing list/detail responses. A
blip may include `payloads` summaries with:

- `payload_kind`
- `mime_type`
- `platform_format`
- `byte_size`
- `preview_state`
- `preview_text`
- opaque `preview_ref`
- `has_blob`
- `has_inline_text`
- bounded `metadata_summary`

Default list/detail responses must stay safe and lightweight. They do not return
raw payload bytes, `blob_ref`, filesystem blob paths, or executable rich markup.
Missing backing blobs are represented as `preview_state = "missing_blob"`;
redacted blips use `preview_state = "redacted"` and omit sensitive metadata.
HTML and RTF summaries expose plain text fallback only.

Workspace summaries include the active rich payload policy fields:

- `rich_capture_enabled`
- `image_capture_enabled`
- `rich_payload_visibility`
- `agent_raw_payload_access`

`rich_payload_visibility = "hidden"` suppresses typed payload summaries from
daemon list/detail responses for that workspace. Ordinary list/detail calls do
not emit payload preview/export/read audit events because they do not dereference
blob bytes.

Explicit byte retrieval uses dedicated daemon commands:

- `get_payload_metadata` returns the same bounded `PayloadSummary` shape for one
  payload id and never returns raw bytes or blob paths.
- `get_payload_preview` returns policy-checked preview bytes for one payload id
  when a safe preview blob exists.
- `export_payload` returns policy-checked raw payload bytes for one payload id
  when the stored payload has a backing blob.

The byte response includes payload id, owning blip id, workspace, payload kind,
MIME type, platform format, byte size, and bytes. Current JSON IPC encodes bytes
as a numeric byte array so clients do not write binary data to stdout by
accident. The CLI writes those bytes only to the requested path and refuses to
overwrite existing files unless `--force` is set.

Preview and export commands must apply the same workspace policy for desktop,
CLI, and agent callers. CLI and desktop preview reads require
`rich_payload_visibility = "safe_preview"`. Raw exports require both text agent
access and `agent_raw_payload_access = true`; they are denied by default. CLI
and desktop raw exports are also denied when rich payload visibility is `hidden`.
The daemon treats client-supplied requester labels as audit context, not as
authorization facts. Successful, denied, and missing-blob byte reads record
dedicated payload audit events. Missing payloads are returned as `not_found`
without writing a payload access audit event.

Payload byte commands return typed errors for:

- `access_denied` when workspace policy blocks the caller.
- `missing_blob` when SQLite references a blob that is absent locally.
- `unsupported_payload` when the payload has no retrievable preview or raw blob
  for the requested operation.
- `payload_too_large` when the daemon refuses an oversized export.

## Error Codes

Daemon errors use stable snake_case codes:

| Code | Meaning |
| --- | --- |
| `unsupported_api_version` | The client and daemon API versions are incompatible. |
| `invalid_request` | The command or payload is malformed or invalid. |
| `not_found` | A requested workspace, blip, payload, or other resource does not exist. |
| `access_denied` | Workspace or payload policy denied the operation. |
| `store_unavailable` | The daemon could not use the local store. |
| `missing_blob` | Payload metadata exists but the referenced blob is missing. |
| `unsupported_payload` | The payload cannot be previewed or exported for this operation. |
| `payload_too_large` | The payload is too large for the requested operation. |
| `internal` | Unexpected daemon failure. |

Workspace policy changes use the daemon-mediated `set_workspace_policy` command,
which records a `workspace_policy_changed` audit event and returns the updated
workspace summary.

Agent-facing reads are scoped by workspace. The current agent read command,
`blip agent recent <workspace>`, is daemon-mediated and returns full blip
content only when that workspace has `agent_access` enabled. The default `inbox`
workspace is created with `agent_access = false`, so `blip agent recent inbox`
returns an `access_denied` daemon error unless a future explicit policy change
grants broader access. Raw binary payload reads require a separate workspace
policy allow flag and are denied by default even when text agent access is
enabled. Human-facing inbox commands remain separate from agent read commands.

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

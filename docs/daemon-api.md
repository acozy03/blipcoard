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

### Filtered blip lists

`list_blips` keeps `workspace`, `limit`, and `offset` for existing clients and
accepts an optional `filters` object:

- `all_workspaces`: include every local workspace instead of only `workspace`
- `created_at_from`: inclusive UTC capture-time boundary
- `created_at_before`: exclusive UTC capture-time boundary
- `blip_types`: any combination of `text`, `image`, `file_list`, `rich_text`,
  and `unknown`

Omitting `filters` preserves workspace-scoped behavior. Filtering and the total
count happen before `limit` and `offset`; aggregate results remain ordered by
effective recency. Date boundaries use the original capture time, so recopying a
blip can promote it without making an older capture qualify for a newer date
range. List summaries include their owning `workspace`, which is required to
identify rows in aggregate results.

New daemons set `filters_supported = true` on `list_blips` responses so desktop
clients fail clearly instead of presenting ignored filters during an upgrade.

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

Workspace creation uses the daemon-mediated `create_workspace` command and
returns the created workspace summary. Duplicate names and invalid workspace
input return `invalid_request`.

Desktop recopy uses `recopy_blip` to promote an existing blip to the front of
its workspace without changing its original capture timestamp. The command also
registers a short-lived clipboard fingerprint so the matching clipboard watcher
event is consumed instead of inserting a duplicate blip.

Workspace policy changes use daemon-mediated commands that record a
`workspace_policy_changed` audit event and return the updated workspace summary:

- `set_agent_access` changes whether agent text reads are allowed for an
  existing workspace.
- `set_workspace_policy` changes rich capture, visibility, and raw payload
  access settings.

The desktop app requires confirmation before enabling agent access. Disabling
access takes effect immediately. Changing `agent_access` does not change
`agent_raw_payload_access`; raw binary payloads remain separately controlled and
denied by default.

Agent-facing reads are scoped by workspace. The current agent read command,
`blip agent recent <workspace>`, is daemon-mediated and returns full blip
content only when that workspace has `agent_access` enabled. The default `inbox`
workspace is created with `agent_access = false`, so `blip agent recent inbox`
returns an `access_denied` daemon error until the user explicitly enables agent
access. Raw binary payload reads require a separate workspace
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

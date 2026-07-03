# Hosted Sync Protocol

The hosted sync protocol defines how local `blipcoard` clients publish selected
blips to a hosted workspace and receive ordered updates from other members.

This is a protocol specification for Phase 11 implementation. It preserves the
local runtime boundary: `blipd` captures locally, and hosted sync operates only
on daemon-approved publish actions.

## Design Goals

- keep hosted sharing explicit or visibly opt-in
- support self-hosted and Tailscale-friendly deployments
- make membership and roles part of every API decision
- make publish idempotent so retries are safe
- make workspace updates ordered and resumable
- keep rich payload metadata separate from raw bytes
- remain compatible with existing `blip-api` concepts such as blips, payload
  summaries, preview states, audit events, and workspace policy

## Core Entities

### HostedWorkspace

```json
{
  "id": "hw_01HY...",
  "name": "auth-bug",
  "created_by_member_id": "hm_01HY...",
  "created_at": "2026-07-03T21:30:00Z",
  "retention_days": 30,
  "default_role": "viewer",
  "last_sequence": 42
}
```

Rules:

- workspace ids are server-generated and opaque
- names are display labels, not unique global identifiers
- `last_sequence` is the highest committed event sequence for reconnects

### HostedMember

```json
{
  "id": "hm_01HY...",
  "workspace_id": "hw_01HY...",
  "display_name": "Adrian",
  "role": "owner",
  "status": "active",
  "joined_at": "2026-07-03T21:30:00Z"
}
```

Roles:

- `owner`: manage workspace, members, join codes, retention, deletion
- `editor`: publish blips, tag blips, delete blips when policy allows
- `viewer`: read hosted blips and audited metadata only

### HostedDeviceSession

```json
{
  "id": "hd_01HY...",
  "member_id": "hm_01HY...",
  "label": "desktop-linux",
  "client_kind": "desktop",
  "created_at": "2026-07-03T21:30:00Z",
  "last_seen_at": "2026-07-03T21:35:00Z"
}
```

`client_kind` values start with `desktop`, `cli`, `daemon`, and `web`.

### HostedTag

```json
{
  "workspace_id": "hw_01HY...",
  "name": "checkout",
  "created_at": "2026-07-03T21:30:00Z"
}
```

Tags are normalized per workspace. Tag changes on a blip emit ordered workspace
events.

### JoinCode

```json
{
  "id": "jc_01HY...",
  "workspace_id": "hw_01HY...",
  "role": "editor",
  "expires_at": "2026-07-04T21:30:00Z",
  "revoked_at": null,
  "max_uses": 10,
  "use_count": 2
}
```

The raw join code is shown once to the creator and stored only as a hash.
Redemption creates membership and device/session state; the code is not the
continuing credential.

### HostedBlip

```json
{
  "id": "hb_01HY...",
  "workspace_id": "hw_01HY...",
  "source": {
    "local_blip_id": "dev-inbox-3",
    "publisher_member_id": "hm_01HY...",
    "publisher_device_id": "hd_01HY..."
  },
  "content_type": "plain_text",
  "content": "copied stack trace",
  "preview": "copied stack trace",
  "size_bytes": 18,
  "token_estimate": 4,
  "is_redacted": false,
  "tags": ["bug"],
  "captured_at": "2026-07-03T21:28:00Z",
  "published_at": "2026-07-03T21:30:00Z",
  "deleted_at": null,
  "payloads": []
}
```

Rules:

- `captured_at` comes from the local blip when available
- `published_at` is assigned by the hosted service
- server timestamps do not overwrite local capture time
- redacted blips can publish metadata without content when policy requires it

### HostedPayload

```json
{
  "id": "hp_01HY...",
  "blip_id": "hb_01HY...",
  "payload_kind": "image",
  "mime_type": "image/png",
  "platform_format": "public.png",
  "byte_size": 245091,
  "preview_state": "available",
  "preview_text": null,
  "blob_state": "stored",
  "metadata_summary": {
    "width": 1280,
    "height": 720
  }
}
```

Payload rules:

- list APIs return metadata, not raw bytes
- file-list payloads publish references and metadata by default, not file bytes
- HTML and RTF use sanitized or plain-text fallback in display paths
- raw bytes are uploaded only through explicit payload upload APIs
- raw bytes are exported only through role-checked, audited APIs

### HostedAuditEvent

```json
{
  "id": "ha_01HY...",
  "workspace_id": "hw_01HY...",
  "sequence": 43,
  "actor_member_id": "hm_01HY...",
  "actor_device_id": "hd_01HY...",
  "event_type": "blip_published",
  "target_blip_id": "hb_01HY...",
  "details": {
    "idempotency_key": "publish:hd_01HY:dev-inbox-3:attempt-1"
  },
  "created_at": "2026-07-03T21:30:00Z"
}
```

Audit events share the workspace sequence stream so clients can resume from one
ordered position.

## API Envelope

All hosted API responses use a stable envelope:

```json
{
  "request_id": "req_01HY...",
  "status": "ok",
  "data": {},
  "error": null
}
```

Error response:

```json
{
  "request_id": "req_01HY...",
  "status": "error",
  "data": null,
  "error": {
    "code": "access_denied",
    "message": "member role cannot publish to this workspace"
  }
}
```

Initial error codes:

- `invalid_request`
- `unauthenticated`
- `access_denied`
- `not_found`
- `join_code_expired`
- `join_code_revoked`
- `rate_limited`
- `conflict`
- `payload_too_large`
- `quota_exceeded`
- `internal`

## REST API

### Create Workspace

```http
POST /v1/workspaces
```

Request:

```json
{
  "name": "auth-bug",
  "retention_days": 30
}
```

Response contains `workspace`, `member`, and `device_session` records for the
creator.

### List Workspaces

```http
GET /v1/workspaces
```

Returns hosted workspaces for the current member and the member's role in each
workspace.

### Create Join Code

```http
POST /v1/workspaces/{workspace_id}/join-codes
```

Request:

```json
{
  "role": "editor",
  "expires_in_seconds": 86400,
  "max_uses": 10
}
```

Response returns the raw join code once plus metadata.

### Revoke Join Code

```http
POST /v1/workspaces/{workspace_id}/join-codes/{join_code_id}/revoke
```

Revokes the code and emits `join_code_revoked`.

### Join By Code

```http
POST /v1/join
```

Request:

```json
{
  "code": "BLIP-7KQ9-M2PA",
  "display_name": "Sam",
  "device_label": "sam-macbook",
  "client_kind": "desktop"
}
```

Response contains workspace, member, device, and session token information.

### List Members

```http
GET /v1/workspaces/{workspace_id}/members
```

Owners can see all members. Editors and viewers can see the membership fields
allowed by workspace policy.

### Publish Blip

```http
POST /v1/workspaces/{workspace_id}/blips
Idempotency-Key: publish:hd_01HY:dev-inbox-3:attempt-1
```

Request:

```json
{
  "local_blip_id": "dev-inbox-3",
  "captured_at": "2026-07-03T21:28:00Z",
  "content_type": "plain_text",
  "content": "copied stack trace",
  "preview": "copied stack trace",
  "size_bytes": 18,
  "token_estimate": 4,
  "is_redacted": false,
  "tags": ["bug"],
  "payloads": []
}
```

Response returns the hosted blip and the committed workspace event.

Rules:

- repeated requests with the same idempotency key return the original result
- repeated requests with the same key and different body return `conflict`
- publisher must be an `owner` or `editor`
- redacted or rich payload publish follows workspace policy

### List Blips

```http
GET /v1/workspaces/{workspace_id}/blips?limit=50&after_sequence=40
```

Returns hosted blips visible to the member and the latest workspace sequence.

### Get Blip

```http
GET /v1/workspaces/{workspace_id}/blips/{blip_id}
```

Returns one hosted blip detail plus payload metadata visible to the member.

### Update Tags

```http
PATCH /v1/workspaces/{workspace_id}/blips/{blip_id}/tags
```

Request:

```json
{
  "tags": ["bug", "checkout"]
}
```

Returns the updated blip and a `blip_tags_updated` event.

### Delete Blip

```http
DELETE /v1/workspaces/{workspace_id}/blips/{blip_id}
```

Soft-deletes the hosted blip, schedules blob cleanup, and emits
`blip_deleted`.

### List Audit Events

```http
GET /v1/workspaces/{workspace_id}/audit?limit=100&after_sequence=40
```

Returns audit entries visible to the current member.

### Export Payload

```http
GET /v1/workspaces/{workspace_id}/payloads/{payload_id}/export
```

Role-checked and audited. The MVP may return a pre-signed object-storage URL or
stream bytes through the service. Either path must enforce authorization before
issuing access.

### Blob Upload

```http
POST /v1/workspaces/{workspace_id}/payloads/{payload_id}/upload
```

Uploads raw payload bytes after the hosted blip and payload metadata have been
accepted. The service validates membership, payload ownership, byte size, MIME
type, hash, quota, and upload state.

## Real-Time Updates

Use WebSocket as the primary real-time transport for the MVP:

```http
GET /v1/workspaces/{workspace_id}/sync?after_sequence=42
Upgrade: websocket
```

Reasons:

- desktop and daemon clients need one long-lived channel for sync state,
  heartbeat, and future bidirectional control messages
- WebSocket works for browser and native clients
- workspace sequence numbers still provide deterministic reconnect behavior
- the service can add explicit acknowledgements without changing transports

Use Server-Sent Events as a read-only fallback:

```http
GET /v1/workspaces/{workspace_id}/events?after_sequence=42
Accept: text/event-stream
```

Polling fallback:

```http
GET /v1/workspaces/{workspace_id}/events?after_sequence=42&limit=100
```

## Event Envelope

```json
{
  "workspace_id": "hw_01HY...",
  "sequence": 43,
  "event_id": "he_01HY...",
  "event_type": "blip_published",
  "created_at": "2026-07-03T21:30:00Z",
  "actor": {
    "member_id": "hm_01HY...",
    "device_id": "hd_01HY...",
    "client_kind": "desktop"
  },
  "data": {}
}
```

Initial event types:

- `workspace_created`
- `member_joined`
- `member_role_changed`
- `member_removed`
- `join_code_created`
- `join_code_revoked`
- `blip_published`
- `blip_tags_updated`
- `blip_deleted`
- `payload_uploaded`
- `payload_exported`
- `access_denied`
- `retention_applied`

## Event Ordering

Each hosted workspace has one monotonically increasing sequence.

Rules:

- committing a state change and assigning its sequence is transactional
- clients process events only when `sequence` is greater than the last applied
  sequence
- clients persist the last applied sequence per hosted workspace
- reconnect uses `after_sequence`
- if the server no longer has enough history for a sequence, it returns
  `conflict` with instructions to resync current state
- clients dedupe by `event_id`
- lower or equal sequence values are replay and should be ignored
- clients reject events for the wrong workspace or membership
- clients may buffer a short future sequence gap, but must refetch if the gap
  does not close quickly
- clients treat persistent gaps as reconnect triggers, not as successful sync

## Idempotency

Mutating APIs that clients may retry require an `Idempotency-Key` header:

- publish blip
- update tags
- delete blip
- create join code
- revoke join code
- upload blob

The server stores the key, member, workspace, request hash, response reference,
and expiry. A retry with the same key and same body returns the same result. A
retry with the same key and a different body returns `conflict`.

Suggested publish key:

```text
publish:{device_id}:{local_blip_id}:{attempt_id}
```

## Payload Limits

MVP defaults:

| Limit | Default |
| --- | --- |
| Plain text content | 1 MiB |
| Single rich payload upload | 25 MiB |
| Preview payload | 2 MiB |
| Payloads per blip | 8 |
| Tags per blip | 32 |
| Tag length | 64 characters |
| Workspace members | 50 |
| Open WebSocket connections per member | 5 |

Services can configure stricter limits. Exceeding limits returns
`payload_too_large` or `quota_exceeded`.

## Binary And Blob Handling

For the MVP:

- publish text inline when under the content limit
- publish rich payload metadata with the blip
- upload raw payload bytes through a separate object/blob path
- store blobs encrypted at rest
- dedupe blobs by content hash when practical
- generate previews locally before upload when the local daemon already has a
  safe preview
- never require server-side OCR or remote media analysis

Blob records should keep:

- payload id
- content hash
- MIME type
- byte size
- storage key
- upload state
- retention/delete state

## Compatibility With Local API Concepts

Hosted schemas should map cleanly to local concepts:

| Local concept | Hosted concept |
| --- | --- |
| `WorkspaceSummary` | `HostedWorkspace` plus current member role |
| `BlipSummary` / `BlipDetail` | `HostedBlip` |
| `PayloadSummary` | `HostedPayload` |
| `PayloadPreviewState` | hosted `preview_state` |
| `AuditEventSummary` | `HostedAuditEvent` / event envelope |
| daemon route/send | explicit hosted publish |
| local workspace policy | hosted role and workspace retention policy |

Do not reuse local daemon API transport directly for hosted sync. Local IPC and
hosted HTTP/SSE are separate boundaries that can share schema concepts.

## Client Responsibilities

Clients must:

- show the hosted destination before publish
- send idempotency keys for retryable writes
- persist last applied sequence per hosted workspace
- reconnect with `after_sequence`
- surface role or policy denials clearly
- write local audit events for publish attempts and outcomes
- avoid storing session tokens in logs
- keep local-only workflows usable with no hosted configuration

## Open Follow-Ups

Implementation issues should decide:

- exact service framework and database migrations in `services/cloud`
- token/session format and refresh behavior
- object storage implementation for self-hosted deployments
- whether web join/view is in the MVP or follows desktop/CLI
- E2EE envelope versioning details for a future encrypted mode

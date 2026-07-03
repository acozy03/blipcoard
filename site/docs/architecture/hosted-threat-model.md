# Hosted Workspace Threat Model

Hosted workspaces move selected clipboard data from one local machine to a
shared service. That changes the privacy boundary and must be explicit in the
product, protocol, and UI.

The MVP remains self-hostable and Tailscale-friendly. A team can run the hosted
service on infrastructure it controls and expose it over a private network. A
managed relay can be added later, but it is not required for Phase 11.

## Security Choice For MVP

Use a server-readable MVP with strong transport, storage, access, audit, and
retention controls.

This means:

- the hosted service can read hosted workspace metadata and published blip
  content
- clients must use HTTPS or a private network transport such as Tailscale
- service storage must be encrypted at rest by the database and object storage
  layer
- the service must enforce workspace membership, roles, retention, deletion, and
  audit policy
- users must be told that published blips are readable by the workspace service
  operator

End-to-end encryption is a future design path, not an MVP promise. Do not build
API or storage assumptions that make E2EE impossible later: keep payload
envelopes versioned, separate metadata from content bytes, and avoid requiring
server-side content inspection for core sync correctness.

## Assets

Sensitive assets include:

- secrets copied to the clipboard, such as tokens, API keys, cookies, SSH keys,
  one-time codes, and passwords
- screenshots containing credentials, customer data, private chats, production
  dashboards, or source code
- copied source code, logs, stack traces, database rows, and customer records
- file-list payloads that reveal local paths, project names, usernames, or
  mounted volumes
- rich text and HTML that may contain hidden content, links, or formatting from
  private tools
- membership lists, device names, IP addresses, join-code activity, and audit
  events
- retention and deletion state

## Trust Boundaries

```text
local OS clipboard
  -> blipd local capture and policy boundary
  -> explicit publish or hosted sticky-share boundary
  -> hosted service API
  -> hosted store and real-time sync
  -> other members' clients
```

Primary trust boundaries:

- local machine to hosted service
- member device to membership/session state
- join code to persistent membership
- hosted metadata to raw payload bytes
- service operator to workspace content
- web client to browser storage

## Threats And Required Controls

### Accidental Over-Sharing

Risk:

- a user copies sensitive material and unknowingly publishes it
- hosted sticky share remains enabled longer than intended
- local `inbox` is treated as shareable by default

Controls:

- explicit publish is the default hosted sharing mode
- hosted sticky share is opt-in, workspace-scoped, visible, and reversible
- `inbox` remains human-only and local by default
- desktop and CLI must show the destination hosted workspace before publishing
- local audit events record hosted publish attempts and outcomes
- redacted local blips require an explicit confirmation or policy grant before
  hosted publish

### Unauthorized Workspace Access

Risk:

- a non-member reads hosted blips
- a viewer performs editor actions
- a removed member keeps using an old session

Controls:

- every hosted API request checks membership and role
- roles start with `owner`, `editor`, and `viewer`
- session tokens are scoped to a member and device
- member removal revokes active sessions for that workspace
- real-time channels re-check access on connect and reconnect
- audit events record joins, role changes, removals, and denied access

### Join-Code Abuse

Risk:

- codes are guessed, leaked, reused, or treated as permanent credentials

Controls:

- join codes are high-entropy temporary invites
- codes expire by default
- owners can revoke and rotate codes
- invalid-code attempts are rate-limited by source and workspace
- successful redemption creates persistent membership; the code is not the
  continuing credential
- code creation, redemption, expiry, revocation, and failed attempts are audited
- anonymous public joins are out of scope for the MVP

### Service Or Operator Exposure

Risk:

- the hosted service operator can read workspace content in the server-readable
  MVP
- logs or errors leak content
- backups retain deleted material longer than expected

Controls:

- document that MVP hosted content is server-readable
- encrypt database and object storage at rest
- never log raw blip content, payload bytes, join codes, or session tokens
- redact sensitive request fields in structured logs
- keep backups encrypted
- define backup retention and deletion delay clearly
- provide an E2EE-compatible envelope path for later phases

### Rich Payload Abuse

Risk:

- large images or files create cost and availability problems
- copied HTML is rendered unsafely
- file-list payloads trick users into exposing local files

Controls:

- hosted payload size limits are mandatory
- raw rich payload upload is explicit and can be disabled per workspace
- HTML and RTF are stored as data and rendered through sanitized fallback only
- file-list payloads publish metadata by default, not file bytes
- raw exports and copies are role-checked and audited
- unsupported payloads remain visible as metadata instead of being silently
  downgraded

### Sync Integrity

Risk:

- duplicate publish attempts create confusing records
- reconnects replay old events out of order
- clients accept forged updates

Controls:

- publish requests use idempotency keys derived from local blip id, workspace,
  device, and attempt id
- hosted events are append-only with monotonically increasing workspace sequence
  numbers
- clients resume from the last acknowledged sequence
- clients reject events for the wrong workspace or membership
- service timestamps do not replace local capture timestamps

### Abuse And Availability

Risk:

- join-code brute force, payload spam, excessive sync connections, or oversized
  uploads degrade the service

Controls:

- rate-limit join attempts, login/session creation, publish, export, and
  real-time connections
- enforce payload and workspace quotas
- expose admin/operator metrics for rejected joins, denied reads, upload size,
  sync lag, and error rates
- provide workspace-level disable or quarantine controls before managed hosting

## Privacy Policy Defaults

Hosted privacy defaults:

- local clipboard capture is not uploaded automatically
- explicit publish is the default share action
- hosted sticky share is off by default
- workspace membership is required for every hosted read
- viewer role cannot publish, delete, or change tags unless explicitly allowed
- raw payload export is audited
- retention defaults should be short and visible until users configure them
- deletion removes hosted metadata and blob references, with backup purge timing
  documented separately

## Deletion, Export, And Retention

Hosted workspaces need clear lifecycle rules:

- owners can delete hosted workspaces
- editors can delete blips only if workspace policy allows it
- delete operations create audit events
- deleting a blip removes metadata and schedules blob deletion
- retention jobs remove expired hosted blips and unreferenced blobs
- exports are role-checked and audited
- backup retention may delay physical purge; that delay must be documented

Local deletion and hosted deletion are separate. If a user deletes a local blip
after publishing it, the hosted copy remains until the hosted workspace policy or
an explicit hosted delete removes it.

## Implementation Requirements

Before hosted launch, implementation issues must include:

- membership roles and per-request authorization
- join-code entropy, expiry, revocation, and rate limiting
- local and hosted audit event types for publish, join, read/export, delete, and
  denied access
- explicit hosted publish and visible sticky-share controls in desktop and CLI
- hosted payload size limits and workspace quotas
- structured logging that redacts content, codes, and tokens
- encrypted-at-rest database and blob storage configuration
- retention jobs and deletion semantics
- idempotent publish and ordered sync events
- an E2EE migration note in API and storage design

## Non-Goals

Phase 11 MVP does not promise:

- end-to-end encryption
- anonymous public workspaces
- automatic upload of every clipboard event
- cloud-side clipboard watching
- unmanaged public internet exposure
- unlimited payload size or unlimited retention
- permanent bearer-token join codes

These are deliberate limits. Hosted sharing should be useful without weakening
the local privacy model that makes `blipcoard` safe for agent workflows.

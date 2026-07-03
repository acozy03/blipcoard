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
- eventually support rich clipboard payloads, such as screenshots, images, file
  lists, HTML, and RTF, without uploading content to third-party services

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
The daemon API and IPC boundary is defined in
[`daemon-api.md`](daemon-api.md).

### 2. `blip`

CLI client responsible for:

- listing inbox items
- routing latest items into workspaces
- setting the active workspace
- bundling workspace contents
- searching history
- scripting and shell integration

During early storage and ingestion work, the CLI may call `blip-core` directly for
bootstrap commands that only read or mutate the local store. Once `blipd` exposes
the local API needed by a command, that command should move behind the daemon so
policy enforcement and audit behavior remain centralized.

The CLI should never become a second clipboard watcher or policy engine.

Bootstrap note:

- in phase 1, the CLI is allowed to talk to `blip-core` storage directly so the
  workspace and audit model can be exercised before the daemon API exists
- phase 2 should replace direct store access with daemon-mediated operations for
  clipboard ingestion and shared runtime behavior

### 3. Desktop app

Desktop UI responsible for:

- visual inbox/workspace inspection
- drag/drop or quick-route workflows
- search and detail inspection
- policy and hotkey configuration
- active workspace switching
- audit viewing

The desktop app should use the same daemon API as the CLI.

## Branching Model

The repository should use:

- `develop` as the default integration and staging branch
- `main` as the production/release branch
- short-lived feature branches targeting `develop`

All ordinary implementation PRs should target `develop` first.

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

All supported platforms should remain in the same repository.

This project should not split macOS, Linux, and Windows into separate repos.
Only the clipboard and OS integration layer should diverge by platform.

## Packaging Model

`blipcoard` should be treated as one runtime system with multiple entry points,
not as three separate products.

Required runtime components:

- `blipd`
- `blip`

Optional GUI surface:

- `blipcoard` desktop app

Supported install modes:

1. Full install
   - daemon
   - CLI
   - desktop app
2. CLI-only install
   - daemon
   - CLI

Unsupported install mode:

- desktop-only install

The desktop application should never be treated as a standalone product.
Any supported desktop distribution should also include the daemon and CLI.
The operational install model, platform paths, startup behavior, and uninstall
policy are defined in
[`runtime-distribution.md`](./runtime-distribution.md).

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

## Runtime Ownership

The simplest architecture that preserves the user experience is:

- `blip-core` owns durable domain and storage rules
- `blip-clipboard` owns platform-specific clipboard observation
- `blipd` owns long-running ingestion, routing, policy, and future IPC/API access
- `blip` owns terminal interaction and should stay thin
- the desktop app owns visual interaction and should use the same daemon API as
  other non-bootstrap clients

This keeps Phase 2 focused: build reliable daemon-owned clipboard ingestion into
`inbox` before introducing a broader API surface. Avoid adding direct clipboard
watching to the CLI or desktop app.

The local daemon boundary should use the shared request and response types from
`blip-api`, with `blipd` exposing newline-delimited JSON over a user-scoped Unix
domain socket on macOS/Linux or named pipe on Windows.

## Persistence Rules

The local store should favor boring, recoverable behavior:

- schema changes go through numbered migrations
- multi-row writes use a transaction
- audit entries commit with the state change they describe
- invalid persisted enum or JSON values are surfaced as errors
- platform paths should be handled as paths, not lossy strings
- expected SQLite writer contention should return typed, user-facing errors
- long-running surfaces should use bounded list queries and lightweight row
  projections, with full clipboard content fetched explicitly by id
- phase 2 should move ordinary runtime writes behind `blipd` so concurrent
  clients do not become independent SQLite writers
- phase 8 should store binary clipboard payloads as local blobs referenced by
  SQLite metadata, not as large base64 strings in text columns

## Data Flow

1. User copies text in any app.
2. OS clipboard changes.
3. `blipd` observes the change.
4. `blipd` creates a `blip` record in `inbox`.
5. User routes the `blip` into a workspace manually or via sticky mode.
6. CLI or desktop sets active workspace.
7. Agent tools query only that workspace.

Phase 2 is intentionally text-first. Non-text clipboard contents should be
reported as unsupported or no readable text until phase 8 adds typed rich
payloads, durable blob storage, previews, and access policy.

Phase 1 bootstrap flow:

1. User runs the CLI locally.
2. CLI opens the same local SQLite store as `blipd`.
3. User creates workspaces, selects an active workspace, and inserts demo blips.
4. Audit events are written directly by the store layer.

This bootstrap path exists only to validate the storage and domain model before
phase 2 introduces the real daemon ingestion boundary.

## Rich Clipboard Content

Phase 8 extends the text-first model to clipboard payloads that are not safely or
usefully represented as `String` content.

Payload categories:

- text: plain text and text-like classified content
- image: screenshots and copied image data, such as PNG, JPEG, TIFF, or platform
  bitmap formats
- file list: copied file references from file managers or drag/copy workflows
- rich text: a product grouping for `html` and `rtf` payload kinds with
  plain-text fallback
- unknown: platform formats that can be observed but not decoded yet

The product should model the clipboard payload independently from the storage
mechanism. A future payload API should look conceptually like:

- text payloads carry inline UTF-8 content
- image payloads carry MIME type, dimensions, byte size, hash, and blob reference
- file-list payloads carry path metadata and capture policy, not eagerly copied
  file bytes by default
- HTML and RTF payloads carry sanitized metadata plus a plain-text fallback
- unknown payloads carry platform format identifiers and byte-size metadata when
  available

Storage rules for rich payloads:

- keep searchable and listable metadata in SQLite
- keep binary bytes in a local blob store under the configured data directory
- store copied file-list payloads as references and metadata by default, not as
  automatically imported copies of the referenced files
- use content hashes for dedupe and integrity checks
- make blob writes recoverable if the daemon crashes between file and SQLite
  updates
- garbage collect unreferenced blobs after retention or explicit deletion
- never store large binary payloads as base64 in `Blip.content`

Cross-platform rich payload reliability checks are documented in
[rich-payload-reliability.md](./rich-payload-reliability.md). Those checks cover
manual screenshot, copied image, file-list, HTML, unsupported-format, backup,
restore, and rollback evidence across macOS, Linux X11, Linux Wayland, and
Windows.

Preview rules:

- list views should show lightweight metadata and bounded previews only
- image thumbnails should be generated locally and size-limited
- desktop preview rendering must not execute untrusted HTML
- HTML and RTF previews should use a sanitized, bounded plain-text fallback by
  default and preserve rich payload metadata separately
- CLI output should never dump binary data by default
- raw payload reads should require an explicit id-based command or API request
- file opens, file imports, rich markup rendering, and external opener actions
  should require explicit user or policy-approved access

Policy rules:

- rich payload capture is configurable globally under `[capture]` with
  `enabled`, `text`, `image`, `file_list`, `html`, `rtf`, `unknown`,
  `max_image_bytes`, and `image_previews`
- workspace policy can further disable rich capture or image capture for that
  destination workspace, and can hide typed payload summaries from list/detail
  responses
- screenshots and images should be treated as sensitive local data even when
  capture is enabled
- agents should not receive raw binary payloads unless workspace policy
  explicitly enables `agent_raw_payload_access`
- agents receive metadata and plain-text fallbacks for HTML, RTF, file-list, and
  unknown payloads by default, not privileged renders or imported file contents
- thumbnail reads, raw payload exports, desktop opens, and agent payload reads
  have dedicated audit event types separate from ordinary list views
- deletion must remove both SQLite records and blob data when no other blip
  references the same blob

Audit rules:

- file-list payloads should record the observed references, source format,
  source app, capture time, and whether any later import/export/open action read
  file bytes; display paths are convenience metadata and should be accompanied by
  platform-native path bytes or UTF-16 code units when available
- unknown payloads should remain visible in audit and list views with their
  platform format identifiers and available size metadata, even when no decoder
  exists yet
- failed or refused rich payload handling should be reported as typed unsupported,
  unsafe, unavailable, or policy-denied outcomes instead of disappearing as empty
  clipboard data

Platform notes:

- macOS screenshot clipboard support should account for pasteboard image types
- Linux support should account for X11 and Wayland differences
- Windows support should account for bitmap and file-drop clipboard formats
- portable rich clipboard capture currently covers file-list and HTML payloads
  through the clipboard backend; RTF and generic unknown-format observation need
  platform-specific readers before live ingestion can emit them
- every platform reader should expose capabilities so the daemon can explain why
  a payload type was ignored or unsupported

Non-goals for phase 8:

- OCR as a requirement for image ingestion
- cloud upload, remote thumbnailing, or remote media analysis
- importing copied files automatically when the clipboard only contains file
  references
- allowing the CLI or desktop app to watch the clipboard directly

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

Future rich payload fields:

- `payload_kind`
- `mime_type`
- `blob_ref`
- `content_hash`
- `preview_ref`
- `width`
- `height`
- `platform_format`
- `capture_policy`

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

- `blip agent recent auth-bug`
- `blip agent search "jwt"`
- `blip agent bundle`

Agent-facing commands require an explicit readable workspace. The default
`inbox` workspace is a human triage queue and is not agent-readable by default;
agent attempts to read it should fail with a clear policy denial.

### Desktop UI

Visual management layer:

- inbox queue
- workspace list
- active workspace indicator
- timeline of blips
- audit panel
- settings and hotkeys

## CI Expectations

Every PR targeting `develop` or `main` should run at minimum:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --workspace`

This is the Rust equivalent of formatting, linting, type-checking pressure, tests,
and build verification on every change.

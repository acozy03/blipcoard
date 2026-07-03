# Project Breakdown

This project should still be built in parts so clipboard ingestion stabilizes
before UI polish or advanced agent integration is attempted, but phase 1 already
delivered more than storage alone. The breakdown below reflects the current
implementation order and what remains.

## Part 1: Foundation baseline

Status:

- merged

Goal:

- prove the data model, local store, config layer, and repo automation before
  real clipboard ingestion exists

Delivered:

- `blip-core` domain and SQLite-backed storage
- numbered DB migration baseline
- transactional write paths for state plus audit events
- persisted-data validation for stored enum and JSON values
- `blip-config` config discovery and persistence
- `blip-api` shared response models
- `blip-clipboard` platform placeholders
- `blip-cli` bootstrap commands for workspaces and demo blips
- `blip-daemon` bootstrap binary
- GitHub Actions quality gates

Questions answered in this phase:

- what is a `blip`
- what is a `workspace`
- what is an `audit event`
- how should the local store be initialized
- how does active workspace state behave

## Part 2: Daemon and clipboard ingestion

Goal:

- make `blipd` the runtime owner for clipboard observation and persisted inbox
  ingestion

Deliverables:

- daemon lifecycle and long-running runtime loop
- platform abstraction for clipboard reading and watching
- ingestion pipeline into `inbox`
- dedupe policy for repeated copies
- clear ownership boundary between `blipd`, `blip-clipboard`, and `blip-core`
- daemon-facing local API or IPC surface

Questions answered in this phase:

- how reliable is cross-platform clipboard observation
- what should polling vs evented behavior look like
- what metadata can we capture consistently
- where should the daemon/client boundary live

Constraint:

- `blipd` owns clipboard observation; the CLI and desktop app should not watch
  the clipboard directly
- this phase is text-first; non-text payloads should remain unsupported or
  represented as no readable text until the rich clipboard phase defines storage,
  previews, and access policy

## Part 3: CLI as a daemon client

Goal:

- keep the system usable from the terminal without bypassing daemon policy

Deliverables:

- `blip` CLI wired through the daemon boundary
- shell-friendly output modes
- human-readable table output
- routing, search, bundle, and workspace commands
- clear errors on stderr and stable data output on stdout

Questions answered in this phase:

- what is the minimum command set that makes the tool useful
- how should active workspace selection behave in scripts

Implementation note:

- direct `blip-core` access is acceptable for early bootstrap/admin commands
  while the daemon API is incomplete
- commands that represent agent reads, policy-sensitive reads, or long-running
  runtime behavior should go through `blipd`

## Part 4: Workspace routing and policy

Goal:

- allow user-controlled separation of concurrent tasks with explicit read policy

Deliverables:

- create/delete/list workspaces
- assign blips to workspaces
- sticky workspace capture mode
- policy gates for agent visibility
- read audit trail

Questions answered in this phase:

- should `inbox` always exist
- how should sticky mode interact with manual routing
- how do users recover misrouted items
- should agents ever read `inbox`

## Part 5: Desktop App

Goal:

- create the main visual management experience

Deliverables:

- Tauri app
- React UI
- inbox/workspace views
- detail panel for individual blips
- audit panel
- settings view

Questions answered in this phase:

- what should the routing UX look like visually
- do users prefer quick route, drag/drop, or picker overlay

## Part 6: Hotkeys and Fast Routing UX

Goal:

- make workspace routing fast enough for real multitasking

Deliverables:

- global hotkey support
- latest-blip routing shortcuts
- workspace picker overlay
- sticky workspace toggle

Questions answered in this phase:

- should routing be explicit or mostly sticky
- should the overlay appear after every copy or only on demand

## Part 7: Classification and Redaction

Goal:

- make blips safer and more useful

Deliverables:

- deterministic type detection
- secret-like pattern detection
- token estimation
- content tags
- optional redaction transforms

Questions answered in this phase:

- which redactions should be automatic vs opt-in
- what metadata is useful enough to display in UI

## Part 8: Rich Clipboard Content

Goal:

- make screenshots, copied images, files, and formatted clipboard payloads usable
  as first-class blips without turning the clipboard layer into storage, policy,
  or UI code

Why this is separate from phase 2:

- text clipboard ingestion can use `String` payloads and simple previews
- screenshots and images require binary storage, thumbnails, hashing, metadata,
  retention, and stricter privacy defaults
- copied files and rich text formats have platform-specific clipboard semantics
  that need deliberate API boundaries
- agent access to binary payloads needs explicit policy because screenshots often
  contain secrets, private messages, browser tabs, terminal output, and customer
  data

Subphases:

1. Payload model and schema design
   - replace the text-only event shape with a typed payload model
   - represent text, image, file list, HTML, RTF, and unknown platform formats
   - add MIME type, original platform format, size, hash, and preview metadata
   - decide which metadata belongs in SQLite and which belongs beside blob files
   - preserve compatibility with existing text blips during migration
2. Blob storage and lifecycle
   - choose a local blob directory layout under the configured data directory
   - store binary payloads by content hash or stable blob id
   - keep SQLite records transactional with blob writes and cleanup
   - add retention, garbage collection, and orphan recovery behavior
   - ensure backups and migrations can reason about blob references
3. Platform capture
   - detect image clipboard formats on macOS, Linux, and Windows
   - support common screenshot flows, including clipboard screenshots and copied
     image data from browsers/editors
   - detect copied file lists without eagerly importing large files
   - capture HTML/RTF as formatted payloads while preserving plain text fallback
   - expose platform capability metadata so callers can explain unsupported types
4. Preview and inspection
   - generate bounded thumbnails for images and screenshots
   - show stable summaries in CLI output without dumping binary data
   - show image/file/rich-text previews in the desktop detail panel
   - record width, height, MIME type, and byte size where available
   - avoid rendering untrusted HTML directly in privileged UI contexts
5. Policy, privacy, and agent access
   - default rich binary payloads to no direct agent access until explicitly
     routed or allowed by policy
   - audit preview, export, and raw payload reads separately from list views
   - add workspace-level controls for capturing screenshots and images
   - add redaction hooks for screenshots without requiring OCR in this phase
   - make accidental capture easy to delete from both SQLite and blob storage
6. CLI, daemon API, and export
   - add daemon APIs for metadata list, thumbnail fetch, and raw payload fetch
   - add CLI commands to inspect metadata and export payloads by id
   - keep full binary retrieval explicit and policy-checked
   - make shell output predictable for non-text content
   - ensure desktop and CLI use the same daemon API
7. Dedupe and reliability
   - dedupe image payloads by content hash
   - separate repeated copy events from duplicate payload storage
   - test crash recovery around partially written blob files
   - test large payload limits and backpressure in the daemon loop
   - verify behavior across supported OS clipboard implementations
   - maintain a manual rich payload reliability checklist for macOS, Linux X11,
     Linux Wayland, and Windows in
     [rich-payload-reliability.md](./rich-payload-reliability.md)

Deliverables:

- `ClipboardPayload` or equivalent typed payload API in `blip-clipboard`
- `blip-core` schema migration for rich payload metadata and blob references
- blob store abstraction with tests for write, read, delete, GC, and recovery
- platform image/screenshot detection for macOS, Linux, and Windows
- file-list and rich-text detection with plain-text fallback
- daemon ingestion path for rich payloads
- desktop previews and CLI metadata/export commands
- privacy controls, audit events, and policy checks for rich payload reads
- migration and compatibility tests for existing text-only blips

Questions answered in this phase:

- which clipboard formats are first-class product surfaces
- which rich payloads are captured by default and which require opt-in
- how should screenshots be previewed without exposing sensitive content too
  casually
- how should file-list clipboard entries differ from imported file content
- what size limits should the daemon enforce
- how should duplicate payload storage differ from duplicate copy events
- what metadata can be captured consistently across macOS, Linux, and Windows

Non-goals:

- OCR as a requirement for screenshot ingestion
- cloud storage, cloud thumbnailing, or remote media analysis
- making the CLI or desktop app independent clipboard watchers
- rendering untrusted HTML as active desktop UI

## Part 9: Packaging and Distribution

Goal:

- make it installable and stable

Deliverables:

- runtime-first distribution model
- desktop bundles for macOS, Linux, Windows
- daemon install/start behavior
- CLI install docs
- upgrade strategy

Packaging rule:

- desktop bundles must include daemon + CLI
- CLI-only installs are supported
- desktop-only installs are not supported

## Part 10: Documentation Site and Public Docs

Goal:

- publish the repository's documentation in a browsable site so product,
  architecture, and developer guidance are easy to navigate and maintain

Why this is its own part:

- the repository already uses Markdown for technical notes and roadmap docs
- a docs site adds navigation, search, layout consistency, and a stable entry
  point for users and contributors
- the docs surface should stay aligned with implementation phases instead of
  becoming a disconnected marketing site

Subphases:

1. Site scaffold and information architecture
   - choose the docs site generator and theme, with Docusaurus as the default
     recommendation unless another static docs stack proves materially better
   - define the sidebar and top-level navigation
   - decide what belongs in the public docs site versus raw repository Markdown
   - wire local dev preview and build commands
2. Content migration
   - move existing architecture and roadmap docs into site pages
   - preserve links between phases, design notes, and reference material
   - clean up duplicated or stale sections as content is migrated
   - maintain source-of-truth references for repository-only notes
3. Product and architecture docs
   - document the problem statement, runtime model, and data flow
   - document daemon ownership, clipboard boundaries, and workspace policy
   - document phase definitions in a user-friendly way
   - document security and privacy defaults clearly
4. Reference docs
   - document CLI commands and expected outputs
   - document daemon startup, health checks, and operational expectations
   - document config, data, and log locations
   - document common troubleshooting and backup/recovery steps
5. Contributor workflow docs
   - document branching, PR review, and issue workflow
   - document coding standards and test expectations
   - document how to update the docs when implementation phases move
   - document release and versioning conventions
6. Publishing and maintenance
   - add CI checks for docs build correctness
   - add a publish workflow or deployment target
   - decide whether docs are versioned alongside releases
   - keep docs and roadmap phases in sync over time

Deliverables:

- docs site scaffold with navigation and local preview
- imported architecture, roadmap, and reference content
- CLI/daemon/user workflow pages
- contributor and release workflow docs
- docs build and publish automation
- maintenance guidance for keeping docs aligned with implementation phases

Questions answered in this phase:

- which docs belong in the browsable site versus raw repo Markdown
- should the docs stack be Docusaurus or another static site generator
- how much versioned documentation is needed for the project lifecycle
- how should docs updates be reviewed alongside code changes

Success condition:

- documentation is easy to browse, searchable, and versioned well enough that a
  new contributor or user can orient themselves without reading the repository
  Markdown tree directly

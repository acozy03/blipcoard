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

## Part 8: Packaging and Distribution

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

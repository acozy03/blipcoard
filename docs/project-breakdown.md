# Project Breakdown

This project should be built in parts so the clipboard ingestion model stabilizes
before UI polish or advanced agent integration is attempted.

## Part 1: Core Domain and Storage

Goal:

- define the data model
- create SQLite schema
- support insert/read/search operations

Deliverables:

- `blip-core` crate
- storage module inside `blip-core` or `blip-daemon`
- DB migrations
- search support through SQLite FTS5

Questions answered in this phase:

- what is a `blip`
- what is a `workspace`
- what is an `audit event`
- how do we support bundling

## Part 2: Daemon and Clipboard Ingestion

Goal:

- create `blipd`
- watch clipboard changes
- insert blips into `inbox`

Deliverables:

- daemon lifecycle
- platform abstraction for clipboard reading/watching
- ingestion pipeline
- dedupe policy for repeated copies

Questions answered in this phase:

- how reliable is cross-platform clipboard observation
- what should polling vs evented behavior look like
- what metadata can we capture consistently

## Part 3: Routing and Workspace Model

Goal:

- allow user-controlled separation of concurrent tasks

Deliverables:

- create/delete/list workspaces
- assign blips to workspaces
- sticky workspace capture mode
- active workspace state
- policy gates for agent visibility

Questions answered in this phase:

- should `inbox` always exist
- how should sticky mode interact with manual routing
- how do users recover misrouted items

## Part 4: CLI

Goal:

- make the system usable without a desktop app

Deliverables:

- `blip` CLI
- shell-friendly JSON output
- human-readable table output
- routing, search, bundle, and workspace commands

Questions answered in this phase:

- what is the minimum command set that makes the tool useful
- how should active workspace selection behave in scripts

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

## Part 7: Agent Access Layer

Goal:

- expose workspace-scoped reads safely to agents

Deliverables:

- scoped CLI subcommands
- optional MCP server wrapper
- read audit trail
- workspace permission checks

Questions answered in this phase:

- should agents ever read `inbox`
- how strict should default scoping be
- what operations should be read-only forever

## Part 8: Classification and Redaction

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

## Part 10: CI / CD

Goal:

- make every PR prove the Rust workspace is healthy before merge

Deliverables:

- GitHub Actions workflow
- formatting checks
- clippy lint checks
- workspace tests
- workspace build verification
- PR targeting `develop` and `main`

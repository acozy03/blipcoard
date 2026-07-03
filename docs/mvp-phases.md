# MVP Phases

This is the implementation order that matches the current repository state on
`develop`.

## Phase 1: Foundation baseline

Status:

- merged

Included deliverables:

- repository and workspace layout for the runtime crates
- SQLite schema for `workspaces`, `blips`, `audit_events`, and app state
- numbered migration baseline for the local store
- transactional writes for state changes plus audit events
- persisted-data validation for stored enum and JSON values
- `blip-core` domain and storage APIs
- default `inbox` workspace and active workspace state
- bootstrap CLI commands for workspace and demo blip management
- bootstrap daemon binary and shared API/config crates
- CI baseline for format, lint, test, build, and PR title checks

Success condition:

- the local store, audit trail, and active workspace model are usable and tested
  before clipboard watching exists

## Phase 2: Daemon and clipboard ingestion

Status:

- next

Must have:

- `blipd` as the runtime owner for ingestion behavior
- clipboard watcher or polling abstraction behind `blip-clipboard`
- automatic ingestion into `inbox`
- dedupe policy for repeated copies
- durable ingestion through `blip-core`
- shared local API or IPC boundary for daemon clients

Boundary:

- `blipd` owns clipboard watching
- `blip-clipboard` owns platform-specific observation
- the daemon/client boundary is newline-delimited JSON over a local Unix domain
  socket on macOS/Linux or a current-user named pipe on Windows; shared request
  and response schemas live in `blip-api`
- CLI commands may still use direct store access for bootstrap/admin workflows
  until the daemon API exists, but they should not watch the clipboard
- non-text clipboard payloads, such as screenshots, copied images, files, HTML,
  and RTF, are out of scope for phase 2 and should be treated as unsupported or
  no readable text until phase 8 defines the rich payload model

Dedupe policy:

- phase 2 suppresses repeated identical text clipboard events observed by
  `blipd` within a two-second window
- suppressed duplicate events are not persisted and do not emit additional
  `blip_ingested` audit events
- different text resets the comparison, and copying the same text again after
  the suppression window creates a new blip

Success condition:

- copying text creates persisted blips automatically through the daemon path

## Phase 3: CLI as a daemon client

Must have:

- `blip` commands wired through the daemon boundary
- inbox and workspace listing
- active workspace selection
- routing commands for user-controlled organization
- shell-friendly output modes

Success condition:

- the CLI is useful without bypassing daemon policy or runtime behavior

## Phase 4: Active workspace scoping and agent access

Must have:

- agent-safe scoped read commands
- no default access to `inbox`
- explicit policy checks around readable workspaces
- daemon-mediated access for agent reads
- read-side audit events

Success condition:

- an agent can read only the chosen workspace unless the user opts into broader
  access

## Phase 5: Desktop app

Must have:

- inbox view
- workspace view
- active workspace badge
- detail panel
- audit visibility

Success condition:

- users can inspect and route blips visually with the same runtime boundary as
  the CLI

## Phase 6: Fast routing

Must have:

- global shortcuts
- sticky workspace mode
- quick workspace send actions

Success condition:

- multitasking between several active workstreams feels practical

## Phase 7: Redaction and search polish

Must have:

- basic secret detection
- FTS search
- type tags
- bundle builder

Success condition:

- blips are searchable, safer, and useful as agent context bundles

## Phase 8: Rich clipboard content

Goal:

- support non-text clipboard payloads without weakening the daemon ownership,
  workspace policy, audit trail, or local-first storage model

Must have:

- rich clipboard payload model for text, images, files, HTML, RTF, and unknown
  platform formats
- durable blob storage for screenshots, copied images, and file-like payloads
- metadata capture for MIME type, source format, byte size, dimensions, hashes,
  source app, and capture time
- platform readers for screenshot/image clipboard data on macOS, Linux, and
  Windows
- safe thumbnail and preview generation for desktop and CLI summaries
- export/open commands that retrieve full binary payloads only by explicit id
- content hash dedupe and retention/garbage-collection rules for blob data
- privacy controls for image capture, screenshot previews, and agent access to
  rich payloads
- migration path from text-only `Blip.content` records to typed payload records

Storage notes:

- blob bytes live beside SQLite as part of the same local store and must be
  backed up, restored, and garbage-collected together with payload metadata
- blob writes use atomic temp files and content hashes so failed writes recover
  cleanly, duplicate bytes share one stored file, and deletion only removes bytes
  after the final SQLite reference is gone

Phase 8.3 platform image behavior:

- image clipboard readers normalize MIME type, width, height, byte size, and
  original platform format before persistence
- macOS support accounts for pasteboard image types; Linux support reports X11,
  Wayland, permission, and headless limitations; Windows support accounts for
  PNG and bitmap/DIB clipboard sources
- unsupported image formats and unavailable clipboard runtimes produce typed
  errors instead of being downgraded to text or hidden as empty clipboard data

Out of scope:

- optical character recognition as an ingestion requirement
- cloud upload or remote preview services
- automatic agent access to raw screenshots or binary payloads

Success condition:

- copying a screenshot or image creates an auditable inbox blip with metadata and
  a preview, while full binary content remains locally stored and available only
  through explicit user or policy-approved retrieval

## Phase 9: Packaging and distribution

Must have:

- runtime-first distribution model
- desktop bundles for macOS, Linux, and Windows
- daemon install/start behavior
- CLI install docs
- upgrade and data-migration strategy

Success condition:

- users can install, upgrade, and run `blipcoard` without bypassing the daemon,
  CLI, or local store ownership model

## Phase 10: Documentation site and public docs

Goal:

- make the product, architecture, and developer workflow easier to understand
  through a browsable documentation site instead of scattered repository notes

Must have:

- docs site scaffold with a maintainable sidebar and landing navigation
- Docusaurus as the docs site generator unless a materially better option is
  chosen during Phase 10.1
- migration path for the existing `docs/` Markdown into the site structure
- product overview, architecture, roadmap, and security/privacy pages
- CLI, daemon, and developer workflow reference pages
- contribution, review, and publishing workflow documentation
- search and link structure that makes phase and feature docs easy to find
- a repeatable docs build and publish pipeline

Out of scope:

- marketing copy that is not product documentation
- replacing source-of-truth repo markdown where raw notes are still the better
  fit
- a public blog or changelog system unrelated to product docs

Success condition:

- a new contributor or user can find the architecture, setup, roadmap, CLI, and
  daemon references from a browsable docs site without reading raw repository
  Markdown first

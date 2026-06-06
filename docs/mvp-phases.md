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
- CLI commands may still use direct store access for bootstrap/admin workflows
  until the daemon API exists, but they should not watch the clipboard

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

# Hosted Workspace Architecture

Hosted workspaces add multiplayer sharing without changing the local-first
clipboard boundary.

`blipd` remains the owner of local clipboard capture. The hosted system receives
only blips the user explicitly publishes or blips that flow through an
opt-in hosted share mode with visible state and an easy off switch.

The MVP should be self-hostable and Tailscale-friendly. A user or organization
can run the shared service on infrastructure they control and expose it over a
private network such as Tailscale. An official `relay.blipcoard.com` or managed
control plane can be added later, but Phase 11 should not require one.

## Architecture Decision

Keep hosted collaboration in the monorepo for the MVP.

Reasoning:

- hosted workspaces need shared product language, schema contracts, daemon
  client behavior, desktop flows, docs, and CI to move together
- the existing repository already contains Rust crates, the desktop app, docs,
  and release automation for one runtime system
- a separate repo would add coordination cost before deployment and security
  ownership are mature enough to justify it
- local-only mode must keep working while hosted work evolves, which is easier
  to enforce when changes are reviewed in one tree

Revisit a split only after the hosted service has independent deployment,
security review, incident response, and release ownership.

## Proposed Monorepo Layout

```text
blipcoard/
  apps/
    desktop/              # local desktop app plus hosted workspace UI
    web/                  # browser join/view surface for hosted workspaces
  services/
    cloud/                # self-hostable workspace API, sync, membership, retention
  crates/
    blip-api/             # local daemon schemas and shared hosted primitives
    blip-sync/            # daemon/client hosted sync protocol, if Rust-owned
  site/
    docs/                 # public product, architecture, operations docs
```

Guidelines:

- keep local daemon API types in `crates/blip-api`
- add hosted-only service internals under `services/cloud`
- use `crates/blip-sync` only for protocol/client logic shared by `blipd`,
  `blip`, and desktop
- use `apps/web` for the lightweight hosted browser experience
- keep deployment configuration close to `services/cloud` until operations
  needs justify a separate infrastructure package

## Diagram

```text
                local machine                         self-hosted or hosted service
┌──────────────────────────────────────┐          ┌─────────────────────────────────┐
│ OS clipboard                         │          │ services/cloud                  │
└──────────────────┬───────────────────┘          │ - workspaces                   │
                   │                              │ - membership / join codes      │
                   v                              │ - sync events                  │
┌──────────────────────────────────────┐ publish  │ - retention / hosted audit     │
│ blipd                                ├─────────►└───────────────┬─────────────────┘
│ - capture owner                      │                          │
│ - local policy                       │ updates                  │ realtime updates
│ - local audit                        │                          │
│ - outbound hosted publish gate       │          ┌───────────────▼─────────────────┐
└───────────────┬──────────────┬───────┘          │ apps/web                         │
                │              │                  │ browser join/view surface         │
                │              │                  └─────────────────────────────────┘
                v              v
┌─────────────────────┐ ┌───────────────────────┐
│ blip CLI             │ │ desktop app           │
│ explicit publish     │ │ create/join/publish   │
│ status / scripting   │ │ visible share mode    │
└─────────────────────┘ └───────────────────────┘
```

## Component Ownership

### Local Runtime

`blipd` owns:

- local clipboard watching
- local ingestion into `inbox`
- local workspace routing
- local redaction and payload policy
- local audit events
- outbound hosted publish decisions

The CLI and desktop app remain clients of `blipd`. They do not watch the
clipboard directly and should not upload raw clipboard activity outside the
daemon-approved publish path.

### Hosted Cloud Service

The hosted service owns:

- hosted workspace records
- membership and roles
- join code lifecycle
- hosted blip metadata and retained payloads
- hosted audit log
- real-time workspace update fanout
- hosted retention and deletion jobs

The hosted service does not observe a user's OS clipboard. It receives publish
requests from authenticated clients or daemon-mediated sync clients. For the MVP,
this service must be runnable by the user or organization that wants the shared
workspace.

### Desktop App

The desktop app is a required MVP workflow surface:

- create or join a hosted workspace
- enter an invite code
- see hosted workspace membership and sync status
- explicitly publish selected or latest local blips
- enable or disable hosted sticky/share mode with visible state

### CLI

The CLI is a required MVP workflow surface for scriptable hosted operations:

- login or configure hosted session
- join by code
- list hosted workspaces
- publish selected/latest blips
- inspect sync status

CLI support should not require hosted mode for local-only users.

### Web App

The web app is an optional MVP surface for collaborators who do not have the
desktop app installed. If it is included in Phase 11, it should live in
`apps/web`; if implementation pressure is high, it can follow the desktop and
CLI path.

MVP web scope:

- enter or open a join code
- view hosted blips in real time
- inspect details and tags according to role
- copy/export with audit logging

The web app is not a replacement for local capture. Browsers cannot become the
local clipboard owner for the desktop runtime.

## Data Flow

### Explicit Publish

```text
OS clipboard -> blipd -> local store -> user selects blip
                                      -> publish request
                                      -> hosted service
                                      -> other members
```

1. `blipd` captures a local blip.
2. The user selects a hosted workspace and publishes the blip.
3. `blipd` checks local policy and writes a local audit event.
4. The hosted client sends the blip metadata and allowed payload data.
5. The hosted service validates membership and writes hosted audit events.
6. Other members receive an ordered workspace update.

### Hosted Sticky Share

```text
OS clipboard -> blipd -> local policy -> hosted sticky enabled?
                                      -> hosted service
```

Hosted sticky share is optional. It must be visibly enabled in desktop and easy
to turn off. It should be scoped to one hosted workspace and should not silently
share every local clipboard event across all workspaces.

### Join Flow

```text
owner creates workspace -> owner creates join code
                         -> collaborator enters code
                         -> hosted service grants membership
                         -> clients sync workspace state
```

Join codes are temporary invites, not permanent credentials. Membership should
persist through a member/device identity after the code is redeemed.

## MVP Client Surface

MVP order:

1. desktop create/join/publish flow
2. daemon-mediated hosted publish/sync client
3. CLI status and explicit publish commands
4. optional web join/view surface

This order keeps the safest user-facing control first: desktop makes hosted
state visible while preserving local capture ownership.

## Non-Goals

Phase 11 MVP does not include:

- automatic upload of every local clipboard event
- cloud-side clipboard watching
- desktop-only hosted behavior that bypasses `blipd`
- anonymous public workspaces
- a mandatory official relay service
- requiring users to expose a public port when a private network such as
  Tailscale is sufficient
- unaudited raw payload export
- treating a join code as a permanent bearer token
- splitting hosted code into a separate repo before service ownership matures

## Follow-Up Architecture Questions

The next Phase 11 issues should decide:

- the required security controls from the
  [Hosted workspace threat model](./hosted-threat-model.md)
- hosted data model and event ordering from the
  [Hosted sync protocol](./hosted-sync-protocol.md)
- join code entropy, expiry, revocation, and rate limits
- hosted blob size limits and retention
- deployment environments, cost model, backups, and incident procedures

Those decisions should keep the invariant from this document: local capture is
daemon-owned, and hosted sharing is explicit or visibly opt-in.

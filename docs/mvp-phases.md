# MVP Phases

This is the implementation order that minimizes risk.

## MVP-0: Repository and crate layout

Create:

- `crates/blip-core`
- `crates/blip-daemon`
- `crates/blip-cli`
- `apps/desktop`

Do not implement UI first.

## MVP-1: Local store

Must have:

- SQLite DB
- `blips` table
- `workspaces` table
- `audit_events` table

Success condition:

- can insert and query blips without clipboard watching

## MVP-2: Clipboard ingestion

Must have:

- daemon process
- clipboard watcher or polling abstraction
- `inbox` ingestion

Success condition:

- copying text creates persisted blips automatically

## MVP-3: Basic CLI

Must have:

- `blip inbox`
- `blip workspaces`
- `blip create <name>`
- `blip send <workspace>`
- `blip use <workspace>`
- `blip list <workspace>`

Success condition:

- a user can manage multiple task buckets from terminal only

## MVP-4: Active workspace scoping

Must have:

- active workspace state
- agent-safe scoped read commands
- no default access to `inbox`

Success condition:

- an agent can read only the chosen workspace

## MVP-5: Desktop app

Must have:

- inbox view
- workspace view
- active workspace badge
- detail panel

Success condition:

- user can inspect and route blips visually

## MVP-6: Fast routing

Must have:

- global shortcuts
- sticky workspace mode
- quick workspace send action for latest blip

Success condition:

- multitasking between 3-5 active workstreams feels practical

## MVP-7: Redaction and search polish

Must have:

- basic secret detection
- FTS search
- type tags
- bundle builder

Success condition:

- blips are searchable, safer, and useful as agent context bundles

## MVP-8: CI baseline

Must have:

- GitHub Actions workflow
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --workspace`

Success condition:

- every PR into `develop` or `main` gets the Rust baseline checks automatically

# Upgrade and Migration Strategy

This guide defines how `blipcoard` upgrades should handle SQLite schema
migrations, daemon/API compatibility, and rich payload blob storage.

## Upgrade Model

Upgrade `blipcoard` as one local runtime:

- `blipd`
- `blip`
- `blipcoard` desktop app

Install matching binaries from the same release. The CLI and desktop app are
daemon clients; they should not be upgraded independently from `blipd` for
normal use.

Recommended upgrade sequence:

1. Stop the daemon with `blip service stop` or stop the foreground `blipd`
   process.
2. Back up config, SQLite, SQLite sidecar files, and `blobs/`.
3. Replace all installed binaries from the same release.
4. Start the daemon with `blip service start` or foreground `blipd`.
5. Run `blip service status` and `blip health`.
6. Smoke-test list/search/routing and at least one rich payload preview/export
   if the store contains rich payloads.

## Migration Timing

SQLite migrations run when a process opens the store through `BlipStore::open()`.
The normal upgrade path is daemon startup:

1. `blipd` loads or creates config.
2. `blipd` opens the configured SQLite database.
3. `BlipStore::open()` applies pending numbered migrations inside an immediate
   SQLite transaction.
4. The store initializes the default `inbox` and active workspace if needed.
5. The daemon starts serving IPC and clipboard ingestion.

`blip service install` only writes service manager files. It does not migrate
the database by itself. The first post-upgrade daemon start performs migration.

Some bootstrap or admin CLI paths currently open the store directly while the
daemon API is incomplete. Those paths also run pending migrations because they
use `BlipStore::open()`. Release tests should therefore verify both daemon
startup and any direct-store admin path that remains supported.

## SQLite Schema Rules

Schema changes are numbered migrations in `crates/blip-core/src/sql/` and are
registered in `crates/blip-core/src/store.rs`.

Rules for new migrations:

- Add a new SQL file with the next sequential number.
- Register it in `MIGRATIONS`.
- Increase `SCHEMA_VERSION`.
- Keep migration SQL idempotent where practical, but rely on `PRAGMA
  user_version` for normal one-time execution.
- Add a migration test that starts from the previous relevant schema version and
  verifies the resulting data shape.
- Treat migration failures as startup failures. Do not continue daemon startup
  against a partially upgraded or unknown store.

The migration runner updates `PRAGMA user_version` after each migration
succeeds. Store initialization also records the current schema version in
`app_state`.

## Backup Requirements

Back up SQLite and blobs together before upgrades that can affect schema,
payload metadata, retention, or blob references.

Include:

- `config.toml`
- `blipcoard.db`
- SQLite sidecar files such as `blipcoard.db-wal` and `blipcoard.db-shm` when
  present
- `blobs/`

Simple file-copy backup:

```sh
blip service stop
# copy config.toml, blipcoard.db*, and blobs/
blip service start
```

The blob directory is part of the store. SQLite contains payload metadata and
blob references; `blobs/` contains the bytes. Restoring only SQLite can leave
payload rows with missing blobs. Restoring only `blobs/` can leave orphan files
that clients cannot see.

Before destructive migrations, release notes must explicitly recommend a full
backup. Destructive means any migration that drops columns/tables, rewrites blob
references, deletes payload metadata, changes retention behavior, or cannot be
reversed by simply reinstalling the older binary.

## Rollback Expectations

Rollback is not guaranteed after a migration has run.

Supported rollback expectation:

- If the new daemon has not started and no migration has run, reinstall the
  previous matching binaries.
- If a migration has run, restore the full pre-upgrade backup before running old
  binaries.

Not rollback-safe:

- Running older binaries against a database with a newer `PRAGMA user_version`.
- Restoring SQLite without matching `blobs/`.
- Restoring `blobs/` without matching SQLite metadata.
- Downgrading after retention cleanup or blob garbage collection has deleted
  data.
- Downgrading after a migration rewrites payload metadata or blob references.

The product should prefer clear startup or client errors over best-effort
downgrade behavior.

## Daemon/API Compatibility

The daemon API version lives in `blip-api` as `DAEMON_API_VERSION`. Clients send
their expected API version with each daemon request. The daemon rejects
unsupported request versions with a typed daemon error. The CLI also checks the
response API version and reports:

```text
daemon API version mismatch: response used <actual>, expected <expected>
```

Action for users:

1. Install matching `blip`, `blipd`, and desktop binaries from the same release.
2. Restart the daemon.
3. Re-run `blip health` or `blip service status`.

Desktop builds should bundle matching `blipd` and `blip` sidecars. CLI-only
packages should install matching `blip` and `blipd` binaries together. Mixed
versions are not a supported steady state.

## Rich Payload Storage

Rich payload upgrades must account for both metadata and bytes:

- Text-compatible payload metadata lives in SQLite.
- Binary payload bytes and generated previews live under `blobs/`.
- Payload summaries can report missing blob states; missing blobs are not
  expected to panic clients.
- Garbage collection removes only unreferenced blobs.

Migration tests for rich payload changes should include:

- SQLite-only restore with missing blobs.
- Blob-only restore with no SQLite references.
- Complete restore with SQLite plus `blobs/`.
- Preview blob references as well as raw payload blob references.
- Retention cleanup after migration.

## Release Migration Checklist

For every release that changes schema, payload metadata, blob layout, daemon API,
or service startup:

- Build previous supported release binaries.
- Create a fixture store with plain text blips, rich text payloads, image
  payloads, file-list metadata, agent-readable and human-only workspaces, and at
  least one redacted/secret-like blip.
- Back up `config.toml`, `blipcoard.db*`, and `blobs/`.
- Upgrade to the candidate release.
- Start `blipd` and confirm migrations complete.
- Verify `PRAGMA user_version` and `app_state.schema_version`.
- Run `blip health`, `blip service status`, `blip workspaces`, `blip inbox`,
  `blip search`, `blip payload inspect`, `blip payload preview`, and `blip
  payload export`.
- Verify CLI and desktop clients can talk to the upgraded daemon without API
  mismatch errors.
- Verify an intentionally mismatched older client reports an actionable API
  mismatch error.
- Verify missing-blob and orphan-blob scenarios remain inspectable and
  garbage-collectable.
- Restore the pre-upgrade backup and confirm the previous supported release
  still works against the restored store.
- Record the migration source version, target version, platform, and result in
  release notes.

For releases without schema or daemon API changes, still smoke-test `blip
health`, `blip service status`, and a basic list/search flow after replacing the
binaries.

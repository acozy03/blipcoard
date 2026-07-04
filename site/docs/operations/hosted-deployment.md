# Hosted Deployment And Operations

Hosted workspaces are self-hostable for the MVP. A user or organization can run
`services/cloud` on infrastructure they control and expose it over a private
network such as Tailscale. A managed relay can be added later, but Phase 11 does
not require `blipcoard` operators to host user data.

This page defines the deployment, operations, and cost model for the
server-readable MVP described in the
[hosted threat model](../architecture/hosted-threat-model.md).

## Environments

| Environment | Purpose | Owner | Data policy |
| --- | --- | --- | --- |
| Local | Development and manual smoke tests | Developer | Disposable local data |
| Staging | Repeatable deploy validation before release | Service operator | Synthetic or approved test data only |
| Production | Self-hosted team workspace service | Service operator | Real user data under that operator's policy |

Local command:

```bash title="Run hosted service locally"
BLIPCOARD_CLOUD_DB=./blipcoard-cloud.db cargo run -p blip-cloud
```

Staging and production should run the same binary and configuration shape. The
only expected differences are bind address, TLS/private-network exposure,
database path or database URL, retention defaults, log destination, and backup
target.

## Repeatable Staging Deploy

The MVP staging path is a single-host service exposed over Tailscale or an
internal HTTPS reverse proxy. It uses the same `blip-cloud` binary as local and
production environments.

Build the service artifact from a clean checkout:

```bash title="Build staging binary"
git fetch origin
git checkout origin/develop
cargo build -p blip-cloud --release --locked
install -D -m 0755 target/release/blip-cloud /opt/blipcoard/bin/blip-cloud
```

Create a staging environment file:

```bash title="/etc/blipcoard/cloud-staging.env"
BLIPCOARD_CLOUD_BIND=127.0.0.1:8732
BLIPCOARD_CLOUD_DB=/var/lib/blipcoard/staging/blipcoard-cloud.db
```

Run it under the host process manager. A minimal systemd unit is enough for the
MVP:

```ini title="/etc/systemd/system/blipcoard-cloud-staging.service"
[Unit]
Description=blipcoard hosted workspace service (staging)
After=network-online.target
Wants=network-online.target

[Service]
EnvironmentFile=/etc/blipcoard/cloud-staging.env
ExecStart=/opt/blipcoard/bin/blip-cloud
Restart=on-failure
RestartSec=5
User=blipcoard
Group=blipcoard
StateDirectory=blipcoard/staging
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

Deploy or update staging:

```bash title="Deploy staging"
systemctl daemon-reload
systemctl enable --now blipcoard-cloud-staging
systemctl restart blipcoard-cloud-staging
systemctl status blipcoard-cloud-staging --no-pager
```

Validate staging after each deploy:

```bash title="Validate staging"
curl -fsS http://127.0.0.1:8732/v1/health
BLIPCOARD_RELAY_URL=http://127.0.0.1:8732 npm run web:build
```

If `/v1/health` changes, use the lowest-cost read endpoint available in
`services/cloud` and update this staging procedure in the same PR.

## Recommended MVP Stack

Start with the smallest stack that is durable and self-hostable:

| Layer | MVP choice | Notes |
| --- | --- | --- |
| Runtime | `blip-cloud` binary from `services/cloud` | Run under systemd, Docker, or a platform process manager. |
| Network | Tailscale or HTTPS reverse proxy | Tailscale is preferred for private teams. Public HTTPS needs TLS termination before the service. |
| Metadata store | SQLite | Good default for self-hosted MVP. Document backup and migration rules before relying on it. |
| Blob store | Local filesystem first | Move to object storage when payload volume, backups, or multi-node service need it. |
| Logs | Structured stdout/stderr | Forward to journald, Docker logs, or hosted log storage. Do not log payload content. |
| Metrics | Prometheus-style counters or structured event aggregates | Phase 11.8 requires the signals below even if the first implementation exports them through logs. |

Move metadata to Postgres only when one of these is true:

- more than one service instance must write concurrently
- backup/restore procedures need point-in-time recovery
- operator tooling already standardizes on Postgres
- SQLite write contention appears in sync latency or error metrics

## Configuration And Secrets

Implemented environment variables:

| Variable | Required | Purpose |
| --- | --- | --- |
| `BLIPCOARD_CLOUD_BIND` | No | Socket address, default `127.0.0.1:8732`. |
| `BLIPCOARD_CLOUD_DB` | Yes outside local dev | SQLite database path. |

Target operational variables:

| Variable | Required before production use | Purpose |
| --- | --- | --- |
| `BLIPCOARD_CLOUD_BLOB_DIR` | When raw blob storage is enabled | Local raw payload storage path. |
| `BLIPCOARD_CLOUD_PUBLIC_URL` | For browser clients and invite links | URL shown to clients and used in invite links. |
| `BLIPCOARD_CLOUD_SESSION_SECRET` | Before durable sessions leave local dev | Secret used to sign or derive session credentials. |
| `BLIPCOARD_CLOUD_LOG_LEVEL` | Before staging observability is standardized | Default should be `info`. |

Secrets rules:

- never commit secrets, join codes, session tokens, database files, or blob
  directories
- load production secrets from the host secret manager, systemd credential,
  Docker secret, or platform secret store
- rotate `BLIPCOARD_CLOUD_SESSION_SECRET` through a planned session invalidation
  window until refresh-token support exists
- redact join codes, session tokens, idempotency keys, raw blip content, and
  payload bytes from logs and errors

## Migrations

Hosted schema migrations follow the local store principles from
[Upgrade and migrations](./upgrade-migrations.md), with hosted-specific
operational gates:

1. Back up the hosted database and, once implemented, the blob directory before
   applying migrations.
2. Stop writes or put the service in maintenance mode for SQLite migrations.
3. Apply numbered migrations at service startup or through an explicit migration
   command.
4. Fail startup if a migration fails; do not run with a partial schema.
5. Record migration source version, target version, commit SHA, operator, start
   time, finish time, and result.

Destructive migrations require release notes and a tested restore path. A
migration is destructive if it drops columns, rewrites payload references,
changes retention behavior, deletes audit data, or changes membership/session
semantics.

## Backups And Restore

Backups must include hosted metadata. Once hosted raw blob storage is enabled,
they must also include blob bytes.

SQLite backup procedure:

```bash title="SQLite backup"
sqlite3 "$BLIPCOARD_CLOUD_DB" ".backup '/backups/blipcoard-cloud-$(date -u +%Y%m%dT%H%M%SZ).db'"
if [ -n "${BLIPCOARD_CLOUD_BLOB_DIR:-}" ] && [ -d "$BLIPCOARD_CLOUD_BLOB_DIR" ]; then
  tar -C "$BLIPCOARD_CLOUD_BLOB_DIR" -czf "/backups/blipcoard-blobs-$(date -u +%Y%m%dT%H%M%SZ).tgz" .
fi
```

Restore test procedure:

1. Start a fresh staging service from the database backup and blob archive.
2. Join a test workspace.
3. List workspaces, blips, members, audit events, and tags.
4. Export at least one text blip and one rich payload.
5. Confirm retention jobs do not delete non-expired restored data.

Backup policy:

- encrypt backups at rest
- store backups outside the primary host
- keep staging backups short-lived
- document production retention, such as daily backups for 14 days and weekly
  backups for 8 weeks
- document that backup retention can delay physical purge after user deletion

## Retention And Deletion Jobs

Retention must be visible and workspace-scoped.

Required jobs:

- expire hosted blips past workspace `retention_days`
- delete raw payload blobs after their metadata is deleted and no live hosted
  payload references remain
- expire unused join codes
- revoke inactive sessions after the configured session lifetime
- compact or archive audit events only after the product policy allows it

Deletion behavior:

- deleting a hosted blip soft-deletes metadata, emits `blip_deleted`, and
  schedules blob cleanup
- deleting a workspace revokes join codes, revokes member sessions, stops sync
  channels, soft-deletes workspace records, and schedules blob cleanup
- physical blob deletion is asynchronous and auditable
- backup purge timing is documented separately from live-store deletion

## Logs And Observability

Logs and metrics must debug sync without exposing clipboard content.

Required counters:

- API requests by route, status, workspace role, and error code
- denied reads/writes by reason
- join-code creation, redemption, expiry, revocation, and failed attempts
- publish attempts, successes, conflicts, payload-too-large failures, and quota
  failures
- payload uploads and exports by byte bucket, not raw content
- WebSocket connects, disconnects, reconnects, and heartbeat timeouts
- retention job runs, deleted blips, deleted blobs, and failures
- migration attempts and results

Required latency histograms:

- API request duration by route
- publish-to-event-commit latency
- event fanout latency
- reconnect catch-up latency
- payload upload and export latency

Operator alerts:

| Signal | Alert when |
| --- | --- |
| API error rate | 5xx rate is elevated for 5 minutes. |
| Join-code failures | invalid or expired attempts spike by source. |
| Sync lag | event fanout or reconnect catch-up exceeds the SLO. |
| WebSocket churn | reconnects spike for active workspaces. |
| Retention failures | any retention job fails twice consecutively. |
| Backup failures | scheduled backup or restore validation fails. |
| Disk pressure | database or blob volume exceeds the warning threshold. |

Never log raw `content`, payload bytes, raw join codes, session tokens, or
authorization headers.

## Cost Drivers

Self-hosted operators own their infrastructure costs. The product should expose
enough limits for operators to stay inside a predictable budget.

Primary cost drivers:

- open WebSocket connections
- event fanout per hosted workspace
- stored blob bytes
- blob upload/download bandwidth
- database size from hosted blips, audit events, and workspace events
- backup storage and retention window
- log volume
- join-code abuse and rate-limited traffic

Default MVP quotas:

| Resource | Default |
| --- | --- |
| Workspace members | 50 |
| Open WebSocket connections per member | 5 |
| Plain text content | 1 MiB |
| Single rich payload upload | 25 MiB |
| Payloads per blip | 8 |
| Tags per blip | 32 |
| Audit/event retention | Same as workspace unless configured otherwise |

Cost review checklist:

1. Estimate average blips per workspace per day.
2. Estimate rich payload percentage and average payload size.
3. Estimate active members and concurrent sync connections.
4. Multiply retained daily bytes by retention days and backup retention factor.
5. Add bandwidth for payload exports and browser viewers.
6. Set rate limits and quotas before exposing a public endpoint.

## Incident Procedure

Use this minimum incident flow for hosted service operators:

1. Triage severity: data exposure, data loss, unavailable sync, degraded sync, or
   abuse.
2. Preserve logs and audit events without copying raw payload content into the
   incident channel.
3. If abuse is active, revoke affected join codes, suspend workspace sync, or
   block source addresses.
4. If session tokens are exposed, rotate the session secret and invalidate
   sessions.
5. If data integrity is at risk, stop writes and take an emergency backup before
   repair.
6. Restore from the most recent verified backup if live data cannot be repaired.
7. Record timeline, affected workspaces, user-visible impact, remediation, and
   follow-up controls.

## Data Deletion Procedure

Workspace owners need a documented deletion path:

1. Authenticate the owner or operator request.
2. Confirm the workspace id and deletion scope.
3. Revoke join codes and active member sessions.
4. Stop new writes and sync streams for the workspace.
5. Soft-delete workspace blips, payload metadata, membership records, and tags.
6. Queue physical blob deletion.
7. Write hosted audit events for the deletion request and completion.
8. Report live-store deletion completion and backup purge timing separately.

The MVP should not promise immediate removal from backups. It should promise
that live hosted reads stop after deletion completes and that backup purge
follows the documented backup retention schedule.

## Release Gate

Before hosted workspaces are treated as a product surface, verify:

- staging deploy is repeatable from documented commands
- database and blob backups are restorable
- migrations fail closed
- logs redact sensitive fields
- API error, sync latency, join-code, retention, and abuse signals are visible
- quotas are configured for payload bytes, members, and sync connections
- incident and data-deletion procedures are documented for operators

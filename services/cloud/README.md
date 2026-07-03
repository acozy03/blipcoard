# blip-cloud

Self-hostable hosted workspace service for Phase 11.

Run locally:

```sh
BLIPCOARD_CLOUD_DB=./blipcoard-cloud.db cargo run -p blip-cloud
```

Start the browser hosted workspace client from the repository root:

```sh
npm run web:dev
```

The web client joins a workspace with a relay URL and join code, then uses the
redeemed member/device session for blip reads, tag updates, copy/export audit
events, and presence.

Default bind address:

```text
127.0.0.1:8732
```

Environment:

| Variable | Purpose |
| --- | --- |
| `BLIPCOARD_CLOUD_BIND` | Socket address for the HTTP service. |
| `BLIPCOARD_CLOUD_DB` | SQLite database path for local/staging metadata. |

The MVP uses SQLite for a self-hosted durable service. Phase 11.8 owns the
production deployment and cost model, including whether managed deployments move
metadata to Postgres.

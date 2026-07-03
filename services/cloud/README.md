# blip-cloud

Self-hostable hosted workspace service for Phase 11.

Run locally:

```sh
BLIPCOARD_CLOUD_DB=./blipcoard-cloud.db cargo run -p blip-cloud
```

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

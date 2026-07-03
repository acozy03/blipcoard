# Troubleshooting

Start with the daemon and service checks:

```sh
blip service status
blip service logs
blip health
```

`blip service status --output json` is useful when filing an issue because it
includes configured service, daemon, config, database, socket, and log paths.

## Daemon Not Running

Error:

```text
daemon is not running at the configured socket; start blipd --ipc-only or the full daemon first
```

Fix:

```sh
blip service start
blip service status
```

For development, run the full daemon in the foreground:

```sh
cargo run -p blip-daemon
```

`blipd --ipc-only` is for tests and manual IPC checks. It does not watch the
clipboard.

## Stale Socket

Error:

```text
daemon socket exists but no daemon accepted the connection; restart blipd
```

Fix:

```sh
blip service restart
blip health
```

If running a foreground daemon, stop it and start one daemon instance using the
same config and socket path as the CLI.

## API Version Mismatch

Error:

```text
daemon API version mismatch: response used <actual>, expected <expected>
```

Fix:

1. Install matching `blip` and `blipd` binaries from the same release.
2. Restart the daemon.
3. Run `blip health`.

For source builds, rebuild both binaries from the same checkout.

## Daemon API Errors

Error shape:

```text
daemon returned <code>: <message>
```

Current daemon error codes:

| Code | Meaning |
| --- | --- |
| `unsupported_api_version` | Client and daemon API versions are incompatible. |
| `invalid_request` | The request payload is malformed or not valid for the command. |
| `not_found` | A workspace, blip, payload, or other resource does not exist. |
| `access_denied` | Workspace or payload policy blocks the request. |
| `store_unavailable` | The local store could not be opened or queried. |
| `missing_blob` | SQLite references a blob file that is missing locally. |
| `unsupported_payload` | The payload cannot be previewed or exported for this operation. |
| `payload_too_large` | The daemon refused an oversized payload operation. |
| `internal` | The daemon hit an unexpected runtime error. |

## Workspace Errors

Error:

```text
workspace `<name>` does not exist
```

Fix:

```sh
blip workspaces
blip create <name>
```

Error:

```text
workspace `<name>` already exists
```

Choose another workspace name or use the existing workspace.

## Payload Export Errors

Error:

```text
refusing to overwrite existing file: <path> (use --force to replace)
```

Fix:

```sh
blip payload preview <payload-id> <path> --force
blip payload export <payload-id> <path> --force
```

Error:

```text
parent directory does not exist: <path>
```

Create the parent directory first or choose a different output path.

Error code:

```text
missing_blob
```

The database has payload metadata, but the corresponding blob file is absent.
Restore the database and `blobs/` directory from the same backup set.

## Service Manager Failures

Error:

```text
<program> failed with status <code>: <stderr>
```

The platform service command failed. Inspect the service manager directly:

```sh
journalctl --user -u blipd
systemctl --user status blipd
```

On macOS:

```sh
launchctl print gui/$(id -u)/com.acozy03.blipcoard.blipd
```

Then compare the paths from:

```sh
blip service plan
```

## Windows Service Startup

Error:

```text
Windows service startup is not available until Windows daemon IPC is implemented
```

Windows service management is not supported yet. Use foreground daemon runs for
development until Windows IPC and service startup land.

## Config And Data Locations

Use `blip service plan` to print the exact paths for the current machine.

Default path model:

| Platform | Config | Data |
| --- | --- | --- |
| macOS | `~/Library/Application Support/com.acozy03.blipcoard/config.toml` | `~/Library/Application Support/com.acozy03.blipcoard/blipcoard.db` and `blobs/` |
| Linux | `$XDG_CONFIG_HOME/blipcoard/config.toml` or `~/.config/blipcoard/config.toml` | `$XDG_DATA_HOME/blipcoard/blipcoard.db` or `~/.local/share/blipcoard/blipcoard.db`, plus `blobs/` |
| Windows | `%APPDATA%\acozy03\blipcoard\config\config.toml` | `%LOCALAPPDATA%\acozy03\blipcoard\data\blipcoard.db` and `blobs/` |

Useful overrides:

| Variable | Purpose |
| --- | --- |
| `BLIPCOARD_CONFIG_DIR` | Use a different config directory. |
| `BLIPCOARD_DB_PATH` | Use a different SQLite database path. |
| `BLIPCOARD_SOCKET_PATH` | Use a different daemon IPC socket path on Unix. |
| `BLIPCOARD_BLIPD_PATH` | Point service installation at a specific `blipd` binary. |

## Backup And Restore Problems

Back up and restore these files together:

- `config.toml`
- `blipcoard.db`
- SQLite sidecar files such as `blipcoard.db-wal` and `blipcoard.db-shm`
- `blobs/`

Stop the daemon before simple file-copy backups:

```sh
blip service stop
# copy config.toml, blipcoard.db*, and blobs/
blip service start
```

Restoring only SQLite or only `blobs/` can cause missing previews or failed raw
payload exports.

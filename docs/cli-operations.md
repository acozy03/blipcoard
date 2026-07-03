# CLI Install and Operations

This guide covers CLI-only `blipcoard` installs for users who want terminal
workflows without the desktop app.

CLI-only mode still includes both binaries:

- `blipd`: the long-running daemon that owns clipboard watching, policy,
  storage, and audit writes.
- `blip`: the terminal client that talks to `blipd`.

Do not run CLI-only mode as `blip` alone. Commands that read, route, preview, or
export runtime data expect the daemon API.

## Install Binaries

A supported CLI-only install places `blip` and `blipd` on the user's `PATH`.
The two binaries must come from the same release so daemon API versions stay
aligned.

Release archives should include both binaries. Choose the archive for your OS
and CPU, then copy both binaries into a directory already on `PATH`:

```sh
mkdir -p ~/.local/bin
tar -xzf blipcoard-cli-<platform>-<arch>.tar.gz
cp blip blipd ~/.local/bin/
chmod +x ~/.local/bin/blip ~/.local/bin/blipd
```

If `~/.local/bin` is not on `PATH`, add it in your shell profile:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Verify that both binaries resolve to the expected install location:

```sh
command -v blip
command -v blipd
```

For source checkout development, build and install both binaries from the same
workspace revision:

```sh
cargo install --path crates/blip-cli --locked
cargo install --path crates/blip-daemon --locked
```

Packagers can use a different install prefix, but their package must install
both `blip` and `blipd` and document the exact binary directory.

After installing the binaries, inspect the service plan:

```sh
blip service plan
```

Install and start the per-user daemon service on macOS or Linux:

```sh
blip service install
blip service start
```

`blip service install` does not create a privileged system service. It installs
a user-scoped service that starts the full `blipd` runtime, not
`blipd --ipc-only`.

Platform service behavior:

| Platform | CLI-only service behavior |
| --- | --- |
| macOS | Writes `~/Library/LaunchAgents/com.acozy03.blipcoard.blipd.plist`; the LaunchAgent runs at login and keeps `blipd` alive. |
| Linux | Writes `~/.config/systemd/user/blipd.service`, reloads the user manager, and enables login startup. |
| Windows | Mutating service commands are unavailable until Windows daemon IPC exists. `plan`, `status`, and `logs` still report configured paths and unsupported state. |

For foreground development or troubleshooting:

```sh
blipd
```

From a source checkout:

```sh
cargo run -p blip-daemon
```

`blipd --ipc-only` is only for tests and manual IPC checks. It does not watch
the clipboard.

## Shell Completions

The CLI does not currently ship a `blip completions` command or generated shell
completion files. CLI-only packages should not advertise completions until a
future release adds a supported generation command or checked-in completion
artifacts.

## Verify Health

Check daemon/service status:

```sh
blip service status
blip service status --output json
```

Check daemon API health:

```sh
blip health
blip health --output json
```

A healthy daemon reports service `blipd`, status `ready`, the database path, the
active workspace, and a timestamp. If the daemon socket is missing or stale, the
CLI prints an actionable error and `blip service status` prints the configured
socket plus `blip service start`.

If `blipd` is already running at the configured socket, a second daemon startup
fails with a message pointing to:

```sh
blip service status
blip service restart
```

## Logs

Print the platform log location or inspection command:

```sh
blip service logs
```

Current locations:

| Platform | Logs |
| --- | --- |
| macOS | `logs/blipd.log` beside the configured `blipcoard.db`. |
| Linux | `journalctl --user -u blipd`. |
| Windows | Unsupported until Windows daemon IPC and service startup exist. |
| Foreground daemon | stdout and stderr in the launching terminal. |

## Config and Data

`blip`, `blipd`, and the desktop app share one config and one local store.

Default paths:

| Platform | Config | Data |
| --- | --- | --- |
| macOS | `~/Library/Application Support/com.acozy03.blipcoard/config.toml` | `~/Library/Application Support/com.acozy03.blipcoard/blipcoard.db` and `blobs/` |
| Linux | `$XDG_CONFIG_HOME/blipcoard/config.toml` or `~/.config/blipcoard/config.toml` | `$XDG_DATA_HOME/blipcoard/blipcoard.db` or `~/.local/share/blipcoard/blipcoard.db`, plus `blobs/` |
| Windows | `%APPDATA%\acozy03\blipcoard\config\config.toml` | `%LOCALAPPDATA%\acozy03\blipcoard\data\blipcoard.db` and `blobs/` |

Useful environment overrides:

| Variable | Purpose |
| --- | --- |
| `BLIPCOARD_CONFIG_DIR` | Use a different config directory. |
| `BLIPCOARD_DB_PATH` | Use a different SQLite database path. |
| `BLIPCOARD_SOCKET_PATH` | Use a different daemon IPC socket path on Unix. |
| `BLIPCOARD_BLIPD_PATH` | Point `blip service install` at a specific `blipd` binary. |

Common config settings live under `[capture]` in `config.toml`:

```toml
[capture]
enabled = true
text = true
image = true
file_list = true
html = true
rtf = true
unknown = true
max_image_bytes = 52428800
image_previews = true
```

Disabling `capture.enabled` stops all clipboard capture while keeping daemon API
commands available.

## Backup Basics

Back up the database and rich payload blob directory together:

- `blipcoard.db`
- SQLite sidecar files such as `blipcoard.db-wal` and `blipcoard.db-shm` when
  present
- `blobs/`
- `config.toml`

Stop the daemon before taking a simple file copy backup:

```sh
blip service stop
# copy config.toml, blipcoard.db*, and blobs/
blip service start
```

If the daemon is running in the foreground, stop that process before copying the
store. Blob files are content-addressed and referenced from SQLite, so restoring
only the database or only `blobs/` can leave payload previews and exports
incomplete.

## CLI Policy Model

The CLI is a daemon client for normal runtime operations. These commands go
through `blipd` and enforce workspace policy:

- `blip health`
- `blip current`
- `blip workspaces`
- `blip policy`
- `blip inbox`
- `blip list`
- `blip search`
- `blip use`
- `blip send`
- `blip agent *`
- `blip payload *`

Agent commands require workspace agent access. Raw payload export for agents
requires the workspace `agent_raw_payload_access` policy. CLI payload preview
and export operations are audited by the daemon.

Bootstrap/admin commands that create demo data or repair local state may open
the store directly while the daemon API is incomplete. Those commands must not
watch the clipboard.

## Troubleshooting

Use this sequence first:

```sh
blip service status
blip service logs
blip health
```

Common fixes:

| Symptom | Action |
| --- | --- |
| Daemon socket missing | Run `blip service start`. |
| Socket exists but daemon is stale | Run `blip service restart`. |
| Another daemon is already running | Use `blip service status`; restart only the configured service. |
| CLI and daemon disagree after upgrade | Install matching `blip` and `blipd` binaries from the same release, then restart the daemon. |
| Capture is unexpectedly quiet | Check `[capture]` settings in `config.toml` and confirm the active workspace with `blip current`. |

Uninstall the user service without deleting data:

```sh
blip service uninstall
```

Default uninstall behavior should preserve `config.toml`, SQLite, and `blobs/`.
Use an explicit purge path only when you intentionally want to delete local
clipboard history and payload files.

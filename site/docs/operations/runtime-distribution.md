# Runtime Distribution Model

Phase 9 treats `blipcoard` as one local runtime system with multiple entry
points. Packaging must preserve daemon ownership of clipboard ingestion,
workspace policy, audit writes, and local store access.

## Install Modes

Supported install modes:

- Full install: installs `blipd`, `blip`, and the `blipcoard` desktop app.
- CLI-only install: installs `blipd` and `blip`, including through the npm
  package for users who want a standard package-manager update path.

Unsupported install mode:

- Desktop-only install. The desktop app is not a standalone product and must not
  ship without the daemon and CLI.

Every supported install includes `blipd`. The CLI and desktop app are clients of
the daemon runtime; they must not become separate clipboard watchers or policy
engines.

CLI-only install and operations details live in
[`cli-operations.md`](cli-operations.md).
Upgrade and migration details live in
[`upgrade-migrations.md`](upgrade-migrations.md).
Release automation details live in
[`release-automation.md`](release-automation.md).

## Component Ownership

- `blipd` owns clipboard watching, ingestion, duplicate suppression, retention
  cleanup, rich payload blob lifecycle, policy checks, and audit writes.
- `blip` owns terminal interaction and formats daemon responses for humans,
  JSON output, and scripts.
- The desktop app owns visual inspection and routing workflows and uses the same
  daemon API as the CLI.
- `blip-core` owns durable SQLite and blob-store rules, but non-repair runtime
  writes should go through `blipd`.

Bootstrap or repair commands may open the store directly only when the daemon is
unavailable or the command is explicitly local-admin behavior. Those paths must
not watch the clipboard.

## Discovery

Installed components discover each other through config and the daemon IPC path:

1. `blipd`, `blip`, and the desktop app load the same config file.
2. Config is created on first run when missing.
3. The config records the SQLite database path.
4. The daemon socket path defaults to the database path with a `.sock`
   extension.
5. `BLIPCOARD_CONFIG_DIR`, `BLIPCOARD_DB_PATH`, and `BLIPCOARD_SOCKET_PATH`
   override the defaults for tests, portable installs, or admin repair.

Current Unix IPC is newline-delimited JSON over a user-scoped Unix socket.
Windows packaging must provide an equivalent current-user IPC path before
Windows installs are considered complete; the current daemon IPC implementation
reports Windows IPC as unavailable.

## Platform Paths

The current implementation uses `directories::ProjectDirs::from("com",
"acozy03", "blipcoard")`.

| Platform | Config file | Data and blob store | Default daemon IPC |
| --- | --- | --- | --- |
| macOS | `~/Library/Application Support/com.acozy03.blipcoard/config.toml` | `~/Library/Application Support/com.acozy03.blipcoard/blipcoard.db` plus `blobs/` under the same data directory | `~/Library/Application Support/com.acozy03.blipcoard/blipcoard.sock` |
| Linux | `$XDG_CONFIG_HOME/blipcoard/config.toml` or `~/.config/blipcoard/config.toml` | `$XDG_DATA_HOME/blipcoard/blipcoard.db` or `~/.local/share/blipcoard/blipcoard.db` plus `blobs/` under the same data directory | same database path with `.sock` extension |
| Windows | `%APPDATA%\acozy03\blipcoard\config\config.toml` | `%LOCALAPPDATA%\acozy03\blipcoard\data\blipcoard.db` plus `blobs/` under the same data directory | target: current-user named pipe or equivalent user-scoped IPC; current implementation is unavailable |

Rich payload blobs are stored beside SQLite and must be backed up, restored,
retained, and garbage-collected with the database.

## Service Commands

The `blip` CLI owns user-facing daemon lifecycle commands:

```sh
blip service plan
blip service install
blip service uninstall
blip service start
blip service stop
blip service restart
blip service status
blip service logs
```

`blip service install` installs a per-user service. It does not install a
privileged system service and it starts the full `blipd` runtime, never
`blipd --ipc-only`. `blip service status --output json` reports service paths,
the configured socket, and daemon health when IPC is reachable. `blip service
logs` prints the platform log location or inspection command.

`blipd --ipc-only` remains a test and manual IPC mode. Normal installs, desktop
fallback startup, and CLI-managed services should start `blipd` without
`--ipc-only` so clipboard ingestion remains daemon-owned.

## Logs

Service logs are platform-specific:

- macOS: the LaunchAgent writes stdout and stderr to `logs/blipd.log` under the
  configured data directory, next to `blipcoard.db`.
- Linux: the systemd user unit writes to the user journal. Inspect with
  `journalctl --user -u blipd`.
- Windows: service startup remains blocked until Windows IPC exists. Future
  Windows support should use Windows Event Log or a documented per-user log
  file.

Foreground `blipd` sessions continue to write operational messages to stdout or
stderr.

## First Run

On first run:

1. The config directory is created.
2. `config.toml` is written with the default database path and capture policy.
3. The data directory is created.
4. `blipd` opens the database and applies SQLite migrations.
5. The default `inbox` workspace and active workspace state are initialized by
   the store layer.
6. The daemon starts IPC before clipboard ingestion in the full runtime path.

CLI commands that require the daemon show an actionable error when the
configured daemon socket is missing or stale. If another daemon is already
accepting connections at the configured socket, `blipd` exits with a message
that points users to `blip service status` or `blip service restart`.

## Startup

Concrete startup behavior:

- macOS: `blip service install` writes
  `~/Library/LaunchAgents/com.acozy03.blipcoard.blipd.plist`. The LaunchAgent
  runs at login and keeps `blipd` alive. `blip service start` bootstraps the
  user LaunchAgent.
- Linux: `blip service install` writes
  `~/.config/systemd/user/blipd.service` and reloads the user manager.
  The unit is enabled for login startup, and `blip service start` starts it for
  the current session.
- Windows: mutating `blip service` commands such as `install`, `start`, `stop`,
  `restart`, and `uninstall` report that service startup is unavailable until
  Windows daemon IPC is implemented. Informational commands such as `plan`,
  `status`, and `logs` can still describe the configured paths and unsupported
  state. Windows packaging must not claim runtime acceptance before the
  named-pipe or equivalent current-user IPC work lands.
- Development checkout: `cargo run -p blip-daemon` starts the full `blipd`
  daemon runtime; `cargo run -p blip-daemon -- --ipc-only` starts only the IPC
  server for tests and manual client checks.

The daemon remains the runtime owner in every mode. Desktop and CLI launchers
must not silently fall back to direct clipboard watching.

## Uninstall

Default uninstall behavior should remove installed binaries, app bundles,
service files, launch agents, shell completions, and desktop entries while
preserving user data.

User data includes:

- `config.toml`
- `blipcoard.db`
- SQLite sidecar files
- rich payload blob files
- retained logs when they live under user data or service log locations

Packaging may offer an explicit purge option that deletes config, database,
blob, and log data. Purge must never be the default uninstall behavior.

## Phase 9 Follow-Ups

- Phase 9.2 should make desktop bundles include `blipd` and `blip`.
- Phase 9.3 implements macOS and Linux service startup and documents concrete
  log locations. Windows remains blocked by the Windows IPC follow-up.
- Phase 9.4 documents CLI-only install and operations in `cli-operations.md`.
- Phase 9.5 documents backup, upgrade, and migration checks around the config,
  SQLite database, blob directory, and daemon API compatibility in
  `upgrade-migrations.md`.

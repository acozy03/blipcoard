# Runtime Distribution Model

Phase 9 treats `blipcoard` as one local runtime system with multiple entry
points. Packaging must preserve daemon ownership of clipboard ingestion,
workspace policy, audit writes, and local store access.

## Install Modes

Supported install modes:

- Full install: installs `blipd`, `blip`, and the `blipcoard` desktop app.
- CLI-only install: installs `blipd` and `blip`.

Unsupported install mode:

- Desktop-only install. The desktop app is not a standalone product and must not
  ship without the daemon and CLI.

Every supported install includes `blipd`. The CLI and desktop app are clients of
the daemon runtime; they must not become separate clipboard watchers or policy
engines.

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

## Logs

Current binaries write operational messages to stdout or stderr. Phase 9 daemon
install work should route service logs to the platform service manager:

- macOS: launchd-managed stdout/stderr or a per-user log location selected by
  the launch agent.
- Linux: systemd user journal for service installs; stderr for foreground
  sessions.
- Windows: Windows Event Log or a per-user log file chosen by the service
  wrapper.

Packaging must document the chosen log location once service startup behavior is
implemented.

## First Run

On first run:

1. The config directory is created.
2. `config.toml` is written with the default database path and capture policy.
3. The data directory is created.
4. `blipd` opens the database and applies SQLite migrations.
5. The default `inbox` workspace and active workspace state are initialized by
   the store layer.
6. The daemon starts IPC before clipboard ingestion in the full runtime path.

CLI commands that require the daemon should show an actionable error when the
configured daemon socket is missing or stale. Installers should start `blipd`
for full installs and should provide a clear command or service entry for
CLI-only installs.

## Startup

Target startup behavior:

- Full desktop install: user login starts `blipd`; launching the desktop app may
  also start or prompt to start the daemon if it is not running.
- CLI-only install: installer registers an optional user service and documents
  foreground startup for terminal-only users.
- Development checkout: `cargo run -p blip-daemon` starts the daemon runtime;
  `cargo run -p blip-daemon -- --ipc-only` starts only the IPC server for tests
  and manual client checks.

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
- Phase 9.3 should implement platform service startup and document concrete log
  locations.
- Phase 9.4 should turn the CLI-only mode into install and operations docs.
- Phase 9.5 should define backup, upgrade, and migration checks around the
  config, SQLite database, and blob directory.

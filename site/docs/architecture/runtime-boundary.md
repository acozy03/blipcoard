# Runtime Boundary

The runtime boundary keeps clipboard capture, policy decisions, and persistence
in one place: `blipd`.

This matters because clipboard content is often sensitive. If the CLI, desktop
app, and future agent integrations all watched the clipboard independently, they
could disagree about policy, duplicate events, or expose content through
different paths.

## Ownership

### `blip-core`

`blip-core` owns durable domain and storage rules:

- SQLite migrations
- workspace and blip records
- audit events
- typed persisted errors
- transactional writes

It should not become a long-running clipboard watcher.

### `blip-clipboard`

`blip-clipboard` owns platform-specific clipboard observation.

It hides differences between macOS pasteboards, Linux X11 or Wayland clipboard
behavior, and Windows clipboard formats. It reports what the platform can
observe; it does not decide workspace policy.

### `blip-api`

`blip-api` owns shared request and response types for local daemon clients.

CLI and desktop commands should use these shared schemas instead of each
surface inventing its own runtime contract.

### `blipd`

`blipd` owns long-running runtime behavior:

- clipboard watching
- ingestion into `inbox`
- dedupe
- redaction
- workspace routing policy
- local API access
- audit writes
- service lifecycle

The daemon is the policy boundary. A command can be convenient, but policy
should be enforced by the daemon.

### `blip`

`blip` owns terminal interaction.

It should stay thin: parse arguments, call the daemon, format output, and exit.
Bootstrap or repair commands may use direct storage access only when the daemon
is unavailable or the command is explicitly operational.

### Desktop App

The desktop app owns visual interaction:

- workspace navigation
- inbox and timeline views
- detail previews
- active workspace controls
- audit inspection
- settings

It should not watch the clipboard directly. It should use the same daemon API as
the CLI.

## Client Boundary

Local clients communicate with the daemon over a current-user local IPC
boundary:

- Unix domain socket on macOS and Linux
- named pipe on Windows

The API is local-only by default. Hosted or shared workspaces are future
features and should add a separate sync boundary instead of weakening the local
daemon boundary.

## Ingestion Flow

1. `blipd` starts and loads config.
2. `blipd` opens the local store through `blip-core`.
3. `blipd` starts the platform watcher through `blip-clipboard`.
4. A clipboard change arrives.
5. `blipd` classifies the payload and applies capture policy.
6. `blipd` writes the blip and audit event transactionally.
7. CLI and desktop clients read list/detail views through the daemon API.

## Routing Flow

1. The user selects or creates a workspace.
2. The user sends a specific blip, the latest blip, or future captures to that
   workspace.
3. `blipd` validates the workspace and policy.
4. `blipd` moves or copies the blip according to the command semantics.
5. `blipd` writes an audit event with the actor and destination.

## Design Rules

- Only `blipd` watches the clipboard.
- The CLI and desktop app are clients, not alternate runtimes.
- `inbox` is a human triage area.
- Agent reads require explicit readable workspace scope.
- Rich payload bytes are read only when needed for an explicit preview, export,
  or policy-approved access.
- Every policy-sensitive read or write should be auditable.

For lower-level request and response details, see
[Daemon API](../reference/daemon-api.md).

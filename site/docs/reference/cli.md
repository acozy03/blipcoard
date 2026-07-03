# CLI Reference

`blip` is the terminal client for the local `blipd` daemon.

Most runtime commands require the daemon to be running. Commands that read,
route, preview, export, or expose agent context should go through `blipd` so
workspace policy and audit behavior stay centralized.

## Output

Commands that support machine-readable output accept:

```sh
--output human
--output json
```

`human` is the default. JSON output is intended for scripts and tests.

List commands default to `--limit 50` unless another limit is provided.

## Health And State

### `blip health`

Checks daemon API health.

```sh
blip health
blip health --output json
```

Human output includes service name, status, database path, active workspace, and
generation timestamp.

### `blip current`

Prints the active workspace.

```sh
blip current
blip current --output json
```

### `blip workspaces`

Lists workspaces and their high-level access flags.

```sh
blip workspaces
blip workspaces --output json
```

Human output marks workspaces as `human-only` or `agent-readable` and includes
`sticky` when sticky capture is enabled.

## Workspace Policy

### `blip policy <workspace>`

Updates rich payload policy for a workspace and prints the resulting policy.

```sh
blip policy auth-bug \
  --rich-capture true \
  --image-capture true \
  --rich-visibility safe-preview \
  --agent-raw-payload-access false
```

Options:

| Option | Values | Meaning |
| --- | --- | --- |
| `--rich-capture` | `true`, `false` | Enable or disable rich payload capture for the workspace. |
| `--image-capture` | `true`, `false` | Enable or disable image capture for the workspace. |
| `--rich-visibility` | `hidden`, `metadata`, `safe-preview` | Controls rich payload summaries in list and detail views. |
| `--agent-raw-payload-access` | `true`, `false` | Allows or denies raw payload byte access for agent callers. |

Omitted options keep the current value.

## Blips

### `blip inbox`

Lists recent blips in `inbox`.

```sh
blip inbox
blip inbox --limit 20
blip inbox --output json
```

### `blip list <workspace>`

Lists recent blips in a workspace.

```sh
blip list auth-bug
blip list auth-bug --limit 100 --output json
```

### `blip search <workspace> <query>`

Searches blips in a workspace.

```sh
blip search auth-bug "jwt expired"
blip search auth-bug "stack trace" --limit 10 --output json
```

### `blip send <workspace>`

Routes the latest inbox blip into a workspace.

```sh
blip send auth-bug
blip send auth-bug --output json
```

### `blip send <workspace> --id <blip-id>`

Routes a specific blip into a workspace.

```sh
blip send auth-bug --id dev-inbox-4
```

## Workspace Administration

### `blip create <name>`

Creates a workspace through the local store path.

```sh
blip create auth-bug
blip create auth-bug --description "Auth investigation" --color blue
blip create agent-feed --agent-access
```

This is a bootstrap/admin command while workspace creation is not fully exposed
through the daemon API. It does not watch the clipboard.

### `blip use <workspace>`

Sets the active workspace through the daemon.

```sh
blip use auth-bug
```

## Agent Reads

Agent commands are daemon-mediated and require workspace agent access.

### `blip agent recent <workspace>`

```sh
blip agent recent agent-feed
blip agent recent agent-feed --limit 10 --output json
```

### `blip agent search <workspace> <query>`

```sh
blip agent search agent-feed "checkout"
```

### `blip agent bundle <workspace>`

Builds a text bundle of recent agent-readable blips.

```sh
blip agent bundle agent-feed
blip agent bundle agent-feed --limit 25 --output json
```

`inbox` is human-only by default, so agent commands against `inbox` should fail
unless policy is explicitly changed.

## Payloads

### `blip payload inspect <payload-id>`

Prints payload metadata without exporting raw bytes.

```sh
blip payload inspect dev-inbox-3:payload:image
blip payload inspect dev-inbox-3:payload:image --output json
```

### `blip payload preview <payload-id> <output-path>`

Writes safe preview bytes to a file.

```sh
blip payload preview dev-inbox-3:payload:image ./preview.png
blip payload preview dev-inbox-3:payload:image ./preview.png --force
```

The command refuses to overwrite an existing file unless `--force` is set.

### `blip payload export <payload-id> <output-path>`

Writes raw payload bytes to a file when policy allows it.

```sh
blip payload export dev-inbox-3:payload:image ./raw.png
```

Raw exports are audited and can be denied by workspace policy.

## Service Management

### `blip service plan`

Shows the service manager, paths, and install plan.

```sh
blip service plan
blip service plan --output json
```

### `blip service install`

Installs the current-user daemon service on macOS or Linux.

```sh
blip service install
blip service install --print
```

`--print` writes the service file content without installing it.

### `blip service start|stop|restart|status|logs`

```sh
blip service start
blip service stop
blip service restart
blip service status
blip service status --output json
blip service logs
```

Windows mutating service commands are unavailable until Windows daemon IPC and
service startup are implemented.

## Demo Data

### `blip add-demo <workspace> <content>`

Creates a demo text blip through the local store path.

```sh
blip add-demo inbox "copied stack trace" --source-app Terminal
```

This is for local development and demos. It does not watch the clipboard.

# Product Model

`blipcoard` is a local-first clipboard routing system for people who work with
coding agents, terminals, bug reports, screenshots, and copied snippets across
several tasks at once.

The product problem is simple: the OS clipboard is global, invisible, and easy
to over-share. `blipcoard` turns clipboard events into visible records called
blips, then lets the user decide which workspace each blip belongs to.

## Core Ideas

### Blips

A blip is one clipboard event captured by `blipd`.

Blips keep the information needed to inspect, route, search, and audit a copied
item:

- when it was captured
- what workspace it currently belongs to
- whether it is text, image, file-list, rich text, or another payload kind
- bounded preview metadata
- redaction state
- tags and source metadata when available

The OS clipboard still behaves normally. `blipcoard` mirrors clipboard content
into its own local store so the user can manage it intentionally.

### Inbox

`inbox` is the default landing area for newly captured blips.

It is a human triage queue, not an agent-readable workspace. This default keeps
fresh clipboard content visible to the user while avoiding accidental agent
access to sensitive material.

### Workspaces

A workspace is a named context bucket for a task, issue, investigation, or
conversation. Examples:

- `auth-bug`
- `checkout-smoke-test`
- `design-review`
- `release-notes`

Users can route blips from `inbox` into a workspace manually, send the latest
blip with the CLI, or enable sticky routing when they want new copies to keep
flowing into the current task.

### Active Workspace

The active workspace is the workspace currently attached to the user's workflow.

CLI commands, desktop controls, and future agent tools should all agree on this
state. Agent-facing reads should default to the active workspace, not the raw
clipboard and not every workspace.

## Product Surfaces

`blipcoard` is one runtime system with multiple entry points:

- `blipd`: background daemon and owner of clipboard ingestion
- `blip`: terminal client for routing, listing, searching, and operations
- `blipcoard`: desktop UI for visual inspection, previews, and routing

The daemon is the source of truth. The CLI and desktop app are clients.

## Default Workflow

1. The user copies text, an image, a file list, or rich content.
2. `blipd` observes the clipboard event and writes a local blip.
3. New blips start in `inbox` unless sticky routing sends them elsewhere.
4. The user reviews the blip in the desktop app or CLI.
5. The user routes useful blips into a workspace.
6. Agents or scripts read only the workspace they are allowed to read.

This keeps ordinary clipboard usage fast while making agent context deliberate.

## What Blipcoard Is Not

`blipcoard` is not a cloud clipboard by default, a password manager, or a
replacement for the operating system clipboard. It is a local coordination layer
between the clipboard, user-controlled workspaces, and agent-safe retrieval.

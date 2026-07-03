# Phase Model

The phase model is a planning tool, not a product mode selector.

Each phase adds one layer of the runtime while preserving earlier boundaries.
When a later phase introduces a new surface, it should not bypass daemon
ownership, workspace policy, or auditability.

## Human Summary

### Foundation

The first layer makes the local store reliable: workspaces, blips, audit events,
migrations, typed errors, and CI. This gives the project a durable domain model
before clipboard watching enters the system.

### Clipboard Runtime

The daemon becomes the runtime owner. It watches the clipboard, ingests text into
`inbox`, dedupes repeated copies, and exposes a local API for clients.

### Client Workflow

The CLI and desktop app become daemon clients. They give users ways to list,
route, search, preview, and inspect blips without becoming separate clipboard
watchers.

### Agent Scope

Agent-readable commands and APIs are added behind explicit workspace policy.
The key behavior is negative by default: agents do not get the raw clipboard or
the inbox just because they are running on the same machine.

### Routing And Search

Fast routing, sticky workspace mode, redaction, search, and bundle commands make
the system practical for daily work across multiple tasks.

### Rich Payloads

The runtime starts handling screenshots, copied images, file lists, HTML, RTF,
and unsupported platform formats. This requires blob storage, safe previews,
metadata-first list views, and stricter access policy.

### Distribution And Documentation

The daemon, CLI, desktop bundle, install paths, service lifecycle, migration
rules, and public docs become stable enough for users outside the repository.

### Shared Workspaces

Hosted or self-hosted workspace sharing can be introduced after the local
runtime is trustworthy. Shared sessions should send only user-selected blips by
default, not automatically upload every clipboard event.

## Phase Invariants

- Keep `blipd` as the only clipboard watcher.
- Keep the CLI and desktop app as clients.
- Keep `inbox` human-only by default.
- Keep policy checks daemon-mediated.
- Keep rich payload bytes local unless the user explicitly exports or shares
  them.
- Keep migrations recoverable and auditable.
- Keep documentation aligned with the active phase.

The detailed implementation checklist lives in [MVP phases](./mvp-phases.md).

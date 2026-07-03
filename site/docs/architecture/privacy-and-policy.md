# Privacy And Workspace Policy

`blipcoard` assumes clipboard content is sensitive.

The default posture is local-first, explicit, and auditable. Captured blips stay
on the user's machine unless a future shared-workspace feature sends selected
items elsewhere by explicit action.

## Privacy Defaults

- New clipboard captures land in `inbox`.
- `inbox` is human-only by default.
- Agents cannot read the raw OS clipboard through `blipcoard`.
- Agents cannot read every workspace by default.
- Rich payload bytes are not dumped in list responses.
- Image previews and raw exports are explicit payload reads.
- Redacted blips do not reveal their payload in ordinary detail views.
- Audit records are written for routing and policy-sensitive access.

These defaults are meant to keep accidental disclosure uncommon even when a user
is moving quickly.

## Workspace Access

Workspaces describe both organization and access.

Common policy modes:

- human-only: visible to the user, not readable by agents
- agent-readable: readable by the current agent session or approved tooling
- locked: protected from accidental routing or reads
- sticky capture: new captures route to this workspace until disabled

The active workspace is a convenience pointer, not a blanket permission grant.
Agent tools still need a readable workspace policy before receiving content.

## Rich Payload Handling

Images, screenshots, file lists, HTML, RTF, and unknown platform formats need
stricter handling than plain text.

Safe behavior:

- Store searchable metadata separately from raw bytes.
- Generate bounded local previews for images.
- Treat copied file lists as references and metadata, not automatic file
  imports.
- Treat HTML and RTF as rich payloads with plain-text fallback, not trusted UI.
- Report unsupported or unavailable platform formats explicitly.
- Deny raw binary agent access unless workspace policy allows it.

The detailed reliability checklist lives in
[Storage and rich payload reliability](./storage-and-blobs.md).

## Redaction

Redaction protects likely-sensitive content from casual display.

When a blip is redacted, list and detail views should show metadata such as
content type, size, source, tags, and audit context without rendering the
payload. Redaction should not delete the audit trail that explains how the blip
entered the store or where it was routed.

## Local Store

The local store is part of the privacy boundary.

- SQLite holds structured metadata and text-like content.
- Blob storage holds binary payload bytes and previews.
- Backups need both SQLite and blob directories.
- Retention and deletion must account for both metadata and blobs.
- The daemon should avoid remote calls for classification or preview generation.

Hosted multiplayer workspaces use a separate
[hosted threat model](./hosted-threat-model.md). The local runtime should remain
useful without hosted infrastructure.

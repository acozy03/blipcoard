# ADR 0001: Typed Clipboard Payloads

## Status

Accepted

## Context

Early blipcoard storage treats every captured clipboard item as text:
`Blip.content` in Rust and `blips.content TEXT` in SQLite. That is enough for
plain text, code, logs, URLs, and search, but it does not model screenshots,
images, file lists, HTML, RTF, or platform-specific clipboard formats.

Phase 8 needs a richer payload model without breaking existing CLI, daemon, API,
agent bundle, and FTS behavior that already depends on text blips.

## Decision

Keep `blips.content` as the backward-compatible text projection for now and add
`blip_payloads` as the typed payload metadata table.

The core domain model uses `ClipboardPayload` with these persisted fields:

- `kind`: `text`, `image`, `file_list`, `html`, `rtf`, or `unknown`
- `mime_type`
- `platform_format`
- `byte_size`
- `content_hash`
- `source_app`
- `captured_at`
- `preview_ref`
- `blob_ref`
- `inline_text`
- `metadata`
- `created_at`

`ContentType` remains the text classifier used by list views, tags, and search.
`PayloadKind` describes the clipboard/media payload shape. The two concepts are
intentionally separate.

Text, HTML, and RTF payloads may store inline text in SQLite. Binary payloads
such as images and file-list payloads must not store large base64 data in text
columns; they will store metadata plus a local `blob_ref` once blob storage is
implemented. List views can query payload kind, MIME type, byte size, preview
reference, and blob reference without loading payload bytes.

## Migration

Schema version 5 adds `blip_payloads` and backfills one text payload for every
existing blip:

- `payload_kind = text`
- `mime_type = text/plain`
- `byte_size = blips.size_bytes`
- `source_app = blips.source_app`
- `captured_at = blips.created_at`
- `inline_text = blips.content`
- `metadata_json = {}`

`blips.content` remains populated for existing and new text records, preserving
old readers and FTS over text content. New text inserts write both the legacy
`blips` row and its `blip_payloads` row in the same transaction.

## API Boundaries

`blip-core` owns payload persistence, enum validation, metadata JSON validation,
and migration behavior.

`blip-clipboard` should eventually emit typed clipboard events with platform
format, MIME type, byte size, content hash, source app, capture timestamp, and
either inline text or a blob reference.

`blip-api`, `blipd`, `blip`, and the desktop app should keep existing text
response fields for compatibility and add typed payload fields additively when
rich payload capture lands. Older clients should continue to read text-only
records without requiring typed payload awareness.

## Consequences

This keeps Phase 8 incremental. Search, summaries, agent bundles, and current
CLI output continue to use the text projection. Rich payload support can be
introduced by adding blob storage and typed capture without replacing the text
APIs in the same change.

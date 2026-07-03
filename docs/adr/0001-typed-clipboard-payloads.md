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

## Blob Storage Semantics

Blob bytes live outside SQLite under a store-owned blob directory. The directory
is content-addressed by payload hash, using a stable fan-out layout such as
`blobs/sha256/ab/<full-hash>`, and `blob_ref` stores only the relative
reference needed to find the file under the current store root. SQLite remains
the source of truth for blip and payload metadata; the blob directory is part of
the same local data store, not a cache.

Backups must include both the SQLite database and the blob directory from the
same store snapshot. Restores must put them back together under one store root;
restoring only SQLite may leave payload rows whose blobs are missing, and
restoring only blobs produces orphan files that are not visible to clients.

Blob writes use an atomic temp-file flow: write bytes to a temporary file in the
blob directory, fsync the file, atomically rename it into the hash-addressed
path, then commit the SQLite row that references it. If the final blob path
already exists with the expected size and hash, the writer reuses it. Startup or
maintenance recovery may delete stale temp files and may either remove orphan
final blobs or leave them for a later garbage-collection pass; it must not
invent payload rows for orphan files.

`content_hash` enables dedupe across payload rows. Multiple blips may reference
the same blob path when their bytes are identical. Deleting a blip or payload
removes the SQLite reference first; physical blob deletion happens only after no
remaining payload references the same hash. Missing blobs should be surfaced as
payload integrity errors rather than silently downgraded to text-only records.

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

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

## Platform Image Clipboard Behavior

Phase 8.3 image capture normalizes platform clipboard data into typed image
payload metadata before storage. For every captured image, the daemon-facing
event should include MIME type, width, height, byte size, original platform
format, content hash, capture timestamp, and either a blob reference or the
bytes needed to create one. Unsupported formats must remain typed failures, not
silent text fallbacks, so callers can distinguish "no image", "unsupported
format", "clipboard unavailable", and "headless or permission-limited runtime".

Platform expectations:

- macOS readers should inspect pasteboard image types, preferring encodable PNG
  or TIFF image data when available and preserving the source pasteboard type in
  `platform_format`.
- Linux readers should report capabilities separately for X11 and Wayland.
  X11 can usually read advertised image targets such as `image/png`; Wayland may
  depend on portal/compositor support and can be unavailable to background or
  headless processes. CI/headless sessions should return typed unavailable or
  unsupported errors rather than pretending image capture succeeded.
- Windows readers should handle bitmap clipboard data, especially DIB/bitmap
  sources from screenshot and image-copy flows, and prefer PNG when an explicit
  PNG clipboard format is present. `platform_format` should preserve whether the
  source was PNG, DIB, bitmap, or another registered format.

Normalized image metadata should use stable field names for `mime_type`,
`width`, `height`, `byte_size`, and `platform_format`. Width and height describe
the decoded pixel dimensions. Byte size describes the stored or to-be-stored
encoded bytes, not an estimated in-memory bitmap size.

## File Lists, Rich Text, and Unknown Formats

Phase 8.4 keeps file-list and rich-text capture conservative by default.

File-list payloads represent copied file references, not automatic file imports.
The stored payload should include path or display metadata, the original platform
format, source app, capture timestamp, and byte-size information when the
clipboard runtime provides it. The daemon must not eagerly copy the referenced
file bytes into blob storage just because a file manager placed file references
on the clipboard. Reading, exporting, opening, or importing those files is a
separate explicit user action and must be audited.

Path metadata must avoid silent lossy conversion. When the platform exposes
paths, store a display string only as a convenience field and also keep a
platform-native representation, such as Unix path bytes or Windows UTF-16 code
units, so later audited open/import actions can reason about the original
reference.

HTML and RTF payloads should remain typed as `html` or `rtf` while also exposing
a bounded plain-text fallback. The fallback is the value used for legacy
`blips.content`, search, summaries, and default agent bundle text. Markup bytes
or rich formatting metadata may be stored as inline text or blob-backed payload
data according to size and safety limits, but clients must not treat captured
HTML or RTF as trusted UI.

Unknown platform formats are first-class audit records. When a clipboard reader
can observe a format but cannot decode it, the payload kind should be `unknown`
with available format identifiers, advertised MIME type or target names, byte
size, source app, capture time, and other structured metadata. Unknown payloads
must not be silently downgraded to plain text or hidden as empty clipboard data.

## Privacy and Policy Controls

Rich payload capture is controlled in two layers. The global `[capture]` config
can disable all capture or individual payload kinds before the platform watcher
reads them. The daemon repeats the same policy check before persistence so tests
and future ingestion sources cannot bypass config. Workspace policy can narrow
that further for the destination workspace by disabling rich capture, disabling
image capture, hiding rich payload summaries, or explicitly allowing raw agent
payload access.

Raw agent payload access defaults to denied and is separate from text
`agent_access`. List and detail views only return safe summaries; future preview,
export, desktop-open, and agent-payload reads must use the dedicated audit event
types added with the workspace policy migration.

Image, HTML, RTF, and unknown payload metadata now carries a no-op redaction hook
marker. This records that no OCR or screenshot processing happened in this
phase, while leaving a stable place for future local processors to record applied
redaction without changing the storage model again.

The portable `arboard` backend used for this phase exposes file-list and HTML
reads, but not RTF reads or generic clipboard format enumeration. The shared
storage and daemon paths support `rtf` and `unknown` so platform-specific readers
can add those observations later without another schema change.

Rendering and import rules are intentionally restrictive. Preview generation
must avoid executing untrusted HTML, loading remote resources, following file
references, or invoking privileged platform renderers without an explicit
user-controlled path. Agent access should receive metadata and plain-text
fallbacks by default; raw rich markup, file contents, and unsafe or unknown
payload bytes require policy-approved retrieval by id.

Phase 8.5 exposes safe inspection summaries through the daemon API. `BlipSummary`
and `BlipDetail` may include bounded `payloads` entries with payload kind, MIME
type, platform format, byte size, preview state, safe preview text, opaque
`preview_ref`, and a small metadata summary. They must not include raw bytes,
absolute blob paths, full metadata JSON, or `blob_ref`.

Preview states are explicit so clients do not infer integrity from legacy
`Blip.content` alone:

- `available`: a safe thumbnail or text preview exists.
- `text_fallback`: HTML or RTF is shown only as bounded plain text.
- `metadata_only`: file lists and similar references expose metadata but no raw
  file content.
- `redacted`: metadata and preview text are intentionally withheld.
- `missing_blob`: SQLite metadata exists but the local backing blob is missing.
- `unsupported`: the platform format was observed but not decoded.
- `unavailable`: no safe preview could be generated.

Image captures may store a bounded local PNG thumbnail as a `preview_ref`
separate from the full payload `blob_ref`. Garbage collection and delete logic
must treat both references as live blob references. List views may use payload
metadata and preview states, but they must not load raw payload bytes by default.

Phase 8.7 adds explicit daemon-mediated payload byte retrieval. Clients request
metadata, preview bytes, or raw bytes by payload id; list and detail responses
remain summary-only. Preview reads, raw exports, and agent raw reads all pass
through the same workspace policy checks and emit dedicated payload access audit
events. Raw exports require the explicit raw-payload workspace grant; the daemon
does not treat client-supplied requester labels as authorization. The CLI writes
exported bytes only to an explicitly requested path and does not overwrite an
existing file unless the user passes `--force`.

## Consequences

This keeps Phase 8 incremental. Search, summaries, agent bundles, and current
CLI output continue to use the text projection, with additive payload summaries
for richer inspection. Rich payload support can be introduced by adding blob
storage and typed capture without replacing the text APIs in the same change.

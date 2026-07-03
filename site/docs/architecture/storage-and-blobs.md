# Rich Payload Reliability Manual Checks

This checklist verifies rich clipboard behavior that depends on live operating
system clipboard integrations and cannot be fully covered by headless CI.

## Scope and Invariants

Run these checks for screenshots, copied images, file lists, HTML, and
unsupported clipboard formats. Across all platforms:

- list and detail views expose bounded payload summaries, never raw bytes,
  blob paths, or executable rich markup
- preview and raw export actions use explicit payload ids
- raw exports require the raw-payload workspace grant
- file-list payloads record references and metadata, not imported file contents
- HTML and RTF are treated as rich text grouping members with plain-text
  fallback, not trusted desktop UI
- unsupported or unavailable platform data is reported as typed
  unsupported/unavailable behavior instead of silent text fallback
- preview reads, raw exports, missing blobs, and denied accesses leave payload
  access audit records when the target payload exists

## Common Setup

1. Start from a clean test database and blob directory.
2. Start `blipd` with clipboard capture enabled.
3. Create a dedicated workspace for manual checks.
4. Confirm `blip workspaces`, `blip list <workspace> --output json`, and
   `blip payload inspect <payload-id> --output json` work through the daemon.
5. For raw export checks, explicitly enable raw payload access for the test
   workspace and disable it again when testing denials.

For each captured payload, record payload kind, MIME type, platform format, byte
size, preview state, preview text, and audit events.

## Shared Assertions

- screenshots and copied images produce `image` payloads with width, height,
  MIME type, byte size, content hash, and a blob reference
- generated thumbnails are bounded PNG preview blobs separate from raw payload
  blobs
- duplicate image bytes share one content-addressed blob while distinct copy
  events may still have separate blip rows when the platform exposes them
- deleting or retaining blips removes only unreferenced raw and preview blobs
- restoring SQLite without blobs produces `missing_blob` inspection state
- restoring blobs without SQLite leaves orphans that garbage collection can
  remove

## macOS Checklist

- Screenshot to clipboard: verify image payload metadata, PNG preview, and raw
  export after policy grant.
- Copied browser/editor image: verify platform format preserves pasteboard image
  source details when available.
- Finder copied file list: verify metadata-only file references and no file byte
  import.
- Browser HTML copy: verify HTML payload with plain-text fallback and no trusted
  render in list/detail views.
- Unsupported/custom payload: verify typed unsupported or unavailable behavior.

## Linux X11 Checklist

- Screenshot to clipboard: verify advertised image target, normalized PNG
  payload, preview, and raw export.
- Copied image: verify image targets such as `image/png` when available.
- File manager copied file list: verify `text/uri-list` style metadata.
- Browser HTML copy: verify `text/html` capture with plain-text fallback.
- Unsupported target: verify typed unsupported/unavailable behavior.

## Linux Wayland Checklist

- Screenshot to clipboard: separate portal/compositor permission failures from
  decoder failures in notes.
- Copied image: verify whether the compositor exposes readable image content to
  the daemon process.
- File manager copied file list: verify metadata-only references when exposed.
- Browser HTML copy: verify plain-text fallback and HTML metadata.
- Headless or denied session: verify unavailable behavior rather than fake
  success.

## Windows Checklist

- Screenshot or Snipping Tool copy: verify bitmap/DIB or PNG source format,
  normalized payload metadata, preview, and raw export.
- Copied PNG/image from browser or editor: verify image metadata and preview.
- Explorer copied files: verify `CF_HDROP` style file-list metadata and no file
  import.
- Browser HTML copy: verify `HTML Format` metadata and plain-text fallback.
- Registered/custom format: verify typed unsupported or unavailable behavior.

## Migration and Rollback Checklist

- Migration to the typed payload schema backfills legacy text blips with text
  payload rows.
- Workspace policy and payload access audit migrations preserve existing
  workspaces and audit history.
- Backups include both SQLite and the blob directory.
- Restore tests cover SQLite-only, blob-only, and complete backup cases.
- Partial temp files are recovered without deleting recent active writes.
- Failed payload metadata inserts leave unreferenced blobs for garbage
  collection rather than deleting shared dedupe blobs.
- Downgrades to pre-rich-payload binaries are not supported for active rich
  stores; use a full SQLite plus blob backup before testing older binaries.

## Evidence Template

For each platform and source application:

- platform and session type
- source application and action
- expected payload kind and preview state
- observed MIME type and platform format
- CLI/API command used
- audit event observed
- result and notes

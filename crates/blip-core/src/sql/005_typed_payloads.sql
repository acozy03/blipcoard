CREATE TABLE IF NOT EXISTS blip_payloads (
  id TEXT PRIMARY KEY,
  blip_id TEXT NOT NULL,
  payload_kind TEXT NOT NULL CHECK (
    payload_kind IN ('text', 'image', 'file_list', 'html', 'rtf', 'unknown')
  ),
  mime_type TEXT,
  platform_format TEXT,
  byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
  content_hash TEXT,
  source_app TEXT,
  captured_at TEXT NOT NULL,
  preview_ref TEXT,
  blob_ref TEXT,
  inline_text TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  CHECK (payload_kind IN ('text', 'html', 'rtf') OR inline_text IS NULL),
  FOREIGN KEY (blip_id) REFERENCES blips(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_blip_payloads_blip_id
ON blip_payloads (blip_id);

CREATE INDEX IF NOT EXISTS idx_blip_payloads_kind_created_at
ON blip_payloads (payload_kind, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_blip_payloads_blob_ref
ON blip_payloads (blob_ref)
WHERE blob_ref IS NOT NULL;

INSERT OR IGNORE INTO blip_payloads (
  id,
  blip_id,
  payload_kind,
  mime_type,
  platform_format,
  byte_size,
  content_hash,
  source_app,
  captured_at,
  preview_ref,
  blob_ref,
  inline_text,
  metadata_json,
  created_at
)
SELECT
  blips.id || ':payload:text',
  blips.id,
  'text',
  'text/plain',
  NULL,
  blips.size_bytes,
  NULL,
  blips.source_app,
  blips.created_at,
  NULL,
  NULL,
  blips.content,
  '{}',
  blips.created_at
FROM blips;

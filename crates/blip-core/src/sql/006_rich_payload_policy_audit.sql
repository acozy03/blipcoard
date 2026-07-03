CREATE TABLE IF NOT EXISTS workspace_policies (
  workspace_name TEXT PRIMARY KEY,
  rich_capture_enabled INTEGER NOT NULL DEFAULT 1 CHECK (rich_capture_enabled IN (0, 1)),
  image_capture_enabled INTEGER NOT NULL DEFAULT 1 CHECK (image_capture_enabled IN (0, 1)),
  rich_payload_visibility TEXT NOT NULL DEFAULT 'safe_preview' CHECK (
    rich_payload_visibility IN ('hidden', 'metadata', 'safe_preview')
  ),
  agent_raw_payload_access INTEGER NOT NULL DEFAULT 0 CHECK (agent_raw_payload_access IN (0, 1)),
  updated_at TEXT NOT NULL,
  FOREIGN KEY (workspace_name) REFERENCES workspaces(name) ON DELETE CASCADE
);

INSERT OR IGNORE INTO workspace_policies (
  workspace_name,
  rich_capture_enabled,
  image_capture_enabled,
  rich_payload_visibility,
  agent_raw_payload_access,
  updated_at
)
SELECT
  name,
  1,
  1,
  'safe_preview',
  0,
  created_at
FROM workspaces;

DROP INDEX IF EXISTS idx_audit_events_created_at;

ALTER TABLE audit_events RENAME TO audit_events_old;

CREATE TABLE audit_events (
  id TEXT PRIMARY KEY,
  actor_type TEXT NOT NULL CHECK (actor_type IN ('system', 'user', 'agent')),
  actor_id TEXT,
  event_type TEXT NOT NULL CHECK (
    event_type IN (
      'schema_initialized',
      'workspace_created',
      'workspace_activated',
      'sticky_capture_changed',
      'workspace_policy_changed',
      'blip_ingested',
      'blip_moved',
      'blips_read',
      'rich_payload_capture_skipped',
      'payload_preview_read',
      'payload_raw_exported',
      'desktop_payload_opened',
      'agent_payload_read'
    )
  ),
  target_blip_id TEXT,
  target_workspace TEXT,
  details_json TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (target_blip_id) REFERENCES blips(id) ON DELETE SET NULL,
  FOREIGN KEY (target_workspace) REFERENCES workspaces(name) ON DELETE SET NULL
);

INSERT INTO audit_events (
  id,
  actor_type,
  actor_id,
  event_type,
  target_blip_id,
  target_workspace,
  details_json,
  created_at
)
SELECT
  id,
  actor_type,
  actor_id,
  event_type,
  target_blip_id,
  target_workspace,
  details_json,
  created_at
FROM audit_events_old;

DROP TABLE audit_events_old;

CREATE INDEX idx_audit_events_created_at
ON audit_events (created_at DESC);

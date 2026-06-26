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
      'blip_ingested',
      'blip_moved',
      'blips_read'
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

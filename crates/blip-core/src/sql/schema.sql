PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS workspaces (
  name TEXT PRIMARY KEY,
  description TEXT,
  color TEXT,
  agent_access INTEGER NOT NULL DEFAULT 0,
  sticky_capture INTEGER NOT NULL DEFAULT 0,
  retention_days INTEGER,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS blips (
  id TEXT PRIMARY KEY,
  workspace_name TEXT NOT NULL,
  source_app TEXT,
  content_type TEXT NOT NULL,
  language TEXT,
  content TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  token_estimate INTEGER,
  is_redacted INTEGER NOT NULL DEFAULT 0,
  tags_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  FOREIGN KEY (workspace_name) REFERENCES workspaces(name) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_blips_workspace_name_created_at
ON blips (workspace_name, created_at DESC);

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  actor_type TEXT NOT NULL,
  actor_id TEXT,
  event_type TEXT NOT NULL,
  target_blip_id TEXT,
  target_workspace TEXT,
  details_json TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (target_blip_id) REFERENCES blips(id) ON DELETE SET NULL,
  FOREIGN KEY (target_workspace) REFERENCES workspaces(name) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_events_created_at
ON audit_events (created_at DESC);

CREATE TABLE IF NOT EXISTS app_state (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

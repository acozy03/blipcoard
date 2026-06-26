CREATE VIRTUAL TABLE IF NOT EXISTS blips_fts USING fts5(
  content,
  content='blips',
  content_rowid='rowid'
);

INSERT INTO blips_fts(blips_fts) VALUES ('rebuild');

CREATE TRIGGER IF NOT EXISTS blips_fts_after_insert
AFTER INSERT ON blips
BEGIN
  INSERT INTO blips_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS blips_fts_after_delete
AFTER DELETE ON blips
BEGIN
  INSERT INTO blips_fts(blips_fts, rowid, content)
  VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS blips_fts_after_content_update
AFTER UPDATE OF content ON blips
BEGIN
  INSERT INTO blips_fts(blips_fts, rowid, content)
  VALUES ('delete', old.rowid, old.content);
  INSERT INTO blips_fts(rowid, content) VALUES (new.rowid, new.content);
END;

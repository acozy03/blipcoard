use crate::LocalBlobStore;
use crate::domain::{
    ActorType, AuditEvent, AuditEventType, Blip, BlipMove, BlipSummary, ClipboardPayload,
    ContentType, NewBlip, NewClipboardMetadataPayload, NewClipboardPayload, NewWorkspace,
    PayloadKind, Workspace,
};
use crate::error::{BlipError, is_sqlite_busy_error};
use crate::secrets::add_secret_tags;
use crate::typing::{add_type_tag, resolved_content_type};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};
use std::collections::HashSet;
use std::path::Path;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

const INBOX_WORKSPACE: &str = "inbox";
const DEFAULT_LIST_LIMIT: usize = 100;
const MAX_LIST_LIMIT: usize = 500;
const DEFAULT_BLIP_PREVIEW_CHARS: i64 = 72;
const BUSY_RETRY_ATTEMPTS: usize = 5;
const BUSY_RETRY_DELAY: Duration = Duration::from_millis(25);
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);
const SCHEMA_VERSION: i32 = 5;

struct Migration {
    version: i32,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("sql/001_initial.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("sql/002_audit_read_events.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("sql/003_sticky_capture_audit_event.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("sql/004_blips_fts.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("sql/005_typed_payloads.sql"),
    },
];

pub struct BlipStore {
    conn: Connection,
}

impl BlipStore {
    pub fn in_memory() -> Result<Self, BlipError> {
        let conn = Connection::open_in_memory()?;
        Self::configure_connection(&conn)?;
        let mut store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, BlipError> {
        let conn = Connection::open(path)?;
        Self::configure_connection(&conn)?;
        let mut store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn configure_connection(conn: &Connection) -> Result<(), BlipError> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        Ok(())
    }

    fn initialize(&mut self) -> Result<(), BlipError> {
        self.with_busy_retry(Self::initialize_once)
    }

    fn initialize_once(&mut self) -> Result<(), BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::apply_migrations(&tx)?;

        Self::insert_workspace(
            &tx,
            &NewWorkspace {
                name: INBOX_WORKSPACE.to_owned(),
                description: Some("Default capture workspace".to_owned()),
                color: Some("#777777".to_owned()),
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            },
            true,
        )?;

        if Self::get_active_workspace_from(&tx)?.is_none() {
            Self::ensure_active_workspace(&tx, INBOX_WORKSPACE)?;
        }

        let schema_initialized = tx.execute(
            "INSERT OR IGNORE INTO app_state(key, value) VALUES('schema_version', ?1)",
            [SCHEMA_VERSION.to_string()],
        )?;

        if schema_initialized == 0 {
            tx.execute(
                "UPDATE app_state SET value = ?1 WHERE key = 'schema_version'",
                [SCHEMA_VERSION.to_string()],
            )?;
        }

        if schema_initialized > 0 {
            Self::insert_audit_event(
                &tx,
                ActorType::System,
                None,
                AuditEventType::SchemaInitialized,
                None,
                Some(INBOX_WORKSPACE.to_owned()),
                Some(format!("{{\"version\":{SCHEMA_VERSION}}}")),
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn apply_migrations(conn: &Connection) -> Result<(), BlipError> {
        let current_version =
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))?;

        for migration in MIGRATIONS
            .iter()
            .filter(|migration| migration.version > current_version)
        {
            conn.execute_batch(migration.sql)?;
            conn.pragma_update(None, "user_version", migration.version)?;
        }

        Ok(())
    }

    fn with_busy_retry<T>(
        &mut self,
        mut operation: impl FnMut(&mut Self) -> Result<T, BlipError>,
    ) -> Result<T, BlipError> {
        for attempt in 0..BUSY_RETRY_ATTEMPTS {
            match operation(self) {
                Err(BlipError::DatabaseBusy) if attempt + 1 < BUSY_RETRY_ATTEMPTS => {
                    thread::sleep(BUSY_RETRY_DELAY);
                }
                result => return result,
            }
        }

        Err(BlipError::DatabaseBusy)
    }

    pub fn create_workspace(&mut self, workspace: &NewWorkspace) -> Result<Workspace, BlipError> {
        validate_workspace(workspace)?;
        self.with_busy_retry(|store| store.create_workspace_once(workspace))
    }

    fn create_workspace_once(&mut self, workspace: &NewWorkspace) -> Result<Workspace, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_workspace_from(&tx, &workspace.name)?.is_some() {
            return Err(BlipError::WorkspaceAlreadyExists(workspace.name.clone()));
        }

        Self::insert_workspace(&tx, workspace, false)?;
        let created = Self::get_workspace_from(&tx, &workspace.name)?
            .ok_or_else(|| BlipError::WorkspaceNotFound(workspace.name.clone()))?;

        tx.commit()?;
        Ok(created)
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>, BlipError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, description, color, agent_access, sticky_capture, retention_days, created_at
             FROM workspaces ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Workspace {
                name: row.get(0)?,
                description: row.get(1)?,
                color: row.get(2)?,
                agent_access: row.get(3)?,
                sticky_capture: row.get(4)?,
                retention_days: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(BlipError::from)
    }

    pub fn get_workspace(&self, name: &str) -> Result<Option<Workspace>, BlipError> {
        Self::get_workspace_from(&self.conn, name)
    }

    fn get_workspace_from(conn: &Connection, name: &str) -> Result<Option<Workspace>, BlipError> {
        conn
            .query_row(
                "SELECT name, description, color, agent_access, sticky_capture, retention_days, created_at
                 FROM workspaces WHERE name = ?1",
                [name],
                |row| {
                    Ok(Workspace {
                        name: row.get(0)?,
                        description: row.get(1)?,
                        color: row.get(2)?,
                        agent_access: row.get(3)?,
                        sticky_capture: row.get(4)?,
                        retention_days: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(BlipError::from)
    }

    pub fn set_active_workspace(&mut self, name: &str) -> Result<(), BlipError> {
        self.with_busy_retry(|store| store.set_active_workspace_once(name))
    }

    fn set_active_workspace_once(&mut self, name: &str) -> Result<(), BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_workspace_from(&tx, name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(name.to_owned()));
        }

        Self::ensure_active_workspace(&tx, name)?;
        Self::insert_audit_event(
            &tx,
            ActorType::User,
            None,
            AuditEventType::WorkspaceActivated,
            None,
            Some(name.to_owned()),
            None,
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn get_active_workspace(&self) -> Result<Option<String>, BlipError> {
        Self::get_active_workspace_from(&self.conn)
    }

    pub fn get_sticky_workspace(&self) -> Result<Option<String>, BlipError> {
        self.conn
            .query_row(
                "SELECT name FROM workspaces WHERE sticky_capture = 1 ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(BlipError::from)
    }

    pub fn set_sticky_capture(
        &mut self,
        name: &str,
        enabled: bool,
    ) -> Result<Workspace, BlipError> {
        self.with_busy_retry(|store| store.set_sticky_capture_once(name, enabled))
    }

    fn set_sticky_capture_once(
        &mut self,
        name: &str,
        enabled: bool,
    ) -> Result<Workspace, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_workspace_from(&tx, name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(name.to_owned()));
        }

        if enabled {
            tx.execute(
                "UPDATE workspaces SET sticky_capture = CASE WHEN name = ?1 THEN 1 ELSE 0 END",
                [name],
            )?;
        } else {
            tx.execute(
                "UPDATE workspaces SET sticky_capture = 0 WHERE name = ?1",
                [name],
            )?;
        }

        Self::insert_audit_event(
            &tx,
            ActorType::User,
            None,
            AuditEventType::StickyCaptureChanged,
            None,
            Some(name.to_owned()),
            Some(format!("{{\"enabled\":{enabled}}}")),
        )?;

        let workspace = Self::get_workspace_from(&tx, name)?
            .ok_or_else(|| BlipError::WorkspaceNotFound(name.to_owned()))?;
        tx.commit()?;
        Ok(workspace)
    }

    fn get_active_workspace_from(conn: &Connection) -> Result<Option<String>, BlipError> {
        conn.query_row(
            "SELECT value FROM app_state WHERE key = 'active_workspace'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(BlipError::from)
    }

    pub fn insert_blip(&mut self, new_blip: &NewBlip) -> Result<Blip, BlipError> {
        validate_blip(new_blip)?;
        self.with_busy_retry(|store| store.insert_blip_once(new_blip))
    }

    fn insert_blip_once(&mut self, new_blip: &NewBlip) -> Result<Blip, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_workspace_from(&tx, &new_blip.workspace_name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(
                new_blip.workspace_name.clone(),
            ));
        }

        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        let content_type = resolved_content_type(new_blip.content_type, &new_blip.content);
        let tags = add_secret_tags(
            &new_blip.content,
            &add_type_tag(content_type, &new_blip.tags),
        );
        let tags_json = serde_json::to_string(&tags)?;
        let size_bytes = new_blip.content.len() as i64;

        tx.execute(
            "INSERT INTO blips (
                id, workspace_name, source_app, content_type, language, content, size_bytes,
                token_estimate, is_redacted, tags_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                new_blip.workspace_name,
                new_blip.source_app,
                content_type.as_str(),
                new_blip.language,
                new_blip.content,
                size_bytes,
                new_blip.token_estimate,
                new_blip.is_redacted,
                tags_json,
                created_at,
            ],
        )?;

        Self::insert_text_payload(&tx, &id, new_blip, size_bytes, created_at)?;

        Self::insert_audit_event(
            &tx,
            ActorType::System,
            None,
            AuditEventType::BlipIngested,
            Some(id.clone()),
            Some(new_blip.workspace_name.clone()),
            None,
        )?;

        let blip = Self::get_blip_from(&tx, &id)?
            .ok_or_else(|| BlipError::Database(rusqlite::Error::QueryReturnedNoRows))?;

        tx.commit()?;
        Ok(blip)
    }

    pub fn get_blip(&self, id: &str) -> Result<Option<Blip>, BlipError> {
        Self::get_blip_from(&self.conn, id)
    }

    pub fn get_blip_payloads(&self, blip_id: &str) -> Result<Vec<ClipboardPayload>, BlipError> {
        Self::get_blip_payloads_from(&self.conn, blip_id)
    }

    pub fn insert_metadata_payload(
        &mut self,
        blip_id: &str,
        payload: &NewClipboardMetadataPayload,
    ) -> Result<ClipboardPayload, BlipError> {
        validate_metadata_payload(payload)?;
        self.with_busy_retry(|store| store.insert_metadata_payload_once(blip_id, payload))
    }

    fn insert_metadata_payload_once(
        &mut self,
        blip_id: &str,
        payload: &NewClipboardMetadataPayload,
    ) -> Result<ClipboardPayload, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_blip_from(&tx, blip_id)?.is_none() {
            return Err(BlipError::BlipNotFound(blip_id.to_owned()));
        }

        let id = format!("{blip_id}:payload:{}", Uuid::new_v4());
        let created_at = Utc::now();
        let metadata_json = serde_json::to_string(&payload.metadata)?;
        tx.execute(
            "INSERT INTO blip_payloads (
                id, blip_id, payload_kind, mime_type, platform_format, byte_size, content_hash,
                source_app, captured_at, preview_ref, blob_ref, inline_text, metadata_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, NULL, ?10, ?11, ?8)",
            params![
                id,
                blip_id,
                payload.kind.as_str(),
                &payload.mime_type,
                &payload.platform_format,
                payload.byte_size,
                &payload.source_app,
                created_at,
                &payload.preview_ref,
                &payload.inline_text,
                metadata_json,
            ],
        )?;

        let inserted = Self::get_payload_from(&tx, &id)?
            .ok_or_else(|| BlipError::Database(rusqlite::Error::QueryReturnedNoRows))?;
        tx.commit()?;
        Ok(inserted)
    }

    pub fn insert_blob_payload(
        &mut self,
        blip_id: &str,
        payload: &NewClipboardPayload,
        blob_store: &LocalBlobStore,
    ) -> Result<ClipboardPayload, BlipError> {
        validate_blob_payload(payload)?;

        blob_store.with_lock(|| {
            let metadata = blob_store.write_unlocked(&payload.bytes)?;
            self.insert_blob_payload_metadata(blip_id, payload, &metadata)
        })
    }

    fn insert_blob_payload_metadata(
        &mut self,
        blip_id: &str,
        payload: &NewClipboardPayload,
        metadata: &crate::BlobMetadata,
    ) -> Result<ClipboardPayload, BlipError> {
        self.with_busy_retry(|store| {
            store.insert_blob_payload_metadata_once(blip_id, payload, metadata)
        })
    }

    fn insert_blob_payload_metadata_once(
        &mut self,
        blip_id: &str,
        payload: &NewClipboardPayload,
        metadata: &crate::BlobMetadata,
    ) -> Result<ClipboardPayload, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_blip_from(&tx, blip_id)?.is_none() {
            return Err(BlipError::BlipNotFound(blip_id.to_owned()));
        }

        let id = format!("{blip_id}:payload:{}", Uuid::new_v4());
        let created_at = Utc::now();
        let metadata_json = serde_json::to_string(&payload.metadata)?;
        tx.execute(
            "INSERT INTO blip_payloads (
                id, blip_id, payload_kind, mime_type, platform_format, byte_size, content_hash,
                source_app, captured_at, preview_ref, blob_ref, inline_text, metadata_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?9)",
            params![
                id,
                blip_id,
                payload.kind.as_str(),
                &payload.mime_type,
                &payload.platform_format,
                i64::try_from(metadata.byte_size).map_err(|_| BlipError::InvalidInput {
                    field: "byte_size",
                    reason: "payload is too large for SQLite metadata",
                })?,
                &metadata.content_hash,
                &payload.source_app,
                created_at,
                &payload.preview_ref,
                &metadata.blob_ref,
                &payload.inline_text,
                metadata_json,
            ],
        )?;

        let inserted = Self::get_payload_from(&tx, &id)?
            .ok_or_else(|| BlipError::Database(rusqlite::Error::QueryReturnedNoRows))?;
        tx.commit()?;
        Ok(inserted)
    }

    pub fn referenced_blob_refs(&self) -> Result<HashSet<String>, BlipError> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT blob_ref
             FROM blip_payloads
             WHERE blob_ref IS NOT NULL
             ORDER BY blob_ref ASC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<HashSet<_>, _>>()
            .map_err(BlipError::from)
    }

    pub fn delete_blip_and_collect_orphans(
        &mut self,
        blob_store: &LocalBlobStore,
        blip_id: &str,
    ) -> Result<bool, BlipError> {
        blob_store.with_lock(|| {
            let deleted_blob_refs = self.delete_blip_and_return_blob_refs(blip_id)?;
            if deleted_blob_refs.is_empty() {
                return Ok(false);
            }

            for blob_ref in deleted_blob_refs {
                if !self.is_blob_ref_referenced(&blob_ref)? {
                    blob_store.delete_unlocked(&blob_ref)?;
                }
            }

            Ok(true)
        })
    }

    pub fn garbage_collect_blobs(
        &self,
        blob_store: &LocalBlobStore,
    ) -> Result<crate::BlobGcReport, BlipError> {
        blob_store.with_lock(|| {
            let referenced_blob_refs = self.referenced_blob_refs()?;
            blob_store.garbage_collect_unlocked(&referenced_blob_refs)
        })
    }

    fn delete_blip_and_return_blob_refs(
        &mut self,
        blip_id: &str,
    ) -> Result<HashSet<String>, BlipError> {
        self.with_busy_retry(|store| store.delete_blip_and_return_blob_refs_once(blip_id))
    }

    fn delete_blip_and_return_blob_refs_once(
        &mut self,
        blip_id: &str,
    ) -> Result<HashSet<String>, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_blip_from(&tx, blip_id)?.is_none() {
            return Err(BlipError::BlipNotFound(blip_id.to_owned()));
        }

        let blob_refs = Self::referenced_blob_refs_for_blip_from(&tx, blip_id)?;
        tx.execute("DELETE FROM blips WHERE id = ?1", [blip_id])?;
        tx.commit()?;
        Ok(blob_refs)
    }

    fn is_blob_ref_referenced(&self, blob_ref: &str) -> Result<bool, BlipError> {
        self.conn
            .query_row(
                "SELECT 1 FROM blip_payloads WHERE blob_ref = ?1 LIMIT 1",
                [blob_ref],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
            .map_err(BlipError::from)
    }

    fn get_payload_from(
        conn: &Connection,
        payload_id: &str,
    ) -> Result<Option<ClipboardPayload>, BlipError> {
        let mut stmt = conn.prepare(
            "SELECT id, blip_id, payload_kind, mime_type, platform_format, byte_size,
                    content_hash, source_app, captured_at, preview_ref, blob_ref, inline_text,
                    metadata_json, created_at
             FROM blip_payloads
             WHERE id = ?1",
        )?;
        let mut rows = stmt.query([payload_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(map_payload_row(row)?))
    }

    fn referenced_blob_refs_for_blip_from(
        conn: &Connection,
        blip_id: &str,
    ) -> Result<HashSet<String>, BlipError> {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT blob_ref
             FROM blip_payloads
             WHERE blip_id = ?1 AND blob_ref IS NOT NULL",
        )?;
        let rows = stmt.query_map([blip_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<HashSet<_>, _>>()
            .map_err(BlipError::from)
    }

    fn get_blip_from(conn: &Connection, id: &str) -> Result<Option<Blip>, BlipError> {
        let mut stmt = conn.prepare(
            "SELECT id, workspace_name, source_app, content_type, language, content, size_bytes,
                    token_estimate, is_redacted, tags_json, created_at
             FROM blips WHERE id = ?1",
        )?;

        let mut rows = stmt.query([id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        Ok(Some(map_blip_row(row)?))
    }

    fn get_blip_payloads_from(
        conn: &Connection,
        blip_id: &str,
    ) -> Result<Vec<ClipboardPayload>, BlipError> {
        let mut stmt = conn.prepare(
            "SELECT id, blip_id, payload_kind, mime_type, platform_format, byte_size,
                    content_hash, source_app, captured_at, preview_ref, blob_ref, inline_text,
                    metadata_json, created_at
             FROM blip_payloads
             WHERE blip_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?;

        let mut rows = stmt.query([blip_id])?;
        let mut payloads = Vec::new();

        while let Some(row) = rows.next()? {
            payloads.push(map_payload_row(row)?);
        }

        Ok(payloads)
    }

    pub fn list_blips(&self, workspace_name: &str) -> Result<Vec<Blip>, BlipError> {
        self.list_blips_limited(workspace_name, DEFAULT_LIST_LIMIT)
    }

    pub fn list_blips_limited(
        &self,
        workspace_name: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        if self.get_workspace(workspace_name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(workspace_name.to_owned()));
        }

        Self::list_blips_limited_from(&self.conn, workspace_name, limit)
    }

    fn list_blips_limited_from(
        conn: &Connection,
        workspace_name: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        let mut stmt = conn.prepare(
            "SELECT id, workspace_name, source_app, content_type, language, content, size_bytes,
                    token_estimate, is_redacted, tags_json, created_at
             FROM blips
             WHERE workspace_name = ?1
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?2",
        )?;

        let mut rows = stmt.query(params![workspace_name, sqlite_limit(limit)])?;
        let mut blips = Vec::new();

        while let Some(row) = rows.next()? {
            blips.push(map_blip_row(row)?);
        }

        Ok(blips)
    }

    pub fn list_blip_summaries(
        &self,
        workspace_name: &str,
        limit: usize,
    ) -> Result<Vec<BlipSummary>, BlipError> {
        if self.get_workspace(workspace_name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(workspace_name.to_owned()));
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, workspace_name, source_app, content_type, language,
                    substr(content, 1, ?2), size_bytes, token_estimate, is_redacted, tags_json,
                    created_at
             FROM blips
             WHERE workspace_name = ?1
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?3",
        )?;

        let mut rows = stmt.query(params![
            workspace_name,
            DEFAULT_BLIP_PREVIEW_CHARS,
            sqlite_limit(limit)
        ])?;
        let mut summaries = Vec::new();

        while let Some(row) = rows.next()? {
            summaries.push(map_blip_summary_row(row)?);
        }

        Ok(summaries)
    }

    pub fn search_blip_summaries(
        &self,
        workspace_name: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<BlipSummary>, BlipError> {
        validate_search_query(query)?;

        if self.get_workspace(workspace_name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(workspace_name.to_owned()));
        }

        Self::search_blip_summaries_from(&self.conn, workspace_name, query, limit)
    }

    fn search_blip_summaries_from(
        conn: &Connection,
        workspace_name: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<BlipSummary>, BlipError> {
        let mut stmt = conn.prepare(
            "SELECT blips.id, blips.workspace_name, blips.source_app, blips.content_type,
                    blips.language, substr(blips.content, 1, ?3), blips.size_bytes,
                    blips.token_estimate, blips.is_redacted, blips.tags_json, blips.created_at
             FROM blips_fts
             JOIN blips ON blips_fts.rowid = blips.rowid
             WHERE blips_fts MATCH ?2 AND blips.workspace_name = ?1
             ORDER BY bm25(blips_fts) ASC, blips.created_at DESC, blips.rowid DESC
             LIMIT ?4",
        )?;

        let mut rows = stmt
            .query(params![
                workspace_name,
                query,
                DEFAULT_BLIP_PREVIEW_CHARS,
                sqlite_limit(limit)
            ])
            .map_err(map_search_error)?;
        let mut summaries = Vec::new();

        while let Some(row) = rows.next().map_err(map_search_error)? {
            summaries.push(map_blip_summary_row(row)?);
        }

        Ok(summaries)
    }

    pub fn list_agent_blips(
        &mut self,
        workspace_name: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        self.with_busy_retry(|store| store.list_agent_blips_once(workspace_name, limit))
    }

    pub fn search_agent_blips(
        &mut self,
        workspace_name: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        validate_search_query(query)?;
        self.with_busy_retry(|store| store.search_agent_blips_once(workspace_name, query, limit))
    }

    fn list_agent_blips_once(
        &mut self,
        workspace_name: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let workspace = Self::require_agent_read_access_from(&tx, workspace_name)?;
        let blips = Self::list_blips_limited_from(&tx, workspace_name, limit)?;
        let details_json = serde_json::json!({
            "limit": sqlite_limit(limit),
            "result_count": blips.len(),
        })
        .to_string();

        Self::insert_audit_event(
            &tx,
            ActorType::Agent,
            None,
            AuditEventType::BlipsRead,
            None,
            Some(workspace.name),
            Some(details_json),
        )?;

        tx.commit()?;
        Ok(blips)
    }

    fn search_agent_blips_once(
        &mut self,
        workspace_name: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let workspace = Self::require_agent_read_access_from(&tx, workspace_name)?;
        let blips = Self::search_blips_limited_from(&tx, workspace_name, query, limit)?;
        let details_json = serde_json::json!({
            "query": query,
            "limit": sqlite_limit(limit),
            "result_count": blips.len(),
        })
        .to_string();

        Self::insert_audit_event(
            &tx,
            ActorType::Agent,
            None,
            AuditEventType::BlipsRead,
            None,
            Some(workspace.name),
            Some(details_json),
        )?;

        tx.commit()?;
        Ok(blips)
    }

    fn search_blips_limited_from(
        conn: &Connection,
        workspace_name: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        let mut stmt = conn.prepare(
            "SELECT blips.id, blips.workspace_name, blips.source_app, blips.content_type,
                    blips.language, blips.content, blips.size_bytes, blips.token_estimate,
                    blips.is_redacted, blips.tags_json, blips.created_at
             FROM blips_fts
             JOIN blips ON blips_fts.rowid = blips.rowid
             WHERE blips_fts MATCH ?2 AND blips.workspace_name = ?1
             ORDER BY bm25(blips_fts) ASC, blips.created_at DESC, blips.rowid DESC
             LIMIT ?3",
        )?;

        let mut rows = stmt
            .query(params![workspace_name, query, sqlite_limit(limit)])
            .map_err(map_search_error)?;
        let mut blips = Vec::new();

        while let Some(row) = rows.next().map_err(map_search_error)? {
            blips.push(map_blip_row(row)?);
        }

        Ok(blips)
    }

    pub fn require_agent_read_access(&self, workspace_name: &str) -> Result<Workspace, BlipError> {
        Self::require_agent_read_access_from(&self.conn, workspace_name)
    }

    fn require_agent_read_access_from(
        conn: &Connection,
        workspace_name: &str,
    ) -> Result<Workspace, BlipError> {
        let workspace = Self::get_workspace_from(conn, workspace_name)?
            .ok_or_else(|| BlipError::WorkspaceNotFound(workspace_name.to_owned()))?;
        workspace.require_agent_read_access()?;
        Ok(workspace)
    }

    pub fn move_blip(&mut self, id: &str, target_workspace: &str) -> Result<BlipMove, BlipError> {
        self.with_busy_retry(|store| store.move_blip_once(id, target_workspace))
    }

    fn move_blip_once(&mut self, id: &str, target_workspace: &str) -> Result<BlipMove, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let moved = Self::move_blip_in_transaction(&tx, id, target_workspace)?;

        tx.commit()?;
        Ok(moved)
    }

    pub fn move_latest_inbox_blip(
        &mut self,
        target_workspace: &str,
    ) -> Result<BlipMove, BlipError> {
        self.with_busy_retry(|store| store.move_latest_inbox_blip_once(target_workspace))
    }

    fn move_latest_inbox_blip_once(
        &mut self,
        target_workspace: &str,
    ) -> Result<BlipMove, BlipError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if Self::get_workspace_from(&tx, target_workspace)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(target_workspace.to_owned()));
        }

        let id = tx
            .query_row(
                "SELECT id FROM blips
                 WHERE workspace_name = ?1
                 ORDER BY created_at DESC, rowid DESC
                 LIMIT 1",
                [INBOX_WORKSPACE],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(BlipError::InboxEmpty)?;

        let moved = Self::move_blip_in_transaction(&tx, &id, target_workspace)?;

        tx.commit()?;
        Ok(moved)
    }

    fn move_blip_in_transaction(
        conn: &Connection,
        id: &str,
        target_workspace: &str,
    ) -> Result<BlipMove, BlipError> {
        if Self::get_workspace_from(conn, target_workspace)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(target_workspace.to_owned()));
        }

        let from_workspace: String = conn
            .query_row(
                "SELECT workspace_name FROM blips WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| BlipError::BlipNotFound(id.to_owned()))?;

        conn.execute(
            "UPDATE blips SET workspace_name = ?1 WHERE id = ?2",
            params![target_workspace, id],
        )?;

        let details_json = serde_json::json!({
            "from_workspace": from_workspace,
            "to_workspace": target_workspace,
        })
        .to_string();

        Self::insert_audit_event(
            conn,
            ActorType::User,
            None,
            AuditEventType::BlipMoved,
            Some(id.to_owned()),
            Some(target_workspace.to_owned()),
            Some(details_json),
        )?;

        Ok(BlipMove {
            id: id.to_owned(),
            from_workspace,
            to_workspace: target_workspace.to_owned(),
        })
    }

    fn insert_workspace(
        conn: &Connection,
        workspace: &NewWorkspace,
        ignore_if_exists: bool,
    ) -> Result<bool, BlipError> {
        let statement = if ignore_if_exists {
            "INSERT OR IGNORE INTO workspaces (
                name, description, color, agent_access, sticky_capture, retention_days, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        } else {
            "INSERT INTO workspaces (
                name, description, color, agent_access, sticky_capture, retention_days, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        };

        let affected_rows = match conn.execute(
            statement,
            params![
                workspace.name,
                workspace.description,
                workspace.color,
                workspace.agent_access,
                workspace.sticky_capture,
                workspace.retention_days,
                Utc::now(),
            ],
        ) {
            Ok(affected_rows) => affected_rows,
            Err(error)
                if !ignore_if_exists
                    && matches!(
                        error,
                        rusqlite::Error::SqliteFailure(sqlite_error, _)
                            if sqlite_error.code == ErrorCode::ConstraintViolation
                    ) =>
            {
                return Err(BlipError::WorkspaceAlreadyExists(workspace.name.clone()));
            }
            Err(error) if is_sqlite_busy_error(&error) => return Err(BlipError::DatabaseBusy),
            Err(error) => return Err(BlipError::Database(error)),
        };

        if affected_rows > 0 {
            Self::insert_audit_event(
                conn,
                ActorType::System,
                None,
                AuditEventType::WorkspaceCreated,
                None,
                Some(workspace.name.clone()),
                None,
            )?;
        }

        Ok(affected_rows > 0)
    }

    fn ensure_active_workspace(conn: &Connection, workspace_name: &str) -> Result<(), BlipError> {
        conn.execute(
            "INSERT INTO app_state(key, value)
             VALUES('active_workspace', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [workspace_name],
        )?;
        Ok(())
    }

    fn insert_text_payload(
        conn: &Connection,
        blip_id: &str,
        blip: &NewBlip,
        size_bytes: i64,
        created_at: DateTime<Utc>,
    ) -> Result<(), BlipError> {
        conn.execute(
            "INSERT INTO blip_payloads (
                id, blip_id, payload_kind, mime_type, platform_format, byte_size, content_hash,
                source_app, captured_at, preview_ref, blob_ref, inline_text, metadata_json, created_at
             ) VALUES (?1, ?2, ?3, 'text/plain', NULL, ?4, NULL, ?5, ?6, NULL, NULL, ?7, '{}', ?6)",
            params![
                format!("{blip_id}:payload:text"),
                blip_id,
                PayloadKind::Text.as_str(),
                size_bytes,
                blip.source_app,
                created_at,
                blip.content,
            ],
        )?;
        Ok(())
    }

    fn insert_audit_event(
        conn: &Connection,
        actor_type: ActorType,
        actor_id: Option<String>,
        event_type: AuditEventType,
        target_blip_id: Option<String>,
        target_workspace: Option<String>,
        details_json: Option<String>,
    ) -> Result<(), BlipError> {
        conn.execute(
            "INSERT INTO audit_events (
                id, actor_type, actor_id, event_type, target_blip_id, target_workspace, details_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Uuid::new_v4().to_string(),
                actor_type.as_str(),
                actor_id,
                event_type.as_str(),
                target_blip_id,
                target_workspace,
                details_json,
                Utc::now(),
            ],
        )?;
        Ok(())
    }

    pub fn list_audit_events(&self) -> Result<Vec<AuditEvent>, BlipError> {
        self.list_audit_events_limited(DEFAULT_LIST_LIMIT)
    }

    pub fn list_audit_events_limited(&self, limit: usize) -> Result<Vec<AuditEvent>, BlipError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, actor_type, actor_id, event_type, target_blip_id, target_workspace, details_json, created_at
             FROM audit_events ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;

        let mut rows = stmt.query([sqlite_limit(limit)])?;
        let mut events = Vec::new();

        while let Some(row) = rows.next()? {
            events.push(AuditEvent {
                id: row.get(0)?,
                actor_type: ActorType::parse(row.get::<_, String>(1)?.as_str())?,
                actor_id: row.get(2)?,
                event_type: AuditEventType::parse(row.get::<_, String>(3)?.as_str())?,
                target_blip_id: row.get(4)?,
                target_workspace: row.get(5)?,
                details_json: row.get(6)?,
                created_at: row.get(7)?,
            });
        }

        Ok(events)
    }
}

fn validate_workspace(workspace: &NewWorkspace) -> Result<(), BlipError> {
    if workspace.name.trim().is_empty() {
        return Err(BlipError::InvalidInput {
            field: "workspace.name",
            reason: "must not be empty",
        });
    }

    if workspace.name.trim() != workspace.name {
        return Err(BlipError::InvalidInput {
            field: "workspace.name",
            reason: "must not have leading or trailing whitespace",
        });
    }

    if workspace.retention_days.is_some_and(|days| days < 0) {
        return Err(BlipError::InvalidInput {
            field: "workspace.retention_days",
            reason: "must be greater than or equal to 0",
        });
    }

    Ok(())
}

fn validate_blip(blip: &NewBlip) -> Result<(), BlipError> {
    if blip.workspace_name.trim().is_empty() {
        return Err(BlipError::InvalidInput {
            field: "blip.workspace_name",
            reason: "must not be empty",
        });
    }

    if blip.token_estimate.is_some_and(|estimate| estimate < 0) {
        return Err(BlipError::InvalidInput {
            field: "blip.token_estimate",
            reason: "must be greater than or equal to 0",
        });
    }

    Ok(())
}

fn validate_blob_payload(payload: &NewClipboardPayload) -> Result<(), BlipError> {
    if payload.bytes.is_empty() {
        return Err(BlipError::InvalidInput {
            field: "payload.bytes",
            reason: "must not be empty",
        });
    }

    if !matches!(
        payload.kind,
        PayloadKind::Image | PayloadKind::Html | PayloadKind::Rtf | PayloadKind::Unknown
    ) {
        return Err(BlipError::InvalidInput {
            field: "payload.kind",
            reason: "must be a blob-backed rich payload kind; file lists are metadata-only",
        });
    }

    if !matches!(
        payload.kind,
        PayloadKind::Text | PayloadKind::Html | PayloadKind::Rtf
    ) && payload.inline_text.is_some()
    {
        return Err(BlipError::InvalidInput {
            field: "payload.inline_text",
            reason: "must be empty for binary payloads",
        });
    }

    Ok(())
}

fn validate_metadata_payload(payload: &NewClipboardMetadataPayload) -> Result<(), BlipError> {
    if payload.byte_size < 0 {
        return Err(BlipError::InvalidInput {
            field: "payload.byte_size",
            reason: "must be greater than or equal to 0",
        });
    }

    if !matches!(payload.kind, PayloadKind::FileList | PayloadKind::Unknown) {
        return Err(BlipError::InvalidInput {
            field: "payload.kind",
            reason: "must be file_list or unknown for metadata-only payloads",
        });
    }

    if payload.inline_text.is_some() {
        return Err(BlipError::InvalidInput {
            field: "payload.inline_text",
            reason: "must be empty for metadata-only payloads",
        });
    }

    Ok(())
}

fn validate_search_query(query: &str) -> Result<(), BlipError> {
    if query.trim().is_empty() {
        return Err(BlipError::InvalidInput {
            field: "search.query",
            reason: "must not be empty",
        });
    }

    Ok(())
}

fn map_search_error(error: rusqlite::Error) -> BlipError {
    match &error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => {
            BlipError::InvalidSearchQuery(message.clone())
        }
        _ => BlipError::from(error),
    }
}

fn map_blip_row(row: &rusqlite::Row<'_>) -> Result<Blip, BlipError> {
    let tags_json: String = row.get(9)?;
    let tags = serde_json::from_str(&tags_json)?;

    Ok(Blip {
        id: row.get(0)?,
        workspace_name: row.get(1)?,
        source_app: row.get(2)?,
        content_type: ContentType::parse(row.get::<_, String>(3)?.as_str())?,
        language: row.get(4)?,
        content: row.get(5)?,
        size_bytes: row.get(6)?,
        token_estimate: row.get(7)?,
        is_redacted: row.get(8)?,
        tags,
        created_at: row.get::<_, DateTime<Utc>>(10)?,
    })
}

fn map_blip_summary_row(row: &rusqlite::Row<'_>) -> Result<BlipSummary, BlipError> {
    let tags_json: String = row.get(9)?;
    let tags = serde_json::from_str(&tags_json)?;

    Ok(BlipSummary {
        id: row.get(0)?,
        workspace_name: row.get(1)?,
        source_app: row.get(2)?,
        content_type: ContentType::parse(row.get::<_, String>(3)?.as_str())?,
        language: row.get(4)?,
        preview: row.get(5)?,
        size_bytes: row.get(6)?,
        token_estimate: row.get(7)?,
        is_redacted: row.get(8)?,
        tags,
        created_at: row.get::<_, DateTime<Utc>>(10)?,
    })
}

fn map_payload_row(row: &rusqlite::Row<'_>) -> Result<ClipboardPayload, BlipError> {
    let metadata_json: String = row.get(12)?;
    let metadata = serde_json::from_str(&metadata_json)?;

    Ok(ClipboardPayload {
        id: row.get(0)?,
        blip_id: row.get(1)?,
        kind: PayloadKind::parse(row.get::<_, String>(2)?.as_str())?,
        mime_type: row.get(3)?,
        platform_format: row.get(4)?,
        byte_size: row.get(5)?,
        content_hash: row.get(6)?,
        source_app: row.get(7)?,
        captured_at: row.get::<_, DateTime<Utc>>(8)?,
        preview_ref: row.get(9)?,
        blob_ref: row.get(10)?,
        inline_text: row.get(11)?,
        metadata,
        created_at: row.get::<_, DateTime<Utc>>(13)?,
    })
}

fn sqlite_limit(limit: usize) -> i64 {
    i64::try_from(limit.min(MAX_LIST_LIMIT)).unwrap_or(MAX_LIST_LIMIT as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContentType, NewBlip, NewWorkspace};
    use std::sync::{Arc, Barrier};

    #[test]
    fn initializes_with_inbox_and_active_workspace() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let active = store
            .get_active_workspace()
            .expect("active workspace query should work");
        assert_eq!(active.as_deref(), Some("inbox"));

        let workspaces = store.list_workspaces().expect("list should work");
        assert!(workspaces.iter().any(|ws| ws.name == "inbox"));

        let audit_events = store.list_audit_events().expect("audit list should work");
        let schema_init_count = audit_events
            .iter()
            .filter(|event| event.event_type == AuditEventType::SchemaInitialized)
            .count();
        assert_eq!(schema_init_count, 1);

        let schema_version = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .expect("schema version should be readable");
        assert_eq!(schema_version, SCHEMA_VERSION);

        let app_state_schema_version = store
            .connection()
            .query_row(
                "SELECT value FROM app_state WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("app_state schema version should be readable");
        assert_eq!(app_state_schema_version, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn payload_kind_parses_persisted_values() {
        assert_eq!(
            PayloadKind::parse("text").expect("text should parse"),
            PayloadKind::Text
        );
        assert_eq!(
            PayloadKind::parse("image").expect("image should parse"),
            PayloadKind::Image
        );
        assert_eq!(
            PayloadKind::parse("file_list").expect("file list should parse"),
            PayloadKind::FileList
        );
        assert_eq!(
            PayloadKind::parse("html").expect("html should parse"),
            PayloadKind::Html
        );
        assert_eq!(
            PayloadKind::parse("rtf").expect("rtf should parse"),
            PayloadKind::Rtf
        );
        assert_eq!(
            PayloadKind::parse("unknown").expect("unknown should parse"),
            PayloadKind::Unknown
        );

        let error = PayloadKind::parse("binary").expect_err("unknown kind should fail");
        assert!(matches!(
            error,
            BlipError::InvalidPersistedValue {
                field: "payload_kind",
                ..
            }
        ));

        let json =
            serde_json::to_string(&PayloadKind::FileList).expect("payload kind should serialize");
        assert_eq!(json, "\"file_list\"");
        let decoded: PayloadKind =
            serde_json::from_str("\"file_list\"").expect("payload kind JSON should decode");
        assert_eq!(decoded, PayloadKind::FileList);
    }

    #[test]
    fn migration_backfills_existing_blips_into_fts_index() {
        let db_path =
            std::env::temp_dir().join(format!("blipcoard-fts-migration-{}.db", Uuid::new_v4()));
        {
            let conn = Connection::open(&db_path).expect("database should open");
            conn.execute_batch(include_str!("sql/001_initial.sql"))
                .expect("initial schema should apply");
            conn.execute_batch(include_str!("sql/002_audit_read_events.sql"))
                .expect("v2 migration should apply");
            conn.execute_batch(include_str!("sql/003_sticky_capture_audit_event.sql"))
                .expect("v3 migration should apply");
            conn.pragma_update(None, "user_version", 3)
                .expect("schema version should set");
            conn.execute(
                "INSERT INTO workspaces (
                    name, description, color, agent_access, sticky_capture, retention_days, created_at
                 ) VALUES ('inbox', NULL, NULL, 0, 0, NULL, ?1)",
                [Utc::now()],
            )
            .expect("workspace should insert");
            conn.execute(
                "INSERT INTO blips (
                    id, workspace_name, content_type, content, size_bytes, tags_json, created_at
                 ) VALUES ('existing-blip', 'inbox', 'plain_text', 'historical login note', 21, '[]', ?1)",
                [Utc::now()],
            )
            .expect("historical blip should insert");
        }

        let store = BlipStore::open(&db_path).expect("store should migrate");

        let results = store
            .search_blip_summaries("inbox", "historical", 50)
            .expect("migrated FTS index should search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "existing-blip");

        let schema_version = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .expect("schema version should be readable");
        assert_eq!(schema_version, SCHEMA_VERSION);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn migration_backfills_existing_text_blips_into_payloads() {
        let db_path =
            std::env::temp_dir().join(format!("blipcoard-payload-migration-{}.db", Uuid::new_v4()));
        {
            let conn = Connection::open(&db_path).expect("database should open");
            conn.execute_batch(include_str!("sql/001_initial.sql"))
                .expect("initial schema should apply");
            conn.execute_batch(include_str!("sql/002_audit_read_events.sql"))
                .expect("v2 migration should apply");
            conn.execute_batch(include_str!("sql/003_sticky_capture_audit_event.sql"))
                .expect("v3 migration should apply");
            conn.execute_batch(include_str!("sql/004_blips_fts.sql"))
                .expect("v4 migration should apply");
            conn.pragma_update(None, "user_version", 4)
                .expect("schema version should set");
            conn.execute(
                "INSERT INTO workspaces (
                    name, description, color, agent_access, sticky_capture, retention_days, created_at
                 ) VALUES ('inbox', NULL, NULL, 0, 0, NULL, ?1)",
                ["2026-06-21T12:34:56Z"],
            )
            .expect("workspace should insert");
            conn.execute(
                "INSERT INTO blips (
                    id, workspace_name, source_app, content_type, content, size_bytes, tags_json, created_at
                 ) VALUES ('existing-blip', 'inbox', 'Terminal', 'plain_text', 'historical note', 15, '[]', ?1)",
                ["2026-06-21T12:35:56Z"],
            )
            .expect("historical blip should insert");
        }

        let store = BlipStore::open(&db_path).expect("store should migrate");
        let blip = store
            .get_blip("existing-blip")
            .expect("legacy blip should decode")
            .expect("legacy blip should exist");
        let payloads = store
            .get_blip_payloads("existing-blip")
            .expect("payloads should decode");

        assert_eq!(blip.content, "historical note");
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].id, "existing-blip:payload:text");
        assert_eq!(payloads[0].kind, PayloadKind::Text);
        assert_eq!(payloads[0].mime_type.as_deref(), Some("text/plain"));
        assert_eq!(payloads[0].source_app.as_deref(), Some("Terminal"));
        assert_eq!(payloads[0].inline_text.as_deref(), Some("historical note"));
        assert!(payloads[0].blob_ref.is_none());
        assert_eq!(payloads[0].metadata, serde_json::json!({}));

        let schema_version = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .expect("schema version should be readable");
        assert_eq!(schema_version, SCHEMA_VERSION);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn connection_enforces_foreign_keys() {
        let store = BlipStore::in_memory().expect("store should initialize");

        let error = store
            .connection()
            .execute(
                "INSERT INTO blips (
                    id, workspace_name, content_type, content, size_bytes, tags_json, created_at
                 ) VALUES ('missing-workspace', 'missing', 'plain_text', 'broken', 6, '[]', ?1)",
                [Utc::now()],
            )
            .expect_err("foreign key should reject missing workspace");

        assert!(matches!(error, rusqlite::Error::SqliteFailure(_, Some(_))));
    }

    #[test]
    fn creates_workspace_and_blip() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&NewWorkspace {
                name: "auth-bug".into(),
                description: Some("Auth bug triage".into()),
                color: Some("#ff0000".into()),
                agent_access: true,
                sticky_capture: false,
                retention_days: Some(7),
            })
            .expect("workspace should be created");

        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "auth-bug".into(),
                source_app: Some("Firefox".into()),
                content_type: ContentType::StackTrace,
                language: Some("text".into()),
                content: "TypeError: broken".into(),
                token_estimate: Some(12),
                is_redacted: false,
                tags: vec!["error".into(), "frontend".into()],
            })
            .expect("blip should be inserted");

        assert_eq!(blip.workspace_name, "auth-bug");

        let blips = store.list_blips("auth-bug").expect("list should work");
        assert_eq!(blips.len(), 1);
        assert_eq!(blips[0].id, blip.id);
    }

    #[test]
    fn insert_blip_writes_backward_compatible_text_payload() {
        let mut store = BlipStore::in_memory().expect("store should initialize");

        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: Some("Safari".into()),
                content_type: ContentType::PlainText,
                language: None,
                content: "copied text".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should insert");

        let payloads = store
            .get_blip_payloads(&blip.id)
            .expect("payloads should list");

        assert_eq!(blip.content, "copied text");
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].blip_id, blip.id);
        assert_eq!(payloads[0].kind, PayloadKind::Text);
        assert_eq!(payloads[0].mime_type.as_deref(), Some("text/plain"));
        assert_eq!(payloads[0].byte_size, "copied text".len() as i64);
        assert_eq!(payloads[0].source_app.as_deref(), Some("Safari"));
        assert_eq!(payloads[0].inline_text.as_deref(), Some("copied text"));
        assert!(payloads[0].blob_ref.is_none());
    }

    #[test]
    fn insert_blob_payload_writes_blob_metadata_and_references() {
        let root = temp_blob_root("store-blob-metadata");
        let blob_store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = insert_test_blip(&mut store, "image placeholder");

        let payload = store
            .insert_blob_payload(&blip.id, &image_payload(b"png bytes"), &blob_store)
            .expect("blob payload should insert");

        assert_eq!(payload.kind, PayloadKind::Image);
        assert_eq!(payload.mime_type.as_deref(), Some("image/png"));
        assert_eq!(payload.byte_size, 9);
        assert!(
            payload
                .content_hash
                .as_deref()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        let blob_ref = payload.blob_ref.as_deref().expect("blob ref should exist");
        assert!(
            blob_store
                .exists(blob_ref)
                .expect("blob exists should work")
        );
        assert_eq!(
            store.referenced_blob_refs().expect("refs should list"),
            HashSet::from([blob_ref.to_owned()])
        );

        let payloads = store
            .get_blip_payloads(&blip.id)
            .expect("payloads should list");
        assert_eq!(payloads.len(), 2);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn insert_metadata_payload_records_file_list_without_blob_ref() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = insert_test_blip(&mut store, "file-list placeholder");

        let payload = store
            .insert_metadata_payload(
                &blip.id,
                &NewClipboardMetadataPayload {
                    kind: PayloadKind::FileList,
                    mime_type: Some("text/uri-list".to_owned()),
                    platform_format: Some("test:file-list".to_owned()),
                    source_app: None,
                    preview_ref: None,
                    inline_text: None,
                    metadata: serde_json::json!({
                        "policy": "metadata_only",
                        "paths": ["/tmp/a.txt", "/tmp/b.txt"],
                    }),
                    byte_size: 20,
                },
            )
            .expect("metadata payload should insert");

        assert_eq!(payload.kind, PayloadKind::FileList);
        assert_eq!(payload.mime_type.as_deref(), Some("text/uri-list"));
        assert_eq!(payload.byte_size, 20);
        assert!(payload.blob_ref.is_none());
        assert!(payload.content_hash.is_none());
        assert!(payload.inline_text.is_none());
        assert_eq!(payload.metadata["policy"], "metadata_only");

        let payloads = store
            .get_blip_payloads(&blip.id)
            .expect("payloads should list");
        assert_eq!(payloads.len(), 2);
        assert!(
            payloads
                .iter()
                .any(|payload| payload.kind == PayloadKind::FileList)
        );
    }

    #[test]
    fn insert_blob_payload_rejects_file_list_bytes() {
        let root = temp_blob_root("store-file-list-blob-reject");
        let blob_store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = insert_test_blip(&mut store, "file-list placeholder");
        let mut payload = image_payload(b"file bytes should not import");
        payload.kind = PayloadKind::FileList;
        payload.mime_type = Some("text/uri-list".to_owned());
        payload.platform_format = Some("test:file-list".to_owned());
        payload.metadata = serde_json::json!({
            "policy": "metadata_only",
        });

        let error = store
            .insert_blob_payload(&blip.id, &payload, &blob_store)
            .expect_err("file-list bytes should be rejected");

        assert!(matches!(
            error,
            BlipError::InvalidInput {
                field: "payload.kind",
                ..
            }
        ));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn delete_blip_keeps_shared_blob_until_last_reference_is_deleted() {
        let root = temp_blob_root("store-shared-delete");
        let blob_store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let first = insert_test_blip(&mut store, "first image placeholder");
        let second = insert_test_blip(&mut store, "second image placeholder");

        let first_payload = store
            .insert_blob_payload(&first.id, &image_payload(b"shared image"), &blob_store)
            .expect("first payload should insert");
        let second_payload = store
            .insert_blob_payload(&second.id, &image_payload(b"shared image"), &blob_store)
            .expect("second payload should insert");
        let blob_ref = first_payload
            .blob_ref
            .as_deref()
            .expect("first payload should have blob ref");
        assert_eq!(first_payload.blob_ref, second_payload.blob_ref);

        assert!(
            store
                .delete_blip_and_collect_orphans(&blob_store, &first.id)
                .expect("first delete should work")
        );
        assert!(
            blob_store
                .exists(blob_ref)
                .expect("blob should still exist")
        );
        assert_eq!(
            store.referenced_blob_refs().expect("refs should list"),
            HashSet::from([blob_ref.to_owned()])
        );

        assert!(
            store
                .delete_blip_and_collect_orphans(&blob_store, &second.id)
                .expect("second delete should work")
        );
        assert!(!blob_store.exists(blob_ref).expect("blob should be gone"));
        assert!(
            store
                .referenced_blob_refs()
                .expect("refs should list")
                .is_empty()
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn garbage_collect_removes_unreferenced_blob_files() {
        let root = temp_blob_root("store-gc");
        let blob_store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let orphan = blob_store.write(b"orphan").expect("orphan should write");
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = insert_test_blip(&mut store, "referenced image placeholder");
        let referenced = store
            .insert_blob_payload(&blip.id, &image_payload(b"referenced"), &blob_store)
            .expect("referenced payload should insert");
        let referenced_blob_ref = referenced
            .blob_ref
            .as_deref()
            .expect("referenced payload should have blob ref")
            .to_owned();

        let report = store
            .garbage_collect_blobs(&blob_store)
            .expect("gc should work");

        assert_eq!(report.removed, vec![orphan.blob_ref]);
        assert!(
            blob_store
                .exists(&referenced_blob_ref)
                .expect("referenced blob should remain")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn insert_blob_payload_leaves_unique_failed_metadata_blob_for_gc() {
        let root = temp_blob_root("store-rollback-gc");
        let blob_store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let metadata = blob_store
            .write(b"rollback bytes")
            .expect("test blob should write");
        blob_store
            .delete(&metadata.blob_ref)
            .expect("test blob should reset");

        let error = store
            .insert_blob_payload(
                "missing-blip",
                &image_payload(b"rollback bytes"),
                &blob_store,
            )
            .expect_err("missing blip should fail after blob write");
        assert!(matches!(error, BlipError::BlipNotFound(id) if id == "missing-blip"));
        assert!(
            blob_store
                .exists(&metadata.blob_ref)
                .expect("unreferenced blob should remain for GC")
        );
        assert!(
            blob_store
                .garbage_collect(&HashSet::new())
                .expect("gc should remove unreferenced leftover")
                .removed
                == vec![metadata.blob_ref]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_blob_payload_metadata_does_not_delete_referenced_dedup_blob() {
        let root = temp_blob_root("store-rollback-shared");
        let blob_store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = insert_test_blip(&mut store, "referenced image placeholder");
        let payload = store
            .insert_blob_payload(&blip.id, &image_payload(b"shared rollback"), &blob_store)
            .expect("payload should insert");
        let blob_ref = payload
            .blob_ref
            .as_deref()
            .expect("payload should have blob ref")
            .to_owned();

        let error = store
            .insert_blob_payload(
                "missing-blip",
                &image_payload(b"shared rollback"),
                &blob_store,
            )
            .expect_err("missing blip should fail after dedupe");
        assert!(matches!(error, BlipError::BlipNotFound(id) if id == "missing-blip"));
        assert!(
            blob_store
                .exists(&blob_ref)
                .expect("referenced blob should remain")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn payload_schema_rejects_binary_inline_text_negative_size_and_orphans() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "schema checks".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should insert");

        let binary_inline = store
            .connection()
            .execute(
                "INSERT INTO blip_payloads (
                    id, blip_id, payload_kind, byte_size, captured_at, inline_text, metadata_json, created_at
                 ) VALUES ('bad-inline', ?1, 'image', 5, ?2, 'abcde', '{}', ?2)",
                params![blip.id, Utc::now()],
            )
            .expect_err("binary payloads should not allow inline text");
        assert!(matches!(
            binary_inline,
            rusqlite::Error::SqliteFailure(_, Some(_))
        ));

        let negative_size = store
            .connection()
            .execute(
                "INSERT INTO blip_payloads (
                    id, blip_id, payload_kind, byte_size, captured_at, metadata_json, created_at
                 ) VALUES ('bad-size', ?1, 'text', -1, ?2, '{}', ?2)",
                params![blip.id, Utc::now()],
            )
            .expect_err("negative payload sizes should fail");
        assert!(matches!(
            negative_size,
            rusqlite::Error::SqliteFailure(_, Some(_))
        ));

        let orphan = store
            .connection()
            .execute(
                "INSERT INTO blip_payloads (
                    id, blip_id, payload_kind, byte_size, captured_at, metadata_json, created_at
                 ) VALUES ('bad-orphan', 'missing-blip', 'text', 1, ?1, '{}', ?1)",
                [Utc::now()],
            )
            .expect_err("orphan payloads should fail");
        assert!(matches!(orphan, rusqlite::Error::SqliteFailure(_, Some(_))));
    }

    #[test]
    fn insert_blip_adds_secret_detection_tags_without_mutating_content() {
        let mut store = BlipStore::in_memory().expect("store should initialize");

        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "api_key = abcdef1234567890".into(),
                token_estimate: None,
                is_redacted: false,
                tags: vec!["clipboard".into()],
            })
            .expect("blip should be inserted");

        assert!(!blip.is_redacted);
        assert_eq!(blip.content, "api_key = abcdef1234567890");
        assert_eq!(
            blip.tags,
            vec![
                "clipboard".to_string(),
                "type:plain_text".to_string(),
                "secret".to_string(),
                "secret:assignment".to_string(),
            ]
        );

        let summary = store
            .list_blip_summaries("inbox", 1)
            .expect("summary list should work")
            .into_iter()
            .next()
            .expect("summary should exist");
        assert_eq!(
            summary.tags,
            vec![
                "clipboard".to_string(),
                "type:plain_text".to_string(),
                "secret".to_string(),
                "secret:assignment".to_string(),
            ]
        );
    }

    #[test]
    fn insert_blip_infers_content_type_and_adds_type_tag() {
        let mut store = BlipStore::in_memory().expect("store should initialize");

        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "{\"ok\":true}".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should be inserted");

        assert_eq!(blip.content_type, ContentType::Json);
        assert_eq!(blip.tags, vec!["type:json".to_string()]);
    }

    #[test]
    fn sticky_capture_is_single_workspace_and_audited() {
        let mut store = store_with_workspace("auth-bug");
        store
            .create_workspace(&NewWorkspace {
                name: "notes".into(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");

        let auth_bug = store
            .set_sticky_capture("auth-bug", true)
            .expect("sticky should enable");
        assert!(auth_bug.sticky_capture);
        assert_eq!(
            store
                .get_sticky_workspace()
                .expect("sticky workspace should read")
                .as_deref(),
            Some("auth-bug")
        );

        let notes = store
            .set_sticky_capture("notes", true)
            .expect("sticky should move");
        assert!(notes.sticky_capture);
        assert!(
            !store
                .get_workspace("auth-bug")
                .expect("workspace should read")
                .expect("workspace should exist")
                .sticky_capture
        );

        let notes = store
            .set_sticky_capture("notes", false)
            .expect("sticky should disable");
        assert!(!notes.sticky_capture);
        assert!(
            store
                .get_sticky_workspace()
                .expect("sticky workspace should read")
                .is_none()
        );

        let audit_events = store.list_audit_events().expect("audit list should work");
        assert!(audit_events.iter().any(|event| {
            event.event_type == AuditEventType::StickyCaptureChanged
                && event.target_workspace.as_deref() == Some("notes")
                && event
                    .details_json
                    .as_deref()
                    .is_some_and(|details| details.contains("\"enabled\":false"))
        }));
    }

    #[test]
    fn moves_latest_inbox_blip_to_workspace_with_audit_event() {
        let mut store = store_with_workspace("auth-bug");
        store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "older note".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("older inbox blip should insert");
        let latest = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "latest note".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("latest inbox blip should insert");

        let moved = store
            .move_latest_inbox_blip("auth-bug")
            .expect("latest inbox blip should move");

        assert_eq!(moved.id, latest.id);
        assert_eq!(moved.from_workspace, "inbox");
        assert_eq!(moved.to_workspace, "auth-bug");
        assert_eq!(
            store
                .get_blip(&latest.id)
                .expect("blip should read")
                .expect("blip should exist")
                .workspace_name,
            "auth-bug"
        );
        let audit_events = store.list_audit_events().expect("audit list should work");
        assert!(audit_events.iter().any(|event| {
            event.event_type == AuditEventType::BlipMoved
                && event.target_blip_id.as_deref() == Some(latest.id.as_str())
                && event.target_workspace.as_deref() == Some("auth-bug")
                && event
                    .details_json
                    .as_deref()
                    .is_some_and(|details| details.contains("\"from_workspace\":\"inbox\""))
        }));
    }

    #[test]
    fn latest_inbox_routing_uses_insert_order_when_timestamps_match() {
        let mut store = store_with_workspace("auth-bug");
        insert_raw_blip(&store, "z-older", "inbox", "older same-time note");
        insert_raw_blip(&store, "a-newer", "inbox", "newer same-time note");

        let moved = store
            .move_latest_inbox_blip("auth-bug")
            .expect("latest same-timestamp inbox blip should move");

        assert_eq!(moved.id, "a-newer");
        assert_eq!(
            store
                .list_blips("auth-bug")
                .expect("target list should work")
                .first()
                .map(|blip| blip.id.as_str()),
            Some("a-newer")
        );
    }

    #[test]
    fn moves_specific_blip_between_workspaces_for_recovery() {
        let mut store = store_with_workspace("auth-bug");
        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "auth-bug".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "misrouted note".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should insert");

        let moved = store
            .move_blip(&blip.id, "inbox")
            .expect("specific blip should move");

        assert_eq!(moved.id, blip.id);
        assert_eq!(moved.from_workspace, "auth-bug");
        assert_eq!(moved.to_workspace, "inbox");
        assert_eq!(
            store
                .get_blip(&blip.id)
                .expect("blip should read")
                .expect("blip should exist")
                .workspace_name,
            "inbox"
        );
    }

    #[test]
    fn move_returns_clear_errors_for_missing_inputs() {
        let mut store = store_with_workspace("auth-bug");
        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "note".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should insert");

        let missing_workspace = store
            .move_blip(&blip.id, "missing")
            .expect_err("missing workspace should fail");
        assert!(matches!(
            missing_workspace,
            BlipError::WorkspaceNotFound(workspace) if workspace == "missing"
        ));

        let missing_blip = store
            .move_blip("missing-blip", "auth-bug")
            .expect_err("missing blip should fail");
        assert!(matches!(
            missing_blip,
            BlipError::BlipNotFound(id) if id == "missing-blip"
        ));
    }

    #[test]
    fn move_latest_inbox_blip_errors_when_inbox_is_empty() {
        let mut store = store_with_workspace("auth-bug");

        let error = store
            .move_latest_inbox_blip("auth-bug")
            .expect_err("empty inbox should fail");

        assert!(matches!(error, BlipError::InboxEmpty));
    }

    #[test]
    fn agent_reads_only_agent_access_workspaces() {
        let mut store = store_with_workspace("auth-bug");
        store
            .create_workspace(&NewWorkspace {
                name: "agent-feed".into(),
                description: None,
                color: None,
                agent_access: true,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("agent workspace should be created");
        store
            .insert_blip(&NewBlip {
                workspace_name: "agent-feed".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "agent-visible note".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("agent blip should insert");

        let visible = store
            .list_agent_blips("agent-feed", 50)
            .expect("agent-readable workspace should list");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].content, "agent-visible note");
        let audit_events = store.list_audit_events().expect("audit list should work");
        assert!(audit_events.iter().any(|event| {
            event.actor_type == ActorType::Agent
                && event.event_type == AuditEventType::BlipsRead
                && event.target_workspace.as_deref() == Some("agent-feed")
                && event.details_json.as_deref().is_some_and(|details| {
                    details.contains("\"limit\":50") && details.contains("\"result_count\":1")
                })
        }));

        let denied = store
            .list_agent_blips("auth-bug", 50)
            .expect_err("human-only workspace should be denied");
        assert!(matches!(
            denied,
            BlipError::AgentAccessDenied(workspace) if workspace == "auth-bug"
        ));

        let inbox_denied = store
            .list_agent_blips("inbox", 50)
            .expect_err("inbox should be denied by default");
        assert!(matches!(
            inbox_denied,
            BlipError::AgentAccessDenied(workspace) if workspace == "inbox"
        ));
    }

    #[test]
    fn search_blips_filters_by_workspace_and_query() {
        let mut store = store_with_workspace("auth-bug");
        store
            .create_workspace(&NewWorkspace {
                name: "notes".into(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");
        let auth_match = store
            .insert_blip(&NewBlip {
                workspace_name: "auth-bug".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "login timeout in auth callback".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("matching blip should insert");
        store
            .insert_blip(&NewBlip {
                workspace_name: "auth-bug".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "billing timeout in webhook".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("nonmatching blip should insert");
        store
            .insert_blip(&NewBlip {
                workspace_name: "notes".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "login timeout in another workspace".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("cross-workspace blip should insert");

        let results = store
            .search_blip_summaries("auth-bug", "login", 50)
            .expect("search should work");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, auth_match.id);
        assert_eq!(results[0].preview, "login timeout in auth callback");
    }

    #[test]
    fn search_rejects_empty_and_invalid_queries() {
        let store = BlipStore::in_memory().expect("store should initialize");

        let empty = store
            .search_blip_summaries("inbox", " ", 50)
            .expect_err("empty search query should fail");
        assert!(matches!(
            empty,
            BlipError::InvalidInput {
                field: "search.query",
                ..
            }
        ));

        let invalid = store
            .search_blip_summaries("inbox", "\"unterminated", 50)
            .expect_err("invalid FTS query should fail");
        assert!(matches!(invalid, BlipError::InvalidSearchQuery(_)));
    }

    #[test]
    fn agent_search_enforces_access_policy_and_audits_reads() {
        let mut store = store_with_workspace("human-only");
        store
            .create_workspace(&NewWorkspace {
                name: "agent-feed".into(),
                description: None,
                color: None,
                agent_access: true,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("agent workspace should be created");
        store
            .insert_blip(&NewBlip {
                workspace_name: "agent-feed".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "deploy rollback note".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("agent blip should insert");

        let results = store
            .search_agent_blips("agent-feed", "rollback", 50)
            .expect("agent search should work");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "deploy rollback note");

        let audit_events = store.list_audit_events().expect("audit list should work");
        assert!(audit_events.iter().any(|event| {
            event.actor_type == ActorType::Agent
                && event.event_type == AuditEventType::BlipsRead
                && event.target_workspace.as_deref() == Some("agent-feed")
                && event.details_json.as_deref().is_some_and(|details| {
                    details.contains("\"query\":\"rollback\"")
                        && details.contains("\"result_count\":1")
                })
        }));

        let denied = store
            .search_agent_blips("human-only", "rollback", 50)
            .expect_err("human-only workspace should be denied");
        assert!(matches!(
            denied,
            BlipError::AgentAccessDenied(workspace) if workspace == "human-only"
        ));
    }

    #[test]
    fn agent_read_policy_returns_explicit_decisions() {
        let mut store = store_with_workspace("human-only");
        store
            .create_workspace(&NewWorkspace {
                name: "agent-readable".into(),
                description: None,
                color: None,
                agent_access: true,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("agent-readable workspace should be created");

        let allowed = store
            .require_agent_read_access("agent-readable")
            .expect("agent-readable workspace should be allowed");
        assert_eq!(allowed.name, "agent-readable");

        let denied = store
            .require_agent_read_access("human-only")
            .expect_err("human-only workspace should be denied");
        assert!(matches!(
            denied,
            BlipError::AgentAccessDenied(workspace) if workspace == "human-only"
        ));

        let inbox_denied = store
            .require_agent_read_access("inbox")
            .expect_err("inbox should be denied by default");
        assert!(matches!(
            inbox_denied,
            BlipError::AgentAccessDenied(workspace) if workspace == "inbox"
        ));

        let missing = store
            .require_agent_read_access("missing")
            .expect_err("missing workspace should be not found");
        assert!(matches!(
            missing,
            BlipError::WorkspaceNotFound(workspace) if workspace == "missing"
        ));
    }

    #[test]
    fn duplicate_workspace_creation_returns_error() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let workspace = NewWorkspace {
            name: "demo".into(),
            description: Some("First".into()),
            color: Some("#ff0000".into()),
            agent_access: false,
            sticky_capture: false,
            retention_days: None,
        };

        store
            .create_workspace(&workspace)
            .expect("first create should succeed");

        let error = store
            .create_workspace(&workspace)
            .expect_err("second create should fail");

        assert!(matches!(error, BlipError::WorkspaceAlreadyExists(name) if name == "demo"));
    }

    #[test]
    fn rejects_invalid_workspace_and_blip_input() {
        let mut store = BlipStore::in_memory().expect("store should initialize");

        let workspace_error = store
            .create_workspace(&NewWorkspace {
                name: " ".into(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            })
            .expect_err("empty workspace names should be rejected");
        assert!(matches!(
            workspace_error,
            BlipError::InvalidInput {
                field: "workspace.name",
                ..
            }
        ));

        let whitespace_error = store
            .create_workspace(&NewWorkspace {
                name: " demo ".into(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            })
            .expect_err("workspace names should not have surrounding whitespace");
        assert!(matches!(
            whitespace_error,
            BlipError::InvalidInput {
                field: "workspace.name",
                ..
            }
        ));

        let retention_error = store
            .create_workspace(&NewWorkspace {
                name: "invalid-retention".into(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: Some(-1),
            })
            .expect_err("negative retention should be rejected");
        assert!(matches!(
            retention_error,
            BlipError::InvalidInput {
                field: "workspace.retention_days",
                ..
            }
        ));

        let blip_error = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "invalid token estimate".into(),
                token_estimate: Some(-1),
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect_err("negative token estimates should be rejected");
        assert!(matches!(
            blip_error,
            BlipError::InvalidInput {
                field: "blip.token_estimate",
                ..
            }
        ));
    }

    #[test]
    fn concurrent_workspace_creation_returns_created_or_duplicate() {
        let db_path = Arc::new(
            std::env::temp_dir().join(format!("blipcoard-concurrent-{}.db", Uuid::new_v4())),
        );
        let barrier = Arc::new(Barrier::new(12));
        let mut handles = Vec::new();

        for _ in 0..12 {
            let db_path = Arc::clone(&db_path);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let mut store = BlipStore::open(db_path.as_ref())?;
                store.create_workspace(&NewWorkspace {
                    name: "race".into(),
                    description: None,
                    color: None,
                    agent_access: false,
                    sticky_capture: false,
                    retention_days: None,
                })
            }));
        }

        let mut created_count = 0;
        let mut duplicate_count = 0;

        for handle in handles {
            match handle.join().expect("thread should finish") {
                Ok(_) => created_count += 1,
                Err(BlipError::WorkspaceAlreadyExists(name)) if name == "race" => {
                    duplicate_count += 1;
                }
                Err(error) => panic!("unexpected error: {error:?}"),
            }
        }

        assert_eq!(created_count, 1);
        assert_eq!(duplicate_count, 11);

        let _ = std::fs::remove_file(db_path.as_ref());
    }

    #[test]
    fn blip_and_audit_list_queries_are_bounded() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let long_content = format!("{}{}", "a".repeat(90), "tail");

        for index in 0..3 {
            store
                .insert_blip(&NewBlip {
                    workspace_name: "inbox".into(),
                    source_app: None,
                    content_type: ContentType::PlainText,
                    language: None,
                    content: format!("{long_content}-{index}"),
                    token_estimate: None,
                    is_redacted: false,
                    tags: Vec::new(),
                })
                .expect("blip insert should work");
        }

        let full_blips = store
            .list_blips_limited("inbox", 2)
            .expect("limited blip list should work");
        assert_eq!(full_blips.len(), 2);

        let summaries = store
            .list_blip_summaries("inbox", 2)
            .expect("summary list should work");
        assert_eq!(summaries.len(), 2);
        assert!(summaries.iter().all(|summary| summary.preview.len() <= 72));
        assert!(
            summaries
                .iter()
                .all(|summary| !summary.preview.contains("tail"))
        );

        let audit_events = store
            .list_audit_events_limited(1)
            .expect("limited audit list should work");
        assert_eq!(audit_events.len(), 1);

        assert_eq!(sqlite_limit(usize::MAX), MAX_LIST_LIMIT as i64);
    }

    #[test]
    fn blip_insert_rolls_back_when_audit_fails() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&NewWorkspace {
                name: "rollback".into(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");

        store
            .connection()
            .execute("DROP TABLE audit_events", [])
            .expect("test should remove audit table");

        let error = store
            .insert_blip(&NewBlip {
                workspace_name: "rollback".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "should not persist".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect_err("audit failure should abort the write");

        assert!(matches!(error, BlipError::Database(_)));
        let blips = store
            .list_blips("rollback")
            .expect("blip query should still work");
        assert!(blips.is_empty());
    }

    #[test]
    fn invalid_persisted_blip_tags_return_error() {
        let store = BlipStore::in_memory().expect("store should initialize");

        store
            .connection()
            .execute(
                "INSERT INTO blips (
                    id, workspace_name, content_type, content, size_bytes, tags_json, created_at
                 ) VALUES ('bad-tags', 'inbox', 'plain_text', 'broken', 6, 'not-json', ?1)",
                [Utc::now()],
            )
            .expect("test row should insert");

        let error = store
            .get_blip("bad-tags")
            .expect_err("invalid persisted JSON should surface");

        assert!(matches!(error, BlipError::Serialization(_)));
    }

    #[test]
    fn invalid_persisted_payload_kind_returns_error() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "payload kind validation".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should insert");

        store
            .connection()
            .execute("PRAGMA ignore_check_constraints = ON", [])
            .expect("test should bypass check constraints");
        store
            .connection()
            .execute(
                "UPDATE blip_payloads SET payload_kind = 'unsupported' WHERE blip_id = ?1",
                [&blip.id],
            )
            .expect("test payload should corrupt");

        let error = store
            .get_blip_payloads(&blip.id)
            .expect_err("invalid payload kind should fail to decode");

        assert!(matches!(
            error,
            BlipError::InvalidPersistedValue {
                field: "payload_kind",
                ..
            }
        ));
    }

    #[test]
    fn invalid_persisted_payload_metadata_returns_error() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let blip = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "payload metadata validation".into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should insert");

        store
            .connection()
            .execute(
                "UPDATE blip_payloads SET metadata_json = 'not-json' WHERE blip_id = ?1",
                [&blip.id],
            )
            .expect("test payload should corrupt");

        let error = store
            .get_blip_payloads(&blip.id)
            .expect_err("invalid metadata JSON should fail to decode");

        assert!(matches!(error, BlipError::Serialization(_)));
    }

    #[test]
    fn invalid_audit_event_type_returns_an_error() {
        let store = BlipStore::in_memory().expect("store should initialize");
        store
            .connection()
            .execute("PRAGMA ignore_check_constraints = ON", [])
            .expect("test should bypass check constraints");
        store
            .connection()
            .execute(
                "INSERT INTO audit_events (
                    id, actor_type, actor_id, event_type, target_blip_id, target_workspace, details_json, created_at
                 ) VALUES (?1, 'system', NULL, 'unexpected_event', NULL, 'inbox', NULL, ?2)",
                params!["bad-audit", Utc::now()],
            )
            .expect("seed insert should succeed");

        let error = store
            .list_audit_events()
            .expect_err("invalid audit enums should fail to decode");

        assert!(matches!(
            error,
            BlipError::InvalidPersistedValue {
                field: "event_type",
                ..
            }
        ));
    }

    #[test]
    fn missing_inbox_backfill_does_not_emit_schema_initialized_again() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let schema_init_before = store
            .list_audit_events()
            .expect("audit list should work")
            .into_iter()
            .filter(|event| event.event_type == AuditEventType::SchemaInitialized)
            .count();

        store
            .connection()
            .execute("DELETE FROM workspaces WHERE name = 'inbox'", [])
            .expect("inbox delete should succeed");

        store.initialize().expect("reinitialize should succeed");

        let audit_events = store.list_audit_events().expect("audit list should work");
        let schema_init_after = audit_events
            .iter()
            .filter(|event| event.event_type == AuditEventType::SchemaInitialized)
            .count();
        let inbox_created_count = audit_events
            .iter()
            .filter(|event| event.event_type == AuditEventType::WorkspaceCreated)
            .filter(|event| event.target_workspace.as_deref() == Some("inbox"))
            .count();

        assert_eq!(schema_init_before, 1);
        assert_eq!(schema_init_after, 1);
        assert_eq!(inbox_created_count, 1);
    }

    fn store_with_workspace(workspace: &str) -> BlipStore {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&NewWorkspace {
                name: workspace.to_owned(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");
        store
    }

    fn insert_test_blip(store: &mut BlipStore, content: &str) -> Blip {
        store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".into(),
                source_app: Some("Screenshot Tool".into()),
                content_type: ContentType::PlainText,
                language: None,
                content: content.into(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("test blip should insert")
    }

    fn image_payload(bytes: &[u8]) -> NewClipboardPayload {
        NewClipboardPayload {
            kind: PayloadKind::Image,
            mime_type: Some("image/png".into()),
            platform_format: Some("public.png".into()),
            source_app: Some("Screenshot Tool".into()),
            preview_ref: None,
            inline_text: None,
            metadata: serde_json::json!({ "width": 1, "height": 1 }),
            bytes: bytes.to_vec(),
        }
    }

    fn temp_blob_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("blipcoard-{label}-{}", Uuid::new_v4()))
    }

    fn insert_raw_blip(store: &BlipStore, id: &str, workspace: &str, content: &str) {
        store
            .connection()
            .execute(
                "INSERT INTO blips (
                    id, workspace_name, content_type, content, size_bytes, tags_json, created_at
                 ) VALUES (?1, ?2, 'plain_text', ?3, ?4, '[]', '2026-06-21T12:34:56Z')",
                params![
                    id,
                    workspace,
                    content,
                    i64::try_from(content.len()).expect("test content length should fit i64")
                ],
            )
            .expect("raw blip should insert");
    }
}

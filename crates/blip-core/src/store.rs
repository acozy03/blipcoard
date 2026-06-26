use crate::domain::{
    ActorType, AuditEvent, AuditEventType, Blip, BlipMove, BlipSummary, ContentType, NewBlip,
    NewWorkspace, Workspace,
};
use crate::error::{BlipError, is_sqlite_busy_error};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};
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

struct Migration {
    version: i32,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("sql/001_initial.sql"),
}];

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
            "INSERT OR IGNORE INTO app_state(key, value) VALUES('schema_version', '1')",
            [],
        )?;

        if schema_initialized > 0 {
            Self::insert_audit_event(
                &tx,
                ActorType::System,
                None,
                AuditEventType::SchemaInitialized,
                None,
                Some(INBOX_WORKSPACE.to_owned()),
                Some("{\"version\":1}".to_owned()),
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
        let tags_json = serde_json::to_string(&new_blip.tags)?;
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
                new_blip.content_type.as_str(),
                new_blip.language,
                new_blip.content,
                size_bytes,
                new_blip.token_estimate,
                new_blip.is_redacted,
                tags_json,
                created_at,
            ],
        )?;

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

        let mut stmt = self.conn.prepare(
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

    pub fn list_agent_blips(
        &self,
        workspace_name: &str,
        limit: usize,
    ) -> Result<Vec<Blip>, BlipError> {
        let workspace = self
            .get_workspace(workspace_name)?
            .ok_or_else(|| BlipError::WorkspaceNotFound(workspace_name.to_owned()))?;

        if !workspace.agent_access {
            return Err(BlipError::AgentAccessDenied(workspace_name.to_owned()));
        }

        self.list_blips_limited(workspace_name, limit)
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
        assert_eq!(schema_version, 1);
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

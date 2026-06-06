use crate::domain::{
    ActorType, AuditEvent, AuditEventType, Blip, ContentType, NewBlip, NewWorkspace, Workspace,
};
use crate::error::BlipError;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

const INBOX_WORKSPACE: &str = "inbox";

pub struct BlipStore {
    conn: Connection,
}

impl BlipStore {
    pub fn in_memory() -> Result<Self, BlipError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    pub fn open(path: &str) -> Result<Self, BlipError> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn initialize(&self) -> Result<(), BlipError> {
        self.conn.execute_batch(include_str!("sql/schema.sql"))?;

        self.create_workspace_if_missing(&NewWorkspace {
            name: INBOX_WORKSPACE.to_owned(),
            description: Some("Default capture workspace".to_owned()),
            color: Some("#777777".to_owned()),
            agent_access: false,
            sticky_capture: false,
            retention_days: None,
        })?;

        if self.get_active_workspace()?.is_none() {
            self.ensure_active_workspace(INBOX_WORKSPACE)?;
        }

        self.insert_audit_event(
            ActorType::System,
            None,
            AuditEventType::SchemaInitialized,
            None,
            Some(INBOX_WORKSPACE.to_owned()),
            Some("{\"version\":1}".to_owned()),
        )?;

        Ok(())
    }

    pub fn create_workspace(&self, workspace: &NewWorkspace) -> Result<Workspace, BlipError> {
        self.create_workspace_if_missing(workspace)?;
        self.get_workspace(&workspace.name)?
            .ok_or_else(|| BlipError::WorkspaceNotFound(workspace.name.clone()))
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
        self.conn
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

    pub fn set_active_workspace(&self, name: &str) -> Result<(), BlipError> {
        if self.get_workspace(name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(name.to_owned()));
        }

        self.ensure_active_workspace(name)?;
        self.insert_audit_event(
            ActorType::User,
            None,
            AuditEventType::WorkspaceActivated,
            None,
            Some(name.to_owned()),
            None,
        )?;
        Ok(())
    }

    pub fn get_active_workspace(&self) -> Result<Option<String>, BlipError> {
        self.conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'active_workspace'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(BlipError::from)
    }

    pub fn insert_blip(&self, new_blip: &NewBlip) -> Result<Blip, BlipError> {
        if self.get_workspace(&new_blip.workspace_name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(
                new_blip.workspace_name.clone(),
            ));
        }

        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        let tags_json = serde_json::to_string(&new_blip.tags)?;
        let size_bytes = new_blip.content.len() as i64;

        self.conn.execute(
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

        self.insert_audit_event(
            ActorType::System,
            None,
            AuditEventType::BlipIngested,
            Some(id.clone()),
            Some(new_blip.workspace_name.clone()),
            None,
        )?;

        self.get_blip(&id)?
            .ok_or_else(|| BlipError::Database(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn get_blip(&self, id: &str) -> Result<Option<Blip>, BlipError> {
        self.conn
            .query_row(
                "SELECT id, workspace_name, source_app, content_type, language, content, size_bytes,
                        token_estimate, is_redacted, tags_json, created_at
                 FROM blips WHERE id = ?1",
                [id],
                map_blip_row,
            )
            .optional()
            .map_err(BlipError::from)
    }

    pub fn list_blips(&self, workspace_name: &str) -> Result<Vec<Blip>, BlipError> {
        if self.get_workspace(workspace_name)?.is_none() {
            return Err(BlipError::WorkspaceNotFound(workspace_name.to_owned()));
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, workspace_name, source_app, content_type, language, content, size_bytes,
                    token_estimate, is_redacted, tags_json, created_at
             FROM blips
             WHERE workspace_name = ?1
             ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([workspace_name], map_blip_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(BlipError::from)
    }

    fn create_workspace_if_missing(&self, workspace: &NewWorkspace) -> Result<(), BlipError> {
        let affected_rows = self.conn.execute(
            "INSERT OR IGNORE INTO workspaces (
                name, description, color, agent_access, sticky_capture, retention_days, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                workspace.name,
                workspace.description,
                workspace.color,
                workspace.agent_access,
                workspace.sticky_capture,
                workspace.retention_days,
                Utc::now(),
            ],
        )?;

        if affected_rows > 0 {
            self.insert_audit_event(
                ActorType::System,
                None,
                AuditEventType::WorkspaceCreated,
                None,
                Some(workspace.name.clone()),
                None,
            )?;
        }

        Ok(())
    }

    fn ensure_active_workspace(&self, workspace_name: &str) -> Result<(), BlipError> {
        self.conn.execute(
            "INSERT INTO app_state(key, value)
             VALUES('active_workspace', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [workspace_name],
        )?;
        Ok(())
    }

    fn insert_audit_event(
        &self,
        actor_type: ActorType,
        actor_id: Option<String>,
        event_type: AuditEventType,
        target_blip_id: Option<String>,
        target_workspace: Option<String>,
        details_json: Option<String>,
    ) -> Result<(), BlipError> {
        self.conn.execute(
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
        let mut stmt = self.conn.prepare(
            "SELECT id, actor_type, actor_id, event_type, target_blip_id, target_workspace, details_json, created_at
             FROM audit_events ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(AuditEvent {
                id: row.get(0)?,
                actor_type: ActorType::parse(row.get::<_, String>(1)?.as_str()),
                actor_id: row.get(2)?,
                event_type: AuditEventType::parse(row.get::<_, String>(3)?.as_str()),
                target_blip_id: row.get(4)?,
                target_workspace: row.get(5)?,
                details_json: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(BlipError::from)
    }
}

fn map_blip_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Blip> {
    let tags_json: String = row.get(9)?;
    let tags = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(Blip {
        id: row.get(0)?,
        workspace_name: row.get(1)?,
        source_app: row.get(2)?,
        content_type: ContentType::parse(row.get::<_, String>(3)?.as_str()),
        language: row.get(4)?,
        content: row.get(5)?,
        size_bytes: row.get(6)?,
        token_estimate: row.get(7)?,
        is_redacted: row.get(8)?,
        tags,
        created_at: row.get::<_, DateTime<Utc>>(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContentType, NewBlip, NewWorkspace};

    #[test]
    fn initializes_with_inbox_and_active_workspace() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let active = store
            .get_active_workspace()
            .expect("active workspace query should work");
        assert_eq!(active.as_deref(), Some("inbox"));

        let workspaces = store.list_workspaces().expect("list should work");
        assert!(workspaces.iter().any(|ws| ws.name == "inbox"));
    }

    #[test]
    fn creates_workspace_and_blip() {
        let store = BlipStore::in_memory().expect("store should initialize");
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
}

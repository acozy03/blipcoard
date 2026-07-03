use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use blip_sync::{HostedJoinCode, HostedMember, HostedRole, JoinCodeOptions, RateLimitDecision};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    env,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub bind_addr: SocketAddr,
    pub database_path: PathBuf,
}

impl CloudConfig {
    pub fn from_env() -> Self {
        let bind_addr = env::var("BLIPCOARD_CLOUD_BIND")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 8732)));
        let database_path = env::var_os("BLIPCOARD_CLOUD_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("blipcoard-cloud.db"));

        Self {
            bind_addr,
            database_path,
        }
    }
}

pub fn app(store: CloudStore) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route(
            "/v1/workspaces",
            post(create_workspace).get(list_workspaces),
        )
        .route(
            "/v1/workspaces/{workspace_id}/join-codes",
            post(create_join_code),
        )
        .route("/v1/join", post(join_workspace))
        .route(
            "/v1/workspaces/{workspace_id}/blips",
            post(publish_blip).get(list_blips),
        )
        .route("/v1/workspaces/{workspace_id}/events", get(list_events))
        .with_state(store)
}

#[derive(Clone)]
pub struct CloudStore {
    connection: Arc<Mutex<Connection>>,
}

impl CloudStore {
    pub fn open(path: impl AsRef<FsPath>) -> Result<Self, CloudError> {
        let connection = Connection::open(path)?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, CloudError> {
        let connection = Connection::open_in_memory()?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), CloudError> {
        let connection = self.lock()?;
        connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_by_member_id TEXT,
                created_at TEXT NOT NULL,
                retention_days INTEGER,
                default_role TEXT NOT NULL,
                last_sequence INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS members (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                display_name TEXT NOT NULL,
                role TEXT NOT NULL,
                status TEXT NOT NULL,
                joined_at TEXT NOT NULL,
                removed_at TEXT
            );
            CREATE TABLE IF NOT EXISTS join_codes (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                code_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                created_by_member_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                revoked_at TEXT,
                max_uses INTEGER NOT NULL,
                use_count INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS blips (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                local_blip_id TEXT NOT NULL,
                publisher_member_id TEXT NOT NULL REFERENCES members(id),
                content_type TEXT NOT NULL,
                content TEXT NOT NULL,
                preview TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                is_redacted INTEGER NOT NULL,
                tags_json TEXT NOT NULL,
                captured_at TEXT,
                published_at TEXT NOT NULL,
                sequence INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                sequence INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                actor_member_id TEXT,
                target_id TEXT,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(workspace_id, sequence)
            );
            ",
        )?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, CloudError> {
        self.connection
            .lock()
            .map_err(|_| CloudError::StorePoisoned)
    }

    fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
        now: DateTime<Utc>,
    ) -> Result<CreateWorkspaceResponse, CloudError> {
        let connection = self.lock()?;
        let workspace_id = hosted_id("hw");
        let member = HostedMember::new(
            workspace_id.clone(),
            request
                .owner_display_name
                .unwrap_or_else(|| "owner".to_string()),
            HostedRole::Owner,
            now,
        );
        let created_at = now.to_rfc3339();

        connection.execute(
            "INSERT INTO workspaces (id, name, created_by_member_id, created_at, retention_days, default_role, last_sequence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![
                workspace_id,
                request.name,
                member.id,
                created_at,
                request.retention_days,
                HostedRole::Viewer.role_name(),
            ],
        )?;
        insert_member(&connection, &member)?;
        let event = append_event(
            &connection,
            &workspace_id,
            "workspace_created",
            Some(&member.id),
            Some(&workspace_id),
            json!({"name": request.name}),
            now,
        )?;

        Ok(CreateWorkspaceResponse {
            workspace: workspace_by_id(&connection, &workspace_id)?,
            member: MemberSummary::from(member),
            event,
        })
    }

    fn list_workspaces(&self) -> Result<Vec<WorkspaceSummary>, CloudError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, name, created_by_member_id, created_at, retention_days, default_role, last_sequence
             FROM workspaces ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([], workspace_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CloudError::from)
    }

    fn create_join_code(
        &self,
        workspace_id: &str,
        request: CreateJoinCodeRequest,
        now: DateTime<Utc>,
    ) -> Result<CreateJoinCodeResponse, CloudError> {
        let connection = self.lock()?;
        let actor = member_by_id(&connection, &request.created_by_member_id)?;
        if actor.workspace_id != workspace_id || !actor.role.can_create_join_codes() {
            return Err(CloudError::AccessDenied);
        }
        let created = HostedJoinCode::create(
            workspace_id,
            actor.id,
            now,
            JoinCodeOptions {
                role: request.role,
                ttl: Duration::seconds(request.expires_in_seconds.unwrap_or(86_400)),
                max_uses: request.max_uses.unwrap_or(10),
            },
        )?;
        insert_join_code(&connection, &created.join_code)?;
        let event = append_event(
            &connection,
            workspace_id,
            "join_code_created",
            Some(&request.created_by_member_id),
            Some(&created.join_code.id),
            json!({"role": request.role.role_name()}),
            now,
        )?;

        Ok(CreateJoinCodeResponse {
            join_code: JoinCodeSummary::from(created.join_code),
            raw_code: created.raw_code,
            event,
        })
    }

    fn join_workspace(
        &self,
        request: JoinWorkspaceRequest,
        now: DateTime<Utc>,
    ) -> Result<JoinWorkspaceResponse, CloudError> {
        let connection = self.lock()?;
        let mut join_code =
            join_code_by_hash(&connection, &blip_sync::hash_join_code(&request.code))?;
        let redemption = join_code.redeem(
            &request.code,
            request.display_name,
            now,
            RateLimitDecision::Allow,
        )?;
        connection.execute(
            "UPDATE join_codes SET use_count = ?1 WHERE id = ?2",
            params![join_code.use_count, join_code.id],
        )?;
        insert_member(&connection, &redemption.member)?;
        let event = append_event(
            &connection,
            &join_code.workspace_id,
            "member_joined",
            Some(&redemption.member.id),
            Some(&redemption.member.id),
            json!({"role": redemption.member.role.role_name()}),
            now,
        )?;

        Ok(JoinWorkspaceResponse {
            workspace: workspace_by_id(&connection, &join_code.workspace_id)?,
            member: MemberSummary::from(redemption.member),
            event,
        })
    }

    fn publish_blip(
        &self,
        workspace_id: &str,
        request: PublishBlipRequest,
        now: DateTime<Utc>,
    ) -> Result<PublishBlipResponse, CloudError> {
        let connection = self.lock()?;
        let actor = member_by_id(&connection, &request.publisher_member_id)?;
        if actor.workspace_id != workspace_id || !actor.role.can_publish_blips() {
            return Err(CloudError::AccessDenied);
        }
        let blip_id = hosted_id("hb");
        let next_sequence = next_sequence(&connection, workspace_id)?;
        let published_at = now.to_rfc3339();
        let tags_json = serde_json::to_string(&request.tags)?;

        connection.execute(
            "INSERT INTO blips
             (id, workspace_id, local_blip_id, publisher_member_id, content_type, content, preview, size_bytes, is_redacted, tags_json, captured_at, published_at, sequence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                blip_id,
                workspace_id,
                request.local_blip_id,
                actor.id,
                request.content_type,
                request.content,
                request.preview,
                request.size_bytes,
                i64::from(request.is_redacted),
                tags_json,
                request.captured_at.map(|timestamp| timestamp.to_rfc3339()),
                published_at,
                next_sequence,
            ],
        )?;
        connection.execute(
            "UPDATE workspaces SET last_sequence = ?1 WHERE id = ?2",
            params![next_sequence, workspace_id],
        )?;
        let event = insert_event_with_sequence(
            &connection,
            NewEvent {
                workspace_id,
                sequence: next_sequence,
                event_type: "blip_published",
                actor_member_id: Some(&actor.id),
                target_id: Some(&blip_id),
                data: json!({"local_blip_id": request.local_blip_id}),
                created_at: now,
            },
        )?;

        Ok(PublishBlipResponse {
            blip: blip_by_id(&connection, &blip_id)?,
            event,
        })
    }

    fn list_blips(&self, workspace_id: &str) -> Result<Vec<HostedBlipSummary>, CloudError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, workspace_id, local_blip_id, publisher_member_id, content_type, content, preview, size_bytes, is_redacted, tags_json, captured_at, published_at, sequence
             FROM blips WHERE workspace_id = ?1 ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map([workspace_id], blip_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CloudError::from)
    }

    fn list_events(
        &self,
        workspace_id: &str,
        after_sequence: i64,
    ) -> Result<Vec<WorkspaceEvent>, CloudError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, workspace_id, sequence, event_type, actor_member_id, target_id, data_json, created_at
             FROM events WHERE workspace_id = ?1 AND sequence > ?2 ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map(params![workspace_id, after_sequence], event_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CloudError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiEnvelope<T> {
    pub request_id: String,
    pub status: String,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

impl<T> ApiEnvelope<T> {
    fn ok(data: T) -> Self {
        Self {
            request_id: request_id(),
            status: "ok".to_string(),
            data: Some(data),
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub owner_display_name: Option<String>,
    pub retention_days: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspaceResponse {
    pub workspace: WorkspaceSummary,
    pub member: MemberSummary,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub created_by_member_id: String,
    pub created_at: DateTime<Utc>,
    pub retention_days: Option<i64>,
    pub default_role: HostedRole,
    pub last_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberSummary {
    pub id: String,
    pub workspace_id: String,
    pub display_name: String,
    pub role: HostedRole,
    pub status: String,
    pub joined_at: DateTime<Utc>,
}

impl From<HostedMember> for MemberSummary {
    fn from(member: HostedMember) -> Self {
        Self {
            id: member.id,
            workspace_id: member.workspace_id,
            display_name: member.display_name,
            role: member.role,
            status: "active".to_string(),
            joined_at: member.joined_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateJoinCodeRequest {
    pub created_by_member_id: String,
    pub role: HostedRole,
    pub expires_in_seconds: Option<i64>,
    pub max_uses: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateJoinCodeResponse {
    pub join_code: JoinCodeSummary,
    pub raw_code: String,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCodeSummary {
    pub id: String,
    pub workspace_id: String,
    pub role: HostedRole,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub max_uses: u32,
    pub use_count: u32,
}

impl From<HostedJoinCode> for JoinCodeSummary {
    fn from(join_code: HostedJoinCode) -> Self {
        Self {
            id: join_code.id,
            workspace_id: join_code.workspace_id,
            role: join_code.role,
            expires_at: join_code.expires_at,
            revoked_at: join_code.revoked_at,
            max_uses: join_code.max_uses,
            use_count: join_code.use_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinWorkspaceRequest {
    pub code: String,
    pub display_name: String,
    pub device_label: Option<String>,
    pub client_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinWorkspaceResponse {
    pub workspace: WorkspaceSummary,
    pub member: MemberSummary,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishBlipRequest {
    pub publisher_member_id: String,
    pub local_blip_id: String,
    pub content_type: String,
    pub content: String,
    pub preview: String,
    pub size_bytes: i64,
    pub is_redacted: bool,
    pub tags: Vec<String>,
    pub captured_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishBlipResponse {
    pub blip: HostedBlipSummary,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedBlipSummary {
    pub id: String,
    pub workspace_id: String,
    pub local_blip_id: String,
    pub publisher_member_id: String,
    pub content_type: String,
    pub content: String,
    pub preview: String,
    pub size_bytes: i64,
    pub is_redacted: bool,
    pub tags: Vec<String>,
    pub captured_at: Option<DateTime<Utc>>,
    pub published_at: DateTime<Utc>,
    pub sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEvent {
    pub id: String,
    pub workspace_id: String,
    pub sequence: i64,
    pub event_type: String,
    pub actor_member_id: Option<String>,
    pub target_id: Option<String>,
    pub data: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventQuery {
    #[serde(default)]
    pub after_sequence: i64,
}

async fn health() -> Json<ApiEnvelope<HealthResponse>> {
    Json(ApiEnvelope::ok(HealthResponse {
        service: "blip-cloud".to_string(),
        status: "ready".to_string(),
    }))
}

async fn create_workspace(
    State(store): State<CloudStore>,
    Json(request): Json<CreateWorkspaceRequest>,
) -> Result<Json<ApiEnvelope<CreateWorkspaceResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        store.create_workspace(request, Utc::now())?,
    )))
}

async fn list_workspaces(
    State(store): State<CloudStore>,
) -> Result<Json<ApiEnvelope<Vec<WorkspaceSummary>>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(store.list_workspaces()?)))
}

async fn create_join_code(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    Json(request): Json<CreateJoinCodeRequest>,
) -> Result<Json<ApiEnvelope<CreateJoinCodeResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(store.create_join_code(
        &workspace_id,
        request,
        Utc::now(),
    )?)))
}

async fn join_workspace(
    State(store): State<CloudStore>,
    Json(request): Json<JoinWorkspaceRequest>,
) -> Result<Json<ApiEnvelope<JoinWorkspaceResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        store.join_workspace(request, Utc::now())?,
    )))
}

async fn publish_blip(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    Json(request): Json<PublishBlipRequest>,
) -> Result<Json<ApiEnvelope<PublishBlipResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(store.publish_blip(
        &workspace_id,
        request,
        Utc::now(),
    )?)))
}

async fn list_blips(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiEnvelope<Vec<HostedBlipSummary>>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(store.list_blips(&workspace_id)?)))
}

async fn list_events(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Result<Json<ApiEnvelope<Vec<WorkspaceEvent>>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        store.list_events(&workspace_id, query.after_sequence)?,
    )))
}

#[derive(Debug, Error)]
pub enum CloudError {
    #[error("store mutex poisoned")]
    StorePoisoned,
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sync error: {0}")]
    Sync(#[from] blip_sync::HostedSyncError),
    #[error("not found")]
    NotFound,
    #[error("access denied")]
    AccessDenied,
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(String),
}

impl IntoResponse for CloudError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::AccessDenied => (StatusCode::FORBIDDEN, "access_denied"),
            Self::Sync(blip_sync::HostedSyncError::JoinCodeExpired { .. }) => {
                (StatusCode::BAD_REQUEST, "join_code_expired")
            }
            Self::Sync(blip_sync::HostedSyncError::JoinCodeRevoked { .. }) => {
                (StatusCode::BAD_REQUEST, "join_code_revoked")
            }
            Self::Sync(blip_sync::HostedSyncError::RateLimited { .. }) => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited")
            }
            Self::Sync(blip_sync::HostedSyncError::InvalidJoinCode { .. }) => {
                (StatusCode::BAD_REQUEST, "invalid_request")
            }
            Self::Sync(blip_sync::HostedSyncError::JoinCodeExhausted { .. }) => {
                (StatusCode::BAD_REQUEST, "join_code_exhausted")
            }
            Self::Sync(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let body = ApiEnvelope::<Value> {
            request_id: request_id(),
            status: "error".to_string(),
            data: None,
            error: Some(ApiError {
                code: code.to_string(),
                message: self.to_string(),
            }),
        };
        (status, Json(body)).into_response()
    }
}

trait RoleName {
    fn role_name(self) -> &'static str;
}

impl RoleName for HostedRole {
    fn role_name(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}

fn parse_role(value: &str) -> Result<HostedRole, CloudError> {
    match value {
        "owner" => Ok(HostedRole::Owner),
        "editor" => Ok(HostedRole::Editor),
        "viewer" => Ok(HostedRole::Viewer),
        _ => Err(CloudError::NotFound),
    }
}

fn insert_member(connection: &Connection, member: &HostedMember) -> Result<(), CloudError> {
    connection.execute(
        "INSERT INTO members (id, workspace_id, display_name, role, status, joined_at, removed_at)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, NULL)",
        params![
            member.id,
            member.workspace_id,
            member.display_name,
            member.role.role_name(),
            member.joined_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn insert_join_code(connection: &Connection, join_code: &HostedJoinCode) -> Result<(), CloudError> {
    connection.execute(
        "INSERT INTO join_codes
         (id, workspace_id, code_hash, role, created_by_member_id, created_at, expires_at, revoked_at, max_uses, use_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)",
        params![
            join_code.id,
            join_code.workspace_id,
            join_code.code_hash,
            join_code.role.role_name(),
            join_code.created_by_member_id,
            join_code.created_at.to_rfc3339(),
            join_code.expires_at.to_rfc3339(),
            join_code.max_uses,
            join_code.use_count,
        ],
    )?;
    Ok(())
}

fn workspace_by_id(
    connection: &Connection,
    workspace_id: &str,
) -> Result<WorkspaceSummary, CloudError> {
    connection
        .query_row(
            "SELECT id, name, created_by_member_id, created_at, retention_days, default_role, last_sequence
             FROM workspaces WHERE id = ?1",
            [workspace_id],
            workspace_from_row,
        )
        .optional()?
        .ok_or(CloudError::NotFound)
}

fn member_by_id(connection: &Connection, member_id: &str) -> Result<HostedMember, CloudError> {
    connection
        .query_row(
            "SELECT id, workspace_id, display_name, role, status, joined_at, removed_at
             FROM members WHERE id = ?1",
            [member_id],
            |row| {
                Ok(HostedMember {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    display_name: row.get(2)?,
                    role: parse_role_for_row(row.get::<_, String>(3)?.as_str())?,
                    status: blip_sync::HostedMemberStatus::Active,
                    joined_at: parse_time_for_row(row.get::<_, String>(5)?.as_str())?,
                    removed_at: None,
                })
            },
        )
        .optional()?
        .ok_or(CloudError::NotFound)
}

fn join_code_by_hash(
    connection: &Connection,
    code_hash: &str,
) -> Result<HostedJoinCode, CloudError> {
    connection
        .query_row(
            "SELECT id, workspace_id, code_hash, role, created_by_member_id, created_at, expires_at, revoked_at, max_uses, use_count
             FROM join_codes WHERE code_hash = ?1",
            [code_hash],
            |row| {
                let revoked_at = row
                    .get::<_, Option<String>>(7)?
                    .map(|value| parse_time_for_row(&value))
                    .transpose()?;
                Ok(HostedJoinCode {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    code_hash: row.get(2)?,
                    role: parse_role_for_row(row.get::<_, String>(3)?.as_str())?,
                    created_by_member_id: row.get(4)?,
                    created_at: parse_time_for_row(row.get::<_, String>(5)?.as_str())?,
                    expires_at: parse_time_for_row(row.get::<_, String>(6)?.as_str())?,
                    revoked_at,
                    max_uses: row.get(8)?,
                    use_count: row.get(9)?,
                })
            },
        )
        .optional()?
        .ok_or(CloudError::NotFound)
}

fn blip_by_id(connection: &Connection, blip_id: &str) -> Result<HostedBlipSummary, CloudError> {
    connection
        .query_row(
            "SELECT id, workspace_id, local_blip_id, publisher_member_id, content_type, content, preview, size_bytes, is_redacted, tags_json, captured_at, published_at, sequence
             FROM blips WHERE id = ?1",
            [blip_id],
            blip_from_row,
        )
        .optional()?
        .ok_or(CloudError::NotFound)
}

fn workspace_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceSummary> {
    Ok(WorkspaceSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        created_by_member_id: row.get(2)?,
        created_at: parse_time_for_row(row.get::<_, String>(3)?.as_str())?,
        retention_days: row.get(4)?,
        default_role: parse_role_for_row(row.get::<_, String>(5)?.as_str())?,
        last_sequence: row.get(6)?,
    })
}

fn blip_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HostedBlipSummary> {
    let captured_at = row
        .get::<_, Option<String>>(10)?
        .map(|value| parse_time_for_row(&value))
        .transpose()?;
    let tags: Vec<String> = serde_json::from_str(row.get::<_, String>(9)?.as_str())
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    Ok(HostedBlipSummary {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        local_blip_id: row.get(2)?,
        publisher_member_id: row.get(3)?,
        content_type: row.get(4)?,
        content: row.get(5)?,
        preview: row.get(6)?,
        size_bytes: row.get(7)?,
        is_redacted: row.get::<_, i64>(8)? != 0,
        tags,
        captured_at,
        published_at: parse_time_for_row(row.get::<_, String>(11)?.as_str())?,
        sequence: row.get(12)?,
    })
}

fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceEvent> {
    let data: Value = serde_json::from_str(row.get::<_, String>(6)?.as_str())
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    Ok(WorkspaceEvent {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        sequence: row.get(2)?,
        event_type: row.get(3)?,
        actor_member_id: row.get(4)?,
        target_id: row.get(5)?,
        data,
        created_at: parse_time_for_row(row.get::<_, String>(7)?.as_str())?,
    })
}

fn append_event(
    connection: &Connection,
    workspace_id: &str,
    event_type: &str,
    actor_member_id: Option<&str>,
    target_id: Option<&str>,
    data: Value,
    created_at: DateTime<Utc>,
) -> Result<WorkspaceEvent, CloudError> {
    let sequence = next_sequence(connection, workspace_id)?;
    connection.execute(
        "UPDATE workspaces SET last_sequence = ?1 WHERE id = ?2",
        params![sequence, workspace_id],
    )?;
    insert_event_with_sequence(
        connection,
        NewEvent {
            workspace_id,
            sequence,
            event_type,
            actor_member_id,
            target_id,
            data,
            created_at,
        },
    )
}

struct NewEvent<'a> {
    workspace_id: &'a str,
    sequence: i64,
    event_type: &'a str,
    actor_member_id: Option<&'a str>,
    target_id: Option<&'a str>,
    data: Value,
    created_at: DateTime<Utc>,
}

fn insert_event_with_sequence(
    connection: &Connection,
    new_event: NewEvent<'_>,
) -> Result<WorkspaceEvent, CloudError> {
    let event = WorkspaceEvent {
        id: hosted_id("he"),
        workspace_id: new_event.workspace_id.to_string(),
        sequence: new_event.sequence,
        event_type: new_event.event_type.to_string(),
        actor_member_id: new_event.actor_member_id.map(ToOwned::to_owned),
        target_id: new_event.target_id.map(ToOwned::to_owned),
        data: new_event.data,
        created_at: new_event.created_at,
    };
    connection.execute(
        "INSERT INTO events (id, workspace_id, sequence, event_type, actor_member_id, target_id, data_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.id,
            event.workspace_id,
            event.sequence,
            event.event_type,
            event.actor_member_id,
            event.target_id,
            serde_json::to_string(&event.data)?,
            event.created_at.to_rfc3339(),
        ],
    )?;
    Ok(event)
}

fn next_sequence(connection: &Connection, workspace_id: &str) -> Result<i64, CloudError> {
    let current: i64 = connection
        .query_row(
            "SELECT last_sequence FROM workspaces WHERE id = ?1",
            [workspace_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(CloudError::NotFound)?;
    Ok(current + 1)
}

fn parse_time_for_row(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn parse_role_for_row(value: &str) -> rusqlite::Result<HostedRole> {
    parse_role(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn hosted_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn request_id() -> String {
    hosted_id("req")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request},
    };
    use tower::ServiceExt;

    async fn request_json<T: for<'de> Deserialize<'de>>(
        router: Router,
        method: Method,
        uri: &str,
        body: Value,
    ) -> (StatusCode, ApiEnvelope<T>) {
        let response = router
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("request should complete");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let envelope = serde_json::from_slice(&bytes).expect("response should decode");
        (status, envelope)
    }

    #[tokio::test]
    async fn service_creates_workspace_join_code_and_member() {
        let router = app(CloudStore::in_memory().expect("store should open"));
        let (status, created): (StatusCode, ApiEnvelope<CreateWorkspaceResponse>) = request_json(
            router.clone(),
            Method::POST,
            "/v1/workspaces",
            json!({"name": "auth-bug", "owner_display_name": "Adrian", "retention_days": 30}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let created = created.data.expect("workspace data should exist");
        assert_eq!(created.workspace.name, "auth-bug");
        assert_eq!(created.member.role, HostedRole::Owner);

        let (status, code): (StatusCode, ApiEnvelope<CreateJoinCodeResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!("/v1/workspaces/{}/join-codes", created.workspace.id).as_str(),
            json!({
                "created_by_member_id": created.member.id,
                "role": "editor",
                "expires_in_seconds": 3600,
                "max_uses": 2
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let code = code.data.expect("join code should exist");
        assert!(code.raw_code.starts_with("BLIP-"));

        let (status, joined): (StatusCode, ApiEnvelope<JoinWorkspaceResponse>) = request_json(
            router,
            Method::POST,
            "/v1/join",
            json!({"code": code.raw_code, "display_name": "Sam", "client_kind": "desktop"}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let joined = joined.data.expect("joined data should exist");
        assert_eq!(joined.member.role, HostedRole::Editor);
        assert_eq!(joined.workspace.id, created.workspace.id);
    }

    #[tokio::test]
    async fn service_publishes_lists_and_returns_events() {
        let router = app(CloudStore::in_memory().expect("store should open"));
        let (_, created): (StatusCode, ApiEnvelope<CreateWorkspaceResponse>) = request_json(
            router.clone(),
            Method::POST,
            "/v1/workspaces",
            json!({"name": "auth-bug", "owner_display_name": "Adrian"}),
        )
        .await;
        let created = created.data.expect("workspace data should exist");

        let (status, published): (StatusCode, ApiEnvelope<PublishBlipResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!("/v1/workspaces/{}/blips", created.workspace.id).as_str(),
            json!({
                "publisher_member_id": created.member.id,
                "local_blip_id": "dev-inbox-1",
                "content_type": "plain_text",
                "content": "copied stack trace",
                "preview": "copied stack trace",
                "size_bytes": 18,
                "is_redacted": false,
                "tags": ["bug"]
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let published = published.data.expect("published data should exist");
        assert_eq!(published.blip.sequence, 2);

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/v1/workspaces/{}/blips", created.workspace.id))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("list request should complete");
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let listed: ApiEnvelope<Vec<HostedBlipSummary>> =
            serde_json::from_slice(&bytes).expect("list should decode");
        assert_eq!(listed.data.expect("blips should exist").len(), 1);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!(
                        "/v1/workspaces/{}/events?after_sequence=1",
                        created.workspace.id
                    ))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("event request should complete");
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let events: ApiEnvelope<Vec<WorkspaceEvent>> =
            serde_json::from_slice(&bytes).expect("events should decode");
        let events = events.data.expect("events should exist");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "blip_published");
    }
}

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use blip_sync::{
    ApiEnvelope, ApiError, CreateJoinCodeRequest, CreateJoinCodeResponse, CreateWorkspaceRequest,
    CreateWorkspaceResponse, EventQuery, HealthResponse, HostedBlipSummary, HostedDeviceSession,
    HostedJoinCode, HostedMember, HostedMemberStatus, HostedRole, HostedWorkspaceSummary,
    JoinCodeOptions, JoinCodeSummary, JoinWorkspaceRequest, JoinWorkspaceResponse,
    MemberPresenceSummary, MemberQuery, MemberSummary, PresenceHeartbeatRequest,
    PresenceHeartbeatResponse, PublishBlipRequest, PublishBlipResponse, RateLimitDecision,
    RecordAccessEventRequest, RecordAccessEventResponse, UpdateTagsRequest, UpdateTagsResponse,
    WorkspaceEvent,
};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    env,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
};
use thiserror::Error;
use tower_http::cors::CorsLayer;
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
        .route(
            "/v1/workspaces/{workspace_id}/blips/{blip_id}",
            get(get_blip),
        )
        .route(
            "/v1/workspaces/{workspace_id}/blips/{blip_id}/tags",
            post(update_blip_tags),
        )
        .route(
            "/v1/workspaces/{workspace_id}/blips/{blip_id}/access-events",
            post(record_blip_access_event),
        )
        .route("/v1/workspaces/{workspace_id}/events", get(list_events))
        .route(
            "/v1/workspaces/{workspace_id}/presence",
            get(list_presence).post(presence_heartbeat),
        )
        .layer(CorsLayer::permissive())
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
            CREATE TABLE IF NOT EXISTS presence (
                member_id TEXT PRIMARY KEY REFERENCES members(id),
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                last_seen_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS device_sessions (
                token_hash TEXT PRIMARY KEY,
                member_id TEXT NOT NULL REFERENCES members(id),
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                device_label TEXT,
                client_kind TEXT,
                created_at TEXT NOT NULL,
                revoked_at TEXT
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
        let raw_token = hosted_id("hs");
        let session = HostedDeviceSession {
            token: raw_token.clone(),
            member_id: member.id.clone(),
            workspace_id: workspace_id.clone(),
            device_label: Some("workspace owner".to_owned()),
            client_kind: Some("owner".to_owned()),
            created_at: now,
        };
        insert_device_session(&connection, &session, &raw_token)?;
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
            session,
            event,
        })
    }

    fn list_workspaces(&self) -> Result<Vec<HostedWorkspaceSummary>, CloudError> {
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
        let actor = member_for_session(
            &connection,
            workspace_id,
            &request.created_by_member_id,
            &request.session_token,
        )?;
        if !actor.role.can_create_join_codes() {
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
            request.display_name.clone(),
            now,
            RateLimitDecision::Allow,
        )?;
        connection.execute(
            "UPDATE join_codes SET use_count = ?1 WHERE id = ?2",
            params![join_code.use_count, join_code.id],
        )?;
        insert_member(&connection, &redemption.member)?;
        let raw_token = hosted_id("hs");
        let session = HostedDeviceSession {
            token: raw_token.clone(),
            member_id: redemption.member.id.clone(),
            workspace_id: join_code.workspace_id.clone(),
            device_label: request.device_label,
            client_kind: request.client_kind,
            created_at: now,
        };
        insert_device_session(&connection, &session, &raw_token)?;
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
            session,
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
        let actor = member_for_session(
            &connection,
            workspace_id,
            &request.publisher_member_id,
            &request.session_token,
        )?;
        if !actor.role.can_publish_blips() {
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

    fn list_blips(
        &self,
        workspace_id: &str,
        member_id: &str,
        session_token: &str,
    ) -> Result<Vec<HostedBlipSummary>, CloudError> {
        let connection = self.lock()?;
        let member = member_for_session(&connection, workspace_id, member_id, session_token)?;
        if !member.role.can_read_blips() {
            return Err(CloudError::AccessDenied);
        }
        let mut statement = connection.prepare(
            "SELECT id, workspace_id, local_blip_id, publisher_member_id, content_type, content, preview, size_bytes, is_redacted, tags_json, captured_at, published_at, sequence
             FROM blips WHERE workspace_id = ?1 ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map([workspace_id], blip_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CloudError::from)
    }

    fn get_blip(
        &self,
        workspace_id: &str,
        blip_id: &str,
        member_id: &str,
        session_token: &str,
    ) -> Result<HostedBlipSummary, CloudError> {
        let connection = self.lock()?;
        let member = member_for_session(&connection, workspace_id, member_id, session_token)?;
        if !member.role.can_read_blips() {
            return Err(CloudError::AccessDenied);
        }
        let blip = blip_by_id(&connection, blip_id)?;
        if blip.workspace_id != workspace_id {
            return Err(CloudError::NotFound);
        }
        Ok(blip)
    }

    fn list_events(
        &self,
        workspace_id: &str,
        after_sequence: i64,
        member_id: &str,
        session_token: &str,
    ) -> Result<Vec<WorkspaceEvent>, CloudError> {
        let connection = self.lock()?;
        let member = member_for_session(&connection, workspace_id, member_id, session_token)?;
        if !member.role.can_read_blips() {
            return Err(CloudError::AccessDenied);
        }
        let mut statement = connection.prepare(
            "SELECT id, workspace_id, sequence, event_type, actor_member_id, target_id, data_json, created_at
             FROM events WHERE workspace_id = ?1 AND sequence > ?2 ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map(params![workspace_id, after_sequence], event_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CloudError::from)
    }

    fn update_blip_tags(
        &self,
        workspace_id: &str,
        blip_id: &str,
        request: UpdateTagsRequest,
        now: DateTime<Utc>,
    ) -> Result<UpdateTagsResponse, CloudError> {
        let connection = self.lock()?;
        let member = member_for_session(
            &connection,
            workspace_id,
            &request.member_id,
            &request.session_token,
        )?;
        if !member.role.can_update_tags() {
            return Err(CloudError::AccessDenied);
        }
        let existing = blip_by_id(&connection, blip_id)?;
        if existing.workspace_id != workspace_id {
            return Err(CloudError::NotFound);
        }
        let tags_json = serde_json::to_string(&normalized_tags(request.tags))?;
        connection.execute(
            "UPDATE blips SET tags_json = ?1 WHERE id = ?2 AND workspace_id = ?3",
            params![tags_json, blip_id, workspace_id],
        )?;
        let event = append_event(
            &connection,
            workspace_id,
            "blip_tags_updated",
            Some(&member.id),
            Some(blip_id),
            json!({"tags": serde_json::from_str::<Value>(&tags_json)?}),
            now,
        )?;

        Ok(UpdateTagsResponse {
            blip: blip_by_id(&connection, blip_id)?,
            event,
        })
    }

    fn record_blip_access_event(
        &self,
        workspace_id: &str,
        blip_id: &str,
        request: RecordAccessEventRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordAccessEventResponse, CloudError> {
        let connection = self.lock()?;
        let member = member_for_session(
            &connection,
            workspace_id,
            &request.member_id,
            &request.session_token,
        )?;
        if !member.role.can_read_blips() {
            return Err(CloudError::AccessDenied);
        }
        let blip = blip_by_id(&connection, blip_id)?;
        if blip.workspace_id != workspace_id {
            return Err(CloudError::NotFound);
        }
        let event = append_event(
            &connection,
            workspace_id,
            request.action.event_type(),
            Some(&member.id),
            Some(blip_id),
            json!({}),
            now,
        )?;

        Ok(RecordAccessEventResponse { event })
    }

    fn presence_heartbeat(
        &self,
        workspace_id: &str,
        request: PresenceHeartbeatRequest,
        now: DateTime<Utc>,
    ) -> Result<PresenceHeartbeatResponse, CloudError> {
        let connection = self.lock()?;
        let member = member_for_session(
            &connection,
            workspace_id,
            &request.member_id,
            &request.session_token,
        )?;
        if !member.role.can_read_blips() {
            return Err(CloudError::AccessDenied);
        }
        connection.execute(
            "INSERT INTO presence (member_id, workspace_id, last_seen_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(member_id) DO UPDATE SET last_seen_at = excluded.last_seen_at",
            params![member.id, workspace_id, now.to_rfc3339()],
        )?;
        Ok(PresenceHeartbeatResponse {
            members: list_presence_from(&connection, workspace_id)?,
        })
    }

    fn list_presence(
        &self,
        workspace_id: &str,
        member_id: &str,
        session_token: &str,
    ) -> Result<PresenceHeartbeatResponse, CloudError> {
        let connection = self.lock()?;
        let member = member_for_session(&connection, workspace_id, member_id, session_token)?;
        if !member.role.can_read_blips() {
            return Err(CloudError::AccessDenied);
        }
        Ok(PresenceHeartbeatResponse {
            members: list_presence_from(&connection, workspace_id)?,
        })
    }
}

async fn health() -> Json<ApiEnvelope<HealthResponse>> {
    Json(ApiEnvelope::ok(
        request_id(),
        HealthResponse {
            service: "blip-cloud".to_string(),
            status: "ready".to_string(),
        },
    ))
}

async fn create_workspace(
    State(store): State<CloudStore>,
    Json(request): Json<CreateWorkspaceRequest>,
) -> Result<Json<ApiEnvelope<CreateWorkspaceResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.create_workspace(request, Utc::now())?,
    )))
}

async fn list_workspaces(
    State(store): State<CloudStore>,
) -> Result<Json<ApiEnvelope<Vec<HostedWorkspaceSummary>>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.list_workspaces()?,
    )))
}

async fn create_join_code(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    Json(request): Json<CreateJoinCodeRequest>,
) -> Result<Json<ApiEnvelope<CreateJoinCodeResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.create_join_code(&workspace_id, request, Utc::now())?,
    )))
}

async fn join_workspace(
    State(store): State<CloudStore>,
    Json(request): Json<JoinWorkspaceRequest>,
) -> Result<Json<ApiEnvelope<JoinWorkspaceResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.join_workspace(request, Utc::now())?,
    )))
}

async fn publish_blip(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    Json(request): Json<PublishBlipRequest>,
) -> Result<Json<ApiEnvelope<PublishBlipResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.publish_blip(&workspace_id, request, Utc::now())?,
    )))
}

async fn list_blips(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiEnvelope<Vec<HostedBlipSummary>>>, CloudError> {
    let query = member_query_from_headers(&headers)?;
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.list_blips(&workspace_id, &query.member_id, &query.session_token)?,
    )))
}

async fn get_blip(
    State(store): State<CloudStore>,
    Path((workspace_id, blip_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ApiEnvelope<HostedBlipSummary>>, CloudError> {
    let query = member_query_from_headers(&headers)?;
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.get_blip(
            &workspace_id,
            &blip_id,
            &query.member_id,
            &query.session_token,
        )?,
    )))
}

async fn list_events(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<EventQuery>,
) -> Result<Json<ApiEnvelope<Vec<WorkspaceEvent>>>, CloudError> {
    let credential = member_query_from_headers(&headers)?;
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.list_events(
            &workspace_id,
            query.after_sequence,
            &credential.member_id,
            &credential.session_token,
        )?,
    )))
}

async fn update_blip_tags(
    State(store): State<CloudStore>,
    Path((workspace_id, blip_id)): Path<(String, String)>,
    Json(request): Json<UpdateTagsRequest>,
) -> Result<Json<ApiEnvelope<UpdateTagsResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.update_blip_tags(&workspace_id, &blip_id, request, Utc::now())?,
    )))
}

async fn record_blip_access_event(
    State(store): State<CloudStore>,
    Path((workspace_id, blip_id)): Path<(String, String)>,
    Json(request): Json<RecordAccessEventRequest>,
) -> Result<Json<ApiEnvelope<RecordAccessEventResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.record_blip_access_event(&workspace_id, &blip_id, request, Utc::now())?,
    )))
}

async fn presence_heartbeat(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    Json(request): Json<PresenceHeartbeatRequest>,
) -> Result<Json<ApiEnvelope<PresenceHeartbeatResponse>>, CloudError> {
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.presence_heartbeat(&workspace_id, request, Utc::now())?,
    )))
}

async fn list_presence(
    State(store): State<CloudStore>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiEnvelope<PresenceHeartbeatResponse>>, CloudError> {
    let query = member_query_from_headers(&headers)?;
    Ok(Json(ApiEnvelope::ok(
        request_id(),
        store.list_presence(&workspace_id, &query.member_id, &query.session_token)?,
    )))
}

fn member_query_from_headers(headers: &HeaderMap) -> Result<MemberQuery, CloudError> {
    let member_id = headers
        .get("x-blip-member-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or(CloudError::AccessDenied)?
        .to_owned();
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or(CloudError::AccessDenied)?;
    let session_token = authorization
        .strip_prefix("Bearer ")
        .filter(|value| !value.trim().is_empty())
        .ok_or(CloudError::AccessDenied)?
        .to_owned();

    Ok(MemberQuery {
        member_id,
        session_token,
    })
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

fn insert_device_session(
    connection: &Connection,
    session: &HostedDeviceSession,
    raw_token: &str,
) -> Result<(), CloudError> {
    connection.execute(
        "INSERT INTO device_sessions
         (token_hash, member_id, workspace_id, device_label, client_kind, created_at, revoked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
        params![
            blip_sync::hash_join_code(raw_token),
            session.member_id,
            session.workspace_id,
            session.device_label,
            session.client_kind,
            session.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn workspace_by_id(
    connection: &Connection,
    workspace_id: &str,
) -> Result<HostedWorkspaceSummary, CloudError> {
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

fn member_for_session(
    connection: &Connection,
    workspace_id: &str,
    member_id: &str,
    session_token: &str,
) -> Result<HostedMember, CloudError> {
    let member = member_by_id(connection, member_id)?;
    if member.workspace_id != workspace_id {
        return Err(CloudError::AccessDenied);
    }
    if member.status != HostedMemberStatus::Active {
        return Err(CloudError::AccessDenied);
    }
    let session_exists = connection
        .query_row(
            "SELECT 1 FROM device_sessions
             WHERE token_hash = ?1 AND member_id = ?2 AND workspace_id = ?3 AND revoked_at IS NULL",
            params![
                blip_sync::hash_join_code(session_token),
                member_id,
                workspace_id
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !session_exists {
        return Err(CloudError::AccessDenied);
    }
    Ok(member)
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

fn workspace_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HostedWorkspaceSummary> {
    Ok(HostedWorkspaceSummary {
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

fn list_presence_from(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<MemberPresenceSummary>, CloudError> {
    let mut statement = connection.prepare(
        "SELECT p.member_id, m.display_name, m.role, p.last_seen_at
         FROM presence p
         JOIN members m ON m.id = p.member_id
         WHERE p.workspace_id = ?1 AND m.status = 'active'
         ORDER BY p.last_seen_at DESC",
    )?;
    let rows = statement.query_map([workspace_id], |row| {
        Ok(MemberPresenceSummary {
            member_id: row.get(0)?,
            display_name: row.get(1)?,
            role: parse_role_for_row(row.get::<_, String>(2)?.as_str())?,
            last_seen_at: parse_time_for_row(row.get::<_, String>(3)?.as_str())?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(CloudError::from)
}

fn normalized_tags(tags: Vec<String>) -> Vec<String> {
    let mut normalized = tags
        .into_iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
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
    use serde::Deserialize;
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
        assert_eq!(created.session.member_id, created.member.id);
        assert!(!created.session.token.is_empty());

        let (status, _code): (StatusCode, ApiEnvelope<CreateJoinCodeResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!("/v1/workspaces/{}/join-codes", created.workspace.id).as_str(),
            json!({
                "created_by_member_id": created.member.id,
                "session_token": "wrong-session",
                "role": "editor",
                "expires_in_seconds": 3600,
                "max_uses": 2
            }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, code): (StatusCode, ApiEnvelope<CreateJoinCodeResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!("/v1/workspaces/{}/join-codes", created.workspace.id).as_str(),
            json!({
                "created_by_member_id": created.member.id,
                "session_token": created.session.token,
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
        assert_eq!(joined.session.member_id, joined.member.id);
        assert!(!joined.session.token.is_empty());
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
        let (_, code): (StatusCode, ApiEnvelope<CreateJoinCodeResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!("/v1/workspaces/{}/join-codes", created.workspace.id).as_str(),
            json!({
                "created_by_member_id": created.member.id,
                "session_token": created.session.token,
                "role": "editor",
                "expires_in_seconds": 3600,
                "max_uses": 2
            }),
        )
        .await;
        let code = code.data.expect("join code should exist");
        let (_, joined): (StatusCode, ApiEnvelope<JoinWorkspaceResponse>) = request_json(
            router.clone(),
            Method::POST,
            "/v1/join",
            json!({"code": code.raw_code, "display_name": "Web", "device_label": "browser", "client_kind": "web"}),
        )
        .await;
        let joined = joined.data.expect("joined member should exist");

        let (status, _published): (StatusCode, ApiEnvelope<PublishBlipResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!("/v1/workspaces/{}/blips", created.workspace.id).as_str(),
            json!({
                "publisher_member_id": created.member.id,
                "session_token": "wrong-session",
                "local_blip_id": "dev-inbox-unauthorized",
                "content_type": "plain_text",
                "content": "copied stack trace",
                "preview": "copied stack trace",
                "size_bytes": 18,
                "is_redacted": false,
                "tags": ["bug"]
            }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, published): (StatusCode, ApiEnvelope<PublishBlipResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!("/v1/workspaces/{}/blips", created.workspace.id).as_str(),
            json!({
                "publisher_member_id": created.member.id,
                "session_token": created.session.token,
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
        assert!(published.blip.sequence > 1);

        let unauthorized = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/v1/workspaces/{}/blips", created.workspace.id))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("unauthorized list request should complete");
        assert_eq!(unauthorized.status(), StatusCode::FORBIDDEN);

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/v1/workspaces/{}/blips", created.workspace.id))
                    .header("x-blip-member-id", joined.member.id.as_str())
                    .header("authorization", format!("Bearer {}", joined.session.token))
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
        let blip_id = published.blip.id.clone();

        let (status, _detail): (StatusCode, ApiEnvelope<HostedBlipSummary>) = request_json(
            router.clone(),
            Method::GET,
            format!("/v1/workspaces/{}/blips/{}", created.workspace.id, blip_id).as_str(),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!(
                        "/v1/workspaces/{}/blips/{}",
                        created.workspace.id, blip_id
                    ))
                    .header("x-blip-member-id", joined.member.id.as_str())
                    .header("authorization", format!("Bearer {}", joined.session.token))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("detail request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let detail: ApiEnvelope<HostedBlipSummary> =
            serde_json::from_slice(&bytes).expect("detail should decode");
        assert_eq!(detail.data.expect("detail should exist").id, blip_id);

        let (status, tags): (StatusCode, ApiEnvelope<UpdateTagsResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!(
                "/v1/workspaces/{}/blips/{}/tags",
                created.workspace.id, blip_id
            )
            .as_str(),
            json!({
                "member_id": joined.member.id,
                "session_token": joined.session.token,
                "tags": [" Bug ", "bug", "ui"]
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            tags.data.expect("tags response should exist").blip.tags,
            vec!["bug".to_string(), "ui".to_string()]
        );

        let (status, _tags): (StatusCode, ApiEnvelope<UpdateTagsResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!(
                "/v1/workspaces/{}/blips/{}/tags",
                created.workspace.id, blip_id
            )
            .as_str(),
            json!({
                "member_id": joined.member.id,
                "session_token": "wrong-session",
                "tags": ["nope"]
            }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, access): (StatusCode, ApiEnvelope<RecordAccessEventResponse>) = request_json(
            router.clone(),
            Method::POST,
            format!(
                "/v1/workspaces/{}/blips/{}/access-events",
                created.workspace.id, blip_id
            )
            .as_str(),
            json!({
                "member_id": joined.member.id,
                "session_token": joined.session.token,
                "action": "copy"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            access
                .data
                .expect("access event should exist")
                .event
                .event_type,
            "blip_copied"
        );

        let (status, presence): (StatusCode, ApiEnvelope<PresenceHeartbeatResponse>) =
            request_json(
                router.clone(),
                Method::POST,
                format!("/v1/workspaces/{}/presence", created.workspace.id).as_str(),
                json!({
                    "member_id": joined.member.id,
                    "session_token": joined.session.token
                }),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            presence.data.expect("presence should exist").members.len(),
            1
        );

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!(
                        "/v1/workspaces/{}/events?after_sequence=1",
                        created.workspace.id
                    ))
                    .header("x-blip-member-id", joined.member.id.as_str())
                    .header("authorization", format!("Bearer {}", joined.session.token))
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
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "blip_published")
        );
        assert!(!events.iter().any(|event| event.event_type == "blip_read"));
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "blip_tags_updated")
        );
        assert!(events.iter().any(|event| event.event_type == "blip_copied"));
    }
}

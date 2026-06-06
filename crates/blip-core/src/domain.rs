use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::BlipError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    PlainText,
    Code,
    Diff,
    Json,
    Url,
    StackTrace,
    Log,
    Unknown,
}

impl ContentType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlainText => "plain_text",
            Self::Code => "code",
            Self::Diff => "diff",
            Self::Json => "json",
            Self::Url => "url",
            Self::StackTrace => "stack_trace",
            Self::Log => "log",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BlipError> {
        match value {
            "plain_text" => Ok(Self::PlainText),
            "code" => Ok(Self::Code),
            "diff" => Ok(Self::Diff),
            "json" => Ok(Self::Json),
            "url" => Ok(Self::Url),
            "stack_trace" => Ok(Self::StackTrace),
            "log" => Ok(Self::Log),
            "unknown" => Ok(Self::Unknown),
            _ => Err(BlipError::InvalidPersistedValue {
                field: "content_type",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blip {
    pub id: String,
    pub workspace_name: String,
    pub source_app: Option<String>,
    pub content_type: ContentType,
    pub language: Option<String>,
    pub content: String,
    pub size_bytes: i64,
    pub token_estimate: Option<i64>,
    pub is_redacted: bool,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlipSummary {
    pub id: String,
    pub workspace_name: String,
    pub source_app: Option<String>,
    pub content_type: ContentType,
    pub language: Option<String>,
    pub preview: String,
    pub size_bytes: i64,
    pub token_estimate: Option<i64>,
    pub is_redacted: bool,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewBlip {
    pub workspace_name: String,
    pub source_app: Option<String>,
    pub content_type: ContentType,
    pub language: Option<String>,
    pub content: String,
    pub token_estimate: Option<i64>,
    pub is_redacted: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub agent_access: bool,
    pub sticky_capture: bool,
    pub retention_days: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewWorkspace {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub agent_access: bool,
    pub sticky_capture: bool,
    pub retention_days: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorType {
    System,
    User,
    Agent,
}

impl ActorType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Agent => "agent",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BlipError> {
        match value {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "agent" => Ok(Self::Agent),
            _ => Err(BlipError::InvalidPersistedValue {
                field: "actor_type",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    SchemaInitialized,
    WorkspaceCreated,
    WorkspaceActivated,
    BlipIngested,
    BlipMoved,
}

impl AuditEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SchemaInitialized => "schema_initialized",
            Self::WorkspaceCreated => "workspace_created",
            Self::WorkspaceActivated => "workspace_activated",
            Self::BlipIngested => "blip_ingested",
            Self::BlipMoved => "blip_moved",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BlipError> {
        match value {
            "schema_initialized" => Ok(Self::SchemaInitialized),
            "workspace_created" => Ok(Self::WorkspaceCreated),
            "workspace_activated" => Ok(Self::WorkspaceActivated),
            "blip_ingested" => Ok(Self::BlipIngested),
            "blip_moved" => Ok(Self::BlipMoved),
            _ => Err(BlipError::InvalidPersistedValue {
                field: "event_type",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub actor_type: ActorType,
    pub actor_id: Option<String>,
    pub event_type: AuditEventType,
    pub target_blip_id: Option<String>,
    pub target_workspace: Option<String>,
    pub details_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

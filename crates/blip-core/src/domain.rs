use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlipTypeFilter {
    Text,
    Image,
    FileList,
    RichText,
    Unknown,
}

impl BlipTypeFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::FileList => "file_list",
            Self::RichText => "rich_text",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BlipListFilter {
    pub all_workspaces: bool,
    pub created_at_from: Option<DateTime<Utc>>,
    pub created_at_before: Option<DateTime<Utc>>,
    pub blip_types: Vec<BlipTypeFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlipSummaryPage {
    pub blips: Vec<BlipSummary>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlipMove {
    pub id: String,
    pub from_workspace: String,
    pub to_workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionCleanupReport {
    pub deleted_blips: usize,
    pub removed_blob_refs: Vec<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    Text,
    Image,
    FileList,
    Html,
    Rtf,
    Unknown,
}

impl PayloadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::FileList => "file_list",
            Self::Html => "html",
            Self::Rtf => "rtf",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BlipError> {
        match value {
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            "file_list" => Ok(Self::FileList),
            "html" => Ok(Self::Html),
            "rtf" => Ok(Self::Rtf),
            "unknown" => Ok(Self::Unknown),
            _ => Err(BlipError::InvalidPersistedValue {
                field: "payload_kind",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub id: String,
    pub blip_id: String,
    pub kind: PayloadKind,
    pub mime_type: Option<String>,
    pub platform_format: Option<String>,
    pub byte_size: i64,
    pub content_hash: Option<String>,
    pub source_app: Option<String>,
    pub captured_at: DateTime<Utc>,
    pub preview_ref: Option<String>,
    pub blob_ref: Option<String>,
    pub inline_text: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardPayloadSummary {
    pub id: String,
    pub blip_id: String,
    pub kind: PayloadKind,
    pub mime_type: Option<String>,
    pub platform_format: Option<String>,
    pub byte_size: i64,
    pub preview_ref: Option<String>,
    pub blob_ref: Option<String>,
    pub inline_text_preview: Option<String>,
    pub has_inline_text: bool,
    pub metadata_summary: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewClipboardPayload {
    pub kind: PayloadKind,
    pub mime_type: Option<String>,
    pub platform_format: Option<String>,
    pub source_app: Option<String>,
    pub preview_ref: Option<String>,
    pub inline_text: Option<String>,
    pub metadata: Value,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewClipboardMetadataPayload {
    pub kind: PayloadKind,
    pub mime_type: Option<String>,
    pub platform_format: Option<String>,
    pub source_app: Option<String>,
    pub preview_ref: Option<String>,
    pub inline_text: Option<String>,
    pub metadata: Value,
    pub byte_size: i64,
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

impl Workspace {
    pub fn require_agent_read_access(&self) -> Result<(), BlipError> {
        if self.agent_access {
            Ok(())
        } else {
            Err(BlipError::AgentAccessDenied(self.name.clone()))
        }
    }
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
#[serde(rename_all = "snake_case")]
pub enum RichPayloadVisibility {
    Hidden,
    Metadata,
    SafePreview,
}

impl RichPayloadVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Metadata => "metadata",
            Self::SafePreview => "safe_preview",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BlipError> {
        match value {
            "hidden" => Ok(Self::Hidden),
            "metadata" => Ok(Self::Metadata),
            "safe_preview" => Ok(Self::SafePreview),
            _ => Err(BlipError::InvalidPersistedValue {
                field: "rich_payload_visibility",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePolicy {
    pub workspace_name: String,
    pub rich_capture_enabled: bool,
    pub image_capture_enabled: bool,
    pub rich_payload_visibility: RichPayloadVisibility,
    pub agent_raw_payload_access: bool,
}

impl WorkspacePolicy {
    pub fn default_for_workspace(workspace_name: impl Into<String>) -> Self {
        Self {
            workspace_name: workspace_name.into(),
            rich_capture_enabled: true,
            image_capture_enabled: true,
            rich_payload_visibility: RichPayloadVisibility::SafePreview,
            agent_raw_payload_access: false,
        }
    }

    pub fn allows_capture(&self, kind: PayloadKind) -> bool {
        match kind {
            PayloadKind::Text => true,
            PayloadKind::Image => self.rich_capture_enabled && self.image_capture_enabled,
            PayloadKind::FileList | PayloadKind::Html | PayloadKind::Rtf | PayloadKind::Unknown => {
                self.rich_capture_enabled
            }
        }
    }
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
    StickyCaptureChanged,
    WorkspacePolicyChanged,
    BlipIngested,
    BlipRecopied,
    BlipMoved,
    BlipsRead,
    RichPayloadCaptureSkipped,
    PayloadPreviewRead,
    PayloadRawExported,
    DesktopPayloadOpened,
    AgentPayloadRead,
    HostedWorkspaceJoined,
    HostedBlipPublished,
    HostedStickyShareChanged,
}

impl AuditEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SchemaInitialized => "schema_initialized",
            Self::WorkspaceCreated => "workspace_created",
            Self::WorkspaceActivated => "workspace_activated",
            Self::StickyCaptureChanged => "sticky_capture_changed",
            Self::WorkspacePolicyChanged => "workspace_policy_changed",
            Self::BlipIngested => "blip_ingested",
            Self::BlipRecopied => "blip_recopied",
            Self::BlipMoved => "blip_moved",
            Self::BlipsRead => "blips_read",
            Self::RichPayloadCaptureSkipped => "rich_payload_capture_skipped",
            Self::PayloadPreviewRead => "payload_preview_read",
            Self::PayloadRawExported => "payload_raw_exported",
            Self::DesktopPayloadOpened => "desktop_payload_opened",
            Self::AgentPayloadRead => "agent_payload_read",
            Self::HostedWorkspaceJoined => "hosted_workspace_joined",
            Self::HostedBlipPublished => "hosted_blip_published",
            Self::HostedStickyShareChanged => "hosted_sticky_share_changed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BlipError> {
        match value {
            "schema_initialized" => Ok(Self::SchemaInitialized),
            "workspace_created" => Ok(Self::WorkspaceCreated),
            "workspace_activated" => Ok(Self::WorkspaceActivated),
            "sticky_capture_changed" => Ok(Self::StickyCaptureChanged),
            "workspace_policy_changed" => Ok(Self::WorkspacePolicyChanged),
            "blip_ingested" => Ok(Self::BlipIngested),
            "blip_recopied" => Ok(Self::BlipRecopied),
            "blip_moved" => Ok(Self::BlipMoved),
            "blips_read" => Ok(Self::BlipsRead),
            "rich_payload_capture_skipped" => Ok(Self::RichPayloadCaptureSkipped),
            "payload_preview_read" => Ok(Self::PayloadPreviewRead),
            "payload_raw_exported" => Ok(Self::PayloadRawExported),
            "desktop_payload_opened" => Ok(Self::DesktopPayloadOpened),
            "agent_payload_read" => Ok(Self::AgentPayloadRead),
            "hosted_workspace_joined" => Ok(Self::HostedWorkspaceJoined),
            "hosted_blip_published" => Ok(Self::HostedBlipPublished),
            "hosted_sticky_share_changed" => Ok(Self::HostedStickyShareChanged),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadAccessAudit {
    pub actor_type: ActorType,
    pub actor_id: Option<String>,
    pub event_type: AuditEventType,
    pub target_blip_id: Option<String>,
    pub target_workspace: Option<String>,
    pub payload_id: String,
    pub payload_kind: PayloadKind,
    pub access_mode: String,
    pub decision: String,
}

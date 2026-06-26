use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DAEMON_API_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
    pub database_path: String,
    pub active_workspace: Option<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonRequest {
    pub api_version: u16,
    pub request_id: String,
    pub command: DaemonCommand,
    pub payload: DaemonRequestPayload,
}

impl DaemonRequest {
    pub fn new(
        request_id: impl Into<String>,
        command: DaemonCommand,
        payload: DaemonRequestPayload,
    ) -> Self {
        Self {
            api_version: DAEMON_API_VERSION,
            request_id: request_id.into(),
            command,
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonCommand {
    Health,
    Version,
    CurrentWorkspace,
    ActivateWorkspace,
    SetStickyCapture,
    ListWorkspaces,
    ListBlips,
    SearchBlips,
    GetBlip,
    ListAuditEvents,
    RouteBlip,
    RouteLatestInboxBlip,
    AgentRecentBlips,
    AgentSearchBlips,
}

impl DaemonCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Version => "version",
            Self::CurrentWorkspace => "current_workspace",
            Self::ActivateWorkspace => "activate_workspace",
            Self::SetStickyCapture => "set_sticky_capture",
            Self::ListWorkspaces => "list_workspaces",
            Self::ListBlips => "list_blips",
            Self::SearchBlips => "search_blips",
            Self::GetBlip => "get_blip",
            Self::ListAuditEvents => "list_audit_events",
            Self::RouteBlip => "route_blip",
            Self::RouteLatestInboxBlip => "route_latest_inbox_blip",
            Self::AgentRecentBlips => "agent_recent_blips",
            Self::AgentSearchBlips => "agent_search_blips",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonRequestPayload {
    Health,
    Version,
    CurrentWorkspace,
    ActivateWorkspace {
        workspace: String,
    },
    SetStickyCapture {
        workspace: String,
        enabled: bool,
    },
    ListWorkspaces,
    ListBlips {
        workspace: String,
        limit: usize,
    },
    SearchBlips {
        workspace: String,
        query: String,
        limit: usize,
    },
    GetBlip {
        blip_id: String,
    },
    ListAuditEvents {
        limit: usize,
    },
    RouteBlip {
        blip_id: String,
        workspace: String,
    },
    RouteLatestInboxBlip {
        workspace: String,
    },
    AgentRecentBlips {
        workspace: String,
        limit: usize,
    },
    AgentSearchBlips {
        workspace: String,
        query: String,
        limit: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub api_version: u16,
    pub request_id: String,
    pub command: DaemonCommand,
    pub status: DaemonResponseStatus,
    pub payload: Option<DaemonResponsePayload>,
    pub error: Option<DaemonApiError>,
}

impl DaemonResponse {
    pub fn ok(
        request_id: impl Into<String>,
        command: DaemonCommand,
        payload: DaemonResponsePayload,
    ) -> Self {
        Self {
            api_version: DAEMON_API_VERSION,
            request_id: request_id.into(),
            command,
            status: DaemonResponseStatus::Ok,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn error(
        request_id: impl Into<String>,
        command: DaemonCommand,
        error: DaemonApiError,
    ) -> Self {
        Self {
            api_version: DAEMON_API_VERSION,
            request_id: request_id.into(),
            command,
            status: DaemonResponseStatus::Error,
            payload: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonResponseStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonResponsePayload {
    Health(HealthResponse),
    Version(DaemonVersionResponse),
    CurrentWorkspace(CurrentWorkspaceResponse),
    WorkspaceActivated(CurrentWorkspaceResponse),
    StickyCaptureSet(WorkspaceSummary),
    Workspaces(WorkspaceListResponse),
    Blips(BlipListResponse),
    Blip(BlipDetail),
    AuditEvents(AuditEventListResponse),
    AgentBlips(AgentBlipListResponse),
    BlipRouted(BlipRoutedResponse),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonVersionResponse {
    pub api_version: u16,
    pub daemon_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentWorkspaceResponse {
    pub active_workspace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceListResponse {
    pub workspaces: Vec<WorkspaceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub name: String,
    pub agent_access: bool,
    pub sticky_capture: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlipListResponse {
    pub workspace: String,
    pub blips: Vec<BlipSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlipSummary {
    pub id: String,
    pub preview: String,
    pub size_bytes: i64,
    #[serde(default)]
    pub is_redacted: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlipDetail {
    pub id: String,
    pub workspace: String,
    pub source_app: Option<String>,
    pub content_type: String,
    pub language: Option<String>,
    pub content: String,
    pub size_bytes: i64,
    pub token_estimate: Option<i64>,
    pub is_redacted: bool,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEventListResponse {
    pub events: Vec<AuditEventSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEventSummary {
    pub id: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub target_blip_id: Option<String>,
    pub target_workspace: Option<String>,
    pub details_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBlipListResponse {
    pub workspace: String,
    pub blips: Vec<AgentBlip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBlip {
    pub id: String,
    pub content: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlipRoutedResponse {
    pub id: String,
    pub from_workspace: String,
    pub to_workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonApiError {
    pub code: DaemonApiErrorCode,
    pub message: String,
}

impl DaemonApiError {
    pub fn new(code: DaemonApiErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonApiErrorCode {
    UnsupportedApiVersion,
    InvalidRequest,
    NotFound,
    AccessDenied,
    StoreUnavailable,
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde_json::json;

    #[test]
    fn health_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-1",
            DaemonCommand::Health,
            DaemonRequestPayload::Health,
        );

        let value = serde_json::to_value(&request).expect("health request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-1",
                "command": "health",
                "payload": "health",
            })
        );

        let decoded =
            serde_json::from_value::<DaemonRequest>(value).expect("health request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn health_response_json_shape_round_trips() {
        let response = DaemonResponse::ok(
            "request-1",
            DaemonCommand::Health,
            DaemonResponsePayload::Health(HealthResponse {
                service: "blipd".to_owned(),
                status: "ready".to_owned(),
                database_path: "/tmp/blipcoard.db".to_owned(),
                active_workspace: Some("inbox".to_owned()),
                generated_at: fixed_generated_at(),
            }),
        );

        let value = serde_json::to_value(&response).expect("health response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-1",
                "command": "health",
                "status": "ok",
                "payload": {
                    "health": {
                        "service": "blipd",
                        "status": "ready",
                        "database_path": "/tmp/blipcoard.db",
                        "active_workspace": "inbox",
                        "generated_at": "2026-06-21T12:34:56Z",
                    }
                },
                "error": null,
            })
        );

        let decoded =
            serde_json::from_value::<DaemonResponse>(value).expect("health response should decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn version_response_json_shape_round_trips() {
        let response = DaemonResponse::ok(
            "request-2",
            DaemonCommand::Version,
            DaemonResponsePayload::Version(DaemonVersionResponse {
                api_version: 1,
                daemon_version: "0.1.0".to_owned(),
            }),
        );

        let value = serde_json::to_value(&response).expect("version response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-2",
                "command": "version",
                "status": "ok",
                "payload": {
                    "version": {
                        "api_version": 1,
                        "daemon_version": "0.1.0",
                    }
                },
                "error": null,
            })
        );

        let decoded = serde_json::from_value::<DaemonResponse>(value)
            .expect("version response should decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn activate_workspace_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-activate",
            DaemonCommand::ActivateWorkspace,
            DaemonRequestPayload::ActivateWorkspace {
                workspace: "auth-bug".to_owned(),
            },
        );

        let value = serde_json::to_value(&request).expect("activate request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-activate",
                "command": "activate_workspace",
                "payload": {
                    "activate_workspace": {
                        "workspace": "auth-bug",
                    }
                },
            })
        );

        let decoded =
            serde_json::from_value::<DaemonRequest>(value).expect("activate request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn workspace_activated_response_json_shape_round_trips() {
        let response = DaemonResponse::ok(
            "request-activate",
            DaemonCommand::ActivateWorkspace,
            DaemonResponsePayload::WorkspaceActivated(CurrentWorkspaceResponse {
                active_workspace: Some("auth-bug".to_owned()),
            }),
        );

        let value = serde_json::to_value(&response).expect("activate response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-activate",
                "command": "activate_workspace",
                "status": "ok",
                "payload": {
                    "workspace_activated": {
                        "active_workspace": "auth-bug",
                    }
                },
                "error": null,
            })
        );

        let decoded = serde_json::from_value::<DaemonResponse>(value)
            .expect("activate response should decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn route_latest_inbox_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-route-latest",
            DaemonCommand::RouteLatestInboxBlip,
            DaemonRequestPayload::RouteLatestInboxBlip {
                workspace: "auth-bug".to_owned(),
            },
        );

        let value = serde_json::to_value(&request).expect("route latest request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-route-latest",
                "command": "route_latest_inbox_blip",
                "payload": {
                    "route_latest_inbox_blip": {
                        "workspace": "auth-bug",
                    }
                },
            })
        );

        let decoded = serde_json::from_value::<DaemonRequest>(value)
            .expect("route latest request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn search_blips_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-search",
            DaemonCommand::SearchBlips,
            DaemonRequestPayload::SearchBlips {
                workspace: "auth-bug".to_owned(),
                query: "login".to_owned(),
                limit: 10,
            },
        );

        let value = serde_json::to_value(&request).expect("search request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-search",
                "command": "search_blips",
                "payload": {
                    "search_blips": {
                        "workspace": "auth-bug",
                        "query": "login",
                        "limit": 10,
                    }
                },
            })
        );

        let decoded =
            serde_json::from_value::<DaemonRequest>(value).expect("search request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn route_blip_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-route-id",
            DaemonCommand::RouteBlip,
            DaemonRequestPayload::RouteBlip {
                blip_id: "blip-1".to_owned(),
                workspace: "inbox".to_owned(),
            },
        );

        let value = serde_json::to_value(&request).expect("route blip request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-route-id",
                "command": "route_blip",
                "payload": {
                    "route_blip": {
                        "blip_id": "blip-1",
                        "workspace": "inbox",
                    }
                },
            })
        );

        let decoded = serde_json::from_value::<DaemonRequest>(value)
            .expect("route blip request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn blip_routed_response_json_shape_round_trips() {
        let response = DaemonResponse::ok(
            "request-route",
            DaemonCommand::RouteBlip,
            DaemonResponsePayload::BlipRouted(BlipRoutedResponse {
                id: "blip-1".to_owned(),
                from_workspace: "inbox".to_owned(),
                to_workspace: "auth-bug".to_owned(),
            }),
        );

        let value = serde_json::to_value(&response).expect("route response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-route",
                "command": "route_blip",
                "status": "ok",
                "payload": {
                    "blip_routed": {
                        "id": "blip-1",
                        "from_workspace": "inbox",
                        "to_workspace": "auth-bug",
                    }
                },
                "error": null,
            })
        );

        let decoded =
            serde_json::from_value::<DaemonResponse>(value).expect("route response should decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn get_blip_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-get-blip",
            DaemonCommand::GetBlip,
            DaemonRequestPayload::GetBlip {
                blip_id: "blip-1".to_owned(),
            },
        );

        let value = serde_json::to_value(&request).expect("get blip request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-get-blip",
                "command": "get_blip",
                "payload": {
                    "get_blip": {
                        "blip_id": "blip-1",
                    }
                },
            })
        );

        let decoded =
            serde_json::from_value::<DaemonRequest>(value).expect("get blip request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn blip_detail_response_json_shape_round_trips() {
        let created_at =
            DateTime::parse_from_rfc3339("2026-06-26T17:30:00Z").expect("timestamp parses");
        let response = DaemonResponse::ok(
            "request-get-blip",
            DaemonCommand::GetBlip,
            DaemonResponsePayload::Blip(BlipDetail {
                id: "blip-1".to_owned(),
                workspace: "auth-bug".to_owned(),
                source_app: Some("Firefox".to_owned()),
                content_type: "plain_text".to_owned(),
                language: None,
                content: "full workspace content".to_owned(),
                size_bytes: 22,
                token_estimate: Some(4),
                is_redacted: true,
                tags: vec!["demo".to_owned()],
                created_at: created_at.with_timezone(&Utc),
            }),
        );

        let value = serde_json::to_value(&response).expect("blip response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-get-blip",
                "command": "get_blip",
                "status": "ok",
                "payload": {
                    "blip": {
                        "id": "blip-1",
                        "workspace": "auth-bug",
                        "source_app": "Firefox",
                        "content_type": "plain_text",
                        "language": null,
                        "content": "full workspace content",
                        "size_bytes": 22,
                        "token_estimate": 4,
                        "is_redacted": true,
                        "tags": ["demo"],
                        "created_at": "2026-06-26T17:30:00Z",
                    }
                },
                "error": null,
            })
        );

        let decoded =
            serde_json::from_value::<DaemonResponse>(value).expect("blip response should decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn blip_list_response_json_shape_round_trips_with_flags() {
        let response = DaemonResponse::ok(
            "request-list-blips",
            DaemonCommand::ListBlips,
            DaemonResponsePayload::Blips(BlipListResponse {
                workspace: "inbox".to_owned(),
                blips: vec![BlipSummary {
                    id: "blip-1".to_owned(),
                    preview: "api_key = abcdef1234567890".to_owned(),
                    size_bytes: 28,
                    is_redacted: false,
                    tags: vec!["secret".to_owned(), "secret:assignment".to_owned()],
                }],
            }),
        );

        let value = serde_json::to_value(&response).expect("blip list response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-list-blips",
                "command": "list_blips",
                "status": "ok",
                "payload": {
                    "blips": {
                        "workspace": "inbox",
                        "blips": [{
                            "id": "blip-1",
                            "preview": "api_key = abcdef1234567890",
                            "size_bytes": 28,
                            "is_redacted": false,
                            "tags": ["secret", "secret:assignment"],
                        }],
                    }
                },
                "error": null,
            })
        );

        let decoded = serde_json::from_value::<DaemonResponse>(value)
            .expect("blip list response should decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn blip_list_response_decodes_legacy_summaries_without_flags() {
        let value = json!({
            "api_version": 1,
            "request_id": "request-list-blips",
            "command": "list_blips",
            "status": "ok",
            "payload": {
                "blips": {
                    "workspace": "inbox",
                    "blips": [{
                        "id": "blip-1",
                        "preview": "copied text",
                        "size_bytes": 11,
                    }],
                }
            },
            "error": null,
        });

        let decoded =
            serde_json::from_value::<DaemonResponse>(value).expect("legacy response should decode");

        assert_eq!(
            decoded.payload,
            Some(DaemonResponsePayload::Blips(BlipListResponse {
                workspace: "inbox".to_owned(),
                blips: vec![BlipSummary {
                    id: "blip-1".to_owned(),
                    preview: "copied text".to_owned(),
                    size_bytes: 11,
                    is_redacted: false,
                    tags: Vec::new(),
                }],
            }))
        );
    }

    #[test]
    fn agent_recent_blips_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-agent-recent",
            DaemonCommand::AgentRecentBlips,
            DaemonRequestPayload::AgentRecentBlips {
                workspace: "auth-bug".to_owned(),
                limit: 10,
            },
        );

        let value = serde_json::to_value(&request).expect("agent recent request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-agent-recent",
                "command": "agent_recent_blips",
                "payload": {
                    "agent_recent_blips": {
                        "workspace": "auth-bug",
                        "limit": 10,
                    }
                },
            })
        );

        let decoded = serde_json::from_value::<DaemonRequest>(value)
            .expect("agent recent request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn agent_search_blips_request_json_shape_round_trips() {
        let request = DaemonRequest::new(
            "request-agent-search",
            DaemonCommand::AgentSearchBlips,
            DaemonRequestPayload::AgentSearchBlips {
                workspace: "agent-feed".to_owned(),
                query: "deploy".to_owned(),
                limit: 10,
            },
        );

        let value = serde_json::to_value(&request).expect("agent search request should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-agent-search",
                "command": "agent_search_blips",
                "payload": {
                    "agent_search_blips": {
                        "workspace": "agent-feed",
                        "query": "deploy",
                        "limit": 10,
                    }
                },
            })
        );

        let decoded = serde_json::from_value::<DaemonRequest>(value)
            .expect("agent search request should decode");
        assert_eq!(decoded, request);
    }

    #[test]
    fn agent_blips_response_json_shape_round_trips() {
        let response = DaemonResponse::ok(
            "request-agent-recent",
            DaemonCommand::AgentRecentBlips,
            DaemonResponsePayload::AgentBlips(AgentBlipListResponse {
                workspace: "auth-bug".to_owned(),
                blips: vec![AgentBlip {
                    id: "blip-1".to_owned(),
                    content: "full workspace content".to_owned(),
                    size_bytes: 22,
                }],
            }),
        );

        let value = serde_json::to_value(&response).expect("agent blips response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-agent-recent",
                "command": "agent_recent_blips",
                "status": "ok",
                "payload": {
                    "agent_blips": {
                        "workspace": "auth-bug",
                        "blips": [{
                            "id": "blip-1",
                            "content": "full workspace content",
                            "size_bytes": 22,
                        }],
                    }
                },
                "error": null,
            })
        );

        let decoded = serde_json::from_value::<DaemonResponse>(value)
            .expect("agent blips response should decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn error_response_json_shape_round_trips() {
        let response = DaemonResponse::error(
            "request-3",
            DaemonCommand::Health,
            DaemonApiError::new(
                DaemonApiErrorCode::UnsupportedApiVersion,
                "unsupported daemon API version 2; expected 1",
            ),
        );

        let value = serde_json::to_value(&response).expect("error response should serialize");

        assert_eq!(
            value,
            json!({
                "api_version": 1,
                "request_id": "request-3",
                "command": "health",
                "status": "error",
                "payload": null,
                "error": {
                    "code": "unsupported_api_version",
                    "message": "unsupported daemon API version 2; expected 1",
                },
            })
        );

        let decoded =
            serde_json::from_value::<DaemonResponse>(value).expect("error response should decode");
        assert_eq!(decoded, response);
    }

    fn fixed_generated_at() -> DateTime<Utc> {
        "2026-06-21T12:34:56Z"
            .parse()
            .expect("fixed timestamp should parse")
    }
}

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
    ListWorkspaces,
    ListBlips,
    RouteBlip,
    RouteLatestInboxBlip,
}

impl DaemonCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Version => "version",
            Self::CurrentWorkspace => "current_workspace",
            Self::ActivateWorkspace => "activate_workspace",
            Self::ListWorkspaces => "list_workspaces",
            Self::ListBlips => "list_blips",
            Self::RouteBlip => "route_blip",
            Self::RouteLatestInboxBlip => "route_latest_inbox_blip",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonRequestPayload {
    Health,
    Version,
    CurrentWorkspace,
    ActivateWorkspace { workspace: String },
    ListWorkspaces,
    ListBlips { workspace: String, limit: usize },
    RouteBlip { blip_id: String, workspace: String },
    RouteLatestInboxBlip { workspace: String },
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
    Workspaces(WorkspaceListResponse),
    Blips(BlipListResponse),
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

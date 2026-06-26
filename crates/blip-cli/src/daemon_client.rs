use blip_api::{
    AgentBlipListResponse, AuditEventListResponse, BlipDetail, BlipListResponse,
    BlipRoutedResponse, CurrentWorkspaceResponse, DAEMON_API_VERSION, DaemonApiError,
    DaemonCommand, DaemonRequest, DaemonRequestPayload, DaemonResponse, DaemonResponsePayload,
    DaemonResponseStatus, DaemonVersionResponse, HealthResponse, WorkspaceListResponse,
};
use blip_config::{BlipConfig, ConfigError};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DaemonClient {
    socket_path: PathBuf,
}

impl DaemonClient {
    pub fn from_config(config: &BlipConfig) -> Result<Self, DaemonClientError> {
        Ok(Self::new(config.daemon_socket_path()?))
    }

    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_owned(),
        }
    }

    pub fn health(&self) -> Result<HealthResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("health"),
            DaemonCommand::Health,
            DaemonRequestPayload::Health,
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::Health(health)) => Ok(health),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::Health,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn version(&self) -> Result<DaemonVersionResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("version"),
            DaemonCommand::Version,
            DaemonRequestPayload::Version,
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::Version(version)) => Ok(version),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::Version,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn current_workspace(&self) -> Result<CurrentWorkspaceResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("current"),
            DaemonCommand::CurrentWorkspace,
            DaemonRequestPayload::CurrentWorkspace,
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::CurrentWorkspace(current)) => Ok(current),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::CurrentWorkspace,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn activate_workspace(
        &self,
        workspace: &str,
    ) -> Result<CurrentWorkspaceResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("activate-workspace"),
            DaemonCommand::ActivateWorkspace,
            DaemonRequestPayload::ActivateWorkspace {
                workspace: workspace.to_owned(),
            },
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::WorkspaceActivated(current)) => Ok(current),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::ActivateWorkspace,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn workspaces(&self) -> Result<WorkspaceListResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("workspaces"),
            DaemonCommand::ListWorkspaces,
            DaemonRequestPayload::ListWorkspaces,
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::Workspaces(workspaces)) => Ok(workspaces),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::ListWorkspaces,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn blips(
        &self,
        workspace: &str,
        limit: usize,
    ) -> Result<BlipListResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("blips"),
            DaemonCommand::ListBlips,
            DaemonRequestPayload::ListBlips {
                workspace: workspace.to_owned(),
                limit,
            },
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::Blips(blips)) => Ok(blips),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::ListBlips,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn blip(&self, blip_id: &str) -> Result<BlipDetail, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("blip"),
            DaemonCommand::GetBlip,
            DaemonRequestPayload::GetBlip {
                blip_id: blip_id.to_owned(),
            },
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::Blip(blip)) => Ok(blip),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::GetBlip,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn audit_events(&self, limit: usize) -> Result<AuditEventListResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("audit-events"),
            DaemonCommand::ListAuditEvents,
            DaemonRequestPayload::ListAuditEvents { limit },
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::AuditEvents(events)) => Ok(events),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::ListAuditEvents,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn agent_recent_blips(
        &self,
        workspace: &str,
        limit: usize,
    ) -> Result<AgentBlipListResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("agent-recent-blips"),
            DaemonCommand::AgentRecentBlips,
            DaemonRequestPayload::AgentRecentBlips {
                workspace: workspace.to_owned(),
                limit,
            },
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::AgentBlips(blips)) => Ok(blips),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::AgentRecentBlips,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn route_latest_inbox_blip(
        &self,
        workspace: &str,
    ) -> Result<BlipRoutedResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("route-latest-inbox"),
            DaemonCommand::RouteLatestInboxBlip,
            DaemonRequestPayload::RouteLatestInboxBlip {
                workspace: workspace.to_owned(),
            },
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::BlipRouted(routed)) => Ok(routed),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::RouteLatestInboxBlip,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn route_blip(
        &self,
        blip_id: &str,
        workspace: &str,
    ) -> Result<BlipRoutedResponse, DaemonClientError> {
        let response = self.request(DaemonRequest::new(
            next_request_id("route-blip"),
            DaemonCommand::RouteBlip,
            DaemonRequestPayload::RouteBlip {
                blip_id: blip_id.to_owned(),
                workspace: workspace.to_owned(),
            },
        ))?;

        match response.payload {
            Some(DaemonResponsePayload::BlipRouted(routed)) => Ok(routed),
            other => Err(DaemonClientError::UnexpectedPayload {
                command: DaemonCommand::RouteBlip,
                payload: payload_name(other.as_ref()),
            }),
        }
    }

    pub fn request(&self, request: DaemonRequest) -> Result<DaemonResponse, DaemonClientError> {
        let response = platform::request(&self.socket_path, &request)?;

        if response.api_version != DAEMON_API_VERSION {
            return Err(DaemonClientError::UnsupportedApiVersion {
                actual: response.api_version,
                expected: DAEMON_API_VERSION,
            });
        }

        if response.status == DaemonResponseStatus::Error {
            let error = response.error.unwrap_or_else(|| {
                DaemonApiError::new(
                    blip_api::DaemonApiErrorCode::Internal,
                    "daemon returned an error response without an error payload",
                )
            });
            return Err(DaemonClientError::Daemon(error));
        }

        Ok(response)
    }
}

#[derive(Debug)]
pub enum DaemonClientError {
    Config(ConfigError),
    UnsupportedPlatform,
    Io(std::io::Error),
    Serialization(serde_json::Error),
    Daemon(DaemonApiError),
    UnsupportedApiVersion {
        actual: u16,
        expected: u16,
    },
    UnexpectedPayload {
        command: DaemonCommand,
        payload: &'static str,
    },
}

impl Display for DaemonClientError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "daemon client configuration error: {error}"),
            Self::UnsupportedPlatform => write!(
                formatter,
                "daemon IPC is only available on Unix platforms in this release"
            ),
            Self::Io(error) if error.kind() == std::io::ErrorKind::NotFound => write!(
                formatter,
                "daemon is not running at the configured socket; start `blip-daemon --ipc-only` or the full daemon first"
            ),
            Self::Io(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => write!(
                formatter,
                "daemon socket exists but no daemon accepted the connection; restart blip-daemon"
            ),
            Self::Io(error) => write!(formatter, "daemon IPC io error: {error}"),
            Self::Serialization(error) => write!(formatter, "daemon IPC JSON error: {error}"),
            Self::Daemon(error) => write!(
                formatter,
                "daemon returned {}: {}",
                error_code_name(error.code),
                error.message
            ),
            Self::UnsupportedApiVersion { actual, expected } => write!(
                formatter,
                "daemon API version mismatch: response used {actual}, expected {expected}"
            ),
            Self::UnexpectedPayload { command, payload } => write!(
                formatter,
                "daemon returned unexpected `{payload}` payload for `{}`",
                command.as_str()
            ),
        }
    }
}

impl Error for DaemonClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::Daemon(_)
            | Self::UnexpectedPayload { .. }
            | Self::UnsupportedApiVersion { .. }
            | Self::UnsupportedPlatform => None,
        }
    }
}

impl From<ConfigError> for DaemonClientError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<std::io::Error> for DaemonClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DaemonClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

fn next_request_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("{prefix}-{}-{nanos}", std::process::id())
}

fn payload_name(payload: Option<&DaemonResponsePayload>) -> &'static str {
    match payload {
        Some(DaemonResponsePayload::Health(_)) => "health",
        Some(DaemonResponsePayload::Version(_)) => "version",
        Some(DaemonResponsePayload::CurrentWorkspace(_)) => "current_workspace",
        Some(DaemonResponsePayload::WorkspaceActivated(_)) => "workspace_activated",
        Some(DaemonResponsePayload::Workspaces(_)) => "workspaces",
        Some(DaemonResponsePayload::Blips(_)) => "blips",
        Some(DaemonResponsePayload::Blip(_)) => "blip",
        Some(DaemonResponsePayload::AuditEvents(_)) => "audit_events",
        Some(DaemonResponsePayload::AgentBlips(_)) => "agent_blips",
        Some(DaemonResponsePayload::BlipRouted(_)) => "blip_routed",
        None => "none",
    }
}

fn error_code_name(code: blip_api::DaemonApiErrorCode) -> &'static str {
    match code {
        blip_api::DaemonApiErrorCode::UnsupportedApiVersion => "unsupported_api_version",
        blip_api::DaemonApiErrorCode::InvalidRequest => "invalid_request",
        blip_api::DaemonApiErrorCode::NotFound => "not_found",
        blip_api::DaemonApiErrorCode::AccessDenied => "access_denied",
        blip_api::DaemonApiErrorCode::StoreUnavailable => "store_unavailable",
        blip_api::DaemonApiErrorCode::Internal => "internal",
    }
}

#[cfg(unix)]
mod platform {
    use super::DaemonClientError;
    use blip_api::{DaemonRequest, DaemonResponse};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::path::Path;

    pub fn request(
        socket_path: &Path,
        request: &DaemonRequest,
    ) -> Result<DaemonResponse, DaemonClientError> {
        let mut stream = UnixStream::connect(socket_path)?;
        serde_json::to_writer(&mut stream, request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        let mut response_line = String::new();
        BufReader::new(stream).read_line(&mut response_line)?;
        Ok(serde_json::from_str(response_line.trim_end())?)
    }
}

#[cfg(not(unix))]
mod platform {
    use super::DaemonClientError;
    use blip_api::{DaemonRequest, DaemonResponse};
    use std::path::Path;

    pub fn request(
        _socket_path: &Path,
        _request: &DaemonRequest,
    ) -> Result<DaemonResponse, DaemonClientError> {
        Err(DaemonClientError::UnsupportedPlatform)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use blip_api::{
        DAEMON_API_VERSION, DaemonResponsePayload, DaemonResponseStatus, DaemonVersionResponse,
    };
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn client_round_trips_version_request_over_unix_socket() {
        let socket_path = unique_socket_path("blip-cli-client-test");
        let server_socket_path = socket_path.clone();
        let handle = thread::spawn(move || {
            let listener = UnixListener::bind(&server_socket_path)
                .expect("test server should bind unix socket");
            let (mut stream, _) = listener.accept().expect("test client should connect");

            let mut request_line = String::new();
            BufReader::new(stream.try_clone().expect("stream should clone"))
                .read_line(&mut request_line)
                .expect("request line should read");
            let request =
                serde_json::from_str::<DaemonRequest>(&request_line).expect("request should parse");
            assert_eq!(request.command, DaemonCommand::Version);

            let response = DaemonResponse::ok(
                request.request_id,
                request.command,
                DaemonResponsePayload::Version(DaemonVersionResponse {
                    api_version: DAEMON_API_VERSION,
                    daemon_version: "0.1.0-test".to_owned(),
                }),
            );
            serde_json::to_writer(&mut stream, &response).expect("response should serialize");
            stream.write_all(b"\n").expect("response should flush");
            std::fs::remove_file(server_socket_path).ok();
        });

        wait_for_socket(&socket_path);
        let version = DaemonClient::new(&socket_path)
            .version()
            .expect("client should read version response");

        assert_eq!(
            version,
            DaemonVersionResponse {
                api_version: DAEMON_API_VERSION,
                daemon_version: "0.1.0-test".to_owned(),
            }
        );

        handle.join().expect("test server should join");
    }

    #[test]
    fn client_surfaces_daemon_error_response() {
        let socket_path = unique_socket_path("blip-cli-client-error-test");
        let server_socket_path = socket_path.clone();
        let handle = thread::spawn(move || {
            let listener = UnixListener::bind(&server_socket_path)
                .expect("test server should bind unix socket");
            let (mut stream, _) = listener.accept().expect("test client should connect");

            let mut request_line = String::new();
            BufReader::new(stream.try_clone().expect("stream should clone"))
                .read_line(&mut request_line)
                .expect("request line should read");
            let request =
                serde_json::from_str::<DaemonRequest>(&request_line).expect("request should parse");
            let response = DaemonResponse {
                api_version: DAEMON_API_VERSION,
                request_id: request.request_id,
                command: request.command,
                status: DaemonResponseStatus::Error,
                payload: None,
                error: Some(DaemonApiError::new(
                    blip_api::DaemonApiErrorCode::StoreUnavailable,
                    "database is busy",
                )),
            };
            serde_json::to_writer(&mut stream, &response).expect("response should serialize");
            stream.write_all(b"\n").expect("response should flush");
            std::fs::remove_file(server_socket_path).ok();
        });

        let error = {
            wait_for_socket(&socket_path);
            DaemonClient::new(&socket_path)
                .health()
                .expect_err("daemon error should surface")
        };

        assert_eq!(
            error.to_string(),
            "daemon returned store_unavailable: database is busy"
        );

        handle.join().expect("test server should join");
    }

    fn unique_socket_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}.sock", std::process::id()))
    }

    fn wait_for_socket(socket_path: &Path) {
        for _ in 0..100 {
            if socket_path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }

        panic!("socket was not created at {}", socket_path.display());
    }
}

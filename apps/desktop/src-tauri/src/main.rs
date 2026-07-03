use blip_api::{
    AuditEventListResponse, BlipDetail, BlipListResponse, BlipRoutedResponse,
    CurrentWorkspaceResponse, DAEMON_API_VERSION, DaemonApiError, DaemonApiErrorCode,
    DaemonCommand, DaemonRequest, DaemonRequestPayload, DaemonResponse, DaemonResponsePayload,
    DaemonResponseStatus, PayloadBytesResponse, PayloadRequester, PayloadSummary,
    WorkspaceListResponse, WorkspaceSummary,
};
use blip_config::{BlipConfig, ConfigError};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::time::Duration;
use tauri_plugin_shell::ShellExt;
use thiserror::Error;

const DAEMON_STARTUP_ATTEMPTS: usize = 25;
const DAEMON_STARTUP_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug, Serialize)]
struct ShortcutRegistrationResponse {
    shortcuts: Vec<ShortcutRegistration>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ShortcutRegistration {
    id: String,
    label: String,
    accelerator: String,
    action: String,
    state: String,
    message: Option<String>,
}

#[derive(Debug, Error)]
enum DesktopError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("daemon IPC io error: {0}")]
    Ipc(#[from] std::io::Error),

    #[error("daemon IPC JSON error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("daemon returned {code}: {message}")]
    Daemon { code: String, message: String },

    #[error("daemon response for `{command}` did not include `{expected}` payload")]
    MissingPayload {
        command: &'static str,
        expected: &'static str,
    },

    #[error("daemon response API version {actual} did not match expected {expected}")]
    ApiVersion { actual: u16, expected: u16 },

    #[error("unable to start bundled daemon sidecar: {0}")]
    Sidecar(String),

    #[cfg(not(unix))]
    #[error("desktop daemon bridge currently requires Unix IPC")]
    UnsupportedIpc,
}

impl Serialize for DesktopError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            current_workspace,
            list_workspaces,
            list_blips,
            get_blip,
            get_payload_metadata,
            get_payload_preview,
            export_payload,
            list_audit_events,
            activate_workspace,
            set_sticky_capture,
            route_latest_inbox_blip,
            register_global_shortcuts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running blipcoard desktop");
}

#[tauri::command(rename_all = "snake_case")]
fn current_workspace(app: tauri::AppHandle) -> Result<CurrentWorkspaceResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::CurrentWorkspace,
        DaemonRequestPayload::CurrentWorkspace,
        |payload| match payload {
            DaemonResponsePayload::CurrentWorkspace(response) => Some(response),
            _ => None,
        },
        "current_workspace",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn list_workspaces(app: tauri::AppHandle) -> Result<WorkspaceListResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::ListWorkspaces,
        DaemonRequestPayload::ListWorkspaces,
        |payload| match payload {
            DaemonResponsePayload::Workspaces(response) => Some(response),
            _ => None,
        },
        "workspaces",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn list_blips(
    app: tauri::AppHandle,
    workspace: String,
    limit: usize,
) -> Result<BlipListResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::ListBlips,
        DaemonRequestPayload::ListBlips { workspace, limit },
        |payload| match payload {
            DaemonResponsePayload::Blips(response) => Some(response),
            _ => None,
        },
        "blips",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn get_blip(app: tauri::AppHandle, blip_id: String) -> Result<BlipDetail, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::GetBlip,
        DaemonRequestPayload::GetBlip { blip_id },
        |payload| match payload {
            DaemonResponsePayload::Blip(response) => Some(response),
            _ => None,
        },
        "blip",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn get_payload_metadata(
    app: tauri::AppHandle,
    payload_id: String,
) -> Result<PayloadSummary, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::GetPayloadMetadata,
        DaemonRequestPayload::GetPayloadMetadata { payload_id },
        |payload| match payload {
            DaemonResponsePayload::PayloadMetadata(response) => Some(response),
            _ => None,
        },
        "payload_metadata",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn get_payload_preview(
    app: tauri::AppHandle,
    payload_id: String,
    requester: PayloadRequester,
) -> Result<PayloadBytesResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::GetPayloadPreview,
        DaemonRequestPayload::GetPayloadPreview {
            payload_id,
            requester,
        },
        |payload| match payload {
            DaemonResponsePayload::PayloadBytes(response) => Some(response),
            _ => None,
        },
        "payload_bytes",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn export_payload(
    app: tauri::AppHandle,
    payload_id: String,
    requester: PayloadRequester,
) -> Result<PayloadBytesResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::ExportPayload,
        DaemonRequestPayload::ExportPayload {
            payload_id,
            requester,
        },
        |payload| match payload {
            DaemonResponsePayload::PayloadBytes(response) => Some(response),
            _ => None,
        },
        "payload_bytes",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn list_audit_events(
    app: tauri::AppHandle,
    limit: usize,
) -> Result<AuditEventListResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::ListAuditEvents,
        DaemonRequestPayload::ListAuditEvents { limit },
        |payload| match payload {
            DaemonResponsePayload::AuditEvents(response) => Some(response),
            _ => None,
        },
        "audit_events",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn activate_workspace(
    app: tauri::AppHandle,
    workspace: String,
) -> Result<CurrentWorkspaceResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::ActivateWorkspace,
        DaemonRequestPayload::ActivateWorkspace { workspace },
        |payload| match payload {
            DaemonResponsePayload::WorkspaceActivated(response) => Some(response),
            _ => None,
        },
        "workspace_activated",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn set_sticky_capture(
    app: tauri::AppHandle,
    workspace: String,
    enabled: bool,
) -> Result<WorkspaceSummary, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::SetStickyCapture,
        DaemonRequestPayload::SetStickyCapture { workspace, enabled },
        |payload| match payload {
            DaemonResponsePayload::StickyCaptureSet(response) => Some(response),
            _ => None,
        },
        "sticky_capture_set",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn route_latest_inbox_blip(
    app: tauri::AppHandle,
    workspace: String,
) -> Result<BlipRoutedResponse, DesktopError> {
    daemon_payload(
        &app,
        DaemonCommand::RouteLatestInboxBlip,
        DaemonRequestPayload::RouteLatestInboxBlip { workspace },
        |payload| match payload {
            DaemonResponsePayload::BlipRouted(response) => Some(response),
            _ => None,
        },
        "blip_routed",
    )
}

#[tauri::command(rename_all = "snake_case")]
fn register_global_shortcuts(shortcuts: Vec<ShortcutRegistration>) -> ShortcutRegistrationResponse {
    ShortcutRegistrationResponse {
        shortcuts: shortcuts
            .into_iter()
            .map(|shortcut| ShortcutRegistration {
                state: "unsupported".to_owned(),
                message: Some(
                    "global shortcut registration is not implemented in the desktop shell yet"
                        .to_owned(),
                ),
                ..shortcut
            })
            .collect(),
    }
}

fn daemon_payload<T>(
    app: &tauri::AppHandle,
    command: DaemonCommand,
    payload: DaemonRequestPayload,
    extract: impl FnOnce(DaemonResponsePayload) -> Option<T>,
    expected: &'static str,
) -> Result<T, DesktopError> {
    let response = send_daemon_request(app, command, payload)?;
    if response.api_version != DAEMON_API_VERSION {
        return Err(DesktopError::ApiVersion {
            actual: response.api_version,
            expected: DAEMON_API_VERSION,
        });
    }
    if response.status == DaemonResponseStatus::Error {
        let error = response.error.unwrap_or_else(|| {
            DaemonApiError::new(DaemonApiErrorCode::Internal, "missing daemon error payload")
        });
        return Err(DesktopError::Daemon {
            code: error_code(error.code),
            message: error.message,
        });
    }

    response
        .payload
        .and_then(extract)
        .ok_or_else(|| DesktopError::MissingPayload {
            command: command.as_str(),
            expected,
        })
}

fn send_daemon_request(
    app: &tauri::AppHandle,
    command: DaemonCommand,
    payload: DaemonRequestPayload,
) -> Result<DaemonResponse, DesktopError> {
    send_daemon_request_impl(app, command, payload)
}

#[cfg(unix)]
fn send_daemon_request_impl(
    app: &tauri::AppHandle,
    command: DaemonCommand,
    payload: DaemonRequestPayload,
) -> Result<DaemonResponse, DesktopError> {
    let config = BlipConfig::load_or_create()?;
    let socket_path = config.daemon_socket_path()?;
    let request = DaemonRequest::new(
        format!("desktop-{}-{}", command.as_str(), std::process::id()),
        command,
        payload,
    );

    match send_once(&socket_path, &request) {
        Ok(response) => Ok(response),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            start_daemon_sidecar(app)?;
            wait_for_daemon(&socket_path)?;
            Ok(send_once(&socket_path, &request)?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            start_daemon_sidecar(app)?;
            wait_for_daemon(&socket_path)?;
            Ok(send_once(&socket_path, &request)?)
        }
        Err(error) => Err(DesktopError::Ipc(error)),
    }
}

#[cfg(not(unix))]
fn send_daemon_request_impl(
    _app: &tauri::AppHandle,
    _command: DaemonCommand,
    _payload: DaemonRequestPayload,
) -> Result<DaemonResponse, DesktopError> {
    Err(DesktopError::UnsupportedIpc)
}

#[cfg(unix)]
fn send_once(
    socket_path: &std::path::Path,
    request: &DaemonRequest,
) -> Result<DaemonResponse, std::io::Error> {
    let mut stream = UnixStream::connect(socket_path)?;
    let request_bytes = serde_json::to_vec(request)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    stream.write_all(&request_bytes)?;
    stream.write_all(b"\n")?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    serde_json::from_str(&line)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(unix)]
fn start_daemon_sidecar(app: &tauri::AppHandle) -> Result<(), DesktopError> {
    let sidecar = app
        .shell()
        .sidecar("binaries/blipd")
        .map_err(|error| DesktopError::Sidecar(error.to_string()))?;
    sidecar
        .spawn()
        .map_err(|error| DesktopError::Sidecar(error.to_string()))?;
    Ok(())
}

#[cfg(unix)]
fn wait_for_daemon(socket_path: &std::path::Path) -> Result<(), DesktopError> {
    for _ in 0..DAEMON_STARTUP_ATTEMPTS {
        if socket_path.exists() {
            return Ok(());
        }
        std::thread::sleep(DAEMON_STARTUP_DELAY);
    }
    Err(DesktopError::Ipc(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!("daemon socket was not created at {}", socket_path.display()),
    )))
}

fn error_code(code: DaemonApiErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "internal".to_owned())
}

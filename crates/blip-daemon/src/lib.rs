//! Daemon runtime ownership for clipboard ingestion.
//!
//! `blipd` owns the long-running ingestion loop. Platform clipboard code should
//! feed this runtime through an ingestion source; it should not own storage,
//! policy, or process lifecycle decisions.

pub mod ipc;

use blip_api::{
    AgentBlip, AgentBlipListResponse, BlipDetail, BlipListResponse, BlipRoutedResponse,
    BlipSummary, CurrentWorkspaceResponse, DAEMON_API_VERSION, DaemonApiError, DaemonApiErrorCode,
    DaemonCommand, DaemonRequest, DaemonRequestPayload, DaemonResponse, DaemonResponsePayload,
    DaemonVersionResponse, HealthResponse, WorkspaceListResponse, WorkspaceSummary,
};
use blip_clipboard::{ClipboardError, ClipboardWatcher};
use blip_core::Blip;
use blip_core::{BlipError, BlipStore, ContentType, NewBlip};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

const PENDING_SOURCE_INTERVAL: Duration = Duration::from_secs(1);
const DUPLICATE_SUPPRESSION_WINDOW: Duration = Duration::from_secs(2);
const INBOX_WORKSPACE: &str = "inbox";

pub struct DaemonRuntime<S> {
    database_path: PathBuf,
    store: BlipStore,
    source: S,
    duplicate_suppression: DuplicateSuppression,
}

impl<S> DaemonRuntime<S>
where
    S: IngestionSource,
{
    pub fn new(database_path: impl AsRef<Path>, store: BlipStore, source: S) -> Self {
        Self {
            database_path: database_path.as_ref().to_owned(),
            store,
            source,
            duplicate_suppression: DuplicateSuppression::new(DUPLICATE_SUPPRESSION_WINDOW),
        }
    }

    pub fn with_duplicate_suppression_window(
        database_path: impl AsRef<Path>,
        store: BlipStore,
        source: S,
        duplicate_suppression_window: Duration,
    ) -> Self {
        Self {
            database_path: database_path.as_ref().to_owned(),
            store,
            source,
            duplicate_suppression: DuplicateSuppression::new(duplicate_suppression_window),
        }
    }

    pub fn health_response(&self) -> Result<HealthResponse, BlipError> {
        Ok(HealthResponse {
            service: "blipd".to_string(),
            status: "ready".to_string(),
            database_path: self.database_path.display().to_string(),
            active_workspace: self.store.get_active_workspace()?,
            generated_at: Utc::now(),
        })
    }

    pub fn dispatch_daemon_request(&mut self, request: DaemonRequest) -> DaemonResponse {
        let DaemonRequest {
            api_version,
            request_id,
            command,
            payload,
        } = request;

        if api_version != DAEMON_API_VERSION {
            return DaemonResponse::error(
                request_id,
                command,
                DaemonApiError::new(
                    DaemonApiErrorCode::UnsupportedApiVersion,
                    format!(
                        "unsupported daemon API version {api_version}; expected {DAEMON_API_VERSION}"
                    ),
                ),
            );
        }

        match (command, payload) {
            (DaemonCommand::Health, DaemonRequestPayload::Health) => match self.health_response() {
                Ok(response) => {
                    DaemonResponse::ok(request_id, command, DaemonResponsePayload::Health(response))
                }
                Err(error) => DaemonResponse::error(
                    request_id,
                    command,
                    DaemonApiError::new(DaemonApiErrorCode::StoreUnavailable, error.to_string()),
                ),
            },
            (DaemonCommand::Version, DaemonRequestPayload::Version) => DaemonResponse::ok(
                request_id,
                command,
                DaemonResponsePayload::Version(DaemonVersionResponse {
                    api_version: DAEMON_API_VERSION,
                    daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                }),
            ),
            (DaemonCommand::CurrentWorkspace, DaemonRequestPayload::CurrentWorkspace) => match self
                .store
                .get_active_workspace()
            {
                Ok(active_workspace) => DaemonResponse::ok(
                    request_id,
                    command,
                    DaemonResponsePayload::CurrentWorkspace(CurrentWorkspaceResponse {
                        active_workspace,
                    }),
                ),
                Err(error) => DaemonResponse::error(
                    request_id,
                    command,
                    DaemonApiError::new(DaemonApiErrorCode::StoreUnavailable, error.to_string()),
                ),
            },
            (
                DaemonCommand::ActivateWorkspace,
                DaemonRequestPayload::ActivateWorkspace { workspace },
            ) => match self.store.set_active_workspace(&workspace) {
                Ok(()) => DaemonResponse::ok(
                    request_id,
                    command,
                    DaemonResponsePayload::WorkspaceActivated(CurrentWorkspaceResponse {
                        active_workspace: Some(workspace),
                    }),
                ),
                Err(BlipError::WorkspaceNotFound(workspace)) => DaemonResponse::error(
                    request_id,
                    command,
                    DaemonApiError::new(
                        DaemonApiErrorCode::NotFound,
                        format!("workspace `{workspace}` does not exist"),
                    ),
                ),
                Err(error) => DaemonResponse::error(
                    request_id,
                    command,
                    DaemonApiError::new(DaemonApiErrorCode::StoreUnavailable, error.to_string()),
                ),
            },
            (DaemonCommand::ListWorkspaces, DaemonRequestPayload::ListWorkspaces) => {
                match self.store.list_workspaces() {
                    Ok(workspaces) => DaemonResponse::ok(
                        request_id,
                        command,
                        DaemonResponsePayload::Workspaces(WorkspaceListResponse {
                            workspaces: workspaces
                                .into_iter()
                                .map(|workspace| WorkspaceSummary {
                                    name: workspace.name,
                                    agent_access: workspace.agent_access,
                                })
                                .collect(),
                        }),
                    ),
                    Err(error) => DaemonResponse::error(
                        request_id,
                        command,
                        DaemonApiError::new(
                            DaemonApiErrorCode::StoreUnavailable,
                            error.to_string(),
                        ),
                    ),
                }
            }
            (DaemonCommand::ListBlips, DaemonRequestPayload::ListBlips { workspace, limit }) => {
                match self.store.list_blip_summaries(&workspace, limit) {
                    Ok(blips) => DaemonResponse::ok(
                        request_id,
                        command,
                        DaemonResponsePayload::Blips(BlipListResponse {
                            workspace,
                            blips: blips
                                .into_iter()
                                .map(|blip| BlipSummary {
                                    id: blip.id,
                                    preview: blip.preview,
                                    size_bytes: blip.size_bytes,
                                })
                                .collect(),
                        }),
                    ),
                    Err(error) => DaemonResponse::error(
                        request_id,
                        command,
                        DaemonApiError::new(
                            DaemonApiErrorCode::StoreUnavailable,
                            error.to_string(),
                        ),
                    ),
                }
            }
            (DaemonCommand::GetBlip, DaemonRequestPayload::GetBlip { blip_id }) => {
                match self.store.get_blip(&blip_id) {
                    Ok(Some(blip)) => DaemonResponse::ok(
                        request_id,
                        command,
                        DaemonResponsePayload::Blip(blip_detail_from_store(blip)),
                    ),
                    Ok(None) => DaemonResponse::error(
                        request_id,
                        command,
                        DaemonApiError::new(
                            DaemonApiErrorCode::NotFound,
                            format!("blip `{blip_id}` does not exist"),
                        ),
                    ),
                    Err(error) => DaemonResponse::error(
                        request_id,
                        command,
                        DaemonApiError::new(
                            DaemonApiErrorCode::StoreUnavailable,
                            error.to_string(),
                        ),
                    ),
                }
            }
            (
                DaemonCommand::AgentRecentBlips,
                DaemonRequestPayload::AgentRecentBlips { workspace, limit },
            ) => match self.store.list_agent_blips(&workspace, limit) {
                Ok(blips) => DaemonResponse::ok(
                    request_id,
                    command,
                    DaemonResponsePayload::AgentBlips(AgentBlipListResponse {
                        workspace,
                        blips: blips
                            .into_iter()
                            .map(|blip| AgentBlip {
                                id: blip.id,
                                content: blip.content,
                                size_bytes: blip.size_bytes,
                            })
                            .collect(),
                    }),
                ),
                Err(error) => agent_read_error_response(request_id, command, error),
            },
            (
                DaemonCommand::RouteLatestInboxBlip,
                DaemonRequestPayload::RouteLatestInboxBlip { workspace },
            ) => match self.store.move_latest_inbox_blip(&workspace) {
                Ok(moved) => DaemonResponse::ok(
                    request_id,
                    command,
                    DaemonResponsePayload::BlipRouted(BlipRoutedResponse {
                        id: moved.id,
                        from_workspace: moved.from_workspace,
                        to_workspace: moved.to_workspace,
                    }),
                ),
                Err(error) => route_error_response(request_id, command, error),
            },
            (DaemonCommand::RouteBlip, DaemonRequestPayload::RouteBlip { blip_id, workspace }) => {
                match self.store.move_blip(&blip_id, &workspace) {
                    Ok(moved) => DaemonResponse::ok(
                        request_id,
                        command,
                        DaemonResponsePayload::BlipRouted(BlipRoutedResponse {
                            id: moved.id,
                            from_workspace: moved.from_workspace,
                            to_workspace: moved.to_workspace,
                        }),
                    ),
                    Err(error) => route_error_response(request_id, command, error),
                }
            }
            _ => DaemonResponse::error(
                request_id,
                command,
                DaemonApiError::new(
                    DaemonApiErrorCode::InvalidRequest,
                    format!(
                        "payload does not match daemon command `{}`",
                        command.as_str()
                    ),
                ),
            ),
        }
    }

    pub fn run(&mut self) -> Result<(), DaemonError> {
        loop {
            match self.source.wait_for_next()? {
                RuntimeEvent::Idle => {}
                RuntimeEvent::ClipboardTextChanged { text } => {
                    self.ingest_clipboard_text(text)?;
                }
                RuntimeEvent::Shutdown => return Ok(()),
            }
        }
    }

    fn ingest_clipboard_text(&mut self, text: String) -> Result<(), DaemonError> {
        let observed_at = Instant::now();
        if self
            .duplicate_suppression
            .should_suppress(&text, observed_at)
        {
            return Ok(());
        }

        self.store.insert_blip(&NewBlip {
            workspace_name: INBOX_WORKSPACE.to_owned(),
            source_app: None,
            content_type: ContentType::PlainText,
            language: None,
            content: text.clone(),
            token_estimate: None,
            is_redacted: false,
            tags: Vec::new(),
        })?;
        self.duplicate_suppression
            .record_ingested(text, observed_at);

        Ok(())
    }
}

fn blip_detail_from_store(blip: Blip) -> BlipDetail {
    BlipDetail {
        id: blip.id,
        workspace: blip.workspace_name,
        source_app: blip.source_app,
        content_type: blip.content_type.as_str().to_owned(),
        language: blip.language,
        content: blip.content,
        size_bytes: blip.size_bytes,
        token_estimate: blip.token_estimate,
        is_redacted: blip.is_redacted,
        tags: blip.tags,
        created_at: blip.created_at,
    }
}

fn route_error_response(
    request_id: String,
    command: DaemonCommand,
    error: BlipError,
) -> DaemonResponse {
    match error {
        BlipError::WorkspaceNotFound(workspace) => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::NotFound,
                format!("workspace `{workspace}` does not exist"),
            ),
        ),
        BlipError::BlipNotFound(id) => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::NotFound,
                format!("blip `{id}` does not exist"),
            ),
        ),
        BlipError::InboxEmpty => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(DaemonApiErrorCode::NotFound, "inbox is empty"),
        ),
        error => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(DaemonApiErrorCode::StoreUnavailable, error.to_string()),
        ),
    }
}

fn agent_read_error_response(
    request_id: String,
    command: DaemonCommand,
    error: BlipError,
) -> DaemonResponse {
    match error {
        BlipError::WorkspaceNotFound(workspace) => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::NotFound,
                format!("workspace `{workspace}` does not exist"),
            ),
        ),
        BlipError::AgentAccessDenied(workspace) => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::AccessDenied,
                format!("agent access to workspace `{workspace}` is denied"),
            ),
        ),
        error => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(DaemonApiErrorCode::StoreUnavailable, error.to_string()),
        ),
    }
}

struct DuplicateSuppression {
    window: Duration,
    last_ingested: Option<IngestedClipboardText>,
}

impl DuplicateSuppression {
    fn new(window: Duration) -> Self {
        Self {
            window,
            last_ingested: None,
        }
    }

    fn should_suppress(&self, text: &str, observed_at: Instant) -> bool {
        self.last_ingested.as_ref().is_some_and(|last| {
            last.text == text && observed_at.duration_since(last.observed_at) <= self.window
        })
    }

    fn record_ingested(&mut self, text: String, observed_at: Instant) {
        self.last_ingested = Some(IngestedClipboardText { text, observed_at });
    }
}

struct IngestedClipboardText {
    text: String,
    observed_at: Instant,
}

pub trait IngestionSource {
    fn wait_for_next(&mut self) -> Result<RuntimeEvent, DaemonError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    Idle,
    ClipboardTextChanged { text: String },
    Shutdown,
}

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error(transparent)]
    Store(#[from] BlipError),

    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
}

pub struct PendingIngestionSource {
    interval: Duration,
}

impl Default for PendingIngestionSource {
    fn default() -> Self {
        Self {
            interval: PENDING_SOURCE_INTERVAL,
        }
    }
}

impl IngestionSource for PendingIngestionSource {
    fn wait_for_next(&mut self) -> Result<RuntimeEvent, DaemonError> {
        thread::sleep(self.interval);
        Ok(RuntimeEvent::Idle)
    }
}

pub struct ClipboardIngestionSource<W> {
    watcher: W,
    idle_interval: Duration,
}

impl<W> ClipboardIngestionSource<W>
where
    W: ClipboardWatcher,
{
    pub fn new(watcher: W, idle_interval: Duration) -> Self {
        Self {
            watcher,
            idle_interval,
        }
    }
}

impl<W> IngestionSource for ClipboardIngestionSource<W>
where
    W: ClipboardWatcher,
{
    fn wait_for_next(&mut self) -> Result<RuntimeEvent, DaemonError> {
        let Some(event) = self.watcher.poll_next()? else {
            thread::sleep(self.idle_interval);
            return Ok(RuntimeEvent::Idle);
        };

        Ok(RuntimeEvent::ClipboardTextChanged { text: event.text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blip_clipboard::{ClipboardEvent, ClipboardWatcher};

    struct ScriptedIngestionSource {
        events: Vec<RuntimeEvent>,
        calls: usize,
    }

    impl IngestionSource for ScriptedIngestionSource {
        fn wait_for_next(&mut self) -> Result<RuntimeEvent, DaemonError> {
            self.calls += 1;
            Ok(self.events.pop().unwrap_or(RuntimeEvent::Shutdown))
        }
    }

    struct ScriptedClipboardWatcher {
        events: Vec<Option<ClipboardEvent>>,
    }

    impl ClipboardWatcher for ScriptedClipboardWatcher {
        fn poll_next(&mut self) -> Result<Option<ClipboardEvent>, ClipboardError> {
            Ok(self.events.pop().unwrap_or(None))
        }
    }

    #[test]
    fn runtime_runs_until_shutdown_without_clipboard_code() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::Idle,
                RuntimeEvent::Idle,
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new("/tmp/blipcoard-test.db", store, source);

        runtime.run().expect("runtime should stop cleanly");

        assert_eq!(runtime.source.calls, 3);
    }

    #[test]
    fn runtime_reports_health_from_owned_store() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime
            .health_response()
            .expect("health response should be built");

        assert_eq!(response.service, "blipd");
        assert_eq!(response.status, "ready");
        assert_eq!(response.active_workspace.as_deref(), Some("inbox"));
    }

    #[test]
    fn dispatch_returns_health_payload() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "health-1",
            DaemonCommand::Health,
            DaemonRequestPayload::Health,
        ));

        assert_eq!(response.request_id, "health-1");
        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        match response.payload {
            Some(DaemonResponsePayload::Health(health)) => {
                assert_eq!(health.service, "blipd");
                assert_eq!(health.active_workspace.as_deref(), Some("inbox"));
            }
            other => panic!("expected health payload, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_returns_version_payload() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "version-1",
            DaemonCommand::Version,
            DaemonRequestPayload::Version,
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::Version(DaemonVersionResponse {
                api_version: DAEMON_API_VERSION,
                daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            }))
        );
    }

    #[test]
    fn dispatch_returns_current_workspace_payload() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "current-1",
            DaemonCommand::CurrentWorkspace,
            DaemonRequestPayload::CurrentWorkspace,
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::CurrentWorkspace(
                CurrentWorkspaceResponse {
                    active_workspace: Some("inbox".to_owned()),
                },
            )),
        );
    }

    #[test]
    fn dispatch_activates_workspace_and_records_audit_event() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&blip_core::NewWorkspace {
                name: "auth-bug".to_owned(),
                description: None,
                color: None,
                agent_access: false,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "activate-1",
            DaemonCommand::ActivateWorkspace,
            DaemonRequestPayload::ActivateWorkspace {
                workspace: "auth-bug".to_owned(),
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::WorkspaceActivated(
                CurrentWorkspaceResponse {
                    active_workspace: Some("auth-bug".to_owned()),
                },
            )),
        );
        assert_eq!(
            runtime
                .store
                .get_active_workspace()
                .expect("active workspace should read")
                .as_deref(),
            Some("auth-bug")
        );
        let audit_events = runtime
            .store
            .list_audit_events()
            .expect("audit events should list");
        assert!(audit_events.iter().any(|event| {
            event.event_type == blip_core::AuditEventType::WorkspaceActivated
                && event.target_workspace.as_deref() == Some("auth-bug")
        }));
    }

    #[test]
    fn dispatch_returns_not_found_for_missing_workspace_activation() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "activate-missing",
            DaemonCommand::ActivateWorkspace,
            DaemonRequestPayload::ActivateWorkspace {
                workspace: "missing".to_owned(),
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Error);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(DaemonApiErrorCode::NotFound)
        );
    }

    #[test]
    fn dispatch_returns_workspace_list_payload() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&blip_core::NewWorkspace {
                name: "auth-bug".to_owned(),
                description: None,
                color: None,
                agent_access: true,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "workspaces-1",
            DaemonCommand::ListWorkspaces,
            DaemonRequestPayload::ListWorkspaces,
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        match response.payload {
            Some(DaemonResponsePayload::Workspaces(workspaces)) => {
                assert!(
                    workspaces
                        .workspaces
                        .iter()
                        .any(|workspace| { workspace.name == "inbox" && !workspace.agent_access })
                );
                assert!(
                    workspaces.workspaces.iter().any(|workspace| {
                        workspace.name == "auth-bug" && workspace.agent_access
                    })
                );
            }
            other => panic!("expected workspaces response, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_returns_blip_list_payload() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let inserted = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".to_owned(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "copied text".to_owned(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should be inserted");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "blips-1",
            DaemonCommand::ListBlips,
            DaemonRequestPayload::ListBlips {
                workspace: "inbox".to_owned(),
                limit: 50,
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::Blips(BlipListResponse {
                workspace: "inbox".to_owned(),
                blips: vec![BlipSummary {
                    id: inserted.id,
                    preview: "copied text".to_owned(),
                    size_bytes: 11,
                }],
            })),
        );
    }

    #[test]
    fn dispatch_returns_full_blip_detail_payload() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let inserted = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".to_owned(),
                source_app: Some("Firefox".to_owned()),
                content_type: ContentType::PlainText,
                language: None,
                content: "sensitive copied text".to_owned(),
                token_estimate: Some(3),
                is_redacted: true,
                tags: vec!["demo".to_owned()],
            })
            .expect("blip should be inserted");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "get-blip-1",
            DaemonCommand::GetBlip,
            DaemonRequestPayload::GetBlip {
                blip_id: inserted.id.clone(),
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::Blip(BlipDetail {
                id: inserted.id,
                workspace: "inbox".to_owned(),
                source_app: Some("Firefox".to_owned()),
                content_type: "plain_text".to_owned(),
                language: None,
                content: "sensitive copied text".to_owned(),
                size_bytes: 21,
                token_estimate: Some(3),
                is_redacted: true,
                tags: vec!["demo".to_owned()],
                created_at: inserted.created_at,
            })),
        );
    }

    #[test]
    fn dispatch_returns_not_found_for_missing_blip_detail() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "get-blip-missing",
            DaemonCommand::GetBlip,
            DaemonRequestPayload::GetBlip {
                blip_id: "missing".to_owned(),
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Error);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(DaemonApiErrorCode::NotFound)
        );
    }

    #[test]
    fn dispatch_returns_agent_recent_blips_for_agent_access_workspace() {
        let mut store = store_with_agent_workspace("agent-feed");
        let inserted = insert_test_blip(&mut store, "agent-feed", "agent-visible note");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "agent-recent",
            DaemonCommand::AgentRecentBlips,
            DaemonRequestPayload::AgentRecentBlips {
                workspace: "agent-feed".to_owned(),
                limit: 50,
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::AgentBlips(AgentBlipListResponse {
                workspace: "agent-feed".to_owned(),
                blips: vec![AgentBlip {
                    id: inserted.id,
                    content: "agent-visible note".to_owned(),
                    size_bytes: 18,
                }],
            })),
        );

        let audit_events = runtime
            .store
            .list_audit_events()
            .expect("audit events should list");
        assert!(audit_events.iter().any(|event| {
            event.actor_type == blip_core::ActorType::Agent
                && event.event_type == blip_core::AuditEventType::BlipsRead
                && event.target_workspace.as_deref() == Some("agent-feed")
        }));
    }

    #[test]
    fn dispatch_denies_agent_recent_blips_for_human_only_workspace() {
        let store = store_with_workspace("auth-bug");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "agent-denied",
            DaemonCommand::AgentRecentBlips,
            DaemonRequestPayload::AgentRecentBlips {
                workspace: "auth-bug".to_owned(),
                limit: 50,
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Error);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(DaemonApiErrorCode::AccessDenied)
        );
    }

    #[test]
    fn dispatch_routes_latest_inbox_blip_payload() {
        let mut store = store_with_workspace("auth-bug");
        let inserted = insert_test_blip(&mut store, "inbox", "copied note");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "route-latest",
            DaemonCommand::RouteLatestInboxBlip,
            DaemonRequestPayload::RouteLatestInboxBlip {
                workspace: "auth-bug".to_owned(),
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::BlipRouted(BlipRoutedResponse {
                id: inserted.id,
                from_workspace: "inbox".to_owned(),
                to_workspace: "auth-bug".to_owned(),
            })),
        );
    }

    #[test]
    fn dispatch_routes_specific_blip_payload() {
        let mut store = store_with_workspace("auth-bug");
        let inserted = insert_test_blip(&mut store, "auth-bug", "misrouted note");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "route-id",
            DaemonCommand::RouteBlip,
            DaemonRequestPayload::RouteBlip {
                blip_id: inserted.id.clone(),
                workspace: "inbox".to_owned(),
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::BlipRouted(BlipRoutedResponse {
                id: inserted.id,
                from_workspace: "auth-bug".to_owned(),
                to_workspace: "inbox".to_owned(),
            })),
        );
    }

    #[test]
    fn dispatch_returns_not_found_for_empty_inbox_route() {
        let store = store_with_workspace("auth-bug");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "route-empty",
            DaemonCommand::RouteLatestInboxBlip,
            DaemonRequestPayload::RouteLatestInboxBlip {
                workspace: "auth-bug".to_owned(),
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Error);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(DaemonApiErrorCode::NotFound)
        );
    }

    #[test]
    fn dispatch_rejects_unsupported_api_version() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );
        let mut request = DaemonRequest::new(
            "bad-version",
            DaemonCommand::Health,
            DaemonRequestPayload::Health,
        );
        request.api_version = DAEMON_API_VERSION + 1;

        let response = runtime.dispatch_daemon_request(request);

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Error);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(DaemonApiErrorCode::UnsupportedApiVersion)
        );
    }

    #[test]
    fn dispatch_rejects_command_payload_mismatch() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "mismatch",
            DaemonCommand::Health,
            DaemonRequestPayload::Version,
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Error);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(DaemonApiErrorCode::InvalidRequest)
        );
    }

    #[cfg(unix)]
    #[test]
    fn ipc_server_serves_runtime_health_dispatch() {
        use crate::ipc::DaemonIpcServer;
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixStream;

        let socket_path = unique_socket_path("blip-daemon-runtime-ipc-test");
        let server_socket_path = socket_path.clone();
        let handle = thread::spawn(move || {
            let store = BlipStore::in_memory().expect("store should initialize");
            let mut runtime = DaemonRuntime::new(
                "/tmp/blipcoard-test.db",
                store,
                ScriptedIngestionSource {
                    events: Vec::new(),
                    calls: 0,
                },
            );
            DaemonIpcServer::new(&server_socket_path)
                .serve_one(|request| runtime.dispatch_daemon_request(request))
                .expect("IPC server should handle one runtime request");
            std::fs::remove_file(server_socket_path).ok();
        });

        wait_for_socket(&socket_path);
        let mut stream = UnixStream::connect(&socket_path).expect("client should connect");
        let request = DaemonRequest::new(
            "runtime-health",
            DaemonCommand::Health,
            DaemonRequestPayload::Health,
        );
        serde_json::to_writer(&mut stream, &request).expect("request should serialize");
        stream.write_all(b"\n").expect("request should flush");

        let mut response_line = String::new();
        BufReader::new(stream)
            .read_line(&mut response_line)
            .expect("response line should read");
        let response =
            serde_json::from_str::<DaemonResponse>(&response_line).expect("response should decode");

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        match response.payload {
            Some(DaemonResponsePayload::Health(health)) => {
                assert_eq!(health.service, "blipd");
                assert_eq!(health.active_workspace.as_deref(), Some("inbox"));
            }
            other => panic!("expected health response, got {other:?}"),
        }

        handle.join().expect("IPC server thread should join");
    }

    #[test]
    fn clipboard_source_maps_watcher_events_to_runtime_events() {
        let watcher = ScriptedClipboardWatcher {
            events: vec![Some(ClipboardEvent {
                text: "copied text".to_owned(),
            })],
        };
        let mut source = ClipboardIngestionSource::new(watcher, Duration::ZERO);

        assert_eq!(
            source
                .wait_for_next()
                .expect("clipboard source should poll watcher"),
            RuntimeEvent::ClipboardTextChanged {
                text: "copied text".to_owned(),
            }
        );
    }

    #[test]
    fn runtime_persists_clipboard_text_events_into_inbox() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardTextChanged {
                    text: "copied text".to_owned(),
                },
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new("/tmp/blipcoard-test.db", store, source);

        runtime.run().expect("runtime should ingest clipboard text");

        let blips = runtime
            .store
            .list_blips("inbox")
            .expect("inbox blips should be listed");
        assert_eq!(blips.len(), 1);
        assert_eq!(blips[0].content, "copied text");
        assert_eq!(blips[0].content_type, ContentType::PlainText);

        let audit_events = runtime
            .store
            .list_audit_events()
            .expect("audit events should be listed");
        assert!(audit_events.iter().any(|event| {
            event.event_type == blip_core::AuditEventType::BlipIngested
                && event.target_blip_id.as_deref() == Some(blips[0].id.as_str())
                && event.target_workspace.as_deref() == Some("inbox")
        }));
    }

    #[test]
    fn runtime_suppresses_repeated_identical_clipboard_text_within_window() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardTextChanged {
                    text: "same text".to_owned(),
                },
                RuntimeEvent::ClipboardTextChanged {
                    text: "same text".to_owned(),
                },
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::with_duplicate_suppression_window(
            "/tmp/blipcoard-test.db",
            store,
            source,
            Duration::MAX,
        );

        runtime
            .run()
            .expect("runtime should suppress duplicate clipboard text");

        let blips = runtime
            .store
            .list_blips("inbox")
            .expect("inbox blips should be listed");
        assert_eq!(blips.len(), 1);
        assert_eq!(blips[0].content, "same text");

        let ingested_audit_events = runtime
            .store
            .list_audit_events()
            .expect("audit events should be listed")
            .into_iter()
            .filter(|event| event.event_type == blip_core::AuditEventType::BlipIngested)
            .count();
        assert_eq!(ingested_audit_events, 1);
    }

    #[test]
    fn duplicate_suppression_uses_identical_text_and_time_window() {
        let mut suppression = DuplicateSuppression::new(Duration::from_secs(2));
        let observed_at = Instant::now();

        assert!(!suppression.should_suppress("copied text", observed_at));
        suppression.record_ingested("copied text".to_owned(), observed_at);

        assert!(suppression.should_suppress("copied text", observed_at + Duration::from_secs(1)));
        assert!(
            !suppression.should_suppress("different text", observed_at + Duration::from_secs(1))
        );
        assert!(!suppression.should_suppress("copied text", observed_at + Duration::from_secs(3)));
    }

    #[test]
    fn runtime_propagates_store_errors_from_clipboard_ingestion() {
        let store = BlipStore::in_memory().expect("store should initialize");
        store
            .connection()
            .execute("DELETE FROM workspaces WHERE name = 'inbox'", [])
            .expect("test setup should remove inbox");
        let source = ScriptedIngestionSource {
            events: vec![RuntimeEvent::ClipboardTextChanged {
                text: "copied text".to_owned(),
            }],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new("/tmp/blipcoard-test.db", store, source);

        let error = runtime
            .run()
            .expect_err("missing inbox should surface as a store error");

        match error {
            DaemonError::Store(BlipError::WorkspaceNotFound(workspace)) => {
                assert_eq!(workspace, "inbox");
            }
            other => panic!("expected missing inbox store error, got {other:?}"),
        }
    }

    #[cfg(unix)]
    fn unique_socket_path(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}.sock", std::process::id()))
    }

    #[cfg(unix)]
    fn wait_for_socket(socket_path: &Path) {
        for _ in 0..100 {
            if socket_path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }

        panic!("socket was not created at {}", socket_path.display());
    }

    fn store_with_workspace(workspace: &str) -> BlipStore {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&blip_core::NewWorkspace {
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

    fn store_with_agent_workspace(workspace: &str) -> BlipStore {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&blip_core::NewWorkspace {
                name: workspace.to_owned(),
                description: None,
                color: None,
                agent_access: true,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");
        store
    }

    fn insert_test_blip(store: &mut BlipStore, workspace: &str, content: &str) -> blip_core::Blip {
        store
            .insert_blip(&NewBlip {
                workspace_name: workspace.to_owned(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: content.to_owned(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should be inserted")
    }
}

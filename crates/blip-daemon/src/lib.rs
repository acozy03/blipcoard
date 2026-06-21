//! Daemon runtime ownership for clipboard ingestion.
//!
//! `blipd` owns the long-running ingestion loop. Platform clipboard code should
//! feed this runtime through an ingestion source; it should not own storage,
//! policy, or process lifecycle decisions.

use blip_api::{
    DAEMON_API_VERSION, DaemonApiError, DaemonApiErrorCode, DaemonCommand, DaemonRequest,
    DaemonRequestPayload, DaemonResponse, DaemonResponsePayload, DaemonVersionResponse,
    HealthResponse,
};
use blip_clipboard::{ClipboardError, ClipboardWatcher};
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

    pub fn dispatch_daemon_request(&self, request: DaemonRequest) -> DaemonResponse {
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
        let runtime = DaemonRuntime::new(
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
        let runtime = DaemonRuntime::new(
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
    fn dispatch_rejects_unsupported_api_version() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let runtime = DaemonRuntime::new(
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
        let runtime = DaemonRuntime::new(
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
}

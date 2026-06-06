//! Daemon runtime ownership for clipboard ingestion.
//!
//! `blipd` owns the long-running ingestion loop. Platform clipboard code should
//! feed this runtime through an ingestion source; it should not own storage,
//! policy, or process lifecycle decisions.

use blip_api::HealthResponse;
use blip_core::{BlipError, BlipStore};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const PENDING_SOURCE_INTERVAL: Duration = Duration::from_secs(1);

pub struct DaemonRuntime<S> {
    database_path: PathBuf,
    store: BlipStore,
    source: S,
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

    pub fn run(&mut self) {
        loop {
            match self.source.wait_for_next() {
                RuntimeEvent::Idle => {}
                RuntimeEvent::Shutdown => return,
            }
        }
    }
}

pub trait IngestionSource {
    fn wait_for_next(&mut self) -> RuntimeEvent;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEvent {
    Idle,
    Shutdown,
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
    fn wait_for_next(&mut self) -> RuntimeEvent {
        thread::sleep(self.interval);
        RuntimeEvent::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedIngestionSource {
        events: Vec<RuntimeEvent>,
        calls: usize,
    }

    impl IngestionSource for ScriptedIngestionSource {
        fn wait_for_next(&mut self) -> RuntimeEvent {
            self.calls += 1;
            self.events.pop().unwrap_or(RuntimeEvent::Shutdown)
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

        runtime.run();

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
}

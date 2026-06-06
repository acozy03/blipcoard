//! Daemon runtime ownership for clipboard ingestion.
//!
//! `blipd` owns the long-running ingestion loop. Platform clipboard code should
//! feed this runtime through an ingestion source; it should not own storage,
//! policy, or process lifecycle decisions.

use blip_api::HealthResponse;
use blip_core::{BlipError, BlipStore};
use chrono::Utc;
use std::path::Path;
use std::thread;
use std::time::Duration;

const PENDING_SOURCE_INTERVAL: Duration = Duration::from_secs(1);

pub struct DaemonRuntime<S> {
    store: BlipStore,
    source: S,
}

impl<S> DaemonRuntime<S>
where
    S: IngestionSource,
{
    pub fn new(store: BlipStore, source: S) -> Self {
        Self { store, source }
    }

    pub fn health_response(&self, database_path: &Path) -> Result<HealthResponse, BlipError> {
        Ok(HealthResponse {
            service: "blipd".to_string(),
            status: "ready".to_string(),
            database_path: database_path.display().to_string(),
            active_workspace: self.store.get_active_workspace()?,
            generated_at: Utc::now(),
        })
    }

    pub fn run(&mut self) -> RuntimeStats {
        let mut stats = RuntimeStats::default();

        loop {
            match self.source.wait_for_next() {
                RuntimeEvent::Idle => {
                    stats.idle_cycles = stats.idle_cycles.saturating_add(1);
                }
                RuntimeEvent::Shutdown => return stats,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStats {
    pub idle_cycles: u64,
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
    }

    impl IngestionSource for ScriptedIngestionSource {
        fn wait_for_next(&mut self) -> RuntimeEvent {
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
        };
        let mut runtime = DaemonRuntime::new(store, source);

        let stats = runtime.run();

        assert_eq!(stats.idle_cycles, 2);
    }

    #[test]
    fn runtime_reports_health_from_owned_store() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let runtime = DaemonRuntime::new(store, ScriptedIngestionSource { events: Vec::new() });

        let response = runtime
            .health_response(Path::new("/tmp/blipcoard-test.db"))
            .expect("health response should be built");

        assert_eq!(response.service, "blipd");
        assert_eq!(response.status, "ready");
        assert_eq!(response.active_workspace.as_deref(), Some("inbox"));
    }
}

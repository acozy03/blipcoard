use blip_core::{Blip, BlipError, BlipStore, BlipSummary, NewBlip, NewWorkspace, Workspace};
use std::path::Path;

pub struct StoreCommandBackend {
    store: BlipStore,
}

impl StoreCommandBackend {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, BlipError> {
        Ok(Self {
            store: BlipStore::open(database_path)?,
        })
    }

    pub fn active_workspace(&self) -> Result<Option<String>, BlipError> {
        self.store.get_active_workspace()
    }

    pub fn workspaces(&self) -> Result<Vec<Workspace>, BlipError> {
        self.store.list_workspaces()
    }

    pub fn blip_summaries(
        &self,
        workspace: &str,
        limit: usize,
    ) -> Result<Vec<BlipSummary>, BlipError> {
        self.store.list_blip_summaries(workspace, limit)
    }

    pub fn create_workspace(&mut self, workspace: &NewWorkspace) -> Result<Workspace, BlipError> {
        self.store.create_workspace(workspace)
    }

    pub fn set_active_workspace(&mut self, workspace: &str) -> Result<(), BlipError> {
        self.store.set_active_workspace(workspace)
    }

    pub fn insert_blip(&mut self, blip: &NewBlip) -> Result<Blip, BlipError> {
        self.store.insert_blip(blip)
    }
}

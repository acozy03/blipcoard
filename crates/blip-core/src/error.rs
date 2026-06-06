use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlipError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("invalid persisted value for {field}: {value}")]
    InvalidPersistedValue { field: &'static str, value: String },

    #[error("workspace `{0}` does not exist")]
    WorkspaceNotFound(String),

    #[error("workspace `{0}` already exists")]
    WorkspaceAlreadyExists(String),

    #[error("active workspace is not set")]
    ActiveWorkspaceNotSet,
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlipError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("workspace `{0}` does not exist")]
    WorkspaceNotFound(String),

    #[error("active workspace is not set")]
    ActiveWorkspaceNotSet,
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlipError {
    #[error("database error: {0}")]
    Database(rusqlite::Error),

    #[error("database is busy; retry after the current blipcoard operation finishes")]
    DatabaseBusy,

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("invalid persisted value for {field}: {value}")]
    InvalidPersistedValue { field: &'static str, value: String },

    #[error("invalid {field}: {reason}")]
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },

    #[error("invalid search query: {0}")]
    InvalidSearchQuery(String),

    #[error("workspace `{0}` does not exist")]
    WorkspaceNotFound(String),

    #[error("workspace `{0}` already exists")]
    WorkspaceAlreadyExists(String),

    #[error("blip `{0}` does not exist")]
    BlipNotFound(String),

    #[error("inbox is empty")]
    InboxEmpty,

    #[error("agent access to workspace `{0}` is denied")]
    AgentAccessDenied(String),

    #[error("active workspace is not set")]
    ActiveWorkspaceNotSet,
}

impl From<rusqlite::Error> for BlipError {
    fn from(error: rusqlite::Error) -> Self {
        if is_sqlite_busy_error(&error) {
            Self::DatabaseBusy
        } else {
            Self::Database(error)
        }
    }
}

pub(crate) fn is_sqlite_busy_error(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(error, _)
            if matches!(
                error.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

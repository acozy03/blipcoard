use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
    pub database_path: String,
    pub active_workspace: Option<String>,
    pub generated_at: DateTime<Utc>,
}

use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

const JOIN_CODE_PREFIX: &str = "BLIP";
const DEFAULT_JOIN_CODE_TTL_HOURS: i64 = 24;
const DEFAULT_MAX_USES: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedRole {
    Owner,
    Editor,
    Viewer,
}

impl HostedRole {
    pub const fn can_manage_workspace(self) -> bool {
        matches!(self, Self::Owner)
    }

    pub const fn can_manage_members(self) -> bool {
        matches!(self, Self::Owner)
    }

    pub const fn can_create_join_codes(self) -> bool {
        matches!(self, Self::Owner)
    }

    pub const fn can_publish_blips(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    pub const fn can_update_tags(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    pub const fn can_read_blips(self) -> bool {
        matches!(self, Self::Owner | Self::Editor | Self::Viewer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedMemberStatus {
    Active,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedMember {
    pub id: String,
    pub workspace_id: String,
    pub display_name: String,
    pub role: HostedRole,
    pub status: HostedMemberStatus,
    pub joined_at: DateTime<Utc>,
    pub removed_at: Option<DateTime<Utc>>,
}

impl HostedMember {
    pub fn new(
        workspace_id: impl Into<String>,
        display_name: impl Into<String>,
        role: HostedRole,
        joined_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: hosted_id("hm"),
            workspace_id: workspace_id.into(),
            display_name: display_name.into(),
            role,
            status: HostedMemberStatus::Active,
            joined_at,
            removed_at: None,
        }
    }

    pub fn remove(&mut self, removed_at: DateTime<Utc>) -> HostedAuditEvent {
        self.status = HostedMemberStatus::Removed;
        self.removed_at = Some(removed_at);
        HostedAuditEvent::new(
            self.workspace_id.clone(),
            HostedAuditEventType::MemberRemoved,
            Some(self.id.clone()),
            removed_at,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedJoinCode {
    pub id: String,
    pub workspace_id: String,
    pub code_hash: String,
    pub role: HostedRole,
    pub created_by_member_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub max_uses: u32,
    pub use_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewHostedJoinCode {
    pub join_code: HostedJoinCode,
    pub raw_code: String,
    pub audit_event: HostedAuditEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinCodeOptions {
    pub role: HostedRole,
    pub ttl: Duration,
    pub max_uses: u32,
}

impl Default for JoinCodeOptions {
    fn default() -> Self {
        Self {
            role: HostedRole::Editor,
            ttl: Duration::hours(DEFAULT_JOIN_CODE_TTL_HOURS),
            max_uses: DEFAULT_MAX_USES,
        }
    }
}

impl HostedJoinCode {
    pub fn create(
        workspace_id: impl Into<String>,
        created_by_member_id: impl Into<String>,
        now: DateTime<Utc>,
        options: JoinCodeOptions,
    ) -> Result<NewHostedJoinCode, HostedSyncError> {
        if options.ttl <= Duration::zero() {
            return Err(HostedSyncError::InvalidJoinCodeTtl);
        }
        if options.max_uses == 0 {
            return Err(HostedSyncError::InvalidJoinCodeMaxUses);
        }

        let workspace_id = workspace_id.into();
        let raw_code = generate_join_code();
        let join_code = Self {
            id: hosted_id("jc"),
            workspace_id: workspace_id.clone(),
            code_hash: hash_join_code(&raw_code),
            role: options.role,
            created_by_member_id: created_by_member_id.into(),
            created_at: now,
            expires_at: now + options.ttl,
            revoked_at: None,
            max_uses: options.max_uses,
            use_count: 0,
        };
        let audit_event = HostedAuditEvent::new(
            workspace_id,
            HostedAuditEventType::JoinCodeCreated,
            Some(join_code.id.clone()),
            now,
        );

        Ok(NewHostedJoinCode {
            join_code,
            raw_code,
            audit_event,
        })
    }

    pub fn revoke(&mut self, now: DateTime<Utc>) -> HostedAuditEvent {
        self.revoked_at = Some(now);
        HostedAuditEvent::new(
            self.workspace_id.clone(),
            HostedAuditEventType::JoinCodeRevoked,
            Some(self.id.clone()),
            now,
        )
    }

    pub fn redeem(
        &mut self,
        raw_code: &str,
        display_name: impl Into<String>,
        now: DateTime<Utc>,
        rate_limit: RateLimitDecision,
    ) -> Result<JoinCodeRedemption, HostedSyncError> {
        if let RateLimitDecision::Deny {
            retry_after_seconds,
        } = rate_limit
        {
            let audit_event =
                self.failed_redeem_event(HostedAuditEventType::JoinCodeRateLimited, now);
            return Err(HostedSyncError::RateLimited {
                retry_after_seconds,
                audit_event,
            });
        }
        if self.code_hash != hash_join_code(raw_code) {
            let audit_event = self.failed_redeem_event(HostedAuditEventType::JoinCodeInvalid, now);
            return Err(HostedSyncError::InvalidJoinCode { audit_event });
        }
        if self.revoked_at.is_some() {
            let audit_event = self.failed_redeem_event(HostedAuditEventType::JoinCodeRejected, now);
            return Err(HostedSyncError::JoinCodeRevoked { audit_event });
        }
        if now >= self.expires_at {
            let audit_event = self.failed_redeem_event(HostedAuditEventType::JoinCodeRejected, now);
            return Err(HostedSyncError::JoinCodeExpired { audit_event });
        }
        if self.use_count >= self.max_uses {
            let audit_event = self.failed_redeem_event(HostedAuditEventType::JoinCodeRejected, now);
            return Err(HostedSyncError::JoinCodeExhausted { audit_event });
        }

        self.use_count += 1;
        let member = HostedMember::new(self.workspace_id.clone(), display_name, self.role, now);
        let audit_event = HostedAuditEvent::new(
            self.workspace_id.clone(),
            HostedAuditEventType::MemberJoined,
            Some(member.id.clone()),
            now,
        );

        Ok(JoinCodeRedemption {
            member,
            audit_event,
        })
    }

    fn failed_redeem_event(
        &self,
        event_type: HostedAuditEventType,
        now: DateTime<Utc>,
    ) -> HostedAuditEvent {
        HostedAuditEvent::new(
            self.workspace_id.clone(),
            event_type,
            Some(self.id.clone()),
            now,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCodeRedemption {
    pub member: HostedMember,
    pub audit_event: HostedAuditEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allow,
    Deny { retry_after_seconds: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedAuditEventType {
    JoinCodeCreated,
    JoinCodeRevoked,
    JoinCodeInvalid,
    JoinCodeRejected,
    JoinCodeRateLimited,
    MemberJoined,
    MemberRemoved,
    MemberRoleChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAuditEvent {
    pub id: String,
    pub workspace_id: String,
    pub event_type: HostedAuditEventType,
    pub target_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiEnvelope<T> {
    pub request_id: String,
    pub status: String,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

impl<T> ApiEnvelope<T> {
    pub fn ok(request_id: impl Into<String>, data: T) -> Self {
        Self {
            request_id: request_id.into(),
            status: "ok".to_string(),
            data: Some(data),
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub owner_display_name: Option<String>,
    pub retention_days: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspaceResponse {
    pub workspace: HostedWorkspaceSummary,
    pub member: MemberSummary,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedWorkspaceSummary {
    pub id: String,
    pub name: String,
    pub created_by_member_id: String,
    pub created_at: DateTime<Utc>,
    pub retention_days: Option<i64>,
    pub default_role: HostedRole,
    pub last_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberSummary {
    pub id: String,
    pub workspace_id: String,
    pub display_name: String,
    pub role: HostedRole,
    pub status: String,
    pub joined_at: DateTime<Utc>,
}

impl From<HostedMember> for MemberSummary {
    fn from(member: HostedMember) -> Self {
        Self {
            id: member.id,
            workspace_id: member.workspace_id,
            display_name: member.display_name,
            role: member.role,
            status: "active".to_string(),
            joined_at: member.joined_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateJoinCodeRequest {
    pub created_by_member_id: String,
    pub role: HostedRole,
    pub expires_in_seconds: Option<i64>,
    pub max_uses: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateJoinCodeResponse {
    pub join_code: JoinCodeSummary,
    pub raw_code: String,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCodeSummary {
    pub id: String,
    pub workspace_id: String,
    pub role: HostedRole,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub max_uses: u32,
    pub use_count: u32,
}

impl From<HostedJoinCode> for JoinCodeSummary {
    fn from(join_code: HostedJoinCode) -> Self {
        Self {
            id: join_code.id,
            workspace_id: join_code.workspace_id,
            role: join_code.role,
            expires_at: join_code.expires_at,
            revoked_at: join_code.revoked_at,
            max_uses: join_code.max_uses,
            use_count: join_code.use_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinWorkspaceRequest {
    pub code: String,
    pub display_name: String,
    pub device_label: Option<String>,
    pub client_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinWorkspaceResponse {
    pub workspace: HostedWorkspaceSummary,
    pub member: MemberSummary,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishBlipRequest {
    pub publisher_member_id: String,
    pub local_blip_id: String,
    pub content_type: String,
    pub content: String,
    pub preview: String,
    pub size_bytes: i64,
    pub is_redacted: bool,
    pub tags: Vec<String>,
    pub captured_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishBlipResponse {
    pub blip: HostedBlipSummary,
    pub event: WorkspaceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedBlipSummary {
    pub id: String,
    pub workspace_id: String,
    pub local_blip_id: String,
    pub publisher_member_id: String,
    pub content_type: String,
    pub content: String,
    pub preview: String,
    pub size_bytes: i64,
    pub is_redacted: bool,
    pub tags: Vec<String>,
    pub captured_at: Option<DateTime<Utc>>,
    pub published_at: DateTime<Utc>,
    pub sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEvent {
    pub id: String,
    pub workspace_id: String,
    pub sequence: i64,
    pub event_type: String,
    pub actor_member_id: Option<String>,
    pub target_id: Option<String>,
    pub data: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventQuery {
    #[serde(default)]
    pub after_sequence: i64,
}

#[derive(Debug, Clone)]
pub struct HostedClient {
    base_url: String,
    http: Client,
}

impl HostedClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, HostedClientError> {
        let base_url = normalize_base_url(base_url.into())?;
        Ok(Self {
            base_url,
            http: Client::new(),
        })
    }

    pub fn join_workspace(
        &self,
        request: &JoinWorkspaceRequest,
    ) -> Result<JoinWorkspaceResponse, HostedClientError> {
        self.post("/v1/join", request)
    }

    pub fn create_workspace(
        &self,
        request: &CreateWorkspaceRequest,
    ) -> Result<CreateWorkspaceResponse, HostedClientError> {
        self.post("/v1/workspaces", request)
    }

    pub fn create_join_code(
        &self,
        workspace_id: &str,
        request: &CreateJoinCodeRequest,
    ) -> Result<CreateJoinCodeResponse, HostedClientError> {
        self.post(
            &format!("/v1/workspaces/{workspace_id}/join-codes"),
            request,
        )
    }

    pub fn publish_blip(
        &self,
        workspace_id: &str,
        request: &PublishBlipRequest,
    ) -> Result<PublishBlipResponse, HostedClientError> {
        self.post(&format!("/v1/workspaces/{workspace_id}/blips"), request)
    }

    fn post<T, R>(&self, path: &str, request: &T) -> Result<R, HostedClientError>
    where
        T: Serialize + ?Sized,
        R: for<'de> Deserialize<'de>,
    {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .json(request)
            .send()?;
        let status = response.status();
        let envelope = response.json::<ApiEnvelope<R>>()?;
        if !status.is_success() || envelope.status != "ok" {
            let error = envelope.error.unwrap_or(ApiError {
                code: status.as_u16().to_string(),
                message: format!("hosted service returned HTTP {status}"),
            });
            return Err(HostedClientError::Api(error));
        }
        envelope.data.ok_or(HostedClientError::MissingData)
    }
}

#[derive(Debug, Error)]
pub enum HostedClientError {
    #[error("hosted service URL must start with http:// or https://")]
    InvalidBaseUrl,
    #[error("hosted HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("hosted API error {code}: {message}", code = .0.code, message = .0.message)]
    Api(ApiError),
    #[error("hosted API returned success without data")]
    MissingData,
}

impl HostedAuditEvent {
    pub fn new(
        workspace_id: impl Into<String>,
        event_type: HostedAuditEventType,
        target_id: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: hosted_id("ha"),
            workspace_id: workspace_id.into(),
            event_type,
            target_id,
            created_at,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostedSyncError {
    #[error("join code ttl must be positive")]
    InvalidJoinCodeTtl,
    #[error("join code max uses must be greater than zero")]
    InvalidJoinCodeMaxUses,
    #[error("join code is invalid")]
    InvalidJoinCode { audit_event: HostedAuditEvent },
    #[error("join code is expired")]
    JoinCodeExpired { audit_event: HostedAuditEvent },
    #[error("join code is revoked")]
    JoinCodeRevoked { audit_event: HostedAuditEvent },
    #[error("join code has no remaining uses")]
    JoinCodeExhausted { audit_event: HostedAuditEvent },
    #[error("join-code redemption is rate limited; retry after {retry_after_seconds}s")]
    RateLimited {
        retry_after_seconds: u64,
        audit_event: HostedAuditEvent,
    },
}

pub fn hash_join_code(raw_code: &str) -> String {
    let normalized = normalize_join_code(raw_code);
    let digest = Sha256::digest(normalized.as_bytes());
    format!("{digest:x}")
}

pub fn normalize_join_code(raw_code: &str) -> String {
    raw_code
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .flat_map(char::to_uppercase)
        .collect()
}

fn generate_join_code() -> String {
    let hex = Uuid::new_v4().simple().to_string().to_uppercase();
    format!(
        "{JOIN_CODE_PREFIX}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..16],
        &hex[16..24],
        &hex[24..32]
    )
}

fn hosted_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn normalize_base_url(value: String) -> Result<String, HostedClientError> {
    let trimmed = value.trim().trim_end_matches('/').to_owned();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed)
    } else {
        Err(HostedClientError::InvalidBaseUrl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 3, 21, 0, 0)
            .single()
            .expect("valid fixed timestamp")
    }

    #[test]
    fn generated_join_code_has_blip_prefix_and_hash_only_storage() {
        let created = HostedJoinCode::create("hw_1", "hm_owner", now(), JoinCodeOptions::default())
            .expect("join code should be created");

        assert!(created.raw_code.starts_with("BLIP-"));
        assert_ne!(created.join_code.code_hash, created.raw_code);
        assert_eq!(created.join_code.role, HostedRole::Editor);
        assert_eq!(
            created.audit_event.event_type,
            HostedAuditEventType::JoinCodeCreated
        );
    }

    #[test]
    fn join_code_redeems_to_member_with_configured_role() {
        let created = HostedJoinCode::create(
            "hw_1",
            "hm_owner",
            now(),
            JoinCodeOptions {
                role: HostedRole::Viewer,
                ..JoinCodeOptions::default()
            },
        )
        .expect("join code should be created");
        let mut join_code = created.join_code;

        let redemption = join_code
            .redeem(
                &created.raw_code,
                "Sam",
                now() + Duration::minutes(1),
                RateLimitDecision::Allow,
            )
            .expect("join code should redeem");

        assert_eq!(redemption.member.workspace_id, "hw_1");
        assert_eq!(redemption.member.display_name, "Sam");
        assert_eq!(redemption.member.role, HostedRole::Viewer);
        assert_eq!(join_code.use_count, 1);
        assert_eq!(
            redemption.audit_event.event_type,
            HostedAuditEventType::MemberJoined
        );
    }

    #[test]
    fn expired_join_code_fails_with_audit_event() {
        let created = HostedJoinCode::create(
            "hw_1",
            "hm_owner",
            now(),
            JoinCodeOptions {
                ttl: Duration::minutes(5),
                ..JoinCodeOptions::default()
            },
        )
        .expect("join code should be created");
        let mut join_code = created.join_code;

        let error = join_code
            .redeem(
                &created.raw_code,
                "Sam",
                now() + Duration::minutes(6),
                RateLimitDecision::Allow,
            )
            .expect_err("expired code should fail");

        assert!(matches!(
            error,
            HostedSyncError::JoinCodeExpired {
                audit_event: HostedAuditEvent {
                    event_type: HostedAuditEventType::JoinCodeRejected,
                    ..
                }
            }
        ));
        assert_eq!(join_code.use_count, 0);
    }

    #[test]
    fn revoked_join_code_fails_with_audit_event() {
        let created = HostedJoinCode::create("hw_1", "hm_owner", now(), JoinCodeOptions::default())
            .expect("join code should be created");
        let mut join_code = created.join_code;
        let revoke_event = join_code.revoke(now() + Duration::minutes(1));

        assert_eq!(
            revoke_event.event_type,
            HostedAuditEventType::JoinCodeRevoked
        );

        let error = join_code
            .redeem(
                &created.raw_code,
                "Sam",
                now() + Duration::minutes(2),
                RateLimitDecision::Allow,
            )
            .expect_err("revoked code should fail");

        assert!(matches!(error, HostedSyncError::JoinCodeRevoked { .. }));
        assert_eq!(join_code.use_count, 0);
    }

    #[test]
    fn invalid_code_and_rate_limit_do_not_increment_use_count() {
        let created = HostedJoinCode::create("hw_1", "hm_owner", now(), JoinCodeOptions::default())
            .expect("join code should be created");
        let mut join_code = created.join_code;

        let invalid_error = join_code
            .redeem(
                "BLIP-wrong",
                "Sam",
                now() + Duration::minutes(1),
                RateLimitDecision::Allow,
            )
            .expect_err("wrong code should fail");
        assert!(matches!(
            invalid_error,
            HostedSyncError::InvalidJoinCode {
                audit_event: HostedAuditEvent {
                    event_type: HostedAuditEventType::JoinCodeInvalid,
                    ..
                }
            }
        ));

        let limited_error = join_code
            .redeem(
                &created.raw_code,
                "Sam",
                now() + Duration::minutes(1),
                RateLimitDecision::Deny {
                    retry_after_seconds: 60,
                },
            )
            .expect_err("rate-limited code should fail");
        assert!(matches!(
            limited_error,
            HostedSyncError::RateLimited {
                retry_after_seconds: 60,
                audit_event: HostedAuditEvent {
                    event_type: HostedAuditEventType::JoinCodeRateLimited,
                    ..
                }
            }
        ));

        assert_eq!(join_code.use_count, 0);
    }

    #[test]
    fn exhausted_join_code_fails_after_max_uses() {
        let created = HostedJoinCode::create(
            "hw_1",
            "hm_owner",
            now(),
            JoinCodeOptions {
                max_uses: 1,
                ..JoinCodeOptions::default()
            },
        )
        .expect("join code should be created");
        let mut join_code = created.join_code;

        join_code
            .redeem(
                &created.raw_code,
                "Sam",
                now() + Duration::minutes(1),
                RateLimitDecision::Allow,
            )
            .expect("first redemption should pass");
        let error = join_code
            .redeem(
                &created.raw_code,
                "Pat",
                now() + Duration::minutes(2),
                RateLimitDecision::Allow,
            )
            .expect_err("second redemption should fail");

        assert!(matches!(error, HostedSyncError::JoinCodeExhausted { .. }));
        assert_eq!(join_code.use_count, 1);
    }

    #[test]
    fn role_permissions_match_mvp_policy() {
        assert!(HostedRole::Owner.can_manage_workspace());
        assert!(HostedRole::Owner.can_create_join_codes());
        assert!(HostedRole::Editor.can_publish_blips());
        assert!(HostedRole::Editor.can_update_tags());
        assert!(HostedRole::Viewer.can_read_blips());
        assert!(!HostedRole::Viewer.can_publish_blips());
        assert!(!HostedRole::Editor.can_create_join_codes());
    }

    #[test]
    fn hosted_client_normalizes_base_url() {
        assert_eq!(
            normalize_base_url(" http://127.0.0.1:8732/ ".to_string()).expect("valid base URL"),
            "http://127.0.0.1:8732"
        );
        assert!(matches!(
            normalize_base_url("127.0.0.1:8732".to_string()),
            Err(HostedClientError::InvalidBaseUrl)
        ));
    }
}

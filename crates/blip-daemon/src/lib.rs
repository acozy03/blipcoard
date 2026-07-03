//! Daemon runtime ownership for clipboard ingestion.
//!
//! `blipd` owns the long-running ingestion loop. Platform clipboard code should
//! feed this runtime through an ingestion source; it should not own storage,
//! policy, or process lifecycle decisions.

pub mod ipc;

use blip_api::{
    AgentBlip, AgentBlipListResponse, AgentBundleResponse, AuditEventListResponse,
    AuditEventSummary, BlipDetail, BlipListResponse, BlipRoutedResponse, BlipSummary,
    CurrentWorkspaceResponse, DAEMON_API_VERSION, DaemonApiError, DaemonApiErrorCode,
    DaemonCommand, DaemonRequest, DaemonRequestPayload, DaemonResponse, DaemonResponsePayload,
    DaemonVersionResponse, HealthResponse, WorkspaceListResponse, WorkspaceSummary,
};
use blip_clipboard::{
    ClipboardError, ClipboardFileList, ClipboardImage, ClipboardPayload, ClipboardRichText,
    ClipboardUnknown, ClipboardWatcher,
};
use blip_core::{AuditEvent, Blip};
use blip_core::{
    BlipError, BlipStore, ContentType, LocalBlobStore, NewBlip, NewClipboardMetadataPayload,
    NewClipboardPayload, PayloadKind,
};
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
            (
                DaemonCommand::SetStickyCapture,
                DaemonRequestPayload::SetStickyCapture { workspace, enabled },
            ) => match self.store.set_sticky_capture(&workspace, enabled) {
                Ok(workspace) => DaemonResponse::ok(
                    request_id,
                    command,
                    DaemonResponsePayload::StickyCaptureSet(workspace_summary(workspace)),
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
                            workspaces: workspaces.into_iter().map(workspace_summary).collect(),
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
                            blips: blips.into_iter().map(blip_summary).collect(),
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
            (
                DaemonCommand::SearchBlips,
                DaemonRequestPayload::SearchBlips {
                    workspace,
                    query,
                    limit,
                },
            ) => match self.store.search_blip_summaries(&workspace, &query, limit) {
                Ok(blips) => DaemonResponse::ok(
                    request_id,
                    command,
                    DaemonResponsePayload::Blips(BlipListResponse {
                        workspace,
                        blips: blips.into_iter().map(blip_summary).collect(),
                    }),
                ),
                Err(error) => search_error_response(request_id, command, error),
            },
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
            (DaemonCommand::ListAuditEvents, DaemonRequestPayload::ListAuditEvents { limit }) => {
                match self.store.list_audit_events_limited(limit) {
                    Ok(events) => DaemonResponse::ok(
                        request_id,
                        command,
                        DaemonResponsePayload::AuditEvents(AuditEventListResponse {
                            events: events.into_iter().map(audit_event_summary).collect(),
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
                DaemonCommand::AgentSearchBlips,
                DaemonRequestPayload::AgentSearchBlips {
                    workspace,
                    query,
                    limit,
                },
            ) => match self.store.search_agent_blips(&workspace, &query, limit) {
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
                DaemonCommand::AgentBundle,
                DaemonRequestPayload::AgentBundle { workspace, limit },
            ) => match self.store.list_agent_blips(&workspace, limit) {
                Ok(blips) => {
                    let blip_count = blips.len();
                    DaemonResponse::ok(
                        request_id,
                        command,
                        DaemonResponsePayload::AgentBundle(AgentBundleResponse {
                            content: render_agent_bundle(&workspace, &blips),
                            workspace,
                            format: "markdown".to_owned(),
                            blip_count,
                        }),
                    )
                }
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
                RuntimeEvent::ClipboardImageChanged { image } => {
                    self.ingest_clipboard_image(image)?;
                }
                RuntimeEvent::ClipboardFileListChanged { file_list } => {
                    self.ingest_clipboard_file_list(file_list)?;
                }
                RuntimeEvent::ClipboardHtmlChanged { rich_text } => {
                    self.ingest_clipboard_rich_text(PayloadKind::Html, rich_text)?;
                }
                RuntimeEvent::ClipboardRtfChanged { rich_text } => {
                    self.ingest_clipboard_rich_text(PayloadKind::Rtf, rich_text)?;
                }
                RuntimeEvent::ClipboardUnknownChanged { unknown } => {
                    self.ingest_clipboard_unknown(unknown)?;
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
            workspace_name: self
                .store
                .get_sticky_workspace()?
                .unwrap_or_else(|| INBOX_WORKSPACE.to_owned()),
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

    fn ingest_clipboard_image(&mut self, image: ClipboardImage) -> Result<(), DaemonError> {
        let workspace_name = self
            .store
            .get_sticky_workspace()?
            .unwrap_or_else(|| INBOX_WORKSPACE.to_owned());
        let content = format!(
            "Image clipboard payload: {}x{} {} ({} bytes)",
            image.width, image.height, image.mime_type, image.byte_size
        );
        let blip = self.store.insert_blip(&NewBlip {
            workspace_name,
            source_app: None,
            content_type: ContentType::PlainText,
            language: None,
            content,
            token_estimate: None,
            is_redacted: false,
            tags: vec!["clipboard:image".to_owned(), "rich:clipboard".to_owned()],
        })?;

        let blob_store = LocalBlobStore::new(self.blob_data_dir());
        self.store.insert_blob_payload(
            &blip.id,
            &NewClipboardPayload {
                kind: PayloadKind::Image,
                mime_type: Some(image.mime_type),
                platform_format: image.platform_format,
                source_app: None,
                preview_ref: None,
                inline_text: None,
                metadata: serde_json::json!({
                    "width": image.width,
                    "height": image.height,
                }),
                bytes: image.bytes,
            },
            &blob_store,
        )?;

        Ok(())
    }

    fn ingest_clipboard_file_list(
        &mut self,
        file_list: ClipboardFileList,
    ) -> Result<(), DaemonError> {
        let workspace_name = self
            .store
            .get_sticky_workspace()?
            .unwrap_or_else(|| INBOX_WORKSPACE.to_owned());
        let content = format!(
            "File-list clipboard payload: {} path{}",
            file_list.paths.len(),
            if file_list.paths.len() == 1 { "" } else { "s" }
        );
        let blip = self.store.insert_blip(&NewBlip {
            workspace_name,
            source_app: None,
            content_type: ContentType::PlainText,
            language: None,
            content,
            token_estimate: None,
            is_redacted: false,
            tags: vec![
                "clipboard:file-list".to_owned(),
                "rich:clipboard".to_owned(),
            ],
        })?;

        self.store.insert_metadata_payload(
            &blip.id,
            &NewClipboardMetadataPayload {
                kind: PayloadKind::FileList,
                mime_type: Some("text/uri-list".to_owned()),
                platform_format: file_list.platform_format,
                source_app: None,
                preview_ref: None,
                inline_text: None,
                metadata: serde_json::json!({
                    "policy": "metadata_only",
                    "path_count": file_list.paths.len(),
                    "paths": file_list
                        .paths
                        .iter()
                        .map(|path| path_metadata(path))
                        .collect::<Vec<_>>(),
                }),
                byte_size: i64::try_from(file_list.byte_size).map_err(|_| {
                    BlipError::InvalidInput {
                        field: "payload.byte_size",
                        reason: "payload is too large for SQLite metadata",
                    }
                })?,
            },
        )?;

        Ok(())
    }

    fn ingest_clipboard_rich_text(
        &mut self,
        kind: PayloadKind,
        rich_text: ClipboardRichText,
    ) -> Result<(), DaemonError> {
        let workspace_name = self
            .store
            .get_sticky_workspace()?
            .unwrap_or_else(|| INBOX_WORKSPACE.to_owned());
        let fallback = rich_text
            .plain_text
            .clone()
            .unwrap_or_else(|| format!("{} clipboard payload", rich_text.mime_type));
        let tag = match kind {
            PayloadKind::Html => "clipboard:html",
            PayloadKind::Rtf => "clipboard:rtf",
            _ => "clipboard:rich-text",
        };
        let blip = self.store.insert_blip(&NewBlip {
            workspace_name,
            source_app: None,
            content_type: ContentType::PlainText,
            language: None,
            content: fallback,
            token_estimate: None,
            is_redacted: false,
            tags: vec![tag.to_owned(), "rich:clipboard".to_owned()],
        })?;

        let blob_store = LocalBlobStore::new(self.blob_data_dir());
        self.store.insert_blob_payload(
            &blip.id,
            &NewClipboardPayload {
                kind,
                mime_type: Some(rich_text.mime_type),
                platform_format: rich_text.platform_format,
                source_app: None,
                preview_ref: None,
                inline_text: rich_text.plain_text,
                metadata: serde_json::json!({
                    "fallback": "plain_text",
                    "render_policy": "do_not_render_privileged",
                }),
                bytes: rich_text.bytes,
            },
            &blob_store,
        )?;

        Ok(())
    }

    fn ingest_clipboard_unknown(&mut self, unknown: ClipboardUnknown) -> Result<(), DaemonError> {
        let workspace_name = self
            .store
            .get_sticky_workspace()?
            .unwrap_or_else(|| INBOX_WORKSPACE.to_owned());
        let blip = self.store.insert_blip(&NewBlip {
            workspace_name,
            source_app: None,
            content_type: ContentType::PlainText,
            language: None,
            content: format!(
                "Unknown clipboard payload: {} ({} bytes)",
                unknown.platform_format, unknown.byte_size
            ),
            token_estimate: None,
            is_redacted: false,
            tags: vec!["clipboard:unknown".to_owned(), "rich:clipboard".to_owned()],
        })?;

        if unknown.bytes.is_empty() {
            self.store.insert_metadata_payload(
                &blip.id,
                &NewClipboardMetadataPayload {
                    kind: PayloadKind::Unknown,
                    mime_type: unknown.mime_type,
                    platform_format: Some(unknown.platform_format),
                    source_app: None,
                    preview_ref: None,
                    inline_text: None,
                    metadata: serde_json::json!({
                        "policy": "unsupported_format",
                    }),
                    byte_size: i64::try_from(unknown.byte_size).map_err(|_| {
                        BlipError::InvalidInput {
                            field: "payload.byte_size",
                            reason: "payload is too large for SQLite metadata",
                        }
                    })?,
                },
            )?;
        } else {
            let blob_store = LocalBlobStore::new(self.blob_data_dir());
            self.store.insert_blob_payload(
                &blip.id,
                &NewClipboardPayload {
                    kind: PayloadKind::Unknown,
                    mime_type: unknown.mime_type,
                    platform_format: Some(unknown.platform_format),
                    source_app: None,
                    preview_ref: None,
                    inline_text: None,
                    metadata: serde_json::json!({
                        "policy": "unsupported_format",
                    }),
                    bytes: unknown.bytes,
                },
                &blob_store,
            )?;
        }

        Ok(())
    }

    fn blob_data_dir(&self) -> PathBuf {
        self.database_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
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

fn blip_summary(blip: blip_core::BlipSummary) -> BlipSummary {
    BlipSummary {
        id: blip.id,
        preview: blip.preview,
        size_bytes: blip.size_bytes,
        is_redacted: blip.is_redacted,
        tags: blip.tags,
    }
}

fn render_agent_bundle(workspace: &str, blips: &[Blip]) -> String {
    let mut bundle = String::new();
    bundle.push_str("# blipcoard bundle\n\n");
    bundle.push_str(&format!("workspace: {workspace}\n"));
    bundle.push_str(&format!("blip_count: {}\n\n", blips.len()));

    for blip in blips {
        bundle.push_str(&format!("## blip {}\n\n", blip.id));
        bundle.push_str(&format!("content_type: {}\n", blip.content_type.as_str()));
        bundle.push_str(&format!("size_bytes: {}\n", blip.size_bytes));
        bundle.push_str(&format!("redacted: {}\n", blip.is_redacted));
        bundle.push_str("tags:");
        if blip.tags.is_empty() {
            bundle.push_str(" []\n\n");
        } else {
            bundle.push(' ');
            bundle.push_str(&blip.tags.join(","));
            bundle.push_str("\n\n");
        }

        if blip.is_redacted {
            bundle.push_str("[redacted]\n\n");
        } else {
            bundle.push_str(&blip.content);
            if !blip.content.ends_with('\n') {
                bundle.push('\n');
            }
            bundle.push('\n');
        }
    }

    bundle
}

fn audit_event_summary(event: AuditEvent) -> AuditEventSummary {
    AuditEventSummary {
        id: event.id,
        actor_type: event.actor_type.as_str().to_owned(),
        actor_id: event.actor_id,
        event_type: event.event_type.as_str().to_owned(),
        target_blip_id: event.target_blip_id,
        target_workspace: event.target_workspace,
        details_json: event.details_json,
        created_at: event.created_at,
    }
}

fn workspace_summary(workspace: blip_core::Workspace) -> WorkspaceSummary {
    WorkspaceSummary {
        name: workspace.name,
        agent_access: workspace.agent_access,
        sticky_capture: workspace.sticky_capture,
    }
}

fn path_metadata(path: &Path) -> serde_json::Value {
    serde_json::json!({
        "display_path": path.display().to_string(),
        "utf8_path": path.to_str(),
        "raw_encoding": path_raw_encoding(),
        "raw_hex": path_raw_hex(path),
    })
}

#[cfg(unix)]
fn path_raw_encoding() -> &'static str {
    "unix-bytes"
}

#[cfg(windows)]
fn path_raw_encoding() -> &'static str {
    "windows-utf16le"
}

#[cfg(not(any(unix, windows)))]
fn path_raw_encoding() -> &'static str {
    "display-only"
}

#[cfg(unix)]
fn path_raw_hex(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    Some(hex_encode(path.as_os_str().as_bytes()))
}

#[cfg(windows)]
fn path_raw_hex(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    let mut bytes = Vec::new();
    for unit in path.as_os_str().encode_wide() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    Some(hex_encode(&bytes))
}

#[cfg(not(any(unix, windows)))]
fn path_raw_hex(_path: &Path) -> Option<String> {
    None
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
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
        BlipError::InvalidInput { field, reason } => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::InvalidRequest,
                format!("{field} {reason}"),
            ),
        ),
        BlipError::InvalidSearchQuery(message) => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::InvalidRequest,
                format!("invalid search query: {message}"),
            ),
        ),
        error => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(DaemonApiErrorCode::StoreUnavailable, error.to_string()),
        ),
    }
}

fn search_error_response(
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
        BlipError::InvalidInput { field, reason } => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::InvalidRequest,
                format!("{field} {reason}"),
            ),
        ),
        BlipError::InvalidSearchQuery(message) => DaemonResponse::error(
            request_id,
            command,
            DaemonApiError::new(
                DaemonApiErrorCode::InvalidRequest,
                format!("invalid search query: {message}"),
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
    ClipboardImageChanged { image: ClipboardImage },
    ClipboardFileListChanged { file_list: ClipboardFileList },
    ClipboardHtmlChanged { rich_text: ClipboardRichText },
    ClipboardRtfChanged { rich_text: ClipboardRichText },
    ClipboardUnknownChanged { unknown: ClipboardUnknown },
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

        match event.payload {
            ClipboardPayload::Text(text) => Ok(RuntimeEvent::ClipboardTextChanged { text }),
            ClipboardPayload::Image(image) => Ok(RuntimeEvent::ClipboardImageChanged { image }),
            ClipboardPayload::FileList(file_list) => {
                Ok(RuntimeEvent::ClipboardFileListChanged { file_list })
            }
            ClipboardPayload::Html(rich_text) => {
                Ok(RuntimeEvent::ClipboardHtmlChanged { rich_text })
            }
            ClipboardPayload::Rtf(rich_text) => Ok(RuntimeEvent::ClipboardRtfChanged { rich_text }),
            ClipboardPayload::Unknown(unknown) => {
                Ok(RuntimeEvent::ClipboardUnknownChanged { unknown })
            }
        }
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
                    is_redacted: false,
                    tags: vec!["type:plain_text".to_owned()],
                }],
            })),
        );
    }

    #[test]
    fn dispatch_returns_search_blips_payload() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        let inserted = store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".to_owned(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "login callback timeout".to_owned(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("blip should be inserted");
        store
            .insert_blip(&NewBlip {
                workspace_name: "inbox".to_owned(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "billing webhook timeout".to_owned(),
                token_estimate: None,
                is_redacted: false,
                tags: Vec::new(),
            })
            .expect("nonmatching blip should be inserted");
        let mut runtime = DaemonRuntime::new(
            "/tmp/blipcoard-test.db",
            store,
            ScriptedIngestionSource {
                events: Vec::new(),
                calls: 0,
            },
        );

        let response = runtime.dispatch_daemon_request(DaemonRequest::new(
            "search-1",
            DaemonCommand::SearchBlips,
            DaemonRequestPayload::SearchBlips {
                workspace: "inbox".to_owned(),
                query: "login".to_owned(),
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
                    preview: "login callback timeout".to_owned(),
                    size_bytes: 22,
                    is_redacted: false,
                    tags: vec!["type:plain_text".to_owned()],
                }],
            })),
        );
    }

    #[test]
    fn dispatch_rejects_invalid_search_query() {
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
            "search-empty",
            DaemonCommand::SearchBlips,
            DaemonRequestPayload::SearchBlips {
                workspace: "inbox".to_owned(),
                query: " ".to_owned(),
                limit: 50,
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Error);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(DaemonApiErrorCode::InvalidRequest)
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
                tags: vec!["demo".to_owned(), "type:plain_text".to_owned()],
                created_at: inserted.created_at,
            })),
        );
    }

    #[test]
    fn dispatch_returns_recent_audit_events_payload() {
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
            "audit-events-1",
            DaemonCommand::ListAuditEvents,
            DaemonRequestPayload::ListAuditEvents { limit: 5 },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        match response.payload {
            Some(DaemonResponsePayload::AuditEvents(audit_events)) => {
                assert!(!audit_events.events.is_empty());
                assert!(
                    audit_events
                        .events
                        .iter()
                        .any(|event| event.event_type == "schema_initialized")
                );
            }
            other => panic!("expected audit events response, got {other:?}"),
        }
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
    fn dispatch_returns_agent_search_blips_for_agent_access_workspace() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&blip_core::NewWorkspace {
                name: "agent-feed".to_owned(),
                description: None,
                color: None,
                agent_access: true,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");
        let inserted = store
            .insert_blip(&NewBlip {
                workspace_name: "agent-feed".to_owned(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "deploy rollback note".to_owned(),
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
            "agent-search-1",
            DaemonCommand::AgentSearchBlips,
            DaemonRequestPayload::AgentSearchBlips {
                workspace: "agent-feed".to_owned(),
                query: "rollback".to_owned(),
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
                    content: "deploy rollback note".to_owned(),
                    size_bytes: 20,
                }],
            })),
        );
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
    fn dispatch_returns_agent_bundle_for_agent_access_workspace() {
        let mut store = BlipStore::in_memory().expect("store should initialize");
        store
            .create_workspace(&blip_core::NewWorkspace {
                name: "agent-feed".to_owned(),
                description: None,
                color: None,
                agent_access: true,
                sticky_capture: false,
                retention_days: None,
            })
            .expect("workspace should be created");
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
            "agent-bundle-1",
            DaemonCommand::AgentBundle,
            DaemonRequestPayload::AgentBundle {
                workspace: "agent-feed".to_owned(),
                limit: 50,
            },
        ));

        assert_eq!(response.status, blip_api::DaemonResponseStatus::Ok);
        match response.payload {
            Some(DaemonResponsePayload::AgentBundle(bundle)) => {
                assert_eq!(bundle.workspace, "agent-feed");
                assert_eq!(bundle.format, "markdown");
                assert_eq!(bundle.blip_count, 1);
                assert!(bundle.content.contains("# blipcoard bundle"));
                assert!(bundle.content.contains(&format!("## blip {}", inserted.id)));
                assert!(bundle.content.contains("agent-visible note"));
            }
            other => panic!("expected bundle response, got {other:?}"),
        }
    }

    #[test]
    fn rendered_agent_bundle_omits_redacted_content() {
        let bundle = render_agent_bundle(
            "agent-feed",
            &[Blip {
                id: "redacted-blip".to_owned(),
                workspace_name: "agent-feed".to_owned(),
                source_app: None,
                content_type: ContentType::PlainText,
                language: None,
                content: "do not include".to_owned(),
                size_bytes: 14,
                token_estimate: None,
                is_redacted: true,
                tags: vec!["secret".to_owned()],
                created_at: Utc::now(),
            }],
        );

        assert!(bundle.contains("[redacted]"));
        assert!(!bundle.contains("do not include"));
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
            events: vec![Some(ClipboardEvent::text("copied text"))],
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
    fn clipboard_source_maps_image_events_to_runtime_events() {
        let image = sample_clipboard_image();
        let watcher = ScriptedClipboardWatcher {
            events: vec![Some(ClipboardEvent::image(image.clone()))],
        };
        let mut source = ClipboardIngestionSource::new(watcher, Duration::ZERO);

        assert_eq!(
            source
                .wait_for_next()
                .expect("clipboard source should poll watcher"),
            RuntimeEvent::ClipboardImageChanged { image }
        );
    }

    #[test]
    fn clipboard_source_maps_rich_events_to_runtime_events() {
        let file_list = sample_file_list();
        let html = sample_html();
        let rtf = sample_rtf();
        let unknown = sample_unknown();
        let watcher = ScriptedClipboardWatcher {
            events: vec![
                Some(ClipboardEvent::unknown(unknown.clone())),
                Some(ClipboardEvent::rtf(rtf.clone())),
                Some(ClipboardEvent::html(html.clone())),
                Some(ClipboardEvent::file_list(file_list.clone())),
            ],
        };
        let mut source = ClipboardIngestionSource::new(watcher, Duration::ZERO);

        assert_eq!(
            source
                .wait_for_next()
                .expect("clipboard source should poll watcher"),
            RuntimeEvent::ClipboardFileListChanged { file_list }
        );
        assert_eq!(
            source
                .wait_for_next()
                .expect("clipboard source should poll watcher"),
            RuntimeEvent::ClipboardHtmlChanged { rich_text: html }
        );
        assert_eq!(
            source
                .wait_for_next()
                .expect("clipboard source should poll watcher"),
            RuntimeEvent::ClipboardRtfChanged { rich_text: rtf }
        );
        assert_eq!(
            source
                .wait_for_next()
                .expect("clipboard source should poll watcher"),
            RuntimeEvent::ClipboardUnknownChanged { unknown }
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
    fn runtime_persists_clipboard_image_events_with_blob_payload() {
        let root = unique_temp_dir("blipcoard-image");
        std::fs::create_dir_all(&root).expect("temp root should create");
        let db_path = root.join("blipcoard.db");
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardImageChanged {
                    image: sample_clipboard_image(),
                },
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new(&db_path, store, source);

        runtime
            .run()
            .expect("runtime should ingest clipboard image");

        let blips = runtime
            .store
            .list_blips("inbox")
            .expect("inbox blips should be listed");
        assert_eq!(blips.len(), 1);
        assert!(blips[0].content.contains("Image clipboard payload"));
        assert!(blips[0].tags.contains(&"clipboard:image".to_owned()));

        let payloads = runtime
            .store
            .get_blip_payloads(&blips[0].id)
            .expect("payloads should list");
        let image_payload = payloads
            .iter()
            .find(|payload| payload.kind == PayloadKind::Image)
            .expect("image payload should be stored");
        assert_eq!(image_payload.mime_type.as_deref(), Some("image/png"));
        assert_eq!(image_payload.byte_size, 8);
        assert_eq!(image_payload.metadata["width"], 1);
        assert_eq!(image_payload.metadata["height"], 1);
        assert!(image_payload.blob_ref.is_some());

        let blob_store = LocalBlobStore::new(&root);
        assert!(
            blob_store
                .exists(
                    image_payload
                        .blob_ref
                        .as_deref()
                        .expect("blob ref should exist")
                )
                .expect("blob existence should be readable")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_persists_file_list_events_as_metadata_only_payloads() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardFileListChanged {
                    file_list: sample_file_list(),
                },
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new("/tmp/blipcoard-test.db", store, source);

        runtime
            .run()
            .expect("runtime should ingest clipboard file list");

        let blips = runtime
            .store
            .list_blips("inbox")
            .expect("inbox blips should be listed");
        assert_eq!(blips.len(), 1);
        assert!(blips[0].content.contains("File-list clipboard payload"));
        assert!(blips[0].tags.contains(&"clipboard:file-list".to_owned()));

        let payloads = runtime
            .store
            .get_blip_payloads(&blips[0].id)
            .expect("payloads should list");
        let payload = payloads
            .iter()
            .find(|payload| payload.kind == PayloadKind::FileList)
            .expect("file-list payload should be stored");
        assert_eq!(payload.mime_type.as_deref(), Some("text/uri-list"));
        assert_eq!(payload.platform_format.as_deref(), Some("test:file-list"));
        assert!(payload.blob_ref.is_none());
        assert!(payload.inline_text.is_none());
        assert_eq!(payload.metadata["policy"], "metadata_only");
        assert_eq!(payload.metadata["path_count"], 2);
        assert_eq!(
            payload.metadata["paths"][0]["display_path"],
            serde_json::Value::String("/tmp/a.txt".to_owned())
        );
        assert_eq!(
            payload.metadata["paths"][0]["utf8_path"],
            serde_json::Value::String("/tmp/a.txt".to_owned())
        );
        assert!(payload.metadata["paths"][0]["raw_hex"].is_string());
    }

    #[test]
    fn runtime_persists_html_and_rtf_with_plain_text_fallbacks() {
        let root = unique_temp_dir("blipcoard-rich-text");
        std::fs::create_dir_all(&root).expect("temp root should create");
        let db_path = root.join("blipcoard.db");
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardRtfChanged {
                    rich_text: sample_rtf(),
                },
                RuntimeEvent::ClipboardHtmlChanged {
                    rich_text: sample_html(),
                },
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new(&db_path, store, source);

        runtime
            .run()
            .expect("runtime should ingest rich text payloads");

        let blips = runtime
            .store
            .list_blips("inbox")
            .expect("inbox blips should be listed");
        assert_eq!(blips.len(), 2);
        assert!(blips.iter().any(|blip| blip.content == "Hello"));

        let blob_store = LocalBlobStore::new(&root);
        for blip in blips {
            let payloads = runtime
                .store
                .get_blip_payloads(&blip.id)
                .expect("payloads should list");
            let rich_payload = payloads
                .iter()
                .find(|payload| matches!(payload.kind, PayloadKind::Html | PayloadKind::Rtf))
                .expect("rich payload should be stored");
            assert_eq!(rich_payload.inline_text.as_deref(), Some("Hello"));
            assert_eq!(rich_payload.metadata["fallback"], "plain_text");
            let blob_ref = rich_payload
                .blob_ref
                .as_deref()
                .expect("rich payload should have blob ref");
            assert!(blob_store.exists(blob_ref).expect("blob should exist"));
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_persists_unknown_events_as_auditable_metadata() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardUnknownChanged {
                    unknown: sample_unknown(),
                },
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new("/tmp/blipcoard-test.db", store, source);

        runtime
            .run()
            .expect("runtime should ingest unknown payload");

        let blips = runtime
            .store
            .list_blips("inbox")
            .expect("inbox blips should be listed");
        assert_eq!(blips.len(), 1);
        assert!(blips[0].content.contains("Unknown clipboard payload"));
        assert!(blips[0].tags.contains(&"clipboard:unknown".to_owned()));

        let payloads = runtime
            .store
            .get_blip_payloads(&blips[0].id)
            .expect("payloads should list");
        let payload = payloads
            .iter()
            .find(|payload| payload.kind == PayloadKind::Unknown)
            .expect("unknown payload should be stored");
        assert!(payload.blob_ref.is_none());
        assert_eq!(
            payload.platform_format.as_deref(),
            Some("application/x-test")
        );
        assert_eq!(payload.metadata["policy"], "unsupported_format");
    }

    #[test]
    fn runtime_flags_secret_like_clipboard_text() {
        let store = BlipStore::in_memory().expect("store should initialize");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardTextChanged {
                    text: "api_key = abcdef1234567890".to_owned(),
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
        assert_eq!(blips[0].content, "api_key = abcdef1234567890");
        assert!(!blips[0].is_redacted);
        assert!(blips[0].tags.contains(&"secret".to_string()));
        assert!(blips[0].tags.contains(&"secret:assignment".to_string()));
    }

    #[test]
    fn runtime_persists_clipboard_text_events_into_sticky_workspace() {
        let mut store = store_with_workspace("auth-bug");
        store
            .set_sticky_capture("auth-bug", true)
            .expect("sticky capture should enable");
        let source = ScriptedIngestionSource {
            events: vec![
                RuntimeEvent::Shutdown,
                RuntimeEvent::ClipboardTextChanged {
                    text: "sticky copied text".to_owned(),
                },
            ],
            calls: 0,
        };
        let mut runtime = DaemonRuntime::new("/tmp/blipcoard-test.db", store, source);

        runtime.run().expect("runtime should ingest clipboard text");

        let auth_blips = runtime
            .store
            .list_blips("auth-bug")
            .expect("sticky workspace blips should list");
        assert_eq!(auth_blips.len(), 1);
        assert_eq!(auth_blips[0].content, "sticky copied text");

        let inbox_blips = runtime
            .store
            .list_blips("inbox")
            .expect("inbox blips should list");
        assert!(inbox_blips.is_empty());

        let audit_events = runtime
            .store
            .list_audit_events()
            .expect("audit events should be listed");
        assert!(audit_events.iter().any(|event| {
            event.event_type == blip_core::AuditEventType::BlipIngested
                && event.target_workspace.as_deref() == Some("auth-bug")
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

    fn sample_clipboard_image() -> ClipboardImage {
        ClipboardImage {
            bytes: vec![137, 80, 78, 71, 1, 2, 3, 4],
            mime_type: "image/png".to_owned(),
            width: 1,
            height: 1,
            byte_size: 8,
            platform_format: Some("test:image".to_owned()),
        }
    }

    fn sample_file_list() -> ClipboardFileList {
        ClipboardFileList {
            paths: vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
            byte_size: 20,
            platform_format: Some("test:file-list".to_owned()),
        }
    }

    fn sample_html() -> ClipboardRichText {
        ClipboardRichText {
            bytes: b"<strong>Hello</strong>".to_vec(),
            mime_type: "text/html".to_owned(),
            byte_size: 22,
            plain_text: Some("Hello".to_owned()),
            platform_format: Some("test:html".to_owned()),
        }
    }

    fn sample_rtf() -> ClipboardRichText {
        ClipboardRichText {
            bytes: br"{\rtf1 Hello}".to_vec(),
            mime_type: "text/rtf".to_owned(),
            byte_size: 13,
            plain_text: Some("Hello".to_owned()),
            platform_format: Some("test:rtf".to_owned()),
        }
    }

    fn sample_unknown() -> ClipboardUnknown {
        ClipboardUnknown {
            bytes: Vec::new(),
            mime_type: None,
            byte_size: 42,
            platform_format: "application/x-test".to_owned(),
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
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

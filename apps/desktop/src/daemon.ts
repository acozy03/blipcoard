import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export type BlipSummary = {
  id: string;
  preview: string;
  size_bytes: number;
  is_redacted?: boolean;
  tags?: string[];
  payloads?: PayloadSummary[];
};

export type BlipDetail = {
  id: string;
  workspace: string;
  source_app: string | null;
  content_type: string;
  language: string | null;
  content: string;
  size_bytes: number;
  token_estimate: number | null;
  is_redacted: boolean;
  tags: string[];
  created_at: string;
  payloads?: PayloadSummary[];
};

export type PayloadPreviewState =
  | "available"
  | "text_fallback"
  | "metadata_only"
  | "redacted"
  | "missing_blob"
  | "unsupported"
  | "unavailable";

export type PayloadSummary = {
  id: string;
  payload_kind: string;
  mime_type: string | null;
  platform_format: string | null;
  byte_size: number;
  preview_state: PayloadPreviewState;
  preview_text: string | null;
  preview_ref: string | null;
  has_blob: boolean;
  has_inline_text: boolean;
  metadata_summary: unknown;
};

export type PayloadRequester = "cli" | "desktop" | "agent";

export type PayloadBytesResponse = {
  payload_id: string;
  blip_id: string;
  workspace: string;
  payload_kind: string;
  mime_type: string | null;
  platform_format: string | null;
  byte_size: number;
  bytes: number[];
};

export type AuditEventSummary = {
  id: string;
  actor_type: string;
  actor_id: string | null;
  event_type: string;
  target_blip_id: string | null;
  target_workspace: string | null;
  details_json: string | null;
  created_at: string;
};

export type BlipRoutedResponse = {
  id: string;
  from_workspace: string;
  to_workspace: string;
};

export type ShortcutRegistration = {
  id: string;
  label: string;
  accelerator: string;
  action: "route_latest_inbox_to_active_workspace";
  state: "registered" | "conflict" | "unsupported";
  message: string | null;
};

export type WorkspaceSummary = {
  name: string;
  agent_access: boolean;
  sticky_capture: boolean;
  rich_capture_enabled: boolean;
  image_capture_enabled: boolean;
  rich_payload_visibility: string;
  agent_raw_payload_access: boolean;
  hosted_share_enabled: boolean;
  hosted_workspace_id: string | null;
  hosted_workspace_name: string | null;
};

export type HostedStatusResponse = {
  connected: boolean;
  service_url: string | null;
  workspace_id: string | null;
  workspace_name: string | null;
  member_id: string | null;
  member_display_name: string | null;
  member_role: string | null;
  sticky_share_enabled: boolean;
};

export type HostedPublishResponse = {
  local_blip_id: string;
  hosted_blip_id: string;
  hosted_workspace_id: string;
  sequence: number;
};

export type CurrentWorkspaceResponse = {
  active_workspace: string | null;
};

export type WorkspaceListResponse = {
  workspaces: WorkspaceSummary[];
};

export type BlipListResponse = {
  workspace: string;
  blips: BlipSummary[];
};

export type AuditEventListResponse = {
  events: AuditEventSummary[];
};

export type ShortcutRegistrationResponse = {
  shortcuts: ShortcutRegistration[];
};

type TauriCore = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

declare global {
  interface Window {
    __TAURI__?: {
      core?: TauriCore;
    };
    __TAURI_INTERNALS__?: unknown;
  }
}

const DEV_WORKSPACES: WorkspaceSummary[] = [
  {
    name: "inbox",
    agent_access: false,
    sticky_capture: false,
    rich_capture_enabled: true,
    image_capture_enabled: true,
    rich_payload_visibility: "safe_preview",
    agent_raw_payload_access: false,
    hosted_share_enabled: false,
    hosted_workspace_id: null,
    hosted_workspace_name: null
  },
  {
    name: "auth-bug",
    agent_access: false,
    sticky_capture: false,
    rich_capture_enabled: true,
    image_capture_enabled: true,
    rich_payload_visibility: "safe_preview",
    agent_raw_payload_access: false,
    hosted_share_enabled: false,
    hosted_workspace_id: null,
    hosted_workspace_name: null
  },
  {
    name: "agent-feed",
    agent_access: true,
    sticky_capture: false,
    rich_capture_enabled: true,
    image_capture_enabled: true,
    rich_payload_visibility: "safe_preview",
    agent_raw_payload_access: false,
    hosted_share_enabled: false,
    hosted_workspace_id: null,
    hosted_workspace_name: null
  }
];

let devHostedStatus: HostedStatusResponse = {
  connected: false,
  service_url: null,
  workspace_id: null,
  workspace_name: null,
  member_id: null,
  member_display_name: null,
  member_role: null,
  sticky_share_enabled: false
};

const DEV_BLIPS: Record<string, BlipSummary[]> = {
  inbox: [
    {
      id: "dev-inbox-1",
      preview: "Copied API token rotation note",
      size_bytes: 31
    },
    {
      id: "dev-inbox-2",
      preview: "Stack trace from checkout smoke test",
      size_bytes: 37
    },
    {
      id: "dev-inbox-3",
      preview: "Image clipboard payload: 1280x720 image/png (245760 bytes)",
      size_bytes: 245760,
      tags: ["clipboard:image", "rich:clipboard"],
      payloads: [
        {
          id: "dev-inbox-3:payload:image",
          payload_kind: "image",
          mime_type: "image/png",
          platform_format: "public.png",
          byte_size: 245760,
          preview_state: "available",
          preview_text: "image/png 1280x720 240.0 KiB",
          preview_ref: "sha256/00/00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
          has_blob: true,
          has_inline_text: false,
          metadata_summary: { width: 1280, height: 720 }
        }
      ]
    },
    {
      id: "dev-inbox-4",
      preview: "File-list clipboard payload: 2 paths",
      size_bytes: 196,
      tags: ["clipboard:file-list", "rich:clipboard"],
      payloads: [
        {
          id: "dev-inbox-4:payload:file-list",
          payload_kind: "file_list",
          mime_type: "text/uri-list",
          platform_format: "public.file-url",
          byte_size: 196,
          preview_state: "metadata_only",
          preview_text: "2 file references",
          preview_ref: null,
          has_blob: false,
          has_inline_text: false,
          metadata_summary: { path_count: 2, paths: ["/tmp/report.pdf", "/tmp/screenshot.png"] }
        }
      ]
    },
    {
      id: "dev-inbox-5",
      preview: "Hello from pasted HTML",
      size_bytes: 512,
      tags: ["clipboard:html", "rich:clipboard"],
      payloads: [
        {
          id: "dev-inbox-5:payload:html",
          payload_kind: "html",
          mime_type: "text/html",
          platform_format: "public.html",
          byte_size: 512,
          preview_state: "text_fallback",
          preview_text: "Hello from pasted HTML",
          preview_ref: null,
          has_blob: true,
          has_inline_text: true,
          metadata_summary: { render_policy: "plain_text_only" }
        }
      ]
    }
  ],
  "auth-bug": [
    {
      id: "dev-auth-1",
      preview: "OAuth redirect mismatch report",
      size_bytes: 30
    }
  ],
  "agent-feed": [
    {
      id: "dev-agent-1",
      preview: "Refactor notes for checkout worker",
      size_bytes: 34
    }
  ]
};

const DEV_DETAILS: Record<string, BlipDetail> = {
  "dev-inbox-1": {
    id: "dev-inbox-1",
    workspace: "inbox",
    source_app: "Terminal",
    content_type: "plain_text",
    language: null,
    content: "Copied API token rotation note\nRotate staging credentials before release.",
    size_bytes: 68,
    token_estimate: 10,
    is_redacted: true,
    tags: ["demo", "secret"],
    created_at: "2026-06-26T17:30:00Z"
  },
  "dev-inbox-2": {
    id: "dev-inbox-2",
    workspace: "inbox",
    source_app: "Browser",
    content_type: "stack_trace",
    language: null,
    content: "Error: checkout smoke test failed\n    at submitOrder\n    at retryWithBackoff",
    size_bytes: 76,
    token_estimate: 9,
    is_redacted: false,
    tags: ["demo"],
    created_at: "2026-06-26T17:31:00Z"
  },
  "dev-inbox-3": {
    id: "dev-inbox-3",
    workspace: "inbox",
    source_app: "Screenshot Tool",
    content_type: "plain_text",
    language: null,
    content: "Image clipboard payload: 1280x720 image/png (245760 bytes)",
    size_bytes: 245760,
    token_estimate: 7,
    is_redacted: false,
    tags: ["clipboard:image", "rich:clipboard"],
    created_at: "2026-06-26T17:31:30Z",
    payloads: DEV_BLIPS.inbox[2].payloads
  },
  "dev-inbox-4": {
    id: "dev-inbox-4",
    workspace: "inbox",
    source_app: "Finder",
    content_type: "plain_text",
    language: null,
    content: "File-list clipboard payload: 2 paths",
    size_bytes: 196,
    token_estimate: 5,
    is_redacted: false,
    tags: ["clipboard:file-list", "rich:clipboard"],
    created_at: "2026-06-26T17:31:40Z",
    payloads: DEV_BLIPS.inbox[3].payloads
  },
  "dev-inbox-5": {
    id: "dev-inbox-5",
    workspace: "inbox",
    source_app: "Browser",
    content_type: "plain_text",
    language: null,
    content: "Hello from pasted HTML",
    size_bytes: 512,
    token_estimate: 4,
    is_redacted: false,
    tags: ["clipboard:html", "rich:clipboard"],
    created_at: "2026-06-26T17:31:50Z",
    payloads: DEV_BLIPS.inbox[4].payloads
  },
  "dev-auth-1": {
    id: "dev-auth-1",
    workspace: "auth-bug",
    source_app: "Firefox",
    content_type: "plain_text",
    language: null,
    content: "OAuth redirect mismatch report\nExpected /callback, received /auth/callback.",
    size_bytes: 74,
    token_estimate: 9,
    is_redacted: false,
    tags: ["auth"],
    created_at: "2026-06-26T17:32:00Z"
  },
  "dev-agent-1": {
    id: "dev-agent-1",
    workspace: "agent-feed",
    source_app: "Editor",
    content_type: "plain_text",
    language: null,
    content: "Refactor notes for checkout worker\nKeep retry policy outside transport code.",
    size_bytes: 75,
    token_estimate: 10,
    is_redacted: false,
    tags: ["agent"],
    created_at: "2026-06-26T17:33:00Z"
  }
};

const DEV_AUDIT_EVENTS: AuditEventSummary[] = [
  {
    id: "audit-dev-1",
    actor_type: "agent",
    actor_id: "codex",
    event_type: "blips_read",
    target_blip_id: null,
    target_workspace: "agent-feed",
    details_json: "{\"limit\":50}",
    created_at: "2026-06-26T17:36:00Z"
  },
  {
    id: "audit-dev-2",
    actor_type: "user",
    actor_id: null,
    event_type: "blip_moved",
    target_blip_id: "dev-inbox-1",
    target_workspace: "auth-bug",
    details_json: "{\"from_workspace\":\"inbox\",\"to_workspace\":\"auth-bug\"}",
    created_at: "2026-06-26T17:35:00Z"
  },
  {
    id: "audit-dev-3",
    actor_type: "user",
    actor_id: null,
    event_type: "workspace_activated",
    target_blip_id: null,
    target_workspace: "auth-bug",
    details_json: null,
    created_at: "2026-06-26T17:34:00Z"
  }
];

let devActiveWorkspace = "inbox";

const ROUTE_LATEST_SHORTCUTS = [
  {
    id: "route-latest-to-active",
    label: "Route latest to active",
    accelerator: "Ctrl+Alt+B",
    action: "route_latest_inbox_to_active_workspace" as const
  }
];

export async function currentWorkspace(): Promise<CurrentWorkspaceResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<CurrentWorkspaceResponse>("current_workspace");
  }

  await devDelay();
  return { active_workspace: devActiveWorkspace };
}

export async function listWorkspaces(): Promise<WorkspaceListResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<WorkspaceListResponse>("list_workspaces");
  }

  await devDelay();
  return { workspaces: DEV_WORKSPACES };
}

export async function listWorkspaceBlips(
  workspace: string,
  limit = 50
): Promise<BlipListResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<BlipListResponse>("list_blips", { workspace, limit });
  }

  await devDelay();
  return { workspace, blips: DEV_BLIPS[workspace] ?? [] };
}

export async function getBlip(blipId: string): Promise<BlipDetail> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<BlipDetail>("get_blip", { blip_id: blipId });
  }

  await devDelay();
  const blip = DEV_DETAILS[blipId];

  if (!blip) {
    throw new Error(`blip \`${blipId}\` does not exist`);
  }

  return blip;
}

export async function getPayloadMetadata(payloadId: string): Promise<PayloadSummary> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<PayloadSummary>("get_payload_metadata", { payload_id: payloadId });
  }

  await devDelay();
  for (const detail of Object.values(DEV_DETAILS)) {
    const payload = detail.payloads?.find((candidate) => candidate.id === payloadId);
    if (payload) {
      return payload;
    }
  }

  throw new Error(`payload \`${payloadId}\` does not exist`);
}

export async function getPayloadPreview(payloadId: string): Promise<PayloadBytesResponse> {
  const invoke = getInvoke();

  if (invoke) {
    const requester: PayloadRequester = "desktop";
    return invoke<PayloadBytesResponse>("get_payload_preview", {
      payload_id: payloadId,
      requester
    });
  }

  await devDelay();
  throw new Error(`payload preview is unavailable in desktop dev mode for ${payloadId}`);
}

export async function exportPayload(payloadId: string): Promise<PayloadBytesResponse> {
  const invoke = getInvoke();

  if (invoke) {
    const requester: PayloadRequester = "desktop";
    return invoke<PayloadBytesResponse>("export_payload", {
      payload_id: payloadId,
      requester
    });
  }

  await devDelay();
  throw new Error(`payload export is unavailable in desktop dev mode for ${payloadId}`);
}

export async function listAuditEvents(limit = 25): Promise<AuditEventListResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<AuditEventListResponse>("list_audit_events", { limit });
  }

  await devDelay();
  return { events: DEV_AUDIT_EVENTS.slice(0, limit) };
}

export async function registerGlobalShortcuts(): Promise<ShortcutRegistrationResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<ShortcutRegistrationResponse>("register_global_shortcuts", {
      shortcuts: ROUTE_LATEST_SHORTCUTS
    });
  }

  await devDelay();
  return {
    shortcuts: ROUTE_LATEST_SHORTCUTS.map((shortcut) => ({
      ...shortcut,
      state: "unsupported",
      message: "Global shortcuts require a desktop runtime"
    }))
  };
}

export async function activateWorkspace(workspace: string): Promise<CurrentWorkspaceResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<CurrentWorkspaceResponse>("activate_workspace", { workspace });
  }

  await devDelay();
  devActiveWorkspace = workspace;
  return { active_workspace: workspace };
}

export async function setStickyCapture(
  workspace: string,
  enabled: boolean
): Promise<WorkspaceSummary> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<WorkspaceSummary>("set_sticky_capture", { workspace, enabled });
  }

  await devDelay();
  for (const devWorkspace of DEV_WORKSPACES) {
    devWorkspace.sticky_capture = enabled && devWorkspace.name === workspace;
  }

  const updated = DEV_WORKSPACES.find((devWorkspace) => devWorkspace.name === workspace);

  if (!updated) {
    throw new Error(`workspace \`${workspace}\` does not exist`);
  }

  return updated;
}

export async function routeLatestInboxBlip(workspace: string): Promise<BlipRoutedResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<BlipRoutedResponse>("route_latest_inbox_blip", { workspace });
  }

  await devDelay();
  const [latest] = DEV_BLIPS.inbox;

  if (!latest) {
    throw new Error("inbox is empty");
  }

  DEV_BLIPS.inbox = DEV_BLIPS.inbox.filter((blip) => blip.id !== latest.id);
  DEV_BLIPS[workspace] = [latest, ...(DEV_BLIPS[workspace] ?? [])];
  const detail = DEV_DETAILS[latest.id];

  if (detail) {
    detail.workspace = workspace;
  }

  return {
    id: latest.id,
    from_workspace: "inbox",
    to_workspace: workspace
  };
}

export async function hostedStatus(): Promise<HostedStatusResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<HostedStatusResponse>("hosted_status");
  }

  await devDelay();
  return devHostedStatus;
}

export async function hostedJoinWorkspace(input: {
  service_url: string;
  join_code: string;
  display_name: string;
  device_label?: string | null;
}): Promise<HostedStatusResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<HostedStatusResponse>("hosted_join_workspace", input);
  }

  await devDelay();
  devHostedStatus = {
    connected: true,
    service_url: input.service_url,
    workspace_id: "hw_dev",
    workspace_name: "dev shared workspace",
    member_id: "hm_dev",
    member_display_name: input.display_name,
    member_role: "editor",
    sticky_share_enabled: false
  };
  return devHostedStatus;
}

export async function hostedPublishBlip(blipId: string): Promise<HostedPublishResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<HostedPublishResponse>("hosted_publish_blip", { blip_id: blipId });
  }

  await devDelay();
  if (!devHostedStatus.connected || !devHostedStatus.workspace_id) {
    throw new Error("join a hosted workspace before publishing");
  }
  return {
    local_blip_id: blipId,
    hosted_blip_id: `hb_${blipId}`,
    hosted_workspace_id: devHostedStatus.workspace_id,
    sequence: Date.now()
  };
}

export async function hostedSetStickyShare(enabled: boolean): Promise<HostedStatusResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<HostedStatusResponse>("hosted_set_sticky_share", { enabled });
  }

  await devDelay();
  if (enabled && !devHostedStatus.connected) {
    throw new Error("join a hosted workspace before enabling sticky share");
  }
  devHostedStatus = { ...devHostedStatus, sticky_share_enabled: enabled };
  return devHostedStatus;
}

function getInvoke(): TauriCore["invoke"] | null {
  const invoke = window.__TAURI__?.core?.invoke;

  if (invoke) {
    return invoke;
  }

  if (window.__TAURI_INTERNALS__) {
    return tauriInvoke;
  }

  if (import.meta.env.DEV) {
    return null;
  }

  throw new Error("Desktop daemon bridge is unavailable");
}

function devDelay() {
  return new Promise((resolve) => window.setTimeout(resolve, 180));
}

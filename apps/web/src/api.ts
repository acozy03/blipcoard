export type HostedRole = "owner" | "editor" | "viewer";

export interface ApiEnvelope<T> {
  request_id: string;
  status: string;
  data?: T;
  error?: {
    code: string;
    message: string;
  };
}

export interface HostedWorkspaceSummary {
  id: string;
  name: string;
  created_by_member_id: string;
  created_at: string;
  retention_days: number | null;
  default_role: HostedRole;
  last_sequence: number;
}

export interface MemberSummary {
  id: string;
  workspace_id: string;
  display_name: string;
  role: HostedRole;
  status: string;
  joined_at: string;
}

export interface HostedDeviceSession {
  token: string;
  member_id: string;
  workspace_id: string;
  device_label: string | null;
  client_kind: string | null;
  created_at: string;
}

export interface JoinWorkspaceResponse {
  workspace: HostedWorkspaceSummary;
  member: MemberSummary;
  session: HostedDeviceSession;
  event: WorkspaceEvent;
}

export interface HostedBlipSummary {
  id: string;
  workspace_id: string;
  local_blip_id: string;
  publisher_member_id: string;
  content_type: string;
  content: string;
  preview: string;
  size_bytes: number;
  is_redacted: boolean;
  tags: string[];
  captured_at: string | null;
  published_at: string;
  sequence: number;
}

export interface WorkspaceEvent {
  id: string;
  workspace_id: string;
  sequence: number;
  event_type: string;
  actor_member_id: string | null;
  target_id: string | null;
  data: unknown;
  created_at: string;
}

export interface MemberPresenceSummary {
  member_id: string;
  display_name: string;
  role: HostedRole;
  last_seen_at: string;
}

export interface HostedSession {
  serviceUrl: string;
  workspace: HostedWorkspaceSummary;
  member: MemberSummary;
  session: HostedDeviceSession;
  lastSequence: number;
}

const SESSION_STORAGE_KEY = "blipcoard.hosted.session.v1";

export function loadStoredSession(): HostedSession | null {
  const raw = window.localStorage.getItem(SESSION_STORAGE_KEY);
  if (!raw) {
    return null;
  }

  try {
    return JSON.parse(raw) as HostedSession;
  } catch {
    window.localStorage.removeItem(SESSION_STORAGE_KEY);
    return null;
  }
}

export function storeSession(session: HostedSession) {
  window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
}

export function clearStoredSession() {
  window.localStorage.removeItem(SESSION_STORAGE_KEY);
}

export async function joinWorkspace(
  serviceUrl: string,
  code: string,
  displayName: string
): Promise<JoinWorkspaceResponse> {
  const response = await request<JoinWorkspaceResponse>(serviceUrl, "/v1/join", {
    method: "POST",
    body: {
      code,
      display_name: displayName,
      device_label: browserDeviceLabel(),
      client_kind: "web"
    }
  });
  return response;
}

export async function listBlips(session: HostedSession): Promise<HostedBlipSummary[]> {
  return request<HostedBlipSummary[]>(session.serviceUrl, `/v1/workspaces/${session.workspace.id}/blips`, {
    headers: authHeaders(session)
  });
}

export async function getBlip(session: HostedSession, blipId: string): Promise<HostedBlipSummary> {
  return request<HostedBlipSummary>(
    session.serviceUrl,
    `/v1/workspaces/${session.workspace.id}/blips/${blipId}`,
    { headers: authHeaders(session) }
  );
}

export async function listEvents(
  session: HostedSession,
  afterSequence: number
): Promise<WorkspaceEvent[]> {
  const query = new URLSearchParams({ after_sequence: String(afterSequence) });
  return request<WorkspaceEvent[]>(
    session.serviceUrl,
    `/v1/workspaces/${session.workspace.id}/events?${query.toString()}`,
    { headers: authHeaders(session) }
  );
}

export async function updateTags(
  session: HostedSession,
  blipId: string,
  tags: string[]
): Promise<HostedBlipSummary> {
  const response = await request<{ blip: HostedBlipSummary }>(
    session.serviceUrl,
    `/v1/workspaces/${session.workspace.id}/blips/${blipId}/tags`,
    {
      method: "POST",
      body: {
        member_id: session.member.id,
        session_token: session.session.token,
        tags
      }
    }
  );
  return response.blip;
}

export async function recordAccess(
  session: HostedSession,
  blipId: string,
  action: "copy" | "export"
): Promise<void> {
  await request<{ event: WorkspaceEvent }>(
    session.serviceUrl,
    `/v1/workspaces/${session.workspace.id}/blips/${blipId}/access-events`,
    {
      method: "POST",
      body: {
        member_id: session.member.id,
        session_token: session.session.token,
        action
      }
    }
  );
}

export async function heartbeat(session: HostedSession): Promise<MemberPresenceSummary[]> {
  const response = await request<{ members: MemberPresenceSummary[] }>(
    session.serviceUrl,
    `/v1/workspaces/${session.workspace.id}/presence`,
    {
      method: "POST",
      body: {
        member_id: session.member.id,
        session_token: session.session.token
      }
    }
  );
  return response.members;
}

export async function listPresence(session: HostedSession): Promise<MemberPresenceSummary[]> {
  const response = await request<{ members: MemberPresenceSummary[] }>(
    session.serviceUrl,
    `/v1/workspaces/${session.workspace.id}/presence`,
    { headers: authHeaders(session) }
  );
  return response.members;
}

function authHeaders(session: HostedSession) {
  return {
    Authorization: `Bearer ${session.session.token}`,
    "X-Blip-Member-Id": session.member.id
  };
}

async function request<T>(
  serviceUrl: string,
  path: string,
  options: { method?: "GET" | "POST"; body?: unknown; headers?: Record<string, string> } = {}
): Promise<T> {
  const response = await fetch(`${normalizeServiceUrl(serviceUrl)}${path}`, {
    method: options.method ?? "GET",
    headers: {
      ...options.headers,
      ...(options.body ? { "Content-Type": "application/json" } : {})
    },
    body: options.body ? JSON.stringify(options.body) : undefined
  });
  const envelope = (await response.json()) as ApiEnvelope<T>;

  if (!response.ok || envelope.status !== "ok" || !envelope.data) {
    throw new Error(envelope.error?.message ?? `Request failed with ${response.status}`);
  }

  return envelope.data;
}

function normalizeServiceUrl(serviceUrl: string) {
  return serviceUrl.trim().replace(/\/+$/, "");
}

function browserDeviceLabel() {
  const agent = window.navigator.userAgent.split(" ").slice(0, 3).join(" ");
  return `Browser ${agent}`.slice(0, 80);
}

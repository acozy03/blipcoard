export type BlipSummary = {
  id: string;
  preview: string;
  size_bytes: number;
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
};

export type WorkspaceSummary = {
  name: string;
  agent_access: boolean;
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

type TauriCore = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

declare global {
  interface Window {
    __TAURI__?: {
      core?: TauriCore;
    };
  }
}

const DEV_WORKSPACES: WorkspaceSummary[] = [
  { name: "inbox", agent_access: false },
  { name: "auth-bug", agent_access: false },
  { name: "agent-feed", agent_access: true }
];

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

let devActiveWorkspace = "inbox";

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

export async function activateWorkspace(workspace: string): Promise<CurrentWorkspaceResponse> {
  const invoke = getInvoke();

  if (invoke) {
    return invoke<CurrentWorkspaceResponse>("activate_workspace", { workspace });
  }

  await devDelay();
  devActiveWorkspace = workspace;
  return { active_workspace: workspace };
}

function getInvoke() {
  const invoke = window.__TAURI__?.core?.invoke;

  if (invoke) {
    return invoke;
  }

  if (import.meta.env.DEV) {
    return null;
  }

  throw new Error("Desktop daemon bridge is unavailable");
}

function devDelay() {
  return new Promise((resolve) => window.setTimeout(resolve, 180));
}

export type BlipSummary = {
  id: string;
  preview: string;
  size_bytes: number;
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

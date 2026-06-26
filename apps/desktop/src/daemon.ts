export type InboxBlip = {
  id: string;
  preview: string;
  size_bytes: number;
};

export type InboxResponse = {
  workspace: "inbox";
  blips: InboxBlip[];
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

export async function listInboxBlips(): Promise<InboxResponse> {
  const invoke = window.__TAURI__?.core?.invoke;

  if (invoke) {
    return invoke<InboxResponse>("list_inbox_blips", { limit: 50 });
  }

  if (import.meta.env.DEV) {
    await new Promise((resolve) => window.setTimeout(resolve, 250));
    return {
      workspace: "inbox",
      blips: [
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
      ]
    };
  }

  throw new Error("Desktop daemon bridge is unavailable");
}

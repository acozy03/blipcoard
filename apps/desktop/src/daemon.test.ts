import { afterEach, beforeEach, expect, it, vi } from "vitest";

const tauriInvokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriInvokeMock
}));

import {
  createWorkspace,
  DEFAULT_BLIP_LIST_FILTERS,
  getBlip,
  listWorkspaceBlips,
  listWorkspaces,
  recopyBlip,
  setAgentAccess
} from "./daemon";

beforeEach(() => {
  tauriInvokeMock.mockReset();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it("uses the imported Tauri invoke API when Tauri internals are available", async () => {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    setTimeout
  });
  tauriInvokeMock.mockResolvedValueOnce({
    workspace: "inbox",
    blips: [],
    filters_supported: true
  });

  await listWorkspaceBlips("inbox", 12, 8);

  expect(tauriInvokeMock).toHaveBeenCalledWith("list_blips", {
    workspace: "inbox",
    limit: 12,
    offset: 8,
    filters: DEFAULT_BLIP_LIST_FILTERS
  });
});

it("passes composed blip filters through the Rust command bridge", async () => {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    setTimeout
  });
  tauriInvokeMock.mockResolvedValueOnce({
    workspace: "inbox",
    blips: [],
    total: 0,
    filters_supported: true
  });
  const filters = {
    all_workspaces: true,
    created_at_from: "2026-07-01T04:00:00.000Z",
    created_at_before: "2026-07-08T04:00:00.000Z",
    blip_types: ["image", "rich_text"] as const
  };

  await listWorkspaceBlips("inbox", 8, 0, {
    ...filters,
    blip_types: [...filters.blip_types]
  });

  expect(tauriInvokeMock).toHaveBeenCalledWith("list_blips", {
    workspace: "inbox",
    limit: 8,
    offset: 0,
    filters
  });
});

it("requires a restarted daemon before presenting filters", async () => {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    setTimeout
  });
  tauriInvokeMock.mockResolvedValueOnce({ workspace: "inbox", blips: [], total: 0 });

  await expect(listWorkspaceBlips("inbox")).rejects.toThrow(
    "The running blipd must be restarted to use blip filters"
  );
});

it("keeps snake_case argument keys for the Rust command bridge", async () => {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    setTimeout
  });
  tauriInvokeMock.mockResolvedValueOnce({
    id: "blip-1",
    workspace: "inbox",
    source_app: null,
    content_type: "plain_text",
    language: null,
    content: "hello",
    size_bytes: 5,
    token_estimate: 1,
    is_redacted: false,
    tags: [],
    created_at: "2026-07-03T00:00:00Z"
  });

  await getBlip("blip-1");

  expect(tauriInvokeMock).toHaveBeenCalledWith("get_blip", {
    blip_id: "blip-1"
  });
});

it("uses browser fixtures only when the Tauri runtime is absent", async () => {
  vi.stubGlobal("window", { setTimeout });

  const response = await listWorkspaceBlips("inbox", 1);

  expect(response.blips[0]?.id).toBe("dev-inbox-1");
  expect(tauriInvokeMock).not.toHaveBeenCalled();
});

it("paginates browser fixtures with limit and offset", async () => {
  vi.stubGlobal("window", { setTimeout });

  const response = await listWorkspaceBlips("inbox", 2, 2);

  expect(response.blips.map((blip) => blip.id)).toEqual(["dev-inbox-3", "dev-inbox-4"]);
});

it("filters browser fixtures before paginating aggregate results", async () => {
  vi.stubGlobal("window", { setTimeout });

  const response = await listWorkspaceBlips("inbox", 2, 0, {
    all_workspaces: true,
    created_at_from: "2026-06-26T17:31:30Z",
    created_at_before: "2026-06-26T17:33:01Z",
    blip_types: ["image", "rich_text", "text"]
  });

  expect(response.total).toBe(4);
  expect(response.blips.map((blip) => blip.id)).toEqual(["dev-agent-1", "dev-auth-1"]);
  expect(response.blips.map((blip) => blip.workspace)).toEqual(["agent-feed", "auth-bug"]);
});

it("updates workspace agent access through the Rust command bridge", async () => {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    setTimeout
  });
  tauriInvokeMock.mockResolvedValueOnce({ name: "inbox", agent_access: true });

  await setAgentAccess("inbox", true);

  expect(tauriInvokeMock).toHaveBeenCalledWith("set_agent_access", {
    workspace: "inbox",
    enabled: true
  });
});

it("creates workspaces through the Rust command bridge", async () => {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    setTimeout
  });
  tauriInvokeMock.mockResolvedValueOnce({ name: "release-notes", agent_access: false });

  await createWorkspace({
    name: "release-notes",
    description: null,
    color: null,
    agent_access: false
  });

  expect(tauriInvokeMock).toHaveBeenCalledWith("create_workspace", {
    name: "release-notes",
    description: null,
    color: null,
    agent_access: false
  });
});

it("prepares existing blips for recopy through the Rust command bridge", async () => {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    setTimeout
  });
  tauriInvokeMock.mockResolvedValueOnce({ id: "blip-1", workspace: "inbox" });

  await recopyBlip("blip-1", "text:abc123");

  expect(tauriInvokeMock).toHaveBeenCalledWith("recopy_blip", {
    blip_id: "blip-1",
    clipboard_fingerprint: "text:abc123"
  });
});

it("persists workspace agent access in browser development mode", async () => {
  vi.stubGlobal("window", { setTimeout });

  await setAgentAccess("auth-bug", true);
  const enabled = await listWorkspaces();
  expect(enabled.workspaces.find((workspace) => workspace.name === "auth-bug")?.agent_access).toBe(
    true
  );

  await setAgentAccess("auth-bug", false);
});

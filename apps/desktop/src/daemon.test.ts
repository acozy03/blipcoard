import { afterEach, beforeEach, expect, it, vi } from "vitest";

const tauriInvokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriInvokeMock
}));

import { getBlip, listWorkspaceBlips } from "./daemon";

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
  tauriInvokeMock.mockResolvedValueOnce({ workspace: "inbox", blips: [] });

  await listWorkspaceBlips("inbox", 12);

  expect(tauriInvokeMock).toHaveBeenCalledWith("list_blips", {
    workspace: "inbox",
    limit: 12
  });
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

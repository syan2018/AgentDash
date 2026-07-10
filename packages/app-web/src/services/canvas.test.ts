import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  delete: vi.fn(),
  get: vi.fn(),
  post: vi.fn(),
  put: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    delete: mocks.delete,
    get: mocks.get,
    post: mocks.post,
    put: mocks.put,
  },
}));

import {
  copyCanvasToPersonal,
  buildStandaloneCanvasPreviewSnapshot,
  fetchAgentRunCanvasRuntimeSnapshot,
  fetchProjectCanvases,
  invokeCanvasRuntimeAction,
  publishCanvasToProject,
  submitCanvasAgentInput,
  unpublishCanvas,
  uploadCanvasInteractionSnapshot,
  uploadCanvasRenderObservation,
} from "./canvas";

describe("canvas service", () => {
  beforeEach(() => {
    mocks.delete.mockReset();
    mocks.get.mockReset();
    mocks.post.mockReset();
    mocks.put.mockReset();
  });

  it("fetches project canvases without a scope query when scope is omitted", async () => {
    mocks.get.mockResolvedValueOnce([]);

    await fetchProjectCanvases("project 1");

    expect(mocks.get).toHaveBeenCalledWith("/projects/project%201/canvases");
  });

  it("serializes project canvas list scope", async () => {
    mocks.get.mockResolvedValue([]);

    await fetchProjectCanvases("project-1", "mine");
    await fetchProjectCanvases("project-1", "shared");
    await fetchProjectCanvases("project-1", "all");

    expect(mocks.get).toHaveBeenNthCalledWith(1, "/projects/project-1/canvases?scope=mine");
    expect(mocks.get).toHaveBeenNthCalledWith(2, "/projects/project-1/canvases?scope=shared");
    expect(mocks.get).toHaveBeenNthCalledWith(3, "/projects/project-1/canvases?scope=all");
  });

  it("publishes a personal canvas to the project shared scope", async () => {
    const response = { canvas_id: "shared-1" };
    mocks.post.mockResolvedValueOnce(response);

    const result = await publishCanvasToProject("canvas/source", {
      title: "Shared dashboard",
      description: "Stable team view",
    });

    expect(mocks.post).toHaveBeenCalledWith("/canvases/canvas%2Fsource/publish-to-project", {
      title: "Shared dashboard",
      description: "Stable team view",
    });
    expect(result).toBe(response);
  });

  it("copies a shared canvas to a personal canvas", async () => {
    const response = { canvas_id: "personal-copy-1" };
    mocks.post.mockResolvedValueOnce(response);

    const result = await copyCanvasToPersonal("shared-1", {
      canvas_mount_id: "cvs-personal-copy",
    });

    expect(mocks.post).toHaveBeenCalledWith("/canvases/shared-1/copy-to-personal", {
      canvas_mount_id: "cvs-personal-copy",
    });
    expect(result).toBe(response);
  });

  it("unpublishes a project shared canvas", async () => {
    const response = {
      unpublished_canvas_id: "shared-1",
      source_canvas_id: "personal-1",
    };
    mocks.post.mockResolvedValueOnce(response);

    const result = await unpublishCanvas("shared-1");

    expect(mocks.post).toHaveBeenCalledWith("/canvases/shared-1/unpublish", {});
    expect(result).toBe(response);
  });

  it("builds standalone preview directly from immutable Canvas source", () => {
    const result = buildStandaloneCanvasPreviewSnapshot({
      canvas_id: "canvas-1",
      project_id: "project-1",
      owner_user_id: "user-1",
      scope: "personal",
      access: {
        can_view: true,
        can_edit_source: true,
        can_publish: true,
        can_manage_shared: false,
        can_copy: false,
        runtime_write_allowed: true,
      },
      canvas_mount_id: "cvs-1",
      vfs_mount_id: "canvas:cvs-1",
      title: "Demo",
      description: "",
      entry_file: "src/main.tsx",
      sandbox_config: { libraries: ["react"], import_map: { imports: {} } },
      files: [{ path: "src/main.tsx", content: "export default 1" }],
      published_from_canvas_id: null,
      shared_canvas_id: null,
      cloned_from_canvas_id: null,
      published_at: null,
      published_by_user_id: null,
      created_at: "2026-07-10T00:00:00Z",
      updated_at: "2026-07-10T00:00:00Z",
    });

    expect(result.entry).toBe("src/main.tsx");
    expect(result.files[0]?.file_type).toBe("tsx");
    expect(result.runtime_bridge.enabled).toBe(false);
    expect(mocks.get).not.toHaveBeenCalled();
  });

  it("fetches AgentRun-scoped runtime snapshot by Canvas mount", async () => {
    const bridge = bridgeIdentity();
    const response = { canvas_mount_id: "cvs-dashboard" };
    mocks.get.mockResolvedValueOnce(response);

    const result = await fetchAgentRunCanvasRuntimeSnapshot(bridge);

    expect(mocks.get).toHaveBeenCalledWith(
      "/agent-runs/run%201/agents/agent%201/canvases/cvs-dashboard/runtime-snapshot",
    );
    expect(result).toBe(response);
  });

  it("invokes Canvas runtime actions through AgentRun-scoped route", async () => {
    const bridge = bridgeIdentity();
    const response = { output: { ok: true } };
    mocks.post.mockResolvedValueOnce(response);

    const result = await invokeCanvasRuntimeAction(bridge, {
      action_key: "demo.action",
      input: { value: "x" },
    });

    expect(mocks.post).toHaveBeenCalledWith(
      "/agent-runs/run%201/agents/agent%201/canvases/cvs-dashboard/runtime-invoke",
      { action_key: "demo.action", input: { value: "x" } },
    );
    expect(result).toBe(response);
  });

  it("uploads render observation and interaction snapshots through AgentRun Canvas routes", async () => {
    const bridge = bridgeIdentity();
    mocks.post.mockResolvedValue(undefined);

    await uploadCanvasRenderObservation(bridge, {
      frame_id: "frame-1",
      generation: 1,
      status: "ready",
      viewport: { width: 100, height: 200, device_pixel_ratio: 1 },
      document: { root_empty: false, body_text_preview: "Ready", element_count: 3 },
      diagnostics: [],
    });
    await uploadCanvasInteractionSnapshot(bridge, {
      frame_id: "frame-1",
      state: { selection: "row-1" },
      recent_events: [{
        kind: "selection_changed",
        payload: { id: "row-1" },
        occurred_at: "2026-06-25T00:00:00Z",
      }],
    });

    expect(mocks.post).toHaveBeenNthCalledWith(
      1,
      "/agent-runs/run%201/agents/agent%201/canvases/cvs-dashboard/runtime-observation",
      {
        frame_id: "frame-1",
        generation: 1,
        status: "ready",
        viewport: { width: 100, height: 200, device_pixel_ratio: 1 },
        document: { root_empty: false, body_text_preview: "Ready", element_count: 3 },
        diagnostics: [],
      },
    );
    expect(mocks.post).toHaveBeenNthCalledWith(
      2,
      "/agent-runs/run%201/agents/agent%201/canvases/cvs-dashboard/interaction-snapshot",
      {
        frame_id: "frame-1",
        state: { selection: "row-1" },
        recent_events: [{
          kind: "selection_changed",
          payload: { id: "row-1" },
          occurred_at: "2026-06-25T00:00:00Z",
        }],
      },
    );
  });

  it("submits Canvas user intent to AgentRun mailbox route", async () => {
    const bridge = bridgeIdentity();
    const response = { outcome: "queued" };
    mocks.post.mockResolvedValueOnce(response);

    const result = await submitCanvasAgentInput(bridge, {
      text: "分析当前选择",
      include_interaction_state: true,
      include_render_observation: true,
      delivery_intent: "queue",
      client_command_id: "cmd-1",
    });

    expect(mocks.post).toHaveBeenCalledWith(
      "/agent-runs/run%201/agents/agent%201/canvases/cvs-dashboard/agent-input-submit",
      {
        input: [{ type: "text", text: "分析当前选择", text_elements: [] }],
        delivery_intent: "queue",
        client_command_id: "cmd-1",
      },
    );
    expect(result).toBe(response);
  });

  it("submits Canvas observation and interaction refs with canonical input", async () => {
    const bridge = bridgeIdentity();
    const response = { outcome: "queued" };
    mocks.post.mockResolvedValueOnce(response);

    await submitCanvasAgentInput(bridge, {
      input: [{ type: "text", text: "处理当前状态", text_elements: [] }],
      delivery_intent: "steer",
      client_command_id: "cmd-refs",
      interaction_snapshot_id: "snapshot-1",
      render_observation_id: "observation-1",
    });

    expect(mocks.post).toHaveBeenCalledWith(
      "/agent-runs/run%201/agents/agent%201/canvases/cvs-dashboard/agent-input-submit",
      {
        input: [{ type: "text", text: "处理当前状态", text_elements: [] }],
        delivery_intent: "steer",
        client_command_id: "cmd-refs",
        interaction_snapshot_id: "snapshot-1",
        render_observation_id: "observation-1",
      },
    );
  });
});

function bridgeIdentity() {
  return {
    run_id: "run 1",
    agent_id: "agent 1",
    project_id: "project-1",
    canvas_mount_id: "cvs-dashboard",
  };
}

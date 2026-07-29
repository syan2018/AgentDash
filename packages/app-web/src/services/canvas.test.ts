import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  CanvasDefinitionDto,
  CanvasRuntimeSnapshotDto,
} from "../generated/interaction-contracts";

const mocks = vi.hoisted(() => ({
  get: vi.fn(),
  post: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.get,
    post: mocks.post,
  },
}));

import {
  fetchCanvasRuntimeSnapshot,
  fetchInteractionCanvasRuntimeSnapshot,
  fetchProjectCanvases,
  invokeCanvasOperation,
  submitCanvasAgentInput,
} from "./canvas";

function definition(): CanvasDefinitionDto {
  return {
    definition_id: "canvas-1",
    canvas_mount_id: "cvs-canvas-1",
    project_id: "project-1",
    owner: { kind: "project", id: "project-1" },
    status: "active",
    current_revision_id: "revision-1",
    revision_number: 1n,
    definition_format_version: 1,
    interaction_contract_version: 1,
    title: "Canvas",
    description: "",
    source_bundle: {
      format_version: 1,
      entry_file: "src/main.tsx",
      files: [{ path: "src/main.tsx", content: "export {};" }],
      sandbox: { libraries: [], import_map: {} },
      digest: "digest-1",
    },
    initial_state: {},
    state_schema: { type: "object" },
    agent_projection: { version: 1, allowed_state_paths: [] },
    command_definitions: [],
    component_bindings: [],
    resource_slots: [],
    access: {
      can_view: true,
      can_edit_source: true,
      can_publish: true,
      can_manage_shared: true,
      can_copy: true,
    },
    created_at: "2026-07-29T00:00:00Z",
    updated_at: "2026-07-29T00:00:00Z",
  };
}

function runtimeSnapshot(): CanvasRuntimeSnapshotDto {
  return {
    definition_id: "canvas-1",
    definition_revision_id: "revision-1",
    canvas_mount_id: "cvs-canvas-1",
    source_bundle: definition().source_bundle,
    operations: [],
    features: {
      operations: true,
      assets: false,
      interaction: false,
      agent_submit: false,
      diagnostics: true,
    },
  };
}

describe("canvas service", () => {
  beforeEach(() => {
    mocks.get.mockReset();
    mocks.post.mockReset();
  });

  it("通过 InteractionDefinition 路由列出 Canvas", async () => {
    mocks.get.mockResolvedValueOnce([]);

    await fetchProjectCanvases("project 1", "mine");

    expect(mocks.get).toHaveBeenCalledWith(
      "/projects/project%201/interaction-definitions/canvas?scope=mine",
    );
  });

  it("把定义快照投影为 Canvas runtime", async () => {
    mocks.get
      .mockResolvedValueOnce(runtimeSnapshot())
      .mockResolvedValueOnce(definition());

    const snapshot = await fetchCanvasRuntimeSnapshot("canvas-1");

    expect(snapshot).toMatchObject({
      project_id: "project-1",
      canvas_id: "canvas-1",
      definition_revision_id: "revision-1",
      canvas_mount_id: "cvs-canvas-1",
      entry: "src/main.tsx",
    });
  });

  it("以完整 OperationRef 调用 Canvas context", async () => {
    mocks.post.mockResolvedValueOnce({
      result: {
        value: {
          kind: "inline",
          value: { ok: true },
        },
      },
    });

    await expect(invokeCanvasOperation({
      projectId: "project-1",
      definitionId: "canvas-1",
      operationRef: {
        namespace: "demo",
        provider_key: "provider",
        operation_key: "run",
        contract_version: 1,
      },
      value: { input: true },
    })).resolves.toEqual({ ok: true });

    expect(mocks.post).toHaveBeenCalledWith(
      "/projects/project-1/operation-workshop/invoke",
      expect.objectContaining({
        context: { kind: "canvas", definition_id: "canvas-1" },
        operation_ref: {
          namespace: "demo",
          provider_key: "provider",
          operation_key: "run",
          contract_version: 1,
        },
      }),
    );
  });

  it("提交 Canvas 用户输入时携带幂等键与显式事实选择", async () => {
    mocks.post.mockResolvedValueOnce({
      handoff_id: "handoff-1",
      status: "accepted",
      duplicate: false,
    });

    await submitCanvasAgentInput({
      instanceId: "instance 1",
      runId: "run-1",
      agentId: "agent-1",
      clientCommandId: "command-1",
      content: [{ kind: "text", text: "继续" }],
      includeInteractionState: true,
      includeRenderObservation: false,
    });

    expect(mocks.post).toHaveBeenCalledWith(
      "/interaction-instances/instance%201/agent-submit",
      {
        run_id: "run-1",
        agent_id: "agent-1",
        client_command_id: "command-1",
        input: [{ kind: "text", text: "继续" }],
        include_interaction_state: true,
        include_render_observation: false,
      },
    );
  });

  it("attached runtime 读取当前 AgentRun 的 attachment-local bindings", async () => {
    mocks.get
      .mockResolvedValueOnce({
        instance: {
          instance_id: "instance-1",
          definition_id: "canvas-1",
          definition_revision_id: "revision-1",
          state: {},
          state_revision: 0,
        },
        runtime_bindings: [],
      })
      .mockResolvedValueOnce(definition());
    mocks.post.mockResolvedValueOnce({ operations: [] });
    mocks.get.mockResolvedValueOnce(runtimeSnapshot());

    await fetchInteractionCanvasRuntimeSnapshot("instance 1", {
      runId: "run 1",
      agentId: "agent 1",
    });

    expect(mocks.get).toHaveBeenNthCalledWith(
      1,
      "/interaction-instances/instance%201?run_id=run%201&agent_id=agent%201",
    );
  });
});

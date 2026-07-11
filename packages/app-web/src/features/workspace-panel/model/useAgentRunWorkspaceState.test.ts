import { describe, expect, it } from "vitest";

import type { AgentRunProductView } from "../../../generated/workflow-contracts";
import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
import type { AgentRunRuntimeInspectResponse } from "../../../services/agentRunRuntime";
import {
  agentRunWorkspaceResourceSurface,
  beginAgentRunWorkspaceStateLoad,
  emptyAgentRunWorkspaceState,
  settleAgentRunWorkspaceStateLoad,
  type AgentRunWorkspaceState,
} from "./useAgentRunWorkspaceState";

const runtimeSurface: ResolvedVfsSurface = {
  surface_ref: "agent-run:run-1:agent-1",
  source: { source_type: "agent_run", run_id: "run-1", agent_id: "agent-1" },
  mounts: [],
};

const workspace: AgentRunProductView = {
  run_ref: { run_id: "run-1" },
  agent_ref: { run_id: "run-1", agent_id: "agent-1" },
  project_id: "project-1",
  shell: {
    display_title: "Workspace title",
    title_source: "user",
    lifecycle_status: "active",
    last_activity_at: "2026-07-11T00:00:00.000Z",
  },
  agent: {
    agent_ref: { run_id: "run-1", agent_id: "agent-1" },
    project_id: "project-1",
    source: "project_agent",
    status: "active",
    created_at: "2026-07-11T00:00:00.000Z",
    updated_at: "2026-07-11T00:00:00.000Z",
  },
  current_frame: {
    frame_ref: { agent_id: "agent-1", frame_id: "frame-1", revision: 1 },
    capability_surface: {},
    context_slice: {},
    vfs_surface: {},
    mcp_surface: {},
    model_config: {
      status: "resolved",
      effective_executor_config: {
        executor: "PI_AGENT",
        provider_id: "provider-1",
        model_id: "model-1",
        source: "frame_execution_profile",
      },
      missing_fields: [],
    },
  },
  subject_associations: [],
  lineage: { children: [] },
  resource_surface: runtimeSurface,
};

const runtimeInspect: AgentRunRuntimeInspectResponse = {
  target: { run_id: "run-1", agent_id: "agent-1" },
  binding: null,
  snapshot: null,
};

function loadedState(): AgentRunWorkspaceState {
  return settleAgentRunWorkspaceStateLoad(
    emptyAgentRunWorkspaceState(),
    "run-1",
    "agent-1",
    "agentrun:run-1:agent-1",
    "replace",
    { status: "fulfilled", value: workspace },
    { status: "fulfilled", value: runtimeInspect },
  );
}

describe("AgentRun product and Runtime projection state", () => {
  it("直接使用 current product projection resource_surface", () => {
    expect(agentRunWorkspaceResourceSurface(workspace)).toBe(runtimeSurface);
  });

  it("refresh pending 期间保留已加载的两路事实", () => {
    const refreshing = beginAgentRunWorkspaceStateLoad(
      loadedState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
    );

    expect(refreshing.status).toBe("refreshing");
    expect(refreshing.workspace).toBe(workspace);
    expect(refreshing.runtime_inspect).toBe(runtimeInspect);
  });

  it("workspace 失败时保留成功的 Runtime inspect", () => {
    const state = settleAgentRunWorkspaceStateLoad(
      emptyAgentRunWorkspaceState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "replace",
      { status: "rejected", reason: new Error("workspace failed") },
      { status: "fulfilled", value: runtimeInspect },
    );

    expect(state.status).toBe("error");
    expect(state.workspace).toBeNull();
    expect(state.workspace_error).toBe("workspace failed");
    expect(state.runtime_inspect).toBe(runtimeInspect);
    expect(state.runtime_inspect_error).toBeNull();
  });

  it("refresh Runtime inspect 失败时保留上一份 Runtime snapshot 和新 workspace", () => {
    const state = settleAgentRunWorkspaceStateLoad(
      loadedState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
      { status: "fulfilled", value: workspace },
      { status: "rejected", reason: new Error("runtime failed") },
    );

    expect(state.status).toBe("ready");
    expect(state.workspace).toBe(workspace);
    expect(state.workspace_error).toBeNull();
    expect(state.runtime_inspect).toBe(runtimeInspect);
    expect(state.runtime_inspect_error).toBe("runtime failed");
  });

  it("refresh workspace 失败时保留上一份 workspace surface 和新 Runtime inspect", () => {
    const state = settleAgentRunWorkspaceStateLoad(
      loadedState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
      { status: "rejected", reason: new Error("workspace failed") },
      { status: "fulfilled", value: runtimeInspect },
    );

    expect(state.workspace).toBe(workspace);
    expect(state.frame).toBe(workspace.current_frame);
    expect(state.runtime_surface).toBe(runtimeSurface);
    expect(state.runtime_inspect).toBe(runtimeInspect);
    expect(state.workspace_error).toBe("workspace failed");
  });

  it("empty state 不包含其他 target 的事实", () => {
    expect(emptyAgentRunWorkspaceState().workspace).toBeNull();
    expect(emptyAgentRunWorkspaceState().runtime_inspect).toBeNull();
  });
});

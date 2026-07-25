import { describe, expect, it } from "vitest";

import type { AgentRunWorkspaceView } from "../../../types";
import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
import {
  agentRunWorkspaceResourceSurface,
  beginAgentRunWorkspaceStateLoad,
  emptyAgentRunWorkspaceState,
  failAgentRunWorkspaceStateLoad,
  type AgentRunWorkspaceState,
} from "./useAgentRunWorkspaceState";

const runtimeSurface: ResolvedVfsSurface = {
  surface_ref: "agent-run:run-1:agent-1",
  source: { source_type: "agent_run", run_id: "run-1", agent_id: "agent-1" },
  mounts: [],
};

const workspace: AgentRunWorkspaceView = {
  run_ref: { run_id: "run-1" },
  agent_ref: { run_id: "run-1", agent_id: "agent-1" },
  project_id: "project-1",
  shell: {
    display_title: "Workspace title",
    title_source: "user",
    delivery_status: "active",
    last_activity_at: "2026-07-11T00:00:00.000Z",
  },
  control_plane: {
    status: "running",
    ownership: {
      run_created_by_user_id: "owner-user",
      agent_created_by_user_id: "owner-user",
      current_user_controls_run: true,
    },
  },
  workspace_modules: [],
  agent: {
    agent_ref: { run_id: "run-1", agent_id: "agent-1" },
    project_id: "project-1",
    source: "project_agent",
    status: "active",
    created_at: "2026-07-11T00:00:00.000Z",
    updated_at: "2026-07-11T00:00:00.000Z",
  },
  subject_associations: [],
  children: [],
  resource_surface: runtimeSurface,
};

function loadedState(): AgentRunWorkspaceState {
  return {
    ...emptyAgentRunWorkspaceState(),
    run_id: "run-1",
    agent_id: "agent-1",
    source_key: "agentrun:run-1:agent-1",
    status: "ready",
    workspace,
    runtime_surface: runtimeSurface,
  };
}

describe("AgentRun workspace state", () => {
  it("直接使用 Main workspace projection resource_surface", () => {
    expect(agentRunWorkspaceResourceSurface(workspace)).toBe(runtimeSurface);
  });

  it("无 frame 的 refresh pending 与 Main 一样保持 loading 并保留 workspace", () => {
    const refreshing = beginAgentRunWorkspaceStateLoad(
      loadedState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
    );

    expect(refreshing.status).toBe("loading");
    expect(refreshing.workspace).toBe(workspace);
  });

  it("workspace 失败时进入 error", () => {
    const state = failAgentRunWorkspaceStateLoad(
      emptyAgentRunWorkspaceState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "replace",
      "workspace failed",
    );

    expect(state.status).toBe("error");
    expect(state.workspace).toBeNull();
    expect(state.error).toBe("workspace failed");
  });

  it("refresh workspace 失败时保留上一份 workspace surface", () => {
    const state = failAgentRunWorkspaceStateLoad(
      loadedState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
      "workspace failed",
    );

    expect(state.workspace).toBe(workspace);
    expect(state.frame).toBeNull();
    expect(state.runtime_surface).toBe(runtimeSurface);
    expect(state.status).toBe("error");
    expect(state.error).toBe("workspace failed");
  });

  it("empty state 不包含其他 target 的事实", () => {
    expect(emptyAgentRunWorkspaceState().workspace).toBeNull();
  });
});

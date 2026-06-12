import { describe, expect, it } from "vitest";

import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
import type {
  AgentFrameRuntimeView,
  AgentRunWorkspaceView,
} from "../../../types";
import {
  agentRunWorkspaceResourceSurface,
  beginAgentRunWorkspaceStateLoad,
  emptyAgentRunWorkspaceState,
  failAgentRunWorkspaceStateLoad,
  type AgentRunWorkspaceProjectionState,
} from "./useAgentRunWorkspaceState";

const frameRuntime: AgentFrameRuntimeView = {
  frame_ref: {
    agent_id: "agent-1",
    frame_id: "frame-1",
  },
  capability_surface: {},
  context_slice: {},
  vfs_surface: {},
  mcp_surface: {},
  runtime_session_refs: [{ runtime_session_id: "session-1" }],
};

const workspace: AgentRunWorkspaceView = {
  run_ref: { run_id: "run-1" },
  agent_ref: { run_id: "run-1", agent_id: "agent-1" },
  project_id: "project-1",
  shell: {
    display_title: "Workspace title",
    title_source: "session_meta",
    workspace_status: "running",
    delivery_status: "running",
    last_activity_at: "2026-06-12T00:00:00.000Z",
  },
  delivery_runtime_ref: { runtime_session_id: "session-1" },
  control_plane: {
    status: "running",
  },
  frame_runtime: frameRuntime,
  subject_associations: [],
  actions: {
    send_next: { enabled: false, unavailable_reason: "running" },
    enqueue: { enabled: true },
    steer: { enabled: true },
    cancel: { enabled: true },
  },
  pending_queue: {
    paused: false,
    can_resume: false,
  },
  pending_messages: [],
};

const runtimeSurface: ResolvedVfsSurface = {
  surface_ref: "agent-run:run-1:agent-1",
  source: { source_type: "agent_run", run_id: "run-1", agent_id: "agent-1" },
  mounts: [
    {
      id: "main",
      display_name: "main",
      provider: "relay_fs",
      backend_id: "backend-1",
      capabilities: ["read", "write", "list", "search", "exec"],
      default_write: true,
      purpose: "workspace",
      backend_online: true,
      edit_capabilities: {
        create: true,
        delete: true,
        rename: true,
      },
    },
  ],
  default_mount_id: "main",
};

function loadedState(): AgentRunWorkspaceProjectionState {
  return {
    ...emptyAgentRunWorkspaceState(),
    run_id: "run-1",
    agent_id: "agent-1",
    source_key: "agentrun:run-1:agent-1",
    status: "ready",
    workspace,
    runtime_session_id: "session-1",
    runtime_surface: runtimeSurface,
    frame: frameRuntime,
  };
}

describe("AgentRun workspace refresh state", () => {
  it("直接使用 AgentRun workspace snapshot resource_surface", () => {
    const snapshotWorkspace: AgentRunWorkspaceView = {
      ...workspace,
      resource_surface: runtimeSurface,
      conversation: {
        identity: {
          run_ref: { run_id: "run-1" },
          agent_ref: { run_id: "run-1", agent_id: "agent-1" },
          project_id: "project-1",
        },
        lifecycle_context: {
          frame_ref: {
            agent_id: "agent-1",
            frame_id: "frame-1",
          },
          delivery_runtime_ref: { runtime_session_id: "session-1" },
          subject_associations: [],
        },
        execution: {
          status: "running_active",
          runtime_session_ref: { runtime_session_id: "session-1" },
          active_turn_id: "turn-1",
        },
        model_config: {
          status: "resolved",
          missing_fields: [],
        },
        commands: {
          keyboard: {
            enter: "enqueue",
            ctrl_enter: "steer",
          },
          primary: "enqueue",
          secondary: "steer",
          commands: [],
        },
        pending: {
          visible_message_count: 0,
          paused: false,
          user_attention: false,
        },
        resource_surface: runtimeSurface,
        diagnostics: [],
      },
    };

    expect(agentRunWorkspaceResourceSurface(snapshotWorkspace)).toBe(runtimeSurface);
  });

  it("初始加载成功后触发 refresh 时 pending 期间保留 runtime identity 与 workspace", () => {
    const refreshing = beginAgentRunWorkspaceStateLoad(
      loadedState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
    );

    expect(refreshing.status).toBe("refreshing");
    expect(refreshing.runtime_session_id).toBe("session-1");
    expect(refreshing.workspace).toBe(workspace);
    expect(refreshing.runtime_surface).toBe(runtimeSurface);
    expect(refreshing.frame).toBe(frameRuntime);
    expect(refreshing.error).toBeNull();
  });

  it("refresh 失败时不清空上一帧 runtime identity", () => {
    const refreshing = beginAgentRunWorkspaceStateLoad(
      loadedState(),
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
    );
    const failed = failAgentRunWorkspaceStateLoad(
      refreshing,
      "run-1",
      "agent-1",
      "agentrun:run-1:agent-1",
      "refresh",
      "refresh failed",
    );

    expect(failed.status).toBe("error");
    expect(failed.error).toBe("refresh failed");
    expect(failed.runtime_session_id).toBe("session-1");
    expect(failed.workspace).toBe(workspace);
    expect(failed.runtime_surface).toBe(runtimeSurface);
    expect(failed.frame).toBe(frameRuntime);
  });
});

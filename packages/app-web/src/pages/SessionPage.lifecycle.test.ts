import { describe, expect, it } from "vitest";

import type { AgentFrameRuntimeView, LifecycleAgentView } from "../types";
import {
  resolveSessionLifecycleDetailTarget,
  resolveSessionLifecycleRunId,
} from "./SessionPage.lifecycle";

function frame(runtimeSessionId = "runtime-1"): AgentFrameRuntimeView {
  return {
    frame_ref: {
      agent_id: "agent-1",
      frame_id: "frame-1",
      revision: 1,
    },
    capability_surface: {},
    context_slice: {},
    vfs_surface: {},
    mcp_surface: {},
    runtime_session_refs: [{ runtime_session_id: runtimeSessionId }],
  };
}

function agent(runtimeSessionId = "runtime-1"): LifecycleAgentView {
  return {
    agent_ref: {
      run_id: "run-1",
      agent_id: "agent-1",
    },
    project_id: "project-1",
    agent_kind: "project_agent",
    agent_role: "primary",
    status: "running",
    current_frame_id: "frame-1",
    delivery_runtime_ref: { runtime_session_id: runtimeSessionId },
    created_at: "2026-06-03T00:00:00Z",
    updated_at: "2026-06-03T00:00:00Z",
  };
}

describe("SessionPage lifecycle detail resolution", () => {
  it("优先使用 hook runtime active workflow 中的 run id", () => {
    const target = resolveSessionLifecycleDetailTarget({
      currentSessionId: "runtime-1",
      activeWorkflowRunId: "run-from-hook",
      activeFrame: frame(),
      agents: new Map(),
    });

    expect(target).toEqual({
      runId: "run-from-hook",
      agentId: "agent-1",
      frameId: "frame-1",
    });
  });

  it("从当前 frame 对应的 lifecycle agent projection 解析 run id", () => {
    const agents = new Map([["agent-1", agent()]]);

    expect(resolveSessionLifecycleRunId({
      currentSessionId: "runtime-1",
      activeWorkflowRunId: null,
      activeFrame: frame(),
      agents,
    })).toBe("run-1");
  });

  it("普通 runtime trace 没有 frame/agent 关联时不暴露详情入口", () => {
    const target = resolveSessionLifecycleDetailTarget({
      currentSessionId: "runtime-1",
      activeWorkflowRunId: null,
      activeFrame: null,
      agents: new Map(),
    });

    expect(target).toBeNull();
  });

  it("frame 未绑定当前 runtime session 时不解析 agent projection", () => {
    const target = resolveSessionLifecycleDetailTarget({
      currentSessionId: "runtime-1",
      activeWorkflowRunId: null,
      activeFrame: frame("runtime-other"),
      agents: new Map([["agent-1", agent()]]),
    });

    expect(target).toBeNull();
  });
});

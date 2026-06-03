import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  AgentFrameHookRuntimeInfo,
  LifecycleRunView,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
} from "../../types";
import { ContextOverviewTab } from "./ContextOverviewTab";

const contextSnapshot: SessionContextSnapshot = {
  executor: {
    executor: "PI_AGENT",
    source: "session.meta.executor_config",
  },
  project_defaults: {
    context_containers: [],
  },
  owner_context: {
    owner_level: "project",
    agent_key: "default",
    agent_display_name: "Default Agent",
  },
  effective: {
    session_composition: {
      persona_label: null,
      persona_prompt: null,
      workflow_steps: [],
      required_context_blocks: [],
    },
    tool_visibility: {
      markdown: "",
      resolved: true,
      toolset_label: "runtime",
      tool_names: [],
      mcp_servers: [],
    },
    runtime_policy: {
      markdown: "",
      workspace_attached: true,
      vfs_attached: true,
      mcp_enabled: false,
      visible_mounts: ["runtime-shared", "runtime-lifecycle"],
      visible_tools: [],
      writable_mounts: [],
      exec_mounts: [],
      path_policy: "vfs",
    },
  },
};

const runtimeSurface: ResolvedVfsSurface = {
  surface_ref: "session:sess-projection",
  source: {
    source_type: "session_runtime",
    session_id: "sess-projection",
  },
  default_mount_id: "runtime-shared",
  mounts: [
    {
      id: "runtime-shared",
      display_name: "Runtime Shared",
      provider: "inline_fs",
      backend_id: "inline",
      capabilities: ["read", "write", "list"],
      default_write: true,
      purpose: "vfs_mount",
      backend_online: undefined,
      file_count: undefined,
      edit_capabilities: {
        create: true,
        delete: true,
        rename: true,
      },
    },
    {
      id: "runtime-lifecycle",
      display_name: "Runtime Lifecycle",
      provider: "lifecycle_vfs",
      backend_id: "lifecycle",
      capabilities: ["read", "list"],
      default_write: false,
      purpose: "lifecycle",
      backend_online: undefined,
      file_count: undefined,
      edit_capabilities: {
        create: false,
        delete: false,
        rename: false,
      },
    },
  ],
};

const sessionCapabilities: SessionBaselineCapabilities = {
  skills: [
    {
      name: "runtime-skill",
      description: "Skill from final projection",
      file_path: "runtime-shared://skills/runtime-skill/SKILL.md",
      disable_model_invocation: false,
    },
  ],
};

const lifecycleRunView: LifecycleRunView = {
  run_ref: { run_id: "run-projection-123456" },
  project_id: "project-projection",
  topology: "workflow_graph",
  root_graph_id: "lifecycle-projection",
  status: "running",
  workflow_graph_instances: [
    {
      id: "graph-instance-projection",
      run_id: "run-projection-123456",
      graph_id: "graph-projection",
      role: "primary",
      status: "running",
      activities: [
        {
          activity_key: "implement",
          status: "running",
          attempts: [
            {
              graph_instance_id: "graph-instance-projection",
              activity_key: "implement",
              attempt: 2,
              status: "running",
            },
            {
              graph_instance_id: "graph-instance-projection",
              activity_key: "review",
              attempt: 1,
              status: "completed",
            },
          ],
        },
      ],
    },
  ],
  agents: [],
  subject_associations: [],
  runtime_trace_refs: [],
  execution_log: [],
  created_at: "2026-06-02T00:00:00Z",
  updated_at: "2026-06-02T00:00:00Z",
  last_activity_at: "2026-06-02T00:00:00Z",
};

const hookRuntime: AgentFrameHookRuntimeInfo = {
  runtime_adapter_session_id: "sess-projection",
  revision: 1,
  snapshot: {
    runtime_adapter_session_id: "sess-projection",
    sources: [],
    tags: [],
    injections: [],
    diagnostics: [],
    metadata: {
      active_workflow: {
        workflow_graph_id: "lifecycle-projection",
        lifecycle_key: "projection-lifecycle",
        lifecycle_name: "Projection Lifecycle",
        run_id: "run-projection-123456",
        run_status: "running",
        activity_key: "implement",
        activity_title: "Implement Projection",
        primary_workflow_id: "graph-projection",
        primary_workflow_name: "Primary Projection",
      },
    },
  },
  diagnostics: [],
  trace: [],
  pending_actions: [],
};

describe("ContextOverviewTab projection contract", () => {
  it("只从 final runtime surface 展示 Session 地址空间与派生能力", () => {
    const html = renderToStaticMarkup(
      <ContextOverviewTab
        contextSnapshot={contextSnapshot}
        ownerStory={null}
        ownerProjectName="Projection Project"
        executorSummary={contextSnapshot.executor}
        runtimeSurface={runtimeSurface}
        hookRuntime={null}
        sessionCapabilities={sessionCapabilities}
        lifecycleRun={null}
      />,
    );

    expect(html).toContain("Runtime Shared");
    expect(html).toContain("Runtime Lifecycle");
    expect(html).toContain("2 个运行时 mount");
    expect(html).toContain("runtime-skill");
  });

  it("从 lifecycle run view 的 graph instance projection 展示活跃 attempt", () => {
    const html = renderToStaticMarkup(
      <ContextOverviewTab
        contextSnapshot={contextSnapshot}
        ownerStory={null}
        ownerProjectName="Projection Project"
        executorSummary={contextSnapshot.executor}
        runtimeSurface={runtimeSurface}
        hookRuntime={hookRuntime}
        sessionCapabilities={sessionCapabilities}
        lifecycleRun={lifecycleRunView}
      />,
    );

    expect(html).toContain("Projection Lifecycle");
    expect(html).toContain("Attempt · Running");
    expect(html).toContain("进度 1/2");
    expect(html).toContain("graph-instance-projection:implement");
  });

  it("无 owner/context snapshot 时仍从 lifecycle run projection 展示运行状态", () => {
    const html = renderToStaticMarkup(
      <ContextOverviewTab
        contextSnapshot={null}
        ownerStory={null}
        ownerProjectName=""
        executorSummary={null}
        runtimeSurface={null}
        hookRuntime={null}
        sessionCapabilities={null}
        lifecycleRun={lifecycleRunView}
      />,
    );

    expect(html).toContain("会话上下文");
    expect(html).toContain("Session Agent");
    expect(html).toContain("Run · Running");
    expect(html).not.toContain("当前会话还没有关联的上下文信息。");
  });
});

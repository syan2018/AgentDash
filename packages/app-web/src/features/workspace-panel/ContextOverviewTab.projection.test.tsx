import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
  WorkflowRun,
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

const workflowRuns: WorkflowRun[] = [];

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
        workflowRuns={workflowRuns}
      />,
    );

    expect(html).toContain("Runtime Shared");
    expect(html).toContain("Runtime Lifecycle");
    expect(html).toContain("2 个运行时 mount");
    expect(html).toContain("runtime-skill");
  });
});

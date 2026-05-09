import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { parseContextFrame } from "../model/contextFrame";
import { ContextFrameCard } from "./ContextFrameCard";

describe("ContextFrameCard", () => {
  it("解析 context_frame 的结构化 sections 与 Agent 可见文本", () => {
    const notice = parseContextFrame(sampleNotice());

    expect(notice?.phase_node).toBe("apply");
    expect(notice?.rendered_text).toContain("Tool Schema Delta");
    expect(notice?.sections).toHaveLength(2);
    expect(notice?.sections[1]?.kind).toBe("tool_schema_delta");
  });

  it("渲染 context_frame 专用卡片入口", () => {
    const markup = renderToStaticMarkup(<ContextFrameCard data={sampleNotice()} />);

    expect(markup).toContain("CTX");
    expect(markup).toContain("Agent 上下文已更新");
    expect(markup).toContain("阶段 apply");
    expect(markup).toContain("工具 Schema 变化 1 项变化");
    expect(markup).not.toContain("工具 Schema 变化 2 项变化");
  });

  it("解析 bootstrap_context section", () => {
    const notice = parseContextFrame(sampleBootstrapNotice());

    expect(notice?.kind).toBe("bootstrap_context");
    expect(notice?.sections[0]?.kind).toBe("bootstrap_context");
    const markup = renderToStaticMarkup(<ContextFrameCard data={sampleBootstrapNotice()} />);

    expect(markup).toContain("Bootstrap Context 2 个片段");
    expect(markup).toContain("Agent 上下文已更新");
  });

  it("解析并渲染独立 surface frames", () => {
    const workspace = parseContextFrame(sampleWorkspaceSurfaceNotice());
    const skill = parseContextFrame(sampleSkillSurfaceNotice());
    const hook = parseContextFrame(sampleHookRuntimeSurfaceNotice());

    expect(workspace?.kind).toBe("workspace_surface");
    expect(workspace?.sections[0]?.kind).toBe("workspace_surface");
    expect(skill?.sections[0]?.kind).toBe("skill_surface");
    expect(hook?.sections[0]?.kind).toBe("hook_runtime_surface");

    const markup = renderToStaticMarkup(
      <>
        <ContextFrameCard data={sampleWorkspaceSurfaceNotice()} />
        <ContextFrameCard data={sampleSkillSurfaceNotice()} />
        <ContextFrameCard data={sampleHookRuntimeSurfaceNotice()} />
      </>
    );

    expect(markup).toContain("Workspace Surface 1 个挂载");
    expect(markup).toContain("Skill Surface 1 个 skill");
    expect(markup).toContain("Hook Runtime Surface 2 个 pending action");
  });

  it("解析并渲染 auto_resume frame", () => {
    const notice = parseContextFrame(sampleAutoResumeNotice());

    expect(notice?.kind).toBe("auto_resume");
    expect(notice?.sections[0]?.kind).toBe("auto_resume");

    const markup = renderToStaticMarkup(<ContextFrameCard data={sampleAutoResumeNotice()} />);
    expect(markup).toContain("Auto Resume hook_before_stop_continue");
    expect(markup).toContain("Agent 上下文已更新");
  });

  it("解析并渲染 compaction_summary frame", () => {
    const notice = parseContextFrame(sampleCompactionNotice());

    expect(notice?.kind).toBe("compaction_summary");
    expect(notice?.sections[0]?.kind).toBe("compaction_summary");

    const markup = renderToStaticMarkup(<ContextFrameCard data={sampleCompactionNotice()} />);
    expect(markup).toContain("Compaction Summary 12 条消息");
    expect(markup).toContain("Agent 上下文已更新");
  });
});

function sampleNotice(): Record<string, unknown> {
  return {
      id: "runtime-context-apply-1",
      kind: "runtime_context_update",
      source: "runtime_context_update",
      phase_node: "apply",
      apply_mode: "live",
      delivery_status: "queued_for_transform_context",
      delivery_channel: "turn_start",
      message_role: "user",
      rendered_text: "## Tool Schema Delta — Step Transition: apply",
      created_at_ms: 1,
      sections: [
        {
          kind: "capability_delta",
          added_capabilities: [],
          removed_capabilities: [],
          effective_capabilities: ["workflow_management"],
          blocked_tool_paths: [],
          unblocked_tool_paths: ["workflow_management::upsert_workflow_tool"],
          whitelisted_tool_paths: [],
          removed_whitelist_paths: [],
          added_mcp_servers: ["agentdash-workflow-tools"],
          removed_mcp_servers: [],
          changed_mcp_servers: [],
          vfs_mounts_added: [],
          vfs_mounts_removed: [],
        },
        {
          kind: "tool_schema_delta",
          restored_tool_paths: ["workflow_management::upsert_workflow_tool"],
          blocked_tool_paths: [],
          removed_tool_paths: [],
          added_tools: [
            {
              name: "mcp_agentdash_workflow_tools_upsert_workflow_tool",
              description: "创建或更新 Workflow 定义",
              parameters_schema: {
                type: "object",
                properties: {
                  key: { type: "string" },
                },
              },
              capability_key: "workflow_management",
              source: "platform_mcp:workflow",
              tool_path: "workflow_management::upsert_workflow_tool",
            },
          ],
        },
      ],
    };
}

function sampleBootstrapNotice(): Record<string, unknown> {
  return {
    id: "bootstrap-context-task-1",
    kind: "bootstrap_context",
    source: "runtime_context_update",
    phase_node: "task_start",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Bootstrap Context",
    created_at_ms: 1,
    sections: [
      {
        kind: "bootstrap_context",
        title: "Bootstrap Context",
        summary: "Session 启动上下文已注入",
        fragments: [
          {
            slot: "user_preferences",
            label: "User Preferences",
            source: "settings:user_preferences",
            content: "- 中文交流",
          },
          {
            slot: "task",
            label: "Task",
            source: "test",
            content: "处理 ContextFrame",
          },
        ],
      },
    ],
  };
}

function sampleWorkspaceSurfaceNotice(): Record<string, unknown> {
  return {
    id: "workspace-surface-1",
    kind: "workspace_surface",
    source: "runtime_context_update",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Workspace Surface",
    created_at_ms: 1,
    sections: [
      {
        kind: "workspace_surface",
        title: "Workspace Surface",
        summary: "当前工作目录与 1 个 VFS 挂载已作为独立上下文帧注入。",
        working_directory: "/repo",
        default_mount: "workspace",
        mounts: [
          {
            id: "workspace",
            display_name: "Workspace",
            provider: "local",
            root_ref: "/repo",
            capabilities: ["read", "write"],
          },
        ],
      },
    ],
  };
}

function sampleSkillSurfaceNotice(): Record<string, unknown> {
  return {
    id: "skill-surface-1",
    kind: "skill_surface",
    source: "runtime_context_update",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Skill Surface",
    created_at_ms: 1,
    sections: [
      {
        kind: "skill_surface",
        title: "Skill Surface",
        summary: "当前有 1 个可由模型按需加载的 skill。",
        read_tool: "fs_read",
        skills: [
          {
            name: "trellis-check",
            description: "质量验证",
            file_path: "/repo/.agents/skills/trellis-check/SKILL.md",
            disable_model_invocation: false,
          },
        ],
      },
    ],
  };
}

function sampleHookRuntimeSurfaceNotice(): Record<string, unknown> {
  return {
    id: "hook-runtime-surface-1",
    kind: "hook_runtime_surface",
    source: "runtime_context_update",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Hook Runtime Surface",
    created_at_ms: 1,
    sections: [
      {
        kind: "hook_runtime_surface",
        title: "Hook Runtime Surface",
        summary: "Hook Runtime 已启用",
        pending_action_count: 2,
      },
    ],
  };
}

function sampleAutoResumeNotice(): Record<string, unknown> {
  return {
    id: "auto-resume-1",
    kind: "auto_resume",
    source: "runtime_context_update",
    delivery_status: "queued_as_user_prompt",
    delivery_channel: "user_prompt",
    message_role: "user",
    rendered_text: "继续执行当前流程。",
    created_at_ms: 1,
    sections: [
      {
        kind: "auto_resume",
        title: "Auto Resume",
        summary: "系统根据 Hook stop gate 自动发起续跑提示。",
        reason: "hook_before_stop_continue",
        prompt: "继续执行当前流程。",
      },
    ],
  };
}

function sampleCompactionNotice(): Record<string, unknown> {
  return {
    id: "compaction-summary-1",
    kind: "compaction_summary",
    source: "runtime_context_update",
    delivery_status: "applied_to_compacted_context",
    delivery_channel: "continuation",
    message_role: "system",
    rendered_text: "## Compaction Summary\n压缩后的历史摘要",
    created_at_ms: 1,
    sections: [
      {
        kind: "compaction_summary",
        title: "Compaction Summary",
        summary: "压缩后的历史摘要",
        tokens_before: 48000,
        messages_compacted: 12,
        compacted_until_ref: { turn_id: "turn-1", entry_index: 3 },
        timestamp_ms: 1710000000000,
      },
    ],
  };
}

import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { parseContextFrame, type ContextFrame } from "../model/contextFrame";
import { ContextFrameCard } from "./ContextFrameCard";

describe("ContextFrameCard", () => {
  it("默认折叠时仅渲染 header", () => {
    const markup = renderToStaticMarkup(<ContextFrameCard frame={readFrame(sampleNotice())} />);

    expect(markup).toContain("CTX");
    expect(markup).toContain("CAPABILITY");
    // 折叠态：phase node 出现在 summary 小字
    expect(markup).toContain("apply");
    // 折叠态：不渲染内层 section body
    expect(markup).not.toContain("Capability Keys");
  });

  it("展开后按 sections[] 原顺序渲染单列长页", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleNotice())} defaultExpanded />,
    );

    // 空 capability_key_delta 兼容旧事件解析，但不在 UI 中展示成“no change”能力变更。
    expect(markup).not.toContain("Capability Keys");
    expect(markup).not.toContain("本次无能力 key 变更");
    expect(markup).toContain("Tool Paths");
    expect(markup).toContain("MCP Servers");
    expect(markup).toContain("Tool Schema");
    // tool_schema_delta after dedup: 1 added tool
    expect(markup).toContain("+1");
    // tab bar contains CAP token (capability_state_delta)
    expect(markup).toContain("CAP");
    // Agent 原文折叠块
    expect(markup).toContain("Agent 实际原文");
    // 调试信息折叠块
    expect(markup).toContain("调试信息");
  });

  it("同一卡展示 MCP server delta 与 project MCP ToolSchema", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleProjectMcpNotice())} defaultExpanded />,
    );

    expect(markup).toContain("MCP Servers");
    expect(markup).toContain("code-analyzer");
    expect(markup).toContain("Tool Schema");
    expect(markup).toContain("mcp_code_analyzer_scan_repo");
    expect(markup).toContain("mcp:code-analyzer");
    expect(markup).toContain("1 params");
    expect(markup).toContain("Agent 实际原文");
  });

  it("assignment_context 解析并渲染 ASN token", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleAssignmentNotice())} defaultExpanded />,
    );
    // frame tab 的 ASN token + section block 标题
    expect(markup).toContain("ASN");
    expect(markup).toContain("Assignment Context");
    expect(markup).toContain("2 fragments");
    expect(markup).toContain("ASN");
  });

  it("渲染 identity frame", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleIdentityNotice())} defaultExpanded />,
    );
    expect(markup).toContain("IDN");
    expect(markup).toContain("Identity");
    expect(markup).toContain("System Prompt");
  });

  it("渲染 pending_action frame", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(samplePendingActionNotice())} defaultExpanded />,
    );
    expect(markup).toContain("Pending Action");
    expect(markup).toContain("follow_up_required");
    expect(markup).toContain("ACT");
  });

  it("渲染 auto_resume frame", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleAutoResumeNotice())} defaultExpanded />,
    );
    expect(markup).toContain("Auto Resume");
    expect(markup).toContain("hook_before_stop_continue");
    expect(markup).toContain("RESUME");
    expect(markup).toContain("RSM");
  });

  it("渲染 compaction_summary frame", () => {
    const frame = readFrame(sampleCompactionNotice());
    const compaction = frame.sections.find((section) => section.kind === "compaction_summary");

    expect(compaction?.kind).toBe("compaction_summary");
    if (compaction?.kind !== "compaction_summary") {
      throw new Error("compaction_summary section should parse");
    }
    expect(compaction.messages_compacted).toBe(12);
    expect(compaction.projection_version).toBe(7);
    expect(compaction.compaction_id).toBe("compaction-1");
    expect(compaction.source_start_event_seq).toBe(3);
    expect(compaction.source_end_event_seq).toBe(42);
    expect(compaction.first_kept_event_seq).toBe(43);

    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={frame} defaultExpanded />,
    );
    expect(markup).toContain("Compaction Summary");
    expect(markup).toContain("12 messages");
    expect(markup).toContain("COMPACTION");
    expect(markup).toContain("CMP");
    expect(markup).toContain("projection: v7");
    expect(markup).toContain("source: 3-42");
    expect(markup).toContain("checkpoint compaction-1");
  });

  it("渲染 skill_delta 的 provider scoped identity", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleSkillDeltaNotice())} defaultExpanded />,
    );
    expect(markup).toContain("SKILL UPDATE");
    expect(markup).not.toContain("CAPABILITY DELTA");
    expect(markup).toContain("Skills");
    expect(markup).toContain("+2");
    expect(markup).toContain("−1");
    expect(markup).toContain("↻1");
    expect(markup).toContain("copilot");
    expect(markup).toContain("copilot/config-edit");
    expect(markup).toContain("workspace");
    expect(markup).toContain("workspace/config-edit");
    expect(markup).toContain("explicit only");
  });

  it("渲染 memory inventory delta", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleMemoryDeltaNotice())} defaultExpanded />,
    );
    expect(markup).toContain("MEMORY UPDATE");
    expect(markup).toContain("Memory Inventory");
    expect(markup).toContain("+1");
    expect(markup).toContain("Agent Memory");
    expect(markup).toContain("builtin.project_agent_memory");
    expect(markup).toContain("present");
    expect(markup).toContain("rev abc123");
  });

  it("有能力 key 增删时仍展示 capability delta", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleCapabilityKeyDeltaNotice())} defaultExpanded />,
    );

    expect(markup).toContain("CAPABILITY DELTA");
    expect(markup).toContain("Capability Keys");
    expect(markup).toContain("workflow_management");
  });

  it("渲染 companion agent roster delta", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleCompanionRosterNotice())} defaultExpanded />,
    );
    expect(markup).toContain("Companion Agents");
    expect(markup).toContain("+1");
    expect(markup).toContain("−1");
    expect(markup).toContain("Review Agent");
    expect(markup).toContain("agent: reviewer");
    expect(markup).toContain("executor: PI_AGENT");
    expect(markup).toContain("当前可用 companion");
  });

  it("渲染 system_guidelines frame", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard frame={readFrame(sampleGuidelinesNotice())} defaultExpanded />,
    );
    expect(markup).toContain("GUIDELINES");
    expect(markup).toContain("User Preferences");
    expect(markup).toContain("Project Guidelines");
    expect(markup).toContain("使用中文");
    expect(markup).toContain("AGENTS.md");
    expect(markup).toContain("项目约定");
  });
});

function readFrame(value: Record<string, unknown>): ContextFrame {
  const frame = parseContextFrame(value);
  if (!frame) {
    throw new Error("invalid context frame test fixture");
  }
  return frame;
}

function sampleNotice(): Record<string, unknown> {
  return {
      id: "runtime-context-apply-1",
      kind: "capability_state_delta",
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
          kind: "capability_key_delta",
          added_capabilities: [],
          removed_capabilities: [],
          effective_capabilities: ["workflow_management"],
        },
        {
          kind: "tool_path_delta",
          blocked_tool_paths: [],
          unblocked_tool_paths: ["workflow_management::upsert_workflow_tool"],
          whitelisted_tool_paths: [],
          removed_whitelist_paths: [],
        },
        {
          kind: "mcp_server_delta",
          added_mcp_servers: ["agentdash-workflow-tools"],
          removed_mcp_servers: [],
          changed_mcp_servers: [],
        },
        {
          kind: "tool_schema_delta",
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

function sampleProjectMcpNotice(): Record<string, unknown> {
  return {
    id: "runtime-context-bootstrap-mcp",
    kind: "capability_state_delta",
    source: "runtime_context_update",
    phase_node: "bootstrap",
    apply_mode: "initial",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text:
      "## Tool Schema Delta\n\n### `mcp_code_analyzer_scan_repo`\n\ncapability: `mcp:code-analyzer`；source: `mcp:code-analyzer`；path: `mcp:code-analyzer::scan_repo`\n\n扫描仓库结构\n\n参数说明：\n\n- `root` (required, string): 扫描根目录",
    created_at_ms: 1,
    sections: [
      {
        kind: "mcp_server_delta",
        added_mcp_servers: ["code-analyzer"],
        removed_mcp_servers: [],
        changed_mcp_servers: [],
      },
      {
        kind: "tool_schema_delta",
        added_tools: [
          {
            name: "mcp_code_analyzer_scan_repo",
            description: "扫描仓库结构",
            parameters_schema: {
              type: "object",
              properties: {
                root: {
                  type: "string",
                  description: "扫描根目录",
                },
              },
              required: ["root"],
            },
            capability_key: "mcp:code-analyzer",
            source: "mcp:code-analyzer",
            tool_path: "mcp:code-analyzer::scan_repo",
            context_usage_kind: "mcp_tools",
          },
        ],
      },
    ],
  };
}

function sampleAssignmentNotice(): Record<string, unknown> {
  return {
    id: "bootstrap-context-task-1",
    kind: "assignment_context",
    source: "runtime_context_update",
    phase_node: "task_start",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Assignment Context",
    created_at_ms: 1,
    sections: [
      {
        kind: "assignment_context",
        title: "Assignment Context",
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

function sampleIdentityNotice(): Record<string, unknown> {
  return {
    id: "identity-1",
    kind: "identity",
    source: "runtime_context_update",
    delivery_status: "prepared_for_connector",
    delivery_channel: "connector_context",
    message_role: "system",
    rendered_text: "## Identity\n\n你是 AgentDash 内置编码助手。",
    created_at_ms: 1,
    sections: [
      {
        kind: "identity",
        title: "Identity",
        summary: "Connector 启动时使用的稳定 system identity。",
        fragments: [
          {
            slot: "identity",
            label: "identity_system_prompt",
            source: "connector",
            content: "## System Prompt\n你是 AgentDash 内置编码助手。",
          },
        ],
      },
    ],
  };
}

function samplePendingActionNotice(): Record<string, unknown> {
  return {
    id: "pending-action-1",
    kind: "pending_action",
    source: "companion_result",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Pending Action",
    created_at_ms: 1,
    sections: [
      {
        kind: "pending_action",
        title: "Pending Action",
        summary: "补充 follow-up",
        action_id: "follow-up-1",
        action_type: "follow_up_required",
        status: "pending",
        revision: 3,
        turn_id: "turn-1",
        instructions: ["请补充 follow-up 说明。"],
        injections: [
          {
            slot: "workflow",
            source: "follow_up",
            content: "继续落实下一步",
          },
        ],
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
        compaction_id: "compaction-1",
        projection_version: 7,
        strategy: "rolling_summary",
        trigger: "context_pressure",
        phase: "completed",
        source_start_event_seq: 3,
        source_end_event_seq: 42,
        first_kept_event_seq: 43,
        compacted_until_ref: { turn_id: "turn-1", entry_index: 3 },
        timestamp_ms: 1710000000000,
      },
    ],
  };
}

function sampleSkillDeltaNotice(): Record<string, unknown> {
  return {
    id: "skill-delta-1",
    kind: "capability_state_delta",
    source: "runtime_context_update",
    phase_node: "apply",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Skill Delta",
    created_at_ms: 1,
    sections: [
      {
        kind: "skill_delta",
        added_skills: [
          {
            name: "config-edit",
            capability_key: "copilot/config-edit",
            provider_key: "copilot",
            local_name: "config-edit",
            display_name: "Config Edit",
            description: "Edit config with provider context",
            file_path: "copilot://skills/config-edit/SKILL.md",
            exposure: "default_exposed",
          },
          {
            name: "config-edit",
            capability_key: "workspace/config-edit",
            provider_key: "workspace",
            local_name: "config-edit",
            description: "Workspace config edit",
            file_path: "workspace://skills/config-edit/SKILL.md",
          },
        ],
        removed_skills: [
          {
            name: "legacy-review",
            description: "Legacy review skill",
            file_path: "workspace://skills/legacy-review/SKILL.md",
          },
        ],
        changed_skills: [
          {
            capability_key: "provider-x/manual-only",
            provider_key: "provider-x",
            local_name: "manual-only",
            display_name: "Manual Only",
            description: "Explicit path only",
            file_path: "provider-x://skills/manual-only/SKILL.md",
            exposure: "explicit_only",
          },
        ],
      },
    ],
  };
}

function sampleMemoryDeltaNotice(): Record<string, unknown> {
  return {
    id: "memory-delta-1",
    kind: "capability_state_delta",
    source: "runtime_context_update",
    phase_node: "memory-refresh",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Memory Inventory Delta",
    created_at_ms: 1,
    sections: [
      {
        kind: "memory_inventory",
        title: "Memory Inventory Delta",
        summary: "Runtime-discovered memory sources changed.",
        mode: "delta",
        sources: [
          {
            provider_key: "builtin.project_agent_memory",
            source_key: "agent",
            display_name: "Agent Memory",
            source_uri: "agent://",
            index_uri: "agent://MEMORY.md",
            mount_id: "agent",
            scope: "agent",
            index_status: "present",
            trust_level: "first_party",
            revision: "abc123456789",
            context_usage_kind: "memory",
          },
        ],
        diagnostics: [],
        added_sources: [
          {
            provider_key: "builtin.project_agent_memory",
            source_key: "agent",
            display_name: "Agent Memory",
            source_uri: "agent://",
            index_uri: "agent://MEMORY.md",
            mount_id: "agent",
            scope: "agent",
            index_status: "present",
            trust_level: "first_party",
            revision: "abc123456789",
            context_usage_kind: "memory",
          },
        ],
        removed_sources: [],
        changed_sources: [],
      },
    ],
  };
}

function sampleCapabilityKeyDeltaNotice(): Record<string, unknown> {
  return {
    id: "capability-key-delta-1",
    kind: "capability_state_delta",
    source: "runtime_context_update",
    phase_node: "grant",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Capability State Update",
    created_at_ms: 1,
    sections: [
      {
        kind: "capability_key_delta",
        added_capabilities: ["workflow_management"],
        removed_capabilities: [],
        effective_capabilities: ["workflow_management"],
      },
    ],
  };
}

function sampleCompanionRosterNotice(): Record<string, unknown> {
  return {
    id: "companion-delta-1",
    kind: "capability_state_snapshot",
    source: "runtime_context_update",
    phase_node: "apply",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    rendered_text: "## Companion Agent Roster Delta",
    created_at_ms: 1,
    sections: [
      {
        kind: "companion_agent_roster_delta",
        added_agents: [
          {
            agent_key: "reviewer",
            executor: "PI_AGENT",
            display_name: "Review Agent",
            context_usage_kind: "agents",
          },
        ],
        removed_agent_keys: ["legacy-reviewer"],
        changed_agents: [],
        effective_agents: [
          {
            agent_key: "reviewer",
            executor: "PI_AGENT",
            display_name: "Review Agent",
            context_usage_kind: "agents",
          },
        ],
      },
    ],
  };
}

function sampleGuidelinesNotice(): Record<string, unknown> {
  return {
    id: "system-guidelines-1",
    kind: "system_guidelines",
    source: "runtime_context_update",
    delivery_status: "prepared_for_connector",
    delivery_channel: "connector_context",
    message_role: "system",
    rendered_text: "## User Preferences\n\n- 使用中文\n\n## Project Guidelines\n\n### AGENTS.md\n\n项目约定",
    created_at_ms: 1,
    sections: [
      {
        kind: "user_preferences",
        title: "User Preferences",
        summary: "用户级偏好设置。",
        items: ["使用中文"],
      },
      {
        kind: "project_guidelines",
        title: "Project Guidelines",
        summary: "工作区中发现的项目级指引文件。",
        entries: [
          {
            path: "AGENTS.md",
            content: "项目约定",
          },
        ],
      },
    ],
  };
}

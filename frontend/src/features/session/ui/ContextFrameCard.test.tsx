import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { parseContextFrame } from "../model/contextFrame";
import { ContextFrameCard } from "./ContextFrameCard";

describe("ContextFrameCard", () => {
  it("解析 context_frame 的结构化 sections 与 Agent 可见文本", () => {
    const notice = parseContextFrame(sampleNotice());

    expect(notice?.phase_node).toBe("apply");
    expect(notice?.rendered_text).toContain("Tool Schema Delta");
    expect(notice?.sections).toHaveLength(4);
    expect(notice?.sections[0]?.kind).toBe("capability_key_delta");
    expect(notice?.sections[1]?.kind).toBe("tool_path_delta");
    expect(notice?.sections[2]?.kind).toBe("mcp_server_delta");
    expect(notice?.sections[3]?.kind).toBe("tool_schema_delta");
  });

  it("默认折叠时仅渲染 header", () => {
    const markup = renderToStaticMarkup(<ContextFrameCard data={sampleNotice()} />);

    expect(markup).toContain("CTX");
    expect(markup).toContain("能力状态");
    // 折叠态：阶段 / kind 汇总出现在 header 小字
    expect(markup).toContain("阶段 apply");
    // 折叠态：不渲染内层 section body
    expect(markup).not.toContain("能力 Key 变化");
  });

  it("展开后按 sections[] 原顺序渲染单列长页", () => {
    const markup = renderToStaticMarkup(
      <ContextFrameCard data={sampleNotice()} defaultExpanded />,
    );

    // section header token + 标题 + hint
    expect(markup).toContain("能力 Key 变化");
    expect(markup).toContain("工具路径变化");
    expect(markup).toContain("MCP Server 变化");
    expect(markup).toContain("工具 Schema 变化");
    // tool_schema_delta 去重后仅 1 项变化（restored 与 added 同一 tool_path）
    expect(markup).toContain("1 项变化");
    // 确认 tab 条包含 BDL（capability_state_update）
    expect(markup).toContain("BDL");
    // Agent 原文折叠块
    expect(markup).toContain("Agent 实际原文");
    // 调试信息折叠块
    expect(markup).toContain("调试信息");
  });

  it("assignment_context 解析并渲染 ASN token", () => {
    const notice = parseContextFrame(sampleAssignmentNotice());
    expect(notice?.kind).toBe("assignment_context");
    expect(notice?.sections[0]?.kind).toBe("assignment_context");

    const markup = renderToStaticMarkup(
      <ContextFrameCard data={sampleAssignmentNotice()} defaultExpanded />,
    );
    // frame tab 的 ASN token + section block 标题
    expect(markup).toContain("ASN");
    expect(markup).toContain("Assignment Context");
    expect(markup).toContain("2 个片段");
    expect(markup).toContain("任务分派");
  });

  it("解析并渲染 identity frame", () => {
    const notice = parseContextFrame(sampleIdentityNotice());
    expect(notice?.kind).toBe("identity");
    expect(notice?.sections[0]?.kind).toBe("identity");

    const markup = renderToStaticMarkup(
      <ContextFrameCard data={sampleIdentityNotice()} defaultExpanded />,
    );
    expect(markup).toContain("IDN");
    expect(markup).toContain("Identity");
    expect(markup).toContain("override");
  });

  it("解析并渲染 pending_action frame", () => {
    const notice = parseContextFrame(samplePendingActionNotice());
    expect(notice?.kind).toBe("pending_action");
    expect(notice?.sections[0]?.kind).toBe("pending_action");

    const markup = renderToStaticMarkup(
      <ContextFrameCard data={samplePendingActionNotice()} defaultExpanded />,
    );
    expect(markup).toContain("Pending Action");
    expect(markup).toContain("follow_up_required");
    expect(markup).toContain("ACT");
  });

  it("解析并渲染 auto_resume frame", () => {
    const notice = parseContextFrame(sampleAutoResumeNotice());

    expect(notice?.kind).toBe("auto_resume");
    expect(notice?.sections[0]?.kind).toBe("auto_resume");

    const markup = renderToStaticMarkup(
      <ContextFrameCard data={sampleAutoResumeNotice()} defaultExpanded />,
    );
    expect(markup).toContain("Auto Resume");
    expect(markup).toContain("hook_before_stop_continue");
    expect(markup).toContain("自动续跑");
    expect(markup).toContain("RES");
  });

  it("解析并渲染 compaction_summary frame", () => {
    const notice = parseContextFrame(sampleCompactionNotice());

    expect(notice?.kind).toBe("compaction_summary");
    expect(notice?.sections[0]?.kind).toBe("compaction_summary");

    const markup = renderToStaticMarkup(
      <ContextFrameCard data={sampleCompactionNotice()} defaultExpanded />,
    );
    expect(markup).toContain("Compaction Summary");
    expect(markup).toContain("12 条消息");
    expect(markup).toContain("上下文压缩");
    expect(markup).toContain("CMP");
  });

  it("解析并渲染 continuation_context frame", () => {
    const notice = parseContextFrame(sampleContinuationNotice());

    expect(notice?.kind).toBe("continuation_context");
    expect(notice?.sections[0]?.kind).toBe("continuation_context");

    const markup = renderToStaticMarkup(
      <ContextFrameCard data={sampleContinuationNotice()} defaultExpanded />,
    );
    expect(markup).toContain("CNT");
    expect(markup).toContain("Session Continuation");
    expect(markup).toContain("从会话仓储恢复 3 条历史消息");
  });
});

function sampleNotice(): Record<string, unknown> {
  return {
      id: "runtime-context-apply-1",
      kind: "capability_state_update",
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
        base_prompt: "base",
        mode: "override",
        effective_prompt: "你是 AgentDash 内置编码助手。",
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
        compacted_until_ref: { turn_id: "turn-1", entry_index: 3 },
        timestamp_ms: 1710000000000,
      },
    ],
  };
}

function sampleContinuationNotice(): Record<string, unknown> {
  return {
    id: "continuation-context-1",
    kind: "continuation_context",
    source: "runtime_context_update",
    delivery_status: "prepared_for_connector",
    delivery_channel: "connector_context",
    message_role: "system",
    rendered_text: "## Session Continuation\n\n### Transcript\n#### 用户\n继续处理",
    created_at_ms: 1,
    sections: [
      {
        kind: "continuation_context",
        title: "Session Continuation",
        summary: "从会话仓储恢复 3 条历史消息。",
        owner_context: "## Owner Context\nproject",
        transcript_markdown: "### Transcript\n#### 用户\n继续处理",
      },
    ],
  };
}

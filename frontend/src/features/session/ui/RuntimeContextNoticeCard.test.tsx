import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { parseRuntimeContextNotice } from "../model/runtimeContextNotice";
import { RuntimeContextNoticeCard } from "./RuntimeContextNoticeCard";

describe("RuntimeContextNoticeCard", () => {
  it("解析 runtime_context_notice 的结构化 sections 与 Agent 可见文本", () => {
    const notice = parseRuntimeContextNotice(sampleNotice());

    expect(notice?.phase_node).toBe("apply");
    expect(notice?.agent_visible_text).toContain("Runtime Tool Schema");
    expect(notice?.sections).toHaveLength(2);
    expect(notice?.sections[1]?.kind).toBe("tool_schema");
  });

  it("渲染 runtime_context_notice 专用卡片入口", () => {
    const markup = renderToStaticMarkup(<RuntimeContextNoticeCard data={sampleNotice()} />);

    expect(markup).toContain("STEER");
    expect(markup).toContain("Agent 行为上下文已更新");
    expect(markup).toContain("阶段 apply");
  });
});

function sampleNotice(): Record<string, unknown> {
  return {
      id: "runtime-context-apply-1",
      source: "runtime_context_update",
      phase_node: "apply",
      apply_mode: "live",
      delivery_status: "queued_for_transform_context",
      agent_visible_text: "## Runtime Tool Schema — Step Transition: apply",
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
          kind: "tool_schema",
          tools: [
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
            },
          ],
        },
      ],
    };
}

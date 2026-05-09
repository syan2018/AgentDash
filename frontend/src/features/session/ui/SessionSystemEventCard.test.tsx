import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { AcpSystemEventCard } from "./SessionSystemEventCard";
import { isRenderableSystemEventUpdate } from "./SessionSystemEventGuard";

describe("AcpSystemEventCard", () => {
  it("放行并渲染 context_frame 事件", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "context_frame",
          value: {
            id: "runtime-context-1",
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
                      properties: { key: { type: "string" } },
                    },
                    capability_key: "workflow_management",
                    source: "platform_mcp:workflow",
                    tool_path: "workflow_management::upsert_workflow_tool",
                  },
                ],
              },
            ],
          },
        },
      },
    };

    expect(isRenderableSystemEventUpdate(event)).toBe(true);

    const html = renderToStaticMarkup(<AcpSystemEventCard event={event} />);

    expect(html).toContain("CTX");
    expect(html).toContain("Agent 上下文已更新");
  });

  it("legacy context injection 不再冒充用户可见上下文卡", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "hook_trace",
        data: {
          eventType: "hook:user_prompt_submit:context_injected",
          message: "Hook 已为当前输入注入动态上下文",
          data: {
            trigger: "user_prompt_submit",
            decision: "context_injected",
            sequence: 1n,
            revision: 2n,
            severity: "info",
            tool_name: null,
            tool_call_id: null,
            subagent_type: null,
            matched_rule_keys: [],
            refresh_snapshot: false,
            block_reason: null,
            completion: null,
            diagnostic_codes: [],
            diagnostics: [],
            injections: [
              {
                slot: "workflow",
                source: "workflow:admin:plan",
                content: "Active Workflow Step: Plan\nWorkflow Guidance: 先分析再流转",
              },
            ],
          },
        },
      },
    };

    expect(isRenderableSystemEventUpdate(event)).toBe(false);
    expect(renderToStaticMarkup(<AcpSystemEventCard event={event} />)).toBe("");
  });

  it("没有 injections 的 context_injected 不再显示空壳 CTX", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "hook_trace",
        data: {
          eventType: "hook:user_prompt_submit:context_injected",
          message: "Hook 已为当前输入注入动态上下文",
          data: {
            trigger: "user_prompt_submit",
            decision: "context_injected",
            sequence: 1n,
            revision: 2n,
            severity: "info",
            tool_name: null,
            tool_call_id: null,
            subagent_type: null,
            matched_rule_keys: [],
            refresh_snapshot: false,
            block_reason: null,
            completion: null,
            diagnostic_codes: [],
            diagnostics: [],
            injections: [],
          },
        },
      },
    };

    expect(isRenderableSystemEventUpdate(event)).toBe(false);
    expect(renderToStaticMarkup(<AcpSystemEventCard event={event} />)).toBe("");
  });
});

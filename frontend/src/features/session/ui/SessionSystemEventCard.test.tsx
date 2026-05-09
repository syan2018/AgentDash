import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { AcpSystemEventCard } from "./SessionSystemEventCard";
import { isRenderableSystemEventUpdate } from "./SessionSystemEventGuard";

describe("AcpSystemEventCard", () => {
  it("放行并渲染 runtime_context_notice 事件", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "runtime_context_notice",
          value: {
            id: "runtime-context-1",
            source: "runtime_context_update",
            phase_node: "apply",
            apply_mode: "live",
            delivery_status: "queued_for_transform_context",
            agent_visible_text: "## Runtime Tool Schema — Step Transition: apply",
            created_at_ms: 1,
            sections: [
              {
                kind: "tool_schema",
                tools: [
                  {
                    name: "mcp_agentdash_workflow_tools_upsert_workflow_tool",
                    description: "创建或更新 Workflow 定义",
                    parameters_schema: {
                      type: "object",
                      properties: { key: { type: "string" } },
                    },
                    capability_key: "workflow_management",
                    source: "platform_mcp:workflow",
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

    expect(html).toContain("STEER");
    expect(html).toContain("Agent 行为上下文已更新");
  });

  it("带 injections 的 hook_trace 直接展示注入内容", () => {
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

    const html = renderToStaticMarkup(<AcpSystemEventCard event={event} />);

    expect(html).toContain("Agent 收到 1 项上下文注入");
    expect(html).toContain("workflow:admin:plan");
    expect(html).toContain("Active Workflow Step: Plan");
    expect(html).not.toContain("已注入动态上下文（1 项注入）");
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

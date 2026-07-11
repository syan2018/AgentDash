import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { JsonValue } from "../../../generated/common-contracts";
import { parseContextFrame, type ContextFrame } from "../model/contextFrame";
import { SessionSystemEventCard } from "./SessionSystemEventCard";
import { isRenderableSystemEventUpdate } from "./SessionSystemEventGuard";

type JsonObject = { [key: string]: JsonValue | undefined };

describe("SessionSystemEventCard", () => {
  it("放行并渲染 session_branch_forked 事件", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "session_branch_forked",
          value: {
            child_session_id: "sess-child",
            parent_session_id: "sess-parent",
            fork_point_event_seq: 42,
            relation_kind: "fork",
          },
        },
      },
    };

    expect(isRenderableSystemEventUpdate(event)).toBe(true);
    const markup = renderToStaticMarkup(<SessionSystemEventCard event={event} />);
    expect(markup).toContain("会话已分叉");
    expect(markup).toContain("已从父会话分叉出当前会话");
  });

  it("放行并渲染 context_frame 事件", () => {
    const frameData = sampleContextFrameData();
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "context_frame",
          value: frameData,
        },
      },
    };

    expect(isRenderableSystemEventUpdate(event)).toBe(true);

    const html = renderToStaticMarkup(
      <SessionSystemEventCard event={event} contextFrame={readFrame(frameData)} />,
    );

    expect(html).toContain("CTX");
    expect(html).toContain("TOOL SURFACE");
  });

  it("context_frame 事件没有 parsed frame 时不渲染", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "context_frame",
          value: sampleContextFrameData(),
        },
      },
    };

    expect(renderToStaticMarkup(<SessionSystemEventCard event={event} />)).toBe("");
  });

  it("有 injections 的 context_injected 应展示注入卡片", () => {
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
            effects_applied: false,
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

    expect(isRenderableSystemEventUpdate(event)).toBe(true);
    const markup = renderToStaticMarkup(<SessionSystemEventCard event={event} />);
    expect(markup).toContain("1 项上下文注入");
    expect(markup).toContain("workflow");
  });

  it("companion_human_request 不再作为 session system event 渲染", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "companion_human_request",
          value: {
            request_id: "human-1",
            prompt: "Agent 请求临时能力扩展",
            wait: true,
            payload_type: "capability_grant_request",
            ui_hint: "capability_grant_card",
            payload: {
              type: "capability_grant_request",
              requested_paths: ["workflow_management::upsert_lifecycle_tool"],
              reason: "需要更新 lifecycle 定义",
              scope: "session",
              ttl_seconds: 3600,
            },
          },
        },
      },
    };

    expect(isRenderableSystemEventUpdate(event)).toBe(false);
    const markup = renderToStaticMarkup(<SessionSystemEventCard event={event} />);
    expect(markup).toBe("");
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
            effects_applied: false,
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
    expect(renderToStaticMarkup(<SessionSystemEventCard event={event} />)).toBe("");
  });

  it("session_start 的 context_injected 在 injections 为空时隐藏", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "hook_trace",
        data: {
          eventType: "hook:session_start:context_injected",
          message: "Hook 已注入启动上下文",
          data: {
            trigger: "session_start",
            decision: "context_injected",
            sequence: 1n,
            revision: 1n,
            severity: "info",
            tool_name: null,
            tool_call_id: null,
            subagent_type: null,
            matched_rule_keys: [],
            refresh_snapshot: false,
            effects_applied: false,
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
    expect(renderToStaticMarkup(<SessionSystemEventCard event={event} />)).toBe("");
  });

});

function sampleContextFrameData(): JsonObject {
  return {
    id: "runtime-context-1",
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
        kind: "tool_schema_delta",
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
  };
}

function readFrame(value: Record<string, unknown>): ContextFrame {
  const frame = parseContextFrame(value);
  if (!frame) {
    throw new Error("invalid context frame test fixture");
  }
  return frame;
}

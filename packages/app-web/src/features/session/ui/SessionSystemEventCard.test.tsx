import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { BackboneEvent, ContextFrame as WireContextFrame } from "../../../generated/backbone-protocol";
import { parseContextFrame, type ContextFrame as UiContextFrame } from "../model/contextFrame";
import { SessionSystemEventCard } from "./SessionSystemEventCard";
import { isRenderableSystemEventUpdate } from "./SessionSystemEventGuard";

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
        kind: "context_frame_changed",
        data: {
          frame: frameData,
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
        kind: "context_frame_changed",
        data: {
          frame: sampleContextFrameData(),
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

  it("session_rewound 长 provider message 默认不展开完整 HTML", () => {
    const htmlBody = `<html><head><style>${"body{}".repeat(80)}</style></head><body>UNIQUE_REWOUND_HTML_TAIL</body></html>`;
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "session_rewound",
        data: {
          discarded_turn_id: "turn-failed",
          discarded_entry_index: 1,
          stable_event_seq: 42n,
          stable_turn_id: "turn-stable",
          reason: "runtime_failure",
          replacement_turn_id: null,
          message: `执行器运行错误: Pi Agent loop 错误: LLM 桥接错误: Codex API 返回 403 Forbidden: ${htmlBody}`,
        },
      },
    };

    expect(isRenderableSystemEventUpdate(event)).toBe(true);
    const markup = renderToStaticMarkup(<SessionSystemEventCard event={event} />);

    expect(markup).toContain("SESSION_REWOUND");
    expect(markup).toContain("Codex API 返回 403 Forbidden");
    expect(markup).toContain("丢弃轮次：turn-failed");
    expect(markup).toContain("稳定轮次：turn-stable");
    expect(markup).not.toContain("UNIQUE_REWOUND_HTML_TAIL");
  });
});

function sampleContextFrameData(): WireContextFrame {
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
    delivery_metadata: {
      delivery_phase: "discovered_inventory",
      delivery_order: 50,
      cache_policy: "discovery_digest",
      cache_key: null,
      cache_revision: "surface-1",
      model_channel: "context",
      agent_consumption: {
        target: "dash-agent",
        mode: "connector_native",
        reason: "dash_materialized_tool_registry",
      },
      frontend_label: "Capability State Delta",
      connector_profile: {
        profile_id: "dash-agent",
        declared_consumption_modes: ["connector_native"],
      },
    },
    created_at_ms: 1n,
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

function readFrame(value: unknown): UiContextFrame {
  const frame = parseContextFrame(value);
  if (!frame) {
    throw new Error("invalid context frame test fixture");
  }
  return frame;
}

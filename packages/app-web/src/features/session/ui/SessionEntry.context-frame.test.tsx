import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { AggregatedContextFrameGroup, SessionDisplayEntry } from "../model/types";
import type { ContextFrame } from "../model/contextFrame";
import { SessionEntry } from "./SessionEntry";

describe("SessionEntry ContextFrame 聚合", () => {
  it("把多个 context_frame 渲染为一张批量更新卡片", () => {
    const group: AggregatedContextFrameGroup = {
      type: "aggregated_context_frames",
      id: "ctx-1",
      groupKey: "context-frame-ctx-1",
      entries: [
        contextFrameEntry("ctx-1", "capability_state_delta"),
        contextFrameEntry("ctx-2", "assignment_context"),
      ],
    };

    const html = renderToStaticMarkup(<SessionEntry item={group} />);

    expect(html).toContain("CAPABILITY");
    expect(html).toContain("ASSIGNMENT");
    expect(html).toContain("2x");
    expect(html).toContain("apply");
    expect(html).not.toContain("已注入动态上下文");
  });
});

describe("SessionEntry 工具聚合", () => {
  it("raw dynamicToolCall entry 也通过 TOOLS group 渲染", () => {
    const entry: SessionDisplayEntry = {
      id: "item:turn_001:tool_001",
      sessionId: "session-1",
      timestamp: 1,
      eventSeq: 1,
      turnId: "turn-1",
      event: {
        type: "item_completed",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          completedAtMs: 1,
          item: {
            type: "dynamicToolCall",
            id: "turn_001:tool_001",
            tool: "companion_respond",
            arguments: {},
            status: "completed",
            contentItems: null,
            durationMs: null,
            success: true,
            namespace: null,
          },
        },
      },
    };

    const html = renderToStaticMarkup(<SessionEntry item={entry} followedByMessage />);

    expect(html).toContain("TOOLS");
    expect(html).toContain("调用 1 个");
    expect(html).not.toContain("companion_respond");
  });
});

describe("SessionEntry 错误事件", () => {
  it("把 PiAgent fatal error 渲染为独立错误块", () => {
    const entry: SessionDisplayEntry = {
      id: "error-1",
      sessionId: "session-1",
      timestamp: 1,
      eventSeq: 1,
      turnId: "turn-1",
      event: {
        type: "error",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          willRetry: false,
          error: {
            message: "刷新 Codex token 返回 401 Unauthorized",
            codexErrorInfo: "unauthorized",
            additionalDetails: "code=refresh_token_reused",
          },
        },
      },
    };

    const html = renderToStaticMarkup(<SessionEntry item={entry} />);

    expect(html).toContain("ERROR");
    expect(html).toContain("执行失败");
    expect(html).toContain("unauthorized");
    expect(html).toContain("刷新 Codex token 返回 401 Unauthorized");
    expect(html).toContain("code=refresh_token_reused");
    expect(html).toContain("turn turn-1");
  });

  it("provider HTML 错误默认只展示摘要和结构化详情", () => {
    const htmlBody = `<html><head><style>${"body{}".repeat(80)}</style></head><body>UNIQUE_PROVIDER_HTML_TAIL</body></html>`;
    const entry: SessionDisplayEntry = {
      id: "error-provider-1",
      sessionId: "session-1",
      timestamp: 1,
      eventSeq: 1,
      turnId: "turn-1",
      event: {
        type: "error",
        payload: {
          threadId: "thread-1",
          turnId: "turn-1",
          willRetry: false,
          error: {
            message: `Codex API 返回 403 Forbidden: ${htmlBody}`,
            codexErrorInfo: null,
            additionalDetails: [
              "kind=Provider",
              "code=auth_error",
              "http_status=403",
              `body=${htmlBody}`,
            ].join("\n"),
          },
        },
      },
    };

    const html = renderToStaticMarkup(<SessionEntry item={entry} />);

    expect(html).toContain("Codex API 返回 403 Forbidden");
    expect(html).toContain("kind=Provider");
    expect(html).toContain("code=auth_error");
    expect(html).toContain("http_status=403");
    expect(html).toContain("完整错误响应");
    expect(html).toContain("完整详情");
    expect(html).not.toContain("UNIQUE_PROVIDER_HTML_TAIL");
  });
});

function contextFrameEntry(
  id: string,
  kind: "capability_state_delta" | "assignment_context",
): AggregatedContextFrameGroup["entries"][number] {
  return {
    id,
    sessionId: "session-1",
    timestamp: 1,
    eventSeq: id === "ctx-1" ? 1 : 2,
    event: {
      type: "platform",
      payload: {
        kind: "context_frame_changed",
        data: {
          frame: {
            id,
            kind,
            source: "runtime_context_update",
            phase_node: "apply",
            apply_mode: "live",
            delivery_status: "queued_for_transform_context",
            delivery_channel: "turn_start",
            message_role: "user",
            delivery_metadata: {
              delivery_phase: "discovered_inventory",
              delivery_order: 50,
              cache_policy: "discovery_digest",
              cache_key: null,
              cache_revision: "surface-1",
              model_channel: "context",
              agent_consumption: {
                target: "dash-agent",
                mode: "consume",
                reason: "test",
              },
              frontend_label: "Capability State Delta",
              connector_profile: {
                profile_id: "dash-agent",
                declared_consumption_modes: ["consume"],
              },
            },
            rendered_text: "## Capability Update",
            created_at_ms: 1n,
            sections: [
              {
                kind: "capability_key_delta",
                added_capabilities: [],
                removed_capabilities: [],
                effective_capabilities: ["workflow_management"],
              },
            ],
          },
        },
      },
    },
    contextFrame: contextFrame(id, kind),
  };
}

function contextFrame(id: string, kind: string): ContextFrame {
  return {
    id,
    kind,
    source: "runtime_context_update",
    phase_node: "apply",
    apply_mode: "live",
    delivery_status: "queued_for_transform_context",
    delivery_channel: "turn_start",
    message_role: "user",
    delivery_metadata: {
      delivery_phase: "discovered_inventory",
      delivery_order: 50,
      cache_policy: "discovery_digest",
      model_channel: "context",
      agent_consumption: {
        target: "",
        mode: "consume",
        reason: "test",
      },
      frontend_label: "Capability State Delta",
      connector_profile: {
        profile_id: "",
        declared_consumption_modes: [],
      },
    },
    rendered_text: "## Capability Update",
    created_at_ms: 1,
    sections: [
      {
        kind: "capability_key_delta",
        added_capabilities: [],
        removed_capabilities: [],
        effective_capabilities: ["workflow_management"],
      },
    ],
  };
}

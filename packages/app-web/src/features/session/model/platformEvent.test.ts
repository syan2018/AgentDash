import { describe, expect, it } from "vitest";
import type { BackboneEvent } from "../../../generated/backbone-protocol";
import {
  extractPlatformEventData,
  extractPlatformEventMessage,
  extractPlatformEventType,
} from "./platformEvent";
import { getPlatformEventPolicy } from "./systemEventPolicy";

describe("platformEvent", () => {
  it("识别一等 provider attempt status 事件", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "provider_attempt_status",
        data: {
          turn_id: "turn-1",
          phase: "retry_scheduled",
          attempt: 1,
          max_attempts: 3,
          will_retry: true,
          delay_ms: 2_000n,
          reason_code: "provider_5xx",
          message: "Reconnecting... 2/3",
          provider: "openai",
          model: "gpt-5",
        },
      },
    };

    expect(extractPlatformEventType(event)).toBe("provider_attempt_status");
    expect(extractPlatformEventData(event)?.phase).toBe("retry_scheduled");
    expect(extractPlatformEventMessage(event)).toBe("Reconnecting... 2/3");
    expect(getPlatformEventPolicy(event).isRenderableSystemEvent).toBe(false);
    expect(getPlatformEventPolicy(event).feedBoundary).toBe("neutral");
    expect(getPlatformEventPolicy(event, { includeVerboseEvents: true }).isRenderableSystemEvent).toBe(true);
    expect(getPlatformEventPolicy(event, { includeVerboseEvents: true }).feedBoundary).toBe("hard");
  });

  it("识别一等 session rewound 事件", () => {
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
          message: "已丢弃失败轮次，恢复到上一稳定状态",
        },
      },
    };

    expect(extractPlatformEventType(event)).toBe("session_rewound");
    expect(extractPlatformEventData(event)?.discarded_turn_id).toBe("turn-failed");
    expect(extractPlatformEventMessage(event)).toBe("已丢弃失败轮次，恢复到上一稳定状态");
    expect(getPlatformEventPolicy(event).isRenderableSystemEvent).toBe(true);
    expect(getPlatformEventPolicy(event).feedBoundary).toBe("hard");
  });

  it("将 Workspace Module 展示投影识别为可渲染的成功事件", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "control_plane_projection_changed",
        data: {
          projection: "resource_surface",
          reason: "capability_state_changed",
          run_id: "run-1",
          agent_id: "agent-1",
          frame_id: "frame-1",
          gate_id: null,
          mailbox_message_id: null,
          delivery_runtime_session_id: "runtime-1",
          workspace_module_presentation: {
            module_id: "canvas:cvs-canvas",
            view_key: "preview",
            renderer_kind: "canvas",
            presentation_uri: "canvas://cvs-canvas",
            title: "临时 Canvas 展示测试",
            payload: { reason: "smoke-test" },
            diagnostics: null,
          },
        },
      },
    };

    expect(extractPlatformEventType(event)).toBe("workspace_module_presented");
    expect(extractPlatformEventMessage(event)).toBe("已展示「临时 Canvas 展示测试」");
    expect(getPlatformEventPolicy(event).isRenderableSystemEvent).toBe(true);
    expect(getPlatformEventPolicy(event).feedBoundary).toBe("hard");
  });
});

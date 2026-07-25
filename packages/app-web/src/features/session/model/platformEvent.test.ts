import { describe, expect, it } from "vitest";
import type { BackboneEvent } from "../../../generated/backbone-protocol";
import {
  extractPlatformEventData,
  extractPlatformEventMessage,
  extractPlatformEventType,
} from "./platformEvent";
import { getPlatformEventPolicy } from "./systemEventPolicy";
import { makeDisplayEntry } from "./sessionStreamReducer";

describe("platformEvent", () => {
  it("把 canonical context_frame_changed 作为 ContextFrame 展示边界", () => {
    const event: BackboneEvent = {
      type: "platform",
      payload: {
        kind: "context_frame_changed",
        data: {
          frame: {
            id: "surface:1:identity",
            kind: "identity",
            source: "runtime_context_update",
            phase_node: null,
            apply_mode: "surface_apply",
            delivery_status: "applied_before_prompt",
            delivery_channel: "connector_context",
            message_role: "system",
            delivery_metadata: {
              delivery_phase: "stable_system",
              delivery_order: 10,
              cache_policy: "static",
              cache_key: null,
              cache_revision: "surface-1",
              model_channel: "system",
              agent_consumption: {
                target: "dash-agent",
                mode: "system_append",
                reason: "materialized_surface",
              },
              frontend_label: "Identity",
              connector_profile: {
                profile_id: "dash-agent",
                declared_consumption_modes: ["system_append"],
              },
            },
            rendered_text: "You are the project agent.",
            sections: [],
            created_at_ms: 0n,
          },
        },
      },
    };

    expect(extractPlatformEventType(event)).toBe("context_frame");
    expect(extractPlatformEventData(event)?.id).toBe("surface:1:identity");
    expect(getPlatformEventPolicy(event).isRenderableSystemEvent).toBe(true);
    expect(getPlatformEventPolicy(event).feedBoundary).toBe("hard");
    expect(makeDisplayEntry({
      session_id: "session-1",
      event_seq: 1,
      occurred_at_ms: 0,
      committed_at_ms: 0,
      session_update_type: "platform",
      turn_id: null,
      entry_index: 1,
      tool_call_id: null,
      notification: {
        event,
        sessionId: "session-1",
        source: {
          connectorId: "dash-agent",
          connectorType: "native",
          executorId: null,
        },
        trace: { turnId: null, entryIndex: 1 },
        observedAt: "1970-01-01T00:00:00Z",
      },
      presentation_id: "native:surface:1",
      runtime_change_sequence: null,
      baseline: true,
    }, event).contextFrame?.id).toBe("surface:1:identity");
  });

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
});

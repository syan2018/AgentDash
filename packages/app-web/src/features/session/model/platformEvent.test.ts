import { describe, expect, it } from "vitest";
import type { SessionPresentationEvent } from "./types";
import {
  extractPlatformEventData,
  extractPlatformEventMessage,
  extractPlatformEventType,
} from "./platformEvent";
import { getPlatformEventPolicy } from "./systemEventPolicy";

describe("platformEvent", () => {
  it("识别一等 provider attempt status 事件", () => {
    const event: SessionPresentationEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "runtime_provider_status",
          value: { turn_id: "turn-1", phase: "retry_scheduled", message: "Reconnecting... 2/3" },
        },
      },
    };

    expect(extractPlatformEventType(event)).toBe("runtime_provider_status");
    expect(extractPlatformEventData(event)?.phase).toBe("retry_scheduled");
    expect(extractPlatformEventMessage(event)).toBe("Reconnecting... 2/3");
    expect(getPlatformEventPolicy(event).isRenderableSystemEvent).toBe(false);
    expect(getPlatformEventPolicy(event).feedBoundary).toBe("neutral");
    expect(getPlatformEventPolicy(event, { includeVerboseEvents: true }).isRenderableSystemEvent).toBe(true);
    expect(getPlatformEventPolicy(event, { includeVerboseEvents: true }).feedBoundary).toBe("hard");
  });

  it("识别一等 session rewound 事件", () => {
    const event: SessionPresentationEvent = {
      type: "platform",
      payload: {
        kind: "session_meta_update",
        data: {
          key: "session_rewound",
          value: { discarded_turn_id: "turn-failed", message: "已丢弃失败轮次，恢复到上一稳定状态" },
        },
      },
    };

    expect(extractPlatformEventType(event)).toBe("session_rewound");
    expect(extractPlatformEventData(event)?.discarded_turn_id).toBe("turn-failed");
    expect(extractPlatformEventMessage(event)).toBe("已丢弃失败轮次，恢复到上一稳定状态");
    expect(getPlatformEventPolicy(event).isRenderableSystemEvent).toBe(false);
    expect(getPlatformEventPolicy(event).feedBoundary).toBe("neutral");
  });
});

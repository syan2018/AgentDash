import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../../generated/common-contracts";
import type { BackboneEvent, Turn } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "../model/types";
import { computeProjectionRefreshKey, extractTurnLifecycleEventType } from "./SessionChatViewModel";
import {
  collectAllPlatformEvents,
  collectRenderableSystemEvents,
  collectTurnLifecycleEvents,
} from "./SessionChatViewModel";
import {
  isSessionComposerSubmitDisabled,
  isSessionModelRequirementSatisfied,
} from "./SessionChatComposerState";

const completedTurn: Turn = {
  id: "turn-1",
  items: [],
  itemsView: "full",
  status: "completed",
  error: null,
  startedAt: null,
  completedAt: null,
  durationMs: null,
};

function eventEnvelope(eventSeq: number, event: BackboneEvent): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: eventSeq,
    session_update_type: event.type,
    notification: {
      event,
      sessionId: "session-1",
      source: {
        connectorId: "test",
        connectorType: "unit",
        executorId: null,
      },
      trace: {
        turnId: null,
        entryIndex: null,
      },
      observedAt: "2026-05-26T00:00:00.000Z",
    },
  };
}

function agentDeltaEvent(itemId: string): BackboneEvent {
  return {
    type: "agent_message_delta",
    payload: {
      threadId: "thread-1",
      turnId: "turn-1",
      itemId,
      delta: "delta",
    },
  };
}

function platformMetaEvent(key: string, value: Record<string, JsonValue>): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: { key, value },
    },
  };
}

function turnTerminalMetaEvent(terminalType: "turn_completed" | "turn_failed" | "turn_interrupted"): BackboneEvent {
  return platformMetaEvent("turn_terminal", {
    terminal_type: terminalType,
    message: null,
  });
}

describe("computeProjectionRefreshKey", () => {
  it("普通 delta event 不推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, {
        type: "turn_completed",
        payload: { threadId: "thread-1", turn: completedTurn },
      }),
      eventEnvelope(2, agentDeltaEvent("assistant-1")),
      eventEnvelope(3, agentDeltaEvent("assistant-1")),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(1);
  });

  it("外部 executor_context_compacted 不推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(2, {
        type: "executor_context_compacted",
        payload: { threadId: "thread-1", turnId: "turn-1" },
      }),
      eventEnvelope(3, agentDeltaEvent("assistant-2")),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(0);
  });

  it("compaction_summary context_frame 会推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(3, platformMetaEvent("context_frame", {
        kind: "compaction_summary",
        id: "frame-1",
      })),
      eventEnvelope(4, agentDeltaEvent("assistant-2")),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(3);
  });

  it("platform context_compacted meta event 会推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(2, platformMetaEvent("context_compacted", {
        summary: "历史摘要",
        messages_compacted: 2,
      })),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(2);
  });

  it("platform turn_terminal meta event 会推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(7, turnTerminalMetaEvent("turn_completed")),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(7);
  });

  it("session_rewound meta event 会推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(8, platformMetaEvent("session_rewound", {
        discarded_turn_id: "turn-1",
        stable_event_seq: 0,
        reason: "provider_retry",
      })),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(8);
  });
});

describe("extractTurnLifecycleEventType", () => {
  it("从 turn_terminal platform 事件提取终态类型", () => {
    expect(extractTurnLifecycleEventType(turnTerminalMetaEvent("turn_completed"))).toBe("turn_completed");
    expect(extractTurnLifecycleEventType(turnTerminalMetaEvent("turn_failed"))).toBe("turn_failed");
    expect(extractTurnLifecycleEventType(turnTerminalMetaEvent("turn_interrupted"))).toBe("turn_interrupted");
  });
});

describe("collectRenderableSystemEvents", () => {
  it("只收集可渲染 system event，同时推进 lastSeenSeq", () => {
    const events = [
      eventEnvelope(1, platformMetaEvent("system_message", { message: "需要用户确认" })),
      eventEnvelope(2, {
        type: "platform",
        payload: {
          kind: "hook_trace",
          data: {
            eventType: "hook:before_provider_request:observed",
            message: "Hook 已观测到 LLM API 请求即将发出",
            data: {
              trigger: "before_provider_request",
              decision: "observed",
              sequence: 1n,
              revision: 1n,
              severity: "info",
              tool_name: null,
              tool_call_id: null,
              subagent_type: null,
              matched_rule_keys: [],
              refresh_snapshot: false,
              block_reason: null,
              completion: null,
              diagnostic_codes: ["session_binding_found"],
              diagnostics: [{ code: "session_binding_found", message: "命中运行时绑定" }],
              injections: [],
            },
          },
        },
      }),
      eventEnvelope(3, agentDeltaEvent("assistant-1")),
    ];

    const result = collectRenderableSystemEvents(events, 0);

    expect(result.lastSeenSeq).toBe(3);
    expect(result.items.map((item) => item.eventSeq)).toEqual([1]);
    expect(result.items[0]?.eventType).toBe("system_message");
  });

  it("全量 platform 收集函数保留不可渲染事件入口", () => {
    const events = [
      eventEnvelope(1, platformMetaEvent("system_message", { message: "需要用户确认" })),
      eventEnvelope(2, platformMetaEvent("unknown_meta", { message: "静默" })),
      eventEnvelope(3, agentDeltaEvent("assistant-1")),
    ];

    const result = collectAllPlatformEvents(events, 0);

    expect(result.lastSeenSeq).toBe(3);
    expect(result.items.map((item) => item.eventType)).toEqual([
      "system_message",
      "unknown_meta",
    ]);
  });

  it("全量 platform 收集函数可用历史边界跳过 hydrate 事件", () => {
    const events = [
      eventEnvelope(9, platformMetaEvent("workspace_module_presented", {
        module_id: "canvas:history",
        view_key: "preview",
        renderer_kind: "canvas",
        presentation_uri: "canvas://history",
        title: "History Canvas",
      })),
      eventEnvelope(10, platformMetaEvent("session_meta_updated", { title: "历史标题" })),
      eventEnvelope(11, platformMetaEvent("workspace_module_presented", {
        module_id: "canvas:live",
        view_key: "preview",
        renderer_kind: "canvas",
        presentation_uri: "canvas://live",
        title: "Live Canvas",
      })),
      eventEnvelope(12, platformMetaEvent("session_meta_updated", { title: "新标题" })),
    ];

    const result = collectAllPlatformEvents(events, 10);

    expect(result.lastSeenSeq).toBe(12);
    expect(result.items.map((item) => item.eventType)).toEqual([
      "workspace_module_presented",
      "session_meta_updated",
    ]);
    expect(result.items.map((item) => item.eventSeq)).toEqual([11, 12]);
  });
});

describe("collectTurnLifecycleEvents", () => {
  it("按边界只收集 live turn lifecycle 并推进 lastSeenSeq", () => {
    const events = [
      eventEnvelope(8, {
        type: "turn_completed",
        payload: { threadId: "thread-1", turn: completedTurn },
      }),
      eventEnvelope(10, turnTerminalMetaEvent("turn_completed")),
      eventEnvelope(11, {
        type: "turn_started",
        payload: {
          threadId: "thread-1",
          turn: { ...completedTurn, id: "turn-2", status: "inProgress" },
        },
      }),
      eventEnvelope(12, turnTerminalMetaEvent("turn_failed")),
    ];

    const result = collectTurnLifecycleEvents(events, 10);

    expect(result.lastSeenSeq).toBe(12);
    expect(result.items.map((item) => item.eventSeq)).toEqual([11, 12]);
    expect(result.items.map((item) => item.eventType)).toEqual([
      "turn_started",
      "turn_failed",
    ]);
  });
});

describe("isSessionComposerSubmitDisabled", () => {
  it("command 不可用时即使有输入也不可提交", () => {
    expect(isSessionComposerSubmitDisabled({
      commandEnabled: false,
      requirePromptText: true,
      inputValue: "hello",
      isCancelling: false,
      isSending: false,
    })).toBe(true);
  });

  it("command 可用但需要输入时空文本不可提交", () => {
    expect(isSessionComposerSubmitDisabled({
      commandEnabled: true,
      requirePromptText: true,
      inputValue: "",
      isCancelling: false,
      isSending: false,
    })).toBe(true);
  });

  it("command 可用且有输入时允许提交", () => {
    expect(isSessionComposerSubmitDisabled({
      commandEnabled: true,
      requirePromptText: true,
      inputValue: "hello",
      isCancelling: false,
      isSending: false,
    })).toBe(false);
  });
});

describe("isSessionModelRequirementSatisfied", () => {
  it("keeps model_required blocked without a complete explicit override", () => {
    expect(isSessionModelRequirementSatisfied("model_required", {
      executor: "PI_AGENT",
      provider_id: "openai",
    })).toBe(false);
  });

  it("allows model_required to be satisfied by explicit provider and model selection", () => {
    expect(isSessionModelRequirementSatisfied("model_required", {
      executor: "PI_AGENT",
      provider_id: "openai",
      model_id: "gpt-5.4-mini",
    })).toBe(true);
  });

  it("allows model_required when the selected model has reasoning even if thinking level is unset", () => {
    expect(isSessionModelRequirementSatisfied("model_required", {
      executor: "PI_AGENT",
      provider_id: "openai",
      model_id: "reasoning-model",
      thinking_level: undefined,
    })).toBe(true);
  });
});

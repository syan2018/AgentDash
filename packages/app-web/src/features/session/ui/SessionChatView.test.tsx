import { describe, expect, it, vi } from "vitest";
import type { JsonValue } from "../../../generated/common-contracts";
import type { BackboneEvent, Turn } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "../model/types";
import { historyReplayBoundaryAfterCompletedLoad } from "../model/useSessionStream";
import {
  computeProjectionRefreshKey,
  dispatchLiveSessionEvents,
  extractTurnLifecycleEventType,
  isAgentRunWorkspaceActionRunning,
  rawEventsBelongToRuntimeStreamTarget,
} from "./SessionChatViewModel";
import { collectRenderableSystemEvents } from "./SessionChatViewModel";
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

describe("history replay boundary lifecycle", () => {
  it("establishes the boundary on the first completed load after a StrictMode setup was cancelled", () => {
    expect(historyReplayBoundaryAfterCompletedLoad(null, 116)).toBe(116);
  });

  it("preserves the original hydration boundary when the same target reconnects", () => {
    expect(historyReplayBoundaryAfterCompletedLoad(57, 116)).toBe(57);
  });
});

function eventEnvelope(eventSeq: number, event: BackboneEvent, sessionId = "session-1"): SessionEventEnvelope {
  return {
    session_id: sessionId,
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: eventSeq,
    session_update_type: event.type,
    notification: {
      event,
      sessionId,
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

function workspaceModulePresentationRequestedEvent(): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "workspace_module_presentation_requested",
      data: {
        module_id: "canvas:cvs-canvas",
        view_key: "preview",
        renderer_kind: "canvas",
        presentation_uri: "canvas://cvs-canvas",
        title: "Canvas",
        payload: null,
        diagnostics: null,
      },
    },
  };
}

function turnTerminalMetaEvent(terminalType: "turn_completed" | "turn_failed" | "turn_interrupted"): BackboneEvent {
  return platformMetaEvent("turn_terminal", {
    terminal_type: terminalType,
    message: null,
  });
}

function contextFrameChangedEvent(): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "context_frame_changed",
      data: {
        frame: {
          id: "surface-frame-2",
          kind: "capability_state_delta",
          source: "runtime_context_update",
          delivery_status: "accepted",
          delivery_channel: "continuation",
          message_role: "context",
          delivery_metadata: {
            delivery_phase: "run_state",
            delivery_order: 0,
            cache_policy: "runtime_state_digest",
            model_channel: "audit_only",
            agent_consumption: {
              target: "runtime",
              mode: "audit_only",
              reason: "surface adopted",
            },
            frontend_label: "VFS UPDATE",
            connector_profile: {
              profile_id: "managed-runtime",
            },
          },
          rendered_text: "Canvas mount added",
          sections: [{
            kind: "vfs_delta",
            vfs_mounts_added: ["cvs-canvas"],
            vfs_mounts_removed: [],
          }],
          created_at_ms: 1n,
        },
      },
    },
  };
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

describe("rawEventsBelongToRuntimeStreamTarget", () => {
  it("matches AgentRun events by synthetic stream key when raw session id is absent", () => {
    const syntheticSessionId = "agentrun:run-1:agent-1";
    const events = [eventEnvelope(1, turnTerminalMetaEvent("turn_completed"), syntheticSessionId)];

    expect(rawEventsBelongToRuntimeStreamTarget({
      rawEvents: events,
      agentRunTarget: { runId: "run-1", agentId: "agent-1" },
    })).toBe(true);
  });

  it("matches AgentRun synthetic stream key as the only chat stream identity", () => {
    const syntheticSessionId = "agentrun:run-1:agent-1";
    const events = [eventEnvelope(1, turnTerminalMetaEvent("turn_completed"), syntheticSessionId)];

    expect(rawEventsBelongToRuntimeStreamTarget({
      rawEvents: events,
      agentRunTarget: { runId: "run-1", agentId: "agent-1" },
    })).toBe(true);
  });

  it("rejects stale raw RuntimeSession events on AgentRun scoped chat", () => {
    const events = [eventEnvelope(1, turnTerminalMetaEvent("turn_completed"), "runtime-session-1")];

    expect(rawEventsBelongToRuntimeStreamTarget({
      rawEvents: events,
      agentRunTarget: { runId: "run-1", agentId: "agent-1" },
    })).toBe(false);
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
              effects_applied: false,
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

  it("将 Workspace Module 展示请求作为审计事实收进可渲染会话流", () => {
    const result = collectRenderableSystemEvents([
      eventEnvelope(4, workspaceModulePresentationRequestedEvent()),
    ], 0);

    expect(result.items).toHaveLength(1);
    expect(result.items[0]?.eventType).toBe("workspace_module_presentation_requested");
    expect(result.items[0]?.eventSeq).toBe(4);
  });

  it("history hydration 只恢复事实，不执行命令式页面动作", () => {
    const onLiveEvent = vi.fn();

    const lastSeenSeq = dispatchLiveSessionEvents(
      [
        eventEnvelope(93, platformMetaEvent("system_message", {
          message: "hydrated message",
        })),
        eventEnvelope(94, workspaceModulePresentationRequestedEvent()),
        eventEnvelope(97, {
          type: "item_completed",
          payload: {
            threadId: "thread-1",
            turnId: "turn-1",
            item: {
              type: "dynamicToolCall",
              id: "tool-1",
              tool: "workspace_module_present",
              status: "completed",
              success: true,
              arguments: {},
              namespace: null,
              durationMs: null,
              contentItems: null,
            },
            completedAtMs: 97,
          },
        }),
      ],
      null,
      97,
      onLiveEvent,
    );

    expect(lastSeenSeq).toBe(97);
    expect(onLiveEvent).not.toHaveBeenCalled();
  });

  it("在同一入口按 sequence 分发所有 live 事件", () => {
    const onLiveEvent = vi.fn();
    const presentation = workspaceModulePresentationRequestedEvent();
    const contextFrame = contextFrameChangedEvent();

    const lastSeenSeq = dispatchLiveSessionEvents(
      [
        eventEnvelope(51, presentation),
        eventEnvelope(52, contextFrame),
      ],
      50,
      50,
      onLiveEvent,
    );

    expect(lastSeenSeq).toBe(52);
    expect(onLiveEvent.mock.calls.map(([event]) => event)).toEqual([
      presentation,
      contextFrame,
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

describe("isAgentRunWorkspaceActionRunning", () => {
  it("uses AgentRun execution projection without requiring a runtime trace session id", () => {
    expect(isAgentRunWorkspaceActionRunning({
      executionStatus: "running_active",
    })).toBe(true);
    expect(isAgentRunWorkspaceActionRunning({
      executionStatus: "ready",
    })).toBe(false);
    expect(isAgentRunWorkspaceActionRunning({
      executionStatus: "cancelling",
    })).toBe(true);
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

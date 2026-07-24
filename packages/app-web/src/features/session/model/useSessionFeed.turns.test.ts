import { describe, expect, it } from "vitest";

import type { SessionDisplayEntry, SessionEventEnvelope } from "./types";
import { createInitialStreamState, reduceStreamState } from "./sessionStreamReducer";
import { mergeThinkingIntoDisplayItems, segmentByTurn } from "./useSessionFeed";

function failedTurnEvent(): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: 1,
    occurred_at_ms: 0,
    committed_at_ms: 0,
    session_update_type: "turn_completed",
    turn_id: "turn-1",
    entry_index: 3,
    tool_call_id: null,
    notification: {
      event: {
        type: "turn_completed",
        payload: {
          threadId: "session-1",
          turn: {
            id: "turn-1",
            items: [],
            itemsView: "full",
            status: "failed",
            error: {
              message: "provider rejected reasoning effort",
              additionalDetails: "code=unsupported_value; retryable=false",
            },
          },
        },
      },
      sessionId: "session-1",
      source: {
        connectorId: "dash-agent",
        connectorType: "native",
        executorId: null,
      },
      trace: { turnId: "turn-1", entryIndex: 3 },
      observedAt: "1970-01-01T00:00:00.000Z",
    },
    presentation_id: "turn-1:terminal",
    runtime_change_sequence: null,
    baseline: true,
  };
}

function providerStatusEvent(
  phase: "connected_waiting_first_delta" | "succeeded",
  eventSeq: number,
): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: null,
    session_update_type: "platform",
    turn_id: "turn-1",
    entry_index: null,
    tool_call_id: null,
    notification: {
      event: {
        type: "platform",
        payload: {
          kind: "provider_attempt_status",
          data: {
            turn_id: "turn-1",
            phase,
            attempt: 1,
            max_attempts: 1,
            will_retry: false,
            delay_ms: null,
            reason_code: null,
            message: null,
            provider: null,
            model: null,
          },
        },
      },
      sessionId: "session-1",
      source: {
        connectorId: "dash-agent",
        connectorType: "native",
        executorId: null,
      },
      trace: { turnId: "turn-1", entryIndex: null },
      observedAt: "1970-01-01T00:00:00.000Z",
    },
    ephemeral: true,
    presentation_id: `live:${eventSeq}`,
    runtime_change_sequence: null,
    baseline: false,
  };
}

function completedMessage(id: string, eventSeq: number): SessionDisplayEntry {
  return {
    id,
    sessionId: "session-1",
    timestamp: eventSeq,
    eventSeq,
    turnId: "turn-1",
    itemFreshness: "completed",
    isStreaming: false,
    event: {
      type: "item_completed",
      payload: {
        threadId: "session-1",
        turnId: "turn-1",
        completedAtMs: eventSeq,
        item: {
          type: "agentMessage",
          id,
          text: id,
        },
      },
    },
  };
}

function contextFrameEntry(): SessionDisplayEntry {
  return {
    id: "context-frame",
    sessionId: "session-1",
    timestamp: 2,
    eventSeq: 2,
    turnId: undefined,
    event: {
      type: "platform",
      payload: {
        kind: "context_frame_changed",
        data: {
          frame: { id: "surface:2:capability-state-delta" },
        },
      },
    },
  } as unknown as SessionDisplayEntry;
}

describe("session turn segmentation", () => {
  it("keeps a failed terminal-only turn and its authoritative error", () => {
    const segments = segmentByTurn([], [failedTurnEvent()], null);

    expect(segments).toEqual([
      expect.objectContaining({
        turnId: "turn-1",
        status: "failed",
        errorMessage: "provider rejected reasoning effort",
        items: [],
      }),
    ]);
  });

  it("keeps a durable completed agent message as the collapsed turn output", () => {
    const agentMessage: SessionDisplayEntry = {
      id: "item:assistant-1",
      sessionId: "session-1",
      timestamp: 2,
      eventSeq: 2,
      turnId: "turn-1",
      itemFreshness: "completed",
      isStreaming: false,
      event: {
        type: "item_completed",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          completedAtMs: 2,
          item: {
            type: "agentMessage",
            id: "assistant-1",
            text: "最终回答",
          },
        },
      },
    };

    const [segment] = segmentByTurn([agentMessage], [], null);

    expect(segment?.finalOutput).toBe(agentMessage);
  });

  it("keeps durable turn timing for the execution-time presentation", () => {
    const event = failedTurnEvent();
    if (event.notification.event.type !== "turn_completed") {
      throw new Error("expected terminal event");
    }
    event.notification.event.payload.turn.startedAt = 1_000;
    event.notification.event.payload.turn.completedAt = 1_003;
    event.notification.event.payload.turn.durationMs = 2_500;

    const [segment] = segmentByTurn([], [event], null);

    expect(segment).toEqual(expect.objectContaining({
      startedAtMs: 1_000_000,
      durationMs: 2_500,
    }));
  });

  it("keeps an unscoped ContextFrame inside one completed canonical turn", () => {
    const terminal = failedTurnEvent();
    if (terminal.notification.event.type !== "turn_completed") {
      throw new Error("expected terminal event");
    }
    terminal.notification.event.payload.turn.status = "completed";
    terminal.notification.event.payload.turn.error = null;
    terminal.notification.event.payload.turn.durationMs = 2_500;

    const segments = segmentByTurn(
      [
        completedMessage("before-context", 1),
        contextFrameEntry(),
        completedMessage("after-context", 3),
      ],
      [terminal],
      null,
    );

    expect(segments).toEqual([
      expect.objectContaining({
        turnId: "turn-1",
        status: "completed",
        durationMs: 2_500,
        items: expect.arrayContaining([
          expect.objectContaining({ id: "context-frame" }),
        ]),
      }),
    ]);
  });
});

describe("provider waiting thinking state", () => {
  it("shows thinking before the first delta and clears it when the provider round completes", () => {
    const waitingState = reduceStreamState(
      createInitialStreamState([]),
      [providerStatusEvent("connected_waiting_first_delta", 1)],
    );

    expect(waitingState.providerWaitingSeqs.get("turn-1")).toBe(1);
    expect(mergeThinkingIntoDisplayItems([], waitingState.providerWaitingSeqs)).toEqual([
      expect.objectContaining({
        type: "aggregated_thinking",
        turnId: "turn-1",
        entries: [],
        isStreamingThinking: true,
      }),
    ]);

    const completedState = reduceStreamState(
      waitingState,
      [providerStatusEvent("succeeded", 2)],
    );

    expect(completedState.providerWaitingSeqs.has("turn-1")).toBe(false);
    expect(mergeThinkingIntoDisplayItems([], completedState.providerWaitingSeqs)).toEqual([]);
  });
});

import { describe, expect, it } from "vitest";
import type { BackboneEnvelope, BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "./types";
import {
  createInitialStreamState,
  reduceStreamState,
  shouldFlushStreamEventImmediately,
} from "./sessionStreamReducer";

function envelope(event: BackboneEvent): BackboneEnvelope {
  return {
    sessionId: "s1",
    source: {
      connectorId: "connector",
      connectorType: "test",
      executorId: null,
    },
    trace: {
      turnId: "turn-1",
      entryIndex: 0,
    },
    observedAt: "2026-06-11T00:00:00.000Z",
    event,
  };
}

function streamEvent(event_seq: number, event: BackboneEvent): SessionEventEnvelope {
  return {
    session_id: "s1",
    event_seq,
    occurred_at_ms: event_seq,
    committed_at_ms: event_seq,
    session_update_type: event.type,
    turn_id: "turn-1",
    entry_index: 0,
    notification: envelope(event),
  };
}

function agentDelta(event_seq: number, delta: string): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "agent_message_delta",
    payload: {
      threadId: "thread-1",
      turnId: "turn-1",
      itemId: "item-1",
      delta,
    },
  });
}

describe("sessionStreamReducer", () => {
  it("按 event_seq 排序、去重，并累积 agent message delta", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      agentDelta(2, " world"),
      agentDelta(1, "hello"),
      agentDelta(1, "duplicate"),
    ]);

    expect(state.lastAppliedSeq).toBe(2);
    expect(state.rawEvents.map((event) => event.event_seq)).toEqual([1, 2]);
    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.accumulatedText).toBe("hello world");
  });

  it("在 model 层解析 context_frame 到 SessionDisplayEntry", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      streamEvent(1, {
        type: "platform",
        payload: {
          kind: "session_meta_update",
          data: {
            key: "context_frame",
            value: {
              id: "ctx-1",
              kind: "identity",
              source: "runtime",
              delivery_status: "delivered",
              delivery_channel: "system",
              message_role: "system",
              rendered_text: "",
              created_at_ms: 123,
              sections: [],
            },
          },
        },
      }),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.contextFrame?.id).toBe("ctx-1");
    expect(state.entries[0]?.contextFrame?.rendered_text).toBe("");
  });

  it("approval_request 需要立即 flush", () => {
    const approvalEvent = streamEvent(1, {
      type: "approval_request",
      payload: {
        kind: "tool_user_input",
        data: {
          request_id: "req-1",
          params: {
            threadId: "thread-1",
            turnId: "turn-1",
            itemId: "item-1",
            questions: [],
            autoResolutionMs: null,
          },
        },
      },
    });

    expect(shouldFlushStreamEventImmediately(approvalEvent)).toBe(true);
  });
});

import { describe, expect, it } from "vitest";
import type { BackboneEnvelope, BackboneEvent, ThreadItem } from "../../../generated/backbone-protocol";
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

function retryError(event_seq: number): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "error",
    payload: {
      threadId: "thread-1",
      turnId: "turn-1",
      willRetry: true,
      error: {
        message: "Reconnecting... 1/3",
        codexErrorInfo: null,
        additionalDetails: null,
      },
    },
  });
}

function sessionRewound(event_seq: number, stableEventSeq: number): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: {
        key: "session_rewound",
        value: {
          discarded_turn_id: "turn-1",
          stable_event_seq: stableEventSeq,
          reason: "provider_retry",
          message: "已丢弃失败轮次，恢复到上一稳定状态",
        },
      },
    },
  });
}

function commandItem(id: string, aggregatedOutput: string | null): Extract<ThreadItem, { type: "commandExecution" }> {
  return {
    type: "commandExecution",
    id,
    command: "printf test",
    cwd: "/tmp",
    processId: null,
    source: "agent",
    status: "completed",
    commandActions: [],
    aggregatedOutput,
    exitCode: 0,
    durationMs: 10,
  };
}

function fileChangeItem(id: string, diff: string): Extract<ThreadItem, { type: "fileChange" }> {
  return {
    type: "fileChange",
    id,
    changes: [
      {
        path: "workspace://notes.txt",
        kind: { type: "add" },
        diff,
      },
    ],
    status: "inProgress",
  };
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

  it("willRetry error 保留 raw event，但不生成普通 fatal error entry", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      retryError(1),
    ]);

    expect(state.rawEvents).toHaveLength(1);
    expect(state.rawEvents[0]?.notification.event.type).toBe("error");
    expect(state.entries).toHaveLength(0);
    expect(state.lastAppliedSeq).toBe(1);
  });

  it("session_rewound 会按稳定边界移除失败轮次的半截展示", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      agentDelta(1, "partial failed output"),
      streamEvent(2, {
        type: "platform",
        payload: {
          kind: "session_meta_update",
          data: {
            key: "turn_terminal",
            value: {
              terminal_type: "turn_failed",
              turn_id: "turn-1",
              message: "provider disconnected",
            },
          },
        },
      }),
      sessionRewound(3, 0),
    ]);

    expect(state.lastAppliedSeq).toBe(3);
    expect(state.rawEvents.map((event) => event.event_seq)).toEqual([3]);
    expect(state.entries.some((entry) => entry.accumulatedText?.includes("partial failed output"))).toBe(false);
  });

  it("command completed 后使用 final aggregatedOutput 作为终态 bounded 展示源", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      streamEvent(1, {
        type: "item_started",
        payload: {
          item: commandItem("cmd-1", null),
          threadId: "thread-1",
          turnId: "turn-1",
          startedAtMs: 1,
        },
      }),
      streamEvent(2, {
        type: "command_output_delta",
        payload: {
          threadId: "thread-1",
          turnId: "turn-1",
          itemId: "cmd-1",
          delta: "live preview\n",
        },
      }),
      streamEvent(3, {
        type: "item_completed",
        payload: {
          item: commandItem(
            "cmd-1",
            "command: printf test\noutput_truncated: true (omitted_bytes=4096)\nfinal bounded output",
          ),
          threadId: "thread-1",
          turnId: "turn-1",
          completedAtMs: 3,
        },
      }),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.accumulatedText).toContain("output_truncated: true");
    expect(state.entries[0]?.accumulatedText).toContain("final bounded output");
    expect(state.entries[0]?.accumulatedText).not.toBe("live preview\n");
  });

  it("相同 item_id 的 repeated item_started 更新 fileChange 条目而非追加", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      streamEvent(1, {
        type: "item_started",
        payload: {
          item: fileChangeItem("patch-1", "+hello"),
          threadId: "thread-1",
          turnId: "turn-1",
          startedAtMs: 1,
        },
      }),
      streamEvent(2, {
        type: "item_started",
        payload: {
          item: fileChangeItem("patch-1", "+hello\n+world"),
          threadId: "thread-1",
          turnId: "turn-1",
          startedAtMs: 2,
        },
      }),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.eventSeq).toBe(2);
    const event = state.entries[0]?.event;
    expect(event?.type).toBe("item_started");
    if (event?.type !== "item_started") {
      throw new Error("expected item_started");
    }
    const item = event.payload.item;
    expect(item.type).toBe("fileChange");
    if (item.type !== "fileChange") {
      throw new Error("expected fileChange");
    }
    expect(item.changes[0]?.diff).toBe("+hello\n+world");
  });

  it("item_updated 更新同一 item_id 的卡片而非追加", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      streamEvent(1, {
        type: "item_started",
        payload: {
          item: fileChangeItem("patch-1", "+hello"),
          threadId: "thread-1",
          turnId: "turn-1",
          startedAtMs: 1,
        },
      }),
      streamEvent(2, {
        type: "item_updated",
        payload: {
          item: fileChangeItem("patch-1", "+hello\n+world"),
          threadId: "thread-1",
          turnId: "turn-1",
          updatedAtMs: 2,
        },
      }),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.eventSeq).toBe(2);
    const event = state.entries[0]?.event;
    expect(event?.type).toBe("item_updated");
    if (event?.type !== "item_updated") {
      throw new Error("expected item_updated");
    }
    const item = event.payload.item;
    if (item.type !== "fileChange") {
      throw new Error("expected fileChange");
    }
    expect(item.changes[0]?.diff).toBe("+hello\n+world");
  });

  it("item_updated 不属于立即 flush 的可重放谓词", () => {
    const updatedEvent = streamEvent(1, {
      type: "item_updated",
      payload: {
        item: fileChangeItem("patch-1", "+hello"),
        threadId: "thread-1",
        turnId: "turn-1",
        updatedAtMs: 1,
      },
    });

    expect(shouldFlushStreamEventImmediately(updatedEvent)).toBe(false);
  });
});

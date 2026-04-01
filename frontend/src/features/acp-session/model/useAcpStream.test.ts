import { describe, expect, it } from "vitest";
import type { SessionNotification } from "@agentclientprotocol/sdk";

import { reduceStreamState } from "./useAcpStream";
import type { SessionEventEnvelope } from "./types";

function buildTextEvent(seq: number, text: string): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: "agent_message_chunk",
    turn_id: "turn-1",
    entry_index: 0,
    tool_call_id: null,
    notification: {
      sessionId: "sess-1",
      update: {
        sessionUpdate: "agent_message_chunk",
        content: {
          type: "text",
          text,
        },
        _meta: {
          agentdash: {
            v: 1,
            trace: {
              turnId: "turn-1",
              entryIndex: 0,
            },
          },
        },
      },
    } as unknown as SessionNotification,
  };
}

function buildToolCallEvent(seq: number, status: "pending" | "completed"): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: status === "pending" ? "tool_call" : "tool_call_update",
    turn_id: "turn-1",
    entry_index: null,
    tool_call_id: "tool-1",
    notification: {
      sessionId: "sess-1",
      update: {
        sessionUpdate: status === "pending" ? "tool_call" : "tool_call_update",
        toolCallId: "tool-1",
        title: "执行 shell",
        kind: "execute",
        status,
        content: [],
      },
    } as unknown as SessionNotification,
  };
}

function buildPendingApprovalUpdate(seq: number): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: "tool_call_update",
    turn_id: "turn-1",
    entry_index: null,
    tool_call_id: "tool-1",
    notification: {
      sessionId: "sess-1",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        title: "执行 shell",
        kind: "execute",
        status: "pending",
        content: [],
        rawOutput: {
          approval_state: "pending",
        },
      },
    } as unknown as SessionNotification,
  };
}

function buildEnvelopeAnchoredToolCallUpdate(
  seq: number,
  status: "in_progress" | "completed",
): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: "tool_call_update",
    turn_id: "turn-1",
    entry_index: 7,
    tool_call_id: "tool-1",
    notification: {
      sessionId: "sess-1",
      update: {
        sessionUpdate: "tool_call_update",
        title: "执行 shell",
        kind: "execute",
        status,
        content: [],
      },
    } as unknown as SessionNotification,
  };
}

function buildTextEventWithoutTrace(seq: number, text: string): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: "agent_message_chunk",
    turn_id: null,
    entry_index: null,
    tool_call_id: null,
    notification: {
      sessionId: "sess-1",
      update: {
        sessionUpdate: "agent_message_chunk",
        content: {
          type: "text",
          text,
        },
      },
    } as unknown as SessionNotification,
  };
}

function buildMessageIdTextEvent(seq: number, text: string, messageId: string): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: "agent_message_chunk",
    turn_id: null,
    entry_index: null,
    tool_call_id: null,
    notification: {
      sessionId: "sess-1",
      update: {
        sessionUpdate: "agent_message_chunk",
        messageId,
        content: {
          type: "text",
          text,
        },
      },
    } as unknown as SessionNotification,
  };
}

function buildSystemEvent(seq: number, eventType: string): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: seq,
    occurred_at_ms: seq,
    committed_at_ms: seq,
    session_update_type: "session_info_update",
    turn_id: "turn-1",
    entry_index: null,
    tool_call_id: null,
    notification: {
      sessionId: "sess-1",
      update: {
        sessionUpdate: "session_info_update",
        _meta: {
          agentdash: {
            v: 1,
            trace: {
              turnId: "turn-1",
            },
            event: {
              type: eventType,
            },
          },
        },
      },
    } as unknown as SessionNotification,
  };
}

describe("reduceStreamState", () => {
  it("同一段历史重复 hydrate 不会重复追加 entries 或 rawEvents", () => {
    const initial = {
      entries: [],
      rawEvents: [],
      tokenUsage: null,
      lastAppliedSeq: 0,
    };

    const history = [
      buildTextEvent(1, "hello"),
      buildToolCallEvent(2, "pending"),
      buildSystemEvent(3, "turn_started"),
    ];

    const first = reduceStreamState(initial, history);
    const replayed = reduceStreamState(first, history);

    expect(first.rawEvents).toHaveLength(3);
    expect(first.entries).toHaveLength(3);
    expect(first.lastAppliedSeq).toBe(3);
    expect(replayed.rawEvents).toHaveLength(3);
    expect(replayed.entries).toHaveLength(3);
    expect(replayed.lastAppliedSeq).toBe(3);
  });

  it("tool_call_update 会更新已有 tool 条目而不是在底部堆积新卡片", () => {
    const initial = {
      entries: [],
      rawEvents: [],
      tokenUsage: null,
      lastAppliedSeq: 0,
    };

    const started = reduceStreamState(initial, [
      buildToolCallEvent(2, "pending"),
      buildSystemEvent(3, "turn_started"),
    ]);
    const completed = reduceStreamState(started, [buildToolCallEvent(4, "completed")]);

    expect(completed.rawEvents.map((event) => event.event_seq)).toEqual([2, 3, 4]);
    expect(completed.entries).toHaveLength(2);
    expect(completed.entries[0]?.update.sessionUpdate).toBe("tool_call");
    expect((completed.entries[0]?.update as { status?: string }).status).toBe("completed");
    expect(completed.entries[0]?.eventSeq).toBe(4);
  });

  it("普通 pending tool_call 不会被误判成等待审批", () => {
    const initial = {
      entries: [],
      rawEvents: [],
      tokenUsage: null,
      lastAppliedSeq: 0,
    };

    const started = reduceStreamState(initial, [buildToolCallEvent(2, "pending")]);

    expect(started.entries).toHaveLength(1);
    expect(started.entries[0]?.isPendingApproval).toBe(false);

    const approvalPending = reduceStreamState(started, [buildPendingApprovalUpdate(3)]);
    expect(approvalPending.entries[0]?.isPendingApproval).toBe(true);
  });

  it("实时 envelope 提供 tool_call_id 时，缺少 payload id 的 update 仍会合并回已有 tool 条目", () => {
    const initial = {
      entries: [],
      rawEvents: [],
      tokenUsage: null,
      lastAppliedSeq: 0,
    };

    const started = reduceStreamState(initial, [buildToolCallEvent(2, "pending")]);
    const completed = reduceStreamState(started, [
      buildEnvelopeAnchoredToolCallUpdate(3, "completed"),
    ]);

    expect(completed.entries).toHaveLength(1);
    expect(completed.entries[0]?.eventSeq).toBe(3);
    expect((completed.entries[0]?.update as { status?: string }).status).toBe("completed");
  });

  it("incoming 等于 previous 尾部时仍按增量拼接，不会误吞字", () => {
    const initial = {
      entries: [],
      rawEvents: [],
      tokenUsage: null,
      lastAppliedSeq: 0,
    };

    const first = reduceStreamState(initial, [buildTextEvent(1, "the")]);
    const second = reduceStreamState(first, [buildTextEvent(2, "he")]);

    expect(second.entries).toHaveLength(1);
    const update = second.entries[0]?.update as {
      content?: { type?: string; text?: string };
    };
    expect(update.content?.text).toBe("thehe");
  });

  it("缺少 turn trace 时不做尾部猜测合并，避免跨 turn 误拼接", () => {
    const initial = {
      entries: [],
      rawEvents: [],
      tokenUsage: null,
      lastAppliedSeq: 0,
    };

    const first = reduceStreamState(initial, [buildTextEventWithoutTrace(1, "hello")]);
    const second = reduceStreamState(first, [buildTextEventWithoutTrace(2, " world")]);

    expect(second.entries).toHaveLength(2);
  });

  it("缺少 turn/entry trace 但存在同 messageId 时，仍可稳定合并 chunk", () => {
    const initial = {
      entries: [],
      rawEvents: [],
      tokenUsage: null,
      lastAppliedSeq: 0,
    };

    const messageId = "11111111-1111-4111-8111-111111111111";
    const first = reduceStreamState(initial, [buildMessageIdTextEvent(1, "he", messageId)]);
    const second = reduceStreamState(first, [buildMessageIdTextEvent(2, "llo", messageId)]);

    expect(second.entries).toHaveLength(1);
    const update = second.entries[0]?.update as {
      content?: { type?: string; text?: string };
    };
    expect(update.content?.text).toBe("hello");
  });
});

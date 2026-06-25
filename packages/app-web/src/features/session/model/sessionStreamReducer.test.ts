import { describe, expect, it } from "vitest";
import type {
  BackboneEnvelope,
  BackboneEvent,
  ProviderAttemptPhase,
  ThreadItem,
} from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "./types";
import {
  createInitialStreamState,
  reduceStreamState,
  resetEphemeralCursor,
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

// ephemeral 事件的 event_seq 字段承载单调 ephemeral_seq（后端 push_ephemeral 分配）。
function ephemeralAgentDelta(ephemeralSeq: number, delta: string): SessionEventEnvelope {
  return { ...agentDelta(ephemeralSeq, delta), ephemeral: true };
}

function ephemeralProviderAttemptStatus(
  ephemeralSeq: number,
  phase: ProviderAttemptPhase = "connected_waiting_first_delta",
): SessionEventEnvelope {
  return {
    ...streamEvent(ephemeralSeq, {
      type: "platform",
      payload: {
        kind: "provider_attempt_status",
        data: {
          turn_id: "turn-1",
          phase,
          attempt: 1,
          max_attempts: 3,
          will_retry: false,
          delay_ms: null,
          reason_code: null,
          message: null,
          provider: null,
          model: null,
        },
      },
    }),
    session_update_type: "provider_attempt_status",
    ephemeral: true,
  };
}

function turnTerminal(event_seq: number): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: {
        key: "turn_terminal",
        value: {
          terminal_type: "turn_completed",
          turn_id: "turn-1",
          duration_ms: 100,
        },
      },
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

function commandItem(
  id: string,
  aggregatedOutput: string | null,
  status: Extract<ThreadItem, { type: "commandExecution" }>["status"] = "completed",
): Extract<ThreadItem, { type: "commandExecution" }> {
  return {
    type: "commandExecution",
    id,
    command: "printf test",
    cwd: "/tmp",
    processId: null,
    source: "agent",
    status,
    commandActions: [],
    aggregatedOutput,
    exitCode: status === "completed" ? 0 : null,
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

function agentMessageItem(id: string, text: string): Extract<ThreadItem, { type: "agentMessage" }> {
  return { type: "agentMessage", id, text, phase: null, memoryCitation: null };
}

function reasoningItem(id: string, content: string[], summary: string[] = []): Extract<ThreadItem, { type: "reasoning" }> {
  return { type: "reasoning", id, summary, content };
}

function reasoningDelta(event_seq: number, delta: string): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "reasoning_text_delta",
    payload: { threadId: "thread-1", turnId: "turn-1", itemId: "item-1", delta, contentIndex: 0 },
  });
}

function itemCompleted(event_seq: number, item: ThreadItem): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "item_completed",
    payload: { item, threadId: "thread-1", turnId: "turn-1", completedAtMs: event_seq },
  });
}

function itemStarted(event_seq: number, item: ThreadItem): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "item_started",
    payload: { item, threadId: "thread-1", turnId: "turn-1", startedAtMs: event_seq },
  });
}

function itemUpdated(event_seq: number, item: ThreadItem): SessionEventEnvelope {
  return streamEvent(event_seq, {
    type: "item_updated",
    payload: { item, threadId: "thread-1", turnId: "turn-1", updatedAtMs: event_seq },
  });
}

function ephemeralItemUpdated(ephemeralSeq: number, item: ThreadItem): SessionEventEnvelope {
  return { ...itemUpdated(ephemeralSeq, item), ephemeral: true };
}

function ephemeralCommandOutputDelta(ephemeralSeq: number, itemId: string, delta: string): SessionEventEnvelope {
  return {
    ...streamEvent(ephemeralSeq, {
      type: "command_output_delta",
      payload: {
        threadId: "thread-1",
        turnId: "turn-1",
        itemId,
        delta,
      },
    }),
    ephemeral: true,
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

  it("session_rewound 不裁剪前端 rawEvents 或失败轮次展示", () => {
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
    expect(state.rawEvents.map((event) => event.event_seq)).toEqual([1, 2, 3]);
    expect(state.entries.some((entry) => entry.accumulatedText?.includes("partial failed output"))).toBe(true);
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

  it("durable + ephemeral 混合批次按 lane 位置应用，同 item update 不被 started 回写", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      itemStarted(10, commandItem("cmd-1", null, "inProgress")),
      ephemeralItemUpdated(1, commandItem("cmd-1", "newer live state", "inProgress")),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.rawEvents.map((event) => event.event_seq)).toEqual([10]);
    expect(state.lastAppliedSeq).toBe(10);
    expect(state.lastEphemeralSeq).toBe(1);
    expect(state.entries[0]?.eventSeq).toBe(10);
    expect(state.entries[0]?.progressSeq).toBe(1);
    expect(state.entries[0]?.timelineOrder).toEqual({ kind: "durable", seq: 10 });
    expect(state.entries[0]?.itemFreshness).toBe("progress");

    const event = state.entries[0]?.event;
    expect(event?.type).toBe("item_updated");
    if (event?.type !== "item_updated") {
      throw new Error("expected item_updated");
    }
    const item = event.payload.item;
    if (item.type !== "commandExecution") {
      throw new Error("expected commandExecution");
    }
    expect(item.aggregatedOutput).toBe("newer live state");
  });

  it("后到 durable item_started 只补 durable anchor，不覆盖同 item 更新态", () => {
    const afterEphemeral = reduceStreamState(createInitialStreamState([]), [
      ephemeralItemUpdated(1, commandItem("cmd-1", "newer live state", "inProgress")),
    ]);
    expect(afterEphemeral.entries[0]?.eventSeq).toBe(1);
    expect(afterEphemeral.entries[0]?.timelineOrder).toEqual({
      kind: "anchored_progress",
      anchorId: "item:cmd-1",
      progressSeq: 1,
    });

    const afterStarted = reduceStreamState(afterEphemeral, [
      itemStarted(10, commandItem("cmd-1", null, "inProgress")),
    ]);

    expect(afterStarted.entries).toHaveLength(1);
    expect(afterStarted.rawEvents.map((event) => event.event_seq)).toEqual([10]);
    expect(afterStarted.entries[0]?.eventSeq).toBe(10);
    expect(afterStarted.entries[0]?.progressSeq).toBe(1);
    expect(afterStarted.entries[0]?.timelineOrder).toEqual({ kind: "durable", seq: 10 });
    expect(afterStarted.entries[0]?.itemFreshness).toBe("progress");

    const event = afterStarted.entries[0]?.event;
    expect(event?.type).toBe("item_updated");
    if (event?.type !== "item_updated") {
      throw new Error("expected item_updated");
    }
    const item = event.payload.item;
    if (item.type !== "commandExecution") {
      throw new Error("expected commandExecution");
    }
    expect(item.aggregatedOutput).toBe("newer live state");
  });

  it("completed 终态高于后续 item_updated/progress，不被 ephemeral 污染", () => {
    const afterCompleted = reduceStreamState(createInitialStreamState([]), [
      itemStarted(1, commandItem("cmd-1", null, "inProgress")),
      itemCompleted(2, commandItem("cmd-1", "final output", "completed")),
    ]);

    const afterEphemeral = reduceStreamState(afterCompleted, [
      ephemeralItemUpdated(1, commandItem("cmd-1", "stale live state", "inProgress")),
      ephemeralCommandOutputDelta(2, "cmd-1", "stale output"),
    ]);

    expect(afterEphemeral.entries).toHaveLength(1);
    expect(afterEphemeral.rawEvents.map((event) => event.event_seq)).toEqual([1, 2]);
    expect(afterEphemeral.entries[0]?.eventSeq).toBe(2);
    expect(afterEphemeral.entries[0]?.progressSeq).toBe(1);
    expect(afterEphemeral.entries[0]?.itemFreshness).toBe("completed");
    expect(afterEphemeral.entries[0]?.accumulatedText).toBe("final output");

    const event = afterEphemeral.entries[0]?.event;
    expect(event?.type).toBe("item_completed");
  });

  it("ephemeral progress 更新 durable anchor 时不把 ephemeral_seq 写入 durable 时间线", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      itemStarted(20, commandItem("cmd-1", null, "inProgress")),
      ephemeralCommandOutputDelta(1, "cmd-1", "live preview"),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.eventSeq).toBe(20);
    expect(state.entries[0]?.progressSeq).toBe(1);
    expect(state.entries[0]?.timelineOrder).toEqual({ kind: "durable", seq: 20 });
    expect(state.entries[0]?.accumulatedText).toBe("live preview");
    expect(state.rawEvents.map((event) => event.event_seq)).toEqual([20]);
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

  it("终态 agentMessage 并入流式气泡为单条并以终态文本 finalize", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      agentDelta(1, "partial"),
      itemCompleted(2, agentMessageItem("item-1", "FULL FINAL")),
    ]);

    // 单条 assistant 气泡，文本=终态权威，isStreaming=false，且仍渲染为 agent_message_delta。
    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.accumulatedText).toBe("FULL FINAL");
    expect(state.entries[0]?.isStreaming).toBe(false);
    expect(state.entries[0]?.event.type).toBe("agent_message_delta");
  });

  it("hydrate 仅终态 agentMessage（无 delta）仍渲染助手气泡", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      itemCompleted(1, agentMessageItem("turn-1:0:msg", "ONLY FINAL")),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.accumulatedText).toBe("ONLY FINAL");
    expect(state.entries[0]?.event.type).toBe("agent_message_delta");
  });

  it("P1-b：终态 finalize 后再来同 item_id 的 ephemeral delta 不脏化已 final 气泡", () => {
    // durable 终态先 finalize（isStreaming=false），随后剪枝前在途的旧 ephemeral delta 到达。
    const afterFinal = reduceStreamState(createInitialStreamState([]), [
      itemCompleted(2, agentMessageItem("item-1", "FULL FINAL")),
    ]);
    expect(afterFinal.entries[0]?.accumulatedText).toBe("FULL FINAL");
    expect(afterFinal.entries[0]?.isStreaming).toBe(false);

    // ephemeral 旧 delta（同 item_id）应被跳过，accumulatedText 不变。
    const afterStaleDelta = reduceStreamState(afterFinal, [ephemeralAgentDelta(1, "partial")]);
    expect(afterStaleDelta.entries).toHaveLength(1);
    expect(afterStaleDelta.entries[0]?.accumulatedText).toBe("FULL FINAL");
    expect(afterStaleDelta.entries[0]?.isStreaming).toBe(false);
  });

  it("ephemeral delta 更新 entries 但不进 rawEvents、不动 lastAppliedSeq", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      ephemeralAgentDelta(1, "hello"),
      ephemeralAgentDelta(2, " world"),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.accumulatedText).toBe("hello world");
    expect(state.rawEvents).toHaveLength(0);
    expect(state.lastAppliedSeq).toBe(0);
    expect(state.lastEphemeralSeq).toBe(2);
  });

  it("ephemeral provider status 只更新 live waiting 状态，不进 rawEvents 或 entries", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      ephemeralProviderAttemptStatus(1),
    ]);

    expect(state.rawEvents).toHaveLength(0);
    expect(state.entries).toHaveLength(0);
    expect(state.providerWaitingSeqs.get("turn-1")).toBe(1);
    expect(state.lastAppliedSeq).toBe(0);
    expect(state.lastEphemeralSeq).toBe(1);
  });

  it("provider succeeded 清理 live waiting 状态", () => {
    const waiting = reduceStreamState(createInitialStreamState([]), [
      ephemeralProviderAttemptStatus(1),
    ]);
    const cleared = reduceStreamState(waiting, [
      ephemeralProviderAttemptStatus(2, "succeeded"),
    ]);

    expect(cleared.providerWaitingSeqs.size).toBe(0);
    expect(cleared.rawEvents).toHaveLength(0);
    expect(cleared.entries).toHaveLength(0);
    expect(cleared.lastEphemeralSeq).toBe(2);
  });

  it("turn terminal 清理 live provider waiting 状态", () => {
    const waiting = reduceStreamState(createInitialStreamState([]), [
      ephemeralProviderAttemptStatus(1),
    ]);
    const cleared = reduceStreamState(waiting, [turnTerminal(2)]);

    expect(cleared.providerWaitingSeqs.size).toBe(0);
    expect(cleared.rawEvents.map((event) => event.event_seq)).toEqual([2]);
    expect(cleared.lastAppliedSeq).toBe(2);
  });

  it("durable 事件后再来 ephemeral：dedup 不误杀且不污染 rawEvents", () => {
    const afterDurable = reduceStreamState(createInitialStreamState([]), [
      agentDelta(5, "durable"),
    ]);
    expect(afterDurable.lastAppliedSeq).toBe(5);
    expect(afterDurable.rawEvents).toHaveLength(1);

    // ephemeral_seq 独立于 durable event_seq，不应被 `<= lastAppliedSeq(5)` 跳过。
    const afterEphemeral = reduceStreamState(afterDurable, [ephemeralAgentDelta(1, " live")]);
    expect(afterEphemeral.entries[0]?.accumulatedText).toBe("durable live");
    expect(afterEphemeral.rawEvents).toHaveLength(1);
    expect(afterEphemeral.lastAppliedSeq).toBe(5);
    expect(afterEphemeral.lastEphemeralSeq).toBe(1);
  });

  it("整页刷新：lastEphemeralSeq=0 回放服务端 buffer 全部累积 delta", () => {
    // 模拟整页刷新后 NDJSON 补发的 ephemeral buffer 快照（seq 1..3）。
    const state = reduceStreamState(createInitialStreamState([]), [
      ephemeralAgentDelta(1, "AB"),
      ephemeralAgentDelta(2, "CD"),
      ephemeralAgentDelta(3, "EF"),
    ]);
    expect(state.entries[0]?.accumulatedText).toBe("ABCDEF");
    expect(state.lastEphemeralSeq).toBe(3);
    expect(state.rawEvents).toHaveLength(0);
  });

  it("断线重连：只应用 seq>lastEphemeralSeq，不重复累加", () => {
    // 重连前已应用 seq 1..2。
    const before = reduceStreamState(createInitialStreamState([]), [
      ephemeralAgentDelta(1, "AB"),
      ephemeralAgentDelta(2, "CD"),
    ]);
    expect(before.entries[0]?.accumulatedText).toBe("ABCD");
    expect(before.lastEphemeralSeq).toBe(2);

    // 重连后服务端全量补发 buffer（seq 1..3）；前端只应用 seq>2（即 seq 3）。
    const after = reduceStreamState(before, [
      ephemeralAgentDelta(1, "AB"),
      ephemeralAgentDelta(2, "CD"),
      ephemeralAgentDelta(3, "EF"),
    ]);
    expect(after.entries[0]?.accumulatedText).toBe("ABCDEF");
    expect(after.lastEphemeralSeq).toBe(3);
  });

  it("P2：resetEphemeralCursor 归零 lastEphemeralSeq（epoch 变化时）", () => {
    const before = reduceStreamState(createInitialStreamState([]), [
      ephemeralAgentDelta(1, "AB"),
      ephemeralAgentDelta(2, "CD"),
    ]);
    expect(before.lastEphemeralSeq).toBe(2);

    const reset = resetEphemeralCursor(before);
    expect(reset.lastEphemeralSeq).toBe(0);
    // 仅动游标，不触碰已累积 entries。
    expect(reset.entries[0]?.accumulatedText).toBe("ABCD");

    // lastEphemeralSeq 已为 0 时返回原引用（无效更新）。
    expect(resetEphemeralCursor(reset)).toBe(reset);
  });

  it("P2：resetEphemeralCursor 同时清理 live provider waiting 状态", () => {
    const before = reduceStreamState(createInitialStreamState([]), [
      ephemeralProviderAttemptStatus(1),
    ]);
    expect(before.providerWaitingSeqs.get("turn-1")).toBe(1);

    const reset = resetEphemeralCursor(before);
    expect(reset.lastEphemeralSeq).toBe(0);
    expect(reset.providerWaitingSeqs.size).toBe(0);
  });

  it("ephemeral 乱序到达仍按 ephemeral_seq 升序应用", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      ephemeralAgentDelta(2, "CD"),
      ephemeralAgentDelta(1, "AB"),
      ephemeralAgentDelta(3, "EF"),
    ]);
    expect(state.entries[0]?.accumulatedText).toBe("ABCDEF");
    expect(state.lastEphemeralSeq).toBe(3);
  });

  it("终态 reasoning 并入 reasoning 气泡为单条", () => {
    const state = reduceStreamState(createInitialStreamState([]), [
      reasoningDelta(1, "rpart"),
      itemCompleted(2, reasoningItem("item-1", ["FULL REASONING"])),
    ]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.accumulatedText).toBe("FULL REASONING");
    expect(state.entries[0]?.event.type).toBe("reasoning_text_delta");
  });
});

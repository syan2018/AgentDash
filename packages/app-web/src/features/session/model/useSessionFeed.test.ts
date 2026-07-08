import { describe, expect, it } from "vitest";
import { aggregateEntries, mergeThinkingIntoDisplayItems, segmentByTurn } from "./useSessionFeed";
import type {
  SessionDisplayEntry,
  AggregatedEntryGroup,
  AggregatedThinkingGroup,
  SessionEventEnvelope,
} from "./types";
import type { BackboneEnvelope, BackboneEvent, ThreadItem, Turn } from "../../../generated/backbone-protocol";

let nextSeq = 1;
function seq(): number {
  return nextSeq++;
}

function asEntry(id: string, event: BackboneEvent, extra?: Partial<SessionDisplayEntry>): SessionDisplayEntry {
  return {
    id,
    sessionId: "s1",
    timestamp: 0,
    eventSeq: seq(),
    event,
    ...extra,
  };
}

function mkCmdEntry(
  id: string,
  command: string,
  opts?: {
    isPendingApproval?: boolean;
    status?: "inProgress" | "completed" | "failed" | "declined";
    output?: string | null;
  },
): SessionDisplayEntry {
  const item = {
    type: "commandExecution",
    id,
    command,
    cwd: "/tmp",
    processId: null,
    source: "agent",
    status: opts?.status ?? "completed",
    commandActions: [],
    aggregatedOutput: opts?.output ?? null,
    exitCode: 0,
    durationMs: 100,
  } as unknown as ThreadItem;
  const event: BackboneEvent = {
    type: "item_started",
    payload: { item, threadId: "t1", turnId: "u1", startedAtMs: 0 },
  };
  return asEntry(id, event, { isPendingApproval: opts?.isPendingApproval, turnId: "u1" });
}

function mkCmdUpdatedEntry(
  id: string,
  command: string,
  opts?: {
    isPendingApproval?: boolean;
    status?: "inProgress" | "completed" | "failed" | "declined";
    output?: string | null;
  },
): SessionDisplayEntry {
  const started = mkCmdEntry(id, command, opts);
  if (started.event.type !== "item_started") {
    throw new Error("expected command helper to create item_started");
  }
  const event: BackboneEvent = {
    type: "item_updated",
    payload: {
      item: started.event.payload.item,
      threadId: "t1",
      turnId: "u1",
      updatedAtMs: 1,
    },
  };
  return { ...started, event };
}

function mkFileChangeEntry(id: string, path: string): SessionDisplayEntry {
  const item = {
    type: "fileChange",
    id,
    changes: [{ path, diff: "" }],
    status: "completed",
  } as unknown as ThreadItem;
  const event: BackboneEvent = {
    type: "item_started",
    payload: { item, threadId: "t1", turnId: "u1", startedAtMs: 0 },
  };
  return asEntry(id, event);
}

function mkMcpEntry(id: string): SessionDisplayEntry {
  const item = {
    type: "mcpToolCall",
    id,
    server: "srv",
    tool: "do",
    status: "completed",
    arguments: null,
    result: null,
    error: null,
    durationMs: 50,
  } as unknown as ThreadItem;
  const event: BackboneEvent = {
    type: "item_started",
    payload: { item, threadId: "t1", turnId: "u1", startedAtMs: 0 },
  };
  return asEntry(id, event);
}

function mkDynamicToolEntry(id: string, tool: string): SessionDisplayEntry {
  const item = {
    type: "dynamicToolCall",
    id,
    tool,
    status: "completed",
    arguments: {},
    contentItems: null,
    durationMs: null,
    success: true,
    namespace: null,
  } as unknown as ThreadItem;
  const event: BackboneEvent = {
    type: "item_completed",
    payload: { item, threadId: "t1", turnId: "u1", completedAtMs: 0 },
  };
  return asEntry(id, event);
}

function mkCompanionSubagentEntry(id: string): SessionDisplayEntry {
  const item = {
    type: "dynamicToolCall",
    id,
    tool: "companion_request",
    status: "completed",
    arguments: {
      target: "sub",
      payload: {
        agent_key: "reviewer",
        message: "Review this change",
      },
    },
    contentItems: [
      {
        type: "inputText",
        text: JSON.stringify({
          details: {
            kind: "companion_subagent_dispatch",
            child: { agent_id: "agent-child" },
            journal: { uri: "lifecycle://agent-runs/agent-child/sessions/messages" },
            status: "running",
            summary: "Reviewer launched",
          },
        }),
      },
    ],
    durationMs: null,
    success: true,
    namespace: null,
  } as unknown as ThreadItem;
  const event: BackboneEvent = {
    type: "item_completed",
    payload: { item, threadId: "t1", turnId: "u1", completedAtMs: 0 },
  };
  return asEntry(id, event);
}

function mkMessageEntry(id: string, text: string): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "agent_message_delta",
    payload: { threadId: "t1", turnId: "u1", itemId: id, delta: text },
  };
  return asEntry(id, event, { accumulatedText: text, turnId: "u1" });
}

function mkUserInputEntry(id: string, text: string, eventSeq?: number): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "user_input_submitted",
    payload: {
      threadId: "t1",
      turnId: "u1",
      itemId: id,
      submissionKind: "prompt",
      source: {
        namespace: "core",
        kind: "composer",
        actor: "user",
        displayLabelKey: "mailbox.source.core.composer",
      },
      content: [{ type: "text", text, text_elements: [] }],
    },
  };
  return asEntry(id, event, { accumulatedText: text, turnId: "u1", eventSeq });
}

function backboneEnvelope(event: BackboneEvent, turnId = "u1"): BackboneEnvelope {
  return {
    sessionId: "s1",
    source: {
      connectorId: "connector",
      connectorType: "test",
      executorId: null,
    },
    trace: {
      turnId,
      entryIndex: 0,
    },
    observedAt: "2026-06-23T00:00:00.000Z",
    event,
  };
}

function rawEvent(eventSeq: number, event: BackboneEvent, turnId = "u1"): SessionEventEnvelope {
  return {
    session_id: "s1",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: eventSeq,
    session_update_type: event.type,
    turn_id: turnId,
    entry_index: 0,
    notification: backboneEnvelope(event, turnId),
  };
}

function turnPayload(
  id: string,
  status: Turn["status"],
  durationMs: number | null,
  opts?: { startedAt?: number | null },
): Turn {
  return {
    id,
    items: [],
    itemsView: "full",
    status,
    error: null,
    startedAt: opts?.startedAt ?? null,
    completedAt: null,
    durationMs,
  };
}

function rawTurnStarted(eventSeq: number, turnId = "u1", startedAt?: number): SessionEventEnvelope {
  return rawEvent(eventSeq, {
    type: "turn_started",
    payload: {
      threadId: "t1",
      turn: turnPayload(turnId, "inProgress", null, { startedAt }),
    },
  }, turnId);
}

function rawTurnCompleted(
  eventSeq: number,
  status: Turn["status"],
  durationMs: number,
  turnId = "u1",
): SessionEventEnvelope {
  return rawEvent(eventSeq, {
    type: "turn_completed",
    payload: {
      threadId: "t1",
      turn: turnPayload(turnId, status, durationMs),
    },
  }, turnId);
}

function rawTurnTerminal(
  eventSeq: number,
  terminalType: "turn_completed" | "turn_failed" | "turn_interrupted",
  durationMs: number,
  turnId = "u1",
): SessionEventEnvelope {
  return rawEvent(eventSeq, {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: {
        key: "turn_terminal",
        value: {
          terminal_type: terminalType,
          turn_id: turnId,
          duration_ms: durationMs,
          message: terminalType,
        },
      },
    },
  }, turnId);
}

function rawProviderAttemptStatus(
  eventSeq: number,
  value: Record<string, unknown>,
  turnId = "u1",
): SessionEventEnvelope {
  return rawEvent(eventSeq, {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: {
        key: "provider_attempt_status",
        value: {
          turn_id: turnId,
          ...value,
        },
      },
    },
  }, turnId);
}

function mkProviderStatusEntry(id: string): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: {
        key: "provider_attempt_status",
        value: {
          turn_id: "u1",
          phase: "retrying",
          attempt: 2,
          max_attempts: 3,
          will_retry: true,
        },
      },
    },
  };
  return asEntry(id, event, { turnId: "u1" });
}

function mkTurnStarted(id = "ts"): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "turn_started",
    payload: { threadId: "t1", turn: { id: "u1" } as unknown as never },
  };
  return asEntry(id, event);
}

function mkTurnCompleted(id = "tc"): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "turn_completed",
    payload: { threadId: "t1", turn: { id: "u1" } as unknown as never },
  };
  return asEntry(id, event);
}

function mkSilentHookTraceEntry(id: string): SessionDisplayEntry {
  const event = {
    type: "platform",
    payload: {
      kind: "hook_trace",
      data: {
        eventType: "hook:before_tool:allow",
        message: null,
        data: {
          code: "hook:before_tool:allow",
          diagnostics: [],
          injections: [],
        },
      },
    },
  } as unknown as BackboneEvent;
  return asEntry(id, event);
}

function mkObservedHookTraceEntry(id: string): SessionDisplayEntry {
  const event = {
    type: "platform",
    payload: {
      kind: "hook_trace",
      data: {
        eventType: "hook:before_provider_request:observed",
        message: "Hook 已观测到 LLM API 请求即将发出",
        data: {
          trigger: "before_provider_request",
          decision: "observed",
          sequence: 1,
          revision: 1,
          severity: "info",
          matched_rule_keys: [],
          refresh_snapshot: false,
          effects_applied: false,
          diagnostic_codes: ["session_binding_found"],
          diagnostics: [
            {
              code: "session_binding_found",
              message: "命中运行时绑定",
            },
          ],
          injections: [],
        },
      },
    },
  } as unknown as BackboneEvent;
  return asEntry(id, event);
}

function mkSystemMessageEntry(id: string, message: string): SessionDisplayEntry {
  const event = {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: {
        key: "system_message",
        value: { message },
      },
    },
  } as unknown as BackboneEvent;
  return asEntry(id, event);
}

function mkReasoningEntry(id: string): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "reasoning_text_delta",
    payload: { threadId: "t1", turnId: "u1", itemId: id, delta: "...", contentIndex: 0 },
  };
  return asEntry(id, event, { accumulatedText: "...", turnId: "u1" });
}

function mkReasoningEntryWithText(id: string, text: string): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "reasoning_text_delta",
    payload: { threadId: "t1", turnId: "u1", itemId: id, delta: text, contentIndex: 0 },
  };
  return asEntry(id, event, { accumulatedText: text, turnId: "u1" });
}

function mkContextFrameEntry(id: string): SessionDisplayEntry {
  const event = {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: {
        key: "context_frame",
        value: { id, kind: "identity" },
      },
    },
  } as unknown as BackboneEvent;
  return asEntry(id, event);
}

function isToolGroup(item: unknown): item is AggregatedEntryGroup {
  return (item as AggregatedEntryGroup)?.type === "aggregated_group";
}
function isContextFrameGroup(item: unknown): boolean {
  return (item as { type: string })?.type === "aggregated_context_frames";
}
function isThinkingGroup(item: unknown): item is AggregatedThinkingGroup {
  return (item as AggregatedThinkingGroup)?.type === "aggregated_thinking";
}

function providerWaitingSeqs(eventSeq: number, turnId = "u1"): ReadonlyMap<string, number> {
  return new Map([[turnId, eventSeq]]);
}

describe("aggregateEntries — tool burst", () => {
  it("T1: 5 connected commands fold into one tool_burst unit", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd"),
      mkCmdEntry("c3", "echo hi"),
      mkCmdEntry("c4", "uname"),
      mkCmdEntry("c5", "date"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    const group = result[0] as AggregatedEntryGroup;
    expect(group.aggregationType).toBe("tool_burst");
    expect(group.entries).toHaveLength(5);
  });

  it("T2: empty agent_message between tools is dropped, tools stay one unit", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkMessageEntry("m1", "   "),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries).toHaveLength(2);
  });

  it("T3: non-empty agent_message splits into two units with message between", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd"),
      mkMessageEntry("m1", "好的"),
      mkCmdEntry("c3", "uname"),
      mkCmdEntry("c4", "date"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(3);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries).toHaveLength(2);
    expect((result[1] as SessionDisplayEntry).id).toBe("m1");
    expect(isToolGroup(result[2])).toBe(true);
    expect((result[2] as AggregatedEntryGroup).entries).toHaveLength(2);
  });

  it("T4: single tool entry is represented as a one-item tool group", () => {
    const entries = [mkCmdEntry("c1", "ls")];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c1"]);
  });

  it("T4b: single dynamic tool entry is represented as a one-item tool group", () => {
    const entries = [mkDynamicToolEntry("companion", "companion_respond")];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["companion"]);
  });

  it("keeps Companion subagent dispatch out of ordinary tool bursts", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkCompanionSubagentEntry("subagent-1"),
      mkCmdEntry("c2", "pwd"),
    ];

    const result = aggregateEntries(entries);

    expect(result).toHaveLength(3);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[1] as SessionDisplayEntry).id).toBe("subagent-1");
    expect(isToolGroup(result[2])).toBe(true);
  });

  it("T5: turn boundaries are neutral, so tool bursts can span provider turns", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd"),
      mkTurnCompleted("tc1"),
      mkTurnStarted("ts2"),
      mkCmdEntry("c3", "uname"),
      mkCmdEntry("c4", "date"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual([
      "c1",
      "c2",
      "c3",
      "c4",
    ]);
  });

  it("T6: mixed tool kinds fold together", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkFileChangeEntry("f1", "a.ts"),
      mkMcpEntry("m1"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries).toHaveLength(3);
  });

  it("T7: pending approval entry preserved inside unit", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd", { isPendingApproval: true }),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    const group = result[0] as AggregatedEntryGroup;
    expect(group.entries.some((e) => e.isPendingApproval === true)).toBe(true);
  });

  it("T8: thinking entries stay individual; only tools merge", () => {
    // reasoning 同 itemId 已在 useSessionStream 层累积为单条，
    // aggregateEntries 这层无需对 thinking 再做聚合。
    const entries = [
      mkReasoningEntry("r1"),
      mkReasoningEntry("r2"),
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(3);
    expect((result[0] as SessionDisplayEntry).id).toBe("r1");
    expect((result[1] as SessionDisplayEntry).id).toBe("r2");
    expect(isToolGroup(result[2])).toBe(true);
    expect((result[2] as AggregatedEntryGroup).entries).toHaveLength(2);
  });

  it("T9: thinking → tool → thinking sequence flushes correctly", () => {
    const entries = [
      mkReasoningEntry("r1"),
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd"),
      mkReasoningEntry("r2"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(3);
    expect((result[0] as SessionDisplayEntry).id).toBe("r1");
    expect(isToolGroup(result[1])).toBe(true);
    expect((result[2] as SessionDisplayEntry).id).toBe("r2");
  });

  it("T10: turn boundary inside a flowing tool sequence does not flush the unit", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkTurnCompleted("tc"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c1", "c2"]);
  });

  it("T11: empty entries array returns empty array", () => {
    expect(aggregateEntries([])).toEqual([]);
  });

  it("T12: streaming message — empty frame keeps fold, non-empty frame breaks it", () => {
    const cmd1 = mkCmdEntry("c1", "ls");
    const cmd2 = mkCmdEntry("c2", "pwd");

    const frame1 = aggregateEntries([cmd1, mkMessageEntry("m1", ""), cmd2]);
    expect(frame1).toHaveLength(1);
    expect(isToolGroup(frame1[0])).toBe(true);
    expect((frame1[0] as AggregatedEntryGroup).entries).toHaveLength(2);

    const frame2 = aggregateEntries([cmd1, mkMessageEntry("m1", "进度更新"), cmd2]);
    expect(frame2).toHaveLength(3);
    expect((frame2[0] as SessionDisplayEntry).id).toBe("c1");
    expect((frame2[1] as SessionDisplayEntry).id).toBe("m1");
    expect((frame2[2] as SessionDisplayEntry).id).toBe("c2");
  });

  it("T13: silent platform lifecycle events do not split tool bursts", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkSilentHookTraceEntry("h1"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c1", "c2"]);
  });

  it("T14: visible system events remain hard boundaries", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkSystemMessageEntry("s1", "需要用户确认"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(3);
    expect((result[0] as SessionDisplayEntry).id).toBe("c1");
    expect((result[1] as SessionDisplayEntry).id).toBe("s1");
    expect((result[2] as SessionDisplayEntry).id).toBe("c2");
  });

  it("T15: context_frame is a hard boundary — tools do not merge across it, single CTX flattened", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkContextFrameEntry("ctx1"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    // CTX 截断工具 burst；两侧单工具也保持统一的 tool group 外壳。
    const toolGroups = result.filter(isToolGroup) as AggregatedEntryGroup[];
    expect(toolGroups).toHaveLength(2);
    expect(toolGroups[0]!.entries.map((entry) => entry.id)).toEqual(["c1"]);
    expect(toolGroups[1]!.entries.map((entry) => entry.id)).toEqual(["c2"]);
    expect(
      result.some((item) => (item as SessionDisplayEntry)?.id === "ctx1"),
    ).toBe(true);
    expect(result.map((item) => (item as SessionDisplayEntry).id)).toEqual(["c1", "ctx1", "c2"]);
  });

  it("T16: agent message stays as hard boundary across context_frame", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkContextFrameEntry("ctx1"),
      mkMessageEntry("m1", "进度更新"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    // 非空 agent message 是 hard boundary，c1 和 c2 必须分裂为两个单项 tool group。
    const toolGroups = result.filter(isToolGroup) as AggregatedEntryGroup[];
    expect(toolGroups).toHaveLength(2);
    expect(toolGroups.map((group) => group.entries.map((entry) => entry.id))).toEqual([["c1"], ["c2"]]);
  });

  it("T17: consecutive context_frames fold into one CTX group and split surrounding tool bursts", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkContextFrameEntry("ctx1"),
      mkContextFrameEntry("ctx2"),
      mkContextFrameEntry("ctx3"),
      mkCmdEntry("c2", "pwd"),
      mkCmdEntry("c3", "echo"),
    ];
    const result = aggregateEntries(entries);
    // CTX 内部合并为一个 group；CTX 前后的工具不能跨上下文合并
    const toolGroups = result.filter(isToolGroup) as AggregatedEntryGroup[];
    expect(toolGroups).toHaveLength(2);
    expect(toolGroups[0]!.entries.map((e) => e.id)).toEqual(["c1"]);
    expect(toolGroups[1]!.entries.map((e) => e.id)).toEqual(["c2", "c3"]);
    const ctxGroups = result.filter(isContextFrameGroup);
    expect(ctxGroups).toHaveLength(1);
    expect((result[0] as SessionDisplayEntry).id).toBe("c1");
  });

  it("T18: real-world scenario — capability CTX splits tool bursts", () => {
    // 模拟用户截图: mounts_list → ctx × 3 → Read → workspace_module_operate
    const entries = [
      mkCmdEntry("mounts_list", "mounts"),
      mkContextFrameEntry("ctx1"),
      mkContextFrameEntry("ctx2"),
      mkContextFrameEntry("ctx3"),
      mkCmdEntry("read", "Read"),
      mkCmdEntry("workspace_module_operate", "workspace_module"),
    ];
    const result = aggregateEntries(entries);
    const toolGroups = result.filter(isToolGroup) as AggregatedEntryGroup[];
    expect(toolGroups).toHaveLength(2);
    expect(toolGroups[0]!.entries.map((e) => e.id)).toEqual(["mounts_list"]);
    expect(toolGroups[1]!.entries.map((e) => e.id)).toEqual([
      "read",
      "workspace_module_operate",
    ]);
    expect((result[0] as SessionDisplayEntry).id).toBe("mounts_list");
    expect(result.filter(isContextFrameGroup)).toHaveLength(1);
  });

  it("T19: observed hook trace is silent and does not split tool bursts", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkObservedHookTraceEntry("h-observed"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c1", "c2"]);
  });

  it("T20: empty thinking stays silent and does not split tool bursts", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkReasoningEntryWithText("r-empty", "   "),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c1", "c2"]);
  });

  it("T21: in-progress tools enter the burst immediately and keep their live state", () => {
    const activeFrame = aggregateEntries([
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd"),
      mkCmdEntry("c3", "sleep 1", {
        status: "inProgress",
        isPendingApproval: true,
        output: "running output",
      }),
    ]);
    expect(activeFrame).toHaveLength(1);
    expect(isToolGroup(activeFrame[0])).toBe(true);
    const activeGroup = activeFrame[0] as AggregatedEntryGroup;
    expect(activeGroup.entries.map((entry) => entry.id)).toEqual(["c1", "c2", "c3"]);
    const runningEntry = activeGroup.entries[2]!;
    expect(runningEntry.isPendingApproval).toBe(true);
    if (runningEntry.event.type !== "item_started") {
      throw new Error("expected running entry to be an item_started event");
    }
    const runningItem = runningEntry.event.payload.item;
    if (runningItem.type !== "commandExecution") {
      throw new Error("expected running entry to be a commandExecution item");
    }
    expect(runningItem.status).toBe("inProgress");
    expect(runningItem.aggregatedOutput).toBe("running output");

    const completedFrame = aggregateEntries([
      mkCmdEntry("c1", "ls"),
      mkCmdEntry("c2", "pwd"),
      mkCmdEntry("c3", "sleep 1", { status: "completed" }),
    ]);
    expect(completedFrame).toHaveLength(1);
    expect(isToolGroup(completedFrame[0])).toBe(true);
    expect((completedFrame[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual([
      "c1",
      "c2",
      "c3",
    ]);
  });

  it("T21b: item_updated tool entries also enter the burst after live progress wins freshness", () => {
    const activeFrame = aggregateEntries([
      mkCmdEntry("c1", "ls"),
      mkCmdUpdatedEntry("c2", "sleep 1", {
        status: "inProgress",
        isPendingApproval: true,
        output: "running from item_updated",
      }),
    ]);

    expect(activeFrame).toHaveLength(1);
    expect(isToolGroup(activeFrame[0])).toBe(true);
    const activeGroup = activeFrame[0] as AggregatedEntryGroup;
    expect(activeGroup.entries.map((entry) => entry.id)).toEqual(["c1", "c2"]);
    const runningEntry = activeGroup.entries[1]!;
    expect(runningEntry.isPendingApproval).toBe(true);
    expect(runningEntry.event.type).toBe("item_updated");
    if (runningEntry.event.type !== "item_updated") {
      throw new Error("expected running entry to be an item_updated event");
    }
    const runningItem = runningEntry.event.payload.item;
    if (runningItem.type !== "commandExecution") {
      throw new Error("expected running entry to be a commandExecution item");
    }
    expect(runningItem.status).toBe("inProgress");
    expect(runningItem.aggregatedOutput).toBe("running from item_updated");
  });

  it("T22: bounded output tools stay isolated while using the tool group shell", () => {
    const entries = [
      mkCmdEntry("c0", "whoami"),
      mkCmdEntry("c00", "hostname"),
      mkCmdEntry("c1", "large-output", {
        output: "[tool result truncated]\nlifecycle_path: lifecycle://session/tool-results/turn_001/tool_001/result.txt\npolicy: head_tail\n\npreview",
      }),
      mkCmdEntry("c2", "pwd"),
      mkCmdEntry("c3", "date"),
    ];

    const result = aggregateEntries(entries);

    expect(result).toHaveLength(3);
    expect(isToolGroup(result[0])).toBe(true);
    expect((result[0] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c0", "c00"]);
    expect(isToolGroup(result[1])).toBe(true);
    expect((result[1] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c1"]);
    expect(isToolGroup(result[2])).toBe(true);
    expect((result[2] as AggregatedEntryGroup).entries.map((entry) => entry.id)).toEqual(["c2", "c3"]);
  });

  it("T23: provider retry status is only visible in verbose mode and does not create turn activity", () => {
    const statusEntry = mkProviderStatusEntry("provider-status");
    const aggregated = aggregateEntries([statusEntry]);
    expect(aggregated).toHaveLength(0);

    const verboseAggregated = aggregateEntries([statusEntry], { includeVerboseEvents: true });
    expect(verboseAggregated).toHaveLength(1);
    expect((verboseAggregated[0] as SessionDisplayEntry).id).toBe("provider-status");

    const segments = segmentByTurn([], [
      rawTurnStarted(1),
      rawProviderAttemptStatus(2, {
        phase: "retrying",
        attempt: 2,
        max_attempts: 3,
        will_retry: true,
      }),
    ]);

    expect(segments).toHaveLength(0);
  });

  it("T24: active turn without provider waiting does not create turn-level thinking", () => {
    const segments = segmentByTurn([], [rawTurnStarted(1)]);

    expect(segments).toHaveLength(0);
  });

  it("T25: durationMs is read from completed, failed and interrupted terminal facts", () => {
    const completed = segmentByTurn([mkMessageEntry("completed-message", "done")], [
      rawTurnCompleted(1, "completed", 12_000),
    ]);
    const failed = segmentByTurn([mkMessageEntry("failed-message", "partial")], [
      rawTurnTerminal(1, "turn_failed", 34_000),
    ]);
    const interrupted = segmentByTurn([mkMessageEntry("interrupted-message", "partial")], [
      rawTurnTerminal(1, "turn_interrupted", 56_000),
    ]);

    expect(completed[0]?.status).toBe("completed");
    expect(completed[0]?.durationMs).toBe(12_000);
    expect(failed[0]?.status).toBe("failed");
    expect(failed[0]?.durationMs).toBe(34_000);
    expect(interrupted[0]?.status).toBe("interrupted");
    expect(interrupted[0]?.durationMs).toBe(56_000);
  });

  it("T25b: active turn exposes startedAtMs for live turn elapsed UI", () => {
    const segments = segmentByTurn([mkMessageEntry("active-message", "working")], [
      rawTurnStarted(1, "u1", 1_700_000_000),
    ]);

    expect(segments[0]?.status).toBe("active");
    expect(segments[0]?.startedAtMs).toBe(1_700_000_000_000);
  });

  it("T25c: projected AgentRun feed messages are stable completed turn segments", () => {
    const projected = {
      ...mkMessageEntry("projected-message", "inherited answer"),
      projectedTranscriptStable: true,
    };

    const segments = segmentByTurn([projected], []);

    expect(segments).toHaveLength(1);
    expect(segments[0]?.status).toBe("completed");
    expect(segments[0]?.finalOutput).toBe(projected);
  });

  it("T25d: AgentRun controlled active turn does not mark unrelated turn items active", () => {
    const segments = segmentByTurn(
      [mkMessageEntry("fork-marker", "已从父会话分叉出当前会话")],
      [],
      "turn-active",
    );

    expect(segments).toHaveLength(1);
    expect(segments[0]?.status).toBe("completed");
  });

  it("T26: retry exhausted does not create a status-only turn segment", () => {
    const segments = segmentByTurn([], [
      rawTurnStarted(1),
      rawProviderAttemptStatus(2, {
        phase: "failed",
        attempt: 3,
        max_attempts: 3,
        will_retry: false,
      }),
    ]);

    expect(segments).toHaveLength(0);
  });

  it("T27: provider waiting creates a streaming thinking card without reasoning text", () => {
    const display = mergeThinkingIntoDisplayItems([], providerWaitingSeqs(2));

    expect(display).toHaveLength(1);
    expect(isThinkingGroup(display[0])).toBe(true);
    const group = display[0] as AggregatedThinkingGroup;
    expect(group.turnId).toBe("u1");
    expect(group.entries).toHaveLength(0);
    expect(group.isStreamingThinking).toBe(true);
  });

  it("T27b: provider waiting placeholder is anchored after the user input", () => {
    const user = mkUserInputEntry("u-input", "请帮我看一下", 1);
    const display = mergeThinkingIntoDisplayItems(aggregateEntries([user]), providerWaitingSeqs(2));

    expect(display).toHaveLength(2);
    expect((display[0] as SessionDisplayEntry).id).toBe("u-input");
    expect(isThinkingGroup(display[1])).toBe(true);
    const group = display[1] as AggregatedThinkingGroup;
    expect(group.entries).toHaveLength(0);
    expect(group.isStreamingThinking).toBe(true);
  });

  it("T27c: provider waiting placeholder follows the latest visible item in the turn", () => {
    const user = mkUserInputEntry("u-input", "请帮我看一下", 1);
    const tool = mkCmdEntry("c1", "ls", { status: "inProgress" });
    const display = mergeThinkingIntoDisplayItems(aggregateEntries([user, tool]), providerWaitingSeqs(2));

    expect(display).toHaveLength(3);
    expect((display[0] as SessionDisplayEntry).id).toBe("u-input");
    expect((display[1] as SessionDisplayEntry).id).toBe("c1");
    expect(isThinkingGroup(display[2])).toBe(true);
    const group = display[2] as AggregatedThinkingGroup;
    expect(group.entries).toHaveLength(0);
    expect(group.isStreamingThinking).toBe(true);
  });

  it("T28: provider waiting merges with reasoning into one streaming thinking card", () => {
    const reasoning = mkReasoningEntryWithText("r1", "分析中");
    const display = mergeThinkingIntoDisplayItems(aggregateEntries([reasoning]), providerWaitingSeqs(1));

    expect(display).toHaveLength(1);
    expect(isThinkingGroup(display[0])).toBe(true);
    const group = display[0] as AggregatedThinkingGroup;
    expect(group.entries.map((entry) => entry.id)).toEqual(["r1"]);
    expect(group.isStreamingThinking).toBe(true);
  });

  it("T29: agent message removes empty provider thinking placeholder", () => {
    const agent = mkMessageEntry("m1", "正式输出");
    const display = mergeThinkingIntoDisplayItems(aggregateEntries([agent]), providerWaitingSeqs(1));

    expect(display).toHaveLength(1);
    expect(isThinkingGroup(display[0])).toBe(false);
    expect((display[0] as SessionDisplayEntry).id).toBe("m1");
  });

  it("T30: reasoning stays as historical thinking once agent message arrives", () => {
    const reasoning = mkReasoningEntryWithText("r1", "分析中");
    const agent = mkMessageEntry("m1", "正式输出");
    const display = mergeThinkingIntoDisplayItems(aggregateEntries([reasoning, agent]), providerWaitingSeqs(1));

    expect(display).toHaveLength(2);
    expect(isThinkingGroup(display[0])).toBe(true);
    const group = display[0] as AggregatedThinkingGroup;
    expect(group.entries.map((entry) => entry.id)).toEqual(["r1"]);
    expect(group.isStreamingThinking).toBe(false);
    expect((display[1] as SessionDisplayEntry).id).toBe("m1");
  });

  it("T30b: historical reasoning stays after user input and before the final agent message", () => {
    const user = mkUserInputEntry("u-input", "请帮我看一下", 1);
    const reasoning = mkReasoningEntryWithText("r1", "分析中");
    const agent = mkMessageEntry("m1", "正式输出");
    const display = mergeThinkingIntoDisplayItems(
      aggregateEntries([user, reasoning, agent]),
      providerWaitingSeqs(2),
    );

    expect(display).toHaveLength(3);
    expect((display[0] as SessionDisplayEntry).id).toBe("u-input");
    expect(isThinkingGroup(display[1])).toBe(true);
    const group = display[1] as AggregatedThinkingGroup;
    expect(group.entries.map((entry) => entry.id)).toEqual(["r1"]);
    expect(group.isStreamingThinking).toBe(false);
    expect((display[2] as SessionDisplayEntry).id).toBe("m1");
  });

  it("T31: user message remains outside completed assistant turn segment", () => {
    const user = mkUserInputEntry("u-input", "hello", 1);
    const segments = segmentByTurn([user], [
      rawTurnStarted(1),
      rawTurnCompleted(2, "completed", 1_000),
    ]);

    expect(segments[0]?.turnId).toBeNull();
    expect(segments[0]?.items).toHaveLength(1);
    expect((segments[0]?.items[0] as SessionDisplayEntry | undefined)?.event.type).toBe("user_input_submitted");
    expect(segments.some((segment) =>
      segment.status === "completed" &&
      segment.items.some((item) => (item as SessionDisplayEntry).event?.type === "user_input_submitted")
    )).toBe(false);
  });

  it("T32: later provider status clears waiting thinking placeholder", () => {
    const display = mergeThinkingIntoDisplayItems([], new Map());

    expect(display).toHaveLength(0);
  });

  it("T32b: turn terminal clears empty provider waiting placeholder even without provider succeeded", () => {
    const user = mkUserInputEntry("u-input", "请帮我看一下", 1);
    const display = mergeThinkingIntoDisplayItems(aggregateEntries([user]), new Map());

    expect(display).toHaveLength(1);
    expect((display[0] as SessionDisplayEntry).id).toBe("u-input");
  });

  it("T33: thinking merge keeps display order instead of sorting ephemeral and durable seq together", () => {
    const tool = mkCmdEntry("c1", "ls");
    tool.eventSeq = 100;
    const reasoning = mkReasoningEntryWithText("r1", "分析中");
    reasoning.eventSeq = 1;
    const agent = mkMessageEntry("m1", "正式输出");
    agent.eventSeq = 101;

    const display = mergeThinkingIntoDisplayItems([tool, reasoning, agent], new Map());

    expect(display).toHaveLength(3);
    expect((display[0] as SessionDisplayEntry).id).toBe("c1");
    expect(isThinkingGroup(display[1])).toBe(true);
    expect(((display[1] as AggregatedThinkingGroup).entries[0] as SessionDisplayEntry).id).toBe("r1");
    expect((display[2] as SessionDisplayEntry).id).toBe("m1");
  });
});

import { describe, expect, it } from "vitest";
import { aggregateEntries } from "./useSessionFeed";
import type {
  SessionDisplayEntry,
  AggregatedEntryGroup,
} from "./types";
import type { BackboneEvent, ThreadItem } from "../../../generated/backbone-protocol";

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

function mkCmdEntry(id: string, command: string, opts?: { isPendingApproval?: boolean }): SessionDisplayEntry {
  const item = {
    type: "commandExecution",
    id,
    command,
    cwd: "/tmp",
    processId: null,
    source: "agent",
    status: "completed",
    commandActions: [],
    aggregatedOutput: null,
    exitCode: 0,
    durationMs: 100,
  } as unknown as ThreadItem;
  const event: BackboneEvent = {
    type: "item_started",
    payload: { item, threadId: "t1", turnId: "u1", startedAtMs: 0 },
  };
  return asEntry(id, event, { isPendingApproval: opts?.isPendingApproval });
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

function mkMessageEntry(id: string, text: string): SessionDisplayEntry {
  const event: BackboneEvent = {
    type: "agent_message_delta",
    payload: { threadId: "t1", turnId: "u1", itemId: id, delta: text },
  };
  return asEntry(id, event, { accumulatedText: text });
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
  return asEntry(id, event, { accumulatedText: "..." });
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

  it("T4: single tool entry not folded — flattened to entry", () => {
    const entries = [mkCmdEntry("c1", "ls")];
    const result = aggregateEntries(entries);
    expect(result).toHaveLength(1);
    expect(isToolGroup(result[0])).toBe(false);
    expect((result[0] as SessionDisplayEntry).id).toBe("c1");
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

  it("T15: context_frame is a soft boundary — tools merge across it, single CTX flattened", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkContextFrameEntry("ctx1"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    // tool burst 合并为一组（c1+c2），单 CTX 被扁平化为单 entry（不成 group）
    const toolGroups = result.filter(isToolGroup) as AggregatedEntryGroup[];
    expect(toolGroups).toHaveLength(1);
    expect(toolGroups[0]!.entries.map((e) => e.id)).toEqual(["c1", "c2"]);
    expect(
      result.some((item) => (item as SessionDisplayEntry)?.id === "ctx1"),
    ).toBe(true);
  });

  it("T16: agent message stays as hard boundary across context_frame", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkContextFrameEntry("ctx1"),
      mkMessageEntry("m1", "进度更新"),
      mkCmdEntry("c2", "pwd"),
    ];
    const result = aggregateEntries(entries);
    // 非空 agent message 是 hard boundary，c1 和 c2 必须分裂
    const toolGroups = result.filter(isToolGroup);
    expect(toolGroups).toHaveLength(0);  // 各 1 条工具不会折成 group
    const cmdEntries = result.filter(
      (item) => (item as SessionDisplayEntry)?.id === "c1" || (item as SessionDisplayEntry)?.id === "c2"
    );
    expect(cmdEntries).toHaveLength(2);
  });

  it("T17: consecutive context_frames fold into one CTX group, tools merge separately", () => {
    const entries = [
      mkCmdEntry("c1", "ls"),
      mkContextFrameEntry("ctx1"),
      mkContextFrameEntry("ctx2"),
      mkContextFrameEntry("ctx3"),
      mkCmdEntry("c2", "pwd"),
      mkCmdEntry("c3", "echo"),
    ];
    const result = aggregateEntries(entries);
    // CTX 内部合并为一个 group；tool 自己合并；两类互不混
    const toolGroups = result.filter(isToolGroup) as AggregatedEntryGroup[];
    expect(toolGroups).toHaveLength(1);
    expect(toolGroups[0]!.entries.map((e) => e.id)).toEqual(["c1", "c2", "c3"]);
    const ctxGroups = result.filter(isContextFrameGroup);
    expect(ctxGroups).toHaveLength(1);
  });

  it("T18: real-world scenario — tools, ctx, more tools all merge into one burst", () => {
    // 模拟用户截图: mounts_list → ctx × 3 → Read → canvas_start
    const entries = [
      mkCmdEntry("mounts_list", "mounts"),
      mkContextFrameEntry("ctx1"),
      mkContextFrameEntry("ctx2"),
      mkContextFrameEntry("ctx3"),
      mkCmdEntry("read", "Read"),
      mkCmdEntry("canvas_start", "canvas"),
    ];
    const result = aggregateEntries(entries);
    const toolGroups = result.filter(isToolGroup) as AggregatedEntryGroup[];
    expect(toolGroups).toHaveLength(1);
    expect(toolGroups[0]!.entries.map((e) => e.id)).toEqual([
      "mounts_list",
      "read",
      "canvas_start",
    ]);
  });
});

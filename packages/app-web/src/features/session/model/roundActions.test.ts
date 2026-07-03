import { describe, expect, it } from "vitest";
import type { SessionDisplayEntry } from "./types";
import type { TurnSegment } from "./useSessionFeed";
import {
  buildRoundActionModel,
  lastAgentReplyText,
} from "./roundActions";

function agentEntry(params: {
  id: string;
  text: string;
  turnId: string;
  entryIndex: number;
}): SessionDisplayEntry {
  return {
    id: params.id,
    sessionId: "session-1",
    timestamp: 1,
    eventSeq: params.entryIndex + 1,
    event: {
      type: "agent_message_delta",
      payload: {
        threadId: "thread-1",
        turnId: params.turnId,
        itemId: params.id,
        delta: params.text,
      },
    },
    turnId: params.turnId,
    entryIndex: params.entryIndex,
    accumulatedText: params.text,
  };
}

function segment(items: SessionDisplayEntry[], status: TurnSegment["status"] = "completed"): TurnSegment {
  return {
    turnId: "turn-1",
    status,
    items,
    finalOutput: items[items.length - 1] ?? null,
  };
}

describe("round action model", () => {
  it("copies only the current round last agent reply readable text", () => {
    const first = agentEntry({
      id: "assistant-1",
      text: "intermediate answer",
      turnId: "turn-1",
      entryIndex: 1,
    });
    const last = agentEntry({
      id: "assistant-2",
      text: "final answer\nwith detail",
      turnId: "turn-1",
      entryIndex: 3,
    });

    expect(lastAgentReplyText(segment([first, last]))).toBe("final answer\nwith detail");
  });

  it("builds a stable fork point from a completed round final MessageRef", () => {
    const model = buildRoundActionModel(segment([
      agentEntry({
        id: "assistant-final",
        text: "done",
        turnId: "turn-42",
        entryIndex: 7,
      }),
    ]));

    expect(model.forkFromHere).toMatchObject({
      enabled: true,
      forkPointRef: { turn_id: "turn-42", entry_index: 7 },
    });
  });

  it("disables fork for active or incomplete boundaries with a reason", () => {
    const model = buildRoundActionModel(segment([
      agentEntry({
        id: "assistant-streaming",
        text: "still running",
        turnId: "turn-1",
        entryIndex: 2,
      }),
    ], "active"));

    expect(model.forkFromHere.enabled).toBe(false);
    expect(model.forkFromHere.disabledReason).toContain("仍在运行");
  });
});

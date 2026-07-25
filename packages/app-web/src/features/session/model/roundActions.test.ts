import { describe, expect, it } from "vitest";
import type { SessionDisplayEntry } from "./types";
import type { TurnSegment } from "./useSessionFeed";
import { buildRoundActionModel, lastAgentReplyText } from "./roundActions";

function agentEntry(text: string, entryIndex: number): SessionDisplayEntry {
  return {
    id: `assistant-${entryIndex}`,
    sessionId: "thread-1",
    timestamp: entryIndex,
    eventSeq: entryIndex,
    turnId: "turn-1",
    entryIndex,
    accumulatedText: text,
    event: {
      type: "agent_message_delta",
      payload: {
        threadId: "thread-1",
        turnId: "turn-1",
        itemId: `assistant-${entryIndex}`,
        delta: text,
      },
    },
  };
}

function segment(
  entries: SessionDisplayEntry[],
  status: TurnSegment["status"] = "completed",
): TurnSegment {
  return {
    turnId: "turn-1",
    status,
    items: entries,
    finalOutput: entries.at(-1) ?? null,
  };
}

describe("round action model", () => {
  it("copies the canonical final agent reply", () => {
    expect(lastAgentReplyText(segment([
      agentEntry("intermediate", 1),
      agentEntry("final answer\nwith detail", 3),
    ]))).toBe("final answer\nwith detail");
  });

  it("uses the canonical turn and entry coordinates as the fork point", () => {
    const model = buildRoundActionModel(segment([agentEntry("done", 7)]));

    expect(model.forkFromHere).toMatchObject({
      enabled: true,
      forkPointRef: { turn_id: "turn-1", entry_index: 7 },
    });
  });

  it("disables fork while the canonical turn is active", () => {
    const model = buildRoundActionModel(segment([agentEntry("still running", 2)], "active"));

    expect(model.forkFromHere.enabled).toBe(false);
    expect(model.forkFromHere.disabledReason).toContain("仍在运行");
  });
});

import { describe, expect, it } from "vitest";
import type {
  AgentRunRuntimeItem,
  AgentRunRuntimeTurnSegment,
} from "../../agent-run-runtime";
import {
  buildRoundActionModel,
  lastAgentReplyText,
} from "./roundActions";

function agentEntry(params: {
  id: string;
  text: string;
  turnId: string;
  entryIndex: number;
}): AgentRunRuntimeItem {
  return {
    id: params.id,
    turn_id: params.turnId,
    status: "completed",
    presentation: {
      body: {
        kind: "agent_message",
        content: [{ kind: "text", text: params.text }],
        phase: null,
      },
      started_at_ms: BigInt(params.entryIndex),
      updated_at_ms: BigInt(params.entryIndex + 1),
      terminal: {
        outcome: "completed",
        completed_at_ms: BigInt(params.entryIndex + 1),
        duration_ms: 1n,
        process_exit: null,
        error: null,
      },
      body_digest: `sha256:${params.id}:body`,
      presentation_digest: `sha256:${params.id}:presentation`,
    },
  };
}

function segment(
  items: AgentRunRuntimeItem[],
  status: AgentRunRuntimeTurnSegment["status"] = "completed",
): AgentRunRuntimeTurnSegment {
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

  it("does not manufacture a MessageRef from Runtime item identity", () => {
    const model = buildRoundActionModel(segment([
      agentEntry({
        id: "assistant-final",
        text: "done",
        turnId: "turn-42",
        entryIndex: 7,
      }),
    ]));

    expect(model.forkFromHere.enabled).toBe(false);
    expect(model.forkFromHere.forkPointRef).toBeUndefined();
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

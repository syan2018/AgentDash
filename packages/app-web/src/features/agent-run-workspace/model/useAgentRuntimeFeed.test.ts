import { describe, expect, it } from "vitest";

import type { RuntimeEventEnvelope } from "../../../generated/agent-runtime-contracts";
import {
  applyRuntimeEvent,
  runtimeEventRequestsRuntimeInspectRefresh,
} from "./useAgentRuntimeFeed";

function envelope(event: RuntimeEventEnvelope["event"], sequence: bigint): RuntimeEventEnvelope {
  return {
    thread_id: "thread-1",
    sequence,
    revision: sequence,
    event,
  };
}

describe("Agent Runtime interaction feed", () => {
  it("retains interaction identity and converges terminal state", () => {
    const requested = applyRuntimeEvent([], envelope({
      kind: "interaction_requested",
      turn_id: "turn-1",
      item_id: "item-1",
      interaction_id: "interaction-1",
      interaction_kind: "permission_approval",
      prompt: "Allow permission?",
    }, 1n), new Set());

    expect(requested[0]?.interaction).toEqual({
      interaction_id: "interaction-1",
      interaction_kind: "permission_approval",
      terminal: null,
    });
    expect(runtimeEventRequestsRuntimeInspectRefresh(envelope({
      kind: "interaction_requested",
      turn_id: "turn-1",
      item_id: "item-1",
      interaction_id: "interaction-1",
      interaction_kind: "permission_approval",
      prompt: "Allow permission?",
    }, 1n))).toBe(true);
    expect(runtimeEventRequestsRuntimeInspectRefresh(envelope({
      kind: "turn_started",
      turn_id: "turn-1",
    }, 2n))).toBe(true);

    const resolved = applyRuntimeEvent(requested, envelope({
      kind: "interaction_terminal",
      turn_id: "turn-1",
      interaction_id: "interaction-1",
      terminal: "resolved",
    }, 3n), new Set());

    expect(runtimeEventRequestsRuntimeInspectRefresh(envelope({
      kind: "interaction_terminal",
      turn_id: "turn-1",
      interaction_id: "interaction-1",
      terminal: "resolved",
    }, 4n))).toBe(true);

    expect(resolved[0]?.interaction?.terminal).toBe("resolved");
    expect(resolved[0]?.status).toBe("completed");
  });
});

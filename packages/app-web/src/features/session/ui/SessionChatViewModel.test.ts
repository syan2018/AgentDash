import { describe, expect, it } from "vitest";

import {
  liveSideEffectCursor,
  rawEventsBelongToRuntimeStreamTarget,
} from "./SessionChatViewModel";

describe("SessionChatView live side-effect cursor", () => {
  it("starts after the hydrated snapshot baseline", () => {
    expect(liveSideEffectCursor(null, 12)).toBe(12);
  });

  it("keeps an already advanced live cursor", () => {
    expect(liveSideEffectCursor(15, 12)).toBe(15);
  });

  it("advances past records rehydrated by a gap snapshot reload", () => {
    expect(liveSideEffectCursor(15, 31)).toBe(31);
  });
});

describe("SessionChatView Runtime feed target fence", () => {
  it("uses the Product AgentRun target instead of rewriting source session identity", () => {
    expect(
      rawEventsBelongToRuntimeStreamTarget({
        rawEvents: [
          {
            session_id: "codex-source-thread",
          } as never,
        ],
        agentRunTarget: { runId: "run-1", agentId: "agent-1" },
        boundTargetKey: "run-1:agent-1",
      }),
    ).toBe(true);
  });

  it("rejects a stale snapshot while a new AgentRun target is connecting", () => {
    expect(
      rawEventsBelongToRuntimeStreamTarget({
        rawEvents: [
          {
            session_id: "native-source-thread",
          } as never,
        ],
        agentRunTarget: { runId: "run-2", agentId: "agent-2" },
        boundTargetKey: "run-1:agent-1",
      }),
    ).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import { runtimeEventInvalidatesInspect, shouldInvalidateRuntimeInspect } from "./useSessionStream";

function event(kind: Parameters<typeof runtimeEventInvalidatesInspect>[0]["kind"]): Pick<Parameters<typeof runtimeEventInvalidatesInspect>[0], "kind"> {
  return { kind };
}

describe("Runtime inspect invalidation", () => {
  it.each([
    "turn_started", "turn_terminal", "interaction_requested", "interaction_terminal",
    "binding_established", "binding_lost", "binding_reestablished", "thread_status_changed",
  ] as const)("invalidates for %s", (kind) => {
    expect(runtimeEventInvalidatesInspect(event(kind))).toBe(true);
  });

  it("does not invalidate for presentation-only deltas", () => {
    expect(runtimeEventInvalidatesInspect(event("conversation_delta"))).toBe(false);
  });

  it("rejects duplicate/history events and stale target callbacks", () => {
    const base = { event: event("turn_terminal"), durableCursor: 12, historyBoundary: 11, streamTargetKey: "run-a:agent-a", currentTargetKey: "run-a:agent-a", accepted: true };
    expect(shouldInvalidateRuntimeInspect(base)).toBe(true);
    expect(shouldInvalidateRuntimeInspect({ ...base, accepted: false })).toBe(false);
    expect(shouldInvalidateRuntimeInspect({ ...base, durableCursor: 11 })).toBe(false);
    expect(shouldInvalidateRuntimeInspect({ ...base, currentTargetKey: "run-b:agent-b" })).toBe(false);
  });
});

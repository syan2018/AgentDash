import { describe, expect, it } from "vitest";

import {
  dispatchLiveSessionEvents,
  liveSideEffectCursor,
  rawEventsBelongToRuntimeStreamTarget,
  resolveSessionInitialSubmit,
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

describe("SessionChatView canonical live event dispatch", () => {
  it("dispatches every event after the hydration boundary in sequence order", () => {
    const received: string[] = [];
    const event = (eventSeq: number, type: string) => ({
      event_seq: eventSeq,
      notification: { event: { type } },
    }) as never;

    const cursor = dispatchLiveSessionEvents(
      [event(14, "turn_completed"), event(11, "item_updated"), event(12, "platform")],
      null,
      11,
      (value) => received.push(value.type),
    );

    expect(received).toEqual(["platform", "turn_completed"]);
    expect(cursor).toBe(14);
  });

  it("does not replay hydrated or already dispatched events", () => {
    const received: string[] = [];
    const cursor = dispatchLiveSessionEvents(
      [{
        event_seq: 20,
        notification: { event: { type: "platform" } },
      } as never],
      20,
      18,
      (value) => received.push(value.type),
    );

    expect(received).toEqual([]);
    expect(cursor).toBe(20);
  });
});

describe("SessionChatView Draft transition", () => {
  const initialSubmit = {
    transitionId: "create-command",
    intent: { prompt: "hello" },
  };
  const commands = [{
    command_id: "turn-start",
    kind: "turn_start",
    enabled: true,
    requires_input: true,
    executor_config_policy: "optional" as const,
  }];

  it("waits for the target history/live baseline before submitting", () => {
    expect(resolveSessionInitialSubmit({
      initialSubmit,
      isConnected: true,
      historyReplayBoundarySeq: null,
      isSending: false,
      commands,
      primaryCommandId: "turn-start",
    })).toBeNull();
  });

  it("routes the preserved Draft intent through the target primary command", () => {
    expect(resolveSessionInitialSubmit({
      initialSubmit,
      isConnected: true,
      historyReplayBoundarySeq: 0,
      isSending: false,
      commands,
      primaryCommandId: "turn-start",
    })).toEqual({
      command_id: "turn-start",
      prompt: "hello",
    });
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

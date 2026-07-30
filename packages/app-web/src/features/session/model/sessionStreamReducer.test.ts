import { describe, expect, it } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "./types";
import {
  createInitialStreamState,
  reduceStreamState,
} from "./sessionStreamReducer";

function event(
  eventSeq: number,
  notification: BackboneEvent,
  ephemeral: boolean,
  turnId = "turn-1",
): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: ephemeral ? null : eventSeq,
    session_update_type: notification.type,
    turn_id: turnId,
    entry_index: null,
    tool_call_id: "patch-1",
    notification: {
      event: notification,
      sessionId: "session-1",
      source: {
        connectorId: "codex-app-server",
        connectorType: "codex",
        executorId: null,
      },
      trace: { turnId, entryIndex: null },
      observedAt: "2026-07-24T00:00:00Z",
    },
    ephemeral,
    presentation_id: `event-${eventSeq}`,
    runtime_change_sequence: null,
    baseline: !ephemeral,
  };
}

describe("session stream tool progress", () => {
  it("merges file_change_patch_updated into the active fileChange item", () => {
    const started = event(
      1,
      {
        type: "item_started",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          startedAtMs: 1,
          item: {
            type: "fileChange",
            id: "patch-1",
            changes: [],
            status: "inProgress",
          },
        },
      },
      false,
    );
    const firstPatchUpdated = event(
      1,
      {
        type: "file_change_patch_updated",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          itemId: "patch-1",
          changes: [
            {
              path: "src/main.ts",
              kind: { type: "update", move_path: null },
              diff: "@@ -1 +1 @@\n-old\n+first",
            },
          ],
        },
      },
      true,
    );
    const secondPatchUpdated = event(
      2,
      {
        type: "file_change_patch_updated",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          itemId: "patch-1",
          changes: [
            {
              path: "src/main.ts",
              kind: { type: "update", move_path: null },
              diff: "@@ -1 +1 @@\n-old\n+second",
            },
          ],
        },
      },
      true,
    );

    const state = reduceStreamState(
      createInitialStreamState([]),
      [started, firstPatchUpdated, secondPatchUpdated],
    );

    expect(state.entries).toHaveLength(1);
    const itemEvent = state.entries[0]?.event;
    expect(itemEvent?.type).toBe("item_started");
    if (itemEvent?.type !== "item_started") {
      throw new Error("expected active fileChange item");
    }
    expect(itemEvent.payload.item).toMatchObject({
      type: "fileChange",
      id: "patch-1",
      changes: [
        expect.objectContaining({
          path: "src/main.ts",
          diff: expect.stringContaining("+second"),
        }),
      ],
    });
  });

  it.each(["succeeded", "failed", "lost", "cancelled"] as const)(
    "stops streaming when context compaction reaches %s",
    (status) => {
      const item = {
        type: "contextCompaction" as const,
        id: "compaction-1",
        status,
        error: status === "failed" ? "compaction apply failed" : null,
        startedAtMs: 1n,
        completedAtMs: 2n,
        contextRevision: status === "succeeded" ? "context-2" : null,
      };
      const started = event(
        1,
        {
          type: "item_started",
          payload: {
            threadId: "session-1",
            turnId: "turn-1",
            startedAtMs: 1,
            item: { ...item, status: "inProgress", completedAtMs: null },
          },
        },
        false,
      );
      const terminalEvent: BackboneEvent = {
        type: "item_completed",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          completedAtMs: 2,
          terminal: {
            outcome: status,
            error: item.error,
          },
          item,
        },
      };
      const terminal = event(2, terminalEvent, false);

      const state = reduceStreamState(
        createInitialStreamState([]),
        [started, terminal],
      );

      expect(state.entries).toHaveLength(1);
      expect(state.entries[0]?.isStreaming).toBe(false);
      expect(state.entries[0]?.itemLifecycle).toEqual({
        phase: "terminal",
        outcome: status,
        error: item.error,
      });
      expect(state.entries[0]?.event).toMatchObject({
        payload: { item: { status } },
      });
    },
  );

  it("materializes a terminal item even when its start was not observed", () => {
    const terminal = event(
      2,
      {
        type: "item_completed",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          completedAtMs: 2,
          terminal: {
            outcome: "lost",
            error: "terminal recovered after reconnect",
          },
          item: {
            type: "contextCompaction",
            id: "compaction-1",
            status: "lost",
            error: "terminal recovered after reconnect",
            startedAtMs: null,
            completedAtMs: 2n,
            contextRevision: null,
          },
        },
      },
      false,
    );

    const state = reduceStreamState(createInitialStreamState([]), [terminal]);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({
      itemFreshness: "completed",
      itemLifecycle: {
        phase: "terminal",
        outcome: "lost",
        error: "terminal recovered after reconnect",
      },
      isStreaming: false,
    });
  });

  it("does not let late progress overwrite terminal evidence", () => {
    const terminal = event(
      2,
      {
        type: "item_completed",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          completedAtMs: 2,
          terminal: { outcome: "cancelled", error: null },
          item: {
            type: "contextCompaction",
            id: "compaction-1",
            status: "cancelled",
            error: null,
            startedAtMs: 1n,
            completedAtMs: 2n,
            contextRevision: null,
          },
        },
      },
      false,
    );
    const lateProgress = event(
      3,
      {
        type: "item_updated",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          updatedAtMs: 3,
          item: {
            type: "contextCompaction",
            id: "compaction-1",
            status: "inProgress",
            error: null,
            startedAtMs: 1n,
            completedAtMs: null,
            contextRevision: null,
          },
        },
      },
      false,
    );

    const state = reduceStreamState(
      createInitialStreamState([]),
      [terminal, lateProgress],
    );

    expect(state.entries[0]?.event.type).toBe("item_completed");
    expect(state.entries[0]?.itemLifecycle).toEqual({
      phase: "terminal",
      outcome: "cancelled",
      error: null,
    });
    expect(state.entries[0]?.isStreaming).toBe(false);
  });

  it("continues to merge generic item_updated events by item ID", () => {
    const started = event(
      1,
      {
        type: "item_started",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          startedAtMs: 1,
          item: {
            type: "fileChange",
            id: "patch-1",
            changes: [],
            status: "inProgress",
          },
        },
      },
      false,
    );
    const updated = event(
      2,
      {
        type: "item_updated",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          updatedAtMs: 2,
          item: {
            type: "fileChange",
            id: "patch-1",
            status: "inProgress",
            changes: [
              {
                path: "src/generic.ts",
                kind: { type: "add" },
                diff: "+generic update",
              },
            ],
          },
        },
      },
      false,
    );

    const state = reduceStreamState(
      createInitialStreamState([]),
      [started, updated],
    );

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.event).toMatchObject({
      type: "item_updated",
      payload: {
        item: {
          id: "patch-1",
          changes: [expect.objectContaining({ path: "src/generic.ts" })],
        },
      },
    });
    expect(state.entries[0]?.itemLifecycle).toEqual({ phase: "progress" });
  });
});

describe("terminal turn absorption", () => {
  it("ignores late message, reasoning and tool progress while allowing a new turn", () => {
    const toolStarted = event(
      1,
      {
        type: "item_started",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          startedAtMs: 1,
          item: {
            type: "fileChange",
            id: "patch-1",
            changes: [],
            status: "inProgress",
          },
        },
      },
      false,
    );
    const message = event(
      1,
      {
        type: "agent_message_delta",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          itemId: "message-1",
          delta: "before terminal",
        },
      },
      true,
    );
    const terminal = event(
      2,
      {
        type: "turn_completed",
        payload: {
          threadId: "session-1",
          turn: {
            id: "turn-1",
            items: [],
            itemsView: "full",
            status: "completed",
            error: null,
          },
        },
      },
      false,
    );

    let state = reduceStreamState(
      createInitialStreamState([]),
      [toolStarted, message, terminal],
    );
    expect(state.entries.every((entry) => entry.isStreaming !== true)).toBe(true);
    const terminalEntries = state.entries;

    state = reduceStreamState(state, [
      event(
        2,
        {
          type: "agent_message_delta",
          payload: {
            threadId: "session-1",
            turnId: "turn-1",
            itemId: "message-1",
            delta: " late message",
          },
        },
        true,
      ),
      event(
        3,
        {
          type: "reasoning_text_delta",
          payload: {
            threadId: "session-1",
            turnId: "turn-1",
            itemId: "reasoning-1",
            contentIndex: 0,
            delta: "late reasoning",
          },
        },
        true,
      ),
      event(
        4,
        {
          type: "file_change_patch_updated",
          payload: {
            threadId: "session-1",
            turnId: "turn-1",
            itemId: "patch-1",
            changes: [
              {
                path: "src/main.ts",
                kind: { type: "update", move_path: null },
                diff: "@@ -1 +1 @@\n-old\n+late",
              },
            ],
          },
        },
        true,
      ),
    ]);

    expect(state.entries).toEqual(terminalEntries);
    expect(state.lastEphemeralSeq).toBe(4);

    state = reduceStreamState(state, [
      event(
        5,
        {
          type: "agent_message_delta",
          payload: {
            threadId: "session-1",
            turnId: "turn-2",
            itemId: "message-2",
            delta: "new turn",
          },
        },
        true,
        "turn-2",
      ),
    ]);

    expect(state.entries).toHaveLength(3);
    expect(state.entries.at(-1)).toMatchObject({
      turnId: "turn-2",
      accumulatedText: "new turn",
      isStreaming: true,
    });
  });
});

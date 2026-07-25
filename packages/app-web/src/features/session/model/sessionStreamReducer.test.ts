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
): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: ephemeral ? null : eventSeq,
    session_update_type: notification.type,
    turn_id: "turn-1",
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
      trace: { turnId: "turn-1", entryIndex: null },
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
    const patchUpdated = event(
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
              diff: "@@ -1 +1 @@\n-old\n+new",
            },
          ],
        },
      },
      true,
    );

    const state = reduceStreamState(
      createInitialStreamState([]),
      [started, patchUpdated],
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
          diff: expect.stringContaining("+new"),
        }),
      ],
    });
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
  });
});

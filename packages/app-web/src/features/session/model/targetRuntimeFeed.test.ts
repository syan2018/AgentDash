import { describe, expect, it } from "vitest";

import {
  availabilityForCompaction,
  reduceTargetRuntimeChangePage,
  type TargetRuntimeCommittedChangePayload,
  type TargetRuntimeFeedSnapshot,
} from "./targetRuntimeFeed";

function snapshot(): TargetRuntimeFeedSnapshot {
  return {
    sourceCoordinate: "runtime-child",
    snapshotRevision: 4,
    committedSequence: 7,
    cursor: "cursor-7",
    items: [
      {
        itemId: "child-item",
        turnId: "child-turn",
        kind: "message",
        text: "child-visible",
      },
    ],
    compaction: { state: "idle" },
    availability: { submitInput: true, compact: true, reason: null },
  };
}

function reduce(payload: TargetRuntimeCommittedChangePayload) {
  return reduceTargetRuntimeChangePage(snapshot(), {
    sourceCoordinate: "runtime-child",
    afterCursor: "cursor-7",
    nextCursor: "cursor-8",
    gap: false,
    changes: [
      {
        sequence: 8,
        previousSnapshotRevision: 4,
        snapshotRevision: 5,
        payload,
      },
    ],
  });
}

describe("target Runtime feed protocol", () => {
  it("uses only the target Runtime snapshot as visible history", () => {
    const current = snapshot();

    expect(current.items.map((item) => item.text)).toEqual(["child-visible"]);
    expect(JSON.stringify(current)).not.toContain("ancestor");
  });

  it("requests a snapshot reload when the Runtime reports a cursor gap", () => {
    expect(
      reduceTargetRuntimeChangePage(snapshot(), {
        sourceCoordinate: "runtime-child",
        afterCursor: "cursor-7",
        nextCursor: "cursor-12",
        gap: true,
        changes: [],
      }),
    ).toEqual({ kind: "snapshot_reload_required" });
  });

  it.each([
    [
      { kind: "compaction_started", operationId: "compact-1" } as const,
      "started",
      false,
    ],
    [
      { kind: "compaction_completed", operationId: "compact-1" } as const,
      "completed",
      true,
    ],
    [
      {
        kind: "compaction_failed",
        operationId: "compact-1",
        reason: "rejected",
      } as const,
      "failed",
      true,
    ],
    [
      {
        kind: "compaction_lost",
        operationId: "compact-1",
        reason: "unknown final state",
      } as const,
      "lost",
      false,
    ],
  ])(
    "projects %s with command availability",
    (payload, expectedState, expectedSubmitInput) => {
      const outcome = reduce(payload);
      expect(outcome.kind).toBe("applied");
      if (outcome.kind === "applied") {
        expect(outcome.snapshot.compaction.state).toBe(expectedState);
        expect(outcome.snapshot.availability.submitInput).toBe(
          expectedSubmitInput,
        );
      }
    },
  );

  it("keeps failed and lost availability semantically distinct", () => {
    expect(
      availabilityForCompaction({
        state: "failed",
        operationId: "c1",
        reason: "rejected",
      }),
    ).toEqual({ submitInput: true, compact: true, reason: null });
    expect(
      availabilityForCompaction({
        state: "lost",
        operationId: "c2",
        reason: "unknown",
      }),
    ).toEqual({
      submitInput: false,
      compact: false,
      reason: "compaction_state_lost",
    });
  });
});

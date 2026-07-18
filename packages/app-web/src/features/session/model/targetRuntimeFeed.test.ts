import { describe, expect, it } from "vitest";

import canonicalFixture from "./fixtures/managedRuntimeProjection.json";
import {
  consumeManagedRuntimeChangePage,
  managedRuntimeCommandAvailability,
} from "./targetRuntimeFeed";

describe("managed Runtime feed protocol", () => {
  it("applies the canonical Rust change page without identity or delta translation", () => {
    const outcome = consumeManagedRuntimeChangePage(
      canonicalFixture.snapshots.started,
      canonicalFixture.change_page,
    );

    expect(outcome.kind).toBe("apply");
    if (outcome.kind === "apply") {
      expect(outcome.change_page).toBe(canonicalFixture.change_page);
      expect(outcome.change_page.thread_id).toBe("runtime-thread-child");
      expect(outcome.change_page.changes[0]).toEqual({
        thread_id: "runtime-thread-child",
        sequence: 9,
        revision: 6,
        delta: {
          kind: "item_upserted",
          item: {
            id: "item-compaction",
            turn_id: "turn-compaction",
            status: "completed",
            content: { kind: "context_compaction" },
            content_digest: "sha256:compaction-6",
          },
        },
      });
    }
  });

  it("reloads the canonical snapshot when Runtime reports a typed gap", () => {
    expect(
      consumeManagedRuntimeChangePage(
        canonicalFixture.snapshots.started,
        canonicalFixture.gap_page,
      ),
    ).toEqual({ kind: "snapshot_reload_required" });
    expect(canonicalFixture.gap_page.gap).toEqual({
      requested_after: 4,
      earliest_available: 9,
      latest_available: 12,
      snapshot_revision: 8,
    });
  });

  it("renders compaction lifecycle from canonical item statuses", () => {
    expect(
      [
        canonicalFixture.snapshots.started,
        canonicalFixture.snapshots.completed,
        canonicalFixture.snapshots.failed,
        canonicalFixture.snapshots.lost,
      ].map((snapshot) => snapshot.items[0]?.status),
    ).toEqual(["running", "completed", "failed", "lost"]);
  });

  it("uses the Runtime-owned availability decision verbatim", () => {
    const started =
      canonicalFixture.snapshots.started.command_availability.submit_input;
    const completed =
      canonicalFixture.snapshots.completed.command_availability.submit_input;

    expect(
      managedRuntimeCommandAvailability(
        canonicalFixture.snapshots.started,
        "submit_input",
      ),
    ).toBe(started);
    expect(started).toMatchObject({
      status: "unavailable",
      reason: "operation_in_flight",
      evidence: { decided_at_revision: 5 },
    });
    expect(
      managedRuntimeCommandAvailability(
        canonicalFixture.snapshots.completed,
        "submit_input",
      ),
    ).toBe(completed);
    expect(completed).toMatchObject({
      status: "available",
      evidence: { decided_at_revision: 6 },
    });
    expect(
      managedRuntimeCommandAvailability(
        canonicalFixture.snapshots.failed,
        "submit_input",
      ),
    ).toMatchObject({
      status: "available",
      evidence: { decided_at_revision: 7 },
    });
    expect(
      managedRuntimeCommandAvailability(
        canonicalFixture.snapshots.lost,
        "submit_input",
      ),
    ).toMatchObject({
      status: "unavailable",
      reason: "source_unavailable",
      evidence: { decided_at_revision: 8 },
    });
  });

  it("rejects translated identity and non-contiguous canonical ordering", () => {
    expect(() =>
      consumeManagedRuntimeChangePage(canonicalFixture.snapshots.started, {
        ...canonicalFixture.change_page,
        thread_id: "translated-thread",
      }),
    ).toThrow("thread does not match");
    expect(() =>
      consumeManagedRuntimeChangePage(canonicalFixture.snapshots.started, {
        ...canonicalFixture.change_page,
        changes: canonicalFixture.change_page.changes.map((change) => ({
          ...change,
          sequence: 10,
        })),
        next: 10,
      }),
    ).toThrow("not contiguous");
  });
});

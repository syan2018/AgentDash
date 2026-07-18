import { describe, expect, it } from "vitest";

import { managedRuntimeTestFixtures } from "./managedRuntimeTestFixtures";
import {
  applyManagedRuntimeChangePage,
  consumeManagedRuntimeChangePage,
  managedRuntimeCommandAvailability,
} from "./managedRuntimeProjection";

describe("managed Runtime feed protocol", () => {
  it("applies the canonical Rust change page without identity or delta translation", () => {
    const outcome = consumeManagedRuntimeChangePage(
      managedRuntimeTestFixtures.snapshots.started,
      managedRuntimeTestFixtures.changePage,
    );

    expect(outcome.kind).toBe("apply");
    if (outcome.kind === "apply") {
      expect(outcome.change_page).toBe(managedRuntimeTestFixtures.changePage);
      expect(outcome.change_page.thread_id).toBe("runtime-thread-child");
      expect(outcome.change_page.changes[0]?.delta).toMatchObject({
        kind: "source_projection_changed",
        section: "snapshot",
        delta: {
          kind: "snapshot_replaced",
          items: [{ id: "item-compaction", status: "completed" }],
        },
      });
    }
  });

  it("folds snapshot then committed changes in sequence order", () => {
    const applied = applyManagedRuntimeChangePage(
      managedRuntimeTestFixtures.snapshots.started,
      managedRuntimeTestFixtures.changePage,
    );

    expect(applied).not.toBeNull();
    expect(applied).toMatchObject({
      revision: 6,
      latest_change_sequence: 9,
      active_turn_id: null,
      turns: [{ id: "turn-compaction", status: "completed" }],
      items: [{ id: "item-compaction", status: "completed" }],
    });
  });

  it("reloads the canonical snapshot when Runtime reports a typed gap", () => {
    expect(
      consumeManagedRuntimeChangePage(
        managedRuntimeTestFixtures.snapshots.started,
        managedRuntimeTestFixtures.gapPage,
      ),
    ).toEqual({ kind: "snapshot_reload_required" });
    expect(managedRuntimeTestFixtures.gapPage.gap).toEqual({
      requested_after: 4,
      earliest_available: 9,
      latest_available: 12,
      snapshot_revision: 8,
    });
  });

  it("renders compaction lifecycle from canonical item statuses", () => {
    expect(
      [
        managedRuntimeTestFixtures.snapshots.started,
        managedRuntimeTestFixtures.snapshots.completed,
        managedRuntimeTestFixtures.snapshots.failed,
        managedRuntimeTestFixtures.snapshots.lost,
      ].map((snapshot) => snapshot.items[0]?.status),
    ).toEqual(["running", "completed", "failed", "lost"]);
  });

  it("uses the Runtime-owned availability decision verbatim", () => {
    const started =
      managedRuntimeTestFixtures.snapshots.started.command_availability.submit_input;
    const completed =
      managedRuntimeTestFixtures.snapshots.completed.command_availability.submit_input;

    expect(
      managedRuntimeCommandAvailability(
        managedRuntimeTestFixtures.snapshots.started,
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
        managedRuntimeTestFixtures.snapshots.completed,
        "submit_input",
      ),
    ).toBe(completed);
    expect(completed).toMatchObject({
      status: "available",
      evidence: { decided_at_revision: 6 },
    });
    expect(
      managedRuntimeCommandAvailability(
        managedRuntimeTestFixtures.snapshots.failed,
        "submit_input",
      ),
    ).toMatchObject({
      status: "available",
      evidence: { decided_at_revision: 7 },
    });
    expect(
      managedRuntimeCommandAvailability(
        managedRuntimeTestFixtures.snapshots.lost,
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
      consumeManagedRuntimeChangePage(managedRuntimeTestFixtures.snapshots.started, {
        ...managedRuntimeTestFixtures.changePage,
        thread_id: "translated-thread",
      }),
    ).toThrow("thread does not match");
    expect(() =>
      consumeManagedRuntimeChangePage(managedRuntimeTestFixtures.snapshots.started, {
        ...managedRuntimeTestFixtures.changePage,
        changes: managedRuntimeTestFixtures.changePage.changes.map((change) => ({
          ...change,
          sequence: 10,
        })),
        next: 10,
      }),
    ).toThrow("not contiguous");
  });

  it("ignores a duplicate reconnect page without reducing it twice", () => {
    const completed = managedRuntimeTestFixtures.snapshots.completed;
    const duplicatePage = managedRuntimeTestFixtures.changePage;

    expect(consumeManagedRuntimeChangePage(completed, duplicatePage)).toEqual({
      kind: "duplicate",
    });
    expect(applyManagedRuntimeChangePage(completed, duplicatePage)).toBe(completed);
  });
});

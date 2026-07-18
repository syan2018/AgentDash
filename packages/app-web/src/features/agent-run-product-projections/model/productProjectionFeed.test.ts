import { describe, expect, it, vi } from "vitest";

import { connectProductProjectionFeed } from "./productProjectionFeed";

interface Snapshot {
  target: { run_id: string; agent_id: string };
  latest_change_sequence: number;
  marker: string;
}

interface Change {
  target: { run_id: string; agent_id: string };
  sequence: number;
}

interface Page {
  target: { run_id: string; agent_id: string };
  changes: Change[];
  next: number;
  gap?: object | null;
}

const target = { runId: "run-1", agentId: "agent-1" };
const wireTarget = { run_id: "run-1", agent_id: "agent-1" };

function harness(
  snapshots: Snapshot[],
  pages: Page[],
) {
  const scheduled: Array<() => void> = [];
  return {
    scheduled,
    dependencies: {
      fetchSnapshot: vi.fn(async () => {
        const snapshot = snapshots.shift();
        if (!snapshot) throw new Error("missing snapshot");
        return snapshot;
      }),
      fetchChanges: vi.fn(async () => {
        const page = pages.shift();
        if (!page) throw new Error("missing page");
        return page;
      }),
      schedule: (callback: () => void) => {
        scheduled.push(callback);
        return callback;
      },
      cancel: vi.fn(),
    },
  };
}

describe("connectProductProjectionFeed", () => {
  it("keeps snapshot hydration separate from imperative tail changes", async () => {
    const fixture = harness(
      [{ target: wireTarget, latest_change_sequence: 4, marker: "baseline" }],
      [{
        target: wireTarget,
        changes: [{ target: wireTarget, sequence: 5 }],
        next: 5,
      }],
    );
    const onSnapshot = vi.fn();
    const onChanges = vi.fn();
    const connection = connectProductProjectionFeed<Snapshot, Change, Page>(
      target,
      { onSnapshot, onChanges },
      fixture.dependencies,
    );
    await connection.ready;

    expect(onSnapshot).toHaveBeenCalledWith(
      expect.objectContaining({ marker: "baseline" }),
      "initial",
    );
    expect(onChanges).toHaveBeenCalledWith([
      expect.objectContaining({ sequence: 5 }),
    ]);
    connection.close();
  });

  it("reloads a durable snapshot when the change source reports a gap", async () => {
    const fixture = harness(
      [
        { target: wireTarget, latest_change_sequence: 4, marker: "initial" },
        { target: wireTarget, latest_change_sequence: 9, marker: "reloaded" },
      ],
      [{
        target: wireTarget,
        changes: [],
        next: 9,
        gap: { earliest_available: 7 },
      }],
    );
    const onSnapshot = vi.fn();
    const onChanges = vi.fn();
    const connection = connectProductProjectionFeed<Snapshot, Change, Page>(
      target,
      { onSnapshot, onChanges },
      fixture.dependencies,
    );
    await connection.ready;

    expect(onSnapshot.mock.calls).toEqual([
      [expect.objectContaining({ marker: "initial" }), "initial"],
      [expect.objectContaining({ marker: "reloaded" }), "gap_reload"],
    ]);
    expect(onChanges).not.toHaveBeenCalled();
    connection.close();
  });

  it("rejects a projection page from a different AgentRun target", async () => {
    const fixture = harness(
      [{ target: wireTarget, latest_change_sequence: 4, marker: "baseline" }],
      [{
        target: { run_id: "other", agent_id: "agent-1" },
        changes: [],
        next: 4,
      }],
    );
    const onError = vi.fn();
    const connection = connectProductProjectionFeed<Snapshot, Change, Page>(
      target,
      { onSnapshot: vi.fn(), onChanges: vi.fn(), onError },
      fixture.dependencies,
    );
    await connection.ready;

    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({
        message: "Product projection change target fence mismatch",
      }),
    );
    connection.close();
  });

  it("schedules a reconnect when the initial snapshot load fails", async () => {
    const scheduled: Array<() => void> = [];
    const fetchSnapshot = vi.fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({
        target: wireTarget,
        latest_change_sequence: 4,
        marker: "reconnected",
      });
    const onSnapshot = vi.fn();
    const onError = vi.fn();
    const connection = connectProductProjectionFeed<Snapshot, Change, Page>(
      target,
      { onSnapshot, onChanges: vi.fn(), onError },
      {
        fetchSnapshot,
        fetchChanges: vi.fn(async () => ({
          target: wireTarget,
          changes: [],
          next: 4,
        })),
        schedule: (callback) => {
          scheduled.push(callback);
          return callback;
        },
        cancel: vi.fn(),
      },
    );
    await connection.ready;

    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({ message: "offline" }),
    );
    expect(scheduled).toHaveLength(1);
    scheduled.shift()?.();
    await vi.waitFor(() => {
      expect(onSnapshot).toHaveBeenCalledWith(
        expect.objectContaining({ marker: "reconnected" }),
        "initial",
      );
    });
    connection.close();
  });
});

import { describe, expect, it, vi } from "vitest";

import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-contracts";
import { managedRuntimeTestFixtures } from "./managedRuntimeTestFixtures";
import {
  connectManagedRuntimeFeed,
  type ManagedRuntimeFeedConnectionObserver,
} from "./managedRuntimeFeedConnection";
import type {
  ManagedRuntimeFeedTransport,
  ManagedRuntimeFeedTransportOptions,
} from "./managedRuntimeFeedTransport";

function observer(): ManagedRuntimeFeedConnectionObserver {
  return {
    onBaseline: vi.fn(),
    onProjection: vi.fn(),
    onLifecycleChange: vi.fn(),
    onError: vi.fn(),
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolvePromise = (_value: T): void => {};
  const promise = new Promise<T>((resolve) => {
    resolvePromise = resolve;
  });
  return { promise, resolve: resolvePromise };
}

describe("Managed Runtime feed connection", () => {
  it("subscribes after the snapshot tail and reduces each committed change once", async () => {
    const baseline = managedRuntimeTestFixtures.snapshots.started;
    const changePage = managedRuntimeTestFixtures.changePage;
    const options: ManagedRuntimeFeedTransportOptions[] = [];
    const connectionObserver = observer();

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot: vi.fn().mockResolvedValue(baseline),
        createTransport: (value) => {
          options.push(value);
          return { close: vi.fn() };
        },
      },
    );
    await connection.ready;

    expect(options[0]?.after).toBe(8);
    options[0]?.onPage(changePage);
    expect(connectionObserver.onProjection).toHaveBeenCalledTimes(1);
    expect(connectionObserver.onProjection).toHaveBeenCalledWith(
      expect.objectContaining({
        revision: 6,
        latest_change_sequence: 9,
      }),
      changePage.changes,
    );

    options[0]?.onPage(changePage);
    expect(connectionObserver.onProjection).toHaveBeenCalledTimes(1);
  });

  it("closes the stale tail and reloads a snapshot before subscribing after a gap", async () => {
    const started = managedRuntimeTestFixtures.snapshots.started;
    const lost = managedRuntimeTestFixtures.snapshots.lost;
    const transports: Array<{
      options: ManagedRuntimeFeedTransportOptions;
      transport: ManagedRuntimeFeedTransport;
    }> = [];
    const connectionObserver = observer();
    const fetchSnapshot = vi
      .fn()
      .mockResolvedValueOnce(started)
      .mockResolvedValueOnce(lost);

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot,
        createTransport: (options) => {
          const transport = { close: vi.fn() };
          transports.push({ options, transport });
          return transport;
        },
      },
    );
    await connection.ready;

    transports[0]?.options.onPage(managedRuntimeTestFixtures.gapPage);
    await Promise.resolve();
    await Promise.resolve();

    expect(transports[0]?.transport.close).toHaveBeenCalledOnce();
    expect(fetchSnapshot).toHaveBeenCalledTimes(2);
    expect(connectionObserver.onBaseline).toHaveBeenLastCalledWith(lost);
    expect(transports[1]?.options.after).toBe(11);
  });

  it("does not publish a late baseline after the connection is closed", async () => {
    const pendingSnapshot = deferred<ManagedRuntimeSnapshot>();
    const connectionObserver = observer();
    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot: () => pendingSnapshot.promise,
        createTransport: () => ({ close: vi.fn() }),
      },
    );

    connection.close();
    pendingSnapshot.resolve(managedRuntimeTestFixtures.snapshots.completed);
    await connection.ready;

    expect(connectionObserver.onBaseline).not.toHaveBeenCalled();
  });
});

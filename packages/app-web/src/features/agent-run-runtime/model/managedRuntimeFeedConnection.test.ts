import { describe, expect, it, vi } from "vitest";

import type { AgentLiveEvent } from "../../../generated/agent-service-api";
import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-validators";
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
  const textDelta: AgentLiveEvent = {
    source: "source-1",
    turn_id: "turn-live",
    item_id: "item-live",
    sequence: "1" as AgentLiveEvent["sequence"],
    payload: {
      kind: "text_delta",
      round: 1,
      delta: "hello",
    },
  };

  it("subscribes after the authoritative snapshot and folds live events as disposable presentation", async () => {
    const baseline = managedRuntimeTestFixtures.snapshots.started;
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

    options[0]?.onEvent(textDelta);
    expect(connectionObserver.onProjection).toHaveBeenCalledTimes(1);
    expect(connectionObserver.onProjection).toHaveBeenCalledWith(
      expect.objectContaining({
        active_turn_id: "agent-turn:turn-live",
        items: expect.arrayContaining([
          expect.objectContaining({
            id: "agent-item:item-live",
          }),
        ]),
      }),
    );
  });

  it("reloads the authoritative snapshot when the live connection reconnects", async () => {
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

    transports[0]?.options.onLifecycleChange("reconnecting");
    await Promise.resolve();
    await Promise.resolve();

    expect(fetchSnapshot).toHaveBeenCalledTimes(2);
    expect(connectionObserver.onProjection).toHaveBeenLastCalledWith(lost);
    expect(transports).toHaveLength(1);
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

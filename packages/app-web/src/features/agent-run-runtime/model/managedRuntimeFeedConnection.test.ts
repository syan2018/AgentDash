import { describe, expect, it, vi } from "vitest";

import type { AgentLiveEvent } from "../../../generated/agent-service-api";
import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-validators";
import { managedRuntimeTestFixtures } from "./managedRuntimeTestFixtures";
import { hasActiveCanonicalTurn } from "./agentLiveProjection";
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
  const turnStarted: AgentLiveEvent = {
    source: "source-1",
    sequence: "0" as AgentLiveEvent["sequence"],
    record: {
      presentation_id: "live:source-1:turn-started",
      presentation: {
        durability: "durable",
        envelope: {
          event: {
            type: "turn_started",
            payload: {
              threadId: "source-1",
              turn: {
                id: "turn-live",
                items: [],
                itemsView: "full",
                status: "inProgress",
                error: null,
              },
            },
          },
          sessionId: "source-1",
          source: {
            connectorId: "dash-agent",
            connectorType: "native",
            executorId: null,
          },
          trace: { turnId: "turn-live", entryIndex: null },
          observedAt: "2026-07-21T00:00:00Z",
        },
      },
    },
  };

  const textDelta: AgentLiveEvent = {
    source: "source-1",
    sequence: "1" as AgentLiveEvent["sequence"],
    record: {
      presentation_id: "live:source-1:1",
      presentation: {
        durability: "ephemeral",
        envelope: {
          event: {
            type: "agent_message_delta",
            payload: {
              threadId: "source-1",
              turnId: "turn-live",
              itemId: "item-live",
              delta: "hello",
            },
          },
          sessionId: "source-1",
          source: {
            connectorId: "dash-agent",
            connectorType: "native",
            executorId: null,
          },
          trace: { turnId: "turn-live", entryIndex: null },
          observedAt: "2026-07-21T00:00:00Z",
        },
      },
    },
  };

  const turnCompleted: AgentLiveEvent = {
    source: "source-1",
    sequence: "2" as AgentLiveEvent["sequence"],
    record: {
      presentation_id: "live:source-1:turn-completed",
      presentation: {
        durability: "durable",
        envelope: {
          event: {
            type: "turn_completed",
            payload: {
              threadId: "source-1",
              turn: {
                id: "turn-live",
                items: [],
                itemsView: "full",
                status: "completed",
                error: null,
              },
            },
          },
          sessionId: "source-1",
          source: {
            connectorId: "dash-agent",
            connectorType: "native",
            executorId: null,
          },
          trace: { turnId: "turn-live", entryIndex: null },
          observedAt: "2026-07-21T00:00:01Z",
        },
      },
    },
  };

  const threadNameUpdated: AgentLiveEvent = {
    source: "source-1",
    sequence: "3" as AgentLiveEvent["sequence"],
    record: {
      presentation_id: "live:source-1:thread-name",
      presentation: {
        durability: "durable",
        envelope: {
          event: {
            type: "thread_name_updated",
            payload: {
              threadId: "source-1",
              threadName: "Terminal convergence",
            },
          },
          sessionId: "source-1",
          source: {
            connectorId: "dash-agent",
            connectorType: "native",
            executorId: null,
          },
          trace: { turnId: null, entryIndex: null },
          observedAt: "2026-07-21T00:00:02Z",
        },
      },
    },
  };

  it("subscribes before loading the authoritative snapshot and folds buffered live events", async () => {
    const baseline = managedRuntimeTestFixtures.snapshots.started;
    const pendingBaseline = deferred<ManagedRuntimeSnapshot>();
    const options: ManagedRuntimeFeedTransportOptions[] = [];
    const connectionObserver = observer();

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot: vi.fn(() => pendingBaseline.promise),
        createTransport: (value) => {
          options.push(value);
          return { close: vi.fn() };
        },
      },
    );

    options[0]?.onEvent(textDelta);
    expect(connectionObserver.onBaseline).not.toHaveBeenCalled();
    pendingBaseline.resolve(baseline);
    await connection.ready;

    expect(connectionObserver.onBaseline).toHaveBeenCalledWith(baseline);
    expect(connectionObserver.onProjection).toHaveBeenCalledTimes(1);
    expect(connectionObserver.onProjection).toHaveBeenCalledWith(
      expect.objectContaining({
        conversation_history: expect.arrayContaining([
          expect.objectContaining({
            presentation_id: "live:source-1:1",
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
    expect(fetchSnapshot).toHaveBeenCalledTimes(1);

    transports[0]?.options.onLifecycleChange("connected");
    await Promise.resolve();

    expect(fetchSnapshot).toHaveBeenCalledTimes(2);
    expect(connectionObserver.onProjection).toHaveBeenLastCalledWith(lost);
    expect(transports).toHaveLength(1);
  });

  it("reloads the authoritative snapshot after a completed Product command", async () => {
    const started = managedRuntimeTestFixtures.snapshots.started;
    const completed = managedRuntimeTestFixtures.snapshots.completed;
    const connectionObserver = observer();
    const fetchSnapshot = vi
      .fn()
      .mockResolvedValueOnce(started)
      .mockResolvedValueOnce(completed);

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot,
        createTransport: () => ({ close: vi.fn() }),
      },
    );
    await connection.ready;
    await connection.reload();

    expect(fetchSnapshot).toHaveBeenCalledTimes(2);
    expect(connectionObserver.onProjection).toHaveBeenLastCalledWith(completed);
  });

  it("replaces the disposable live overlay with the authoritative terminal snapshot", async () => {
    const started = managedRuntimeTestFixtures.snapshots.started;
    const completed = managedRuntimeTestFixtures.snapshots.completed;
    const transports: ManagedRuntimeFeedTransportOptions[] = [];
    const connectionObserver = observer();
    const fetchSnapshot = vi
      .fn()
      .mockResolvedValueOnce(started)
      .mockResolvedValueOnce(completed);

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot,
        createTransport: (options) => {
          transports.push(options);
          return { close: vi.fn() };
        },
      },
    );
    await connection.ready;

    transports[0]?.onEvent(textDelta);
    transports[0]?.onEvent(turnCompleted);
    await Promise.resolve();
    await Promise.resolve();

    expect(fetchSnapshot).toHaveBeenCalledTimes(2);
    const converged = vi.mocked(connectionObserver.onProjection).mock.calls.at(-1)?.[0];
    expect(converged?.conversation_history).toContainEqual(
      expect.objectContaining({
        presentation_id: turnCompleted.record.presentation_id,
      }),
    );
    expect(converged?.conversation_history).not.toContainEqual(
      expect.objectContaining({ presentation_id: textDelta.record.presentation_id }),
    );
  });

  it("keeps the terminal boundary when the immediate authoritative reload is stale", async () => {
    const staleStarted = {
      ...managedRuntimeTestFixtures.snapshots.started,
      conversation_history: [turnStarted.record],
    };
    const transports: ManagedRuntimeFeedTransportOptions[] = [];
    const connectionObserver = observer();
    const fetchSnapshot = vi.fn().mockResolvedValue(staleStarted);

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot,
        createTransport: (options) => {
          transports.push(options);
          return { close: vi.fn() };
        },
      },
    );
    await connection.ready;

    transports[0]?.onEvent(turnCompleted);
    await Promise.resolve();
    await Promise.resolve();

    const converged = vi.mocked(connectionObserver.onProjection).mock.calls.at(-1)?.[0];
    expect(converged).toBeDefined();
    expect(hasActiveCanonicalTurn(converged?.conversation_history ?? [])).toBe(false);
  });

  it("preserves canonical live records received while the terminal snapshot is loading", async () => {
    const started = managedRuntimeTestFixtures.snapshots.started;
    const completed = managedRuntimeTestFixtures.snapshots.completed;
    const terminalSnapshot = deferred<ManagedRuntimeSnapshot>();
    const transports: ManagedRuntimeFeedTransportOptions[] = [];
    const connectionObserver = observer();
    const fetchSnapshot = vi
      .fn()
      .mockResolvedValueOnce(started)
      .mockImplementationOnce(() => terminalSnapshot.promise);

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot,
        createTransport: (options) => {
          transports.push(options);
          return { close: vi.fn() };
        },
      },
    );
    await connection.ready;

    transports[0]?.onEvent(textDelta);
    transports[0]?.onEvent(turnCompleted);
    transports[0]?.onEvent(threadNameUpdated);
    terminalSnapshot.resolve(completed);
    await Promise.resolve();
    await Promise.resolve();

    expect(connectionObserver.onProjection).toHaveBeenLastCalledWith(
      expect.objectContaining({
        conversation_history: expect.arrayContaining([
          expect.objectContaining({
            presentation_id: threadNameUpdated.record.presentation_id,
          }),
        ]),
      }),
    );
    const converged = vi.mocked(connectionObserver.onProjection).mock.calls.at(-1)?.[0];
    expect(converged?.conversation_history).not.toContainEqual(
      expect.objectContaining({ presentation_id: textDelta.record.presentation_id }),
    );
  });

  it("queues another authoritative reload when a later turn completes during convergence", async () => {
    const started = managedRuntimeTestFixtures.snapshots.started;
    const completed = managedRuntimeTestFixtures.snapshots.completed;
    const firstTerminalSnapshot = deferred<ManagedRuntimeSnapshot>();
    const transports: ManagedRuntimeFeedTransportOptions[] = [];
    const connectionObserver = observer();
    const fetchSnapshot = vi
      .fn()
      .mockResolvedValueOnce(started)
      .mockImplementationOnce(() => firstTerminalSnapshot.promise)
      .mockResolvedValueOnce(completed);
    const laterDelta: AgentLiveEvent = {
      ...textDelta,
      sequence: "4" as AgentLiveEvent["sequence"],
      record: {
        ...textDelta.record,
        presentation_id: "live:source-1:later-delta",
      },
    };
    const laterCompleted: AgentLiveEvent = {
      ...turnCompleted,
      sequence: "5" as AgentLiveEvent["sequence"],
      record: {
        ...turnCompleted.record,
        presentation_id: "live:source-1:later-completed",
      },
    };

    const connection = connectManagedRuntimeFeed(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      {
        fetchSnapshot,
        createTransport: (options) => {
          transports.push(options);
          return { close: vi.fn() };
        },
      },
    );
    await connection.ready;

    transports[0]?.onEvent(turnCompleted);
    transports[0]?.onEvent(laterDelta);
    transports[0]?.onEvent(laterCompleted);
    firstTerminalSnapshot.resolve(completed);
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(fetchSnapshot).toHaveBeenCalledTimes(3);
    const converged = vi.mocked(connectionObserver.onProjection).mock.calls.at(-1)?.[0];
    expect(converged?.conversation_history).toContainEqual(
      expect.objectContaining({
        presentation_id: laterCompleted.record.presentation_id,
      }),
    );
    expect(converged?.conversation_history).not.toContainEqual(
      expect.objectContaining({ presentation_id: laterDelta.record.presentation_id }),
    );
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

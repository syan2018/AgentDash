import { describe, expect, it, vi } from "vitest";

import type { CanonicalConversationRecord } from "../../../generated/backbone-protocol";
import type {
  AgentRuntimeOperationReceipt,
  AgentRuntimeStreamFrame,
  AgentRuntimeUpdate,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";
import { agentRuntimeTestFixtures } from "./agentRuntimeTestFixtures";
import {
  connectAgentRuntimeConnection,
  type AgentRuntimeConnectionObserver,
} from "./agentRuntimeConnection";
import type { AgentRuntimeUpdateTransportOptions } from "./agentRuntimeUpdateTransport";

function observer(): AgentRuntimeConnectionObserver {
  return {
    onBaseline: vi.fn(),
    onView: vi.fn(),
    onUpdate: vi.fn(),
    onReset: vi.fn(),
    onLifecycleChange: vi.fn(),
    onError: vi.fn(),
  };
}

function record(
  presentationId: string,
  event: CanonicalConversationRecord["presentation"]["envelope"]["event"],
  durability: "durable" | "ephemeral" = "ephemeral",
): CanonicalConversationRecord {
  return {
    presentation_id: presentationId,
    presentation: {
      durability,
      envelope: {
        event,
        sessionId: "source-1",
        source: {
          connectorId: "dash-agent",
          connectorType: "native",
          executorId: null,
        },
        trace: { turnId: "turn-live", entryIndex: null },
        observedAt: "2026-07-28T00:00:00Z",
      },
    },
  };
}

function update(
  sequence: bigint,
  overrides: Partial<Omit<AgentRuntimeUpdate, "state">> & {
    view_revision?: bigint;
    execution?: NonNullable<AgentRuntimeUpdate["state"]>["execution"];
  } = {},
): AgentRuntimeUpdate {
  const {
    view_revision: revision = sequence + 10n,
    execution,
    ...updateOverrides
  } = overrides;
  return {
    lane_sequence: sequence,
    state: {
      ...agentRuntimeTestFixtures.snapshots.started.observation,
      revision,
      execution: execution ?? {
        active_turn: {
          turn_id: "turn-live",
          kind: "conversation",
          phase: "running",
          started_at_ms: 1000n,
          cancellable: true,
        },
        queued_compaction: null,
        last_compaction_outcome: null,
      },
      context: {
        ...agentRuntimeTestFixtures.snapshots.started.observation.context,
        snapshot_revision: revision,
        context_revision: `context-${revision}`,
        recipe_digest: `sha256:context-${revision}`,
      },
    },
    presentations: [],
    ...updateOverrides,
  };
}

function baseline(
  connectionEpoch: bigint,
  view: AgentRuntimeView,
): AgentRuntimeStreamFrame {
  return {
    kind: "baseline",
    connection_epoch: connectionEpoch,
    view,
  };
}

function updateFrame(
  connectionEpoch: bigint,
  value: AgentRuntimeUpdate,
): AgentRuntimeStreamFrame {
  return {
    kind: "update",
    connection_epoch: connectionEpoch,
    lane_sequence: value.lane_sequence,
    state: value.state,
    presentations: value.presentations,
  };
}

function dependencies(input: {
  fetchView?: () => Promise<AgentRuntimeView>;
  executeCommand?: () => Promise<AgentRuntimeOperationReceipt>;
  transportOptions: AgentRuntimeUpdateTransportOptions[];
}) {
  return {
    fetchView: vi.fn(
      input.fetchView
        ?? (async () => agentRuntimeTestFixtures.snapshots.completed),
    ),
    executeCommand: vi.fn(
      input.executeCommand
        ?? (async () => ({
          operation_id: "operation-1",
          thread_id: "runtime-thread-child",
          status: "accepted" as const,
          duplicate: false,
        })),
    ),
    createTransport: (options: AgentRuntimeUpdateTransportOptions) => {
      input.transportOptions.push(options);
      return { close: vi.fn() };
    },
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolvePromise: ((value: T) => void) | null = null;
  const promise = new Promise<T>((resolve) => {
    resolvePromise = resolve;
  });
  return {
    promise,
    resolve: (value) => resolvePromise?.(value),
  };
}

function connect(
  connectionObserver: AgentRuntimeConnectionObserver,
  transportOptions: AgentRuntimeUpdateTransportOptions[],
  deps: ReturnType<typeof dependencies>,
) {
  const connection = connectAgentRuntimeConnection(
    { runId: "run-1", agentId: "agent-1" },
    connectionObserver,
    deps,
  );
  transportOptions[0]?.onEvent(
    baseline(1n, agentRuntimeTestFixtures.snapshots.completed),
  );
  return connection;
}

describe("AgentRuntimeConnection", () => {
  it("以 stream baseline 建立连接，普通 update 增量更新控制态", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connect(connectionObserver, transportOptions, deps);
    await connection.ready;

    transportOptions[0]?.onEvent(updateFrame(1n, update(1n)));

    expect(deps.fetchView).not.toHaveBeenCalled();
    expect(connectionObserver.onBaseline).toHaveBeenCalledTimes(1);
    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        observation: expect.objectContaining({
          execution: expect.objectContaining({
            active_turn: expect.objectContaining({ turn_id: "turn-live" }),
          }),
        }),
      }),
    );
    connection.close();
  });

  it("lane 缺口只重置当前 epoch 一次，不并发读取 baseline", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connect(connectionObserver, transportOptions, deps);
    await connection.ready;

    transportOptions[0]?.onEvent(updateFrame(1n, update(1n)));
    transportOptions[0]?.onEvent(updateFrame(1n, update(3n)));
    transportOptions[0]?.onEvent(updateFrame(1n, update(4n)));
    transportOptions[0]?.onLifecycleChange("reconnecting");

    expect(connectionObserver.onReset).toHaveBeenCalledTimes(1);
    expect(connectionObserver.onReset).toHaveBeenCalledWith("sequence_gap");
    expect(deps.fetchView).not.toHaveBeenCalled();
    expect(connectionObserver.onUpdate).toHaveBeenCalledTimes(1);
    connection.close();
  });

  it("服务端 reset 与随后断连对同一 epoch 只清理一次", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connect(connectionObserver, transportOptions, deps);
    await connection.ready;

    transportOptions[0]?.onEvent({
      kind: "reset_required",
      connection_epoch: 1n,
      reason: "lagged",
      last_sequence: 8n,
    });
    transportOptions[0]?.onLifecycleChange("reconnecting");

    expect(connectionObserver.onReset).toHaveBeenCalledTimes(1);
    expect(connectionObserver.onReset).toHaveBeenCalledWith("lagged");
    connection.close();
  });

  it("重连后的新 epoch baseline 替换失效 lane", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const recovered = {
      ...agentRuntimeTestFixtures.snapshots.completed,
      observation: {
        ...agentRuntimeTestFixtures.snapshots.completed.observation,
        revision: 41n,
        conversation: [
          record(
            "recovered-presentation",
            {
              type: "thread_name_updated",
              payload: {
                threadId: "source-1",
                threadName: "recovered",
              },
            },
            "durable",
          ),
        ],
      },
    };
    const deps = dependencies({ transportOptions });
    const connection = connect(connectionObserver, transportOptions, deps);
    await connection.ready;

    transportOptions[0]?.onLifecycleChange("reconnecting");
    transportOptions[0]?.onLifecycleChange("connected");
    transportOptions[0]?.onEvent(baseline(2n, recovered));

    expect(connectionObserver.onReset).toHaveBeenCalledWith(
      "transport_disconnected",
    );
    expect(connectionObserver.onBaseline).toHaveBeenLastCalledWith(recovered);
    expect(deps.fetchView).not.toHaveBeenCalled();
    connection.close();
  });

  it("旧 revision presentation 仍进入 live lane，但不回退权威 control", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    transportOptions[0]?.onEvent(
      baseline(1n, agentRuntimeTestFixtures.snapshots.started),
    );
    await connection.ready;

    transportOptions[0]?.onEvent(updateFrame(1n, update(1n, {
      view_revision:
        agentRuntimeTestFixtures.snapshots.started.observation.revision - 1n,
      execution: {
        active_turn: null,
        queued_compaction: null,
        last_compaction_outcome: null,
      },
      presentations: [
        record("stale-presentation", {
          type: "thread_name_updated",
          payload: {
            threadId: "source-1",
            threadName: "late presentation",
          },
        }),
      ],
    })));

    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        observation: expect.objectContaining({
          revision: agentRuntimeTestFixtures.snapshots.started.observation.revision,
          execution: expect.objectContaining({
            active_turn: expect.objectContaining({ turn_id: "turn-compaction" }),
          }),
        }),
      }),
    );
    expect(connectionObserver.onUpdate).toHaveBeenLastCalledWith(
      expect.objectContaining({
        presentations: [
          expect.objectContaining({ presentation_id: "stale-presentation" }),
        ],
      }),
    );
    connection.close();
  });

  it("baseline 前的 update 触发协议重置，不能伪造初始状态", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );

    transportOptions[0]?.onEvent(updateFrame(1n, update(1n)));
    expect(connectionObserver.onReset).toHaveBeenCalledWith("protocol_error");
    expect(connectionObserver.onUpdate).not.toHaveBeenCalled();

    transportOptions[0]?.onEvent(
      baseline(1n, agentRuntimeTestFixtures.snapshots.completed),
    );
    await connection.ready;
    connection.close();
  });

  it("transport parse error 带 target、epoch 与最后成功 sequence", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connect(connectionObserver, transportOptions, deps);
    await connection.ready;

    transportOptions[0]?.onEvent(updateFrame(1n, update(7n)));
    transportOptions[0]?.onError(new Error("Agent Runtime update 解析失败"));

    expect(connectionObserver.onError).toHaveBeenCalledWith(
      expect.objectContaining({
        message: expect.stringContaining(
          "target=run-1:agent-1, connection_epoch=1, last_sequence=7",
        ),
      }),
    );
    connection.close();
  });

  it("命令后显式 refresh 仍读取一次权威 view", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({
      fetchView: async () => agentRuntimeTestFixtures.snapshots.completed,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    transportOptions[0]?.onEvent(
      baseline(1n, agentRuntimeTestFixtures.snapshots.started),
    );
    await connection.ready;

    await connection.execute({
      client_command_id: "command-1",
      command: { kind: "interrupt" },
    });

    expect(deps.executeCommand).toHaveBeenCalledOnce();
    expect(deps.fetchView).toHaveBeenCalledOnce();
    expect(connectionObserver.onBaseline).toHaveBeenLastCalledWith(
      agentRuntimeTestFixtures.snapshots.completed,
    );
    connection.close();
  });

  it("refresh 期间到达的 live batches 在 baseline replacement 后按序保留", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const refreshView = deferred<AgentRuntimeView>();
    const deps = dependencies({
      fetchView: () => refreshView.promise,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    transportOptions[0]?.onEvent(
      baseline(1n, agentRuntimeTestFixtures.snapshots.started),
    );
    await connection.ready;

    const refresh = connection.refresh();
    const first = update(1n, {
      presentations: [
        record("refresh-overlay-1", {
          type: "thread_name_updated",
          payload: {
            threadId: "source-1",
            threadName: "first",
          },
        }),
      ],
    });
    const second = update(2n, {
      execution: {
        active_turn: null,
        queued_compaction: null,
        last_compaction_outcome: null,
      },
      presentations: [
        record("refresh-overlay-2", {
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
        }, "durable"),
      ],
    });
    transportOptions[0]?.onEvent(updateFrame(1n, first));
    transportOptions[0]?.onEvent(updateFrame(1n, second));

    refreshView.resolve({
      ...agentRuntimeTestFixtures.snapshots.completed,
      observation: {
        ...agentRuntimeTestFixtures.snapshots.completed.observation,
        revision: 20n,
        conversation: [second.presentations[0]!],
      },
    });
    await refresh;

    expect(connectionObserver.onBaseline).toHaveBeenCalledTimes(2);
    expect(connectionObserver.onUpdate).toHaveBeenCalledTimes(3);
    expect(
      vi.mocked(connectionObserver.onUpdate).mock.calls.slice(-1)
        .map(([value]) => value.lane_sequence),
    ).toEqual([1n]);
    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        observation: expect.objectContaining({
          execution: expect.objectContaining({ active_turn: null }),
        }),
      }),
    );
    connection.close();
  });

  it("同一 epoch 的重复 baseline 触发协议重置，不能覆盖已观察 update", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connect(connectionObserver, transportOptions, deps);
    await connection.ready;

    transportOptions[0]?.onEvent(updateFrame(1n, update(1n)));
    transportOptions[0]?.onEvent(
      baseline(1n, agentRuntimeTestFixtures.snapshots.completed),
    );

    expect(connectionObserver.onReset).toHaveBeenCalledOnce();
    expect(connectionObserver.onReset).toHaveBeenCalledWith("protocol_error");
    expect(connectionObserver.onBaseline).toHaveBeenCalledOnce();
    connection.close();
  });

  it("reset 会作废 refresh overlay，失效 lane 不会被旧 baseline 重放", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const refreshView = deferred<AgentRuntimeView>();
    const deps = dependencies({
      fetchView: () => refreshView.promise,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    transportOptions[0]?.onEvent(
      baseline(1n, agentRuntimeTestFixtures.snapshots.started),
    );
    await connection.ready;

    const refresh = connection.refresh();
    transportOptions[0]?.onEvent(updateFrame(1n, update(1n, {
      presentations: [
        record("invalidated-overlay", {
          type: "thread_name_updated",
          payload: {
            threadId: "source-1",
            threadName: "invalidated",
          },
        }),
      ],
    })));
    transportOptions[0]?.onEvent({
      kind: "reset_required",
      connection_epoch: 1n,
      reason: "lagged",
      last_sequence: 1n,
    });

    refreshView.resolve(agentRuntimeTestFixtures.snapshots.completed);
    await refresh;

    expect(connectionObserver.onReset).toHaveBeenCalledOnce();
    expect(connectionObserver.onBaseline).toHaveBeenCalledOnce();
    expect(connectionObserver.onUpdate).toHaveBeenCalledOnce();
    connection.close();
  });

  it("新 epoch baseline fence 掉仍在途的旧 refresh", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const refreshView = deferred<AgentRuntimeView>();
    const deps = dependencies({
      fetchView: () => refreshView.promise,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    transportOptions[0]?.onEvent(
      baseline(1n, agentRuntimeTestFixtures.snapshots.started),
    );
    await connection.ready;

    const refresh = connection.refresh();
    transportOptions[0]?.onEvent(
      baseline(2n, agentRuntimeTestFixtures.snapshots.completed),
    );
    refreshView.resolve(agentRuntimeTestFixtures.snapshots.started);
    await refresh;

    expect(connectionObserver.onBaseline).toHaveBeenCalledTimes(2);
    expect(connectionObserver.onBaseline).toHaveBeenLastCalledWith(
      agentRuntimeTestFixtures.snapshots.completed,
    );
    connection.close();
  });

  it("终态 durable batch 自身收敛控制态且不触发例行快照", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({ transportOptions });
    const connection = connect(connectionObserver, transportOptions, deps);
    await connection.ready;
    const terminal = record(
      "terminal-turn-live",
      {
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
      "durable",
    );

    transportOptions[0]?.onEvent(updateFrame(1n, update(1n, {
      execution: {
        active_turn: null,
        queued_compaction: null,
        last_compaction_outcome: null,
      },
      presentations: [terminal],
    })));

    expect(deps.fetchView).not.toHaveBeenCalled();
    expect(connectionObserver.onUpdate).toHaveBeenLastCalledWith(
      expect.objectContaining({ presentations: [terminal] }),
    );
    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        observation: expect.objectContaining({
          execution: expect.objectContaining({ active_turn: null }),
        }),
      }),
    );
    connection.close();
  });
});

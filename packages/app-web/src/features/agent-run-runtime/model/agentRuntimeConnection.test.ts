import { describe, expect, it, vi } from "vitest";

import type { CanonicalConversationRecord } from "../../../generated/backbone-protocol";
import type {
  AgentRuntimeOperationReceipt,
  AgentRuntimeUpdate,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-validators";
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
  overrides: Partial<AgentRuntimeUpdate> = {},
): AgentRuntimeUpdate {
  return {
    lane_sequence: sequence,
    view_revision: sequence + 10n,
    execution: {
      status: "active",
      active_turn: {
        turn_id: "turn-live",
        kind: "conversation",
        phase: "running",
        operation_id: null,
        started_at_ms: 1000n,
        cancellable: true,
      },
      queued_compaction: null,
      last_compaction_outcome: null,
      latest_turn_id: "turn-live",
    },
    context: {
      snapshot_revision: sequence + 10n,
      context_revision: `context-${sequence + 10n}`,
      recipe_digest: `sha256:context-${sequence + 10n}`,
      authority: "source_authoritative",
      fidelity: "exact",
    },
    command_availability:
      agentRuntimeTestFixtures.snapshots.started.command_availability,
    interactions: [],
    presentations: [],
    ...overrides,
  };
}

function dependencies(input: {
  fetchView: () => Promise<AgentRuntimeView>;
  executeCommand?: () => Promise<AgentRuntimeOperationReceipt>;
  transportOptions: AgentRuntimeUpdateTransportOptions[];
}) {
  return {
    fetchView: vi.fn(input.fetchView),
    executeCommand: vi.fn(
      input.executeCommand
        ?? (async () => ({
          operation_id: "operation-1",
          thread_id: "runtime-thread-child",
          status: "accepted" as const,
          evidence: null,
          duplicate: false,
        })),
    ),
    createTransport: (options: AgentRuntimeUpdateTransportOptions) => {
      input.transportOptions.push(options);
      return { close: vi.fn() };
    },
  };
}

describe("AgentRuntimeConnection", () => {
  it("终态 history 收缩后下一轮 update 仍直接恢复运行态与停止能力", async () => {
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
    await connection.ready;

    transportOptions[0]?.onEvent(update(1n));

    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        execution: expect.objectContaining({
          status: "active",
          active_turn: expect.objectContaining({ turn_id: "turn-live" }),
        }),
      }),
    );
    connection.close();
  });

  it("lane 出现缺口时重载权威 view 后再应用已缓冲 update", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const views = [
      agentRuntimeTestFixtures.snapshots.completed,
      {
        ...agentRuntimeTestFixtures.snapshots.completed,
        view_revision: 11n,
      },
    ];
    const deps = dependencies({
      fetchView: async () => views.shift() ?? agentRuntimeTestFixtures.snapshots.completed,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    await connection.ready;

    transportOptions[0]?.onEvent(update(1n));
    transportOptions[0]?.onEvent(update(3n, {
      execution: {
        status: "idle",
        active_turn: null,
        queued_compaction: null,
        last_compaction_outcome: null,
        latest_turn_id: "turn-live",
      },
    }));
    await vi.waitFor(() => expect(deps.fetchView).toHaveBeenCalledTimes(2));
    expect(connectionObserver.onBaseline).toHaveBeenCalledTimes(2);
    await vi.waitFor(() => {
      expect(connectionObserver.onView).toHaveBeenLastCalledWith(
        expect.objectContaining({
          view_revision: 13n,
          execution: expect.objectContaining({ status: "idle" }),
        }),
      );
    });
    connection.close();
  });

  it("重连恢复 view 重新建立 presentation hydration baseline", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const recovered = {
      ...agentRuntimeTestFixtures.snapshots.completed,
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
    };
    const views = [
      agentRuntimeTestFixtures.snapshots.completed,
      recovered,
    ];
    const deps = dependencies({
      fetchView: async () => views.shift() ?? recovered,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    await connection.ready;

    transportOptions[0]?.onLifecycleChange("reconnecting");
    transportOptions[0]?.onLifecycleChange("connected");

    await vi.waitFor(() => expect(deps.fetchView).toHaveBeenCalledTimes(2));
    expect(connectionObserver.onBaseline).toHaveBeenLastCalledWith(recovered);
    connection.close();
  });

  it("旧 revision 只叠加 presentation，不回退权威 control", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const deps = dependencies({
      fetchView: async () => agentRuntimeTestFixtures.snapshots.started,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    await connection.ready;

    transportOptions[0]?.onEvent(update(1n, {
      view_revision:
        agentRuntimeTestFixtures.snapshots.started.view_revision - 1n,
      execution: {
        status: "idle",
        active_turn: null,
        queued_compaction: null,
        last_compaction_outcome: null,
        latest_turn_id: "turn-live",
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
    }));

    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        view_revision:
          agentRuntimeTestFixtures.snapshots.started.view_revision,
        execution: expect.objectContaining({ status: "active" }),
        conversation: expect.arrayContaining([
          expect.objectContaining({ presentation_id: "stale-presentation" }),
        ]),
      }),
    );
    connection.close();
  });

  it("refresh 期间缓冲的旧 revision presentation 不被整条丢弃", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    let resolveView: ((view: AgentRuntimeView) => void) | undefined;
    const deps = dependencies({
      fetchView: () => new Promise<AgentRuntimeView>((resolve) => {
        resolveView = resolve;
      }),
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    const bufferedPresentation = record("buffered-stale-presentation", {
      type: "thread_name_updated",
      payload: {
        threadId: "source-1",
        threadName: "buffered",
      },
    });
    transportOptions[0]?.onEvent(update(1n, {
      view_revision: 1n,
      execution: {
        status: "idle",
        active_turn: null,
        queued_compaction: null,
        last_compaction_outcome: null,
        latest_turn_id: null,
      },
      presentations: [bufferedPresentation],
    }));

    resolveView?.(agentRuntimeTestFixtures.snapshots.started);
    await connection.ready;

    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        execution: expect.objectContaining({ status: "active" }),
        conversation: expect.arrayContaining([bufferedPresentation]),
      }),
    );
    connection.close();
  });

  it("命令统一经 Connection 执行并刷新权威 view", async () => {
    const transportOptions: AgentRuntimeUpdateTransportOptions[] = [];
    const connectionObserver = observer();
    const views = [
      agentRuntimeTestFixtures.snapshots.started,
      agentRuntimeTestFixtures.snapshots.completed,
    ];
    const deps = dependencies({
      fetchView: async () =>
        views.shift() ?? agentRuntimeTestFixtures.snapshots.completed,
      transportOptions,
    });
    const connection = connectAgentRuntimeConnection(
      { runId: "run-1", agentId: "agent-1" },
      connectionObserver,
      deps,
    );
    await connection.ready;

    await connection.execute({
      client_command_id: "command-1",
      command: { kind: "interrupt" },
    });

    expect(deps.executeCommand).toHaveBeenCalledWith(
      { runId: "run-1", agentId: "agent-1" },
      {
        client_command_id: "command-1",
        command: { kind: "interrupt" },
      },
    );
    expect(deps.fetchView).toHaveBeenCalledTimes(2);
    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        execution: expect.objectContaining({
          status: "idle",
          active_turn: null,
        }),
      }),
    );
    connection.close();
  });

  it("终态 durable presentation 在快照确认前不会被旧基线覆盖", async () => {
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

    transportOptions[0]?.onEvent(update(1n, {
      execution: {
        status: "idle",
        active_turn: null,
        queued_compaction: null,
        last_compaction_outcome: null,
        latest_turn_id: "turn-live",
      },
      presentations: [terminal],
    }));

    await vi.waitFor(() => expect(deps.fetchView).toHaveBeenCalledTimes(2));
    expect(connectionObserver.onView).toHaveBeenLastCalledWith(
      expect.objectContaining({
        conversation: expect.arrayContaining([terminal]),
      }),
    );
    connection.close();
  });
});

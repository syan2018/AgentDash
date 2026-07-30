import type {
  AgentRuntimeOperationReceipt,
  AgentRuntimeStreamFrame,
  AgentRuntimeUpdate,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";
import {
  executeAgentRunRuntimeCommand,
  fetchAgentRuntimeView,
  type AgentRunProductRuntimeCommandRequest,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import {
  createAgentRuntimeUpdateTransport,
  type AgentRuntimeConnectionLifecycle,
  type AgentRuntimeUpdateTransport,
  type AgentRuntimeUpdateTransportOptions,
} from "./agentRuntimeUpdateTransport";
import { applyAgentRuntimeUpdate } from "./agentRuntimeProjection";

export type AgentRuntimeResetReason = Extract<
  AgentRuntimeStreamFrame,
  { kind: "reset_required" }
>["reason"];

export interface AgentRuntimeConnectionObserver {
  onBaseline: (view: AgentRuntimeView) => void;
  onView: (view: AgentRuntimeView) => void;
  onUpdate: (update: AgentRuntimeUpdate) => void;
  onReset: (reason: AgentRuntimeResetReason) => void;
  onLifecycleChange: (lifecycle: AgentRuntimeConnectionLifecycle) => void;
  onError: (error: Error) => void;
}

export interface AgentRuntimeConnectionDependencies {
  fetchView: (target: AgentRunRuntimeTarget) => Promise<AgentRuntimeView>;
  executeCommand: (
    target: AgentRunRuntimeTarget,
    request: AgentRunProductRuntimeCommandRequest,
  ) => Promise<AgentRuntimeOperationReceipt>;
  createTransport: (
    options: AgentRuntimeUpdateTransportOptions,
  ) => AgentRuntimeUpdateTransport;
}

export interface AgentRuntimeConnection {
  readonly ready: Promise<void>;
  refresh: () => Promise<void>;
  execute: (
    request: AgentRunProductRuntimeCommandRequest,
  ) => Promise<AgentRuntimeOperationReceipt>;
  close: () => void;
}

const PRODUCTION_DEPENDENCIES: AgentRuntimeConnectionDependencies = {
  fetchView: fetchAgentRuntimeView,
  executeCommand: executeAgentRunRuntimeCommand,
  createTransport: createAgentRuntimeUpdateTransport,
};

export function connectAgentRuntimeConnection(
  agentRunTarget: AgentRunRuntimeTarget,
  observer: AgentRuntimeConnectionObserver,
  dependencies: AgentRuntimeConnectionDependencies = PRODUCTION_DEPENDENCIES,
): AgentRuntimeConnection {
  interface RefreshAttempt {
    readonly streamBaselineGeneration: number;
    readonly updates: AgentRuntimeUpdate[];
    invalidated: boolean;
  }

  let closed = false;
  let transport: AgentRuntimeUpdateTransport | null = null;
  let currentView: AgentRuntimeView | null = null;
  let baselineView: AgentRuntimeView | null = null;
  let refreshInFlight: Promise<void> | null = null;
  let refreshAttempt: RefreshAttempt | null = null;
  let streamBaselineGeneration = 0;
  let connectionEpoch: bigint | null = null;
  let resetHandledEpoch: bigint | null = null;
  let resetHandledBeforeBaseline = false;
  let lastLaneSequence: bigint | null = null;
  let lastLifecycle: AgentRuntimeConnectionLifecycle | null = null;
  let resolveReady: (() => void) | null = null;
  const ready = new Promise<void>((resolve) => {
    resolveReady = resolve;
  });

  const emitLifecycle = (lifecycle: AgentRuntimeConnectionLifecycle): void => {
    if (lastLifecycle === lifecycle) return;
    lastLifecycle = lifecycle;
    observer.onLifecycleChange(lifecycle);
  };

  const reportTransportError = (error: Error): void => {
    const target = `${agentRunTarget.runId}:${agentRunTarget.agentId}`;
    const epoch = connectionEpoch?.toString() ?? "none";
    const sequence = lastLaneSequence?.toString() ?? "none";
    observer.onError(new Error(
      `${error.message} [target=${target}, connection_epoch=${epoch}, last_sequence=${sequence}]`,
      { cause: error },
    ));
  };

  const applyUpdate = (
    update: AgentRuntimeUpdate,
    preserveForRefresh = true,
  ): void => {
    if (!currentView) return;
    if (
      preserveForRefresh
      && refreshAttempt
      && !refreshAttempt.invalidated
    ) {
      refreshAttempt.updates.push(update);
    }
    currentView = applyAgentRuntimeUpdate(currentView, update);
    observer.onUpdate(update);
    observer.onView(currentView);
  };

  const resetLiveLane = (reason: AgentRuntimeResetReason): void => {
    if (connectionEpoch == null) {
      if (resetHandledBeforeBaseline) return;
      resetHandledBeforeBaseline = true;
    } else {
      if (resetHandledEpoch === connectionEpoch) return;
      resetHandledEpoch = connectionEpoch;
    }
    if (refreshAttempt) {
      refreshAttempt.invalidated = true;
      refreshAttempt.updates.length = 0;
    }
    lastLaneSequence = null;
    currentView = baselineView;
    if (currentView) observer.onView(currentView);
    observer.onReset(reason);
  };

  const refreshAuthoritativeView = (): Promise<void> => {
    if (refreshInFlight) return refreshInFlight;
    const attempt: RefreshAttempt = {
      streamBaselineGeneration,
      updates: [],
      invalidated: false,
    };
    refreshAttempt = attempt;
    refreshInFlight = dependencies
      .fetchView(agentRunTarget)
      .then((view) => {
        if (
          closed
          || attempt.invalidated
          || attempt.streamBaselineGeneration !== streamBaselineGeneration
        ) {
          return;
        }
        if (refreshAttempt === attempt) {
          refreshAttempt = null;
        }
        baselineView = view;
        currentView = view;
        observer.onBaseline(currentView);
        const baselinePresentationIds = new Set(
          currentView.observation.conversation.map(
            (record) => record.presentation_id,
          ),
        );
        for (const update of attempt.updates) {
          const presentations = update.presentations.filter(
            (record) =>
              record.presentation.durability === "ephemeral"
              || !baselinePresentationIds.has(record.presentation_id),
          );
          const stateAdvances =
            update.state != null
            && update.state.revision > currentView.observation.revision;
          if (presentations.length === 0 && !stateAdvances) {
            continue;
          }
          applyUpdate({ ...update, presentations }, false);
        }
      })
      .finally(() => {
        if (refreshAttempt === attempt) {
          refreshAttempt = null;
        }
        refreshInFlight = null;
      });
    return refreshInFlight;
  };

  const acceptBaseline = (
    epoch: bigint,
    view: AgentRuntimeView,
  ): void => {
    if (connectionEpoch === epoch) {
      resetLiveLane("protocol_error");
      return;
    }
    streamBaselineGeneration += 1;
    if (refreshAttempt) {
      refreshAttempt.invalidated = true;
      refreshAttempt.updates.length = 0;
    }
    connectionEpoch = epoch;
    resetHandledEpoch = null;
    resetHandledBeforeBaseline = false;
    lastLaneSequence = null;
    baselineView = view;
    currentView = view;
    observer.onBaseline(view);
    resolveReady?.();
    resolveReady = null;
  };

  const acceptUpdate = (
    frame: Extract<AgentRuntimeStreamFrame, { kind: "update" }>,
  ): void => {
    if (connectionEpoch == null || frame.connection_epoch !== connectionEpoch) {
      resetLiveLane("protocol_error");
      return;
    }
    if (resetHandledEpoch === connectionEpoch) return;
    const previousLaneSequence = lastLaneSequence;
    if (
      previousLaneSequence != null
      && frame.lane_sequence <= previousLaneSequence
    ) {
      return;
    }
    if (
      previousLaneSequence != null
      && frame.lane_sequence !== previousLaneSequence + 1n
    ) {
      resetLiveLane("sequence_gap");
      return;
    }
    lastLaneSequence = frame.lane_sequence;
    applyUpdate({
      lane_sequence: frame.lane_sequence,
      state: frame.state,
      presentations: frame.presentations,
    });
  };

  transport = dependencies.createTransport({
    agentRunTarget,
    onLifecycleChange: (lifecycle) => {
      emitLifecycle(lifecycle);
      if (lifecycle === "reconnecting") {
        resetLiveLane("transport_disconnected");
      }
    },
    onError: reportTransportError,
    onEvent: (frame) => {
      if (closed) return;
      if (frame.kind === "baseline") {
        acceptBaseline(frame.connection_epoch, frame.view);
      } else if (frame.kind === "update") {
        acceptUpdate(frame);
      } else if (
        connectionEpoch == null
        || frame.connection_epoch === connectionEpoch
      ) {
        resetLiveLane(frame.reason);
      }
    },
  });

  return {
    ready,
    refresh: () => refreshAuthoritativeView(),
    execute: async (request) => {
      const receipt = await dependencies.executeCommand(agentRunTarget, request);
      await refreshAuthoritativeView();
      return receipt;
    },
    close: () => {
      if (closed) return;
      closed = true;
      resolveReady?.();
      resolveReady = null;
      transport?.close();
      transport = null;
      emitLifecycle("closed");
    },
  };
}

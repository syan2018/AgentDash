import type {
  AgentRuntimeOperationReceipt,
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

export interface AgentRuntimeConnectionObserver {
  onBaseline: (view: AgentRuntimeView) => void;
  onView: (view: AgentRuntimeView) => void;
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

function normalizeError(error: unknown, message: string): Error {
  return error instanceof Error ? error : new Error(message);
}

function hasTerminalPresentation(update: AgentRuntimeUpdate): boolean {
  return update.presentations.some(
    (record) => record.presentation.envelope.event.type === "turn_completed",
  );
}

function overlayPendingPresentations(
  view: AgentRuntimeView,
  pending: ReadonlyMap<string, AgentRuntimeUpdate>,
): AgentRuntimeView {
  let projected = view;
  for (const update of pending.values()) {
    projected = applyAgentRuntimeUpdate(projected, {
      ...update,
      observation: projected.observation,
    });
  }
  return projected;
}

export function connectAgentRuntimeConnection(
  agentRunTarget: AgentRunRuntimeTarget,
  observer: AgentRuntimeConnectionObserver,
  dependencies: AgentRuntimeConnectionDependencies = PRODUCTION_DEPENDENCIES,
): AgentRuntimeConnection {
  let closed = false;
  let transport: AgentRuntimeUpdateTransport | null = null;
  let currentView: AgentRuntimeView | null = null;
  let refreshInFlight: Promise<void> | null = null;
  let bufferedUpdates: AgentRuntimeUpdate[] = [];
  const pendingDurableUpdates = new Map<string, AgentRuntimeUpdate>();
  let terminalRefreshQueued = false;
  let reconnectPending = false;
  let recoveryBaselineRequested = false;
  let lastLaneSequence: bigint | null = null;

  const reportRefreshError = (error: unknown): void => {
    if (!closed) {
      observer.onError(
        normalizeError(error, "Agent Runtime authoritative view 刷新失败"),
      );
    }
  };

  const rememberDurablePresentations = (update: AgentRuntimeUpdate): void => {
    for (const record of update.presentations) {
      if (record.presentation.durability === "durable") {
        pendingDurableUpdates.set(record.presentation_id, {
          ...update,
          presentations: [record],
        });
      }
    }
  };

  const applyUpdate = (update: AgentRuntimeUpdate): void => {
    if (!currentView) {
      bufferedUpdates.push(update);
      return;
    }
    rememberDurablePresentations(update);
    const projectedUpdate =
      update.observation.revision < currentView.observation.revision
      ? {
          ...update,
          observation: currentView.observation,
        }
      : update;
    currentView = applyAgentRuntimeUpdate(currentView, projectedUpdate);
    observer.onView(currentView);
  };

  const applyBufferedUpdates = (): void => {
    if (closed || reconnectPending || refreshInFlight || !currentView) return;
    const updates = bufferedUpdates;
    bufferedUpdates = [];
    for (const update of updates) {
      applyUpdate(update);
    }
  };

  const refreshAuthoritativeView = (publishBaseline = false): Promise<void> => {
    if (publishBaseline) recoveryBaselineRequested = true;
    if (refreshInFlight) return refreshInFlight;
    refreshInFlight = dependencies
      .fetchView(agentRunTarget)
      .then((view) => {
        if (closed) return;
        const publishRecoveredBaseline = recoveryBaselineRequested;
        recoveryBaselineRequested = false;
        const baselinePresentationIds = new Set(
          view.observation.conversation.map((record) => record.presentation_id),
        );
        for (const presentationId of baselinePresentationIds) {
          pendingDurableUpdates.delete(presentationId);
        }
        currentView = overlayPendingPresentations(view, pendingDurableUpdates);
        if (publishRecoveredBaseline) {
          observer.onBaseline(currentView);
        } else {
          observer.onView(currentView);
        }
      })
      .finally(() => {
        refreshInFlight = null;
        applyBufferedUpdates();
        if (terminalRefreshQueued && !closed && !reconnectPending) {
          terminalRefreshQueued = false;
          void refreshAuthoritativeView(false).catch(reportRefreshError);
        }
      });
    return refreshInFlight;
  };

  transport = dependencies.createTransport({
    agentRunTarget,
    onLifecycleChange: (lifecycle) => {
      observer.onLifecycleChange(lifecycle);
      if (lifecycle === "reconnecting") {
        reconnectPending = true;
        lastLaneSequence = null;
        return;
      }
      if (lifecycle === "connected" && reconnectPending) {
        reconnectPending = false;
        terminalRefreshQueued = false;
        void refreshAuthoritativeView(true).catch(reportRefreshError);
      }
    },
    onError: observer.onError,
    onEvent: (update) => {
      if (closed) return;
      const previousLaneSequence = lastLaneSequence;
      if (
        previousLaneSequence != null
        && update.lane_sequence <= previousLaneSequence
      ) {
        return;
      }
      lastLaneSequence = update.lane_sequence;
      if (
        previousLaneSequence != null
        && update.lane_sequence !== previousLaneSequence + 1n
      ) {
        bufferedUpdates.push(update);
        void refreshAuthoritativeView(true).catch(reportRefreshError);
        return;
      }
      if (refreshInFlight || reconnectPending || !currentView) {
        bufferedUpdates.push(update);
        if (hasTerminalPresentation(update)) terminalRefreshQueued = true;
        return;
      }
      applyUpdate(update);
      if (hasTerminalPresentation(update)) {
        void refreshAuthoritativeView(false).catch(reportRefreshError);
      }
    },
  });

  const ready = refreshAuthoritativeView(true).catch((error: unknown) => {
    if (closed) return;
    observer.onError(normalizeError(error, "Agent Runtime connection 建立失败"));
    observer.onLifecycleChange("reconnecting");
  });

  return {
    ready,
    refresh: () => refreshAuthoritativeView(false),
    execute: async (request) => {
      const receipt = await dependencies.executeCommand(agentRunTarget, request);
      await refreshAuthoritativeView(false);
      return receipt;
    },
    close: () => {
      if (closed) return;
      closed = true;
      terminalRefreshQueued = false;
      reconnectPending = false;
      recoveryBaselineRequested = false;
      bufferedUpdates = [];
      pendingDurableUpdates.clear();
      transport?.close();
      transport = null;
      observer.onLifecycleChange("closed");
    },
  };
}

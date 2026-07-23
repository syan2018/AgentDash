import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-validators";
import type { AgentLiveEvent } from "../../../generated/agent-service-api";
import {
  fetchManagedRuntimeSnapshot,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import {
  createManagedRuntimeFeedTransport,
  type ManagedRuntimeFeedLifecycle,
  type ManagedRuntimeFeedTransport,
  type ManagedRuntimeFeedTransportOptions,
} from "./managedRuntimeFeedTransport";
import { applyAgentLiveEvent } from "./agentLiveProjection";

export interface ManagedRuntimeFeedConnectionObserver {
  onBaseline: (snapshot: ManagedRuntimeSnapshot) => void;
  onProjection: (snapshot: ManagedRuntimeSnapshot) => void;
  onLifecycleChange: (lifecycle: ManagedRuntimeFeedLifecycle) => void;
  onError: (error: Error) => void;
}

export interface ManagedRuntimeFeedConnectionDependencies {
  fetchSnapshot: (
    target: AgentRunRuntimeTarget,
  ) => Promise<ManagedRuntimeSnapshot>;
  createTransport: (
    options: ManagedRuntimeFeedTransportOptions,
  ) => ManagedRuntimeFeedTransport;
}

export interface ManagedRuntimeFeedConnection {
  ready: Promise<void>;
  reload: () => Promise<void>;
  close: () => void;
}

const PRODUCTION_DEPENDENCIES: ManagedRuntimeFeedConnectionDependencies = {
  fetchSnapshot: fetchManagedRuntimeSnapshot,
  createTransport: createManagedRuntimeFeedTransport,
};

function normalizeError(error: unknown, message: string): Error {
  return error instanceof Error ? error : new Error(message);
}

function isAuthoritativeSnapshotBoundary(event: AgentLiveEvent): boolean {
  return event.record.presentation.envelope.event.type === "turn_completed";
}

export function connectManagedRuntimeFeed(
  agentRunTarget: AgentRunRuntimeTarget,
  observer: ManagedRuntimeFeedConnectionObserver,
  dependencies: ManagedRuntimeFeedConnectionDependencies = PRODUCTION_DEPENDENCIES,
): ManagedRuntimeFeedConnection {
  let closed = false;
  let transport: ManagedRuntimeFeedTransport | null = null;
  let currentSnapshot: ManagedRuntimeSnapshot | null = null;
  let reloadInFlight: Promise<void> | null = null;
  let bufferedLiveEvents: AgentLiveEvent[] = [];
  let terminalReloadQueued = false;
  let reconnectPending = false;
  let baselinePublished = false;

  const reportReloadError = (error: unknown): void => {
    if (!closed) {
      observer.onError(
        normalizeError(error, "Agent authoritative terminal snapshot reload 失败"),
      );
    }
  };

  const applyBufferedEvents = (): void => {
    if (closed || reconnectPending || reloadInFlight || !currentSnapshot) return;
    const events = bufferedLiveEvents;
    bufferedLiveEvents = [];
    const converged = events.reduce(applyAgentLiveEvent, currentSnapshot);
    if (converged !== currentSnapshot) {
      currentSnapshot = converged;
      observer.onProjection(converged);
    }
  };

  const reloadAuthoritativeSnapshot = (publishBaseline = false): Promise<void> => {
    if (reloadInFlight) return reloadInFlight;
    reloadInFlight = dependencies
      .fetchSnapshot(agentRunTarget)
      .then((snapshot) => {
        if (closed) return;
        const events = bufferedLiveEvents;
        bufferedLiveEvents = [];
        currentSnapshot = snapshot;
        if (publishBaseline && !baselinePublished) {
          baselinePublished = true;
          observer.onBaseline(snapshot);
        }
        const converged = events.reduce(applyAgentLiveEvent, snapshot);
        currentSnapshot = converged;
        if (!publishBaseline || converged !== snapshot) {
          observer.onProjection(converged);
        }
      })
      .finally(() => {
        reloadInFlight = null;
        applyBufferedEvents();
        if (terminalReloadQueued && !closed && !reconnectPending) {
          terminalReloadQueued = false;
          void reloadAuthoritativeSnapshot(false).catch(reportReloadError);
        }
      });
    return reloadInFlight;
  };

  transport = dependencies.createTransport({
    agentRunTarget,
    onLifecycleChange: (lifecycle) => {
      observer.onLifecycleChange(lifecycle);
      if (lifecycle === "reconnecting") {
        reconnectPending = true;
        return;
      }
      if (lifecycle === "connected" && reconnectPending) {
        reconnectPending = false;
        terminalReloadQueued = false;
        void reloadAuthoritativeSnapshot(false).catch((error: unknown) => {
          if (!closed) {
            observer.onError(
              normalizeError(error, "Agent authoritative snapshot reload 失败"),
            );
          }
        });
      }
    },
    onError: observer.onError,
    onEvent: (event) => {
      if (closed) return;
      if (reloadInFlight || reconnectPending || !currentSnapshot) {
        bufferedLiveEvents.push(event);
        if (isAuthoritativeSnapshotBoundary(event)) {
          terminalReloadQueued = true;
        }
        return;
      }
      const projected = applyAgentLiveEvent(currentSnapshot, event);
      if (projected !== currentSnapshot) {
        currentSnapshot = projected;
        observer.onProjection(projected);
      }
      if (isAuthoritativeSnapshotBoundary(event)) {
        void reloadAuthoritativeSnapshot(false).catch(reportReloadError);
      }
    },
  });

  const ready = reloadAuthoritativeSnapshot(true).catch((error: unknown) => {
    if (closed) return;
    observer.onError(normalizeError(error, "Managed Runtime feed 连接失败"));
    observer.onLifecycleChange("reconnecting");
  });

  return {
    ready,
    reload: () => reloadAuthoritativeSnapshot(false),
    close: () => {
      if (closed) return;
      closed = true;
      terminalReloadQueued = false;
      reconnectPending = false;
      bufferedLiveEvents = [];
      transport?.close();
      transport = null;
      observer.onLifecycleChange("closed");
    },
  };
}

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
  let liveEventsDuringReload: AgentLiveEvent[] = [];
  let terminalReloadQueued = false;

  const reportReloadError = (error: unknown): void => {
    if (!closed) {
      observer.onError(
        normalizeError(error, "Agent authoritative terminal snapshot reload 失败"),
      );
    }
  };

  const reloadAuthoritativeSnapshot = (): Promise<void> => {
    if (reloadInFlight) return reloadInFlight;
    reloadInFlight = dependencies
      .fetchSnapshot(agentRunTarget)
      .then((snapshot) => {
        if (closed) return;
        const events = liveEventsDuringReload;
        liveEventsDuringReload = [];
        const converged = events.reduce(applyAgentLiveEvent, snapshot);
        currentSnapshot = converged;
        observer.onProjection(converged);
      })
      .finally(() => {
        liveEventsDuringReload = [];
        reloadInFlight = null;
        if (terminalReloadQueued && !closed) {
          terminalReloadQueued = false;
          void reloadAuthoritativeSnapshot().catch(reportReloadError);
        }
      });
    return reloadInFlight;
  };

  const loadBaselineAndSubscribe = async (): Promise<void> => {
    const loaded = await dependencies.fetchSnapshot(agentRunTarget);
    if (closed) return;

    currentSnapshot = loaded;
    observer.onBaseline(loaded);
    transport = dependencies.createTransport({
      agentRunTarget,
      onLifecycleChange: (lifecycle) => {
        observer.onLifecycleChange(lifecycle);
        if (lifecycle === "reconnecting") {
          void reloadAuthoritativeSnapshot().catch((error: unknown) => {
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
        if (reloadInFlight) {
          liveEventsDuringReload.push(event);
        }
        const current = currentSnapshot;
        if (!current || closed) return;
        const projected = applyAgentLiveEvent(current, event);
        if (projected !== current) {
          currentSnapshot = projected;
          observer.onProjection(projected);
        }
        if (isAuthoritativeSnapshotBoundary(event)) {
          if (reloadInFlight) {
            terminalReloadQueued = true;
          } else {
            void reloadAuthoritativeSnapshot().catch(reportReloadError);
          }
        }
      },
    });
  };

  const ready = loadBaselineAndSubscribe().catch((error: unknown) => {
    if (closed) return;
    observer.onError(normalizeError(error, "Managed Runtime feed 连接失败"));
    observer.onLifecycleChange("reconnecting");
  });

  return {
    ready,
    reload: reloadAuthoritativeSnapshot,
    close: () => {
      if (closed) return;
      closed = true;
      terminalReloadQueued = false;
      liveEventsDuringReload = [];
      transport?.close();
      transport = null;
      observer.onLifecycleChange("closed");
    },
  };
}

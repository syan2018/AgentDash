import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-validators";
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

export function connectManagedRuntimeFeed(
  agentRunTarget: AgentRunRuntimeTarget,
  observer: ManagedRuntimeFeedConnectionObserver,
  dependencies: ManagedRuntimeFeedConnectionDependencies = PRODUCTION_DEPENDENCIES,
): ManagedRuntimeFeedConnection {
  let closed = false;
  let transport: ManagedRuntimeFeedTransport | null = null;
  let currentSnapshot: ManagedRuntimeSnapshot | null = null;
  let reloadInFlight: Promise<void> | null = null;

  const reloadAuthoritativeSnapshot = (): Promise<void> => {
    if (reloadInFlight) return reloadInFlight;
    reloadInFlight = dependencies
      .fetchSnapshot(agentRunTarget)
      .then((snapshot) => {
        if (closed) return;
        currentSnapshot = snapshot;
        observer.onProjection(snapshot);
      })
      .finally(() => {
        reloadInFlight = null;
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
        const current = currentSnapshot;
        if (!current || closed) return;
        const projected = applyAgentLiveEvent(current, event);
        if (projected !== current) {
          currentSnapshot = projected;
          observer.onProjection(projected);
        }
        if (event.payload.kind === "provider_round_completed") {
          void reloadAuthoritativeSnapshot().catch((error: unknown) => {
            if (!closed) {
              observer.onError(
                normalizeError(error, "Agent authoritative snapshot reload 失败"),
              );
            }
          });
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
      transport?.close();
      transport = null;
      observer.onLifecycleChange("closed");
    },
  };
}

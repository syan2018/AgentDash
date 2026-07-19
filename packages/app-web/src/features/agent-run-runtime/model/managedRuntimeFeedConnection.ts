import type {
  ManagedRuntimePlatformChange,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";
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
import { applyManagedRuntimeChangePage } from "./managedRuntimeProjection";

export interface ManagedRuntimeFeedConnectionObserver {
  onBaseline: (snapshot: ManagedRuntimeSnapshot) => void;
  onProjection: (
    snapshot: ManagedRuntimeSnapshot,
    changes: ManagedRuntimePlatformChange[],
  ) => void;
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

  const loadBaselineAndSubscribe = async (): Promise<void> => {
    const loaded = await dependencies.fetchSnapshot(agentRunTarget);
    if (closed) return;

    currentSnapshot = loaded;
    observer.onBaseline(loaded);
    transport = dependencies.createTransport({
      agentRunTarget,
      after: loaded.latest_change_sequence,
      onLifecycleChange: observer.onLifecycleChange,
      onError: observer.onError,
      onPage: (page) => {
        const current = currentSnapshot;
        if (!current || closed) return;

        const unappliedChanges = page.changes.filter(
          (change) => change.sequence > current.latest_change_sequence,
        );
        const applied = applyManagedRuntimeChangePage(current, page);
        if (applied) {
          currentSnapshot = applied;
          if (applied !== current || unappliedChanges.length > 0) {
            observer.onProjection(applied, unappliedChanges);
          }
          return;
        }

        transport?.close();
        transport = null;
        observer.onLifecycleChange("connecting");
        void loadBaselineAndSubscribe().catch((error: unknown) => {
          if (closed) return;
          observer.onError(
            normalizeError(error, "Managed Runtime snapshot reload 失败"),
          );
          observer.onLifecycleChange("reconnecting");
        });
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
    close: () => {
      if (closed) return;
      closed = true;
      transport?.close();
      transport = null;
      observer.onLifecycleChange("closed");
    },
  };
}

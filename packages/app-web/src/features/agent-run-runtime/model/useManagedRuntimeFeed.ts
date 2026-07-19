import { useCallback, useEffect, useRef, useState } from "react";

import type {
  ManagedRuntimePlatformChange,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import {
  connectManagedRuntimeFeed,
  type ManagedRuntimeFeedConnection,
} from "./managedRuntimeFeedConnection";
import type { ManagedRuntimeFeedLifecycle } from "./managedRuntimeFeedTransport";

export interface UseManagedRuntimeFeedOptions {
  agentRunTarget: AgentRunRuntimeTarget | null;
  enabled: boolean;
}

export interface UseManagedRuntimeFeedResult {
  snapshot: ManagedRuntimeSnapshot | null;
  changes: ManagedRuntimePlatformChange[];
  boundTargetKey: string | null;
  lifecycle: ManagedRuntimeFeedLifecycle;
  isLoading: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
}

export function useManagedRuntimeFeed({
  agentRunTarget,
  enabled,
}: UseManagedRuntimeFeedOptions): UseManagedRuntimeFeedResult {
  const [snapshot, setSnapshot] = useState<ManagedRuntimeSnapshot | null>(null);
  const [changes, setChanges] = useState<ManagedRuntimePlatformChange[]>([]);
  const [boundTargetKey, setBoundTargetKey] = useState<string | null>(null);
  const [lifecycle, setLifecycle] =
    useState<ManagedRuntimeFeedLifecycle>("closed");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const connectionRef = useRef<ManagedRuntimeFeedConnection | null>(null);

  const close = useCallback(() => {
    connectionRef.current?.close();
    connectionRef.current = null;
    setLifecycle("closed");
    setBoundTargetKey(null);
  }, []);

  const connect = useCallback(() => {
    close();
    setSnapshot(null);
    setChanges([]);
    if (!enabled || !agentRunTarget) {
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    setLifecycle("connecting");

    const connection = connectManagedRuntimeFeed(agentRunTarget, {
      onBaseline: (loaded) => {
        setSnapshot(loaded);
        setChanges([]);
        setBoundTargetKey(
          `${agentRunTarget.runId}:${agentRunTarget.agentId}`,
        );
        setIsLoading(false);
      },
      onProjection: (projected, appliedChanges) => {
        setSnapshot(projected);
        setChanges((previous) => [...previous, ...appliedChanges]);
      },
      onLifecycleChange: setLifecycle,
      onError: (connectionError) => {
        setError(connectionError);
        setIsLoading(false);
      },
    });
    connectionRef.current = connection;
  }, [agentRunTarget, close, enabled]);

  useEffect(() => {
    connect();
    return close;
  }, [close, connect]);

  return {
    snapshot,
    changes,
    boundTargetKey,
    lifecycle,
    isLoading,
    error,
    reconnect: connect,
    close,
  };
}

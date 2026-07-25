import { useCallback, useEffect, useRef, useState } from "react";

import type {
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
  baselinePresentationIds: ReadonlySet<string>;
  boundTargetKey: string | null;
  lifecycle: ManagedRuntimeFeedLifecycle;
  isLoading: boolean;
  error: Error | null;
  refresh: () => Promise<void>;
  reconnect: () => void;
  close: () => void;
}

export function useManagedRuntimeFeed({
  agentRunTarget,
  enabled,
}: UseManagedRuntimeFeedOptions): UseManagedRuntimeFeedResult {
  const [snapshot, setSnapshot] = useState<ManagedRuntimeSnapshot | null>(null);
  const [baselinePresentationIds, setBaselinePresentationIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
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
    setBaselinePresentationIds(new Set());
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
        setBaselinePresentationIds(new Set(
          loaded.conversation_history.map((record) => record.presentation_id),
        ));
        setBoundTargetKey(
          `${agentRunTarget.runId}:${agentRunTarget.agentId}`,
        );
        setIsLoading(false);
      },
      onProjection: (projected) => {
        setSnapshot(projected);
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

  const refresh = useCallback(async () => {
    const connection = connectionRef.current;
    if (!connection) return;
    setError(null);
    try {
      await connection.reload();
    } catch (refreshError) {
      setError(
        refreshError instanceof Error
          ? refreshError
          : new Error("Agent authoritative snapshot 刷新失败"),
      );
    }
  }, []);

  return {
    snapshot,
    baselinePresentationIds,
    boundTargetKey,
    lifecycle,
    isLoading,
    error,
    refresh,
    reconnect: connect,
    close,
  };
}

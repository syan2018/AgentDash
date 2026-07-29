import { useCallback, useEffect, useRef, useState } from "react";

import type {
  AgentRuntimeOperationReceipt,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";
import type {
  AgentRunProductRuntimeCommandRequest,
  AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import {
  connectAgentRuntimeConnection,
  type AgentRuntimeConnection,
} from "./agentRuntimeConnection";
import type { AgentRuntimeConnectionLifecycle } from "./agentRuntimeUpdateTransport";

export interface UseAgentRuntimeConnectionOptions {
  agentRunTarget: AgentRunRuntimeTarget | null;
  enabled: boolean;
}

export interface UseAgentRuntimeConnectionResult {
  view: AgentRuntimeView | null;
  baselinePresentationIds: ReadonlySet<string>;
  boundTargetKey: string | null;
  lifecycle: AgentRuntimeConnectionLifecycle;
  isLoading: boolean;
  error: Error | null;
  refresh: () => Promise<void>;
  execute: (
    request: AgentRunProductRuntimeCommandRequest,
  ) => Promise<AgentRuntimeOperationReceipt>;
  reconnect: () => void;
  close: () => void;
}

export function useAgentRuntimeConnection({
  agentRunTarget,
  enabled,
}: UseAgentRuntimeConnectionOptions): UseAgentRuntimeConnectionResult {
  const [view, setView] = useState<AgentRuntimeView | null>(null);
  const [baselinePresentationIds, setBaselinePresentationIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [boundTargetKey, setBoundTargetKey] = useState<string | null>(null);
  const [lifecycle, setLifecycle] =
    useState<AgentRuntimeConnectionLifecycle>("closed");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const connectionRef = useRef<AgentRuntimeConnection | null>(null);

  const close = useCallback(() => {
    connectionRef.current?.close();
    connectionRef.current = null;
    setLifecycle("closed");
    setBoundTargetKey(null);
  }, []);

  const connect = useCallback(() => {
    close();
    setView(null);
    setBaselinePresentationIds(new Set());
    if (!enabled || !agentRunTarget) {
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    setLifecycle("connecting");

    const connection = connectAgentRuntimeConnection(agentRunTarget, {
      onBaseline: (loaded) => {
        setView(loaded);
        setBaselinePresentationIds(new Set(
          loaded.observation.conversation.map((record) => record.presentation_id),
        ));
        setBoundTargetKey(
          `${agentRunTarget.runId}:${agentRunTarget.agentId}`,
        );
        setIsLoading(false);
      },
      onView: (projected) => {
        setView(projected);
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
    let cancelled = false;
    queueMicrotask(() => {
      if (!cancelled) connect();
    });
    return () => {
      cancelled = true;
      close();
    };
  }, [close, connect]);

  const refresh = useCallback(async () => {
    const connection = connectionRef.current;
    if (!connection) return;
    setError(null);
    try {
      await connection.refresh();
    } catch (refreshError) {
      setError(
        refreshError instanceof Error
          ? refreshError
          : new Error("Agent Runtime view 刷新失败"),
      );
    }
  }, []);

  const execute = useCallback(async (
    request: AgentRunProductRuntimeCommandRequest,
  ): Promise<AgentRuntimeOperationReceipt> => {
    const connection = connectionRef.current;
    if (!connection) {
      throw new Error("Agent Runtime connection 尚未建立");
    }
    return connection.execute(request);
  }, []);

  return {
    view,
    baselinePresentationIds,
    boundTargetKey,
    lifecycle,
    isLoading,
    error,
    refresh,
    execute,
    reconnect: connect,
    close,
  };
}

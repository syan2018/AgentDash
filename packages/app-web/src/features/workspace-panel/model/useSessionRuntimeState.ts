import { useCallback, useEffect, useMemo, useState } from "react";

import {
  fetchSessionContext,
  fetchSessionHookRuntime,
  type SessionContextPayload,
} from "../../../services/session";
import type { HookSessionRuntimeInfo } from "../../../types";

export type SessionRuntimeStateStatus = "idle" | "loading" | "ready" | "refreshing" | "error";

export interface SessionRuntimeProjectionState {
  session_id: string | null;
  source_key: string | null;
  status: SessionRuntimeStateStatus;
  context: SessionContextPayload | null;
  hook_runtime: HookSessionRuntimeInfo | null;
  error: string | null;
}

interface UseSessionRuntimeStateInput {
  sessionId: string | null;
  sourceKey: string | null;
}

export function emptySessionRuntimeState(): SessionRuntimeProjectionState {
  return {
    session_id: null,
    source_key: null,
    status: "idle",
    context: null,
    hook_runtime: null,
    error: null,
  };
}

function stateMatches(
  state: SessionRuntimeProjectionState,
  sessionId: string,
  sourceKey: string,
): boolean {
  return state.session_id === sessionId && state.source_key === sourceKey;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Session runtime state 加载失败";
}

export function useSessionRuntimeState({
  sessionId,
  sourceKey,
}: UseSessionRuntimeStateInput) {
  const [state, setState] = useState<SessionRuntimeProjectionState>(() => emptySessionRuntimeState());

  useEffect(() => {
    if (!sessionId || !sourceKey) {
      setState(emptySessionRuntimeState());
      return;
    }

    let cancelled = false;
    setState({
      session_id: sessionId,
      source_key: sourceKey,
      status: "loading",
      context: null,
      hook_runtime: null,
      error: null,
    });

    void Promise.all([
      fetchSessionContext(sessionId),
      fetchSessionHookRuntime(sessionId).catch(() => null),
    ])
      .then(([context, hookRuntime]) => {
        if (cancelled) return;
        setState({
          session_id: sessionId,
          source_key: sourceKey,
          status: "ready",
          context,
          hook_runtime: hookRuntime,
          error: null,
        });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setState({
          session_id: sessionId,
          source_key: sourceKey,
          status: "error",
          context: null,
          hook_runtime: null,
          error: errorMessage(error),
        });
      });

    return () => {
      cancelled = true;
    };
  }, [sessionId, sourceKey]);

  const refreshContext = useCallback(async () => {
    if (!sessionId || !sourceKey) return;
    setState((current) => {
      if (!stateMatches(current, sessionId, sourceKey)) {
        return {
          session_id: sessionId,
          source_key: sourceKey,
          status: "loading",
          context: null,
          hook_runtime: null,
          error: null,
        };
      }
      return {
        ...current,
        status: current.context ? "refreshing" : "loading",
        error: null,
      };
    });

    try {
      const context = await fetchSessionContext(sessionId);
      setState((current) => {
        if (!stateMatches(current, sessionId, sourceKey)) return current;
        return {
          ...current,
          status: "ready",
          context,
          error: null,
        };
      });
    } catch (error: unknown) {
      setState((current) => {
        if (!stateMatches(current, sessionId, sourceKey)) return current;
        return {
          ...current,
          status: "error",
          context: null,
          error: errorMessage(error),
        };
      });
    }
  }, [sessionId, sourceKey]);

  const refreshHookRuntime = useCallback(async () => {
    if (!sessionId || !sourceKey) return;
    try {
      const hookRuntime = await fetchSessionHookRuntime(sessionId);
      setState((current) => {
        if (!stateMatches(current, sessionId, sourceKey)) return current;
        return {
          ...current,
          hook_runtime: hookRuntime,
          error: null,
        };
      });
    } catch {
      setState((current) => {
        if (!stateMatches(current, sessionId, sourceKey)) return current;
        return {
          ...current,
          hook_runtime: null,
        };
      });
    }
  }, [sessionId, sourceKey]);

  const activeState = useMemo(() => {
    return selectActiveSessionRuntimeState(state, sessionId, sourceKey);
  }, [sessionId, sourceKey, state]);

  return {
    state: activeState,
    refreshContext,
    refreshHookRuntime,
  };
}

export function selectActiveSessionRuntimeState(
  state: SessionRuntimeProjectionState,
  sessionId: string | null,
  sourceKey: string | null,
): SessionRuntimeProjectionState {
  if (!sessionId || !sourceKey || !stateMatches(state, sessionId, sourceKey)) {
    return emptySessionRuntimeState();
  }
  return state;
}
